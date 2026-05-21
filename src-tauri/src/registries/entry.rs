//! Common contract every registry entry implements.

use std::collections::BTreeMap;

/// What every registry entry must expose.
///
/// Strings rather than typed ids so the `Registry<E>` storage layer
/// stays a thin map without generic-id plumbing. Callers convert
/// strings to typed ids at the boundary.
pub trait RegistryEntry {
    /// Stable id within the registry's namespace.
    fn id(&self) -> &str;
    /// Sub-kind / category (e.g. "anthropic-mcp" for a connector).
    fn kind(&self) -> &str;
    /// Free-form tags used for filtering. Empty = untagged.
    fn tags(&self) -> &BTreeMap<String, String>;
}

/// Failures the registry layer can surface to callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// `register` was called with an id already in the registry.
    DuplicateId(String),
    /// `lookup` was called with an id the registry doesn't have.
    NotFound(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::DuplicateId(id) => write!(f, "registry id already exists: {id}"),
            RegistryError::NotFound(id) => write!(f, "registry id not found: {id}"),
        }
    }
}

impl std::error::Error for RegistryError {}
