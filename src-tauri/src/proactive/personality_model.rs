//! 长期人格/行为模式建模
//!
//! 分析用户的交互历史，构建人格画像，包括：
//! - 沟通风格偏好（正式/随意/技术性）
//! - 决策模式（快速/谨慎/数据驱动）
//! - 技术栈偏好
//! - 工作模式（活跃时段）
//!
//! ## 设计
//! ```text
//! 用户交互历史 → update_personality_profile()
//!     ├─ 收集 UserProfile/Directive/Behavior 节点
//!     ├─ 分析模式
//!     │   ├─ 消息长度 → 详细 vs 简洁
//!     │   ├─ 技术术语密度 → 技术性 vs 通用
//!     │   ├─ 纠正频率 → 精确性要求
//!     │   └─ 工具使用偏好
//!     └─ 创建/更新 Boot 节点（Identity/Value/Directive）
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Datelike, Timelike};
use rusqlite::params;

use crate::error::Error;
use crate::memory_graph::store::MemoryGraphStore;

// ─── 沟通风格 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunicationStyle {
    /// 正式
    Formal,
    /// 随意
    Casual,
    /// 技术性
    Technical,
    /// 混合
    Mixed,
}

impl CommunicationStyle {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Formal => "formal",
            Self::Casual => "casual",
            Self::Technical => "technical",
            Self::Mixed => "mixed",
        }
    }
}

// ─── 决策模式 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionPattern {
    /// 快速决策
    Quick,
    /// 谨慎决策
    Cautious,
    /// 数据驱动
    DataDriven,
    /// 未确定
    Unknown,
}

impl DecisionPattern {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Quick => "quick",
            Self::Cautious => "cautious",
            Self::DataDriven => "data_driven",
            Self::Unknown => "unknown",
        }
    }
}

// ─── 技术偏好 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TechPreference {
    pub category: String,  // "language", "framework", "tool", "platform"
    pub name: String,
    pub frequency: usize,
    pub last_used: String,
}

// ─── 工作模式 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WorkPatterns {
    /// 活跃时段（按小时分布）
    pub active_hours: Vec<usize>,
    /// 平均会话时长（分钟）
    pub avg_session_minutes: f64,
    /// 每日平均消息数
    pub avg_daily_messages: f64,
    /// 偏好工作日
    pub preferred_days: Vec<usize>, // 0=Sun, 1=Mon, ..., 6=Sat
}

// ─── 人格画像 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PersonalityProfile {
    /// 沟通风格
    pub communication_style: CommunicationStyle,
    /// 决策模式
    pub decision_pattern: DecisionPattern,
    /// 技术偏好
    pub tech_preferences: Vec<TechPreference>,
    /// 工作模式
    pub work_patterns: WorkPatterns,
    /// 核心价值观（从 Value 类型节点聚合）
    pub core_values: Vec<String>,
    /// 更新时间
    pub updated_at: String,
}

// ─── 行为信号 ─────────────────────────────────────────────────────────

/// 近期行为信号（用于一致性检查）
#[derive(Debug, Clone)]
pub struct BehaviorSignal {
    /// 消息文本
    pub message_text: String,
    /// 消息长度（字符数）
    pub message_length: usize,
    /// 技术术语密度（0-1）
    pub tech_terms_density: f32,
    /// 是否包含纠正
    pub has_correction: bool,
    /// 使用的工具
    pub tools_used: Vec<String>,
    /// 时间戳
    pub timestamp: String,
}

// ─── 一致性警告 ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConsistencyWarning {
    pub category: String,
    pub existing_behavior: String,
    pub recent_behavior: String,
    pub severity: String, // "info" | "warning" | "critical"
}

// ─── 人格行为模式管理器 ───────────────────────────────────────────────

/// 人格行为模式管理器
pub struct PersonalityModel {
    store: Arc<MemoryGraphStore>,
}

impl PersonalityModel {
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self { store }
    }

    // ─── 人格画像更新 ────────────────────────────────────────────

    /// 更新人格画像。
    ///
    /// 从 UserProfile/Directive/Value 节点聚合分析用户行为模式。
    pub fn update_personality_profile(
        &self,
        space_id: &str,
    ) -> Result<PersonalityProfile, Error> {
        let now = chrono::Utc::now().to_rfc3339();

        // 1. 聚合现有偏好节点
        let communication_style = self
            .analyze_communication_style(space_id)
            .unwrap_or(CommunicationStyle::Mixed);
        let decision_pattern = self
            .analyze_decision_pattern(space_id)
            .unwrap_or(DecisionPattern::Unknown);
        let tech_preferences = self.analyze_tech_preferences(space_id)?;
        let work_patterns = self.analyze_work_patterns(space_id)?;
        let core_values = self.collect_core_values(space_id)?;

        let profile = PersonalityProfile {
            communication_style,
            decision_pattern,
            tech_preferences,
            work_patterns,
            core_values,
            updated_at: now.clone(),
        };

        // 2. 更新/创建 Boot 节点
        self.upsert_personality_boot_node(space_id, &profile)?;

        tracing::info!("[PersonalityModel] updated personality profile");

        Ok(profile)
    }

    /// 获取当前人格画像
    pub fn get_profile(
        &self,
        space_id: &str,
    ) -> Result<Option<PersonalityProfile>, Error> {
        self.update_personality_profile(space_id).map(Some)
    }

    // ─── 行为一致性检查 ──────────────────────────────────────────

    /// 检查近期行为与长期画像的一致性。
    pub fn check_behavior_consistency(
        &self,
        profile: &PersonalityProfile,
        recent_behavior: &[BehaviorSignal],
    ) -> Vec<ConsistencyWarning> {
        let mut warnings = Vec::new();

        // 检查沟通风格一致性
        let recent_style = self.infer_style_from_signals(recent_behavior);
        if recent_style != profile.communication_style
            && recent_style != CommunicationStyle::Mixed
        {
            warnings.push(ConsistencyWarning {
                category: "communication_style".to_string(),
                existing_behavior: format!(
                    "长期风格: {}",
                    profile.communication_style.as_str()
                ),
                recent_behavior: format!("近期表现: {}", recent_style.as_str()),
                severity: "warning".to_string(),
            });
        }

        // 检查纠正频率（如果近期频繁纠正，可能偏好发生了变化）
        let correction_rate = recent_behavior
            .iter()
            .filter(|s| s.has_correction)
            .count() as f32
            / recent_behavior.len().max(1) as f32;

        if correction_rate > 0.3 {
            warnings.push(ConsistencyWarning {
                category: "correction_frequency".to_string(),
                existing_behavior: "低纠正率".to_string(),
                recent_behavior: format!("纠正率: {:.0}%", correction_rate * 100.0),
                severity: "info".to_string(),
            });
        }

        // 检查消息长度变化
        let avg_recent_len: f64 = recent_behavior
            .iter()
            .map(|s| s.message_length as f64)
            .sum::<f64>()
            / recent_behavior.len().max(1) as f64;

        if avg_recent_len < 20.0 && recent_behavior.len() >= 3 {
            warnings.push(ConsistencyWarning {
                category: "message_length".to_string(),
                existing_behavior: "正常消息长度".to_string(),
                recent_behavior: format!("近期消息偏短: {:.0} 字符", avg_recent_len),
                severity: "info".to_string(),
            });
        }

        warnings
    }

    /// 从行为信号中提取行为特征
    pub fn extract_behavior_signals(
        &self,
        messages: &[crate::agent::types::ChatMessage],
        tools_used: &[String],
    ) -> Vec<BehaviorSignal> {
        messages
            .iter()
            .filter(|m| matches!(m.role, crate::agent::types::MessageRole::User))
            .map(|m| {
                // Extract text from ContentBlock::Text variants
                let text: String = m
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        crate::agent::types::ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                let length = text.len();
                let tech_terms = count_tech_terms(&text);
                let density = if length > 0 {
                    (tech_terms as f32 / length as f32 * 100.0).min(1.0)
                } else {
                    0.0
                };

                BehaviorSignal {
                    message_text: text.clone(),
                    message_length: length,
                    tech_terms_density: density,
                    has_correction: is_correction_message(&text),
                    tools_used: tools_used.to_vec(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                }
            })
            .collect()
    }

    // ─── 内部分析方法 ────────────────────────────────────────────

    fn analyze_communication_style(
        &self,
        space_id: &str,
    ) -> Result<CommunicationStyle, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        // 检查 UserProfile 节点中是否已有沟通风格记录
        let mut stmt = conn
            .prepare(
                "SELECT v.content
                 FROM memory_nodes n
                 JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                 WHERE n.space_id = ?1
                   AND n.kind = 'user_profile'
                   AND json_extract(n.metadata_json, '$.preference_category') = 'communication'
                 LIMIT 1",
            )
            .map_err(|e| Error::Database(e))?;

        let style_str: Option<String> = stmt
            .query_row(params![space_id], |row| row.get(0))
            .ok();

        match style_str {
            Some(s) if s.contains("formal") || s.contains("正式") => {
                Ok(CommunicationStyle::Formal)
            }
            Some(s) if s.contains("casual") || s.contains("随意") => {
                Ok(CommunicationStyle::Casual)
            }
            Some(s) if s.contains("technical") || s.contains("技术") => {
                Ok(CommunicationStyle::Technical)
            }
            _ => Ok(CommunicationStyle::Mixed),
        }
    }

    fn analyze_decision_pattern(
        &self,
        space_id: &str,
    ) -> Result<DecisionPattern, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT v.content
                 FROM memory_nodes n
                 JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                 WHERE n.space_id = ?1
                   AND n.kind IN ('user_profile', 'directive')
                   AND (
                       v.content LIKE '%决策%'
                       OR v.content LIKE '%decision%'
                       OR v.content LIKE '%选择%'
                   )
                 LIMIT 5",
            )
            .map_err(|e| Error::Database(e))?;

        let contents: Vec<String> = stmt
            .query_map(params![space_id], |row| row.get(0))
            .map_err(|e| Error::Database(e))?
            .filter_map(|r| r.ok())
            .collect();

        let combined = contents.join(" ").to_lowercase();

        if combined.contains("快") || combined.contains("quick") || combined.contains("直接") {
            Ok(DecisionPattern::Quick)
        } else if combined.contains("谨慎") || combined.contains("cautious") || combined.contains("小心") {
            Ok(DecisionPattern::Cautious)
        } else if combined.contains("数据") || combined.contains("data") || combined.contains("分析") {
            Ok(DecisionPattern::DataDriven)
        } else {
            Ok(DecisionPattern::Unknown)
        }
    }

    fn analyze_tech_preferences(
        &self,
        space_id: &str,
    ) -> Result<Vec<TechPreference>, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT metadata_json, updated_at
                 FROM memory_nodes
                 WHERE space_id = ?1
                   AND kind IN ('user_profile', 'procedure')
                   AND (
                       json_extract(metadata_json, '$.language') IS NOT NULL
                       OR json_extract(metadata_json, '$.tool_name') IS NOT NULL
                       OR json_extract(metadata_json, '$.framework') IS NOT NULL
                   )
                 ORDER BY updated_at DESC
                 LIMIT 50",
            )
            .map_err(|e| Error::Database(e))?;

        let mut freq_map: HashMap<String, (usize, String)> = HashMap::new();

        let rows: Vec<(String, String)> = stmt
            .query_map(params![space_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| Error::Database(e))?
            .filter_map(|r| r.ok())
            .collect();

        for (metadata_str, updated_at) in &rows {
            let meta: serde_json::Value =
                serde_json::from_str(metadata_str).unwrap_or_default();

            if let Some(lang) = meta.get("language").and_then(|v| v.as_str()) {
                let entry = freq_map
                    .entry(format!("language:{}", lang))
                    .or_insert((0, updated_at.clone()));
                entry.0 += 1;
            }

            if let Some(tool) = meta.get("tool_name").and_then(|v| v.as_str()) {
                let entry = freq_map
                    .entry(format!("tool:{}", tool))
                    .or_insert((0, updated_at.clone()));
                entry.0 += 1;
            }
        }

        let mut prefs: Vec<TechPreference> = freq_map
            .into_iter()
            .map(|(key, (freq, last_used))| {
                let (cat, name) = key.split_once(':').unwrap_or(("other", &key));
                TechPreference {
                    category: cat.to_string(),
                    name: name.to_string(),
                    frequency: freq,
                    last_used,
                }
            })
            .collect();

        prefs.sort_by(|a, b| b.frequency.cmp(&a.frequency));
        prefs.truncate(20);

        Ok(prefs)
    }

    fn analyze_work_patterns(
        &self,
        space_id: &str,
    ) -> Result<WorkPatterns, Error> {
        // 从节点创建时间分析活跃时段
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT created_at
                 FROM memory_nodes
                 WHERE space_id = ?1
                 ORDER BY created_at DESC
                 LIMIT 200",
            )
            .map_err(|e| Error::Database(e))?;

        let timestamps: Vec<String> = stmt
            .query_map(params![space_id], |row| row.get(0))
            .map_err(|e| Error::Database(e))?
            .filter_map(|r| r.ok())
            .collect();

        let mut hour_counts: HashMap<usize, usize> = HashMap::new();
        let mut day_counts: HashMap<usize, usize> = HashMap::new();

        for ts in &timestamps {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                let dt = dt.with_timezone(&chrono::Utc);
                *hour_counts.entry(dt.hour() as usize).or_insert(0) += 1;
                *day_counts
                    .entry(dt.weekday().num_days_from_sunday() as usize)
                    .or_insert(0) += 1;
            }
        }

        // 找出活跃时段（top-3 小时）
        let mut hours: Vec<(usize, usize)> = hour_counts.into_iter().collect();
        hours.sort_by(|a, b| b.1.cmp(&a.1));
        let active_hours: Vec<usize> = hours.iter().take(3).map(|(h, _)| *h).collect();

        // 偏好工作日
        let mut days: Vec<(usize, usize)> = day_counts.into_iter().collect();
        days.sort_by(|a, b| b.1.cmp(&a.1));
        let preferred_days: Vec<usize> = days.iter().take(5).map(|(d, _)| *d).collect();

        Ok(WorkPatterns {
            active_hours,
            avg_session_minutes: 30.0, // 默认值
            avg_daily_messages: timestamps.len() as f64 / 7.0, // 粗略估计
            preferred_days,
        })
    }

    fn collect_core_values(&self, space_id: &str) -> Result<Vec<String>, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT v.content
                 FROM memory_nodes n
                 JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                 WHERE n.space_id = ?1
                   AND n.kind = 'value'
                 LIMIT 10",
            )
            .map_err(|e| Error::Database(e))?;

        let values: Vec<String> = stmt
            .query_map(params![space_id], |row| row.get(0))
            .map_err(|e| Error::Database(e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(values)
    }

    fn infer_style_from_signals(
        &self,
        signals: &[BehaviorSignal],
    ) -> CommunicationStyle {
        if signals.is_empty() {
            return CommunicationStyle::Mixed;
        }

        let avg_len: f64 = signals.iter().map(|s| s.message_length as f64).sum::<f64>()
            / signals.len() as f64;
        let avg_tech: f32 = signals.iter().map(|s| s.tech_terms_density).sum::<f32>()
            / signals.len() as f32;

        if avg_tech > 0.05 {
            CommunicationStyle::Technical
        } else if avg_len > 100.0 {
            CommunicationStyle::Formal
        } else {
            CommunicationStyle::Casual
        }
    }

    fn upsert_personality_boot_node(
        &self,
        space_id: &str,
        profile: &PersonalityProfile,
    ) -> Result<(), Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let now = chrono::Utc::now().to_rfc3339();

        // 查找已有的 personality boot 节点
        let existing_id: Option<String> = conn
            .query_row(
                "SELECT id FROM memory_nodes
                 WHERE space_id = ?1 AND kind = 'identity'
                   AND title LIKE '%人格画像%'
                 LIMIT 1",
                params![space_id],
                |row| row.get(0),
            )
            .ok();

        let content = format!(
            "人格画像（自动生成）\n\
             沟通风格: {}\n\
             决策模式: {}\n\
             技术偏好: {:?}\n\
             活跃时段: {:?}\n\
             核心价值观: {:?}",
            profile.communication_style.as_str(),
            profile.decision_pattern.as_str(),
            profile
                .tech_preferences
                .iter()
                .take(5)
                .map(|t| format!("{}({})", t.name, t.frequency))
                .collect::<Vec<_>>(),
            profile.work_patterns.active_hours,
            profile.core_values,
        );

        if let Some(node_id) = existing_id {
            // 更新已有节点
            let version_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "UPDATE memory_nodes SET updated_at = ?1 WHERE id = ?2",
                params![now, node_id],
            )
            .map_err(|e| Error::Database(e))?;

            conn.execute(
                "UPDATE memory_versions SET status = 'superseded', updated_at = ?1
                 WHERE node_id = ?2 AND status = 'active'",
                params![now, node_id],
            )
            .map_err(|e| Error::Database(e))?;

            conn.execute(
                "INSERT INTO memory_versions
                 (id, node_id, content, status, embedding_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'active', NULL, ?4, ?5)",
                params![version_id, node_id, content, now, now],
            )
            .map_err(|e| Error::Database(e))?;
        } else {
            // 创建新节点
            let node_id = uuid::Uuid::new_v4().to_string();
            let version_id = uuid::Uuid::new_v4().to_string();

            conn.execute(
                "INSERT INTO memory_nodes
                 (id, space_id, kind, title, metadata_json, created_at, updated_at)
                 VALUES (?1, ?2, 'identity', '人格画像（自动生成）',
                         json_object('auto_generated', 'true', 'updated_at', ?3),
                         ?4, ?5)",
                params![node_id, space_id, now, now, now],
            )
            .map_err(|e| Error::Database(e))?;

            conn.execute(
                "INSERT INTO memory_versions
                 (id, node_id, content, status, embedding_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'active', NULL, ?4, ?5)",
                params![version_id, node_id, content, now, now],
            )
            .map_err(|e| Error::Database(e))?;
        }

        Ok(())
    }
}

// ─── 辅助函数 ─────────────────────────────────────────────────────────

/// 计算技术术语出现次数
fn count_tech_terms(text: &str) -> usize {
    let tech_terms = [
        "async", "await", "fn", "impl", "struct", "enum", "trait",
        "API", "JSON", "REST", "HTTP", "SQL", "Docker", "Git",
        "compile", "runtime", "error", "debug", "optimize",
        "函数", "接口", "数据库", "编译", "测试", "部署",
        "use", "let", "mut", "pub", "mod", "type", "where",
    ];

    let text_lower = text.to_lowercase();
    tech_terms
        .iter()
        .filter(|t| text_lower.contains(&t.to_lowercase()))
        .count()
}

/// 判断消息是否包含纠正意图
fn is_correction_message(text: &str) -> bool {
    let correction_patterns = [
        "不对", "不是这样的", "错了", "应该是",
        "换一种", "不要", "别用", "能不能换成",
        "no", "wrong", "not correct", "incorrect",
        "instead", "rather", "actually",
    ];

    correction_patterns
        .iter()
        .any(|p| text.to_lowercase().contains(&p.to_lowercase()))
}
