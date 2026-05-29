//! Plugin manifest types.

use serde::{Deserialize, Serialize};

/// Author block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginAuthor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Permissions the plugin asks for at install time. User confirms each.
///
/// Mirrors macOS-style "this app wants" categories.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPermissions {
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub filesystem_read: bool,
    #[serde(default)]
    pub filesystem_write: bool,
    #[serde(default)]
    pub memory_read: bool,
    #[serde(default)]
    pub memory_write: bool,
    #[serde(default)]
    pub run_subprocess: bool,
    /// Free-form additional permission identifiers — plugins can
    /// request capabilities that haven't been promoted to a typed
    /// field yet.
    #[serde(default)]
    pub additional: Vec<String>,
}

impl PluginPermissions {
    /// Convenient accessor: total number of permission categories
    /// requested. Useful for the install-time UI ("requests N
    /// permissions").
    pub fn count(&self) -> usize {
        [
            self.network,
            self.filesystem_read,
            self.filesystem_write,
            self.memory_read,
            self.memory_write,
            self.run_subprocess,
        ]
        .iter()
        .filter(|b| **b)
        .count()
            + self.additional.len()
    }
}

/// Runtime requirements + optional subprocess invocation details.
///
/// `min_uclaw_version` is required (compatibility check at load time).
/// The subprocess fields (`kind`, `executable`, `args`, `working_dir`)
/// are all optional — plugins that only contribute non-subprocess items
/// (skills, themes) don't need them.  Existing manifests that only set
/// `min_uclaw_version` continue to parse unchanged (additive extension).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRuntimeRequirement {
    /// Semver-ish lower bound, e.g. `"0.4.0"`. Manifests fail to load if
    /// the running uClaw is older than this.
    pub min_uclaw_version: String,

    /// Subprocess kind (`"subprocess"` today; future: `"wasm"`, `"inproc"`).
    /// Absent means the plugin has no subprocess component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// Path to the executable to spawn, relative to the plugin directory.
    /// Absent means no subprocess is spawned at load time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,

    /// CLI arguments forwarded to the executable.  Defaults to empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Working-directory override for the subprocess, relative to the plugin
    /// directory.  When absent the subprocess inherits the parent's cwd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
}

/// What the plugin contributes to the registries. Each list is
/// optional (default empty).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginContribution {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<String>,
}

impl PluginContribution {
    pub fn total_items(&self) -> usize {
        self.mcp_servers.len()
            + self.skills.len()
            + self.commands.len()
            + self.tools.len()
            + self.themes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.total_items() == 0
    }
}

/// Top-level manifest deserialized from `plugin.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub author: PluginAuthor,
    pub runtime: PluginRuntimeRequirement,
    #[serde(default)]
    pub permissions: PluginPermissions,
    #[serde(default)]
    pub contributes: PluginContribution,
}

impl PluginManifest {
    /// `true` if the plugin contributes nothing — a hollow plugin
    /// shouldn't pass validation in commit 2.
    pub fn contributes_anything(&self) -> bool {
        !self.contributes.is_empty()
    }

    /// `true` if the plugin requests at least one permission.
    /// Permission-free plugins skip the install-time confirmation.
    pub fn requests_permissions(&self) -> bool {
        self.permissions.count() > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_manifest() -> PluginManifest {
        PluginManifest {
            id: "com.example.slacker".into(),
            version: "1.2.3".into(),
            display_name: "Slacker".into(),
            description: Some("A slack connector".into()),
            author: PluginAuthor {
                name: "Acme Co".into(),
                email: Some("ops@acme.com".into()),
                url: Some("https://acme.com".into()),
            },
            runtime: PluginRuntimeRequirement {
                min_uclaw_version: "0.4.0".into(),
                kind: None,
                executable: None,
                args: vec![],
                working_dir: None,
            },
            permissions: PluginPermissions {
                network: true,
                filesystem_read: false,
                filesystem_write: false,
                memory_read: true,
                memory_write: false,
                run_subprocess: false,
                additional: vec!["clipboard:read".into()],
            },
            contributes: PluginContribution {
                mcp_servers: vec!["slack-mcp".into()],
                skills: vec!["search-slack".into(), "post-slack".into()],
                commands: vec!["/slack".into()],
                tools: vec![],
                themes: vec!["acme-dark".into()],
            },
        }
    }

    // ── PluginPermissions ──────────────────────────────────────────

    #[test]
    fn permissions_count_sums_bool_fields_and_additional() {
        let p = PluginPermissions {
            network: true,
            filesystem_read: true,
            filesystem_write: false,
            memory_read: false,
            memory_write: true,
            run_subprocess: false,
            additional: vec!["a".into(), "b".into()],
        };
        // 3 bools + 2 additional = 5.
        assert_eq!(p.count(), 5);
    }

    #[test]
    fn permissions_default_count_zero() {
        assert_eq!(PluginPermissions::default().count(), 0);
    }

    // ── PluginContribution ─────────────────────────────────────────

    #[test]
    fn contribution_total_items_sums_categories() {
        let c = PluginContribution {
            mcp_servers: vec!["a".into()],
            skills: vec!["x".into(), "y".into()],
            commands: vec!["/q".into()],
            tools: vec!["t1".into()],
            themes: vec![],
        };
        assert_eq!(c.total_items(), 5);
        assert!(!c.is_empty());
    }

    #[test]
    fn empty_contribution_is_empty() {
        let c = PluginContribution::default();
        assert!(c.is_empty());
        assert_eq!(c.total_items(), 0);
    }

    // ── PluginManifest accessors ──────────────────────────────────

    #[test]
    fn contributes_anything_true_for_populated() {
        assert!(full_manifest().contributes_anything());
    }

    #[test]
    fn contributes_anything_false_for_hollow() {
        let mut m = full_manifest();
        m.contributes = PluginContribution::default();
        assert!(!m.contributes_anything());
    }

    #[test]
    fn requests_permissions_reflects_permission_block() {
        let m = full_manifest();
        assert!(m.requests_permissions());
        let mut m = m;
        m.permissions = PluginPermissions::default();
        assert!(!m.requests_permissions());
    }

    // ── serde JSON roundtrip ──────────────────────────────────────

    #[test]
    fn manifest_json_roundtrip() {
        let m = full_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn manifest_minimal_json_deserializes() {
        // Only required fields. Permissions + contributes default.
        let json = r#"{
            "id": "x",
            "version": "0.1.0",
            "display_name": "X",
            "author": {"name": "A"},
            "runtime": {"min_uclaw_version": "0.4.0"}
        }"#;
        let m: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, "x");
        assert!(!m.requests_permissions());
        assert!(!m.contributes_anything());
        assert!(m.description.is_none());
    }

    // ── optional fields skipped ───────────────────────────────────

    #[test]
    fn author_optional_fields_skipped_when_none() {
        let a = PluginAuthor {
            name: "Solo".into(),
            email: None,
            url: None,
        };
        let json = serde_json::to_string(&a).unwrap();
        assert!(!json.contains("email"));
        assert!(!json.contains("url"));
    }

    #[test]
    fn empty_contribution_lists_skipped_in_json() {
        let c = PluginContribution::default();
        let json = serde_json::to_string(&c).unwrap();
        // Default = empty object.
        assert_eq!(json, "{}");
    }
}
