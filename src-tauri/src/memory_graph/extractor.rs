//! Step 3b-3 — native memory extractor (replaces memU's `memorize`).
//!
//! Single JSON-mode LLM call (via `MemoryOsLlm`, cost-tag "memory_extract")
//! that extracts typed memory items from a conversation. Mirrors
//! `gbrain::chat_extractor`'s prompt->parse pattern. Output feeds
//! `reflection::persist_items_to_graph` (memory_graph node creation).

use std::sync::Arc;

use crate::memory_graph::memory_os_llm::MemoryOsLlm;

/// Hard cap on extractor `max_tokens` (typed-item lists are compact).
pub const EXTRACT_MAX_TOKENS: u32 = 1200;
/// Minimum input chars before invoking the LLM (short turns carry no memory).
pub const LLM_MIN_CHARS: usize = 12;

/// One extracted memory item. `memory_type` in profile|event|knowledge|
/// behavior|skill|tool — the taxonomy `map_memu_type_to_kind` consumes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ExtractedItem {
    pub memory_type: String,
    pub content: String,
}

pub struct MemoryExtractor {
    llm: Arc<dyn MemoryOsLlm>,
}

impl MemoryExtractor {
    pub fn new(llm: Arc<dyn MemoryOsLlm>) -> Self {
        Self { llm }
    }

    /// Extract typed items from conversation text. Empty Vec on short input,
    /// LLM error, or unparseable response (all logged + swallowed).
    pub async fn extract(&self, conversation: &str) -> Vec<ExtractedItem> {
        if conversation.chars().count() < LLM_MIN_CHARS {
            return vec![];
        }
        let out = match self
            .llm
            .complete_text(
                "memory_extract",
                extract_system_prompt(),
                conversation.trim(),
                EXTRACT_MAX_TOKENS,
            )
            .await
        {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(error = %e, "memory_extractor: LLM call failed");
                return vec![];
            }
        };
        parse_items(&out.text)
    }
}

/// Ported from memU's per-type extraction rules, condensed into one
/// JSON-output prompt.
fn extract_system_prompt() -> &'static str {
    "You extract long-term memory items from a conversation for a personal AI \
assistant. Output a JSON array; each item is {\"memory_type\": <type>, \"content\": <text>}.\n\
\n\
Types and rules:\n\
- profile: stable user traits/preferences/attributes. <30 words. Direct facts, \
NO meta-phrasing (\"Enjoys table tennis\" NOT \"User said they like...\"). No timestamps.\n\
- event: time-bound concrete happenings (explicit or implicit time). <50 words. \
NOT general statements.\n\
- knowledge: objective factual knowledge discussed. <50 words. No personal opinions.\n\
- behavior: recurring patterns/routines/solutions (NOT one-off events). <50 words.\n\
- skill: an actionable skill the user/assistant demonstrated — name + what it does + when to use.\n\
- tool: a tool usage pattern/learning — include when to use it.\n\
\n\
Extract in the conversation's primary language (Chinese->Chinese, English->English). \
Only emit items carrying durable value; return [] for small talk / this-turn-only content. \
Output ONLY the JSON array — no markdown fences, no prose."
}

fn parse_items(raw: &str) -> Vec<ExtractedItem> {
    let body = strip_markdown_fences(raw.trim());
    let items: Vec<ExtractedItem> = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                error = %e,
                raw_preview = &body.chars().take(120).collect::<String>(),
                "memory_extractor: failed to parse LLM response as JSON array"
            );
            return vec![];
        }
    };
    const VALID: [&str; 6] = ["profile", "event", "knowledge", "behavior", "skill", "tool"];
    items
        .into_iter()
        .filter_map(|mut it| {
            let mt = it.memory_type.trim().to_lowercase();
            if !VALID.contains(&mt.as_str()) || it.content.trim().is_empty() {
                return None;
            }
            it.memory_type = mt;
            it.content = it.content.trim().to_string();
            Some(it)
        })
        .collect()
}

fn strip_markdown_fences(body: &str) -> &str {
    let trimmed = body.trim();
    if !trimmed.starts_with("```") {
        return body;
    }
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
    async fn extract_parses_typed_items() {
        let canned = r#"[{"memory_type":"profile","content":"Enjoys table tennis"},{"memory_type":"event","content":"Has a competition tomorrow"}]"#;
        let ex = MemoryExtractor::new(mock_llm(canned));
        let items = ex.extract("A reasonably long conversation about hobbies and plans.").await;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].memory_type, "profile");
        assert_eq!(items[1].memory_type, "event");
    }

    #[tokio::test]
    async fn extract_empty_array_yields_no_items() {
        let ex = MemoryExtractor::new(mock_llm("[]"));
        assert!(ex.extract("Some sufficiently long small talk here.").await.is_empty());
    }

    #[tokio::test]
    async fn extract_short_input_skips_llm() {
        // canned would parse, but input is below LLM_MIN_CHARS so LLM never runs
        let ex = MemoryExtractor::new(mock_llm(r#"[{"memory_type":"profile","content":"x"}]"#));
        assert!(ex.extract("hi").await.is_empty());
    }

    #[test]
    fn parse_strips_fences_and_drops_invalid() {
        let raw = "```json\n[{\"memory_type\":\"profile\",\"content\":\"ok\"},{\"memory_type\":\"bogus\",\"content\":\"x\"},{\"memory_type\":\"event\",\"content\":\"  \"}]\n```";
        let items = parse_items(raw);
        assert_eq!(items.len(), 1, "only the valid profile item survives");
        assert_eq!(items[0].memory_type, "profile");
    }

    #[test]
    fn parse_returns_empty_on_invalid_json() {
        assert!(parse_items("not json").is_empty());
    }
}
