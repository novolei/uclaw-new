//! Event surface for AgentApi hooks (Pi ExtensionAPI parallel; smaller scope).
//!
//! `EventKind` is intentionally smaller than Pi's 32-event set — uClaw-essential
//! only (13 events). New events should only be added when a hook needs them.

use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    SessionStart,
    SessionShutdown,
    TurnStart,
    TurnEnd,
    BeforeProviderRequest,
    AfterProviderResponse,
    ToolCall,
    ToolResult,
    MessageStart,
    MessageEnd,
    BeforeContextAssembly,
    BeforeCancellation,
    PluginShutdown,
}

/// Payload variants matched to `EventKind`. New kinds must add a variant here.
#[derive(Debug, Clone)]
pub enum EventPayload {
    SessionStart { session_id: String },
    SessionShutdown { session_id: String },
    TurnStart { turn_id: String },
    TurnEnd { turn_id: String, duration_ms: u64 },
    BeforeProviderRequest { provider: String, model: String },
    AfterProviderResponse { provider: String, model: String, token_count: u64 },
    ToolCall { tool_name: String, args: serde_json::Value },
    ToolResult { tool_name: String, result: serde_json::Value },
    MessageStart { message_id: String },
    MessageEnd { message_id: String },
    BeforeContextAssembly { session_id: String },
    BeforeCancellation { reason: String },
    PluginShutdown { plugin_id: String },
}

/// Patches a hook can return to mutate downstream state.
#[derive(Debug, Clone)]
pub enum EventPatch {
    ToolResult(serde_json::Value),
    Context(String),
    Message(String),
}

/// Hook outcome — fold into the loop's next step.
#[derive(Debug, Clone)]
pub enum EventOutcome {
    /// No mutation; loop continues normally.
    Continue,
    /// Replace some downstream value (variant determines which).
    Patch(EventPatch),
    /// Hook vetoes; loop surfaces as a safety/policy denial.
    Abort(String),
}

/// Event envelope passed to every hook. `cancellation_token` ties hook execution
/// to Slice 1a's cancellation flight points.
pub struct Event {
    pub kind: EventKind,
    pub payload: EventPayload,
    pub session_id: String,
    pub cancellation_token: CancellationToken,
}
