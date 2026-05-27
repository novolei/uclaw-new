//! 工具注册表装配 —— 从 send_message 处理器抽出,集中一处构建。
use std::path::PathBuf;
use std::sync::Arc;
use crate::agent::tools::tool::ToolRegistry;
use crate::agent::tools::builtin;
use crate::app::AppState;

/// 构建某会话的工具注册表(builtin + memu + browser + MCP proxy)。
/// async:需 read-lock state.settings + state.mcp_manager 生成 MCP proxy。
pub async fn build_tool_registry(
    app_handle: tauri::AppHandle,
    state: &AppState,
    session_id: String,
    workspace: PathBuf,
    llm: Arc<dyn crate::llm::LlmProvider>,
    model: String,
) -> Arc<ToolRegistry> {
    let mut tools = ToolRegistry::new();
    tools.register(builtin::file::ReadFileTool::new(workspace.clone()));
    tools.register(builtin::file::WriteFileTool::new(workspace.clone()));
    tools.register(builtin::get_file_skeleton::GetFileSkeletonTool::new(workspace.clone()));
    tools.register(builtin::search::GrepTool::new(workspace.clone()));
    tools.register(builtin::search::GlobTool::new(workspace.clone()));
    tools.register(builtin::web::WebFetchTool::new());
    tools.register(builtin::web::HttpRequestTool::new());
    tools.register(builtin::edit::EditTool::new(workspace.clone()));
    tools.register(builtin::shell::BashTool::new(workspace.clone()));
    tools.register(builtin::ask_user::AskUserTool::new(
        app_handle.clone(),
        Arc::clone(&state.pending_ask_users),
        session_id.clone(),
    ));
    tools.register(builtin::exit_plan_mode::ExitPlanModeTool::new(
        app_handle.clone(),
        Arc::clone(&state.pending_exit_plans),
        session_id.clone(),
    ));
    tools.register(builtin::plan::PlanWriteTool::new(workspace.clone(), app_handle.clone()));
    tools.register(builtin::plan::PlanUpdateTool::new(workspace.clone(), app_handle.clone()));
    tools.register(builtin::plan_mode::RequestPlanModeSwitchTool::new(
        app_handle.clone(),
        session_id.clone(),
        Arc::clone(&state.db),
    ));
    tools.register(
        builtin::self_eval::SelfEvalTool::new(
            session_id.clone(),
            Arc::clone(&state.db),
            app_handle.clone(),
        ).with_infra(Arc::clone(&state.infra_service))
    );
    // C2-Dirac-B2 — M2-F context tools. ONLY the two working ops are
    // registered: context.search + context.read (spec §8.5). The other
    // five ContextToolSet ops (fold/cite/compare/pin/release) are
    // unimplemented stubs / lifecycle ops out of B2 scope and MUST NOT be
    // wrapped — registering them would let the LLM call tools that just
    // fail. The ContextToolSet starts empty; fragment lifecycle (when
    // fragments enter/leave the set) is a M2-D follow-up. It is a separate
    // fragment set from the ChatDelegate's ContextManager (selection for
    // the prompt) — unifying the two is also M2-D's job.
    {
        let context_toolset = Arc::new(tokio::sync::RwLock::new(
            crate::runtime::context_tools::ContextToolSet::new(),
        ));
        tools.register(builtin::context_tools_adapter::ContextSearchTool::new(
            context_toolset.clone(),
        ));
        tools.register(builtin::context_tools_adapter::ContextReadTool::new(
            context_toolset,
        ));
    }
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
