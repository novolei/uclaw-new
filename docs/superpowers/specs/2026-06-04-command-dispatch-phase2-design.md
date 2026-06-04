# Command Dispatch Phase 2 — Plugin Command Contribution Design

**Date:** 2026-06-04
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b. Builds on command dispatch Phase 1 (PR #673 — the dispatch mechanism: `resolve_slash_command` → `command_if_enabled` → run handler → inject result). Phase 2 makes plugin commands real end-to-end: a plugin declares `commands`, the user types `/cmd`, it routes to the plugin's MCP server and the result is injected. Completes the contribution trinity (tools ✓ / skills ✓ / commands ✓).

## Problem

Phase 1 wired the dispatch path but registers NO commands — `plugins/registration.rs` still only does `summary.commands_registered.push(cmd_name)` (placeholder; never `register_command`). So `command_if_enabled` finds nothing, the path is dormant. There is also no `list_commands` endpoint and the frontend slash popup (`ComposerMentionController`) lists only `/compact` + skills — commands are invisible.

## Decision (approved 2026-06-04)

- **Plugin command → MCP `call_tool` routing**: a plugin's `contributes.commands: ["greet"]` registers a real `Command` whose handler calls the plugin's MCP server `call_tool(plugin_id, "greet", { args })`. The command name equals an MCP tool/method name the plugin server handles (it need NOT also be in `contributes.tools` — command-only methods are allowed).
- **Thread `SharedMcpManager` into registration** (not defer): the `mcp_manager` Arc is created BEFORE plugin registration in boot, threaded through `connect_and_register` → `PluginRegistrar::register`, and captured by the command handler closure. By call time (runtime) the manager is fully connected. Keeps the dispatch path uniform with Phase 1 (always: `command_if_enabled` → run handler).
- **`list_commands` Tauri endpoint** + **frontend slash listing** make commands discoverable.
- **Result**: `CallToolResult.content` text blocks → serialized to the handler's `Value`; `is_error` → `Err` (no injection). Phase 1's `resolve_slash_command` wraps Ok in `<command>` + injects as a system message (unchanged).

## Design

### §1 Thread `SharedMcpManager` to registration (app.rs + lifecycle.rs + registration.rs)
- **app.rs boot**: ensure the `mcp_manager: SharedMcpManager` Arc is constructed BEFORE the `agent_api`/plugin-registration block (plan pins the current creation line; if it's after, move the `Arc::new(RwLock::new(McpManager::new(data_dir)))` up — the empty manager has no deps; `add_server` population stays where it is). Pass `mcp_manager.clone()` into `connect_and_register`.
- **lifecycle.rs**: `PluginLifecycleOwner::connect_and_register(&self, api: &mut AgentApi, mcp_manager: SharedMcpManager)` — thread it to each `PluginRegistrar::register(api, &loaded, &mcp_manager)`.
- **registration.rs**: `PluginRegistrar::register(api, loaded, mcp_manager: &SharedMcpManager)`.

### §2 Register plugin commands routing to MCP (registration.rs)
Replace the command placeholder. For each `cmd_name in &contrib.commands`:
```rust
let mgr = mcp_manager.clone();
let pid = loaded.manifest.id.clone();
let cname = cmd_name.clone();
let handler: CommandHandlerFn = Arc::new(move |args: serde_json::Value| {
    let mgr = mgr.clone(); let pid = pid.clone(); let cname = cname.clone();
    Box::pin(async move {
        let res = mgr.read().await.call_tool(&pid, &cname, args).await
            .map_err(|e| format!("plugin command '{cname}' failed: {e}"))?;
        if res.is_error {
            return Err(format!("plugin command '{cname}' returned an error"));
        }
        // Serialize text content blocks → a JSON string the dispatcher injects.
        let text = /* join text ContentBlocks (plan pins ContentBlock shape) */;
        Ok(serde_json::Value::String(text))
    })
});
api.register_command(Command { name: cname_for_struct, description: format!("Command from plugin {pid_str}"), handler });
summary.commands_registered.push(cmd_name.clone()); // keep — feeds plugin_index gating (set.commands)
```
The existing `set.commands = summary.commands_registered.clone()` + `register_plugin` (plugin_index attribution for `disabled_command_names` gating) is unchanged.

### §3 `list_commands` Tauri endpoint (tauri_commands.rs + main.rs)
Mirror `list_plugins`:
```rust
#[derive(serde::Serialize)]
pub struct CommandInfo { pub name: String, pub description: String }

#[tauri::command]
pub async fn list_commands(state: State<'_, AppState>) -> Result<Vec<CommandInfo>, Error> {
    let enabled_map = state.plugin_enabled.read().map(|m| m.clone()).unwrap_or_default();
    Ok(state.agent_api.list_commands(&enabled_map)) // new AgentApi accessor: name+desc, minus disabled-plugin
}
```
- **AgentApi accessor** `list_commands(&self, enabled_map) -> Vec<CommandInfo-ish>`: iterate `self.commands`, skip names in `disabled_command_names(&self.plugin_index, enabled_map)`, return `(name, description)` pairs. (Return a plain `Vec<(String,String)>` or a small struct; tauri_commands maps to `CommandInfo`. Plan picks the cleaner split to avoid AgentApi depending on the Tauri type.)
- Register `list_commands` in `main.rs` `generate_handler!`.

### §4 Frontend slash listing (ComposerMentionController.tsx + tauri-bridge.ts + types.ts)
- `tauri-bridge.ts`: `export const listCommands = (): Promise<CommandInfo[]> => invoke('list_commands')`.
- `types.ts`: `export interface CommandInfo { name: string; description: string }`.
- `ComposerMentionController.tsx`: add `{ kind: 'command'; data: CommandInfo }` to the `Row` union; in the `/` trigger branch, `const commands = await listCommands()` (filter by query), build rows as `builtins → commands → skills`; add a render branch for `kind === 'command'` (mirror the builtin/skill row); `commitRow` for a command inserts `/${name} ` (same as builtin — the dispatch is backend on send).

### §5 Example plugin command + E2E (examples/plugins/hello-uclaw)
- `plugin.toml`: add `commands = ["greet"]` to `[contributes]`.
- `server.mjs`: handle `tools/call` for `name === "greet"` (return a text content block, e.g. greet using `arguments.args`).
- E2E soak: type `/greet …` → popup lists 插件命令 greet → on send, `resolve_slash_command` routes to the plugin MCP → result injected as `<command>` system message → LLM responds.

## Data flow (after Phase 2)

```
boot: mcp_manager Arc created → connect_and_register(api, mcp_manager)
      → register: for each contrib.command → register_command(handler → call_tool(pid,name,args)) + plugin_index.commands
runtime /greet x:
  send_agent_message → extract /greet → skill miss → resolve_slash_command (Phase 1)
    → command_if_enabled("greet", enabled_map)  (gated)
    → handler(args={args:"x"}) → mcp call_tool(plugin_id,"greet",{args:"x"}) → CallToolResult
    → text → <command name="greet">…</command> system msg → LLM next turn
frontend: /  → popup builtins + listCommands() + skills
```

## Out of scope

Structured arg parsing (still `{ "args": "<raw remainder>" }`); a dedicated `command/invoke` plugin protocol (reuse `call_tool`); built-in (non-plugin) commands beyond the registrar seam; direct-to-user (non-LLM) results; command argument schemas/autocomplete; per-command permissions beyond the plugin enable/disable gate; lock-free call (use `call_tool` which holds the read lock during the await — acceptable for infrequent user-initiated commands; optimize later).

## Error handling

`call_tool` Err (server not found/not connected/transport) → handler returns `Err(String)` → `resolve_slash_command` logs + injects nothing (the `/greet` posts as a plain message — same as a skill/command miss). `is_error` result → `Err`. Disabled plugin → `command_if_enabled` returns None (Phase 1, gated) → not dispatched + not listed. Poisoned `plugin_enabled` → fail-safe (Phase 1). `list_commands` with no commands → `[]`. Boot: if `mcp_manager` reorder is needed, it's a pure move of an empty-manager construction (no behavior change).

## Testing

1. **registration**: a `LoadedPlugin` with `contributes.commands=["greet"]` → after `register(api, loaded, &mgr)`, `api.command("greet")` is Some AND `plugin_index[pid].commands` contains "greet" (gating intact). (Mirror the existing registration tests; the handler need not be invoked in the unit test — assert it's registered.)
2. **list_commands accessor**: AgentApi with a builtin command + a plugin command (plugin enabled) → both listed; plugin disabled → plugin command omitted, builtin kept.
3. **handler routing** (if feasible without a live MCP): a unit test can assert the handler is built; the actual call_tool routing is covered by the E2E soak (live example plugin).
4. **frontend**: `ComposerMentionController` lists a mocked `listCommands` result under `/` (mirror any existing popup test) + `tsc --noEmit` clean.
5. **E2E soak** (manual, document): the hello-uclaw example with `commands=["greet"]` → `/greet` appears in the popup, sends, routes to the plugin MCP, injects a result.
`cargo build`/clippy + `cargo test --lib plugins agent::api` + `cd ui && npx tsc --noEmit` + vitest.

## Scope / files

| File | Change |
|---|---|
| `app.rs` | ensure mcp_manager Arc before plugin reg; pass to `connect_and_register` |
| `plugins/lifecycle.rs` | thread `SharedMcpManager` through `connect_and_register` → `register` |
| `plugins/registration.rs` | build command handlers (call_tool routing) + `register_command`; keep set.commands |
| `agent/api/mod.rs` | `list_commands(&self, enabled_map)` accessor (name+desc minus disabled-plugin) |
| `tauri_commands.rs` + `main.rs` | `list_commands` command + `CommandInfo` + invoke_handler |
| `ui/.../ComposerMentionController.tsx`, `lib/tauri-bridge.ts`, `lib/types.ts` | `listCommands` + `CommandInfo` + `command` Row kind + fetch/render |
| `examples/plugins/hello-uclaw/{plugin.toml,server.mjs}` | `commands=["greet"]` + server `greet` handler |

## Risk

Med. Touches boot wiring (mcp_manager threading — the main risk), plugin registration, a hot-ish path (command handler over MCP), a new Tauri command, frontend, and the example. Main risks: (1) **boot ordering** — mcp_manager Arc must exist before `connect_and_register`; moving its creation up must not break anything constructed between (plan verifies what's in the gap). (2) **handler captures the Arc, not a populated manager** — fine, populated by call time; but if a command is invoked before MCP connects, `call_tool` returns "not connected" → Err → no injection (graceful). (3) **gating preserved** — keep `set.commands` + `register_plugin` so `disabled_command_names` still gates (Phase 1 tests + a new registration test). (4) **read-lock during call_tool await** — acceptable v1 (note it). (5) **two-edit Tauri** — register `list_commands` in main.rs. (6) **frontend Row union exhaustiveness** — add the `command` branch everywhere the union is matched (tsc enforces). Bisectable: thread+register (T1) → list_commands (T2) → frontend (T3) → example+E2E (T4). After Phase 2, a plugin ships a command, the user sees + invokes it, and it runs against the plugin over MCP — the plugin contribution surface is complete (tools + skills + commands).
