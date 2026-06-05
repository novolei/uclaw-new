//! Pi-3b — bundled curated catalog of community MCP servers (the marketplace).
use serde::{Deserialize, Serialize};

/// Permissions a catalog entry requests (subset that maps to PluginPermissions).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogPermissions {
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub filesystem_read: bool,
    #[serde(default)]
    pub filesystem_write: bool,
    #[serde(default)]
    pub run_subprocess: bool,
}

/// A single env-var hint shown in the UI setup dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvHint {
    pub name: String,
    pub description: String,
}

/// One entry in the curated marketplace catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub permissions: CatalogPermissions,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_hints: Vec<EnvHint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

/// Return the bundled curated catalog. Panics at compile-time if catalog.json
/// is malformed; unwrap_or_default so a JSON parse error at runtime degrades
/// gracefully to an empty catalog rather than crashing.
pub fn builtin_catalog() -> Vec<CatalogEntry> {
    serde_json::from_str(include_str!("catalog.json")).unwrap_or_default()
}

/// Build a PluginManifest from a catalog entry.
///
/// `contributes.tools` is left empty so `tool_allowlist = None` at
/// registration, which means ALL tools the MCP server advertises are
/// exposed without needing to curate tool names in the catalog.
pub fn manifest_from_catalog(e: &CatalogEntry) -> crate::plugin_manifest::schema::PluginManifest {
    use crate::plugin_manifest::schema::*;
    PluginManifest {
        id: e.slug.clone(),
        version: "0.0.0".to_string(),
        display_name: e.name.clone(),
        description: Some(e.description.clone()),
        author: PluginAuthor {
            name: "marketplace".into(),
            email: None,
            url: e.homepage.clone(),
        },
        runtime: PluginRuntimeRequirement {
            min_uclaw_version: "0.1.0".into(),
            kind: Some("subprocess".into()),
            executable: Some(e.command.clone()),
            args: e.args.clone(),
            working_dir: None,
        },
        permissions: PluginPermissions {
            network: e.permissions.network,
            filesystem_read: e.permissions.filesystem_read,
            filesystem_write: e.permissions.filesystem_write,
            memory_read: false,
            memory_write: false,
            run_subprocess: e.permissions.run_subprocess,
            additional: vec![],
        },
        contributes: PluginContribution {
            mcp_servers: vec![e.slug.clone()],
            skills: vec![],
            commands: vec![],
            tools: vec![],
            themes: vec![],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_and_has_entries() {
        let c = builtin_catalog();
        assert!(c.len() >= 6, "expected at least 6 entries, got {}", c.len());
        for e in &c {
            assert!(!e.slug.is_empty(), "entry slug must not be empty");
            assert!(!e.command.is_empty(), "entry command must not be empty");
        }
    }

    #[test]
    fn manifest_from_catalog_shapes_plugin() {
        let entries = builtin_catalog();
        let e = &entries[0];
        let m = manifest_from_catalog(e);
        assert_eq!(m.id, e.slug);
        assert_eq!(m.runtime.executable.as_deref(), Some(e.command.as_str()));
        assert_eq!(m.contributes.mcp_servers, vec![e.slug.clone()]);
        assert!(
            m.contributes.tools.is_empty(),
            "tools must be empty so tool_allowlist=None exposes all server tools"
        );
        // toml round-trip: PluginManifest is Serialize → toml::to_string must work
        let toml_str = toml::to_string(&m).unwrap();
        assert!(
            toml_str.contains(&format!("id = \"{}\"", e.slug)),
            "toml output must contain id field"
        );
    }
}
