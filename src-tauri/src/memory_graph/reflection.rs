use std::sync::Arc;

use tauri::Emitter;
use tracing::{error, info};

use super::models::*;
use super::store::MemoryGraphStore;
use crate::agent::types::{ReflectionDetail, ReflectionMessage, ReflectionToolCall};

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

/// Find a char-boundary–safe slice end for preview strings.
fn safe_char_boundary(s: &str, max_bytes: usize) -> usize {
    let mut end = s.len().min(max_bytes);
    while end < s.len() && !s.is_char_boundary(end) {
        end += 1;
    }
    end.min(s.len())
}

/// Check if a node kind qualifies for the Boot set.
fn is_boot_eligible(kind: MemoryNodeKind) -> bool {
    matches!(
        kind,
        MemoryNodeKind::Identity | MemoryNodeKind::Value | MemoryNodeKind::Directive
    )
}

/// Persist extracted memory items as memory_graph nodes (node + version +
/// route + keywords + Boot eligibility). Shared by ReflectionOrchestrator and
/// ProactiveService. Returns the count of nodes created.
///
/// `tool_calls` is populated with per-item success/error entries so that
/// `reflect()` can forward them to `emit_reflection` without re-iterating.
/// This keeps the free-function signature free of `&self` while preserving
/// identical behavior to the original inline loop.
pub fn persist_items_to_graph(
    store: &MemoryGraphStore,
    space_id: &str,
    items: &[crate::memory_graph::extractor::ExtractedItem],
    tool_calls: &mut Vec<crate::agent::types::ReflectionToolCall>,
) -> anyhow::Result<usize> {
    let mut created_count = 0usize;

    for item in items {
        let now = chrono::Utc::now().to_rfc3339();

        let memu_type = item.memory_type.as_str();
        let summary = item.content.as_str();

        // Derive title: use first 50 bytes of content (byte-safe boundary)
        let title_owned: String;
        let title: &str = {
            if summary.len() > 50 {
                let mut end = 50;
                while end < summary.len() && !summary.is_char_boundary(end) {
                    end += 1;
                }
                title_owned = summary[..end].to_string();
                &title_owned
            } else {
                summary
            }
        };

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

        if let Err(e) = store.create_node(&node) {
            error!(error = %e, node_id = %node_id, "persist_items: failed to create node");
            tool_calls.push(crate::agent::types::ReflectionToolCall {
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

        if let Err(e) = store.create_version(&version) {
            error!(error = %e, version_id = %version_id, "persist_items: failed to create version");
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

        if let Err(e) = store.create_route(&route) {
            error!(error = %e, route_id = %route_id, "persist_items: failed to create route");
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
            if let Err(e) = store.create_keyword(&kw_entry) {
                error!(error = %e, keyword = %kw, "persist_items: failed to create keyword");
            }
        }

        // Evaluate Boot set eligibility
        if is_boot_eligible(kind) {
            // Check if already in Boot set (by checking if kind is Boot)
            let already_boot = store.get_node(&node_id)
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
                if let Err(e) = store.add_to_boot(space_id, &node_id, priority) {
                    error!(error = %e, node_id = %node_id, "persist_items: failed to add to boot set");
                } else {
                    info!(node_id = %node_id, title = %title, "persist_items: added to boot set");
                }
            }
        }

        created_count += 1;

        tool_calls.push(crate::agent::types::ReflectionToolCall {
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

    Ok(created_count)
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
    extractor: std::sync::Arc<crate::memory_graph::extractor::MemoryExtractor>,
    bucket_seal_adapter: std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>,
    app_handle: tauri::AppHandle,
}

impl ReflectionOrchestrator {
    pub fn new(
        store: Arc<MemoryGraphStore>,
        extractor: std::sync::Arc<crate::memory_graph::extractor::MemoryExtractor>,
        bucket_seal_adapter: std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>,
        app_handle: tauri::AppHandle,
    ) -> Self {
        Self {
            store,
            extractor,
            bucket_seal_adapter,
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

        // Only perform recall check if user input is long enough to be meaningful.
        // Uses bucket_seal hybrid recall instead of memU retrieve.
        // Fail-open: recall_hybrid is infallible; empty results → proceed to extract.
        if trimmed_input.len() >= 5 {
            let query = extract_query_from_input(user_input);
            let hits = self.bucket_seal_adapter.recall_hybrid(&query, None, 3).await;
            // Mirror old skip heuristic: skip only on a strong existing match (score >= 0.9).
            let already_covered = hits
                .first()
                .map(|e| e.score.unwrap_or(0.0) >= 0.9)
                .unwrap_or(false);
            for (i, e) in hits.iter().enumerate().take(3) {
                let preview_end = safe_char_boundary(&e.content, 100);
                info!(
                    index = i,
                    score = ?e.score,
                    preview = &e.content[..preview_end],
                    "reflection: bucket_seal recall hit"
                );
            }
            info!(
                count = hits.len(),
                already_covered,
                "reflection: bucket_seal recall check"
            );
            if already_covered {
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
                            "Content covered by {} existing memories (score >= 0.9); memorize skipped",
                            hits.len()
                        ),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    }],
                });
                return Ok(());
            }
        }

        // 5. Native extractor — only user_input, no assistant output
        let items = self.extractor.extract(&content).await;

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
        let updated_count = 0usize;

        // items is already Vec<ExtractedItem> — persist directly, no shim needed.
        info!(
            item_count = items.len(),
            "reflection: native extractor returned items"
        );

        let created_count = persist_items_to_graph(&self.store, space_id, &items, &mut tool_calls)?;

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
            // Derive a category set from the extracted items. The chip shows
            // up to 3 categories; surfacing the memory_type values
            // (knowledge/profile/event/...) is more informative than a
            // hardcoded "reflection" tag.
            let mut categories: Vec<String> = items
                .iter()
                .map(|item| item.memory_type.clone())
                .collect();
            categories.sort();
            categories.dedup();
            self.emit_proactive_learning_chip(
                _conversation_id,
                created_count + updated_count,
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
                    "Reflection completed: {} created, {} updated from {} extracted items",
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::memory_graph::extractor::ExtractedItem;
    use crate::memory_graph::models::MemoryNodeKind;

    /// Spin up an in-memory SQLite store with V4 graph schema (same helper
    /// pattern as `store::tests::fresh_test_store`).
    fn fresh_store() -> MemoryGraphStore {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).expect("V4 schema");
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1).expect("V35 schema");
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        MemoryGraphStore::new(Arc::new(Mutex::new(conn)))
    }

    #[test]
    fn persist_items_maps_kinds_correctly() {
        let store = fresh_store();
        let items = vec![
            ExtractedItem { memory_type: "profile".to_string(),   content: "Enjoys table tennis".to_string() },
            ExtractedItem { memory_type: "event".to_string(),     content: "Has a competition tomorrow".to_string() },
            ExtractedItem { memory_type: "skill".to_string(),     content: "Can write Rust async code".to_string() },
        ];
        let mut tool_calls = Vec::new();
        let count = persist_items_to_graph(&store, "test-space", &items, &mut tool_calls)
            .expect("persist_items_to_graph should succeed");

        assert_eq!(count, 3, "three items should be created");
        assert_eq!(tool_calls.len(), 3);
        assert!(tool_calls.iter().all(|tc| tc.status == "completed"));

        // Verify node kinds via store lookup
        // Find created node ids from tool_calls
        for tc in &tool_calls {
            let node = store.get_node(&tc.id).unwrap().expect("node must exist");
            let expected_kind = if tc.result_preview.as_deref().unwrap_or("").contains("user_profile") {
                MemoryNodeKind::UserProfile
            } else if tc.result_preview.as_deref().unwrap_or("").contains("episode") {
                MemoryNodeKind::Episode
            } else if tc.result_preview.as_deref().unwrap_or("").contains("procedure") {
                MemoryNodeKind::Procedure
            } else {
                continue;
            };
            assert_eq!(node.kind, expected_kind, "kind mismatch for node {}", tc.id);
        }
    }

    #[test]
    fn persist_items_kind_per_type() {
        let store = fresh_store();
        let items = vec![
            ExtractedItem { memory_type: "profile".to_string(),   content: "Prefers dark mode".to_string() },
            ExtractedItem { memory_type: "event".to_string(),     content: "Attended a team meeting".to_string() },
            ExtractedItem { memory_type: "knowledge".to_string(), content: "Rust ownership model prevents data races".to_string() },
            ExtractedItem { memory_type: "behavior".to_string(),  content: "Reviews PRs every morning".to_string() },
            ExtractedItem { memory_type: "skill".to_string(),     content: "Uses cargo-watch for live reloading".to_string() },
        ];
        let mut tool_calls = Vec::new();
        persist_items_to_graph(&store, "space-kind", &items, &mut tool_calls)
            .expect("persist must succeed");

        // Fetch all created nodes and map title -> kind
        let kind_map: std::collections::HashMap<String, MemoryNodeKind> = tool_calls
            .iter()
            .filter_map(|tc| store.get_node(&tc.id).ok().flatten().map(|n| (n.title.clone(), n.kind)))
            .collect();

        // profile → UserProfile
        assert_eq!(kind_map.get("Prefers dark mode").copied(), Some(MemoryNodeKind::UserProfile));
        // event → Episode
        assert_eq!(kind_map.get("Attended a team meeting").copied(), Some(MemoryNodeKind::Episode));
        // skill → Procedure
        assert!(kind_map.values().any(|k| *k == MemoryNodeKind::Procedure || *k == MemoryNodeKind::Boot),
            "expected at least one Procedure node");
    }

    #[test]
    fn persist_items_boot_eligible_kinds_added_to_boot() {
        let store = fresh_store();
        // Identity, Value, Directive are boot-eligible; map from behavior/unknown
        // Actually Identity/Value/Directive are direct kind matches. memU types
        // "behavior" → Directive, which is boot-eligible.
        let items = vec![
            ExtractedItem { memory_type: "behavior".to_string(), content: "Always writes tests first".to_string() },
        ];
        let mut tool_calls = Vec::new();
        let count = persist_items_to_graph(&store, "boot-space", &items, &mut tool_calls)
            .expect("persist must succeed");
        assert_eq!(count, 1);

        // The behavior → Directive node should be added to boot (kind flipped to Boot)
        let node = store.get_node(&tool_calls[0].id).unwrap().expect("node must exist");
        assert_eq!(node.kind, MemoryNodeKind::Boot,
            "behavior->Directive item should be boot-eligible and have kind=Boot");
    }

    #[test]
    fn persist_items_creates_active_version() {
        let store = fresh_store();
        let items = vec![
            ExtractedItem { memory_type: "knowledge".to_string(), content: "SQLite supports WAL mode".to_string() },
        ];
        let mut tool_calls = Vec::new();
        persist_items_to_graph(&store, "ver-space", &items, &mut tool_calls)
            .expect("persist must succeed");

        let node_id = &tool_calls[0].id;
        let version = store.get_active_version(node_id).unwrap();
        assert!(version.is_some(), "active version must exist");
        assert_eq!(version.unwrap().content, "SQLite supports WAL mode");
    }

    #[test]
    fn persist_items_empty_content_skipped() {
        let store = fresh_store();
        let items = vec![
            ExtractedItem { memory_type: "profile".to_string(), content: "".to_string() },
            ExtractedItem { memory_type: "event".to_string(),   content: "Concert next week".to_string() },
        ];
        let mut tool_calls = Vec::new();
        let count = persist_items_to_graph(&store, "skip-space", &items, &mut tool_calls)
            .expect("persist must succeed");
        // Empty-content item is skipped; only the event is persisted
        assert_eq!(count, 1);
    }
}
