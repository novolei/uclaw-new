//! `TruncationPolicy` — per-handler byte budgets and the
//! UTF-8-safe `truncate_with_marker` helper.
//!
//! Default budgets per ADR §M2-H L1:
//!
//! | Handler | Budget |
//! |---|---|
//! | shell / exec | 8192 bytes |
//! | file read | 4096 bytes |
//! | search | 4096 bytes |
//! | web fetch | 6144 bytes |
//! | mcp tool | 5120 bytes |
//! | (catch-all) | 4096 bytes |
//!
//! These mirror the Codex defaults adjusted for uClaw's typical
//! per-turn budget (≤ 32K input tokens after baseline + context).

use std::collections::BTreeMap;

/// Logical class of agent tool handler. Each kind has its own
/// truncation budget.
///
/// Variants intentionally **closed** — adding a new handler kind is
/// the deliberate trigger for registering a budget. Use [`Custom`]
/// for plugin-defined handlers whose category isn't known up front.
///
/// [`Custom`]: HandlerKind::Custom
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HandlerKind {
    /// Shell / exec output (`bash`, `cmd_exec`).
    Shell,
    /// File-read style tools (`read_file`, `cat`, `view`).
    FileRead,
    /// File-write style tools (`write_file`, `edit`). Their *output*
    /// is usually short (status string) but pathological cases exist.
    FileWrite,
    /// Local search (`grep`, `glob`, `find`).
    Search,
    /// Web fetch (`fetch_url`, `browser_get`).
    WebFetch,
    /// MCP-routed tool call. All MCP tools share one budget; if a
    /// specific MCP needs custom shaping use [`Custom`] with the MCP id.
    ///
    /// [`Custom`]: HandlerKind::Custom
    Mcp,
    /// Memory / knowledge graph recall.
    Memory,
    /// Catch-all for plugin / user-defined handler categories. Carries
    /// a stable string id so the policy can hold per-handler overrides.
    Custom(String),
}

impl HandlerKind {
    /// Stable string id for table lookup / serialization. Matches the
    /// keys used by the eventual `[tool_output_budgets]` settings
    /// surface (M2-H L1 commit 3).
    pub fn id(&self) -> String {
        match self {
            HandlerKind::Shell => "shell".into(),
            HandlerKind::FileRead => "file_read".into(),
            HandlerKind::FileWrite => "file_write".into(),
            HandlerKind::Search => "search".into(),
            HandlerKind::WebFetch => "web_fetch".into(),
            HandlerKind::Mcp => "mcp".into(),
            HandlerKind::Memory => "memory".into(),
            HandlerKind::Custom(id) => format!("custom:{id}"),
        }
    }
}

/// Per-handler byte budgets, with a catch-all `default_budget` used
/// when no specific entry is registered.
///
/// Lookup is O(log n) via `BTreeMap` keyed on `HandlerKind::id()`.
/// Insertion via [`with_budget`] / [`set_default`] returns `Self` for
/// builder-style construction.
///
/// [`with_budget`]: TruncationPolicy::with_budget
/// [`set_default`]: TruncationPolicy::set_default
#[derive(Debug, Clone)]
pub struct TruncationPolicy {
    budgets: BTreeMap<String, usize>,
    default_budget: usize,
}

impl TruncationPolicy {
    /// Empty policy — every lookup returns `default_budget` (4096).
    pub fn new() -> Self {
        Self {
            budgets: BTreeMap::new(),
            default_budget: 4096,
        }
    }

    /// ADR §M2-H L1 default budget table. Use this in production.
    pub fn default_budgets() -> Self {
        Self::new()
            .with_budget(HandlerKind::Shell, 8192)
            .with_budget(HandlerKind::FileRead, 4096)
            .with_budget(HandlerKind::FileWrite, 1024)
            .with_budget(HandlerKind::Search, 4096)
            .with_budget(HandlerKind::WebFetch, 6144)
            .with_budget(HandlerKind::Mcp, 5120)
            .with_budget(HandlerKind::Memory, 4096)
    }

    /// Builder: set or override a budget for one handler kind.
    pub fn with_budget(mut self, kind: HandlerKind, bytes: usize) -> Self {
        self.budgets.insert(kind.id(), bytes);
        self
    }

    /// Override the catch-all default.
    pub fn set_default(mut self, bytes: usize) -> Self {
        self.default_budget = bytes;
        self
    }

    /// Look up the byte budget for a handler. Falls back to the
    /// catch-all default if no specific entry is registered.
    pub fn budget_for(&self, kind: &HandlerKind) -> usize {
        self.budgets
            .get(&kind.id())
            .copied()
            .unwrap_or(self.default_budget)
    }

    /// Truncate `text` to the budget for `kind`, appending a marker
    /// describing how much was dropped. Returns the original `text` as
    /// `Cow::Borrowed` when no truncation is needed.
    pub fn apply<'a>(&self, kind: &HandlerKind, text: &'a str) -> std::borrow::Cow<'a, str> {
        truncate_with_marker(text, self.budget_for(kind))
    }
}

impl Default for TruncationPolicy {
    fn default() -> Self {
        Self::default_budgets()
    }
}

/// Append-a-marker truncation. UTF-8 safe — truncates at the **last
/// char boundary at or before `budget`** so the returned string is
/// always valid UTF-8.
///
/// When truncation happens, the returned string is
/// `"{prefix}…[truncated {dropped} of {total} bytes]"`. The marker
/// itself counts toward the truncated string's length — callers that
/// need a hard byte cap should subtract the marker length from their
/// requested budget.
///
/// Returns `Cow::Borrowed(text)` when `text.len() <= budget`.
pub fn truncate_with_marker(text: &str, budget: usize) -> std::borrow::Cow<'_, str> {
    if text.len() <= budget {
        return std::borrow::Cow::Borrowed(text);
    }
    let total = text.len();
    // Find the largest valid char boundary at or before `budget`.
    let mut cut = budget;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let kept = &text[..cut];
    let dropped = total - cut;
    std::borrow::Cow::Owned(format!(
        "{kept}…[truncated {dropped} of {total} bytes]"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── HandlerKind ids ─────────────────────────────────────────────

    #[test]
    fn handler_kind_id_stable_for_builtin() {
        assert_eq!(HandlerKind::Shell.id(), "shell");
        assert_eq!(HandlerKind::FileRead.id(), "file_read");
        assert_eq!(HandlerKind::FileWrite.id(), "file_write");
        assert_eq!(HandlerKind::Search.id(), "search");
        assert_eq!(HandlerKind::WebFetch.id(), "web_fetch");
        assert_eq!(HandlerKind::Mcp.id(), "mcp");
        assert_eq!(HandlerKind::Memory.id(), "memory");
    }

    #[test]
    fn handler_kind_custom_carries_id() {
        let h = HandlerKind::Custom("plugin.weather".into());
        assert_eq!(h.id(), "custom:plugin.weather");
    }

    // ── default_budgets ─────────────────────────────────────────────

    #[test]
    fn default_budgets_match_adr_table() {
        let p = TruncationPolicy::default_budgets();
        assert_eq!(p.budget_for(&HandlerKind::Shell), 8192);
        assert_eq!(p.budget_for(&HandlerKind::FileRead), 4096);
        assert_eq!(p.budget_for(&HandlerKind::FileWrite), 1024);
        assert_eq!(p.budget_for(&HandlerKind::Search), 4096);
        assert_eq!(p.budget_for(&HandlerKind::WebFetch), 6144);
        assert_eq!(p.budget_for(&HandlerKind::Mcp), 5120);
        assert_eq!(p.budget_for(&HandlerKind::Memory), 4096);
    }

    #[test]
    fn default_budgets_falls_back_to_catchall_for_custom() {
        let p = TruncationPolicy::default_budgets();
        let custom = HandlerKind::Custom("unregistered".into());
        // 4096 is the catch-all default.
        assert_eq!(p.budget_for(&custom), 4096);
    }

    // ── builder ─────────────────────────────────────────────────────

    #[test]
    fn with_budget_overrides_builtin() {
        let p = TruncationPolicy::default_budgets()
            .with_budget(HandlerKind::Shell, 1024);
        assert_eq!(p.budget_for(&HandlerKind::Shell), 1024);
        // Others unchanged.
        assert_eq!(p.budget_for(&HandlerKind::FileRead), 4096);
    }

    #[test]
    fn with_budget_registers_custom_id() {
        let kind = HandlerKind::Custom("my.tool".into());
        let p = TruncationPolicy::new().with_budget(kind.clone(), 2048);
        assert_eq!(p.budget_for(&kind), 2048);
    }

    #[test]
    fn set_default_changes_catchall() {
        let p = TruncationPolicy::new().set_default(99);
        // No specific entries — every kind hits the new default.
        assert_eq!(p.budget_for(&HandlerKind::Shell), 99);
        assert_eq!(
            p.budget_for(&HandlerKind::Custom("x".into())),
            99
        );
    }

    // ── truncate_with_marker ────────────────────────────────────────

    #[test]
    fn no_truncation_below_budget_returns_borrowed() {
        let s = "hello";
        let out = truncate_with_marker(s, 100);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
        assert_eq!(out, "hello");
    }

    #[test]
    fn no_truncation_when_exactly_at_budget() {
        let s = "abcdef";
        let out = truncate_with_marker(s, 6);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
        assert_eq!(out, "abcdef");
    }

    #[test]
    fn truncates_with_marker_when_over_budget() {
        let s = "0123456789".repeat(10); // 100 bytes
        let out = truncate_with_marker(&s, 20);
        assert!(matches!(out, std::borrow::Cow::Owned(_)));
        assert!(out.starts_with("01234567890123456789"));
        assert!(out.contains("[truncated 80 of 100 bytes]"));
    }

    #[test]
    fn utf8_safe_at_char_boundary() {
        // "日本語テスト" — each char is 3 bytes.
        let s = "日本語テスト"; // 18 bytes
        // Budget 5 lands inside a multi-byte char. Must back off to
        // last valid boundary (3, i.e. "日").
        let out = truncate_with_marker(s, 5);
        // The kept prefix must be valid UTF-8 — String::from_utf8
        // would have caught a bad cut, but we assert prefix instead.
        assert!(out.starts_with("日"));
        assert!(!out.starts_with("日本"));
        assert!(out.contains("[truncated"));
    }

    #[test]
    fn utf8_zero_budget_returns_pure_marker() {
        let s = "日本語"; // 9 bytes
        let out = truncate_with_marker(s, 0);
        // No content kept (budget 0, first char boundary is 0).
        assert!(out.starts_with("…[truncated 9 of 9 bytes]"));
    }

    // ── TruncationPolicy::apply ─────────────────────────────────────

    #[test]
    fn apply_uses_per_handler_budget() {
        let p = TruncationPolicy::default_budgets();
        let big = "x".repeat(10_000);
        // Shell budget is 8192 — truncates.
        let shell_out = p.apply(&HandlerKind::Shell, &big);
        assert!(matches!(shell_out, std::borrow::Cow::Owned(_)));
        assert!(shell_out.contains("[truncated"));
        // FileWrite budget is 1024 — much shorter result.
        let fw_out = p.apply(&HandlerKind::FileWrite, &big);
        assert!(fw_out.len() < shell_out.len());
    }

    #[test]
    fn apply_borrows_when_under_budget() {
        let p = TruncationPolicy::default_budgets();
        let small = "tiny output";
        let out = p.apply(&HandlerKind::Shell, small);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    // ── clone semantics ─────────────────────────────────────────────

    #[test]
    fn policy_clone_independent() {
        let p1 = TruncationPolicy::default_budgets();
        let p2 = p1.clone().with_budget(HandlerKind::Shell, 1);
        // Mutation on clone doesn't leak back.
        assert_eq!(p1.budget_for(&HandlerKind::Shell), 8192);
        assert_eq!(p2.budget_for(&HandlerKind::Shell), 1);
    }
}
