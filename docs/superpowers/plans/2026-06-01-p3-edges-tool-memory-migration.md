# P3-edges tool_memory Store Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate `tool_memory`'s co-used-tools graph off the frozen `memory_graph` onto a new `memory_adapter::tool_stats` facade (per-tool stats) + the existing `edges.rs` (co-usage edges, by tool name), so the **last exempted memory_graph writer** retires and P4 can drop the freeze hook.

**Architecture:** New typed `tool_stats` facade homes `ToolNodeStats`; co-usage edges reuse `edges.rs` `relate`/`neighbors` keyed by tool name. One-time boot migration (disjoint from P3-skills via version-absence), then `ToolUsageMemoryManager`'s three methods convert sync→async and repoint to the facade behind `tool_memory_repoint_enabled` (default on; rollback restores memory_graph).

**Tech Stack:** Rust, Tauri, `memory_adapter::{tool_stats(new), edges}` + `BucketSealAdapter`, `MemoryGraphStore` (read-only migration source + rollback).

---

## Recon findings (complete — ground truth)

- **Struct: `ToolUsageMemoryManager`** (`proactive/tool_memory.rs`), `store: Arc<MemoryGraphStore>`, `::new(store)`; constructed at `proactive/service.rs:643` (`ToolUsageMemoryManager::new(memory_graph_store.clone())`), stored on the service, cloned at `service.rs:785`.
- **`ToolNodeStats`** (stored in node metadata, tool_memory.rs:81): `{ total_uses: u64, success_count: u64, failure_count: u64, total_latency_ms: u64, output_sizes: Vec<u64>, parameter_fingerprints: HashMap<String,u64>, last_used_at: String }`. The public **`ToolStats`** (`total_uses, success_rate, avg_latency_ms, typical_output_size, common_parameters, last_used_at, co_used_tools`) is **derived** in `get_tool_stats` from `ToolNodeStats` + `get_co_used_tools`.
- **Write sites:** `record_tool_usage(space_id, &ToolUsageRecord)` (tool_memory.rs:124; caller `service.rs:882`, `let _ = tool_memory.record_tool_usage(...)`) → `find_or_create_tool_node` (uuid node, kind=`Procedure`, title=tool_name) + accumulate `ToolNodeStats` → `update_node`. `record_co_usage(space_id, &[String])` (tool_memory.rs:181) → `create_edge(RelatesTo)` per pair. **Recon `record_co_usage`'s production caller** (`grep -rn "record_co_usage" src-tauri/src/` — non-test): if none, implement W2 faithfully anyway (harmless; edges stay empty in prod).
- **Read:** `get_tool_stats(space_id, tool_name) -> Option<ToolStats>` (tool_memory.rs:224) → `find_tool_node_id` → `get_node` metadata → `ToolNodeStats` → derive → `get_co_used_tools(space_id, node_id)` (SQL JOIN on memory_edges). Consumers: `proactive/proactive_recall.rs:192` (`.get_tool_stats(&context.space_id, tool_name)`) + an internal call at tool_memory.rs:355.
- **`edges.rs`:** `relate(adapter, from, to, kind) -> Result<()>` (idempotent undirected, key sorts endpoints); `neighbors(adapter, node, kind: Option<&str>) -> Result<Vec<String>>`. Namespace `"edges"`. No change needed.
- **`skills.rs`** is the facade template (space-qualified `{space}\u{1}{key}`, `serde_json` value, `adapter.store/get`, in-memory `InMemoryAdapter` test double). P3-skills shipped the analogous structure.
- **Config / threading:** `MemoryOsConfig` flag pattern (mirror `skill_store_repoint_enabled`); `ProactiveService`/`ProactiveStateRefs` already carry `skill_adapter` (`Arc<dyn MemoryAdapter>`) + `MemoryOsRuntimeConfig.skill_store_repoint_enabled` (P3-skills) — reuse the same adapter handle, add the new flag. `app.rs` has the `migrate_skills` boot spawn to sit beside.

## Worktree setup

Worktree under `/Users/ryanliu/Documents/uclaw-worktrees/` on `claude/p3-edges-tool-memory-migration` off `origin/main`. Placeholders:
```bash
WT=/Users/ryanliu/Documents/uclaw-worktrees/p3-edges-tool-memory-migration
mkdir -p "$WT/src-tauri/bunembed" "$WT/src-tauri/pyembed" "$WT/src-tauri/gbrain-source"
touch "$WT/src-tauri/bunembed/bun" "$WT/src-tauri/pyembed/python"
echo x > "$WT/src-tauri/gbrain-source/placeholder.txt"
```
Baseline `cargo build` clean before Task 1.

## File structure

| File | Responsibility |
|---|---|
| `memory_adapter/tool_stats.rs` (new) | `ToolStatsRecord` + `put_stats`/`get_stats` + tests |
| `memory_adapter/mod.rs` | `pub mod tool_stats;` |
| `proactive/tool_memory_migration.rs` (new) | stat-node + co-usage-edge migration + marker + tests |
| `app.rs` | boot spawn |
| `memubot_config.rs` | `tool_memory_repoint_enabled` flag |
| `proactive/tool_memory.rs` | 3 sites repointed, sync→async; `::new` gains adapter + flag |
| `proactive/service.rs` | thread adapter/flag; `.await` async calls |
| `proactive/proactive_recall.rs` | `.await` async `get_tool_stats` |

---

### Task 1: `tool_stats` facade

**Files:** Create `src-tauri/src/memory_adapter/tool_stats.rs`; modify `memory_adapter/mod.rs` (`pub mod tool_stats;`).

- [ ] **Step 1: Create the facade** (mirror `skills.rs`'s key/store/get pattern — Read it first):

```rust
//! P3-edges — per-tool usage stats facade over `MemoryAdapter`. Stores the raw
//! ToolNodeStats accumulator (derived ToolStats fields are computed by the caller).
//! Space-qualified key in the "tool_stats" namespace. Mirrors skills.rs.

use std::collections::HashMap;
use std::sync::Arc;
use crate::memory_adapter::{MemoryAdapter, MemoryCategory};

const TOOL_STATS_NAMESPACE: &str = "tool_stats";

fn stats_key(space: &str, tool_name: &str) -> String {
    format!("{space}\u{1}{tool_name}")
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolStatsRecord {
    pub space: String,
    pub tool_name: String,
    #[serde(default)] pub total_uses: u64,
    #[serde(default)] pub success_count: u64,
    #[serde(default)] pub failure_count: u64,
    #[serde(default)] pub total_latency_ms: u64,
    #[serde(default)] pub output_sizes: Vec<u64>,
    #[serde(default)] pub parameter_fingerprints: HashMap<String, u64>,
    #[serde(default)] pub last_used_at: String,
}

pub async fn put_stats(adapter: &Arc<dyn MemoryAdapter>, rec: &ToolStatsRecord) -> anyhow::Result<()> {
    let content = serde_json::to_string(rec)?;
    adapter.store(TOOL_STATS_NAMESPACE, &stats_key(&rec.space, &rec.tool_name), &content, MemoryCategory::Core, None).await
}

pub async fn get_stats(adapter: &Arc<dyn MemoryAdapter>, space_id: &str, tool_name: &str) -> anyhow::Result<Option<ToolStatsRecord>> {
    match adapter.get(TOOL_STATS_NAMESPACE, &stats_key(space_id, tool_name)).await? {
        Some(e) => Ok(serde_json::from_str::<ToolStatsRecord>(&e.content).ok()),
        None => Ok(None),
    }
}
```

(Confirm `MemoryCategory` import path + `adapter.store`/`get` signatures match `skills.rs`.)

- [ ] **Step 2: Register** `pub mod tool_stats;` in `memory_adapter/mod.rs` (beside the other `pub mod`s).

- [ ] **Step 3: Tests** (copy `skills.rs`'s `InMemoryAdapter` double into this module's `#[cfg(test)]`, or factor — match how P3-skills did it; the simplest is to copy the double, consistent with the codebase pattern):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // <-- in-memory MemoryAdapter double (copy from skills.rs tests)

    fn rec(space: &str, tool: &str, uses: u64) -> ToolStatsRecord {
        ToolStatsRecord { space: space.into(), tool_name: tool.into(), total_uses: uses, success_count: uses, failure_count: 0, total_latency_ms: 10*uses, output_sizes: vec![], parameter_fingerprints: HashMap::new(), last_used_at: "t".into() }
    }

    #[tokio::test]
    async fn put_get_round_trips() {
        let a = InMemoryAdapter::new();
        put_stats(&a, &rec("sp","write_file",3)).await.unwrap();
        let got = get_stats(&a,"sp","write_file").await.unwrap().unwrap();
        assert_eq!((got.total_uses, got.success_count), (3,3));
    }

    #[tokio::test]
    async fn space_isolation_and_absent() {
        let a = InMemoryAdapter::new();
        put_stats(&a, &rec("s1","t",1)).await.unwrap();
        put_stats(&a, &rec("s2","t",9)).await.unwrap();
        assert_eq!(get_stats(&a,"s1","t").await.unwrap().unwrap().total_uses, 1);
        assert_eq!(get_stats(&a,"s2","t").await.unwrap().unwrap().total_uses, 9);
        assert!(get_stats(&a,"s1","absent").await.unwrap().is_none());
    }
}
```

- [ ] **Step 4: Test + build** — `cd src-tauri && cargo test --lib memory_adapter::tool_stats 2>&1 | tail -10` green; `cargo build 2>&1 | grep -E "^error" | head` empty.

- [ ] **Step 5: Commit** (paths: tool_stats.rs + mod.rs): `feat(memory_adapter): tool_stats facade — per-tool usage stats (P3-edges)`.

---

### Task 2: migration module

**Files:** Create `src-tauri/src/proactive/tool_memory_migration.rs`; modify `proactive/mod.rs` (`pub mod tool_memory_migration;`) + `app.rs` (boot spawn).

- [ ] **Step 1: Recon** — `MemoryGraphStore::list_nodes_by_kind(space_id, MemoryNodeKind::Procedure, limit)` + the DISTINCT-spaces enumeration `skill_migration.rs` added (reuse its `list_procedure_spaces` approach or factor); `get_active_version` (to detect skills — tool nodes have NONE); how to read a node's `metadata` as `ToolNodeStats` (serde_json). Read `skill_migration.rs` for the marker/spawn idiom. For the co-usage edges: recon how to read `RelatesTo` `memory_edges` from the store (a `list_edges`/SQL — `get_co_used_tools` shows the JOIN; you need the raw edges + endpoint node ids) and the `node_id → title(tool_name)` map.

- [ ] **Step 2: Migration fn** (pure `node_to_record` seam + the pass):

```rust
//! P3-edges — one-time migration of memory_graph tool-stat nodes + co-usage edges
//! into the adapter tool_stats facade + edges.rs. Idempotent, marker-gated, boot-safe.
//! Disjoint from P3-skills: tool nodes are Procedure-kind but have NO MemoryVersion.

use std::sync::Arc;
use crate::memory_adapter::{tool_stats::{self, ToolStatsRecord}, edges, MemoryAdapter};
use crate::memory_graph::store::MemoryGraphStore;

const MARKER_TOOL: &str = "__tool_memory_migrated_v1__";
const MARKER_SPACE: &str = "__migration__";

/// Pure: a tool node's (space, title, ToolNodeStats-derived fields) → ToolStatsRecord.
fn node_to_record(space: &str, tool_name: &str, total_uses: u64, success_count: u64, failure_count: u64, total_latency_ms: u64, output_sizes: Vec<u64>, parameter_fingerprints: std::collections::HashMap<String,u64>, last_used_at: String) -> ToolStatsRecord {
    ToolStatsRecord { space: space.into(), tool_name: tool_name.into(), total_uses, success_count, failure_count, total_latency_ms, output_sizes, parameter_fingerprints, last_used_at }
}

pub async fn migrate_tool_memory(store: &Arc<MemoryGraphStore>, adapter: &Arc<dyn MemoryAdapter>) -> usize {
    if matches!(tool_stats::get_stats(adapter, MARKER_SPACE, MARKER_TOOL).await, Ok(Some(_))) { return 0; }
    let mut migrated = 0usize; let mut all_ok = true;
    // For each space with Procedure nodes (reuse skill_migration's space enumeration):
    //   for each Procedure node:
    //     if get_active_version(node.id) is Some -> it's a SKILL, skip.
    //     parse node.metadata as ToolNodeStats; if it parses (tool node) ->
    //         put_stats(node_to_record(node.space_id, node.title, …)); build node_id->title map; migrated += 1
    //   read RelatesTo edges among those tool node ids -> edges::relate(map[parent], map[child], "co_used")
    // (log+skip per-item errors; all_ok=false on any)
    if all_ok {
        let marker = ToolStatsRecord { space: MARKER_SPACE.into(), tool_name: MARKER_TOOL.into(), ..Default::default_for_marker() };
        let _ = tool_stats::put_stats(adapter, &marker).await;
    }
    tracing::info!(migrated, all_ok, "tool_memory migration pass complete");
    migrated
}
```

(`ToolStatsRecord` needs a way to build the marker — either add `#[derive(Default)]` and use `..Default::default()` with space/tool overridden, or construct explicitly. Prefer `#[derive(Default)]` on `ToolStatsRecord` in Task 1 so the marker is `ToolStatsRecord { space: MARKER_SPACE.into(), tool_name: MARKER_TOOL.into(), ..Default::default() }`. Add that derive in Task 1 if not present.) Fill the loop with the recon'd reads; extract `node_to_record` as the unit-testable seam.

- [ ] **Step 3: Boot spawn** in `app.rs` beside `migrate_skills`:

```rust
{
    let adapter = bucket_seal_adapter.clone() as Arc<dyn crate::memory_adapter::MemoryAdapter>;
    let store = memory_graph_store.clone();
    tauri::async_runtime::spawn(async move {
        let n = crate::proactive::tool_memory_migration::migrate_tool_memory(&store, &adapter).await;
        tracing::info!(migrated = n, "P3-edges: tool_memory migration spawn complete");
    });
}
```

- [ ] **Step 4: Tests** — `node_to_record` field mapping; marker idempotency (marker present → 0). If wiring a store for an integration test is heavy, test `node_to_record` (pure) + the marker short-circuit (build an in-memory adapter, put the marker, assert 0). The migration test file may need the freeze-hook allowlist for its `create_node`/`create_edge` test fixtures — if so, add `tool_memory_migration.rs` to the allowlist with a "test fixtures only; production is read-only" comment (as `skill_migration.rs` did).

- [ ] **Step 5: build + test + commit** (paths: tool_memory_migration.rs + proactive/mod.rs + app.rs [+ allowlist if touched]): `feat(proactive): tool_memory_migration — stat nodes + co-usage edges → adapter (boot) (P3-edges)`.

---

### Task 3: config flag `tool_memory_repoint_enabled`

**Files:** `src-tauri/src/memubot_config.rs` (mirror `skill_store_repoint_enabled` EXACTLY: field + `default_*` fn + manual `impl Default` entry + 2 tests `tool_memory_repoint_enabled_defaults_on` + `memory_os_deserializes_without_tool_memory_repoint_field`). Verify `cargo test --lib tool_memory_repoint` passes + build clean. Commit `feat(config): tool_memory_repoint_enabled (default on) (P3-edges)`.

---

### Task 4: W1 — `record_tool_usage` repoint (sync→async) + thread adapter

**Files:** `proactive/tool_memory.rs` (`ToolUsageMemoryManager` struct + `::new` + `record_tool_usage`), `proactive/service.rs` (construction `:643` + caller `:882`).

- [ ] **Step 1:** Add `skill_adapter`-style fields to `ToolUsageMemoryManager`: `repoint_adapter: Option<Arc<dyn crate::memory_adapter::MemoryAdapter>>` + `repoint_enabled: bool` (or pass per-call — but a field set at construction is cleaner). Change `::new(store)` → `::new(store, repoint_adapter, repoint_enabled)`; update `service.rs:643` to pass `Some(Arc::clone(&...bucket_seal...) as Arc<dyn MemoryAdapter>)` + `cfg.tool_memory_repoint_enabled` (the service already has the adapter + a runtime config from P3-skills — add the flag to `MemoryOsRuntimeConfig` like `skill_store_repoint_enabled`).
- [ ] **Step 2:** Make `record_tool_usage` **`async`**. When `repoint_enabled` + adapter present: read `tool_stats::get_stats(adapter, space_id, &usage.tool_name)` → accumulate into a `ToolStatsRecord` (port the existing `ToolNodeStats` accumulation math: `total_uses+=1`, success/failure, `total_latency_ms += usage.duration_ms`, push output size, bump parameter fingerprint, `last_used_at`) → `tool_stats::put_stats`. Else the unchanged memory_graph path.
- [ ] **Step 3:** Caller `service.rs:882` — `let _ = tool_memory.record_tool_usage(...).await;` (confirm it's in an async fn — it is). Update any other `record_tool_usage` caller.
- [ ] **Step 4:** build clean; `cargo test --lib tool_memory 2>&1 | grep "test result" | tail -1` (the existing tests call these methods sync → they need `.await` + `#[tokio::test]`; update them, or they were already async — recon). Commit `feat(proactive): site W1 — record_tool_usage repoints to tool_stats facade (sync→async) (P3-edges)`.

---

### Task 5: W2 — `record_co_usage` repoint (sync→async)

**Files:** `proactive/tool_memory.rs` (`record_co_usage`), its caller(s).

- [ ] **Step 1:** `grep -rn "record_co_usage" src-tauri/src/` — find production caller(s). Make `record_co_usage` **`async`**. When `repoint_enabled` + adapter: for each tool pair, `edges::relate(adapter, &tools[i], &tools[j], "co_used").await` (by NAME — no node lookup needed). Else unchanged memory_graph.
- [ ] **Step 2:** Update caller(s) with `.await` (if a production caller exists; if only tests, update the tests). build clean; tests pass. Commit `feat(proactive): site W2 — record_co_usage repoints to edges facade (sync→async) (P3-edges)`.

---

### Task 6: R — `get_tool_stats` repoint (sync→async)

**Files:** `proactive/tool_memory.rs` (`get_tool_stats`), `proactive/proactive_recall.rs:192` + the internal caller `tool_memory.rs:355`.

- [ ] **Step 1:** Make `get_tool_stats` **`async`**. When `repoint_enabled` + adapter: `tool_stats::get_stats(adapter, space_id, tool_name)` → if Some, derive the public `ToolStats` (success_rate = success/total; avg_latency = total_latency/total; typical_output_size from `output_sizes`; common_parameters from `parameter_fingerprints` sorted by freq, top 5 — PORT the existing derivation) + `co_used_tools = edges::neighbors(adapter, tool_name, Some("co_used")).await?`. Return `Some(ToolStats)`; None if no stats. Else unchanged memory_graph path.
- [ ] **Step 2:** Callers: `proactive_recall.rs:192` (`.get_tool_stats(...).await`) + the internal `tool_memory.rs:355` call (`.await` — that enclosing fn becomes async too; ripple its caller if needed — recon). build clean; `cargo test --lib tool_memory`, `--lib proactive` pass. Commit `feat(proactive): site R — get_tool_stats reads tool_stats + edges (sync→async) (P3-edges)`.

---

### Task 7: Whole-slice verification

- [ ] `cargo build 2>&1 | grep -E "^error" | head` → empty.
- [ ] `cargo test --lib memory_adapter::tool_stats`, `--lib tool_memory_migration`, `--lib tool_memory`, `--lib proactive`, `--lib tool_memory_repoint` → green (modulo known env failures).
- [ ] `cargo clippy --lib 2>&1 | grep -E "^error" | head` → empty.
- [ ] confirm: `grep -rn "tool_memory_repoint_enabled" src-tauri/src/` shows the 3 sites + config; production `tool_memory_migration.rs` has no `create_*` outside tests.
- [ ] `gitnexus_detect_changes()` before PR.

## Adjacent-edit checklist (PR body)

- `ToolUsageMemoryManager::new` signature changed (+adapter +flag) → caller `service.rs:643` updated; `MemoryOsRuntimeConfig` gains `tool_memory_repoint_enabled`.
- 3 `ToolUsageMemoryManager` methods sync→async → all callers `.await` (service.rs, proactive_recall.rs, internal tool_memory.rs:355).
- `MemoryOsConfig` new `#[serde(default)]` flag + manual `impl Default` (back-compat test).
- New boot migration spawn in `app.rs`.
- `tool_memory_migration.rs` test fixtures may need the freeze-hook allowlist (test-only writes; production read-only).
- `edges.rs` reused unchanged.

## PR shape

One branch `claude/p3-edges-tool-memory-migration`, PR with a `## Commits (bisectable)` table (Tasks 1–6 = 6 commits). Title: `feat(memory): P3-edges — migrate tool_memory to adapter (tool_stats facade + edges, gated)`. Body: co-usage edges existence-only → edges.rs as-is; new tool_stats facade; migration disjoint from skills (version-absence); 3 sites sync→async behind `tool_memory_repoint_enabled`; **last exempted memory_graph writer — P4 can now remove the freeze hook**; memory_graph retained (gated) until P4.

## Self-review notes

- **Spec coverage:** §1 facade+edges → Task 1; §2 migration → Task 2; §3 gate+3 sites(W1/W2/R)+sync→async → Tasks 3–6. ✔
- **Type consistency:** `ToolStatsRecord` mirrors `ToolNodeStats` (raw accumulators, NOT derived `ToolStats`) — used identically in facade/migration/W1/R; `edges::relate(&str,&str,"co_used")` + `neighbors(...,Some("co_used"))` consistent. ✔
- **Bisectability:** Task 1 (facade, tests-only) compiles; Task 2 (migration, uses facade) compiles; Task 3 (flag) compiles; Tasks 4/5/6 each convert ONE method sync→async + its callers (independent — other methods stay sync until their task) → each compiles. ✔
- **Follow-the-recon items** (flagged): `list_nodes_by_kind`/edge-read shapes + `node_id→title` map (Task 2); `record_co_usage` prod caller (Task 5); the internal `get_tool_stats` caller at 355's async ripple (Task 6); existing `tool_memory.rs` tests' sync→async update (Tasks 4–6); `#[derive(Default)]` on `ToolStatsRecord` for the marker (Task 1/2). Each has explicit guidance.
