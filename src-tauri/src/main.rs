// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::sync::Mutex;
use std::collections::HashMap;
use tauri::Manager;
use tauri::Emitter;
use uclaw_core::app::AppState;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

/// 全局快捷键当前绑定注册表。
/// 存储 shortcut_id -> 当前绑定的 Tauri 格式组合键字符串（如 "Super+Shift+C"）。
pub struct GlobalShortcutRegistry {
    pub bindings: Mutex<HashMap<String, String>>,
}

fn main() {
    // _guard flushes the non-blocking file writer on Drop. Must outlive
    // the rest of main, hence the underscore-prefixed binding here.
    let _guard = uclaw_core::observability::init();
    uclaw_core::observability::install_panic_hook();

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
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

            // ─── Debug 菜单（仅 debug 模式可见） ────────────────────
            #[cfg(debug_assertions)]
            {
                use tauri::menu::SubmenuBuilder;

                let debug_submenu = SubmenuBuilder::new(app, "Debug")
                    .text("emit-memory-recall", "发射 Memory Recall 测试事件")
                    .text("emit-proactive-learning", "发射 Proactive Learning 测试事件")
                    .text("emit-self-eval", "发射 Self-Eval 测试数据（含 SkillLearned 事件）")
                    .separator()
                    .text("generate-default-config", "生成默认配置文件 (memubot_config.json)")
                    .build()?;

                let menu = app.menu().expect("menu must exist");
                menu.append(&debug_submenu)?;
                tracing::info!("[Debug] Debug menu registered (debug mode only)");
            }

            // ─── Stage 3 & 4：注册后台服务并启动 ──────────────────────
            {
                let state: tauri::State<'_, AppState> = app.state();
                let service_manager = state.service_manager.clone();
                let infra_service = state.infra_service.clone();
                let memubot_config_arc = state.memubot_config.clone();
                let data_dir = state.data_dir.clone();
                let memu_client = state.memu_client.clone();
                let provider_service = state.provider_service.clone();
                let memory_graph_store = state.memory_graph_store.clone();
                let db = state.db.clone();
                let app_handle = app.handle().clone();
                let files_rail_service = state.files_rail_service.clone();

                // 在后台异步执行服务注册和启动
                tauri::async_runtime::spawn(async move {
                    // Stage 3: 注册后台服务
                    tracing::info!("[Stage 3] Registering background services...");
                    // Snapshot config at boot — services read their flags once at startup.
                    let memubot_config = memubot_config_arc.read().await.clone();

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
                                // 注入 MemoryGraphStore
                                mem_svc.set_graph_store(memory_graph_store.clone()).await;
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

                        // Scenario 2: Self-Improving Agent (Skill Extraction)
                        if memubot_config.scenarios.skill_extraction.enabled {
                            let scenario = Arc::new(
                                uclaw_core::proactive::scenarios::skill_extraction::SkillExtractionScenario::new(
                                    memubot_config.scenarios.skill_extraction.clone(),
                                )
                            );
                            scenario_manager.register(scenario);
                            tracing::info!("[Stage 3] SkillExtractionScenario registered");
                        }

                        // Scenario 2b: GEP Gene Evolution
                        if memubot_config.gene_evolution.enabled {
                            let scenario = Arc::new(
                                uclaw_core::proactive::scenarios::gene_evolution::GeneEvolutionScenario::new(
                                    memubot_config.gene_evolution.clone(),
                                )
                            );
                            scenario_manager.register(scenario);
                            tracing::info!("[Stage 3] GeneEvolutionScenario registered");
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

                        // Scenario 4: Plan-mode calibration (no LLM; runs DB-only calibration)
                        {
                            let scenario = Arc::new(
                                uclaw_core::proactive::scenarios::plan_mode_calibration::PlanModeCalibrationScenario::new(
                                    db.clone(),
                                )
                            );
                            scenario_manager.register(scenario);
                            tracing::info!("[Stage 3] PlanModeCalibrationScenario registered");
                        }

                        tracing::info!("[Stage 3] {} proactive scenarios registered", scenario_manager.scenario_count());

                        let pro_db_path = data_dir.join("proactive.db");
                        let gep_path = data_dir.join("gep");
                        let gene_repo = match uclaw_core::agent::gep::repository::GeneRepository::new(gep_path) {
                            Ok(repo) => Arc::new(std::sync::Mutex::new(repo)),
                            Err(e) => {
                                tracing::warn!("[Stage 3] GeneRepository init failed: {}, gene evolution disabled", e);
                                // Create a fallback with temp dir to avoid crash
                                Arc::new(std::sync::Mutex::new(
                                    uclaw_core::agent::gep::repository::GeneRepository::new(
                                        std::env::temp_dir().join("uclaw_gep_fallback")
                                    ).unwrap_or_else(|_| panic!("Cannot create GeneRepository fallback"))
                                ))
                            }
                        };
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
                                        db.clone(),
                                        gene_repo,
                                        memubot_config.gene_evolution.clone(),
                                        // Phase 3/4/5 runtime knobs bundled into
                                        // MemoryOsRuntimeConfig — replaces what used
                                        // to be five trailing positional args.
                                        // Future Memory OS phases just add fields
                                        // to the struct without touching this call
                                        // site.
                                        {
                                            let state_ref: tauri::State<'_, AppState> = app_handle.state();
                                            uclaw_core::proactive::MemoryOsRuntimeConfig::from_memubot_config(
                                                &memubot_config.memory_os,
                                                state_ref.lint_analyzer.clone(),
                                            )
                                        },
                                    )
                                );
                                // Inject into AppState for tauri_commands access
                                {
                                    let state_ref: tauri::State<'_, AppState> = app_handle.state();
                                    *state_ref.proactive_service.write().await = Some(pro_svc.clone());
                                }
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

                    // FilesRailService — always registered (not gated on memubot_config)
                    service_manager.register(files_rail_service).await;
                    tracing::info!("[Stage 3] FilesRailService registered");

                    // AppRuntimeService — already constructed in AppState::new(); we just
                    // register the shared Arc into ServiceManager here so it gets started.
                    {
                        let state_ref: tauri::State<'_, AppState> = app_handle.state();
                        let app_runtime_svc = state_ref.runtime_service.clone();
                        service_manager.register(app_runtime_svc).await;
                        tracing::info!("[Stage 3] AppRuntimeService registered");
                    }

                    // SymphonyService — third parallel runtime (DAG-of-agent-runs).
                    // Gated on memubot_config.symphony.enabled. Reuses the
                    // automation MemoryStore so Symphony per-workflow notes share
                    // the on-disk layout the rest of uClaw expects.
                    if memubot_config.symphony.enabled {
                        use std::path::PathBuf;
                        use std::sync::Arc;
                        let workspace_root: PathBuf = data_dir.join("symphony");
                        if let Err(e) = std::fs::create_dir_all(&workspace_root) {
                            tracing::warn!(
                                "[Stage 3] symphony workspace root mkdir failed: {} (continuing)",
                                e
                            );
                        }
                        let memory = Arc::new(
                            uclaw_core::automation::memory::MemoryStore::new(
                                data_dir.join("symphony-memory"),
                            ),
                        );
                        let symphony_svc = uclaw_core::symphony::runtime::service::SymphonyService::new(
                            db.clone(),
                            infra_service.clone(),
                            provider_service.clone(),
                            memubot_config.symphony.clone(),
                            Some(app_handle.clone()),
                            workspace_root,
                            memory,
                        );
                        service_manager.register(symphony_svc.clone()).await;
                        // Fill the AppState slot so Tauri commands can borrow it.
                        let state_ref: tauri::State<'_, AppState> = app_handle.state();
                        *state_ref.symphony_service.write().await = Some(symphony_svc);
                        tracing::info!("[Stage 3] SymphonyService registered");
                    } else {
                        tracing::info!("[Stage 3] SymphonyService disabled by config — skipping");
                    }

                    // Start ImChannelManager (load DB instances + start notify senders)
                    {
                        let state_ref: tauri::State<'_, AppState> = app_handle.state();
                        let im_mgr = state_ref.im_channel_manager.clone();
                        let im_reg = state_ref.im_session_registry.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = im_reg.load_from_db().await {
                                tracing::warn!("[Stage 3] ImSessionRegistry load_from_db failed: {}", e);
                            }
                            if let Err(e) = im_mgr.start_all().await {
                                tracing::warn!("[Stage 3] ImChannelManager start_all failed: {}", e);
                            }
                            tracing::info!("[Stage 3] ImChannelManager started");
                        });
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

                    // Stage 5: 启动间隔复习调度器 & 每日摘要服务
                    tracing::info!("[Stage 5] Starting review scheduler & daily summary...");

                    let review_scheduler = uclaw_core::proactive::review_scheduler::ReviewScheduler::new(
                        app_handle.clone(),
                        db.clone(),
                    );
                    review_scheduler.start();
                    tracing::info!("[Stage 5] ReviewScheduler started");

                    let daily_summary = uclaw_core::proactive::daily_summary::DailySummaryService::new(
                        app_handle.clone(),
                        db.clone(),
                        9, // 默认每天 9 点生成昨日摘要
                    );
                    daily_summary.start();
                    tracing::info!("[Stage 5] DailySummaryService started");
                });
            }

            // ─── 全局快捷键注册 ──────────────────────────────────────────
            {
                let voice_combo = if cfg!(target_os = "macos") {
                    "Super+Shift+M"
                } else {
                    "Control+Shift+M"
                };
                let clip_combo = if cfg!(target_os = "macos") {
                    "Super+Shift+C"
                } else {
                    "Control+Shift+C"
                };

                // Register voice memory shortcut
                let voice_shortcut = Shortcut::new(
                    Some(if cfg!(target_os = "macos") { Modifiers::SUPER | Modifiers::SHIFT } else { Modifiers::CONTROL | Modifiers::SHIFT }),
                    Code::KeyM,
                );
                let voice_handle = app.handle().clone();
                app.global_shortcut().on_shortcut(voice_shortcut, move |_app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        dispatch_global_shortcut_action(&voice_handle, "quick-memory-voice");
                    }
                })?;
                tracing::info!("[GlobalShortcut] {} registered for voice memory", voice_combo);

                // Register clipboard capture shortcut
                let clip_shortcut = Shortcut::new(
                    Some(if cfg!(target_os = "macos") { Modifiers::SUPER | Modifiers::SHIFT } else { Modifiers::CONTROL | Modifiers::SHIFT }),
                    Code::KeyC,
                );
                let clip_handle = app.handle().clone();
                app.global_shortcut().on_shortcut(clip_shortcut, move |_app, _shortcut, event| {
                    if event.state != ShortcutState::Pressed { return; }
                    dispatch_global_shortcut_action(&clip_handle, "clipboard-capture-silent");
                })?;
                tracing::info!("[GlobalShortcut] {} registered for clipboard capture", clip_combo);

                // 将初始绑定存入 GlobalShortcutRegistry State
                let mut initial_bindings = HashMap::new();
                initial_bindings.insert("quick-memory-voice".to_string(), voice_combo.to_string());
                initial_bindings.insert("clipboard-capture-silent".to_string(), clip_combo.to_string());
                app.manage(GlobalShortcutRegistry {
                    bindings: Mutex::new(initial_bindings),
                });
                tracing::info!("[GlobalShortcut] Registry state initialized");
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
            uclaw_core::tauri_commands::list_workspace_cost_rollup,
            uclaw_core::tauri_commands::get_month_cost_total,
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
            uclaw_core::tauri_commands::update_mcp_server,
            uclaw_core::tauri_commands::remove_mcp_server,
            uclaw_core::tauri_commands::toggle_mcp_server,
            uclaw_core::tauri_commands::connect_mcp_server,
            uclaw_core::tauri_commands::disconnect_mcp_server,
            uclaw_core::tauri_commands::restart_mcp_server,
            uclaw_core::tauri_commands::list_mcp_tools,
            // Skills
            uclaw_core::tauri_commands::list_skills,
            uclaw_core::tauri_commands::get_workspace_capabilities,
            uclaw_core::tauri_commands::toggle_skill,
            uclaw_core::tauri_commands::discover_skills,
            uclaw_core::tauri_commands::reload_skills,
            uclaw_core::tauri_commands::fork_skill_to_user,
            uclaw_core::tauri_commands::list_active_manifest_skills,
            uclaw_core::tauri_commands::get_skill_detail,
            uclaw_core::tauri_commands::match_skills,
            // Channels
            uclaw_core::tauri_commands::list_channels,
            uclaw_core::tauri_commands::add_channel,
            uclaw_core::tauri_commands::remove_channel,
            uclaw_core::tauri_commands::toggle_channel,
            // IM Channel Instance CRUD
            uclaw_core::tauri_commands::list_im_channels,
            uclaw_core::tauri_commands::get_im_channel_statuses,
            uclaw_core::tauri_commands::create_im_channel,
            uclaw_core::tauri_commands::update_im_channel,
            uclaw_core::tauri_commands::delete_im_channel,
            uclaw_core::tauri_commands::toggle_im_channel,
            uclaw_core::tauri_commands::request_wechat_ilink_qrcode,
            uclaw_core::tauri_commands::poll_wechat_ilink_qrcode_status,
            uclaw_core::tauri_commands::save_wechat_ilink_token,
            uclaw_core::tauri_commands::disconnect_wechat_ilink,
            uclaw_core::tauri_commands::list_spec_channel_bindings,
            uclaw_core::tauri_commands::update_spec_channel_bindings,
            uclaw_core::tauri_commands::update_spec_im_settings,
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
            // System Prompts
            uclaw_core::tauri_commands::get_system_prompt_config,
            uclaw_core::tauri_commands::create_system_prompt,
            uclaw_core::tauri_commands::delete_system_prompt,
            uclaw_core::tauri_commands::update_system_prompt,
            uclaw_core::tauri_commands::set_default_prompt,
            uclaw_core::tauri_commands::get_system_prompt_versions,
            uclaw_core::tauri_commands::update_append_setting,
            // Tool Approval
            uclaw_core::tauri_commands::approve_tool_call,
            uclaw_core::tauri_commands::respond_ask_user,
            uclaw_core::tauri_commands::respond_exit_plan_mode,
            uclaw_core::tauri_commands::respond_plan_mode_suggest,
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
            uclaw_core::tauri_commands::get_memory_recall_config,
            uclaw_core::tauri_commands::patch_memory_recall_config,
            uclaw_core::tauri_commands::memory_graph_get_full_graph,
            uclaw_core::tauri_commands::memory_graph_create_node,
            uclaw_core::tauri_commands::memory_graph_quick_capture,
            uclaw_core::tauri_commands::memory_graph_update_node,
            uclaw_core::tauri_commands::memory_graph_delete_node,
            // EntityPage (Memory OS Foundation Phase 1)
            uclaw_core::tauri_commands::memory_entity_page_create,
            uclaw_core::tauri_commands::memory_entity_page_get,
            uclaw_core::tauri_commands::memory_entity_page_find_by_slug,
            uclaw_core::tauri_commands::memory_entity_page_list,
            uclaw_core::tauri_commands::memory_entity_page_append_timeline,
            // Wiki artifacts (Memory OS Foundation Phase 3)
            uclaw_core::tauri_commands::memory_wiki_get_overview,
            uclaw_core::tauri_commands::memory_wiki_get_index,
            uclaw_core::tauri_commands::memory_wiki_regenerate,
            // Health findings (Memory OS Foundation Phase 4)
            uclaw_core::tauri_commands::memory_health_list_findings,
            uclaw_core::tauri_commands::memory_health_dismiss_finding,
            uclaw_core::tauri_commands::memory_health_run_now,
            // Lint scan (Memory OS Foundation Phase 5)
            uclaw_core::tauri_commands::memory_lint_run_now,
            // EntityPage synthesis (Memory OS Foundation Phase 6.2/6.3)
            uclaw_core::tauri_commands::memory_entity_page_synthesize_now,
            // Markdown export (Memory OS Foundation Phase 7.1)
            uclaw_core::tauri_commands::memory_wiki_export,
            // Markdown sync-from-disk (Memory OS Foundation Phase 7.2)
            uclaw_core::tauri_commands::memory_wiki_sync_from_disk,
            // Fragment / Daily Summary
            uclaw_core::tauri_commands::memory_graph_list_fragments,
            uclaw_core::tauri_commands::search_fragments,
            uclaw_core::tauri_commands::list_daily_summaries,
            // Learned Skills
            uclaw_core::tauri_commands::list_learned_skills,
            uclaw_core::tauri_commands::get_learned_skill,
            uclaw_core::tauri_commands::toggle_learned_skill,
            uclaw_core::tauri_commands::delete_learned_skill,
            uclaw_core::tauri_commands::update_learned_skill,
            uclaw_core::tauri_commands::record_skill_cited,
            uclaw_core::tauri_commands::set_skill_lifecycle,
            uclaw_core::tauri_commands::list_invocable_skills,
            uclaw_core::tauri_commands::get_skill_versions,
            // GEP Gene Evolution
            uclaw_core::tauri_commands::list_genes,
            uclaw_core::tauri_commands::get_gene_detail,
            uclaw_core::tauri_commands::get_gene_evolution_tree,
            uclaw_core::tauri_commands::retire_gene,
            uclaw_core::tauri_commands::reactivate_gene,
            // Symphony runtime (T14)
            uclaw_core::tauri_commands::symphony_list_workflows,
            uclaw_core::tauri_commands::symphony_get_workflow,
            uclaw_core::tauri_commands::symphony_save_workflow,
            uclaw_core::tauri_commands::symphony_delete_workflow,
            uclaw_core::tauri_commands::symphony_import_workflow_md,
            uclaw_core::tauri_commands::symphony_export_workflow_md,
            uclaw_core::tauri_commands::symphony_list_runs,
            uclaw_core::tauri_commands::symphony_get_run,
            uclaw_core::tauri_commands::symphony_trigger_run,
            uclaw_core::tauri_commands::symphony_cancel_run,
            uclaw_core::tauri_commands::symphony_get_node_session_id,
            uclaw_core::tauri_commands::symphony_get_service_health,
            uclaw_core::tauri_commands::backfill_skill_keywords,
            uclaw_core::tauri_commands::propose_skill_consolidation,
            uclaw_core::tauri_commands::cancel_skill_consolidation,
            uclaw_core::tauri_commands::apply_skill_consolidation,
            // MEMUBOT Services
            uclaw_core::tauri_commands::services_health,
            uclaw_core::tauri_commands::memorization_status,
            uclaw_core::tauri_commands::proactive_status,
            uclaw_core::tauri_commands::proactive_start,
            uclaw_core::tauri_commands::proactive_stop,
            uclaw_core::tauri_commands::metrics_summary,
            uclaw_core::tauri_commands::memubot_config_get,
            uclaw_core::tauri_commands::get_plan_mode_suggest_enabled,
            uclaw_core::tauri_commands::set_plan_mode_suggest_enabled,
            // Dev / Testing
            uclaw_core::tauri_commands::trigger_proactive_scenario,
            // Agent Session Control
            uclaw_core::tauri_commands::stop_agent_session,
            uclaw_core::tauri_commands::create_agent_session,
            uclaw_core::tauri_commands::delete_agent_session,
            uclaw_core::tauri_commands::toggle_pin_agent_session,
            uclaw_core::tauri_commands::toggle_archive_agent_session,
            uclaw_core::tauri_commands::toggle_archive_conversation,
            uclaw_core::tauri_commands::list_agent_sessions,
            uclaw_core::tauri_commands::estimate_session_context,
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
            uclaw_core::tauri_commands::list_automations,
            uclaw_core::tauri_commands::trigger_automation_manual,
            uclaw_core::tauri_commands::get_automation_activity,
            uclaw_core::tauri_commands::get_or_create_spec_home_thread,
            // Humane Automation Commands (Phase 1 spec § 7.3)
            uclaw_core::tauri_commands::install_humane_spec,
            uclaw_core::tauri_commands::import_humane_spec_file,
            uclaw_core::tauri_commands::get_automation_spec,
            uclaw_core::tauri_commands::update_user_config,
            uclaw_core::tauri_commands::set_automation_permission,
            uclaw_core::tauri_commands::set_automation_enabled,
            uclaw_core::tauri_commands::uninstall_automation,
            uclaw_core::tauri_commands::resolve_escalation,
            uclaw_core::tauri_commands::list_pending_escalations,
            uclaw_core::tauri_commands::read_automation_memory,
            uclaw_core::tauri_commands::compact_automation_memory,
            uclaw_core::tauri_commands::list_installed_marketplace_automations,
            uclaw_core::tauri_commands::list_marketplace_humans,
            uclaw_core::tauri_commands::install_marketplace_human,
            uclaw_core::tauri_commands::uninstall_marketplace_human,
            uclaw_core::tauri_commands::list_standalone_installs,
            uclaw_core::tauri_commands::query_marketplace,
            uclaw_core::tauri_commands::marketplace_category_counts,
            uclaw_core::tauri_commands::get_marketplace_detail,
            uclaw_core::tauri_commands::check_marketplace_updates,
            uclaw_core::tauri_commands::refresh_marketplace,
            // Files Rail Commands
            uclaw_core::files_rail::commands::files_rail_list_mounts,
            uclaw_core::files_rail::commands::files_rail_read_dir,
            uclaw_core::files_rail::commands::files_rail_watch_start,
            uclaw_core::files_rail::commands::files_rail_watch_stop,
            // Preview Commands
            uclaw_core::preview::commands::preview_read_bytes,
            uclaw_core::preview::commands::preview_resolve_chips,
            uclaw_core::preview::commands::preview_write_text,
            uclaw_core::preview::commands::approve_preview_write,
            // ─── Git Commands ───
            uclaw_core::tauri_commands_git::gh_available,
            uclaw_core::tauri_commands_git::gh_create_issue,
            uclaw_core::tauri_commands_git::gh_create_pr,
            uclaw_core::tauri_commands_git::git_branches,
            uclaw_core::tauri_commands_git::git_checkout_branch,
            uclaw_core::tauri_commands_git::git_commit,
            uclaw_core::tauri_commands_git::git_commit_push_pr,
            uclaw_core::tauri_commands_git::git_create_branch,
            uclaw_core::tauri_commands_git::git_current_branch,
            uclaw_core::tauri_commands_git::git_default_branch,
            uclaw_core::tauri_commands_git::git_diff,
            uclaw_core::tauri_commands_git::git_init_repo,
            uclaw_core::tauri_commands_git::git_is_repo,
            uclaw_core::tauri_commands_git::git_status,
            // Workspace Commands
            uclaw_core::tauri_commands::get_active_workspace_id,
            uclaw_core::tauri_commands::set_active_workspace_id,
            uclaw_core::tauri_commands::create_workspace,
            uclaw_core::tauri_commands::update_workspace,
            uclaw_core::tauri_commands::get_workspace_skill_tags,
            uclaw_core::tauri_commands::set_workspace_skill_tags,
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
            uclaw_core::tauri_commands::search_workspace_files_for_mention,
            uclaw_core::tauri_commands::upload_workspace_file,
            uclaw_core::tauri_commands::copy_file_into_workspace,
            uclaw_core::tauri_commands::list_always_allowed_paths,
            uclaw_core::tauri_commands::add_always_allowed_path,
            uclaw_core::tauri_commands::remove_always_allowed_path,
            uclaw_core::tauri_commands::list_session_allowed_paths,
            uclaw_core::tauri_commands::promote_session_path_to_global,
            uclaw_core::tauri_commands::path_is_directory,
            uclaw_core::tauri_commands::delete_workspace_file,
            uclaw_core::tauri_commands::read_workspace_uclaw_md,
            uclaw_core::tauri_commands::write_workspace_uclaw_md,
            uclaw_core::tauri_commands::read_default_prompts,
            uclaw_core::tauri_commands::open_workspace_uclaw_md_externally,
            uclaw_core::tauri_commands::reveal_path_in_file_manager,
            // Trajectory
            uclaw_core::tauri_commands::get_session_trajectory,
            uclaw_core::tauri_commands::search_trajectories,
            // Session Title
            uclaw_core::tauri_commands::generate_session_title,
            // Agent Teams
            uclaw_core::tauri_commands::start_agent_teams,
            uclaw_core::tauri_commands::get_team_channel,
            uclaw_core::tauri_commands::stop_agent_teams,
            // STT (SenseVoice ONNX, local)
            uclaw_core::stt::commands::stt_transcribe,
            uclaw_core::stt::commands::stt_model_status,
            uclaw_core::stt::commands::stt_download_model,
            uclaw_core::stt::commands::stt_get_settings,
            uclaw_core::stt::commands::stt_save_settings,
            uclaw_core::stt::commands::stt_list_microphones,
            // Global Shortcut
            update_global_shortcut,
        ]);

    // ─── Debug 菜单事件处理器（仅 debug 模式） ──────────────────────
    #[cfg(debug_assertions)]
    {
        builder = builder.on_menu_event(|app_handle, event| {
            use tauri::Emitter;
            tracing::info!("[Debug] Menu event received, id={}", event.id().as_ref());
            let state = app_handle.state::<AppState>();
            let data_dir = state.data_dir.clone();
            match event.id().as_ref() {
                "emit-memory-recall" => {
                    let _ = app_handle.emit("agent:memory-recall", serde_json::json!({
                        "totalCandidates": 8,
                        "skillsCount": 2,
                        "bootCount": 3,
                        "triggeredCount": 2,
                        "relevantCount": 2,
                        "expandedCount": 1,
                        "recentCount": 2,
                        "items": [
                            {"nodeId": "debug-1", "title": "Git 提交规范 (示例)", "kind": "procedure", "source": "boot"},
                            {"nodeId": "debug-2", "title": "React 组件拆分最佳实践 (示例)", "kind": "procedure", "source": "boot"},
                            {"nodeId": "debug-3", "title": "用户偏好：浅色主题 (示例)", "kind": "userProfile", "source": "trigger"},
                        ],
                        "conversationId": null,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }));
                    tracing::info!("[Debug] Emitted test memory-recall event");
                }
                "emit-proactive-learning" => {
                    let _ = app_handle.emit("agent:proactive-learning", serde_json::json!({
                        "scenario": "skill_extraction",
                        "items_extracted": 2,
                        "categories": ["procedure"],
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "summary": "[Debug] 测试技能提取 — 从最近对话中识别到 2 个可复用操作模式",
                        "sessionId": null,
                    }));
                    tracing::info!("[Debug] Emitted test proactive-learning event");
                }
                "emit-self-eval" => {
                    // 模拟 SelfEvalTool.execute() 的完整流程：
                    // 1. INSERT session_evals
                    // 2. 对每个 learning 分类并 publish SkillLearned（非 Noise 类）
                    // 3. Emit session:eval-complete 给前端
                    // 注意：publish_skill_learned 是 async，需要在 spawn 中执行
                    let session_id = format!("debug-self-eval-{}", uuid::Uuid::new_v4());
                    let score: f32 = 0.72;
                    let reasoning = "[Debug] 手动注入测试数据 — 模拟 Agent 完成任务后调用 self_eval";
                    let learnings: Vec<String> = vec![
                        "先检查文件是否存在再写入，避免覆盖用户已有数据".to_string(),
                        "工具调用失败时先重试一次，重试前检查网络连接".to_string(),
                        "大文件应分批读取，每批不超过10MB避免内存溢出".to_string(),
                        "路径拼接使用 PathBuf 而非字符串拼接，跨平台更安全".to_string(),
                        "错误信息应区分网络错误和权限错误，分别给出不同建议".to_string(),
                        "在完成多步任务后应主动总结已完成步骤供用户确认".to_string(),
                    ];
                    let learnings_json = serde_json::to_string(&learnings).unwrap_or_default();
                    let now_ms = chrono::Utc::now().timestamp_millis();

                    // 1. INSERT into session_evals（同步）
                    {
                        let db = state.db.lock().unwrap();
                        if let Err(e) = db.execute(
                            "INSERT INTO session_evals (id, session_id, score, reasoning, learnings, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                            rusqlite::params![uuid::Uuid::new_v4().to_string(), session_id, score, reasoning, learnings_json, now_ms],
                        ) {
                            tracing::error!("[Debug] self-eval DB insert failed: {}", e);
                        } else {
                            tracing::info!("[Debug] self-eval INSERT into session_evals OK (score={:.2}, {} learnings)", score, learnings.len());
                        }
                    }

                    // 2. Publish SkillLearned events via InfraService（异步：spawn）
                    let infra_clone = state.infra_service.clone();
                    let session_id_clone = session_id.clone();
                    let learnings_clone = learnings.clone();
                    tauri::async_runtime::spawn(async move {
                        for learning in &learnings_clone {
                            let card = uclaw_core::agent::tools::builtin::self_eval::classify_learning(
                                learning, score, &session_id_clone, None,
                            );
                            if card.card_type == uclaw_core::agent::gep::types::LearningCardType::Noise {
                                continue;
                            }
                            infra_clone.publish_skill_learned(
                                "self_eval",
                                learning,
                                serde_json::json!({
                                    "session_id": session_id_clone,
                                    "score": score,
                                    "source": "self_eval",
                                    "learning_card": {
                                        "card_type": card.card_type,
                                        "failure_signal": card.failure_signal,
                                        "tool_name": card.tool_name,
                                        "strategy_hint": {
                                            "condition": card.strategy_hint.condition,
                                            "action": card.strategy_hint.action,
                                            "reason": card.strategy_hint.reason,
                                        },
                                    },
                                }),
                            ).await;
                        }
                        tracing::info!("[Debug] Published {} SkillLearned events via InfraService", learnings_clone.len());
                    });

                    // 3. Emit session:eval-complete to frontend（同步）
                    let _ = app_handle.emit("session:eval-complete", serde_json::json!({
                        "sessionId": session_id,
                        "score": score,
                        "reasoning": reasoning,
                        "learnings": learnings,
                    }));
                    tracing::info!("[Debug] Emitted test self-eval data ({} learnings, score={:.2})", learnings.len(), score);
                }
                "generate-default-config" => {
                    let config_path = data_dir.join("memubot_config.json");
                    if config_path.exists() {
                        tracing::info!("[Debug] Config already exists at {:?}, skipping", config_path);
                    } else {
                        let config = uclaw_core::memubot_config::MemubotConfig::default();
                        match config.save(&data_dir) {
                            Ok(()) => tracing::info!("[Debug] Default config saved to {:?}", config_path),
                            Err(e) => tracing::error!("[Debug] Failed to save config: {:?}", e),
                        }
                    }
                }
                other => {
                    tracing::info!("[Debug] Unhandled menu event: {}", other);
                }
            }
        });
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running uClaw");
}

/// 根据 shortcut_id 分派全局快捷键操作
pub fn dispatch_global_shortcut_action(handle: &tauri::AppHandle, shortcut_id: &str) {
    match shortcut_id {
        "quick-memory-voice" => {
            // 显示主窗口 + 触发前端语音记忆录制
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            let _ = handle.emit("memory-voice-global-trigger", ());
            tracing::info!("[GlobalShortcut] memory-voice-global-trigger emitted");
        }
        "clipboard-capture-silent" => {
            let h = handle.clone();
            std::thread::spawn(move || {
                clipboard_capture_action(&h);
            });
        }
        _ => {
            tracing::warn!("[GlobalShortcut] Unknown shortcut_id: {}", shortcut_id);
        }
    }
}

/// 将前端快捷键格式转换为 Tauri 全局快捷键格式
/// 前端: "Cmd+Shift+C" -> Tauri: "Super+Shift+C" (macOS)
/// 前端: "Ctrl+Shift+C" -> Tauri: "Control+Shift+C" (Windows/Linux)
fn convert_frontend_to_tauri_shortcut(combo: &str) -> String {
    combo
        .replace("Cmd", "Super")
        .replace("Ctrl", "Control")
}

/// IPC 命令：动态更新全局快捷键绑定
#[tauri::command]
fn update_global_shortcut(
    app: tauri::AppHandle,
    registry: tauri::State<'_, GlobalShortcutRegistry>,
    shortcut_id: String,
    new_combo: String,
) -> Result<(), String> {
    let mut bindings = registry.bindings.lock().map_err(|e| e.to_string())?;

    // 1. 获取旧绑定并解除
    if let Some(old_combo) = bindings.get(&shortcut_id).cloned() {
        if let Err(e) = app.global_shortcut().unregister(old_combo.as_str()) {
            tracing::warn!("[GlobalShortcut] Failed to unregister '{}': {}", old_combo, e);
            // 继续执行，不要因为旧快捷键解除失败而中断
        }
    }

    // 2. 如果 new_combo 为空，只移除不重新注册
    if new_combo.is_empty() {
        bindings.remove(&shortcut_id);
        tracing::info!("[GlobalShortcut] Removed binding for '{}'", shortcut_id);
        return Ok(());
    }

    // 3. 转换前端格式到 Tauri 格式
    let tauri_combo = convert_frontend_to_tauri_shortcut(&new_combo);

    // 4. 注册新的全局快捷键
    let shortcut_id_clone = shortcut_id.clone();
    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(tauri_combo.as_str(), move |_app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            dispatch_global_shortcut_action(&handle, &shortcut_id_clone);
        })
        .map_err(|e| format!("Failed to register shortcut '{}': {}", tauri_combo, e))?;

    // 5. 更新 State
    tracing::info!(
        "[GlobalShortcut] Updated '{}': -> '{}'",
        shortcut_id,
        tauri_combo
    );
    bindings.insert(shortcut_id, tauri_combo);

    Ok(())
}

/// 执行剪贴板捕获动作：读取剪贴板 -> 保存到记忆 -> 音效 + 通知
fn clipboard_capture_action(handle: &tauri::AppHandle) {
    // 1. 读取剪贴板
    let text = match arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
        Ok(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => {
            play_system_sound(false);
            let _ = handle.notification()
                .builder()
                .title("记忆拾取")
                .body("剪贴板为空，无内容可保存")
                .show();
            return;
        }
    };

    // 2. 保存到记忆图谱
    let save_result = save_clipboard_to_memory(handle, &text);

    // 3. 音效 + OS 通知
    match save_result {
        Ok(_) => {
            play_system_sound(true);
            let preview: String = text.chars().take(30).collect();
            let body = if text.chars().count() > 30 {
                format!("{}...", preview)
            } else {
                preview
            };
            let _ = handle.notification()
                .builder()
                .title("记忆已保存")
                .body(&body)
                .show();
        }
        Err(e) => {
            play_system_sound(false);
            let _ = handle.notification()
                .builder()
                .title("记忆保存失败")
                .body(&e)
                .show();
        }
    }
}

/// 从 app state 获取 MemoryGraphStore 并调用 quick_capture_core
fn save_clipboard_to_memory(handle: &tauri::AppHandle, text: &str) -> Result<(), String> {
    let state = handle.state::<AppState>();
    let store = &state.memory_graph_store;
    uclaw_core::tauri_commands::quick_capture_core(store, text, "clipboard", None, None)?;
    Ok(())
}

fn play_system_sound(success: bool) {
    #[cfg(target_os = "macos")]
    {
        let sound = if success {
            "/System/Library/Sounds/Glass.aiff"
        } else {
            "/System/Library/Sounds/Basso.aiff"
        };
        let _ = std::process::Command::new("afplay").arg(sound).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let sound = if success { "Asterisk" } else { "Hand" };
        let _ = std::process::Command::new("powershell")
            .args(["-c", &format!("[System.Media.SystemSounds]::{}::Play()", sound)])
            .spawn();
    }
}
