# P2c-1 Passive-Recall Read Repoint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retire the redundant gbrain leg of the passive-recall system-prompt injection (the bucket_seal hybrid leg already covers the migrated + dual-written pages), behind a new read flag, after a one-time re-sync that guarantees bucket_seal completeness.

**Architecture:** A new `gbrain_read_repoint_enabled` flag (default on) gates the gbrain recall leg at both recall sites in `tauri_commands.rs`; the bucket_seal primary leg is untouched. A one-line marker bump (`v1 → v2`) makes the existing boot-spawned `migrate_gbrain_pages` run one more idempotent pass to backfill any drift before the leg retires.

**Tech Stack:** Rust, Tauri, `MemoryOsConfig` (serde), the bucket_seal adapter + `gbrain_page_migration` (P2b).

---

## Recon findings (complete — ground truth)

- **Site 1:** `async fn append_unified_recall(state: &AppState, delegate: &mut crate::agent::dispatcher::ChatDelegate, query: …)` (`tauri_commands.rs:~1811`). bucket_seal leg = `state.bucket_seal_adapter.recall_hybrid(query, None, 6).await` + `render_recall_block(BUCKET_SEAL_RECALL_MARKER, …)`. gbrain leg = `if let Some(adapter) = state.memory_adapters.get("gbrain") { let opts = RecallOpts{namespace:None,…}; match adapter.recall(query, 6, opts).await { … render_recall_block(GBRAIN_RECALL_MARKER, …) … } }` at `~1836`. `state` is in scope.
- **Site 2:** the spawn-prep block `let gbrain_recall_block_for_spawn: Option<String> = { let query = input.user_message.trim(); if !query.is_empty() { if let Some(adapter) = state.memory_adapters.get("gbrain") { … match adapter.recall(query, 6, opts).await { Ok(entries) if !entries.is_empty() => render_recall_block(GBRAIN_RECALL_MARKER, &entries, 1500).map(|block| format!("\n\n{block}")), … } } else { None } } else { None } };` (`tauri_commands.rs:~11149`). `state` is in scope. (A sibling `bucket_seal_recall_block_for_spawn` block precedes it — leave it untouched.)
- **Config-read idiom:** `state.memubot_config.read().await.memory_os.<flag>` (mirror `unified_load_context_enabled` at `tauri_commands.rs:2256`). Read into a local `bool` before any `.await` so no `RwLockReadGuard` is held across the recall awaits.
- **`MemoryOsConfig`** struct + `default_*` fns in `src-tauri/src/memubot_config.rs`; manual `impl Default for MemoryOsConfig` at line `679` (entries `unified_load_context_enabled: true` ~764, `gbrain_dual_write_pages_enabled: true` ~768). The config has a `#[cfg(test)]` module with `*_defaults_*` / `deserializes_without_*` tests.
- **Re-sync:** `MIGRATION_MARKER_SLUG` is a single `const` in `src-tauri/src/memory_adapter/gbrain_page_migration.rs:10` referenced by `already_migrated`, `migrate_gbrain_pages`, and tests (via the const, not the literal). Bumping its value re-runs the boot migration once.

## Worktree setup

Worktree under `/Users/ryanliu/Documents/uclaw-worktrees/` on branch `claude/p2c-1-recall-read-repoint` off `origin/main`. Fresh-worktree build needs the gitignored resource placeholders:
```bash
WT=/Users/ryanliu/Documents/uclaw-worktrees/p2c-1-recall-read-repoint
mkdir -p "$WT/src-tauri/bunembed" "$WT/src-tauri/pyembed" "$WT/src-tauri/gbrain-source"
touch "$WT/src-tauri/bunembed/bun" "$WT/src-tauri/pyembed/python"
echo x > "$WT/src-tauri/gbrain-source/placeholder.txt"
```
Baseline: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` clean before Task 1.

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/memubot_config.rs` | new `gbrain_read_repoint_enabled` flag + default + Default entry + tests |
| `src-tauri/src/memory_adapter/gbrain_page_migration.rs` | marker `v1 → v2` (one-time re-sync) |
| `src-tauri/src/tauri_commands.rs` | gate the gbrain recall leg at both sites behind the flag |

---

### Task 1: config flag `gbrain_read_repoint_enabled`

**Files:**
- Modify: `src-tauri/src/memubot_config.rs` (struct field; `default_*` fn near the other defaults; manual `impl Default` ~line 679/768; test module)

- [ ] **Step 1: Add the struct field**

In the `MemoryOsConfig` struct, near `gbrain_dual_write_pages_enabled`:

```rust
    /// P2c-1 — when on, gbrain knowledge READS are served from the adapter
    /// (bucket_seal), not gbrain. Gates the redundant passive-recall gbrain leg
    /// (the bucket_seal hybrid leg already surfaces the migrated + dual-written
    /// pages). Default ON = repointed; rollback = false restores the gbrain leg.
    /// Independent of `gbrain_dual_write_pages_enabled` (write side).
    #[serde(default = "default_gbrain_read_repoint_enabled")]
    pub gbrain_read_repoint_enabled: bool,
```

- [ ] **Step 2: Add the default fn**

Near `default_gbrain_dual_write_pages_enabled`:

```rust
/// P2c-1 — gbrain read repoint defaults ON (reads served from the adapter).
/// See `MemoryOsConfig::gbrain_read_repoint_enabled`.
fn default_gbrain_read_repoint_enabled() -> bool {
    true
}
```

- [ ] **Step 3: Add to the manual `impl Default`**

In `impl Default for MemoryOsConfig` (line ~679), beside `gbrain_dual_write_pages_enabled: true,` (~768):

```rust
            gbrain_read_repoint_enabled: true,
```

- [ ] **Step 4: Add default tests**

In the `#[cfg(test)]` module (beside the `gbrain_dual_write_pages_enabled` default tests):

```rust
#[test]
fn gbrain_read_repoint_enabled_defaults_on() {
    assert!(default_gbrain_read_repoint_enabled());
    assert!(MemoryOsConfig::default().gbrain_read_repoint_enabled);
}

#[test]
fn memory_os_deserializes_without_gbrain_read_repoint_field() {
    let cfg: MemubotConfig = serde_json::from_str("{}").unwrap();
    assert!(cfg.memory_os.gbrain_read_repoint_enabled);
}
```

> Match the exact type/name used by the sibling `deserializes_without_*` test (it may deserialize `MemoryOsConfig` directly or the top-level config — copy that test's shape, just swapping the asserted field). If the sibling uses a different top-level type than `MemubotConfig`, use that.

- [ ] **Step 5: Build + test**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/p2c-1-recall-read-repoint/src-tauri && cargo test --lib gbrain_read_repoint 2>&1 | tail -10`
Expected: PASS. Then `cargo build 2>&1 | grep -E "^error" | head` → empty.

- [ ] **Step 6: Commit**

EXPLICIT paths only (never `git add -A`/`.` — gitignored build placeholders must not be committed). No `--no-verify`.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/p2c-1-recall-read-repoint
git add src-tauri/src/memubot_config.rs
git commit -m "feat(config): gbrain_read_repoint_enabled (default on) (P2c-1)"
```

---

### Task 2: re-sync marker bump (v1 → v2)

**Files:**
- Modify: `src-tauri/src/memory_adapter/gbrain_page_migration.rs:10`

- [ ] **Step 1: Bump the marker constant**

Change line 10:

```rust
// before:
const MIGRATION_MARKER_SLUG: &str = "__gbrain_pages_migrated_v1__";
// after:
// P2c-1 re-sync: bumped v1 → v2 so the boot migration runs one more full
// idempotent pass, backfilling any gbrain page not yet in bucket_seal before the
// passive-recall gbrain leg retires. Sets v2 on success → skips thereafter.
const MIGRATION_MARKER_SLUG: &str = "__gbrain_pages_migrated_v2__";
```

No other change — `already_migrated`, `migrate_gbrain_pages`, the `app.rs` boot spawn, and all tests reference the `const`, so they are unaffected.

- [ ] **Step 2: Build + the migration tests**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/p2c-1-recall-read-repoint/src-tauri && cargo test --lib gbrain_page_migration 2>&1 | tail -12`
Expected: existing tests still pass (they use the const — the bump is transparent; e.g. `already_migrated_returns_true_when_marker_present` stores `MIGRATION_MARKER_SLUG` then asserts true, which still holds).
Run: `cargo build 2>&1 | grep -E "^error" | head` → empty.

- [ ] **Step 3: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/p2c-1-recall-read-repoint
git add src-tauri/src/memory_adapter/gbrain_page_migration.rs
git commit -m "feat(memory_adapter): bump gbrain page-migration marker v1→v2 — P2c-1 re-sync"
```

---

### Task 3: gate the gbrain recall leg at both sites

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (Site 1 `append_unified_recall` ~1836; Site 2 `gbrain_recall_block_for_spawn` ~11149)

- [ ] **Step 1: Read both sites**

Read `tauri_commands.rs` ~1811–1875 (`append_unified_recall`) and ~11135–11185 (the `gbrain_recall_block_for_spawn` block) to confirm the exact bounds of each gbrain leg and that `state` is in scope at each.

- [ ] **Step 2: Gate Site 1 (`append_unified_recall`)**

The gbrain leg is `if let Some(adapter) = state.memory_adapters.get("gbrain") { …RecallOpts… match adapter.recall(query, 6, opts).await { … } }`. Just before it (after the bucket_seal leg), add the flag read; wrap the gbrain leg:

```rust
    // P2c-1 — when the read repoint is on (default), the bucket_seal leg above
    // already covers the migrated + dual-written pages; skip the redundant gbrain leg.
    let gbrain_read_repoint = state
        .memubot_config
        .read()
        .await
        .memory_os
        .gbrain_read_repoint_enabled;
    if !gbrain_read_repoint {
        if let Some(adapter) = state.memory_adapters.get("gbrain") {
            // …existing gbrain leg, unchanged…
        }
    }
```

(Read `gbrain_read_repoint` into the `bool` local first — the guard drops at the `;` — so no `RwLockReadGuard` is held across the `adapter.recall(...).await`.)

- [ ] **Step 3: Gate Site 2 (`gbrain_recall_block_for_spawn`)**

The block computes `Option<String>`. Gate so it is `None` when the repoint is on. Read the flag before the block, then short-circuit:

```rust
    let gbrain_read_repoint = state
        .memubot_config
        .read()
        .await
        .memory_os
        .gbrain_read_repoint_enabled;
    let gbrain_recall_block_for_spawn: Option<String> = if gbrain_read_repoint {
        None
    } else {
        let query = input.user_message.trim();
        if !query.is_empty() {
            if let Some(adapter) = state.memory_adapters.get("gbrain") {
                // …existing inner logic, unchanged…
            } else {
                None
            }
        } else {
            None
        }
    };
```

(Preserve the existing inner block verbatim — only the outer `if gbrain_read_repoint { None } else { …existing… }` wrapper is added. The sibling `bucket_seal_recall_block_for_spawn` block is untouched.)

- [ ] **Step 4: Build + clippy**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/p2c-1-recall-read-repoint/src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
Run: `cargo clippy --lib 2>&1 | grep -E "^error" | head` → empty.

Watch for: a `RwLockReadGuard` held across `.await` (read the flag into a `bool` first); an unused-variable warning if a site already had a config read you can reuse (reuse it rather than double-read).

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/p2c-1-recall-read-repoint
git add src-tauri/src/tauri_commands.rs
git commit -m "feat(memory): gate gbrain passive-recall leg behind gbrain_read_repoint_enabled (P2c-1)"
```

---

### Task 4: Whole-slice verification

**Files:** none.

- [ ] **Step 1:** `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
- [ ] **Step 2:** `cargo test --lib memubot_config 2>&1 | grep "test result" | tail -1` (incl. new default tests) and `cargo test --lib gbrain_page_migration 2>&1 | grep "test result" | tail -1` → green.
- [ ] **Step 3:** `cargo clippy --lib 2>&1 | grep -E "^error" | head` → empty.
- [ ] **Step 4:** confirm the gate is at both sites: `grep -n "gbrain_read_repoint_enabled\|gbrain_read_repoint" src-tauri/src/tauri_commands.rs` shows two reads (Site 1 + Site 2); `grep -n "migrated_v2" src-tauri/src/memory_adapter/gbrain_page_migration.rs` shows the bumped marker.
- [ ] **Step 5:** `gitnexus_detect_changes()` per CLAUDE.md before the PR.

## Adjacent-edit checklist (PR body)

- **`MemoryOsConfig` new field** is `#[serde(default)]` + added to the manual `impl Default` → backward-compatible (deserialize-without-field test added).
- **Marker bump** triggers one extra idempotent boot migration pass on first launch after deploy (expected; one-time).
- No migration (schema), no new Tauri command, no new dependency. gbrain-leg code retained (gated) — deletion is P2d.

## PR shape

One branch `claude/p2c-1-recall-read-repoint`, one PR with a `## Commits (bisectable)` table (Tasks 1–3 = 3 commits). Title: `feat(memory): P2c-1 — passive-recall read repoint (retire redundant gbrain leg, gated) + v2 re-sync`. Body: bucket_seal hybrid leg already covers pages; gbrain leg gated off by default; v2 re-sync guarantees completeness; rollback = flip `gbrain_read_repoint_enabled`; P2c-2 (LLM read tools) + P2c-3 (UI/IPC) + P2d (delete + retire gbrain) later.

## Self-review notes

- **Spec coverage:** §1 gate + sites → Tasks 1+3; §2 re-sync → Task 2; testing → Task 1 tests + Task 4; rollback (flag retained) → Task 3 structure. ✔
- **Type consistency:** flag `gbrain_read_repoint_enabled: bool` identical across struct/default/Default/tests/both reads; config-read expr identical to the `unified_load_context_enabled` idiom. ✔
- **Bisectability:** Task 1 (flag, used by tests) compiles; Task 2 (marker bump) compiles independently; Task 3 (gate, uses the flag from Task 1) compiles. Order flag → bump → gate. ✔
- **Follow-the-recon items** (flagged, not placeholders): the sibling `deserializes_without_*` test's exact top-level type (Task 1 Step 4); the exact gbrain-leg bounds at each site (Task 3 Step 1 reads them first). Each has concrete guidance.
