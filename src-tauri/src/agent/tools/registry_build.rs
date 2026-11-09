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
///
/// `announce_full_browser` (context continuity P3, Decision 14): the session's
/// frame already announced the full browser tool set, so it stays registered
/// even without a live browser context — the tool list (and the provider's
/// prompt cache) stays stable. Without a context those tools return a tool
/// error instead of silently relaunching Chrome. Non-transcript callers pass
/// `false` (unchanged behavior).
pub async fn build_tool_registry(
    app_handle: tauri::AppHandle,
    state: &AppState,
    session_id: String,
    workspace: PathBuf,
    llm: Arc<dyn crate::llm::LlmProvider>,
    model: String,
    announce_full_browser: bool,
) -> Arc<ToolRegistry> {
    // P3-2.5 — resolve the per-session tool config here (async; one read guard)
    // and hand it to the sync core-tool constructors via SessionContext.
    let tool_config = {
        let cfg = state.memubot_config.read().await;
        crate::agent::tools::core_tools::ToolConfig {
            edit_project_check: if cfg.memory_os.edit_project_check_enabled {
                Some(crate::agent::tools::builtin::edit_verify::ProjectCheckCfg {
                    timeout_secs: cfg.memory_os.edit_project_check_timeout_secs,
                })
            } else {
                None
            },
            read_file_max_chars: cfg.memory_os.read_file_max_chars,
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
        tool_config,
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
        std::sync::Arc::clone(&state.bucket_seal_adapter),
    );
    // Step 2c — native memory_* page tools backed by EntityPage + bucket_seal.
    // These replace the former gbrain MCP page tools (no longer exposed to the agent).
    {
        use crate::agent::tools::builtin::memory_pages::{
            MemoryGetPageTool, MemoryListPagesTool, MemoryPutPageTool, MemoryQueryTool,
            MemorySearchPagesTool,
        };
        let bucket_seal_as_adapter: std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter> =
            std::sync::Arc::clone(&state.bucket_seal_adapter)
                as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;
        tools.register(MemoryPutPageTool::new(
            std::sync::Arc::clone(&state.memory_graph_store),
            bucket_seal_as_adapter,
        ));
        tools.register(MemoryGetPageTool::new(std::sync::Arc::clone(
            &state.memory_graph_store,
        )));
        tools.register(MemoryListPagesTool::new(std::sync::Arc::clone(
            &state.memory_graph_store,
        )));
        tools.register(MemorySearchPagesTool::new(std::sync::Arc::clone(
            &state.memory_graph_store,
        )));
        tools.register(MemoryQueryTool::new(std::sync::Arc::clone(
            &state.bucket_seal_adapter,
        )));
    }
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
        let bucket_seal_adapter: Arc<dyn crate::memory_adapter::MemoryAdapter> =
            Arc::clone(&state.bucket_seal_adapter) as Arc<dyn crate::memory_adapter::MemoryAdapter>;
        let long_term_memory = Arc::new(BrowserLongTermMemoryAdapter::new(
            Arc::clone(&state.memory_store),
            Some(Arc::clone(&state.memory_graph_store)),
            Some(bucket_seal_adapter),
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
        let direct = DirectBrowserToolDeps {
            ctx_mgr: Arc::clone(&ctx_mgr),
            session_id: sid.clone(),
            runtime_status_service: runtime_status_service.clone(),
            runtime_provider_config: runtime_provider_config.clone(),
            mcp_manager: mcp_manager.clone(),
        };
        let direct_tools = register_direct_browser_tools(
            &mut tools,
            &direct,
            &workspace,
            browser_active,
            announce_full_browser,
        );
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
            announce_full_browser,
            browser_tools = direct_tools + 3,
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

/// Shared state of the per-session direct browser tools (`bt!` family).
pub(crate) struct DirectBrowserToolDeps {
    pub ctx_mgr: Arc<crate::browser::context_manager::BrowserContextManager>,
    pub session_id: String,
    pub runtime_status_service: Option<Arc<crate::browser::runtime_status::BrowserRuntimeStatusService>>,
    pub runtime_provider_config: crate::browser::runtime_control_center::BrowserRuntimeProviderConfig,
    pub mcp_manager: Option<crate::mcp::SharedMcpManager>,
}

/// Tools that only inspect / tear down contexts — fine without a live one.
const CONTEXT_FREE_BROWSER_TOOLS: &[&str] =
    &["browser_list_sessions", "browser_close_session", "browser_close_all"];

/// Register `browser_navigate` and — when the session has a live browser
/// context or its frame already announced the full set
/// (`browser_active || announce_full_browser`) — the 25 context-bound direct
/// browser tools. Registered only because of the announcement (no context at
/// build time), the context-bound tools are wrapped in
/// [`ContextBoundBrowserTool`]. Returns the number of tools registered.
pub(crate) fn register_direct_browser_tools(
    tools: &mut ToolRegistry,
    d: &DirectBrowserToolDeps,
    workspace: &std::path::Path,
    browser_active: bool,
    announce_full_browser: bool,
) -> usize {
    use crate::browser::tools::*;
    macro_rules! bt {
        ($T:ident) => {
            Box::new($T {
                ctx_mgr: Arc::clone(&d.ctx_mgr),
                session_id: d.session_id.clone(),
                runtime_status_service: d.runtime_status_service.clone(),
                runtime_provider_config: d.runtime_provider_config.clone(),
                mcp_manager: d.mcp_manager.clone(),
            }) as Box<dyn crate::agent::tools::tool::Tool>
        };
    }
    tools.register_boxed(bt!(BrowserNavigateTool));
    if !(browser_active || announce_full_browser) {
        return 1;
    }
    let context_bound: Vec<Box<dyn crate::agent::tools::tool::Tool>> = vec![
        bt!(BrowserGoBackTool),
        bt!(BrowserGoForwardTool),
        bt!(BrowserReloadTool),
        bt!(BrowserGetDomTool),
        Box::new(BrowserScreenshotTool {
            ctx_mgr: Arc::clone(&d.ctx_mgr),
            session_id: d.session_id.clone(),
            runtime_status_service: d.runtime_status_service.clone(),
            runtime_provider_config: d.runtime_provider_config.clone(),
            mcp_manager: d.mcp_manager.clone(),
            workspace_root: Some(workspace.to_path_buf()),
        }),
        bt!(BrowserExtractTool),
        bt!(BrowserClickTool),
        bt!(BrowserTypeTool),
        bt!(BrowserSelectTool),
        bt!(BrowserScrollTool),
        bt!(BrowserSendKeysTool),
        bt!(BrowserEvaluateTool),
        bt!(BrowserManageTabsTool),
        bt!(BrowserGetCookiesTool),
        bt!(BrowserSetCookieTool),
        bt!(BrowserWaitTool),
        bt!(BrowserHoverTool),
        bt!(BrowserUploadFileTool),
        bt!(BrowserGetStateTool),
        bt!(BrowserListTabsTool),
        bt!(BrowserSwitchTabTool),
        bt!(BrowserCloseTabTool),
        bt!(BrowserListSessionsTool),
        bt!(BrowserCloseSessionTool),
        bt!(BrowserCloseAllTool),
    ];
    let n = 1 + context_bound.len();
    for tool in context_bound {
        if browser_active || CONTEXT_FREE_BROWSER_TOOLS.contains(&tool.name()) {
            tools.register_boxed(tool);
        } else {
            tools.register_boxed(Box::new(ContextBoundBrowserTool {
                inner: tool,
                ctx_mgr: Arc::clone(&d.ctx_mgr),
                session_id: d.session_id.clone(),
            }));
        }
    }
    n
}

/// A context-bound browser tool kept announced (sticky) after the session's
/// browser context went away. Identical definition (name, description,
/// schema, effects, approval); executing it without a live context is a
/// `PreconditionFailed` tool error rather than a silent Chrome relaunch.
/// The context is checked at call time, so once `browser_navigate` reopens
/// one, the tool works within the same turn.
struct ContextBoundBrowserTool {
    inner: Box<dyn crate::agent::tools::tool::Tool>,
    ctx_mgr: Arc<crate::browser::context_manager::BrowserContextManager>,
    session_id: String,
}

impl ContextBoundBrowserTool {
    async fn require_context(&self) -> Result<(), crate::agent::tools::tool::ToolError> {
        if self.ctx_mgr.has_context(&self.session_id).await {
            return Ok(());
        }
        Err(crate::agent::tools::tool::ToolError::kinded(
            crate::agent::tools::tool::ToolErrorKind::PreconditionFailed,
            format!(
                "浏览器会话已关闭（当前没有打开的浏览器上下文），{} 无法执行。请先调用 browser_navigate 打开页面，再重试。",
                self.inner.name()
            ),
        ))
    }
}

#[async_trait::async_trait]
impl crate::agent::tools::tool::Tool for ContextBoundBrowserTool {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn description(&self) -> &str {
        self.inner.description()
    }
    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }
    async fn execute(
        &self,
        params: serde_json::Value,
    ) -> Result<crate::agent::tools::tool::ToolOutput, crate::agent::tools::tool::ToolError> {
        self.require_context().await?;
        self.inner.execute(params).await
    }
    fn supports_streaming(&self) -> bool {
        self.inner.supports_streaming()
    }
    async fn execute_streaming(
        &self,
        params: serde_json::Value,
        sink: crate::agent::tools::stream::ToolStreamSink,
    ) -> Result<crate::agent::tools::tool::ToolOutput, crate::agent::tools::tool::ToolError> {
        self.require_context().await?;
        self.inner.execute_streaming(params, sink).await
    }
    async fn execute_streaming_with_cancel(
        &self,
        params: serde_json::Value,
        sink: crate::agent::tools::stream::ToolStreamSink,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<crate::agent::tools::tool::ToolOutput, crate::agent::tools::tool::ToolError> {
        self.require_context().await?;
        self.inner.execute_streaming_with_cancel(params, sink, cancel).await
    }
    fn estimated_cost(&self, params: &serde_json::Value) -> Option<f64> {
        self.inner.estimated_cost(params)
    }
    fn estimated_duration(&self, params: &serde_json::Value) -> Option<std::time::Duration> {
        self.inner.estimated_duration(params)
    }
    fn requires_approval(
        &self,
        params: &serde_json::Value,
    ) -> crate::agent::tools::tool::ApprovalRequirement {
        self.inner.requires_approval(params)
    }
    fn path_args<'a>(&self, arguments: &'a serde_json::Value) -> Vec<&'a str> {
        self.inner.path_args(arguments)
    }
    fn preview_target_path(&self, args: &serde_json::Value) -> Option<String> {
        self.inner.preview_target_path(args)
    }
    fn effects(&self) -> crate::agent::tools::tool::ToolEffects {
        self.inner.effects()
    }
    fn concurrency(&self) -> crate::agent::tools::tool::ToolConcurrency {
        self.inner.concurrency()
    }
}

fn register_skill_tools(
    tools: &mut ToolRegistry,
    app_handle: tauri::AppHandle,
    state: &AppState,
    session_id: String,
    workspace: PathBuf,
) {
    {
        tools.register(
            crate::agent::tools::builtin::skill_search::SkillSearchTool::new(
                Arc::clone(&state.skills_registry),
                Arc::clone(&state.bucket_seal_adapter),
                Arc::clone(&state.memory_graph_store),
                app_handle.clone(),
                session_id.clone(),
                "default".into(),
            ),
        );
    }
    {
        let skill_adapter: std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter> =
            std::sync::Arc::clone(&state.bucket_seal_adapter) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;
        tools.register(crate::agent::tools::builtin::load_skill::LoadSkillTool::new(
            Arc::clone(&state.skills_registry),
            skill_adapter,
            Arc::clone(&state.memory_graph_store),
            app_handle.clone(),
            session_id.clone(),
            "default".into(),
        ));
    }
    tools.register(crate::agent::tools::builtin::skill_write::SkillWriteTool::new(
        Arc::clone(&state.skills_registry),
        state.data_dir.clone(),
        Some(workspace.clone()),
        app_handle.clone(),
        session_id.clone(),
    ));
    // Marketplace keys are read once per registry build (never logged).
    let (skills_sh_key, skillsmp_key) = state
        .db
        .lock()
        .map(|conn| {
            use crate::skills_marketplace::{commands::read_api_key, MarketplaceProvider};
            (
                read_api_key(&conn, MarketplaceProvider::SkillsSh),
                read_api_key(&conn, MarketplaceProvider::Skillsmp),
            )
        })
        .unwrap_or((None, None));
    tools.register(
        crate::agent::tools::builtin::skill_marketplace::SkillMarketplaceSearchTool::new(skills_sh_key, skillsmp_key),
    );
    tools.register(
        crate::agent::tools::builtin::skill_marketplace::SkillInstallFromMarketplaceTool::new(
            Arc::clone(&state.skills_registry),
            state.data_dir.clone(),
            Arc::clone(&state.db),
            Some(workspace),
            app_handle,
            session_id,
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::context_manager::BrowserContextManager;

    fn deps() -> DirectBrowserToolDeps {
        DirectBrowserToolDeps {
            ctx_mgr: Arc::new(BrowserContextManager::new_for_test(
                std::env::temp_dir().join("uclaw-registry-build-test"),
            )),
            session_id: "s1".into(),
            runtime_status_service: None,
            runtime_provider_config: Default::default(),
            mcp_manager: None,
        }
    }

    fn names(tools: &ToolRegistry) -> Vec<String> {
        tools.list_definitions().into_iter().map(|d| d.name).collect()
    }

    #[test]
    fn sticky_browser_tools_registered_when_frame_announced() {
        let ws = std::env::temp_dir();
        let mut sticky = ToolRegistry::new();
        let n = register_direct_browser_tools(&mut sticky, &deps(), &ws, false, true);
        // 26 direct tools + the 3 always-registered task tools = 29 announced.
        assert_eq!(n, 26);
        assert_eq!(sticky.len(), 26);
        let got = names(&sticky);
        for t in ["browser_navigate", "browser_go_back", "browser_screenshot", "browser_close_all"] {
            assert!(got.iter().any(|n| n == t), "{t} missing from {got:?}");
        }

        // Same names, descriptions and schemas as with a live context.
        let mut active = ToolRegistry::new();
        assert_eq!(register_direct_browser_tools(&mut active, &deps(), &ws, true, false), 26);
        let defs = |r: &ToolRegistry| serde_json::to_string(&r.list_definitions()).unwrap();
        assert_eq!(defs(&sticky), defs(&active));

        // Without the flag and without a context: only browser_navigate.
        let mut bare = ToolRegistry::new();
        assert_eq!(register_direct_browser_tools(&mut bare, &deps(), &ws, false, false), 1);
        assert_eq!(names(&bare), vec!["browser_navigate"]);
    }

    #[tokio::test]
    async fn sticky_browser_tools_without_context_return_tool_error() {
        let ws = std::env::temp_dir();
        let mut tools = ToolRegistry::new();
        register_direct_browser_tools(&mut tools, &deps(), &ws, false, true);
        let err = tools
            .get("browser_go_back")
            .unwrap()
            .execute(serde_json::json!({ "tab_id": "tab-1" }))
            .await
            .expect_err("no browser context → tool error, no relaunch");
        let msg = err.to_string();
        assert!(msg.contains("PreconditionFailed") && msg.contains("browser_navigate"), "{msg}");
        // Session-management tools still work without a context.
        let listed = tools
            .get("browser_list_sessions")
            .unwrap()
            .execute(serde_json::json!({}))
            .await;
        assert!(listed.is_ok(), "{listed:?}");
    }
}
