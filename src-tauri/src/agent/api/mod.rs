//! AgentApi — single handle replacing the 4-Registry pattern.
//!
//! Pi ExtensionAPI shape, materialized as a Rust struct. Boot: register builtins
//! via `&mut self`; after boot the handle is wrapped in `Arc` and shared via
//! `AppState.agent_api`. Runtime queries use `&self`.
//!
//! See: `docs/superpowers/specs/2026-05-28-stage3-agentapi-handle-design.md` §4.

pub mod command;
pub mod events;
pub mod hookbus_bridge;
pub mod plugin;
pub mod renderer;
pub mod session_context;
pub mod tool;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use futures::future::BoxFuture;

use crate::providers::service::ProviderService;

use self::command::Command;
use self::events::{Event, EventKind, EventOutcome};
use self::plugin::{PluginId, PluginRegistrationSet};
use self::renderer::{Renderer, RendererFn};
use self::session_context::SessionContext;
use self::tool::ToolDescriptor;

pub type HookFn =
    Arc<dyn Fn(&Event) -> BoxFuture<'static, Result<EventOutcome, String>> + Send + Sync>;

pub struct AgentApi {
    pub(crate) tools: HashMap<String, Arc<ToolDescriptor>>,
    pub(crate) provider_service: Option<Arc<ProviderService>>,
    pub(crate) hook_bus: Option<Arc<crate::agent::hook_bus::HookBus>>,
    pub(crate) commands: HashMap<String, Arc<Command>>,
    pub(crate) renderers: HashMap<&'static str, RendererFn>,
    pub(crate) hooks: HashMap<EventKind, Vec<HookFn>>,
    pub(crate) plugin_index: HashMap<PluginId, PluginRegistrationSet>,
}

impl AgentApi {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            provider_service: None,
            hook_bus: None,
            commands: HashMap::new(),
            renderers: HashMap::new(),
            hooks: HashMap::new(),
            plugin_index: HashMap::new(),
        }
    }

    /// Register a tool descriptor. The builder closure is invoked at
    /// session-build time (via `build_session_registry`) to construct a
    /// concrete `Box<dyn Tool>` instance per session.
    pub fn register_tool(&mut self, descriptor: ToolDescriptor) {
        let name = descriptor.name.clone();
        self.tools.insert(name, Arc::new(descriptor));
    }

    /// Look up a registered tool descriptor by name. Returns the descriptor,
    /// not the instance — callers wanting an instance use `build_session_registry`.
    pub fn tool(&self, name: &str) -> Option<&Arc<ToolDescriptor>> {
        self.tools.get(name)
    }

    /// Construct a session-scoped `ToolRegistry` by invoking each registered
    /// `ToolDescriptor.builder` with the given `SessionContext`.
    ///
    /// Walks descriptors in insertion order (HashMap order is non-deterministic
    /// but `ToolRegistry::list_definitions` sorts by name for prompt-cache
    /// stability, so insertion order doesn't affect agent behavior). Each
    /// builder produces a `Box<dyn Tool>` instance registered into a fresh
    /// `ToolRegistry`.
    pub fn build_session_registry(
        &self,
        ctx: &SessionContext<'_>,
    ) -> crate::agent::tools::tool::ToolRegistry {
        let mut registry = crate::agent::tools::tool::ToolRegistry::new();
        for descriptor in self.tools.values() {
            let instance = (descriptor.builder)(ctx);
            registry.register_boxed(instance);
        }
        registry
    }

    /// Test-only shim: constructs an empty registry without invoking builders.
    /// Used by unit tests that can't build a live `SessionContext` cheaply.
    /// The real `build_session_registry` is exercised via Task 5/6 integration
    /// in the live AppState boot path.
    #[cfg(test)]
    pub(crate) fn build_session_registry_empty_for_test(
        &self,
    ) -> crate::agent::tools::tool::ToolRegistry {
        crate::agent::tools::tool::ToolRegistry::new()
    }

    /// Set the singleton ProviderService handle. Called once at boot
    /// (`AppState::new()`) before `Arc::new(api)` seals. Last write wins.
    pub fn set_provider_service(&mut self, svc: Arc<ProviderService>) {
        self.provider_service = Some(svc);
    }

    /// Get the singleton ProviderService handle if set. Returns None if
    /// not yet wired (pre-boot or in unit tests using AgentApi::new()).
    pub fn provider_service(&self) -> Option<&Arc<ProviderService>> {
        self.provider_service.as_ref()
    }

    /// Set the singleton HookBus handle. Called once at boot
    /// (`AppState::new()`) before `Arc::new(api)` seals. Enables the
    /// HookBus bridge in `emit()` and `emit_with_decision()`.
    pub fn set_hook_bus(&mut self, bus: Arc<crate::agent::hook_bus::HookBus>) {
        self.hook_bus = Some(bus);
    }

    /// Get the singleton HookBus handle if set.
    pub fn hook_bus(&self) -> Option<&Arc<crate::agent::hook_bus::HookBus>> {
        self.hook_bus.as_ref()
    }

    /// Register a slash command.
    pub fn register_command(&mut self, cmd: Command) {
        let name = cmd.name.clone();
        self.commands.insert(name, Arc::new(cmd));
    }

    /// Look up a registered command by name.
    pub fn command(&self, name: &str) -> Option<&Arc<Command>> {
        self.commands.get(name)
    }

    /// Register a renderer for a specific custom_type.
    pub fn register_renderer(&mut self, r: Renderer) {
        self.renderers.insert(r.custom_type, r.render);
    }

    /// Look up a registered renderer by custom_type.
    pub fn renderer(&self, custom_type: &str) -> Option<&RendererFn> {
        self.renderers.get(custom_type)
    }

    /// Register a hook handler for an event kind. Hooks fire in registration order.
    pub fn on<F>(&mut self, ev: EventKind, h: F)
    where
        F: Fn(&Event) -> BoxFuture<'static, Result<EventOutcome, String>> + Send + Sync + 'static,
    {
        self.hooks.entry(ev).or_default().push(Arc::new(h));
    }

    /// Fire an event. Hooks for `ev.kind` run in registration order. The first
    /// hook returning `Abort` or `Patch` short-circuits AgentApi hooks; `Continue`
    /// outcomes are skipped. If no hooks are registered for the kind, the
    /// AgentApi outcome is `Continue`.
    ///
    /// **HookBus bridge (P3-3)**: after AgentApi hooks complete (regardless of
    /// outcome), if a HookBus is wired AND this Event has a HookEvent peer
    /// (per `event_to_hook_event`), fan out to `hook_bus.dispatch_observe`.
    /// The HookBus result does NOT affect the returned EventOutcome — this is
    /// observe-only fan-out for audit/logging subscribers. For decision-capable
    /// fan-out use `emit_with_decision`.
    pub async fn emit(&self, ev: Event) -> Result<EventOutcome, String> {
        let kind = ev.kind;
        let agentapi_outcome = if let Some(hooks) = self.hooks.get(&kind) {
            let mut outcome = EventOutcome::Continue;
            for h in hooks {
                let result = h(&ev).await?;
                match result {
                    EventOutcome::Continue => continue,
                    other => {
                        outcome = other;
                        break;
                    }
                }
            }
            outcome
        } else {
            EventOutcome::Continue
        };

        // Bridge: observe-only HookBus dispatch (always — subscribers see
        // the event regardless of AgentApi veto, for audit logging).
        if let Some(bus) = &self.hook_bus {
            if let Some(hook_event) = crate::agent::api::hookbus_bridge::event_to_hook_event(&ev) {
                bus.dispatch_observe(&hook_event).await;
            }
        }

        Ok(agentapi_outcome)
    }

    /// Like `emit()` but uses HookBus's `dispatch_with_decision` for the
    /// decision-capable fan-out.
    ///
    /// AgentApi hooks run first (same as `emit`). If their outcome is `Continue`
    /// AND HookBus is wired AND the event has a HookEvent peer, HookBus's
    /// verdict is folded into the final outcome:
    /// - `HookDecision::Allow` → `EventOutcome::Continue`
    /// - `HookDecision::Deny { reason }` → `EventOutcome::Abort(reason)`
    /// - `HookDecision::AskUser { ... }` → `EventOutcome::Abort("askuser:...")`
    ///
    /// If AgentApi hooks return `Patch` or `Abort`, HookBus is NOT consulted
    /// (AgentApi-side veto has priority over policy aggregation).
    ///
    /// Used by callers wanting to consult policy subscribers
    /// (PolicySpecSubscriber, human-boundary gates, etc.). Caller checks
    /// `EventOutcome::Abort` reason for the "askuser:" prefix to distinguish
    /// from regular denials.
    pub async fn emit_with_decision(&self, ev: Event) -> Result<EventOutcome, String> {
        let kind = ev.kind;
        let agentapi_outcome = if let Some(hooks) = self.hooks.get(&kind) {
            let mut outcome = EventOutcome::Continue;
            for h in hooks {
                let result = h(&ev).await?;
                match result {
                    EventOutcome::Continue => continue,
                    other => {
                        outcome = other;
                        break;
                    }
                }
            }
            outcome
        } else {
            EventOutcome::Continue
        };

        // If AgentApi hooks short-circuited, return that — HookBus is NOT
        // consulted (AgentApi-side veto has priority).
        if !matches!(agentapi_outcome, EventOutcome::Continue) {
            return Ok(agentapi_outcome);
        }

        // Fan out to HookBus's decision-capable dispatch.
        if let Some(bus) = &self.hook_bus {
            if let Some(hook_event) = crate::agent::api::hookbus_bridge::event_to_hook_event(&ev) {
                let decision = bus.dispatch_with_decision(&hook_event).await;
                return Ok(
                    crate::agent::api::hookbus_bridge::hook_decision_to_event_outcome(decision),
                );
            }
        }

        Ok(EventOutcome::Continue)
    }

    /// Record the set of registrations a subprocess plugin contributed.
    /// Called by `SubprocessPluginManager` AFTER the corresponding register_*
    /// calls. Used for clean unregistration on plugin shutdown (P3-4 surface).
    pub(crate) fn register_plugin(&mut self, id: PluginId, set: PluginRegistrationSet) {
        self.plugin_index.insert(id, set);
    }

    /// Remove all contributions from the given plugin. Inverse of register_plugin
    /// + the underlying register_tool/provider/command/renderer calls.
    ///
    /// NOTE: Hook unregistration is INTENTIONALLY a no-op in P3-1. P3-4 will
    /// introduce a `HookFn` wrapper that carries an optional `PluginId`, and
    /// this method will then filter hooks by plugin attribution. P3-1 has no
    /// subprocess hooks registered — only compile-time hooks, which never need
    /// to be unregistered — so the no-op is correct for this PR's scope.
    ///
    /// NOTE: ProviderService is a singleton (P3-3), not a registry. Plugin
    /// unregistration does not clear the singleton. The singleton is a
    /// process-scope resource managed at boot time.
    pub(crate) fn unregister_plugin(&mut self, id: &PluginId) -> Option<PluginRegistrationSet> {
        if let Some(set) = self.plugin_index.remove(id) {
            for name in &set.tools {
                self.tools.remove(name);
            }
            // provider_service: singleton, not cleared by plugin unregistration
            for cname in &set.commands {
                self.commands.remove(cname);
            }
            for ct in &set.renderers {
                self.renderers.remove(ct);
            }
            // Hooks: see method docstring — P3-4 surface, intentional no-op here.
            Some(set)
        } else {
            None
        }
    }
}

impl Default for AgentApi {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AgentApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentApi")
            .field("tools", &self.tools.len())
            .field("provider_service", &self.provider_service.is_some())
            .field("hook_bus", &self.hook_bus.is_some())
            .field("commands", &self.commands.len())
            .field("renderers", &self.renderers.len())
            .field(
                "hooks_total",
                &self.hooks.values().map(|v| v.len()).sum::<usize>(),
            )
            .field("plugins", &self.plugin_index.len())
            .finish()
    }
}
