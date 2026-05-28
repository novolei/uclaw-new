//! AgentApi — single handle replacing the 4-Registry pattern.
//!
//! Pi ExtensionAPI shape, materialized as a Rust struct. Boot: register builtins
//! via `&mut self`; after boot the handle is wrapped in `Arc` and shared via
//! `AppState.agent_api`. Runtime queries use `&self`.
//!
//! See: `docs/superpowers/specs/2026-05-28-stage3-agentapi-handle-design.md` §4.

pub mod command;
pub mod events;
pub mod plugin;
pub mod renderer;
pub mod session_context;
pub mod tool;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use futures::future::BoxFuture;

use crate::agent::tools::tool::Tool;
use crate::providers::service::ProviderService;

use self::command::Command;
use self::events::{Event, EventKind, EventOutcome};
use self::plugin::{PluginId, PluginRegistrationSet};
use self::renderer::{Renderer, RendererFn};

pub type HookFn = Arc<
    dyn Fn(&Event) -> BoxFuture<'static, Result<EventOutcome, String>>
        + Send
        + Sync,
>;

pub struct AgentApi {
    pub(crate) tools: HashMap<String, Arc<dyn Tool>>,
    pub(crate) providers: HashMap<String, Arc<ProviderService>>,
    pub(crate) commands: HashMap<String, Arc<Command>>,
    pub(crate) renderers: HashMap<&'static str, RendererFn>,
    pub(crate) hooks: HashMap<EventKind, Vec<HookFn>>,
    pub(crate) plugin_index: HashMap<PluginId, PluginRegistrationSet>,
}

impl AgentApi {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            providers: HashMap::new(),
            commands: HashMap::new(),
            renderers: HashMap::new(),
            hooks: HashMap::new(),
            plugin_index: HashMap::new(),
        }
    }

    /// Register a tool by its name. Last write wins on name collision.
    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    /// Look up a registered tool by name.
    pub fn tool(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    /// Register a provider by its id.
    pub fn register_provider(&mut self, id: String, provider: Arc<ProviderService>) {
        self.providers.insert(id, provider);
    }

    /// Look up a registered provider by id.
    pub fn provider(&self, id: &str) -> Option<&Arc<ProviderService>> {
        self.providers.get(id)
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
        F: Fn(&Event) -> BoxFuture<'static, Result<EventOutcome, String>>
            + Send
            + Sync
            + 'static,
    {
        self.hooks.entry(ev).or_default().push(Arc::new(h));
    }

    /// Fire an event. Hooks for `ev.kind` run in registration order. The first
    /// hook returning `Abort` or `Patch` short-circuits and the outcome is returned;
    /// `Continue` outcomes are skipped and the next hook runs.
    /// If no hooks are registered for the kind, returns `Continue`.
    pub async fn emit(&self, ev: Event) -> Result<EventOutcome, String> {
        let kind = ev.kind;
        let Some(hooks) = self.hooks.get(&kind) else {
            return Ok(EventOutcome::Continue);
        };
        for h in hooks {
            let outcome = h(&ev).await?;
            match outcome {
                EventOutcome::Continue => continue,
                EventOutcome::Patch(_) | EventOutcome::Abort(_) => return Ok(outcome),
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
    pub(crate) fn unregister_plugin(&mut self, id: &PluginId) {
        if let Some(set) = self.plugin_index.remove(id) {
            for name in &set.tools {
                self.tools.remove(name);
            }
            for pid in &set.providers {
                self.providers.remove(pid);
            }
            for cname in &set.commands {
                self.commands.remove(cname);
            }
            for ct in &set.renderers {
                self.renderers.remove(ct);
            }
            // Hooks: see method docstring — P3-4 surface, intentional no-op here.
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
            .field("providers", &self.providers.len())
            .field("commands", &self.commands.len())
            .field("renderers", &self.renderers.len())
            .field("hooks_total", &self.hooks.values().map(|v| v.len()).sum::<usize>())
            .field("plugins", &self.plugin_index.len())
            .finish()
    }
}
