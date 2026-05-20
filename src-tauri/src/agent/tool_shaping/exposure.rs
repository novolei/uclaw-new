//! `ToolExposure` — per-tool exposure decision for the system prompt.
//!
//! Each tool advertised to the LLM costs tokens (name + description +
//! schema). For MCP servers that ship dozens of tools, exposing them
//! all every turn quickly burns through the prompt budget. L2 lets
//! uClaw classify tools so only the relevant subset is announced
//! per turn.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Whether a tool is visible to the LLM in a given turn.
///
/// - **`Always`**: announce every turn. Use for core tools the agent
///   needs constantly (e.g. `shell`, `read_file`).
/// - **`OnDemand`**: hide by default; surface only when the agent's
///   intent or the context-manager pin set asks for the tool's topic.
///   Most MCP tools fall here.
/// - **`Hidden`**: never announce (used to block a tool without
///   uninstalling its plugin — e.g. dangerous tools gated by policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolExposure {
    Always,
    OnDemand,
    Hidden,
}

impl ToolExposure {
    /// `true` if this exposure means the tool should appear in the
    /// current turn's tool catalogue. For `OnDemand` the answer is
    /// `false` here — the caller decides whether to flip it based on
    /// the per-turn pin set.
    pub fn announced_by_default(self) -> bool {
        matches!(self, ToolExposure::Always)
    }
}

/// Per-tool exposure decisions.
///
/// Tools are keyed by their **fully-qualified id** — for MCP tools
/// this is `"{server_id}::{tool_name}"`; for builtins it's just the
/// tool name (e.g. `"shell"`). Lookup is O(log n) via `BTreeMap` so
/// the policy serializes deterministically into the eventual
/// `~/.uclaw/tool_exposure.toml` settings surface.
///
/// Unregistered tools fall back to `default_exposure` (Always — keep
/// surprise behaviour visible until policy explicitly hides it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExposurePolicy {
    by_tool: BTreeMap<String, ToolExposure>,
    default_exposure: ToolExposure,
}

impl ToolExposurePolicy {
    /// Empty policy — all tools `Always` exposed.
    pub fn new() -> Self {
        Self {
            by_tool: BTreeMap::new(),
            default_exposure: ToolExposure::Always,
        }
    }

    /// Default uClaw policy: all builtins Always; flagging MCP tools
    /// individually happens via `with_tool`. To mass-hide an MCP
    /// server's tools, use `with_server_default`.
    pub fn default_policy() -> Self {
        Self::new()
    }

    /// Set or override one tool's exposure.
    pub fn with_tool(
        mut self,
        tool_id: impl Into<String>,
        exposure: ToolExposure,
    ) -> Self {
        self.by_tool.insert(tool_id.into(), exposure);
        self
    }

    /// Override the catch-all default.
    pub fn set_default(mut self, exposure: ToolExposure) -> Self {
        self.default_exposure = exposure;
        self
    }

    /// Mass-set the default exposure for every tool whose id starts
    /// with `server_prefix::`. Existing per-tool entries are kept.
    ///
    /// Use case: "default every Slack tool to OnDemand, except
    /// `slack::list_channels` which I previously set to Always".
    /// Re-applying this only writes entries that aren't already set.
    pub fn with_server_default(
        mut self,
        server_prefix: &str,
        exposure: ToolExposure,
        known_tools: &[&str],
    ) -> Self {
        for tool in known_tools {
            let id = format!("{server_prefix}::{tool}");
            self.by_tool.entry(id).or_insert(exposure);
        }
        self
    }

    /// Look up a tool's exposure. Falls back to `default_exposure`.
    pub fn exposure_for(&self, tool_id: &str) -> ToolExposure {
        self.by_tool
            .get(tool_id)
            .copied()
            .unwrap_or(self.default_exposure)
    }

    /// Convenience: would this tool appear in the default catalogue?
    pub fn is_announced_by_default(&self, tool_id: &str) -> bool {
        self.exposure_for(tool_id).announced_by_default()
    }

    /// Number of explicit per-tool overrides registered.
    pub fn override_count(&self) -> usize {
        self.by_tool.len()
    }
}

impl Default for ToolExposurePolicy {
    fn default() -> Self {
        Self::default_policy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ToolExposure ────────────────────────────────────────────────

    #[test]
    fn always_is_announced_by_default() {
        assert!(ToolExposure::Always.announced_by_default());
    }

    #[test]
    fn on_demand_is_not_announced_by_default() {
        assert!(!ToolExposure::OnDemand.announced_by_default());
    }

    #[test]
    fn hidden_is_not_announced() {
        assert!(!ToolExposure::Hidden.announced_by_default());
    }

    #[test]
    fn serde_roundtrip_exposure_uses_snake_case() {
        let always = serde_json::to_value(ToolExposure::Always).unwrap();
        assert_eq!(always, serde_json::json!("always"));
        let od = serde_json::to_value(ToolExposure::OnDemand).unwrap();
        assert_eq!(od, serde_json::json!("on_demand"));
        let hidden = serde_json::to_value(ToolExposure::Hidden).unwrap();
        assert_eq!(hidden, serde_json::json!("hidden"));

        let back: ToolExposure = serde_json::from_value(od).unwrap();
        assert_eq!(back, ToolExposure::OnDemand);
    }

    // ── ToolExposurePolicy ──────────────────────────────────────────

    #[test]
    fn empty_policy_treats_everything_as_always() {
        let p = ToolExposurePolicy::new();
        assert_eq!(p.exposure_for("shell"), ToolExposure::Always);
        assert_eq!(p.exposure_for("slack::list_channels"), ToolExposure::Always);
        assert!(p.is_announced_by_default("anything"));
    }

    #[test]
    fn with_tool_registers_override() {
        let p = ToolExposurePolicy::default_policy()
            .with_tool("slack::send_message", ToolExposure::OnDemand);
        assert_eq!(
            p.exposure_for("slack::send_message"),
            ToolExposure::OnDemand
        );
        // Other tools unaffected.
        assert_eq!(p.exposure_for("shell"), ToolExposure::Always);
        assert_eq!(p.override_count(), 1);
    }

    #[test]
    fn set_default_changes_catchall() {
        let p = ToolExposurePolicy::new().set_default(ToolExposure::Hidden);
        assert_eq!(p.exposure_for("anything"), ToolExposure::Hidden);
        assert!(!p.is_announced_by_default("anything"));
    }

    #[test]
    fn with_server_default_sets_each_tool() {
        let p = ToolExposurePolicy::default_policy().with_server_default(
            "slack",
            ToolExposure::OnDemand,
            &["list_channels", "send_message", "list_users"],
        );
        assert_eq!(
            p.exposure_for("slack::list_channels"),
            ToolExposure::OnDemand
        );
        assert_eq!(p.exposure_for("slack::send_message"), ToolExposure::OnDemand);
        assert_eq!(p.exposure_for("slack::list_users"), ToolExposure::OnDemand);
        // Tool not in the list keeps the catch-all default.
        assert_eq!(p.exposure_for("slack::other"), ToolExposure::Always);
    }

    #[test]
    fn with_server_default_does_not_clobber_existing_entries() {
        // User explicitly set list_channels to Always — bulk OnDemand
        // must not overwrite.
        let p = ToolExposurePolicy::default_policy()
            .with_tool("slack::list_channels", ToolExposure::Always)
            .with_server_default(
                "slack",
                ToolExposure::OnDemand,
                &["list_channels", "send_message"],
            );
        assert_eq!(
            p.exposure_for("slack::list_channels"),
            ToolExposure::Always
        );
        assert_eq!(p.exposure_for("slack::send_message"), ToolExposure::OnDemand);
    }

    #[test]
    fn serde_roundtrip_policy_preserves_overrides() {
        let p = ToolExposurePolicy::default_policy()
            .with_tool("shell", ToolExposure::Always)
            .with_tool("slack::send_message", ToolExposure::OnDemand)
            .with_tool("dangerous_tool", ToolExposure::Hidden);
        let json = serde_json::to_string(&p).unwrap();
        let back: ToolExposurePolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back.exposure_for("shell"), ToolExposure::Always);
        assert_eq!(
            back.exposure_for("slack::send_message"),
            ToolExposure::OnDemand
        );
        assert_eq!(back.exposure_for("dangerous_tool"), ToolExposure::Hidden);
        assert_eq!(back.override_count(), 3);
    }
}
