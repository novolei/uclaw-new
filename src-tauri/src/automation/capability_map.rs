//! Map MCP-id capability declarations from automation specs to uClaw's
//! in-process built-in tools. Today's only mapping is `ai-browser` → the
//! chromiumoxide-backed `browser/` module. Phase 3b-γ replaces this match
//! with a configurable table.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinCapability {
    /// chromiumoxide-backed browser, surfaced via agent dispatcher's
    /// `browser_navigate` / `browser_run` / `browser_evaluate` tools.
    Browser,
}

pub fn resolve_capability(mcp_id: &str) -> Option<BuiltinCapability> {
    match mcp_id {
        "ai-browser" => Some(BuiltinCapability::Browser),
        _ => None,
    }
}

/// Chinese label for the "Required Capabilities" UI row.
pub fn human_label(cap: BuiltinCapability) -> &'static str {
    match cap {
        BuiltinCapability::Browser => "uClaw 内建浏览器",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_browser_resolves_to_builtin_browser() {
        assert_eq!(
            resolve_capability("ai-browser"),
            Some(BuiltinCapability::Browser)
        );
    }

    #[test]
    fn unknown_id_returns_none() {
        assert_eq!(resolve_capability("totally-fake-mcp"), None);
        assert_eq!(resolve_capability(""), None);
    }

    #[test]
    fn human_label_is_chinese_when_mapped() {
        assert_eq!(
            human_label(BuiltinCapability::Browser),
            "uClaw 内建浏览器"
        );
    }
}
