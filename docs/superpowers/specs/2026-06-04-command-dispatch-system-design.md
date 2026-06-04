# Command Dispatch System Design (Phase 1)

**Date:** 2026-06-04
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b plugin system. Foundation for plugin **command** contribution (the contribution trinity is tools ✓ / skills ✓ / commands ✗). This is **Phase 1 — the dispatch mechanism only**. Phase 2 (plugin command contribution + `list_commands` + frontend slash listing + example plugin command + E2E) is a separate follow-up slice.

## Problem

`AgentApi.Command { name, description, handler: CommandHandlerFn }` + `register_command` + `command(name)` lookup all exist (`agent/api/command.rs`, `agent/api/mod.rs`), but have **ZERO execution path** — `command()` is never called in production. Slash commands typed by the user (`/foo`) resolve ONLY to skills: `send_agent_message` (`tauri_commands.rs`) calls `extract_slash_command_name` → `resolve_slash_skill`, and on a hit injects the skill prompt as a `system` `agent_messages` row (created_at = now-1) so the LLM sees it next turn. A registered `Command`'s handler is never invoked. There is also no lifecycle gating for commands (tools have `disabled_tool_names`).

## Decision (approved 2026-06-04)

- **Execution semantics: inject result as LLM context** — a matched command's handler runs, and its (Ok) result is serialized to a `system` `agent_messages` row, exactly like the slash-skill flow. The LLM sees it next turn and uses/formats it. (Pure-action commands can return empty and rely on side effects.)
- **Lifecycle-gated** — a disabled plugin's commands do NOT dispatch (mirror `disabled_tool_names`).
- **Skill precedence** — `/foo` resolves skill first (existing `resolve_slash_skill`), then command only on a skill miss. Mirrors the existing order; a skill shadows a same-named command (acceptable edge case).
- **Phase 1 is the mechanism only** — no command registration (builtins/plugins), no `list_commands`, no frontend. The dispatch path is unit-tested via a test-registered command; it ships dormant (no command sources yet) by design, the foundation Phase 2 builds on. `/compact` stays the special hardcoded intercept (untouched).

## Design

### §1 `disabled_command_names` + `command_if_enabled` (agent/api/mod.rs)
Mirror the existing tool gating:
```rust
// free fn, mirrors disabled_tool_names
pub(crate) fn disabled_command_names(
    plugin_index: &HashMap<PluginId, PluginRegistrationSet>,
    enabled_map: &HashMap<String, bool>,
) -> HashSet<String> {
    plugin_index.iter()
        .filter(|(pid, _)| matches!(enabled_map.get(pid.as_str()), Some(false)))
        .flat_map(|(_, set)| set.commands.iter().cloned())
        .collect()
}
```
```rust
impl AgentApi {
    /// Look up a command by name, returning it only if it exists AND its owning
    /// plugin (if any) is not disabled. Builtins (no plugin_index entry) always
    /// pass. enabled_map is the live AppState.plugin_enabled snapshot.
    pub fn command_if_enabled(
        &self, name: &str, enabled_map: &HashMap<String, bool>,
    ) -> Option<Arc<Command>> {
        if disabled_command_names(&self.plugin_index, enabled_map).contains(name) {
            return None;
        }
        self.command(name).cloned()
    }
}
```
(`PluginRegistrationSet.commands: Vec<String>` already exists + is populated by registration. Builtins won't appear in plugin_index, so they're never gated.)

### §2 `resolve_slash_command` (tauri_commands.rs, beside `resolve_slash_skill`)
```rust
/// Resolve a `/<name> <args>` against the AgentApi command registry (after a
/// skill miss). Executes the command handler with `{ "args": <raw args str> }`
/// and serializes the Ok result into a system-message prompt (mirrors the
/// slash-skill injection). Disabled-plugin commands are skipped. Errors are
/// logged and do not inject (the message continues as a plain prompt).
async fn resolve_slash_command(state: &AppState, name: &str, args_raw: &str) -> Option<String> {
    let enabled_map = state.plugin_enabled.read().ok()?.clone(); // std RwLock snapshot
    let cmd = state.agent_api.command_if_enabled(name, &enabled_map)?;
    let args = serde_json::json!({ "args": args_raw });
    match (cmd.handler)(args).await {
        Ok(val) => {
            let body = if val.is_null() { String::new() } else {
                val.as_str().map(str::to_string).unwrap_or_else(|| val.to_string())
            };
            Some(format!("<command name=\"{}\">\n{}\n</command>", name, body))
        }
        Err(e) => { tracing::warn!(command = %name, error = %e, "slash command handler failed"); None }
    }
}
```
Plan pins: `state.agent_api` type (`Arc<AgentApi>`) + `state.plugin_enabled` (std RwLock) access in `tauri_commands` (the lifecycle slice added both to `AppState`); the exact arg-split (the raw remainder after `/name`).

### §3 Wire into `send_agent_message`
Where `slash_skill_prompt` is computed (~line 9640), after the skill lookup, add a command fallback on skill-miss and inject whichever resolved:
```rust
let slash_skill_prompt = if let Some(cmd_name) = extract_slash_command_name(&input.user_message) {
    let skill = resolve_slash_skill(&state, &input.session_id, &cmd_name).await;
    if skill.is_some() { skill }
    else {
        let args_raw = /* remainder of user_message after the /<cmd_name> token */;
        resolve_slash_command(&state, &cmd_name, args_raw).await
    }
} else { None };
```
The existing persistence block (injects `slash_skill_prompt` as a `system` row with `created_at = now-1`, bumps message_count by 2) is unchanged — it now also carries command results. (Variable can stay named `slash_skill_prompt` or be renamed `slash_prompt`; plan picks the lower-churn option.)

## Data flow (after Phase 1)

```
send_agent_message: extract /<name>
  → resolve_slash_skill (static/learned)  ── hit → inject skill prompt (existing)
  → (skill miss) resolve_slash_command:
       enabled_map snapshot → command_if_enabled(name) (skip if owning plugin disabled)
       → handler({args}).await → Ok → serialize → inject as <command> system row
  → LLM next turn sees the injected result
(No command sources registered yet in Phase 1 → path is dormant but tested.)
```

## Out of scope (Phase 2)

Registering commands (builtin registrar + plugin command contribution routing to the plugin's MCP `call_tool`); `list_commands` Tauri endpoint; frontend slash-popup command listing (`ComposerMentionController`); example-plugin command + E2E; structured arg parsing beyond a raw remainder string; migrating `/compact` into the registry (stays special); direct-to-user (non-LLM) command results.

## Error handling

`resolve_slash_command`: poisoned `plugin_enabled` lock → `None` (fail-safe: don't dispatch). Command not found / disabled-plugin → `None` (message continues as a plain prompt — identical to a skill miss). Handler `Err` → warn-log + `None` (no injection; the user's `/foo` still posts as a normal message). Handler panic isn't caught here (a handler should not panic; plugin handlers in Phase 2 run over MCP/RPC which isolates). No DB/migration changes.

## Testing

Unit tests at the AgentApi level (mirror `agent/api/tests.rs` which already registers commands):
1. `command_if_enabled` returns a registered builtin command (no plugin_index entry) regardless of enabled_map.
2. `command_if_enabled` returns a plugin-owned command when its plugin is enabled/absent-from-map, and `None` when the plugin is disabled (`{pid:false}`) — set up via `register_plugin(pid, set{commands:[name]})`.
3. `disabled_command_names` collects exactly the commands of disabled plugins.
4. A serialization unit test for the Ok→`<command>` text + null→empty (extract the body-format into a tiny pure fn if it eases testing).
(`resolve_slash_command` needs `AppState` (DB/registry) so it's integration-shaped — the gating + serialization are the unit-tested core; the `send_agent_message` wiring is a small mirror of the skill path, verified by build + a focused read.)
`cd src-tauri && cargo build` + `cargo test --lib agent::api` + broad dependent run; clippy clean.

## Scope / files

| File | Change |
|---|---|
| `agent/api/mod.rs` | `disabled_command_names` free fn + `AgentApi::command_if_enabled` + tests |
| `tauri_commands.rs` | `resolve_slash_command` + wire into `send_agent_message` (skill-miss fallback) |

## Risk

Low-med. Touches the hot `send_agent_message` send path, but the change is additive (a fallback on the existing skill-miss branch reusing the existing injection) and the dispatch is dormant until Phase 2 registers commands. Main risks: (1) **`send_agent_message` wiring** — must run command resolution ONLY on skill-miss + inject via the existing block without disturbing the skill path or message ordering (created_at = now-1, count bump); plan quotes the exact block. (2) **gating correctness** — builtins (no plugin_index entry) must never be gated; only disabled-plugin commands skipped (mirror disabled_tool_names; unit-tested). (3) **arg-split** — extract the remainder after `/<name>` safely (empty when no args). (4) poisoned-lock → None (fail-safe). (5) **AppState access** — `agent_api` + `plugin_enabled` must be reachable in `tauri_commands` (added by the lifecycle slice; plan confirms). Bisectable: gating+lookup (api) → resolve+wire (tauri). After Phase 1, the command dispatch mechanism is live + tested; Phase 2 plugs in actual command sources (plugins) + the frontend.
