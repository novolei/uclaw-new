#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LiveComment {
    pub platform: String,
    pub platform_comment_id: String,
    pub author_id: String,
    pub author_name: String,
    pub text: String,
    pub timestamp_ms: i64,
    pub badges: Vec<String>,
    pub is_new: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModerationActionKind {
    Warn,
    Mute,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModerationAction {
    pub kind: ModerationActionKind,
    pub author_id: String,
    pub reason: String,
    pub evidence_comment_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModerationDecision {
    pub actions: Vec<ModerationAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModerationConfig {
    pub spam_window_seconds: i64,
    pub spam_threshold: usize,
    pub whitelisted_author_ids: Vec<String>,
}

impl Default for ModerationConfig {
    fn default() -> Self {
        Self {
            spam_window_seconds: 60,
            spam_threshold: 5,
            whitelisted_author_ids: Vec::new(),
        }
    }
}
