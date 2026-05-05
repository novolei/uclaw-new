use tauri::State;
use crate::app::AppState;
use crate::error::Error;
use crate::ipc::*;
use crate::agent::types::*;
use crate::agent::tools::tool::ToolRegistry;
use crate::agent::tools::builtin;
use crate::llm;
use std::sync::Arc;
use tauri::Emitter;

// ─── Bootstrap Commands ────────────────────────────────────────────────

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<GetSettingsResponse, Error> {
    let settings = state.settings.read().await;
    Ok(GetSettingsResponse {
        language: settings.language.clone(),
        theme: settings.theme.clone(),
        config_path: state.config_path.to_string_lossy().into(),
        data_path: state.data_dir.to_string_lossy().into(),
    })
}

#[tauri::command]
pub async fn patch_settings(state: State<'_, AppState>, input: PatchSettingsInput) -> Result<GetSettingsResponse, Error> {
    let mut settings = state.settings.write().await;
    if let Some(lang) = input.language {
        settings.language = lang;
    }
    if let Some(theme) = input.theme {
        settings.theme = theme;
    }
    settings.save(&state.config_path)?;
    drop(settings);
    get_settings(state).await
}

#[tauri::command]
pub async fn get_platform() -> Result<PlatformInfo, Error> {
    Ok(PlatformInfo {
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        version: std::env::consts::OS.into(),
    })
}

#[tauri::command]
pub async fn get_version() -> Result<VersionInfo, Error> {
    Ok(VersionInfo {
        app_version: env!("CARGO_PKG_VERSION").into(),
        tauri_version: "2.0".into(),
        rust_version: "1.95.0".into(),
    })
}

#[tauri::command]
pub async fn get_bootstrap_status(state: State<'_, AppState>) -> Result<BootstrapStatus, Error> {
    let settings = state.settings.read().await;
    Ok(BootstrapStatus {
        initialized: true,
        db_ready: state.db_ready,
        config_ready: !settings.language.is_empty(),
    })
}

// ─── Chat Commands ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    input: SendMessageInput,
) -> Result<SendMessageResponse, Error> {
    // ── Resolve LLM config ──────────────────────────────────────────
    // Prefer the active model from the multi-provider system.
    // Fall back to the legacy LlmConfig if no active model is set.
    // Always read legacy config for max_tokens / temperature overrides.
    let legacy_config = state.llm_config.read().await;
    let max_tokens = legacy_config.max_tokens.unwrap_or(8192);
    let temperature = legacy_config.temperature.unwrap_or(0.7);

    let llm_config = if let Some((provider_id, model, api_key, base_url)) =
        state.provider_service.get_active_llm_config().await
    {
        llm::llm_config_from_provider(&provider_id, &model, &api_key, &base_url, max_tokens, temperature)
    } else {
        if legacy_config.api_key.is_empty() {
            return Err(Error::InvalidInput(
                "No API key configured. Please set up your AI provider in Settings.".into(),
            ));
        }
        legacy_config.clone()
    };

    if llm_config.api_key.is_empty() && llm_config.provider != "ollama" {
        return Err(Error::InvalidInput(
            "No API key configured. Please set up your AI provider in Settings.".into(),
        ));
    }

    // Setup tools
    let mut tools = ToolRegistry::new();
    let workspace = state.workspace_root.clone();
    tools.register(builtin::file::ReadFileTool::new(workspace.clone()));
    tools.register(builtin::file::WriteFileTool::new(workspace.clone()));
    tools.register(builtin::search::GrepTool::new(workspace.clone()));
    tools.register(builtin::search::GlobTool::new(workspace.clone()));
    tools.register(builtin::web::WebFetchTool::new());
    tools.register(builtin::web::HttpRequestTool::new());
    tools.register(builtin::edit::EditTool::new(workspace.clone()));
    tools.register(builtin::shell::BashTool::new(workspace.clone()));
    let tools = Arc::new(tools);

    // Create LLM provider
    let llm = llm::create_provider(&llm_config)?;

    // Add user message to session
    {
        let mut session_mgr = state.session_manager.write().await;
        session_mgr.add_message(&input.conversation_id, ChatMessage::user(&input.content));
    }

    // ── InfraService: publish incoming message event ────────────────
    state.infra_service.publish_incoming("local", &input.content, serde_json::json!({
        "conversation_id": input.conversation_id,
        "space_id": "default",
    })).await;

    // Build reasoning context
    let mut reason_ctx = ReasoningContext::new(get_system_prompt());
    {
        let session_mgr = state.session_manager.read().await;
        if let Some(session) = session_mgr.get(&input.conversation_id) {
            reason_ctx.messages = session.messages.clone();
            // Restore cumulative token counts from session
            reason_ctx.total_input_tokens = session.cumulative_input_tokens;
            reason_ctx.total_output_tokens = session.cumulative_output_tokens;
            tracing::info!(
                conversation_id = %input.conversation_id,
                restored_input_tokens = session.cumulative_input_tokens,
                restored_output_tokens = session.cumulative_output_tokens,
                "Restored cumulative token counts from session"
            );
        }
    }

    // Create delegate and run agent loop
    let safety_mode = input.safety_mode.as_deref()
        .map(|s| parse_safety_mode(s))
        .transpose()?;

    let mut delegate = crate::agent::dispatcher::ChatDelegate::new(
        llm,
        tools,
        app_handle.clone(),
        llm_config.model.clone(),
        get_system_prompt(),
        state.safety_manager.clone(),
        safety_mode,
        state.pending_approvals.clone(),
        input.conversation_id.clone(),
    );

    // Inject InfraService so dispatcher publishes ToolExecuted events
    delegate.set_infra_service(state.infra_service.clone());

    // ── Memory Recall Integration ────────────────────────────────────
    // Build a recall plan and inject memory context into the system prompt.
    {
        let recall_store = state.memory_graph_store.clone();
        let recall_memu = state.memu_client.clone();
        let recall_engine = crate::memory_graph::recall::MemoryRecallEngine::new(
            recall_store,
            recall_memu,
            crate::memory_graph::recall::MemoryRecallConfig::default(),
        );
        let space_id: String = {
            let session_mgr = state.session_manager.read().await;
            session_mgr.get_space_id(&input.conversation_id).unwrap_or_else(|| "default".to_string())
        };
        match recall_engine.build_recall_plan(&space_id, &input.content, false).await {
            Ok(plan) => {
                let total = plan.boot.len() + plan.triggered.len() + plan.relevant.len()
                    + plan.expanded.len() + plan.recent.len();
                if total > 0 {
                    let memory_ctx = crate::memory_graph::recall::MemoryRecallEngine::format_recall_for_prompt(&plan);
                    tracing::info!(total_candidates = total, "Memory recall injected into system prompt");
                    delegate.set_memory_context(memory_ctx);
                } else {
                    tracing::info!("Memory recall returned no candidates");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Memory recall failed, proceeding without memory context");
            }
        }
    }

    let config = AgenticLoopConfig::default();

    // Run the agent loop
    let outcome = crate::agent::agentic_loop::run_agentic_loop(&delegate, &mut reason_ctx, &config).await;

    let response_text = match &outcome {
        LoopOutcome::Response { text, .. } => text.clone(),
        LoopOutcome::ToolResult { results } => results.join("\n"),
        LoopOutcome::Stopped => "Conversation stopped.".into(),
        LoopOutcome::Cancelled => "Conversation cancelled.".into(),
        LoopOutcome::MaxIterations => "I've reached the maximum number of steps. Let me summarize what I've done so far.".into(),
        LoopOutcome::Failure { error } => format!("An error occurred: {}", error),
        LoopOutcome::NeedApproval { tool_name, tool_call_id, .. } => {
            // The approval event was already emitted by dispatcher.
            // Return a structured message so the frontend knows to wait.
            format!("Waiting for approval to run tool: {} ({})", tool_name, tool_call_id)
        }
    };

    // ── InfraService: publish loop completed/failed events ─────────
    {
        let loop_meta = serde_json::json!({
            "conversation_id": input.conversation_id,
            "total_input_tokens": reason_ctx.total_input_tokens,
            "total_output_tokens": reason_ctx.total_output_tokens,
        });
        match &outcome {
            LoopOutcome::Failure { error } => {
                state.infra_service.publish_loop_failed("local", error, loop_meta).await;
            }
            LoopOutcome::Response { .. }
            | LoopOutcome::ToolResult { .. }
            | LoopOutcome::MaxIterations => {
                state.infra_service.publish_loop_completed("local", &response_text, loop_meta).await;
            }
            _ => {} // Stopped / Cancelled / NeedApproval — no loop event
        }
    }

    // Save assistant response and cumulative token counts
    let message_id = uuid::Uuid::new_v4().to_string();
    {
        let mut session_mgr = state.session_manager.write().await;
        session_mgr.add_message(&input.conversation_id, ChatMessage::assistant(&response_text));
        // Persist cumulative token counts back to session
        if let Some(session) = session_mgr.get_mut(&input.conversation_id) {
            session.cumulative_input_tokens = reason_ctx.total_input_tokens;
            session.cumulative_output_tokens = reason_ctx.total_output_tokens;
            tracing::info!(
                conversation_id = %input.conversation_id,
                saved_input_tokens = reason_ctx.total_input_tokens,
                saved_output_tokens = reason_ctx.total_output_tokens,
                "Saved cumulative token counts to session"
            );
        }
    }

    // Emit completion
    let _ = app_handle.emit("agent:done", serde_json::json!({
        "text": response_text,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }));

    // ── InfraService: publish outgoing + processed events ──────────
    state.infra_service.publish_outgoing("local", &response_text, serde_json::json!({
        "conversation_id": input.conversation_id,
        "message_id": message_id,
    })).await;
    state.infra_service.publish_processed("local", serde_json::json!({
        "conversation_id": input.conversation_id,
    })).await;

    // ── Memory Reflection ─────────────────────────────────────────────
    // Spawn async reflection in background — non-blocking.
    {
        let reflection_msg_id = message_id.clone();
        let reflection_store = state.memory_graph_store.clone();
        let reflection_memu = state.memu_client.clone();
        let reflection_app_handle = app_handle.clone();
        let reflection_space_id = {
            let session_mgr = state.session_manager.read().await;
            session_mgr.get_space_id(&input.conversation_id).unwrap_or_else(|| "default".to_string())
        };
        let reflection_conv_id = input.conversation_id.clone();
        let reflection_user_input = input.content.clone();
        let reflection_assistant_output = response_text.clone();

        tokio::spawn(async move {
            let orchestrator = crate::memory_graph::reflection::ReflectionOrchestrator::new(
                reflection_store,
                reflection_memu,
                reflection_app_handle,
            );
            if let Err(e) = orchestrator.reflect(
                &reflection_space_id,
                &reflection_conv_id,
                &reflection_user_input,
                &reflection_assistant_output,
                &reflection_msg_id,
            ).await {
                tracing::error!(error = %e, "Background reflection failed");
            }
        });

        tracing::info!(
            assistant_message_id = %message_id,
            "Memory reflection spawned in background"
        );
    }
    Ok(SendMessageResponse {
        message_id,
        conversation_id: input.conversation_id,
        response: response_text,
    })
}

// ─── Conversation Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn create_conversation(
    state: State<'_, AppState>,
    input: CreateConversationInput,
) -> Result<ConversationResponse, Error> {
    let space_id = input.space_id.unwrap_or_else(|| "default".into());
    let title = input.title.unwrap_or_else(|| "New Chat".into());

    let summary = {
        let mut session_mgr = state.session_manager.write().await;
        session_mgr.create(&title, &space_id)
    };

    Ok(ConversationResponse {
        id: summary.id,
        space_id: summary.space_id,
        title: summary.title,
        message_count: summary.message_count,
        created_at: summary.created_at,
        updated_at: summary.updated_at,
    })
}

#[tauri::command]
pub async fn list_conversations(state: State<'_, AppState>) -> Result<Vec<ConversationResponse>, Error> {
    let session_mgr = state.session_manager.read().await;
    Ok(session_mgr.list().into_iter().map(|s| ConversationResponse {
        id: s.id,
        space_id: s.space_id,
        title: s.title,
        message_count: s.message_count,
        created_at: s.created_at,
        updated_at: s.updated_at,
    }).collect())
}

#[tauri::command]
pub async fn get_messages(state: State<'_, AppState>, input: GetMessagesInput) -> Result<Vec<MessageResponse>, Error> {
    let session_mgr = state.session_manager.read().await;
    if let Some(session) = session_mgr.get(&input.conversation_id) {
        Ok(session.messages.iter().enumerate().map(|(i, msg)| {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
            };
            let content = msg.content.iter()
                .filter_map(|b| if let ContentBlock::Text { text } = b { Some(text.clone()) } else { None })
                .collect::<Vec<_>>()
                .join("\n");
            MessageResponse {
                id: format!("msg-{}", i),
                conversation_id: input.conversation_id.clone(),
                role: role.into(),
                content,
                created_at: chrono::Utc::now().to_rfc3339(),
            }
        }).collect())
    } else {
        Ok(Vec::new())
    }
}

#[tauri::command]
pub async fn delete_conversation(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let mut session_mgr = state.session_manager.write().await;
    Ok(session_mgr.delete(&id))
}

#[tauri::command]
pub async fn toggle_star_conversation(
    state: State<'_, AppState>,
    input: ToggleStarInput,
) -> Result<ToggleStarResponse, Error> {
    let db = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

    let current: bool = db.query_row(
        "SELECT COALESCE(starred, 0) FROM conversations WHERE id = ?1",
        rusqlite::params![input.conversation_id],
        |row| row.get::<_, i32>(0),
    ).unwrap_or(0) != 0;

    let new_starred = !current;
    db.execute(
        "UPDATE conversations SET starred = ?1 WHERE id = ?2",
        rusqlite::params![new_starred as i32, input.conversation_id],
    ).map_err(Error::Database)?;

    Ok(ToggleStarResponse {
        conversation_id: input.conversation_id,
        starred: new_starred,
    })
}

// ─── Space Commands ────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_space(state: State<'_, AppState>, input: CreateSpaceInput) -> Result<SpaceResponse, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let icon = input.icon.unwrap_or_else(|| "📁".into());

    let db = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    db.execute(
        "INSERT INTO spaces (id, name, icon, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, input.name, icon, now, now],
    ).map_err(Error::Database)?;

    Ok(SpaceResponse {
        id,
        name: input.name,
        icon,
        created_at: now.clone(),
        updated_at: now,
    })
}

#[tauri::command]
pub async fn list_spaces(state: State<'_, AppState>) -> Result<Vec<SpaceResponse>, Error> {
    let db = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

    let mut stmt = db.prepare(
        "SELECT id, name, icon, created_at, updated_at FROM spaces ORDER BY created_at DESC",
    ).map_err(Error::Database)?;

    let spaces: Vec<SpaceResponse> = stmt.query_map([], |row| {
        Ok(SpaceResponse {
            id: row.get(0)?,
            name: row.get(1)?,
            icon: row.get::<_, String>(2).unwrap_or_else(|_| "📁".into()),
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
        })
    }).map_err(Error::Database)?
    .filter_map(|r| r.ok())
    .collect();

    if spaces.is_empty() {
        Ok(vec![SpaceResponse {
            id: "default".into(),
            name: "Default".into(),
            icon: "📁".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }])
    } else {
        Ok(spaces)
    }
}

#[tauri::command]
pub async fn delete_space(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let db = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let rows = db.execute(
        "DELETE FROM spaces WHERE id = ?1",
        rusqlite::params![id],
    ).map_err(Error::Database)?;
    Ok(rows > 0)
}

// ─── LLM Config Commands ───────────────────────────────────────────────

#[tauri::command]
pub async fn get_llm_config(state: State<'_, AppState>) -> Result<LlmConfigResponse, Error> {
    let config = state.llm_config.read().await;
    Ok(LlmConfigResponse {
        provider: config.provider.clone(),
        model: config.model.clone(),
        has_api_key: !config.api_key.is_empty(),
        base_url: config.base_url.clone(),
        max_tokens: config.max_tokens,
        temperature: config.temperature,
    })
}

#[tauri::command]
pub async fn update_llm_config(
    state: State<'_, AppState>,
    input: LlmConfigInput,
) -> Result<LlmConfigResponse, Error> {
    let mut config = state.llm_config.write().await;
    config.provider = input.provider;
    config.model = input.model;
    if !input.api_key.is_empty() {
        config.api_key = input.api_key;
    }
    config.base_url = input.base_url;
    config.max_tokens = input.max_tokens;
    config.temperature = input.temperature;

    config.save(&state.llm_config_path)?;

    Ok(LlmConfigResponse {
        provider: config.provider.clone(),
        model: config.model.clone(),
        has_api_key: !config.api_key.is_empty(),
        base_url: config.base_url.clone(),
        max_tokens: config.max_tokens,
        temperature: config.temperature,
    })
}

// ─── Artifact Commands ─────────────────────────────────────────────────

#[tauri::command]
pub async fn list_artifacts(state: State<'_, AppState>) -> Result<Vec<ArtifactNode>, Error> {
    let workspace = state.workspace_root.clone();
    build_artifact_tree(&workspace, &workspace).await
}

#[tauri::command]
pub async fn read_artifact(state: State<'_, AppState>, input: ReadArtifactInput) -> Result<ArtifactContentResponse, Error> {
    let workspace = state.workspace_root.clone();
    let full_path = workspace.join(&input.path);
    let content = tokio::fs::read_to_string(&full_path).await
        .map_err(|e| Error::Io(e))?;
    let size = content.len() as u64;
    Ok(ArtifactContentResponse { path: input.path, content, size })
}

#[tauri::command]
pub async fn write_artifact(state: State<'_, AppState>, input: WriteArtifactInput) -> Result<ArtifactContentResponse, Error> {
    let workspace = state.workspace_root.clone();
    let full_path = workspace.join(&input.path);
    if let Some(parent) = full_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| Error::Io(e))?;
    }
    tokio::fs::write(&full_path, &input.content).await.map_err(|e| Error::Io(e))?;
    let size = input.content.len() as u64;
    Ok(ArtifactContentResponse { path: input.path, content: input.content, size })
}

#[tauri::command]
pub async fn delete_artifact(state: State<'_, AppState>, path: String) -> Result<bool, Error> {
    let workspace = state.workspace_root.clone();
    let full_path = workspace.join(&path);
    tokio::fs::remove_file(&full_path).await.map_err(|e| Error::Io(e))?;
    Ok(true)
}

// ─── Enhanced Artifact Tree Commands ─────────────────────────────────────

#[tauri::command]
pub async fn list_artifacts_tree(
    state: State<'_, AppState>,
    input: ListArtifactTreeInput,
) -> Result<Vec<ArtifactTreeNodeResponse>, Error> {
    let space_dir = state.data_dir.join("spaces").join(&input.space_id).join("workspace");
    if !space_dir.exists() {
        tokio::fs::create_dir_all(&space_dir).await.map_err(Error::Io)?;
    }
    crate::workspace::list_artifact_tree(&space_dir, &input.path).await
}

#[tauri::command]
pub async fn load_artifact_children(
    state: State<'_, AppState>,
    input: LoadArtifactChildrenInput,
) -> Result<Vec<ArtifactTreeNodeResponse>, Error> {
    let space_dir = state.data_dir.join("spaces").join(&input.space_id).join("workspace");
    crate::workspace::load_artifact_children(&space_dir, &input.path).await
}

// ─── Extended Artifact Commands ─────────────────────────────────────────

#[tauri::command]
pub async fn create_artifact(
    state: State<'_, AppState>,
    input: CreateArtifactInput,
) -> Result<ArtifactTreeNodeResponse, Error> {
    let space_dir = state.data_dir.join("spaces").join(&input.space_id).join("workspace");
    let clean = input.path.trim_start_matches('/');
    let full_path = space_dir.join(clean);

    if input.is_dir.unwrap_or(false) {
        tokio::fs::create_dir_all(&full_path).await.map_err(Error::Io)?;
    } else {
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(Error::Io)?;
        }
        tokio::fs::write(&full_path, input.content.unwrap_or_default())
            .await
            .map_err(Error::Io)?;
    }

    let name = full_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let metadata = tokio::fs::metadata(&full_path).await.map_err(Error::Io)?;
    let parent_path = std::path::Path::new(clean).parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(ArtifactTreeNodeResponse {
        path: clean.to_string(),
        name,
        is_dir: metadata.is_dir(),
        parent_path,
        size_bytes: if metadata.is_dir() { None } else { Some(metadata.len()) },
        mime_type: if metadata.is_dir() { None } else { crate::workspace::mime_from_path(&full_path) },
        modified_at: metadata.modified().ok().map(|t| {
            chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
        }),
        children: if metadata.is_dir() { Some(vec![]) } else { None },
    })
}

#[tauri::command]
pub async fn rename_artifact(
    state: State<'_, AppState>,
    input: RenameArtifactInput,
) -> Result<bool, Error> {
    let space_dir = state.data_dir.join("spaces").join(&input.space_id).join("workspace");
    let old_path = space_dir.join(input.old_path.trim_start_matches('/'));
    let new_path = space_dir.join(input.new_path.trim_start_matches('/'));

    if !old_path.exists() {
        return Err(Error::NotFound(format!("File not found: {}", input.old_path)));
    }

    tokio::fs::rename(&old_path, &new_path).await.map_err(Error::Io)?;
    Ok(true)
}

#[tauri::command]
pub async fn move_artifact(
    state: State<'_, AppState>,
    input: MoveArtifactInput,
) -> Result<bool, Error> {
    let space_dir = state.data_dir.join("spaces").join(&input.space_id).join("workspace");
    let src = space_dir.join(input.src_path.trim_start_matches('/'));
    let dest = space_dir.join(input.dest_path.trim_start_matches('/'));

    if !src.exists() {
        return Err(Error::NotFound(format!("File not found: {}", input.src_path)));
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(Error::Io)?;
    }

    tokio::fs::rename(&src, &dest).await.map_err(Error::Io)?;
    Ok(true)
}

#[tauri::command]
pub async fn delete_artifact_recursive(
    state: State<'_, AppState>,
    space_id: String,
    path: String,
) -> Result<bool, Error> {
    let space_dir = state.data_dir.join("spaces").join(&space_id).join("workspace");
    let clean = path.trim_start_matches('/');
    let full_path = space_dir.join(clean);

    if !full_path.exists() {
        return Err(Error::NotFound(format!("File not found: {}", path)));
    }

    if full_path.is_dir() {
        tokio::fs::remove_dir_all(&full_path).await.map_err(Error::Io)?;
    } else {
        tokio::fs::remove_file(&full_path).await.map_err(Error::Io)?;
    }

    Ok(true)
}

#[tauri::command]
pub async fn detect_file_type(
    path: String,
) -> Result<DetectFileTypeResponse, Error> {
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let (mime_type, category) = match ext.as_str() {
        "ts" | "tsx" | "js" | "jsx" | "rs" | "py" | "go" | "java" | "c" | "cpp" | "h" | "css" | "scss" | "less" | "json" | "svelte" | "sql" | "sh" | "bash" | "zsh" | "yaml" | "yml" | "toml" | "xml" | "swift" | "kt" | "rb" | "php" | "r" | "dart" | "lua" => {
            (format!("text/{}", if ext == "rs" { "x-rust" } else if ext == "py" { "x-python" } else if ext == "go" { "x-go" } else if ext == "svelte" { "x-svelte" } else if ext == "sh" || ext == "bash" || ext == "zsh" { "x-shellscript" } else if ext == "sql" { "x-sql" } else if ext == "yaml" || ext == "yml" { "yaml" } else if ext == "toml" { "toml" } else { &ext }), "code")
        },
        "html" | "htm" => ("text/html".to_string(), "html"),
        "md" | "markdown" => ("text/markdown".to_string(), "markdown"),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico" => {
            (format!("image/{}", if ext == "jpg" { "jpeg" } else if ext == "svg" { "svg+xml" } else { &ext }), "image")
        },
        "txt" | "log" | "csv" => ("text/plain".to_string(), "text"),
        _ => ("application/octet-stream".to_string(), "binary"),
    };

    Ok(DetectFileTypeResponse { mime_type, category: category.to_string() })
}

// ─── Search Commands ───────────────────────────────────────────────────

#[tauri::command]
pub async fn search_workspace(state: State<'_, AppState>, input: SearchInput) -> Result<Vec<SearchResult>, Error> {
    let workspace = state.data_dir.join("workspace");
    let query = input.query.to_lowercase();
    let mut results = Vec::new();

    search_files(&workspace, &workspace, &query, &mut results).await?;
    results.truncate(20);
    Ok(results)
}

#[tauri::command]
pub async fn search_conversations(state: State<'_, AppState>, input: SearchInput) -> Result<Vec<SearchResult>, Error> {
    let mut results = Vec::new();
    let query = input.query.to_lowercase();

    let session_mgr = state.session_manager.read().await;
    for session in session_mgr.list() {
        if session.title.to_lowercase().contains(&query) {
            results.push(SearchResult {
                id: uuid::Uuid::new_v4().to_string(),
                title: session.title.clone(),
                snippet: format!("{} messages", session.message_count),
                source: "conversation".into(),
                source_id: session.id.clone(),
                created_at: session.updated_at.clone(),
            });
        }
    }
    results.truncate(20);
    Ok(results)
}

#[tauri::command]
pub async fn search_all(state: State<'_, AppState>, input: SearchInput) -> Result<Vec<SearchResult>, Error> {
    let mut results = Vec::new();

    // Search conversations
    let conv_results = search_conversations_inner(&state, &input.query).await?;
    results.extend(conv_results);

    // Search workspace files
    let workspace = state.data_dir.join("workspace");
    search_files(&workspace, &workspace, &input.query.to_lowercase(), &mut results).await?;

    results.truncate(30);
    Ok(results)
}

async fn search_conversations_inner(state: &State<'_, AppState>, query: &str) -> Result<Vec<SearchResult>, Error> {
    let mut results = Vec::new();
    let q = query.to_lowercase();
    let session_mgr = state.session_manager.read().await;
    for session in session_mgr.list() {
        if session.title.to_lowercase().contains(&q) {
            results.push(SearchResult {
                id: uuid::Uuid::new_v4().to_string(),
                title: session.title.clone(),
                snippet: format!("{} messages", session.message_count),
                source: "conversation".into(),
                source_id: session.id.clone(),
                created_at: session.updated_at.clone(),
            });
        }
    }
    Ok(results)
}

async fn search_files(root: &std::path::Path, base: &std::path::Path, query: &str, results: &mut Vec<SearchResult>) -> Result<(), Error> {
    let mut entries = tokio::fs::read_dir(root).await.map_err(|e| Error::Io(e))?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| Error::Io(e))? {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "node_modules" || name == "target" { continue; }

        if path.is_dir() {
            Box::pin(search_files(&path, base, query, results)).await?;
        } else {
            let relative = path.strip_prefix(base).unwrap_or(&path);
            if relative.to_string_lossy().to_lowercase().contains(query) {
                let size = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
                results.push(SearchResult {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: relative.to_string_lossy().into(),
                    snippet: format!("{} bytes", size),
                    source: "file".into(),
                    source_id: relative.to_string_lossy().into(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                });
            }
            if results.len() >= 30 { return Ok(()); }
        }
    }
    Ok(())
}

// ─── Helpers ───────────────────────────────────────────────────────────

fn get_system_prompt() -> String {
    r#"You are uClaw, a helpful AI assistant powered by Claude. You have access to tools that let you interact with the user's computer.

## Available Tools
You can:
- **read_file**: Read any file on the user's system
- **write_file**: Write or create files
- **grep**: Search for patterns in files
- **glob**: Find files matching patterns
- **web_fetch**: Fetch content from URLs

## Guidelines
1. Always use tools when you need to access files or search for information
2. If a tool fails, explain the error and try an alternative approach
3. Be concise but thorough in your responses
4. If you're unsure about something, ask before taking action
5. Always explain what you're doing before using tools that modify files

## Response Style
- Use Markdown for formatting
- Show code snippets with language hints
- Be friendly and professional"#.to_string()
}

async fn build_artifact_tree(root: &std::path::PathBuf, base: &std::path::PathBuf) -> Result<Vec<ArtifactNode>, Error> {
    let mut nodes = Vec::new();
    let mut entries = tokio::fs::read_dir(root).await.map_err(|e| Error::Io(e))?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| Error::Io(e))? {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
        let relative = path.strip_prefix(base).unwrap_or(&path);

        if name.starts_with('.') || name == "node_modules" || name == "target" { continue; }

        if path.is_dir() {
            let children = Box::pin(build_artifact_tree(&path, base)).await?;
            nodes.push(ArtifactNode {
                name: name.into(),
                path: relative.to_string_lossy().into(),
                is_dir: true,
                size: None,
                children: if children.is_empty() { None } else { Some(children) },
            });
        } else {
            let size = entry.metadata().await.map(|m| m.len()).ok();
            nodes.push(ArtifactNode {
                name: name.into(),
                path: relative.to_string_lossy().into(),
                is_dir: false,
                size,
                children: None,
            });
        }
    }
    nodes.sort_by(|a, b| {
        if a.is_dir != b.is_dir { b.is_dir.cmp(&a.is_dir) }
        else { a.name.to_lowercase().cmp(&b.name.to_lowercase()) }
    });
    Ok(nodes)
}

// ─── Notification Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn get_notifications(state: State<'_, AppState>) -> Result<Vec<NotificationItem>, Error> {
    let mgr = state.notifications.lock().await;
    Ok(mgr.history().into_iter().map(|n| NotificationItem {
        id: n.id,
        title: n.title,
        message: n.message,
        level: match n.level {
            crate::notifications::NotificationLevel::Info => "info".into(),
            crate::notifications::NotificationLevel::Success => "success".into(),
            crate::notifications::NotificationLevel::Warning => "warning".into(),
            crate::notifications::NotificationLevel::Error => "error".into(),
        },
        source: n.source,
        timestamp: n.timestamp,
    }).collect())
}

#[tauri::command]
pub async fn clear_notifications(state: State<'_, AppState>) -> Result<bool, Error> {
    let mut mgr = state.notifications.lock().await;
    mgr.clear();
    Ok(true)
}

// ─── Background Task Commands ──────────────────────────────────────────

#[tauri::command]
pub async fn get_background_tasks(state: State<'_, AppState>) -> Result<Vec<crate::background::BackgroundTask>, Error> {
    let mgr = state.background_tasks.lock().await;
    Ok(mgr.list().into_iter().cloned().collect())
}

// ─── Memory Commands ───────────────────────────────────────────────────

fn entry_to_response(e: crate::memory::MemoryEntry) -> MemoryEntryResponse {
    MemoryEntryResponse {
        id: e.id,
        key: e.key,
        value: e.value,
        kind: e.kind,
        namespace: e.namespace,
        space_id: e.space_id,
        tags: e.tags,
        metadata: e.metadata,
        created_at: e.created_at,
        updated_at: e.updated_at,
        expires_at: e.expires_at,
    }
}

#[tauri::command]
pub async fn memory_set(state: State<'_, AppState>, input: MemorySetInput) -> Result<MemoryEntryResponse, Error> {
    use crate::memory::{MemoryKind, SetMemoryOpts};
    let kind = MemoryKind::from_str(input.kind.as_deref().unwrap_or("note"));
    let entry = state.memory_store.set_full(SetMemoryOpts {
        space_id: input.space_id.unwrap_or_else(|| "global".into()),
        namespace: input.namespace.unwrap_or_else(|| "default".into()),
        key: input.key,
        value: input.value,
        kind,
        tags: input.tags.unwrap_or_default(),
        metadata: input.metadata,
        ttl_seconds: input.ttl_seconds,
    })?;
    Ok(entry_to_response(entry))
}

#[tauri::command]
pub async fn memory_get(state: State<'_, AppState>, input: MemoryGetInput) -> Result<Option<MemoryEntryResponse>, Error> {
    let namespace = input.namespace.unwrap_or_else(|| "default".into());
    let space_id = input.space_id.unwrap_or_else(|| "global".into());
    Ok(state.memory_store.get_full(&input.key, &namespace, &space_id).map(entry_to_response))
}

#[tauri::command]
pub async fn memory_delete(state: State<'_, AppState>, input: MemoryGetInput) -> Result<bool, Error> {
    let namespace = input.namespace.unwrap_or_else(|| "default".into());
    let space_id = input.space_id.unwrap_or_else(|| "global".into());
    Ok(state.memory_store.delete_full(&input.key, &namespace, &space_id))
}

#[tauri::command]
pub async fn memory_search(state: State<'_, AppState>, input: MemorySearchInput) -> Result<Vec<MemoryEntryResponse>, Error> {
    let limit = input.limit.unwrap_or(20);
    let results = state.memory_store.search_full(
        &input.query,
        input.namespace.as_deref(),
        input.space_id.as_deref(),
        input.kind.as_deref(),
        limit,
    );
    Ok(results.into_iter().map(entry_to_response).collect())
}

#[tauri::command]
pub async fn memory_list(state: State<'_, AppState>, input: MemoryListInput) -> Result<Vec<MemoryEntryResponse>, Error> {
    use crate::memory::ListFilter;
    let filter = ListFilter {
        space_id: input.space_id,
        namespace: input.namespace,
        kind: input.kind,
        tag: input.tag,
        limit: input.limit,
        offset: input.offset,
    };
    let results = state.memory_store.list_filtered(&filter);
    Ok(results.into_iter().map(entry_to_response).collect())
}

#[tauri::command]
pub async fn memory_clear_namespace(state: State<'_, AppState>, input: MemoryClearInput) -> Result<MemoryClearResponse, Error> {
    let deleted = state.memory_store.clear_namespace(&input.namespace, input.space_id.as_deref());
    Ok(MemoryClearResponse { deleted })
}

#[tauri::command]
pub async fn memory_prune_expired(state: State<'_, AppState>) -> Result<MemoryClearResponse, Error> {
    let deleted = state.memory_store.prune_expired();
    Ok(MemoryClearResponse { deleted })
}

#[tauri::command]
pub async fn memory_bulk_import(state: State<'_, AppState>, input: MemoryBulkImportInput) -> Result<MemoryBulkImportResponse, Error> {
    use crate::memory::{MemoryKind, SetMemoryOpts};
    let entries: Vec<SetMemoryOpts> = input.entries.into_iter().map(|e| {
        SetMemoryOpts {
            space_id: e.space_id.unwrap_or_else(|| "global".into()),
            namespace: e.namespace.unwrap_or_else(|| "default".into()),
            key: e.key,
            value: e.value,
            kind: MemoryKind::from_str(e.kind.as_deref().unwrap_or("note")),
            tags: e.tags.unwrap_or_default(),
            metadata: e.metadata,
            ttl_seconds: e.ttl_seconds,
        }
    }).collect();
    let result = state.memory_store.bulk_import(entries);
    Ok(MemoryBulkImportResponse {
        imported: result.imported,
        skipped: result.skipped,
        errors: result.errors,
    })
}

#[tauri::command]
pub async fn memory_export(state: State<'_, AppState>, input: MemoryListInput) -> Result<Vec<MemoryEntryResponse>, Error> {
    use crate::memory::ListFilter;
    let filter = ListFilter {
        space_id: input.space_id,
        namespace: input.namespace,
        kind: input.kind,
        tag: input.tag,
        limit: input.limit,
        offset: input.offset,
    };
    let results = state.memory_store.export(&filter);
    Ok(results.into_iter().map(entry_to_response).collect())
}

#[tauri::command]
pub async fn memory_list_namespaces(state: State<'_, AppState>, space_id: Option<String>) -> Result<Vec<String>, Error> {
    Ok(state.memory_store.list_namespaces(space_id.as_deref()))
}

// ─── MCP Commands ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServerInfo>, Error> {
    let mgr = state.mcp_manager.read().await;
    let statuses: std::collections::HashMap<String, (crate::mcp::McpServerStatus, Option<String>)> = mgr
        .all_server_statuses()
        .into_iter()
        .map(|(id, st, err)| (id, (st, err)))
        .collect();
    Ok(mgr.all_servers().into_iter().map(|c| {
        let (status_enum, _err) = statuses.get(&c.id)
            .cloned()
            .unwrap_or((crate::mcp::McpServerStatus::Disconnected, None));
        let status = match status_enum {
            crate::mcp::McpServerStatus::Disconnected => "disconnected",
            crate::mcp::McpServerStatus::Connecting => "connecting",
            crate::mcp::McpServerStatus::Connected => "connected",
            crate::mcp::McpServerStatus::Error => "error",
        };
        McpServerInfo {
            id: c.id.clone(),
            name: c.name.clone(),
            description: c.description.clone(),
            command: c.command.clone(),
            args: c.args.clone(),
            env: Some(c.env.clone()),
            enabled: c.enabled,
            auto_approve: c.auto_approve,
            status: status.into(),
        }
    }).collect())
}

#[tauri::command]
pub async fn add_mcp_server(state: State<'_, AppState>, input: McpServerInput) -> Result<McpServerInfo, Error> {
    let config = crate::mcp::McpServerConfig {
        id: input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: input.name.clone(),
        description: input.description,
        transport_type: crate::mcp::TransportType::Stdio,
        command: input.command,
        args: input.args.unwrap_or_default(),
        env: input.env.unwrap_or_default(),
        url: None,
        enabled: true,
        auto_approve: false,
    };
    let mut mgr = state.mcp_manager.write().await;
    mgr.add_server(config.clone()).map_err(|e| Error::InvalidInput(e))?;
    Ok(McpServerInfo {
        id: config.id,
        name: config.name,
        description: config.description,
        command: config.command,
        args: config.args,
        env: Some(config.env),
        enabled: config.enabled,
        auto_approve: config.auto_approve,
        status: "disconnected".into(),
    })
}

#[tauri::command]
pub async fn remove_mcp_server(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let mut mgr = state.mcp_manager.write().await;
    Ok(mgr.remove_server(&id).is_some())
}

#[tauri::command]
pub async fn toggle_mcp_server(state: State<'_, AppState>, id: String, enabled: bool) -> Result<bool, Error> {
    let mut mgr = state.mcp_manager.write().await;
    Ok(mgr.set_enabled(&id, enabled))
}

#[tauri::command]
pub async fn connect_mcp_server(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let mut mgr = state.mcp_manager.write().await;
    mgr.connect_server(&id).await.map_err(|e| Error::Internal(e.to_string()))?;
    Ok(true)
}

#[tauri::command]
pub async fn disconnect_mcp_server(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let mut mgr = state.mcp_manager.write().await;
    mgr.disconnect_server(&id).await.map_err(|e| Error::Internal(e.to_string()))?;
    Ok(true)
}

#[tauri::command]
pub async fn restart_mcp_server(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let mut mgr = state.mcp_manager.write().await;
    mgr.restart_server(&id).await.map_err(|e| Error::Internal(e.to_string()))?;
    Ok(true)
}

#[tauri::command]
pub async fn list_mcp_tools(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, Error> {
    let mgr = state.mcp_manager.read().await;
    Ok(mgr.all_tools().into_iter().map(|t| serde_json::json!({
        "serverId": t.server_id,
        "name": t.name,
        "description": t.description,
        "parameters": t.parameters,
    })).collect())
}

// ─── Skills Commands ───────────────────────────────────────────────────

#[tauri::command]
pub async fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, Error> {
    let registry = state.skills_registry.read().await;
    Ok(registry.list().into_iter().map(|s| SkillInfo {
        name: s.name.clone(),
        version: s.version.clone(),
        description: s.description.clone(),
        author: s.author.clone(),
        enabled: registry.is_enabled(&s.name),
        category: s.category.clone(),
    }).collect())
}

#[tauri::command]
pub async fn toggle_skill(state: State<'_, AppState>, input: SkillToggleInput) -> Result<bool, Error> {
    let mut registry = state.skills_registry.write().await;
    if input.enabled {
        Ok(registry.enable(&input.name))
    } else {
        Ok(registry.disable(&input.name))
    }
}

#[tauri::command]
pub async fn discover_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, Error> {
    let mut registry = state.skills_registry.write().await;
    let _names = registry.discover();
    Ok(registry.list().into_iter().map(|s| SkillInfo {
        name: s.name.clone(),
        version: s.version.clone(),
        description: s.description.clone(),
        author: s.author.clone(),
        enabled: registry.is_enabled(&s.name),
        category: s.category.clone(),
    }).collect())
}

#[tauri::command]
pub async fn reload_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, Error> {
    let mut registry = state.skills_registry.write().await;
    let _names = registry.reload();
    Ok(registry.list().into_iter().map(|s| SkillInfo {
        name: s.name.clone(),
        version: s.version.clone(),
        description: s.description.clone(),
        author: s.author.clone(),
        enabled: registry.is_enabled(&s.name),
        category: s.category.clone(),
    }).collect())
}

#[tauri::command]
pub async fn get_skill_detail(state: State<'_, AppState>, name: String) -> Result<SkillDetailResponse, Error> {
    let registry = state.skills_registry.read().await;
    let loaded = registry.get_loaded(&name)
        .ok_or_else(|| Error::NotFound(format!("Skill '{}' not found", name)))?;
    Ok(SkillDetailResponse {
        name: loaded.manifest.name.clone(),
        version: loaded.manifest.version.clone(),
        description: loaded.manifest.description.clone(),
        author: loaded.manifest.author.clone(),
        enabled: registry.is_enabled(&loaded.manifest.name),
        category: loaded.manifest.category.clone(),
        keywords: loaded.manifest.activation.keywords.clone(),
        tags: loaded.manifest.activation.tags.clone(),
        patterns: loaded.manifest.activation.patterns.clone(),
        parameters: loaded.manifest.parameters.iter().map(|p| SkillParamInfo {
            name: p.name.clone(),
            param_type: p.r#type.clone(),
            required: p.required,
            description: p.description.clone(),
            default: p.default.clone(),
        }).collect(),
        prompt_length: loaded.prompt_content.len(),
        path: loaded.manifest.path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn match_skills(state: State<'_, AppState>, input: SkillMatchInput) -> Result<Vec<SkillMatchResult>, Error> {
    let registry = state.skills_registry.read().await;
    let matched = registry.match_skills(&input.message);
    Ok(matched.into_iter().map(|s| {
        let score = crate::skills::score_skill(s, &input.message);
        let preview = if s.prompt_content.len() > 200 {
            format!("{}...", &s.prompt_content[..200])
        } else {
            s.prompt_content.clone()
        };
        SkillMatchResult {
            name: s.manifest.name.clone(),
            score,
            prompt_preview: preview,
        }
    }).collect())
}

// ─── Channel Commands ──────────────────────────────────────────────────

#[tauri::command]
pub async fn list_channels(state: State<'_, AppState>) -> Result<Vec<ChannelInfo>, Error> {
    let mgr = state.channel_manager.read().await;
    Ok(mgr.list().into_iter().map(|c| ChannelInfo {
        id: c.id.clone(),
        name: c.name.clone(),
        channel_type: match c.channel_type {
            crate::channels::ChannelType::Webhook => "webhook",
            crate::channels::ChannelType::Email => "email",
            crate::channels::ChannelType::WeChat => "wechat",
            crate::channels::ChannelType::DingTalk => "dingtalk",
            crate::channels::ChannelType::Feishu => "feishu",
            crate::channels::ChannelType::Custom => "custom",
        }.into(),
        enabled: c.enabled,
        webhook_url: c.webhook_url.clone(),
    }).collect())
}

#[tauri::command]
pub async fn add_channel(state: State<'_, AppState>, input: ChannelInput) -> Result<ChannelInfo, Error> {
    let channel_type = match input.channel_type.as_str() {
        "webhook" => crate::channels::ChannelType::Webhook,
        "email" => crate::channels::ChannelType::Email,
        "wechat" => crate::channels::ChannelType::WeChat,
        "dingtalk" => crate::channels::ChannelType::DingTalk,
        "feishu" => crate::channels::ChannelType::Feishu,
        _ => crate::channels::ChannelType::Custom,
    };
    let config = crate::channels::ChannelConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: input.name.clone(),
        channel_type: channel_type.clone(),
        enabled: true,
        webhook_url: input.webhook_url.clone(),
        config: input.config.clone(),
    };
    let id = config.id.clone();
    let mut mgr = state.channel_manager.write().await;
    mgr.add_channel(config);
    Ok(ChannelInfo {
        id,
        name: input.name,
        channel_type: input.channel_type,
        enabled: true,
        webhook_url: input.webhook_url,
    })
}

#[tauri::command]
pub async fn remove_channel(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let mut mgr = state.channel_manager.write().await;
    Ok(mgr.remove_channel(&id).is_some())
}

#[tauri::command]
pub async fn toggle_channel(state: State<'_, AppState>, id: String, enabled: bool) -> Result<bool, Error> {
    let mut mgr = state.channel_manager.write().await;
    Ok(mgr.set_enabled(&id, enabled))
}

// ─── Provider Commands ──────────────────────────────────────────────────

/// List all built-in providers.
#[tauri::command]
pub fn list_providers() -> Vec<ProviderInfo> {
    crate::providers::registry::all()
        .iter()
        .map(|p| ProviderInfo {
            id: p.id.to_string(),
            display_name: p.display_name.to_string(),
            auth_type: format!("{:?}", p.auth_type).to_lowercase(),
            default_base_url: p.default_base_url.to_string(),
            default_api: format!("{:?}", p.default_api),
            service_category: format!("{:?}", p.service_category),
            geo_category: format!("{:?}", p.geo_category),
            supports_models: p.supports_models,
        })
        .collect()
}

/// List all configured provider IDs.
#[tauri::command]
pub async fn list_configured_providers(state: State<'_, AppState>) -> Result<Vec<String>, Error> {
    Ok(state.provider_service.list_configured_ids().await)
}

/// Get saved provider config.
#[tauri::command]
pub async fn get_provider_config(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<Option<ProviderConfigResponse>, Error> {
    let config = state.provider_service.get_provider_config(&provider_id).await;
    Ok(config.map(|c| ProviderConfigResponse {
        provider_id: c.provider_id,
        display_name: c.display_name,
        has_api_key: c.api_key.is_some_and(|k| !k.is_empty()),
        base_url: c.base_url,
        api: c.api.map(|a| format!("{:?}", a)),
    }))
}

/// Save a provider configuration.
#[tauri::command]
pub async fn configure_provider(
    state: State<'_, AppState>,
    input: ProviderConfigInput,
) -> Result<(), Error> {
    let config = crate::providers::types::ProviderConfig {
        provider_id: input.provider_id,
        display_name: input.display_name,
        api_key: input.api_key.filter(|k| !k.is_empty()),
        base_url: input.base_url.filter(|u| !u.is_empty()),
        api: input.api.and_then(|a| parse_api_type(&a)),
    };
    state.provider_service.configure_provider(config).await
}

/// Save a provider configuration with model selections.
#[tauri::command]
pub async fn configure_provider_with_models(
    state: State<'_, AppState>,
    provider_config: ProviderConfigInput,
    model_ids: Vec<String>,
) -> Result<(), Error> {
    let config = crate::providers::types::ProviderConfig {
        provider_id: provider_config.provider_id,
        display_name: provider_config.display_name,
        api_key: provider_config.api_key.filter(|k| !k.is_empty()),
        base_url: provider_config.base_url.filter(|u| !u.is_empty()),
        api: provider_config.api.and_then(|a| parse_api_type(&a)),
    };
    state
        .provider_service
        .configure_provider_with_models(config, &model_ids)
        .await
}

/// Remove a provider configuration.
#[tauri::command]
pub async fn remove_provider_config(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<(), Error> {
    state.provider_service.remove_provider(&provider_id).await
}

/// Test provider connection.
#[tauri::command]
pub async fn test_provider_connection(
    state: State<'_, AppState>,
    input: TestConnectionInput,
) -> Result<TestResultInfo, Error> {
    let result = state
        .provider_service
        .test_connection(
            &input.provider_id,
            &input.base_url,
            input.api_key.as_deref(),
        )
        .await;
    Ok(TestResultInfo {
        success: result.success,
        message: result.message,
        latency_ms: result.latency_ms,
        details: result.details,
    })
}

/// List available models from a provider.
#[tauri::command]
pub async fn list_provider_models(
    state: State<'_, AppState>,
    input: ListModelsInput,
) -> Result<Vec<ModelInfo>, Error> {
    let models = state
        .provider_service
        .list_models(&input.provider_id, &input.base_url, input.api_key.as_deref())
        .await
        .map_err(|e| Error::Internal(format!("Failed to list models: {e}")))?;

    Ok(models
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id,
            name: m.name,
            context_window: m.context_window,
            max_tokens: m.max_tokens,
            modality: format!("{:?}", m.modality),
            reasoning: m.reasoning,
            supports_reasoning_effort: m.supports_reasoning_effort,
        })
        .collect())
}

/// Get configured models for a specific provider.
#[tauri::command]
pub async fn get_configured_models(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<Vec<String>, Error> {
    Ok(state.provider_service.get_configured_models(&provider_id).await)
}

/// Get all configured models grouped by provider.
#[tauri::command]
pub async fn get_all_configured_models(
    state: State<'_, AppState>,
) -> Result<Vec<(String, Vec<String>)>, Error> {
    Ok(state.provider_service.get_all_configured_models().await)
}

/// Get the current active model.
#[tauri::command]
pub async fn get_active_model(
    state: State<'_, AppState>,
) -> Result<Option<ModelSelectionInfo>, Error> {
    let selection = state.provider_service.get_active_model().await;
    Ok(selection.map(|s| ModelSelectionInfo {
        provider_id: s.provider_id,
        model_id: s.model_id,
    }))
}

/// Set the active model.
#[tauri::command]
pub async fn set_active_model(
    state: State<'_, AppState>,
    provider_id: String,
    model_id: String,
) -> Result<(), Error> {
    state
        .provider_service
        .select_model(&provider_id, &model_id)
        .await
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn parse_api_type(s: &str) -> Option<crate::providers::types::ApiType> {
    match s {
        "OpenAiCompletions" | "openai_completions" | "openai-completions" => {
            Some(crate::providers::types::ApiType::OpenAiCompletions)
        }
        "AnthropicMessages" | "anthropic_messages" | "anthropic-messages" => {
            Some(crate::providers::types::ApiType::AnthropicMessages)
        }
        "OpenAiResponses" | "openai_responses" | "openai-responses" => {
            Some(crate::providers::types::ApiType::OpenAiResponses)
        }
        "OpenAiCodexResponses" | "openai_codex_responses" | "openai-codex-responses" => {
            Some(crate::providers::types::ApiType::OpenAiCodexResponses)
        }
        _ => None,
    }
}

fn parse_safety_mode(s: &str) -> Result<crate::safety::SafetyMode, Error> {
    match s {
        "ask" => Ok(crate::safety::SafetyMode::Ask),
        "supervised" => Ok(crate::safety::SafetyMode::Supervised),
        "yolo" => Ok(crate::safety::SafetyMode::Yolo),
        _ => Err(Error::InvalidInput(format!("Invalid safety mode: '{}'. Use 'ask', 'supervised', or 'yolo'", s))),
    }
}

fn safety_mode_to_str(mode: &crate::safety::SafetyMode) -> &'static str {
    match mode {
        crate::safety::SafetyMode::Ask => "ask",
        crate::safety::SafetyMode::Supervised => "supervised",
        crate::safety::SafetyMode::Yolo => "yolo",
    }
}

// ─── Safety Commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_safety_policy(state: State<'_, AppState>) -> Result<SafetyPolicyResponse, Error> {
    let mgr = state.safety_manager.read().await;
    let policy = mgr.policy();
    Ok(SafetyPolicyResponse {
        global_mode: safety_mode_to_str(&policy.global_mode).to_string(),
        tool_overrides: policy.tool_overrides.iter()
            .map(|(k, v)| (k.clone(), safety_mode_to_str(v).to_string()))
            .collect(),
        auto_approved_tools: policy.auto_approved_tools.iter().cloned().collect(),
        blocked_tools: policy.blocked_tools.iter().cloned().collect(),
    })
}

#[tauri::command]
pub async fn set_safety_mode(state: State<'_, AppState>, input: SetSafetyModeInput) -> Result<SafetyPolicyResponse, Error> {
    let mode = parse_safety_mode(&input.mode)?;
    let mut mgr = state.safety_manager.write().await;
    mgr.set_global_mode(mode)?;
    let policy = mgr.policy();
    Ok(SafetyPolicyResponse {
        global_mode: safety_mode_to_str(&policy.global_mode).to_string(),
        tool_overrides: policy.tool_overrides.iter()
            .map(|(k, v)| (k.clone(), safety_mode_to_str(v).to_string()))
            .collect(),
        auto_approved_tools: policy.auto_approved_tools.iter().cloned().collect(),
        blocked_tools: policy.blocked_tools.iter().cloned().collect(),
    })
}

#[tauri::command]
pub async fn set_tool_safety_override(state: State<'_, AppState>, input: SetToolOverrideInput) -> Result<SafetyPolicyResponse, Error> {
    let mode = parse_safety_mode(&input.mode)?;
    let mut mgr = state.safety_manager.write().await;
    mgr.set_tool_override(&input.tool_name, mode)?;
    let policy = mgr.policy();
    Ok(SafetyPolicyResponse {
        global_mode: safety_mode_to_str(&policy.global_mode).to_string(),
        tool_overrides: policy.tool_overrides.iter()
            .map(|(k, v)| (k.clone(), safety_mode_to_str(v).to_string()))
            .collect(),
        auto_approved_tools: policy.auto_approved_tools.iter().cloned().collect(),
        blocked_tools: policy.blocked_tools.iter().cloned().collect(),
    })
}

#[tauri::command]
pub async fn remove_tool_safety_override(state: State<'_, AppState>, input: ToolNameInput) -> Result<SafetyPolicyResponse, Error> {
    let mut mgr = state.safety_manager.write().await;
    mgr.remove_tool_override(&input.tool_name)?;
    let policy = mgr.policy();
    Ok(SafetyPolicyResponse {
        global_mode: safety_mode_to_str(&policy.global_mode).to_string(),
        tool_overrides: policy.tool_overrides.iter()
            .map(|(k, v)| (k.clone(), safety_mode_to_str(v).to_string()))
            .collect(),
        auto_approved_tools: policy.auto_approved_tools.iter().cloned().collect(),
        blocked_tools: policy.blocked_tools.iter().cloned().collect(),
    })
}

#[tauri::command]
pub async fn add_auto_approved_tool(state: State<'_, AppState>, input: ToolNameInput) -> Result<SafetyPolicyResponse, Error> {
    let mut mgr = state.safety_manager.write().await;
    mgr.add_auto_approved(&input.tool_name)?;
    let policy = mgr.policy();
    Ok(SafetyPolicyResponse {
        global_mode: safety_mode_to_str(&policy.global_mode).to_string(),
        tool_overrides: policy.tool_overrides.iter()
            .map(|(k, v)| (k.clone(), safety_mode_to_str(v).to_string()))
            .collect(),
        auto_approved_tools: policy.auto_approved_tools.iter().cloned().collect(),
        blocked_tools: policy.blocked_tools.iter().cloned().collect(),
    })
}

#[tauri::command]
pub async fn remove_auto_approved_tool(state: State<'_, AppState>, input: ToolNameInput) -> Result<SafetyPolicyResponse, Error> {
    let mut mgr = state.safety_manager.write().await;
    mgr.remove_auto_approved(&input.tool_name)?;
    let policy = mgr.policy();
    Ok(SafetyPolicyResponse {
        global_mode: safety_mode_to_str(&policy.global_mode).to_string(),
        tool_overrides: policy.tool_overrides.iter()
            .map(|(k, v)| (k.clone(), safety_mode_to_str(v).to_string()))
            .collect(),
        auto_approved_tools: policy.auto_approved_tools.iter().cloned().collect(),
        blocked_tools: policy.blocked_tools.iter().cloned().collect(),
    })
}

#[tauri::command]
pub async fn block_tool(state: State<'_, AppState>, input: ToolNameInput) -> Result<SafetyPolicyResponse, Error> {
    let mut mgr = state.safety_manager.write().await;
    mgr.block_tool(&input.tool_name)?;
    let policy = mgr.policy();
    Ok(SafetyPolicyResponse {
        global_mode: safety_mode_to_str(&policy.global_mode).to_string(),
        tool_overrides: policy.tool_overrides.iter()
            .map(|(k, v)| (k.clone(), safety_mode_to_str(v).to_string()))
            .collect(),
        auto_approved_tools: policy.auto_approved_tools.iter().cloned().collect(),
        blocked_tools: policy.blocked_tools.iter().cloned().collect(),
    })
}

#[tauri::command]
pub async fn unblock_tool(state: State<'_, AppState>, input: ToolNameInput) -> Result<SafetyPolicyResponse, Error> {
    let mut mgr = state.safety_manager.write().await;
    mgr.unblock_tool(&input.tool_name)?;
    let policy = mgr.policy();
    Ok(SafetyPolicyResponse {
        global_mode: safety_mode_to_str(&policy.global_mode).to_string(),
        tool_overrides: policy.tool_overrides.iter()
            .map(|(k, v)| (k.clone(), safety_mode_to_str(v).to_string()))
            .collect(),
        auto_approved_tools: policy.auto_approved_tools.iter().cloned().collect(),
        blocked_tools: policy.blocked_tools.iter().cloned().collect(),
    })
}

#[tauri::command]
pub async fn assess_command_risk(state: State<'_, AppState>, input: AssessCommandInput) -> Result<CommandRiskResponse, Error> {
    let mgr = state.safety_manager.read().await;
    let assessment = mgr.assess_command_risk(&input.command);
    let suggested = match &assessment.suggested_action {
        crate::safety::ApprovalDecision::AutoApprove => "auto_approve".to_string(),
        crate::safety::ApprovalDecision::RequireApproval { .. } => "require_approval".to_string(),
        crate::safety::ApprovalDecision::Block { .. } => "block".to_string(),
    };
    Ok(CommandRiskResponse {
        level: format!("{:?}", assessment.level).to_lowercase(),
        reasons: assessment.reasons,
        suggested_action: suggested,
    })
}

// ─── Tool Approval Commands ─────────────────────────────────────────────────

#[tauri::command]
pub async fn approve_tool_call(
    state: State<'_, AppState>,
    _app_handle: tauri::AppHandle,
    input: ApproveToolCallInput,
) -> Result<ApproveToolCallResponse, Error> {
    tracing::info!(
        session_id = %input.session_id,
        tool_id = %input.tool_id,
        approved = input.approved,
        always_allow = ?input.always_allow,
        tool_name = ?input.tool_name,
        "Tool approval response received"
    );

    // If approved with always_allow, add tool to auto-approved whitelist immediately
    if input.approved {
        if input.always_allow.unwrap_or(false) {
            if let Some(ref tool_name) = input.tool_name {
                let mut mgr = state.safety_manager.write().await;
                let _ = mgr.add_auto_approved(tool_name);
                tracing::info!(tool_name = %tool_name, "Tool added to auto-approved whitelist via always_allow");
            }
        }
    }

    // Resolve the pending approval via oneshot channel
    let result = crate::app::ApprovalResult {
        approved: input.approved,
        always_allow: input.always_allow.unwrap_or(false),
        tool_name: input.tool_name,
    };

    let resolved = state.pending_approvals.resolve(&input.tool_id, result);
    if !resolved {
        tracing::warn!(tool_id = %input.tool_id, "No pending approval found for tool_id");
    }

    Ok(ApproveToolCallResponse { success: resolved })
}

// ─── Memory Graph Commands ──────────────────────────────────────────────

/// 搜索记忆图（触发 5 层召回）
#[tauri::command]
pub async fn memory_graph_search(
    state: State<'_, AppState>,
    input: MemoryGraphSearchInput,
) -> Result<serde_json::Value, String> {
    let store = &state.memory_graph_store;
    let memu_client = state.memu_client.clone();
    let space_id = input.space_id.unwrap_or_else(|| "global".into());

    let engine = crate::memory_graph::recall::MemoryRecallEngine::new(
        store.clone(),
        memu_client,
        crate::memory_graph::recall::MemoryRecallConfig::default(),
    );

    let plan = engine.build_recall_plan(&space_id, &input.query, false)
        .await
        .map_err(|e| format!("Recall failed: {}", e))?;

    serde_json::to_value(&plan).map_err(|e| format!("Serialization failed: {}", e))
}

/// 获取记忆节点详情（含版本历史）
#[tauri::command]
pub async fn memory_graph_get_node(
    state: State<'_, AppState>,
    input: MemoryGraphGetNodeInput,
) -> Result<serde_json::Value, String> {
    let store = &state.memory_graph_store;

    let detail = store.get_node_detail(&input.node_id)
        .map_err(|e| format!("Failed to get node detail: {}", e))?
        .ok_or_else(|| format!("Node not found: {}", input.node_id))?;

    let all_versions = store.get_versions(&input.node_id)
        .map_err(|e| format!("Failed to get versions: {}", e))?;

    serde_json::to_value(serde_json::json!({
        "node": detail.node,
        "activeVersion": detail.active_version,
        "allVersions": all_versions,
        "routes": detail.routes,
        "keywords": detail.keywords,
    })).map_err(|e| format!("Serialization failed: {}", e))
}

/// 列出 Boot 集成员
#[tauri::command]
pub async fn memory_graph_list_boot(
    state: State<'_, AppState>,
    input: MemoryGraphListBootInput,
) -> Result<serde_json::Value, String> {
    let store = &state.memory_graph_store;
    let space_id = input.space_id.unwrap_or_else(|| "global".into());
    let limit = input.limit.unwrap_or(8);

    let boot_nodes = store.list_boot_nodes(&space_id, limit)
        .map_err(|e| format!("Failed to list boot nodes: {}", e))?;

    serde_json::to_value(&boot_nodes).map_err(|e| format!("Serialization failed: {}", e))
}

/// 管理 Boot 集（添加/移除）
#[tauri::command]
pub async fn memory_graph_manage_boot(
    state: State<'_, AppState>,
    input: MemoryGraphManageBootInput,
) -> Result<serde_json::Value, String> {
    let store = &state.memory_graph_store;
    let space_id = input.space_id.unwrap_or_else(|| "global".into());

    match input.action.as_str() {
        "add" => {
            let priority = input.priority.unwrap_or(0);
            store.add_to_boot(&space_id, &input.node_id, priority)
                .map_err(|e| format!("Failed to add to boot: {}", e))?;
            Ok(serde_json::json!({ "success": true, "action": "add", "nodeId": input.node_id }))
        }
        "remove" => {
            store.remove_from_boot(&space_id, &input.node_id)
                .map_err(|e| format!("Failed to remove from boot: {}", e))?;
            Ok(serde_json::json!({ "success": true, "action": "remove", "nodeId": input.node_id }))
        }
        _ => Err(format!("Invalid action: '{}'. Use 'add' or 'remove'", input.action)),
    }
}

/// 时间线
#[tauri::command]
pub async fn memory_graph_list_timeline(
    state: State<'_, AppState>,
    input: MemoryGraphTimelineInput,
) -> Result<serde_json::Value, String> {
    let store = &state.memory_graph_store;
    let space_id = input.space_id.unwrap_or_else(|| "global".into());
    let limit = input.limit.unwrap_or(20);

    let nodes = store.list_recent_nodes(&space_id, limit)
        .map_err(|e| format!("Failed to list recent nodes: {}", e))?;

    let mut entries = Vec::new();
    for node in nodes {
        let active_version = store.get_active_version(&node.id)
            .map_err(|e| format!("Failed to get active version: {}", e))?;
        let content_snippet = active_version
            .as_ref()
            .map(|v| {
                if v.content.chars().count() > 120 {
                    format!("{}...", v.content.chars().take(120).collect::<String>())
                } else {
                    v.content.clone()
                }
            })
            .unwrap_or_default();
        entries.push(serde_json::json!({
            "nodeId": node.id,
            "title": node.title,
            "contentSnippet": content_snippet,
            "kind": node.kind,
            "updatedAt": node.updated_at,
        }));
    }

    serde_json::to_value(&entries).map_err(|e| format!("Serialization failed: {}", e))
}

/// 召回解释（调试用）
#[tauri::command]
pub async fn memory_graph_explain_recall(
    state: State<'_, AppState>,
    input: MemoryGraphExplainRecallInput,
) -> Result<serde_json::Value, String> {
    let store = &state.memory_graph_store;
    let memu_client = state.memu_client.clone();
    let space_id = input.space_id.unwrap_or_else(|| "global".into());

    let engine = crate::memory_graph::recall::MemoryRecallEngine::new(
        store.clone(),
        memu_client,
        crate::memory_graph::recall::MemoryRecallConfig::default(),
    );

    let explanation = engine.explain_recall(&space_id, &input.query)
        .await
        .map_err(|e| format!("Explain recall failed: {}", e))?;

    serde_json::to_value(&explanation).map_err(|e| format!("Serialization failed: {}", e))
}

/// 获取完整图谱数据（所有节点 + 边 + 路由），供前端渲染图形化视图
#[tauri::command]
pub async fn memory_graph_get_full_graph(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let store = &state.memory_graph_store;
    let nodes = store.list_all_nodes(200).map_err(|e| format!("Failed to list nodes: {}", e))?;
    let edges = store.list_all_edges().map_err(|e| format!("Failed to list edges: {}", e))?;
    let routes = store.list_all_routes().map_err(|e| format!("Failed to list routes: {}", e))?;
    Ok(serde_json::json!({
        "nodes": nodes,
        "edges": edges,
        "routes": routes,
    }))
}

/// 创建记忆节点
#[tauri::command]
pub async fn memory_graph_create_node(
    state: State<'_, AppState>,
    input: MemoryGraphCreateNodeInput,
) -> Result<serde_json::Value, String> {
    use crate::memory_graph::models::{MemoryNode, MemoryNodeKind};

    let now = chrono::Utc::now().to_rfc3339();
    let node = MemoryNode {
        id: uuid::Uuid::new_v4().to_string(),
        space_id: input.space_id,
        kind: MemoryNodeKind::from_str(&input.kind),
        title: input.title,
        metadata: input.metadata,
        created_at: now.clone(),
        updated_at: now,
    };

    let store = &state.memory_graph_store;
    store.create_node(&node).map_err(|e| format!("Failed to create node: {}", e))?;

    serde_json::to_value(&node).map_err(|e| format!("Serialization failed: {}", e))
}

/// 更新记忆节点
#[tauri::command]
pub async fn memory_graph_update_node(
    state: State<'_, AppState>,
    input: MemoryGraphUpdateNodeInput,
) -> Result<serde_json::Value, String> {
    use crate::memory_graph::models::MemoryNodeKind;

    let store = &state.memory_graph_store;
    let kind = input.kind.as_deref().map(MemoryNodeKind::from_str);

    store.update_node(
        &input.node_id,
        input.title.as_deref(),
        kind,
        input.metadata.as_ref(),
    ).map_err(|e| format!("Failed to update node: {}", e))?;

    Ok(serde_json::json!({ "success": true, "nodeId": input.node_id }))
}

// ===== MEMUBOT 服务控制命令 =====

/// 获取所有服务的健康状态
#[tauri::command]
pub async fn services_health(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let summary = state.service_manager.get_all_health().await;
    serde_json::to_value(summary).map_err(|e| e.to_string())
}

/// 获取记忆提取服务状态
#[tauri::command]
pub async fn memorization_status(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let health = state.service_manager.get_health("memorization").await;
    match health {
        Some(h) => serde_json::to_value(h).map_err(|e| e.to_string()),
        None => Ok(serde_json::json!({"status": "not_registered"})),
    }
}

/// 获取主动服务状态
#[tauri::command]
pub async fn proactive_status(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let health = state.service_manager.get_health("proactive").await;
    match health {
        Some(h) => serde_json::to_value(h).map_err(|e| e.to_string()),
        None => Ok(serde_json::json!({"status": "not_registered", "enabled": false})),
    }
}

/// 启动主动服务
#[tauri::command]
pub async fn proactive_start(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.service_manager.restart_service("proactive").await
        .map_err(|e| e.to_string())
}

/// 停止主动服务
#[tauri::command]
pub async fn proactive_stop(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.service_manager.stop_service("proactive").await
        .map_err(|e| e.to_string())
}

/// 获取可观测性指标
#[tauri::command]
pub async fn metrics_summary(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let summary = state.metrics_service.get_summary().await;
    serde_json::to_value(summary).map_err(|e| e.to_string())
}

/// 获取 MEMUBOT 配置
#[tauri::command]
pub async fn memubot_config_get(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    serde_json::to_value(&state.memubot_config).map_err(|e| e.to_string())
}

/// 删除记忆节点
#[tauri::command]
pub async fn memory_graph_delete_node(
    state: State<'_, AppState>,
    input: MemoryGraphDeleteNodeInput,
) -> Result<serde_json::Value, String> {
    let store = &state.memory_graph_store;
    store.delete_node(&input.node_id).map_err(|e| format!("Failed to delete node: {}", e))?;

    Ok(serde_json::json!({ "success": true, "nodeId": input.node_id }))
}

// ─── Learned Skills Commands ─────────────────────────────────────────────────

/// 列出所有学到的技能（Procedure 节点 + metadata.skill_type == "learned"）
#[tauri::command]
pub async fn list_learned_skills(
    state: State<'_, AppState>,
    space_id: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    use crate::memory_graph::models::MemoryNodeKind;

    let store = &state.memory_graph_store;
    let sid = space_id.unwrap_or_else(|| "global".into());

    let nodes = store.list_nodes_by_kind(&sid, MemoryNodeKind::Procedure, 500)
        .map_err(|e| format!("Failed to list procedure nodes: {}", e))?;

    let mut results = Vec::new();
    for node in nodes {
        if let Some(ref meta) = node.metadata {
            if meta.get("skill_type").and_then(|v| v.as_str()) == Some("learned") {
                results.push(serde_json::json!({
                    "id": node.id,
                    "name": node.title,
                    "context": meta.get("context").cloned().unwrap_or(serde_json::Value::Null),
                    "principles": meta.get("principles").cloned().unwrap_or(serde_json::Value::Null),
                    "steps": meta.get("steps").cloned().unwrap_or(serde_json::Value::Null),
                    "pitfalls": meta.get("pitfalls").cloned().unwrap_or(serde_json::Value::Null),
                    "enabled": meta.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                    "usageCount": meta.get("usage_count").and_then(|v| v.as_u64()).unwrap_or(0),
                    "createdAt": node.created_at,
                }));
            }
        }
    }
    Ok(results)
}

/// 获取单个学到的技能详情（含 version content）
#[tauri::command]
pub async fn get_learned_skill(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<serde_json::Value, String> {
    let store = &state.memory_graph_store;

    let node = store.get_node(&skill_id)
        .map_err(|e| format!("Failed to get node: {}", e))?
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;

    let meta = node.metadata.as_ref().cloned().unwrap_or(serde_json::json!({}));
    let active_version = store.get_active_version(&skill_id)
        .map_err(|e| format!("Failed to get active version: {}", e))?;

    let content = active_version.map(|v| v.content).unwrap_or_default();

    Ok(serde_json::json!({
        "id": node.id,
        "name": node.title,
        "context": meta.get("context").cloned().unwrap_or(serde_json::Value::Null),
        "principles": meta.get("principles").cloned().unwrap_or(serde_json::Value::Null),
        "steps": meta.get("steps").cloned().unwrap_or(serde_json::Value::Null),
        "pitfalls": meta.get("pitfalls").cloned().unwrap_or(serde_json::Value::Null),
        "enabled": meta.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "usageCount": meta.get("usage_count").and_then(|v| v.as_u64()).unwrap_or(0),
        "createdAt": node.created_at,
        "content": content,
    }))
}

/// 切换学到的技能的启用/禁用状态
#[tauri::command]
pub async fn toggle_learned_skill(
    state: State<'_, AppState>,
    skill_id: String,
    enabled: bool,
) -> Result<(), String> {
    let store = &state.memory_graph_store;

    let node = store.get_node(&skill_id)
        .map_err(|e| format!("Failed to get node: {}", e))?
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;

    let mut meta = node.metadata.unwrap_or(serde_json::json!({}));
    if let Some(obj) = meta.as_object_mut() {
        obj.insert("enabled".to_string(), serde_json::Value::Bool(enabled));
    }

    store.update_node(&skill_id, None, None, Some(&meta))
        .map_err(|e| format!("Failed to update node: {}", e))?;
    Ok(())
}

/// 删除学到的技能
#[tauri::command]
pub async fn delete_learned_skill(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<(), String> {
    let store = &state.memory_graph_store;
    store.delete_node(&skill_id)
        .map_err(|e| format!("Failed to delete skill: {}", e))?;
    Ok(())
}

// ─── Dev / Testing Commands ──────────────────────────────────────────────────

/// 手动触发指定的 Proactive 场景（跳过定时器和阈值条件）
///
/// 用于端到端验证完整链路：场景 → memorize → IPC 事件。
/// 生产环境也可调用，日志会标注为手动触发。
#[tauri::command]
pub async fn trigger_proactive_scenario(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    scenario_name: String,
) -> Result<serde_json::Value, String> {
    let valid_scenarios = ["conversation_learning", "skill_extraction", "multimodal_context"];
    if !valid_scenarios.contains(&scenario_name.as_str()) {
        return Err(format!(
            "Unknown scenario: {}. Valid: {:?}",
            scenario_name, valid_scenarios
        ));
    }

    tracing::info!(
        "[DevTrigger] Manually triggering proactive scenario: {}",
        scenario_name
    );

    // 尝试通过 memU client 执行真实的 memorize
    let mut items_extracted: usize = 0;
    let mut categories: Vec<String> = vec![];

    if let Some(ref memu) = state.memu_client {
        let (memory_types, source_type): (Vec<&str>, &str) = match scenario_name.as_str() {
            "conversation_learning" => (
                vec!["profile", "behavior"],
                "proactive_test_conversation",
            ),
            "skill_extraction" => (
                vec!["skill", "tool"],
                "proactive_test_skill",
            ),
            _ => (
                vec!["knowledge"],
                "proactive_test_multimodal",
            ),
        };

        let test_content = format!(
            "[Dev Test] Triggered {} scenario manually at {}",
            scenario_name,
            chrono::Utc::now().to_rfc3339()
        );

        match memu
            .memorize_with_config(&test_content, &memory_types, None, source_type)
            .await
        {
            Ok(result) => {
                items_extracted = result.items_extracted;
                categories = result.categories_updated;
                tracing::info!(
                    "[DevTrigger] memorize_with_config OK: items={}, categories={:?}",
                    items_extracted,
                    categories
                );
            }
            Err(e) => {
                tracing::warn!("[DevTrigger] memorize_with_config failed: {}", e);
            }
        }
    } else {
        tracing::warn!("[DevTrigger] memu_client is None, skipping memorize");
    }

    // Emit IPC 事件到前端
    let summary = format!("[Dev Test] {} 场景手动触发成功", scenario_name);
    let _ = app_handle.emit(
        "agent:proactive-learning",
        serde_json::json!({
            "scenario": scenario_name,
            "items_extracted": items_extracted,
            "categories": categories,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "summary": summary,
            "dev_trigger": true,
        }),
    );

    Ok(serde_json::json!({
        "success": true,
        "scenario": scenario_name,
        "items_extracted": items_extracted,
        "categories": categories,
        "dev_trigger": true,
    }))
}

// ─── Agent Session Control ───────────────────────────────────────────────────

/// Stop a running agentic loop for the given conversation.
/// Returns true if a session was found and cancelled, false if no session was running.
#[tauri::command]
pub async fn stop_agent_session(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<bool, Error> {
    let mut sessions = state.running_sessions.lock().await;
    if let Some(token) = sessions.remove(&conversation_id) {
        token.cancel();
        Ok(true)
    } else {
        Ok(false)
    }
}

// ─── Browser Commands (Phase 3) ─────────────────────────────────────────────

#[tauri::command]
pub async fn browser_get_state(
    state: State<'_, AppState>,
) -> Result<crate::browser::types::BrowserState, Error> {
    Ok(state.browser_service.get_state().await)
}

#[tauri::command]
pub async fn browser_launch(
    state: State<'_, AppState>,
) -> Result<bool, Error> {
    state.browser_service.launch().await?;
    Ok(true)
}

#[tauri::command]
pub async fn browser_shutdown(
    state: State<'_, AppState>,
) -> Result<bool, Error> {
    state.browser_service.shutdown().await?;
    Ok(true)
}
