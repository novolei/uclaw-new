//! `summarize_to_fold` — drive an LLM call that consumes a slice of
//! conversation history and emits a [`StructuredFold`] JSON object.
//!
//! Replaces the prior "/compact discards everything and writes a
//! one-line placeholder" behaviour with a real, lossy-but-structured
//! summary. The compacted messages stay in the DB for UI replay
//! (M1-T2c logical-marking); this summary is what the LLM sees in
//! their place on subsequent turns.
//!
//! Failure mode design:
//!
//! - LLM returns malformed JSON → return `SummarizeError::ParseFailed`
//!   carrying the raw text so the caller can fall back to the legacy
//!   placeholder without dropping the fold attempt entirely.
//! - LLM API call fails → `SummarizeError::LlmFailed(Error)` —
//!   transient errors should not block the user's `/compact` flow.
//!
//! The caller (tauri_commands::/compact intercept) is expected to
//! treat both errors as soft-fail: write the legacy placeholder and
//! log a warning. Compaction itself (marking compacted=1) is
//! unaffected — we still mark messages compacted, just the summary
//! text degrades to the old behaviour.

use std::sync::Arc;

use crate::agent::types::{ChatMessage, ContentBlock, MessageRole};
use crate::error::Error;
use crate::llm::{CompletionConfig, LlmProvider};

use super::fold::{MicroCapsule, StructuredFold};

/// Token budget guidance for the summarizer prompt. The LLM is asked
/// to compress N input tokens of conversation into ~`TARGET_FOLD_TOKENS`
/// of structured output. Below this, the fold is too sparse to be
/// useful; above, we lose the compression benefit.
const TARGET_FOLD_TOKENS: u32 = 800;

/// Max output tokens for the summarizer call. Generous — folds with
/// lots of decisions / failed_attempts can easily hit 1.5K.
const SUMMARIZER_MAX_TOKENS: u32 = 2048;

/// Errors from `summarize_to_fold`.
#[derive(Debug, thiserror::Error)]
pub enum SummarizeError {
    /// LLM API call itself failed (network, auth, rate limit, …).
    #[error("LLM call failed during fold summarization: {0}")]
    LlmFailed(#[source] Error),

    /// LLM returned text we couldn't parse as `StructuredFold` JSON.
    /// Includes the raw response so the caller can log it for debugging.
    #[error("LLM produced malformed StructuredFold JSON: {error}")]
    ParseFailed {
        error: String,
        raw_response: String,
    },

    /// Input was empty — nothing to summarize.
    #[error("no input messages to summarize")]
    EmptyInput,
}

/// Summarize `messages` (the about-to-be-compacted slice) into a
/// [`StructuredFold`] by calling `llm` with a structured-output prompt.
///
/// `model_id` selects which model handles the summarization — usually
/// the session's main model, but callers can downgrade to a cheaper
/// model (e.g. haiku) since the summarizer prompt is straightforward.
///
/// Idempotent — calling twice on the same input produces equivalent
/// folds (modulo LLM nondeterminism).
pub async fn summarize_to_fold(
    llm: Arc<dyn LlmProvider>,
    model_id: &str,
    messages: &[ChatMessage],
) -> Result<StructuredFold, SummarizeError> {
    if messages.is_empty() {
        return Err(SummarizeError::EmptyInput);
    }

    let transcript = render_transcript(messages);
    let system_prompt = build_system_prompt();
    let user_prompt = build_user_prompt(&transcript);

    let req_messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: vec![ContentBlock::Text { text: system_prompt }],
            compacted: false,
        },
        ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: user_prompt }],
            compacted: false,
        },
    ];

    let config = CompletionConfig {
        model: model_id.to_string(),
        max_tokens: SUMMARIZER_MAX_TOKENS,
        // Lower temperature — we want a deterministic-ish extraction,
        // not creative paraphrasing.
        temperature: 0.2,
        thinking_enabled: false,
    };

    let resp = llm
        .complete(req_messages, Vec::new(), &config)
        .await
        .map_err(SummarizeError::LlmFailed)?;

    let raw_text = extract_text(&resp);
    parse_fold_from_text(&raw_text).map_err(|e| SummarizeError::ParseFailed {
        error: e.to_string(),
        raw_response: raw_text,
    })
}

/// Extractive fallback to construct a basic [`StructuredFold`] consisting
/// of chronologically mapped [`MicroCapsule`]s directly from raw messages.
/// This acts as a robust fail-safe to guarantee zero-loss turn recall even
/// when the LLM service is down, misbehaving, or rate-limited.
pub fn extractive_fallback_fold(messages: &[ChatMessage]) -> StructuredFold {
    let mut capsules = Vec::new();
    let mut current_turn_index = 0;

    let mut current_user_query = String::new();
    let mut current_outcomes = Vec::new();

    for msg in messages {
        match msg.role {
            MessageRole::User => {
                // Flush the previous turn if we have one
                if !current_user_query.is_empty() {
                    let agent_outcome = if current_outcomes.is_empty() {
                        "No outcome recorded.".to_string()
                    } else {
                        current_outcomes.join("; ")
                    };
                    capsules.push(MicroCapsule {
                        turn_index: current_turn_index,
                        user_query: truncate_string(&current_user_query, 200),
                        agent_outcome: truncate_string(&agent_outcome, 300),
                    });
                    current_turn_index += 1;
                    current_outcomes.clear();
                }

                // Gather user query
                let mut parts = Vec::new();
                for block in &msg.content {
                    if let ContentBlock::Text { text } = block {
                        parts.push(text.trim().to_string());
                    }
                }
                current_user_query = parts.join(" ");
            }
            MessageRole::Assistant => {
                for block in &msg.content {
                    match block {
                        ContentBlock::Text { text } => {
                            if !text.trim().is_empty() {
                                current_outcomes.push(format!("Responded: {}", text.trim()));
                            }
                        }
                        ContentBlock::ToolUse { name, .. } => {
                            current_outcomes.push(format!("Called tool: {}", name));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    // Flush the last turn
    if !current_user_query.is_empty() {
        let agent_outcome = if current_outcomes.is_empty() {
            "No outcome recorded.".to_string()
        } else {
            current_outcomes.join("; ")
        };
        capsules.push(MicroCapsule {
            turn_index: current_turn_index,
            user_query: truncate_string(&current_user_query, 200),
            agent_outcome: truncate_string(&agent_outcome, 300),
        });
    }

    StructuredFold::default().with_micro_capsules(capsules)
}

/// 增量更新一份已有 fold:把 `prior_fold` 渲染为 markdown,连同**仅**自上次
/// 压缩以来的 `new_messages` 一起喂给 LLM,要求产出**完整的、更新后的**
/// StructuredFold JSON。输入 O(1)。复用 parse_fold_from_text / extract_text。
pub async fn update_fold_incremental(
    llm: Arc<dyn LlmProvider>,
    model_id: &str,
    prior_fold: &StructuredFold,
    new_messages: &[ChatMessage],
) -> Result<StructuredFold, SummarizeError> {
    if new_messages.is_empty() {
        return Ok(prior_fold.clone());
    }
    let prior_md = prior_fold.to_markdown();
    let transcript = render_transcript(new_messages);
    let system_prompt = build_update_system_prompt();
    let user_prompt = build_update_user_prompt(&prior_md, &transcript);

    let req_messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: vec![ContentBlock::Text { text: system_prompt }],
            compacted: false,
        },
        ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: user_prompt }],
            compacted: false,
        },
    ];
    let config = CompletionConfig {
        model: model_id.to_string(),
        max_tokens: SUMMARIZER_MAX_TOKENS,
        temperature: 0.2,
        thinking_enabled: false,
    };

    let resp = llm
        .complete(req_messages, Vec::new(), &config)
        .await
        .map_err(SummarizeError::LlmFailed)?;
    let raw_text = extract_text(&resp);
    parse_fold_from_text(&raw_text).map_err(|e| SummarizeError::ParseFailed {
        error: e.to_string(),
        raw_response: raw_text,
    })
}

fn build_update_system_prompt() -> String {
    format!(
        r#"You are UPDATING an existing structured conversation summary, not creating one from scratch.

You are given a PREVIOUS SUMMARY (the running compressed memory of this session) and a set of NEW MESSAGES that occurred since that summary was last produced. Produce a COMPLETE, UPDATED StructuredFold JSON object (same schema) that folds the new messages into the previous summary:

- facts / decisions: keep still-relevant ones; add new ones; when new evidence contradicts an old fact, prefer the new.
- next_actions: drop completed ones; add newly surfaced ones.
- unresolved_questions: drop resolved ones; add new ones.
- failed_attempts / active_constraints / rollback_points / evidence_refs: accumulate.
- file_ops: merge file operations seen in the new messages.
- micro_capsules: add capsules for the key new turns.

Output ONLY the JSON object, ~{target} tokens, no prose, no code fence."#,
        target = TARGET_FOLD_TOKENS
    )
}

fn build_update_user_prompt(prior_markdown: &str, new_transcript: &str) -> String {
    format!(
        "<previous_summary>\n{prior_markdown}\n</previous_summary>\n\n<new_messages>\n{new_transcript}\n</new_messages>\n\nReturn the complete updated StructuredFold JSON:"
    )
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

// ── prompt construction ───────────────────────────────────────────────

fn build_system_prompt() -> String {
    format!(
        r#"You are a structured-context summarizer for an autonomous coding agent.

Your job: read the conversation transcript below and extract its structured fold
as a JSON object. The fold replaces the verbatim conversation in the agent's
future context windows, so missing facts / decisions / failed-attempts will cause
the agent to re-discover them downstream — be thorough.

OUTPUT FORMAT (strict — return ONLY this JSON, no surrounding text):

{{
  "facts":               [ {{"statement": "...", "evidence": [{{"id": "...", "label": "..."}}], "confidence": 0.9 }} ],
  "decisions":           [ {{"decision": "...", "rationale": "...", "alternatives_considered": ["..."], "evidence": [] }} ],
  "unresolved_questions":[ "..." ],
  "evidence_refs":       [ {{"id": "rollout:...", "label": "..." }} ],
  "failed_attempts":     [ {{"what_was_tried": "...", "why_it_failed": "...", "evidence": null }} ],
  "active_constraints":  [],
  "next_actions":        [ "..." ],
  "rollback_points":     [ {{"id": "ckpt-...", "note": "..." }} ],
  "micro_capsules":      [ {{"turn_index": 1, "user_query": "user request verbatim", "agent_outcome": "what the agent tried and the brief result" }} ]
}}

Rules:

1. Every array MUST be present, even when empty. Use `[]` not `null`.
2. `confidence` is optional (0.0–1.0). Omit the field when you can't justify a score.
3. `evidence` arrays in facts/decisions/failed_attempts are optional but useful.
4. `active_constraints` may stay `[]` — the constraint type is internal.
5. Compress aggressively. Target output ~{target} tokens of JSON.
6. Preserve any tool / file / URL identifiers verbatim — agents need exact strings.
7. Do not add commentary, markdown, or apology text. JSON only.
8. Under 'micro_capsules', chronologically record EVERY conversation turn from the transcript. `turn_index` should match the index of the user request. Keep user_query as close to verbatim as possible (summarized only if extremely long), and agent_outcome as a concise summary of what was accomplished or observed during that turn. This is critical for exact turn-by-turn recollection."#,
        target = TARGET_FOLD_TOKENS
    )
}

fn build_user_prompt(transcript: &str) -> String {
    format!(
        "Conversation transcript to summarize:\n\n---\n{transcript}\n---\n\n\
        Produce the StructuredFold JSON object now.",
        transcript = transcript
    )
}

/// Render a flat text transcript from the messages. Includes role
/// labels and content text; tool calls get a compact `[tool_call name=X
/// input=...]` marker, tool results get `[tool_result for=Y]`.
///
/// Total length is roughly the input token budget — the caller is
/// responsible for trimming if they need to stay under the
/// summarizer's context window.
fn render_transcript(messages: &[ChatMessage]) -> String {
    let mut out = String::with_capacity(messages.len() * 256);
    for (idx, m) in messages.iter().enumerate() {
        let role = match m.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
        };
        out.push_str(&format!("[{idx}] {role}:\n"));
        for block in &m.content {
            match block {
                ContentBlock::Text { text } => {
                    out.push_str(text);
                    out.push('\n');
                }
                ContentBlock::Thinking { thinking, .. } => {
                    out.push_str("[thinking] ");
                    out.push_str(thinking);
                    out.push('\n');
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    out.push_str(&format!(
                        "[tool_call name={name} input={}]\n",
                        // Compact JSON to save chars — pretty-printed input would
                        // double the transcript size for no comprehension benefit.
                        serde_json::to_string(input).unwrap_or_else(|_| "{}".into())
                    ));
                }
                ContentBlock::ToolResult { content, is_error, .. } => {
                    let err = if is_error.unwrap_or(false) { " ERROR" } else { "" };
                    // Truncate very long tool results — the summarizer doesn't
                    // need the full payload, just the gist.
                    let preview = if content.len() > 800 {
                        format!("{}... [+{} bytes]", &content[..800], content.len() - 800)
                    } else {
                        content.clone()
                    };
                    out.push_str(&format!("[tool_result{err}] {preview}\n"));
                }
            }
        }
        out.push('\n');
    }
    out
}

pub(crate) fn extract_text(resp: &crate::agent::types::RespondOutput) -> String {
    use crate::agent::types::RespondOutput;
    match resp {
        RespondOutput::Text { text, .. } => text.clone(),
        // Summarizer prompt asks for JSON output only — tool calls are
        // unexpected but if they happen we fall back to whatever text
        // accompanied them (often empty).
        RespondOutput::ToolCalls { text, .. } => text.clone().unwrap_or_default(),
    }
}

/// Parse a `StructuredFold` from raw LLM text. Tolerant of common
/// LLM-wrapping patterns (fenced code blocks, leading prose) by
/// extracting the first balanced `{...}` substring.
pub(crate) fn parse_fold_from_text(text: &str) -> Result<StructuredFold, serde_json::Error> {
    // 1) Strip code-fence wrappers (```json … ```), if any.
    let stripped = strip_code_fence(text.trim());

    // 2) Try direct parse first — cheapest path.
    if let Ok(fold) = serde_json::from_str::<StructuredFold>(stripped) {
        return Ok(fold);
    }

    // 3) Fallback: find the first balanced JSON object substring.
    if let Some(span) = first_balanced_object(stripped) {
        if let Ok(fold) = serde_json::from_str::<StructuredFold>(span) {
            return Ok(fold);
        }
    }

    // 4) Surface the original error from the direct attempt for caller logging.
    serde_json::from_str::<StructuredFold>(stripped)
}

fn strip_code_fence(s: &str) -> &str {
    let s = s.trim();
    // ```json\n{...}\n```  or  ```\n{...}\n```
    if let Some(rest) = s.strip_prefix("```json") {
        return rest.trim_start_matches('\n').trim_end_matches("```").trim();
    }
    if let Some(rest) = s.strip_prefix("```") {
        return rest.trim_start_matches('\n').trim_end_matches("```").trim();
    }
    s
}

/// Find the first balanced `{...}` substring. Used when the LLM
/// wraps JSON in prose ("Here's the fold:\n{...}\nLet me know if...").
/// Naive: counts braces, ignores string-escape edge cases (LLMs
/// rarely emit `"}"` inside strings in this prompt).
fn first_balanced_object(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if escape {
            escape = false;
            continue;
        }
        match b {
            b'\\' if in_string => escape = true,
            b'"' => in_string = !in_string,
            b'{' if !in_string => depth += 1,
            b'}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::compact::fold::{FactWithEvidence, FailedAttempt};
    use crate::agent::types::{ResponseMetadata, RespondOutput, TokenUsage};
    use crate::error::Error;
    use crate::llm::CompletionConfig;
    use crate::agent::types::ToolDefinition;
    use std::sync::Mutex;

    // ── minimal capturing LLM mock ───────────────────────────────────

    struct CapturingMock {
        response_text: String,
        captured_user_prompt: Mutex<Option<String>>,
    }

    impl CapturingMock {
        fn new(response_text: &str) -> Arc<Self> {
            Arc::new(Self {
                response_text: response_text.to_string(),
                captured_user_prompt: Mutex::new(None),
            })
        }

        fn get_user_prompt(&self) -> String {
            self.captured_user_prompt
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_default()
        }
    }

    #[async_trait::async_trait]
    impl crate::llm::LlmProvider for CapturingMock {
        async fn complete(
            &self,
            messages: Vec<crate::agent::types::ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<RespondOutput, Error> {
            // Capture the user-role message content
            let user_content = messages
                .iter()
                .find(|m| matches!(m.role, MessageRole::User))
                .and_then(|m| {
                    m.content.iter().find_map(|b| {
                        if let ContentBlock::Text { text } = b {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_default();
            *self.captured_user_prompt.lock().unwrap() = Some(user_content);

            Ok(RespondOutput::Text {
                text: self.response_text.clone(),
                thinking: None,
                thinking_signature: None,
                metadata: ResponseMetadata {
                    model: "test-model".into(),
                    finish_reason: Some("end_turn".into()),
                    usage: None,
                },
            })
        }

        async fn stream(
            &self,
            _messages: Vec<crate::agent::types::ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<Box<dyn futures::Stream<Item = Result<crate::agent::types::StreamDelta, Error>> + Send + Unpin>, Error> {
            unimplemented!()
        }
    }

    // ── panic mock: must NOT be called ──────────────────────────────

    struct PanicMock;

    #[async_trait::async_trait]
    impl crate::llm::LlmProvider for PanicMock {
        async fn complete(
            &self,
            _messages: Vec<crate::agent::types::ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<RespondOutput, Error> {
            panic!("LLM must NOT be called when new_messages is empty");
        }

        async fn stream(
            &self,
            _messages: Vec<crate::agent::types::ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<Box<dyn futures::Stream<Item = Result<crate::agent::types::StreamDelta, Error>> + Send + Unpin>, Error> {
            unimplemented!()
        }
    }

    fn make_msg(role: MessageRole, text: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            compacted: false,
        }
    }

    #[test]
    fn render_transcript_includes_role_labels() {
        let msgs = vec![
            make_msg(MessageRole::User, "hi"),
            make_msg(MessageRole::Assistant, "hello"),
        ];
        let out = render_transcript(&msgs);
        assert!(out.contains("[0] user:"));
        assert!(out.contains("hi"));
        assert!(out.contains("[1] assistant:"));
        assert!(out.contains("hello"));
    }

    #[test]
    fn render_transcript_compact_serializes_tool_use() {
        let msg = ChatMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "tu_1".into(),
                name: "shell".into(),
                input: serde_json::json!({"cmd": "ls"}),
            }],
            compacted: false,
        };
        let out = render_transcript(&[msg]);
        assert!(out.contains("[tool_call name=shell input={\"cmd\":\"ls\"}"));
    }

    #[test]
    fn render_transcript_truncates_long_tool_results() {
        let big = "x".repeat(2000);
        let msg = ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "tu_1".into(),
                content: big,
                is_error: None,
            }],
            compacted: false,
        };
        let out = render_transcript(&[msg]);
        assert!(out.contains("[+1200 bytes]"));
    }

    #[test]
    fn parse_fold_from_text_handles_clean_json() {
        let raw = r#"{
            "facts": [{"statement": "A", "evidence": []}],
            "decisions": [],
            "unresolved_questions": [],
            "evidence_refs": [],
            "failed_attempts": [],
            "active_constraints": [],
            "next_actions": ["B"],
            "rollback_points": []
        }"#;
        let fold = parse_fold_from_text(raw).unwrap();
        assert_eq!(fold.facts.len(), 1);
        assert_eq!(fold.next_actions, vec!["B"]);
    }

    #[test]
    fn parse_fold_from_text_strips_json_code_fence() {
        let raw = "```json\n{\"facts\": [], \"decisions\": [], \"unresolved_questions\": [], \"evidence_refs\": [], \"failed_attempts\": [], \"active_constraints\": [], \"next_actions\": [], \"rollback_points\": []}\n```";
        let fold = parse_fold_from_text(raw).unwrap();
        assert!(fold.is_empty());
    }

    #[test]
    fn parse_fold_from_text_strips_plain_code_fence() {
        let raw = "```\n{\"facts\": [], \"decisions\": [], \"unresolved_questions\": [], \"evidence_refs\": [], \"failed_attempts\": [], \"active_constraints\": [], \"next_actions\": [], \"rollback_points\": []}\n```";
        let fold = parse_fold_from_text(raw).unwrap();
        assert!(fold.is_empty());
    }

    #[test]
    fn parse_fold_from_text_extracts_from_surrounding_prose() {
        let raw = "Here's the structured fold for your conversation:\n\n{\"facts\":[{\"statement\":\"A\",\"evidence\":[]}],\"decisions\":[],\"unresolved_questions\":[],\"evidence_refs\":[],\"failed_attempts\":[],\"active_constraints\":[],\"next_actions\":[],\"rollback_points\":[]}\n\nLet me know if you want adjustments.";
        let fold = parse_fold_from_text(raw).unwrap();
        assert_eq!(fold.facts.len(), 1);
        assert_eq!(fold.facts[0].statement, "A");
    }

    #[test]
    fn parse_fold_from_text_handles_nested_braces() {
        // Realistic: tool_use input has nested braces.
        let raw = r#"{
            "facts": [{"statement": "config is {a:1, b:{c:2}}", "evidence": []}],
            "decisions": [],
            "unresolved_questions": [],
            "evidence_refs": [],
            "failed_attempts": [],
            "active_constraints": [],
            "next_actions": [],
            "rollback_points": []
        }"#;
        let fold = parse_fold_from_text(raw).unwrap();
        assert_eq!(fold.facts.len(), 1);
    }

    #[test]
    fn parse_fold_from_text_fails_on_garbage() {
        let result = parse_fold_from_text("not a fold at all");
        assert!(result.is_err());
    }

    #[test]
    fn first_balanced_object_finds_outer_braces() {
        let s = "prefix {\"a\": {\"b\": 1}} suffix";
        assert_eq!(first_balanced_object(s).unwrap(), "{\"a\": {\"b\": 1}}");
    }

    #[test]
    fn first_balanced_object_ignores_braces_in_strings() {
        let s = r#"{"a": "has } in string"}"#;
        assert_eq!(first_balanced_object(s).unwrap(), s);
    }

    #[test]
    fn build_system_prompt_includes_token_target() {
        let p = build_system_prompt();
        assert!(p.contains("800"));
        assert!(p.contains("structured fold"));
        assert!(p.contains("micro_capsules"));
    }

    // ── update_fold_incremental tests ─────────────────────────────────

    #[tokio::test]
    async fn update_fold_incremental_feeds_prior_fold_and_only_new_messages() {
        use crate::agent::compact::fold::FactWithEvidence;
        let prior = StructuredFold::default().with_facts(vec![FactWithEvidence {
            statement: "auth uses JWT".into(),
            evidence: vec![],
            confidence: None,
        }]);
        // JSON response merging prior + new fact
        let response = r#"{"facts":[{"statement":"auth uses JWT","evidence":[]},{"statement":"added refresh tokens","evidence":[]}],"decisions":[],"unresolved_questions":[],"evidence_refs":[],"failed_attempts":[],"active_constraints":[],"next_actions":["ship"],"rollback_points":[]}"#;
        let llm = CapturingMock::new(response);

        let new_msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: "NEW_MESSAGE_MARKER add refresh tokens".into(),
            }],
            compacted: false,
        }];

        let updated = update_fold_incremental(llm.clone(), "test-model", &prior, &new_msgs)
            .await
            .unwrap();

        assert!(
            updated.facts.iter().any(|f| f.statement.contains("refresh tokens")),
            "updated fold should contain new refresh tokens fact"
        );
        assert!(
            updated.facts.iter().any(|f| f.statement.contains("JWT")),
            "updated fold should retain prior JWT fact"
        );

        let prompt = llm.get_user_prompt();
        assert!(
            prompt.contains("auth uses JWT"),
            "prompt should carry prior fold markdown; got: {prompt}"
        );
        assert!(
            prompt.contains("NEW_MESSAGE_MARKER"),
            "prompt should carry new messages; got: {prompt}"
        );
    }

    #[tokio::test]
    async fn update_fold_incremental_empty_new_messages_returns_prior() {
        use crate::agent::compact::fold::FactWithEvidence;
        let prior = StructuredFold::default().with_facts(vec![FactWithEvidence {
            statement: "auth uses JWT".into(),
            evidence: vec![],
            confidence: None,
        }]);
        let llm: Arc<dyn crate::llm::LlmProvider> = Arc::new(PanicMock);
        let out = update_fold_incremental(llm, "test-model", &prior, &[])
            .await
            .unwrap();
        assert_eq!(out, prior, "empty new_messages must return prior fold unchanged");
    }
}
