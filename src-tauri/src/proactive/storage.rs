//! 主动服务消息存储
//!
//! 基于 SQLite 持久化主动服务生成的消息，
//! 供 AgentService 重新加载上下文或前端展示历史主动消息时使用。

use std::sync::Mutex;

use rusqlite::Connection;

use super::types::ProactiveMessage;

// ─── ProactiveStorage ─────────────────────────────────────────────────

/// 主动服务消息存储
///
/// 使用独立的 SQLite 数据库文件 `~/.uclaw/proactive.db`，
/// 与主数据库隔离，避免互相干扰。
pub struct ProactiveStorage {
    /// SQLite 连接（Mutex 保护线程安全）
    pub(crate) conn: Mutex<Connection>,
}

impl ProactiveStorage {
    /// 创建新的 ProactiveStorage 实例
    ///
    /// - `db_path`: 数据库文件路径（如 `~/.uclaw/proactive.db`）
    /// - 自动创建表结构（如不存在）
    pub fn new(db_path: &std::path::Path) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;
        Self::ensure_tables(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 确保所需的数据表存在
    fn ensure_tables(conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS proactive_messages (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                generated_at TEXT NOT NULL,
                trigger_reason TEXT NOT NULL DEFAULT '',
                tools_used TEXT NOT NULL DEFAULT '[]',
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_proactive_messages_created_at
                ON proactive_messages(created_at);
            ",
        )?;
        Ok(())
    }

    /// 保存一条主动消息
    pub fn save_message(&self, msg: &ProactiveMessage) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("获取数据库锁失败: {}", e))?;

        let tools_json = serde_json::to_string(&msg.tools_used)?;

        conn.execute(
            "INSERT OR REPLACE INTO proactive_messages (id, content, generated_at, trigger_reason, tools_used)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                msg.id,
                msg.content,
                msg.generated_at,
                msg.trigger_reason,
                tools_json,
            ],
        )?;
        Ok(())
    }

    /// 获取最近的主动消息
    ///
    /// - `limit`: 最多返回的消息数量
    /// - 按 `created_at` 倒序排列（最新在前）
    pub fn get_recent_messages(&self, limit: usize) -> anyhow::Result<Vec<ProactiveMessage>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("获取数据库锁失败: {}", e))?;

        let mut stmt = conn.prepare(
            "SELECT id, content, generated_at, trigger_reason, tools_used
             FROM proactive_messages
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;

        let messages = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                let tools_json: String = row.get(4)?;
                let tools_used: Vec<String> =
                    serde_json::from_str(&tools_json).unwrap_or_default();

                Ok(ProactiveMessage {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    generated_at: row.get(2)?,
                    trigger_reason: row.get(3)?,
                    tools_used,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(messages)
    }

    /// 清理旧消息，只保留最近 `keep_count` 条
    ///
    /// 避免数据库无限增长，在每次 tick 后定期调用。
    pub fn clear_old_messages(&self, keep_count: usize) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("获取数据库锁失败: {}", e))?;

        conn.execute(
            "DELETE FROM proactive_messages
             WHERE id NOT IN (
                 SELECT id FROM proactive_messages
                 ORDER BY created_at DESC
                 LIMIT ?1
             )",
            rusqlite::params![keep_count as i64],
        )?;
        Ok(())
    }
}

// ─── 单元测试 ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建临时内存数据库用于测试
    fn make_test_storage() -> ProactiveStorage {
        let conn = Connection::open_in_memory().unwrap();
        ProactiveStorage::ensure_tables(&conn).unwrap();
        ProactiveStorage {
            conn: Mutex::new(conn),
        }
    }

    fn make_msg(id: &str, content: &str) -> ProactiveMessage {
        ProactiveMessage {
            id: id.to_string(),
            content: content.to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            trigger_reason: "测试触发".to_string(),
            tools_used: vec!["tool_a".to_string()],
        }
    }

    #[test]
    fn test_save_and_get_messages() {
        let storage = make_test_storage();
        storage.save_message(&make_msg("m1", "消息1")).unwrap();
        storage.save_message(&make_msg("m2", "消息2")).unwrap();

        let msgs = storage.get_recent_messages(10).unwrap();
        assert_eq!(msgs.len(), 2);
        // 最新在前
        assert_eq!(msgs[0].id, "m2");
        assert_eq!(msgs[1].id, "m1");
    }

    #[test]
    fn test_get_recent_respects_limit() {
        let storage = make_test_storage();
        for i in 0..5 {
            storage
                .save_message(&make_msg(&format!("m{}", i), &format!("内容{}", i)))
                .unwrap();
        }

        let msgs = storage.get_recent_messages(3).unwrap();
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn test_clear_old_messages() {
        let storage = make_test_storage();
        for i in 0..10 {
            storage
                .save_message(&make_msg(&format!("m{}", i), &format!("内容{}", i)))
                .unwrap();
        }

        storage.clear_old_messages(3).unwrap();
        let msgs = storage.get_recent_messages(100).unwrap();
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn test_upsert_on_duplicate_id() {
        let storage = make_test_storage();
        storage.save_message(&make_msg("m1", "原始内容")).unwrap();
        storage.save_message(&make_msg("m1", "更新内容")).unwrap();

        let msgs = storage.get_recent_messages(10).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "更新内容");
    }
}
