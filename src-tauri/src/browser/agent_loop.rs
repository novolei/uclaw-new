use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::time::{sleep, Duration};

use crate::browser::action::BrowserAction;
use crate::browser::action_registry::BrowserActionRegistry;
use crate::browser::boundary::detect_intervention_boundary;
use crate::browser::context_manager::BrowserContextManager;
use crate::browser::decision::{BrowserDecisionAdapter, BrowserDecisionStatus};
use crate::browser::intervention_bridge::{
    BrowserAskUserBridge, BrowserInterventionDecision, BrowserInterventionPrompt,
};
use crate::browser::loop_detector::{make_fingerprint, LoopDetector};
use crate::browser::observation::BrowserObservation;
use crate::browser::recovery::{classify_browser_error, BrowserRecoveryKind};
use crate::browser::session_state::{
    BrowserTaskRun, BrowserTaskStatus, BrowserTaskStep, BrowserTaskStepPhase,
};
use crate::browser::task_store::{BrowserTaskMemory, BrowserTaskStore};

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
    #[serde(default)]
    pub available_file_paths: Vec<String>,
    pub resume_run_id: Option<String>,
}

pub struct BrowserAgentLoop {
    ctx_mgr: Arc<BrowserContextManager>,
    decision_adapter: Arc<dyn BrowserDecisionAdapter>,
    action_registry: BrowserActionRegistry,
    task_store: Option<Arc<BrowserTaskStore>>,
    ask_user_bridge: Option<Arc<BrowserAskUserBridge>>,
}

impl BrowserAgentLoop {
    pub fn new(
        ctx_mgr: Arc<BrowserContextManager>,
        decision_adapter: Arc<dyn BrowserDecisionAdapter>,
    ) -> Self {
        let action_registry = BrowserActionRegistry::new(Arc::clone(&ctx_mgr));
        Self { ctx_mgr, decision_adapter, action_registry, task_store: None, ask_user_bridge: None }
    }

    pub fn with_task_store(mut self, task_store: Option<Arc<BrowserTaskStore>>) -> Self {
        self.task_store = task_store;
        self
    }

    pub fn with_ask_user_bridge(mut self, ask_user_bridge: Option<Arc<BrowserAskUserBridge>>) -> Self {
        self.ask_user_bridge = ask_user_bridge;
        self
    }

    pub async fn run(&self, request: BrowserTaskRequest) -> Result<BrowserTaskRun> {
        let mut segment_steps = clamp_max_steps(request.max_steps);
        let mut run = self
            .load_resume_run(&request)?
            .unwrap_or_else(|| BrowserTaskRun {
                run_id: uuid::Uuid::new_v4().to_string(),
                session_id: request.session_id.clone(),
                task: request.task.clone(),
                status: BrowserTaskStatus::Running,
                steps: Vec::new(),
            });
        run.status = BrowserTaskStatus::Running;
        self.emit_run(&run);

        let ctx = self.ctx_mgr.get_or_create(&request.session_id).await?;
        let resume_checkpoint = self.latest_checkpoint(&request);
        let tab_id = if let Some(tab_id) = resume_checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.active_tab_id.clone())
        {
            tab_id
        } else if let Some(start_url) = request.start_url.as_deref() {
            ctx.navigate("new", start_url, self.ctx_mgr.app_handle()).await?
        } else {
            ctx.active_or_first_tab_id()
                .await
                .ok_or_else(|| anyhow!("No browser tab is available for task"))?
        };

        let mut active_tab_id = tab_id;
        let mut step_index = run.steps.last().map(|step| step.step_index + 1).unwrap_or(0);
        let mut loop_detector = LoopDetector::default();
        let mut latest_memory = resume_checkpoint.and_then(|checkpoint| checkpoint.memory);

        'segments: loop {
        for _ in 0..segment_steps {
            let observation = ctx.observe(&active_tab_id, false).await?;
            let observation_json = serde_json::to_value(&observation)?;
            let memory = self.update_memory(&request, &observation_json);
            latest_memory = memory.clone().or(latest_memory);
            self.push_step(&mut run, BrowserTaskStep {
                step_index,
                phase: BrowserTaskStepPhase::Observe,
                observation_summary: summarize_observation(&observation),
                reasoning: "Captured current browser state for planning.".to_string(),
                action_name: "observe".to_string(),
                action_args: serde_json::json!({
                    "tabId": active_tab_id,
                    "includeScreenshot": false
                }),
                ok: true,
                message: Some("Observed current browser state.".to_string()),
                error: None,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
            });
            step_index += 1;

            if let Some(boundary) = detect_intervention_boundary(&observation_json) {
                let reason = boundary.reason.clone();
                run.status = BrowserTaskStatus::NeedsUserIntervention;
                self.push_step(&mut run, BrowserTaskStep {
                    step_index,
                    phase: BrowserTaskStepPhase::UserIntervention,
                    observation_summary: summarize_observation(&observation),
                    reasoning: boundary.reason.clone(),
                    action_name: "needs_user_intervention".to_string(),
                    action_args: serde_json::to_value(&boundary)?,
                    ok: false,
                    message: Some(reason.clone()),
                    error: None,
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                });
                self.emit_run(&run);
                self.persist_checkpoint(&run, step_index, Some(&active_tab_id), latest_memory.as_ref());
                if let Some(decision) = self.ask_for_intervention(&run, &reason).await? {
                    self.push_intervention_answer_step(
                        &mut run,
                        step_index + 1,
                        decision,
                        "Browser user-intervention prompt was answered.",
                    );
                    match decision {
                        BrowserInterventionDecision::Continue
                        | BrowserInterventionDecision::ContinueWithSteps(_) => {
                            run.status = BrowserTaskStatus::Running;
                            self.emit_run(&run);
                            step_index += 2;
                            continue;
                        }
                        BrowserInterventionDecision::Stop => {
                            run.status = BrowserTaskStatus::Stopped;
                            self.emit_run(&run);
                        }
                    }
                }
                return Ok(run);
            }

            let decision = self
                .decision_adapter
                .decide(
                    &request.task,
                    &observation_json,
                    memory.as_ref(),
                    &request.available_file_paths,
                    &run.steps,
                )
                .await?;
            self.push_step(&mut run, BrowserTaskStep {
                step_index,
                phase: BrowserTaskStepPhase::Decide,
                observation_summary: summarize_observation(&observation),
                reasoning: decision.reasoning.clone(),
                action_name: "decide".to_string(),
                action_args: serde_json::to_value(&decision.action)?,
                ok: !matches!(
                    decision.status,
                    BrowserDecisionStatus::Failed | BrowserDecisionStatus::NeedsUserIntervention
                ),
                message: decision.final_answer.clone(),
                error: if matches!(
                    decision.status,
                    BrowserDecisionStatus::Failed | BrowserDecisionStatus::NeedsUserIntervention
                ) {
                    decision.final_answer.clone()
                } else {
                    None
                },
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
            });
            step_index += 1;

            match decision.status {
                BrowserDecisionStatus::Done => {
                    run.status = BrowserTaskStatus::Completed;
                    self.push_step(&mut run, BrowserTaskStep {
                        step_index,
                        phase: BrowserTaskStepPhase::Done,
                        observation_summary: summarize_observation(&observation),
                        reasoning: "Browser task reported completion.".to_string(),
                        action_name: "done".to_string(),
                        action_args: serde_json::json!({ "task": request.task }),
                        ok: true,
                        message: decision.final_answer,
                        error: None,
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    });
                    self.emit_run(&run);
                    self.persist_checkpoint(&run, step_index, Some(&active_tab_id), latest_memory.as_ref());
                    return Ok(run);
                }
                BrowserDecisionStatus::Failed => {
                    run.status = BrowserTaskStatus::Failed;
                    self.emit_run(&run);
                    self.persist_checkpoint(&run, step_index, Some(&active_tab_id), latest_memory.as_ref());
                    return Ok(run);
                }
                BrowserDecisionStatus::NeedsUserIntervention => {
                    let reason = decision
                        .final_answer
                        .clone()
                        .unwrap_or_else(|| "Browser decision requested user intervention.".to_string());
                    run.status = BrowserTaskStatus::NeedsUserIntervention;
                    self.push_step(&mut run, BrowserTaskStep {
                        step_index,
                        phase: BrowserTaskStepPhase::UserIntervention,
                        observation_summary: summarize_observation(&observation),
                        reasoning: "Browser decision requested user intervention.".to_string(),
                        action_name: "needs_user_intervention".to_string(),
                        action_args: serde_json::json!({ "task": request.task }),
                        ok: false,
                        message: Some(reason.clone()),
                        error: None,
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    });
                    self.emit_run(&run);
                    self.persist_checkpoint(&run, step_index, Some(&active_tab_id), latest_memory.as_ref());
                    if let Some(user_decision) = self.ask_for_intervention(&run, &reason).await? {
                        self.push_intervention_answer_step(
                            &mut run,
                            step_index + 1,
                            user_decision,
                            "Browser decision-intervention prompt was answered.",
                        );
                        match user_decision {
                            BrowserInterventionDecision::Continue
                            | BrowserInterventionDecision::ContinueWithSteps(_) => {
                                run.status = BrowserTaskStatus::Running;
                                self.emit_run(&run);
                                step_index += 2;
                                continue;
                            }
                            BrowserInterventionDecision::Stop => {
                                run.status = BrowserTaskStatus::Stopped;
                                self.emit_run(&run);
                            }
                        }
                    }
                    return Ok(run);
                }
                BrowserDecisionStatus::Continue => {}
            }

            let action = decision
                .action
                .clone()
                .ok_or_else(|| anyhow!("browser decision status=continue but action was null"))?;
            let action_args = serde_json::to_value(&action)?;
            let action_args_text = serde_json::to_string(&action_args)?;
            let fingerprint = make_fingerprint(
                &observation.url,
                action_name(&action),
                &action_args_text,
            );
            if loop_detector.record(&fingerprint) {
                run.status = BrowserTaskStatus::Failed;
                self.push_step(&mut run, BrowserTaskStep {
                    step_index,
                    phase: BrowserTaskStepPhase::Recover,
                    observation_summary: summarize_observation(&observation),
                    reasoning: "Detected repeated browser action loop.".to_string(),
                    action_name: "loop_detector".to_string(),
                    action_args,
                    ok: false,
                    message: None,
                    error: Some("Browser agent repeated the same action on the same URL; stopping for re-plan.".to_string()),
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                });
                    self.emit_run(&run);
                    self.persist_checkpoint(&run, step_index, Some(&active_tab_id), latest_memory.as_ref());
                    return Ok(run);
                }

            match self
                .action_registry
                .execute(&request.session_id, action.clone())
                .await
            {
                Ok(result) => {
                    if let Some(tab_id) = result.tab_id.as_ref() {
                        active_tab_id = tab_id.clone();
                    } else if let Some(tab_id) = tab_id_from_action(&action) {
                        active_tab_id = tab_id;
                    }
                    self.push_step(&mut run, BrowserTaskStep {
                        step_index,
                        phase: BrowserTaskStepPhase::Act,
                        observation_summary: String::new(),
                        reasoning: format!("Executed {}.", result.action_name),
                        action_name: result.action_name,
                        action_args,
                        ok: result.ok,
                        message: result.message,
                        error: result.error,
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    });
                    step_index += 1;
                }
                Err(error) => {
                    let err = error.to_string();
                    self.push_step(&mut run, BrowserTaskStep {
                        step_index,
                        phase: BrowserTaskStepPhase::Act,
                        observation_summary: String::new(),
                        reasoning: "Browser action failed.".to_string(),
                        action_name: action_name(&action).to_string(),
                        action_args: action_args.clone(),
                        ok: false,
                        message: None,
                        error: Some(err.clone()),
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    });
                    step_index += 1;

                    match self
                        .recover_after_error(
                            &mut run,
                            &mut active_tab_id,
                            step_index,
                            &err,
                            &Some(action.clone()),
                        )
                        .await?
                    {
                        RecoveryOutcome::Continue(next_step_index) => {
                            step_index = next_step_index;
                            continue;
                        }
                        RecoveryOutcome::Stop => {
                            run.status = BrowserTaskStatus::Failed;
                            self.emit_run(&run);
                            self.persist_checkpoint(&run, step_index, Some(&active_tab_id), latest_memory.as_ref());
                            return Ok(run);
                        }
                    }
                }
            }
        }

        run.status = BrowserTaskStatus::PausedCheckpointed;
        self.push_step(&mut run, BrowserTaskStep {
            step_index,
            phase: BrowserTaskStepPhase::Done,
            observation_summary: String::new(),
            reasoning: format!("Paused at checkpoint after reaching max_steps={segment_steps}."),
            action_name: "checkpoint_pause".to_string(),
            action_args: serde_json::json!({ "maxSteps": segment_steps }),
            ok: false,
            message: None,
            error: Some("Browser task reached max_steps and saved a resumable checkpoint.".to_string()),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        });
        self.persist_checkpoint(&run, step_index, Some(&active_tab_id), latest_memory.as_ref());
        self.emit_run(&run);
        if let Some(decision) = self.ask_for_checkpoint(&run).await? {
            self.push_intervention_answer_step(
                &mut run,
                step_index + 1,
                decision,
                "Browser checkpoint prompt was answered.",
            );
            match decision {
                BrowserInterventionDecision::Continue => {
                    segment_steps = clamp_max_steps(Some(8));
                    run.status = BrowserTaskStatus::Running;
                    self.emit_run(&run);
                    step_index += 2;
                    continue 'segments;
                }
                BrowserInterventionDecision::ContinueWithSteps(steps) => {
                    segment_steps = clamp_max_steps(Some(steps));
                    run.status = BrowserTaskStatus::Running;
                    self.emit_run(&run);
                    step_index += 2;
                    continue 'segments;
                }
                BrowserInterventionDecision::Stop => {}
            }
        }
        return Ok(run);
        }
    }

    fn push_intervention_answer_step(
        &self,
        run: &mut BrowserTaskRun,
        step_index: u32,
        decision: BrowserInterventionDecision,
        reasoning: &str,
    ) {
        self.push_step(run, BrowserTaskStep {
            step_index,
            phase: BrowserTaskStepPhase::UserIntervention,
            observation_summary: String::new(),
            reasoning: reasoning.to_string(),
            action_name: "ask_user_response".to_string(),
            action_args: serde_json::json!({ "decision": decision.label() }),
            ok: !matches!(decision, BrowserInterventionDecision::Stop),
            message: Some(format!("User answered: {}", decision.label())),
            error: if matches!(decision, BrowserInterventionDecision::Stop) {
                Some("User chose to stop the browser task.".to_string())
            } else {
                None
            },
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        });
    }

    async fn ask_for_intervention(
        &self,
        run: &BrowserTaskRun,
        reason: &str,
    ) -> Result<Option<BrowserInterventionDecision>> {
        let Some(bridge) = self.ask_user_bridge.as_ref() else {
            return Ok(None);
        };
        Ok(Some(
            bridge
                .ask(BrowserInterventionPrompt::human_boundary(&run.run_id, reason))
                .await?,
        ))
    }

    async fn ask_for_checkpoint(&self, run: &BrowserTaskRun) -> Result<Option<BrowserInterventionDecision>> {
        let Some(bridge) = self.ask_user_bridge.as_ref() else {
            return Ok(None);
        };
        Ok(Some(
            bridge
                .ask(BrowserInterventionPrompt::checkpoint(&run.run_id))
                .await?,
        ))
    }

    async fn recover_after_error(
        &self,
        run: &mut BrowserTaskRun,
        active_tab_id: &mut String,
        step_index: u32,
        error: &str,
        failed_action: &Option<BrowserAction>,
    ) -> Result<RecoveryOutcome> {
        let kind = classify_browser_error(error);
        let (ok, message) = match kind {
            BrowserRecoveryKind::RefreshTabsAndRetry => {
                let ctx = self.ctx_mgr.get_or_create(&run.session_id).await?;
                if let Some(tab_id) = ctx.active_or_first_tab_id().await {
                    *active_tab_id = tab_id;
                    (true, "Refreshed active tab id; retrying with a fresh observation.".to_string())
                } else {
                    (false, "No live browser tab remains after refresh.".to_string())
                }
            }
            BrowserRecoveryKind::RefreshDomAndRetry => {
                let ctx = self.ctx_mgr.get_or_create(&run.session_id).await?;
                ctx.invalidate_dom_cache(active_tab_id).await;
                (true, "Invalidated DOM cache; retrying with a fresh observation.".to_string())
            }
            BrowserRecoveryKind::WaitAndRetry => {
                sleep(Duration::from_millis(800)).await;
                (true, "Waited for page stability; retrying with a fresh observation.".to_string())
            }
            BrowserRecoveryKind::Stop => {
                (false, "Error is not recoverable by the browser agent.".to_string())
            }
        };

        self.push_step(run, BrowserTaskStep {
            step_index,
            phase: BrowserTaskStepPhase::Recover,
            observation_summary: String::new(),
            reasoning: format!("Recovery classification: {kind:?}."),
            action_name: "recover".to_string(),
            action_args: serde_json::json!({
                "kind": format!("{kind:?}"),
                "failedAction": failed_action,
            }),
            ok,
            message: Some(message),
            error: if ok { None } else { Some(error.to_string()) },
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        });

        if ok {
            Ok(RecoveryOutcome::Continue(step_index + 1))
        } else {
            Ok(RecoveryOutcome::Stop)
        }
    }

    fn push_step(&self, run: &mut BrowserTaskRun, step: BrowserTaskStep) {
        run.steps.push(step.clone());
        if let Some(store) = self.task_store.as_ref() {
            if let Err(e) = store.persist_run(run) {
                tracing::warn!(run_id = %run.run_id, error = %e, "failed to persist browser task step");
            }
        }
        let _ = self.ctx_mgr.app_handle().emit("browser:task-step", serde_json::json!({
            "runId": run.run_id,
            "sessionId": run.session_id,
            "status": run.status,
            "step": step,
        }));
    }

    fn emit_run(&self, run: &BrowserTaskRun) {
        if let Some(store) = self.task_store.as_ref() {
            if let Err(e) = store.persist_run(run) {
                tracing::warn!(run_id = %run.run_id, error = %e, "failed to persist browser task run");
            }
        }
        let _ = self.ctx_mgr.app_handle().emit("browser:task-run", run);
    }

    fn update_memory(
        &self,
        request: &BrowserTaskRequest,
        observation_json: &serde_json::Value,
    ) -> Option<BrowserTaskMemory> {
        self.task_store.as_ref().and_then(|store| {
            match store.merge_observation(&request.session_id, &request.task, observation_json) {
                Ok(memory) => Some(memory),
                Err(e) => {
                    tracing::warn!(
                        session_id = %request.session_id,
                        error = %e,
                        "failed to update browser task memory"
                    );
                    None
                }
            }
        })
    }

    fn load_resume_run(&self, request: &BrowserTaskRequest) -> Result<Option<BrowserTaskRun>> {
        let Some(run_id) = request.resume_run_id.as_deref() else {
            return Ok(None);
        };
        let Some(store) = self.task_store.as_ref() else {
            return Err(anyhow!("browser_task resume_run_id requires a task store"));
        };
        store
            .load_run(run_id)?
            .map(Some)
            .ok_or_else(|| anyhow!("browser task run '{}' was not found for resume", run_id))
    }

    fn latest_checkpoint(
        &self,
        request: &BrowserTaskRequest,
    ) -> Option<crate::browser::task_store::BrowserTaskCheckpoint> {
        let run_id = request.resume_run_id.as_deref()?;
        self.task_store
            .as_ref()
            .and_then(|store| store.latest_checkpoint(run_id).ok().flatten())
    }

    fn persist_checkpoint(
        &self,
        run: &BrowserTaskRun,
        step_index: u32,
        active_tab_id: Option<&str>,
        memory: Option<&BrowserTaskMemory>,
    ) {
        if let Some(store) = self.task_store.as_ref() {
            let loop_state = serde_json::json!({
                "status": run.status,
                "stepCount": run.steps.len(),
            });
            if let Err(e) = store.persist_checkpoint(run, step_index, active_tab_id, memory, loop_state) {
                tracing::warn!(run_id = %run.run_id, error = %e, "failed to persist browser task checkpoint");
            }
        }
    }
}

enum RecoveryOutcome {
    Continue(u32),
    Stop,
}

fn summarize_observation(observation: &BrowserObservation) -> String {
    let mut text = observation.page_text.trim().replace('\n', " ");
    if text.chars().count() > 240 {
        text = text.chars().take(240).collect();
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

fn tab_id_from_action(action: &BrowserAction) -> Option<String> {
    match action {
        BrowserAction::Navigate { tab_id, .. } => tab_id.clone(),
        BrowserAction::Click { tab_id, .. }
        | BrowserAction::Type { tab_id, .. }
        | BrowserAction::Scroll { tab_id, .. }
        | BrowserAction::SendKeys { tab_id, .. }
        | BrowserAction::Evaluate { tab_id, .. }
        | BrowserAction::GetState { tab_id, .. }
        | BrowserAction::SwitchTab { tab_id } => Some(tab_id.clone()),
        | BrowserAction::UploadFile { tab_id, .. } => Some(tab_id.clone()),
        BrowserAction::ListTabs | BrowserAction::CloseTab { .. } => None,
    }
}

fn action_name(action: &BrowserAction) -> &'static str {
    match action {
        BrowserAction::Navigate { .. } => "browser_navigate",
        BrowserAction::Click { .. } => "browser_click",
        BrowserAction::Type { .. } => "browser_type",
        BrowserAction::Scroll { .. } => "browser_scroll",
        BrowserAction::SendKeys { .. } => "browser_send_keys",
        BrowserAction::Evaluate { .. } => "browser_evaluate",
        BrowserAction::GetState { .. } => "browser_get_state",
        BrowserAction::ListTabs => "browser_list_tabs",
        BrowserAction::SwitchTab { .. } => "browser_switch_tab",
        BrowserAction::CloseTab { .. } => "browser_close_tab",
        BrowserAction::UploadFile { .. } => "browser_upload_file",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_observation_truncates_non_ascii_safely() {
        let observation = BrowserObservation {
            session_id: "s1".into(),
            tab_id: "t1".into(),
            url: "https://example.com".into(),
            title: "中文页面".into(),
            page_text: "苹果官网".repeat(100),
            elements: vec![],
            tabs: vec![],
            screenshot_b64: None,
            timestamp_ms: 123,
        };

        let summary = summarize_observation(&observation);

        assert!(summary.contains("中文页面"));
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn max_steps_bounds_task_loop() {
        assert_eq!(super::clamp_max_steps(Some(0)), 1);
        assert_eq!(super::clamp_max_steps(Some(8)), 8);
        assert_eq!(super::clamp_max_steps(Some(100)), 25);
    }
}
