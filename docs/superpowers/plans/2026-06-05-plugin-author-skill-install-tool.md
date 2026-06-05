# Plugin-Author Skill + install_plugin Tool Implementation Plan (Slice A)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** An approval-gated `install_plugin` agent tool + a bundled `plugin-author` skill so the Agent can author a self-contained plugin from a natural-language request and install it.

**Spec:** `docs/superpowers/specs/2026-06-05-plugin-author-skill-install-tool-design.md`

---

## Pinned facts (verbatim)

- **Builtin tool pattern** (`agent/tools/builtin_descriptors.rs::register_all(api)`, called at app.rs:931): each tool = `api.register_tool(ToolDescriptor { name, description, parameters_schema, builder: Arc::new(|ctx| Box::new(SomeTool::new(...))) })`. Example capturing app_state.db: `builder: Arc::new(|ctx| Box::new(builtin::self_eval::SelfEvalTool::new(ctx.session_id.clone(), Arc::clone(&ctx.app_state.db), ctx.app_handle.clone())))`.
- **`SessionContext`** (agent/api/session_context.rs): `{ session_id: String, workspace: PathBuf, model, app_handle: AppHandle, llm, app_state: &AppState, tool_config }`. `ctx.app_state.data_dir: PathBuf`, `ctx.app_state.db: Arc<std::sync::Mutex<rusqlite::Connection>>`.
- **`Tool` trait** (agent/tools/tool.rs): `#[async_trait] trait Tool { fn name()->&str; fn description()->&str; fn parameters_schema()->Value; async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError>; fn requires_approval(&self,_)->ApprovalRequirement { Never }; fn effects()->ToolEffects { write() } }`. `ApprovalRequirement::{Never, UnlessAutoApproved, Always}`. `ToolOutput::success(text:&str, duration_ms:u64)` / `ToolOutput::error(text:&str, duration_ms:u64)`. `ToolError::InvalidParams(String)` / `ToolError::Execution(String)`.
- **builtin tool struct template** (self_eval.rs): `pub struct SelfEvalTool { db: Arc<std::sync::Mutex<rusqlite::Connection>>, … }` + `#[async_trait] impl Tool for SelfEvalTool { … async fn execute(&self, params) { let start = std::time::Instant::now(); … Ok(ToolOutput::success(&msg, start.elapsed().as_millis() as u64)) } }`.
- **`agent/tools/builtin/mod.rs`** — add `pub mod install_plugin;` (mirror existing `pub mod self_eval;` etc.).
- **install** (`plugins/install.rs`): `pub fn install_from_local_dir(src: &Path, plugins_root: &Path) -> Result<InstalledPlugin, InstallError>`; `InstalledPlugin { id, display_name, version }`; `InstallError` (Display via thiserror). `pub fn ensure_plugin_row(conn: &Connection, id: &str, now_ms: i64) -> rusqlite::Result<()>` (plugins/state.rs). `plugins_root = data_dir.join("plugins")`. `now_ms = chrono::Utc::now().timestamp_millis()`.
- **Bundled skills**: repo top-level `skills/` → `resource_dir/skills/` (app.rs:526). Frontmatter shape (from `skills/writing-assistant/SKILL.md`): `name, version, description, author, enabled, category, activation:{keywords:[],patterns:[],tags:[],exclude_keywords:[],max_context_tokens}, parameters:[]`. (`name` MUST equal something the loader keys on; match writing-assistant exactly.)
- **Template for the skill body**: `examples/plugins/hello-uclaw/{plugin.toml, server.mjs}` (the canonical self-contained stdio MCP server).
- **NEW files**: `agent/tools/builtin/install_plugin.rs` + `skills/plugin-author/SKILL.md` need explicit `git add`.

---

## Task 1: `install_plugin` built-in tool

**Files:** Create `agent/tools/builtin/install_plugin.rs`; modify `agent/tools/builtin/mod.rs`, `agent/tools/builtin_descriptors.rs`

- [ ] **Step 1: create `agent/tools/builtin/install_plugin.rs`**
```rust
//! Pi-3b — `install_plugin` agent tool: install a plugin from a local directory
//! the agent has scaffolded (plugin-author skill). Approval-gated (installs
//! runnable code). The plugin activates on next app restart.

use std::path::Path;
use std::sync::Arc;
use async_trait::async_trait;

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};

pub struct InstallPluginTool {
    data_dir: std::path::PathBuf,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl InstallPluginTool {
    pub fn new(data_dir: std::path::PathBuf, db: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        Self { data_dir, db }
    }
}

#[async_trait]
impl Tool for InstallPluginTool {
    fn name(&self) -> &str { "install_plugin" }

    fn description(&self) -> &str {
        "Install a uClaw plugin from a local directory you have scaffolded (the directory must \
         contain a valid plugin.toml + its stdio MCP server executable). The plugin activates on \
         the next app restart. Use this after authoring a complete, self-contained plugin (see the \
         plugin-author skill). Requires user approval."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "dir": {
                    "type": "string",
                    "description": "Absolute path to the scaffolded plugin directory containing plugin.toml."
                }
            },
            "required": ["dir"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // Installs runnable third-party code — always confirm with the user.
        ApprovalRequirement::Always
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let dir = params
            .get("dir")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("missing 'dir'".into()))?;
        let plugins_root = self.data_dir.join("plugins");
        match crate::plugins::install::install_from_local_dir(Path::new(dir), &plugins_root) {
            Ok(p) => {
                let now_ms = chrono::Utc::now().timestamp_millis();
                if let Ok(conn) = self.db.lock() {
                    let _ = crate::plugins::state::ensure_plugin_row(&conn, &p.id, now_ms);
                }
                Ok(ToolOutput::success(
                    &format!(
                        "Installed plugin '{}' v{} (id: {}). Restart uClaw to activate its tools/commands.",
                        p.display_name, p.version, p.id
                    ),
                    start.elapsed().as_millis() as u64,
                ))
            }
            Err(e) => Ok(ToolOutput::error(
                &format!("Install failed: {e}"),
                start.elapsed().as_millis() as u64,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mem_db() -> Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE plugins (id TEXT PRIMARY KEY, enabled INTEGER NOT NULL DEFAULT 1, updated_at INTEGER NOT NULL)", []).unwrap();
        Arc::new(std::sync::Mutex::new(conn))
    }
    fn write_plugin(dir: &Path, id: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("plugin.toml"), format!(
            "id = \"{id}\"\nversion = \"0.1.0\"\ndisplay_name = \"Demo\"\n\n[author]\nname = \"t\"\n\n[runtime]\nmin_uclaw_version = \"0.1.0\"\n"
        )).unwrap();
        std::fs::write(dir.join("server.mjs"), "// demo").unwrap();
    }

    #[tokio::test]
    async fn install_plugin_tool_installs_and_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let src = tmp.path().join("src");
        write_plugin(&src, "demo");
        let tool = InstallPluginTool::new(data_dir.clone(), mem_db());
        assert_eq!(tool.requires_approval(&serde_json::json!({})), ApprovalRequirement::Always);
        let out = tool.execute(serde_json::json!({ "dir": src.to_str().unwrap() })).await.unwrap();
        assert_eq!(out.result["ok"], true, "got {:?}", out.result);
        assert!(data_dir.join("plugins/demo/plugin.toml").exists());
    }

    #[tokio::test]
    async fn install_plugin_tool_reports_error_on_bad_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let tool = InstallPluginTool::new(tmp.path().join("data"), mem_db());
        let out = tool.execute(serde_json::json!({ "dir": tmp.path().join("nope").to_str().unwrap() })).await.unwrap();
        assert_eq!(out.result["ok"], false);
    }

    #[tokio::test]
    async fn install_plugin_tool_missing_dir_param() {
        let tmp = tempfile::tempdir().unwrap();
        let tool = InstallPluginTool::new(tmp.path().join("data"), mem_db());
        let err = tool.execute(serde_json::json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidParams(_)));
    }
}
```
(CONFIRM the exact `ToolOutput`/`ToolError`/`ApprovalRequirement` import path — `crate::agent::tools::tool::*` per the recon; adjust if the module path differs. CONFIRM `ToolOutput.result` shape is `{"ok":bool,...}` per the recon's `ToolOutput::success/error`.)

- [ ] **Step 2: `agent/tools/builtin/mod.rs`** — add `pub mod install_plugin;`.

- [ ] **Step 3: register in `builtin_descriptors.rs`** — inside `register_all`, add a descriptor block:
```rust
    {
        api.register_tool(ToolDescriptor {
            name: "install_plugin".to_string(),
            description: "Install a uClaw plugin from a local directory you scaffolded (must contain plugin.toml). Activates on next restart. Requires approval.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": { "dir": { "type": "string", "description": "Absolute path to the scaffolded plugin directory containing plugin.toml." } },
                "required": ["dir"]
            }),
            builder: Arc::new(|ctx| {
                Box::new(builtin::install_plugin::InstallPluginTool::new(
                    ctx.app_state.data_dir.clone(),
                    Arc::clone(&ctx.app_state.db),
                ))
            }),
        });
    }
```
(Match the file's existing `builtin::` import alias + `ToolDescriptor` path.)

- [ ] **Step 4: build + test + commit**
`cd src-tauri && cargo test --lib install_plugin 2>&1 | tail` → green. `cargo build 2>&1 | grep -E "^error"` → empty. `cargo clippy --lib 2>&1 | grep install_plugin | grep -iE "warning|error"` → none.
```bash
git add src-tauri/src/agent/tools/builtin/install_plugin.rs src-tauri/src/agent/tools/builtin/mod.rs src-tauri/src/agent/tools/builtin_descriptors.rs
git commit -m "feat(agent): install_plugin tool — approval-gated plugin install for agent-authored plugins"
```
Verify `git show HEAD --stat` lists `builtin/install_plugin.rs`.

---

## Task 2: `plugin-author` bundled skill

**Files:** Create `skills/plugin-author/SKILL.md`

- [ ] **Step 1: create `skills/plugin-author/SKILL.md`** — frontmatter mirroring `skills/writing-assistant/SKILL.md`:
```markdown
---
name: plugin-author
version: "1.0.0"
description: Author and install a uClaw plugin (a stdio MCP server + plugin.toml) from a natural-language request.
author: uclaw
enabled: true
category: development
activation:
  keywords:
    - plugin
    - 插件
    - "make a plugin"
    - "create a plugin"
    - scaffold
    - "add a tool"
    - "add a command"
  patterns:
    - "(?i)\\b(make|create|build|write|scaffold)\\b.*\\bplugin\\b"
    - "(?i)(做|写|生成|创建).*插件"
  tags:
    - plugin
    - extension
    - mcp
  exclude_keywords: []
  max_context_tokens: 3000
---

# Authoring a uClaw plugin

When the user asks you to create a plugin / a new tool or slash command, author a
**self-contained** uClaw plugin and install it with the `install_plugin` tool.

## What a uClaw plugin is
A directory `<id>/` containing:
- `plugin.toml` — the manifest (the dir name MUST equal `id`)
- a stdio MCP server executable (e.g. `server.mjs`, Node)

A plugin contributes **tools** (agent-callable), **commands** (`/name`, user-typed), and/or
**skills**. Tools and commands both route to a `tools/call` on the server by name.

## plugin.toml format (example)
```toml
id = "weather"                 # kebab-case; MUST equal the directory name
version = "0.1.0"
display_name = "Weather"
description = "Look up weather for a city."

[author]
name = "uClaw user"

[runtime]
min_uclaw_version = "0.1.0"
kind = "subprocess"
executable = "server.mjs"

[permissions]                  # request the MINIMUM needed; the sandbox enforces these
run_subprocess = true
network = true                 # only if the server makes network calls
# filesystem_read = true / filesystem_write = true — only if needed

[contributes]
tools = ["weather"]            # agent-callable tools (names = tools/call names)
commands = ["weather"]         # /weather slash command (routes to the same tools/call)
mcp_servers = ["weather"]      # the server itself
```

## stdio MCP server contract (copy-paste template)
Line-delimited JSON-RPC 2.0 on stdin/stdout. Handle `initialize`, `tools/list`, `tools/call`;
ignore `notifications/*`. **Self-contained — Node builtins only, no `npm install`.**
```javascript
#!/usr/bin/env node
import readline from "readline";
const rl = readline.createInterface({ input: process.stdin, terminal: false });
const reply = (o) => process.stdout.write(JSON.stringify(o) + "\n");
rl.on("line", async (raw) => {
  const line = raw.trim(); if (!line) return;
  let req; try { req = JSON.parse(line); } catch { return reply({ jsonrpc:"2.0", id:null, error:{ code:-32700, message:"parse error" } }); }
  const { id, method, params } = req;
  if (typeof method === "string" && method.startsWith("notifications/")) return;
  switch (method) {
    case "initialize":
      return reply({ jsonrpc:"2.0", id, result:{ protocolVersion:"2024-11-05", capabilities:{ tools:{} }, serverInfo:{ name:"weather", version:"0.1.0" } } });
    case "tools/list":
      return reply({ jsonrpc:"2.0", id, result:{ tools:[ { name:"weather", description:"Get weather for a city.", inputSchema:{ type:"object", properties:{ city:{ type:"string" } }, required:[] } } ] } });
    case "tools/call": {
      const name = params?.name;
      // `/weather <city>` arrives as arguments.args; agent tool calls pass arguments.city
      const city = (params?.arguments?.city ?? params?.arguments?.args ?? "").trim() || "your area";
      if (name === "weather") {
        // example: a real impl would `fetch` a weather API (needs network permission)
        return reply({ jsonrpc:"2.0", id, result:{ content:[{ type:"text", text:`Weather for ${city}: (stub)` }] } });
      }
      return reply({ jsonrpc:"2.0", id, error:{ code:-32601, message:`tool not found: ${name}` } });
    }
    default:
      return reply({ jsonrpc:"2.0", id, error:{ code:-32601, message:"method not found" } });
  }
});
```

## Workflow (follow this ritual)
1. **Clarify** what the plugin should do — its tool(s)/command(s), inputs, and whether it needs network/filesystem.
2. **Pick a kebab-case `id`** (e.g. `weather`).
3. **Scaffold to a temp dir** `<tmpdir>/<id>/` (use the bash tool, e.g. under `/tmp`): write `plugin.toml` + the server (`server.mjs`). Keep it **self-contained** (Node builtins only).
4. **Self-check**: run the server with a probe initialize and confirm a valid reply:
   `printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | node <tmpdir>/<id>/server.mjs` → expect a JSON result with `serverInfo`.
5. **Summarize** the `plugin.toml` + the requested permissions to the user (so they know what they're approving).
6. **Install**: call the `install_plugin` tool with `{ "dir": "<tmpdir>/<id>" }`. The user will be asked to approve.
7. **Tell the user to restart uClaw** to activate the plugin's tools/commands (registration is boot-time).

## Rules
- Request the **minimum** permissions; the macOS sandbox enforces them (no network unless `network=true`, writes jailed unless `filesystem_write=true`).
- **Self-contained only** in v1 — do not rely on `npm install` / external packages.
- If `install_plugin` reports an error (e.g. id already installed, invalid plugin.toml), fix it and retry.
```

- [ ] **Step 2: build + (skill parse) + commit**
`cd src-tauri && cargo build 2>&1 | grep -E "^error"` → empty. If a skills-registry test scans bundled skills, `cargo test --lib skills 2>&1 | tail` → green (the new SKILL.md must parse). (If no auto-scan test, the frontmatter matches writing-assistant's shape → parses.)
```bash
git add skills/plugin-author/SKILL.md
git commit -m "feat(skills): plugin-author bundled skill — guide the agent to author + install plugins"
```
Verify `git show HEAD --stat` lists `skills/plugin-author/SKILL.md`.

---

## Task 3: Whole-slice verify + ship

- [ ] **Step 1**: `cargo build` + `cargo clippy --lib` clean; `cargo test --lib agent::tools plugins skills` green.
- [ ] **Step 2**: grep gates — `install_plugin` registered in builtin_descriptors; `pub mod install_plugin` in builtin/mod.rs; `requires_approval → Always`; `skills/plugin-author/SKILL.md` present with valid frontmatter.
- [ ] **Step 3**: PR with `## Commits (bisectable)` table. Note: approval-gated install_plugin (installs runnable code); self-contained-only authoring; restart-to-activate; the skill teaches format + workflow + self-check; sandbox (#669) contains the result.
- [ ] **Step 4**: rebase onto latest origin/main, rebase-merge, sync main, cleanup, reindex, update memory ([[project-pi-lightweight-vs-agent-os]]: self-extending plugins — plugin-author skill + install_plugin tool shipped; next = marketplace catalog (Slice B)).

---

## Self-Review

**Spec coverage:** §1 tool → T1; §2 skill → T2. ✓
**Placeholder scan:** the ToolOutput/ToolError import-path + ToolOutput.result-shape confirms (T1 Step 1) are flagged-against-recon, not TODOs. ✓
**Type consistency:** `InstallPluginTool::new(data_dir: PathBuf, db: Arc<Mutex<Connection>>)`; `execute -> Result<ToolOutput, ToolError>`; `requires_approval -> ApprovalRequirement::Always`; builder captures `ctx.app_state.{data_dir,db}` (mirrors SelfEvalTool). ✓
**Approval-gating:** `requires_approval` returns `Always` (verified not in SafetyPolicy.auto_approved_tools default — only read_file/grep/glob are). ✓
**Self-extension loop closed:** skill teaches scaffold→self-check→install_plugin→restart; tool finalizes with validation + DB row + restart message. ✓
**New-file safety:** T1+T2+T3 verify the 2 new files in `git show --stat`. ✓
