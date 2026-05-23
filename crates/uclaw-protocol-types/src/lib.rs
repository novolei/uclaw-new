//! Shared protocol envelope types for uClaw IPC, runtime traces,
//! and future provider/plugin boundaries.

use serde::{Deserialize, Serialize};

pub use uclaw_message_types::{ChatMessage, ContentBlock, MessageRole};
pub use uclaw_runtime_contracts::{
    AutonomyLevel, CapabilityQuery, ContextRef, IntentOrigin, IntentSpec, RiskClass, TaskEvent,
    TaskEventSource, TaskSpec, TaskVerdict,
};
pub use uclaw_tool_types::{ToolCall, ToolDefinition};

pub const UCLAW_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolDomain {
    Agent,
    Browser,
    Automation,
    Tool,
    Harness,
    World,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolEnvelope<T> {
    pub version: u32,
    pub domain: ProtocolDomain,
    pub payload: T,
}

impl<T> ProtocolEnvelope<T> {
    pub fn new(domain: ProtocolDomain, payload: T) -> Self {
        Self {
            version: UCLAW_PROTOCOL_VERSION,
            domain,
            payload,
        }
    }
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
