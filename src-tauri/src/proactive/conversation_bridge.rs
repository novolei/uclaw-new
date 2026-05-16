//! 双轨会话统一桥接
//!
//! 将传统会话（conversations + messages）桥接到记忆系统，
//! 解决两套会话系统（conversations vs agent_sessions）导致的记忆碎片化问题。
//!
//! ## 设计
//! ```text
//! conversations + messages → 读取最近对话
//!     ↓
//! 转换为 ConversationMessage 格式
//!     ↓
//! 入队到 memorization_queue（标记 source="conversation"）
//!     ↓
//! MemorizationService 统一处理 → memU 提取 → MemoryGraph
//! ```

use std::sync::Arc;

use rusqlite::params;

use crate::error::Error;
use crate::memory_graph::store::MemoryGraphStore;

// ─── 桥接消息格式 ─────────────────────────────────────────────────────

/// 统一消息表示（兼容 conversations 和 agent_sessions）
#[derive(Debug, Clone)]
pub struct BridgedMessage {
    /// 消息角色: "user" | "assistant" | "system"
    pub role: String,
    /// 消息内容
    pub content: String,
    /// 消息时间（ISO 8601）
    pub timestamp: String,
    /// 会话 ID
    pub conversation_id: String,
    /// 工作区 ID
    pub space_id: String,
    /// 消息来源: "conversation" | "agent_session"
    pub source: String,
}

/// 桥接会话摘要
#[derive(Debug, Clone)]
pub struct BridgedConversation {
    /// 会话 ID
    pub conversation_id: String,
    /// 会话标题或首条消息摘要
    pub title: String,
    /// 消息列表（按时间排序）
    pub messages: Vec<BridgedMessage>,
    /// 消息总数（原始表中的）
    pub total_messages: usize,
    /// 最后活跃时间
    pub last_active_at: String,
    /// 来源
    pub source: String,
}

// ─── 桥接统计 ─────────────────────────────────────────────────────────

/// 桥接操作统计
#[derive(Debug, Clone, Default)]
pub struct BridgeStats {
    /// 扫描的传统会话数
    pub conversations_scanned: usize,
    /// 有未桥接消息的会话数
    pub conversations_with_new: usize,
    /// 入队的消息总数
    pub messages_enqueued: usize,
    /// 跳过的消息数（已桥接或空消息）
    pub messages_skipped: usize,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
}

// ─── 双轨桥接器 ───────────────────────────────────────────────────────

/// 双轨会话统一桥接器
///
/// 将从 `conversations` + `messages` 表中读取的对话，
/// 桥接到 `memorization_queue`，供 `MemorizationService` 统一处理。
pub struct ConversationBridge {
    store: Arc<MemoryGraphStore>,
    /// 每个会话最后桥接的消息时间（用于增量同步）
    last_bridge_ts: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl ConversationBridge {
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self {
            store,
            last_bridge_ts: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    // ─── 读取传统会话 ────────────────────────────────────────────

    /// 读取指定空间下的活跃传统会话
    pub fn list_active_conversations(
        &self,
        space_id: &str,
        limit: usize,
    ) -> Result<Vec<BridgedConversation>, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        // 读取 conversations 表（假设存在于同一 DB）
        let mut stmt = conn
            .prepare(
                "SELECT id, title, created_at, updated_at
                 FROM conversations
                 WHERE space_id = ?1
                   AND archived = 0
                 ORDER BY updated_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| Error::Database(e))?;

        let conv_rows: Vec<(String, String, String, String)> = stmt
            .query_map(params![space_id, limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)
                        .unwrap_or_else(|_| "Untitled".to_string()),
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| Error::Database(e))?
            .filter_map(|r| r.ok())
            .collect();

        let mut conversations = Vec::new();

        for (conv_id, title, _created_at, updated_at) in conv_rows {
            let messages = self.read_conversation_messages(&conn, &conv_id, 50)?;
            let total = self.count_messages(&conn, &conv_id)?;

            conversations.push(BridgedConversation {
                conversation_id: conv_id,
                title,
                messages,
                total_messages: total,
                last_active_at: updated_at,
                source: "conversation".to_string(),
            });
        }

        Ok(conversations)
    }

    /// 读取指定会话的消息
    fn read_conversation_messages(
        &self,
        conn: &rusqlite::Connection,
        conversation_id: &str,
        limit: usize,
    ) -> Result<Vec<BridgedMessage>, Error> {
        let mut stmt = conn
            .prepare(
                "SELECT role, content, created_at
                 FROM messages
                 WHERE conversation_id = ?1
                 ORDER BY created_at ASC
                 LIMIT ?2",
            )
            .map_err(|e| Error::Database(e))?;

        let messages: Vec<BridgedMessage> = stmt
            .query_map(params![conversation_id, limit as i64], |row| {
                Ok(BridgedMessage {
                    role: row.get::<_, String>(0)?,
                    content: row.get::<_, String>(1)?,
                    timestamp: row.get::<_, String>(2)?,
                    conversation_id: conversation_id.to_string(),
                    space_id: String::new(), // 由调用者填充
                    source: "conversation".to_string(),
                })
            })
            .map_err(|e| Error::Database(e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(messages)
    }

    /// 统计会话消息总数
    fn count_messages(
        &self,
        conn: &rusqlite::Connection,
        conversation_id: &str,
    ) -> Result<usize, Error> {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
                params![conversation_id],
                |row| row.get(0),
            )
            .map_err(|e| Error::Database(e))?;
        Ok(count as usize)
    }

    // ─── 增量桥接 ────────────────────────────────────────────────

    /// 增量桥接：将最新的传统对话入队到 memorization_queue。
    ///
    /// 只处理上次桥接之后新增的消息。
    pub fn bridge_incremental(
        &self,
        space_id: &str,
        max_messages_per_conversation: usize,
    ) -> Result<BridgeStats, Error> {
        let start = std::time::Instant::now();
        let mut stats = BridgeStats::default();

        let conversations = self.list_active_conversations(space_id, 20)?;
        stats.conversations_scanned = conversations.len();

        for conv in &conversations {
            // 获取上次桥接时间
            let last_ts = {
                let map = self
                    .last_bridge_ts
                    .lock()
                    .map_err(|e| Error::Internal(format!("Mutex lock: {}", e)))?;
                map.get(&conv.conversation_id).cloned()
            };

            // 过滤新增消息
            let new_messages: Vec<&BridgedMessage> = if let Some(ref ts) = last_ts {
                conv.messages
                    .iter()
                    .filter(|m| m.timestamp.as_str() > ts.as_str())
                    .take(max_messages_per_conversation)
                    .collect()
            } else {
                conv.messages
                    .iter()
                    .take(max_messages_per_conversation)
                    .collect()
            };

            if !new_messages.is_empty() {
                stats.conversations_with_new += 1;

                // 入队到 memorization_queue
                for msg in &new_messages {
                    match self.enqueue_message(space_id, msg) {
                        Ok(()) => stats.messages_enqueued += 1,
                        Err(e) => {
                            tracing::warn!(
                                conv_id = %conv.conversation_id,
                                error = %e,
                                "Failed to enqueue bridged message"
                            );
                            stats.messages_skipped += 1;
                        }
                    }
                }

                // 更新最后桥接时间
                if let Some(last_msg) = new_messages.last() {
                    let mut map = self
                        .last_bridge_ts
                        .lock()
                        .map_err(|e| Error::Internal(format!("Mutex lock: {}", e)))?;
                    map.insert(
                        conv.conversation_id.clone(),
                        last_msg.timestamp.clone(),
                    );
                }
            }
        }

        stats.duration_ms = start.elapsed().as_millis() as u64;

        if stats.messages_enqueued > 0 {
            tracing::info!(
                enqueued = stats.messages_enqueued,
                conversations = stats.conversations_with_new,
                duration_ms = stats.duration_ms,
                "[ConversationBridge] bridge_incremental completed"
            );
        }

        Ok(stats)
    }

    /// 将单条桥接消息入队到 memorization_queue
    fn enqueue_message(
        &self,
        space_id: &str,
        msg: &BridgedMessage,
    ) -> Result<(), Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO memorization_queue
             (platform, role, content, conversation_id, space_id, timestamp, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                "conversation_bridge",
                msg.role,
                msg.content,
                msg.conversation_id,
                space_id,
                now,
                serde_json::json!({
                    "source": "conversation",
                    "original_timestamp": msg.timestamp,
                })
                .to_string(),
            ],
        )
        .map_err(|e| Error::Database(e))?;

        Ok(())
    }

    // ─── 全量桥接 ────────────────────────────────────────────────

    /// 全量桥接：将指定会话的所有消息入队（不受 last_bridge_ts 限制）。
    ///
    /// 用于首次桥接或手动同步。
    pub fn bridge_full(
        &self,
        space_id: &str,
        conversation_id: &str,
    ) -> Result<BridgeStats, Error> {
        let start = std::time::Instant::now();
        let mut stats = BridgeStats::default();

        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let messages = self.read_conversation_messages(&conn, conversation_id, 200)?;
        stats.conversations_scanned = 1;

        for msg in &messages {
            let mut msg_with_space = msg.clone();
            msg_with_space.space_id = space_id.to_string();

            match self.enqueue_message(space_id, &msg_with_space) {
                Ok(()) => stats.messages_enqueued += 1,
                Err(e) => {
                    tracing::warn!(
                        conv_id = %conversation_id,
                        error = %e,
                        "Failed to enqueue bridged message"
                    );
                    stats.messages_skipped += 1;
                }
            }
        }

        if !messages.is_empty() {
            stats.conversations_with_new = 1;
            // 更新最后桥接时间
            if let Some(last_msg) = messages.last() {
                let mut map = self
                    .last_bridge_ts
                    .lock()
                    .map_err(|e| Error::Internal(format!("Mutex lock: {}", e)))?;
                map.insert(conversation_id.to_string(), last_msg.timestamp.clone());
            }
        }

        stats.duration_ms = start.elapsed().as_millis() as u64;
        Ok(stats)
    }
}
