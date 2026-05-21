//! TOML-string loader for `plugin.toml`.

use super::schema::PluginManifest;

/// Failures `load_plugin_manifest` can surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginLoadError {
    /// The TOML string failed to parse.
    TomlInvalid(String),
    /// Manifest parsed but contributes nothing.
    EmptyContribution,
    /// Manifest is missing or has empty required fields after parse.
    MissingField(&'static str),
}

impl std::fmt::Display for PluginLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TomlInvalid(m) => write!(f, "plugin manifest: toml invalid: {m}"),
            Self::EmptyContribution => {
                write!(f, "plugin manifest: contributes nothing")
            }
            Self::MissingField(name) => {
                write!(f, "plugin manifest: missing or empty field: {name}")
            }
        }
    }
}

impl std::error::Error for PluginLoadError {}

/// Parse a `plugin.toml` source string. Performs structural
/// validation:
///
/// - `id` / `version` / `display_name` non-empty
/// - `runtime.min_uclaw_version` non-empty
/// - `author.name` non-empty
/// - At least one contribution present
pub fn load_plugin_manifest(toml_src: &str) -> Result<PluginManifest, PluginLoadError> {
    let manifest: PluginManifest = toml::from_str(toml_src)
        .map_err(|e| PluginLoadError::TomlInvalid(e.to_string()))?;
    if manifest.id.trim().is_empty() {
        return Err(PluginLoadError::MissingField("id"));
    }
    if manifest.version.trim().is_empty() {
        return Err(PluginLoadError::MissingField("version"));
    }
    if manifest.display_name.trim().is_empty() {
        return Err(PluginLoadError::MissingField("display_name"));
    }
    if manifest.author.name.trim().is_empty() {
        return Err(PluginLoadError::MissingField("author.name"));
    }
    if manifest.runtime.min_uclaw_version.trim().is_empty() {
        return Err(PluginLoadError::MissingField("runtime.min_uclaw_version"));
    }
    if !manifest.contributes_anything() {
        return Err(PluginLoadError::EmptyContribution);
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_toml() -> &'static str {
        r#"
id = "com.example.slacker"
version = "1.2.3"
display_name = "Slacker"
description = "Slack connector"

[author]
name = "Acme"
email = "ops@acme.com"

[runtime]
min_uclaw_version = "0.4.0"

[permissions]
network = true
memory_read = true
additional = ["clipboard:read"]

[contributes]
mcp_servers = ["slack-mcp"]
skills = ["search-slack"]
"#
    }

    // ── happy path ────────────────────────────────────────────────

    #[test]
    fn loads_full_manifest() {
        let m = load_plugin_manifest(good_toml()).unwrap();
        assert_eq!(m.id, "com.example.slacker");
        assert_eq!(m.version, "1.2.3");
        assert_eq!(m.display_name, "Slacker");
        assert_eq!(m.description.as_deref(), Some("Slack connector"));
        assert!(m.permissions.network);
        assert!(m.contributes_anything());
        assert_eq!(m.contributes.skills.len(), 1);
        assert_eq!(m.permissions.additional, vec!["clipboard:read"]);
    }

    // ── malformed TOML ────────────────────────────────────────────

    #[test]
    fn malformed_toml_returns_error() {
        let bad = "id = unquoted_value\n";
        let err = load_plugin_manifest(bad).unwrap_err();
        assert!(matches!(err, PluginLoadError::TomlInvalid(_)));
    }

    // ── missing fields ────────────────────────────────────────────

    #[test]
    fn missing_id_field_errors() {
        let toml = r#"
version = "0.1.0"
display_name = "X"

[author]
name = "A"

[runtime]
min_uclaw_version = "0.4.0"

[contributes]
skills = ["x"]
"#;
        let err = load_plugin_manifest(toml).unwrap_err();
        // Missing id → toml errors (because id is a required field
        // with no default in the struct).
        assert!(matches!(err, PluginLoadError::TomlInvalid(_)));
    }

    #[test]
    fn empty_id_errors() {
        let toml = r#"
id = ""
version = "1"
display_name = "X"

[author]
name = "A"

[runtime]
min_uclaw_version = "0.4.0"

[contributes]
skills = ["x"]
"#;
        let err = load_plugin_manifest(toml).unwrap_err();
        assert_eq!(err, PluginLoadError::MissingField("id"));
    }

    #[test]
    fn empty_author_name_errors() {
        let toml = r#"
id = "x"
version = "1"
display_name = "X"

[author]
name = ""

[runtime]
min_uclaw_version = "0.4.0"

[contributes]
skills = ["x"]
"#;
        let err = load_plugin_manifest(toml).unwrap_err();
        assert_eq!(err, PluginLoadError::MissingField("author.name"));
    }

    #[test]
    fn empty_runtime_min_version_errors() {
        let toml = r#"
id = "x"
version = "1"
display_name = "X"

[author]
name = "A"

[runtime]
min_uclaw_version = ""

[contributes]
skills = ["x"]
"#;
        let err = load_plugin_manifest(toml).unwrap_err();
        assert_eq!(
            err,
            PluginLoadError::MissingField("runtime.min_uclaw_version")
        );
    }

    // ── empty contribution ────────────────────────────────────────

    #[test]
    fn empty_contribution_errors() {
        let toml = r#"
id = "x"
version = "1"
display_name = "X"

[author]
name = "A"

[runtime]
min_uclaw_version = "0.4.0"
"#;
        let err = load_plugin_manifest(toml).unwrap_err();
        assert_eq!(err, PluginLoadError::EmptyContribution);
    }

    // ── error Display ─────────────────────────────────────────────

    #[test]
    fn plugin_load_error_display() {
        for (e, contains) in [
            (PluginLoadError::TomlInvalid("oops".into()), "toml invalid"),
            (PluginLoadError::EmptyContribution, "contributes nothing"),
            (PluginLoadError::MissingField("id"), "missing or empty field: id"),
        ] {
            assert!(e.to_string().contains(contains));
        }
    }
}
