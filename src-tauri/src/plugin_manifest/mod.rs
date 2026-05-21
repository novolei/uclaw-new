//! M7-T1 — Plugin manifest schema (pilot).
//!
//! A `.plugin` file is a zipped directory containing a top-level
//! `plugin.toml` manifest. The manifest declares:
//!
//! - plugin id + version + display name + author
//! - what the plugin contributes:
//!   - `mcp_servers` — connectors the plugin ships
//!   - `skills` — SKILL.md entries the plugin ships
//!   - `commands` — slash commands
//!   - `tools` — built-in tool registrations
//!   - `themes` — UI themes
//! - permission requirements
//! - target uClaw runtime version
//!
//! This pilot ships the **typed manifest** + a TOML-string loader
//! that uses `serde::Deserialize` (toml crate already in workspace).
//! Installer that unzips the archive + registers contributions into
//! the registries lives in M7-T1 commit 2.
//!
//! Layout:
//!
//! - [`schema`] — `PluginManifest`, `PluginContribution`, etc.
//! - [`load`] — `load_plugin_manifest` from TOML string

pub mod load;
pub mod schema;

pub use load::{load_plugin_manifest, PluginLoadError};
pub use schema::{
    PluginAuthor, PluginContribution, PluginManifest, PluginPermissions,
    PluginRuntimeRequirement,
};
