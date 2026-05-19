//! Sprint 2.4a — chat-turn auto-extractor for gbrain.
//!
//! Sprint 2.3 (PR #223) gave the agent system-prompt instructions telling
//! it when to call `mcp__gbrain__put_page`. That covers the reactive path
//! — agent decides what to remember during its own reasoning.
//!
//! This module is the **proactive** path: after every user-facing chat
//! turn (user message + assistant response), run a Haiku-cheap extractor
//! to spot entities/facts the agent _didn't_ explicitly persist but
//! probably should have. Returns 0+ [`GbrainPageProposal`]s. Consumer
//! (`agent::dispatcher::ChatDelegate` in Sprint 2.4b) decides whether
//! to actually call `mcp__gbrain__put_page` based on confidence
//! threshold + daily token budget.
//!
//! Mirrors the shape of `crate::learning::extractor` so the two
//! producers can sit side-by-side in `ChatDelegate::before_llm_call`
//! with the same cost-recording + budget-gating pattern.
//!
//! ## Cost tagging
//!
//! Every LLM call passes `"gbrain_extract"` to `MemoryOsLlm::complete_text`.
//! That tag becomes the prefix in `cost_records.model` (e.g.
//! `"gbrain_extract:claude-haiku-4-5-20251001"`), which lets
//! `cost_store::today_gbrain_extract_tokens` (Sprint 2.4b) sum the
//! daily spend with `LIKE 'gbrain_extract%'` and gate further calls
//! once the budget is exhausted. Symmetrical to Sprint 2.1b's
//! `memory_learning` tag.
//!
//! ## Non-goals
//!
//! - This module DOES NOT call `mcp__gbrain__put_page`. It returns
//!   proposals; the dispatcher (Sprint 2.4b) is the consumer.
//! - This module DOES NOT enforce daily budgets. The dispatcher checks
//!   the budget _before_ calling `extract_from_chat_turn` so we never
//!   spawn the LLM call when we're already over.

use std::sync::Arc;

use crate::memory_graph::memory_os_llm::MemoryOsLlm;

/// Minimum confidence the LLM must report before downstream consumers
/// treat a proposal as actionable. Below this we still surface the
/// proposal for telemetry (and Sprint 2.5+ batched review) but the
/// dispatcher won't auto-fire `put_page`. Tuned empirically against
/// the prompt in [`extract_system_prompt`]; rebalance once Sprint
/// 2.4b ships and we have real confidence histograms.
pub const MIN_ACT_CONFIDENCE: f32 = 0.7;

/// Hard cap on `max_tokens` requested from the extractor LLM. Chosen
/// so even the most prolific Haiku response fits under one
/// `cost_records` row's expected size, and so a malicious / hallucinating
/// LLM can't burn the whole daily budget on a single chat turn.
pub const EXTRACT_MAX_TOKENS: u32 = 800;

/// Minimum combined char count (user_message + assistant_response) before
/// the LLM extractor runs. Short turns ("hi", "thanks") essentially
/// never carry persistent knowledge and would just burn tokens.
/// Mirrors `learning::extractor::LLM_MIN_TURN_CHARS` shape (different
/// numeric tuning because gbrain wants more substantive content than
/// facet extraction).
pub const LLM_MIN_TURN_CHARS: usize = 80;

/// Sprint 2.4 — one page proposal output by the LLM extractor for a
/// single chat turn. The dispatcher (Sprint 2.4b) decides whether to
/// call `mcp__gbrain__put_page` based on `confidence >= MIN_ACT_CONFIDENCE`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct GbrainPageProposal {
    /// kebab-case english identifier (gbrain convention). The extractor
    /// is instructed to namespace when natural (e.g. "person-jane-doe",
    /// "decision-ports-trigram-2026"); a bare slug like "openai-gpt-5"
    /// is also fine.
    pub slug: String,
    /// YAML frontmatter + markdown body. Same shape Sprint 2.3 told the
    /// agent to use for explicit put_page calls so the two ingestion
    /// paths converge on one page format downstream.
    pub content: String,
    /// 0.0..1.0 — extractor LLM's self-rated confidence that this page
    /// is worth long-term retention. The dispatcher checks
    /// `>= MIN_ACT_CONFIDENCE` before auto-firing put_page. Values
    /// outside [0.0, 1.0] are clamped at parse time.
    pub confidence: f32,
}

/// System prompt for the extractor LLM. Returned as `&'static str` so the
/// caller can pass it through `MemoryOsLlm::complete_text` without
/// cloning. Kept in its own fn (not a const) so tests can grep for it
/// and future versions can A/B against alternates without touching the
/// const-naming convention.
fn extract_system_prompt() -> &'static str {
    "You are a knowledge extractor for a personal AI assistant's long-term \
memory system (gbrain). Given one chat turn (user message + assistant \
response), identify any **entities** (people, projects, companies, \
concepts) or **stable facts** that would be valuable for the assistant \
to remember across future conversations.\n\
\n\
Output a JSON array of page proposals. Each proposal:\n\
- `slug`: kebab-case english identifier. Namespace when natural \
(`person-jane-doe`, `decision-...`, `project-...`). Required.\n\
- `content`: YAML frontmatter (with `title`, `type`, optional `aliases`/`tags`) \
+ markdown body. Keep bodies under 300 words. Link sub-pages via `[[other-slug]]`. Required.\n\
- `confidence`: 0.0–1.0 self-rated. Use >=0.7 only when the fact is \
explicit and stable (proper nouns, dated events, user-introduced \
identities). Use 0.5–0.7 for inferred but useful info. Use <0.5 for \
ephemeral / one-off conversational details.\n\
\n\
Return an empty array `[]` if the turn carries no persistent knowledge \
(small talk, this-turn-only questions, hypotheticals).\n\
\n\
Output ONLY the JSON array — no markdown fences, no prose."
}

/// Sprint 2.4a — run one Haiku-cheap extraction over a single chat turn.
/// Returns 0+ page proposals. Empty Vec on:
/// - Combined input shorter than [`LLM_MIN_TURN_CHARS`]
/// - LLM call error (logged + swallowed — don't poison the agent loop)
/// - LLM returns a non-JSON-array response (logged + swallowed)
/// - LLM returns an empty array (the "no persistent knowledge in this
///   turn" signal)
///
/// Callers are responsible for the daily-budget check BEFORE invoking
/// this fn — `crate::cost_store::today_gbrain_extract_tokens` returns
/// the running spend and Sprint 2.4b in `ChatDelegate` short-circuits
/// the extractor when over budget.
pub async fn extract_from_chat_turn(
    user_message: &str,
    assistant_response: &str,
    llm: &Arc<dyn MemoryOsLlm>,
) -> Vec<GbrainPageProposal> {
    let combined_chars = user_message.chars().count() + assistant_response.chars().count();
    if combined_chars < LLM_MIN_TURN_CHARS {
        tracing::debug!(
            combined_chars,
            min = LLM_MIN_TURN_CHARS,
            "gbrain chat_extractor: skipping — turn too short"
        );
        return vec![];
    }

    let user_prompt = format!(
        "User: {}\n\nAssistant: {}",
        user_message.trim(),
        assistant_response.trim()
    );
    let out = match llm
        .complete_text(
            "gbrain_extract",
            extract_system_prompt(),
            &user_prompt,
            EXTRACT_MAX_TOKENS,
        )
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(error = %e, "gbrain chat_extractor: LLM call failed");
            return vec![];
        }
    };

    parse_proposals(&out.text)
}

/// Parse the LLM's JSON response into a `Vec<GbrainPageProposal>`. Lenient
/// by design — tolerates trailing whitespace, markdown code fences the
/// model added despite instructions, and out-of-range confidence values.
/// Returns empty Vec on any parse failure (logged at warn level).
fn parse_proposals(raw: &str) -> Vec<GbrainPageProposal> {
    let body = strip_markdown_fences(raw.trim());
    let proposals: Vec<GbrainPageProposal> = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                error = %e,
                raw_preview = &body.chars().take(120).collect::<String>(),
                "gbrain chat_extractor: failed to parse LLM response as JSON array"
            );
            return vec![];
        }
    };
    // Clamp confidence + drop empty-slug rows (defensive against a
    // hallucinated proposal with no identifier).
    proposals
        .into_iter()
        .filter_map(|mut p| {
            let slug = p.slug.trim();
            if slug.is_empty() || p.content.trim().is_empty() {
                return None;
            }
            p.slug = slug.to_string();
            p.confidence = p.confidence.clamp(0.0, 1.0);
            Some(p)
        })
        .collect()
}

/// If `body` is wrapped in a ``` fence (the LLM sometimes ignores "no
/// markdown fences" in the prompt), return the inner content. Otherwise
/// return `body` unchanged. Idempotent and case-insensitive on the
/// language hint.
fn strip_markdown_fences(body: &str) -> &str {
    let trimmed = body.trim();
    if !trimmed.starts_with("```") {
        return body;
    }
    // Find the first newline after the opening fence (may include a
    // language hint like ```json on the same line).
    let after_open = match trimmed.find('\n') {
        Some(idx) => &trimmed[idx + 1..],
        None => return body,
    };
    if let Some(end) = after_open.rfind("```") {
        after_open[..end].trim_end()
    } else {
        after_open
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::memory_os_llm::MockMemoryOsLlm;

    fn mock_llm(canned_text: &str) -> Arc<dyn MemoryOsLlm> {
        Arc::new(MockMemoryOsLlm {
            canned_text: canned_text.to_string(),
            ..MockMemoryOsLlm::default()
        }) as Arc<dyn MemoryOsLlm>
    }

    #[tokio::test]
    async fn extract_returns_empty_when_turn_too_short() {
        // Below LLM_MIN_TURN_CHARS the function never invokes the LLM —
        // so the canned response (well-formed JSON) is irrelevant.
        let llm = mock_llm(r#"[{"slug":"x","content":"y","confidence":0.9}]"#);
        let result = extract_from_chat_turn("hi", "hello!", &llm).await;
        assert!(
            result.is_empty(),
            "short turn should skip extractor, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn extract_parses_canned_proposals_above_threshold() {
        let user_msg = "OpenAI released GPT-5 on May 18 2026. Major upgrade over GPT-4.";
        let assistant_msg = "Got it — GPT-5 is a major upgrade released 2026-05-18.";
        let canned = r#"[
            {
                "slug": "openai-gpt-5-release",
                "content": "---\ntitle: GPT-5 Release\ntype: event\n---\n\nOpenAI released GPT-5 on 2026-05-18.",
                "confidence": 0.92
            }
        ]"#;
        let llm = mock_llm(canned);
        let result = extract_from_chat_turn(user_msg, assistant_msg, &llm).await;
        assert_eq!(result.len(), 1, "expected 1 proposal, got {:?}", result);
        assert_eq!(result[0].slug, "openai-gpt-5-release");
        assert!(result[0].confidence > MIN_ACT_CONFIDENCE);
        assert!(result[0].content.contains("title: GPT-5 Release"));
    }

    #[tokio::test]
    async fn extract_handles_empty_array_response() {
        let user_msg = "What's 2+2? Walking through a long problem here so the message is long enough.";
        let assistant_msg = "2+2 is 4. Let me know if you want me to walk through it.";
        let llm = mock_llm("[]");
        let result = extract_from_chat_turn(user_msg, assistant_msg, &llm).await;
        assert!(result.is_empty(), "empty array → no proposals");
    }

    #[test]
    fn parse_strips_markdown_fence_with_language_hint() {
        let raw = "```json\n[{\"slug\":\"x\",\"content\":\"body\",\"confidence\":0.8}]\n```";
        let result = parse_proposals(raw);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slug, "x");
    }

    #[test]
    fn parse_clamps_out_of_range_confidence() {
        let raw = r#"[
            {"slug": "a", "content": "x", "confidence": 1.5},
            {"slug": "b", "content": "y", "confidence": -0.2}
        ]"#;
        let result = parse_proposals(raw);
        assert_eq!(result.len(), 2);
        assert!((result[0].confidence - 1.0).abs() < f32::EPSILON);
        assert!((result[1].confidence - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_drops_proposals_with_empty_slug_or_content() {
        let raw = r#"[
            {"slug": "", "content": "body", "confidence": 0.9},
            {"slug": "ok", "content": "", "confidence": 0.9},
            {"slug": "  ", "content": "body", "confidence": 0.9},
            {"slug": "valid", "content": "real body", "confidence": 0.8}
        ]"#;
        let result = parse_proposals(raw);
        assert_eq!(result.len(), 1, "only the fully-populated row should survive");
        assert_eq!(result[0].slug, "valid");
    }

    #[test]
    fn parse_returns_empty_on_invalid_json() {
        let result = parse_proposals("not a json array at all");
        assert!(result.is_empty());
    }
}
