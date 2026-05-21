use std::sync::Arc;

use tauri::Emitter;
use tracing::{error, info, warn};

use super::models::*;
use super::store::MemoryGraphStore;
use crate::agent::types::{ReflectionDetail, ReflectionMessage, ReflectionToolCall};
use crate::memu::client::MemUClient;

/// memU memory_type -> Steward MemoryNodeKind mapping
fn map_memu_type_to_kind(memu_type: &str) -> MemoryNodeKind {
    match memu_type {
        "profile" => MemoryNodeKind::UserProfile,
        "event" => MemoryNodeKind::Episode,
        "knowledge" => MemoryNodeKind::Reference,
        "behavior" => MemoryNodeKind::Directive,
        "skill" => MemoryNodeKind::Procedure,
        "tool" => MemoryNodeKind::Procedure,
        _ => MemoryNodeKind::Reference,
    }
}

/// Convert a title string into a URL-friendly slug.
fn title_to_slug(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c == ' ' || c == '_' {
                '-'
            } else {
                // Keep CJK characters as-is
                if c as u32 >= 0x4E00 {
                    c
                } else {
                    '-'
                }
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Generate a URI path for a memory node based on its kind and title.
fn generate_route_path(kind: MemoryNodeKind, title: &str) -> String {
    let slug = title_to_slug(title);
    match kind {
        MemoryNodeKind::UserProfile => format!("user/profile/{}", slug),
        MemoryNodeKind::Identity => format!("user/identity/{}", slug),
        MemoryNodeKind::Value => format!("user/value/{}", slug),
        MemoryNodeKind::Directive => format!("directives/{}", slug),
        MemoryNodeKind::Episode => format!("episodes/{}", slug),
        MemoryNodeKind::Procedure => format!("procedures/{}", slug),
        MemoryNodeKind::Curated => format!("curated/{}", slug),
        MemoryNodeKind::Reference => format!("reference/{}", slug),
        MemoryNodeKind::Boot => format!("boot/{}", slug),
        // EntityPage (Memory OS Foundation Phase 1) — per-entity wiki page.
        // The dedicated `entity/<slug>` namespace mirrors gbrain's MECE
        // directory convention and gives EntityPage routes a stable home
        // that won't collide with the historical kinds above.
        MemoryNodeKind::EntityPage => format!("entity/{}", slug),
    }
}

/// Extract keywords from a summary string.
/// Simple strategy: split on whitespace/punctuation, filter short tokens, deduplicate.
fn extract_keywords(summary: &str) -> Vec<String> {
    let mut keywords = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for word in summary.split(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';' || c == ':' || c == '!' || c == '?' || c == '(' || c == ')' || c == '[' || c == ']' || c == '/' || c == '\\') {
        let trimmed = word.trim().to_lowercase();
        // Keep words that are meaningful (length >= 2 for ASCII, >= 1 for CJK)
        let is_meaningful = if trimmed.chars().any(|c| c as u32 >= 0x4E00) {
            !trimmed.is_empty()
        } else {
            trimmed.len() >= 3
        };
        if is_meaningful && seen.insert(trimmed.clone()) {
            keywords.push(trimmed);
        }
        if keywords.len() >= 10 {
            break;
        }
    }
    keywords
}

/// Extract a query string from user input for recall-before-memorize.
/// Trims and truncates to 200 chars.
fn extract_query_from_input(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() > 200 {
        // Find a safe char boundary
        let mut end = 200;
        while end < trimmed.len() && !trimmed.is_char_boundary(end) {
            end += 1;
        }
        trimmed[..end].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Check if new content is already covered by existing retrieved memories.
///
/// We trust the memU retrieve (vector search) results: if retrieve returned
/// items, they are semantically similar to the input — even across languages
/// (e.g. Chinese input vs English stored memory). This avoids the failure mode
/// of word-level Jaccard overlap which always yields 0 for cross-language pairs.
fn is_covered_by_existing(existing_items: &[serde_json::Value], _new_content: &str) -> bool {
    if existing_items.is_empty() {
        return false;
    }

    for item in existing_items {
        let summary = item
            .get("summary")
            .or_else(|| item.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Skip items with no meaningful content
        if summary.is_empty() {
            continue;
        }

        // If a score/relevance field exists, use threshold check
        if let Some(score) = item
            .get("score")
            .or_else(|| item.get("relevance"))
            .and_then(|v| v.as_f64())
        {
            if score > 0.3 {
                let preview_end = safe_char_boundary(summary, 80);
                info!(
                    score = format!("{:.2}", score),
                    existing_preview = &summary[..preview_end],
                    "reflection: existing memory covers input (by score)"
                );
                return true;
            }
            // Low score — not a real match, skip this item
            continue;
        }

        // No score field — retrieve returned this item so the vector DB
        // considers it relevant. Trust the result.
        let preview_end = safe_char_boundary(summary, 80);
        info!(
            existing_preview = &summary[..preview_end],
            "reflection: existing memory covers input (retrieve hit)"
        );
        return true;
    }

    false
}

/// Find a char-boundary–safe slice end for preview strings.
fn safe_char_boundary(s: &str, max_bytes: usize) -> usize {
    let mut end = s.len().min(max_bytes);
    while end < s.len() && !s.is_char_boundary(end) {
        end += 1;
    }
    end.min(s.len())
}

/// Deduplicate items based on text similarity.
/// Returns references to unique items, merging semantically similar ones.
fn deduplicate_items(items: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    let mut result: Vec<&serde_json::Value> = Vec::new();

    for item in items {
        let summary = item
            .get("summary")
            .or_else(|| item.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if summary.is_empty() {
            continue;
        }

        // Check if this item is semantically similar to any existing item
        let is_duplicate = result.iter().any(|existing| {
            let existing_summary = existing
                .get("summary")
                .or_else(|| existing.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            text_similarity(summary, existing_summary) > 0.6
        });

        if !is_duplicate {
            result.push(item);
        }
    }
    result
}

/// Compute Jaccard similarity on word sets for simple text comparison.
fn text_similarity(a: &str, b: &str) -> f64 {
    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();

    if words_a.is_empty() && words_b.is_empty() {
        return 1.0;
    }

    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();

    if union == 0 {
        return 0.0;
    }

    intersection as f64 / union as f64
}

/// Check if a node kind qualifies for the Boot set.
fn is_boot_eligible(kind: MemoryNodeKind) -> bool {
    matches!(
        kind,
        MemoryNodeKind::Identity | MemoryNodeKind::Value | MemoryNodeKind::Directive
    )
}

/// 检查输入是否为纯问候语
fn is_greeting(input: &str) -> bool {
    let normalized = input.to_lowercase();
    let greetings = [
        "你好", "您好", "hi", "hello", "hey", "嗨", "哈喽",
        "早", "早上好", "上午好", "下午好", "晚上好", "晚安",
        "good morning", "good afternoon", "good evening", "good night",
        "嗯", "哦", "ok", "okay", "好的", "好", "是的", "对",
        "谢谢", "感谢", "thanks", "thank you", "thx",
        "再见", "拜拜", "bye", "goodbye",
    ];
    greetings.iter().any(|g| {
        normalized == *g
            || normalized == format!("{}！", g)
            || normalized == format!("{}!", g)
    })
}

/// 检查输入是否为纯命令型（不含个人信息的指令）
fn is_command_only(input: &str) -> bool {
    let command_patterns = [
        "帮我", "请", "写一个", "生成", "翻译", "解释", "分析",
        "help me", "please", "write", "generate", "translate", "explain",
        "搜索", "查找", "打开", "运行", "执行",
    ];
    let normalized = input.to_lowercase();
    // 如果输入很短且只是一个命令词，跳过
    if input.chars().count() < 10 {
        return command_patterns
            .iter()
            .any(|p| normalized.starts_with(p) || normalized == *p);
    }
    false
}

pub struct ReflectionOrchestrator {
    store: Arc<MemoryGraphStore>,
    memu_client: Option<Arc<MemUClient>>,
    app_handle: tauri::AppHandle,
}

impl ReflectionOrchestrator {
    pub fn new(
        store: Arc<MemoryGraphStore>,
        memu_client: Option<Arc<MemUClient>>,
        app_handle: tauri::AppHandle,
    ) -> Self {
        Self {
            store,
            memu_client,
            app_handle,
        }
    }

    /// Emit a reflection status event to the frontend.
    fn emit_status(&self, assistant_message_id: &str, status: &str) {
        let payload = serde_json::json!({
            "assistant_message_id": assistant_message_id,
            "status": status,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        info!(assistant_message_id, status, "reflection: emitting status");
        let _ = self.app_handle.emit("agent:reflection_status", &payload);
    }

    /// Emit a full reflection detail event.
    fn emit_reflection(&self, detail: &ReflectionDetail) {
        info!(
            assistant_message_id = %detail.assistant_message_id,
            status = %detail.status,
            outcome = ?detail.outcome,
            "reflection: emitting detail"
        );
        let _ = self.app_handle.emit("agent:reflection", detail);
    }

    /// Emit `agent:proactive-learning` so the AgentMessages chip surfaces a
    /// "对话学习 · N 条 · [categories]" badge after a successful reflection.
    ///
    /// Bundle 4 — previously this only fired from `proactive::service.rs`
    /// (the scenario-driven path), so the reflection-driven memorize (which
    /// runs after every chat turn) was silent in the UI even when items
    /// were extracted. With this hook the chip appears for both paths
    /// and the frontend listener can dedup by timestamp if needed.
    fn emit_proactive_learning_chip(
        &self,
        conversation_id: &str,
        items_count: usize,
        categories: Vec<String>,
        summary: String,
    ) {
        if items_count == 0 {
            return;
        }
        let payload = serde_json::json!({
            "scenario": "conversation_learning",
            "items_extracted": items_count,
            "categories": categories,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "summary": summary,
            // tauri_commands::send_message passes the chat conversation_id
            // here; the chip filter is `ev.sessionId === sessionId || null`
            // so this surfaces under the right session.
            "sessionId": conversation_id,
        });
        info!(
            items = items_count,
            "reflection: emitting agent:proactive-learning chip event"
        );
        let _ = self.app_handle.emit("agent:proactive-learning", &payload);
    }

    /// Async reflection flow, called after conversation completes.
    pub async fn reflect(
        &self,
        space_id: &str,
        _conversation_id: &str,
        user_input: &str,
        _assistant_output: &str,
        assistant_message_id: &str,
    ) -> anyhow::Result<()> {
        let run_started = chrono::Utc::now().to_rfc3339();

        // 1. Emit queued status
        self.emit_status(assistant_message_id, "queued");

        // 2. Emit running status
        self.emit_status(assistant_message_id, "running");

        // 3. Check if memU is available
        let memu = match &self.memu_client {
            Some(client) => client.clone(),
            None => {
                // memU not available — emit no_op
                info!("reflection: memU not available, emitting no_op");
                let run_completed = chrono::Utc::now().to_rfc3339();
                self.emit_status(assistant_message_id, "completed");
                self.emit_reflection(&ReflectionDetail {
                    assistant_message_id: assistant_message_id.to_string(),
                    status: "completed".to_string(),
                    outcome: Some("no_op".to_string()),
                    summary: Some("memU 不可用，跳过记忆反思".to_string()),
                    detail: None,
                    run_started_at: Some(run_started),
                    run_completed_at: Some(run_completed),
                    tool_calls: vec![],
                    messages: vec![ReflectionMessage {
                        id: uuid::Uuid::new_v4().to_string(),
                        content: "memU service unavailable; reflection skipped".to_string(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    }],
                });
                return Ok(());
            }
        };

        // === 信息量预过滤 ===
        let trimmed_input = user_input.trim();

        // 1. 过短输入跳过
        if trimmed_input.chars().count() < 4 {
            info!("reflection: input too short ({} chars), skipping memorize", trimmed_input.len());
            let run_completed = chrono::Utc::now().to_rfc3339();
            self.emit_status(assistant_message_id, "completed");
            self.emit_reflection(&ReflectionDetail {
                assistant_message_id: assistant_message_id.to_string(),
                status: "completed".to_string(),
                outcome: Some("no_op".to_string()),
                summary: Some("输入过短，跳过记忆反思".to_string()),
                detail: None,
                run_started_at: Some(run_started.clone()),
                run_completed_at: Some(run_completed),
                tool_calls: vec![],
                messages: vec![ReflectionMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    content: "Input too short; reflection skipped".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                }],
            });
            return Ok(());
        }

        // 2. 纯问候语跳过
        if is_greeting(trimmed_input) {
            info!("reflection: input is greeting, skipping memorize");
            let run_completed = chrono::Utc::now().to_rfc3339();
            self.emit_status(assistant_message_id, "completed");
            self.emit_reflection(&ReflectionDetail {
                assistant_message_id: assistant_message_id.to_string(),
                status: "completed".to_string(),
                outcome: Some("no_op".to_string()),
                summary: Some("问候语输入，跳过记忆反思".to_string()),
                detail: None,
                run_started_at: Some(run_started.clone()),
                run_completed_at: Some(run_completed),
                tool_calls: vec![],
                messages: vec![ReflectionMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    content: "Input is greeting; reflection skipped".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                }],
            });
            return Ok(());
        }

        // 3. 纯指令型输入跳过（用户只是让 agent 做事，没有新的个人信息）
        if is_command_only(trimmed_input) {
            info!("reflection: input is command-only, skipping memorize");
            let run_completed = chrono::Utc::now().to_rfc3339();
            self.emit_status(assistant_message_id, "completed");
            self.emit_reflection(&ReflectionDetail {
                assistant_message_id: assistant_message_id.to_string(),
                status: "completed".to_string(),
                outcome: Some("no_op".to_string()),
                summary: Some("纯指令输入，跳过记忆反思".to_string()),
                detail: None,
                run_started_at: Some(run_started.clone()),
                run_completed_at: Some(run_completed),
                tool_calls: vec![],
                messages: vec![ReflectionMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    content: "Input is command-only; reflection skipped".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                }],
            });
            return Ok(());
        }

        // 4. Recall-before-memorize: check if content is already covered
        // 只用 user_input 作为记忆源，不包含 assistant output
        // assistant 的回复只是对已有记忆的回顾，不应被当作新信息
        let content = user_input.to_string();

        // Only perform recall check if user input is long enough to be meaningful
        if trimmed_input.len() >= 5 {
            let query = extract_query_from_input(user_input);
            let query_msg = serde_json::json!({ "role": "user", "content": query });
            match memu.retrieve(vec![query_msg], None, None).await {
                Ok(retrieve_result) => {
                    let existing = &retrieve_result.items;
                    // Log detailed info about retrieved items for debugging
                    for (i, item) in existing.iter().enumerate().take(3) {
                        let summary = item
                            .get("summary")
                            .or_else(|| item.get("content"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("(empty)");
                        let preview_end = safe_char_boundary(summary, 100);
                        let score = item
                            .get("score")
                            .or_else(|| item.get("relevance"))
                            .and_then(|v| v.as_f64());
                        info!(
                            index = i,
                            score = ?score,
                            preview = &summary[..preview_end],
                            "reflection: retrieve item"
                        );
                    }
                    info!(
                        count = existing.len(),
                        "reflection: retrieve returned existing memories"
                    );
                    if !existing.is_empty() && is_covered_by_existing(existing, &content) {
                        info!("reflection: content already covered by existing memories, skipping memorize");
                        let run_completed = chrono::Utc::now().to_rfc3339();
                        self.emit_status(assistant_message_id, "completed");
                        self.emit_reflection(&ReflectionDetail {
                            assistant_message_id: assistant_message_id.to_string(),
                            status: "completed".to_string(),
                            outcome: Some("no_op".to_string()),
                            summary: Some("内容已被现有记忆覆盖，跳过记忆".to_string()),
                            detail: None,
                            run_started_at: Some(run_started.clone()),
                            run_completed_at: Some(run_completed),
                            tool_calls: vec![],
                            messages: vec![ReflectionMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                content: format!(
                                    "Content covered by {} existing memories; memorize skipped",
                                    existing.len()
                                ),
                                created_at: chrono::Utc::now().to_rfc3339(),
                            }],
                        });
                        return Ok(());
                    }
                }
                Err(e) => {
                    warn!("reflection: retrieve failed, proceeding with memorize: {}", e);
                }
            }
        }

        // 5. Call memU memorize() — only user_input, no assistant output
        let memorize_result = match memu.memorize(&content, "conversation", None).await {
            Ok(result) => result,
            Err(e) => {
                // Graceful degradation: treat memU errors as no_op, not failure.
                // This avoids showing a red "failed" badge in the UI when the
                // Python subprocess is unavailable (e.g. memu not installed).
                info!(error = %e, "reflection: memU memorize unavailable, degrading to no_op");
                let run_completed = chrono::Utc::now().to_rfc3339();
                self.emit_status(assistant_message_id, "completed");
                self.emit_reflection(&ReflectionDetail {
                    assistant_message_id: assistant_message_id.to_string(),
                    status: "completed".to_string(),
                    outcome: Some("no_op".to_string()),
                    summary: Some("memU 服务暂不可用，已跳过记忆反思".to_string()),
                    detail: None,
                    run_started_at: Some(run_started),
                    run_completed_at: Some(run_completed),
                    tool_calls: vec![],
                    messages: vec![ReflectionMessage {
                        id: uuid::Uuid::new_v4().to_string(),
                        content: format!("memU memorize unavailable ({}); reflection skipped", e),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    }],
                });
                return Ok(()); // Don't propagate — reflection failure is non-fatal
            }
        };

        // 6. Map memU items to graph model
        let items = &memorize_result.items;
        if items.is_empty() {
            // No memories extracted
            let run_completed = chrono::Utc::now().to_rfc3339();
            self.emit_status(assistant_message_id, "completed");
            self.emit_reflection(&ReflectionDetail {
                assistant_message_id: assistant_message_id.to_string(),
                status: "completed".to_string(),
                outcome: Some("no_op".to_string()),
                summary: Some("本轮对话已处理，未发现需要记忆的内容".to_string()),
                detail: None,
                run_started_at: Some(run_started),
                run_completed_at: Some(run_completed),
                tool_calls: vec![],
                messages: vec![ReflectionMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    content: "No memory items extracted from conversation".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                }],
            });
            return Ok(());
        }

        let mut tool_calls = Vec::new();
        let mut created_count = 0u32;
        let updated_count = 0u32;

        // Deduplicate items before processing to avoid creating redundant memory nodes
        let unique_items = deduplicate_items(items);
        info!(
            original_count = items.len(),
            deduped_count = unique_items.len(),
            "reflection: deduplicated memU items"
        );

        for item in unique_items {
            let now = chrono::Utc::now().to_rfc3339();

            // Extract fields from memU item (JSON value)
            let memu_type = item.get("memory_type")
                .or_else(|| item.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("knowledge");
            let summary = item.get("summary")
                .or_else(|| item.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let title = item.get("title")
                .or_else(|| item.get("key"))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    // Use first 50 chars of summary as title
                    if summary.len() > 50 { &summary[..50] } else { summary }
                });

            if summary.is_empty() {
                continue;
            }

            let kind = map_memu_type_to_kind(memu_type);
            let node_id = uuid::Uuid::new_v4().to_string();
            let version_id = uuid::Uuid::new_v4().to_string();
            let route_id = uuid::Uuid::new_v4().to_string();

            // Create MemoryNode
            let node = MemoryNode {
                id: node_id.clone(),
                space_id: space_id.to_string(),
                kind,
                title: title.to_string(),
                metadata: Some(serde_json::json!({
                    "source": "reflection",
                    "memu_type": memu_type,
                })),
                created_at: now.clone(),
                updated_at: now.clone(),
            };

            if let Err(e) = self.store.create_node(&node) {
                error!(error = %e, node_id = %node_id, "reflection: failed to create node");
                tool_calls.push(ReflectionToolCall {
                    id: node_id.clone(),
                    created_at: now.clone(),
                    name: "create_node".to_string(),
                    status: "error".to_string(),
                    parameters: Some(title.to_string()),
                    result_preview: None,
                    error: Some(e.to_string()),
                });
                continue;
            }

            // Create MemoryVersion
            let version = MemoryVersion {
                id: version_id.clone(),
                node_id: node_id.clone(),
                supersedes_version_id: None,
                status: MemoryVersionStatus::Active,
                content: summary.to_string(),
                metadata: None,
                embedding_json: None,
                created_at: now.clone(),
            };

            if let Err(e) = self.store.create_version(&version) {
                error!(error = %e, version_id = %version_id, "reflection: failed to create version");
            }

            // Create MemoryRoute
            let path = generate_route_path(kind, title);
            let route = MemoryRoute {
                id: route_id.clone(),
                space_id: space_id.to_string(),
                edge_id: None,
                node_id: node_id.clone(),
                domain: "core".to_string(),
                path,
                is_primary: true,
                created_at: now.clone(),
                updated_at: now.clone(),
            };

            if let Err(e) = self.store.create_route(&route) {
                error!(error = %e, route_id = %route_id, "reflection: failed to create route");
            }

            // Extract and create keywords
            let keywords = extract_keywords(summary);
            for kw in &keywords {
                let kw_entry = MemoryKeyword {
                    id: uuid::Uuid::new_v4().to_string(),
                    space_id: space_id.to_string(),
                    node_id: node_id.clone(),
                    keyword: kw.clone(),
                    created_at: now.clone(),
                };
                if let Err(e) = self.store.create_keyword(&kw_entry) {
                    error!(error = %e, keyword = %kw, "reflection: failed to create keyword");
                }
            }

            // Evaluate Boot set eligibility
            if is_boot_eligible(kind) {
                // Check if already in Boot set (by checking if kind is Boot)
                let already_boot = self.store.get_node(&node_id)
                    .ok()
                    .flatten()
                    .map(|n| n.kind == MemoryNodeKind::Boot)
                    .unwrap_or(false);

                if !already_boot {
                    let priority = match kind {
                        MemoryNodeKind::Identity => 100,
                        MemoryNodeKind::Value => 90,
                        MemoryNodeKind::Directive => 80,
                        _ => 50,
                    };
                    if let Err(e) = self.store.add_to_boot(space_id, &node_id, priority) {
                        error!(error = %e, node_id = %node_id, "reflection: failed to add to boot set");
                    } else {
                        info!(node_id = %node_id, title = %title, "reflection: added to boot set");
                    }
                }
            }

            created_count += 1;

            tool_calls.push(ReflectionToolCall {
                id: node_id.clone(),
                created_at: now.clone(),
                name: "create_node".to_string(),
                status: "completed".to_string(),
                parameters: Some(serde_json::json!({
                    "kind": kind.as_str(),
                    "title": title,
                }).to_string()),
                result_preview: Some(format!("Created {} node: {}", kind.as_str(), title)),
                error: None,
            });
        }

        // 7. Emit completion
        let run_completed = chrono::Utc::now().to_rfc3339();
        let outcome = if created_count > 0 || updated_count > 0 {
            if updated_count > 0 { "updated" } else { "created" }
        } else {
            "no_op"
        };

        let summary_text = if created_count > 0 || updated_count > 0 {
            format!(
                "反思完成：创建 {} 个记忆节点，更新 {} 个节点",
                created_count, updated_count
            )
        } else {
            "本轮对话已处理，未发现需要记忆的内容".to_string()
        };

        // Bundle 4 — fire the chip event before status. Only when we
        // actually created/updated nodes (no_op cases don't produce a
        // chip; the toast-style status panel already covers them).
        if created_count > 0 || updated_count > 0 {
            // Derive a category set from the memU items we processed. The
            // chip shows up to 3 categories; surfacing the actual memU
            // memory_type values (knowledge/profile/event/...) is more
            // informative than a hardcoded "reflection" tag.
            let mut categories: Vec<String> = items
                .iter()
                .filter_map(|item| {
                    item.get("memory_type")
                        .or_else(|| item.get("type"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            categories.sort();
            categories.dedup();
            self.emit_proactive_learning_chip(
                _conversation_id,
                (created_count as usize) + (updated_count as usize),
                categories,
                summary_text.clone(),
            );
        }

        self.emit_status(assistant_message_id, "completed");
        self.emit_reflection(&ReflectionDetail {
            assistant_message_id: assistant_message_id.to_string(),
            status: "completed".to_string(),
            outcome: Some(outcome.to_string()),
            summary: Some(summary_text),
            detail: None,
            run_started_at: Some(run_started),
            run_completed_at: Some(run_completed),
            tool_calls,
            messages: vec![ReflectionMessage {
                id: uuid::Uuid::new_v4().to_string(),
                content: format!(
                    "Reflection completed: {} created, {} updated from {} memU items",
                    created_count, updated_count, items.len()
                ),
                created_at: chrono::Utc::now().to_rfc3339(),
            }],
        });

        info!(
            created = created_count,
            updated = updated_count,
            total_items = items.len(),
            "reflection: completed successfully"
        );

        Ok(())
    }
}
