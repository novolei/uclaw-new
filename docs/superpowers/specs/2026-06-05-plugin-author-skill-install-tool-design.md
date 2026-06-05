# Plugin-Author Skill + install_plugin Tool Design (Slice A)

**Date:** 2026-06-05
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b — self-extending plugins. Lets the user say "make me a plugin that does X" and the Agent authors a self-contained plugin (plugin.toml + stdio MCP server) and installs it via a new `install_plugin` tool. Builds on the install mechanism (#675). Slice A of two (Slice B = marketplace catalog).

## Problem

uClaw can run + install plugins, but creating one is a manual authoring task. There's no agent-assisted path to scaffold + install a plugin from a natural-language request. We have all the pieces — `install_from_local_dir` (writes a plugin dir + validates), the plugin.toml schema, the hello-uclaw template, and the agent can write files — but no built-in skill that teaches the agent the format/workflow, and no agent-callable tool to finalize the install (validate + DB row + a clean "installed" signal with user approval).

## Decision (approved 2026-06-05)

- **`install_plugin` = an approval-gated built-in agent tool** wrapping `install_from_local_dir`. The agent scaffolds a plugin dir, then calls `install_plugin({ dir })`; the tool requires user approval (`ApprovalRequirement::Always`) so the user confirms before runnable code is installed; on approve it validates + copies into `plugins/<id>/` + ensures the DB row + returns "installed; restart to activate".
- **`plugin-author` = a bundled (first-party) skill** (`skills/plugin-author/SKILL.md`) teaching the agent: the plugin.toml format, the stdio MCP server contract (initialize / tools/list / tools/call), the hello-uclaw template, and the workflow (scaffold to a temp dir → self-check → summarize plugin.toml + permissions to the user → call `install_plugin` → tell them to restart). Constraint: **self-contained servers only** (Node builtins / single file; no npm install in v1).

## Design

### §1 `install_plugin` tool (`agent/tools/builtin/install_plugin.rs` new + register in `builtin_descriptors.rs`)
```rust
pub struct InstallPluginTool {
    data_dir: std::path::PathBuf,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
}
impl InstallPluginTool { pub fn new(data_dir, db) -> Self {…} }

#[async_trait]
impl Tool for InstallPluginTool {
    fn name(&self) -> &str { "install_plugin" }
    fn description(&self) -> &str { "Install a plugin from a local directory you have scaffolded (must contain plugin.toml). The plugin activates on the next app restart. Use after writing a complete, self-contained plugin dir." }
    fn parameters_schema(&self) -> Value { object { dir: string (required) — "Absolute path to the scaffolded plugin directory containing plugin.toml" } }
    fn requires_approval(&self, _p) -> ApprovalRequirement { ApprovalRequirement::Always }   // installs runnable code → always confirm
    async fn execute(&self, params) -> Result<ToolOutput, ToolError> {
        let dir = params["dir"].as_str().ok_or(InvalidParams)?;
        let plugins_root = self.data_dir.join("plugins");
        match crate::plugins::install::install_from_local_dir(Path::new(dir), &plugins_root) {
            Ok(p) => {
                let now = chrono::Utc::now().timestamp_millis();
                if let Ok(conn) = self.db.lock() { let _ = crate::plugins::state::ensure_plugin_row(&conn, &p.id, now); }
                Ok(ToolOutput::success(&format!("Installed plugin '{}' v{}. Restart uClaw to activate its tools/commands.", p.display_name, p.version), …))
            }
            Err(e) => Ok(ToolOutput::error(&format!("Install failed: {e}"), …)),  // tool-level error, not a hard ToolError, so the agent can react
        }
    }
}
```
Register in `builtin_descriptors::register_all`: `builder: Arc::new(|ctx| Box::new(InstallPluginTool::new(ctx.app_state.data_dir.clone(), Arc::clone(&ctx.app_state.db))))`. (Plan pins the exact descriptor block + the parameters_schema literal.)

### §2 `plugin-author` skill (`skills/plugin-author/SKILL.md` new)
Frontmatter (match the bundled `writing-assistant` shape): name `plugin-author`, version, description ("Author + install a uClaw plugin from a natural-language request"), author `uclaw`, enabled true, category `productivity` (or `development`), activation keywords (plugin, 插件, scaffold, "make a plugin", "create a tool/command"), tags.
Body teaches:
- **What a uClaw plugin is**: a dir `<id>/` with `plugin.toml` + a stdio MCP server executable.
- **plugin.toml format** (full schema with a filled example — id==dirname, version, display_name, author, runtime{kind=subprocess, executable}, permissions{network/filesystem_read/filesystem_write/run_subprocess}, contributes{tools, commands, mcp_servers}).
- **stdio MCP server contract**: line-delimited JSON-RPC; handle `initialize` (return protocolVersion + capabilities + serverInfo), `tools/list` (declare tools w/ inputSchema), `tools/call` (dispatch by name, return `{content:[{type:"text",text}]}`); ignore `notifications/*`. Give the hello-uclaw `server.mjs` as the copy-paste template.
- **Mapping**: a `tool` (agent-callable) and a `command` (`/name`, user-typed) both route to a `tools/call` on the server by name — so the server's `tools/call` handles both; declare tools in `contributes.tools` and commands in `contributes.commands`.
- **Workflow** (the ritual): (1) clarify what the plugin should do; (2) pick an id (kebab-case); (3) write to a temp dir `<tmp>/<id>/` — `plugin.toml` + the server (`server.mjs`, Node, **self-contained, no npm**); (4) self-check: run the server with a probe `initialize` line (via the bash tool) and confirm a valid reply; (5) **summarize the plugin.toml + requested permissions to the user**; (6) call the `install_plugin` tool with the temp dir; (7) tell the user to **restart uClaw** to activate.
- **Constraints/safety**: self-contained only; minimal permissions (only request what's needed — the sandbox enforces them); the install requires user approval.

## Data flow

```
user: "做个插件，给我一个 /weather 命令查城市天气"
 → agent (guided by plugin-author skill):
    scaffold /tmp/uclaw-plugin-weather/{plugin.toml, server.mjs}
    self-check: echo '{"...initialize..."}' | node server.mjs  → valid reply ✓
    summarize plugin.toml + permissions(network) to user
    call install_plugin({ dir: "/tmp/uclaw-plugin-weather" })  → APPROVAL prompt
 → on approve: install_from_local_dir → plugins/weather/ + ensure_plugin_row
 → "Installed weather v0.1.0. Restart uClaw to use /weather."
 → restart → /weather Tokyo routes to the new plugin (sandboxed)
```

## Out of scope

git-based authoring (the agent scaffolds local); the marketplace catalog (Slice B); npm-dependency plugins (self-contained only v1); auto-restart (user restarts); editing/upgrading an existing plugin via the tool (reject-if-exists); WASM/inproc runtimes; uninstall. The `install_plugin` tool is local-dir only (the user-facing git/folder UI install stays in PluginsSettings).

## Error handling

`install_plugin`: missing `dir` param → `ToolError::InvalidParams`. `install_from_local_dir` Err (AlreadyInstalled / ManifestMissing / ManifestInvalid / Io) → `ToolOutput::error(...)` (soft error so the agent can fix + retry, not a hard abort). DB-row failure → logged, non-fatal (the plugin still discovers + defaults enabled on boot). Approval declined → the tool isn't executed (standard safety-gate path); the agent sees the decline. The skill is content-only (no runtime risk); the sandbox (#669) contains the installed plugin at boot.

## Testing

1. **install_plugin tool**: unit-test the tool's execute against a temp data_dir with a valid scaffolded plugin dir → returns success + the plugin lands in `<data_dir>/plugins/<id>/` + a DB row exists; invalid dir (no plugin.toml) → `ToolOutput::error`; missing `dir` param → InvalidParams. `requires_approval` returns `Always`.
2. **skill discovery**: the new `skills/plugin-author/SKILL.md` parses (valid frontmatter) — covered by the existing skills-registry discovery tests if they scan the bundled dir; at minimum `cargo build` + a manual parse check.
3. `cargo build`/clippy clean; `cargo test --lib agent::tools plugins`.

## Scope / files

| File | Change |
|---|---|
| `agent/tools/builtin/install_plugin.rs` (new) | `InstallPluginTool` (Tool impl, approval=Always, calls install_from_local_dir + ensure_plugin_row) + tests |
| `agent/tools/builtin/mod.rs` | `pub mod install_plugin;` |
| `agent/tools/builtin_descriptors.rs` | register `install_plugin` descriptor (builder captures data_dir + db) |
| `skills/plugin-author/SKILL.md` (new) | the authoring skill |

## Risk

Low-med. New agent tool (installs code) + a content skill. Main risks: (1) **approval gating** — `requires_approval` MUST return `Always` so the user confirms before runnable code installs (verify it's not in SafetyPolicy.auto_approved_tools). (2) **tool builder app_state capture** — the descriptor builder captures `data_dir` + `db` from `ctx.app_state` (mirror the SelfEvalTool/RequestPlanModeSwitch builders). (3) **self-check guidance** — the skill must steer the agent to verify the server boots (a broken server installs but fails at runtime); the probe-initialize step catches it. (4) **restart-to-activate** — the tool's success message + the skill both state it. (5) **bundled-skill location** — `skills/plugin-author/SKILL.md` at repo root (mirrored to resource_dir/skills by Tauri); frontmatter must match the loader (name/version/description/...). Bisectable: install_plugin tool+tests → skill → verify. After this slice, uClaw self-extends: a natural-language request becomes an installed, sandboxed plugin — the agent grows its own capabilities.
