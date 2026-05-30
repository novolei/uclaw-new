// SPDX-License-Identifier: Apache-2.0
//! Renders bucket_seal hybrid-recall results into a system-prompt block.
//!
//! Mirror of `gbrain_prompt::GbrainKnowledgeSection` by shape (marker const +
//! render fn), but injects RETRIEVED CONTENT rather than tool-usage
//! instructions. Called best-effort from the chat/agent send sites and appended
//! to the per-turn memory context via `delegate.append_memory_context`.

use crate::memory_adapter::MemoryEntry;

/// Marker used in logs and tests (mirrors `GBRAIN_SECTION_MARKER`).
pub const BUCKET_SEAL_RECALL_MARKER: &str = "## Relevant Memory (bucket-seal)";

/// Cheap token estimate (chars/4) — recall budgeting only.
fn est_tokens(s: &str) -> usize {
    s.chars().count().div_ceil(4)
}

/// Render recalled entries into a prompt block (summaries-first ordering is
/// the caller's responsibility — `recall_hybrid` already guarantees it).
///
/// Greedy budget fill — stops once adding the next entry would exceed
/// `token_budget`. Returns `None` when entries is empty or nothing fits.
pub fn render_bucket_seal_recall(entries: &[MemoryEntry], token_budget: usize) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    let mut body = String::new();
    let mut used = est_tokens(BUCKET_SEAL_RECALL_MARKER);
    for (i, e) in entries.iter().enumerate() {
        let ns = e.namespace.as_deref().unwrap_or("");
        let score = e.score.unwrap_or(0.0);
        let line = format!("- [{score:.2} · {ns}] {}\n", e.content.trim());
        let cost = est_tokens(&line);
        // Always include the first entry even if it alone exceeds the budget;
        // this ensures we always emit at least one result when entries exist.
        if i > 0 && used + cost > token_budget {
            break;
        }
        body.push_str(&line);
        used += cost;
    }
    if body.is_empty() {
        return None;
    }
    Some(format!("{BUCKET_SEAL_RECALL_MARKER}\n\n{body}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_adapter::{MemoryCategory, MemoryEntry};

    fn entry(id: &str, content: &str, score: f64) -> MemoryEntry {
        MemoryEntry {
            id: id.into(),
            key: id.into(),
            content: content.into(),
            namespace: Some("ns1".into()),
            category: MemoryCategory::Conversation,
            timestamp: "2026-05-30T00:00:00Z".into(),
            session_id: None,
            score: Some(score),
        }
    }

    #[test]
    fn empty_entries_returns_none() {
        assert!(render_bucket_seal_recall(&[], 1500).is_none());
    }

    #[test]
    fn renders_marker_and_entries() {
        let block = render_bucket_seal_recall(&[entry("s1", "alpha recap", 0.9)], 1500).unwrap();
        assert!(block.contains(BUCKET_SEAL_RECALL_MARKER));
        assert!(block.contains("alpha recap"));
    }

    #[test]
    fn budget_truncates() {
        let big = "x".repeat(8000); // ~2000 tokens at chars/4
        let entries = vec![entry("s1", &big, 0.9), entry("s2", "second", 0.8)];
        let block = render_bucket_seal_recall(&entries, 100).unwrap(); // ~400 chars budget
        // First entry alone exceeds the budget → second entry dropped.
        assert!(block.contains(BUCKET_SEAL_RECALL_MARKER));
        assert!(!block.contains("second"), "budget should truncate before the second entry");
    }
}
