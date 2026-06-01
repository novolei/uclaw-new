# P3-edges — tool_memory Store Migration → Adapter (gated, faithful) Design

**Date:** 2026-06-01
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory-store convergence (ADR `2026-05-31-memory-store-convergence-openhuman-primary.md`), Phase **P3** (migrate memory_graph rich writers → the extended adapter), second/final slice **P3-edges**. Builds on **P1b** (`memory_adapter/edges.rs`). Sibling of **P3-skills** (shipped, PR #631) and mirrors its structure. Completing this retires the last exempted memory_graph writer, unblocking **P4** (remove the freeze hook).

## Problem

`tool_memory` (`src-tauri/src/proactive/tool_memory.rs`) is the second exempted memory_graph writer. It maintains a co-used-tools graph driving proactive tool suggestions: per-tool **stat nodes** (`record_tool_usage` accumulates `ToolNodeStats` into node metadata) and **co-usage edges** (`record_co_usage` creates `RelatesTo` edges between tool nodes), read back by `get_tool_stats` (consumed by `proactive_recall.rs:192`). Until it migrates, memory_graph cannot retire.

Recon corrected an earlier assumption: the co-usage edges are **existence-only** (unweighted `RelatesTo`, idempotent on endpoints) — so `edges.rs` (`relate`/`neighbors`) is a faithful match **as-is, no extension**. The only real gap is a home for the per-tool `ToolStats` (rich telemetry: `total_uses`, `success_rate`, `avg_latency_ms`, `typical_output_size`, `common_parameters`, `last_used_at`, `co_used_tools`). Tool stat nodes are `MemoryNodeKind::Procedure` (same as skills) but have **no `MemoryVersion`** — so P3-skills' migration (which skips version-less nodes) already left them untouched; the two migrations are disjoint.

## Decision (P3-edges scope)

A new `memory_adapter::tool_stats` facade homes the per-tool stats; co-usage edges reuse `edges.rs` keyed by **tool name** (no uuid-node indirection). One-time-migrate the existing stat nodes + co-usage edges, then repoint `tool_memory`'s three touch points (stat-write, edge-write, read) behind a new `tool_memory_repoint_enabled` flag (default on; rollback restores memory_graph). The `ToolMemory` methods convert sync→async (the facade is async; their callers are in async context).

Out of scope: P4 (remove memory_graph + freeze hook + delete gated paths). Versioning N/A (tool nodes have none).

## Design

### §1 `tool_stats` facade (new) + `edges.rs` reuse

`src-tauri/src/memory_adapter/tool_stats.rs` (mirrors `skills.rs`): `"tool_stats"` namespace, space-qualified key `{space}\u{1}{tool_name}`.

```rust
pub struct ToolStatsRecord {
    pub space: String,
    pub tool_name: String,
    pub total_uses: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub total_latency_ms: u64,
    pub typical_output_size: Option<u64>,
    pub common_parameters: Vec<String>,
    pub last_used_at: Option<String>,
}
// put_stats(adapter, &ToolStatsRecord) -> Result<()>            (key = {space}\u1{tool_name})
// get_stats(adapter, space_id, tool_name) -> Result<Option<ToolStatsRecord>>
```

(Stored as JSON; the raw accumulators — success/failure/latency — live in the record, and the derived `success_rate`/`avg_latency_ms`/`typical_output_size` of the public `ToolStats` are computed by `get_tool_stats` from them, exactly as `tool_memory` derives them today. Accumulation logic stays in `record_tool_usage` via get→mutate→put.)

Co-usage edges reuse `edges.rs` unchanged: `relate(adapter, tool_a, tool_b, "co_used")` (idempotent, undirected) and `neighbors(adapter, tool_name, Some("co_used")) -> Vec<String>`. Tools are keyed by **name** directly.

### §2 Migration — `proactive/tool_memory_migration.rs` (new)

One-time, idempotent, marker-gated, boot-spawned (P3-skills/P2b idiom), reading `MemoryGraphStore`:

- **Stat nodes:** read `Procedure` nodes; a node is a *tool* node iff its `metadata` parses as the tool stats shape (`total_uses`/`success_count`/… present) **and** it has no Active `MemoryVersion` (disjoint from skills). Map → `ToolStatsRecord` (space = `node.space_id`, tool_name = `node.title`) → `put_stats`. Build a `node_id → tool_name` map.
- **Co-usage edges:** read `RelatesTo` `memory_edges` whose endpoints are both tool nodes; translate each edge's uuid endpoints to tool names via the map → `edges::relate(name_a, name_b, "co_used")`.
- Completion marker `__tool_memory_migrated_v1__` (a reserved-space `ToolStatsRecord` with a sentinel tool_name, set only on a fully-successful pass — the P2b completion-marker pattern). Infallible / boot-safe (graph-absent / per-item error → log + skip; partial → retry next boot).
- Boot spawn in `app.rs` next to the P3-skills `migrate_skills` spawn (same `bucket_seal_adapter` + `memory_graph_store` handles).

### §3 Gate + repoint (sync→async)

`MemoryOsConfig.tool_memory_repoint_enabled: bool` (default `true`; rollback restores the memory_graph paths). Thread the bucket_seal adapter (`Arc<dyn MemoryAdapter>`) + the flag into `ToolMemory::new` (`ProactiveService` already holds the adapter from P3-skills — pass it through; `MemoryOsRuntimeConfig` already carries repoint flags — add this one).

| Site | Today (sync, memory_graph) | Repointed (gated, async) |
|---|---|---|
| **W1** `record_tool_usage` (caller `proactive/service.rs:882`) | accumulate `ToolNodeStats` into node metadata via `update_node` | `tool_stats::get_stats` → accumulate (existing math) → `tool_stats::put_stats` |
| **W2** `record_co_usage` (recon its caller) | `create_edge(RelatesTo)` per pair | `edges::relate(a, b, "co_used")` per pair (tool names) |
| **R** `get_tool_stats` (consumer `proactive/proactive_recall.rs:192`) | node metadata + SQL `get_co_used_tools` | `tool_stats::get_stats` + `edges::neighbors(tool, "co_used")` → assemble `ToolStats` (derive success_rate/avg_latency) |

**Sync→async conversion:** `ToolMemory`'s `record_tool_usage`/`record_co_usage`/`get_tool_stats` are sync (`pub fn`, sync SQL); the facade is `async`. Repointing makes them `async`. The callers (`service.rs:882`, `proactive_recall.rs:192`) are in async context → add `.await`. The plan recons every caller; if any is genuinely sync, that branch keeps the sync memory_graph path (the gate already provides a sync fallback) or uses a `block_in_place` bridge — flagged, not assumed.

### Data flow

```
boot: tool_memory_migration (marker absent) → tool Procedure nodes → put_stats; RelatesTo edges → relate("co_used")
record usage:   record_tool_usage → get_stats → accumulate → put_stats          [W1]
record co-use:  record_co_usage → relate(a,b,"co_used") per pair                 [W2]
read:           get_tool_stats → get_stats + neighbors(tool,"co_used") → ToolStats [R] → proactive_recall
flag off ⇒ unchanged memory_graph paths (rollback)
```

## Error handling

Migration: P3-skills/P2b infallible boot posture (graph-absent / per-item error → log+skip; marker on full success; retry next boot). Repoint: facade errors log + are non-fatal (tool-memory recording is best-effort today — `record_co_usage` already ignores duplicate-edge errors; `record_tool_usage` is called with `let _ =`). Flag off → unchanged.

## Testing

1. **`tool_stats` facade** (in-memory adapter): `put_stats`/`get_stats` round-trip; space isolation (same tool_name, two spaces, no collision); absent → `None`.
2. **Migration mapping** (pure, testable seam): a tool-stats `Procedure` node (metadata) → `ToolStatsRecord`; a skill node (has version / no tool-stats metadata) is ignored; a `RelatesTo` edge with two known endpoints → `relate` of the two names; marker idempotency.
3. **`get_tool_stats` assembly:** stats + `neighbors` → `ToolStats` with derived `success_rate`/`avg_latency_ms` + `co_used_tools`.
4. `cargo build` + `cargo test --lib memory_adapter::tool_stats` + `--lib tool_memory` + `--lib proactive` + clippy clean.

## Scope / files

| File | Change |
|---|---|
| `memory_adapter/tool_stats.rs` (new) | `ToolStatsRecord` + `put_stats`/`get_stats` + tests |
| `memory_adapter/mod.rs` | `pub mod tool_stats;` |
| `proactive/tool_memory_migration.rs` (new) | stat-node + co-usage-edge migration + marker + tests |
| `app.rs` | boot spawn |
| `memubot_config.rs` | `tool_memory_repoint_enabled` flag + default + Default + tests |
| `proactive/tool_memory.rs` | 3 sites repointed (W1/W2/R), sync→async; `ToolMemory::new` gains adapter + flag |
| `proactive/service.rs` | thread adapter/flag to `ToolMemory`; `.await` the now-async W1/W2 calls |
| `proactive/proactive_recall.rs` | `.await` the now-async `get_tool_stats` |

**Out of scope:** **P4** retire memory_graph + the freeze hook + delete the gated paths + the now-dead `tool_memory` SQL.

## Risk

Medium. The main ripple is the **sync→async** conversion of `ToolMemory`'s methods (recon'd callers are async). Otherwise it mirrors P3-skills: a new typed facade + `edges.rs` reused unchanged (the edge-weight gap was illusory — existence-only co-usage), migration-first, all gated by `tool_memory_repoint_enabled` (default on, rollback restores memory_graph, retained until P4). The tool-stat node↔skill-node disjointness (version-absence) is verified, so no double-migration. One branch, bisectable: facade → migration → flag → repoint (sync→async) sites. **This is the last exempted memory_graph writer — after it, P4 can remove the freeze hook.**
