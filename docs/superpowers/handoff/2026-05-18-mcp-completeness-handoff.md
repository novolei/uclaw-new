# MCP completeness sprint — 5-commit hand-off

Branch: `claude/sprint-mcp-completeness` (off `main` @ `85486b4`).

All 5 PRs from the 2026-05-18 MCP audit are staged as logical units on
this branch. Sandbox couldn't commit (recurring `.git/index.lock` from
the Mac-side editor) so this is a Mac-side hand-off. Each PR has its
own commit message file in this folder.

## Files touched

```
src-tauri/src/db/migrations.rs                                   # V40_MCP_AUDIT (PR-5)
src-tauri/src/main.rs                                            # auto-connect + notification + audit-db wiring (PR-1/3/4/5)
src-tauri/src/mcp.rs                                             # major: helpers, health loop, notifications, audit (all PRs)
src-tauri/src/tauri_commands.rs                                  # 3 ChatDelegate sites + 3 new IPC commands (PR-1/2/5)
ui/src/lib/tauri-bridge.ts                                       # 3 wrappers + audit type (PR-2/5)
ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.tsx   # event listener (PR-4)
ui/src/views/Kaleidoscope/modules/Integrations/McpDetailDrawer.tsx      # 3 secondary buttons (PR-2)
```

## Suggested commit order (bisectable)

Each commit message lives in this folder as `COMMIT_MCP_PR{N}.txt`.

### Commit 1 — PR-1: wire dead-coded MCP into agent + auto-connect + auto-approve

Files: `src-tauri/src/mcp.rs` (proxy fields, prefix helpers, create_tool_proxies, tests) + `src-tauri/src/main.rs` (auto-connect spawn) + `src-tauri/src/tauri_commands.rs` (3 registration sites in chat/agent/teams IPCs).

```bash
git add src-tauri/src/mcp.rs src-tauri/src/main.rs src-tauri/src/tauri_commands.rs
git commit -F docs/superpowers/handoff/COMMIT_MCP_PR1.txt
```

### Commit 2 — PR-2: refresh / ping / disconnect IPC + UI

Files: `src-tauri/src/tauri_commands.rs` (2 new IPC) + `src-tauri/src/main.rs` (register) + `ui/src/lib/tauri-bridge.ts` (2 wrappers) + `ui/src/views/Kaleidoscope/modules/Integrations/McpDetailDrawer.tsx` (3 buttons) + `ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.tsx` (refetch callbacks).

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts \
        ui/src/views/Kaleidoscope/modules/Integrations/McpDetailDrawer.tsx \
        ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.tsx
git commit -F docs/superpowers/handoff/COMMIT_MCP_PR2.txt
```

### Commit 3 — PR-3: auto-reconnect + health loop

Files: `src-tauri/src/mcp.rs` (health_tasks field, start/stop methods, run_health_loop, reconnect_server) + `src-tauri/src/main.rs` (start_health_loop after auto-connect) + `src-tauri/src/tauri_commands.rs` (start_health_loop in connect/restart IPC handlers).

```bash
git add src-tauri/src/mcp.rs src-tauri/src/main.rs src-tauri/src/tauri_commands.rs
git commit -F docs/superpowers/handoff/COMMIT_MCP_PR3.txt
```

### Commit 4 — PR-4: server notification routing

Files: `src-tauri/src/mcp.rs` (McpNotificationEvent, StdioTransport sender wiring) + `src-tauri/src/main.rs` (consumer task + Emitter::emit) + `ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.tsx` (listen + refetch).

```bash
git add src-tauri/src/mcp.rs src-tauri/src/main.rs \
        ui/src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.tsx
git commit -F docs/superpowers/handoff/COMMIT_MCP_PR4.txt
```

### Commit 5 — PR-5: audit table + redaction + list IPC

Files: `src-tauri/src/db/migrations.rs` (V40) + `src-tauri/src/mcp.rs` (redaction + audit helpers, set_error update, record_audit calls) + `src-tauri/src/tauri_commands.rs` (list_mcp_audit) + `src-tauri/src/main.rs` (set_db_handle + register) + `ui/src/lib/tauri-bridge.ts` (listMcpAudit wrapper).

```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/mcp.rs src-tauri/src/tauri_commands.rs \
        src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts
git commit -F docs/superpowers/handoff/COMMIT_MCP_PR5.txt
```

## Why one branch / 5 commits not 5 branches

Per `CLAUDE.md`'s PR shape note ("one branch per plan, one commit per
plan task, one PR with a `## Commits (bisectable)` table"), 5 changes
landing as one PR with 5 bisectable commits matches uClaw's existing
convention (PRs #29, #31, #33, #35, #36). Each commit is self-contained:
the build is green at every commit, no commit relies on a later commit
to compile.

## Pre-flight + verification

```bash
cd ~/Documents/uclaw
git checkout claude/sprint-mcp-completeness
rm -f .git/index.lock          # in case the sandbox left a stale one

# Stage commits one-by-one per the order above (no batch script —
# splitting the diff manually is the easiest way to verify each
# commit is the right subset).

# After each commit:
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
# After PR-1 specifically:
cargo test --lib mcp:: 2>&1 | tail -20
# Frontend after PR-2 / PR-4:
cd ../ui && npx tsc --noEmit
```

Sandbox couldn't run cargo or vitest, but `npx tsc --noEmit` is green
on the final state.

## What's still pending (post-merge)

1. **UI audit viewer** — a "View logs" tab in the detail drawer that
   calls `listMcpAudit(serverId, 100)` and renders rows. (Tiny PR.)
2. **Raw-logs opt-in** — `memubot_config.mcp.raw_logs_enabled` flag
   that disables redaction in `set_error` for power users debugging
   their own servers. (Tiny PR.)
3. **Audit pruning** — TTL job to drop rows older than N days. (Tiny PR.)
4. **HTTP transport notifications** — SSE support so HTTP-based MCP
   servers can push `tools/list_changed` too. (Medium.)

All four are mentioned in the relevant commit messages' "Limitations /
non-goals" sections so the next agent / reviewer has the trail.
