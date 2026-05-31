//! 工具注册表装配 —— 从 send_message 处理器抽出,集中一处构建。
//!
//! ## P3-2 migration (2026-05-29)
//!
//! `build_tool_registry()` is now the stateful tool assembly seam:
//!
//! - **17 ToolDescriptor-migrated tools** (filesystem, web, plan, ask_user,
//!   exit_plan_mode, request_plan_mode_switch, self_eval, context.*) are
//!   registered via `state.agent_api.build_session_registry(&ctx)`.  Their
//!   descriptors were registered at boot by `builtin_descriptors::register_all()`
//!   (Task 4) wired into `AppState::new()` (Task 5).
//!
//! - Stateful tools (skills, memu, browser automation, MCP proxy) remain
//!   concrete adapters assembled here because their constructors need live
//!   `AppState` fields not yet surfaced on `SessionContext`.
use std::path::PathBuf;
use std::sync::Arc;

use crate::agent::tools::tool::ToolRegistry;
use crate::app::AppState;

/// 构建某会话的工具注册表(builtin + memu + browser + MCP proxy)。
///
/// Starts from `AgentApi.build_session_registry(&ctx)` (descriptor-based
/// tools), then appends stateful adapters behind one assembly seam.
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
    // item2 — resolve the project-check config here (async) so the sync
    // descriptor builder for `edit` can apply it without awaiting the lock.
    let edit_project_check = {
        let cfg = state.memubot_config.read().await;
        if cfg.memory_os.edit_project_check_enabled {
            Some(crate::agent::tools::builtin::edit_verify::ProjectCheckCfg {
                timeout_secs: cfg.memory_os.edit_project_check_timeout_secs,
            })
        } else {
            None
        }
    };

    // P3-2: Construct SessionContext and obtain the 17 descriptor-migrated tools.
    let ctx = crate::agent::api::session_context::SessionContext {
        session_id: session_id.clone(),
        workspace: workspace.clone(),
        model: model.clone(),
        app_handle: app_handle.clone(),
        llm: llm.clone(),
        app_state: state,
        edit_project_check,
    };
    let mut tools = state.agent_api.build_session_registry(&ctx);

    register_skill_tools(
        &mut tools,
        app_handle.clone(),
        state,
        session_id.clone(),
        workspace.clone(),
    );

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
        let runtime_provider_config = state
            .settings
            .read()
            .await
            .browser_runtime_provider_config
            .clone();
        let mcp_manager = Some(Arc::clone(&state.mcp_manager));
        let browser_active = ctx_mgr.has_context(&sid).await;
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
        if browser_active {
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
        }
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
        tracing::info!(
            session_id = %sid,
            browser_active,
            browser_tools = if browser_active { 28 } else { 3 },
            "Registered browser tools for agent loop"
        );
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

fn register_skill_tools(
    tools: &mut ToolRegistry,
    app_handle: tauri::AppHandle,
    state: &AppState,
    session_id: String,
    workspace: PathBuf,
) {
    tools.register(
        crate::agent::tools::builtin::skill_search::SkillSearchTool::new(
            Arc::clone(&state.skills_registry),
            Arc::clone(&state.memory_graph_store),
            app_handle.clone(),
            session_id.clone(),
            "default".into(),
        )
        .with_memu(state.memu_client.clone()),
    );
    tools.register(crate::agent::tools::builtin::load_skill::LoadSkillTool::new(
        Arc::clone(&state.skills_registry),
        Arc::clone(&state.memory_graph_store),
        app_handle.clone(),
        session_id.clone(),
        "default".into(),
    ));
    tools.register(crate::agent::tools::builtin::skill_write::SkillWriteTool::new(
        Arc::clone(&state.skills_registry),
        state.data_dir.clone(),
        Some(workspace),
        app_handle.clone(),
        session_id.clone(),
    ));
    tools.register(
        crate::agent::tools::builtin::skill_marketplace::SkillMarketplaceSearchTool::new(),
    );
    tools.register(
        crate::agent::tools::builtin::skill_marketplace::SkillInstallFromMarketplaceTool::new(
            Arc::clone(&state.skills_registry),
            state.data_dir.clone(),
            app_handle,
            session_id,
        ),
    );
}
