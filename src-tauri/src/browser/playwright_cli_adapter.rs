use std::time::Instant;

use serde_json::json;

use crate::browser::action::{BrowserAction, BrowserActionResult};
use crate::browser::action_registry::BrowserActionRegistry;
use crate::browser::playwright_cli::{
    execute_official_playwright_cli_provider_action, execute_playwright_cli_provider_action,
    PlaywrightCliAction, PlaywrightCliAddress, PlaywrightCliOfficialCommandConfig,
    PlaywrightCliProviderExecutionResult, PlaywrightCliProviderExecutionStatus,
};
use crate::browser::provider_execution::BrowserProviderActionBlocked;
use crate::browser::runtime_contracts::BrowserRuntimeFeatureFlags;
use crate::browser::runtime_pack::BrowserRuntimePackStatusReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaywrightCliAdapterError {
    UnsupportedSkillCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaywrightCliCommand {
    pub command: String,
    pub args: Vec<String>,
}

impl PlaywrightCliCommand {
    pub fn install_skills() -> Self {
        Self {
            command: "playwright-cli".to_string(),
            args: vec!["install".to_string(), "--skills".to_string()],
        }
    }
}

pub struct PlaywrightCliActionCommand;

impl PlaywrightCliActionCommand {
    pub fn from_skill_command(
        command: &str,
    ) -> Result<PlaywrightCliCommand, PlaywrightCliAdapterError> {
        match command.trim() {
            "playwright-cli install --skills" => Ok(PlaywrightCliCommand::install_skills()),
            _ => Err(PlaywrightCliAdapterError::UnsupportedSkillCommand),
        }
    }
}

pub struct PlaywrightCliProviderAdapterConfig {
    pub feature_flags: BrowserRuntimeFeatureFlags,
    pub runtime_report: Option<BrowserRuntimePackStatusReport>,
    pub command: Option<std::path::PathBuf>,
    pub cwd: Option<std::path::PathBuf>,
}

pub struct PlaywrightCliProviderAdapter<'a> {
    action_registry: &'a BrowserActionRegistry,
    config: PlaywrightCliProviderAdapterConfig,
}

impl<'a> PlaywrightCliProviderAdapter<'a> {
    pub fn new(
        action_registry: &'a BrowserActionRegistry,
        config: PlaywrightCliProviderAdapterConfig,
    ) -> Self {
        Self {
            action_registry,
            config,
        }
    }

    pub async fn execute_action(
        &self,
        request_id: String,
        session_id: &str,
        identity_profile_id: Option<&str>,
        action: BrowserAction,
    ) -> Result<BrowserActionResult, BrowserProviderActionBlocked> {
        let input_action = action.clone();
        let cli_action = playwright_cli_action_for_browser_action(action)?;
        let started = Instant::now();
        let session_state_path = uclaw_utils_home::uclaw_home_pathbuf()
            .map(|home| {
                home.join("browser-profiles")
                    .join(session_id)
                    .join("playwright_state.json")
            })
            .ok();

        let report = self.config.runtime_report.clone();

        let use_worker = report
            .as_ref()
            .map(|report| report.ready && report.can_run_browser_tasks)
            .unwrap_or(false);

        let (provider_result, is_child_worker) = if use_worker {
            let result = execute_playwright_cli_provider_action(
                session_id,
                request_id,
                self.config.feature_flags,
                cli_action,
                report
                    .as_ref()
                    .expect("use_worker implies a runtime report is available"),
                session_state_path,
            )
            .await;
            (result, true)
        } else {
            let mut command_config =
                PlaywrightCliOfficialCommandConfig::for_uclaw_session(session_id);
            if let Some(command) = self.config.command.clone() {
                command_config = command_config.with_command(command);
            }
            if let Some(cwd) = self.config.cwd.clone() {
                command_config = command_config.with_cwd(cwd);
            }
            let result = execute_official_playwright_cli_provider_action(
                request_id,
                self.config.feature_flags,
                cli_action,
                command_config,
            )
            .await;
            (result, false)
        };

        let duration_ms = started.elapsed().as_millis() as u64;
        let mut action_result = browser_action_result_from_playwright_cli(
            provider_result,
            duration_ms,
            &input_action,
            is_child_worker,
        );
        self.mirror_preview(
            session_id,
            identity_profile_id,
            &input_action,
            &mut action_result,
        )
        .await;
        Ok(action_result)
    }

    async fn mirror_preview(
        &self,
        session_id: &str,
        identity_profile_id: Option<&str>,
        input_action: &BrowserAction,
        provider_result: &mut BrowserActionResult,
    ) {
        if !provider_result.ok || !self.action_registry.supports_live_preview_events() {
            return;
        }
        let Some(mirror_action) = playwright_cli_preview_mirror_action(input_action) else {
            return;
        };
        let provider_tab_id = provider_result.tab_id.clone();
        match self
            .action_registry
            .execute_with_identity(session_id, identity_profile_id, mirror_action)
            .await
        {
            Ok(preview_result) => {
                let preview_tab_id = preview_result
                    .tab_id
                    .clone()
                    .or_else(|| tab_id_from_browser_action(input_action));
                if let Some(preview_tab_id) = preview_tab_id {
                    apply_preview_mirror_success(provider_result, provider_tab_id, preview_tab_id);
                }
            }
            Err(error) => {
                tracing::warn!(
                    session_id,
                    provider_tab_id = provider_tab_id.as_deref().unwrap_or(""),
                    error = %error,
                    "Playwright CLI preview mirror failed; provider action remains successful"
                );
                apply_preview_mirror_failure(provider_result, provider_tab_id, error.to_string());
            }
        }
    }
}

pub(crate) fn playwright_cli_action_for_browser_action(
    action: BrowserAction,
) -> Result<PlaywrightCliAction, BrowserProviderActionBlocked> {
    match action {
        BrowserAction::Navigate { url, .. } => Ok(PlaywrightCliAction::Navigate { url }),
        BrowserAction::Click { index, .. } => Ok(PlaywrightCliAction::Click {
            target: PlaywrightCliAddress::UclawDomElementId {
                element_id: index.to_string(),
            },
        }),
        BrowserAction::Type { index, text, .. } => Ok(PlaywrightCliAction::Type {
            target: PlaywrightCliAddress::UclawDomElementId {
                element_id: index.to_string(),
            },
            text,
        }),
        BrowserAction::GetState {
            include_screenshot, ..
        } => {
            if include_screenshot {
                Ok(PlaywrightCliAction::Screenshot {
                    full_page: false,
                    filename: None,
                })
            } else {
                Ok(PlaywrightCliAction::Extract { target: None })
            }
        }
        BrowserAction::Screenshot {
            full_page,
            save_path,
            ..
        } => Ok(PlaywrightCliAction::Screenshot {
            full_page,
            filename: save_path,
        }),
        BrowserAction::Scroll { .. }
        | BrowserAction::SendKeys { .. }
        | BrowserAction::Evaluate { .. }
        | BrowserAction::ListTabs
        | BrowserAction::SwitchTab { .. }
        | BrowserAction::CloseTab { .. }
        | BrowserAction::UploadFile { .. } => Err(BrowserProviderActionBlocked {
            selected_provider_id: Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID.to_string()),
            message: "Selected Playwright CLI route does not support this browser action yet."
                .to_string(),
        }),
    }
}

fn browser_action_result_from_playwright_cli(
    result: PlaywrightCliProviderExecutionResult,
    duration_ms: u64,
    input_action: &BrowserAction,
    is_child_worker: bool,
) -> BrowserActionResult {
    let ok = result.status == PlaywrightCliProviderExecutionStatus::Succeeded;
    let error = result.error.as_ref().map(|error| {
        if error.code.is_empty() {
            error.message.clone()
        } else {
            format!("{}: {}", error.code, error.message)
        }
    });
    let tab_id = result
        .output
        .as_ref()
        .and_then(|output| output.get("tabId"))
        .and_then(|value| value.as_str())
        .map(str::to_string);

    let url = result
        .output
        .as_ref()
        .and_then(|output| output.get("url"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let title = result
        .output
        .as_ref()
        .and_then(|output| output.get("title"))
        .and_then(|value| value.as_str())
        .map(str::to_string);

    let route_evidence = if is_child_worker {
        json!({
            "source": "playwright_cli_worker",
            "officialCli": false,
            "runtimePackRequired": true,
            "url": url,
            "title": title,
        })
    } else {
        json!({
            "source": "official_playwright_cli",
            "officialCli": true,
            "runtimePackRequired": false,
            "url": url,
            "title": title,
        })
    };

    BrowserActionResult {
        ok,
        action_name: format!("browser_playwright_cli_{}", result.action_kind.as_str()),
        message: Some(result.summary.clone()),
        tab_id,
        observation_json: Some(json!({
            "providerId": result.provider_id,
            "requestId": result.request_id,
            "status": result.status,
            "actionKind": result.action_kind,
            "summary": result.summary,
            "inputAction": input_action,
            "artifactRefs": result.artifact_refs,
            "output": result.output,
            "error": result.error,
            "routeEvidence": route_evidence,
        })),
        error,
        duration_ms,
    }
}

fn playwright_cli_preview_mirror_action(action: &BrowserAction) -> Option<BrowserAction> {
    match action {
        BrowserAction::Navigate { .. }
        | BrowserAction::Click { .. }
        | BrowserAction::Type { .. } => Some(action.clone()),
        BrowserAction::Screenshot { .. } => None,
        BrowserAction::GetState { .. }
        | BrowserAction::Scroll { .. }
        | BrowserAction::SendKeys { .. }
        | BrowserAction::Evaluate { .. }
        | BrowserAction::ListTabs
        | BrowserAction::SwitchTab { .. }
        | BrowserAction::CloseTab { .. }
        | BrowserAction::UploadFile { .. } => None,
    }
}

fn tab_id_from_browser_action(action: &BrowserAction) -> Option<String> {
    match action {
        BrowserAction::Navigate { tab_id, .. } => tab_id.clone(),
        BrowserAction::Click { tab_id, .. }
        | BrowserAction::Type { tab_id, .. }
        | BrowserAction::Scroll { tab_id, .. }
        | BrowserAction::SendKeys { tab_id, .. }
        | BrowserAction::Evaluate { tab_id, .. }
        | BrowserAction::GetState { tab_id, .. }
        | BrowserAction::Screenshot { tab_id, .. }
        | BrowserAction::SwitchTab { tab_id }
        | BrowserAction::CloseTab { tab_id }
        | BrowserAction::UploadFile { tab_id, .. } => Some(tab_id.clone()),
        BrowserAction::ListTabs => None,
    }
}

pub(crate) fn apply_preview_mirror_success(
    result: &mut BrowserActionResult,
    provider_tab_id: Option<String>,
    preview_tab_id: String,
) {
    result.tab_id = Some(preview_tab_id.clone());
    annotate_preview_mirror(
        result,
        provider_tab_id,
        json!({
            "status": "mirrored",
            "source": "local_chromium_mirror",
            "previewTabId": preview_tab_id,
        }),
    );
}

fn apply_preview_mirror_failure(
    result: &mut BrowserActionResult,
    provider_tab_id: Option<String>,
    error: String,
) {
    annotate_preview_mirror(
        result,
        provider_tab_id,
        json!({
            "status": "failed",
            "source": "local_chromium_mirror",
            "error": error,
        }),
    );
}

fn annotate_preview_mirror(
    result: &mut BrowserActionResult,
    provider_tab_id: Option<String>,
    preview: serde_json::Value,
) {
    let Some(observation) = result.observation_json.as_mut() else {
        return;
    };
    observation["providerTabId"] = json!(provider_tab_id.clone());
    observation["previewMirror"] = preview.clone();
    if let Some(output) = observation
        .get_mut("output")
        .and_then(|value| value.as_object_mut())
    {
        output.insert("providerTabId".to_string(), json!(provider_tab_id.clone()));
        output.insert("previewMirror".to_string(), preview.clone());
        if let Some(preview_tab_id) = preview
            .get("previewTabId")
            .and_then(|value| value.as_str())
            .map(str::to_string)
        {
            output.insert("previewTabId".to_string(), json!(preview_tab_id));
        }
    }
}

#[cfg(test)]
#[path = "playwright_cli_adapter_tests.rs"]
mod playwright_cli_adapter_tests;
