//! `SkillEntry` — built-in or plugin-installed skill descriptor.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::entry::RegistryEntry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntry {
    pub id: String,
    /// "builtin" / "plugin" / "user".
    pub kind: String,
    pub title: String,
    pub description: String,
    pub token_estimate: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
}

impl RegistryEntry for SkillEntry {
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
