//! Plugin attribution — tracks which subprocess plugin registered which items.
//!
//! Populated by `SubprocessPluginManager` during the registration step of the
//! plugin lifecycle (P3-4). Used to unregister cleanly when a subprocess plugin
//! crashes or exits.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginId(pub String);

impl PluginId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The set of things a single plugin contributed via the AgentApi handle.
/// Used to roll back registrations when the plugin shuts down.
#[derive(Debug, Clone, Default)]
pub struct PluginRegistrationSet {
    pub tools: Vec<String>,           // tool names
    pub providers: Vec<String>,        // provider ids
    pub commands: Vec<String>,         // command names
    pub renderers: Vec<&'static str>,  // renderer custom_types
    pub hook_events: Vec<crate::agent::api::events::EventKind>,
}
