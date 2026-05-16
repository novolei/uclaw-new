//! 记忆提取持久化存储
//!
//! 使用独立的 SQLite 数据库文件（`~/.uclaw/memorization.db`）存储：
//! - 未记忆的消息队列（`memorization_queue` 表）
//! - 任务状态快照（`memorization_state` 表）
//!
//! 所有方法均通过 `Mutex<Connection>` 保证线程安全。

use std::sync::Mutex;

use rusqlite::{params, Connection};

use super::types::*;

/// 记忆提取持久化存储
///
/// 使用 SQLite 存储未记忆的消息队列和任务状态，
/// 确保进程重启后可以恢复未完成的提取任务。
pub struct MemorizationStorage {
    conn: Mutex<Connection>,
}

impl MemorizationStorage {
    /// 创建存储并初始化表结构
    ///
    /// # Arguments
    /// * `db_path` - SQLite 数据库文件路径（通常为 `~/.uclaw/memorization.db`）
    pub fn new(db_path: &std::path::Path) -> anyhow::Result<Self> {
        // 确保父目录存在
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;

        // Enable WAL mode for better concurrent access (matches main DB)
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", "5000")?;

        Self::ensure_tables(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 初始化表结构（幂等）
    fn ensure_tables(conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memorization_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                platform TEXT NOT NULL DEFAULT 'local',
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                conversation_id TEXT,
                space_id TEXT,
                timestamp INTEGER NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS memorization_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
        ",
        )?;
        Ok(())
    }

    /// 将一条消息追加到持久化队列
    pub fn append_message(&self, msg: &UnmemorizedMessage) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("锁获取失败: {}", e))?;
        conn.execute(
            "INSERT INTO memorization_queue (platform, role, content, conversation_id, space_id, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                msg.platform,
                msg.role,
                msg.content,
                msg.conversation_id,
                msg.space_id,
                msg.timestamp,
            ],
        )?;
        Ok(())
    }

    /// 获取队列中所有未记忆的消息（按时间正序）
    pub fn get_queue(&self) -> anyhow::Result<Vec<UnmemorizedMessage>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("锁获取失败: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT id, platform, role, content, conversation_id, space_id, timestamp
             FROM memorization_queue
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(UnmemorizedMessage {
                id: row.get(0)?,
                platform: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                conversation_id: row.get(4)?,
                space_id: row.get(5)?,
                timestamp: row.get(6)?,
            })
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }

    /// 获取队列中的消息总数
    pub fn get_count(&self) -> anyhow::Result<usize> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("锁获取失败: {}", e))?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memorization_queue",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// 清除队列中最早的 count 条消息
    ///
    /// 使用子查询按 id 升序选出最早的 N 条记录进行删除。
    pub fn clear_queue(&self, count: usize) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("锁获取失败: {}", e))?;
        conn.execute(
            "DELETE FROM memorization_queue WHERE id IN (
                SELECT id FROM memorization_queue ORDER BY id ASC LIMIT ?1
            )",
            params![count as i64],
        )?;
        Ok(())
    }

    /// 保存当前提取任务的状态（用于崩溃恢复）
    ///
    /// # Arguments
    /// * `task_id` - 任务唯一标识
    /// * `message_count` - 本次任务处理的消息数量
    pub fn save_task_state(&self, task_id: &str, message_count: usize) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("锁获取失败: {}", e))?;
        let now = chrono::Utc::now().timestamp_millis();
        let value = serde_json::json!({
            "task_id": task_id,
            "message_count": message_count,
            "started_at": now,
        });
        conn.execute(
            "INSERT OR REPLACE INTO memorization_state (key, value, updated_at)
             VALUES ('pending_task', ?1, CURRENT_TIMESTAMP)",
            params![value.to_string()],
        )?;
        Ok(())
    }

    /// 获取上次未完成的任务状态（如有）
    pub fn get_pending_task(&self) -> anyhow::Result<Option<PendingTask>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("锁获取失败: {}", e))?;
        let result: Result<String, _> = conn.query_row(
            "SELECT value FROM memorization_state WHERE key = 'pending_task'",
            [],
            |row| row.get(0),
        );
        match result {
            Ok(value_str) => {
                let task: PendingTask = serde_json::from_str(&value_str)?;
                Ok(Some(task))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 清除任务状态（任务完成或恢复后调用）
    pub fn clear_task_state(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("锁获取失败: {}", e))?;
        conn.execute(
            "DELETE FROM memorization_state WHERE key = 'pending_task'",
            [],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// 创建临时数据库用于测试
    fn test_storage() -> (MemorizationStorage, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test_memorization.db");
        let storage = MemorizationStorage::new(&db_path).unwrap();
        (storage, dir)
    }

    /// 辅助方法：构造一条测试消息
    fn make_msg(role: &str, content: &str) -> UnmemorizedMessage {
        UnmemorizedMessage {
            id: 0,
            platform: "test".to_string(),
            role: role.to_string(),
            content: content.to_string(),
            conversation_id: Some("conv1".to_string()),
            space_id: Some("space1".to_string()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    #[test]
    fn test_append_and_get_queue() {
        let (storage, _dir) = test_storage();
        storage.append_message(&make_msg("user", "你好")).unwrap();
        storage.append_message(&make_msg("assistant", "你好！有什么可以帮你的？")).unwrap();

        let queue = storage.get_queue().unwrap();
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].role, "user");
        assert_eq!(queue[1].role, "assistant");
    }

    #[test]
    fn test_get_count() {
        let (storage, _dir) = test_storage();
        assert_eq!(storage.get_count().unwrap(), 0);

        storage.append_message(&make_msg("user", "消息1")).unwrap();
        storage.append_message(&make_msg("user", "消息2")).unwrap();
        assert_eq!(storage.get_count().unwrap(), 2);
    }

    #[test]
    fn test_clear_queue() {
        let (storage, _dir) = test_storage();
        for i in 0..5 {
            storage.append_message(&make_msg("user", &format!("消息{}", i))).unwrap();
        }
        assert_eq!(storage.get_count().unwrap(), 5);

        // 清除最早的 3 条
        storage.clear_queue(3).unwrap();
        assert_eq!(storage.get_count().unwrap(), 2);

        // 剩余的应该是 消息3 和 消息4
        let queue = storage.get_queue().unwrap();
        assert_eq!(queue[0].content, "消息3");
        assert_eq!(queue[1].content, "消息4");
    }

    #[test]
    fn test_task_state_lifecycle() {
        let (storage, _dir) = test_storage();

        // 初始无任务
        assert!(storage.get_pending_task().unwrap().is_none());

        // 保存任务状态
        storage.save_task_state("task-001", 15).unwrap();
        let task = storage.get_pending_task().unwrap().unwrap();
        assert_eq!(task.task_id, "task-001");
        assert_eq!(task.message_count, 15);

        // 清除任务状态
        storage.clear_task_state().unwrap();
        assert!(storage.get_pending_task().unwrap().is_none());
    }
}
