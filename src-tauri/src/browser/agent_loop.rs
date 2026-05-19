use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::browser::context_manager::BrowserContextManager;
use crate::browser::observation::BrowserObservation;
use crate::browser::session_state::{BrowserTaskRun, BrowserTaskStatus, BrowserTaskStep};

pub fn clamp_max_steps(max_steps: Option<u32>) -> u32 {
    max_steps.unwrap_or(8).clamp(1, 25)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTaskRequest {
    pub session_id: String,
    pub task: String,
    pub max_steps: Option<u32>,
    pub start_url: Option<String>,
}

pub struct BrowserAgentLoop {
    ctx_mgr: Arc<BrowserContextManager>,
}

impl BrowserAgentLoop {
    pub fn new(ctx_mgr: Arc<BrowserContextManager>) -> Self {
        Self { ctx_mgr }
    }

    pub async fn run(&self, request: BrowserTaskRequest) -> Result<BrowserTaskRun> {
        let max_steps = clamp_max_steps(request.max_steps);
        let run_id = uuid::Uuid::new_v4().to_string();
        let mut run = BrowserTaskRun {
            run_id,
            session_id: request.session_id.clone(),
            task: request.task.clone(),
            status: BrowserTaskStatus::Running,
            steps: Vec::new(),
        };
        self.emit_run(&run);

        let ctx = self.ctx_mgr.get_or_create(&request.session_id).await?;
        let tab_id = if let Some(start_url) = request.start_url.as_deref() {
            ctx.navigate("new", start_url, self.ctx_mgr.app_handle()).await?
        } else {
            ctx.active_or_first_tab_id()
                .await
                .ok_or_else(|| anyhow!("No browser tab is available for task"))?
        };

        let observation = ctx.observe(&tab_id, false).await?;
        let observe_step = BrowserTaskStep {
            step_index: 0,
            observation_summary: summarize_observation(&observation),
            reasoning: format!("Initial browser observation captured. max_steps={max_steps}."),
            action_name: "observe".to_string(),
            action_args: serde_json::json!({
                "tabId": tab_id,
                "includeScreenshot": false
            }),
            ok: true,
            message: Some("Observed current browser state.".to_string()),
            error: None,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        };
        self.push_step(&mut run, observe_step);

        let planner_error = "browser_task planner is not wired yet; Task 5 will attach the LLM decision adapter.";
        let stop_step = BrowserTaskStep {
            step_index: 1,
            observation_summary: String::new(),
            reasoning: "Stopping before action execution because no decision adapter is available.".to_string(),
            action_name: "planner".to_string(),
            action_args: serde_json::json!({
                "task": request.task,
                "maxSteps": max_steps
            }),
            ok: false,
            message: None,
            error: Some(planner_error.to_string()),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        };
        self.push_step(&mut run, stop_step);
        run.status = BrowserTaskStatus::Failed;
        self.emit_run(&run);
        Ok(run)
    }

    fn push_step(&self, run: &mut BrowserTaskRun, step: BrowserTaskStep) {
        run.steps.push(step.clone());
        let _ = self.ctx_mgr.app_handle().emit("browser:task-step", serde_json::json!({
            "runId": run.run_id,
            "sessionId": run.session_id,
            "status": run.status,
            "step": step,
        }));
    }

    fn emit_run(&self, run: &BrowserTaskRun) {
        let _ = self.ctx_mgr.app_handle().emit("browser:task-run", run);
    }
}

fn summarize_observation(observation: &BrowserObservation) -> String {
    let mut text = observation.page_text.trim().replace('\n', " ");
    if text.len() > 240 {
        text.truncate(240);
        text.push_str("...");
    }
    format!(
        "url={} title={} elements={} text={}",
        observation.url,
        observation.title,
        observation.elements.len(),
        text
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn max_steps_bounds_task_loop() {
        assert_eq!(super::clamp_max_steps(Some(0)), 1);
        assert_eq!(super::clamp_max_steps(Some(8)), 8);
        assert_eq!(super::clamp_max_steps(Some(100)), 25);
    }
}
