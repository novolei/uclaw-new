# gbrain Sprint 2.1 — Mac-side commit + diagnostic hand-off

Branch: `claude/sprint-gbrain-2-1` (off post-Sprint-2.0 main).

This sprint wires the bundled gbrain into the MCP auto-connect path
that PR-1 already proved out. After this PR, `cargo tauri dev` boots
into "gbrain (bundled)" attempting to connect on its own — no manual
"添加 MCP server" step from the user.

Mac-side validation has already confirmed the seed + spawn + connect
chain works (see Verification status below). One open question
remains around PGlite data dir; this doc lays out the diagnostic
sequence and the three possible fix paths.

## What's in this commit

```
src-tauri/src/app.rs   | +52  find_bun_path / find_gbrain_entry helpers
src-tauri/src/main.rs  | +53  Stage 3 seed + path resolution + mkdir
src-tauri/src/mcp.rs   | +75  McpManager::seed_bundled_gbrain
```

Plus this handoff doc + `COMMIT_GBRAIN_SPRINT_2_1.txt`.

## Pre-flight

```bash
cd ~/Documents/uclaw
git checkout claude/sprint-gbrain-2-1
rm -f .git/index.lock          # if sandbox left one
git status                      # 3 modified + 2 untracked docs
git diff --stat
```

## Commit

```bash
git add src-tauri/src/app.rs src-tauri/src/main.rs src-tauri/src/mcp.rs
git commit -F docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_1.txt

# Docs separately (or fold into above):
git add docs/superpowers/handoff/
git commit -m "docs(gbrain): Sprint 2.1 hand-off + commit message"
```

## Verification status (post-merge sanity)

Mac-side validation already confirmed 5/6 expected outcomes after
`cargo tauri dev`:

| Check | Status | Source of truth |
|---|---|---|
| `cargo build` clean | ✅ | exit 0 |
| Stage 3 spawns 6 services | ✅ | `[Stage 3]` log lines |
| Bundled gbrain CLI resolved | ✅ | `Found bundled gbrain CLI at .../target/debug/gbrain/src/cli.ts` |
| Seed fires + paths look right | ✅ | `gbrain Sprint 2.1: seeded bundled MCP entry bun=... entry=... pgdata=/Users/ryanliu/.uclaw/gbrain/pgdata` |
| Auto-connect picks it up | ✅ | `Connecting to MCP server 'gbrain (bundled)'` |
| PGlite writes to `~/.uclaw/gbrain/pgdata/` | ⚠️  empty | needs diagnostic below |

The 6th item is the only open question. The directory is created
correctly (mkdir_p in Stage 3) but PGlite hasn't written into it.
Three things could explain that — diagnostic flow below figures
out which.

## Open question: empty pgdata diagnostic

### Step 1 — read the audit table

```bash
sqlite3 ~/.uclaw/uclaw.db \
  "SELECT datetime(created_at/1000, 'unixepoch', 'localtime') AS t,
          event_kind, substr(message_redacted, 1, 100) AS msg
   FROM mcp_audit WHERE server_id='gbrain'
   ORDER BY created_at DESC LIMIT 20;"
```

The mcp_audit table (V40 from PR-5) records every lifecycle event.
Look for:

| Sequence seen | Interpretation |
|---|---|
| `connect_attempt` → `connect_succeeded` | MCP initialize + tools/list completed. Go to step 2. |
| `connect_attempt` → `connect_failed` | initialize failed. Error text in `message_redacted`. Diagnose from message. |
| `connect_attempt` → repeated `health_failed` | initialize succeeded once but ping started failing — subprocess probably died after init. Go to step 3. |
| Only `connect_attempt`, no success/failure | Subprocess hung mid-initialize. Go to step 3. |

### Step 2 — list gbrain's tool surface

If connect_succeeded, the next question is what tools gbrain
exposes. Two ways:

**Via UI:** Open Integrations module → click gbrain → tool list
shows in the detail drawer.

**Via Rust debug:** Add a temporary log to `list_mcp_tools` IPC
or just call it from frontend dev tools:

```typescript
await invoke('list_mcp_tools')
```

This is the input for Sprint 2.2 (data migration). Without seeing
gbrain's actual tool names + schemas we can't write the migration
script.

### Step 3 — verify subprocess is still alive

```bash
ps aux | grep -E "bun.*gbrain|src/cli.ts" | grep -v grep
```

If no rows → subprocess died. Go to step 4.
If rows → subprocess alive but PGlite still empty. Most likely the
env var name issue (step 5).

### Step 4 — capture subprocess stderr

The stdio reader logs stderr at `debug`, which isn't visible by
default. Two routes:

```bash
# Route A — high log level on the next dev run
RUST_LOG=uclaw_core::mcp=debug,uclaw_core=info cargo tauri dev 2>&1 \
  | grep -E "\[gbrain\]|stderr|PGlite|panic|error"

# Route B — manual JSON-RPC handshake to see what gbrain says
PGLITE_DATA_DIR=/tmp/gbrain-probe-pgdata src-tauri/bunembed/bun \
  src-tauri/gbrain-source/src/cli.ts serve <<<'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"probe","version":"0"},"capabilities":{}}}'
```

Route B almost always exposes the issue immediately. If
`/tmp/gbrain-probe-pgdata` stays empty too, the env var name is
wrong (jump to step 5). If it gets populated, then the same code
path works outside uClaw — re-check whether uClaw's spawn passes
env correctly (look at `StdioTransport::spawn` in
`src-tauri/src/mcp.rs`).

### Step 5 — find the real env var name

Grep gbrain source for env-var reads:

```bash
grep -rnE "process\.env\.|Bun\.env\." src-tauri/gbrain-source/src/ \
  | grep -iE "data|dir|path|db" | head -20
```

The data-dir env name is one of those matches. Plausible candidates:

- `PGLITE_DATA_DIR` (current guess in this commit)
- `BRAIN_DATA_DIR` / `GBRAIN_DATA_DIR`
- `BRAIN_DB_PATH` / `BRAIN_DB_DIR`
- `DATABASE_URL` (Postgres-style — would need `postgresql://...`
  format, not just a path)
- `PG_DATA_DIR` / `PGDATA`

If gbrain reads via CLI flag instead of env, it'll show up as a
`process.argv` parse near the `serve` command handler.

## Fix paths

Depending on what step 5 finds, edit
`src-tauri/src/mcp.rs::seed_bundled_gbrain`:

### Fix A — wrong env var name (most likely)

```rust
// In seed_bundled_gbrain:
env.insert(
    "BRAIN_DATA_DIR".to_string(),   // ← rename to whatever step 5 found
    pgdata_dir.to_string_lossy().to_string(),
);
```

One-line change. Re-build, re-run, pgdata fills.

### Fix B — gbrain uses CLI flag, not env

```rust
args: vec![
    entry_path.to_string_lossy().to_string(),
    "serve".to_string(),
    "--data-dir".to_string(),       // ← name from step 5
    pgdata_dir.to_string_lossy().to_string(),
],
// remove the env.insert for the data dir
```

### Fix C — gbrain reads from cwd, no override mechanism

This is the messiest. `StdioTransport::spawn` in `mcp.rs` would
need a `cwd: Option<PathBuf>` parameter, threaded through:

- `McpServerConfig` gains `pub cwd: Option<String>`
- `connect_server` reads it and passes to `StdioTransport::spawn`
- `StdioTransport::spawn` calls `cmd.current_dir(cwd)` on the
  child Command builder
- `seed_bundled_gbrain` sets `cwd = Some(pgdata_dir)` on the
  emitted config

Substantive change (touches transport signature, McpServerConfig
schema is JSON-serialized so consider a migration story). Worth
treating as Sprint 2.1b if needed.

## What Sprint 2.2 needs

The data migration script (SQLite EntityPage → gbrain) blocks on
step 2 above: we need to know gbrain's ingest tool name + schema.

Likely tool surface (educated guess from Engines spec):
- `entity_create(name, kind, content, aliases?)` or similar
- `entity_update(id, content)`
- `entity_search(query, k?)`
- `wiki_render(id)`

Once Sprint 2.1 is fully green and you can list tools, paste the
output and I'll write Sprint 2.2 (typically: read all EntityPages
from SQLite, batch-call `entity_create` against gbrain, mark them
migrated in a new column or sidecar table).

## Push

Once cargo build + cargo tauri dev are clean and the audit shows
`connect_succeeded`:

```bash
git push -u origin claude/sprint-gbrain-2-1
```

If PGlite is still empty but everything else is green, push anyway
— the env-var fix is a follow-up commit, not a blocker. The seed +
auto-connect + tool registration plumbing is the real Sprint 2.1
deliverable.

## Followups already on the radar

Tracked in tasks:
- **Sprint 2.1b (conditional)** — Fix C above if cwd-override path
  is needed
- **Sprint 2.2** — data migration from SQLite EntityPage → gbrain
- **Sprint 2.3** — Foundation deprecation roadmap (when to retire
  uClaw native memory_graph substrate)
- **Sprint 2.4** — Engines spec (Phase 15-21) revision marking
  pieces as superseded by gbrain MCP
- **Sprint 2.5 (size optim)** — gbrain-source is 264MB largely
  from tree-sitter-wasms. If installer size matters, prune unused
  language grammars
