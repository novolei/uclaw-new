# Command Dispatch System (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** Wire the dormant `AgentApi.Command` registry into the slash-command send path: on a `/foo` skill-miss, look up + execute a (lifecycle-gated) command and inject its result as a system message (mirror the slash-skill flow). Phase 1 = mechanism only (no command sources yet).

**Spec:** `docs/superpowers/specs/2026-06-04-command-dispatch-system-design.md`

---

## Pinned facts (verbatim)

- `agent/api/mod.rs`: imports `use std::collections::HashMap; use std::sync::Arc; use self::command::Command; use self::plugin::{PluginId, PluginRegistrationSet};`. The `disabled_tool_names` free fn is at module level (~line 42, BEFORE `impl AgentApi`). `AgentApi { …, commands: HashMap<String, Arc<Command>>, plugin_index: HashMap<PluginId, PluginRegistrationSet> }`. `register_command`/`command(name) -> Option<&Arc<Command>>`/`register_plugin(id,set)` exist; insert new method after `register_plugin` (~line 287).
- `Command` is `#[derive(Clone)]`; `command(name).cloned()` → `Arc<Command>`. `CommandHandlerFn = Arc<dyn Fn(serde_json::Value) -> BoxFuture<'static, Result<serde_json::Value, String>> + Send + Sync>`.
- `PluginId`: `new(impl Into<String>)`, `as_str()`. `PluginRegistrationSet::default()` + `.commands: Vec<String>`.
- `agent/api/tests.rs`: tests use `AgentApi::new()`; build a Command with `handler: std::sync::Arc::new(|_args| async move { Ok(serde_json::json!({})) }.boxed())` + `use futures::FutureExt;`; plugin attribution via `let mut set = PluginRegistrationSet::default(); set.commands.push("x".into()); api.register_plugin(PluginId::new("p1"), set);`. `api.commands` / `api.plugin_index` are `pub(crate)` (same-crate tests read them).
- `tauri_commands.rs`: `extract_slash_command_name(msg)->Option<String>` (7343, filters `compact`); `async fn resolve_slash_skill(state: &AppState, session_id: &str, name: &str) -> Option<String>` (7361, ends ~7430; `InvocableSkill` struct starts 7435 — insert `resolve_slash_command` between). `send_agent_message` slash block computes `let slash_skill_prompt: Option<String> = if let Some(cmd_name)=extract_slash_command_name(&input.user_message){ resolve_slash_skill(&state,&input.session_id,&cmd_name).await } else { None };` (~9640-9660). Persistence block (~9684-9707) injects `slash_skill_prompt` as a `system` `agent_messages` row (created_at = now-1) + user row + `message_count + (2 if Some else 1)` — UNCHANGED.
- `AppState`: `pub agent_api: Arc<crate::agent::api::AgentApi>` + `pub plugin_enabled: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String,bool>>>`. Reachable as `state.agent_api` / `state.plugin_enabled`.
- `tauri_commands.rs` uses `serde_json::json!` + `tracing::*` fully-qualified (no extra import needed). `input.user_message` is the full text.

---

## Task 1: `disabled_command_names` + `command_if_enabled` + tests (agent/api)

**Files:** Modify `agent/api/mod.rs`, `agent/api/tests.rs`

- [ ] **Step 1: free fn `disabled_command_names`** — in `agent/api/mod.rs`, right after the `disabled_tool_names` fn (module level):
```rust
/// Pi-3b — command names owned by currently-disabled plugins (skip at dispatch).
/// Builtins (no plugin_index entry) are never included. Mirrors disabled_tool_names.
pub(crate) fn disabled_command_names(
    plugin_index: &std::collections::HashMap<PluginId, PluginRegistrationSet>,
    enabled_map: &std::collections::HashMap<String, bool>,
) -> std::collections::HashSet<String> {
    plugin_index
        .iter()
        .filter(|(pid, _)| matches!(enabled_map.get(pid.as_str()), Some(false)))
        .flat_map(|(_, set)| set.commands.iter().cloned())
        .collect()
}
```

- [ ] **Step 2: method `command_if_enabled`** — in `impl AgentApi`, after `register_plugin` (~line 287):
```rust
    /// Pi-3b — look up a command by name, returning it only if it exists AND its
    /// owning plugin (if any) is not disabled. Builtins always pass. `enabled_map`
    /// is the live AppState.plugin_enabled snapshot.
    pub fn command_if_enabled(
        &self,
        name: &str,
        enabled_map: &std::collections::HashMap<String, bool>,
    ) -> Option<std::sync::Arc<Command>> {
        if disabled_command_names(&self.plugin_index, enabled_map).contains(name) {
            return None;
        }
        self.command(name).cloned()
    }
```

- [ ] **Step 3: tests** — in `agent/api/tests.rs`:
```rust
#[test]
fn command_if_enabled_returns_builtin_regardless_of_map() {
    use futures::FutureExt;
    let mut api = AgentApi::new();
    api.register_command(crate::agent::api::command::Command {
        name: "ping".into(),
        description: String::new(),
        handler: std::sync::Arc::new(|_a| async move { Ok(serde_json::json!({})) }.boxed()),
    });
    assert!(api.command_if_enabled("ping", &std::collections::HashMap::new()).is_some());
    let m = std::collections::HashMap::from([("other".to_string(), false)]);
    assert!(api.command_if_enabled("ping", &m).is_some()); // unrelated disabled plugin: still ok
    assert!(api.command_if_enabled("missing", &std::collections::HashMap::new()).is_none());
}

#[test]
fn command_if_enabled_gates_disabled_plugin_command() {
    use futures::FutureExt;
    use crate::agent::api::plugin::{PluginId, PluginRegistrationSet};
    let mut api = AgentApi::new();
    api.register_command(crate::agent::api::command::Command {
        name: "pcmd".into(),
        description: String::new(),
        handler: std::sync::Arc::new(|_a| async move { Ok(serde_json::json!({})) }.boxed()),
    });
    let mut set = PluginRegistrationSet::default();
    set.commands.push("pcmd".into());
    api.register_plugin(PluginId::new("p1"), set);
    assert!(api.command_if_enabled("pcmd", &std::collections::HashMap::new()).is_some()); // enabled (absent)
    let disabled = std::collections::HashMap::from([("p1".to_string(), false)]);
    assert!(api.command_if_enabled("pcmd", &disabled).is_none()); // disabled → gated
}

#[test]
fn disabled_command_names_collects_disabled_plugin_commands() {
    use crate::agent::api::plugin::{PluginId, PluginRegistrationSet};
    let mut api = AgentApi::new();
    let mut set = PluginRegistrationSet::default();
    set.commands.push("a".into());
    set.commands.push("b".into());
    api.register_plugin(PluginId::new("p1"), set);
    let disabled = std::collections::HashMap::from([("p1".to_string(), false)]);
    let names = crate::agent::api::disabled_command_names(&api.plugin_index, &disabled);
    assert!(names.contains("a") && names.contains("b"));
    let enabled = std::collections::HashMap::from([("p1".to_string(), true)]);
    assert!(crate::agent::api::disabled_command_names(&api.plugin_index, &enabled).is_empty());
}
```
(If `disabled_command_names` isn't visible as `crate::agent::api::disabled_command_names` from tests, reference it via the module path used for `disabled_tool_names` in existing tests — match that. It's `pub(crate)`.)

- [ ] **Step 4: run + commit**
`cd src-tauri && cargo test --lib agent::api 2>&1 | tail -12` → green. `cargo build 2>&1 | grep -E "^error"` → empty.
```bash
git add src-tauri/src/agent/api/mod.rs src-tauri/src/agent/api/tests.rs
git commit -m "feat(agent): disabled_command_names + command_if_enabled — lifecycle-gated command lookup (Pi-3b command dispatch)"
```

---

## Task 2: `resolve_slash_command` + wire into `send_agent_message`

**Files:** Modify `tauri_commands.rs`

- [ ] **Step 1: `resolve_slash_command`** — insert AFTER `resolve_slash_skill` (before the `InvocableSkill` struct, ~line 7433):
```rust
/// Pi-3b — resolve a `/<name> <args>` against the AgentApi command registry,
/// called AFTER a skill miss. Executes the handler with `{ "args": <raw
/// remainder> }` and serializes the Ok result into a system-message prompt
/// (mirrors the slash-skill injection). Disabled-plugin commands are skipped;
/// handler errors are logged and inject nothing (the message stays a plain prompt).
async fn resolve_slash_command(state: &AppState, name: &str, args_raw: &str) -> Option<String> {
    let enabled_map = match state.plugin_enabled.read() {
        Ok(m) => m.clone(),
        Err(_) => return None, // poisoned → fail-safe: don't dispatch
    };
    let cmd = state.agent_api.command_if_enabled(name, &enabled_map)?;
    let args = serde_json::json!({ "args": args_raw });
    match (cmd.handler)(args).await {
        Ok(val) => {
            let body = if val.is_null() {
                String::new()
            } else {
                val.as_str().map(|s| s.to_string()).unwrap_or_else(|| val.to_string())
            };
            tracing::info!(command = %name, "slash command: executed registered command");
            Some(format!("<command name=\"{}\">\n{}\n</command>", name, body))
        }
        Err(e) => {
            tracing::warn!(command = %name, error = %e, "slash command handler failed");
            None
        }
    }
}
```

- [ ] **Step 2: wire the skill-miss fallback** — replace the `slash_skill_prompt` computation block (~9655-9660):
```rust
    let slash_skill_prompt: Option<String> = if let Some(cmd_name) =
        extract_slash_command_name(&input.user_message)
    {
        match resolve_slash_skill(&state, &input.session_id, &cmd_name).await {
            Some(skill) => Some(skill),
            None => {
                // Pi-3b — skill miss → try the command registry. The remainder
                // after `/<name>` becomes the raw args string.
                let args_raw = input
                    .user_message
                    .trim_start()
                    .strip_prefix('/')
                    .map(|rest| rest.split_whitespace().skip(1).collect::<Vec<_>>().join(" "))
                    .unwrap_or_default();
                resolve_slash_command(&state, &cmd_name, &args_raw).await
            }
        }
    } else {
        None
    };
```
The persistence block (injects `slash_skill_prompt` as a `system` row + user row + count bump) is UNCHANGED — it now carries command results too.

- [ ] **Step 3: build + verify + commit**
`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty. `cargo test --lib agent::api 2>&1 | tail -5` → green (no regressions). `cargo clippy --lib 2>&1 | grep -iE "tauri_commands|agent/api" | grep -iE "warning|error"` → no new.
```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "feat(agent): dispatch /slash commands via AgentApi registry on skill-miss (inject result as system msg)"
```

---

## Task 3: Whole-slice verification + ship

- [ ] **Step 1**: `cargo build` + `cargo clippy --lib` clean (no new warnings in agent/api, tauri_commands).
- [ ] **Step 2**: tests — `agent::api` + broad dependent run (`mcp`, `plugins`). Green.
- [ ] **Step 3**: grep gates — `disabled_command_names` + `command_if_enabled` present; `resolve_slash_command` called ONLY on skill-miss in `send_agent_message`; persistence block unchanged (still keys off `slash_skill_prompt`); builtins not gated (disabled_command_names only collects plugin_index commands).
- [ ] **Step 4**: PR with `## Commits (bisectable)` table. Note: Phase 1 = dispatch mechanism, dormant until Phase 2 registers commands; result injected as system msg (mirror skills); skill precedence; lifecycle-gated; `/compact` untouched.
- [ ] **Step 5**: rebase onto latest origin/main, rebase-merge, sync main, cleanup worktree+branch, reindex, update memory ([[project-pi-lightweight-vs-agent-os]]: command dispatch Phase 1 shipped; Phase 2 = plugin command contribution + list_commands + frontend + example/E2E).

---

## Self-Review

**Spec coverage:** §1 gating → T1; §2 resolve → T2; §3 wire → T2. ✓
**Placeholder scan:** the `disabled_command_names` test-visibility note is a flagged fallback (match existing `disabled_tool_names` test access), not a TODO. ✓
**Type consistency:** `command_if_enabled -> Option<Arc<Command>>` (Command is Clone, `command().cloned()`); `enabled_map: HashMap<String,bool>` (matches AppState.plugin_enabled inner); `disabled_command_names` signature mirrors `disabled_tool_names`. ✓
**Skill-path untouched:** the skill branch + persistence block are byte-identical except the `None` arm now falls back to commands; injection keyed off `slash_skill_prompt` unchanged. ✓
**Gating correctness:** builtins (no plugin_index entry) never in `disabled_command_names` → never gated; only disabled-plugin commands skipped (unit-tested). ✓
**Dormant-but-tested:** no command sources registered in Phase 1; gating+lookup unit-tested at the api level; wiring is a small mirror of the skill path. ✓
