# Follow-up PR17 — MemUAdapter (5th backend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Wrap the memU bridge client as a `MemUAdapter` behind the `MemoryAdapter` trait — the design's 5th concrete impl — reachable via the unified `memory.unified.*` IPC under `backend:"memu"`. A recall + append + list adapter (memU is item-based, not `(namespace,key)`-addressed).

**Architecture:** A marshalling layer in `memory_adapter/memu.rs` over the existing `MemUClient` (`create_item` / `retrieve_with_context` / `list_items`) held as `Option<Arc<MemUClient>>`. store→create_item, recall→retrieve_with_context (fast non-LLM-enriched path), list→list_items, namespace_summaries→derived; get→None and delete/clear→no-op (memU items are id-addressed, not key-addressed). Always registered; methods return Err/empty when the bridge is absent. Mirrors PR14's GbrainAdapter.

**Tech Stack:** Rust, `async-trait`, `anyhow`, `tracing`, `serde_json`, uClaw's `memu::client::MemUClient`. No new deps.

---

## Source-of-truth references (verified)

- `memu/client.rs` — `MemUClient`:
  - `create_item(memory_type: &str, memory_content: &str, memory_categories: Vec<String>, user_scope: Option<serde_json::Value>) -> Result<CreateItemResult, BridgeError>` (CreateItemResult { memory_item: Option<Value> })
  - `retrieve_with_context(query: &str, memory_types: Option<&[&str]>, limit: usize, include_categories: bool) -> Result<Vec<EnrichedMemoryItem>, BridgeError>` (90s bridge timeout when include_categories=true; pass **false** for the fast path)
  - `list_items(category: Option<&str>, memory_type: Option<&str>, limit: Option<u32>, offset: Option<u32>, user_scope: Option<serde_json::Value>) -> Result<ListItemsResult, BridgeError>`
  - `delete_item(...)` (id-addressed — not used by the adapter; get/delete by (ns,key) are no-ops)
  - `BridgeError` — has a Display/Debug for error mapping.
- `EnrichedMemoryItem` — return elem of `retrieve_with_context`. Fields used by `agent/tools/memu_tools.rs`: `item.content` (String), `item.memory_type` (String), `item.id` (verify). **Implementer: read its def (likely in `memu/client.rs` or `memu/types.rs`) for the exact fields.**
- `ListItemsResult` — `memu/client.rs:41` — `{ items: Vec<...> }` (verify the item elem type — may be `serde_json::Value` or a typed item).
- `app.rs:208` — `memu_client: Option<Arc<MemUClient>>` AppState field.
- `memory_adapter/mod.rs` — re-exports (`pub use gbrain::GbrainAdapter;` etc.); add `pub mod memu; pub use memu::MemUAdapter;`.
- `memory_adapter/gbrain.rs` (PR14) — **the template**: pure marshalling fns + 8 trait methods + graceful no-ops + always-register. Mirror its structure.
- `memory_adapter/types.rs` — `MemoryEntry { id, key, content, namespace: Option<String>, category: MemoryCategory, timestamp: String, session_id: Option<String>, score: Option<f64> }`; `MemoryCategory { Core, Daily, Conversation, Custom(String) }`; `RecallOpts<'a> { namespace, category, session_id, min_score }`; `NamespaceSummary { namespace, count, last_updated }`.
- `app.rs` — memory_adapters_map registration block (gbrain/bucket_seal/legacy inserts).

---

## CRITICAL facts

1. **memU is item-based, not (ns,key)-addressed.** store appends items; recall/list query them. `get(ns,key)` → `Ok(None)`; `delete`/`clear_namespace` → `Ok(false)`/`Ok(0)` no-op (no key→item-id mapping). This is honest, not a gap — same lossy pattern as gbrain's delete.
2. **recall uses the FAST path** — `retrieve_with_context(query, None, limit, /*include_categories=*/false)` to avoid the LLM-enrichment 90s timeout. Best-effort: bridge error → `Err` (callers skip).
3. **`Option<Arc<MemUClient>>`** — when `None` (bridge not initialized), every method returns `Err`/empty gracefully. Always registered (mirrors gbrain; the bridge may start late).
4. **category encoding** — `MemoryCategory` → memU `memory_type` string (core/daily/conversation/custom:X); namespace → a memU category in `memory_categories`. So `list_items(category=ns)` filters by namespace.

---

## File Structure

| File | New/Mod | Purpose | LoC |
|---|---|---|---|
| `memory_adapter/memu.rs` | new | `MemUAdapter` + pure marshalling fns + tests | ~340 (incl. ~140 tests) |
| `memory_adapter/mod.rs` | mod | `pub mod memu; pub use memu::MemUAdapter;` | +2 |
| `app.rs` | mod | register `MemUAdapter::new(memu_client.clone())` under `"memu"` | +5 |

Est. ~200 source + ~140 tests.

---

## Adaptation responsibilities

1. **Read `gbrain.rs` first** — mirror its shape (pure fns + thin trait methods + graceful no-ops + error→anyhow mapping).
2. **`EnrichedMemoryItem` + `ListItemsResult` item fields** — read the defs; the pure `enriched_item_to_entry`/`list_item_to_entry` map them to `MemoryEntry` (content, memory_type→category, id→entry.id; namespace from the item's category if recoverable, else None). If `ListItemsResult.items` is `Vec<serde_json::Value>`, parse defensively (missing fields → defaults).
3. **`MemUClient` method signatures** — verify exact param types (`retrieve_with_context` takes `&str`/`Option<&[&str]>`/`usize`/`bool`; `create_item` takes `&str`/`&str`/`Vec<String>`/`Option<Value>`; `list_items` takes `Option<&str>`/.../`Option<Value>`).
4. **`BridgeError` → anyhow** — `.map_err(|e| anyhow::anyhow!("memu: {e}"))` (verify Display).
5. **Absent client** — `self.client.as_ref().ok_or_else(|| anyhow::anyhow!("memu bridge not available"))?` at the top of each non-no-op method.
6. **Tests are pure** — construct `EnrichedMemoryItem`/`ListItemsResult` (or their JSON) directly + test the marshalling fns. No live bridge. The trait methods that call the client are thin; an integration test needs the bridge (out of scope, note it).
7. **category_to_str/from_str** — reuse gbrain's convention (`core`/`daily`/`conversation`/`custom:X`) locally; memU's `memory_type` is a free string so this round-trips.
8. **Pre-commit hooks** — no `--no-verify`.

---

## Tasks

### Task 1: pure marshalling fns + tests

- [ ] **Step 1: Write failing tests** in `memory_adapter/memu.rs` for: `category_to_str`/`from_str` round-trip; `enriched_item_to_entry` (content/category/id/score); `list_item_to_entry`; `summaries_from_items` (group by namespace-category, count). Construct the memU item types (or JSON) directly. **Mirror gbrain.rs's test style.**

- [ ] **Step 2: Run → fail.** `cd src-tauri && cargo test --lib memory_adapter::memu 2>&1 | tail`

- [ ] **Step 3: Implement the module header + pure fns** (mirror gbrain.rs):

```rust
// SPDX-License-Identifier: Apache-2.0
//! `MemUAdapter` — wraps the memU bridge (item-based memory) behind the
//! `MemoryAdapter` trait. Marshalling layer over `MemUClient`
//! (create_item / retrieve_with_context / list_items). memU items are
//! id-addressed, so get/delete/clear by (namespace,key) are no-ops; the
//! adapter's value is recall + append + list. Mirrors `gbrain.rs`.

use std::sync::Arc;
use async_trait::async_trait;
use chrono::Utc;

use crate::memu::client::MemUClient;
use crate::memory_adapter::traits::MemoryAdapter;
use crate::memory_adapter::types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

fn category_to_str(c: &MemoryCategory) -> String { /* core/daily/conversation/custom:X — copy gbrain */ }
fn category_from_str(s: &str) -> MemoryCategory { /* inverse — copy gbrain */ }

// enriched_item_to_entry, list_item_to_entry, summaries_from_items — pure,
// per the verified EnrichedMemoryItem/ListItemsResult shapes.
```

(Implement the pure fns per the verified item shapes; `MemoryEntry` 8-field construction with `category` from `memory_type`, `timestamp = Utc::now().to_rfc3339()` or the item's timestamp if present, `score` from the item's relevance if present else None.)

- [ ] **Step 4: Run → pass.** Commit:
```bash
git add src-tauri/src/memory_adapter/memu.rs
git commit -m "feat(memory_adapter): memu marshalling fns (PR17.1)"
```

### Task 2: MemUAdapter trait impl

- [ ] **Step 1: Implement the struct + `impl MemoryAdapter`** (mirror gbrain.rs):

```rust
pub struct MemUAdapter { client: Option<Arc<MemUClient>> }
impl MemUAdapter { pub fn new(client: Option<Arc<MemUClient>>) -> Self { Self { client } } }

#[async_trait]
impl MemoryAdapter for MemUAdapter {
    fn name(&self) -> &str { "memu" }
    async fn store(&self, ns, key, content, category, session_id) -> anyhow::Result<()> {
        // client.create_item(&category_to_str(&category), content, vec![ns.to_string()], user_scope from session)
    }
    async fn recall(&self, query, limit, opts) -> anyhow::Result<Vec<MemoryEntry>> {
        // client.retrieve_with_context(query, None, limit, false) → enriched_item_to_entry,
        // then opts.namespace/category/min_score filters
    }
    async fn get(&self, _ns, _key) -> anyhow::Result<Option<MemoryEntry>> { Ok(None) } // not key-addressed
    async fn list(&self, namespace, _category, _session) -> anyhow::Result<Vec<MemoryEntry>> {
        // client.list_items(namespace, None, Some(200), Some(0), None) → list_item_to_entry
    }
    async fn delete(&self, ns, key) -> anyhow::Result<bool> { tracing::warn!(..); Ok(false) }
    async fn clear_namespace(&self, ns) -> anyhow::Result<u64> { tracing::warn!(..); Ok(0) }
    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
        // client.list_items(None,...) → summaries_from_items
    }
}
```

- [ ] **Step 2:** `memory_adapter/mod.rs` += `pub mod memu; pub use memu::MemUAdapter;`

- [ ] **Step 3: Build + run.** `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head` + `cargo test --lib memory_adapter::memu 2>&1 | tail`

- [ ] **Step 4: Commit:**
```bash
git add src-tauri/src/memory_adapter/memu.rs src-tauri/src/memory_adapter/mod.rs
git commit -m "feat(memory_adapter): MemUAdapter MemoryAdapter impl (PR17.2)"
```

### Task 3: app.rs registration

- [ ] **Step 1:** In the memory_adapters_map block (where gbrain/bucket_seal register), add:

```rust
let memu_adapter = std::sync::Arc::new(
    crate::memory_adapter::MemUAdapter::new(memu_client.clone()),
) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;
memory_adapters_map.insert(memu_adapter.name().to_string(), memu_adapter);
```

**Adaptation:** verify `memu_client` (`Option<Arc<MemUClient>>`) is in scope at the registry block (it's an AppState field built earlier ~line 630). `.clone()` on the Option is cheap.

- [ ] **Step 2: Full build + commit:**
```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
git add src-tauri/src/app.rs
git commit -m "feat(app): register MemUAdapter in the memory-adapter registry (PR17.3)"
```

### Task 4: Verification

- [ ] `cargo test --lib memory_adapter 2>&1 | tail` (existing + new memu tests pass).
- [ ] `cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cargo clippy --lib -- -D warnings 2>&1 | grep -E "memory_adapter/memu|app\.rs"` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] `grep -n "MemUAdapter" src-tauri/src/app.rs src-tauri/src/memory_adapter/mod.rs` (registered + re-exported).

---

## Self-Review

- ✅ Spec coverage: store/recall/list/namespace_summaries functional; get/delete/clear honest no-ops; always-register + graceful-absent.
- ✅ No placeholders (pure-fn bodies are "implement per verified shapes" with the shapes named — implementer reads EnrichedMemoryItem/ListItemsResult, like PR14 read PageDetail).
- ✅ Type consistency: `MemUAdapter::new(Option<Arc<MemUClient>>)`, `category_to_str`/`from_str`, `enriched_item_to_entry`/`list_item_to_entry`/`summaries_from_items`, `MemoryEntry` 8-field — consistent.
- ✅ Mirrors PR14 (GbrainAdapter) — proven lossy-adapter pattern. Decision: get/delete/clear no-op because memU is id-addressed not (ns,key)-addressed; recall uses the non-LLM-enriched fast path.
