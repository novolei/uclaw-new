# P1b — Graph-Edge Adapter Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Add a thin, additive graph-edge facade (`Edge` + `relate`/`neighbors`) over the existing `MemoryAdapter` methods, under an `"edges"` namespace — unblocking P3's tool_memory co-used-graph migration. Undirected (matches co-used). No trait change, no live wiring, no tool_memory change.

**Architecture:** A new `memory_adapter/edges.rs`: free functions over `Arc<dyn MemoryAdapter>` that store an `Edge` as a `MemoryEntry` (JSON content, canonical sorted key under `"edges"`) via `store`, and compute `neighbors` by `list`-ing the namespace + filtering incident edges. Pure capability + unit tests; nothing calls it yet (P3 will). Mirrors the P1a `pages.rs` facade.

**Tech Stack:** Rust, `serde_json`, existing `MemoryAdapter` trait. No new deps. Spec: `docs/superpowers/specs/2026-05-31-p1b-graph-edge-adapter-layer-design.md`.

---

## Source-of-truth references (verified)

- `memory_adapter/traits.rs`: `async fn store(&self, namespace: &str, key: &str, content: &str, category: MemoryCategory, session_id: Option<&str>) -> anyhow::Result<()>`; `async fn list(&self, namespace: Option<&str>, category: Option<&MemoryCategory>, session_id: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>>`. `MemoryEntry { id, key, content, namespace, category, timestamp, session_id, score }`; `MemoryCategory::Core` valid.
- `memory_adapter/pages.rs` (landed in P1a, #621): the established thin-facade pattern + an in-memory `MemoryAdapter` test stub in its `#[cfg(test)]` module — **use it as the template** for the edges test stub. The stub's `list(Some(ns), None, None)` must return all entries in `ns`.
- `memory_adapter/mod.rs`: `pub mod pages;` + `pub use pages::{...}` — mirror that for `edges`.
- Consumer shape (for P3, not this slice): `proactive/tool_memory.rs` writes pairwise `create_edge(RelatesTo)` (symmetric, dedup-on-conflict) + reads `get_co_used_tools` (neighbor traversal).

---

## CRITICAL facts

1. **Facade, not trait** — free `async fn`s over `&Arc<dyn MemoryAdapter>`; no `MemoryAdapter` trait change.
2. **No live wiring** — nothing in production calls these in P1b (P3 repoints tool_memory). Purely additive.
3. **Undirected + idempotent** — `edge_key` sorts the two endpoints so `relate(a,b,k) == relate(b,a,k)` (one stored entry); `neighbors` matches either endpoint. This is co-used-tools' shape.
4. **Robust** — a malformed `"edges"` entry is skipped in `neighbors`, never panics.
5. **List-scan neighbors** is O(edges) — fine for the small co-used graph; an index is a later optimization (NOT a silent cap — documented).
6. **Pre-commit hooks** — no `--no-verify`.

---

## File Structure

| File | Change | LoC |
|---|---|---|
| `memory_adapter/edges.rs` | **new** — `Edge` + `edge_key` + `relate`/`neighbors` + in-memory test stub + tests | ~55 src + ~100 test |
| `memory_adapter/mod.rs` | `pub mod edges;` + `pub use edges::{Edge, relate, neighbors};` | +2 |

---

## Tasks

### Task 1: `edges.rs` facade + tests (TDD)

**Files:** Create `src-tauri/src/memory_adapter/edges.rs`; modify `src-tauri/src/memory_adapter/mod.rs`.

- [ ] **Step 1: Declare the module.** In `memory_adapter/mod.rs`, add `pub mod edges;` (next to `pub mod pages;`) + `pub use edges::{Edge, relate, neighbors};`.

- [ ] **Step 2: Write `edges.rs` with the facade + a `#[cfg(test)]` in-memory stub + failing tests.** Source (above the test module):
```rust
//! Thin graph-edge facade over `MemoryAdapter` (convergence ADR P1b).
//! Free functions — NOT trait methods — over the existing store/list. Undirected
//! (matches co-used-tools). No live wiring yet; P3 repoints tool_memory here.
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
/// `relate(b,a,k)` collide (idempotent symmetric dedup). `\u{1}` separates fields.
fn edge_key(a: &str, b: &str, kind: &str) -> String {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    format!("{kind}\u{1}{lo}\u{1}{hi}")
}

/// Create (or overwrite) an undirected edge. Idempotent on (from,to,kind) up to order.
pub async fn relate(adapter: &Arc<dyn MemoryAdapter>, from: &str, to: &str, kind: &str) -> anyhow::Result<()> {
    let edge = Edge { from: from.to_string(), to: to.to_string(), kind: kind.to_string() };
    let content = serde_json::to_string(&edge)?;
    adapter
        .store(EDGES_NAMESPACE, &edge_key(from, to, kind), &content, MemoryCategory::Core, None)
        .await
}

/// Neighbors of `node` (the other endpoint of each incident edge), optionally
/// filtered by `kind`. Deduped. List-scans the "edges" namespace (fine for the
/// small co-used graph; an index is a later optimization). Unparseable entries skipped.
pub async fn neighbors(adapter: &Arc<dyn MemoryAdapter>, node: &str, kind: Option<&str>) -> anyhow::Result<Vec<String>> {
    let entries = adapter.list(Some(EDGES_NAMESPACE), None, None).await?;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for e in entries {
        let edge: Edge = match serde_json::from_str(&e.content) {
            Ok(ed) => ed,
            Err(_) => continue,
        };
        if let Some(k) = kind {
            if edge.kind != k { continue; }
        }
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
Tests (in `#[cfg(test)] mod tests`): copy the in-memory `MemoryAdapter` stub from `memory_adapter/pages.rs`'s test module (HashMap<(namespace,key), MemoryEntry>; `store` inserts, `list(Some(ns),..)` returns all entries with that namespace; other methods minimal). Then:
```rust
#[tokio::test]
async fn relate_then_neighbors_is_symmetric() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    relate(&a, "x", "y", "relates_to").await.unwrap();
    assert_eq!(neighbors(&a, "x", None).await.unwrap(), vec!["y".to_string()]);
    assert_eq!(neighbors(&a, "y", None).await.unwrap(), vec!["x".to_string()]);
}
#[tokio::test]
async fn relate_is_idempotent_and_order_insensitive() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    relate(&a, "x", "y", "relates_to").await.unwrap();
    relate(&a, "x", "y", "relates_to").await.unwrap();
    relate(&a, "y", "x", "relates_to").await.unwrap(); // reversed → same key
    assert_eq!(a.list(Some("edges"), None, None).await.unwrap().len(), 1);
    assert_eq!(neighbors(&a, "x", None).await.unwrap(), vec!["y".to_string()]);
}
#[tokio::test]
async fn neighbors_dedupes_and_collects_multiple() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    relate(&a, "a", "b", "relates_to").await.unwrap();
    relate(&a, "a", "c", "relates_to").await.unwrap();
    let mut ns = neighbors(&a, "a", None).await.unwrap();
    ns.sort();
    assert_eq!(ns, vec!["b".to_string(), "c".to_string()]);
}
#[tokio::test]
async fn neighbors_filters_by_kind() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    relate(&a, "a", "b", "relates_to").await.unwrap();
    relate(&a, "a", "c", "co_used").await.unwrap();
    assert_eq!(neighbors(&a, "a", Some("relates_to")).await.unwrap(), vec!["b".to_string()]);
}
#[tokio::test]
async fn neighbors_unknown_node_is_empty() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    relate(&a, "a", "b", "relates_to").await.unwrap();
    assert!(neighbors(&a, "zzz", None).await.unwrap().is_empty());
}
#[tokio::test]
async fn neighbors_skips_malformed_entries() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    a.store("edges", "junk", "not json", MemoryCategory::Core, None).await.unwrap();
    relate(&a, "a", "b", "relates_to").await.unwrap();
    assert_eq!(neighbors(&a, "a", None).await.unwrap(), vec!["b".to_string()]);
}
```
(Match the crate's async-test attribute — `#[tokio::test]` per pages.rs. Confirm the stub's `list` honors the namespace filter.)

- [ ] **Step 3: Run → red→green.** `cd src-tauri && cargo test --lib memory_adapter::edges 2>&1 | tail`.

- [ ] **Step 4: Commit.**
```bash
git add src-tauri/src/memory_adapter/edges.rs src-tauri/src/memory_adapter/mod.rs
git commit -m "feat(memory): graph-edge adapter facade (Edge + relate/neighbors, undirected) — convergence P1b"
```

### Task 2: Verification

- [ ] `cd src-tauri && cargo test --lib memory_adapter::edges 2>&1 | tail` (6 tests pass).
- [ ] `cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cargo test --lib memory_adapter 2>&1 | tail -3` (broader memory_adapter green — no regression to adapter/router/bucket_seal/pages tests).
- [ ] `cargo clippy --lib -- -D warnings 2>&1 | grep -E "memory_adapter/edges" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] **Additive-only confirm:** `grep -rn "edges::relate\|edges::neighbors" src-tauri/src | grep -v "memory_adapter/edges.rs\|memory_adapter/mod.rs"` → empty (nothing wires it yet; P3's job).

---

## Self-Review

- ✅ **Spec coverage:** `Edge`/`edge_key`/`relate`/`neighbors` facade (Task 1) + verification incl. additive-only confirm (Task 2). Directed edges / weights / index / P3 wiring / P1c explicitly out of scope.
- ✅ **Placeholder scan:** full facade + full test code; the in-memory stub is a copy-from-pages.rs instruction with a concrete behavior contract (list honors namespace).
- ✅ **Type consistency:** `Edge { from, to, kind }`; `edge_key(&str,&str,&str) -> String`; `relate(&Arc<dyn MemoryAdapter>, &str, &str, &str) -> Result<()>`; `neighbors(&Arc<dyn MemoryAdapter>, &str, Option<&str>) -> Result<Vec<String>>`; matches `store`/`list` signatures verified in traits.rs.
- ✅ **Risk-scaled:** lowest — pure additive facade, no trait change, no live wiring, no tool_memory change; one module + tests. The list-scan neighbors is documented as a later optimization (not silent).
- Decisions: facade over trait; `"edges"` namespace; undirected canonical key (symmetric dedup); list-scan neighbors; robust-skip malformed; no live wiring (P3).
