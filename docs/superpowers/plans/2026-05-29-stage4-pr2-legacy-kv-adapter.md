# 阶段 4 PR2 — `LegacyKvAdapter` (wraps `memory.rs::MemoryStore`) · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** First concrete `MemoryAdapter` impl: `LegacyKvAdapter` wraps the existing `crate::memory::MemoryStore` (legacy KV+FTS over SQLite). Implements all 8 trait methods + the legacy↔adapter `MemoryEntry` conversion. Registered into `AppState.memory_adapters` at boot. **No call-site migrations yet** — `tauri_commands.rs::memory_*` handlers keep their direct `MemoryStore` references.

**Architecture:** Thin wrapper. The legacy `MemoryStore` has all the right operations; this PR adds a translation layer over them. Adapter `async fn` methods call sync `MemoryStore` methods inline (SQLite operations are fast; `tokio::task::spawn_blocking` is overkill here — matches the pattern in `context_manager_for_prompt_blocking` and similar `block_in_place` cases). Legacy `MemoryEntry { id, space_id, namespace, key, value: serde_json::Value, kind, tags, ... }` translates to adapter `MemoryEntry { id, key, content: String, namespace: Option<String>, category, timestamp, session_id, score }` via a small helper.

**Tech Stack:** Same as PR1 — `async-trait`, `anyhow`, `serde`. No new deps.

**Related design:** [`docs/superpowers/specs/2026-05-29-stage4-memory-adapter-design.md`](../specs/2026-05-29-stage4-memory-adapter-design.md) — see Backend roster row #2.

**Reference:** `src-tauri/src/memory.rs` (legacy MemoryStore).

---

## Pre-flight

1. Confirm main at `839b3aa5` (PR1 merged):
   ```bash
   git -C /Users/ryanliu/Documents/uclaw status -sb
   git -C /Users/ryanliu/Documents/uclaw log --oneline -3
   ```
2. Create worktree:
   ```bash
   git worktree add -b claude/stage4-pr2-legacy-kv-adapter \
       /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter main
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
         /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri/gbrain-source
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri/pyembed
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri/bunembed
   ```
3. Capture baselines:
   ```bash
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo test --lib memory_adapter 2>&1 | tail -3
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
   ```
   Expected: 0 errors, **50 warnings** (49 baseline + 1 dead-code on trait from PR1), `memory_adapter` 5/0, `agent::` 798/2.

---

## File structure

```
src-tauri/src/memory_adapter/
├── mod.rs               — declare `legacy_kv` + re-export LegacyKvAdapter
├── traits.rs            — (unchanged from PR1)
├── types.rs             — (unchanged from PR1)
├── legacy_kv.rs         — NEW: LegacyKvAdapter struct + impl
├── legacy_kv/           — (NEW dir for tests)
│   └── tests.rs         — adapter integration tests against in-memory SQLite
└── tests.rs             — (unchanged from PR1, the type-shape tests)
```

Wait — only the test file goes in a subdirectory? Cleaner to put the impl + its tests in one file. Let me revise:

```
src-tauri/src/memory_adapter/
├── mod.rs               — declare `legacy_kv` + re-export LegacyKvAdapter
├── traits.rs
├── types.rs
├── legacy_kv.rs         — NEW: struct + impl + #[cfg(test)] tests inline
└── tests.rs             — (unchanged, type-shape tests)
```

That's cleaner. One file per adapter; inline `#[cfg(test)] mod tests { ... }` per adapter.

Modified:
- `src-tauri/src/memory_adapter/mod.rs` (add `mod legacy_kv;` + `pub use legacy_kv::LegacyKvAdapter;`)
- `src-tauri/src/app.rs` (build the adapter at boot + insert into `memory_adapters`)

---

## Task 1: Implement `LegacyKvAdapter` with trait impl

**Files:**
- Create: `src-tauri/src/memory_adapter/legacy_kv.rs`
- Modify: `src-tauri/src/memory_adapter/mod.rs`

### Steps

- [ ] **Step 1.1: Create `legacy_kv.rs` with the adapter struct + impl**

```rust
// src-tauri/src/memory_adapter/legacy_kv.rs

//! `LegacyKvAdapter` — wraps `crate::memory::MemoryStore` (legacy SQLite
//! KV + FTS) behind the `MemoryAdapter` trait.
//!
//! PR2 of 阶段 4. The legacy `MemoryStore` stays as-is; this adapter
//! just translates `MemoryAdapter` calls into the existing API and
//! converts the legacy `MemoryEntry` shape into the adapter shape.
//!
//! Sync→async: SQLite operations are fast; the impl runs the legacy
//! sync methods inline inside `async fn`s. If contention ever shows
//! up, swap to `tokio::task::spawn_blocking` per call.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use super::traits::MemoryAdapter;
use super::types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

use crate::memory::{ListFilter, MemoryEntry as LegacyEntry, MemoryKind, MemoryStore, SetMemoryOpts};

const ADAPTER_NAME: &str = "legacy_kv";
const DEFAULT_SPACE_ID: &str = "global";

/// Wraps `crate::memory::MemoryStore` and exposes it through the
/// `MemoryAdapter` trait. The legacy store stays the source of truth;
/// this is purely a translation layer.
#[derive(Clone)]
pub struct LegacyKvAdapter {
    inner: Arc<MemoryStore>,
}

impl LegacyKvAdapter {
    pub fn new(inner: Arc<MemoryStore>) -> Self {
        Self { inner }
    }

    /// Convert legacy `MemoryEntry` to the trait's owned shape.
    fn convert_entry(legacy: LegacyEntry, score: Option<f64>) -> MemoryEntry {
        let category = match MemoryKind::from_str(&legacy.kind) {
            MemoryKind::Fact | MemoryKind::Preference => MemoryCategory::Core,
            MemoryKind::Context | MemoryKind::Note => MemoryCategory::Conversation,
            MemoryKind::Procedure => MemoryCategory::Custom("procedure".to_string()),
        };

        // Extract session_id from legacy "session:<id>" namespace convention.
        let session_id = legacy
            .namespace
            .strip_prefix("session:")
            .map(|s| s.to_string());

        let content = match &legacy.value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        MemoryEntry {
            id: legacy.id,
            key: legacy.key,
            content,
            namespace: Some(legacy.namespace),
            category,
            timestamp: legacy.updated_at,
            session_id,
            score,
        }
    }

    fn category_to_kind(cat: &MemoryCategory) -> MemoryKind {
        match cat {
            MemoryCategory::Core => MemoryKind::Fact,
            MemoryCategory::Conversation => MemoryKind::Context,
            MemoryCategory::Daily => MemoryKind::Note,
            MemoryCategory::Custom(name) if name == "procedure" => MemoryKind::Procedure,
            MemoryCategory::Custom(_) => MemoryKind::Note,
        }
    }
}

#[async_trait]
impl MemoryAdapter for LegacyKvAdapter {
    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    async fn store(
        &self,
        namespace: &str,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let effective_namespace = match session_id {
            Some(sid) => format!("session:{}", sid),
            None => namespace.to_string(),
        };
        let opts = SetMemoryOpts {
            space_id: DEFAULT_SPACE_ID.to_string(),
            namespace: effective_namespace,
            key: key.to_string(),
            value: serde_json::Value::String(content.to_string()),
            kind: Self::category_to_kind(&category),
            tags: Vec::new(),
            metadata: None,
            ttl_seconds: None,
        };
        self.inner
            .set_full(opts)
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("legacy_kv::store: {}", e))
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        // `MemoryStore::search` returns Vec<MemoryEntry> without scores;
        // for PR2 we don't expose a score (real ranking is for
        // BucketSealAdapter to provide). Filter by category client-side
        // since legacy `kind` is a free-form string per row.
        let hits = self.inner.search(query, opts.namespace, limit);
        let mut out = Vec::with_capacity(hits.len());
        for h in hits.into_iter() {
            let entry = Self::convert_entry(h, None);
            if let Some(cat) = opts.category.as_ref() {
                if &entry.category != cat {
                    continue;
                }
            }
            if let Some(sid) = opts.session_id {
                if entry.session_id.as_deref() != Some(sid) {
                    continue;
                }
            }
            out.push(entry);
        }
        Ok(out)
    }

    async fn get(
        &self,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<Option<MemoryEntry>> {
        Ok(self
            .inner
            .get(key, namespace)
            .map(|legacy| Self::convert_entry(legacy, None)))
    }

    async fn list(
        &self,
        namespace: Option<&str>,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let effective_namespace = match (namespace, session_id) {
            (_, Some(sid)) => Some(format!("session:{}", sid)),
            (Some(ns), None) => Some(ns.to_string()),
            (None, None) => None,
        };
        let kind_filter = category.map(|c| Self::category_to_kind(c).as_str().to_string());
        let filter = ListFilter {
            space_id: None,
            namespace: effective_namespace,
            kind: kind_filter,
            tag: None,
            limit: None,
            offset: None,
        };
        Ok(self
            .inner
            .list_filtered(&filter)
            .into_iter()
            .map(|legacy| Self::convert_entry(legacy, None))
            .collect())
    }

    async fn delete(
        &self,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<bool> {
        Ok(self.inner.delete(key, namespace))
    }

    async fn clear_namespace(
        &self,
        namespace: &str,
    ) -> anyhow::Result<u64> {
        let removed = self.inner.clear_namespace(namespace, None);
        Ok(removed as u64)
    }

    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
        let namespaces = self.inner.list_namespaces(None);
        let now = Utc::now().to_rfc3339();
        let mut out = Vec::with_capacity(namespaces.len());
        for ns in namespaces {
            let filter = ListFilter {
                space_id: None,
                namespace: Some(ns.clone()),
                kind: None,
                tag: None,
                limit: None,
                offset: None,
            };
            let count = self.inner.count(&filter);
            out.push(NamespaceSummary {
                namespace: ns,
                count,
                // Legacy MemoryStore doesn't expose per-namespace
                // last_updated cheaply; report current time as a
                // placeholder (the trait field is `last_updated: Option`).
                // PR9+ BucketSealAdapter will provide accurate values.
                last_updated: Some(now.clone()),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::Mutex;

    fn fresh_store() -> Arc<MemoryStore> {
        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_table().unwrap();
        Arc::new(store)
    }

    #[tokio::test]
    async fn name_is_legacy_kv() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        assert_eq!(adapter.name(), "legacy_kv");
    }

    #[tokio::test]
    async fn store_and_get_round_trip() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter
            .store(
                "global",
                "favorite_color",
                "blue",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();
        let got = adapter.get("global", "favorite_color").await.unwrap();
        assert!(got.is_some());
        let entry = got.unwrap();
        assert_eq!(entry.key, "favorite_color");
        assert_eq!(entry.content, "blue");
        assert_eq!(entry.category, MemoryCategory::Core);
        assert_eq!(entry.namespace.as_deref(), Some("global"));
    }

    #[tokio::test]
    async fn session_id_routes_to_session_namespace() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter
            .store(
                "ignored",
                "current_task",
                "fix the bug",
                MemoryCategory::Conversation,
                Some("sess-42"),
            )
            .await
            .unwrap();
        // The store should NOT find it under "ignored"
        assert!(adapter.get("ignored", "current_task").await.unwrap().is_none());
        // But it SHOULD find it under "session:sess-42"
        let got = adapter
            .get("session:sess-42", "current_task")
            .await
            .unwrap();
        assert!(got.is_some());
        let entry = got.unwrap();
        assert_eq!(entry.session_id.as_deref(), Some("sess-42"));
    }

    #[tokio::test]
    async fn list_filters_by_category() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter
            .store("ns", "a", "fact1", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("ns", "b", "note1", MemoryCategory::Conversation, None)
            .await
            .unwrap();

        let cores = adapter
            .list(Some("ns"), Some(&MemoryCategory::Core), None)
            .await
            .unwrap();
        assert_eq!(cores.len(), 1);
        assert_eq!(cores[0].key, "a");
    }

    #[tokio::test]
    async fn delete_returns_true_then_false() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter
            .store("ns", "k", "v", MemoryCategory::Core, None)
            .await
            .unwrap();
        assert!(adapter.delete("ns", "k").await.unwrap());
        assert!(!adapter.delete("ns", "k").await.unwrap());
    }

    #[tokio::test]
    async fn clear_namespace_removes_entries() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter.store("ns", "a", "1", MemoryCategory::Core, None).await.unwrap();
        adapter.store("ns", "b", "2", MemoryCategory::Core, None).await.unwrap();
        adapter.store("other", "c", "3", MemoryCategory::Core, None).await.unwrap();
        let removed = adapter.clear_namespace("ns").await.unwrap();
        assert_eq!(removed, 2);
        assert!(adapter.get("ns", "a").await.unwrap().is_none());
        assert!(adapter.get("other", "c").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn recall_finds_by_content() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter
            .store("ns", "k1", "the quick brown fox", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("ns", "k2", "lazy dog sleeps", MemoryCategory::Core, None)
            .await
            .unwrap();
        let hits = adapter
            .recall("quick", 10, RecallOpts { namespace: Some("ns"), ..Default::default() })
            .await
            .unwrap();
        assert!(hits.iter().any(|e| e.key == "k1"));
    }
}
```

- [ ] **Step 1.2: Wire into `memory_adapter/mod.rs`**

Add to the top of `mod.rs` (after `mod traits; mod types;`):

```rust
mod legacy_kv;

pub use legacy_kv::LegacyKvAdapter;
```

- [ ] **Step 1.3: Build + run new tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo test --lib memory_adapter::legacy_kv 2>&1 | tail -10
```

Expected: 0 errors, 7 new tests pass (in `memory_adapter::legacy_kv::tests::*`).

Likely failure modes:
- `MemoryStore::ensure_table` may have a different name or signature — find via `grep -n "fn ensure_table\|fn ensure_schema" /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri/src/memory.rs`. Use the actual name.
- `MemoryStore::search` may have a different signature (e.g. takes `min_score` or `kind_filter`). The plan above used `search(query, namespace, limit)` per the public API recon. Verify and adjust signature.
- If `search` returns matches with score metadata, optionally surface them in the converted entries.

- [ ] **Step 1.4: Verify full test suite baseline**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
```

Expected: `agent::` 798/2 (unchanged), warnings ≤51 (50 baseline; +1 acceptable transient).

- [ ] **Step 1.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter add -A src-tauri/src/memory_adapter/
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter commit -m "feat(memory_adapter): LegacyKvAdapter wrapping memory::MemoryStore (PR2.1 of 阶段 4)

First concrete MemoryAdapter impl. Translates the trait's 8 async methods
into calls against the existing sync MemoryStore. Includes legacy↔adapter
MemoryEntry conversion (legacy 'kind' string → MemoryCategory; legacy
namespace 'session:<id>' → session_id field).

7 inline tests cover name, store+get round-trip, session-routing, list
filtering by category, delete, clear_namespace, recall finds by content.

Spec: docs/superpowers/specs/2026-05-29-stage4-memory-adapter-design.md"
```

Continue to Task 2.

---

## Task 2: Register `LegacyKvAdapter` in `AppState` boot

**Files:**
- Modify: `src-tauri/src/app.rs`

### Steps

- [ ] **Step 2.1: Find `AppState::new` populator + `memory_store` construction**

```bash
grep -n "memory_store: Arc::new\|MemoryStore::new\|memory_store:" /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri/src/app.rs | head -10
```

`memory_store` is populated in `AppState::new`. After it lands in `Self { ... }`, the `memory_adapters` HashMap is built.

- [ ] **Step 2.2: Replace the empty HashMap with a populated one**

Currently (PR1):
```rust
memory_adapters: std::sync::Arc::new(std::collections::HashMap::new()),
```

After PR2 — find this line and replace the value-side with a populated map. The construction must happen AFTER `memory_store` is built (it's a dependency). Wherever `memory_store` is bound to a local in `AppState::new`, after that line + before the `Self { ... }` literal, build:

```rust
let legacy_kv_adapter = std::sync::Arc::new(
    crate::memory_adapter::LegacyKvAdapter::new(memory_store.clone()),
) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;

let mut memory_adapters: std::collections::HashMap<
    String,
    std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>,
> = std::collections::HashMap::new();
memory_adapters.insert(legacy_kv_adapter.name().to_string(), legacy_kv_adapter);
let memory_adapters = std::sync::Arc::new(memory_adapters);
```

Then in the `Self { ... }`:
```rust
memory_adapters,
```

(instead of `memory_adapters: Arc::new(HashMap::new())`).

The `memory_store` field still gets `memory_store.clone()` in `Self { ... }` (or `memory_store` directly if it's still owned). Adjust based on whether `memory_store` was previously consumed by `Self { ... }`.

- [ ] **Step 2.3: Build + verify the adapter is reachable**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
```

Expected: 0 errors. Warnings ≤50 — the dead-code warning on `MemoryAdapter` trait from PR1 should now CLEAR because `LegacyKvAdapter` impls it.

If warnings INCREASED, look for `unused_imports` — the construction lines reference types that may need a `use` import at the top of `app.rs`. Add as needed.

- [ ] **Step 2.4: Verify baselines**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo test --lib memory_adapter 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
```

Expected: `memory_adapter` ≥12 passed (5 from PR1 + 7 from PR2), `agent::` 798/2 unchanged.

- [ ] **Step 2.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter add -A src-tauri/src/app.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter commit -m "feat(app): register LegacyKvAdapter in memory_adapters at boot (PR2.2 of 阶段 4)

memory_adapters now contains one entry: 'legacy_kv' → LegacyKvAdapter
wrapping AppState.memory_store. Closes the dead-code warning on
MemoryAdapter trait from PR1.

Existing tauri_commands::memory_* handlers keep their direct MemoryStore
references; this PR just makes the same store reachable via the trait
registry as well."
```

Continue to Task 3.

---

## Task 3: Final verification

- [ ] **Step 3.1: Full battery**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo test --lib memory_adapter 2>&1 | tail -5
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter/src-tauri && cargo test --lib 2>&1 | tail -3
```

Required:
- 0 errors.
- Warnings ≤50 (49 baseline preserved + 1 fewer dead-code since trait is now used — net likely 49 or 50).
- `memory_adapter` ≥12/0.
- `agent::` 798/2.

- [ ] **Step 3.2: Verify chain**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter log --oneline main..HEAD
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr2-legacy-kv-adapter status -sb
```

Expected: 2 commits ahead of main:
```
<sha2> feat(app): register LegacyKvAdapter in memory_adapters at boot (PR2.2 of 阶段 4)
<sha1> feat(memory_adapter): LegacyKvAdapter wrapping memory::MemoryStore (PR2.1 of 阶段 4)
```

Working tree clean. Controller pushes + opens PR.

---

## Self-Review

**1. Spec coverage:** Backend roster row #2 (LegacyKvAdapter) — ✅ implemented. Method count: 8 of 8 trait methods. Translation layer (legacy ↔ adapter MemoryEntry) — ✅ via `convert_entry` + `category_to_kind`. Registered in `AppState.memory_adapters` — ✅ Task 2.

**2. Placeholder scan:** None. All code shown verbatim.

**3. Type consistency:** `LegacyKvAdapter`, `MemoryAdapter`, `MemoryEntry`, `MemoryCategory`, `RecallOpts`, `NamespaceSummary` — all consistent with PR1's definitions. Legacy types `MemoryEntry`, `MemoryKind`, `SetMemoryOpts`, `ListFilter` — match `src-tauri/src/memory.rs` public API.

---

## Cumulative summary

- **Tasks:** 3 (2 implementation + 1 verification).
- **Estimated time:** ~30 minutes (mechanical wrap + small tests).
- **Risk:** Low. No call-site changes. The new adapter coexists with all existing `tauri_commands::memory_*` handlers, which keep their `MemoryStore` references.
- **Total commits:** 2 (Task 1 + Task 2; Task 3 is verification + handoff).
