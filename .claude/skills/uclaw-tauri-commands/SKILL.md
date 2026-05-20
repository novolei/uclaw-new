---
name: uclaw-tauri-commands
description: Use whenever you add or modify a Tauri IPC command in src-tauri/src/tauri_commands.rs. Trigger phrases include "Tauri command", "invoke", "IPC handler", "expose to frontend", "register command", "invoke_handler!", "#[tauri::command]", or any frontend ↔ backend bridge work. Loads the two-edit registration rule (forgetting either step compiles fine but fails at runtime), the DMZ review requirement, and the canonical command-shape template.
---

# uClaw — Tauri Commands

`src-tauri/src/tauri_commands.rs` is a **single flat module** that exposes
every IPC command. From the frontend, calls go through `@tauri-apps/api`
`invoke('command_name', { ... })`. Lower-level IPC types live in
`ui/src/lib/tauri-bridge.ts`.

## DMZ file warning

`tauri_commands.rs` is on the DMZ list (BEHAVIOR.md §"DMZ Files Need
Two-Session Review"). Every edit should have a Writer (you) and a Reviewer
(separate session or human) re-read the diff before merge. The advisory
`.claude/hooks/warn-dmz-edit.sh` fires on every edit to remind you.

## The two-edit rule — break this and runtime fails silently

Adding a new command requires **both**:

1. **Define it in `tauri_commands.rs`** (or in a sub-file imported by it).
2. **Register it in the `invoke_handler!` macro in `src-tauri/src/main.rs`.**

Forgetting step 2 **compiles cleanly** but the frontend gets
`Command "your_command" not found` at runtime. Always grep `main.rs` after
adding a command:

```bash
grep -n "your_command_name" src-tauri/src/main.rs
```

Both files must show the name.

## Procedure — adding a new command

1. **Decide the surface area.** Is this a one-off, or part of a feature
   that deserves its own sub-module? Match the existing shape — most
   commands sit directly in `tauri_commands.rs`; a few large features have
   their own sub-modules (e.g. `tauri_commands_browser.rs`).
2. **Write the command** with the standard signature:
   ```rust
   #[tauri::command]
   pub async fn my_command(
       state: tauri::State<'_, AppState>,
       arg1: String,
       arg2: Option<i64>,
   ) -> Result<MyResponse, String> {
       // ...
   }
   ```
   - Always return `Result<T, String>` — Tauri serializes errors as the
     `String` to the frontend.
   - First arg should be `State<'_, AppState>` if you need any shared
     state (DB, providers, sessions, etc.).
   - All non-state args must be `serde::Deserialize`. Use camelCase from
     the frontend: `invoke('my_command', { arg1: 'x', arg2: 42 })` maps
     to snake_case Rust args via Tauri's automatic conversion.
3. **Register in `main.rs`**:
   ```rust
   .invoke_handler(tauri::generate_handler![
       // ... existing commands ...
       tauri_commands::my_command,
   ])
   ```
   If your command lives in a sub-module, the path must be correct.
4. **Add the TypeScript wrapper** in `ui/src/lib/tauri-bridge.ts` (or in the
   feature's own bridge file). Match the existing pattern: typed wrapper
   function around `invoke()`.
5. **Build both sides**:
   ```bash
   cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
   cd ui && npx tsc --noEmit 2>&1 | head -10
   ```

## Common shapes

| Need | Pattern |
|---|---|
| Shared DB / providers / sessions | `state: tauri::State<'_, AppState>` |
| Long-running task with progress | Emit events: `app: tauri::AppHandle` + `app.emit("event-name", payload)` |
| Returning binary blobs | `Result<Vec<u8>, String>` — Tauri auto-base64s for IPC |
| Optional args | `Option<T>` works; frontend can pass `null` or omit |
| Async file IO | Always `async` + `tokio::fs` (not `std::fs`) |

## Safety integration

If your command triggers a destructive action (file write outside the
workspace, shell exec, browser action), wire it through `SafetyManager`
and `PendingApprovals` (see existing examples like `approve_tool_call`).
Don't add a new "fire-and-forget" command for destructive ops without
the approval round-trip.

## See also

- `src-tauri/src/tauri_commands.rs` — every existing command as reference
- `src-tauri/src/main.rs` — `invoke_handler!` macro registration block
- `ui/src/lib/tauri-bridge.ts` — frontend bridge types
- `BEHAVIOR.md §"DMZ Files Need Two-Session Review"`
- CLAUDE.md Part 1 *Adjacent edits that look like scope creep*
