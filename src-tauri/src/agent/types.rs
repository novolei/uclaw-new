use serde::{Deserialize, Serialize};
use crate::error::Error;

// ─── Thread State Machine ─────────────────────────────────────────────

/// Tracks the lifecycle state of an agent thread.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ThreadState {
    /// Idle — waiting for user input.
    Idle,
    /// Actively processing an iteration.
    Processing,
    /// Paused — a tool requires user approval before continuing.
    AwaitingApproval {
        tool_name: String,
        tool_id: String,
        arguments: serde_json::Value,
    },
    /// The loop completed normally.
    Completed,
    /// The loop was interrupted by a signal.
    Interrupted,
    /// The loop terminated with a failure.
    Failed { error: String },
}

impl ThreadState {
    /// Returns true if the state allows a transition to `Processing`.
    pub fn can_start_processing(&self) -> bool {
        matches!(self, ThreadState::Idle)
    }

    /// Returns true if the thread is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ThreadState::Completed | ThreadState::Interrupted | ThreadState::Failed { .. }
        )
    }
}

// ─── Core Message Types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Thinking { thinking: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String, is_error: Option<bool> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
}

impl ChatMessage {
    pub fn system(text: &str) -> Self {
        Self { role: MessageRole::System, content: vec![ContentBlock::Text { text: text.to_string() }] }
    }
    pub fn user(text: &str) -> Self {
        Self { role: MessageRole::User, content: vec![ContentBlock::Text { text: text.to_string() }] }
    }
    pub fn assistant(text: &str) -> Self {
        Self { role: MessageRole::Assistant, content: vec![ContentBlock::Text { text: text.to_string() }] }
    }
    pub fn assistant_with_tool_use(id: &str, name: &str, input: serde_json::Value) -> Self {
        Self { role: MessageRole::Assistant, content: vec![ContentBlock::ToolUse { id: id.to_string(), name: name.to_string(), input }] }
    }
    pub fn user_tool_result(tool_use_id: &str, content: &str, is_error: bool) -> Self {
        Self { role: MessageRole::User, content: vec![ContentBlock::ToolResult { tool_use_id: tool_use_id.to_string(), content: content.to_string(), is_error: Some(is_error) }] }
    }
}

// ─── Reasoning Context ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ReasoningContext {
    pub messages: Vec<ChatMessage>,
    pub system_prompt: String,
    pub force_text: bool,
    /// Current thread state.
    pub thread_state: ThreadState,
    /// Cumulative token usage tracked across iterations.
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

impl ReasoningContext {
    pub fn new(system_prompt: String) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt,
            force_text: false,
            thread_state: ThreadState::Idle,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    /// Rough estimate of token count based on message content length.
    /// Uses ~4 chars per token heuristic.
    pub fn estimate_token_count(&self) -> usize {
        let system_tokens = self.system_prompt.len() / 4;
        let msg_tokens: usize = self.messages.iter().map(|m| {
            m.content.iter().map(|b| match b {
                ContentBlock::Text { text } => text.len() / 4,
                ContentBlock::Thinking { thinking } => thinking.len() / 4,
                ContentBlock::ToolUse { input, .. } => input.to_string().len() / 4 + 20,
                ContentBlock::ToolResult { content, .. } => content.len() / 4 + 10,
            }).sum::<usize>()
        }).sum();
        system_tokens + msg_tokens
    }
}

// ─── Tool Call / Definition ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ─── Token Usage / Response Metadata ───────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMetadata {
    pub model: String,
    pub finish_reason: Option<String>,
    pub usage: Option<TokenUsage>,
}

// ─── Context & Cost Stats ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextStats {
    pub model_context_length: u32,
    pub system_prompt_tokens: u32,
    pub mcp_prompts_tokens: u32,
    pub skills_tokens: u32,
    pub messages_tokens: u32,
    pub tool_use_tokens: u32,
    pub compact_buffer_tokens: u32,
    pub free_tokens: i32,
    /// Cumulative input tokens from API usage across all iterations.
    pub cumulative_input_tokens: Option<u32>,
    /// Cumulative output tokens from API usage across all iterations.
    pub cumulative_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnCostInfo {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_usd: String,
}

// ─── Token Estimation ─────────────────────────────────────────────────

/// CJK-aware token estimation (fallback when tiktoken is unavailable).
pub fn estimate_tokens(text: &str) -> u32 {
    let mut tokens: f32 = 0.0;
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() {
            tokens += 0.25;
        } else if ch.is_ascii_digit() {
            tokens += 0.4;
        } else if is_cjk(ch) {
            tokens += 1.1;
        } else if ch == '\n' {
            tokens += 1.0;
        } else if ch.is_whitespace() {
            tokens += 0.15;
        } else {
            tokens += 0.5;
        }
    }
    tokens.ceil() as u32 + 4 // message overhead
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x4E00..=0x9FFF |   // CJK Unified Ideographs
        0x3400..=0x4DBF |   // CJK Extension A
        0x3000..=0x303F |   // CJK Symbols & Punctuation
        0x3040..=0x309F |   // Hiragana
        0x30A0..=0x30FF |   // Katakana
        0xAC00..=0xD7AF     // Hangul
    )
}

/// Estimate tokens for a single ChatMessage.
pub fn estimate_message_tokens(msg: &ChatMessage) -> u32 {
    msg.content.iter().map(|b| match b {
        ContentBlock::Text { text } => estimate_tokens(text),
        ContentBlock::Thinking { thinking } => estimate_tokens(thinking),
        ContentBlock::ToolUse { input, name, .. } => {
            estimate_tokens(name) + estimate_tokens(&input.to_string()) + 10
        }
        ContentBlock::ToolResult { content, .. } => estimate_tokens(content) + 5,
    }).sum()
}

// ─── Cost Calculation ─────────────────────────────────────────────────

/// Calculate USD cost based on model pricing (per 1M tokens).
pub fn calculate_cost(model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
    let (input_price, output_price) = match model {
        m if m.contains("claude-3-5-sonnet") || m.contains("claude-4") || m.contains("claude-sonnet-4") => (3.0, 15.0),
        m if m.contains("claude-3-5-haiku") || m.contains("claude-haiku") => (0.8, 4.0),
        m if m.contains("gpt-4o-mini") => (0.15, 0.6),
        m if m.contains("gpt-4o") => (2.5, 10.0),
        m if m.contains("gpt-4") => (2.5, 10.0),
        m if m.contains("deepseek") => (0.14, 0.28),
        m if m.contains("qwen") => (0.5, 2.0),
        _ => (1.0, 3.0),
    };
    (input_tokens as f64 * input_price + output_tokens as f64 * output_price) / 1_000_000.0
}

pub fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

/// Get model context window length.
pub fn get_model_context_length(model: &str) -> u32 {
    match model {
        m if m.contains("claude") => 200_000,
        m if m.contains("gpt-4o") => 128_000,
        m if m.contains("gpt-4") => 128_000,
        m if m.contains("deepseek") => 128_000,
        m if m.contains("qwen") => 131_072,
        _ => 200_000,
    }
}

// ─── LLM Response Outputs ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RespondOutput {
    Text { text: String, thinking: Option<String>, metadata: ResponseMetadata },
    ToolCalls { tool_calls: Vec<ToolCall>, text: Option<String>, thinking: Option<String>, metadata: ResponseMetadata },
}

/// Streaming delta
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamDelta {
    TextDelta { text: String },
    ThinkingDelta { thinking: String },
    ToolCallDelta { id: String, name: Option<String>, input_json: Option<String> },
    Done { finish_reason: Option<String>, usage: Option<TokenUsage> },
}

// ─── Loop Signals & Outcomes ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum LoopSignal {
    Continue,
    Stop,
    Cancel,
    InjectMessage { content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoopOutcome {
    Response { text: String, usage: Option<TokenUsage> },
    ToolResult { results: Vec<String> },
    Stopped,
    Cancelled,
    MaxIterations,
    Failure { error: String },
    NeedApproval { tool_name: String, tool_call_id: String, parameters: serde_json::Value },
}

#[derive(Debug)]
pub enum TextAction {
    Return(LoopOutcome),
    Continue,
}

// ─── Loop Delegate Trait ───────────────────────────────────────────────

#[async_trait::async_trait]
pub trait LoopDelegate: Send + Sync {
    async fn check_signals(&self) -> LoopSignal;
    async fn before_llm_call(&self, reason_ctx: &mut ReasoningContext, iteration: usize) -> Option<LoopOutcome>;
    async fn call_llm(&self, reason_ctx: &mut ReasoningContext, iteration: usize) -> Result<RespondOutput, Error>;
    async fn handle_text_response(&self, text: &str, metadata: ResponseMetadata, reason_ctx: &mut ReasoningContext) -> TextAction;
    async fn execute_tool_calls(&self, tool_calls: Vec<ToolCall>, reason_ctx: &mut ReasoningContext) -> Result<Option<LoopOutcome>, Error>;
    async fn on_tool_intent_nudge(&self, _text: &str, _ctx: &mut ReasoningContext) {}
    async fn on_usage(&self, _usage: &TokenUsage, _reason_ctx: &ReasoningContext) {}
    async fn after_iteration(&self, _iteration: usize) {}
}

// ─── Agentic Loop Config ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AgenticLoopConfig {
    /// Maximum loop iterations before giving up.
    pub max_iterations: usize,
    /// Whether to detect and nudge tool intent without tool calls.
    pub enable_tool_intent_nudge: bool,
    /// Maximum consecutive nudge attempts before stopping.
    pub max_tool_intent_nudges: usize,
    /// Truncations before forcing text-only mode.
    pub max_truncations: usize,
    /// Total token budget for the conversation.
    pub token_budget: usize,
    /// Fraction of token_budget that triggers context compression (0.0–1.0).
    pub compression_threshold: f32,
    /// Fraction of token_budget that triggers hard truncation (0.0–1.0).
    pub hard_truncation_threshold: f32,
    /// Number of recent turns to keep during compression.
    pub compression_keep_turns: usize,
}

impl Default for AgenticLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 50,
            enable_tool_intent_nudge: true,
            max_tool_intent_nudges: 2,
            max_truncations: 3,
            token_budget: 128_000,
            compression_threshold: 0.80,
            hard_truncation_threshold: 0.95,
            compression_keep_turns: 10,
        }
    }
}

// ─── Constants ─────────────────────────────────────────────────────────

pub const TOOL_INTENT_NUDGE: &str = "You mentioned an action — if you intended to use a tool, please re-invoke it. Otherwise, continue your response.";

/// Detect LLM signalling tool intent without actually calling a tool
// ─── Reflection Types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionDetail {
    pub assistant_message_id: String,
    pub status: String, // "queued", "running", "completed", "failed"
    pub outcome: Option<String>, // "updated", "created", "no_op"
    pub summary: Option<String>,
    pub detail: Option<String>,
    pub run_started_at: Option<String>,
    pub run_completed_at: Option<String>,
    pub tool_calls: Vec<ReflectionToolCall>,
    pub messages: Vec<ReflectionMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionToolCall {
    pub id: String,
    pub created_at: String,
    pub name: String,
    pub status: String,
    pub parameters: Option<String>,
    pub result_preview: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionMessage {
    pub id: String,
    pub content: String,
    pub created_at: String,
}

pub fn llm_signals_tool_intent(text: &str) -> bool {
    let lower = text.to_lowercase();
    let patterns = [
        "let me search", "i'll look", "i will search", "let me check",
        "i'll find", "let me find", "i'll read", "let me read",
        "i'll grep", "let me grep", "i'll fetch", "let me fetch",
        "let me run", "i'll run", "let me open", "i'll open",
        "let me list", "i'll list", "let me write", "i'll write",
    ];
    lower.len() < 200 && patterns.iter().any(|p| lower.contains(p))
}
