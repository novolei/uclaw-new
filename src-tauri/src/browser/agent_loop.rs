use std::{collections::HashSet, sync::Arc};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::time::{sleep, Duration};

use crate::browser::action::BrowserAction;
use crate::browser::boundary::{detect_intervention_boundary, BrowserBoundaryEvent};
use crate::browser::context_manager::BrowserContextManager;
use crate::browser::decision::{BrowserDecisionAdapter, BrowserDecisionStatus};
use crate::browser::identity::{
    BrowserAuthProfileBroker, BrowserIdentityProfile, PlaywrightStorageState,
};
use crate::browser::identity_tasks::{
    BrowserIdentityRevocationDecision, BrowserIdentityTaskRegistry,
};
use crate::browser::intervention_bridge::{
    BrowserAskUserBridge, BrowserInterventionDecision, BrowserInterventionPrompt,
};
use crate::browser::loop_detector::{make_fingerprint, LoopDetector};
use crate::browser::memory_adapter::BrowserLongTermMemoryAdapter;
use crate::browser::observation::BrowserObservation;
use crate::browser::provider::BrowserProviderRouteDecision;
use crate::browser::provider_execution::BrowserProviderActionBlocked;
use crate::browser::recovery::{classify_browser_error, BrowserRecoveryKind};
use crate::browser::runtime_control_center::BrowserRuntimeProviderConfig;
use crate::browser::runtime_execution::{
    BrowserRuntimeActionExecutionOutcome, BrowserRuntimeActionExecutor, BrowserRuntimeActionRequest,
    EvaluateApprovalContext,
};
use crate::browser::runtime_status::BrowserRuntimeStatusService;
use crate::browser::session_state::{
    BrowserTaskRun, BrowserTaskStatus, BrowserTaskStep, BrowserTaskStepPhase,
};
use crate::browser::task_store::{BrowserTaskMemory, BrowserTaskStore};
use crate::browser::types::TabInfo;
use crate::mcp::SharedMcpManager;

pub fn clamp_max_steps(max_steps: Option<u32>) -> u32 {
    max_steps.unwrap_or(8).clamp(1, 25)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserTaskRuntimePreparationDecision {
    Ready,
    Defer,
}

impl Default for BrowserTaskRuntimePreparationDecision {
    fn default() -> Self {
        Self::Ready
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserIdentityResumeDecision {
    RequireAuth,
    IsolatedProfile,
    Reauthorize,
    EndTask,
}

impl Default for BrowserIdentityResumeDecision {
    fn default() -> Self {
        Self::RequireAuth
    }
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
    pub auth_profile_id: Option<String>,
    pub auth_origin: Option<String>,
    #[serde(default)]
    pub runtime_preparation_decision: BrowserTaskRuntimePreparationDecision,
    #[serde(default)]
    pub identity_resume_decision: BrowserIdentityResumeDecision,
}

pub struct BrowserAgentLoop {
    ctx_mgr: Arc<BrowserContextManager>,
    decision_adapter: Arc<dyn BrowserDecisionAdapter>,
    runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
    runtime_provider_config: BrowserRuntimeProviderConfig,
    mcp_manager: Option<SharedMcpManager>,
    task_store: Option<Arc<BrowserTaskStore>>,
    ask_user_bridge: Option<Arc<BrowserAskUserBridge>>,
    auth_profile_broker: Option<Arc<BrowserAuthProfileBroker>>,
    long_term_memory: Option<Arc<BrowserLongTermMemoryAdapter>>,
    identity_task_registry: Option<Arc<BrowserIdentityTaskRegistry>>,

    /// Slice 1b — shared SafetyManager singleton (from AppState). When None,
    /// the sub-loop falls back to its existing bespoke dispatch (regression-
    /// safe; production sets this via with_safety_manager).
    ///
    /// NOTE (Slice 1b structural finding): BrowserAgentLoop::run does NOT
    /// dispatch ToolCall objects through ToolRegistry. It dispatches browser-
    /// domain actions (navigate, click, type, evaluate, …) through
    /// BrowserRuntimeActionExecutor. There is no centralized tool-dispatch
    /// site compatible with ToolDispatcher. These three fields are the
    /// infrastructure hook. The Evaluate-gate is wired in
    /// BrowserRuntimeActionExecutor::execute_action, with production injection
    /// at the three browser-task tool construction sites:
    /// agent/tools/registry_build.rs, tauri_commands.rs::send_agent_message,
    /// and browser/tools.rs::RetryWithBrowserAgentTool.
    safety_manager: Option<Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>>,
    /// Slice 1b — shared ToolDispatcher singleton. Unused by the sub-loop
    /// today (no ToolCall path exists); reserved for the Evaluate-gate follow-up.
    tool_dispatcher: Option<Arc<crate::agent::tool_dispatch::ToolDispatcher<tauri::Wry>>>,
    /// Slice 1b — approval handler (ChatApprovalHandler — the user is in chat).
    approval_handler: Option<Arc<dyn crate::safety::ApprovalHandler>>,
}

impl BrowserAgentLoop {
    pub fn new(
        ctx_mgr: Arc<BrowserContextManager>,
        decision_adapter: Arc<dyn BrowserDecisionAdapter>,
    ) -> Self {
        Self {
            ctx_mgr,
            decision_adapter,
            runtime_status_service: None,
            runtime_provider_config: BrowserRuntimeProviderConfig::default(),
            mcp_manager: None,
            task_store: None,
            ask_user_bridge: None,
            auth_profile_broker: BrowserAuthProfileBroker::system_default()
                .ok()
                .map(Arc::new),
            long_term_memory: None,
            identity_task_registry: None,
            // Slice 1b fields — None by default; set via with_* builders.
            safety_manager: None,
            tool_dispatcher: None,
            approval_handler: None,
        }
    }

    pub fn with_task_store(mut self, task_store: Option<Arc<BrowserTaskStore>>) -> Self {
        self.task_store = task_store;
        self
    }

    pub fn with_ask_user_bridge(
        mut self,
        ask_user_bridge: Option<Arc<BrowserAskUserBridge>>,
    ) -> Self {
        self.ask_user_bridge = ask_user_bridge;
        self
    }

    pub fn with_runtime_status_service(
        mut self,
        runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
    ) -> Self {
        self.runtime_status_service = runtime_status_service;
        self
    }

    pub fn with_runtime_provider_config(
        mut self,
        runtime_provider_config: BrowserRuntimeProviderConfig,
    ) -> Self {
        self.runtime_provider_config = runtime_provider_config;
        self
    }

    pub fn with_mcp_manager(mut self, mcp_manager: Option<SharedMcpManager>) -> Self {
        self.mcp_manager = mcp_manager;
        self
    }

    pub fn with_auth_profile_broker(
        mut self,
        auth_profile_broker: Option<Arc<BrowserAuthProfileBroker>>,
    ) -> Self {
        self.auth_profile_broker = auth_profile_broker;
        self
    }

    pub fn with_long_term_memory(
        mut self,
        long_term_memory: Option<Arc<BrowserLongTermMemoryAdapter>>,
    ) -> Self {
        self.long_term_memory = long_term_memory;
        self
    }

    pub fn with_identity_task_registry(
        mut self,
        identity_task_registry: Option<Arc<BrowserIdentityTaskRegistry>>,
    ) -> Self {
        self.identity_task_registry = identity_task_registry;
        self
    }

    // ─── Slice 1b: safety chokepoint infrastructure ───────────────────────
    // These three builders wire the outer SafetyManager, ToolDispatcher, and
    // ApprovalHandler into the sub-loop for future Evaluate-action gating.
    // All default to None so existing call sites compile unchanged.
    //
    // STRUCTURAL NOTE: BrowserAgentLoop::run dispatches browser-domain actions
    // (BrowserAction::{Navigate,Click,Type,Evaluate,…}) via
    // BrowserRuntimeActionExecutor — NOT ToolCall objects through ToolRegistry.
    // The BrowserAction::Evaluate gate is wired in BrowserRuntimeActionExecutor::
    // execute_action, consulting SafetyManager for arbitrary JS execution approvals.
    // Production wire-up happens at the three browser-task tool construction sites:
    // agent/tools/registry_build.rs, tauri_commands.rs, and browser/tools.rs.

    pub fn with_safety_manager(
        mut self,
        safety_manager: Option<Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>>,
    ) -> Self {
        self.safety_manager = safety_manager;
        self
    }

    pub fn with_tool_dispatcher(
        mut self,
        tool_dispatcher: Option<Arc<crate::agent::tool_dispatch::ToolDispatcher<tauri::Wry>>>,
    ) -> Self {
        self.tool_dispatcher = tool_dispatcher;
        self
    }

    pub fn with_approval_handler(
        mut self,
        approval_handler: Option<Arc<dyn crate::safety::ApprovalHandler>>,
    ) -> Self {
        self.approval_handler = approval_handler;
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
        if should_pause_for_runtime_preparation(&request) {
            let step_index = run
                .steps
                .last()
                .map(|step| step.step_index + 1)
                .unwrap_or(0);
            run.status = BrowserTaskStatus::PausedWaitingForBrowserRuntime;
            self.push_step(
                &mut run,
                runtime_preparation_pause_step(step_index, &request),
            );
            self.emit_run(&run);
            self.persist_checkpoint(
                &run,
                step_index,
                None,
                None,
                "browser runtime preparation deferred",
                None,
            );
            self.record_final_state(&run).await;
            return Ok(run);
        }
        run.status = BrowserTaskStatus::Running;
        self.emit_run(&run);

        let mut step_index = run
            .steps
            .last()
            .map(|step| step.step_index + 1)
            .unwrap_or(0);
        let resume_checkpoint = self.latest_checkpoint(&request);
        let resume_identity_profile_id =
            checkpoint_identity_profile_id(resume_checkpoint.as_ref()).map(str::to_string);
        let requested_resume_auth = request.resume_run_id.is_some()
            && (request.auth_profile_id.is_some() || request.auth_origin.is_some());
        if request.resume_run_id.is_some()
            && matches!(
                request.identity_resume_decision,
                BrowserIdentityResumeDecision::EndTask
            )
        {
            run.status = BrowserTaskStatus::Stopped;
            self.push_step(
                &mut run,
                identity_resume_end_task_step(step_index, resume_identity_profile_id.as_deref()),
            );
            self.emit_run(&run);
            self.persist_checkpoint(
                &run,
                step_index,
                resume_checkpoint
                    .as_ref()
                    .and_then(|checkpoint| checkpoint.active_tab_id.as_deref()),
                resume_checkpoint
                    .as_ref()
                    .and_then(|checkpoint| checkpoint.memory.as_ref()),
                "browser identity boundary ended task",
                resume_identity_profile_id.as_deref(),
            );
            self.record_final_state(&run).await;
            return Ok(run);
        }

        let mut auth_request = request.clone();
        if matches!(
            auth_request.identity_resume_decision,
            BrowserIdentityResumeDecision::IsolatedProfile
        ) {
            auth_request.auth_profile_id = None;
            auth_request.auth_origin = None;
        } else if auth_request.auth_profile_id.is_none() && auth_request.auth_origin.is_none() {
            if should_inherit_checkpoint_identity(&auth_request.identity_resume_decision) {
                auth_request.auth_profile_id = resume_identity_profile_id.clone();
            }
        }
        let auth_profile = self.resolve_auth_profile(&auth_request)?;
        let auth_profile_requested =
            auth_request.auth_profile_id.is_some() || auth_request.auth_origin.is_some();
        let resolved_replacement_auth = requested_resume_auth && auth_profile.is_some();

        if request.resume_run_id.is_some()
            && matches!(
                request.identity_resume_decision,
                BrowserIdentityResumeDecision::Reauthorize
            )
            && !resolved_replacement_auth
        {
            run.status = BrowserTaskStatus::PausedCheckpointed;
            self.push_step(
                &mut run,
                identity_reauthorize_missing_step(
                    step_index,
                    resume_identity_profile_id.as_deref(),
                ),
            );
            self.emit_run(&run);
            self.persist_checkpoint(
                &run,
                step_index,
                resume_checkpoint
                    .as_ref()
                    .and_then(|checkpoint| checkpoint.active_tab_id.as_deref()),
                resume_checkpoint
                    .as_ref()
                    .and_then(|checkpoint| checkpoint.memory.as_ref()),
                "browser identity reauthorize missing auth",
                resume_identity_profile_id.as_deref(),
            );
            self.record_final_state(&run).await;
            return Ok(run);
        }
        if request.resume_run_id.is_some() && !resolved_replacement_auth {
            if let Some(profile_id) = resume_identity_profile_id.as_deref() {
                if should_block_revoked_identity_resume(
                    &request.identity_resume_decision,
                    resolved_replacement_auth,
                ) && (checkpoint_marks_identity_revoked(resume_checkpoint.as_ref())
                    || self.identity_profile_is_revoked(profile_id)?)
                {
                    run.status = BrowserTaskStatus::PausedCheckpointed;
                    self.push_step(
                        &mut run,
                        identity_revocation_resume_blocked_step(step_index, profile_id),
                    );
                    self.emit_run(&run);
                    self.persist_checkpoint(
                        &run,
                        step_index,
                        resume_checkpoint
                            .as_ref()
                            .and_then(|checkpoint| checkpoint.active_tab_id.as_deref()),
                        resume_checkpoint
                            .as_ref()
                            .and_then(|checkpoint| checkpoint.memory.as_ref()),
                        "browser identity revoked resume blocked",
                        Some(profile_id),
                    );
                    self.record_final_state(&run).await;
                    return Ok(run);
                }
            }
        }

        let identity_profile_id = auth_profile
            .as_ref()
            .map(|(profile, _)| profile.id.as_str());
        let _identity_task_registration = identity_profile_id.and_then(|profile_id| {
            self.identity_task_registry
                .as_ref()
                .map(|registry| registry.register(profile_id, &run))
        });
        if let Some(profile_id) = identity_profile_id {
            if self
                .checkpoint_if_identity_revoked(&mut run, profile_id, step_index, None, None)
                .await?
            {
                return Ok(run);
            }
        }
        if auth_profile_requested && auth_profile.is_none() && request.resume_run_id.is_none() {
            self.push_step(&mut run, BrowserTaskStep {
                step_index,
                phase: BrowserTaskStepPhase::Act,
                observation_summary: String::new(),
                reasoning: "No matching authorized browser auth profile was found for this task.".to_string(),
                action_name: "browser_auth_profile_missing".to_string(),
                action_args: serde_json::json!({
                    "authProfileId": request.auth_profile_id.as_deref(),
                    "authOrigin": request.auth_origin.as_deref(),
                    "startUrl": request.start_url.as_deref(),
                }),
                ok: false,
                message: None,
                error: Some("No matching authorized browser auth profile was found; continuing without injected auth state.".to_string()),
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
            });
            step_index += 1;
        }
        let ctx = self
            .ctx_mgr
            .get_or_create_with_identity(&request.session_id, identity_profile_id)
            .await?;
        let reuse_checkpoint_tab = should_reuse_checkpoint_tab(
            &request.identity_resume_decision,
            resolved_replacement_auth,
        );
        let tab_id = if reuse_checkpoint_tab {
            if let Some(tab_id) = resume_checkpoint
                .as_ref()
                .and_then(|checkpoint| checkpoint.active_tab_id.clone())
            {
                tab_id
            } else if let Some(start_url) = request.start_url.as_deref() {
                ctx.navigate("new", start_url, self.ctx_mgr.app_handle())
                    .await?
            } else {
                ctx.active_or_first_tab_id()
                    .await
                    .ok_or_else(|| anyhow!("No browser tab is available for task"))?
            }
        } else if let Some(start_url) = request.start_url.as_deref() {
            ctx.navigate("new", start_url, self.ctx_mgr.app_handle())
                .await?
        } else {
            ctx.active_or_first_tab_id()
                .await
                .ok_or_else(|| anyhow!("No browser tab is available for task"))?
        };

        let mut active_tab_id = tab_id;
        if should_apply_auth_profile(&request, resolved_replacement_auth) {
            if let Some((profile, state)) = auth_profile.as_ref() {
                ctx.apply_storage_state(&active_tab_id, state, self.ctx_mgr.app_handle())
                    .await?;
                self.push_step(
                    &mut run,
                    BrowserTaskStep {
                        step_index,
                        phase: BrowserTaskStepPhase::Act,
                        observation_summary: String::new(),
                        reasoning: "Applied authorized browser auth profile before task startup."
                            .to_string(),
                        action_name: "browser_auth_profile_apply".to_string(),
                        action_args: serde_json::json!({
                            "profileId": profile.id.clone(),
                            "label": profile.label.clone(),
                            "originPattern": profile.origin_pattern.clone(),
                            "resumeDecision": request.identity_resume_decision.clone(),
                        }),
                        ok: true,
                        message: Some(format!(
                            "Applied auth profile '{}' for browser task.",
                            profile.label
                        )),
                        error: None,
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    },
                );
                self.record_auth_profile_applied(&run, profile, &active_tab_id)
                    .await;
                step_index += 1;
                if let Some(start_url) = request.start_url.as_deref() {
                    active_tab_id = ctx
                        .navigate(&active_tab_id, start_url, self.ctx_mgr.app_handle())
                        .await?;
                }
            }
        }
        if active_tab_id.is_empty() {
            active_tab_id = ctx
                .active_or_first_tab_id()
                .await
                .ok_or_else(|| anyhow!("No browser tab is available for task"))?;
        }
        let mut loop_detector = LoopDetector::default();
        let mut latest_memory = resume_checkpoint.and_then(|checkpoint| checkpoint.memory);
        let mut acknowledged_boundaries: HashSet<String> = HashSet::new();
        let mut provider_observation: Option<BrowserObservation> = None;

        'segments: loop {
            for _ in 0..segment_steps {
                if let Some(profile_id) = identity_profile_id {
                    if self
                        .checkpoint_if_identity_revoked(
                            &mut run,
                            profile_id,
                            step_index,
                            Some(&active_tab_id),
                            latest_memory.as_ref(),
                        )
                        .await?
                    {
                        return Ok(run);
                    }
                }

                let mut observation = if let Some(observation) = provider_observation.clone() {
                    observation
                } else {
                    match ctx.observe_with_visual(&active_tab_id, false, true).await {
                        Ok(observation) => observation,
                        Err(error) => {
                            let err = error.to_string();
                            match self
                                .recover_after_error(
                                    &mut run,
                                    &mut active_tab_id,
                                    step_index,
                                    &err,
                                    &None,
                                    identity_profile_id,
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
                                    self.persist_checkpoint(
                                        &run,
                                        step_index,
                                        Some(&active_tab_id),
                                        latest_memory.as_ref(),
                                        "observation failed",
                                        identity_profile_id,
                                    );
                                    self.record_final_state(&run).await;
                                    return Ok(run);
                                }
                            }
                        }
                    }
                };
                observation.screenshot_b64 = None;
                let mut observation_json = serde_json::to_value(&observation)?;
                strip_raw_screenshot(&mut observation_json);
                let memory = self.update_memory(&request, &observation_json);
                latest_memory = memory.clone().or(latest_memory);
                self.record_visual_observation(&run, &observation_json)
                    .await;
                self.push_step(
                    &mut run,
                    BrowserTaskStep {
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
                    },
                );
                step_index += 1;

                if let Some(boundary) = detect_intervention_boundary(&observation_json) {
                    let boundary_fingerprint = boundary_fingerprint(&boundary);
                    if acknowledged_boundaries.contains(&boundary_fingerprint) {
                        run.status = BrowserTaskStatus::NeedsUserIntervention;
                        self.push_step(&mut run, BrowserTaskStep {
                        step_index,
                        phase: BrowserTaskStepPhase::UserIntervention,
                        observation_summary: summarize_observation(&observation),
                        reasoning: "The same browser boundary is still present after the user chose to continue.".to_string(),
                        action_name: "boundary_still_present_after_continue".to_string(),
                        action_args: serde_json::to_value(&boundary)?,
                        ok: false,
                        message: Some(boundary.reason.clone()),
                        error: Some(
                            "Manual intervention was marked as handled, but the same boundary is still visible; stopping to avoid an ask_user loop.".to_string(),
                        ),
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    });
                        self.emit_run(&run);
                        self.persist_checkpoint(
                            &run,
                            step_index,
                            Some(&active_tab_id),
                            latest_memory.as_ref(),
                            "human boundary still present after continue",
                            identity_profile_id,
                        );
                        self.record_final_state(&run).await;
                        return Ok(run);
                    }
                    let reason = boundary.reason.clone();
                    run.status = BrowserTaskStatus::NeedsUserIntervention;
                    self.record_boundary(&run, &boundary).await;
                    self.push_step(
                        &mut run,
                        BrowserTaskStep {
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
                        },
                    );
                    self.emit_run(&run);
                    self.persist_checkpoint(
                        &run,
                        step_index,
                        Some(&active_tab_id),
                        latest_memory.as_ref(),
                        "human boundary",
                        identity_profile_id,
                    );
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
                                acknowledged_boundaries.insert(boundary_fingerprint);
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
                self.push_step(
                    &mut run,
                    BrowserTaskStep {
                        step_index,
                        phase: BrowserTaskStepPhase::Decide,
                        observation_summary: summarize_observation(&observation),
                        reasoning: decision.reasoning.clone(),
                        action_name: "decide".to_string(),
                        action_args: serde_json::to_value(&decision.action)?,
                        ok: !matches!(
                            decision.status,
                            BrowserDecisionStatus::Failed
                                | BrowserDecisionStatus::NeedsUserIntervention
                        ),
                        message: decision.final_answer.clone(),
                        error: if matches!(
                            decision.status,
                            BrowserDecisionStatus::Failed
                                | BrowserDecisionStatus::NeedsUserIntervention
                        ) {
                            decision.final_answer.clone()
                        } else {
                            None
                        },
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    },
                );
                step_index += 1;

                match decision.status {
                    BrowserDecisionStatus::Done => {
                        run.status = BrowserTaskStatus::Completed;
                        self.push_step(
                            &mut run,
                            BrowserTaskStep {
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
                            },
                        );
                        self.emit_run(&run);
                        self.persist_checkpoint(
                            &run,
                            step_index,
                            Some(&active_tab_id),
                            latest_memory.as_ref(),
                            "completed",
                            identity_profile_id,
                        );
                        self.record_final_state(&run).await;
                        return Ok(run);
                    }
                    BrowserDecisionStatus::Failed => {
                        run.status = BrowserTaskStatus::Failed;
                        self.emit_run(&run);
                        self.persist_checkpoint(
                            &run,
                            step_index,
                            Some(&active_tab_id),
                            latest_memory.as_ref(),
                            "failed",
                            identity_profile_id,
                        );
                        self.record_final_state(&run).await;
                        return Ok(run);
                    }
                    BrowserDecisionStatus::NeedsUserIntervention => {
                        let reason = decision.final_answer.clone().unwrap_or_else(|| {
                            "Browser decision requested user intervention.".to_string()
                        });
                        run.status = BrowserTaskStatus::NeedsUserIntervention;
                        self.push_step(
                            &mut run,
                            BrowserTaskStep {
                                step_index,
                                phase: BrowserTaskStepPhase::UserIntervention,
                                observation_summary: summarize_observation(&observation),
                                reasoning: "Browser decision requested user intervention."
                                    .to_string(),
                                action_name: "needs_user_intervention".to_string(),
                                action_args: serde_json::json!({ "task": request.task }),
                                ok: false,
                                message: Some(reason.clone()),
                                error: None,
                                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                            },
                        );
                        self.emit_run(&run);
                        self.persist_checkpoint(
                            &run,
                            step_index,
                            Some(&active_tab_id),
                            latest_memory.as_ref(),
                            "decision requested intervention",
                            identity_profile_id,
                        );
                        if let Some(user_decision) =
                            self.ask_for_intervention(&run, &reason).await?
                        {
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

                let action = decision.action.clone().ok_or_else(|| {
                    anyhow!("browser decision status=continue but action was null")
                })?;
                let action_args = serde_json::to_value(&action)?;
                let action_args_text = serde_json::to_string(&action_args)?;
                let fingerprint =
                    make_fingerprint(&observation.url, action_name(&action), &action_args_text);
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
                    self.persist_checkpoint(
                        &run,
                        step_index,
                        Some(&active_tab_id),
                        latest_memory.as_ref(),
                        "loop detected",
                        identity_profile_id,
                    );
                    self.record_final_state(&run).await;
                    return Ok(run);
                }

                let runtime_executor = BrowserRuntimeActionExecutor::new(
                    Arc::clone(&self.ctx_mgr),
                    self.runtime_status_service.clone(),
                )
                .with_provider_config(self.runtime_provider_config.clone());
                let runtime_executor = if let Some(mcp_manager) = self.mcp_manager.as_ref() {
                    runtime_executor.with_mcp_manager(mcp_manager.clone())
                } else {
                    runtime_executor
                };
                // Slice 1b follow-up — thread safety fields through to the executor so
                // the Evaluate-gate in execute_action has access to SafetyManager +
                // ApprovalHandler. conversation_id = session_id (browser sub-loop runs
                // within a chat session); browser_task_id = run.run_id (per-task).
                let runtime_executor = runtime_executor
                    .with_safety_manager(self.safety_manager.clone())
                    .with_approval_handler(self.approval_handler.clone())
                    .with_approval_context(Some(EvaluateApprovalContext {
                        conversation_id: request.session_id.clone(),
                        browser_task_id: run.run_id.clone(),
                    }));
                match runtime_executor
                    .execute_action(BrowserRuntimeActionRequest {
                        session_id: request.session_id.clone(),
                        identity_profile_id: identity_profile_id.map(str::to_string),
                        task_id: run.run_id.clone(),
                        action: action.clone(),
                    })
                    .await
                {
                    Ok(execution) => match execution.outcome {
                        BrowserRuntimeActionExecutionOutcome::Executed(result) => {
                            if let Some(tab_id) = result.tab_id.as_ref() {
                                active_tab_id = tab_id.clone();
                            } else if let Some(tab_id) = tab_id_from_action(&action) {
                                active_tab_id = tab_id;
                            }
                            provider_observation = provider_observation_from_action_result(
                                &request.session_id,
                                &result,
                            )
                            .or(provider_observation);
                            self.push_step(
                                &mut run,
                                BrowserTaskStep {
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
                                },
                            );
                            step_index += 1;
                        }
                        BrowserRuntimeActionExecutionOutcome::Blocked(blocked) => {
                            run.status = BrowserTaskStatus::Failed;
                            self.push_step(
                                &mut run,
                                provider_route_blocked_step(
                                    step_index,
                                    &observation,
                                    &action,
                                    &execution.route_decision,
                                    &blocked,
                                )?,
                            );
                            self.emit_run(&run);
                            self.persist_checkpoint(
                                &run,
                                step_index,
                                Some(&active_tab_id),
                                latest_memory.as_ref(),
                                "provider route blocked",
                                identity_profile_id,
                            );
                            self.record_final_state(&run).await;
                            return Ok(run);
                        }
                    },
                    Err(error) => {
                        let err = error.to_string();
                        self.push_step(
                            &mut run,
                            BrowserTaskStep {
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
                            },
                        );
                        step_index += 1;

                        match self
                            .recover_after_error(
                                &mut run,
                                &mut active_tab_id,
                                step_index,
                                &err,
                                &Some(action.clone()),
                                identity_profile_id,
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
                                self.persist_checkpoint(
                                    &run,
                                    step_index,
                                    Some(&active_tab_id),
                                    latest_memory.as_ref(),
                                    "recovery stopped",
                                    identity_profile_id,
                                );
                                self.record_final_state(&run).await;
                                return Ok(run);
                            }
                        }
                    }
                }
            }

            run.status = BrowserTaskStatus::PausedCheckpointed;
            self.push_step(
                &mut run,
                BrowserTaskStep {
                    step_index,
                    phase: BrowserTaskStepPhase::Done,
                    observation_summary: String::new(),
                    reasoning: format!(
                        "Paused at checkpoint after reaching max_steps={segment_steps}."
                    ),
                    action_name: "checkpoint_pause".to_string(),
                    action_args: serde_json::json!({ "maxSteps": segment_steps }),
                    ok: false,
                    message: None,
                    error: Some(
                        "Browser task reached max_steps and saved a resumable checkpoint."
                            .to_string(),
                    ),
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                },
            );
            self.persist_checkpoint(
                &run,
                step_index,
                Some(&active_tab_id),
                latest_memory.as_ref(),
                "max steps reached",
                identity_profile_id,
            );
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
        self.push_step(
            run,
            BrowserTaskStep {
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
            },
        );
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
                .ask(BrowserInterventionPrompt::human_boundary(
                    &run.run_id,
                    reason,
                ))
                .await?,
        ))
    }

    async fn ask_for_checkpoint(
        &self,
        run: &BrowserTaskRun,
    ) -> Result<Option<BrowserInterventionDecision>> {
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
        identity_profile_id: Option<&str>,
    ) -> Result<RecoveryOutcome> {
        let kind = classify_browser_error(error);
        let (ok, message) = match kind {
            BrowserRecoveryKind::RefreshTabsAndRetry => {
                let ctx = self
                    .ctx_mgr
                    .get_or_create_with_identity(&run.session_id, identity_profile_id)
                    .await?;
                if let Some(tab_id) = ctx.active_or_first_tab_id().await {
                    *active_tab_id = tab_id;
                    (
                        true,
                        "Refreshed active tab id; retrying with a fresh observation.".to_string(),
                    )
                } else {
                    (
                        false,
                        "No live browser tab remains after refresh.".to_string(),
                    )
                }
            }
            BrowserRecoveryKind::RefreshDomAndRetry => {
                let ctx = self
                    .ctx_mgr
                    .get_or_create_with_identity(&run.session_id, identity_profile_id)
                    .await?;
                ctx.invalidate_dom_cache(active_tab_id).await;
                (
                    true,
                    "Invalidated DOM cache; retrying with a fresh observation.".to_string(),
                )
            }
            BrowserRecoveryKind::WaitAndRetry => {
                sleep(Duration::from_millis(800)).await;
                (
                    true,
                    "Waited for page stability; retrying with a fresh observation.".to_string(),
                )
            }
            BrowserRecoveryKind::Stop => (
                false,
                "Error is not recoverable by the browser agent.".to_string(),
            ),
        };

        self.push_step(
            run,
            BrowserTaskStep {
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
            },
        );

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
        let _ = self.ctx_mgr.app_handle().emit(
            "browser:task-step",
            serde_json::json!({
                "runId": run.run_id,
                "sessionId": run.session_id,
                "status": run.status,
                "step": step,
            }),
        );
    }

    fn emit_run(&self, run: &BrowserTaskRun) {
        if let Some(registry) = self.identity_task_registry.as_ref() {
            registry.update_status(&run.run_id, run.status.clone());
        }
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

    async fn record_auth_profile_applied(
        &self,
        run: &BrowserTaskRun,
        profile: &BrowserIdentityProfile,
        tab_id: &str,
    ) {
        if let Some(adapter) = self.long_term_memory.as_ref() {
            adapter
                .record_auth_profile_applied(run, profile, tab_id)
                .await;
        }
    }

    async fn record_visual_observation(
        &self,
        run: &BrowserTaskRun,
        observation_json: &serde_json::Value,
    ) {
        if let Some(adapter) = self.long_term_memory.as_ref() {
            adapter
                .record_visual_observation(run, observation_json)
                .await;
        }
    }

    async fn record_boundary(
        &self,
        run: &BrowserTaskRun,
        boundary: &crate::browser::boundary::BrowserBoundaryEvent,
    ) {
        if let Some(adapter) = self.long_term_memory.as_ref() {
            adapter.record_boundary(run, boundary).await;
        }
    }

    async fn record_final_state(&self, run: &BrowserTaskRun) {
        if let Some(adapter) = self.long_term_memory.as_ref() {
            adapter.record_final_state(run).await;
        }
    }

    async fn checkpoint_if_identity_revoked(
        &self,
        run: &mut BrowserTaskRun,
        profile_id: &str,
        step_index: u32,
        active_tab_id: Option<&str>,
        memory: Option<&BrowserTaskMemory>,
    ) -> Result<bool> {
        let Some(registry) = self.identity_task_registry.as_ref() else {
            return Ok(false);
        };
        let decision = registry.revocation_decision(profile_id);
        let drain_deadline_ms = match decision {
            BrowserIdentityRevocationDecision::NotRevoked => return Ok(false),
            BrowserIdentityRevocationDecision::Draining { drain_deadline_ms }
            | BrowserIdentityRevocationDecision::CheckpointRequired { drain_deadline_ms } => {
                drain_deadline_ms
            }
        };

        run.status = BrowserTaskStatus::PausedCheckpointed;
        self.push_step(
            run,
            identity_revocation_checkpoint_step(step_index, profile_id, drain_deadline_ms),
        );
        self.emit_run(run);
        self.persist_checkpoint(
            run,
            step_index,
            active_tab_id,
            memory,
            "browser identity revoked",
            Some(profile_id),
        );
        self.record_final_state(run).await;
        Ok(true)
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

    fn identity_profile_is_revoked(&self, profile_id: &str) -> Result<bool> {
        let Some(broker) = self.auth_profile_broker.as_ref() else {
            return Ok(false);
        };
        Ok(broker
            .list_profiles()
            .map_err(|e| anyhow!("list browser auth profiles: {e}"))?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .map(|profile| profile.is_revoked())
            .unwrap_or(false))
    }

    fn persist_checkpoint(
        &self,
        run: &BrowserTaskRun,
        step_index: u32,
        active_tab_id: Option<&str>,
        memory: Option<&BrowserTaskMemory>,
        reason: &str,
        identity_profile_id: Option<&str>,
    ) {
        if let Some(store) = self.task_store.as_ref() {
            let mut loop_state = serde_json::json!({
                "status": run.status,
                "stepCount": run.steps.len(),
                "reason": reason,
            });
            if let Some(object) = loop_state.as_object_mut() {
                if let Some(profile_id) = identity_profile_id {
                    object.insert(
                        "identityProfileId".to_string(),
                        serde_json::Value::String(profile_id.to_string()),
                    );
                }
                if reason.starts_with("browser identity revoked") {
                    object.insert("identityRevoked".to_string(), serde_json::Value::Bool(true));
                }
            }
            if let Err(e) =
                store.persist_checkpoint(run, step_index, active_tab_id, memory, loop_state)
            {
                tracing::warn!(run_id = %run.run_id, error = %e, "failed to persist browser task checkpoint");
            }
        }
        if let Some(adapter) = self.long_term_memory.clone() {
            let run = run.clone();
            let active_tab_id = active_tab_id.map(str::to_string);
            let memory = memory.cloned();
            let reason = reason.to_string();
            tauri::async_runtime::spawn(async move {
                adapter
                    .record_checkpoint(
                        &run,
                        step_index,
                        active_tab_id.as_deref(),
                        memory.as_ref(),
                        &reason,
                    )
                    .await;
            });
        }
    }

    fn resolve_auth_profile(
        &self,
        request: &BrowserTaskRequest,
    ) -> Result<Option<(BrowserIdentityProfile, PlaywrightStorageState)>> {
        let Some(broker) = self.auth_profile_broker.as_ref() else {
            if request.auth_profile_id.is_some() || request.auth_origin.is_some() {
                tracing::warn!(
                    session_id = %request.session_id,
                    "browser auth profile requested but broker is unavailable"
                );
            }
            return Ok(None);
        };

        if let Some(profile_id) = request.auth_profile_id.as_deref() {
            return broker
                .load_storage_state_for_profile(profile_id)
                .map(Some)
                .map_err(|e| anyhow!("load browser auth profile '{profile_id}': {e}"));
        }

        let lookup_origin = request
            .auth_origin
            .as_deref()
            .or(request.start_url.as_deref())
            .and_then(normalize_origin_for_lookup);
        let Some(origin) = lookup_origin else {
            return Ok(None);
        };

        broker
            .resolve_storage_state_for_origin(&origin)
            .map_err(|e| anyhow!("resolve browser auth profile for '{origin}': {e}"))
    }
}

enum RecoveryOutcome {
    Continue(u32),
    Stop,
}

fn should_pause_for_runtime_preparation(request: &BrowserTaskRequest) -> bool {
    matches!(
        request.runtime_preparation_decision,
        BrowserTaskRuntimePreparationDecision::Defer
    )
}

fn should_block_revoked_identity_resume(
    decision: &BrowserIdentityResumeDecision,
    resolved_replacement_auth: bool,
) -> bool {
    !resolved_replacement_auth
        && !matches!(decision, BrowserIdentityResumeDecision::IsolatedProfile)
}

fn should_inherit_checkpoint_identity(decision: &BrowserIdentityResumeDecision) -> bool {
    !matches!(decision, BrowserIdentityResumeDecision::IsolatedProfile)
}

fn should_reuse_checkpoint_tab(
    decision: &BrowserIdentityResumeDecision,
    resolved_replacement_auth: bool,
) -> bool {
    !resolved_replacement_auth
        && !matches!(decision, BrowserIdentityResumeDecision::IsolatedProfile)
}

fn should_apply_auth_profile(
    request: &BrowserTaskRequest,
    resolved_replacement_auth: bool,
) -> bool {
    request.resume_run_id.is_none() || resolved_replacement_auth
}

fn runtime_preparation_pause_step(
    step_index: u32,
    request: &BrowserTaskRequest,
) -> BrowserTaskStep {
    BrowserTaskStep {
        step_index,
        phase: BrowserTaskStepPhase::UserIntervention,
        observation_summary: String::new(),
        reasoning: "Browser runtime preparation was deferred before browser automation started."
            .to_string(),
        action_name: "browser_runtime_preparation_deferred".to_string(),
        action_args: serde_json::json!({
            "decision": "defer",
            "task": request.task,
        }),
        ok: false,
        message: Some(
            "Browser runtime preparation was deferred; resume after runtime setup is ready."
                .to_string(),
        ),
        error: Some(
            "Browser task paused while waiting for Browser runtime preparation.".to_string(),
        ),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    }
}

fn identity_revocation_checkpoint_step(
    step_index: u32,
    profile_id: &str,
    drain_deadline_ms: i64,
) -> BrowserTaskStep {
    BrowserTaskStep {
        step_index,
        phase: BrowserTaskStepPhase::UserIntervention,
        observation_summary: String::new(),
        reasoning: "Browser identity was revoked; pausing task at the next safe action boundary."
            .to_string(),
        action_name: "browser_identity_revoked_checkpoint".to_string(),
        action_args: serde_json::json!({
            "profileId": profile_id,
            "drainDeadlineMs": drain_deadline_ms,
        }),
        ok: false,
        message: Some(
            "Browser identity was revoked; the task was checkpointed before more browser actions."
                .to_string(),
        ),
        error: Some(
            "Browser task paused because its authorized browser identity was revoked.".to_string(),
        ),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    }
}

fn identity_revocation_resume_blocked_step(step_index: u32, profile_id: &str) -> BrowserTaskStep {
    BrowserTaskStep {
        step_index,
        phase: BrowserTaskStepPhase::UserIntervention,
        observation_summary: String::new(),
        reasoning: "Browser identity was revoked; resume is blocked until an explicit replacement identity is provided.".to_string(),
        action_name: "browser_identity_revoked_resume_blocked".to_string(),
        action_args: serde_json::json!({
            "profileId": profile_id,
            "availableDecisions": ["isolated_profile", "reauthorize", "end_task"],
        }),
        ok: false,
        message: Some(
            "Browser identity was revoked; choose isolated profile, reauthorize, or end the task."
                .to_string(),
        ),
        error: Some(
            "Browser task resume blocked because its authorized browser identity was revoked."
                .to_string(),
        ),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    }
}

fn identity_reauthorize_missing_step(step_index: u32, profile_id: Option<&str>) -> BrowserTaskStep {
    BrowserTaskStep {
        step_index,
        phase: BrowserTaskStepPhase::UserIntervention,
        observation_summary: String::new(),
        reasoning:
            "Reauthorization was selected, but no replacement browser identity was provided."
                .to_string(),
        action_name: "browser_identity_reauthorize_missing_auth".to_string(),
        action_args: serde_json::json!({
            "profileId": profile_id,
            "requiredInputs": ["auth_profile_id", "auth_origin"],
            "availableDecisions": ["isolated_profile", "reauthorize", "end_task"],
        }),
        ok: false,
        message: Some(
            "Choose a replacement browser identity before reauthorizing this task.".to_string(),
        ),
        error: Some(
            "Browser task reauthorize decision requires auth_profile_id or auth_origin."
                .to_string(),
        ),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    }
}

fn identity_resume_end_task_step(step_index: u32, profile_id: Option<&str>) -> BrowserTaskStep {
    BrowserTaskStep {
        step_index,
        phase: BrowserTaskStepPhase::UserIntervention,
        observation_summary: String::new(),
        reasoning: "User chose to end a browser task stopped at an identity boundary.".to_string(),
        action_name: "browser_identity_boundary_end_task".to_string(),
        action_args: serde_json::json!({
            "profileId": profile_id,
            "decision": "end_task",
        }),
        ok: true,
        message: Some("Browser task ended at the identity boundary.".to_string()),
        error: None,
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    }
}

fn checkpoint_identity_profile_id(
    checkpoint: Option<&crate::browser::task_store::BrowserTaskCheckpoint>,
) -> Option<&str> {
    checkpoint?
        .loop_state
        .get("identityProfileId")
        .and_then(|value| value.as_str())
}

fn checkpoint_marks_identity_revoked(
    checkpoint: Option<&crate::browser::task_store::BrowserTaskCheckpoint>,
) -> bool {
    checkpoint
        .and_then(|checkpoint| checkpoint.loop_state.get("identityRevoked"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
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
        | BrowserAction::Screenshot { tab_id, .. }
        | BrowserAction::SwitchTab { tab_id } => Some(tab_id.clone()),
        BrowserAction::UploadFile { tab_id, .. } => Some(tab_id.clone()),
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
        BrowserAction::Screenshot { .. } => "browser_screenshot",
        BrowserAction::ListTabs => "browser_list_tabs",
        BrowserAction::SwitchTab { .. } => "browser_switch_tab",
        BrowserAction::CloseTab { .. } => "browser_close_tab",
        BrowserAction::UploadFile { .. } => "browser_upload_file",
    }
}

fn provider_route_blocked_step(
    step_index: u32,
    observation: &BrowserObservation,
    action: &BrowserAction,
    decision: &BrowserProviderRouteDecision,
    blocked: &BrowserProviderActionBlocked,
) -> Result<BrowserTaskStep> {
    Ok(BrowserTaskStep {
        step_index,
        phase: BrowserTaskStepPhase::Recover,
        observation_summary: summarize_observation(observation),
        reasoning: "Provider route selected a provider/action boundary that could not execute this browser action.".to_string(),
        action_name: "browser_provider_route_blocked".to_string(),
        action_args: serde_json::json!({
            "action": serde_json::to_value(action)?,
            "routeStatus": decision.status,
            "selectedProviderId": decision.selected_provider_id.as_deref(),
            "blockedProviderId": blocked.selected_provider_id.as_deref(),
            "candidates": decision.candidates,
        }),
        ok: false,
        message: None,
        error: Some(blocked.message.clone()),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    })
}

fn provider_observation_from_action_result(
    session_id: &str,
    result: &crate::browser::action::BrowserActionResult,
) -> Option<BrowserObservation> {
    let output = result
        .observation_json
        .as_ref()
        .and_then(|value| value.get("output"))?;
    let tab_id = result.tab_id.clone().or_else(|| {
        output
            .get("tabId")
            .and_then(|value| value.as_str())
            .map(str::to_string)
    })?;
    let url = output
        .get("url")
        .and_then(|value| value.as_str())
        .unwrap_or("about:blank")
        .to_string();
    let title = output
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let page_text = output
        .get("pageText")
        .and_then(|value| value.as_str())
        .or_else(|| output.get("stdout").and_then(|value| value.as_str()))
        .unwrap_or("")
        .to_string();
    Some(BrowserObservation {
        session_id: session_id.to_string(),
        tab_id: tab_id.clone(),
        url: url.clone(),
        title: title.clone(),
        page_text,
        elements: Vec::new(),
        tabs: vec![TabInfo {
            tab_id,
            url,
            title,
            active: true,
        }],
        screenshot_b64: None,
        visual_observation: None,
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    })
}

fn strip_raw_screenshot(value: &mut serde_json::Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("screenshotB64");
    }
}

fn boundary_fingerprint(boundary: &BrowserBoundaryEvent) -> String {
    format!(
        "{:?}|{}|{}",
        boundary.kind,
        normalize_origin_for_lookup(&boundary.url).unwrap_or_else(|| boundary.url.clone()),
        boundary.reason
    )
}

fn normalize_origin_for_lookup(input: &str) -> Option<String> {
    if input.trim().is_empty() {
        return None;
    }
    if let Ok(url) = url::Url::parse(input) {
        let host = url.host_str()?;
        let mut origin = format!("{}://{}", url.scheme(), host);
        if let Some(port) = url.port() {
            origin.push(':');
            origin.push_str(&port.to_string());
        }
        return Some(origin);
    }
    Some(input.trim().to_string())
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
            visual_observation: None,
            timestamp_ms: 123,
        };

        let summary = summarize_observation(&observation);

        assert!(summary.contains("中文页面"));
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn provider_observation_uses_playwright_cli_output_state() {
        let result = crate::browser::action::BrowserActionResult {
            ok: true,
            action_name: "browser_playwright_cli_navigate".to_string(),
            message: Some("Navigated with Playwright CLI.".to_string()),
            tab_id: Some("playwright-cli:uclaw-session-1".to_string()),
            observation_json: Some(serde_json::json!({
                "output": {
                    "tabId": "playwright-cli:uclaw-session-1",
                    "url": "https://example.com/",
                    "title": "Example Domain",
                    "pageText": "Example page text"
                }
            })),
            error: None,
            duration_ms: 12,
        };

        let observation =
            provider_observation_from_action_result("session-1", &result).expect("observation");

        assert_eq!(observation.session_id, "session-1");
        assert_eq!(observation.tab_id, "playwright-cli:uclaw-session-1");
        assert_eq!(observation.url, "https://example.com/");
        assert_eq!(observation.title, "Example Domain");
        assert_eq!(observation.page_text, "Example page text");
        assert_eq!(observation.tabs.len(), 1);
        assert!(observation.tabs[0].active);
    }

    #[test]
    fn max_steps_bounds_task_loop() {
        assert_eq!(super::clamp_max_steps(Some(0)), 1);
        assert_eq!(super::clamp_max_steps(Some(8)), 8);
        assert_eq!(super::clamp_max_steps(Some(100)), 25);
    }

    #[test]
    fn normalize_origin_strips_url_path_for_profile_lookup() {
        assert_eq!(
            normalize_origin_for_lookup("https://app.example.com/login?x=1").as_deref(),
            Some("https://app.example.com")
        );
        assert_eq!(
            normalize_origin_for_lookup("http://localhost:3000/path").as_deref(),
            Some("http://localhost:3000")
        );
    }

    #[test]
    fn runtime_preparation_decision_defaults_to_ready() {
        let request: BrowserTaskRequest = serde_json::from_value(serde_json::json!({
            "sessionId": "s1",
            "task": "check a page",
            "maxSteps": 4,
            "startUrl": null,
            "availableFilePaths": [],
            "resumeRunId": null,
            "authProfileId": null,
            "authOrigin": null
        }))
        .expect("request should deserialize with default runtime decision");

        assert_eq!(
            request.runtime_preparation_decision,
            BrowserTaskRuntimePreparationDecision::Ready
        );
        assert_eq!(
            request.identity_resume_decision,
            BrowserIdentityResumeDecision::RequireAuth
        );
        assert!(!should_pause_for_runtime_preparation(&request));
    }

    #[test]
    fn deferred_runtime_preparation_requests_pause_checkpoint() {
        let request = BrowserTaskRequest {
            session_id: "s1".to_string(),
            task: "open the dashboard".to_string(),
            max_steps: Some(8),
            start_url: Some("https://example.com".to_string()),
            available_file_paths: Vec::new(),
            resume_run_id: None,
            auth_profile_id: None,
            auth_origin: None,
            runtime_preparation_decision: BrowserTaskRuntimePreparationDecision::Defer,
            identity_resume_decision: BrowserIdentityResumeDecision::RequireAuth,
        };

        assert!(should_pause_for_runtime_preparation(&request));

        let step = runtime_preparation_pause_step(3, &request);

        assert_eq!(step.step_index, 3);
        assert_eq!(step.phase, BrowserTaskStepPhase::UserIntervention);
        assert_eq!(
            step.action_name,
            "browser_runtime_preparation_deferred".to_string()
        );
        assert_eq!(step.action_args["decision"], serde_json::json!("defer"));
        assert_eq!(
            step.action_args["task"],
            serde_json::json!("open the dashboard")
        );
        assert!(!step.ok);
        assert!(step
            .error
            .as_deref()
            .expect("pause step should have error copy")
            .contains("waiting for Browser runtime preparation"));
    }

    #[test]
    fn revoked_identity_resume_blocks_unless_isolated_or_explicit_auth() {
        assert!(should_block_revoked_identity_resume(
            &BrowserIdentityResumeDecision::RequireAuth,
            false
        ));
        assert!(should_block_revoked_identity_resume(
            &BrowserIdentityResumeDecision::Reauthorize,
            false
        ));
        assert!(!should_block_revoked_identity_resume(
            &BrowserIdentityResumeDecision::IsolatedProfile,
            false
        ));
        assert!(!should_block_revoked_identity_resume(
            &BrowserIdentityResumeDecision::RequireAuth,
            true
        ));
    }

    #[test]
    fn isolated_profile_resume_does_not_inherit_checkpoint_identity() {
        assert!(should_inherit_checkpoint_identity(
            &BrowserIdentityResumeDecision::RequireAuth
        ));
        assert!(should_inherit_checkpoint_identity(
            &BrowserIdentityResumeDecision::Reauthorize
        ));
        assert!(!should_inherit_checkpoint_identity(
            &BrowserIdentityResumeDecision::IsolatedProfile
        ));
    }

    #[test]
    fn identity_boundary_context_switch_does_not_reuse_checkpoint_tab() {
        assert!(should_reuse_checkpoint_tab(
            &BrowserIdentityResumeDecision::RequireAuth,
            false
        ));
        assert!(!should_reuse_checkpoint_tab(
            &BrowserIdentityResumeDecision::IsolatedProfile,
            false
        ));
        assert!(!should_reuse_checkpoint_tab(
            &BrowserIdentityResumeDecision::Reauthorize,
            true
        ));
    }

    #[test]
    fn resume_only_applies_auth_after_replacement_auth_resolves() {
        let base = BrowserTaskRequest {
            session_id: "s1".to_string(),
            task: "resume".to_string(),
            max_steps: Some(1),
            start_url: None,
            available_file_paths: Vec::new(),
            resume_run_id: Some("run-1".to_string()),
            auth_profile_id: Some("auth-2".to_string()),
            auth_origin: None,
            runtime_preparation_decision: BrowserTaskRuntimePreparationDecision::Ready,
            identity_resume_decision: BrowserIdentityResumeDecision::Reauthorize,
        };

        assert!(should_apply_auth_profile(&base, true));
        assert!(!should_apply_auth_profile(&base, false));

        let fresh = BrowserTaskRequest {
            resume_run_id: None,
            ..base
        };
        assert!(should_apply_auth_profile(&fresh, false));
    }

    #[test]
    fn identity_revocation_checkpoint_step_records_profile_boundary() {
        let step = identity_revocation_checkpoint_step(4, "auth-1", 1_770_000_000_000);

        assert_eq!(step.step_index, 4);
        assert_eq!(step.phase, BrowserTaskStepPhase::UserIntervention);
        assert_eq!(step.action_name, "browser_identity_revoked_checkpoint");
        assert_eq!(step.action_args["profileId"], serde_json::json!("auth-1"));
        assert_eq!(
            step.action_args["drainDeadlineMs"],
            serde_json::json!(1_770_000_000_000_i64)
        );
        assert!(!step.ok);
        assert!(step
            .error
            .as_deref()
            .expect("revocation step error")
            .contains("identity was revoked"));
    }

    #[test]
    fn identity_revocation_resume_blocked_step_requires_reauth() {
        let step = identity_revocation_resume_blocked_step(7, "auth-revoked");

        assert_eq!(step.step_index, 7);
        assert_eq!(step.phase, BrowserTaskStepPhase::UserIntervention);
        assert_eq!(step.action_name, "browser_identity_revoked_resume_blocked");
        assert_eq!(
            step.action_args["profileId"],
            serde_json::json!("auth-revoked")
        );
        assert_eq!(
            step.action_args["availableDecisions"],
            serde_json::json!(["isolated_profile", "reauthorize", "end_task"])
        );
        assert!(!step.ok);
        assert!(step
            .error
            .as_deref()
            .expect("resume blocked error")
            .contains("resume blocked"));
    }

    #[test]
    fn identity_reauthorize_missing_step_requires_replacement_auth() {
        let step = identity_reauthorize_missing_step(8, Some("auth-revoked"));

        assert_eq!(step.step_index, 8);
        assert_eq!(step.phase, BrowserTaskStepPhase::UserIntervention);
        assert_eq!(
            step.action_name,
            "browser_identity_reauthorize_missing_auth"
        );
        assert_eq!(
            step.action_args["requiredInputs"],
            serde_json::json!(["auth_profile_id", "auth_origin"])
        );
        assert!(!step.ok);
        assert!(step
            .error
            .as_deref()
            .expect("reauthorize missing error")
            .contains("auth_profile_id"));
    }

    #[test]
    fn identity_resume_end_task_step_records_boundary_decision() {
        let step = identity_resume_end_task_step(9, Some("auth-revoked"));

        assert_eq!(step.step_index, 9);
        assert_eq!(step.phase, BrowserTaskStepPhase::UserIntervention);
        assert_eq!(step.action_name, "browser_identity_boundary_end_task");
        assert_eq!(step.action_args["decision"], serde_json::json!("end_task"));
        assert_eq!(
            step.action_args["profileId"],
            serde_json::json!("auth-revoked")
        );
        assert!(step.ok);
        assert!(step.error.is_none());
    }

    #[test]
    fn checkpoint_identity_metadata_detects_revoked_profile() {
        let checkpoint = crate::browser::task_store::BrowserTaskCheckpoint {
            checkpoint_id: "checkpoint-1".to_string(),
            run_id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            step_index: 3,
            active_tab_id: Some("tab-1".to_string()),
            memory: None,
            loop_state: serde_json::json!({
                "identityProfileId": "auth-1",
                "identityRevoked": true,
            }),
            created_at: 1_770_000_000_000,
        };

        assert_eq!(
            checkpoint_identity_profile_id(Some(&checkpoint)),
            Some("auth-1")
        );
        assert!(checkpoint_marks_identity_revoked(Some(&checkpoint)));
        assert_eq!(checkpoint_identity_profile_id(None), None);
        assert!(!checkpoint_marks_identity_revoked(None));
    }
}

/// Contract test: the ToolDispatcher + ApprovalHandler chokepoint routes
/// correctly for BrowserSubLoop origin.  The test does NOT construct a full
/// BrowserAgentLoop::run (which requires live browser/Tauri deps); instead
/// it directly exercises the ToolDispatcher with
/// origin_kind = BrowserSubLoop, verifying that:
///   1. ApprovalHandler.handle_ask is called.
///   2. The origin delivered is ApprovalOrigin::BrowserSubLoop.
///
/// STRUCTURAL NOTE (Slice 1b): BrowserAgentLoop::run dispatches
/// BrowserAction::{Navigate,Click,Type,Evaluate,…} via
/// BrowserRuntimeActionExecutor, not ToolCall objects through ToolRegistry.
/// Task 3.3's "prepend dispatcher-routing branch" has no valid insertion site
/// in the current run() body — the sub-loop is a browser-only automation loop
/// with no general shell/file-system tool path.  The three struct fields
/// (safety_manager, tool_dispatcher, approval_handler) provide the infrastructure.
/// The Evaluate-gate is wired in BrowserRuntimeActionExecutor::execute_action,
/// consulting SafetyManager for arbitrary JS execution approvals. Production
/// injection happens at the three browser-task tool construction sites:
/// agent/tools/registry_build.rs, tauri_commands.rs::send_agent_message,
/// and browser/tools.rs::RetryWithBrowserAgentTool.
///
/// Known limitation: ChatApprovalHandler.handle_ask uses key
/// "browser-sub:{conversation_id}:{browser_task_id}" without including tc.id,
/// so two concurrent sub-loop tool calls in the same browser task share a key.
/// In practice the sub-loop dispatches serially per step, so no collision occurs.
#[cfg(test)]
mod safety_chokepoint_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use crate::agent::tool_dispatch::{
        ApprovalOriginKind, ToolDispatchContext, ToolDispatcher,
    };
    use crate::agent::types::ToolCall;
    use crate::agent::tools::tool::ToolRegistry;
    use crate::agent::hook_bus::HookBus;
    use crate::app::PendingApprovals;
    use crate::safety::{ApprovalHandler, ApprovalOrigin, ApprovalOutcome, SafetyManager, SafetyMode};
    use tauri::test::MockRuntime;

    struct ObservingApprovalHandler {
        called: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl ApprovalHandler for ObservingApprovalHandler {
        async fn handle_ask(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
            origin: &ApprovalOrigin,
        ) -> ApprovalOutcome {
            self.called.store(true, Ordering::SeqCst);
            assert!(
                matches!(origin, ApprovalOrigin::BrowserSubLoop { .. }),
                "browser sub-loop must route via BrowserSubLoop origin, got: {origin:?}"
            );
            ApprovalOutcome::Approved
        }
    }

    /// Locks the contract: any tool dispatch built with
    /// `origin_kind = BrowserSubLoop { .. }` goes through the same
    /// SafetyManager + ApprovalHandler chokepoint as chat/automation.
    /// An ObservingApprovalHandler verifies via AtomicBool that the
    /// handler was invoked AND that the origin is BrowserSubLoop.
    #[tokio::test]
    async fn subloop_dispatch_routes_through_outer_safetymanager() {
        let called = Arc::new(AtomicBool::new(false));

        let mut mgr = SafetyManager::new(&std::env::temp_dir());
        mgr.set_global_mode(SafetyMode::Ask).unwrap();
        let safety_manager = Arc::new(tokio::sync::RwLock::new(mgr));

        let app = tauri::test::mock_app();
        let pending_approvals = Arc::new(PendingApprovals::new());
        let hook_bus = Arc::new(HookBus::new());
        let observing = Arc::new(ObservingApprovalHandler { called: called.clone() });

        // Use AlwaysApprovalTool (pub(crate) from tool_dispatch::tests) so
        // SafetyManager returns RequireApproval and routes to the handler.
        let mut reg = ToolRegistry::new();
        reg.register(
            crate::agent::tool_dispatch::tests::AlwaysApprovalTool::new(
                Arc::new(AtomicBool::new(false)),
                "bash",
            ),
        );

        let dispatcher: Arc<ToolDispatcher<MockRuntime>> =
            Arc::new(ToolDispatcher::new_with_approval_handler(
                Arc::new(reg),
                app.handle().clone(),
                safety_manager,
                observing,
                pending_approvals,
                None, // infra_service
                None, // trajectory_store
                None, // tool_budget
                hook_bus,
                None, // heartbeat
            ));

        let ctx = ToolDispatchContext {
            session_id: "s".into(),
            conversation_id: "c1".into(),
            workspace_root: None,
            attached_dirs: vec![],
            safety_mode: None,
            iteration: 1,
            cancel: None,
            permissions: None,
            origin_kind: ApprovalOriginKind::BrowserSubLoop {
                conversation_id: "c1".into(),
                browser_task_id: "bt-1".into(),
            },
        };
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "bash".into(),
            arguments: serde_json::json!({}),
        }];
        let outs = dispatcher.dispatch(calls, &ctx).await;

        assert_eq!(outs.len(), 1, "one outcome per call");
        assert!(
            called.load(Ordering::SeqCst),
            "ObservingApprovalHandler.handle_ask must have been called for BrowserSubLoop origin"
        );
    }
}
