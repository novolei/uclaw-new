# P1b — Graph-Edge Adapter Layer (thin facade) Design

**Date:** 2026-05-31
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory-store convergence (ADR `2026-05-31-memory-store-convergence-openhuman-primary.md`), Phase **P1** (MemoryAdapter capability growth), second slice **P1b** (graph edges). Unblocks **P3** (migrate tool_memory's co-used-tools graph off the frozen memory_graph). Follows the P1a (`pages.rs`) thin-facade pattern.

## Problem

P3 will migrate `proactive/tool_memory.rs`'s co-used-tools graph off memory_graph onto the adapter. tool_memory currently writes pairwise `MemoryEdge { parent_node_id, child_node_id, relation_kind: RelatesTo, … }` (best-effort, dedup-on-conflict) via `store.create_edge`, and reads via `get_co_used_tools` (SQL JOIN traversal: given a tool, find connected tools). The `MemoryAdapter` (store/recall/get/list/delete/…) + bucket_seal have **no edge concept**.

## Decision (P1b scope)

Add a thin, additive **graph-edge facade** over the existing `MemoryAdapter` methods — `Edge` + `relate` + `neighbors` under an `"edges"` namespace. **Undirected** (the only consumer, co-used-tools, is symmetric; directed edges / weights / indexed lookup are YAGNI-deferred). No trait change, no live wiring, no tool_memory change (P3 repoints). Pure new capability + unit tests.

## Design

### New module `src-tauri/src/memory_adapter/edges.rs`

```rust
use std::sync::Arc;
use crate::memory_adapter::{MemoryAdapter, MemoryCategory};

const EDGES_NAMESPACE: &str = "edges";

/// An undirected graph edge between two node ids, with a relation kind.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

/// Canonical undirected key: kind + endpoints sorted, so `relate(a,b,k)` and
/// `relate(b,a,k)` collide (idempotent symmetric dedup — matches tool_memory's
/// "ignore duplicate edge" intent). `\u{1}` separator avoids id-content clashes.
fn edge_key(a: &str, b: &str, kind: &str) -> String {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    format!("{kind}\u{1}{lo}\u{1}{hi}")
}

/// Create (or overwrite) an undirected edge. Idempotent on (from,to,kind) up to order.
pub async fn relate(adapter: &Arc<dyn MemoryAdapter>, from: &str, to: &str, kind: &str) -> anyhow::Result<()> {
    let edge = Edge { from: from.to_string(), to: to.to_string(), kind: kind.to_string() };
    let content = serde_json::to_string(&edge)?;
    adapter.store(EDGES_NAMESPACE, &edge_key(from, to, kind), &content, MemoryCategory::Core, None).await
}

/// Neighbors of `node` (the other endpoint of each incident edge), optionally
/// filtered by `kind`. Deduped. List-scans the "edges" namespace (fine for the
/// small co-used graphs; an index is a later optimization). Unparseable entries skipped.
pub async fn neighbors(adapter: &Arc<dyn MemoryAdapter>, node: &str, kind: Option<&str>) -> anyhow::Result<Vec<String>> {
    let entries = adapter.list(Some(EDGES_NAMESPACE), None, None).await?;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for e in entries {
        let Ok(edge) = serde_json::from_str::<Edge>(&e.content) else { continue };
        if let Some(k) = kind { if edge.kind != k { continue } }
        let other = if edge.from == node {
            Some(edge.to)
        } else if edge.to == node {
            Some(edge.from)
        } else {
            None
        };
        if let Some(o) = other {
            if seen.insert(o.clone()) { out.push(o); }
        }
    }
    Ok(out)
}
```

Declared `pub mod edges;` in `memory_adapter/mod.rs` + re-export `Edge`/`relate`/`neighbors`.

- **Facade, not trait** — over the existing `store`/`list`; no `MemoryAdapter` trait change.
- **Undirected + idempotent** — `edge_key` sorts endpoints → symmetric dedup; `neighbors` matches either endpoint. This is exactly co-used-tools' shape (P3's `relate` ≈ `create_edge(RelatesTo)`, `neighbors` ≈ `get_co_used_tools`).
- **`kind`** as a string (e.g. `"relates_to"`, matching `MemoryRelationKind::as_str`).

## Data flow

```
relate(adapter, a, b, "relates_to")
  → adapter.store("edges", edge_key(a,b,"relates_to"), JSON(Edge), Core, None)
neighbors(adapter, a, Some("relates_to"))
  → adapter.list("edges") → parse Edges → endpoints where a is from/to → dedup → [b, …]
```

(Not invoked by any live path in P1b. P3 wires tool_memory's create_edge/get_co_used_tools to it.)

## Error handling

`relate`/`neighbors` propagate the adapter's `anyhow::Result`. A malformed stored edge entry is skipped in `neighbors`, never panics.

## Testing

1. `relate` then `neighbors(from)` returns `[to]`, and `neighbors(to)` returns `[from]` (undirected/symmetric).
2. Idempotent: `relate(a,b,k)` twice (and once as `relate(b,a,k)`) → the `"edges"` namespace has a single entry; `neighbors` returns `b` once.
3. `kind` filter: edges of two kinds; `neighbors(node, Some("k1"))` returns only k1 partners.
4. A multi-edge node (a–b, a–c) → `neighbors(a)` = `{b, c}` (deduped, order-insensitive assert).
5. `neighbors` of an unknown node → empty.
6. Malformed `"edges"` content → skipped (robust).
7. `cargo test --lib memory_adapter::edges` + build clean + clippy clean; broader `memory_adapter` green; `Cargo.toml` unchanged.

(Reuse the in-memory `MemoryAdapter` test stub pattern from `pages.rs`/`task_memory.rs` — its `list(namespace)` must return all entries in the namespace.)

## Scope / files

| File | Change |
|---|---|
| `memory_adapter/edges.rs` | **new** — `Edge` + `relate`/`neighbors` + in-memory-stub tests |
| `memory_adapter/mod.rs` | `pub mod edges;` + re-exports |

**Out of scope (later):** directed edges / edge weights / edge metadata / an index for neighbor lookup (list-scan is fine for the small co-used graph); **P3** the tool_memory migration + repointing; P1c (versioning+ranking). No live wiring in P1b.

## Risk

Low. Pure additive facade over existing tested adapter methods; no trait change, no live wiring, no tool_memory change. List-scan neighbors is O(edges) — acceptable for co-used graphs (small); flagged as a later optimization, not a silent cap. One branch, bisectable.
