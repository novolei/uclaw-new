# MCP service audit — 2026-05-18

Author: Claude (Cowork mode, on behalf of Ryan)
Scope: `src-tauri/src/mcp.rs`, `src-tauri/src/tauri_commands.rs` MCP block,
`src-tauri/src/main.rs` invoke handler, `ui/src/views/Kaleidoscope/modules/Integrations/*`,
`ui/src/lib/tauri-bridge.ts` MCP wrappers.

## TL;DR

The MCP subsystem is **architecturally complete but functionally dead** in
three load-bearing places:

1. **MCP tools never reach the agent.** `McpToolProxy::create_tool_proxies`
   (mcp.rs:1217) is defined and tested but has zero callers outside its
   own module. `tauri_commands.rs:376–422` (chat IPC) and `:8147+`
   (agent IPC) register builtin + browser tools and seal the registry
   with `Arc::new(tools)` without ever asking the MCP manager. The
   user can connect a server, see its tools listed in the
   Integrations module, and have those tools be invisible to the LLM.
2. **No auto-connect on startup.** `connect_all_enabled` (mcp.rs:1090) is
   also defined and dead-coded. `app.rs:402` instantiates the manager
   and `manager.load_config()` reads `~/.uclaw/mcp_servers.json`, but
   nothing reconnects servers that were connected last session. Users
   must click "重启" on each server after every app restart.
3. **`auto_approve` toggle does nothing.** The UI exposes a per-server
   "auto approve" switch (McpEditorModal.tsx:283–293), persists it to
   config (mcp.rs:246), and round-trips it through IPC, but no code
   path in `SafetyManager` or the agent dispatcher consults
   `config.auto_approve`. The toggle is observably present and
   observably ineffective.

The good news: lifecycle, transport, JSON-RPC, persistence, and tool
discovery primitives are all solid and unit-tested (mcp.rs:1244+).
Fixing the three gaps above is **mostly wiring, not redesign**.

## What works

- **Storage** (mcp.rs:806–842): JSON file at `~/.uclaw/mcp_servers.json`,
  loaded on construction, written on every mutation. Tested.
- **stdio + HTTP transports** with proper framing and 60s hardcoded
  read timeout (mcp.rs:450, 505). JSON-RPC `initialize` → `tools/list`
  flow works.
- **Editor UI** (McpEditorModal.tsx): transport selector, args list,
  env vars list, HTTP URL, auto-approve checkbox, all editable.
- **Detail drawer** (McpDetailDrawer.tsx): shows status, error text in
  a `<pre>` block, restart + remove buttons, enable/disable toggle.
- **IPC surface** (9 commands at tauri_commands.rs:2606–2779): all
  registered in `main.rs:557`, type-mirrored in `tauri-bridge.ts:983+`.

## Verified dead code

| Symbol | Defined | Callers outside its file |
|---|---|---|
| `McpToolProxy::create_tool_proxies` | mcp.rs:1217 | **0** |
| `McpManager::connect_all_enabled` | mcp.rs:1090 | **0** |
| `McpManager::refresh_tools` | mcp.rs:1125 | **0** |
| `McpManager::ping_server` | mcp.rs:1114 | **0** |
| `disconnectMcpServer` (frontend) | tauri-bridge.ts:1013 | **0** |
| `config.auto_approve` (read) | mcp.rs:246 | persisted only, not enforced |

I verified each by grepping `src-tauri/src/` and `ui/src/` for any
caller. All confirmed unreferenced as of `85486b4`.

## Findings by dimension

### 1. Lifecycle

Manual only. `connect_mcp_server` / `disconnect_mcp_server` /
`restart_mcp_server` are user-triggered. No reconnect-on-failure, no
exponential backoff, no startup auto-connect. The
`connect_all_enabled` helper would solve startup in one line but is
never called.

If a server crashes mid-session, the proxy fails on next invocation
(mcp.rs:734–793) with a transport error. The status doesn't flip to
`Error` automatically — the user has to either trigger a new tool call
that fails, or notice in the detail drawer. The next agent turn will
still see the dead tool in its manifest (assuming finding 1 is fixed).

### 2. Tool listing & refresh

Discovery happens once, inside `connect_server` (mcp.rs:1031–1057). The
backend declares `capabilities.tools.listChanged = true` in the
`initialize` request (mcp.rs:56), but inbound server notifications are
explicitly discarded:

```rust
// mcp.rs:346–353
Some(_method) => {
    // Server notification/request — log and skip
    tracing::debug!("ignored server message");
}
```

No `tools/list_changed` handler. No periodic refresh. No UI button to
manually refresh tools on a connected server. If an MCP server adds a
new tool while uClaw is running, the user must restart that server in
the UI to see it.

### 3. Error handling

Errors are stored on `McpServerState.error: Option<String>` (mcp.rs:706)
and surfaced via `all_server_statuses()` → IPC → UI badge + `<pre>`.
**In-memory only** — closing and re-opening the app loses the error
text (only the last-known status survives via JSON config?). No audit
trail. Stderr from the subprocess is `tracing::debug!`-logged (mcp.rs:382),
which means it's effectively invisible to users running release builds
at default log levels.

No raw JSON-RPC traffic log in the UI. To debug a misbehaving server
the user must inspect the Tauri log file on disk.

### 4. Health monitoring

Nothing in this dimension. `ping_server` exists, is unwired, no
periodic task pings anything. A "dead but listed" server in the
manifest is the default failure mode — even if finding 1 is fixed,
the agent will happily try to call tools that hang or fail.

### 5. Configuration

Good baseline:

- ✓ Per-server enabled
- ✓ Transport (stdio / http)
- ✓ Args (stdio)
- ✓ Env vars (stdio)
- ✓ URL (http)
- ✓ auto_approve (stored, **but never consulted** — see finding 3 in TL;DR)

Missing:

- Per-tool enable/disable (some MCP servers expose 20+ tools; users
  often want a subset)
- Per-server timeout override (hardcoded 60s)
- Transport-specific knobs (SSE keepalive, HTTP headers besides auth)
- "Dry-run" / test-connection without persisting

### 6. Security

No allowlist of binaries. The user can put any command in the editor
and uClaw will `tokio::process::Command::new(...)` spawn it (mcp.rs:301).
Env vars are passed verbatim and stored plaintext in
`mcp_servers.json`; if a subprocess fails to spawn, the error message
may include the command + env (mcp.rs:309–311 logs but I didn't audit
every error path for leak risk).

Not catastrophic — uClaw is a desktop app launched by the user, so
arbitrary subprocess is the user's prerogative. But two specific
improvements would be cheap: (a) redact env values from error strings
sent to the UI, (b) optional confirmation-on-first-spawn for any new
command path.

### 7. IPC completeness

9 commands defined and registered. Frontend uses 7 of them. Orphaned:

- `disconnect_mcp_server` — wrapped in `tauri-bridge.ts` but no UI
  affordance. Easy fix: add a "Disconnect" button next to "Restart"
  in `McpDetailDrawer.tsx`.

Missing:

- `refresh_mcp_tools(id: String) -> Vec<McpToolDef>` — Rust-side
  `refresh_tools` exists; just needs a `#[tauri::command]` wrapper +
  bridge wrapper + UI button.
- `ping_mcp_server(id: String) -> Result<u32 /*latency_ms*/, String>`
  — Rust-side `ping_server` exists; same shape as above.

### 8. Frontend wiring

`IntegrationsModule.tsx:50–69` refetches on mount + after mutations.
No polling. Status badge is correct at fetch-time but can go stale —
e.g. open the detail drawer, leave it for 10 min, server dies, drawer
still shows "已连接".

### 9. Persistence & migration

Plain JSON, no schema version. `serde(default)` is used on `McpServerConfig`
so adding new optional fields is backward-compatible, but removing or
renaming a field would break old configs silently (`unwrap_or_default`
on parse failure swallows the error per mcp.rs:826–831). Not urgent —
the schema is small and rarely changes — but worth a `schema_version`
field if we ever do a non-additive change.

### 10. Logs & debuggability

`tracing` everywhere, no UI exposure. The most useful debug info
(stderr from subprocesses, JSON-RPC bodies) sits at `debug` level. A
single-screen "MCP logs" tab streaming the relevant `tracing` events
would be a high-value affordance — same shape as the existing cost
dashboard's event stream.

## Quick wins (each ≤ 1 day)

1. **Wire `connect_all_enabled` into Stage 3 boot.** One call in
   `main.rs` post-AppState construction:
   ```rust
   tokio::spawn(async move {
       let mut mgr = state.mcp_manager.write().await;
       mgr.connect_all_enabled().await;
   });
   ```
   Fixes the "every restart needs manual reconnect" friction.

2. **Wire `create_tool_proxies` into both ChatDelegate callsites.**
   `tauri_commands.rs:422` and `:8147+`, right before `Arc::new(tools)`.
   Note: `create_tool_proxies` is a method on `McpManager`, not on
   `McpToolProxy` — the original audit draft had this wrong; corrected
   in the actual PR-1.
   ```rust
   {
       let mgr = state.mcp_manager.read().await;
       let proxies = crate::mcp::McpManager::create_tool_proxies(
           &state.mcp_manager,
           &*mgr,
       );
       for p in proxies { tools.register(p); }
   }
   ```
   Single biggest unlock. Without this the whole MCP system is a UI
   demo. Pair with finding 3 below so auto_approve actually works.

3. **Honor `config.auto_approve` in SafetyManager.** When the dispatcher
   asks SafetyManager for approval on an MCP-proxied tool call, look up
   the source server's `auto_approve` and skip the approval prompt if
   true. Pin this together with finding 2 so the UX is coherent on
   first use.

4. **Add `refresh_mcp_tools` and `ping_mcp_server` IPC + UI buttons.**
   Two new `#[tauri::command]`s wrapping existing Rust methods, two
   bridge wrappers, two buttons in `McpDetailDrawer.tsx`. Lets users
   pick up new tools without restarting and check health without
   triggering a full reconnect.

5. **Add "Disconnect" button in detail drawer.** `disconnectMcpServer`
   is already plumbed end-to-end; just needs a `<Button>` next to
   "Restart". Useful when debugging — separates "stop talking to it"
   from "remove it from the list".

## Substantive gaps (multi-PR, each Sprint-sized)

A. **Auto-reconnect + health loop.** Background task per connected
   server that pings every N seconds, marks the server `Error` on
   failure, retries with exponential backoff. UI shows a "reconnecting"
   spinner badge. Cancellation safety + interaction with
   `connect_all_enabled` is the gnarly part.

B. **Server notification routing (`tools/list_changed`, log events,
   sampling requests).** The MCP spec has a richer notification
   surface than uClaw currently uses. Plumbing requires an internal
   event bus from the per-server reader task to the manager + UI,
   plus tests for ordering/duplication. Touches the transport layer.

C. **Audit + secrets hardening.** Persist connection-attempt errors to
   a small `mcp_audit` table (timestamp, server_id, error_text,
   redacted). Add a "raw logs" opt-in setting. Redact env values from
   any error string sent to the UI. Crosses storage, IPC, and
   settings.

## Suggested PR sequence

1. **PR-1 (S, ~half day):** Quick wins 1 + 2 + 3 — bundled, because
   together they make MCP actually functional. Tests: a sanity test
   that `connect_all_enabled` is invoked at boot, a SafetyManager
   test honoring `auto_approve`.
2. **PR-2 (S, half day):** Quick wins 4 + 5 — UI affordances.
3. **PR-3 (M, 1 day):** Substantive gap A (auto-reconnect).
4. **PR-4 (M, 1–2 days):** Substantive gap B (notifications).
5. **PR-5 (S, half day):** Substantive gap C (audit + redaction).

## Verification methodology

I cross-checked the agent's report by direct grep:

```
grep -rn "create_tool_proxies\|McpToolProxy"     src-tauri/src/
grep -rn "connect_all_enabled\b"                 src-tauri/src/
grep -rn "refresh_tools\b"                       src-tauri/src/
grep -rn "ping_server\b"                         src-tauri/src/
grep -rn "disconnectMcpServer\b"                 ui/src/
grep -rn "\.auto_approve\b"                      src-tauri/src/
```

Every "dead" symbol in the table above has zero callers outside its
defining file.
