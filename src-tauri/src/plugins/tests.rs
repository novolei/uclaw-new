//! Unit tests for the plugins module.

use super::*;

// HELPER: construct minimal valid plugin.toml content for a given id + tools.
// Matches the actual PluginManifest schema: id, version, display_name, author,
// runtime (min_uclaw_version), and contributes.
fn make_test_manifest_toml(id: &str, tools: &[&str]) -> String {
    let tools_array = tools
        .iter()
        .map(|t| format!("\"{}\"", t))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"
id = "{}"
version = "0.1.0"
display_name = "Test Plugin"

[author]
name = "test-author"

[runtime]
min_uclaw_version = "0.1.0"

[contributes]
tools = [{}]
"#,
        id, tools_array
    )
}

#[test]
fn discover_returns_empty_when_root_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("nonexistent-plugins");
    let d = PluginDiscovery::new(&missing);
    let results = d.discover().unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn discover_returns_empty_for_empty_root() {
    let tmp = tempfile::tempdir().unwrap();
    let plugins_root = tmp.path().join("plugins");
    std::fs::create_dir_all(&plugins_root).unwrap();
    let d = PluginDiscovery::new(&plugins_root);
    let results = d.discover().unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn discover_loads_valid_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let plugins_root = tmp.path().join("plugins");
    let echo_dir = plugins_root.join("echo");
    std::fs::create_dir_all(&echo_dir).unwrap();
    std::fs::write(
        echo_dir.join("plugin.toml"),
        make_test_manifest_toml("echo", &["echo"]),
    )
    .unwrap();
    let d = PluginDiscovery::new(&plugins_root);
    let results = d.discover().unwrap();
    assert_eq!(results.len(), 1);
    let loaded = results[0].as_ref().unwrap();
    assert_eq!(loaded.manifest.id, "echo");
    assert_eq!(loaded.manifest.version, "0.1.0");
    assert_eq!(loaded.manifest.display_name, "Test Plugin");
    assert_eq!(loaded.manifest.contributes.tools, vec!["echo".to_string()]);
}

#[test]
fn discover_skips_dir_without_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let plugins_root = tmp.path().join("plugins");
    let dir_a = plugins_root.join("with_manifest");
    let dir_b = plugins_root.join("without_manifest");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();
    std::fs::write(
        dir_a.join("plugin.toml"),
        make_test_manifest_toml("with_manifest", &[]),
    )
    .unwrap();
    let d = PluginDiscovery::new(&plugins_root);
    let results = d.discover().unwrap();
    assert_eq!(results.len(), 1, "only the dir with a manifest is loaded");
}

#[test]
fn registrar_records_contributions() {
    use crate::plugins::registration::PluginRegistrar;

    let tmp = tempfile::tempdir().unwrap();
    let plugins_root = tmp.path().join("plugins");
    let dir = plugins_root.join("test-plugin");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("plugin.toml"),
        r#"
id = "test-plugin"
version = "0.1.0"
display_name = "Test"

[author]
name = "test"

[runtime]
min_uclaw_version = "0.1.0"

[contributes]
tools = ["foo", "bar"]
commands = ["greet"]
skills = ["mathy"]
themes = ["dark"]
"#,
    )
    .unwrap();

    let d = PluginDiscovery::new(&plugins_root);
    let mut results = d.discover().unwrap();
    assert_eq!(results.len(), 1);
    let loaded = results.remove(0).unwrap();

    let mut api = crate::agent::api::AgentApi::new();
    let summary = PluginRegistrar::register(&mut api, &loaded).unwrap();

    assert_eq!(summary.plugin_id, "test-plugin");
    assert_eq!(summary.tools_registered, vec!["foo".to_string(), "bar".to_string()]);
    assert_eq!(summary.commands_registered, vec!["greet".to_string()]);
    assert_eq!(summary.skills_skipped, vec!["mathy".to_string()]);
    assert_eq!(summary.themes_skipped, vec!["dark".to_string()]);

    // Verify ToolDescriptors were registered (with the plugin_id:name prefix).
    assert!(api.tool("test-plugin:foo").is_some());
    assert!(api.tool("test-plugin:bar").is_some());
}

#[test]
fn manifest_id_mismatch_with_dir_name_is_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let plugins_root = tmp.path().join("plugins");
    let echo_dir = plugins_root.join("expected-id");
    std::fs::create_dir_all(&echo_dir).unwrap();
    std::fs::write(
        echo_dir.join("plugin.toml"),
        make_test_manifest_toml("actually-different-id", &[]),
    )
    .unwrap();
    let d = PluginDiscovery::new(&plugins_root);
    let results = d.discover().unwrap();
    assert_eq!(results.len(), 1);
    let err = results[0].as_ref().unwrap_err();
    assert!(
        matches!(err, DiscoveryError::ManifestInvalid { .. }),
        "expected ManifestInvalid, got {:?}",
        err
    );
}
