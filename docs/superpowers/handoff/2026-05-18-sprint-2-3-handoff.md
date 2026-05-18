# Sprint 2.3 — promote / demote facet IPC + UI (Mac-side commit hand-off)

Branch: `claude/sprint-2-3-promote-demote` (off the Sprint 2.2-merged
state of `main`, but locally branched off `85486b4` because the sandbox
proxy can't reach github to refetch).

The sandbox `.git/index.lock` is stuck again, so the commit needs to
land Mac-side. Sprint 2.2 staged content from the previous branch is
still in this working tree — when rebased onto post-2.2 main it should
be a no-op (identical content); only the Sprint 2.3 deltas apply.

## Pre-flight

```bash
cd ~/Documents/uclaw
git checkout claude/sprint-2-3-promote-demote
rm -f .git/index.lock
git pull origin main --rebase                  # bring in Sprint 2.2 merged content
git status                                      # should show only Sprint 2.3 deltas
```

If the rebase has conflicts in LearnedProfileTab.tsx / types.ts /
tauri-bridge.ts — accept the upstream (Sprint 2.2) version and re-apply
Sprint 2.3 additions. Easier path: `git reset --soft origin/main`, then
let TypeScript guide which hunks to add back (only Sprint 2.3 changes
should remain — promote/demote handlers, button group, type+bridge
additions).

## Commit

```bash
git commit -F docs/superpowers/handoff/COMMIT_SPRINT_2_3.txt
```

## Verify

```bash
cd src-tauri
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib learning_set_state_sql_tests 2>&1 | tail -10

cd ../ui
npx tsc --noEmit
npm test -- --run src/components/settings/LearnedProfileTab.test.tsx
```

`npx tsc --noEmit` already green in sandbox. Vitest sandbox blocked
by missing platform-specific rollup binary — must run Mac-side. Rust
unit tests need cargo (also Mac-side).

## What's in the commit

```
src-tauri/src/ipc.rs                                  # +LearningPromote/DemoteFacetInput DTOs
src-tauri/src/main.rs                                 # register 2 new commands
src-tauri/src/tauri_commands.rs                       # +2 IPC handlers + shared set_facet_state helper
                                                      # +6 unit tests for the SQL contract
ui/src/components/settings/LearnedProfileTab.tsx      # +promote/demote handlers + hover button cluster
ui/src/components/settings/LearnedProfileTab.test.tsx # +3 tests (promote, demote, hidden-on-candidate)
ui/src/lib/tauri-bridge.ts                            # +2 wrappers
ui/src/lib/types.ts                                   # +Promote/Demote inputs + shared outcome
```

## What this unlocks

Sprint 2.2 gave the user **read-only** visibility into the
FacetCache. Sprint 2.3 closes the **manual override** loop:

- Promote (chevron-up) → flips state to `active`. Available on
  provisional / candidate / forgotten rows (the latter as a recovery
  path — "I changed my mind, bring it back").
- Demote (chevron-down) → flips to `provisional`. Available on
  active and provisional rows.
- Dismiss (X) → flips to `forgotten`. Unchanged from Sprint 2.2.

All three are **soft overrides**: the next 30-min stability rebuild
re-evaluates each facet based on cumulative evidence, so the user
move sticks only if the underlying stability remains consistent.
This is consistent with openhuman's "user choice is signal, not a
ceiling" design and avoids the pin-forever footgun.

Buttons hide at rest and reveal on row-hover (group-hover pattern),
so the list stays visually quiet while still being click-friendly.

## Design notes worth flagging in review

- One IPC per intent (vs one IPC with `target_state: String` param)
  matches the existing `dismiss` pattern and keeps the call site
  self-documenting. Internally all three route through the shared
  `set_facet_state` helper so there's one UPDATE site.
- Test `idempotent_on_same_target_state` is intentional: promoting
  an already-active facet still bumps `updated_at`. This isn't an
  issue today (the timestamp doesn't drive any other behaviour
  beyond audit), but if a future change makes it sticky we should
  re-evaluate.
- Forgotten facets keep promote visible but hide dismiss + demote.
  This treats "forgotten → active" as a single click (recovery)
  rather than forcing a 2-step "promote to provisional → promote
  to active".
