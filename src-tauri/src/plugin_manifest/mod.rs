//! M7-T1 — Plugin manifest schema (reserved for future subprocess RPC protocol).
//!
//! Typed manifest schema only. The TOML loader (`load_plugin_manifest`) and
//! the `.plugin` zip installer were removed in P2 cleanup — installer commit 2
//! never landed, zero non-test callers. The manifest type schema
//! (`PluginManifest`, `PluginContribution`, `PluginAuthor`, `PluginPermissions`,
//! `PluginRuntimeRequirement`) remains valuable for the future subprocess RPC
//! plugin protocol per ADR §6.5.
//!
//! Layout:
//!
//! - [`schema`] — `PluginManifest`, `PluginContribution`, etc.

pub mod schema;

pub use schema::{
    PluginAuthor, PluginContribution, PluginManifest, PluginPermissions,
    PluginRuntimeRequirement,
};
