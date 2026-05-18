# MCP `connect_server` — Three-Stage Refactor (No `await` Under Write Lock)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the 60-second freeze on the first `send_agent_message` after every app launch by removing the "hold `RwLock::write()` across slow network IO" anti-pattern in the MCP subsystem.

**Architecture:** Today `McpManager::connect_server(&mut self, id)` does `transport.spawn() → conn.initialize() → conn.discover_tools()` while the caller holds the manager-wide `RwLock` write guard. When the bundled `gbrain` MCP server's initialize handshake times out (60s — the default `McpConnection::initialize` deadline), the write lock is held for the full 60s, blocking `tauri_commands::send_agent_message` (which needs `mcp_manager.read().await` to enumerate tools) and every other reader. We split `connect_server` into three phases — **prepare** (short write lock: read config + mark `Connecting`), **IO** (no lock held: spawn transport + initialize + discover tools), **commit** (short write lock: install connection + mark `Connected`/`Error` + audit). The signature shifts from `&mut self` to `(shared: &SharedMcpManager, id: &str)` so the function itself manages lock acquisition. `connect_all_enabled` is rewritten to snapshot enabled IDs then call the new free function per ID, and Stage 3 in `main.rs` is simplified to use the same shape. `restart_server` is updated as well so manual user-triggered restarts benefit from the same non-blocking property.

**Tech Stack:** Rust, tokio `RwLock`, Tauri commands path.

**Pre-existing context:**
- Root cause investigation evidence is in this conversation transcript (log timestamps at `~/.uclaw/logs/uclaw.log.2026-05-18` between 14:35:45 and 14:36:37). Do not re-investigate.
- The user has approved the three-stage approach. Do not propose alternatives.
- A small follow-up is to also probe gbrain bun stdio handshake — that's NOT in this plan. Track it as a separate issue.

---

## File Structure

- **Modify** [src-tauri/src/mcp.rs](../../src-tauri/src/mcp.rs) — refactor `connect_server` into prepare/IO/commit triplet, rewrite `connect_all_enabled` and `restart_server`/`reconnect_server` to use the new signature, add a `list_enabled_ids` helper.
- **Modify** [src-tauri/src/main.rs](../../src-tauri/src/main.rs) — simplify the Stage 3 auto-connect block; the long-held write-lock pattern goes away.
- **No changes** to [src-tauri/src/tauri_commands.rs](../../src-tauri/src/tauri_commands.rs) — the `read().await` site at line ~563 was always correct; it was the writer that was wrong.

## Coding conventions

- Logging: `tracing::info!` / `tracing::warn!` / `tracing::error!` already in use.
- No `unwrap()` on `Result`; use existing `McpError` types.
- New helper functions on `McpManager` impl are `pub(crate)` unless they need to be `pub`.
- Match the codebase's existing flat style (no new traits).

## Verification

After all tasks land:

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head    # backend compile errors only
cd src-tauri && cargo test --lib mcp 2>&1 | tail -30           # mcp unit tests
```

Manual smoke (uClaw running via `cargo tauri dev`):
1. Fully quit + relaunch the app.
2. Open the agent and send a message. **Expected**: agent loop starts within ~1s of `send_agent_message ENTRY` in `~/.uclaw/logs/uclaw.log.2026-05-18`, even while `gbrain (bundled)` is still timing out in the background.
3. Tail `~/.uclaw/logs/uclaw.log.2026-05-18` and confirm `AGENTIC_LOOP: starting` no longer lags the first `send_agent_message` by 30+ seconds.

---

### Task 1: Add `list_enabled_ids` helper

**Files:**
- Modify: `src-tauri/src/mcp.rs` — add a small read-only method on `McpManager` near the existing `connect_all_enabled` (around line 1640).

- [ ] **Step 1: Read context**

Open `src-tauri/src/mcp.rs:1639-1655` to see the existing `connect_all_enabled`:

```rust
/// Connect all enabled servers
pub async fn connect_all_enabled(&mut self) {
    let ids: Vec<String> = self
        .servers
        .values()
        .filter(|s| s.config.enabled)
        .map(|s| s.config.id.clone())
        .collect();

    for id in ids {
        if let Err(e) = self.connect_server(&id).await {
            tracing::error!("Failed to connect MCP server '{}': {}", id, e);
        }
    }
}
```

- [ ] **Step 2: Add helper above `connect_all_enabled`**

Insert immediately before the `pub async fn connect_all_enabled` line:

```rust
/// Snapshot the IDs of enabled servers. Cheap; takes only a `&self`
/// borrow so callers can release the lock before doing async work.
pub fn list_enabled_ids(&self) -> Vec<String> {
    self.servers
        .values()
        .filter(|s| s.config.enabled)
        .map(|s| s.config.id.clone())
        .collect()
}
```

- [ ] **Step 3: Compile**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/mcp.rs
git commit -m "refactor(mcp): add list_enabled_ids helper for lock-free ID snapshot"
```

---

### Task 2: Extract `connect_server` into prepare/IO/commit (free function)

**Files:**
- Modify: `src-tauri/src/mcp.rs` — replace the existing `pub async fn connect_server(&mut self, id: &str)` body (around lines 1334-1474) with a thin shim that calls a new free function `connect_server_shared(shared: &SharedMcpManager, id: &str)`.

This is the core fix. The free function owns lock acquisition; the slow `await` lines (`StdioTransport::spawn`, `conn.initialize`, `conn.discover_tools`) run **without** the manager's write lock held.

- [ ] **Step 1: Read existing `connect_server`**

Read `src-tauri/src/mcp.rs:1334-1474` in full so you understand:
- What state must be read up-front (config, transport_type, url, env, notification_tx).
- What state must be written on success (status=Connected, tools, connection).
- What state must be written on failure (status=Error, error message).
- Where audit rows go (`self.record_audit(...)` at start + on success).

- [ ] **Step 2: Add the free function below the existing impl block**

Find the closing `}` of the `impl McpManager { ... }` block that contains `connect_server` (search for `// ── Tool Proxying`; the impl block ends a bit above). Add this free function **outside** the impl, but inside the same module:

```rust
/// Connect to an MCP server without holding the manager's write lock
/// across the slow network IO. Three phases:
///   1. Prepare (short write lock): clone config, snapshot
///      `notification_tx`, mark `Connecting`, audit `ConnectAttempt`.
///   2. IO (no lock held): spawn transport, run `initialize` and
///      `discover_tools`.
///   3. Commit (short write lock): install the connection + tool defs +
///      `Connected` status, OR mark `Error` and audit appropriately.
///
/// This is the lock-discipline fix for the "first message after app
/// launch hangs for 60s" bug — the bundled gbrain server's initialize
/// can take up to a full 60s to time out, and prior to this refactor
/// the manager-wide write lock was held the entire time, blocking
/// `send_agent_message`'s `mcp_manager.read().await`.
pub(crate) async fn connect_server_shared(
    shared: &SharedMcpManager,
    id: &str,
) -> Result<(), McpError> {
    // ── Phase 1: prepare ────────────────────────────────────────────
    let (config, notification_tx) = {
        let mut guard = shared.write().await;
        let state = guard.servers.get(id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", id))
        })?;
        let config = state.config.clone();
        let notification_tx = guard.notification_tx.clone();
        if let Some(state) = guard.servers.get_mut(id) {
            state.status = McpServerStatus::Connecting;
            state.error = None;
        }
        guard.record_audit(
            id,
            McpAuditKind::ConnectAttempt,
            &format!("Connecting to {}", config.name),
        );
        (config, notification_tx)
    };

    tracing::info!("Connecting to MCP server '{}' ({})", config.name, id);

    // ── Phase 2: IO (no lock held) ──────────────────────────────────
    let io_result: Result<McpConnection, McpError> = async {
        let transport: Arc<dyn McpTransport> = match config.transport_type {
            TransportType::Stdio => {
                let t = StdioTransport::spawn(
                    &config.name,
                    &config.command,
                    &config.args,
                    &config.env,
                    id,
                    notification_tx.clone(),
                )
                .await?;
                Arc::new(t)
            }
            TransportType::Http => {
                let url = config.url.clone().unwrap_or_default();
                if url.is_empty() {
                    return Err(McpError::Server(
                        "HTTP transport requires a URL".into(),
                    ));
                }
                Arc::new(HttpTransport::new(&config.name, &url))
            }
        };

        let mut conn = McpConnection {
            transport,
            next_id: AtomicU64::new(1),
            initialized: false,
            tools: Vec::new(),
            server_info: None,
        };

        // initialize is the expensive call (up to ~60s on a hung
        // stdio server). Critically, no lock is held here.
        let init_result = conn.initialize().await?;
        tracing::info!(
            "MCP server '{}' initialized (protocol: {:?}, server: {:?})",
            config.name,
            init_result.protocol_version,
            init_result.server_info.as_ref().map(|s| &s.name),
        );

        // discover_tools failure is non-fatal — the server may simply
        // not implement tools/list. We still keep the connection.
        if let Err(e) = conn.discover_tools().await {
            tracing::warn!(
                "MCP server '{}' tools/list failed: {}",
                config.name,
                e
            );
        }

        Ok(conn)
    }
    .await;

    // ── Phase 3: commit ─────────────────────────────────────────────
    let mut guard = shared.write().await;
    match io_result {
        Ok(mut conn) => {
            // discover_tools result lives on `conn.tools` already (set
            // by McpConnection::discover_tools). We just need to
            // mirror it into ServerState.tools as McpToolDef entries.
            let tool_defs: Vec<McpToolDef> = conn
                .tools
                .iter()
                .map(|t| McpToolDef {
                    server_id: id.to_string(),
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                })
                .collect();
            let tool_count = tool_defs.len();
            tracing::info!(
                "MCP server '{}' has {} tool(s): [{}]",
                config.name,
                tool_count,
                tool_defs.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
            );
            if let Some(state) = guard.servers.get_mut(id) {
                state.status = McpServerStatus::Connected;
                state.error = None;
                state.tools = tool_defs;
                state.connection = Some(conn);
            }
            guard.record_audit(
                id,
                McpAuditKind::ConnectSucceeded,
                &format!("Connected ({} tool(s) discovered)", tool_count),
            );
            Ok(())
        }
        Err(e) => {
            tracing::error!(
                "MCP server '{}' connect failed: {}",
                config.name,
                e
            );
            if let Some(state) = guard.servers.get_mut(id) {
                state.status = McpServerStatus::Error;
                state.error = Some(e.to_string());
            }
            // No explicit audit row on failure — record_audit on
            // ConnectAttempt + the Error status is enough for the
            // existing audit log shape. Mirrors the prior behavior.
            Err(e)
        }
    }
}
```

Notes:
- `McpConnection::discover_tools` mutates `conn.tools` in place; we read it back after the IO phase to build `McpToolDef` rows. This matches the prior code path. If you find `discover_tools` does NOT mutate `conn.tools`, instead capture its `Result<Vec<McpRemoteTool>>` return value and build `tool_defs` from that. (Verify by reading `src-tauri/src/mcp.rs` around line 773.)
- If `connect_server_shared` is the only new free function, no other `use` statements should be needed — `Arc`, `AtomicU64`, etc. are already in scope at the top of `mcp.rs`. If the compiler asks for an import, add it.

- [ ] **Step 3: Replace the old `connect_server` body with a thin shim**

The old `pub async fn connect_server(&mut self, id: &str) -> Result<(), McpError>` cannot easily call the new free function because it has `&mut self` (it doesn't have access to the outer `SharedMcpManager`). The simplest cut is to **delete the old function** and update every caller. There are three callers inside `mcp.rs`:
  1. `connect_all_enabled` (line ~1640) — handled in Task 3.
  2. `reconnect_server` (line ~1627) — handled in Task 3.
  3. `restart_server` (line ~1497) — handled in Task 4.

And one Tauri command caller in `tauri_commands.rs` — search for `\.connect_server\(` to find it. Each call site moves from `manager.connect_server(&id).await` to `connect_server_shared(&shared_manager, &id).await`, where `shared_manager` is the `SharedMcpManager` arc (not the locked guard).

For this step: **delete the old `pub async fn connect_server(&mut self, id: &str)` method entirely** (lines ~1334-1474). The free function `connect_server_shared` replaces it.

- [ ] **Step 4: Compile (expect errors at call sites)**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -30`
Expected: errors of the form `no method named connect_server found for ...` at the 3-4 call sites. That's correct — we fix them in Tasks 3 and 4.

- [ ] **Step 5: Commit (compile is broken — that's OK between tasks)**

```bash
git add src-tauri/src/mcp.rs
git commit -m "refactor(mcp): split connect_server into prepare/IO/commit (free function)

The old connect_server held RwLock::write() across initialize/discover IO,
freezing every send_agent_message reader for up to 60s while gbrain (bundled)
timed out. Replaced with connect_server_shared(shared, id) which only holds
the lock during the cheap prepare + commit phases. Callers updated in
follow-up commits."
```

---

### Task 3: Rewrite `connect_all_enabled` and `reconnect_server` to call the free function

**Files:**
- Modify: `src-tauri/src/mcp.rs` — `connect_all_enabled` and `reconnect_server` near lines 1627 and 1640.

- [ ] **Step 1: Change `connect_all_enabled` signature + body**

The new shape is a free function (or static method) that takes `&SharedMcpManager`:

```rust
/// Connect all enabled servers. Each server's connect runs through
/// `connect_server_shared`, which releases the write lock during the
/// slow initialize/discover IO. Sequential iteration is fine because
/// the slow part no longer blocks readers — parallelizing would only
/// help startup wall time, not user-perceived latency.
pub async fn connect_all_enabled(shared: &SharedMcpManager) {
    let ids: Vec<String> = {
        let guard = shared.read().await;
        guard.list_enabled_ids()
    };
    for id in ids {
        if let Err(e) = connect_server_shared(shared, &id).await {
            tracing::error!("Failed to connect MCP server '{}': {}", id, e);
        }
    }
}
```

Move it OUT of the `impl McpManager { ... }` block (since it no longer takes `&mut self`). Place it next to `connect_server_shared`.

- [ ] **Step 2: Replace `reconnect_server`**

The current `async fn reconnect_server(&mut self, id: &str)` (line ~1627) is called from the per-server health loop. It does a `disconnect + connect`. Convert to a free function:

```rust
/// Internal reconnect for the health loop. Mirrors `restart_server`'s
/// disconnect+connect shape but without aborting the health loop
/// (we *are* the health loop).
pub(crate) async fn reconnect_server_shared(
    shared: &SharedMcpManager,
    id: &str,
) -> Result<(), McpError> {
    {
        let mut guard = shared.write().await;
        if let Some(state) = guard.servers.get_mut(id) {
            if let Some(conn) = state.connection.take() {
                // shutdown is best-effort; do it without await under
                // the lock would be cleaner but the shutdown itself
                // doesn't block on network IO the way connect does.
                let _ = conn.shutdown().await;
            }
            state.status = McpServerStatus::Disconnected;
        }
    }
    connect_server_shared(shared, id).await
}
```

Delete the old `reconnect_server` method. Find its caller (search for `reconnect_server(`); it's invoked from `start_health_loop`'s spawned task. Update that call site:

Replace `m.reconnect_server(id).await` with `reconnect_server_shared(&shared, id).await`. The health loop already has the `Arc<RwLock<Self>>` handle available; if not, capture it before spawning.

- [ ] **Step 3: Compile**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -20`
Expected: errors only at the remaining caller in `restart_server` (Task 4). If you see other errors, the `start_health_loop` rewire needs adjustment — read the surrounding code and pass the `SharedMcpManager` through.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/mcp.rs
git commit -m "refactor(mcp): rewrite connect_all_enabled + reconnect_server to use shared-lock helper"
```

---

### Task 4: Rewrite `restart_server` + update Tauri command caller

**Files:**
- Modify: `src-tauri/src/mcp.rs` — `restart_server` method around line 1497.
- Modify: `src-tauri/src/tauri_commands.rs` — search for `\.restart_server\(` (the IPC handler that exposes restart to the frontend).

- [ ] **Step 1: Replace `restart_server`**

```rust
/// Restart a server connection. User-triggered (Tauri command) and
/// distinct from the health loop's internal `reconnect_server_shared`
/// in that it ALSO aborts the health loop so the loop's pending
/// reconnect can't fight the user.
pub async fn restart_server_shared(
    shared: &SharedMcpManager,
    id: &str,
) -> Result<(), McpError> {
    {
        let mut guard = shared.write().await;
        guard.stop_health_loop(id);
        if let Some(state) = guard.servers.get_mut(id) {
            if let Some(conn) = state.connection.take() {
                let _ = conn.shutdown().await;
            }
            state.status = McpServerStatus::Disconnected;
            state.tools.clear();
            state.error = None;
        }
        guard.record_audit(id, McpAuditKind::Disconnect, "Disconnected (restart)");
    }
    connect_server_shared(shared, id).await?;
    // Health loop is reattached by the caller — the previous
    // restart_server did the same (returned Ok and let the
    // caller re-start_health_loop).
    Ok(())
}
```

Delete the old `restart_server`. Note that `disconnect_server`'s method version stays — we only refactor the connect-side hot paths.

- [ ] **Step 2: Find the Tauri command caller and update**

```bash
rg -n "restart_server\(" src-tauri/src/tauri_commands.rs
```

There should be at least one site. Replace `state.mcp_manager.write().await.restart_server(&id).await` (or similar) with:

```rust
crate::mcp::restart_server_shared(&state.mcp_manager, &id).await
```

If the caller also re-starts the health loop after restart, leave that alone (it's correct to re-attach the health loop after a manual restart).

- [ ] **Step 3: Compile**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no errors.

- [ ] **Step 4: Run mcp unit tests**

Run: `cd src-tauri && cargo test --lib mcp 2>&1 | tail -30`
Expected: existing tests pass. If a test relied on the `&mut self` shape of `connect_server`, update it to use `connect_server_shared(&shared, id)`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/mcp.rs src-tauri/src/tauri_commands.rs
git commit -m "refactor(mcp): rewrite restart_server to release write lock during connect IO

Manual restart now benefits from the same non-blocking property as
auto-connect — a hung MCP server's initialize no longer freezes
send_agent_message for up to 60s."
```

---

### Task 5: Simplify Stage 3 in `main.rs`

**Files:**
- Modify: `src-tauri/src/main.rs` — the Stage 3 spawn block at lines ~452-567.

This is the block that originally held the write lock for the full 60s.

- [ ] **Step 1: Read the current shape**

Open `src-tauri/src/main.rs:452-567` and re-read. Key landmarks:
- Line 509: `let mut guard = mcp_mgr.write().await;` (the write lock that was held too long).
- Line 510: `guard.set_notification_tx(tx);`
- Line 515: `guard.set_db_handle(db_for_mcp);`
- Line 523-550: seed_bundled_gbrain (short, synchronous).
- Line 551: `guard.connect_all_enabled().await;` (the offender).
- Line 552-562: read connected IDs back + `guard.start_health_loop(...)` per ID.

- [ ] **Step 2: Rewrite the spawn body**

Replace the contents of the inner `tauri::async_runtime::spawn(async move { ... })` (starting at line 452) — from `let (tx, mut rx) = ...` through the `tracing::info!("[Stage 3] MCP servers auto-connect pass complete ...)` line. The new body:

```rust
let (tx, mut rx) =
    tokio::sync::mpsc::unbounded_channel::<
        uclaw_core::mcp::McpNotificationEvent,
    >();

// Consumer task (unchanged — drains the channel for the lifetime
// of the app, refreshes tools on tools/list_changed, etc.).
let consumer_mgr = mcp_mgr.clone();
let consumer_app = app_for_consumer.clone();
tauri::async_runtime::spawn(async move {
    while let Some(event) = rx.recv().await {
        if event.method == uclaw_core::mcp::NOTIFY_TOOLS_LIST_CHANGED {
            let id = event.server_id.clone();
            let refresh_result = {
                let mut m = consumer_mgr.write().await;
                m.refresh_tools(&id).await
            };
            match refresh_result {
                Ok(tools) => {
                    use tauri::Emitter;
                    let _ = consumer_app.emit(
                        "mcp:tools-changed",
                        serde_json::json!({
                            "serverId": id,
                            "toolCount": tools.len(),
                        }),
                    );
                    tracing::info!(
                        "[mcp-notif] {} tools/list_changed → {} tools",
                        id,
                        tools.len()
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "[mcp-notif] {} refresh failed: {}",
                        id,
                        e
                    );
                }
            }
        } else {
            tracing::debug!(
                "[mcp-notif] {} {}",
                event.server_id,
                event.method
            );
        }
    }
    tracing::debug!("[mcp-notif] consumer task ended");
});

// ── Brief write lock: tx + db handle + seed gbrain ──────────────
{
    let mut guard = mcp_mgr.write().await;
    guard.set_notification_tx(tx);
    guard.set_db_handle(db_for_mcp);
    if let (Some(bun), Some(entry)) =
        (bun_path.as_ref(), gbrain_entry.as_ref())
    {
        match guard.seed_bundled_gbrain(bun, entry, &pgdata_dir) {
            Ok(seeded) => {
                if seeded {
                    tracing::info!(
                        "[Stage 3] gbrain MCP entry seeded (first launch)"
                    );
                } else {
                    tracing::debug!(
                        "[Stage 3] gbrain MCP entry already present, skipping seed"
                    );
                }
                *gbrain_mcp_id_slot.lock().unwrap() =
                    Some("gbrain".to_string());
            }
            Err(e) => tracing::warn!(
                error = %e,
                "[Stage 3] gbrain MCP seed failed (continuing without bundled gbrain)"
            ),
        }
    } else {
        tracing::info!(
            "[Stage 3] gbrain MCP seed skipped (bundle artifacts missing — run setup-bun-runtime.sh + setup-gbrain-source.sh)"
        );
    }
}
// Write lock dropped here.

// ── Connect each enabled server (write lock released across IO) ─
uclaw_core::mcp::connect_all_enabled(&mcp_mgr).await;

// ── Start health loops for the servers that came up Connected ───
let connected_ids: Vec<String> = {
    let guard = mcp_mgr.read().await;
    guard
        .all_server_statuses()
        .into_iter()
        .filter(|(_, status, _)| {
            matches!(status, uclaw_core::mcp::McpServerStatus::Connected)
        })
        .map(|(id, _, _)| id)
        .collect()
};
{
    let mut guard = mcp_mgr.write().await;
    for id in &connected_ids {
        guard.start_health_loop(mcp_mgr.clone(), id);
    }
}
tracing::info!(
    "[Stage 3] MCP servers auto-connect pass complete ({} health loops spawned)",
    connected_ids.len()
);
```

Note: the existing block captured `let shared = mcp_mgr.clone();` early to pass into `start_health_loop`. That's preserved — `mcp_mgr.clone()` inline is equivalent.

- [ ] **Step 3: Compile**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "fix(mcp): release manager write lock across auto-connect IO in Stage 3

Together with the connect_server_shared three-stage refactor, this eliminates
the 60s freeze on the first send_agent_message after every app launch when
the bundled gbrain MCP server's initialize handshake times out."
```

---

### Task 6: Manual smoke test + log evidence

**Files:**
- No code changes.

- [ ] **Step 1: Build + launch dev**

```bash
cd src-tauri && cargo tauri dev
```

Wait for the app window to appear and `[Stage 4] All services started` to land in the log.

- [ ] **Step 2: Send first message + watch logs**

In the app, open the Agent and send any short message (e.g. "你好"). In another terminal:

```bash
tail -f ~/.uclaw/logs/uclaw.log.2026-05-18
```

(Filename auto-updates daily — adjust to today's date.)

- [ ] **Step 3: Verify the fix**

Expected log shape:

```
[Stage 4] All services started
Connecting to MCP server 'gbrain (bundled)' (gbrain)    # auto-connect starts
send_agent_message ENTRY ...                            # user sends first msg
AGENTIC_LOOP: starting                                  # should follow within ~1s
Calling LLM ...
on_usage called ...
... (response streams normally)
MCP server 'gbrain (bundled)' initialize failed: Timeout  # gbrain times out
                                                          # 60s later, but
                                                          # user already got
                                                          # their reply
```

Critically: `AGENTIC_LOOP: starting` lands within ~1s of `send_agent_message ENTRY`, NOT 30+ seconds later.

- [ ] **Step 4: Negative test — confirm second message is still fast**

Send a second message after the first reply. `AGENTIC_LOOP: starting` should again follow within ~1s. No regression.

- [ ] **Step 5: Restart smoke**

Fully quit + relaunch the app. Repeat step 2-3. Confirm the fix holds across cold start, not just hot reload.

- [ ] **Step 6: Commit nothing — this is verification only**

Just confirm in your status report:
- First-message latency: PASS / FAIL with measured delay in seconds.
- Cold-restart reproduction: PASS / FAIL.
- gbrain still shows as `Error: Timeout` in the diagnostics tab (expected — that's the separate gbrain bug, not this PR).
