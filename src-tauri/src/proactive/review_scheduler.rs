use std::sync::Arc;
use std::time::Duration;
use rusqlite::Connection;
use tokio::task::JoinHandle;
use tracing::{info, warn, error};
use tauri_plugin_notification::NotificationExt;

/// 艾宾浩斯间隔序列 (毫秒)
const REVIEW_INTERVALS_MS: [i64; 4] = [
    3_600_000,    // 1小时
    86_400_000,   // 1天
    259_200_000,  // 3天
    604_800_000,  // 7天
];

pub struct ReviewScheduler {
    app_handle: tauri::AppHandle,
    db: Arc<std::sync::Mutex<Connection>>,
}

impl ReviewScheduler {
    pub fn new(app_handle: tauri::AppHandle, db: Arc<std::sync::Mutex<Connection>>) -> Self {
        Self { app_handle, db }
    }

    /// 启动定时轮询 (每 5 分钟检查一次到期复习)
    pub fn start(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            // 启动时先补发所有到期提醒
            self.check_due_reviews().await;

            let mut interval = tokio::time::interval(Duration::from_secs(300));
            loop {
                interval.tick().await;
                self.check_due_reviews().await;
            }
        })
    }

    async fn check_due_reviews(&self) {
        let now = chrono::Utc::now().timestamp_millis();

        // 从 db 查询到期的复习记录
        let due_reviews = {
            let conn = match self.db.lock() {
                Ok(c) => c,
                Err(e) => {
                    warn!("[ReviewScheduler] DB lock failed: {}", e);
                    return;
                }
            };

            let mut stmt = match conn.prepare(
                "SELECT fr.id, fr.node_id, fr.review_count, mn.title, mv.content
                 FROM fragment_reviews fr
                 JOIN memory_nodes mn ON mn.id = fr.node_id
                 LEFT JOIN memory_versions mv ON mv.node_id = fr.node_id AND mv.status = 'active'
                 WHERE fr.next_review_at <= ?1 AND fr.completed = 0
                 ORDER BY fr.next_review_at ASC
                 LIMIT 10"
            ) {
                Ok(s) => s,
                Err(e) => {
                    warn!("[ReviewScheduler] Failed to prepare query: {}", e);
                    return;
                }
            };

            let rows = stmt.query_map(rusqlite::params![now], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i32>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            });

            match rows {
                Ok(mapped) => mapped
                    .filter_map(|r| r.ok())
                    .collect::<Vec<_>>(),
                Err(e) => {
                    warn!("[ReviewScheduler] Query failed: {}", e);
                    return;
                }
            }
        };

        if due_reviews.is_empty() {
            return;
        }

        info!("[ReviewScheduler] Found {} due reviews", due_reviews.len());

        for (review_id, _node_id, review_count, title, content) in due_reviews {
            // 1. 发送 OS 通知
            let notify_title = "记忆复习提醒";
            let notify_body = title.unwrap_or_else(|| {
                content
                    .unwrap_or_default()
                    .chars()
                    .take(50)
                    .collect::<String>()
            });

            let _ = self.app_handle.notification()
                .builder()
                .title(notify_title)
                .body(&notify_body)
                .show();

            // 2. 更新复习记录
            let new_count = review_count + 1;
            let completed = new_count >= 4;
            let next_review_at: Option<i64> = if completed {
                None
            } else {
                Some(now + REVIEW_INTERVALS_MS[new_count as usize])
            };

            if let Ok(conn) = self.db.lock() {
                if let Err(e) = conn.execute(
                    "UPDATE fragment_reviews SET review_count = ?1, next_review_at = ?2, last_reviewed_at = ?3, completed = ?4 WHERE id = ?5",
                    rusqlite::params![new_count, next_review_at, now, completed as i32, review_id],
                ) {
                    error!("[ReviewScheduler] Failed to update review {}: {}", review_id, e);
                }
            }
        }
    }
}

/// 创建新的复习计划 (碎片保存后调用)
pub fn schedule_review(conn: &Connection, node_id: &str) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp_millis();
    let first_review = now + REVIEW_INTERVALS_MS[0]; // 1小时后
    let id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO fragment_reviews (id, node_id, review_count, next_review_at, created_at) VALUES (?1, ?2, 0, ?3, ?4)",
        rusqlite::params![id, node_id, first_review, now],
    ).map_err(|e| format!("Failed to schedule review: {}", e))?;

    info!("[ReviewScheduler] Scheduled review for node {} at +1h", node_id);
    Ok(())
}
