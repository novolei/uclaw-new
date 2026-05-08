use std::sync::Arc;
use std::time::Instant;
use async_trait::async_trait;
use tauri::Emitter;
use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::infra::InfraService;

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

        // Publish each learning as a SkillLearned infra event so SkillExtraction can pick it up
        if let Some(infra) = &self.infra_service {
            for learning in &learnings {
                infra.publish_skill_learned(
                    "self_eval",
                    learning,
                    serde_json::json!({
                        "session_id": self.session_id,
                        "score": score,
                        "source": "self_eval",
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
