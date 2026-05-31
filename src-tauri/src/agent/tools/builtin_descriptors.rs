// SPDX-License-Identifier: Apache-2.0
//! Boot-time registration of builtin tool descriptors into AgentApi.
//!
//! Called from `AppState::new()` BEFORE the AgentApi is Arc-wrapped. Each
//! descriptor's builder closure constructs a session-scoped tool instance
//! at session-build time via `AgentApi.build_session_registry(&ctx)`.
//!
//! This file translates the per-tool registration pattern from
//! `agent/tools/registry_build.rs::build_tool_registry()` into descriptor form.
//! Tools whose constructors require many `Arc<AppState.foo>` refs that can't
//! be probed at boot are intentionally left in the build_tool_registry shim
//! (see Task 6) and will migrate in a follow-up.
//!
//! ## Migrated tools (17 total)
//!
//! ### Workspace-only (probe via PathBuf::from("/tmp"))
//!   - read_file, write_file, get_file_skeleton, grep, glob, edit, bash
//!
//! ### No-arg (constructed directly)
//!   - web_fetch, http_request
//!
//! ### app_handle + workspace
//!   - plan_write, plan_update
//!
//! ### app_handle + session_id + Arc<state.pending_*>
//!   - ask_user, exit_plan_mode
//!
//! ### app_handle + session_id + Arc<state.db>
//!   - request_plan_mode_switch
//!
//! ### app_handle + session_id + Arc<state.db> + Optional<Arc<state.infra_service>>
//!   - self_eval
//!
//! ### Fresh Arc<RwLock<ContextToolSet>> per session
//!   - context.search, context.read
//!
//! ## Deferred tools (kept in build_tool_registry shim by Task 6)
//!
//! All browser tools (BrowserNavigateTool, BrowserGoBackTool, ..., BrowserTaskTool,
//! BrowserTaskResumeTool, RetryWithBrowserAgentTool — ~25 tools) require a complex
//! constructor block: Arc<BrowserContextManager>, Arc<BrowserTaskStore>,
//! Arc<BrowserLongTermMemoryAdapter>, Arc<BrowserAskUserBridge>,
//! Arc<LlmBrowserDecisionAdapter>, runtime_provider_config (from async
//! state.settings.read().await), Arc<Option<McpManager>>, plus safety_manager and
//! pending_approvals. This block needs `async` context that the synchronous
//! `SessionContext` builder closure signature (`Fn(&SessionContext) -> Box<dyn Tool>`)
//! does not provide. Deferral is intentional.
//!
//! memu tools: registered via `memu_tools::register_memu_tools()` which takes
//! `state.memu_client.clone()` — a custom client type outside `SessionContext`.
//! Deferred until memu_client is surfaced on `SessionContext`.
//!
//! MCP proxy tools: dynamically enumerated at call time via async
//! `state.mcp_manager.read().await`. Defer until async builders are supported.

use std::path::PathBuf;
use std::sync::Arc;

use crate::agent::api::AgentApi;
use crate::agent::api::tool::ToolDescriptor;
use crate::agent::tools::builtin;
use crate::agent::tools::tool::Tool;

/// Register all synchronously-constructable builtin tool descriptors into
/// the given `AgentApi`.
///
/// Boot path: called from `AppState::new()` with a fresh `&mut AgentApi`
/// before the Arc-wrap. Tools not migrated here are still registered inline
/// by `build_tool_registry()` (P3-2.5 follow-up will complete the migration).
pub fn register_all(api: &mut AgentApi) {
    // Use a throwaway workspace path for probe construction. The probe is only
    // used to read trait methods (name, description, parameters_schema) at
    // registration time. The actual session workspace comes from `ctx.workspace`
    // and is captured by each builder closure below.
    let probe_ws = PathBuf::from("/tmp/__uclaw_descriptor_probe__");

    // ── Filesystem tools (workspace-only) ─────────────────────────────────────

    {
        let probe = builtin::file::ReadFileTool::new(probe_ws.clone());
        api.register_tool(ToolDescriptor {
            name: probe.name().to_string(),
            description: probe.description().to_string(),
            parameters_schema: probe.parameters_schema(),
            builder: Arc::new(|ctx| {
                // item3 — apply the per-session read cap resolved in
                // build_tool_registry (floor-clamped inside the builder).
                Box::new(
                    builtin::file::ReadFileTool::new(ctx.workspace.clone())
                        .with_max_read_chars(ctx.read_file_max_chars),
                )
            }),
        });
    }

    {
        let probe = builtin::file::WriteFileTool::new(probe_ws.clone());
        api.register_tool(ToolDescriptor {
            name: probe.name().to_string(),
            description: probe.description().to_string(),
            parameters_schema: probe.parameters_schema(),
            builder: Arc::new(|ctx| {
                Box::new(builtin::file::WriteFileTool::new(ctx.workspace.clone()))
            }),
        });
    }

    {
        let probe = builtin::get_file_skeleton::GetFileSkeletonTool::new(probe_ws.clone());
        api.register_tool(ToolDescriptor {
            name: probe.name().to_string(),
            description: probe.description().to_string(),
            parameters_schema: probe.parameters_schema(),
            builder: Arc::new(|ctx| {
                Box::new(builtin::get_file_skeleton::GetFileSkeletonTool::new(
                    ctx.workspace.clone(),
                ))
            }),
        });
    }

    {
        let probe = builtin::search::GrepTool::new(probe_ws.clone());
        api.register_tool(ToolDescriptor {
            name: probe.name().to_string(),
            description: probe.description().to_string(),
            parameters_schema: probe.parameters_schema(),
            builder: Arc::new(|ctx| {
                Box::new(builtin::search::GrepTool::new(ctx.workspace.clone()))
            }),
        });
    }

    {
        let probe = builtin::search::GlobTool::new(probe_ws.clone());
        api.register_tool(ToolDescriptor {
            name: probe.name().to_string(),
            description: probe.description().to_string(),
            parameters_schema: probe.parameters_schema(),
            builder: Arc::new(|ctx| {
                Box::new(builtin::search::GlobTool::new(ctx.workspace.clone()))
            }),
        });
    }

    {
        let probe = builtin::edit::EditTool::new(probe_ws.clone());
        api.register_tool(ToolDescriptor {
            name: probe.name().to_string(),
            description: probe.description().to_string(),
            parameters_schema: probe.parameters_schema(),
            builder: Arc::new(|ctx| {
                let mut tool = builtin::edit::EditTool::new(ctx.workspace.clone());
                // item2 — apply the per-session project-check config resolved in
                // build_tool_registry (None → disabled, unchanged behaviour).
                if let Some(cfg) = &ctx.edit_project_check {
                    tool = tool.with_project_check(true, cfg.timeout_secs);
                }
                Box::new(tool)
            }),
        });
    }

    {
        let probe = builtin::shell::BashTool::new(probe_ws.clone());
        api.register_tool(ToolDescriptor {
            name: probe.name().to_string(),
            description: probe.description().to_string(),
            parameters_schema: probe.parameters_schema(),
            builder: Arc::new(|ctx| {
                Box::new(builtin::shell::BashTool::new(ctx.workspace.clone()))
            }),
        });
    }

    // ── No-arg tools ─────────────────────────────────────────────────────────

    {
        let probe = builtin::web::WebFetchTool::new();
        api.register_tool(ToolDescriptor {
            name: probe.name().to_string(),
            description: probe.description().to_string(),
            parameters_schema: probe.parameters_schema(),
            builder: Arc::new(|_ctx| Box::new(builtin::web::WebFetchTool::new())),
        });
    }

    {
        let probe = builtin::web::HttpRequestTool::new();
        api.register_tool(ToolDescriptor {
            name: probe.name().to_string(),
            description: probe.description().to_string(),
            parameters_schema: probe.parameters_schema(),
            builder: Arc::new(|_ctx| Box::new(builtin::web::HttpRequestTool::new())),
        });
    }

    // ── app_handle + workspace tools ─────────────────────────────────────────
    //
    // These tools need app_handle for Tauri IPC event emission. Probe uses
    // probe_ws for workspace; app_handle is not needed for metadata — hardcoded
    // strings are taken directly from the Tool trait impls above.

    {
        // PlanWriteTool: name="plan_write", cannot probe without AppHandle.
        // Metadata read from builtin/plan.rs source.
        api.register_tool(ToolDescriptor {
            name: "plan_write".to_string(),
            description: "Create a structured plan file before starting a complex task. \
                          Saves to .uclaw/plans/ in the workspace."
                .to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Plan title" },
                    "steps": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Ordered list of steps to complete"
                    },
                    "notes": { "type": "string", "description": "Optional additional notes" }
                },
                "required": ["title", "steps"]
            }),
            builder: Arc::new(|ctx| {
                Box::new(builtin::plan::PlanWriteTool::new(
                    ctx.workspace.clone(),
                    ctx.app_handle.clone(),
                ))
            }),
        });
    }

    {
        // PlanUpdateTool: name="plan_update"
        api.register_tool(ToolDescriptor {
            name: "plan_update".to_string(),
            description: "Update a step in an existing plan file. Mark a step done or add a note."
                .to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "filename": { "type": "string", "description": "Plan filename (e.g. '2025-01-01_120000-my-plan.md')" },
                    "step_index": { "type": "integer", "description": "Zero-based index of the step to update" },
                    "done": { "type": "boolean", "description": "Mark step as done (true) or undone (false)" },
                    "note": { "type": "string", "description": "Optional note to append to the step" }
                },
                "required": ["filename", "step_index"]
            }),
            builder: Arc::new(|ctx| {
                Box::new(builtin::plan::PlanUpdateTool::new(
                    ctx.workspace.clone(),
                    ctx.app_handle.clone(),
                ))
            }),
        });
    }

    // ── app_handle + session_id + Arc<state.pending_ask_users> ───────────────

    {
        // AskUserTool: name="ask_user"
        api.register_tool(ToolDescriptor {
            name: "ask_user".to_string(),
            description: "Pause execution and ask the user one or more clarifying questions \
                          with optional multiple-choice options. Returns the user's answers."
                .to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "questions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "question": {"type": "string"},
                                "header":   {"type": "string"},
                                "multiSelect": {"type": "boolean", "default": false, "description": "Allow selecting multiple options"},
                                "options": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "label":       {"type": "string"},
                                            "description": {"type": "string"},
                                            "preview":     {"type": "string"}
                                        },
                                        "required": ["label"]
                                    }
                                }
                            },
                            "required": ["question"]
                        },
                        "description": "List of questions to ask the user"
                    }
                },
                "required": ["questions"]
            }),
            builder: Arc::new(|ctx| {
                Box::new(builtin::ask_user::AskUserTool::new(
                    ctx.app_handle.clone(),
                    Arc::clone(&ctx.app_state.pending_ask_users),
                    ctx.session_id.clone(),
                ))
            }),
        });
    }

    // ── app_handle + session_id + Arc<state.pending_exit_plans> ─────────────

    {
        // ExitPlanModeTool: name="exit_plan_mode"
        api.register_tool(ToolDescriptor {
            name: "exit_plan_mode".to_string(),
            description: "Submit your plan to the user for approval. The user will see a \
                          confirmation modal and can accept (switching to Auto), accept but \
                          stay in Plan mode (only the commands you list in allowed_prompts \
                          will auto-pass), or reject with feedback."
                .to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "plan": {
                        "type": "string",
                        "description": "Full plan in markdown format"
                    },
                    "allowed_prompts": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of specific commands (e.g. 'bash cargo build') that should auto-pass even if the user chooses to stay in Plan mode"
                    }
                },
                "required": ["plan"]
            }),
            builder: Arc::new(|ctx| {
                Box::new(builtin::exit_plan_mode::ExitPlanModeTool::new(
                    ctx.app_handle.clone(),
                    Arc::clone(&ctx.app_state.pending_exit_plans),
                    ctx.session_id.clone(),
                ))
            }),
        });
    }

    // ── app_handle + session_id + Arc<state.db> ───────────────────────────────

    {
        // RequestPlanModeSwitchTool: name="request_plan_mode_switch"
        api.register_tool(ToolDescriptor {
            name: "request_plan_mode_switch".to_string(),
            description: "Suggest the user switch to Plan mode for the current task. \
                          Fire-and-forget — the user sees a banner and may accept or skip; \
                          the agent continues regardless in the current mode."
                .to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "Why Plan mode would help here. 1-2 sentences."
                    },
                    "preview_steps": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional initial step sketch to show in the banner."
                    }
                },
                "required": ["reason"]
            }),
            builder: Arc::new(|ctx| {
                Box::new(builtin::plan_mode::RequestPlanModeSwitchTool::new(
                    ctx.app_handle.clone(),
                    ctx.session_id.clone(),
                    Arc::clone(&ctx.app_state.db),
                ))
            }),
        });
    }

    // ── app_handle + session_id + Arc<state.db> + Optional<Arc<state.infra_service>> ──

    {
        // SelfEvalTool: name="self_eval"
        api.register_tool(ToolDescriptor {
            name: "self_eval".to_string(),
            description: "Evaluate your own task completion quality. \
                          Record a score and learnings for future improvement."
                .to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "score": {
                        "type": "number",
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "description": "Completion quality score from 0.0 (failed) to 1.0 (perfect)"
                    },
                    "reasoning": {
                        "type": "string",
                        "description": "Why you gave this score"
                    },
                    "learnings": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Reusable insights or patterns that could improve future performance"
                    }
                },
                "required": ["score", "reasoning"]
            }),
            builder: Arc::new(|ctx| {
                let tool = builtin::self_eval::SelfEvalTool::new(
                    ctx.session_id.clone(),
                    Arc::clone(&ctx.app_state.db),
                    ctx.app_handle.clone(),
                )
                .with_infra(Arc::clone(&ctx.app_state.infra_service));
                Box::new(tool)
            }),
        });
    }

    // ── Fresh ContextToolSet per session ──────────────────────────────────────
    //
    // Each session gets its own `Arc<RwLock<ContextToolSet>>` shared between the
    // two context tools. We create it inside the builder closure so each session
    // build gets a fresh, isolated set. Both closures clone the same Arc for the
    // same session build via a shared outer Arc created once per descriptor pair.
    //
    // NOTE: because ToolDescriptor builders are `Fn` (not `FnOnce`), we cannot
    // create a fresh Arc inside one closure and share it with another. The two
    // context tools therefore each construct their own independent ContextToolSet.
    // The fragment lifecycle (fragments entering/leaving the shared set) is a
    // M2-D follow-up anyway; for now both tools start empty and the separation
    // is benign.

    {
        // context.search — metadata read directly from context_tools_adapter.rs source
        api.register_tool(ToolDescriptor {
            name: "context.search".to_string(),
            description: "Search the available context fragments by topic tag(s). Returns a JSON array of \
                          ContextRef objects, each shaped { \"id\", \"source\", \"label\"? }. Pass one or more \
                          lowercase topic tags (e.g. \"conversation\", \"codebase\", \"memory\"); fragments \
                          matching ANY topic are returned (OR), de-duplicated. To read a fragment's content, \
                          copy one whole ContextRef object from the results and pass it as the `ref` argument \
                          of context.read. Use this to pull supporting context on demand instead of preloading \
                          everything."
                .to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "topics": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Lowercase topic tags to search. Multiple topics are OR-combined."
                    }
                },
                "required": ["topics"]
            }),
            builder: Arc::new(|_ctx| {
                let toolset = Arc::new(tokio::sync::RwLock::new(
                    crate::runtime::context_tools::ContextToolSet::new(),
                ));
                Box::new(builtin::context_tools_adapter::ContextSearchTool::new(toolset))
            }),
        });
    }

    {
        // context.read — metadata read directly from context_tools_adapter.rs source
        api.register_tool(ToolDescriptor {
            name: "context.read".to_string(),
            description: "Materialize the content of one context fragment. The `ref` argument is a ContextRef \
                          object exactly as returned by context.search (shaped { \"id\", \"source\", \"label\"? }) \
                          — copy a whole element from the search results. Returns the fragment as a JSON \
                          ContextArtifact with its `content`, `ref`, and any `citations`."
                .to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "object",
                        "description": "A ContextRef object from a prior context.search result.",
                        "properties": {
                            "id": { "type": "string" },
                            "source": {
                                "type": "string",
                                "description": "One of: conversation, task_trace, codebase, browser, memory, artifacts, team, automation, cluster."
                            },
                            "label": { "type": "string" }
                        },
                        "required": ["id", "source"]
                    }
                },
                "required": ["ref"]
            }),
            builder: Arc::new(|_ctx| {
                let toolset = Arc::new(tokio::sync::RwLock::new(
                    crate::runtime::context_tools::ContextToolSet::new(),
                ));
                Box::new(builtin::context_tools_adapter::ContextReadTool::new(toolset))
            }),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke-test: register_all completes without panic and populates descriptors
    /// for all 17 expected tools.
    #[test]
    fn register_all_smoke_registers_expected_tools() {
        let mut api = AgentApi::new();
        register_all(&mut api);

        let expected = [
            "read_file",
            "write_file",
            "get_file_skeleton",
            "grep",
            "glob",
            "edit",
            "bash",
            "web_fetch",
            "http_request",
            "plan_write",
            "plan_update",
            "ask_user",
            "exit_plan_mode",
            "request_plan_mode_switch",
            "self_eval",
            "context.search",
            "context.read",
        ];

        for name in &expected {
            assert!(
                api.tool(name).is_some(),
                "expected descriptor for tool '{name}' to be registered"
            );
        }

        // Minimum count guard: at least 17 descriptors registered.
        assert!(
            api.tools.len() >= 17,
            "expected at least 17 tool descriptors, got {}",
            api.tools.len()
        );
    }

    /// Each registered descriptor has non-empty name, description, and a
    /// JSON object parameters_schema (the LLM tools/list contract).
    #[test]
    fn all_registered_descriptors_have_valid_metadata() {
        let mut api = AgentApi::new();
        register_all(&mut api);

        for (name, descriptor) in &api.tools {
            assert!(!descriptor.name.is_empty(), "name empty for {name}");
            assert!(!descriptor.description.is_empty(), "description empty for {name}");
            assert!(
                descriptor.parameters_schema.is_object(),
                "parameters_schema must be a JSON object for {name}"
            );
        }
    }
}
