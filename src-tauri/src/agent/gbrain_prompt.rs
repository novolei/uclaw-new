//! gbrain Sprint 2.3 — agent system prompt section.
//!
//! Renders a block of instructions telling the LLM when to use the
//! `mcp__gbrain__*` tool family. Without this, the tools are
//! registered in the manifest but the LLM has no incentive structure
//! to call them — gbrain stays "alive but idle" (same dead-code
//! pattern as Foundation Phase 1-7's EntityPage UI).
//!
//! Mirror of Sprint 1.6's `learning::prompt_section::UserProfileSection`
//! by shape: a `render` function that returns `Option<String>`, called
//! once per prompt build from `ChatDelegate::effective_system_prompt`.
//! Returns `None` when no `mcp__gbrain__*` tools are visible (server
//! disconnected, bundle missing, etc.) so we don't tell the LLM about
//! tools that don't exist.

use crate::mcp::McpManager;

/// Token marker added to the rendered block so logs / tests can match
/// it without depending on the prose body.
pub const GBRAIN_SECTION_MARKER: &str = "## Long-term Knowledge (gbrain)";

/// Instruction body — kept as a `const &str` so it's a single
/// recognizable string (greppable, diff-friendly) and we can fold it
/// into integration tests verbatim. Markdown formatting matches the
/// rest of the system prompt sections (Sprint 1.6 user-profile,
/// memory-recall context block).
const GBRAIN_INSTRUCTIONS: &str = "## Long-term Knowledge (gbrain)

You have a persistent local knowledge base via `mcp__gbrain__*` tools.
gbrain is a wiki-style entity graph backed by PGlite. It survives
across conversations and uClaw restarts. Use it PROACTIVELY:

When to call `mcp__gbrain__put_page`:
- User introduces a new entity worth long-term retention (a person,
  company, project, concept, decision, claim)
- User explicitly asks you to remember (\"记住\", \"remember this\")
- Conversation surfaces a stable fact (e.g. \"GPT-5 released in 2026\")
- A multi-turn investigation reaches a conclusion worth preserving

When to call `mcp__gbrain__query` / `mcp__gbrain__search`:
- User asks \"do you remember\" / \"what did we say about\"
- A new question echoes a topic from a prior session
- Before answering a factual question that gbrain might know

When to call `mcp__gbrain__list_pages`:
- User asks what is currently stored in gbrain / the knowledge base
- User asks for all memories, all pages, recent pages, or an inventory
- You need to verify whether gbrain contains any pages before searching
- Do not use `query` or `search` with `*` to list everything; `*` is not a
  supported all-pages query.

Slug format: kebab-case English, namespaced when useful. Examples:
- `openai-gpt-5-release`
- `project-uclaw`
- `decision-trigram-fts-2026-05`

Content format: YAML frontmatter (title, type, aliases, tags) +
markdown body. Keep bodies under 500 words; link to sub-pages via
`[[other-slug]]` when content grows.

After calling `query` / `search`, cite the slug(s) you read from in your
response. After calling `put_page`, mention what you have recorded.

DO NOT:
- Put `put_page` calls in your visible response — chain them as
  separate tool calls
- Use `put_page` for ephemeral things (this turn's question, jokes,
  small talk)
- Call gbrain retrieval for general-knowledge questions that don't reference
  prior conversation";

/// The agent-facing rendering entry point.
pub struct GbrainKnowledgeSection;

impl GbrainKnowledgeSection {
    /// Render the gbrain instruction block.
    ///
    /// Returns `None` when no `mcp__gbrain__*` tools are currently
    /// registered in the manifest — caller skips injection so the
    /// system prompt doesn't reference tools that aren't actually
    /// callable. Possible reasons for `None`:
    /// - bundle missing (setup scripts haven't run)
    /// - gbrain init failed (Sprint 2.2.5a/b surface this in UI)
    /// - server disconnected / Error status
    /// - server explicitly disabled by user
    ///
    /// The presence check matches against the `mcp__` prefix +
    /// `server_id="gbrain"` to handle future prefix changes cleanly.
    pub fn render(mcp_mgr: &McpManager) -> Option<String> {
        let has_gbrain = mcp_mgr
            .all_tools()
            .iter()
            .any(|t| t.server_id == "gbrain");
        if !has_gbrain {
            return None;
        }
        Some(GBRAIN_INSTRUCTIONS.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{
        McpManager, McpServerConfig, McpServerStatus, McpToolDef, TransportType,
    };
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn manager_with_tools() -> McpManager {
        let dir = tempdir().unwrap();
        let mut mgr = McpManager::new(dir.path());
        // Add a server + force-mark it Connected with one tool. We
        // bypass the actual transport — same trick the create_tool_proxies
        // unit test uses (tests/ block in mcp.rs).
        let cfg = McpServerConfig {
            id: "gbrain".into(),
            name: "gbrain (bundled)".into(),
            description: String::new(),
            transport_type: TransportType::Stdio,
            command: "/bin/true".into(),
            args: vec![],
            env: HashMap::new(),
            url: None,
            enabled: true,
            auto_approve: true,
            tool_allowlist: None,
        };
        mgr.add_server(cfg).unwrap();
        // Make the server appear Connected with a tool while avoiding a
        // real transport connection.
        mgr.test_set_server_tools(
            "gbrain",
            McpServerStatus::Connected,
            vec![McpToolDef {
                server_id: "gbrain".into(),
                name: "put_page".into(),
                description: "test".into(),
                parameters: serde_json::json!({}),
            }],
        );
        mgr
    }

    #[test]
    fn render_returns_none_when_no_gbrain_tools() {
        let dir = tempdir().unwrap();
        let mgr = McpManager::new(dir.path());
        assert!(GbrainKnowledgeSection::render(&mgr).is_none());
    }

    #[test]
    fn render_returns_some_with_marker_when_gbrain_connected() {
        let mgr = manager_with_tools();
        let out = GbrainKnowledgeSection::render(&mgr).expect("expected Some");
        assert!(
            out.contains(GBRAIN_SECTION_MARKER),
            "rendered block missing marker — content: {}",
            &out[..120.min(out.len())]
        );
        // Sanity check the most important rules are present.
        assert!(out.contains("put_page"));
        assert!(out.contains("recall"));
        assert!(out.contains("Slug format"));
    }

    #[test]
    fn render_returns_none_when_other_mcp_servers_but_no_gbrain() {
        // Forward-compatibility: a future user adds a github MCP server.
        // gbrain section shouldn't render just because some MCP exists.
        let dir = tempdir().unwrap();
        let mut mgr = McpManager::new(dir.path());
        let cfg = McpServerConfig {
            id: "github".into(),
            name: "github".into(),
            description: String::new(),
            transport_type: TransportType::Stdio,
            command: "/bin/true".into(),
            args: vec![],
            env: HashMap::new(),
            url: None,
            enabled: true,
            auto_approve: false,
            tool_allowlist: None,
        };
        mgr.add_server(cfg).unwrap();
        mgr.test_set_server_tools(
            "github",
            McpServerStatus::Connected,
            vec![McpToolDef {
                server_id: "github".into(),
                name: "list_repos".into(),
                description: String::new(),
                parameters: serde_json::json!({}),
            }],
        );
        assert!(
            GbrainKnowledgeSection::render(&mgr).is_none(),
            "section must not render for non-gbrain MCP servers"
        );
    }
}
