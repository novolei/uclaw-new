//! 记忆质量反馈闭环
//!
//! 在 LLM 调用完成后记录引用的记忆，更新 cited_count / last_cited_at，
//! 并周期性清理低质量记忆（cited_count=0 且 age>30d）。
//!
//! ## 设计
//! ```text
//! LLM 响应 → 提取引用的记忆节点 ID → record_citation()
//!                                          │
//!                              更新 metadata_json:
//!                                $.cited_count += 1
//!                                $.last_cited_at = now
//!                                $.usage_count   += 1  (bump)
//!                                          │
//!                              periodic_cleanup() 清理低质量记忆
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::Error;
use crate::memory_graph::store::MemoryGraphStore;

// ─── 记忆引用记录 ─────────────────────────────────────────────────────

/// 单条记忆引用记录
#[derive(Debug, Clone)]
pub struct MemoryCitation {
    /// 被引用的记忆节点 ID
    pub node_id: String,
    /// LLM 是否认为该记忆对回答有帮助
    pub was_useful: bool,
    /// 使用场景（如 "code generation", "debugging"）
    pub context: String,
    /// 引用时间（ISO 8601）
    pub cited_at: String,
}

// ─── 清理统计 ─────────────────────────────────────────────────────────

/// 周期性清理结果
#[derive(Debug, Clone)]
pub struct CleanupStats {
    /// 扫描的节点总数
    pub scanned: usize,
    /// 因低质量被删除的节点数
    pub removed: usize,
    /// 因 aged 被归档的节点数
    pub archived: usize,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
}

// ─── 反馈管理器 ───────────────────────────────────────────────────────

/// 记忆质量反馈管理器
///
/// 生命周期：
/// 1. 每次 LLM 响应后调用 `record_citation`
/// 2. 定期（默认每小时）调用 `periodic_cleanup`
pub struct FeedbackManager {
    store: Arc<MemoryGraphStore>,
    /// 上次清理时间
    last_cleanup: std::sync::Mutex<Instant>,
    /// 清理间隔
    cleanup_interval: Duration,
}

impl FeedbackManager {
    /// 创建新的反馈管理器
    ///
    /// - `store`: MemoryGraph 存储
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self {
            store,
            last_cleanup: std::sync::Mutex::new(Instant::now()),
            cleanup_interval: Duration::from_secs(3600), // 1 小时
        }
    }

    /// 设置清理间隔
    pub fn with_cleanup_interval(mut self, interval: Duration) -> Self {
        self.cleanup_interval = interval;
        self
    }

    // ─── 引用记录 ─────────────────────────────────────────────────

    /// 记录记忆引用：更新被引用节点的 cited_count 和 last_cited_at。
    ///
    /// 同时 bump usage_count（引用也是一种使用）。
    pub fn record_citation(&self, citation: &MemoryCitation) -> Result<(), Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let now = chrono::Utc::now().to_rfc3339();

        // 更新 metadata_json: cited_count += 1, last_cited_at = now
        conn.execute(
            "UPDATE memory_nodes
             SET metadata_json = json_set(
                     COALESCE(metadata_json, '{}'),
                     '$.cited_count',
                     COALESCE(json_extract(metadata_json, '$.cited_count'), 0) + 1,
                     '$.last_cited_at',
                     ?1,
                     '$.usage_count',
                     COALESCE(json_extract(metadata_json, '$.usage_count'), 0) + 1
                 ),
                 updated_at = ?2
             WHERE id = ?3",
            rusqlite::params![now, now, citation.node_id],
        )
        .map_err(|e| Error::Database(e))?;

        Ok(())
    }

    /// 批量记录记忆引用
    pub fn record_citations(&self, citations: &[MemoryCitation]) -> Result<usize, Error> {
        let mut count = 0;
        for citation in citations {
            match self.record_citation(citation) {
                Ok(()) => count += 1,
                Err(e) => {
                    tracing::warn!(
                        node_id = %citation.node_id,
                        error = %e,
                        "Failed to record citation"
                    );
                }
            }
        }
        Ok(count)
    }

    /// 标记记忆为"未帮助"：衰减其分数（cited_count 不增加，仅 usage_count+1）
    pub fn mark_not_useful(&self, node_id: &str) -> Result<(), Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let now = chrono::Utc::now().to_rfc3339();

        // 不增加 cited_count，只更新 usage_count 和 updated_at
        conn.execute(
            "UPDATE memory_nodes
             SET metadata_json = json_set(
                     COALESCE(metadata_json, '{}'),
                     '$.usage_count',
                     COALESCE(json_extract(metadata_json, '$.usage_count'), 0) + 1
                 ),
                 updated_at = ?1
             WHERE id = ?2",
            rusqlite::params![now, node_id],
        )
        .map_err(|e| Error::Database(e))?;

        Ok(())
    }

    // ─── 清理 ─────────────────────────────────────────────────────

    /// 周期性清理低质量记忆。
    ///
    /// 清理条件：
    /// - cited_count = 0 或 NULL
    /// - usage_count < 2
    /// - age > 30 天
    /// - 非 Boot/Identity/Value 节点（核心节点不清理）
    ///
    /// 如果距离上次清理不足 cleanup_interval，则跳过。
    pub fn periodic_cleanup(&self, space_id: &str) -> Result<CleanupStats, Error> {
        let now = chrono::Utc::now();
        let cutoff = (now - chrono::Duration::days(30)).to_rfc3339();

        // 检查清理间隔
        {
            let mut last = self.last_cleanup.lock().map_err(|e| {
                Error::Internal(format!("Mutex lock: {}", e))
            })?;
            if last.elapsed() < self.cleanup_interval {
                return Ok(CleanupStats {
                    scanned: 0,
                    removed: 0,
                    archived: 0,
                    duration_ms: 0,
                });
            }
            *last = Instant::now();
        }

        let start = std::time::Instant::now();

        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        // 查找低质量节点（排除核心类型）
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, json_extract(metadata_json, '$.cited_count') as cc,
                        json_extract(metadata_json, '$.usage_count') as uc
                 FROM memory_nodes
                 WHERE space_id = ?1
                   AND created_at < ?2
                   AND kind NOT IN ('boot', 'identity', 'value')
                   AND (
                       json_extract(metadata_json, '$.cited_count') IS NULL
                       OR CAST(json_extract(metadata_json, '$.cited_count') AS INTEGER) = 0
                   )
                   AND (
                       json_extract(metadata_json, '$.usage_count') IS NULL
                       OR CAST(json_extract(metadata_json, '$.usage_count') AS INTEGER) < 2
                   )",
            )
            .map_err(|e| Error::Database(e))?;

        let rows = stmt
            .query_map(rusqlite::params![space_id, cutoff], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            })
            .map_err(|e| Error::Database(e))?;

        let candidates: Vec<(String, String, Option<i64>, Option<i64>)> =
            rows.filter_map(|r| r.ok()).collect();

        let scanned = candidates.len();

        // 归档低质量节点（而非直接删除，保留可追溯性）
        let now_str = now.to_rfc3339();
        let mut archived = 0;

        for (node_id, _kind, _cc, _uc) in &candidates {
            // 标记为 archived
            let result = conn.execute(
                "UPDATE memory_nodes
                 SET metadata_json = json_set(
                         COALESCE(metadata_json, '{}'),
                         '$.auto_archived', 'true',
                         '$.archived_at', ?1,
                         '$.archive_reason', 'low_quality_cleanup'
                     ),
                     updated_at = ?2
                 WHERE id = ?3",
                rusqlite::params![now_str, now_str, node_id],
            );
            if result.is_ok() {
                archived += 1;
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        if archived > 0 {
            tracing::info!(
                scanned,
                archived,
                duration_ms,
                "[FeedbackManager] periodic_cleanup completed"
            );
        }

        Ok(CleanupStats {
            scanned,
            removed: 0, // 暂不硬删除
            archived,
            duration_ms,
        })
    }

    /// 立即清理（不受 interval 限制，用于手动触发）
    pub fn force_cleanup(&self, space_id: &str) -> Result<CleanupStats, Error> {
        // 临时重置 last_cleanup 时间
        {
            let mut last = self.last_cleanup.lock().map_err(|e| {
                Error::Internal(format!("Mutex lock: {}", e))
            })?;
            *last = Instant::now()
                .checked_sub(self.cleanup_interval)
                .unwrap_or(Instant::now());
        }
        self.periodic_cleanup(space_id)
    }

    // ─── 统计 ─────────────────────────────────────────────────────

    /// 获取节点的引用统计
    pub fn get_node_stats(&self, node_id: &str) -> Result<Option<CitationStats>, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT
                    json_extract(metadata_json, '$.cited_count') as cc,
                    json_extract(metadata_json, '$.usage_count') as uc,
                    json_extract(metadata_json, '$.last_cited_at') as lca,
                    created_at
                 FROM memory_nodes
                 WHERE id = ?1",
            )
            .map_err(|e| Error::Database(e))?;

        let result = stmt
            .query_row(rusqlite::params![node_id], |row| {
                Ok(CitationStats {
                    cited_count: row.get::<_, Option<i64>>(0)?.unwrap_or(0) as u32,
                    usage_count: row.get::<_, Option<i64>>(1)?.unwrap_or(0) as u32,
                    last_cited_at: row.get::<_, Option<String>>(2)?,
                    created_at: row.get::<_, String>(3)?,
                })
            })
            .ok();

        Ok(result)
    }
}

// ─── 辅助类型 ─────────────────────────────────────────────────────────

/// 节点引用统计
#[derive(Debug, Clone)]
pub struct CitationStats {
    pub cited_count: u32,
    pub usage_count: u32,
    pub last_cited_at: Option<String>,
    pub created_at: String,
}

impl CitationStats {
    /// 记忆质量分（0-1）
    /// 考虑引用次数 + 近期使用 + 时间衰减
    pub fn quality_score(&self, now: &chrono::DateTime<chrono::Utc>) -> f32 {
        let cite_score = (self.cited_count as f32 * 0.6).min(1.0);

        let days_since_created = {
            let created = chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or(*now);
            (*now - created).num_hours() as f32 / 24.0
        };

        let recency_bonus = if let Some(ref lca) = self.last_cited_at {
            let last = chrono::DateTime::parse_from_rfc3339(lca)
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or(*now);
            let days = (*now - last).num_hours() as f32 / 24.0;
            if days < 7.0 {
                0.3
            } else if days < 30.0 {
                0.15
            } else {
                0.0
            }
        } else {
            0.0
        };

        let age_decay = (-days_since_created / 90.0).exp();

        (cite_score + recency_bonus) * age_decay
    }
}
