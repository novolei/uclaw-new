use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::agent::types::{ChatMessage, RespondOutput};
use crate::browser::action::BrowserAction;
use crate::browser::session_state::BrowserTaskStep;
use crate::browser::task_store::BrowserTaskMemory;
use crate::llm::{CompletionConfig, LlmProvider};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserDecisionStatus {
    Continue,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDecision {
    pub status: BrowserDecisionStatus,
    pub reasoning: String,
    pub action: Option<BrowserAction>,
    pub final_answer: Option<String>,
}

pub fn build_browser_decision_prompt(
    task: &str,
    observation_json: &serde_json::Value,
    memory: Option<&BrowserTaskMemory>,
    previous_steps: &[BrowserTaskStep],
) -> String {
    let steps_json = serde_json::to_string_pretty(previous_steps)
        .unwrap_or_else(|_| "[]".to_string());
    let observation_json = serde_json::to_string_pretty(observation_json)
        .unwrap_or_else(|_| "{}".to_string());
    let memory_json = memory
        .and_then(|m| serde_json::to_string_pretty(m).ok())
        .unwrap_or_else(|| "{}".to_string());
    format!(
        "You are the browser decision adapter for an AI browser agent.\n\
         Return exactly one JSON object matching this schema and no markdown:\n\
         {{\n\
           \"status\": \"continue\" | \"done\" | \"failed\",\n\
           \"reasoning\": string,\n\
           \"action\": null | BrowserAction,\n\
           \"finalAnswer\": null | string\n\
         }}\n\n\
         BrowserAction is one of:\n\
         {{\"kind\":\"navigate\",\"url\":string,\"tab_id\"?:string}}\n\
         {{\"kind\":\"click\",\"tab_id\":string,\"index\":number}}\n\
         {{\"kind\":\"type\",\"tab_id\":string,\"index\":number,\"text\":string}}\n\
         {{\"kind\":\"scroll\",\"tab_id\":string,\"direction\":\"up\"|\"down\"|\"left\"|\"right\",\"pixels\"?:number,\"index\"?:number}}\n\
         {{\"kind\":\"send_keys\",\"tab_id\":string,\"keys\":string}}\n\
         {{\"kind\":\"evaluate\",\"tab_id\":string,\"script\":string}}\n\
         {{\"kind\":\"get_state\",\"tab_id\":string,\"include_screenshot\":boolean}}\n\n\
         Rules:\n\
         - Use status=continue only when action is non-null.\n\
         - Use status=done when the task is complete and finalAnswer explains the result.\n\
         - Use status=failed when the task cannot proceed and finalAnswer explains why.\n\
         - Prefer DOM element indexes from the latest observation; do not invent indexes.\n\
         - Keep reasoning concise.\n\n\
         Task:\n{task}\n\n\
         Browser task memory JSON:\n{memory_json}\n\n\
         Latest observation JSON:\n{observation_json}\n\n\
         Previous browser steps:\n{steps_json}\n"
    )
}

pub fn parse_browser_decision(raw: &str) -> Result<BrowserDecision> {
    let trimmed = raw.trim();
    let candidate = strip_json_code_fence(trimmed).unwrap_or(trimmed);
    let candidate = extract_json_object(candidate).unwrap_or(candidate);
    serde_json::from_str(candidate)
        .map_err(|e| anyhow!("browser decision JSON parse error: {e}; raw={trimmed}"))
}

fn strip_json_code_fence(raw: &str) -> Option<&str> {
    let body = raw.strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```JSON"))
        .or_else(|| raw.strip_prefix("```"))?;
    Some(body.strip_suffix("```").unwrap_or(body).trim())
}

fn extract_json_object(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(raw[start..=end].trim())
}

#[async_trait]
pub trait BrowserDecisionAdapter: Send + Sync {
    async fn decide(
        &self,
        task: &str,
        observation_json: &serde_json::Value,
        memory: Option<&BrowserTaskMemory>,
        previous_steps: &[BrowserTaskStep],
    ) -> Result<BrowserDecision>;
}

pub struct LlmBrowserDecisionAdapter {
    llm: Arc<dyn LlmProvider>,
    model: String,
}

impl LlmBrowserDecisionAdapter {
    pub fn new(llm: Arc<dyn LlmProvider>, model: String) -> Self {
        Self { llm, model }
    }
}

#[async_trait]
impl BrowserDecisionAdapter for LlmBrowserDecisionAdapter {
    async fn decide(
        &self,
        task: &str,
        observation_json: &serde_json::Value,
        memory: Option<&BrowserTaskMemory>,
        previous_steps: &[BrowserTaskStep],
    ) -> Result<BrowserDecision> {
        let prompt = build_browser_decision_prompt(task, observation_json, memory, previous_steps);
        let config = CompletionConfig {
            model: self.model.clone(),
            max_tokens: 1024,
            temperature: 0.0,
            thinking_enabled: false,
        };
        match self
            .llm
            .complete(vec![ChatMessage::user(&prompt)], vec![], &config)
            .await
        {
            Ok(RespondOutput::Text { text, .. }) => parse_browser_decision(&text),
            Ok(RespondOutput::ToolCalls { text: Some(text), .. }) => parse_browser_decision(&text),
            Ok(other) => Err(anyhow!("browser decision LLM returned non-text output: {other:?}")),
            Err(e) => Err(anyhow!("browser decision LLM call failed: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_click_decision() {
        let raw = r#"{"status":"continue","reasoning":"Click search","action":{"kind":"click","tab_id":"t1","index":2},"finalAnswer":null}"#;
        let decision: BrowserDecision = serde_json::from_str(raw).unwrap();
        assert_eq!(decision.status, BrowserDecisionStatus::Continue);
    }

    #[test]
    fn parses_decision_from_json_fence() {
        let raw = "```json\n{\"status\":\"done\",\"reasoning\":\"Finished\",\"action\":null,\"finalAnswer\":\"ok\"}\n```";
        let decision = parse_browser_decision(raw).unwrap();
        assert_eq!(decision.status, BrowserDecisionStatus::Done);
        assert_eq!(decision.final_answer.as_deref(), Some("ok"));
    }

    #[test]
    fn prompt_demands_strict_json() {
        let prompt = build_browser_decision_prompt(
            "Search for rust crates",
            &serde_json::json!({"url": "https://example.test", "elements": []}),
            None,
            &[],
        );
        assert!(prompt.contains("Return exactly one JSON object"));
        assert!(prompt.contains("\"status\": \"continue\" | \"done\" | \"failed\""));
        assert!(prompt.contains("Search for rust crates"));
    }
}
