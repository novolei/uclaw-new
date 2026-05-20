use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::browser::agent_loop::BrowserAgentLoop;
use crate::browser::agent_loop::BrowserTaskRequest;
use crate::browser::identity::{
    BrowserAuthProfileBroker, BrowserIdentityKind, BrowserIdentityProfileInput,
    BrowserIdentityProvider, BrowserIdentityScope,
};
use crate::browser::session_state::{BrowserTaskRun, BrowserTaskStatus, BrowserTaskStep};
use crate::harness::adapters::{HarnessAdapter, BROWSER_ADAPTER_ID};
use crate::harness::case::{HarnessBudget, HarnessCase, HarnessPolicy, HarnessSubject};
use crate::harness::episode::HarnessVerdict;
use crate::harness::runtime::HarnessRuntime;
use crate::harness::trace::HarnessEvent;

pub const BUILTIN_BROWSER_PARITY_CASES: &[&str] = &[
    include_str!("../cases/browser/navigation.json"),
    include_str!("../cases/browser/multi-tab-planning.json"),
    include_str!("../cases/browser/file-upload.json"),
    include_str!("../cases/browser/auth-profile-restore.json"),
    include_str!("../cases/browser/boundary-detection.json"),
    include_str!("../cases/browser/checkpoint-resume.json"),
    include_str!("../cases/browser/long-task-recovery.json"),
];

#[derive(Debug, Default, Clone)]
pub struct BrowserHarnessAdapter;

impl HarnessAdapter for BrowserHarnessAdapter {
    fn subject(&self) -> HarnessSubject {
        HarnessSubject::Browser
    }

    fn adapter_id(&self) -> &'static str {
        BROWSER_ADAPTER_ID
    }
}

impl BrowserHarnessAdapter {
    pub fn load_builtin_cases() -> Result<Vec<BrowserParityCase>, serde_json::Error> {
        BUILTIN_BROWSER_PARITY_CASES
            .iter()
            .map(|raw| serde_json::from_str(raw))
            .collect()
    }

    pub fn score_run(&self, input: BrowserParityRunInput) -> BrowserParityScorecard {
        score_browser_run(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserParityCase {
    pub id: String,
    pub title: String,
    pub capability: BrowserParityCapability,
    pub prompt: String,
    #[serde(default)]
    pub start_url: Option<String>,
    #[serde(default)]
    pub max_steps: Option<u32>,
    #[serde(default)]
    pub available_file_paths: Vec<String>,
    #[serde(default)]
    pub auth_origin: Option<String>,
    #[serde(default)]
    pub auth_storage_state: Option<Value>,
    pub expected_status: BrowserTaskStatus,
    #[serde(default)]
    pub required_actions: Vec<String>,
    #[serde(default)]
    pub max_action_count: Option<u32>,
    #[serde(default)]
    pub expected_active_tab_id: Option<String>,
    #[serde(default)]
    pub expected_url_contains: Option<String>,
    #[serde(default)]
    pub expected_boundary_kind: Option<String>,
    #[serde(default)]
    pub min_tab_count: Option<u32>,
    #[serde(default)]
    pub required_file_path: Option<String>,
    #[serde(default)]
    pub require_auth_before_navigation: bool,
    #[serde(default)]
    pub require_checkpoint: bool,
    #[serde(default)]
    pub require_resume: bool,
    #[serde(default)]
    pub require_recovery: bool,
    #[serde(default)]
    pub require_failure_before_recovery: bool,
}

impl BrowserParityCase {
    pub fn materialize(&self, context: &BrowserParityFixtureContext) -> Self {
        let replace = |value: &str| {
            value
                .replace("{{fixtureBaseUrl}}", &context.fixture_base_url)
                .replace("{{workspaceFixtureFile}}", &context.workspace_fixture_file)
        };
        let mut case = self.clone();
        case.prompt = replace(&case.prompt);
        case.start_url = case.start_url.as_deref().map(replace);
        case.available_file_paths = case
            .available_file_paths
            .iter()
            .map(|path| replace(path))
            .collect();
        case.auth_origin = case.auth_origin.as_deref().map(replace);
        case.auth_storage_state = case
            .auth_storage_state
            .map(|value| replace_json_strings(value, &replace));
        case.expected_url_contains = case.expected_url_contains.as_deref().map(replace);
        case.required_file_path = case.required_file_path.as_deref().map(replace);
        case
    }

    pub fn to_task_request(&self, session_id: impl Into<String>) -> BrowserTaskRequest {
        BrowserTaskRequest {
            session_id: session_id.into(),
            task: self.prompt.clone(),
            max_steps: self.max_steps,
            start_url: self.start_url.clone(),
            available_file_paths: self.available_file_paths.clone(),
            resume_run_id: None,
            auth_profile_id: None,
            auth_origin: self.auth_origin.clone(),
        }
    }

    fn to_harness_case(&self) -> HarnessCase {
        HarnessCase {
            id: self.id.clone(),
            subject: HarnessSubject::Browser,
            title: self.title.clone(),
            prompt: self.prompt.clone(),
            setup: Vec::new(),
            policy: HarnessPolicy {
                permission_mode: "bypass".to_string(),
                allowed_tools: vec!["browser_task".to_string()],
                allow_network: true,
                allow_memory_writes: false,
            },
            budgets: HarnessBudget {
                max_steps: self.max_steps.unwrap_or(12),
                max_seconds: 120,
                max_tokens: None,
            },
            assertions: Vec::new(),
            graders: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BrowserParityFixtureContext {
    pub fixture_base_url: String,
    pub workspace_fixture_file: String,
}

impl BrowserParityFixtureContext {
    pub fn new(
        fixture_base_url: impl Into<String>,
        workspace_fixture_file: impl Into<String>,
    ) -> Self {
        Self {
            fixture_base_url: fixture_base_url.into().trim_end_matches('/').to_string(),
            workspace_fixture_file: workspace_fixture_file.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserParityCapability {
    Navigation,
    MultiTabPlanning,
    FileUpload,
    AuthProfileRestore,
    BoundaryDetection,
    CheckpointResume,
    LongTaskRecovery,
}

#[derive(Debug, Clone)]
pub struct BrowserParityRunInput {
    pub case: BrowserParityCase,
    pub run: BrowserTaskRun,
    pub active_tab_id: Option<String>,
    pub checkpoint_present: bool,
}

#[derive(Debug, Clone)]
pub struct BrowserParityRunOutput {
    pub run: BrowserTaskRun,
    pub active_tab_id: Option<String>,
    pub checkpoint_present: bool,
}

#[async_trait]
pub trait BrowserParityExecutor: Send + Sync {
    async fn prepare_case(&self, _case: &BrowserParityCase) -> anyhow::Result<()> {
        Ok(())
    }

    async fn execute_case(
        &self,
        case: &BrowserParityCase,
        request: BrowserTaskRequest,
    ) -> anyhow::Result<BrowserParityRunOutput>;
}

pub struct BrowserAgentLoopParityExecutor {
    agent_loop: Arc<BrowserAgentLoop>,
    auth_profile_broker: Option<Arc<BrowserAuthProfileBroker>>,
}

impl BrowserAgentLoopParityExecutor {
    pub fn new(
        agent_loop: Arc<BrowserAgentLoop>,
        auth_profile_broker: Option<Arc<BrowserAuthProfileBroker>>,
    ) -> Self {
        Self {
            agent_loop,
            auth_profile_broker,
        }
    }
}

#[async_trait]
impl BrowserParityExecutor for BrowserAgentLoopParityExecutor {
    async fn prepare_case(&self, case: &BrowserParityCase) -> anyhow::Result<()> {
        seed_fixture_auth_profile(self.auth_profile_broker.as_deref(), case)?;
        Ok(())
    }

    async fn execute_case(
        &self,
        case: &BrowserParityCase,
        request: BrowserTaskRequest,
    ) -> anyhow::Result<BrowserParityRunOutput> {
        self.agent_loop.execute_case(case, request).await
    }
}

#[async_trait]
impl BrowserParityExecutor for BrowserAgentLoop {
    async fn execute_case(
        &self,
        _case: &BrowserParityCase,
        request: BrowserTaskRequest,
    ) -> anyhow::Result<BrowserParityRunOutput> {
        let run = self.run(request).await?;
        Ok(BrowserParityRunOutput {
            active_tab_id: active_tab_id_from_run(&run),
            checkpoint_present: run
                .steps
                .iter()
                .any(|step| step.action_name == "checkpoint_pause"),
            run,
        })
    }
}

#[derive(Debug, Default)]
pub struct BrowserFixtureParityExecutor;

#[async_trait]
impl BrowserParityExecutor for BrowserFixtureParityExecutor {
    async fn execute_case(
        &self,
        case: &BrowserParityCase,
        request: BrowserTaskRequest,
    ) -> anyhow::Result<BrowserParityRunOutput> {
        Ok(BrowserParityRunOutput {
            active_tab_id: Some("tab-main".to_string()),
            checkpoint_present: case.require_checkpoint,
            run: fixture_run_for_case(case, request),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserParitySuiteReport {
    pub passed: bool,
    pub average_score: f64,
    pub run_ids: Vec<String>,
    pub scorecards: Vec<BrowserParityScorecard>,
}

impl BrowserHarnessAdapter {
    pub async fn run_builtin_suite<E: BrowserParityExecutor>(
        &self,
        runtime: &HarnessRuntime,
        executor: &E,
    ) -> anyhow::Result<BrowserParitySuiteReport> {
        let server = BrowserParityFixtureServer::spawn().await?;
        let context = server.context();
        let cases = Self::load_builtin_cases()?
            .into_iter()
            .map(|case| case.materialize(&context))
            .collect();
        self.run_suite(runtime, executor, cases).await
    }

    pub async fn run_builtin_suite_with_context<E: BrowserParityExecutor>(
        &self,
        runtime: &HarnessRuntime,
        executor: &E,
        context: &BrowserParityFixtureContext,
    ) -> anyhow::Result<BrowserParitySuiteReport> {
        let cases = Self::load_builtin_cases()?
            .into_iter()
            .map(|case| case.materialize(context))
            .collect();
        self.run_suite(runtime, executor, cases).await
    }

    pub async fn run_suite<E: BrowserParityExecutor>(
        &self,
        runtime: &HarnessRuntime,
        executor: &E,
        cases: Vec<BrowserParityCase>,
    ) -> anyhow::Result<BrowserParitySuiteReport> {
        let mut scorecards = Vec::new();
        let mut run_ids = Vec::new();
        for case in cases {
            let harness_case = case.to_harness_case();
            let episode = runtime.start_episode(&harness_case);
            run_ids.push(episode.run_id.clone());
            runtime.append_event(
                &episode.run_id,
                HarnessEvent::ToolCall {
                    ts: chrono::Utc::now().to_rfc3339(),
                    tool_name: "browser_task".to_string(),
                    input_ref: format!("case:{}", case.id),
                },
            );
            let request = case.to_task_request(format!("harness-{}", episode.run_id));
            let scorecard = match executor.prepare_case(&case).await {
                Ok(()) => match executor.execute_case(&case, request).await {
                    Ok(output) => self.score_run(BrowserParityRunInput {
                        case,
                        run: output.run,
                        active_tab_id: output.active_tab_id,
                        checkpoint_present: output.checkpoint_present,
                    }),
                    Err(error) => execution_error_scorecard(case, error),
                },
                Err(error) => execution_error_scorecard(case, error),
            };
            runtime.attach_json_artifact(
                &episode.run_id,
                "browser_parity_scorecard",
                &serde_json::to_value(&scorecard)?,
            )?;
            runtime.append_event(
                &episode.run_id,
                HarnessEvent::ToolResult {
                    ts: chrono::Utc::now().to_rfc3339(),
                    tool_name: "browser_task".to_string(),
                    output_ref: "browser_parity_scorecard".to_string(),
                    ok: scorecard.passed,
                },
            );
            runtime.finish_episode(
                &episode.run_id,
                if scorecard.passed {
                    HarnessVerdict::Pass
                } else {
                    HarnessVerdict::Fail
                },
            );
            scorecards.push(scorecard);
        }
        let average_score = if scorecards.is_empty() {
            0.0
        } else {
            scorecards
                .iter()
                .map(|scorecard| scorecard.score)
                .sum::<f64>()
                / scorecards.len() as f64
        };
        Ok(BrowserParitySuiteReport {
            passed: scorecards.iter().all(|scorecard| scorecard.passed),
            average_score,
            run_ids,
            scorecards,
        })
    }
}

fn fixture_run_for_case(case: &BrowserParityCase, request: BrowserTaskRequest) -> BrowserTaskRun {
    let mut steps = Vec::new();
    let mut step_index = 0;
    let current_url = case
        .expected_url_contains
        .clone()
        .or_else(|| case.start_url.clone())
        .unwrap_or_else(|| "about:blank".to_string());

    if case.require_auth_before_navigation {
        push_fixture_step(
            &mut steps,
            &mut step_index,
            crate::browser::session_state::BrowserTaskStepPhase::Act,
            "browser_auth_profile_apply",
            true,
            serde_json::json!({
                "profileId": "fixture-auth-profile",
                "activeTabId": "tab-main",
            }),
            "Applied deterministic fixture auth profile.",
        );
    }

    if case.start_url.is_some()
        || case
            .required_actions
            .iter()
            .any(|action| action == "browser_navigate")
    {
        push_fixture_step(
            &mut steps,
            &mut step_index,
            crate::browser::session_state::BrowserTaskStepPhase::Observe,
            "browser_navigate",
            true,
            serde_json::json!({
                "url": current_url,
                "currentUrl": case.expected_url_contains.as_deref().unwrap_or(""),
                "activeTabId": "tab-main",
            }),
            "Observed deterministic browser fixture page.",
        );
    }

    if case.min_tab_count.unwrap_or(0) >= 2 {
        push_fixture_step(
            &mut steps,
            &mut step_index,
            crate::browser::session_state::BrowserTaskStepPhase::Act,
            "browser_switch_tab",
            true,
            serde_json::json!({
                "tabId": "tab-compare",
                "targetTabId": "tab-summary",
                "activeTabId": "tab-main",
                "currentUrl": case.expected_url_contains.as_deref().unwrap_or(""),
            }),
            "Switched through deterministic comparison tabs.",
        );
    }

    if let Some(file_path) = case.required_file_path.as_deref() {
        push_fixture_step(
            &mut steps,
            &mut step_index,
            crate::browser::session_state::BrowserTaskStepPhase::Act,
            "browser_upload_file",
            true,
            serde_json::json!({
                "filePath": file_path,
                "activeTabId": "tab-main",
            }),
            "Uploaded deterministic fixture file.",
        );
    }

    if case.require_checkpoint {
        push_fixture_step(
            &mut steps,
            &mut step_index,
            crate::browser::session_state::BrowserTaskStepPhase::Act,
            "checkpoint_pause",
            true,
            serde_json::json!({
                "runId": "fixture-checkpoint",
                "activeTabId": "tab-main",
            }),
            "Saved deterministic checkpoint.",
        );
    }

    if case.require_resume {
        push_fixture_step(
            &mut steps,
            &mut step_index,
            crate::browser::session_state::BrowserTaskStepPhase::UserIntervention,
            "ask_user_response",
            true,
            serde_json::json!({
                "decision": "continue",
                "activeTabId": "tab-main",
            }),
            "Resumed deterministic checkpoint after user acknowledgement.",
        );
    }

    if case.require_failure_before_recovery {
        push_fixture_step(
            &mut steps,
            &mut step_index,
            crate::browser::session_state::BrowserTaskStepPhase::Act,
            "browser_click",
            false,
            serde_json::json!({
                "error": "stale_dom",
                "activeTabId": "tab-main",
            }),
            "Simulated transient stale DOM failure.",
        );
    }

    if case.require_recovery {
        push_fixture_step(
            &mut steps,
            &mut step_index,
            crate::browser::session_state::BrowserTaskStepPhase::Recover,
            "recover",
            true,
            serde_json::json!({
                "kind": "stale_dom_retry",
                "activeTabId": "tab-main",
            }),
            "Recovered from deterministic transient browser failure.",
        );
    }

    if let Some(boundary_kind) = case.expected_boundary_kind.as_deref() {
        push_fixture_step(
            &mut steps,
            &mut step_index,
            crate::browser::session_state::BrowserTaskStepPhase::UserIntervention,
            "needs_user_intervention",
            false,
            serde_json::json!({
                "kind": boundary_kind,
                "currentUrl": current_url,
                "activeTabId": "tab-main",
            }),
            "Detected deterministic human boundary.",
        );
    } else {
        push_fixture_step(
            &mut steps,
            &mut step_index,
            crate::browser::session_state::BrowserTaskStepPhase::Done,
            "done",
            true,
            serde_json::json!({
                "currentUrl": current_url,
                "activeTabId": "tab-main",
            }),
            "Completed deterministic browser parity fixture.",
        );
    }

    BrowserTaskRun {
        run_id: uuid::Uuid::new_v4().to_string(),
        session_id: request.session_id,
        task: request.task,
        status: case.expected_status.clone(),
        steps,
    }
}

fn push_fixture_step(
    steps: &mut Vec<BrowserTaskStep>,
    step_index: &mut u32,
    phase: crate::browser::session_state::BrowserTaskStepPhase,
    action_name: &str,
    ok: bool,
    action_args: serde_json::Value,
    reasoning: &str,
) {
    steps.push(BrowserTaskStep {
        step_index: *step_index,
        phase,
        observation_summary: action_args
            .get("currentUrl")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(reasoning)
            .to_string(),
        reasoning: reasoning.to_string(),
        action_name: action_name.to_string(),
        action_args,
        ok,
        message: Some(reasoning.to_string()),
        error: if ok {
            None
        } else {
            Some(reasoning.to_string())
        },
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    });
    *step_index += 1;
}

#[derive(Debug)]
pub struct BrowserParityFixtureServer {
    pub base_url: String,
    pub workspace_fixture_file: String,
    handle: tokio::task::JoinHandle<()>,
}

impl BrowserParityFixtureServer {
    pub async fn spawn() -> anyhow::Result<Self> {
        use axum::routing::get;
        use axum::Router;

        let workspace_fixture_file = "harness-fixtures/upload.txt".to_string();
        if let Some(home) = dirs::home_dir() {
            let upload_path = home
                .join("Documents/workground")
                .join(&workspace_fixture_file);
            if let Some(parent) = upload_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&upload_path, "uclaw browser parity upload fixture\n")?;
        }

        let app = Router::new()
            .route("/navigation", get(|| async { fixture_html("Navigation Fixture", "Ready") }))
            .route(
                "/tabs/start",
                get(|| async { fixture_html("Tab Start", "Open comparison tabs") }),
            )
            .route(
                "/tabs/summary",
                get(|| async { fixture_html("Summary Tab", "Comparison complete") }),
            )
            .route(
                "/upload",
                get(|| async {
                    r#"<html><head><title>Upload Fixture</title></head><body><h1>Upload Fixture</h1><input type="file" id="file"><p id="status">Waiting for file</p></body></html>"#
                }),
            )
            .route(
                "/protected",
                get(|| async { fixture_html("Protected Fixture", "Signed in as fixture-user") }),
            )
            .route(
                "/login",
                get(|| async {
                    r#"<html><head><title>Login Fixture</title></head><body><h1>Login</h1><label>Password <input type="password" name="password"></label></body></html>"#
                }),
            )
            .route(
                "/checkpoint",
                get(|| async { fixture_html("Checkpoint Fixture", "Step checkpoint target") }),
            )
            .route(
                "/recovery",
                get(|| async { fixture_html("Recovery Fixture", "Transient recovery target") }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let handle = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, app).await {
                tracing::warn!("browser parity fixture server exited: {error}");
            }
        });
        Ok(Self {
            base_url: format!("http://{addr}"),
            workspace_fixture_file,
            handle,
        })
    }

    pub fn context(&self) -> BrowserParityFixtureContext {
        BrowserParityFixtureContext::new(&self.base_url, &self.workspace_fixture_file)
    }
}

impl Drop for BrowserParityFixtureServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

fn fixture_html(title: &'static str, body: &'static str) -> String {
    format!("<html><head><title>{title}</title></head><body><h1>{title}</h1><p>{body}</p></body></html>")
}

fn seed_fixture_auth_profile(
    broker: Option<&BrowserAuthProfileBroker>,
    case: &BrowserParityCase,
) -> anyhow::Result<()> {
    let Some(broker) = broker else {
        return Ok(());
    };
    let (Some(origin), Some(storage_state)) = (
        case.auth_origin.as_deref(),
        case.auth_storage_state.as_ref(),
    ) else {
        return Ok(());
    };
    if broker
        .resolve_storage_state_for_origin(origin)
        .map_err(|error| anyhow::anyhow!("resolve fixture auth profile for '{origin}': {error}"))?
        .is_some()
    {
        return Ok(());
    }
    broker
        .import_playwright_storage_state(
            BrowserIdentityProfileInput {
                label: format!("Browser parity fixture: {}", case.id),
                origin_pattern: origin.to_string(),
                kind: BrowserIdentityKind::StorageState,
                provider: BrowserIdentityProvider::Playwright,
                scope: BrowserIdentityScope::Session,
            },
            &serde_json::to_string(storage_state)?,
        )
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("import fixture auth profile for '{origin}': {error}"))
}

fn replace_json_strings(value: Value, replace: &impl Fn(&str) -> String) -> Value {
    match value {
        Value::String(raw) => Value::String(replace(&raw)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| replace_json_strings(item, replace))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, replace_json_strings(value, replace)))
                .collect(),
        ),
        value => value,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserParityScorecard {
    pub case_id: String,
    pub title: String,
    pub capability: BrowserParityCapability,
    pub passed: bool,
    pub score: f64,
    pub checks: Vec<BrowserParityCheckResult>,
}

impl BrowserParityScorecard {
    pub fn to_markdown(&self) -> String {
        let mut out = format!(
            "# Browser Parity Scorecard\n\n- Case: `{}`\n- Capability: `{:?}`\n- Score: `{:.2}`\n- Result: `{}`\n\n| Check | Result | Detail |\n|---|---:|---|\n",
            self.case_id,
            self.capability,
            self.score,
            if self.passed { "pass" } else { "fail" },
        );
        for check in &self.checks {
            out.push_str(&format!(
                "| `{}` | {} | {} |\n",
                check.id,
                if check.passed { "pass" } else { "fail" },
                check.message.replace('|', "\\|")
            ));
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserParityCheckResult {
    pub id: String,
    pub passed: bool,
    pub score: f64,
    pub message: String,
}

pub fn score_browser_run(input: BrowserParityRunInput) -> BrowserParityScorecard {
    let mut checks = Vec::new();
    let BrowserParityRunInput {
        case,
        run,
        active_tab_id,
        checkpoint_present,
    } = input;

    checks.push(check(
        "status",
        run.status == case.expected_status,
        format!("expected {:?}, got {:?}", case.expected_status, run.status),
    ));

    let action_count = run
        .steps
        .iter()
        .filter(|step| counts_toward_action_budget(&step.action_name))
        .count() as u32;
    if let Some(max) = case.max_action_count {
        checks.push(check(
            "action_count",
            action_count <= max,
            format!("expected <= {max}, got {action_count}"),
        ));
    }

    for action in &case.required_actions {
        let found = run
            .steps
            .iter()
            .any(|step| step.action_name == *action && step.ok);
        checks.push(check(
            format!("action:{action}"),
            found,
            if found {
                format!("found successful action {action}")
            } else {
                format!("missing successful action {action}")
            },
        ));
    }

    if let Some(expected_tab) = case.expected_active_tab_id.as_deref() {
        let actual = active_tab_id.as_deref().unwrap_or("<none>");
        checks.push(check(
            "active_tab",
            actual == expected_tab,
            format!("expected active tab {expected_tab}, got {actual}"),
        ));
    }

    if let Some(expected_url) = case.expected_url_contains.as_deref() {
        let found = run
            .steps
            .iter()
            .any(|step| step_contains_url(step, expected_url));
        checks.push(check(
            "url_observed",
            found,
            if found {
                format!("observed URL containing {expected_url}")
            } else {
                format!("missing observed URL containing {expected_url}")
            },
        ));
    }

    if let Some(min_tabs) = case.min_tab_count {
        let tab_count = distinct_tab_count(&run);
        checks.push(check(
            "tab_count",
            tab_count >= min_tabs,
            format!("expected >= {min_tabs} distinct tab ids, got {tab_count}"),
        ));
    }

    if let Some(required_file_path) = case.required_file_path.as_deref() {
        let found = run.steps.iter().any(|step| {
            step.action_name == "browser_upload_file"
                && step.ok
                && (string_field(&step.action_args, "file_path").as_deref()
                    == Some(required_file_path)
                    || string_field(&step.action_args, "path").as_deref()
                        == Some(required_file_path))
        });
        checks.push(check(
            "file_path",
            found,
            if found {
                format!("uploaded required file {required_file_path}")
            } else {
                format!("missing upload for required file {required_file_path}")
            },
        ));
    }

    if case.require_auth_before_navigation {
        let auth_index = first_step_index(&run, "browser_auth_profile_apply", true);
        let nav_index = first_step_index(&run, "browser_navigate", true);
        let ordered = auth_index
            .zip(nav_index)
            .is_some_and(|(auth, nav)| auth < nav);
        checks.push(check(
            "auth_before_navigation",
            ordered,
            format!("auth_index={auth_index:?}, navigation_index={nav_index:?}"),
        ));
    }

    if let Some(expected_kind) = case.expected_boundary_kind.as_deref() {
        let actual = observed_boundary_kind(&run);
        checks.push(check(
            "boundary_precision",
            actual.as_deref() == Some(expected_kind),
            format!(
                "expected boundary kind {expected_kind}, got {}",
                actual.unwrap_or_else(|| "<none>".to_string())
            ),
        ));
    }

    if case.require_checkpoint {
        let paused = run.status == BrowserTaskStatus::PausedCheckpointed
            || run
                .steps
                .iter()
                .any(|step| step.action_name == "checkpoint_pause");
        checks.push(check(
            "checkpoint_saved",
            paused && checkpoint_present,
            format!("paused_or_checkpoint_step={paused}, checkpoint_present={checkpoint_present}"),
        ));
    }

    if case.require_resume {
        let resumed = run
            .steps
            .iter()
            .any(|step| step.action_name == "ask_user_response" && step.ok)
            || run
                .steps
                .iter()
                .any(|step| step.reasoning.to_lowercase().contains("resume"));
        checks.push(check(
            "resume_success",
            resumed && run.status == BrowserTaskStatus::Completed,
            format!("resumed={resumed}, status={:?}", run.status),
        ));
    }

    if case.require_recovery {
        let recovered = run
            .steps
            .iter()
            .any(|step| step.action_name == "recover" && step.ok);
        checks.push(check(
            "recovery",
            recovered,
            if recovered {
                "found successful recovery step".to_string()
            } else {
                "missing successful recovery step".to_string()
            },
        ));
    }

    if case.require_failure_before_recovery {
        let recover_index = first_step_index(&run, "recover", true);
        let failed_before_recovery = recover_index.is_some_and(|recover_index| {
            run.steps
                .iter()
                .any(|step| step.step_index < recover_index && !step.ok)
        });
        checks.push(check(
            "failure_before_recovery",
            failed_before_recovery,
            format!(
                "recover_index={recover_index:?}, failed_before_recovery={failed_before_recovery}"
            ),
        ));
    }

    let score = if checks.is_empty() {
        0.0
    } else {
        checks.iter().map(|check| check.score).sum::<f64>() / checks.len() as f64
    };
    let passed = checks.iter().all(|check| check.passed);

    BrowserParityScorecard {
        case_id: case.id,
        title: case.title,
        capability: case.capability,
        passed,
        score,
        checks,
    }
}

fn execution_error_scorecard(
    case: BrowserParityCase,
    error: anyhow::Error,
) -> BrowserParityScorecard {
    BrowserParityScorecard {
        case_id: case.id,
        title: case.title,
        capability: case.capability,
        passed: false,
        score: 0.0,
        checks: vec![BrowserParityCheckResult {
            id: "execution_error".to_string(),
            passed: false,
            score: 0.0,
            message: error.to_string(),
        }],
    }
}

fn active_tab_id_from_run(run: &BrowserTaskRun) -> Option<String> {
    run.steps.iter().rev().find_map(|step| {
        string_field(&step.action_args, "active_tab_id")
            .or_else(|| string_field(&step.action_args, "tab_id"))
    })
}

fn observed_boundary_kind(run: &BrowserTaskRun) -> Option<String> {
    run.steps
        .iter()
        .find(|step| step.action_name == "needs_user_intervention")
        .and_then(|step| string_field(&step.action_args, "kind"))
}

fn counts_toward_action_budget(action_name: &str) -> bool {
    action_name.starts_with("browser_") || action_name == "recover"
}

fn first_step_index(run: &BrowserTaskRun, action_name: &str, ok: bool) -> Option<u32> {
    run.steps
        .iter()
        .find(|step| step.action_name == action_name && step.ok == ok)
        .map(|step| step.step_index)
}

fn distinct_tab_count(run: &BrowserTaskRun) -> u32 {
    let mut tabs = std::collections::BTreeSet::new();
    for step in &run.steps {
        for field in ["tab_id", "target_tab_id", "active_tab_id"] {
            if let Some(tab_id) = string_field(&step.action_args, field) {
                tabs.insert(tab_id);
            }
        }
    }
    tabs.len() as u32
}

fn step_contains_url(step: &BrowserTaskStep, expected_url: &str) -> bool {
    matches!(
        step.phase,
        crate::browser::session_state::BrowserTaskStepPhase::Observe
            | crate::browser::session_state::BrowserTaskStepPhase::Done
            | crate::browser::session_state::BrowserTaskStepPhase::UserIntervention
    ) && (step.observation_summary.contains(expected_url)
        || string_field(&step.action_args, "current_url")
            .as_deref()
            .is_some_and(|url| url.contains(expected_url))
        || string_field(&step.action_args, "url")
            .as_deref()
            .is_some_and(|url| url.contains(expected_url)))
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            let camel = to_camel_case(field);
            value
                .get(camel.as_str())
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
}

fn to_camel_case(value: &str) -> String {
    let mut out = String::new();
    let mut upper_next = false;
    for ch in value.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn check(id: impl Into<String>, passed: bool, message: String) -> BrowserParityCheckResult {
    BrowserParityCheckResult {
        id: id.into(),
        passed,
        score: if passed { 1.0 } else { 0.0 },
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::identity::{BrowserAuthProfileBroker, MemoryBrowserSecretStore};
    use crate::browser::session_state::{BrowserTaskStep, BrowserTaskStepPhase};
    use crate::harness::episode::HarnessVerdict;

    fn step(index: u32, action_name: &str, ok: bool, args: Value) -> BrowserTaskStep {
        BrowserTaskStep {
            step_index: index,
            phase: BrowserTaskStepPhase::Act,
            observation_summary: String::new(),
            reasoning: String::new(),
            action_name: action_name.to_string(),
            action_args: args,
            ok,
            message: None,
            error: None,
            timestamp_ms: 0,
        }
    }

    fn observe_step(index: u32, url: &str) -> BrowserTaskStep {
        BrowserTaskStep {
            step_index: index,
            phase: BrowserTaskStepPhase::Observe,
            observation_summary: format!("URL: {url}\nTitle: Fixture"),
            reasoning: "Captured current browser state for planning.".into(),
            action_name: "observe".into(),
            action_args: serde_json::json!({ "currentUrl": url, "tabId": "tab-1" }),
            ok: true,
            message: Some("Observed current browser state.".into()),
            error: None,
            timestamp_ms: 0,
        }
    }

    fn run(status: BrowserTaskStatus, steps: Vec<BrowserTaskStep>) -> BrowserTaskRun {
        BrowserTaskRun {
            run_id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            task: "test".to_string(),
            status,
            steps,
        }
    }

    fn parity_case(
        id: &str,
        capability: BrowserParityCapability,
        expected_status: BrowserTaskStatus,
    ) -> BrowserParityCase {
        BrowserParityCase {
            id: id.into(),
            title: id.into(),
            capability,
            prompt: "Run the browser parity case".into(),
            start_url: Some("http://127.0.0.1:0/test".into()),
            max_steps: Some(8),
            available_file_paths: vec![],
            auth_origin: None,
            auth_storage_state: None,
            expected_status,
            required_actions: vec![],
            max_action_count: None,
            expected_active_tab_id: None,
            expected_url_contains: None,
            expected_boundary_kind: None,
            min_tab_count: None,
            required_file_path: None,
            require_auth_before_navigation: false,
            require_checkpoint: false,
            require_resume: false,
            require_recovery: false,
            require_failure_before_recovery: false,
        }
    }

    #[test]
    fn loads_all_builtin_browser_parity_cases() {
        let cases = BrowserHarnessAdapter::load_builtin_cases().unwrap();
        assert_eq!(cases.len(), 7);
        assert!(cases
            .iter()
            .any(|case| matches!(case.capability, BrowserParityCapability::FileUpload)));
        assert!(cases.iter().any(|case| case.require_checkpoint));
    }

    #[test]
    fn scores_navigation_success_with_action_budget() {
        let mut case = parity_case(
            "navigation",
            BrowserParityCapability::Navigation,
            BrowserTaskStatus::Completed,
        );
        case.required_actions = vec!["browser_navigate".into()];
        case.max_action_count = Some(2);
        case.expected_active_tab_id = Some("tab-1".into());
        case.expected_url_contains = Some("/navigation".into());
        let score = score_browser_run(BrowserParityRunInput {
            case,
            run: run(
                BrowserTaskStatus::Completed,
                vec![
                    step(
                        0,
                        "browser_navigate",
                        true,
                        serde_json::json!({ "tabId": "tab-1", "url": "http://127.0.0.1:0/navigation" }),
                    ),
                    observe_step(1, "http://127.0.0.1:0/navigation"),
                ],
            ),
            active_tab_id: Some("tab-1".into()),
            checkpoint_present: false,
        });
        assert!(score.passed, "{score:#?}");
        assert_eq!(score.score, 1.0);
    }

    #[test]
    fn catches_boundary_kind_mismatch() {
        let mut case = parity_case(
            "boundary",
            BrowserParityCapability::BoundaryDetection,
            BrowserTaskStatus::NeedsUserIntervention,
        );
        case.expected_boundary_kind = Some("password_field".into());
        let score = score_browser_run(BrowserParityRunInput {
            case,
            run: run(
                BrowserTaskStatus::NeedsUserIntervention,
                vec![step(
                    0,
                    "needs_user_intervention",
                    false,
                    serde_json::json!({ "kind": "payment" }),
                )],
            ),
            active_tab_id: None,
            checkpoint_present: false,
        });
        assert!(!score.passed);
        assert!(score
            .checks
            .iter()
            .any(|check| check.id == "boundary_precision" && !check.passed));
    }

    #[test]
    fn scorecard_renders_markdown_artifact() {
        let scorecard = BrowserParityScorecard {
            case_id: "case".into(),
            title: "Case".into(),
            capability: BrowserParityCapability::LongTaskRecovery,
            passed: true,
            score: 1.0,
            checks: vec![check("status", true, "ok".into())],
        };
        let markdown = scorecard.to_markdown();
        assert!(markdown.contains("# Browser Parity Scorecard"));
        assert!(markdown.contains("| `status` | pass | ok |"));
    }

    #[test]
    fn action_budget_counts_recovery_and_auth_profile_steps() {
        let mut case = parity_case(
            "budget",
            BrowserParityCapability::AuthProfileRestore,
            BrowserTaskStatus::Completed,
        );
        case.max_action_count = Some(1);
        let score = score_browser_run(BrowserParityRunInput {
            case,
            run: run(
                BrowserTaskStatus::Completed,
                vec![
                    step(0, "browser_auth_profile_apply", true, serde_json::json!({})),
                    step(1, "recover", true, serde_json::json!({})),
                ],
            ),
            active_tab_id: None,
            checkpoint_present: false,
        });
        assert!(!score.passed);
        assert!(score
            .checks
            .iter()
            .any(|check| check.id == "action_count" && check.message.contains("got 2")));
    }

    #[test]
    fn scores_auth_profile_order_file_upload_tabs_and_recovery() {
        let mut case = parity_case(
            "strict",
            BrowserParityCapability::LongTaskRecovery,
            BrowserTaskStatus::Completed,
        );
        case.min_tab_count = Some(2);
        case.required_file_path = Some("/tmp/upload.txt".into());
        case.require_auth_before_navigation = true;
        case.require_recovery = true;
        case.require_failure_before_recovery = true;

        let score = score_browser_run(BrowserParityRunInput {
            case,
            run: run(
                BrowserTaskStatus::Completed,
                vec![
                    step(0, "browser_auth_profile_apply", true, serde_json::json!({})),
                    step(
                        1,
                        "browser_navigate",
                        true,
                        serde_json::json!({ "tabId": "tab-a" }),
                    ),
                    step(
                        2,
                        "browser_switch_tab",
                        true,
                        serde_json::json!({ "tabId": "tab-b" }),
                    ),
                    step(
                        3,
                        "browser_upload_file",
                        true,
                        serde_json::json!({ "filePath": "/tmp/upload.txt" }),
                    ),
                    step(4, "browser_click", false, serde_json::json!({})),
                    step(5, "recover", true, serde_json::json!({})),
                ],
            ),
            active_tab_id: Some("tab-b".into()),
            checkpoint_present: false,
        });

        assert!(score.passed, "{score:#?}");
    }

    #[test]
    fn url_check_requires_observed_page_state_not_only_navigation_attempt() {
        let mut case = parity_case(
            "navigation",
            BrowserParityCapability::Navigation,
            BrowserTaskStatus::Completed,
        );
        case.required_actions = vec!["browser_navigate".into()];
        case.expected_url_contains = Some("/navigation".into());

        let score = score_browser_run(BrowserParityRunInput {
            case,
            run: run(
                BrowserTaskStatus::Completed,
                vec![step(
                    0,
                    "browser_navigate",
                    true,
                    serde_json::json!({ "url": "http://127.0.0.1:0/navigation" }),
                )],
            ),
            active_tab_id: None,
            checkpoint_present: false,
        });

        assert!(!score.passed);
        assert!(score
            .checks
            .iter()
            .any(|check| check.id == "url_observed" && !check.passed));
    }

    #[test]
    fn url_check_ignores_decide_phase_navigation_plan() {
        let mut case = parity_case(
            "navigation",
            BrowserParityCapability::Navigation,
            BrowserTaskStatus::Completed,
        );
        case.expected_url_contains = Some("/navigation".into());
        let mut decide = step(
            0,
            "decide",
            true,
            serde_json::json!({ "kind": "navigate", "url": "http://127.0.0.1:0/navigation" }),
        );
        decide.phase = BrowserTaskStepPhase::Decide;

        let score = score_browser_run(BrowserParityRunInput {
            case,
            run: run(BrowserTaskStatus::Completed, vec![decide]),
            active_tab_id: None,
            checkpoint_present: false,
        });

        assert!(!score.passed);
        assert!(score
            .checks
            .iter()
            .any(|check| check.id == "url_observed" && !check.passed));
    }

    #[test]
    fn builtin_cases_materialize_runtime_fixture_context() {
        let cases = BrowserHarnessAdapter::load_builtin_cases().unwrap();
        let context = BrowserParityFixtureContext::new(
            "http://127.0.0.1:4173",
            "harness-fixtures/upload.txt",
        );
        let materialized: Vec<_> = cases
            .iter()
            .map(|case| case.materialize(&context))
            .collect();

        assert!(materialized.iter().all(|case| !case
            .start_url
            .as_deref()
            .unwrap_or("")
            .contains(":0")));
        let upload = materialized
            .iter()
            .find(|case| matches!(case.capability, BrowserParityCapability::FileUpload))
            .unwrap();
        assert_eq!(
            upload.required_file_path.as_deref(),
            Some("harness-fixtures/upload.txt")
        );
        assert_eq!(
            upload.available_file_paths,
            vec!["harness-fixtures/upload.txt".to_string()]
        );
        let auth = materialized
            .iter()
            .find(|case| matches!(case.capability, BrowserParityCapability::AuthProfileRestore))
            .unwrap();
        assert!(auth.auth_storage_state.is_some());
        assert!(auth
            .auth_storage_state
            .as_ref()
            .unwrap()
            .to_string()
            .contains("http://127.0.0.1:4173"));
    }

    #[test]
    fn seeds_deterministic_fixture_auth_profile() {
        let temp = tempfile::tempdir().unwrap();
        let broker = BrowserAuthProfileBroker::new_with_secret_store(
            temp.path().join("profiles.json"),
            Arc::new(MemoryBrowserSecretStore::default()),
        );
        let mut case = parity_case(
            "browser.auth_profile.restore",
            BrowserParityCapability::AuthProfileRestore,
            BrowserTaskStatus::Completed,
        );
        case.auth_origin = Some("http://127.0.0.1:4173".into());
        case.auth_storage_state = Some(serde_json::json!({
            "cookies": [{
                "name": "sid",
                "value": "fixture",
                "domain": "127.0.0.1",
                "path": "/"
            }],
            "origins": [{
                "origin": "http://127.0.0.1:4173",
                "localStorage": []
            }]
        }));

        seed_fixture_auth_profile(Some(&broker), &case).unwrap();

        let resolved = broker
            .resolve_storage_state_for_origin("http://127.0.0.1:4173")
            .unwrap();
        assert!(resolved.is_some());
    }

    struct FakeBrowserExecutor;

    #[async_trait]
    impl BrowserParityExecutor for FakeBrowserExecutor {
        async fn execute_case(
            &self,
            _case: &BrowserParityCase,
            request: BrowserTaskRequest,
        ) -> anyhow::Result<BrowserParityRunOutput> {
            assert!(request.session_id.starts_with("harness-run-"));
            Ok(BrowserParityRunOutput {
                run: run(
                    BrowserTaskStatus::Completed,
                    vec![
                        step(
                            0,
                            "browser_navigate",
                            true,
                            serde_json::json!({ "url": "http://127.0.0.1:0/navigation", "tabId": "tab-1" }),
                        ),
                        observe_step(1, "http://127.0.0.1:0/navigation"),
                    ],
                ),
                active_tab_id: Some("tab-1".into()),
                checkpoint_present: false,
            })
        }
    }

    #[tokio::test]
    async fn run_suite_records_harness_episode_and_scorecard_artifact() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = HarnessRuntime::new(tmp.path());
        let adapter = BrowserHarnessAdapter;
        let mut case = parity_case(
            "browser.navigation.basic",
            BrowserParityCapability::Navigation,
            BrowserTaskStatus::Completed,
        );
        case.required_actions = vec!["browser_navigate".into()];
        case.expected_active_tab_id = Some("tab-1".into());
        case.expected_url_contains = Some("/navigation".into());

        let report = adapter
            .run_suite(&runtime, &FakeBrowserExecutor, vec![case])
            .await
            .unwrap();

        assert!(report.passed, "{report:#?}");
        assert_eq!(report.scorecards.len(), 1);
        assert_eq!(report.run_ids.len(), 1);
        let stored = runtime.get_episode(&report.run_ids[0]).unwrap();
        assert_eq!(stored.artifacts.len(), 1);
        assert_eq!(stored.artifacts[0].kind, "browser_parity_scorecard");
        assert_eq!(
            stored
                .trace
                .iter()
                .filter(|event| event.kind() == "tool_result")
                .count(),
            1
        );
        let artifact_content = std::fs::read_to_string(&stored.artifacts[0].path).unwrap();
        assert!(artifact_content.contains("browser.navigation.basic"));
    }

    struct FailingBrowserExecutor;

    #[async_trait]
    impl BrowserParityExecutor for FailingBrowserExecutor {
        async fn execute_case(
            &self,
            _case: &BrowserParityCase,
            _request: BrowserTaskRequest,
        ) -> anyhow::Result<BrowserParityRunOutput> {
            Err(anyhow::anyhow!("fixture browser crashed"))
        }
    }

    #[tokio::test]
    async fn run_suite_records_failed_episode_when_executor_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = HarnessRuntime::new(tmp.path());
        let adapter = BrowserHarnessAdapter;
        let case = parity_case(
            "browser.failure",
            BrowserParityCapability::Navigation,
            BrowserTaskStatus::Completed,
        );

        let report = adapter
            .run_suite(&runtime, &FailingBrowserExecutor, vec![case])
            .await
            .unwrap();

        assert!(!report.passed);
        assert_eq!(report.scorecards[0].checks[0].id, "execution_error");
        let stored = runtime.get_episode(&report.run_ids[0]).unwrap();
        assert_eq!(stored.verdict, HarnessVerdict::Fail);
        assert_eq!(stored.artifacts[0].kind, "browser_parity_scorecard");
    }
}
