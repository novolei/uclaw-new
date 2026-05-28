# Skeleton Cleanup P4 — Registries Kill + tool_families Extract · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the M3-T1 `RegistryHub` scaffold (1,777 LoC across 12 files in `src-tauri/src/registries/`) by (1) extracting the lone valuable data module `tool_families.rs` (+ its tests) to `agent/tool_families.rs`, (2) unwiring the 3 boot/proactive call sites, (3) removing the `registry_hub` field from `AppState` + `ProactiveState`/`ProactiveStateRefs` (9 passthrough sites), and (4) deleting the `registries/` directory. Zero behavior change — the registry hub was "fill-but-not-read" (write path landed in M3-T1 slice 1; the resolver-read path "slice 2" never landed, per the module's own doc).

**Architecture:** 5 bisectable commits in 1 PR. Each commit ends with `cargo build` clean + `cargo test --lib agent::` at baseline. The commit order is **safe-to-bisect**: extract first (no behavior change), then unwire the 2 call sites that DO things (preserves disk-tier flow), then strip the dead-passthrough fields, finally delete the directory.

**Tech Stack:** Rust 2021, Tauri 2, `cargo`, `grep` / `ripgrep` for caller verification.

---

## Background facts verified against HEAD `9be7cfdd` (main after P3 squash-merge)

### Total registries/ surface

```
src-tauri/src/registries/         12 files / 1,777 LoC
├── connectors.rs           35   (typed entry for MCP connectors slot)
├── entry.rs                37   (Registry entry trait)
├── hub.rs                 605   (RegistryHub + sync_skills/models/tools/builtin)  <-- the central scaffold
├── mod.rs                  63   (re-exports)
├── models.rs               44   (typed entry for models slot)
├── resolver.rs            285   (M3-T2 Capability Mesh resolver — 0 production callers)
├── skills.rs               31   (typed entry for skills slot)
├── store.rs               311   (Registry<E> container type)
├── themes.rs               35   (typed entry for themes slot)
├── tool_families.rs       174   (jcode-inspired ToolFamilyCard cards — KEEP, extract)
├── tool_families_tests.rs 120   (tests for above — KEEP, extract)
└── tools.rs                37   (typed entry for tools slot)
```

### Cross-tree consumer surface (3 files only)

**`src-tauri/src/app.rs`** (2 sites — field declaration + init):
- `app.rs:281` (stale doc-comment fragment) + `app.rs:285`: `pub registry_hub: crate::registries::RegistryHub,`
- `app.rs:954`: `registry_hub: crate::registries::RegistryHub::new(),`

**`src-tauri/src/main.rs`** (functional + doc):
- `main.rs:124-199`: boot-sync `spawn` block that calls `sync_skills_from_registry`, `sync_models_from_provider_service`, `register_builtin_tools`, then logs `hub.counts()` for the `[M3-T1] hub-counts after boot` line. ALL OF THIS BLOCK GOES (it produces data into a structure that nothing reads).
- `main.rs:131-133`: 3 doc-comment lines describing the slot model. Go with the spawn block.
- `main.rs:481` (in a different doc-comment context). Keep — generic mention.

**`src-tauri/src/proactive/service.rs`** (7 sites of `registry_hub` field passthrough + 1 Bundle 23 sync call):
- `:420`: `registry_hub: crate::registries::RegistryHub,` (field on `ProactiveState`)
- `:564-565`: doc-comment + field on `ProactiveStateRefs`
- `:628`: `registry_hub: crate::registries::RegistryHub,` (field passthrough in `ProactiveStateRefs::new` parameter)
- `:700`: `registry_hub,` (passed into constructor)
- `:750`: `registry_hub: self.registry_hub.clone(),` (cloned into refs)
- `:2143-2167`: **Bundle 23 same-session resync block** — persists learned skill to disk, rescans disk-tier `skills_registry`, **then syncs the hub Skills slot**. Steps 1-2-4 (persist + rescan + log) STAY; only step 3 (`sync_skills_from_registry` call at :2148) and its log mention of `hub_total` go. Simplified log line keeps the "disk-tier rescan done" message.
- `:3175`: `crate::registries::RegistryHub::new(),` (another construction site — verify and remove)

### Internal registry types — zero cross-tree consumers (verified)

```
grep -rn "Registry<\|ConnectorEntry\|ModelEntry\|SkillEntry\|ThemeEntry\|ToolEntry\|RegistryError" src-tauri/src/ --include="*.rs" | grep -v "src/registries/"
```
Returns ONLY:
- `app.rs:281` — doc comment fragment mentioning `Registry<E>` (will go with the AppState field removal).
- `skill_md_parse/mod.rs:26` — `(M3-T1 SkillEntry) lives in M3-T8 commit 2` doc comment (stale forward ref to a commit that never landed). Update or remove during Task 5 cleanup.
- `main.rs:131-133` — 3 doc-comment slot-overview lines (go with the spawn block).

### tool_families.rs — zero cross-tree consumers (extraction target)

```
grep -rn "ToolFamilyCard\|jcode_inspired_tool_family_cards\|tool_families::" src-tauri/src/ --include="*.rs" | grep -v "src/registries/"
```
Returns: **empty**. The module is pure metadata, no current callers — extracted **per Open Decision #3** to preserve the jcode-inspired domain knowledge for the future Pi-style `AgentApi` handle (ADR §6.5 schema role, analogous to how `plugin_manifest/schema.rs` was preserved in P2).

### Live wire path summary (M3-T1 slice 1 = write-only)

The `hub.rs` module doc explicitly states (lines 28-54):
> "Slice 1 scope: Hub struct with all 5 `Arc<RwLock<Registry<E>>>` slots; `sync_skills_from_registry` bridge; call site in `AppState::new`; tests covering the sync path. **What's NOT in slice 1**: Tools wire-up (slice 2), Connectors wire-up (slice 3), Models wire-up (slice 2), Themes wire-up (slice 4), Resolver invocation in production code paths (slice 2)."

`grep -rn "\bResolver\b\|use crate::registries::resolver" src-tauri/src/ --include="*.rs" | grep -v "src/registries/"` → **empty** (no Resolver invocation anywhere in production).

### Baselines

- `cargo build` (post-P3 main HEAD `9be7cfdd`): green / 48 warnings.
- `cargo test --lib agent::`: 759 passed / 2 pre-existing failures (`shell::tests::test_daemon_mode_approval_unchanged`, `skill_marketplace::tests::truncate_for_error_long`).
- `cargo test --lib` total: ~3044 passed / ~7 pre-existing failures.

---

## Pre-flight (before Task 1)

1. **Confirm main baseline**: `git -C /Users/ryanliu/Documents/uclaw status -sb` → `## main...origin/main`, in sync at `9be7cfdd`.

2. **Create the worktree + symlinks** (parent repo has gitignored `gbrain-source`, `pyembed`, `bunembed` that the build needs):

```bash
git worktree add -b claude/skeleton-cleanup-p4-registries-kill \
    /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill main
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
      /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/gbrain-source
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
      /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/pyembed
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
      /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/bunembed
```

3. **Baseline verifications inside the worktree**:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo build 2>&1 | tail -3
# expect: Finished, no errors

cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
# expect: 759 passed / 2 failed
```

All paths in tasks below are relative to the worktree.

---

## Task 1: Extract `registries/tool_families.rs` + tests → `agent/tool_families.rs`

**Files:**
- Move: `src-tauri/src/registries/tool_families.rs` (174 LoC) → `src-tauri/src/agent/tool_families.rs`
- Move: `src-tauri/src/registries/tool_families_tests.rs` (120 LoC) → `src-tauri/src/agent/tool_families_tests.rs`
- Modify: `src-tauri/src/registries/mod.rs` — remove `pub mod tool_families;` + `#[cfg(test)] mod tool_families_tests;` + any re-exports of `ToolFamilyCard` / `JCODE_INSPIRED_TOOL_FAMILY_CARDS` / `jcode_inspired_tool_family_cards`.
- Modify: `src-tauri/src/agent/mod.rs` — add `pub mod tool_families;` + `#[cfg(test)] mod tool_families_tests;` in alphabetical position (after `tool_dispatch` / `tool_budget`, before `trajectory`).

The tests file uses `super::tool_families::*` style imports; this works identically in the new home (it's still `super::` from the same parent mod position).

### Steps

- [ ] **Step 1.1: Verify zero cross-tree consumers (RED GATE)**

```
grep -rn "ToolFamilyCard\|jcode_inspired_tool_family_cards\|JCODE_INSPIRED_TOOL_FAMILY_CARDS\|tool_families::" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/ --include="*.rs" | grep -v "src/registries/tool_families"
```

Expected: **empty** (zero callers outside `tool_families.rs` itself + its tests). Any non-empty result → STOP, BLOCKED.

- [ ] **Step 1.2: Move both files (preserve git history)**

```
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill mv \
    src-tauri/src/registries/tool_families.rs src-tauri/src/agent/tool_families.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill mv \
    src-tauri/src/registries/tool_families_tests.rs src-tauri/src/agent/tool_families_tests.rs
```

Verify:
```
ls /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/agent/tool_families.rs
ls /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/agent/tool_families_tests.rs
ls /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/registries/tool_families.rs 2>&1
# expect: "No such file or directory" for the registries/ paths
```

- [ ] **Step 1.3: Wire into `agent/mod.rs`**

Read `agent/mod.rs`. The P3 work added `pub mod tool_budget;` and `pub mod trajectory;` already. Add `pub mod tool_families;` at the correct alphabetical position (between `tool_dispatch` and `tool_budget` if those are present — verify the actual ordering convention in the file).

Also add `#[cfg(test)] mod tool_families_tests;` adjacent to it (matching the convention used for other test files in agent/ if any; otherwise place it just below `pub mod tool_families;`).

- [ ] **Step 1.4: Unwire from `registries/mod.rs`**

Read `registries/mod.rs`. Remove:
- The `pub mod tool_families;` declaration.
- The `#[cfg(test)] mod tool_families_tests;` declaration.
- Any `pub use tool_families::*` or `pub use tool_families::{ToolFamilyCard, JCODE_INSPIRED_TOOL_FAMILY_CARDS, jcode_inspired_tool_family_cards}` re-exports.

KEEP all other declarations (`hub`, `store`, `resolver`, etc.) — they go in Task 5.

Verify:
```
grep -n "tool_families" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/registries/mod.rs
```
Expected: **empty**.

- [ ] **Step 1.5: Build + run the moved tests (GREEN GATE)**

```
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo test --lib agent::tool_families 2>&1 | tail -5
```

Expected: empty errors; tests pass (whatever count tool_families had — `wc -l tool_families_tests.rs` / 120 lines suggests several tests).

```
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```

Expected: 759+N passed / 2 failed where N is the number of tool_families tests now under `agent::`. The pre-existing 2 failures unchanged.

(If the tests file had `mod tool_families_tests` content like `use super::tool_families::*;`, it should resolve identically inside `agent/`. If a compile error mentions paths, inspect the file's import lines.)

- [ ] **Step 1.6: Commit**

```
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill add -A \
    src-tauri/src/registries/tool_families.rs \
    src-tauri/src/registries/tool_families_tests.rs \
    src-tauri/src/agent/tool_families.rs \
    src-tauri/src/agent/tool_families_tests.rs \
    src-tauri/src/registries/mod.rs \
    src-tauri/src/agent/mod.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill commit -m "$(cat <<'EOF'
refactor(agent): extract registries/tool_families.rs to agent/ (P4.1 of 阶段 2)

The jcode-inspired ToolFamilyCard cards (20+ family entries describing
filesystem/shell/agent/memory tool groupings, capability tags, policy
tags, event profiles, cost/reliability tiers) carry real domain
knowledge. Zero cross-tree consumers today (verified by grep), but
preserved for the future Pi-style AgentApi handle (ADR §6.5) — same
treatment as plugin_manifest/schema.rs got in P2.

- registries/tool_families.rs (174 LoC) -> agent/tool_families.rs
- registries/tool_families_tests.rs (120 LoC) -> agent/tool_families_tests.rs
- registries/mod.rs unwired
- agent/mod.rs wired (pub mod tool_families; + #[cfg(test)] mod tool_families_tests;)

Zero behavior change — files moved, no content edits. cargo build clean;
agent::tool_families tests pass at the new path. Pre-existing 2 failures
in shell:: and skill_marketplace:: unchanged.

First commit of P4 — clears the way for killing the remaining
registries/ subtree (M3-T1 slice 1 RegistryHub scaffold, never wired
to a downstream reader).
EOF
)"
```

Record the commit SHA. Continue to Task 2.

---

## Task 2: Remove Bundle 23 hub sync block from `proactive/service.rs`

**Goal**: Remove the `sync_skills_from_registry` call at `proactive/service.rs:2148-2160` from the Bundle 23 "same-session skill visibility" flow. The persist-to-disk + disk-tier rescan + log steps stay intact — only the hub sync (which writes into a structure no one reads) goes.

**Files:**
- Modify: `src-tauri/src/proactive/service.rs` — at lines ~2143-2167 (verify with `grep -n` first):
  - REMOVE: `let hub_count = match crate::registries::sync_skills_from_registry(...)` block (3-line `let` + the `Ok(n) => n` / `Err(e) => { tracing::warn!(...); 0 }` arms).
  - SIMPLIFY: the `tracing::info!` log line at the end of the block — drop the `hub_total = hub_count` field; keep `discovered = discover_count` + `skill_name` + `path`. Update the message string to remove the "+ hub Skills resync" phrasing — e.g., change `"[Bundle 23] same-session skill visibility: disk-tier rescan + hub Skills resync done"` → `"[Bundle 23] same-session skill visibility: disk-tier rescan done"`.
- KEEP everything else in the block intact (persist, rescan, surrounding `if let Some(app)` block at line 2169+, etc.).

### Steps

- [ ] **Step 2.1: Locate the Bundle 23 block precisely**

```
grep -n "Bundle 23\|hub_count\|sync_skills_from_registry\|hub Skills resync" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/proactive/service.rs | head -10
```

Read lines ~2140-2175 (the full Bundle 23 block) to understand exact context.

- [ ] **Step 2.2: Edit the block**

Use the Edit tool to remove the `let hub_count = match ...` arm + the corresponding `hub_total = hub_count` log field + update the message string. Be precise — the `match` arm has its own `Ok(n) => n`, `Err(e) => { tracing::warn!(...); 0 }` body that must go entirely.

After the edit, the block should:
1. Still call `skills_registry.write().await.discover()` (disk-tier rescan) — KEEP.
2. Still emit a single `tracing::info!` log line — but mention only "disk-tier rescan done".
3. Continue to the `if let Some(ref app) = refs.app_handle` block as before — KEEP.

- [ ] **Step 2.3: Verify the unwire**

```
grep -n "sync_skills_from_registry\|hub_count\|hub_total" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/proactive/service.rs
```

Expected: **empty** (this call site is now gone; field passthrough `registry_hub` still exists — that's Task 4).

- [ ] **Step 2.4: Build (GREEN GATE) + regression check**

```
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```

Expected: empty errors; 759+N pass / 2 fail (N from Task 1's tool_families tests).

If a proactive-specific test fails, inspect — the disk-tier rescan path should still work; only the unread hub write was removed.

- [ ] **Step 2.5: Commit**

```
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill add -A src-tauri/src/proactive/service.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill commit -m "$(cat <<'EOF'
refactor(proactive): remove dead hub sync from Bundle 23 same-session resync (P4.2 of 阶段 2)

The Bundle 23 block in service.rs ran 3 steps after a learned-skill
save: (1) persist to disk, (2) rescan disk-tier skills_registry, (3)
sync the M3-T1 RegistryHub Skills slot. Step (3) writes into a hub
that no production reader queries — the M3-T1 slice 2 resolver
invocation was never built (verified: grep for `Resolver` in production
returns empty). Steps (1)+(2) provide the actual same-session skill
visibility (disk-tier rescan is what skill_search consumes).

Removed: the `let hub_count = match crate::registries::sync_skills_from_registry(...)`
call site + its log field. Updated the trailing tracing::info! message
to drop the "+ hub Skills resync done" phrasing.

The registry_hub field still passes through ProactiveStateRefs — that
gets stripped in P4.4. lib.rs still has `pub mod registries;` — that
goes in P4.5. cargo build clean; agent:: tests at baseline +N (P4.1's
tool_families count).
EOF
)"
```

Record the commit SHA. Continue to Task 3.

---

## Task 3: Remove boot sync block from `main.rs`

**Goal**: Remove the `spawn` block at `main.rs:124-199` (the entire M3-T1 boot-time hub population: skill sync, model sync, builtin tool registration, counts log). After removal, `app_state.skills_registry` is still untouched, `provider_service` is still untouched — only their pipeline into the dead hub goes.

**Files:**
- Modify: `src-tauri/src/main.rs` — remove the `spawn` block at ~lines 124-199. Verify with `grep -n` first. Also remove the 3 doc-comment slot-overview lines just above (lines ~131-133 in the current snapshot, the "Slot coverage:" enumeration). Keep the surrounding comment context that's not specific to hub population.

### Steps

- [ ] **Step 3.1: Locate the boot sync block precisely**

```
grep -n "registry_hub\|sync_skills_from_registry\|sync_models_from_provider_service\|register_builtin_tools\|hub.counts\|M3-T1.*hub\|registry hub" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/main.rs | head -15
```

Read lines ~115-205 (the full block + a bit of surround) to confirm the exact bounds — the `{` at ~136 / the closing `}` at ~199.

- [ ] **Step 3.2: Edit main.rs to remove the block**

Use the Edit tool to remove the entire spawn block. Approximate bounds (verify in the file):
- Start: a doc-comment block above the `{` brace explaining the slot model (~lines 124-135).
- The `{ ... }` block proper (~lines 136-199).
- End: 1 blank line after the closing `}`.

The result: the file goes directly from whatever precedes the comment block (e.g., the `app_state` initialization area) to the next functional block (the `MemUClient` AppHandle attach at line 201+).

Sanity check: don't accidentally remove the `MemUClient` block — that's separate and starts with `// Attach AppHandle to MemUClient`.

- [ ] **Step 3.3: Verify the unwire**

```
grep -n "registry_hub\|sync_skills_from_registry\|sync_models_from_provider_service\|register_builtin_tools\|hub\.counts\|M3-T1.*hub-counts" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/main.rs
```

Expected: **empty** (the boot-sync references are gone). `registry_hub` may still appear in `tauri::generate_handler!` if anything references it there — verify:
```
grep -n "registries\b" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/main.rs
```
Expected: only `uclaw_core::registries::...` references that were inside the removed block (gone) — if any remain, inspect.

- [ ] **Step 3.4: Build (GREEN GATE) + regression check**

```
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```

Expected: empty errors; agent:: at post-Task-2 count.

- [ ] **Step 3.5: Commit**

```
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill add -A src-tauri/src/main.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill commit -m "$(cat <<'EOF'
refactor(boot): remove dead M3-T1 hub boot-sync block from main.rs (P4.3 of 阶段 2)

The spawn block at main.rs:124-199 ran 4 calls at boot:
- sync_skills_from_registry — populated hub.skills from SkillsRegistry
- sync_models_from_provider_service — populated hub.models from ProviderService
- register_builtin_tools — populated hub.tools from a hardcoded catalog
- hub.counts() — diagnostic log line "[M3-T1] hub-counts after boot"

All four were write-only into a structure no production reader queries.
The hub.rs module doc itself documents this: "slice 1 just makes the
data available; calling the resolver from skill_search / load_skill is
slice 2" — slice 2 never landed.

The downstream subsystems are untouched:
- app_state.skills_registry still exists, still scanned at boot for
  disk-tier skill discovery.
- provider_service still exposes model providers.
- builtin tool catalog still drives ToolDispatch per-session.

What's gone: the 3 sync calls + 1 diagnostic log line, plus 3 doc-comment
lines above describing the slot model. The registry_hub field still
passes through AppState + ProactiveStateRefs — those get stripped in
P4.4. lib.rs still has `pub mod registries;` — that goes in P4.5.

cargo build clean; agent:: at post-P4.2 count.
EOF
)"
```

Record the commit SHA. Continue to Task 4.

---

## Task 4: Strip `registry_hub` field from `AppState` + `ProactiveState` / `ProactiveStateRefs`

**Goal**: Remove the 9 `registry_hub` field-declaration / field-passthrough / field-init sites across `app.rs` (2 sites) and `proactive/service.rs` (7 sites). After this commit, no code outside `src/registries/` mentions `RegistryHub` or `registry_hub` — Task 5 can then safely delete the `registries/` directory.

**Files:**
- Modify: `src-tauri/src/app.rs` — remove:
  - Line ~281: stale doc comment fragment mentioning `Registry<E>` (verify line — may have shifted slightly).
  - Line ~285: `pub registry_hub: crate::registries::RegistryHub,` field declaration on `AppState`.
  - Line ~954: `registry_hub: crate::registries::RegistryHub::new(),` in `AppState::new()`.
- Modify: `src-tauri/src/proactive/service.rs` — remove:
  - Line ~420: `registry_hub: crate::registries::RegistryHub,` field on `ProactiveState` (verify with grep — line may have shifted after Task 2's edit).
  - Lines ~564-565: doc-comment + field on `ProactiveStateRefs`.
  - Line ~628: parameter `registry_hub: crate::registries::RegistryHub,` in some constructor function.
  - Line ~700: `registry_hub,` in struct construction.
  - Line ~750: `registry_hub: self.registry_hub.clone(),` in another struct construction.
  - Line ~3175: `crate::registries::RegistryHub::new(),` in another construction site (verify).

### Steps

- [ ] **Step 4.1: Full survey post-Task-3 to confirm 9 sites + nothing else**

```
grep -n "registry_hub\|RegistryHub" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/ --include="*.rs" -r | grep -v "src/registries/"
```

Expected: ~9 hits across `app.rs` (2) + `proactive/service.rs` (7). Tasks 2 and 3 removed the functional call sites in those files; what remains is structural (field decls, field initializers, passthrough). If anything else appears, inspect.

- [ ] **Step 4.2: Edit `app.rs`**

Read `app.rs` around the recorded line numbers. Use Edit to:
- Remove the `pub registry_hub: ...` field declaration line.
- Remove the corresponding `registry_hub: crate::registries::RegistryHub::new(),` in `AppState::new()` (or whatever construction site).
- Remove any preceding doc-comment line that EXCLUSIVELY documents the `registry_hub` field. If a doc comment groups multiple fields, only excise the `registry_hub` mention; preserve the rest.

- [ ] **Step 4.3: Edit `proactive/service.rs`**

7 sites. Read context around each. For each site:
- **Field declarations** (`registry_hub: crate::registries::RegistryHub,`): remove the line. Remove preceding `///` doc comment line ONLY if it's exclusively about this field.
- **Function parameters** (e.g., `fn foo(..., registry_hub: crate::registries::RegistryHub, ...)`): remove the parameter — AND find every caller of `foo` to remove the corresponding argument. Use grep to find callers if there are any beyond Tasks 2-3 changes.
- **Struct construction** (`Foo { registry_hub: x, ... }` or `Foo { registry_hub, ... }`): remove the field initialization.
- **Clones in passthrough** (`self.registry_hub.clone()`): remove the line.

CRITICAL: after editing `ProactiveStateRefs::new` (or equivalent), the constructor signature changes. Find all callers and update them. Without this, the build will fail. The cargo error messages will guide you.

- [ ] **Step 4.4: Build (GREEN GATE) + regression check**

```
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```

Expected: empty errors; agent:: at post-P4.3 count.

If errors appear:
- "missing field `registry_hub` in initializer" → a struct construction site missed.
- "function takes N arguments but M were supplied" → a caller of a refactored constructor still passes the old `registry_hub` arg.

Fix each by following the cargo error messages.

- [ ] **Step 4.5: Final orphan sweep for the field**

```
grep -rn "registry_hub" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/ --include="*.rs"
```

Expected: **empty** (or only inside `src/registries/` which still has internal self-references — Task 5 deletes those).

```
grep -rn "RegistryHub" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/ --include="*.rs" | grep -v "src/registries/"
```

Expected: **empty** (no `RegistryHub` mentioned outside the to-be-deleted directory).

- [ ] **Step 4.6: Commit**

```
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill add -A src-tauri/src/app.rs src-tauri/src/proactive/service.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill commit -m "$(cat <<'EOF'
refactor(state): strip registry_hub from AppState + ProactiveState (P4.4 of 阶段 2)

After P4.2 + P4.3 removed the only call sites that wrote into the
RegistryHub (Bundle 23 same-session resync + boot-time M3-T1 hub
sync), the registry_hub field on AppState + ProactiveState +
ProactiveStateRefs is dead state. This commit strips the 9
field-declaration / field-init / passthrough sites:

- app.rs:
    * AppState.registry_hub field declaration removed
    * AppState::new initializer removed
    * stale doc-comment fragment about Registry<E> dropped
- proactive/service.rs:
    * ProactiveState.registry_hub field declaration removed
    * ProactiveStateRefs.registry_hub field declaration removed
    * constructor parameter removed + caller signature updated
    * struct construction sites cleaned (4 sites)

After this commit, `grep -rn "registry_hub\|RegistryHub" src/` outside
src/registries/ returns empty. lib.rs still has `pub mod registries;` —
that goes in P4.5 alongside the directory deletion.

cargo build clean; agent:: at post-P4.3 count.
EOF
)"
```

Record the commit SHA. Continue to Task 5.

---

## Task 5: Delete `registries/` directory + `pub mod registries;` declaration

**Goal**: Final cleanup. After Tasks 1-4, no code outside `src/registries/` references the module. This commit deletes the directory entirely + the `pub mod registries;` declaration in `lib.rs` + any stale doc comments referencing M3-T1 internals.

**Files:**
- Delete: `src-tauri/src/registries/` directory entirely (10 remaining files after Task 1 extracted tool_families: `connectors.rs`, `entry.rs`, `hub.rs`, `mod.rs`, `models.rs`, `resolver.rs`, `skills.rs`, `store.rs`, `themes.rs`, `tools.rs`).
- Modify: `src-tauri/src/lib.rs` — remove `pub mod registries;` (verify line with grep).
- Modify: `src-tauri/src/skill_md_parse/mod.rs:26` — update stale doc-comment referencing `M3-T1 SkillEntry` to drop the M3-T1 reference (the SkillEntry type no longer exists). Make the comment self-contained.

### Steps

- [ ] **Step 5.1: Confirm Tasks 1-4 are clean (final RED GATE)**

```
grep -rn "use crate::registries\|crate::registries\|registry_hub\|RegistryHub" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/ --include="*.rs" | grep -v "src/registries/"
```

Expected: **empty** (zero callers outside `registries/`). Any hit → STOP, BLOCKED (Tasks 2-4 incomplete).

```
grep -rn "ToolFamilyCard\|jcode_inspired_tool_family_cards\|tool_families::" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/ --include="*.rs" | grep -v "src/registries/\|src/agent/tool_families"
```

Expected: **empty** (Task 1's extraction landed; new home is `agent/tool_families.rs`).

- [ ] **Step 5.2: Delete the directory**

```
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill rm -r src-tauri/src/registries/
```

Verify:
```
ls /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/registries 2>&1
```
Expected: "No such file or directory".

- [ ] **Step 5.3: Remove `pub mod registries;` from `lib.rs`**

```
grep -n "pub mod registries" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/lib.rs
```

Read 3 lines before/after that line. Use Edit to remove the `pub mod registries;` line plus any preceding `///` doc-comment line that EXCLUSIVELY describes the registries module. If the comment groups multiple modules together, only touch the registries-specific token.

Verify:
```
grep -n "registries" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/lib.rs
```
Expected: **empty** (no `registries`-mentioning lines).

- [ ] **Step 5.4: Update the stale doc comment in `skill_md_parse/mod.rs:26`**

```
grep -n "SkillEntry\|M3-T1\|M3-T8" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/skill_md_parse/mod.rs
```

The line `//! (M3-T1 SkillEntry) lives in M3-T8 commit 2.` references types that no longer exist. Edit to either:
- Drop the parenthetical entirely (simplest), or
- Rephrase to describe the actual current architecture without the M3-T1 / SkillEntry references.

Either is acceptable — judgment call based on the surrounding sentence. Goal: the comment stays accurate after the deletion.

- [ ] **Step 5.5: Final ZERO-GAP sweep**

```
grep -rn "use crate::registries\|crate::registries\|RegistryHub\|registry_hub\|RegistryHubCounts\|SkillEntry\|ConnectorEntry\|ModelEntry\|ThemeEntry\|ToolEntry\|RegistryError\|sync_skills_from_registry\|sync_models_from_provider_service\|register_builtin_tools" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri/src/ --include="*.rs"
```

Expected: **empty**. ALL identifiers gone from the source tree.

- [ ] **Step 5.6: Build (GREEN GATE) + regression check**

```
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill/src-tauri && cargo test --lib 2>&1 | tail -5
```

Expected:
- Empty errors.
- agent:: count drops by however many tests lived in `registries/hub.rs::tests`, `registries/store.rs::tests`, `registries/resolver.rs::tests` etc. If `cargo test --lib agent::` count looks identical to post-P4.4 (i.e., the registries tests didn't live under `agent::`), that's also expected — registry tests sat under `registries::tests::` namespace.
- `cargo test --lib` total drops by the number of removed registry tests. Verify the failure count is unchanged (still 2 pre-existing) — no NEW failures from the deletion.

Compute the expected drop:
```
# Pre-deletion (before this commit): count tests in registries/
grep -c "#\[test\]\|#\[tokio::test\]" /Users/ryanliu/Documents/uclaw/src-tauri/src/registries/hub.rs /Users/ryanliu/Documents/uclaw/src-tauri/src/registries/resolver.rs /Users/ryanliu/Documents/uclaw/src-tauri/src/registries/store.rs 2>/dev/null
```
(Run against the parent repo's pre-P4 state if precise.)

- [ ] **Step 5.7: Commit**

```
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill add -A \
    src-tauri/src/registries/ \
    src-tauri/src/lib.rs \
    src-tauri/src/skill_md_parse/mod.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill commit -m "$(cat <<'EOF'
chore(registries): kill dead M3-T1 RegistryHub subtree (P4.5 of 阶段 2)

~1,605 LoC removed: registries/{hub,store,resolver,connectors,models,
themes,tools,skills,entry,mod}.rs (everything except tool_families,
which P4.1 extracted to agent/). Completes P4 of the 阶段 2 skeleton
cleanup series.

The full P4 arc:
- P4.1: extract tool_families.rs + tests -> agent/ (preserve domain data)
- P4.2: remove dead hub sync from Bundle 23 (proactive/service.rs)
- P4.3: remove dead boot-sync block (main.rs)
- P4.4: strip registry_hub field from AppState + ProactiveState
- P4.5: delete registries/ directory (this commit)

The M3-T1 wire-up was slice 1 only — the hub.rs module doc explicitly
documents the gap: "slice 1 just makes the data available; calling the
resolver from skill_search / load_skill is slice 2". Slice 2 never
landed; nothing in production reads from the hub. The boot sync calls
and Bundle 23 resync were producing data into a structure no one queries.

Strategically, the M3-T1 4-Registry architecture is superseded by the
ADR 2026-05-28 Pi-lightweight design's single AgentApi handle. The
jcode-inspired ToolFamilyCard domain knowledge survives at
agent/tool_families.rs (P4.1) as future schema for the AgentApi
handle — same treatment plugin_manifest/schema.rs got in P2.

Also updated skill_md_parse/mod.rs:26 to drop the stale "M3-T1
SkillEntry / M3-T8 commit 2" forward reference (SkillEntry type
no longer exists; M3-T8 commit 2 never landed).

cargo build clean; agent:: at post-P4.4 count; cargo test --lib total
drops by N (the registry-internal tests that vanished with the deletion).
EOF
)"
```

Verify final chain:
```
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill log --oneline 9be7cfdd..HEAD
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p4-registries-kill status -sb
```

Expected: 5 commits ahead of `main`; working tree clean.

---

## Self-Review

**1. Spec coverage (against assessment §1.D + §2.3 + Open Decision #3):**

- ✅ Extract `registries/tool_families.rs` → `agent/tool_families.rs` (per Open Decision #3 answered "agent/tool_families.rs") → Task 1.
- ✅ Remove proactive/service.rs Bundle 23 hub-sync call → Task 2.
- ✅ Remove main.rs boot-sync block → Task 3.
- ✅ Strip registry_hub field passthrough → Task 4.
- ✅ Delete registries/ directory → Task 5.

Net LoC: -1,605 (registries/ except tool_families, tests, and inlined logic). The assessment quoted -1,731 — the discrepancy is because the assessment counted `load.rs` (already killed in P2) + slight overcount on tool_families overlap. Plan corrects with precise per-file LoC.

**2. Placeholder scan:**
- No "TBD" / "TODO" / "fill in details" / "similar to Task N".
- Step 4.3 "find all callers and update them" is a structured instruction (cargo guides the work via build errors), not a placeholder.
- Step 5.4 "judgment call" wording for the M3-T1 doc-comment rephrase is intentional — the goal (drop the stale reference) is explicit.

**3. Type consistency:**
- `RegistryHub`, `registry_hub`, `ProactiveState`, `ProactiveStateRefs`, `AppState`, `sync_skills_from_registry`, `sync_models_from_provider_service`, `register_builtin_tools`, `ToolFamilyCard`, `JCODE_INSPIRED_TOOL_FAMILY_CARDS`, `jcode_inspired_tool_family_cards`, `Registry<E>`, `ConnectorEntry`/`ModelEntry`/`SkillEntry`/`ThemeEntry`/`ToolEntry`, `RegistryError`, `RegistryHubCounts` all named consistently across §"Background facts" and the 5 tasks.

No spec gaps, no placeholders, no type inconsistencies. Plan ready.

---

## Quick reference

- **Estimated time:** 0.5-1 person-day (5 mechanical commits, well-bounded).
- **Risk:** medium-low. Task 4 (field stripping) is the highest-risk single step — multiple constructor signatures change. `cargo build` is the gate; cargo will guide every missed site.
- **Files touched:**
  - Task 1: 6 (2 moves + 2 mod.rs wire/unwire)
  - Task 2: 1 (proactive/service.rs)
  - Task 3: 1 (main.rs)
  - Task 4: 2 (app.rs, proactive/service.rs)
  - Task 5: 12 (10 deletes + lib.rs + skill_md_parse/mod.rs)
- **Net LoC:** ~-1,605.
- **PR shape:** 1 worktree → 5 commits → 1 PR. Bisectable per-task (each commit builds green + agent:: at moving baseline). Squash-on-land per P1-P3 convention.
- **No new tests written.** No tests deleted from the live path — only registry-internal tests (in hub.rs, resolver.rs, store.rs, etc.) vanish with the directory. The `agent::tool_families` tests (from Task 1) are preserved at their new path.
- **No Open Decisions block P4.** Open Decision #3 (tool_families landing) was answered "agent/tool_families.rs (recommended)". All 5 commit choices align with assessment §2.3.

**Future**: After P4 lands, only P5 (memory cleanup, ~1,878 LoC) remains in the 阶段 2 series. P5 still needs Open Decision #1 answered — the `memory_policy` recency check (is there WIP HookBus / memory-bus rewiring in flight that would conflict with the kill?).
