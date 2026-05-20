//! `ThemeEntry` — UI theme for plugin-extended UIs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::entry::RegistryEntry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeEntry {
    pub id: String,
    /// "builtin" / "plugin" / "user".
    pub kind: String,
    pub display_name: String,
    /// `true` if the theme is meant for dark-background environments.
    pub is_dark: bool,
    /// CSS-variable map: e.g. `{"--bg": "#111", "--fg": "#fff"}`.
    /// Stored as a sorted map for deterministic serialization.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub css_vars: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
}

impl RegistryEntry for ThemeEntry {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &str {
        &self.kind
    }
    fn tags(&self) -> &BTreeMap<String, String> {
        &self.tags
    }
}
