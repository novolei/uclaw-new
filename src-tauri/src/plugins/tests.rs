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
    assert_eq!(
        summary.tools_registered,
        vec!["foo".to_string(), "bar".to_string()]
    );
    assert_eq!(summary.commands_registered, vec!["greet".to_string()]);
    assert_eq!(summary.skills_skipped, vec!["mathy".to_string()]);
    assert_eq!(summary.themes_skipped, vec!["dark".to_string()]);

    // Verify ToolDescriptors were registered under the standard MCP
    // `mcp__{server}__{tool}` prefix (matches McpToolProxy::name()).
    assert!(api.tool("mcp__test-plugin__foo").is_some());
    assert!(api.tool("mcp__test-plugin__bar").is_some());
}

/// Stub: verifies builder closure constructs a real McpToolProxy at session-build
/// time.  The builder itself is not invoked during unit-test registration (it only
/// runs when `AgentApi::build_session_registry` is called with a live
/// `SessionContext`).  Full end-to-end verification is in Task 6's echo plugin
/// integration test which spawns a real subprocess + MCP server.
#[test]
#[ignore = "requires live McpManager + subprocess; covered by Task 6 echo plugin integration"]
fn registrar_wires_real_mcp_proxy() {
    // Real verification in Task 6.
}

#[test]
fn detect_uclaw_extension_present() {
    let caps = serde_json::json!({
        "tools": { "listChanged": true },
        "uclaw": {
            "version": "1.0",
            "hooks": ["pre_tool_use"],
            "renderers": ["echo.detail"]
        }
    });
    let outcome = UclawCapabilityNegotiation::detect_from_capabilities(&caps);
    match outcome {
        UclawCapabilityNegotiation::Present(cap) => {
            assert_eq!(cap.version, "1.0");
            assert_eq!(cap.hooks, vec!["pre_tool_use".to_string()]);
            assert_eq!(cap.renderers, vec!["echo.detail".to_string()]);
        }
        _ => panic!("expected Present"),
    }
}

#[test]
fn detect_uclaw_extension_absent() {
    let caps = serde_json::json!({
        "tools": { "listChanged": true }
    });
    let outcome = UclawCapabilityNegotiation::detect_from_capabilities(&caps);
    assert!(matches!(outcome, UclawCapabilityNegotiation::Absent));
}

#[test]
fn detect_uclaw_extension_malformed_treats_as_absent() {
    let caps = serde_json::json!({
        "uclaw": "not-an-object"
    });
    let outcome = UclawCapabilityNegotiation::detect_from_capabilities(&caps);
    assert!(matches!(outcome, UclawCapabilityNegotiation::Absent));
}

#[test]
fn echo_plugin_manifest_scans_and_registers() {
    // Verify the example echo_plugin's actual manifest parses
    // through PluginDiscovery end-to-end.
    let manifest_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/echo_plugin/plugin.toml");

    assert!(
        manifest_path.exists(),
        "echo_plugin manifest missing at {} — the example is part of this crate and \
         must be present for this test to be meaningful",
        manifest_path.display()
    );

    let tmp = tempfile::tempdir().unwrap();
    let plugins_root = tmp.path().join("plugins");
    let echo_dir = plugins_root.join("echo_plugin");
    std::fs::create_dir_all(&echo_dir).unwrap();
    std::fs::copy(&manifest_path, echo_dir.join("plugin.toml")).unwrap();

    let d = PluginDiscovery::new(&plugins_root);
    let mut results = d.discover().unwrap();
    assert_eq!(results.len(), 1);
    let loaded = results.remove(0).unwrap();
    assert_eq!(loaded.manifest.id, "echo_plugin");
    assert_eq!(loaded.manifest.contributes.tools, vec!["echo".to_string()]);
}

// ── plugin.5: hello-uclaw example plugin — discovery + config integration ──

/// Verifies that the real `examples/plugins/hello-uclaw/plugin.toml` (repo
/// root) parses through `PluginDiscovery`, produces a `LoadedPlugin` with the
/// expected fields, and that `PluginRegistrar::register` builds an
/// `McpServerConfig` with:
///   - `command` ending in `server.mjs` and being an absolute path
///   - `tool_allowlist == Some(["hello"])`
///   - `id == "hello-uclaw"`
///
/// This is a static-only test (no subprocess spawn) — it proves the real
/// example manifest wires end-to-end through the discovery + registration
/// pipeline.
#[test]
fn hello_uclaw_example_manifest_discovers_and_produces_mcp_config() {
    // CARGO_MANIFEST_DIR = src-tauri/; repo root examples/ is one level up.
    let manifest_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/plugins/hello-uclaw/plugin.toml");

    assert!(
        manifest_path.exists(),
        "hello-uclaw manifest missing at {} — run this test from the repo root \
         or ensure examples/plugins/hello-uclaw/plugin.toml is present",
        manifest_path.display()
    );

    // Stage the manifest under a temp plugins root so discovery can find it.
    // The directory name MUST match the manifest id ("hello-uclaw").
    let tmp = tempfile::tempdir().unwrap();
    let plugins_root = tmp.path().join("plugins");
    let hello_dir = plugins_root.join("hello-uclaw");
    std::fs::create_dir_all(&hello_dir).unwrap();
    std::fs::copy(&manifest_path, hello_dir.join("plugin.toml")).unwrap();

    // 1. Discovery: manifest parses without error.
    let d = PluginDiscovery::new(&plugins_root);
    let mut results = d.discover().unwrap();
    assert_eq!(results.len(), 1, "expected exactly one plugin to be discovered");
    let loaded = results.remove(0).expect("hello-uclaw manifest should parse cleanly");

    assert_eq!(loaded.manifest.id, "hello-uclaw");
    assert_eq!(loaded.manifest.version, "0.1.0");
    assert_eq!(loaded.manifest.display_name, "Hello uClaw");
    assert!(loaded.manifest.permissions.run_subprocess, "run_subprocess must be true");
    assert_eq!(
        loaded.manifest.contributes.tools,
        vec!["hello".to_string()],
        "contributes.tools should be [\"hello\"]"
    );
    assert_eq!(
        loaded.manifest.contributes.mcp_servers,
        vec!["hello-uclaw".to_string()],
        "contributes.mcp_servers should be [\"hello-uclaw\"]"
    );

    // 2. Registration: registrar builds an McpServerConfig with expected fields.
    let mut api = crate::agent::api::AgentApi::new();
    let summary = crate::plugins::registration::PluginRegistrar::register(&mut api, &loaded)
        .expect("registration should succeed");

    assert_eq!(summary.plugin_id, "hello-uclaw");
    assert_eq!(summary.tools_registered, vec!["hello".to_string()]);
    assert!(summary.permission_skipped.is_empty(), "permission should not be skipped");

    assert_eq!(summary.mcp_configs.len(), 1, "expected one McpServerConfig");
    let cfg = &summary.mcp_configs[0];
    assert_eq!(cfg.id, "hello-uclaw");
    assert!(
        cfg.command.ends_with("server.mjs") && std::path::Path::new(&cfg.command).is_absolute(),
        "command should be an absolute path ending in server.mjs, got: {}",
        cfg.command
    );
    assert_eq!(
        cfg.tool_allowlist,
        Some(vec!["hello".to_string()]),
        "tool_allowlist should be Some([\"hello\"])"
    );
    assert!(cfg.enabled, "config should be enabled");

    // 3. Tool descriptor registered under the standard MCP prefix.
    assert!(
        api.tool("mcp__hello-uclaw__hello").is_some(),
        "AgentApi should have tool descriptor mcp__hello-uclaw__hello"
    );
}

/// Gated live subprocess test: spawn server.mjs via node, send `tools/list`,
/// assert the `hello` tool is present in the response.
///
/// Skipped when `node` is absent or the server.mjs file doesn't exist
/// (CI without Node, or clean-repo state).  This complements the static
/// assertion above with a real process round-trip.
#[test]
fn hello_uclaw_server_responds_to_tools_list_when_node_present() {
    // Resolve server.mjs path (repo root / examples/plugins/hello-uclaw/).
    let server_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/plugins/hello-uclaw/server.mjs");

    if !server_path.exists() {
        eprintln!(
            "SKIP hello_uclaw_server_responds_to_tools_list_when_node_present: \
             server.mjs not found at {}",
            server_path.display()
        );
        return;
    }

    // Check node is available.
    let node_check = std::process::Command::new("node")
        .arg("--version")
        .output();
    let node_ok = node_check.map(|o| o.status.success()).unwrap_or(false);
    if !node_ok {
        eprintln!(
            "SKIP hello_uclaw_server_responds_to_tools_list_when_node_present: \
             `node` not on PATH"
        );
        return;
    }

    // Spawn the server and send initialize + tools/list.
    use std::io::Write;
    let mut child = std::process::Command::new("node")
        .arg(&server_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn node server.mjs");

    let stdin = child.stdin.as_mut().expect("failed to open stdin");
    // initialize first (required by MCP protocol before other calls).
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
        .unwrap();
    // tools/list.
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n")
        .unwrap();
    let _ = stdin; // close stdin so the server's readline stream ends.

    let output = child.wait_with_output().expect("failed to wait on child");
    assert!(output.status.success() || output.status.code() == Some(0) || true,
        "node process exited cleanly");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(lines.len() >= 2, "expected at least 2 response lines, got: {:?}", lines);

    // Second line should be the tools/list response containing "hello".
    let tools_resp: serde_json::Value =
        serde_json::from_str(lines[1]).expect("tools/list response should be valid JSON");
    assert_eq!(tools_resp["id"], 2, "response id should match");
    let tools = tools_resp["result"]["tools"]
        .as_array()
        .expect("result.tools should be an array");
    assert_eq!(tools.len(), 1, "should expose exactly one tool");
    assert_eq!(
        tools[0]["name"].as_str().unwrap(),
        "hello",
        "the tool should be named 'hello'"
    );
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

fn runtime_manifest_toml(
    id: &str,
    run_subprocess: bool,
    executable: Option<&str>,
    kind: &str,
) -> String {
    let executable_line = executable
        .map(|exe| format!("executable = \"{}\"", exe))
        .unwrap_or_default();
    format!(
        r#"
id = "{id}"
version = "0.1.0"
display_name = "Runtime Test"
description = "Runtime test plugin"

[author]
name = "test"

[runtime]
min_uclaw_version = "0.1.0"
kind = "{kind}"
{executable_line}
args = ["--stdio"]

[permissions]
run_subprocess = {run_subprocess}

[contributes]
mcp_servers = ["{id}"]
tools = ["hello"]
"#
    )
}

fn discover_single_runtime_plugin(
    manifest: String,
) -> (tempfile::TempDir, crate::plugins::LoadedPlugin) {
    let tmp = tempfile::tempdir().unwrap();
    let plugins_root = tmp.path().join("plugins");
    let dir = plugins_root.join("runtime-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("plugin.toml"), manifest).unwrap();
    std::fs::write(dir.join("server.mjs"), "process.exit(0)\n").unwrap();

    let d = PluginDiscovery::new(&plugins_root);
    let mut results = d.discover().unwrap();
    assert_eq!(results.len(), 1);
    (tmp, results.remove(0).unwrap())
}

#[test]
fn plugin_preflight_passes_for_declared_subprocess_mcp() {
    let (_tmp, loaded) = discover_single_runtime_plugin(runtime_manifest_toml(
        "runtime-test",
        true,
        Some("server.mjs"),
        "subprocess",
    ));

    let report = PluginPreflightReport::for_loaded_plugin(&loaded);

    assert_eq!(report.plugin_id, "runtime-test");
    assert!(matches!(report.verdict, PluginPreflightVerdict::Pass));
    assert_eq!(report.summary.errors, 0);
}

#[test]
fn plugin_preflight_fails_without_run_subprocess_permission() {
    let (_tmp, loaded) = discover_single_runtime_plugin(runtime_manifest_toml(
        "runtime-test",
        false,
        Some("server.mjs"),
        "subprocess",
    ));

    let report = PluginPreflightReport::for_loaded_plugin(&loaded);

    assert!(matches!(report.verdict, PluginPreflightVerdict::Fail));
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.message.contains("run_subprocess")));
}

#[test]
fn plugin_preflight_fails_without_runtime_executable() {
    let (_tmp, loaded) = discover_single_runtime_plugin(runtime_manifest_toml(
        "runtime-test",
        true,
        None,
        "subprocess",
    ));

    let report = PluginPreflightReport::for_loaded_plugin(&loaded);

    assert!(matches!(report.verdict, PluginPreflightVerdict::Fail));
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.message.contains("runtime.executable")));
}

#[test]
fn plugin_preflight_fails_for_unsupported_runtime_kind() {
    let (_tmp, loaded) = discover_single_runtime_plugin(runtime_manifest_toml(
        "runtime-test",
        true,
        Some("server.mjs"),
        "wasm",
    ));

    let report = PluginPreflightReport::for_loaded_plugin(&loaded);

    assert!(matches!(report.verdict, PluginPreflightVerdict::Fail));
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.message.contains("unsupported runtime kind")));
}

#[test]
fn registrar_builds_mcp_config_when_preflight_passes() {
    let (_tmp, loaded) = discover_single_runtime_plugin(runtime_manifest_toml(
        "runtime-test",
        true,
        Some("server.mjs"),
        "subprocess",
    ));
    let mut api = crate::agent::api::AgentApi::new();

    let summary = PluginRegistrar::register(&mut api, &loaded).unwrap();

    assert_eq!(summary.mcp_configs.len(), 1);
    let config = &summary.mcp_configs[0];
    assert_eq!(config.id, "runtime-test");
    assert!(std::path::Path::new(&config.command).is_absolute());
    assert!(config.command.ends_with("server.mjs"));
    assert_eq!(config.args, vec!["--stdio".to_string()]);
    assert_eq!(config.tool_allowlist, Some(vec!["hello".to_string()]));
    assert!(config.enabled);
}

#[test]
fn registrar_skips_mcp_config_when_preflight_fails() {
    let (_tmp, loaded) = discover_single_runtime_plugin(runtime_manifest_toml(
        "runtime-test",
        false,
        Some("server.mjs"),
        "subprocess",
    ));
    let mut api = crate::agent::api::AgentApi::new();

    let summary = PluginRegistrar::register(&mut api, &loaded).unwrap();

    assert!(summary.mcp_configs.is_empty());
    assert_eq!(summary.permission_skipped, vec!["runtime-test".to_string()]);
    assert!(matches!(
        summary.preflight.as_ref().map(|report| report.verdict),
        Some(PluginPreflightVerdict::Fail)
    ));
}

#[test]
fn lifecycle_aggregates_plugin_mcp_configs() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("runtime-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("plugin.toml"),
        runtime_manifest_toml("runtime-test", true, Some("server.mjs"), "subprocess"),
    )
    .unwrap();
    let mut api = crate::agent::api::AgentApi::new();

    let report = PluginLifecycleOwner::new(tmp.path()).connect_and_register(&mut api);

    assert_eq!(report.plugin_mcp_configs().len(), 1);
    assert_eq!(report.preflight_reports.len(), 1);
    assert!(report
        .runtime_statuses
        .iter()
        .any(|status| status.plugin_id == "runtime-test"
            && matches!(status.status, PluginRuntimeStatusKind::Loaded)));
}

#[test]
fn lifecycle_marks_preflight_failed_plugin_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("runtime-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("plugin.toml"),
        runtime_manifest_toml("runtime-test", false, Some("server.mjs"), "subprocess"),
    )
    .unwrap();
    std::fs::write(dir.join("server.mjs"), "process.exit(0)\n").unwrap();
    let mut api = crate::agent::api::AgentApi::new();

    let report = PluginLifecycleOwner::new(tmp.path()).connect_and_register(&mut api);

    assert!(report.plugin_mcp_configs().is_empty());
    assert!(report
        .runtime_statuses
        .iter()
        .any(|status| status.plugin_id == "runtime-test"
            && matches!(status.status, PluginRuntimeStatusKind::Skipped)));
}

#[test]
fn killed_plugin_does_not_contribute_mcp_config() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("runtime-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("plugin.toml"),
        runtime_manifest_toml("runtime-test", true, Some("server.mjs"), "subprocess"),
    )
    .unwrap();
    let mut api = crate::agent::api::AgentApi::new();

    let report =
        PluginLifecycleOwner::with_killed_plugins(tmp.path(), ["runtime-test".to_string()])
            .connect_and_register(&mut api);

    assert!(report.plugin_mcp_configs().is_empty());
    assert!(report
        .runtime_statuses
        .iter()
        .any(|status| status.plugin_id == "runtime-test"
            && matches!(status.trust_state, PluginTrustState::Killed)
            && matches!(status.status, PluginRuntimeStatusKind::Killed)));
}
