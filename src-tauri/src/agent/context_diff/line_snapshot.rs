//! Bundle 16-A — Line-level snapshot + diff for a single named
//! fragment (typically `memory_context`).
//!
//! M2-D's pilot snapshot (`FragmentSnapshot` in `diff.rs`) is a
//! single djb2 of the whole fragment — any 1-byte change marks the
//! whole thing as "changed". For Track A of M2-D Phase 2 we need
//! line-level granularity so the cross-turn diff can name which
//! lines added / removed / changed instead of "the block drifted".
//!
//! ## Key derivation
//!
//! The original spec (`docs/superpowers/specs/2026-05-21-m2d-phase2-
//! design.md`) calls for using `MemoryItem::id` as the stable line
//! key. In practice `memory_context` reaches the dispatcher as a
//! pre-rendered string (see `tauri_commands.rs::send_agent_message`
//! around the `delegate.set_memory_context(memory_ctx)` site), so
//! the per-item ids are lost by then.
//!
//! Pragmatic fallback used here (matches spec Risk #1's mitigation):
//!
//! - **key** = `prefix_key(line)` — the bullet's "label" portion:
//!   `- 用户偏好: Chinese` → `"用户偏好"`. Stable under value
//!   changes (preferred_language: en → preferred_language: zh stays
//!   on the same key) but fails when the renderer changes the
//!   prefix punctuation. The fallback when no prefix can be
//!   extracted is `sha1(line)[..8]` — every distinct line is its
//!   own key, and "changed" reduces to (removed-old + added-new).
//! - **hash** = djb2 of full line content. Cheap, stable, the diff
//!   only cares about string equality.
//!
//! ## Diff semantics
//!
//! `line_diff(prior, new)` returns a `LineDiff`:
//!
//! - `added`: lines in `new` whose key isn't in `prior`.
//! - `removed`: lines in `prior` whose key isn't in `new`.
//! - `changed`: same key, different hash. The diff carries the new
//!   line content so the dispatcher can render the delta block
//!   without a second pass over the raw fragment.
//! - `unchanged_count`: lines present + identical-hash on both
//!   sides.
//!
//! The algorithm mirrors `diff_snapshots` in `diff.rs` so the
//! per-line and per-fragment diffs share semantics.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One line inside a fragment snapshot. `key` is the stable line
/// identity (see module docs for derivation), `hash` distinguishes
/// "same line, different value" from "same line, same value", and
/// `content` is the rendered text so the dispatcher can re-emit it
/// in a `<memory_context_changes>` block without re-parsing the
/// raw fragment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineEntry {
    pub key: String,
    pub hash: String,
    pub content: String,
}

impl LineEntry {
    /// Construct from a raw line of text using the default
    /// `prefix_key` extraction. Pure helper — callers that already
    /// have a stable id can build `LineEntry` directly via struct
    /// literal.
    pub fn from_line(line: &str) -> Self {
        let key = prefix_key(line).unwrap_or_else(|| sha1_prefix_key(line));
        Self {
            key,
            hash: djb2_hex(line),
            content: line.to_string(),
        }
    }
}

/// All `LineEntry`s for a single fragment + a backreference to the
/// fragment it came from. `fragment_label` is human-readable
/// (e.g. `"memory_context"`); it's only used in telemetry — diff
/// logic is purely line-vs-line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineFragmentSnapshot {
    pub fragment_label: String,
    pub lines: Vec<LineEntry>,
    pub token_estimate: usize,
}

impl LineFragmentSnapshot {
    /// Build a snapshot from the rendered fragment text. Skips
    /// fully-empty lines (so blank separators don't show as
    /// "added/removed" when the renderer subtly changes whitespace).
    pub fn from_text(label: impl Into<String>, text: &str) -> Self {
        let mut lines = Vec::new();
        for raw in text.lines() {
            let trimmed = raw.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            lines.push(LineEntry::from_line(trimmed));
        }
        Self {
            fragment_label: label.into(),
            // Token estimate matches the existing M2-D heuristic
            // (`ctx.len() / 4`) so the cross-cut budget math stays
            // consistent with what the pilot snapshot reported.
            token_estimate: text.len() / 4,
            lines,
        }
    }

    /// Total entry count — convenience for telemetry.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

/// One line that changed between two snapshots. Stable key, prior
/// hash + new hash so the LLM-facing annotation can render
/// "preferred_language: en → preferred_language: zh".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedLine {
    pub key: String,
    pub prior_hash: String,
    pub new_hash: String,
    pub prior_content: String,
    pub new_content: String,
}

/// Result of diffing two `LineFragmentSnapshot`s. Same shape as
/// `ContextDiff` in `diff.rs`, just at line granularity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineDiff {
    pub added: Vec<LineEntry>,
    pub removed: Vec<LineEntry>,
    pub changed: Vec<ChangedLine>,
    pub unchanged_count: usize,
}

impl LineDiff {
    /// `true` when nothing changed across two snapshots.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// Aggregate stats — matches the `DiffStats` shape in `diff.rs`.
    pub fn stats(&self) -> LineDiffStats {
        let added_or_changed_tokens = self
            .added
            .iter()
            .map(|e| token_estimate_for_line(&e.content))
            .chain(
                self.changed
                    .iter()
                    .map(|c| token_estimate_for_line(&c.new_content)),
            )
            .sum();
        LineDiffStats {
            added: self.added.len(),
            removed: self.removed.len(),
            changed: self.changed.len(),
            unchanged: self.unchanged_count,
            added_or_changed_tokens,
        }
    }
}

/// `LineDiff` summary stats. The `is_significant_change` heuristic
/// gates the dispatcher's "send delta annotation vs. send full
/// block only" decision (Bundle 16-B).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineDiffStats {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub added_or_changed_tokens: usize,
}

impl LineDiffStats {
    pub fn total_prior(&self) -> usize {
        self.removed + self.changed + self.unchanged
    }

    pub fn total_new(&self) -> usize {
        self.added + self.changed + self.unchanged
    }

    /// Returns `true` when drift exceeds `threshold_fraction` of
    /// prior-line count (removed + changed). Mirrors
    /// `DiffStats::is_significant_change` at line granularity.
    pub fn is_significant_change(&self, threshold_fraction: f32) -> bool {
        let prior = self.total_prior();
        if prior == 0 {
            return false;
        }
        let drift = self.removed + self.changed;
        (drift as f32) / (prior as f32) >= threshold_fraction
    }
}

/// Diff two line-level snapshots. O(n + m) using the key-indexed
/// HashMap, same algorithm as `diff_snapshots`.
pub fn line_diff(prior: &LineFragmentSnapshot, new: &LineFragmentSnapshot) -> LineDiff {
    // Build prior index keyed by line key. If the renderer ever
    // produces duplicate keys within a single fragment (e.g. two
    // bullets with the same label), the later one wins — for now
    // that's a renderer bug to fix upstream, not a diff bug.
    let mut prior_index: HashMap<&str, &LineEntry> =
        prior.lines.iter().map(|e| (e.key.as_str(), e)).collect();

    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged_count = 0usize;

    for new_entry in &new.lines {
        match prior_index.remove(new_entry.key.as_str()) {
            None => added.push(new_entry.clone()),
            Some(prior_entry) if prior_entry.hash == new_entry.hash => {
                unchanged_count += 1;
            }
            Some(prior_entry) => {
                changed.push(ChangedLine {
                    key: new_entry.key.clone(),
                    prior_hash: prior_entry.hash.clone(),
                    new_hash: new_entry.hash.clone(),
                    prior_content: prior_entry.content.clone(),
                    new_content: new_entry.content.clone(),
                });
            }
        }
    }

    // Whatever's left in the index was in prior but not in new.
    let mut removed: Vec<LineEntry> = prior_index.into_values().cloned().collect();
    // Deterministic ordering: sort by key.
    removed.sort_by(|a, b| a.key.cmp(&b.key));

    LineDiff {
        added,
        removed,
        changed,
        unchanged_count,
    }
}

/// Render the LLM-facing annotation block that names the diff. Used
/// by Bundle 16-B to inject `<memory_context_changes>` alongside
/// the full memory_context block when small drift is detected.
///
/// Returns `None` when the diff is empty — caller should skip
/// emitting any annotation in that case.
pub fn render_delta_annotation(diff: &LineDiff) -> Option<String> {
    if diff.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("<memory_context_changes vs_prior_turn=\"true\">\n");
    for entry in &diff.added {
        out.push_str("+ added: ");
        out.push_str(&entry.content);
        out.push('\n');
    }
    for entry in &diff.removed {
        out.push_str("- removed: ");
        out.push_str(&entry.content);
        out.push('\n');
    }
    for c in &diff.changed {
        out.push_str("~ changed key=");
        out.push_str(&c.key);
        out.push_str("\n    prior: ");
        out.push_str(&c.prior_content);
        out.push_str("\n    now:   ");
        out.push_str(&c.new_content);
        out.push('\n');
    }
    out.push_str("</memory_context_changes>");
    Some(out)
}

// ─── helpers ───────────────────────────────────────────────────────

/// Per-line token estimate matching the M2-D pilot's heuristic
/// (`bytes / 4`). Conservative for ASCII, slight under-estimate for
/// CJK — same skew as the rest of the codebase.
fn token_estimate_for_line(line: &str) -> usize {
    line.len() / 4
}

/// Stable djb2 of `s`, hex-formatted. Same algorithm as the M2-D
/// pilot uses for `memory_context` whole-block snapshots.
fn djb2_hex(s: &str) -> String {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    h.write(s.as_bytes());
    format!("{:x}", h.finish())
}

/// Extract a bullet-label prefix as a stable key. Recognized
/// shapes:
///
/// - `- 用户偏好: value`        → `"用户偏好"`
/// - `- preferred_language: en` → `"preferred_language"`
/// - `* key: value`             → `"key"`
/// - `key: value`               → `"key"`
///
/// Returns `None` if no recognizable label is present, in which
/// case the caller falls back to `sha1_prefix_key`.
fn prefix_key(line: &str) -> Option<String> {
    // Strip the bullet marker (`- ` / `* ` / numeric `1.`) if any.
    let trimmed = line.trim_start();
    let after_bullet = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("• "))
        .unwrap_or(trimmed);

    // Find the first `:` or `：` (full-width CJK colon).
    let colon_byte = after_bullet
        .char_indices()
        .find(|(_, c)| *c == ':' || *c == '：')
        .map(|(i, _)| i)?;
    let label = after_bullet[..colon_byte].trim();
    if label.is_empty() {
        return None;
    }
    Some(label.to_string())
}

/// Fallback key when `prefix_key` finds no label — short hash of
/// the full line so every distinct line gets its own slot in the
/// diff index.
fn sha1_prefix_key(line: &str) -> String {
    let h = djb2_hex(line);
    // 8 hex chars is plenty of bits for keys-within-a-fragment
    // collision avoidance (a `memory_context` block typically has
    // <50 lines).
    let take = std::cmp::min(8, h.len());
    format!("L:{}", &h[..take])
}

// ───────────────────────────────────────────────────────────────────
// Tests — Bundle 16-A spec acceptance criteria
// ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(text: &str) -> LineFragmentSnapshot {
        LineFragmentSnapshot::from_text("test", text)
    }

    // ── key extraction ──────────────────────────────────────────────

    #[test]
    fn prefix_key_extracts_label_from_dash_bullet_ascii() {
        assert_eq!(prefix_key("- preferred_language: en"), Some("preferred_language".to_string()));
    }

    #[test]
    fn prefix_key_extracts_label_from_dash_bullet_cjk() {
        assert_eq!(prefix_key("- 用户偏好: 中文"), Some("用户偏好".to_string()));
    }

    #[test]
    fn prefix_key_extracts_label_with_fullwidth_colon() {
        assert_eq!(prefix_key("- 用户偏好：中文"), Some("用户偏好".to_string()));
    }

    #[test]
    fn prefix_key_handles_star_bullet() {
        assert_eq!(prefix_key("* foo: bar"), Some("foo".to_string()));
    }

    #[test]
    fn prefix_key_handles_no_bullet() {
        assert_eq!(prefix_key("foo: bar"), Some("foo".to_string()));
    }

    #[test]
    fn prefix_key_returns_none_when_no_colon() {
        assert_eq!(prefix_key("- just a sentence"), None);
        assert_eq!(prefix_key("random text"), None);
    }

    #[test]
    fn line_entry_falls_back_to_hash_when_no_prefix() {
        let e = LineEntry::from_line("- just a sentence with no colon");
        assert!(e.key.starts_with("L:"), "fallback key prefix expected, got {}", e.key);
    }

    // ── line_diff acceptance criteria from spec ─────────────────────

    #[test]
    fn line_diff_detects_added_line() {
        let prior = snap("- a: 1\n");
        let new = snap("- a: 1\n- b: 2\n");
        let d = line_diff(&prior, &new);
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.added[0].key, "b");
        assert!(d.removed.is_empty());
        assert!(d.changed.is_empty());
        assert_eq!(d.unchanged_count, 1);
    }

    #[test]
    fn line_diff_detects_removed_line() {
        let prior = snap("- a: 1\n- b: 2\n");
        let new = snap("- a: 1\n");
        let d = line_diff(&prior, &new);
        assert!(d.added.is_empty());
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.removed[0].key, "b");
        assert_eq!(d.unchanged_count, 1);
    }

    #[test]
    fn line_diff_detects_value_change_via_stable_key() {
        let prior = snap("- preferred_language: en\n");
        let new = snap("- preferred_language: zh\n");
        let d = line_diff(&prior, &new);
        assert!(d.added.is_empty());
        assert!(d.removed.is_empty());
        assert_eq!(d.changed.len(), 1, "stable key must collapse en→zh into 'changed'");
        let c = &d.changed[0];
        assert_eq!(c.key, "preferred_language");
        assert!(c.prior_content.contains("en"));
        assert!(c.new_content.contains("zh"));
        assert_eq!(d.unchanged_count, 0);
    }

    #[test]
    fn line_diff_stable_keys_no_diff_on_reorder() {
        // Bullets reordered → same key set → all unchanged.
        let prior = snap("- a: 1\n- b: 2\n- c: 3\n");
        let new = snap("- c: 3\n- a: 1\n- b: 2\n");
        let d = line_diff(&prior, &new);
        assert!(d.is_empty(), "reorder must not show drift: {:?}", d);
        assert_eq!(d.unchanged_count, 3);
    }

    #[test]
    fn line_diff_empty_for_identical_snapshots() {
        let s = snap("- a: 1\n- b: 2\n");
        let d = line_diff(&s, &s);
        assert!(d.is_empty());
        assert_eq!(d.unchanged_count, 2);
    }

    #[test]
    fn line_diff_significant_change_threshold() {
        let prior = snap("- a: 1\n- b: 2\n- c: 3\n- d: 4\n");
        // 2 removed → drift = 2/4 = 0.5
        let new = snap("- a: 1\n- b: 2\n");
        let d = line_diff(&prior, &new);
        let stats = d.stats();
        assert!(stats.is_significant_change(0.5));
        assert!(stats.is_significant_change(0.4));
        assert!(!stats.is_significant_change(0.6));
    }

    #[test]
    fn line_diff_blank_lines_ignored() {
        // Renderer-injected blank lines must not show as drift.
        let prior = snap("- a: 1\n\n- b: 2\n");
        let new = snap("- a: 1\n- b: 2\n\n\n");
        let d = line_diff(&prior, &new);
        assert!(d.is_empty(), "blank-line differences must be invisible: {:?}", d);
    }

    // ── delta annotation rendering ─────────────────────────────────

    #[test]
    fn render_delta_annotation_emits_added_removed_changed_sections() {
        let prior = snap("- a: 1\n- b: 2\n- c: 3\n");
        let new = snap("- a: 1\n- b: 2-changed\n- d: 4\n");
        let d = line_diff(&prior, &new);
        let out = render_delta_annotation(&d).expect("non-empty diff must render");
        assert!(out.starts_with("<memory_context_changes"));
        assert!(out.ends_with("</memory_context_changes>"));
        assert!(out.contains("+ added: - d: 4"));
        assert!(out.contains("- removed: - c: 3"));
        assert!(out.contains("~ changed key=b"));
        assert!(out.contains("prior: - b: 2"));
        assert!(out.contains("now:   - b: 2-changed"));
    }

    #[test]
    fn render_delta_annotation_empty_for_no_drift() {
        let s = snap("- a: 1\n");
        let d = line_diff(&s, &s);
        assert_eq!(render_delta_annotation(&d), None);
    }

    // ── stats counting ─────────────────────────────────────────────

    #[test]
    fn stats_counts_added_changed_removed_unchanged() {
        let prior = snap("- a: 1\n- b: 2\n- c: 3\n");
        let new = snap("- a: 1\n- b: 2-v2\n- d: 4\n");
        let d = line_diff(&prior, &new);
        let s = d.stats();
        assert_eq!(s.added, 1);
        assert_eq!(s.removed, 1);
        assert_eq!(s.changed, 1);
        assert_eq!(s.unchanged, 1);
        assert_eq!(s.total_prior(), 3);
        assert_eq!(s.total_new(), 3);
    }

    #[test]
    fn snapshot_from_text_skips_blank_lines() {
        let s = snap("\n- a: 1\n\n\n- b: 2\n\n");
        assert_eq!(s.line_count(), 2);
        assert_eq!(s.lines[0].key, "a");
        assert_eq!(s.lines[1].key, "b");
    }

    // ── serde roundtrip ────────────────────────────────────────────

    #[test]
    fn line_diff_serde_camel_case_roundtrip() {
        let prior = snap("- a: 1\n");
        let new = snap("- a: 2\n- b: 3\n");
        let d = line_diff(&prior, &new);
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"unchangedCount\":"));
        assert!(json.contains("\"priorHash\":") || json.contains("\"priorContent\":"));
        let back: LineDiff = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
