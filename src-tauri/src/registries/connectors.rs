//! `ConnectorEntry` — MCP server / chrome / excel / native connector.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::entry::RegistryEntry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorEntry {
    pub id: String,
    /// "anthropic-mcp" / "chrome-extension" / "excel" / "native" / etc.
    pub kind: String,
    pub display_name: String,
    /// OAuth or API-key flavor (informational; secrets stay outside
    /// the registry).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_kind: Option<String>,
    /// `true` if user has approved + connected this connector.
    pub connected: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
}

impl RegistryEntry for ConnectorEntry {
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
