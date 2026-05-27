use std::sync::Arc;
use std::time::Duration;
use rusqlite::Connection;
use tokio::task::JoinHandle;
use tracing::{info, warn, error};
use tauri_plugin_notification::NotificationExt;
use chrono::{Local, NaiveTime};
use tauri::Manager;

use crate::app::AppState;
use crate::agent::types::ChatMessage;
use crate::llm::{create_provider, CompletionConfig};

pub struct DailySummaryService {
    app_handle: tauri::AppHandle,
    db: Arc<std::sync::Mutex<Connection>>,
    summary_hour: u32,
}

impl DailySummaryService {
    pub fn new(app_handle: tauri::AppHandle, db: Arc<std::sync::Mutex<Connection>>, summary_hour: u32) -> Self {
        Self { app_handle, db, summary_hour }
    }

    pub fn start(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let sleep_duration = self.duration_until_next_trigger();
                info!("[DailySummary] Next trigger in {} seconds", sleep_duration.as_secs());
                tokio::time::sleep(sleep_duration).await;
                self.generate_daily_summary().await;
            }
        })
    }

    fn duration_until_next_trigger(&self) -> Duration {
        let now = Local::now();
        let target_time = NaiveTime::from_hms_opt(self.summary_hour, 0, 0)
            .unwrap_or_else(|| NaiveTime::from_hms_opt(9, 0, 0).unwrap());

        let today_target = now.date_naive().and_time(target_time);
        let next_trigger = if now.naive_local() >= today_target {
            // 今天已过，明天触发
            today_target + chrono::Duration::days(1)
        } else {
            today_target
        };

        let diff = next_trigger - now.naive_local();
        Duration::from_secs(diff.num_seconds().max(60) as u64)
    }

    async fn generate_daily_summary(&self) {
        // 1. 计算"昨天"的时间范围 (本地时区)
        let yesterday = Local::now().date_naive() - chrono::Duration::days(1);
        let start_ms = yesterday
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let end_ms = yesterday
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc()
            .timestamp_millis();

        let date_str = yesterday.format("%Y-%m-%d").to_string();

        // 2. 查询昨天的碎片
        let fragments: Vec<(String, String)> = {
            let conn = match self.db.lock() {
                Ok(c) => c,
                Err(e) => {
                    warn!("[DailySummary] DB lock failed: {}", e);
                    return;
                }
            };

            // 检查是否已生成（避免重复）
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM daily_summaries WHERE summary_date = ?1",
                rusqlite::params![date_str],
                |r| r.get(0),
            ).unwrap_or(false);

            if exists {
                info!("[DailySummary] Summary for {} already exists, skipping", date_str);
                return;
            }

            // 查询昨天的碎片节点（尝试整数时间戳和字符串时间戳两种方式）
            let sql = "SELECT mn.id, mv.content FROM memory_nodes mn
                 JOIN memory_versions mv ON mv.node_id = mn.id AND mv.status = 'active'
                 WHERE mn.kind = 'Episode'
                 AND json_extract(mn.metadata_json, '$.subtype') = 'fragment'
                 AND mn.created_at BETWEEN ?1 AND ?2
                 ORDER BY mn.created_at ASC";

            // 先尝试用整数时间戳查询
            let result = conn.prepare(sql)
                .and_then(|mut stmt| {
                    let rows = stmt.query_map(rusqlite::params![start_ms, end_ms], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?;
                    Ok(rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
                });

            match result {
                Ok(rows) if !rows.is_empty() => rows,
                _ => {
                    // 回退到字符串日期范围查询
                    let start_str = format!("{}T00:00:00", date_str);
                    let end_str = format!("{}T23:59:59", date_str);
                    conn.prepare(sql)
                        .and_then(|mut stmt| {
                            let rows = stmt.query_map(rusqlite::params![start_str, end_str], |row| {
                                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                            })?;
                            Ok(rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
                        })
                        .unwrap_or_default()
                }
            }
        };

        if fragments.is_empty() {
            info!("[DailySummary] No fragments for {}, skipping", date_str);
            return;
        }

        info!("[DailySummary] Found {} fragments for {}", fragments.len(), date_str);

        // 3. 调用 LLM 生成摘要
        let state = match self.app_handle.try_state::<AppState>() {
            Some(s) => s,
            None => {
                warn!("[DailySummary] AppState not available");
                return;
            }
        };

        let provider_service = &state.provider_service;
        let llm_params = match provider_service.get_active_llm_config().await {
            Some(params) => params,
            None => {
                info!("[DailySummary] No active LLM configured, storing raw summary");
                // 无 LLM 时存一个简单列表
                self.store_fallback_summary(&date_str, &fragments);
                return;
            }
        };

        let (provider_id, model, api_key, base_url, _api) = llm_params;
        let llm_config = crate::config::llm::LlmConfig {
            provider: provider_id,
            model: model.clone(),
            api_key,
            base_url: if base_url.is_empty() { None } else { Some(base_url) },
            max_tokens: Some(2048),
            temperature: Some(0.5),
            api: None,
        };

        let provider = match create_provider(&llm_config) {
            Ok(p) => p,
            Err(e) => {
                warn!("[DailySummary] Failed to create LLM provider: {}", e);
                self.store_fallback_summary(&date_str, &fragments);
                return;
            }
        };

        // 构造摘要 prompt
        let fragment_list: String = fragments.iter().enumerate().map(|(i, (_, content))| {
            format!("{}. {}", i + 1, content.chars().take(200).collect::<String>())
        }).collect::<Vec<_>>().join("\n");

        let user_msg = format!(
            "请为以下{}条记忆碎片生成一份简洁的每日摘要（不超过200字），突出关键事项和要点：\n\n{}",
            fragments.len(),
            fragment_list,
        );

        let messages = vec![
            ChatMessage::system("你是一个记忆摘要助手。请生成简洁准确的每日记忆摘要，帮助用户快速回顾昨天记录的内容。只输出摘要文本，不要有其他格式。"),
            ChatMessage::user(&user_msg),
        ];

        let config = CompletionConfig {
            model,
            max_tokens: 512,
            temperature: 0.5,
            thinking_enabled: false,
        };

        let summary_content = match provider.complete(messages, vec![], &config).await {
            Ok(response) => {
                match &response {
                    crate::agent::types::RespondOutput::Text { text, .. } => text.clone(),
                    crate::agent::types::RespondOutput::ToolCalls { text, .. } => {
                        text.clone().unwrap_or_else(|| "摘要生成失败".to_string())
                    }
                }
            }
            Err(e) => {
                warn!("[DailySummary] LLM call failed: {}", e);
                // fallback: 简单拼接
                format!("今日记录了{}条记忆碎片。", fragments.len())
            }
        };

        // 4. 存储到 daily_summaries 表
        let fragment_ids: Vec<String> = fragments.iter().map(|(id, _)| id.clone()).collect();
        self.store_summary(&date_str, &summary_content, fragments.len() as i32, &fragment_ids);

        // 5. 发送 OS 通知
        let _ = self.app_handle.notification()
            .builder()
            .title("昨日记忆摘要")
            .body(&summary_content.chars().take(100).collect::<String>())
            .show();

        info!("[DailySummary] Generated and stored summary for {}", date_str);
    }

    fn store_summary(&self, date_str: &str, content: &str, count: i32, fragment_ids: &[String]) {
        let now = chrono::Utc::now().timestamp_millis();
        let id = uuid::Uuid::new_v4().to_string();
        let ids_json = serde_json::to_string(fragment_ids).unwrap_or_else(|_| "[]".to_string());

        if let Ok(conn) = self.db.lock() {
            if let Err(e) = conn.execute(
                "INSERT INTO daily_summaries (id, summary_date, content, fragment_count, fragment_ids_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![id, date_str, content, count, ids_json, now],
            ) {
                error!("[DailySummary] Failed to store summary: {}", e);
            }
        }
    }

    fn store_fallback_summary(&self, date_str: &str, fragments: &[(String, String)]) {
        let content = format!("今日记录了{}条记忆碎片。", fragments.len());
        let fragment_ids: Vec<String> = fragments.iter().map(|(id, _)| id.clone()).collect();
        self.store_summary(date_str, &content, fragments.len() as i32, &fragment_ids);
    }
}
