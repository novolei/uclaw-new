//! 工具注册表装配 —— 从 send_message 处理器抽出,集中一处构建。
//!
//! ## P3-2 migration (2026-05-29)
//!
//! `build_tool_registry()` is now a **partial shim**:
//!
//! - **17 ToolDescriptor-migrated tools** (filesystem, web, plan, ask_user,
//!   exit_plan_mode, request_plan_mode_switch, self_eval, context.*) are
//!   registered via `state.agent_api.build_session_registry(&ctx)`.  Their
//!   descriptors were registered at boot by `builtin_descriptors::register_all()`
//!   (Task 4) wired into `AppState::new()` (Task 5).
//!
//! - **30 deferred tools** (browser automation, memu, MCP proxy) remain as
//!   inline `tools.register(...)` calls.  They need async context or state
//!   fields (`memu_client`, `mcp_manager`, browser `runtime_provider_config`)
//!   not yet surfaced on `SessionContext`.  Migration is tracked as P3-2.5.
use std::path::PathBuf;
use std::sync::Arc;
use crate::agent::tools::tool::ToolRegistry;
use crate::app::AppState;

/// 构建某会话的工具注册表(builtin + memu + browser + MCP proxy)。
///
/// Starts from `AgentApi.build_session_registry(&ctx)` (17 descriptor-based
/// tools), then appends the 30 deferred tools inline.
///
/// async: needs read-lock on state.settings for browser runtime config +
/// state.mcp_manager to enumerate MCP proxy tools.
pub async fn build_tool_registry(
    app_handle: tauri::AppHandle,
    state: &AppState,
    session_id: String,
    workspace: PathBuf,
    llm: Arc<dyn crate::llm::LlmProvider>,
    model: String,
) -> Arc<ToolRegistry> {
    // P3-2: Construct SessionContext and obtain the 17 descriptor-migrated tools.
    let ctx = crate::agent::api::session_context::SessionContext {
        session_id: session_id.clone(),
        workspace: workspace.clone(),
        model: model.clone(),
        app_handle: app_handle.clone(),
        llm: llm.clone(),
        app_state: state,
    };
    let mut tools = state.agent_api.build_session_registry(&ctx);

    // ── Deferred tools (P3-2.5 migration follow-up) ──────────────────────────
    // The following tools are not yet ToolDescriptor-based because their
    // constructors need state not surfaced on SessionContext yet (memu_client,
    // mcp_manager async ops, browser runtime provider config, etc.).

    crate::agent::tools::memu_tools::register_memu_tools(
        &mut tools,
        state.memu_client.clone(),
        Some(Arc::clone(&state.memory_graph_store)),
    );
    // Browser tools (v2 — BrowserContextManager)
    {
        use crate::browser::decision::LlmBrowserDecisionAdapter;
        use crate::browser::intervention_bridge::BrowserAskUserBridge;
        use crate::browser::memory_adapter::BrowserLongTermMemoryAdapter;
        use crate::browser::task_store::BrowserTaskStore;
        use crate::browser::tools::*;
        let ctx_mgr = Arc::clone(&state.browser_context_manager);
        let sid = session_id.clone();
        let task_store = Arc::new(BrowserTaskStore::new(Arc::clone(&state.db)));
        let long_term_memory = Arc::new(BrowserLongTermMemoryAdapter::new(
            Arc::clone(&state.memory_store),
            Some(Arc::clone(&state.mcp_manager)),
        ));
        let ask_user_bridge = Arc::new(BrowserAskUserBridge::new(
            app_handle.clone(),
            Arc::clone(&state.pending_ask_users),
            sid.clone(),
        ));
        let decision_adapter = Arc::new(LlmBrowserDecisionAdapter::new(
            Arc::clone(&llm),
            model.clone(),
        ));
        let runtime_status_service = Some(Arc::clone(&state.browser_runtime_status_service));
        let runtime_provider_config = state.settings.read().await.browser_runtime_provider_config.clone();
        let mcp_manager = Some(Arc::clone(&state.mcp_manager));
        macro_rules! bt {
            ($T:ident) => {
                $T {
                    ctx_mgr: Arc::clone(&ctx_mgr),
                    session_id: sid.clone(),
                    runtime_status_service: runtime_status_service.clone(),
                    runtime_provider_config: runtime_provider_config.clone(),
                    mcp_manager: mcp_manager.clone(),
                }
            };
        }
        tools.register(bt!(BrowserNavigateTool));
        tools.register(bt!(BrowserGoBackTool));
        tools.register(bt!(BrowserGoForwardTool));
        tools.register(bt!(BrowserReloadTool));
        tools.register(bt!(BrowserGetDomTool));
        tools.register(BrowserScreenshotTool {
            ctx_mgr: Arc::clone(&ctx_mgr),
            session_id: sid.clone(),
            runtime_status_service: runtime_status_service.clone(),
            runtime_provider_config: runtime_provider_config.clone(),
            mcp_manager: mcp_manager.clone(),
            workspace_root: Some(workspace.clone()),
        });
        tools.register(bt!(BrowserExtractTool));
        tools.register(bt!(BrowserClickTool));
        tools.register(bt!(BrowserTypeTool));
        tools.register(bt!(BrowserSelectTool));
        tools.register(bt!(BrowserScrollTool));
        tools.register(bt!(BrowserSendKeysTool));
        tools.register(bt!(BrowserEvaluateTool));
        tools.register(bt!(BrowserManageTabsTool));
        tools.register(bt!(BrowserGetCookiesTool));
        tools.register(bt!(BrowserSetCookieTool));
        tools.register(bt!(BrowserWaitTool));
        tools.register(bt!(BrowserHoverTool));
        tools.register(bt!(BrowserUploadFileTool));
        tools.register(bt!(BrowserGetStateTool));
        tools.register(bt!(BrowserListTabsTool));
        tools.register(bt!(BrowserSwitchTabTool));
        tools.register(bt!(BrowserCloseTabTool));
        tools.register(bt!(BrowserListSessionsTool));
        tools.register(bt!(BrowserCloseSessionTool));
        tools.register(bt!(BrowserCloseAllTool));
        tools.register(BrowserTaskTool {
            ctx_mgr: Arc::clone(&ctx_mgr),
            session_id: sid.clone(),
            decision_adapter: decision_adapter.clone(),
            task_store: Some(Arc::clone(&task_store)),
            ask_user_bridge: Some(Arc::clone(&ask_user_bridge)),
            long_term_memory: Some(Arc::clone(&long_term_memory)),
            identity_task_registry: Some(Arc::clone(&state.browser_identity_task_registry)),
            runtime_status_service: runtime_status_service.clone(),
            runtime_provider_config: runtime_provider_config.clone(),
            mcp_manager: mcp_manager.clone(),
            // Slice 1b follow-up: activate the Evaluate-gate chokepoint.
            safety_manager: Some(Arc::clone(&state.safety_manager)),
            pending_approvals: Some(Arc::clone(&state.pending_approvals)),
        });
        tools.register(BrowserTaskResumeTool {
            ctx_mgr: Arc::clone(&ctx_mgr),
            session_id: sid.clone(),
            decision_adapter: decision_adapter.clone(),
            task_store: Some(Arc::clone(&task_store)),
            ask_user_bridge: Some(Arc::clone(&ask_user_bridge)),
            long_term_memory: Some(Arc::clone(&long_term_memory)),
            identity_task_registry: Some(Arc::clone(&state.browser_identity_task_registry)),
            runtime_status_service: runtime_status_service.clone(),
            runtime_provider_config: runtime_provider_config.clone(),
            mcp_manager: mcp_manager.clone(),
            // Slice 1b follow-up: activate the Evaluate-gate chokepoint.
            safety_manager: Some(Arc::clone(&state.safety_manager)),
            pending_approvals: Some(Arc::clone(&state.pending_approvals)),
        });
        tools.register(RetryWithBrowserAgentTool {
            ctx_mgr: Arc::clone(&ctx_mgr),
            session_id: sid.clone(),
            decision_adapter,
            task_store: Some(task_store),
            ask_user_bridge: Some(ask_user_bridge),
            long_term_memory: Some(long_term_memory),
            identity_task_registry: Some(Arc::clone(&state.browser_identity_task_registry)),
            runtime_status_service: runtime_status_service.clone(),
            runtime_provider_config: runtime_provider_config.clone(),
            mcp_manager: mcp_manager.clone(),
            // Slice 1b follow-up: activate the Evaluate-gate chokepoint.
            safety_manager: Some(Arc::clone(&state.safety_manager)),
            pending_approvals: Some(Arc::clone(&state.pending_approvals)),
        });
    }
    // MCP tool proxies — agents see tools from any currently-connected
    // MCP server as `mcp__{server_id}__{tool_name}`. Sourced from
    // `state.mcp_manager`'s live state, so a server connected mid-
    // session won't appear until the next user turn. Without this
    // block the entire MCP subsystem is invisible to the LLM (MCP
    // PR-1 — 2026-05-18 audit).
    {
        let mgr = state.mcp_manager.read().await;
        let proxies = crate::mcp::McpManager::create_tool_proxies(
            &state.mcp_manager,
            &*mgr,
        );
        let n = proxies.len();
        for p in proxies {
            tools.register(p);
        }
        if n > 0 {
            tracing::info!(mcp_tools = n, "Registered MCP tools for agent loop");
        }
    }
    Arc::new(tools)
}
