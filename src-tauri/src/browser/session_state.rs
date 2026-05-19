use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTaskStep {
    pub step_index: u32,
    pub observation_summary: String,
    pub reasoning: String,
    pub action_name: String,
    pub action_args: serde_json::Value,
    pub ok: bool,
    pub message: Option<String>,
    pub error: Option<String>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTaskRun {
    pub run_id: String,
    pub session_id: String,
    pub task: String,
    pub status: BrowserTaskStatus,
    pub steps: Vec<BrowserTaskStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserTaskStatus {
    Running,
    Completed,
    Failed,
    Stopped,
}
