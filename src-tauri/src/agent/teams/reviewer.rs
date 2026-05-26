use super::channel::{AgentTeamChannel, ChannelRole};
use crate::agent::types::{ChatMessage, RespondOutput};
use crate::llm::{CompletionConfig, LlmProvider};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewVerdict {
    Pass,
    Revise(String),
    Fail(String),
}

pub struct ReviewRequest {
    pub original_task: String,
    pub supervisor_plan: String,
    pub worker_results: Vec<(String, String)>, // (role, result)
}

pub async fn run_reviewer(
    request: ReviewRequest,
    llm: Arc<dyn LlmProvider>,
    model: &str,
    channel: Arc<AgentTeamChannel>,
) -> ReviewVerdict {
    channel.send(
        ChannelRole::Reviewer,
        None,
        "Reviewing worker results...".to_string(),
    );

    let results_text = request
        .worker_results
        .iter()
        .map(|(role, result)| format!("## {}\n{}", role, result))
        .collect::<Vec<_>>()
        .join("\n\n");

    let user_prompt = format!(
        "Original task: {}\n\nPlan:\n{}\n\nWorker outputs:\n{}\n\nRespond with JSON only:\n{{\"verdict\": \"pass\" | \"revise\" | \"fail\", \"feedback\": \"...\", \"score\": 0.0-1.0}}",
        request.original_task,
        request.supervisor_plan,
        results_text,
    );

    let system = "You are a strict quality reviewer. Be concise. Return only valid JSON.";

    let messages = vec![ChatMessage::system(system), ChatMessage::user(&user_prompt)];
    let config = CompletionConfig {
        model: model.to_string(),
        max_tokens: 300,
        temperature: 0.0,
        thinking_enabled: false,
    };

    match llm.complete(messages, vec![], &config).await {
        Ok(respond_output) => {
            let response_text = match respond_output {
                RespondOutput::Text { text, .. } => text,
                RespondOutput::ToolCalls { text, .. } => text.unwrap_or_default(),
            };

            let json: serde_json::Value = match serde_json::from_str(&response_text) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        "Reviewer: failed to parse LLM response as JSON: {e}. Response: {}",
                        &response_text[..response_text.len().min(200)]
                    );
                    serde_json::json!({"verdict": "fail", "score": 0.0})
                }
            };

            let verdict_str = json["verdict"].as_str().unwrap_or("pass").to_lowercase();
            let verdict = match verdict_str.as_str() {
                "revise" => ReviewVerdict::Revise(
                    json["feedback"]
                        .as_str()
                        .unwrap_or("Improve output quality")
                        .to_string(),
                ),
                "fail" => ReviewVerdict::Fail(
                    json["feedback"]
                        .as_str()
                        .unwrap_or("Task not completed")
                        .to_string(),
                ),
                _ => ReviewVerdict::Pass,
            };

            let verdict_label = match &verdict {
                ReviewVerdict::Pass => "Pass".to_string(),
                ReviewVerdict::Revise(f) => format!("Revise: {}", f),
                ReviewVerdict::Fail(r) => format!("Fail: {}", r),
            };
            channel.send(
                ChannelRole::Reviewer,
                None,
                format!(
                    "Verdict: {} (score: {:.2})",
                    verdict_label,
                    json["score"].as_f64().unwrap_or(0.0)
                ),
            );

            verdict
        }
        Err(e) => {
            tracing::error!("Reviewer LLM call failed: {e}");
            channel.send(
                ChannelRole::Reviewer,
                None,
                format!("Review failed: {e}. Defaulting to Fail."),
            );
            ReviewVerdict::Fail("Reviewer LLM call failed".to_string())
        }
    }
}
