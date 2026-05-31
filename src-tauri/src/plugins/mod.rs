//! Plugin discovery + manifest → AgentApi/McpManager registration.
//!
//! Per design spec §5: plugins live in `$DATA_DIR/plugins/<id>/`, declare
//! contributions via `plugin.toml`, and are routed through existing
//! infrastructure (McpManager for mcp_servers; AgentApi for tools/commands;
//! `uclaw` MCP capability extension for hooks/renderers).
//!
//! This module is a THIN bridge that reuses McpManager (1,925 LoC of
//! existing JSON-RPC + subprocess infrastructure). It does NOT duplicate
//! protocol code.

pub mod discovery;
pub mod lifecycle;
pub mod registration;
pub mod runtime;
pub mod uclaw_extension;

#[cfg(test)]
mod tests;

pub use discovery::{DiscoveryError, LoadedPlugin, PluginDiscovery};
pub use lifecycle::{PluginLifecycleOwner, PluginLifecycleReport};
pub use registration::{PluginRegistrar, PluginRegistrationSummary, RegistrationError};
pub use runtime::{
    PluginPreflightCategory, PluginPreflightFinding, PluginPreflightReport,
    PluginPreflightSeverity, PluginPreflightSummary, PluginPreflightVerdict, PluginRuntimeStatus,
    PluginRuntimeStatusKind, PluginTrustState, PLUGIN_PREFLIGHT_SCHEMA,
};
pub use uclaw_extension::{UclawCapability, UclawCapabilityNegotiation};
