//! ToolDescriptor — metadata + builder closure for a tool registered through AgentApi.
//!
//! AgentApi owns descriptors at process scope; `build_session_registry` invokes
//! the builders at session-build time with the session's `SessionContext`.

use std::sync::Arc;

use super::session_context::SessionContext;

pub type ToolBuilderFn = Arc<
    dyn for<'a> Fn(&SessionContext<'a>) -> Box<dyn crate::agent::tools::tool::Tool>
        + Send
        + Sync,
>;

/// Descriptor for a tool: process-stable metadata + a session-scoped builder.
///
/// Metadata (name / description / parameters_schema) is reused for prompt
/// assembly and for the LLM's tools/list payload. The builder closure is
/// invoked once per session via `AgentApi.build_session_registry(&ctx)`.
#[derive(Clone)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    pub builder: ToolBuilderFn,
}

impl std::fmt::Debug for ToolDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolDescriptor")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("parameters_schema", &"<json>")
            .field("builder", &"<fn>")
            .finish()
    }
}
