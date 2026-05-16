//! 用户偏好自动提取
//!
//! 从对话中识别用户偏好（代码风格、沟通方式、工具使用、UI 设计、语言等），
//! 创建或更新 UserProfile 类型的 MemoryNode。
//!
//! ## 设计
//! ```text
//! 用户消息 + Assistant 响应 → 偏好模式匹配
//!     ├─ 显式偏好: "我更喜欢..." "我不喜欢..." "请用..."
//!     ├─ 隐性偏好: 反复纠正、对特定方案的倾向
//!     └─ 工具偏好: "用 grep 而不是 find"
//!     ↓
//! 创建/更新 MemoryNode(kind=UserProfile)
//!     ├─ 冲突检测: 新旧偏好矛盾 → contradicts edge
//!     └─ 置信度累加: 多次确认 → confidence++
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::Error;
use crate::memory_graph::store::MemoryGraphStore;

// ─── 偏好类型 ─────────────────────────────────────────────────────────

/// 偏好类别
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PreferenceCategory {
    /// 代码风格偏好
    CodeStyle,
    /// 沟通方式偏好
    Communication,
    /// 工具使用偏好
    ToolUsage,
    /// UI 设计偏好
    UiDesign,
    /// 编程语言偏好
    Language,
    /// 通用偏好
    General,
}

impl PreferenceCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::CodeStyle => "code_style",
            Self::Communication => "communication",
            Self::ToolUsage => "tool_usage",
            Self::UiDesign => "ui_design",
            Self::Language => "language",
            Self::General => "general",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "code_style" => Self::CodeStyle,
            "communication" => Self::Communication,
            "tool_usage" => Self::ToolUsage,
            "ui_design" => Self::UiDesign,
            "language" => Self::Language,
            _ => Self::General,
        }
    }
}

// ─── 偏好项 ───────────────────────────────────────────────────────────

/// 单个偏好项
#[derive(Debug, Clone)]
pub struct PreferenceItem {
    /// 偏好类别
    pub category: PreferenceCategory,
    /// 偏好内容
    pub content: String,
    /// 置信度（0-1）
    pub confidence: f32,
    /// 被提及次数
    pub source_count: usize,
    /// 首次观察到的时间
    pub first_seen: String,
    /// 最近确认的时间
    pub last_confirmed: String,
    /// 关联的 pattern（用于匹配）
    pub pattern: Option<String>,
}

// ─── 冲突解决 ─────────────────────────────────────────────────────────

/// 冲突解决策略
#[derive(Debug, Clone)]
pub enum ConflictResolution {
    /// 新偏好覆盖旧偏好
    Supersede { reason: String },
    /// 合并两个偏好
    Merge { merged_content: String },
    /// 保留旧偏好（新偏好置信度不足）
    KeepExisting { reason: String },
}

// ─── 偏好提取器 ───────────────────────────────────────────────────────

/// 用户偏好自动提取器
pub struct PreferenceExtractor {
    store: Arc<MemoryGraphStore>,
    /// 已知偏好缓存（node_id → PreferenceItem）
    known_preferences: std::sync::Mutex<HashMap<String, PreferenceItem>>,
}

/// 显式偏好表达模式（中文 + 英文）
const EXPLICIT_PATTERNS: &[(&str, PreferenceCategory)] = &[
    // 中文模式
    ("我更喜欢", PreferenceCategory::General),
    ("我不喜欢", PreferenceCategory::General),
    ("我习惯", PreferenceCategory::General),
    ("请用", PreferenceCategory::ToolUsage),
    ("不要用", PreferenceCategory::ToolUsage),
    ("能不能用", PreferenceCategory::ToolUsage),
    ("我希望", PreferenceCategory::General),
    ("我倾向于", PreferenceCategory::General),
    ("对我来说", PreferenceCategory::General),
    ("我的偏好是", PreferenceCategory::General),
    ("用中文", PreferenceCategory::Language),
    ("用英文", PreferenceCategory::Language),
    ("用英文回答", PreferenceCategory::Language),
    ("用中文回答", PreferenceCategory::Language),
    ("简洁一点", PreferenceCategory::Communication),
    ("详细一点", PreferenceCategory::Communication),
    ("不要太啰嗦", PreferenceCategory::Communication),
    ("请直接", PreferenceCategory::Communication),
    // 英文模式
    ("I prefer", PreferenceCategory::General),
    ("I don't like", PreferenceCategory::General),
    ("I'd rather", PreferenceCategory::General),
    ("use simpler", PreferenceCategory::CodeStyle),
    ("more comments", PreferenceCategory::CodeStyle),
    ("less comments", PreferenceCategory::CodeStyle),
    ("functional style", PreferenceCategory::CodeStyle),
    ("object-oriented", PreferenceCategory::CodeStyle),
];

/// 隐性偏好模式（从 assistant 响应中推断）
const IMPLICIT_PATTERNS: &[(&str, PreferenceCategory)] = &[
    // 用户反复纠正同类问题 → 隐式偏好
    ("你误会了", PreferenceCategory::Communication),
    ("不对，我的意思是", PreferenceCategory::Communication),
    ("不要用", PreferenceCategory::ToolUsage),
    ("换一种方式", PreferenceCategory::Communication),
];

impl PreferenceExtractor {
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self {
            store,
            known_preferences: std::sync::Mutex::new(HashMap::new()),
        }
    }

    // ─── 偏好提取 ────────────────────────────────────────────────

    /// 从用户消息中提取偏好。
    ///
    /// 返回检测到的偏好项列表（可能为空）。
    pub fn extract_preferences(
        &self,
        user_message: &str,
        assistant_response: Option<&str>,
    ) -> Vec<PreferenceItem> {
        let mut preferences = Vec::new();
        let now = chrono::Utc::now().to_rfc3339();

        // 1. 显式偏好匹配
        for (pattern, category) in EXPLICIT_PATTERNS {
            if let Some(pos) = user_message.find(pattern) {
                // 提取 pattern 之后的内容作为偏好描述
                let start = pos + pattern.len();
                let end = user_message[start..]
                    .find(|c: char| c == '。' || c == '；' || c == '\n' || c == '，')
                    .map(|p| start + p)
                    .unwrap_or(user_message.len());

                let content = user_message[start..end].trim().to_string();

                if content.len() > 2 && content.len() < 200 {
                    preferences.push(PreferenceItem {
                        category: category.clone(),
                        content: format!("{} {}", pattern, content),
                        confidence: 0.6, // 单次提及的初始置信度
                        source_count: 1,
                        first_seen: now.clone(),
                        last_confirmed: now.clone(),
                        pattern: Some(pattern.to_string()),
                    });
                }
            }
        }

        // 2. 隐性偏好推断
        if assistant_response.is_some() {
            for (pattern, category) in IMPLICIT_PATTERNS {
                if user_message.contains(pattern) {
                    preferences.push(PreferenceItem {
                        category: category.clone(),
                        content: format!(
                            "用户对上一个回复不满意: {}",
                            &user_message[..user_message.len().min(100)]
                        ),
                        confidence: 0.4, // 隐性推断置信度较低
                        source_count: 1,
                        first_seen: now.clone(),
                        last_confirmed: now.clone(),
                        pattern: Some(pattern.to_string()),
                    });
                    break; // 只记录第一个隐性信号
                }
            }
        }

        // 3. 去重：每个 category 保留置信度最高的 MAX_PER_CATEGORY 个
        preferences.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        const MAX_PER_CATEGORY: usize = 3;
        let mut category_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        preferences.retain(|p| {
            let key = format!("{:?}", p.category);
            let count = category_counts.entry(key).or_insert(0);
            if *count < MAX_PER_CATEGORY {
                *count += 1;
                true
            } else {
                false
            }
        });

        preferences
    }

    // ─── 偏好存储 ────────────────────────────────────────────────

    /// 存储提取的偏好项到 MemoryGraph。
    ///
    /// 如果已存在同类别偏好，进行冲突检测和合并。
    pub fn store_preferences(
        &self,
        space_id: &str,
        preferences: &[PreferenceItem],
    ) -> Result<Vec<String>, Error> {
        let mut node_ids = Vec::new();
        let now = chrono::Utc::now().to_rfc3339();

        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        for pref in preferences {
            // 查找同类别已有偏好
            let existing = self.find_existing_preference(&conn, space_id, &pref.category)?;

            match existing {
                Some((existing_id, existing_content)) => {
                    // 冲突检测与合并
                    let resolution = self.detect_conflict(&existing_content, &pref.content);

                    match resolution {
                        ConflictResolution::Supersede { reason } => {
                            // 创建新版本
                            let version_id = uuid::Uuid::new_v4().to_string();
                            conn.execute(
                                "INSERT INTO memory_versions
                                 (id, node_id, content, status, embedding_json, created_at, updated_at)
                                 VALUES (?1, ?2, ?3, 'active', NULL, ?4, ?5)",
                                rusqlite::params![
                                    version_id,
                                    existing_id,
                                    pref.content,
                                    now,
                                    now,
                                ],
                            )
                            .map_err(|e| Error::Database(e))?;

                            // 标记旧版本为 superseded
                            conn.execute(
                                "UPDATE memory_versions SET status = 'superseded', updated_at = ?1
                                 WHERE node_id = ?2 AND status = 'active' AND id != ?3",
                                rusqlite::params![now, existing_id, version_id],
                            )
                            .map_err(|e| Error::Database(e))?;

                            // 更新 node 时间
                            conn.execute(
                                "UPDATE memory_nodes SET updated_at = ?1 WHERE id = ?2",
                                rusqlite::params![now, existing_id],
                            )
                            .map_err(|e| Error::Database(e))?;

                            tracing::info!(
                                node_id = %existing_id,
                                category = %pref.category.as_str(),
                                reason = %reason,
                                "[PreferenceExtractor] superseded existing preference"
                            );

                            node_ids.push(existing_id);
                        }
                        ConflictResolution::Merge { merged_content } => {
                            let version_id = uuid::Uuid::new_v4().to_string();
                            conn.execute(
                                "INSERT INTO memory_versions
                                 (id, node_id, content, status, embedding_json, created_at, updated_at)
                                 VALUES (?1, ?2, ?3, 'active', NULL, ?4, ?5)",
                                rusqlite::params![
                                    version_id,
                                    existing_id,
                                    merged_content,
                                    now,
                                    now,
                                ],
                            )
                            .map_err(|e| Error::Database(e))?;

                            conn.execute(
                                "UPDATE memory_nodes SET updated_at = ?1 WHERE id = ?2",
                                rusqlite::params![now, existing_id],
                            )
                            .map_err(|e| Error::Database(e))?;

                            node_ids.push(existing_id);
                        }
                        ConflictResolution::KeepExisting { reason } => {
                            tracing::debug!(
                                node_id = %existing_id,
                                reason = %reason,
                                "[PreferenceExtractor] kept existing preference"
                            );
                        }
                    }
                }
                None => {
                    // 创建新节点
                    let node_id = uuid::Uuid::new_v4().to_string();
                    let version_id = uuid::Uuid::new_v4().to_string();
                    let title = format!("用户偏好: {}", pref.category.as_str());

                    conn.execute(
                        "INSERT INTO memory_nodes
                         (id, space_id, kind, title, metadata_json, created_at, updated_at)
                         VALUES (?1, ?2, 'user_profile', ?3,
                                 json_object(
                                     'preference_category', ?4,
                                     'confidence', ?5,
                                     'source_count', ?6,
                                     'first_seen', ?7,
                                     'last_confirmed', ?8
                                 ),
                                 ?9, ?10)",
                        rusqlite::params![
                            node_id,
                            space_id,
                            title,
                            pref.category.as_str(),
                            pref.confidence,
                            pref.source_count,
                            pref.first_seen,
                            pref.last_confirmed,
                            now,
                            now,
                        ],
                    )
                    .map_err(|e| Error::Database(e))?;

                    conn.execute(
                        "INSERT INTO memory_versions
                         (id, node_id, content, status, embedding_json, created_at, updated_at)
                         VALUES (?1, ?2, ?3, 'active', NULL, ?4, ?5)",
                        rusqlite::params![version_id, node_id, pref.content, now, now],
                    )
                    .map_err(|e| Error::Database(e))?;

                    node_ids.push(node_id);
                }
            }
        }

        if !node_ids.is_empty() {
            tracing::info!(
                count = node_ids.len(),
                "[PreferenceExtractor] stored new/updated preferences"
            );
        }

        Ok(node_ids)
    }

    // ─── 冲突检测 ────────────────────────────────────────────────

    /// 检测新旧偏好是否冲突
    fn detect_conflict(
        &self,
        existing_content: &str,
        new_content: &str,
    ) -> ConflictResolution {
        // 简单策略：如果新内容包含否定词且旧内容被引用，则冲突
        let negations = ["不要", "不想", "不喜欢", "请改用", "换", "别用", "don't", "not"];

        let has_negation = negations.iter().any(|n| new_content.contains(n));
        let has_reference = existing_content
            .split_whitespace()
            .any(|w| w.len() > 2 && new_content.contains(w));

        if has_negation && has_reference {
            // 检测到潜在冲突 → 合并
            ConflictResolution::Merge {
                merged_content: format!(
                    "【最新偏好】{}\n【历史偏好】{}",
                    new_content, existing_content
                ),
            }
        } else if has_negation {
            // 否定但没有引用旧内容 → 覆盖
            ConflictResolution::Supersede {
                reason: "user expressed contrary preference".to_string(),
            }
        } else {
            // 无冲突 → 合并增强
            ConflictResolution::Merge {
                merged_content: format!(
                    "{}\n补充: {}",
                    existing_content, new_content
                ),
            }
        }
    }

    /// 查找同类别已有偏好节点
    fn find_existing_preference(
        &self,
        conn: &rusqlite::Connection,
        space_id: &str,
        category: &PreferenceCategory,
    ) -> Result<Option<(String, String)>, Error> {
        let mut stmt = conn
            .prepare(
                "SELECT n.id, v.content
                 FROM memory_nodes n
                 JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                 WHERE n.space_id = ?1
                   AND n.kind = 'user_profile'
                   AND json_extract(n.metadata_json, '$.preference_category') = ?2
                 LIMIT 1",
            )
            .map_err(|e| Error::Database(e))?;

        let result = stmt
            .query_row(
                rusqlite::params![space_id, category.as_str()],
                |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                },
            )
            .ok();

        Ok(result)
    }

    // ─── 偏好查询 ────────────────────────────────────────────────

    /// 获取用户所有偏好
    pub fn list_preferences(
        &self,
        space_id: &str,
    ) -> Result<Vec<PreferenceItem>, Error> {
        Self::list_preferences_inner(&self.store, space_id)
    }

    /// 异步版本：获取用户所有偏好
    pub async fn list_preferences_async(
        &self,
        space_id: &str,
    ) -> Result<Vec<PreferenceItem>, Error> {
        let store = self.store.clone();
        let space = space_id.to_string();
        tokio::task::spawn_blocking(move || {
            Self::list_preferences_inner(&store, &space)
        })
        .await
        .map_err(|e| Error::Internal(format!("spawn_blocking join error: {}", e)))?
    }

    /// 内部实现：复用的 list_preferences 逻辑
    fn list_preferences_inner(
        store: &Arc<MemoryGraphStore>,
        space_id: &str,
    ) -> Result<Vec<PreferenceItem>, Error> {
        let conn = store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT n.metadata_json, v.content
                 FROM memory_nodes n
                 JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                 WHERE n.space_id = ?1 AND n.kind = 'user_profile'
                 ORDER BY n.updated_at DESC",
            )
            .map_err(|e| Error::Database(e))?;

        let items: Vec<PreferenceItem> = stmt
            .query_map(rusqlite::params![space_id], |row| {
                let metadata_str: String = row.get(0)?;
                let content: String = row.get(1)?;
                let meta: serde_json::Value =
                    serde_json::from_str(&metadata_str).unwrap_or_default();

                Ok(PreferenceItem {
                    category: PreferenceCategory::from_str(
                        meta.get("preference_category")
                            .and_then(|v| v.as_str())
                            .unwrap_or("general"),
                    ),
                    content,
                    confidence: meta
                        .get("confidence")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.5) as f32,
                    source_count: meta
                        .get("source_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1) as usize,
                    first_seen: meta
                        .get("first_seen")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    last_confirmed: meta
                        .get("last_confirmed")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    pattern: None,
                })
            })
            .map_err(|e| Error::Database(e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }
}
