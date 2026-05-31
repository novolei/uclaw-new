use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::playwright_mcp::{
    playwright_mcp_provider_result_from_adapter_call, playwright_mcp_tool_allowlist,
    PlaywrightMcpAction, PlaywrightMcpActionKind, PlaywrightMcpProviderArtifactRef,
    PlaywrightMcpProviderExecutionError, PlaywrightMcpProviderExecutionResult,
    PlaywrightMcpProviderExecutionStatus, PLAYWRIGHT_MCP_PROVIDER_ID,
};
use super::provider::BrowserProviderRouteDecision;
use super::runtime_contracts::BrowserTaskEventName;
use crate::mcp::{CallToolResult, ContentBlock, SharedMcpManager};

pub const PLAYWRIGHT_MCP_SERVER_ID: &str = "playwright";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaywrightMcpAdapterError {
    RawToolNotAllowed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightMcpAdapterToolCall {
    pub server_id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub action_kind: PlaywrightMcpActionKind,
    pub read_only: bool,
}

impl PlaywrightMcpAdapterToolCall {
    pub fn new(
        tool_name: &str,
        arguments: Value,
        action_kind: PlaywrightMcpActionKind,
        read_only: bool,
    ) -> Result<Self, PlaywrightMcpAdapterError> {
        validate_playwright_mcp_tool(tool_name)?;
        Ok(Self {
            server_id: PLAYWRIGHT_MCP_SERVER_ID.to_string(),
            tool_name: tool_name.to_string(),
            arguments,
            action_kind,
            read_only,
        })
    }

    pub fn navigate(url: &str) -> Self {
        Self::new(
            "browser_navigate",
            json!({ "url": url }),
            PlaywrightMcpActionKind::Navigate,
            false,
        )
        .expect("browser_navigate is allowlisted")
    }

    pub fn from_action(action: &PlaywrightMcpAction) -> Result<Self, PlaywrightMcpAdapterError> {
        match action {
            PlaywrightMcpAction::AccessibilitySnapshot { .. }
            | PlaywrightMcpAction::DiscoverLocators { .. } => {
                Self::new("browser_snapshot", json!({}), action.kind(), true)
            }
            PlaywrightMcpAction::Trace { .. } => {
                Self::new("browser_start_tracing", json!({}), action.kind(), true)
            }
            PlaywrightMcpAction::Navigate { url } => Self::new(
                "browser_navigate",
                json!({ "url": url }),
                action.kind(),
                false,
            ),
            PlaywrightMcpAction::Click { locator } => Self::new(
                "browser_click",
                json!({ "element": locator, "ref": locator }),
                action.kind(),
                false,
            ),
            PlaywrightMcpAction::Type { locator, text } => Self::new(
                "browser_type",
                json!({ "element": locator, "ref": locator, "text": text }),
                action.kind(),
                false,
            ),
        }
    }
}

pub fn validate_playwright_mcp_tool(tool_name: &str) -> Result<(), PlaywrightMcpAdapterError> {
    if playwright_mcp_tool_allowlist()
        .iter()
        .any(|allowed| allowed == tool_name)
    {
        Ok(())
    } else {
        Err(PlaywrightMcpAdapterError::RawToolNotAllowed)
    }
}

pub struct PlaywrightMcpProviderAdapter {
    mcp_manager: Option<SharedMcpManager>,
}

impl PlaywrightMcpProviderAdapter {
    pub fn new(mcp_manager: Option<SharedMcpManager>) -> Self {
        Self { mcp_manager }
    }

    pub async fn execute_action(
        &self,
        request_id: String,
        session_id: &str,
        action: PlaywrightMcpAction,
        route_decision: &BrowserProviderRouteDecision,
    ) -> PlaywrightMcpProviderExecutionResult {
        let call = match PlaywrightMcpAdapterToolCall::from_action(&action) {
            Ok(call) => call,
            Err(_) => {
                return playwright_mcp_provider_failure_from_adapter_call(
                    request_id,
                    &PlaywrightMcpAdapterToolCall::navigate("about:blank"),
                    "raw_tool_not_allowed",
                    "Selected Playwright MCP route is not allowlisted by the Browser Runtime adapter.",
                    false,
                );
            }
        };

        if let Some(mcp_manager) = self.mcp_manager.as_ref() {
            self.execute_adapter_call(mcp_manager, request_id, session_id, &call, route_decision)
                .await
        } else {
            playwright_mcp_provider_result_from_adapter_call(
                request_id,
                &call,
                playwright_mcp_adapter_evidence_output(session_id, &call, route_decision, None),
            )
        }
    }

    async fn execute_adapter_call(
        &self,
        mcp_manager: &SharedMcpManager,
        request_id: String,
        session_id: &str,
        call: &PlaywrightMcpAdapterToolCall,
        route_decision: &BrowserProviderRouteDecision,
    ) -> PlaywrightMcpProviderExecutionResult {
        let call_result = {
            let manager = mcp_manager.read().await;
            manager
                .call_tool(&call.server_id, &call.tool_name, call.arguments.clone())
                .await
        };

        match call_result {
            Ok(result) if !result.is_error => playwright_mcp_provider_result_from_adapter_call(
                request_id,
                call,
                playwright_mcp_adapter_evidence_output(session_id, call, route_decision, Some(result)),
            ),
            Ok(result) => playwright_mcp_provider_failure_from_adapter_call(
                request_id,
                call,
                "mcp_tool_error",
                call_tool_result_text(&result),
                true,
            ),
            Err(error) => playwright_mcp_provider_failure_from_adapter_call(
                request_id,
                call,
                "mcp_transport_error",
                error.to_string(),
                true,
            ),
        }
    }
}

fn playwright_mcp_adapter_evidence_output(
    session_id: &str,
    call: &PlaywrightMcpAdapterToolCall,
    route_decision: &BrowserProviderRouteDecision,
    call_result: Option<CallToolResult>,
) -> serde_json::Value {
    let content = call_result
        .as_ref()
        .map(|result| {
            result
                .content
                .iter()
                .map(content_block_to_json)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let content_text = call_result.as_ref().map(call_tool_result_text).unwrap_or_default();
    let mut output = json!({
        "providerId": PLAYWRIGHT_MCP_PROVIDER_ID,
        "serverId": call.server_id,
        "toolName": call.tool_name,
        "arguments": call.arguments,
        "content": content,
        "contentText": content_text,
        "routeEvidence": {
            "source": "browser_runtime_adapter",
            "rawToolsExposed": false,
            "routeStatus": route_decision.status,
            "eventIntents": route_decision.event_intents,
            "skippedProviders": route_decision.skipped_providers,
        },
    });
    if call.action_kind == PlaywrightMcpActionKind::Navigate {
        output["tabId"] = json!(format!("playwright-mcp:{session_id}"));
    }
    output
}

fn content_block_to_json(block: &ContentBlock) -> serde_json::Value {
    match block {
        ContentBlock::Text { text } => json!({ "type": "text", "text": text }),
        ContentBlock::Image { data, mime_type } => {
            json!({ "type": "image", "data": data, "mimeType": mime_type })
        }
        ContentBlock::Resource { resource } => json!({ "type": "resource", "resource": resource }),
    }
}

fn call_tool_result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn playwright_mcp_provider_failure_from_adapter_call(
    request_id: String,
    call: &PlaywrightMcpAdapterToolCall,
    code: impl Into<String>,
    message: impl Into<String>,
    retryable: bool,
) -> PlaywrightMcpProviderExecutionResult {
    let code = code.into();
    let message = message.into();
    PlaywrightMcpProviderExecutionResult {
        provider_id: PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
        request_id,
        action_kind: call.action_kind,
        status: PlaywrightMcpProviderExecutionStatus::Failed,
        summary: format!("Playwright MCP {} failed: {message}", call.tool_name),
        mcp_tool_name: Some(call.tool_name.clone()),
        read_only: call.read_only,
        raw_tools_exposed: false,
        artifact_refs: Vec::<PlaywrightMcpProviderArtifactRef>::new(),
        event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
        output: Some(json!({
            "providerId": PLAYWRIGHT_MCP_PROVIDER_ID,
            "serverId": call.server_id,
            "toolName": call.tool_name,
            "arguments": call.arguments,
        })),
        error: Some(PlaywrightMcpProviderExecutionError {
            code,
            message,
            retryable,
            event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
            artifact_recommended: true,
        }),
    }
}

#[cfg(test)]
#[path = "playwright_mcp_adapter_tests.rs"]
mod playwright_mcp_adapter_tests;
