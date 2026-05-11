// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tauri::Manager;
use uclaw_core::app::AppState;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| {
                    // chromiumoxide::handler emits WARN for every CDP event type Chrome sends
                    // that isn't defined in its schema — these are untagged-enum parse misses,
                    // not real errors. Silence them so the log stays readable.
                    "info,chromiumoxide::handler=error".into()
                }),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_state = AppState::new(app.handle())?;

            // System tray icon
            let tray = tauri::tray::TrayIconBuilder::new()
                .tooltip("uClaw — AI Coworker")
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left, ..
                    } = event {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;
            let _ = tray; // keep alive — owned by the app

            // Spawn HTTP server for remote access
            let session_mgr = app_state.session_manager.clone();
            let data_dir = app_state.data_dir.clone();
            let jwt_secret = uclaw_core::api::auth::generate_secret();

            let ws_manager = uclaw_core::api::ws::WsConnectionManager::new();

            let http_state = uclaw_core::api::auth::HttpServerState {
                session_manager: session_mgr,
                jwt_secret,
                data_dir,
                ws_manager: ws_manager.clone(),
            };

            let router = uclaw_core::api::router::build_router(http_state);

            // Spawn HTTP server in a background thread with its own Tokio runtime
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new()
                    .expect("Failed to create Tokio runtime for HTTP server");
                rt.block_on(async move {
                    // Spawn stale WebSocket connection reaper
                    uclaw_core::api::ws::spawn_stale_connection_reaper(ws_manager);

                    let listener = tokio::net::TcpListener::bind("127.0.0.1:27270")
                        .await
                        .expect("Failed to bind HTTP server");
                    tracing::info!("uClaw HTTP API server listening on http://127.0.0.1:27270");
                    axum::serve(listener, router).await.expect("HTTP server error");
                });
            });

            app.manage(app_state);

            // ─── Stage 3 & 4：注册后台服务并启动 ──────────────────────
            {
                let state: tauri::State<'_, AppState> = app.state();
                let service_manager = state.service_manager.clone();
                let infra_service = state.infra_service.clone();
                let memubot_config = state.memubot_config.clone();
                let data_dir = state.data_dir.clone();
                let memu_client = state.memu_client.clone();
                let provider_service = state.provider_service.clone();
                let memory_graph_store = state.memory_graph_store.clone();
                let app_handle = app.handle().clone();
                let automation_service = state.automation_service.clone();
                let workspace_root = state.workspace_root.clone();
                let llm_config = state.llm_config.clone();
                let safety_manager = state.safety_manager.clone();
                let pending_approvals = state.pending_approvals.clone();
                let pending_ask_users = state.pending_ask_users.clone();
                let pending_exit_plans = state.pending_exit_plans.clone();

                // 在后台异步执行服务注册和启动
                tauri::async_runtime::spawn(async move {
                    // Stage 3: 注册后台服务
                    tracing::info!("[Stage 3] Registering background services...");

                    // PowerService
                    if memubot_config.power.prevent_sleep {
                        let power_svc = Arc::new(
                            uclaw_core::services::PowerService::new()
                        );
                        service_manager.register(power_svc).await;
                        tracing::info!("[Stage 3] PowerService registered");
                    }

                    // MemorizationService
                    if memubot_config.memorization.enabled {
                        let mem_db_path = data_dir.join("memorization.db");
                        match uclaw_core::memorization::MemorizationStorage::new(&mem_db_path) {
                            Ok(storage) => {
                                let mem_svc = Arc::new(
                                    uclaw_core::memorization::MemorizationService::new(
                                        memubot_config.memorization.clone(),
                                        Arc::new(storage),
                                        infra_service.clone(),
                                    )
                                );
                                // 注入 memU 客户端
                                if let Some(ref client) = memu_client {
                                    mem_svc.set_memu_client(Some(client.clone())).await;
                                }
                                service_manager.register(mem_svc).await;
                                tracing::info!("[Stage 3] MemorizationService registered");
                            }
                            Err(e) => {
                                tracing::warn!("[Stage 3] MemorizationStorage init failed: {}, skipping", e);
                            }
                        }
                    }

                    // ProactiveService
                    if memubot_config.proactive.enabled {
                        // 构建 ScenarioManager 并注册三个场景
                        let mut scenario_manager = uclaw_core::proactive::scenarios::ScenarioManager::new();

                        // Scenario 1: Always-Learning Assistant
                        if memubot_config.scenarios.conversation_learning.enabled {
                            let scenario = Arc::new(
                                uclaw_core::proactive::scenarios::conversation_learning::ConversationLearningScenario::new(
                                    memubot_config.scenarios.conversation_learning.clone(),
                                )
                            );
                            scenario_manager.register(scenario);
                            tracing::info!("[Stage 3] ConversationLearningScenario registered");
                        }

                        // Scenario 2: Self-Improving Agent
                        if memubot_config.scenarios.skill_extraction.enabled {
                            let scenario = Arc::new(
                                uclaw_core::proactive::scenarios::skill_extraction::SkillExtractionScenario::new(
                                    memubot_config.scenarios.skill_extraction.clone(),
                                )
                            );
                            scenario_manager.register(scenario);
                            tracing::info!("[Stage 3] SkillExtractionScenario registered");
                        }

                        // Scenario 3: Multimodal Context Builder
                        if memubot_config.scenarios.multimodal_context.enabled {
                            let scenario = Arc::new(
                                uclaw_core::proactive::scenarios::multimodal_context::MultimodalContextScenario::new(
                                    memubot_config.scenarios.multimodal_context.clone(),
                                )
                            );
                            scenario_manager.register(scenario);
                            tracing::info!("[Stage 3] MultimodalContextScenario registered");
                        }

                        tracing::info!("[Stage 3] {} proactive scenarios registered", scenario_manager.scenario_count());

                        let pro_db_path = data_dir.join("proactive.db");
                        match uclaw_core::proactive::ProactiveStorage::new(&pro_db_path) {
                            Ok(storage) => {
                                let pro_svc = Arc::new(
                                    uclaw_core::proactive::ProactiveService::new(
                                        memubot_config.proactive.clone(),
                                        infra_service.clone(),
                                        Arc::new(storage),
                                        Arc::new(scenario_manager),
                                        Arc::new(uclaw_core::proactive::execution_log::ExecutionLogCollector::new()),
                                        Arc::new(uclaw_core::proactive::multimodal::MultimodalQueue::new()),
                                        provider_service.clone(),
                                        memu_client.clone(),
                                        memory_graph_store.clone(),
                                        Some(app_handle.clone()),
                                    )
                                );
                                service_manager.register(pro_svc).await;
                                tracing::info!("[Stage 3] ProactiveService registered");
                            }
                            Err(e) => {
                                tracing::warn!("[Stage 3] ProactiveStorage init failed: {}, skipping", e);
                            }
                        }
                    }

                    // LocalApiService
                    if memubot_config.local_api.enabled {
                        let local_api_svc = Arc::new(
                            uclaw_core::local_api::LocalApiService::new(
                                memubot_config.local_api.clone(),
                            )
                        );
                        service_manager.register(local_api_svc).await;
                        tracing::info!("[Stage 3] LocalApiService registered");
                    }

                    // Stage 4: 启动所有已注册服务
                    tracing::info!("[Stage 4] Starting all registered services...");
                    let results = service_manager.start_all().await;
                    for (name, result) in &results {
                        match result {
                            Ok(()) => tracing::info!("[Stage 4] Service '{}' started OK", name),
                            Err(e) => tracing::error!("[Stage 4] Service '{}' failed to start: {}", name, e),
                        }
                    }
                    tracing::info!("[Stage 4] All services started ({} total)", results.len());

                    // Stage 4b: Wire AutomationService delegate factory so cron automations can run.
                    if let Some((provider_id, model, api_key, base_url)) =
                        provider_service.get_active_llm_config().await
                    {
                        let llm_cfg = {
                            let legacy = llm_config.read().await;
                            uclaw_core::llm::llm_config_from_provider(
                                &provider_id, &model, &api_key, &base_url,
                                legacy.max_tokens.unwrap_or(8192),
                                legacy.temperature.unwrap_or(0.7),
                            )
                        };
                        match uclaw_core::llm::create_provider(&llm_cfg) {
                            Ok(llm) => {
                                let llm = std::sync::Arc::new(llm);
                                let model = model.clone();
                                let workspace = workspace_root.clone();
                                let app_h = app_handle.clone();
                                let safety = safety_manager.clone();
                                let approvals = pending_approvals.clone();
                                let ask_users = pending_ask_users.clone();
                                let exit_plans = pending_exit_plans.clone();

                                let factory: std::sync::Arc<
                                    dyn Fn(String) -> Box<dyn uclaw_core::agent::types::LoopDelegate + Send>
                                        + Send + Sync,
                                > = std::sync::Arc::new(move |system_prompt: String| {
                                    use uclaw_core::agent::tools::{tool::ToolRegistry, builtin};
                                    let session_id_for_tools = uuid::Uuid::new_v4().to_string();
                                    let mut reg = ToolRegistry::new();
                                    reg.register(builtin::file::ReadFileTool::new(workspace.clone()));
                                    reg.register(builtin::file::WriteFileTool::new(workspace.clone()));
                                    reg.register(builtin::search::GrepTool::new(workspace.clone()));
                                    reg.register(builtin::search::GlobTool::new(workspace.clone()));
                                    reg.register(builtin::web::WebFetchTool::new());
                                    reg.register(builtin::edit::EditTool::new(workspace.clone()));
                                    reg.register(builtin::shell::BashTool::new(workspace.clone()));
                                    reg.register(builtin::ask_user::AskUserTool::new(
                                        app_h.clone(),
                                        std::sync::Arc::clone(&ask_users),
                                        session_id_for_tools.clone(),
                                    ));
                                    reg.register(builtin::exit_plan_mode::ExitPlanModeTool::new(
                                        app_h.clone(),
                                        std::sync::Arc::clone(&exit_plans),
                                        session_id_for_tools.clone(),
                                    ));
                                    let tools = std::sync::Arc::new(reg);
                                    Box::new(uclaw_core::agent::dispatcher::ChatDelegate::new(
                                        std::sync::Arc::clone(&llm),
                                        tools,
                                        app_h.clone(),
                                        model.clone(),
                                        system_prompt,
                                        std::sync::Arc::clone(&safety),
                                        None,
                                        std::sync::Arc::clone(&approvals),
                                        session_id_for_tools,
                                        Some(workspace.clone()),
                                    ))
                                });

                                automation_service.set_delegate_factory(factory).await;
                                tracing::info!("[Stage 4b] AutomationService delegate factory wired");
                            }
                            Err(e) => {
                                tracing::warn!("[Stage 4b] AutomationService: failed to build LLM provider: {}", e);
                            }
                        }
                    } else {
                        tracing::info!("[Stage 4b] AutomationService: no active LLM provider yet — cron will wait for factory");
                    }
                });
            }

            tracing::info!("uClaw started successfully");
            Ok(())
        })
        // ─── 优雅关闭钩子 ──────────────────────────────────────────────
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let app_handle = window.app_handle();
                if let Some(state) = app_handle.try_state::<AppState>() {
                    let service_manager = state.service_manager.clone();
                    let memu_client = state.memu_client.clone();
                    tracing::info!("[Shutdown] Window destroyed, stopping all services...");
                    // 同步阻塞停止所有服务（窗口关闭时在进程退出前执行）
                    let rt = tokio::runtime::Runtime::new().ok();
                    if let Some(rt) = rt {
                        rt.block_on(async {
                            let results = service_manager.stop_all().await;
                            for (name, result) in &results {
                                match result {
                                    Ok(()) => tracing::info!("[Shutdown] Service '{}' stopped", name),
                                    Err(e) => tracing::error!("[Shutdown] Service '{}' stop error: {}", name, e),
                                }
                            }
                            // 优雅关闭 memU client（停止 Python bridge 子进程）
                            if let Some(client) = &memu_client {
                                if let Err(e) = client.shutdown().await {
                                    tracing::warn!("[Shutdown] memU client shutdown error: {}", e);
                                }
                            }
                        });
                    }
                    tracing::info!("[Shutdown] All services stopped");
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Bootstrap
            uclaw_core::tauri_commands::get_settings,
            uclaw_core::tauri_commands::patch_settings,
            uclaw_core::tauri_commands::get_platform,
            uclaw_core::tauri_commands::get_version,
            uclaw_core::tauri_commands::get_bootstrap_status,
            // Chat
            uclaw_core::tauri_commands::send_message,
            uclaw_core::tauri_commands::create_conversation,
            uclaw_core::tauri_commands::list_conversations,
            uclaw_core::tauri_commands::list_recent_threads,
            uclaw_core::tauri_commands::get_daily_costs,
            uclaw_core::tauri_commands::get_model_costs,
            uclaw_core::tauri_commands::get_session_costs,
            uclaw_core::tauri_commands::get_messages,
            uclaw_core::tauri_commands::delete_conversation,
            uclaw_core::tauri_commands::toggle_star_conversation,
            // Spaces
            uclaw_core::tauri_commands::create_space,
            uclaw_core::tauri_commands::list_spaces,
            uclaw_core::tauri_commands::delete_space,
            // LLM Config
            uclaw_core::tauri_commands::get_llm_config,
            uclaw_core::tauri_commands::update_llm_config,
            // Artifacts
            uclaw_core::tauri_commands::list_artifacts,
            uclaw_core::tauri_commands::read_artifact,
            uclaw_core::tauri_commands::write_artifact,
            uclaw_core::tauri_commands::delete_artifact,
            // Enhanced Artifact Tree
            uclaw_core::tauri_commands::list_artifacts_tree,
            uclaw_core::tauri_commands::load_artifact_children,
            // Extended Artifact Commands
            uclaw_core::tauri_commands::create_artifact,
            uclaw_core::tauri_commands::rename_artifact,
            uclaw_core::tauri_commands::move_artifact,
            uclaw_core::tauri_commands::delete_artifact_recursive,
            uclaw_core::tauri_commands::detect_file_type,
            // Search
            uclaw_core::tauri_commands::search_workspace,
            uclaw_core::tauri_commands::search_conversations,
            uclaw_core::tauri_commands::search_all,
            // Notifications
            uclaw_core::tauri_commands::get_notifications,
            uclaw_core::tauri_commands::clear_notifications,
            // Background tasks
            uclaw_core::tauri_commands::get_background_tasks,
            // Memory
            uclaw_core::tauri_commands::memory_set,
            uclaw_core::tauri_commands::memory_get,
            uclaw_core::tauri_commands::memory_delete,
            uclaw_core::tauri_commands::memory_search,
            uclaw_core::tauri_commands::memory_list,
            uclaw_core::tauri_commands::memory_clear_namespace,
            uclaw_core::tauri_commands::memory_prune_expired,
            uclaw_core::tauri_commands::memory_bulk_import,
            uclaw_core::tauri_commands::memory_export,
            uclaw_core::tauri_commands::memory_list_namespaces,
            // MCP
            uclaw_core::tauri_commands::list_mcp_servers,
            uclaw_core::tauri_commands::add_mcp_server,
            uclaw_core::tauri_commands::remove_mcp_server,
            uclaw_core::tauri_commands::toggle_mcp_server,
            uclaw_core::tauri_commands::connect_mcp_server,
            uclaw_core::tauri_commands::disconnect_mcp_server,
            uclaw_core::tauri_commands::restart_mcp_server,
            uclaw_core::tauri_commands::list_mcp_tools,
            // Skills
            uclaw_core::tauri_commands::list_skills,
            uclaw_core::tauri_commands::toggle_skill,
            uclaw_core::tauri_commands::discover_skills,
            uclaw_core::tauri_commands::reload_skills,
            uclaw_core::tauri_commands::get_skill_detail,
            uclaw_core::tauri_commands::match_skills,
            // Channels
            uclaw_core::tauri_commands::list_channels,
            uclaw_core::tauri_commands::add_channel,
            uclaw_core::tauri_commands::remove_channel,
            uclaw_core::tauri_commands::toggle_channel,
            // Providers
            uclaw_core::tauri_commands::list_providers,
            uclaw_core::tauri_commands::list_configured_providers,
            uclaw_core::tauri_commands::get_provider_config,
            uclaw_core::tauri_commands::configure_provider,
            uclaw_core::tauri_commands::configure_provider_with_models,
            uclaw_core::tauri_commands::remove_provider_config,
            uclaw_core::tauri_commands::test_provider_connection,
            uclaw_core::tauri_commands::list_provider_models,
            uclaw_core::tauri_commands::get_configured_models,
            uclaw_core::tauri_commands::get_all_configured_models,
            uclaw_core::tauri_commands::get_active_model,
            uclaw_core::tauri_commands::set_active_model,
            uclaw_core::tauri_commands::get_role_models,
            uclaw_core::tauri_commands::set_role_model,
            // Safety
            uclaw_core::tauri_commands::get_safety_policy,
            uclaw_core::tauri_commands::set_safety_mode,
            uclaw_core::tauri_commands::set_tool_safety_override,
            uclaw_core::tauri_commands::remove_tool_safety_override,
            uclaw_core::tauri_commands::add_auto_approved_tool,
            uclaw_core::tauri_commands::remove_auto_approved_tool,
            uclaw_core::tauri_commands::block_tool,
            uclaw_core::tauri_commands::unblock_tool,
            uclaw_core::tauri_commands::assess_command_risk,
            // Tool Approval
            uclaw_core::tauri_commands::approve_tool_call,
            uclaw_core::tauri_commands::respond_ask_user,
            uclaw_core::tauri_commands::respond_exit_plan_mode,
            uclaw_core::tauri_commands::list_permission_rules,
            uclaw_core::tauri_commands::create_permission_rule,
            uclaw_core::tauri_commands::delete_permission_rule,
            uclaw_core::tauri_commands::list_permission_audit,
            // Memory Graph
            uclaw_core::tauri_commands::memory_graph_search,
            uclaw_core::tauri_commands::memory_graph_get_node,
            uclaw_core::tauri_commands::memory_graph_list_boot,
            uclaw_core::tauri_commands::memory_graph_manage_boot,
            uclaw_core::tauri_commands::memory_graph_list_timeline,
            uclaw_core::tauri_commands::memory_graph_explain_recall,
            uclaw_core::tauri_commands::memory_graph_get_full_graph,
            uclaw_core::tauri_commands::memory_graph_create_node,
            uclaw_core::tauri_commands::memory_graph_update_node,
            uclaw_core::tauri_commands::memory_graph_delete_node,
            // Learned Skills
            uclaw_core::tauri_commands::list_learned_skills,
            uclaw_core::tauri_commands::get_learned_skill,
            uclaw_core::tauri_commands::toggle_learned_skill,
            uclaw_core::tauri_commands::delete_learned_skill,
            uclaw_core::tauri_commands::record_skill_cited,
            uclaw_core::tauri_commands::backfill_skill_keywords,
            uclaw_core::tauri_commands::propose_skill_consolidation,
            uclaw_core::tauri_commands::apply_skill_consolidation,
            // MEMUBOT Services
            uclaw_core::tauri_commands::services_health,
            uclaw_core::tauri_commands::memorization_status,
            uclaw_core::tauri_commands::proactive_status,
            uclaw_core::tauri_commands::proactive_start,
            uclaw_core::tauri_commands::proactive_stop,
            uclaw_core::tauri_commands::metrics_summary,
            uclaw_core::tauri_commands::memubot_config_get,
            // Dev / Testing
            uclaw_core::tauri_commands::trigger_proactive_scenario,
            // Agent Session Control
            uclaw_core::tauri_commands::stop_agent_session,
            uclaw_core::tauri_commands::create_agent_session,
            uclaw_core::tauri_commands::list_agent_sessions,
            uclaw_core::tauri_commands::get_agent_session_messages,
            uclaw_core::tauri_commands::send_agent_message,
            uclaw_core::tauri_commands::move_agent_session_to_workspace,
            uclaw_core::tauri_commands::stop_agent,
            uclaw_core::tauri_commands::queue_agent_message,
            uclaw_core::tauri_commands::fork_agent_session,
            uclaw_core::tauri_commands::rewind_session,
            // Browser Commands (Phase 3)
            uclaw_core::tauri_commands::browser_get_state,
            uclaw_core::tauri_commands::browser_launch,
            uclaw_core::tauri_commands::browser_shutdown,
            uclaw_core::tauri_commands::browser_take_screenshot,
            // System Tray / Badge Commands (Phase 3)
            uclaw_core::tauri_commands::update_badge_count,
            // Automation Commands (Phase 3)
            uclaw_core::tauri_commands::install_automation,
            uclaw_core::tauri_commands::list_automations,
            uclaw_core::tauri_commands::trigger_automation_manual,
            uclaw_core::tauri_commands::get_automation_activity,
            // Workspace Commands
            uclaw_core::tauri_commands::get_active_workspace_id,
            uclaw_core::tauri_commands::set_active_workspace_id,
            uclaw_core::tauri_commands::create_workspace,
            uclaw_core::tauri_commands::update_workspace,
            uclaw_core::tauri_commands::reorder_workspaces,
            uclaw_core::tauri_commands::get_workspace_directories,
            uclaw_core::tauri_commands::attach_workspace_directory,
            uclaw_core::tauri_commands::detach_workspace_directory,
            uclaw_core::tauri_commands::list_session_directories,
            uclaw_core::tauri_commands::attach_session_directory,
            uclaw_core::tauri_commands::detach_session_directory,
            uclaw_core::tauri_commands::rename_attached_file,
            uclaw_core::tauri_commands::move_attached_file,
            uclaw_core::tauri_commands::read_attached_file,
            uclaw_core::tauri_commands::delete_workspace,
            uclaw_core::tauri_commands::list_directory_entries,
            uclaw_core::tauri_commands::upload_workspace_file,
            uclaw_core::tauri_commands::copy_file_into_workspace,
            uclaw_core::tauri_commands::list_always_allowed_paths,
            uclaw_core::tauri_commands::add_always_allowed_path,
            uclaw_core::tauri_commands::remove_always_allowed_path,
            uclaw_core::tauri_commands::list_session_allowed_paths,
            uclaw_core::tauri_commands::promote_session_path_to_global,
            uclaw_core::tauri_commands::path_is_directory,
            uclaw_core::tauri_commands::read_workspace_uclaw_md,
            uclaw_core::tauri_commands::write_workspace_uclaw_md,
            uclaw_core::tauri_commands::read_default_prompts,
            uclaw_core::tauri_commands::open_workspace_uclaw_md_externally,
            // Trajectory
            uclaw_core::tauri_commands::get_session_trajectory,
            uclaw_core::tauri_commands::search_trajectories,
            // Session Title
            uclaw_core::tauri_commands::generate_session_title,
            // Agent Teams
            uclaw_core::tauri_commands::start_agent_teams,
            uclaw_core::tauri_commands::get_team_channel,
            uclaw_core::tauri_commands::stop_agent_teams,
        ])
        .run(tauri::generate_context!())
        .expect("error while running uClaw");
}
