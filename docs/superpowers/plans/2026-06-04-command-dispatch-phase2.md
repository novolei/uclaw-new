# Command Dispatch Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** Plugin commands end-to-end — a plugin's `contributes.commands` register real `Command`s whose handlers route to the plugin's MCP `call_tool`; `list_commands` + the frontend slash popup make them discoverable; the example plugin demonstrates it.

**Spec:** `docs/superpowers/specs/2026-06-04-command-dispatch-phase2-design.md`

---

## Pinned facts (verbatim)

- **`mcp_manager` is created at app.rs:596-599 (`let mcp_manager = Arc::new(RwLock::new(crate::mcp::McpManager::new(&data_dir)));`), BEFORE the agent_api/plugin block (app.rs:929 `let (agent_api, plugin_report) = { … PluginLifecycleOwner::new(plugins_root).connect_and_register(&mut api) … }`).** → NO boot reorder; just pass `mcp_manager.clone()` into `connect_and_register`.
- `plugins/lifecycle.rs`: `pub fn connect_and_register(&self, api: &mut AgentApi) -> PluginLifecycleReport` (line 44); calls `PluginRegistrar::register(api, &loaded)` (line ~62).
- `plugins/registration.rs`: `pub fn register(api: &mut AgentApi, loaded: &LoadedPlugin) -> Result<PluginRegistrationSummary, RegistrationError>` (line 54). Command placeholder: `for cmd_name in &contrib.commands { summary.commands_registered.push(cmd_name.clone()); }` (~94-97). `set.commands = summary.commands_registered.clone(); api.register_plugin(PluginId::new(...), set);` (~160-166) — KEEP (gating). Imports (1-17): `use std::sync::Arc; use crate::agent::api::tool::ToolDescriptor; use crate::agent::api::AgentApi; use crate::mcp::{McpServerConfig, TransportType}; use crate::plugins::discovery::LoadedPlugin; …`.
- `mcp/mod.rs`: `pub type SharedMcpManager = Arc<RwLock<McpManager>>;`. `pub async fn call_tool(&self, server_id: &str, tool_name: &str, arguments: serde_json::Value) -> Result<CallToolResult, McpError>`. `CallToolResult { content: Vec<ContentBlock>, is_error: bool }`. `ContentBlock` `#[serde(tag="type", rename_all="lowercase")] enum { Text { text: String }, Image{..}, Resource{..} }`. Text extraction: `content.iter().filter_map(|b| match b { ContentBlock::Text{text}=>Some(text.as_str()), _=>None }).collect::<Vec<_>>().join("\n")`.
- `agent/api/command.rs`: `pub type CommandHandlerFn = Arc<dyn Fn(serde_json::Value)->BoxFuture<'static,Result<serde_json::Value,String>> + Send + Sync>;` `Command { name, description, handler }`.
- `agent/api/mod.rs`: `register_command` (169), `command` (175), `commands: HashMap<String,Arc<Command>>` (70), `disabled_command_names` (55), `command_if_enabled` (305). Add `list_commands` accessor after 314.
- `tauri_commands.rs`: `list_plugins` (17793-17824) + `PluginInfo` (17785). `state.agent_api: Arc<AgentApi>`, `state.plugin_enabled`.
- `main.rs` generate_handler (953-955): `// Plugins (Pi-3b)\n  uclaw_core::tauri_commands::list_plugins,\n  uclaw_core::tauri_commands::set_plugin_enabled,` — add `list_commands` here.
- Frontend `ComposerMentionController.tsx`: `Row` union (75-78); `/` branch (119-140); `commitRow` (186-218, builtin arm inserts `/${name} `); `renderItem` switch (282-291); `BuiltinRow` (331-354); `matchBuiltinCommands` (41-47). `tauri-bridge.ts`: `listPlugins` (1262), `PluginInfo` import (86-90), `listInvocableSkills` (1322). `types.ts`: `PluginInfo` (885), `InvocableSkill` (925).
- Example: `examples/plugins/hello-uclaw/plugin.toml` `[contributes] mcp_servers=["hello-uclaw"] tools=["hello"]` (17-20); `server.mjs` `tools/call` case (39-101) reads `params?.arguments?.name`.
- No ComposerMentionController test exists (closest `composer-serialize.test.ts`). `cd ui && npx tsc --noEmit` + `npm test -- --run`.

---

## Task 1: Thread mcp_manager + register plugin commands routing to MCP

**Files:** Modify `app.rs`, `plugins/lifecycle.rs`, `plugins/registration.rs`

- [ ] **Step 1: registration.rs — add param + imports + handler**
Add to imports: `use crate::agent::api::command::{Command, CommandHandlerFn};` + `use crate::mcp::{ContentBlock, SharedMcpManager};` (extend the existing `use crate::mcp::{McpServerConfig, TransportType};`).
Change signature: `pub fn register(api: &mut AgentApi, loaded: &LoadedPlugin, mcp_manager: &SharedMcpManager) -> Result<…>`.
Replace the command placeholder block with:
```rust
        // Commands — register a Command whose handler routes to the plugin's MCP
        // server via call_tool (command name == an MCP tool/method the plugin
        // handles). Captures the mcp_manager Arc; by call time it is connected.
        for cmd_name in &contrib.commands {
            let mgr = mcp_manager.clone();
            let pid = loaded.manifest.id.clone();
            let cname = cmd_name.clone();
            let handler: CommandHandlerFn = Arc::new(move |args: serde_json::Value| {
                let mgr = mgr.clone();
                let pid = pid.clone();
                let cname = cname.clone();
                Box::pin(async move {
                    let res = mgr
                        .read()
                        .await
                        .call_tool(&pid, &cname, args)
                        .await
                        .map_err(|e| format!("plugin command '{cname}' failed: {e}"))?;
                    if res.is_error {
                        return Err(format!("plugin command '{cname}' returned an error"));
                    }
                    let text = res
                        .content
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(serde_json::Value::String(text))
                })
            });
            api.register_command(Command {
                name: cmd_name.clone(),
                description: format!("Command from plugin {}", loaded.manifest.id),
                handler,
            });
            summary.commands_registered.push(cmd_name.clone());
        }
```
(The `set.commands = summary.commands_registered.clone()` + `register_plugin` block stays as-is — gating intact.)

- [ ] **Step 2: lifecycle.rs — thread the param**
`connect_and_register(&self, api: &mut AgentApi, mcp_manager: SharedMcpManager) -> PluginLifecycleReport` (add `use crate::mcp::SharedMcpManager;` if needed). Change the call: `PluginRegistrar::register(api, &loaded, &mcp_manager)`.

- [ ] **Step 3: app.rs — pass the clone**
At app.rs:929-931, change `.connect_and_register(&mut api)` → `.connect_and_register(&mut api, mcp_manager.clone())`. (mcp_manager already exists from line 597.)

- [ ] **Step 4: fix existing `register()` / `connect_and_register()` call sites in tests**
Build will flag every call. In `plugins/registration.rs` tests + `plugins/lifecycle.rs` tests (+ `plugins/tests.rs`), pass a test manager: `let mgr = std::sync::Arc::new(tokio::sync::RwLock::new(crate::mcp::McpManager::new(tmp.path())));` then `PluginRegistrar::register(&mut api, &loaded, &mgr)` / `.connect_and_register(&mut api, mgr.clone())`. (Match the existing test fixture style; `McpManager::new` takes a `&Path`.)

- [ ] **Step 5: registration test — a plugin command is registered + gated**
Add to registration.rs tests: a `LoadedPlugin` fixture with `contributes.commands = ["greet"]` (+ run_subprocess + executable so it's a valid plugin), register with a test mgr, assert `api.command("greet").is_some()` AND `api.plugin_index` has the plugin with `commands` containing "greet" (gating intact). (Reuse the existing fixture builder; if it doesn't set `commands`, build the LoadedPlugin manifest directly as the sandbox-policy test did.)

- [ ] **Step 6: build + test + commit**
`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty (signature change ripples — fix all call sites). `cargo test --lib plugins agent::api 2>&1 | tail` → green.
```bash
git add src-tauri/src/plugins/registration.rs src-tauri/src/plugins/lifecycle.rs src-tauri/src/app.rs
git commit -m "feat(plugins): register plugin commands routing to MCP call_tool (thread mcp_manager into registration)"
```

---

## Task 2: `list_commands` accessor + Tauri command

**Files:** Modify `agent/api/mod.rs`, `tauri_commands.rs`, `main.rs`

- [ ] **Step 1: AgentApi accessor** — after `command_if_enabled` (~line 314):
```rust
    /// Pi-3b — list registered commands as (name, description), excluding those
    /// owned by a disabled plugin. Builtins (no plugin_index entry) always listed.
    pub fn list_commands(
        &self,
        enabled_map: &std::collections::HashMap<String, bool>,
    ) -> Vec<(String, String)> {
        let disabled = disabled_command_names(&self.plugin_index, enabled_map);
        let mut out: Vec<(String, String)> = self
            .commands
            .values()
            .filter(|c| !disabled.contains(&c.name))
            .map(|c| (c.name.clone(), c.description.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0)); // deterministic order
        out
    }
```

- [ ] **Step 2: test** — in `agent/api/tests.rs`:
```rust
#[test]
fn list_commands_excludes_disabled_plugin_commands() {
    use futures::FutureExt;
    use crate::agent::api::plugin::{PluginId, PluginRegistrationSet};
    let mut api = AgentApi::new();
    api.register_command(crate::agent::api::command::Command {
        name: "builtinc".into(), description: "b".into(),
        handler: std::sync::Arc::new(|_a| async move { Ok(serde_json::json!({})) }.boxed()),
    });
    api.register_command(crate::agent::api::command::Command {
        name: "pcmd".into(), description: "p".into(),
        handler: std::sync::Arc::new(|_a| async move { Ok(serde_json::json!({})) }.boxed()),
    });
    let mut set = PluginRegistrationSet::default();
    set.commands.push("pcmd".into());
    api.register_plugin(PluginId::new("p1"), set);
    // enabled (absent) → both
    let names: Vec<String> = api.list_commands(&std::collections::HashMap::new()).into_iter().map(|(n,_)| n).collect();
    assert!(names.contains(&"builtinc".to_string()) && names.contains(&"pcmd".to_string()));
    // disabled → pcmd gone, builtin kept
    let disabled = std::collections::HashMap::from([("p1".to_string(), false)]);
    let names2: Vec<String> = api.list_commands(&disabled).into_iter().map(|(n,_)| n).collect();
    assert!(names2.contains(&"builtinc".to_string()) && !names2.contains(&"pcmd".to_string()));
}
```

- [ ] **Step 3: Tauri command** — in `tauri_commands.rs`, near `list_plugins`:
```rust
/// Pi-3b — list invocable slash commands (name + description), minus disabled-plugin ones.
#[derive(serde::Serialize)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
}

#[tauri::command]
pub async fn list_commands(state: State<'_, AppState>) -> Result<Vec<CommandInfo>, Error> {
    let enabled_map = state.plugin_enabled.read().map(|m| m.clone()).unwrap_or_default();
    Ok(state
        .agent_api
        .list_commands(&enabled_map)
        .into_iter()
        .map(|(name, description)| CommandInfo { name, description })
        .collect())
}
```

- [ ] **Step 4: register in main.rs** — after `set_plugin_enabled,` (line ~955): `            uclaw_core::tauri_commands::list_commands,`

- [ ] **Step 5: build + test + commit**
`cargo build 2>&1 | grep -E "^error"` → empty. `cargo test --lib agent::api 2>&1 | tail -4` → green.
```bash
git add src-tauri/src/agent/api/mod.rs src-tauri/src/agent/api/tests.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(agent): list_commands accessor + Tauri command (gated command listing for the slash popup)"
```

---

## Task 3: Frontend slash-popup command listing

**Files:** Modify `ui/src/lib/types.ts`, `ui/src/lib/tauri-bridge.ts`, `ui/src/components/composer/ComposerMentionController.tsx`

- [ ] **Step 1: type** — `types.ts` (near InvocableSkill): `export interface CommandInfo { name: string; description: string }`.
- [ ] **Step 2: binding** — `tauri-bridge.ts`: add `CommandInfo` to the types import block (where `PluginInfo` is), and `export const listCommands = (): Promise<CommandInfo[]> => invoke('list_commands')`.
- [ ] **Step 3: ComposerMentionController**
  - `Row` union (75-78): add `| { kind: 'command'; data: CommandInfo }`.
  - import `listCommands` + `CommandInfo`.
  - `/` branch (119-140): after `listInvocableSkills`, `const commands = await listCommands()`; filter by query (`q ? commands.filter(c => c.name.toLowerCase().includes(q) || c.description.toLowerCase().includes(q)) : commands`); insert into rows BETWEEN builtins and skills: `[...builtins…, ...cmds.map(c => ({ kind: 'command' as const, data: c })), ...filtered skills…]`.
  - `commitRow` (186-218): add an arm BEFORE the chip path: `if (row.kind === 'command') { ed.chain().focus().deleteRange({from:trigger.triggerStart,to:trigger.cursorPos}).insertContent(\`/${row.data.name} \`).run(); return }` (same as builtin).
  - `renderItem` switch (282-291): add `r.kind === 'command' ? <CommandRow cmd={r.data} isSelected={isSelected} /> : …`.
  - Add `CommandRow` component (mirror `BuiltinRow`, ~331): icon (reuse `Layers` or another lucide e.g. `TerminalSquare`), `/{cmd.name}`, a `插件` badge (distinguish from `内置`), `cmd.description`.
- [ ] **Step 4: tsc + test + commit**
`cd ui && npx tsc --noEmit 2>&1 | grep -iE "ComposerMention|tauri-bridge|types\.ts|CommandInfo" | head` → no NEW errors (pre-existing unrelated errors ignored). `cd ui && npm test -- --run 2>&1 | tail -6` → no new failures.
```bash
git add ui/src/lib/types.ts ui/src/lib/tauri-bridge.ts ui/src/components/composer/ComposerMentionController.tsx
git commit -m "feat(ui): list plugin commands in the slash popup (listCommands + command Row kind)"
```

---

## Task 4: Example plugin command + E2E soak + ship

**Files:** Modify `examples/plugins/hello-uclaw/plugin.toml`, `examples/plugins/hello-uclaw/server.mjs`

- [ ] **Step 1: manifest** — add `commands = ["greet"]` to `[contributes]`.
- [ ] **Step 2: server** — in `server.mjs` `tools/call`, handle `name === "greet"` (in addition to `hello`): return `content: [{ type: "text", text: \`Greetings, ${params?.arguments?.args ?? "friend"}!\` }]`. (Keep the `hello` tool; add `greet` so the command routes.) Optionally list `greet` in `tools/list` too (not required for command routing).
- [ ] **Step 3: build + whole-slice verify**
`cd src-tauri && cargo build 2>&1 | grep -E "^error"` empty; `cargo clippy --lib` clean; `cargo test --lib plugins agent::api` green. `cd ui && npx tsc --noEmit` (no new) + `npm test -- --run` (no new failures).
- [ ] **Step 4: macOS E2E soak (manual, document in PR)** — install/symlink the hello-uclaw example into the plugins dir, run the app: confirm `/greet` appears in the slash popup (插件 badge), sending `/greet world` routes to the plugin MCP, and a `<command name="greet">Greetings, world!</command>` system message is injected (LLM responds). If running the full app isn't feasible here, at minimum a shell-level check that the example server replies to a `tools/call` for `greet`, and rely on the registration + dispatch unit tests. Report what was validated.
- [ ] **Step 5**: grep gates — registration registers commands (register_command in the contrib.commands loop); set.commands kept; list_commands in main.rs handler; frontend Row union has 'command' in all match sites.
- [ ] **Step 6**: PR with `## Commits (bisectable)` table. Note: command→MCP call_tool routing (name==tool name); mcp_manager threaded (no boot reorder, created at app.rs:597); result injected via Phase-1 path; gated; example demonstrates command-only method (greet not in contributes.tools).
- [ ] **Step 7**: rebase onto latest origin/main, rebase-merge, sync main, cleanup, reindex, update memory ([[project-pi-lightweight-vs-agent-os]]: command dispatch Phase 2 shipped → contribution trinity complete; next 3b = install-from-registry/marketplace, sandbox v2, UI v2).

---

## Self-Review

**Spec coverage:** §1 thread → T1; §2 register → T1; §3 list_commands → T2; §4 frontend → T3; §5 example/E2E → T4. ✓
**Placeholder scan:** the test fixture-builder note (T1 Step 5) + the E2E soak fallback (T4 Step 4) are flagged-with-fallback, not TODOs. ✓
**Type consistency:** handler is `CommandHandlerFn`; `call_tool(server_id, tool_name, args)`; `ContentBlock::Text{text}`; `list_commands -> Vec<(String,String)>` → mapped to `CommandInfo`; frontend `CommandInfo {name,description}` matches the Tauri struct (snake-free — both fields already camelless). ✓
**No boot reorder:** mcp_manager exists at app.rs:597 before the 929 block — just `.clone()` threaded. ✓
**Gating preserved:** set.commands + register_plugin unchanged; list_commands + command_if_enabled both use disabled_command_names. ✓
**Signature ripple:** `register`/`connect_and_register` gain a param → ALL call sites (lifecycle, tests) fixed in T1 (compiler-enforced). ✓
