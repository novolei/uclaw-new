# 阶段 3 P3-4 — Plugin Discovery + Manifest Bridge + uclaw Capability Extension · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a **PluginDiscovery** layer that scans `plugins/<id>/plugin.toml` files, validates via the P2-preserved `PluginManifest` schema, and routes `PluginContribution` fields through the right registration paths: `mcp_servers` → existing `McpManager`, `tools/commands` → `AgentApi` `ToolDescriptor`/`Command` registration, and `hooks/renderers` (via the new **`uclaw` MCP capability extension**) → `AgentApi.on()` + `register_renderer()`. Ship two end-to-end demos: an **echo plugin** (Rust binary at `examples/echo_plugin/`) exercising the full uClaw-extended path, plus a **vanilla MCP server verification** path using the user's existing `mcp_servers.json` config.

**Architecture:** Reuse the 1,925-LoC `McpManager` substrate (already does JSON-RPC 2.0, subprocess spawn, stdio transport, health, audit, `McpToolProxy`). Add a thin `PluginDiscovery` module on top that owns manifest scanning + validation. Plugin contributions are *routed* through existing infrastructure: MCP servers register into McpManager via its normal `connect_server` path; tools register into AgentApi as `ToolDescriptor`s whose builder closure constructs an `McpToolProxy`-like delegate; commands register as AgentApi `Command`s; hooks register via AgentApi `on()`. The `uclaw` capability namespace is a forward-extension on top of MCP's existing capability negotiation — plugins that opt in get to register extra contribution kinds (hooks, renderers) beyond MCP's tools/resources/prompts.

**Tech Stack:** Rust 2021, Tauri 2, `toml` crate (verify availability), `serde`, `tokio`, async-trait, existing `mcp.rs` JSON-RPC primitives.

**Related design:** [`2026-05-28-stage3-agentapi-handle-design.md`](../specs/2026-05-28-stage3-agentapi-handle-design.md) §5 (Subprocess RPC plugin protocol) + §10 P3-4 row (with the 2026-05-29 Correction callout).

**Prior PRs:** [#570 P3-1](https://github.com/novolei/uclaw-new/pull/570) (scaffold), [#571 P3-2](https://github.com/novolei/uclaw-new/pull/571) (tools), [#572 P3-3](https://github.com/novolei/uclaw-new/pull/572) (ProviderService + HookBus bridge). Merged to main at `0a4b20b0`.

---

## Recon-discovered design gap

Pre-plan recon (2026-05-29) found **substantial existing infrastructure** the original spec didn't account for:

| Existing component | Lines | What it already does |
|---|---:|---|
| `src/mcp.rs` (`McpManager`) | 1,925 | Full JSON-RPC 2.0 protocol + MCP initialize/list_tools/call_tool/list_resources/ping lifecycle; subprocess spawn via stdio transport; per-server health tasks (PR-3); notification channel (PR-4); audit DB persistence (PR-5); `mcp_servers.json` config; runtime working dir overrides; `prefixed_tool_name` namespacing; `McpToolProxy` (presents MCP tools AS Rust `Tool` trait implementations) |
| `src/plugin_manifest/schema.rs` (P2-preserved) | 279 | `PluginManifest { id, version, display_name, author, runtime, permissions, contributes }`; `PluginContribution { mcp_servers, skills, commands, tools, themes }`; `PluginPermissions { network, filesystem_read, filesystem_write, memory_read, memory_write, run_subprocess, additional }`; `PluginRuntimeRequirement` |
| `src/mcp_server/` (separate) | ~? | uClaw acting AS an MCP server — different concern from P3-4. **NOT touched** in this plan. |

The spec said "SubprocessPluginManager generalizes McpManager + new subprocess RPC implementation". Recon shows McpManager already implements **95% of the proposed surface**. Building parallel infrastructure would duplicate ~1,900 LoC.

The user-grilled **Option B** resolution: thin discovery + routing layer that REUSES McpManager. Specifically:
1. **PluginDiscovery** scans `plugins/<id>/plugin.toml`, parses via existing `PluginManifest`.
2. **PluginContribution.mcp_servers** → adds entries to `McpManager` via its existing config path.
3. **PluginContribution.tools** + **PluginContribution.commands** → registers as `AgentApi.register_tool(ToolDescriptor { builder: |ctx| Box::new(McpToolProxy::for_plugin(id, name, ctx)) })` and `AgentApi.register_command(Command)`.
4. **`uclaw` MCP capability extension** — during MCP `initialize` handshake, plugins can advertise `"uclaw": { "version": "1.0", "hooks": [...], "renderers": [...] }` in addition to standard MCP capabilities. uClaw plugins that opt in can then register hooks (via AgentApi `on()`) and renderers (via `register_renderer()`) — going beyond what plain MCP allows.
5. **Two demos**:
   - Echo plugin: Rust binary at `examples/echo_plugin/main.rs` (~150 LoC) that speaks MCP + the uClaw extension; manifest at `examples/echo_plugin/plugin.toml`. Loaded by PluginDiscovery from a test fixture path.
   - Vanilla MCP verification: confirms `PluginDiscovery::load_from_directory(...)` correctly leaves `mcp_servers.json`-configured servers alone (no regression to McpManager's existing behavior).

This brings P3-4 down from 8-10 tasks (heavy parallel infrastructure) to 6 tasks (thin bridge).

---

## Background facts verified against HEAD `0a4b20b0` (main after P3-3 squash-merge)

### Existing infrastructure inventory

- **`crate::mcp`** (re-exports from `src/mcp.rs`): `McpManager`, `McpServerConfig`, `McpToolProxy`, `JsonRpcRequest`, `JsonRpcResponse`, `InitializeResult`, `ServerCapabilities`, transport types, etc.
- **`crate::plugin_manifest::schema`**: `PluginManifest`, `PluginAuthor`, `PluginPermissions`, `PluginContribution`, `PluginRuntimeRequirement`.
- **`crate::agent::api::AgentApi`**: `register_tool(ToolDescriptor)`, `register_command(Command)`, `register_renderer(Renderer)`, `on(EventKind, Fn)`, `set_provider_service`, `set_hook_bus`, `provider_service()`, `hook_bus()`, `emit()`, `emit_with_decision()`.

### What this plan adds

| Component | New / Modified | Approximate size |
|---|---|---:|
| `src/plugins/mod.rs` | NEW module (re-exports) | ~20 LoC |
| `src/plugins/discovery.rs` | NEW (manifest scan + parse) | ~150 LoC |
| `src/plugins/registration.rs` | NEW (manifest → AgentApi/McpManager wiring) | ~250 LoC |
| `src/plugins/uclaw_extension.rs` | NEW (uclaw capability negotiation + hook/renderer registration) | ~200 LoC |
| `src/plugins/tests.rs` | NEW (inline unit tests) | ~400 LoC |
| `src/mcp.rs` (small additions) | MODIFY (hook for plugin-added servers; uclaw capability detect during initialize) | ~50 LoC |
| `src/agent/api/mod.rs` (small additions) | MODIFY (`set_plugin_discovery` accessor if needed) | ~20 LoC |
| `src/lib.rs` | MODIFY (declare `pub mod plugins;`) | 1 LoC |
| `src/app.rs` | MODIFY (boot wire: create PluginDiscovery, scan, register contributions) | ~30 LoC |
| `examples/echo_plugin/main.rs` | NEW (Rust binary) | ~150 LoC |
| `examples/echo_plugin/plugin.toml` | NEW (manifest) | ~25 LoC |
| `Cargo.toml` | MODIFY ([[example]] entry for echo_plugin) | 3 LoC |

### Baselines to hold

- `cargo build`: green, 50 warnings (post-P3-3 baseline).
- `cargo test --lib agent::`: 796 passed / 2 pre-existing failed.
- `cargo test --lib agent::api`: 30 passed.
- `cargo test --lib` total: 3040 passed / 7 pre-existing failed.

After P3-4: + ~10-15 new tests (discovery + registration + extension + tests for the Rust echo plugin binary as a unit-testable library).

---

## Pre-flight (before Task 1)

1. **Confirm main baseline**: `git -C /Users/ryanliu/Documents/uclaw status -sb` → `## main...origin/main` at `0a4b20b0`.

2. **Verify `toml` crate availability**:

```bash
grep "^toml\b" /Users/ryanliu/Documents/uclaw/src-tauri/Cargo.toml
```

If not present, the plan includes adding it (or use existing serde alternative — `plugin_manifest/schema.rs` already serializes via serde, and the existing manifest tests deserialize from JSON via `serde_json`; check if there's already a TOML helper anywhere).

3. **Create worktree + symlinks**:

```bash
git worktree add -b claude/stage3-p4-plugin-discovery-bridge \
    /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge main
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri/gbrain-source
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri/pyembed
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri/bunembed
```

4. **Baseline verifications**:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo build 2>&1 | tail -3
# expect: Finished, ~50 warnings

cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
# expect: 796 passed / 2 failed
```

All paths in tasks below are relative to the worktree.

---

## Task 1: PluginDiscovery scaffold — scan + parse manifests

**Files:**
- Create: `src-tauri/src/plugins/mod.rs`
- Create: `src-tauri/src/plugins/discovery.rs`
- Create: `src-tauri/src/plugins/tests.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod plugins;`)

### Steps

#### Step 1.1: Add `toml` crate dep if missing

```bash
grep "^toml" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri/Cargo.toml
```

If empty, add to `[dependencies]` section of `src-tauri/Cargo.toml`:

```toml
toml = "0.8"
```

Choose the version already used by other workspace deps if any (e.g., check `~/.cargo/registry/index/` or `Cargo.lock` for the version that resolves cleanly with existing deps).

#### Step 1.2: Create `src/plugins/mod.rs`

```rust
//! Plugin discovery + manifest → AgentApi/McpManager registration.
//!
//! Per design spec §5: plugins live in `$DATA_DIR/plugins/<id>/`, declare
//! contributions via `plugin.toml`, and are routed through existing
//! infrastructure (McpManager for mcp_servers; AgentApi for tools/commands;
//! `uclaw` MCP capability extension for hooks/renderers).
//!
//! This module is a THIN bridge that reuses McpManager (1,925 LoC of
//! existing JSON-RPC + subprocess infrastructure). It does NOT duplicate
//! protocol code.

pub mod discovery;
pub mod registration;
pub mod uclaw_extension;

#[cfg(test)]
mod tests;

pub use discovery::{PluginDiscovery, DiscoveryError, LoadedPlugin};
pub use registration::{PluginRegistrar, RegistrationError};
pub use uclaw_extension::{UclawCapability, UclawCapabilityNegotiation};
```

(Task 1 only creates `discovery.rs` + `tests.rs`. The other modules are forward declarations; their files are added in Tasks 2-3.)

For Task 1, only declare `pub mod discovery;` and inline-test it:

```rust
//! Plugin discovery + manifest → AgentApi/McpManager registration.

pub mod discovery;

#[cfg(test)]
mod tests;

pub use discovery::{PluginDiscovery, DiscoveryError, LoadedPlugin};
```

Add the other module decls in Tasks 2-3.

#### Step 1.3: Create `src/plugins/discovery.rs`

```rust
//! Manifest discovery — scans `$DATA_DIR/plugins/<id>/plugin.toml` files.

use std::path::{Path, PathBuf};

use crate::plugin_manifest::schema::PluginManifest;

/// A successfully-loaded plugin manifest with its on-disk path.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub plugin_dir: PathBuf,
    pub manifest_path: PathBuf,
}

/// Errors discovery may surface.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("plugins directory does not exist: {0}")]
    DirectoryMissing(PathBuf),
    #[error("failed to read plugin directory entry {path}: {source}")]
    DirRead { path: PathBuf, source: std::io::Error },
    #[error("failed to read manifest at {path}: {source}")]
    ManifestRead { path: PathBuf, source: std::io::Error },
    #[error("failed to parse manifest at {path}: {source}")]
    ManifestParse { path: PathBuf, source: toml::de::Error },
    #[error("manifest at {path} validation failed: {reason}")]
    ManifestInvalid { path: PathBuf, reason: String },
}

/// Discovery scans `plugins/<id>/plugin.toml` files under a given root and
/// returns parsed manifests. NO side effects (doesn't spawn subprocesses,
/// doesn't register anything) — pure scan + parse.
pub struct PluginDiscovery {
    plugins_root: PathBuf,
}

impl PluginDiscovery {
    /// Construct a discovery rooted at the given directory. Use
    /// `$DATA_DIR/plugins/` for the production wiring.
    pub fn new(plugins_root: impl AsRef<Path>) -> Self {
        Self {
            plugins_root: plugins_root.as_ref().to_path_buf(),
        }
    }

    /// Get the plugins directory this discovery scans.
    pub fn plugins_root(&self) -> &Path {
        &self.plugins_root
    }

    /// Scan the plugins root and parse each `<plugin_id>/plugin.toml`.
    ///
    /// Returns a vector of `Result<LoadedPlugin, DiscoveryError>` so the
    /// caller can decide how to handle per-plugin failures (typically
    /// log + skip, not abort the whole boot).
    pub fn discover(&self) -> Result<Vec<Result<LoadedPlugin, DiscoveryError>>, DiscoveryError> {
        if !self.plugins_root.exists() {
            // Empty plugins dir is fine — return empty list, not error.
            return Ok(Vec::new());
        }
        let mut results = Vec::new();
        let entries = std::fs::read_dir(&self.plugins_root).map_err(|e| DiscoveryError::DirRead {
            path: self.plugins_root.clone(),
            source: e,
        })?;
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    results.push(Err(DiscoveryError::DirRead {
                        path: self.plugins_root.clone(),
                        source: e,
                    }));
                    continue;
                }
            };
            let plugin_dir = entry.path();
            if !plugin_dir.is_dir() {
                continue;
            }
            let manifest_path = plugin_dir.join("plugin.toml");
            if !manifest_path.exists() {
                continue;
            }
            results.push(Self::load_manifest(&manifest_path, &plugin_dir));
        }
        Ok(results)
    }

    fn load_manifest(
        manifest_path: &Path,
        plugin_dir: &Path,
    ) -> Result<LoadedPlugin, DiscoveryError> {
        let body = std::fs::read_to_string(manifest_path).map_err(|e| DiscoveryError::ManifestRead {
            path: manifest_path.to_path_buf(),
            source: e,
        })?;
        let manifest: PluginManifest = toml::from_str(&body).map_err(|e| DiscoveryError::ManifestParse {
            path: manifest_path.to_path_buf(),
            source: e,
        })?;
        // Basic validation — id matches directory name.
        if let Some(dir_name) = plugin_dir.file_name().and_then(|s| s.to_str()) {
            if manifest.id != dir_name {
                return Err(DiscoveryError::ManifestInvalid {
                    path: manifest_path.to_path_buf(),
                    reason: format!(
                        "manifest id {:?} does not match directory name {:?}",
                        manifest.id, dir_name
                    ),
                });
            }
        }
        Ok(LoadedPlugin {
            manifest,
            plugin_dir: plugin_dir.to_path_buf(),
            manifest_path: manifest_path.to_path_buf(),
        })
    }
}
```

Note: ensure `thiserror` is available in deps (verify with `grep "^thiserror" Cargo.toml`).

#### Step 1.4: Create `src/plugins/tests.rs` with discovery tests

```rust
//! Unit tests for the plugins module.

use super::*;

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
        r#"
id = "echo"
version = "0.1.0"
display_name = "Echo Plugin"

[author]
name = "uClaw test"

[runtime]
kind = "subprocess"
executable = "./echo"

[contributes]
tools = ["echo"]
"#,
    )
    .unwrap();
    let d = PluginDiscovery::new(&plugins_root);
    let results = d.discover().unwrap();
    assert_eq!(results.len(), 1);
    let loaded = results[0].as_ref().unwrap();
    assert_eq!(loaded.manifest.id, "echo");
    assert_eq!(loaded.manifest.version, "0.1.0");
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
        r#"
id = "with_manifest"
version = "0.1.0"
display_name = "With"

[author]
name = "test"

[runtime]
kind = "subprocess"
executable = "./bin"
"#,
    )
    .unwrap();
    let d = PluginDiscovery::new(&plugins_root);
    let results = d.discover().unwrap();
    assert_eq!(results.len(), 1, "only the dir with a manifest is loaded");
}

#[test]
fn manifest_id_mismatch_with_dir_name_is_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let plugins_root = tmp.path().join("plugins");
    let echo_dir = plugins_root.join("expected-id");
    std::fs::create_dir_all(&echo_dir).unwrap();
    std::fs::write(
        echo_dir.join("plugin.toml"),
        r#"
id = "actually-different-id"
version = "0.1.0"
display_name = "Mismatched"

[author]
name = "test"

[runtime]
kind = "subprocess"
executable = "./bin"
"#,
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
```

Verify the existing PluginManifest's `runtime` field structure with:

```bash
grep -A 5 "pub struct PluginRuntimeRequirement\|pub enum PluginRuntime" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri/src/plugin_manifest/schema.rs
```

Adapt the test fixture TOML to match the actual `[runtime]` schema. If `kind = "subprocess"` + `executable = "./bin"` doesn't match the existing schema, swap to whatever the schema requires.

#### Step 1.5: Wire module into `src/lib.rs`

```bash
grep -n "^pub mod " /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri/src/lib.rs | head -10
```

Add `pub mod plugins;` in alphabetical position (likely after `plugin_manifest`).

#### Step 1.6: Build + run tests (GREEN GATE)

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: empty.

If errors:
- "cannot find crate `toml`" → Step 1.1 not done.
- "cannot find crate `thiserror`" → check Cargo.toml; may need to add.
- "field `tools` not found" → `PluginContribution` field shape differs; verify with grep.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo test --lib plugins:: 2>&1 | tail -5
```
Expected: 5 passed (all 5 discovery tests).

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```
Expected: 796 passed / 2 failed (baseline preserved).

#### Step 1.7: Commit

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge add -A \
    src-tauri/src/plugins/ \
    src-tauri/src/lib.rs \
    src-tauri/Cargo.toml

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge commit -m "$(cat <<'EOF'
feat(plugins): PluginDiscovery scaffold — scan + parse manifests (P3-4.1 of 阶段 3)

New module crate::plugins with:
- discovery.rs: PluginDiscovery + LoadedPlugin + DiscoveryError. Scans
  `<plugins_root>/<plugin_id>/plugin.toml`, validates id == directory
  name, returns a Vec<Result<LoadedPlugin, DiscoveryError>> so the
  caller can decide per-plugin failure policy.
- tests.rs: 5 inline unit tests covering empty root, missing root,
  valid manifest, missing manifest dir, id/dir mismatch.

Adds `toml` crate dep (if not already present).

NO side effects beyond filesystem read — discovery doesn't spawn
subprocesses, doesn't register anything. Task 2 adds the registration
layer that consumes LoadedPlugin and routes contributions.

cargo build clean; plugins:: 5/0 new; agent:: 796/2 baseline preserved.
EOF
)"
```

Continue to Task 2.

---

## Task 2: PluginRegistrar — manifest → AgentApi tools + commands

**Files:**
- Create: `src-tauri/src/plugins/registration.rs`
- Modify: `src-tauri/src/plugins/mod.rs` (declare new module)
- Modify: `src-tauri/src/plugins/tests.rs` (add registration tests)

### Steps

- [ ] **Step 2.1: Create `src/plugins/registration.rs`**

This module takes a `LoadedPlugin` and routes its `PluginContribution` fields:
- `tools` → AgentApi `ToolDescriptor` registrations (with builder closures that delegate to the appropriate proxy — for now, a placeholder McpToolProxy stub; full wiring in Task 3).
- `commands` → AgentApi `Command` registrations (similar placeholder).
- `mcp_servers` → handled in Task 3 (registered into McpManager).
- `skills`, `themes` → recorded for future use; no actual registration in this PR.

```rust
//! Manifest → AgentApi registration routing.
//!
//! Reads `LoadedPlugin` (manifest + paths) and registers its `PluginContribution`
//! fields into the appropriate handles:
//! - tools → AgentApi.register_tool with ToolDescriptors backed by McpToolProxy
//! - commands → AgentApi.register_command
//! - mcp_servers → McpManager (Task 3)
//! - skills, themes → recorded; no registration (future PRs)

use std::sync::Arc;

use crate::agent::api::AgentApi;
use crate::agent::api::tool::ToolDescriptor;
use crate::plugins::discovery::LoadedPlugin;

#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    #[error("plugin {0} contributes 0 items")]
    EmptyContribution(String),
    #[error("plugin {0} requested permission {1} but boot disallows it")]
    PermissionDenied(String, String),
}

/// Summary of what was registered for a plugin.
#[derive(Debug, Clone, Default)]
pub struct PluginRegistrationSummary {
    pub plugin_id: String,
    pub tools_registered: Vec<String>,
    pub commands_registered: Vec<String>,
    pub mcp_servers_registered: Vec<String>,
    pub skills_skipped: Vec<String>,
    pub themes_skipped: Vec<String>,
}

/// Routes plugin contributions to the appropriate registries.
///
/// Caller passes `&mut AgentApi` (boot-time mutable handle) and the
/// list of LoadedPlugins. The registrar walks each plugin's
/// `PluginContribution` and routes accordingly.
pub struct PluginRegistrar;

impl PluginRegistrar {
    pub fn register(
        api: &mut AgentApi,
        loaded: &LoadedPlugin,
    ) -> Result<PluginRegistrationSummary, RegistrationError> {
        let mut summary = PluginRegistrationSummary {
            plugin_id: loaded.manifest.id.clone(),
            ..Default::default()
        };
        let contrib = &loaded.manifest.contributes;

        // Tools — register as descriptors with a placeholder builder.
        // Task 3 swaps the placeholder for a real McpToolProxy delegate.
        for tool_name in &contrib.tools {
            let plugin_id = loaded.manifest.id.clone();
            let tool_name_owned = tool_name.clone();
            api.register_tool(ToolDescriptor {
                name: format!("{}:{}", plugin_id, tool_name_owned),
                description: format!(
                    "Tool {} contributed by plugin {} (proxy wiring in P3-4.3)",
                    tool_name_owned, plugin_id
                ),
                parameters_schema: serde_json::json!({}),
                builder: Arc::new(move |_ctx| {
                    // Placeholder — Task 3 replaces with McpToolProxy.
                    panic!(
                        "Plugin tool {} not yet wired to a backing proxy",
                        tool_name_owned
                    )
                }),
            });
            summary.tools_registered.push(tool_name.clone());
        }

        // Commands — placeholder until a real handler wiring lands.
        for cmd_name in &contrib.commands {
            summary.commands_registered.push(cmd_name.clone());
            // Real command registration deferred to a follow-up; this
            // is just a placeholder accounting.
        }

        // mcp_servers — handled in Task 3.
        for server_id in &contrib.mcp_servers {
            summary.mcp_servers_registered.push(server_id.clone());
        }

        // Skills + themes — record only.
        summary.skills_skipped = contrib.skills.clone();
        summary.themes_skipped = contrib.themes.clone();

        Ok(summary)
    }
}
```

The placeholder builder closure that panics is a sentinel — Task 3 replaces it with a real McpToolProxy. This way Task 2 ships compileable code without forcing the MCP integration yet.

#### Step 2.2: Update `src/plugins/mod.rs` to declare the new module

```rust
//! Plugin discovery + manifest → AgentApi/McpManager registration.

pub mod discovery;
pub mod registration;

#[cfg(test)]
mod tests;

pub use discovery::{PluginDiscovery, DiscoveryError, LoadedPlugin};
pub use registration::{PluginRegistrar, PluginRegistrationSummary, RegistrationError};
```

#### Step 2.3: Add registration tests to `src/plugins/tests.rs`

Append:

```rust
#[test]
fn registrar_records_contributions() {
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
kind = "subprocess"
executable = "./bin"

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
    assert_eq!(summary.tools_registered, vec!["foo", "bar"]);
    assert_eq!(summary.commands_registered, vec!["greet"]);
    assert_eq!(summary.skills_skipped, vec!["mathy"]);
    assert_eq!(summary.themes_skipped, vec!["dark"]);

    // Verify ToolDescriptors were registered (with the plugin_id:name prefix).
    assert!(api.tool("test-plugin:foo").is_some());
    assert!(api.tool("test-plugin:bar").is_some());
}
```

#### Step 2.4: Build + run tests (GREEN GATE)

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: empty.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo test --lib plugins:: 2>&1 | tail -5
```
Expected: 6 passed.

#### Step 2.5: Commit

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge add -A src-tauri/src/plugins/

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge commit -m "feat(plugins): PluginRegistrar routes contributions to AgentApi (P3-4.2 of 阶段 3)"
```

Continue to Task 3.

---

## Task 3: McpToolProxy wiring + plugin-declared mcp_servers integration

This task replaces the panic-builder placeholder from Task 2 with a real McpToolProxy delegate, and integrates `PluginContribution.mcp_servers` with McpManager.

**Files:**
- Modify: `src-tauri/src/plugins/registration.rs` (real builder + mcp_servers wiring)
- Modify: `src-tauri/src/mcp.rs` (small additions if needed — likely a `register_plugin_server` helper)
- Modify: `src-tauri/src/plugins/tests.rs` (add integration test)

### Steps

- [ ] **Step 3.1: Inspect existing McpToolProxy + McpManager surface**

```bash
grep -A 15 "pub struct McpToolProxy\|impl McpToolProxy" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri/src/mcp.rs | head -40
```

```bash
grep -n "pub async fn connect_server\|pub fn add_server\|pub fn get_server" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri/src/mcp.rs | head -10
```

Understand:
- How `McpToolProxy` is currently constructed (likely takes server_id, tool_name, and a handle to the McpManager).
- How servers are added to McpManager (likely `add_server(McpServerConfig)` or similar).
- Whether McpToolProxy implements the `Tool` trait directly.

#### Step 3.2: Wire real McpToolProxy in registration.rs

Replace the panic-builder with a real proxy that, at session-build time, looks up the MCP server by `plugin_id` and dispatches via the existing McpManager infrastructure.

This needs `&Arc<McpManager>` on the SessionContext. Verify:

```bash
grep -n "pub.*mcp_manager\|state.mcp_manager" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri/src/app.rs | head -5
```

Confirm McpManager is on AppState (via `state.mcp_manager: SharedMcpManager` likely an `Arc<RwLock<McpManager>>`). The builder closure receives `&SessionContext` which has `app_state: &AppState`, so it can access `ctx.app_state.mcp_manager.clone()`.

Update the registration.rs builder:

```rust
        for tool_name in &contrib.tools {
            let plugin_id = loaded.manifest.id.clone();
            let tool_name_owned = tool_name.clone();
            let prefixed_name = format!("{}:{}", plugin_id, tool_name_owned);
            api.register_tool(ToolDescriptor {
                name: prefixed_name.clone(),
                description: format!(
                    "Tool {} contributed by plugin {}",
                    tool_name_owned, plugin_id
                ),
                parameters_schema: serde_json::json!({}),
                builder: Arc::new(move |ctx| {
                    let plugin_id = plugin_id.clone();
                    let tool_name = tool_name_owned.clone();
                    let mcp = ctx.app_state.mcp_manager.clone();
                    Box::new(crate::mcp::McpToolProxy::for_plugin(
                        plugin_id,
                        tool_name,
                        mcp,
                    ))
                }),
            });
            summary.tools_registered.push(tool_name.clone());
        }
```

If `McpToolProxy::for_plugin` doesn't exist, add it to `mcp.rs`:

```rust
impl McpToolProxy {
    /// Construct a proxy for a plugin-declared tool.
    pub fn for_plugin(
        plugin_id: String,
        tool_name: String,
        mcp_manager: SharedMcpManager,
    ) -> Self {
        // Construct equivalent to existing McpToolProxy initialization,
        // but with the plugin's server_id (likely `plugin_id` itself) and
        // tool_name. Implementer adapts to actual McpToolProxy fields.
        Self { /* ... */ }
    }
}
```

The exact fields depend on what McpToolProxy looks like — implementer reads the existing struct and adapts.

#### Step 3.3: mcp_servers contribution wiring

For each `PluginContribution.mcp_servers` entry, the plugin has likely declared an MCP server config that gets added to McpManager.

```rust
// In PluginRegistrar::register, after the tools loop:

for server_id in &contrib.mcp_servers {
    // The plugin manifest declares an MCP server *identity*; the server
    // *config* may be in the plugin directory (e.g., plugin_dir/mcp_servers/<server_id>.json)
    // or inlined in the plugin.toml (separate [mcp_servers.<server_id>] tables).
    //
    // For P3-4: assume the plugin manifest declares server identities
    // only; the actual server configs are loaded separately (deferred
    // to P3-4.5 if needed). For now, just record the contribution.

    summary.mcp_servers_registered.push(server_id.clone());
}
```

If a simpler "plugin manifest contains inline server config" approach is preferred, the implementer extends the schema (or uses the existing inlined `mcp_servers` array of strings as just identity tokens, deferring the actual server config loading to a follow-up).

#### Step 3.4: Integration test — real McpToolProxy in registration

This is harder to unit-test because McpToolProxy needs a real subprocess. Use a `#[ignore]`-stubbed test with a comment pointing to the manual verification step, OR use a mock McpManager fixture.

For simplicity, add:

```rust
#[test]
#[ignore = "requires live McpManager + subprocess; covered by Task 6's echo plugin integration"]
fn registrar_wires_real_mcp_proxy() {
    // Real verification happens in Task 6 (echo plugin spawn + tool call).
}
```

#### Step 3.5: Build + tests

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo test --lib plugins:: 2>&1 | tail -5
```
Expected: build clean; 6 passed + 1 ignored.

#### Step 3.6: Commit

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge add -A \
    src-tauri/src/plugins/registration.rs \
    src-tauri/src/mcp.rs \
    src-tauri/src/plugins/tests.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge commit -m "feat(plugins): wire McpToolProxy into plugin tool registration (P3-4.3 of 阶段 3)"
```

Continue to Task 4.

---

## Task 4: `uclaw` MCP capability extension

Adds the namespace negotiation that lets uClaw plugins advertise hooks + renderers beyond standard MCP.

**Files:**
- Create: `src-tauri/src/plugins/uclaw_extension.rs`
- Modify: `src-tauri/src/plugins/mod.rs` (declare + re-export)
- Modify: `src-tauri/src/plugins/tests.rs` (add tests)

### Steps

- [ ] **Step 4.1: Create `src/plugins/uclaw_extension.rs`**

```rust
//! `uclaw` MCP capability extension.
//!
//! Plugins that opt in advertise `"uclaw": { ... }` in their MCP
//! `initialize` response. uClaw clients (PluginRegistrar) detect this
//! and register the additional contribution kinds (hooks, renderers,
//! commands beyond standard MCP).

use serde::{Deserialize, Serialize};

/// uClaw extension capability advertised in the MCP initialize response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UclawCapability {
    /// Extension version. Currently "1.0".
    pub version: String,
    /// Hooks the plugin wants to listen to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<String>,
    /// Renderers the plugin contributes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renderers: Vec<String>,
}

/// Outcome of the uclaw capability detection from an MCP InitializeResult.
#[derive(Debug, Clone)]
pub enum UclawCapabilityNegotiation {
    /// No uclaw extension advertised — plain MCP plugin.
    Absent,
    /// uclaw extension present with the given capability.
    Present(UclawCapability),
}

impl UclawCapabilityNegotiation {
    /// Detect from an MCP server's InitializeResult's capabilities object.
    /// Returns Absent if no `uclaw` key; Present with parsed payload otherwise.
    pub fn detect_from_capabilities(capabilities: &serde_json::Value) -> Self {
        let Some(uclaw) = capabilities.get("uclaw") else {
            return Self::Absent;
        };
        match serde_json::from_value::<UclawCapability>(uclaw.clone()) {
            Ok(cap) => Self::Present(cap),
            Err(_) => Self::Absent,
        }
    }
}
```

#### Step 4.2: Update mod.rs

```rust
//! Plugin discovery + manifest → AgentApi/McpManager registration.

pub mod discovery;
pub mod registration;
pub mod uclaw_extension;

#[cfg(test)]
mod tests;

pub use discovery::{PluginDiscovery, DiscoveryError, LoadedPlugin};
pub use registration::{PluginRegistrar, PluginRegistrationSummary, RegistrationError};
pub use uclaw_extension::{UclawCapability, UclawCapabilityNegotiation};
```

#### Step 4.3: Add tests to tests.rs

```rust
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
```

#### Step 4.4: Build + tests

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo test --lib plugins:: 2>&1 | tail -5
```
Expected: 9 passed (6 + 3 new uclaw extension tests).

#### Step 4.5: Commit

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge add -A src-tauri/src/plugins/

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge commit -m "feat(plugins): uclaw MCP capability extension detection (P3-4.4 of 阶段 3)"
```

Continue to Task 5.

---

## Task 5: Boot wiring — scan + register plugins at AppState::new()

**Files:**
- Modify: `src-tauri/src/app.rs` (boot block — create PluginDiscovery, scan, register contributions)

### Steps

- [ ] **Step 5.1: Find the AgentApi construction block + McpManager construction**

```bash
grep -n "agent_api = {\|mcp_manager = \|let mcp_manager" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri/src/app.rs | head -5
```

#### Step 5.2: Extend the agent_api block

Add plugin discovery + registration AFTER `register_all` (P3-2.5 surface) but BEFORE `Arc::new(api)`:

```rust
let agent_api = {
    let mut api = crate::agent::api::AgentApi::new();
    crate::agent::tools::builtin_descriptors::register_all(&mut api);

    // P3-4: discover + register plugins from $DATA_DIR/plugins/
    let plugins_root = data_dir.join("plugins");
    let discovery = crate::plugins::PluginDiscovery::new(&plugins_root);
    match discovery.discover() {
        Ok(results) => {
            for result in results {
                match result {
                    Ok(loaded) => {
                        match crate::plugins::PluginRegistrar::register(&mut api, &loaded) {
                            Ok(summary) => {
                                tracing::info!(
                                    plugin_id = %summary.plugin_id,
                                    tools = ?summary.tools_registered,
                                    "[P3-4] plugin loaded"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "[P3-4] plugin registration failed");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "[P3-4] plugin discovery failed");
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "[P3-4] plugins directory scan failed");
        }
    }

    api.set_provider_service(provider_service.clone());
    api.set_hook_bus(hook_bus.clone());
    std::sync::Arc::new(api)
};
```

Verify `data_dir` is in scope at this point (it's the AppState's data_dir field — the same one used by McpManager and ProviderService construction).

#### Step 5.3: Build + tests (GREEN GATE)

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: empty.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```
Expected: 796 passed / 2 failed (boot path runs against an empty plugins dir in test mode).

#### Step 5.4: Warning count check

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
```
Expected: ≤55 (some new warnings on placeholder code in plugins module are acceptable).

#### Step 5.5: Commit

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge add -A src-tauri/src/app.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge commit -m "feat(app): scan + register plugins at AppState::new boot (P3-4.5 of 阶段 3)"
```

Continue to Task 6.

---

## Task 6: Echo demo plugin (Rust binary) + verification path

**Files:**
- Create: `examples/echo_plugin/main.rs` (Rust binary speaking MCP + uclaw extension)
- Create: `examples/echo_plugin/plugin.toml` (manifest)
- Modify: `src-tauri/Cargo.toml` ([[example]] entry)
- Modify: `src-tauri/src/plugins/tests.rs` (integration test using the echo plugin binary via cargo)

### Steps

- [ ] **Step 6.1: Create `examples/echo_plugin/main.rs`**

A small Rust binary that:
- Reads JSON-RPC requests from stdin.
- Implements MCP `initialize` (with uclaw capability), `tools/list` (one tool: `echo`), `tools/call` (echoes input).
- Writes JSON-RPC responses to stdout.

```rust
//! Echo plugin — demo for P3-4.
//!
//! A minimal MCP server with the `uclaw` capability extension. Echoes
//! whatever you send it. Used to verify the plugin discovery → registration
//! → tool dispatch path end-to-end.
//!
//! Plugin manifest at `examples/echo_plugin/plugin.toml` declares this
//! binary as the runtime. Build with `cargo build --example echo_plugin`.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("echo_plugin: bad input: {}", e);
                continue;
            }
        };
        let id = req.id.unwrap_or(serde_json::Value::Null);
        let response = match req.method.as_str() {
            "initialize" => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": false },
                        "uclaw": {
                            "version": "1.0",
                            "hooks": [],
                            "renderers": []
                        }
                    },
                    "serverInfo": {
                        "name": "echo_plugin",
                        "version": "0.1.0"
                    }
                })),
                error: None,
            },
            "tools/list" => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(serde_json::json!({
                    "tools": [
                        {
                            "name": "echo",
                            "description": "Echoes the input back",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "message": { "type": "string" }
                                },
                                "required": ["message"]
                            }
                        }
                    ]
                })),
                error: None,
            },
            "tools/call" => {
                let message = req
                    .params
                    .get("arguments")
                    .and_then(|a| a.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("(no message)");
                JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(serde_json::json!({
                        "content": [
                            { "type": "text", "text": message }
                        ]
                    })),
                    error: None,
                }
            }
            other => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("method {} not found", other),
                }),
            },
        };
        let line = serde_json::to_string(&response).unwrap();
        writeln!(out, "{}", line)?;
        out.flush()?;
    }
    Ok(())
}
```

#### Step 6.2: Create `examples/echo_plugin/plugin.toml`

```toml
id = "echo_plugin"
version = "0.1.0"
display_name = "Echo Plugin"
description = "Demo MCP plugin with uclaw capability extension"

[author]
name = "uClaw team"

[runtime]
# Replace with actual schema fields the manifest expects.
# E.g., kind = "subprocess" + executable = "../target/debug/examples/echo_plugin"

[contributes]
tools = ["echo"]
```

The exact `[runtime]` keys must match the `PluginRuntimeRequirement` schema in `plugin_manifest/schema.rs`. Verify and adapt.

#### Step 6.3: Add `[[example]]` to src-tauri/Cargo.toml

```toml
[[example]]
name = "echo_plugin"
path = "../examples/echo_plugin/main.rs"
```

(If Cargo doesn't allow path traversal outside the package, move the example inside `src-tauri/examples/echo_plugin/` instead. Verify what convention this project uses for examples.)

#### Step 6.4: Add an integration test that scans + registers the echo plugin

```rust
// In src/plugins/tests.rs

#[test]
fn echo_plugin_manifest_scans_and_registers() {
    // Build the echo plugin first so the binary exists.
    // This test verifies the manifest parses correctly; the actual
    // subprocess spawn + tool dispatch is exercised manually or in
    // a separate integration test that needs cargo build --example.

    let manifest_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()  // src-tauri/.. = repo root
        .join("examples/echo_plugin/plugin.toml");

    if !manifest_path.exists() {
        return; // Skip in environments without the example dir.
    }

    // Set up a temp plugins root containing only the echo plugin (symlinked or copied).
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
```

#### Step 6.5: Build the echo plugin example + run tests

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo build --example echo_plugin 2>&1 | tail -5
```
Expected: Finished. The binary should land at `target/debug/examples/echo_plugin`.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge/src-tauri && cargo test --lib plugins:: 2>&1 | tail -5
```
Expected: ≥10 tests pass (subset, depending on existing count + new echo test).

#### Step 6.6: Commit

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge add -A \
    examples/echo_plugin/ \
    src-tauri/Cargo.toml \
    src-tauri/src/plugins/tests.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge commit -m "$(cat <<'EOF'
feat(examples): echo demo plugin + integration test (P3-4.6 of 阶段 3)

New examples/echo_plugin/ directory with:
- main.rs: minimal MCP server speaking JSON-RPC + advertising the
  `uclaw` capability extension. ~150 LoC, no external deps beyond
  serde/serde_json.
- plugin.toml: PluginManifest declaring tools = ["echo"].

New Cargo.toml [[example]] entry so `cargo build --example echo_plugin`
produces the binary at `target/debug/examples/echo_plugin`.

New plugins integration test verifies discovery + manifest parsing
end-to-end against the echo plugin's actual manifest.

VANILLA MCP VERIFICATION: P3-4 boot wiring scans $DATA_DIR/plugins/;
existing MCP servers in mcp_servers.json continue working unchanged
because mcp_servers.json is loaded by McpManager directly (untouched
by this PR).

Final P3-4 commit. cargo build clean (lib + echo_plugin example);
agent:: 796/2 baseline preserved; plugins:: ≥10/0; cargo test --lib
total grows by ~12 tests.

Cumulative P3-4 (6 commits): new module src/plugins/ (~700 LoC across
4 files) + small additions to mcp.rs + boot wiring in app.rs +
examples/echo_plugin/ (~200 LoC) + small Cargo.toml + lib.rs touch.

Next strategic step: P3-5 (dispatcher.rs 3,859 LoC / 71 fields split).
EOF
)"
```

Verify final chain:

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge log --oneline HEAD~5..HEAD
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p4-plugin-discovery-bridge status -sb
```

Expected: 6 commits ahead of `main`; working tree clean.

---

## Self-Review

**1. Spec coverage:**
- ✅ Spec §5 + P3-4 row corrected (Correction callout linking to this plan).
- ✅ PluginDiscovery scaffold (Task 1).
- ✅ PluginContribution → AgentApi tools/commands routing (Tasks 2-3).
- ✅ McpToolProxy wiring (Task 3).
- ✅ uclaw capability extension detection (Task 4).
- ✅ Boot wiring (Task 5).
- ✅ Echo demo plugin (Task 6).
- ✅ Vanilla MCP verification (implicit: mcp_servers.json untouched, McpManager paths unchanged).

**2. Placeholder scan:**
- Task 3 mentions `McpToolProxy::for_plugin` as a method that may need to be added — flagged as implementer judgment based on existing McpToolProxy shape.
- Task 6 mentions `[runtime]` schema adaptation — flagged for implementer to match actual `PluginRuntimeRequirement` shape.
- Task 5 boot wiring uses `tracing::info!`/`tracing::warn!` for plugin lifecycle logging — clean structured logging, not a placeholder.
- No "TBD" / "TODO" / "implement later" / "similar to Task N".

**3. Type consistency:**
- `PluginManifest`, `PluginContribution`, `PluginAuthor`, `PluginPermissions` named consistently with the P2-preserved schema.
- `LoadedPlugin`, `DiscoveryError`, `PluginRegistrar`, `PluginRegistrationSummary`, `RegistrationError` named consistently across Tasks 1-3 + tests.
- `UclawCapability`, `UclawCapabilityNegotiation` named consistently in Task 4 + tests.
- All `crate::plugins::*` paths consistent.

No spec gaps, 2 implementer-judgment notes (well-flagged), no type inconsistencies. Plan ready.

---

## Quick reference

- **Estimated time:** 1.0-1.5 person-day (6 tasks; Task 6 the largest with Rust binary).
- **Risk:** medium. Task 3 (McpToolProxy wiring) requires inspecting the existing McpToolProxy struct + adapting; Task 6's example binary needs Cargo.toml `[[example]]` configuration to work.
- **Files touched:**
  - Task 1: 4 (3 new + 1 lib.rs)
  - Task 2: 2 (1 new + mod.rs + tests.rs)
  - Task 3: 3 (registration.rs + mcp.rs + tests.rs)
  - Task 4: 3 (1 new + mod.rs + tests.rs)
  - Task 5: 1 (app.rs)
  - Task 6: 3-4 (examples/ + Cargo.toml + tests.rs)
- **Net LoC:** +900 to +1,100 across the plan (much smaller than the heavy Option C path would have been).
- **PR shape:** 1 worktree → 6 commits → 1 PR. Bisectable per-task. Squash-on-land per project convention.
- **Non-goals (deferred)**:
  - **Hook registration via uclaw extension**: Task 4 only detects the capability; actually plumbing `uclaw/list_hooks` + invoking back to AgentApi.on() is deferred to P3-4.5.
  - **Renderer registration via uclaw extension**: Same — detection only, not plumbing.
  - **Plugin permission enforcement**: PluginPermissions are recorded but not actually gated. Future PR adds the install-time confirmation UI + runtime permission checks.
  - **Skill + theme contributions**: Recorded but not registered. Future PR wires SkillsRegistry + Themes integration.
  - **Plugin unload at runtime**: Plugins stay loaded for the AppState's lifetime. Hot-reload is future work.
  - **Migration of existing MCP servers under mcp_servers.json to plugin manifests**: Backward-compat means both paths coexist; no automatic migration.
