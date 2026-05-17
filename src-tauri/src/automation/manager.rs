//! Humane spec install / uninstall / list / status management.
//! CRUD impl filled in Task 13 onwards.

// NOTE: HumaneAutomationSpec import will be wired up in Task 13 when the
// install/uninstall logic is implemented against the V20b schema.
// use crate::automation::protocol::humane_v1::HumaneAutomationSpec;

/// Typed representation of a row in `automation_specs` (V20b schema).
///
/// Returned by list_automations / install_humane_spec Tauri commands once
/// those are rewritten in Task 13.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumaneSpecRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub system_prompt: String,
    pub spec_format: String,
    pub spec_yaml: String,
    pub spec_json: String,
    /// JSON object — user-supplied overrides for spec `config` fields.
    pub user_config_values: String,
    /// JSON array of granted permission strings.
    pub permissions_granted: String,
    /// JSON array of denied permission strings.
    pub permissions_denied: String,
    /// Lifecycle status: "active" | "paused" | "error" | "uninstalled".
    pub status: String,
    pub enabled: bool,
    /// Optional space (workspace) this spec is scoped to.
    pub space_id: Option<String>,
    /// Installation provenance: "local" | "marketplace" | "toml-migrated".
    pub source: String,
    pub source_ref: Option<String>,
    pub source_version: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_run_at: Option<i64>,
    pub last_run_outcome: Option<String>,
    /// Automation trigger phrase for IM inbound routing (empty string = not set).
    pub trigger_phrase: String,
    /// System prompt override for IM agent-chat sessions (empty string = use default).
    pub system_prompt_override: String,
}
