# Sprint 2.0 + 2.1a — Mac-side commit hand-off

Sandbox can't write to `.git/` (stale `index.lock` from 09:36 UTC), so the
two commits below need to land from Mac. All file edits are already on disk
on branch `claude/sprint-2-wire-pipeline`.

## Pre-flight (one-time)

```bash
cd ~/Documents/uclaw
rm -f .git/index.lock      # clear the stale lock the sandbox left behind
git status                  # should show 7 modified files on claude/sprint-2-wire-pipeline
```

## Commit 1 — Sprint 2.0 + 2.1b (5 files)

```bash
git add src-tauri/src/agent/dispatcher.rs \
        src-tauri/src/app.rs \
        src-tauri/src/cost_store.rs \
        src-tauri/src/memubot_config.rs \
        src-tauri/src/tauri_commands.rs
git commit -F docs/superpowers/handoff/COMMIT_SPRINT_2_0.txt
```

## Commit 2 — Sprint 2.1a (2 files)

```bash
git add src-tauri/src/main.rs \
        src-tauri/src/proactive/service.rs
git commit -F docs/superpowers/handoff/COMMIT_SPRINT_2_1a.txt
```

## Verify

```bash
cd src-tauri
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib learning 2>&1 | tail -20
cargo test --lib cost_store 2>&1 | tail -10
```

Both commits are designed to compile independently (Commit 1 doesn't touch
`MemoryOsRuntimeConfig`; Commit 2 adds the `profile_md_path` field after).
The 89 Sprint 1 unit tests should remain green — wiring is additive and
guarded behind `learning_enabled`.

## What Sprint 2 unlocks

Before: producer + consumer existed, no callsites — FacetCache stayed
empty, PROFILE block never injected.

After: every user chat turn pushes 0+ candidates into the buffer; the
30-min tick rebuilds the FacetCache + writes PROFILE.md to disk; the
next system prompt sees the learned-profile block.
