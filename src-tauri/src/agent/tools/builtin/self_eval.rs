use std::sync::Arc;
use std::time::Instant;
use async_trait::async_trait;
use tauri::Emitter;
use crate::agent::gep::types::{LearningCard, LearningCardType, StrategyHint};
use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::infra::InfraService;

/// Classify a raw learning string into a LearningCard.
/// Uses lightweight rule-based classification (P0 — Phase 0 of LearningCard).
fn classify_learning(raw: &str, score: f32, session_id: &str, tool_name: Option<&str>) -> LearningCard {
    let card_type = if is_generic_advice(raw) {
        LearningCardType::Noise
    } else if score < 0.5 {
        LearningCardType::FailureLesson
    } else if raw.contains('快') || raw.contains('省') || raw.contains('少') || raw.contains('多') {
        LearningCardType::OptimizationTip
    } else {
        LearningCardType::SuccessPattern
    };

    // Extract a failure signal from the raw text
    let failure_signal = if card_type == LearningCardType::FailureLesson {
        extract_failure_signal(raw)
    } else {
        None
    };

    LearningCard {
        raw: raw.to_string(),
        card_type,
        failure_signal,
        tool_name: tool_name.map(|s| s.to_string()),
        strategy_hint: StrategyHint::default(),
        files_touched: vec![],
        session_id: session_id.to_string(),
        score,
        timestamp: chrono::Utc::now().timestamp_millis(),
    }
}

/// Check if a learning is generic advice (noise to be filtered).
fn is_generic_advice(raw: &str) -> bool {
    let generic_patterns = [
        "应该多", "注意", "仔细", "认真", "保持", "尽量",
        "记得", "别忘了", "好好", "多多", "谨慎",
        "多检查", "小心", "别大意", "要仔细",
    ];
    // Short learnings that are also generic → noise
    if raw.len() < 10 {
        return true;
    }
    generic_patterns.iter().any(|p| raw.contains(p))
}

/// Extract a failure signal from the raw learning text.
fn extract_failure_signal(raw: &str) -> Option<String> {
    let lower = raw.to_lowercase();
    // Common error patterns
    let signals = [
        ("403", "403"),
        ("401", "401"),
        ("404", "404"),
        ("429", "429"),
        ("500", "500"),
        ("timeout", "timeout"),
        ("超时", "timeout"),
        ("parse error", "parse_error"),
        ("解析错误", "parse_error"),
        ("panic", "panic"),
        ("unwrap", "unwrap"),
        ("permission denied", "permission_denied"),
        ("权限", "permission_denied"),
        ("not found", "not_found"),
        ("rate limit", "rate_limit"),
        ("key error", "key_error"),
        ("type error", "type_error"),
    ];
    for (pattern, signal) in &signals {
        if lower.contains(pattern) {
            return Some(signal.to_string());
        }
    }
    None
}

pub struct SelfEvalTool {
    session_id: String,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    app_handle: tauri::AppHandle,
    infra_service: Option<Arc<InfraService>>,
}

impl SelfEvalTool {
    pub fn new(
        session_id: String,
        db: Arc<std::sync::Mutex<rusqlite::Connection>>,
        app_handle: tauri::AppHandle,
    ) -> Self {
        Self { session_id, db, app_handle, infra_service: None }
    }

    pub fn with_infra(mut self, infra: Arc<InfraService>) -> Self {
        self.infra_service = Some(infra);
        self
    }
}

#[async_trait]
impl Tool for SelfEvalTool {
    fn name(&self) -> &str { "self_eval" }
    fn description(&self) -> &str {
        "Evaluate your own task completion quality. Record a score and learnings for future improvement."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "score": {
                    "type": "number",
                    "minimum": 0.0,
                    "maximum": 1.0,
                    "description": "Completion quality score from 0.0 (failed) to 1.0 (perfect)"
                },
                "reasoning": {
                    "type": "string",
                    "description": "Why you gave this score"
                },
                "learnings": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Reusable insights or patterns that could improve future performance"
                }
            },
            "required": ["score", "reasoning"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let score = params["score"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0) as f32;
        let reasoning = params["reasoning"].as_str().unwrap_or("").to_string();
        let learnings: Vec<String> = params["learnings"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let learnings_json = serde_json::to_string(&learnings).unwrap_or_default();

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();

        // Persist to DB
        match self.db.lock() {
            Ok(conn) => {
                if let Err(e) = conn.execute(
                    "INSERT INTO session_evals (id, session_id, score, reasoning, learnings, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                    rusqlite::params![id, self.session_id, score, reasoning, learnings_json, now],
                ) {
                    tracing::error!("SelfEvalTool: DB insert failed: {e}");
                }
            }
            Err(e) => tracing::error!("SelfEvalTool: DB lock failed: {e}"),
        }

        // Emit eval-complete to frontend (always)
        let _ = self.app_handle.emit("session:eval-complete", serde_json::json!({
            "sessionId": self.session_id,
            "score": score,
            "reasoning": reasoning,
            "learnings": learnings,
        }));

        // Emit eval-warning when quality is poor
        if score < 0.5 {
            let _ = self.app_handle.emit("session:eval-warning", serde_json::json!({
                "sessionId": self.session_id,
                "score": score,
                "reasoning": reasoning,
            }));
        }

        // Publish each learning as a SkillLearned infra event with LearningCard metadata
        if let Some(infra) = &self.infra_service {
            for learning in &learnings {
                let card = classify_learning(learning, score, &self.session_id, None);
                // Skip noise learnings (P0 filter)
                if card.card_type == LearningCardType::Noise {
                    continue;
                }
                infra.publish_skill_learned(
                    "self_eval",
                    learning,
                    serde_json::json!({
                        "session_id": self.session_id,
                        "score": score,
                        "source": "self_eval",
                        "learning_card": {
                            "card_type": card.card_type,
                            "failure_signal": card.failure_signal,
                            "tool_name": card.tool_name,
                            "strategy_hint": {
                                "condition": card.strategy_hint.condition,
                                "action": card.strategy_hint.action,
                                "reason": card.strategy_hint.reason,
                            },
                        },
                    }),
                ).await;
            }
        }

        let duration = start.elapsed().as_millis() as u64;
        Ok(ToolOutput::success(
            &format!("Self-evaluation recorded: score={:.2}, {} learnings captured", score, learnings.len()),
            duration,
        ))
    }
}
