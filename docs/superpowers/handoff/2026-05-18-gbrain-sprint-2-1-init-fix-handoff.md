# gbrain Sprint 2.1 init-fix — hand-off

**Status:** ready to merge (assuming end-to-end verify is green on Mac).
**Branch:** `worktree-gbrain-sprint-2-1-init-fix`
**Base:** `main`
**Predecessor:** PR #204 (Sprint 2.1 — seeded MCP entry + PGLite data dir)

## Why this PR exists

Sprint 2.1 (#204) seeded the bundled gbrain MCP entry but never actually
initialized the brain. On every fresh launch, gbrain serve spawns, runs
`Starting GBrain MCP server (stdio)...`, then immediately exits with `No
brain configured. Run: gbrain init`. uClaw's MCP manager waits the full
60s for initialize, gives up, and logs `MCP server 'gbrain (bundled)'
initialize failed: Timeout`. The agent's tool table never sees
`mcp__gbrain__*`, which silently broke the "gbrain MCP is on the agent's
tool table" claim from the Sprint 2.1 hand-off.

Three bugs collided to produce that symptom:

1. **No `gbrain init` call anywhere in the boot path.** `seed_bundled_gbrain`
   wrote the mcp_servers.json entry but did not initialize the brain.
2. **Pre-write of `~/.uclaw/gbrain/.gbrain/config.json` pointed at the
   wrong path** (`pgdata/`, not `.gbrain/brain.pglite/`). Even if you ran
   `gbrain init` manually afterwards, init would overwrite the pre-write
   to the correct path — making the pre-write a no-op at best and a
   silent-shadow at worst.
3. **`seed_bundled_gbrain(..., pgdata_dir: &Path)` reached back through
   `.parent()` to recover GBRAIN_HOME.** Confusing API; refactored to
   take `gbrain_home` directly.

## What changed

| File | Diff |
|---|---|
| `src-tauri/src/mcp.rs` | +2 functions (`is_brain_initialized` pure probe, `ensure_bundled_gbrain_initialized` spawner); `seed_bundled_gbrain` param rename + description string fix + tracing field fix; +4 unit tests |
| `src-tauri/src/main.rs` | Stage 3 block shrinks ~45 lines → ~10; pre-write `config.json` deleted; `ensure_bundled_gbrain_initialized` called before `seed_bundled_gbrain`; `pgdata_dir` → `gbrain_home` rename |

## How init works after this PR

1. Stage 3 resolves bundled `bun` + `gbrain/src/cli.ts` paths.
2. `ensure_bundled_gbrain_initialized` probes
   `~/.uclaw/gbrain/.gbrain/brain.pglite/PG_VERSION`:
   - **Present** → return `Ok(false)`, ~O(1).
   - **Missing** → `std::fs::create_dir_all(gbrain_home)`, then spawn
     `bun cli.ts init --pglite --yes` with `GBRAIN_HOME=~/.uclaw/gbrain`.
     PGLite cold-starts, runs ~63 migrations, exits 0. ~30-60s on Apple
     Silicon. Defense-in-depth: verify `PG_VERSION` landed after the
     spawn returns 0 (catches the "init exited 0 but wrote elsewhere"
     bug class this PR is fixing).
3. `seed_bundled_gbrain` writes the mcp_servers.json entry with
   `GBRAIN_HOME` (NOT `PGLITE_DATA_DIR`) in env.
4. `connect_all_enabled` triggers — gbrain initialize completes in
   ~2-5s on the warm path.

On init failure, the warn log explicitly states seed runs anyway (so the
entry surfaces in the Integrations UI) and points at a recovery command.
The auto-connect failure is more actionable than a silently missing entry.

## How to verify locally

```bash
# Move existing gbrain state aside
mv ~/.uclaw/gbrain ~/.uclaw/gbrain.bak-pre-verify
# Remove the gbrain entry from mcp_servers.json (edit the array)
cp ~/.uclaw/mcp_servers.json ~/.uclaw/mcp_servers.json.bak-pre-verify
# … edit mcp_servers.json to remove the gbrain object …

# Build + run debug app, watch the log
cd src-tauri && cargo tauri dev > /tmp/uclaw-dev.log 2>&1 &
tail -f ~/.uclaw/logs/uclaw.log.$(date +%Y-%m-%d) | grep -E "Stage 3|gbrain"
```

Expected sequence (first launch):
```
[Stage 3] gbrain brain initialized (first launch)
[Stage 3] gbrain MCP entry seeded (first launch)
Connecting to MCP server 'gbrain (bundled)' (gbrain)
[Stage 3] MCP servers auto-connect pass complete (1 health loops spawned)
```

Then quit + relaunch — expected (warm launch):
```
[Stage 3] gbrain MCP entry already present, skipping seed
Connecting to MCP server 'gbrain (bundled)' (gbrain)
[Stage 3] MCP servers auto-connect pass complete (1 health loops spawned)
```

(`gbrain brain already initialized` only logs at DEBUG level — set
`RUST_LOG=debug` if you want to see it.)

## Migration for existing users

This PR's idempotency probe (PG_VERSION) means existing users with a
correctly-initialized brain just see the seed entry skipped and gbrain
connects normally. Users with the pre-existing broken state (env =
`PGLITE_DATA_DIR=...gbrain/pgdata`, empty pgdata, no brain) need a
one-time manual fix:

```bash
# Run gbrain init once
PATH_BUN=$(ls /Users/$USER/Documents/uclaw/src-tauri/target/{debug,release}/bun 2>/dev/null | head -1)
PATH_CLI=$(ls /Users/$USER/Documents/uclaw/src-tauri/target/{debug,release}/gbrain/src/cli.ts 2>/dev/null | head -1)
GBRAIN_HOME=~/.uclaw/gbrain "$PATH_BUN" "$PATH_CLI" init --pglite --yes

# Edit ~/.uclaw/mcp_servers.json — change the gbrain entry's env from
# {"PGLITE_DATA_DIR":"..."} to {"GBRAIN_HOME":"/Users/$USER/.uclaw/gbrain"}
```

Sprint 2.1 was only just merged so the affected population is one user
(the project owner). A migration helper isn't worth the surface area.

## Commits (bisectable)

| # | sha | purpose |
|---|-----|---------|
| 1 | 5c53d37 | task 1: is_brain_initialized probe + 3 unit tests |
| 2 | 2676b04 | task 2: ensure_bundled_gbrain_initialized + seed signature refactor |
| 3 | 2bcc396 | task 2 follow-on: idempotency unit test (review finding) |
| 4 | b67ebbf | task 3: main.rs Stage 3 wiring + pre-write deletion |
| 5 | 6e6c282 | task 3 follow-on: warn message accuracy fix (review finding) |
| 6 | <this commit> | task 4: hand-off doc + commit body + plan file |

## Files index

```
docs/superpowers/plans/2026-05-18-gbrain-sprint-2-1-init-fix.md
docs/superpowers/handoff/2026-05-18-gbrain-sprint-2-1-init-fix-handoff.md
docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_1_INIT_FIX.txt
docs/superpowers/handoff/2026-05-18-gbrain-sprint-2-1-handoff.md
```
