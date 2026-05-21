//! `ToolEntry` — individual tool descriptor (resolved by name).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::entry::RegistryEntry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolEntry {
    /// Fully-qualified id: `"server::tool"` for MCP, plain `"shell"`
    /// for builtins.
    pub id: String,
    /// "builtin" / "mcp" / "plugin".
    pub kind: String,
    pub description: String,
    /// JSON-schema for the tool's args (raw `serde_json::Value` so
    /// the registry doesn't need to depend on schemars).
    pub schema: serde_json::Value,
    /// `true` if the tool's side effects need user approval before
    /// running (e.g. shell, send-message, file-write).
    pub requires_permission: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
}

impl RegistryEntry for ToolEntry {
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
