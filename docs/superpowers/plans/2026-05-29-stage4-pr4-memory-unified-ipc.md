# 阶段 4 PR4 — `memory.unified.*` IPC Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `memory.unified.*` Tauri command family routing every call through `AppState.memory_adapters` registry by backend name, with namespace-prefix routing (`bucket_seal:foo` → bucket_seal backend) and a `route_recall` helper ready for the agent loop (not wired yet).

**Architecture:** A new `memory_adapter/router.rs` module owns backend resolution (explicit-arg beats prefix beats default). Tauri commands in `tauri_commands.rs` are thin wrappers: deserialize input → resolve backend → call trait method → serialize output. `RecallOpts<'a>` lifetime is wrapped by an owned `RecallOptsIpc` for the IPC boundary. `default_memory_backend` flips from `"bucket_seal"` (placeholder) to `"legacy_kv"` so the unified IPC family works end-to-end with the two adapters already on main.

**Tech Stack:** Rust 1.x, Tauri 2 commands, `serde`, `async_trait`, `tokio` for tests. No new deps.

---

## File Structure

| File | Purpose |
|---|---|
| `src-tauri/src/memory_adapter/router.rs` (new, ~150 LoC) | `resolve_backend()`, `split_namespace_prefix()`, `route_recall()`, `RecallOptsIpc`. All registry-aware logic lives here. |
| `src-tauri/src/memory_adapter/mod.rs` (modify, +2 LoC) | Declare `mod router;` + `pub use router::{resolve_backend, route_recall, RecallOptsIpc};` |
| `src-tauri/src/tauri_commands.rs` (modify, ~180 LoC) | Add 9 `memory_unified_*` async commands. All thin: resolve → trait call → return. |
| `src-tauri/src/main.rs` (modify, +9 LoC) | Register 9 new commands in `invoke_handler!`. |
| `src-tauri/src/app.rs` (modify, 1 LoC) | Flip default backend literal `"bucket_seal"` → `"legacy_kv"`. |

**LoC budget:** ~350 (spec). Tests inline within `router.rs` and one integration smoke test in `tauri_commands.rs` exercising one round-trip via the adapter registry directly (no Tauri runtime needed for the test).

---

## Decisions Already Locked

- **Backend resolution priority**: explicit `backend: Some("name")` arg → namespace prefix (`"name:rest"`) → `default_memory_backend`. If explicit and prefix disagree, explicit wins (the prefix is consumed but the explicit name is honored). If the resolved name is not in the registry, return `Error::NotFound`.
- **Prefix syntax**: `"backend_name:rest_of_namespace"` — first `:` is the delimiter. Backend names never contain `:` (enforce in router; legacy_kv/legacy_steward/bucket_seal/gbrain don't).
- **`set_default_backend` is runtime-only**: writes to `Arc<RwLock<String>>`. Not persisted to SQLite — restart resets to compile-time default. Persistence is a follow-up if the UI requests it.
- **Tauri command naming**: spec uses `memory_unified_record`, but the trait method is `MemoryAdapter::store`. The command is named `memory_unified_record` per spec; it calls `MemoryAdapter::store` internally. Same for `memory_unified_recall` → trait `recall`. Keep this naming asymmetry explicit in module docs.
- **`RecallOpts<'a>` handling**: introduce owned `RecallOptsIpc { namespace: Option<String>, category: Option<MemoryCategory>, session_id: Option<String>, min_score: Option<f64> }` for the IPC boundary. Provide `.as_recall_opts(&self) -> RecallOpts<'_>` borrow-conversion.
- **Default backend flip**: `app.rs:1011` changes from `"bucket_seal".to_string()` to `"legacy_kv".to_string()`. This is reversed to `"bucket_seal"` in PR12 (BucketSealAdapter registration). Test that the unified IPC works against the live default.

---

### Task 1: Router module skeleton + types

**Files:**
- Create: `src-tauri/src/memory_adapter/router.rs`
- Modify: `src-tauri/src/memory_adapter/mod.rs`

- [ ] **Step 1: Write failing tests for `split_namespace_prefix`**

Append to a new `router.rs`:

```rust
//! Backend resolution + namespace-prefix routing for the `memory.unified.*`
//! IPC family. Owned-IPC mirror of `RecallOpts<'a>` lives here too because
//! the trait's lifetime cannot cross the Tauri boundary.
//!
//! Resolution priority (first match wins):
//!   1. Explicit `backend` argument
//!   2. Namespace prefix (`"name:rest"` → backend `name`, namespace `rest`)
//!   3. `state.default_memory_backend`

use crate::app::AppState;
use crate::memory_adapter::{MemoryAdapter, MemoryCategory, RecallOpts};
use std::sync::Arc;

/// Split `"backend:rest_of_namespace"` into `(Some("backend"), "rest_of_namespace")`.
/// If no `:` is present, returns `(None, input)`.
/// The empty string before `:` is treated as no prefix.
pub fn split_namespace_prefix(namespace: &str) -> (Option<&str>, &str) {
    match namespace.split_once(':') {
        Some(("", _)) => (None, namespace),
        Some((prefix, rest)) => (Some(prefix), rest),
        None => (None, namespace),
    }
}

/// Owned IPC mirror of `RecallOpts<'a>`. Borrow back via `as_recall_opts`.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecallOptsIpc {
    pub namespace: Option<String>,
    pub category: Option<MemoryCategory>,
    pub session_id: Option<String>,
    pub min_score: Option<f64>,
}

impl RecallOptsIpc {
    pub fn as_recall_opts(&self) -> RecallOpts<'_> {
        RecallOpts {
            namespace: self.namespace.as_deref(),
            category: self.category.clone(),
            session_id: self.session_id.as_deref(),
            min_score: self.min_score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_namespace_prefix_no_colon() {
        assert_eq!(split_namespace_prefix("foo"), (None, "foo"));
    }

    #[test]
    fn split_namespace_prefix_with_colon() {
        assert_eq!(
            split_namespace_prefix("bucket_seal:user_99"),
            (Some("bucket_seal"), "user_99")
        );
    }

    #[test]
    fn split_namespace_prefix_empty_before_colon() {
        assert_eq!(split_namespace_prefix(":foo"), (None, ":foo"));
    }

    #[test]
    fn split_namespace_prefix_multiple_colons() {
        assert_eq!(
            split_namespace_prefix("legacy_kv:user:42"),
            (Some("legacy_kv"), "user:42")
        );
    }

    #[test]
    fn recall_opts_ipc_borrows() {
        let ipc = RecallOptsIpc {
            namespace: Some("ns".to_string()),
            category: Some(MemoryCategory::Core),
            session_id: Some("s1".to_string()),
            min_score: Some(0.5),
        };
        let opts = ipc.as_recall_opts();
        assert_eq!(opts.namespace, Some("ns"));
        assert_eq!(opts.session_id, Some("s1"));
        assert_eq!(opts.min_score, Some(0.5));
        assert_eq!(opts.category, Some(MemoryCategory::Core));
    }
}
```

Then in `memory_adapter/mod.rs`, add (placement: after the existing `mod legacy_steward;` line):

```rust
mod router;
pub use router::{split_namespace_prefix, RecallOptsIpc};
```

- [ ] **Step 2: Run tests — confirm 5/5 pass**

Run: `cd src-tauri && cargo test --lib memory_adapter::router::tests`
Expected: `test result: ok. 5 passed`

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/memory_adapter/router.rs src-tauri/src/memory_adapter/mod.rs
git commit -m "feat(memory_adapter): router module + RecallOptsIpc (PR4.1 of 阶段 4)"
```

---

### Task 2: `resolve_backend` + `route_recall` helpers

**Files:**
- Modify: `src-tauri/src/memory_adapter/router.rs`
- Modify: `src-tauri/src/memory_adapter/mod.rs` (re-export `resolve_backend`, `route_recall`)

- [ ] **Step 1: Add `resolve_backend` function**

Append to `router.rs` (above the `#[cfg(test)]` block):

```rust
/// Result of resolving a backend selection: the chosen adapter + the namespace
/// to pass downstream (prefix stripped if it was consumed).
pub struct ResolvedBackend {
    pub adapter: Arc<dyn MemoryAdapter>,
    pub effective_namespace: String,
    pub backend_name: String,
}

/// Resolve which `MemoryAdapter` should handle a call.
///
/// Priority: explicit `backend` arg → namespace prefix → `state.default_memory_backend`.
/// When `backend` is `Some`, the prefix is parsed (so the caller can still pass
/// `"bucket_seal:user_99"` as namespace and get `user_99` back) but the explicit
/// argument wins for adapter selection.
///
/// Returns `None` (not an error) if no backend matches — callers convert to the
/// IPC error type they prefer.
pub fn resolve_backend(
    state: &AppState,
    explicit_backend: Option<&str>,
    namespace: &str,
) -> Option<ResolvedBackend> {
    let (prefix, stripped_namespace) = split_namespace_prefix(namespace);

    let name: String = match explicit_backend {
        Some(b) => b.to_string(),
        None => match prefix {
            Some(p) => p.to_string(),
            None => state
                .default_memory_backend
                .read()
                .ok()
                .map(|s| s.clone())
                .unwrap_or_else(|| "legacy_kv".to_string()),
        },
    };

    let effective_namespace = if prefix.is_some() {
        stripped_namespace.to_string()
    } else {
        namespace.to_string()
    };

    state
        .memory_adapters
        .get(&name)
        .cloned()
        .map(|adapter| ResolvedBackend {
            adapter,
            effective_namespace,
            backend_name: name,
        })
}

/// High-level recall helper: resolve the backend (by explicit arg, prefix, or
/// default), call `recall`. Used by the unified IPC `recall` command AND by
/// future agent-loop wiring (`effective_system_prompt → memory_context`).
///
/// `namespace` may carry a prefix; if so it overrides the default. `opts.namespace`
/// is NOT used for backend selection — it's just forwarded as part of the recall filter.
pub async fn route_recall(
    state: &AppState,
    explicit_backend: Option<&str>,
    namespace: &str,
    query: &str,
    limit: usize,
    opts: &RecallOptsIpc,
) -> anyhow::Result<Vec<crate::memory_adapter::MemoryEntry>> {
    let resolved = resolve_backend(state, explicit_backend, namespace).ok_or_else(|| {
        anyhow::anyhow!(
            "memory_adapter::route_recall: backend not found (explicit={:?}, namespace={:?})",
            explicit_backend,
            namespace
        )
    })?;

    let recall_opts = RecallOpts {
        namespace: Some(resolved.effective_namespace.as_str()),
        category: opts.category.clone(),
        session_id: opts.session_id.as_deref(),
        min_score: opts.min_score,
    };

    resolved.adapter.recall(query, limit, &recall_opts).await
}
```

- [ ] **Step 2: Add tests for resolver**

Append inside the `#[cfg(test)]` block in `router.rs`:

```rust
    use crate::app::AppState;
    use crate::memory_adapter::legacy_kv::LegacyKvAdapter;
    use crate::memory::MemoryStore;
    use rusqlite::Connection;
    use std::sync::{Arc, RwLock, Mutex};
    use std::collections::HashMap;

    // Builds a minimal AppState with two registered adapters and a chosen default.
    // ONLY the fields touched by resolve_backend are populated; everything else
    // remains at whatever default the implementer's adapter pattern provides.
    fn fixture_state(default_backend: &str) -> AppState {
        let conn = Connection::open_in_memory().expect("sqlite");
        let store = Arc::new(Mutex::new(MemoryStore::new(conn)));
        MemoryStore::ensure_table(&store.lock().unwrap());
        let kv_adapter: Arc<dyn MemoryAdapter> = Arc::new(LegacyKvAdapter::new(store));

        let mut map: HashMap<String, Arc<dyn MemoryAdapter>> = HashMap::new();
        map.insert(kv_adapter.name().to_string(), kv_adapter);

        // Synthesize the rest of AppState fields via the test-only constructor
        // OR partial-fill if a builder exists. The implementer must verify
        // what constructor exists; if no test-only path is available, add one.
        crate::app::AppState::test_fixture_with_memory_adapters(
            Arc::new(map),
            Arc::new(RwLock::new(default_backend.to_string())),
        )
    }

    #[tokio::test]
    async fn resolve_explicit_backend_wins() {
        let state = fixture_state("legacy_kv");
        let r = resolve_backend(&state, Some("legacy_kv"), "ns").unwrap();
        assert_eq!(r.backend_name, "legacy_kv");
        assert_eq!(r.effective_namespace, "ns");
    }

    #[tokio::test]
    async fn resolve_prefix_when_no_explicit() {
        let state = fixture_state("does_not_matter");
        let r = resolve_backend(&state, None, "legacy_kv:user_99").unwrap();
        assert_eq!(r.backend_name, "legacy_kv");
        assert_eq!(r.effective_namespace, "user_99");
    }

    #[tokio::test]
    async fn resolve_default_when_no_explicit_no_prefix() {
        let state = fixture_state("legacy_kv");
        let r = resolve_backend(&state, None, "global").unwrap();
        assert_eq!(r.backend_name, "legacy_kv");
        assert_eq!(r.effective_namespace, "global");
    }

    #[tokio::test]
    async fn resolve_explicit_wins_over_prefix() {
        let state = fixture_state("does_not_matter");
        // prefix says "legacy_steward" but explicit is "legacy_kv"; explicit wins,
        // but prefix is still stripped from namespace
        let r = resolve_backend(&state, Some("legacy_kv"), "legacy_steward:foo").unwrap();
        assert_eq!(r.backend_name, "legacy_kv");
        assert_eq!(r.effective_namespace, "foo");
    }

    #[tokio::test]
    async fn resolve_returns_none_for_unknown_backend() {
        let state = fixture_state("ghost_backend");
        assert!(resolve_backend(&state, None, "ns").is_none());
    }

    #[tokio::test]
    async fn route_recall_routes_through_default() {
        let state = fixture_state("legacy_kv");
        // Store via the adapter, then route_recall via default — confirms end-to-end.
        let adapter = state.memory_adapters.get("legacy_kv").unwrap().clone();
        adapter
            .store("test", "key1", "needle-content", MemoryCategory::Core, None)
            .await
            .unwrap();
        let opts = RecallOptsIpc { namespace: Some("test".to_string()), ..Default::default() };
        let hits = route_recall(&state, None, "test", "needle", 10, &opts).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "needle-content");
    }
```

**Adapter responsibility note**: The plan references `AppState::test_fixture_with_memory_adapters(...)` as a test-only constructor. **Verify whether such a path exists**; if not, the implementer must add a `#[cfg(test)] pub fn test_fixture_with_memory_adapters` builder to `app.rs` that constructs an `AppState` with all other fields stubbed via reasonable defaults (or whatever existing test fixtures use). Look at how PR2/PR3 tests stood up isolated state — they didn't use the full `AppState`. **Acceptable alternative**: refactor `resolve_backend` to take `(adapters: &HashMap<_, Arc<dyn MemoryAdapter>>, default_backend: &str)` instead of `&AppState` — this decouples the helper from `AppState` entirely and tests become trivial. **Strongly prefer the refactor** if `AppState` cannot easily be stubbed.

- [ ] **Step 3: Run tests — confirm all pass**

Run: `cd src-tauri && cargo test --lib memory_adapter::router::tests`
Expected: 11/11 passing (5 from Task 1 + 6 new).

- [ ] **Step 4: Update `memory_adapter/mod.rs` re-exports**

```rust
pub use router::{resolve_backend, route_recall, split_namespace_prefix, RecallOptsIpc, ResolvedBackend};
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_adapter/router.rs src-tauri/src/memory_adapter/mod.rs src-tauri/src/app.rs
git commit -m "feat(memory_adapter): resolve_backend + route_recall helpers (PR4.2 of 阶段 4)"
```

---

### Task 3: 9 `memory_unified_*` Tauri commands

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`

- [ ] **Step 1: Add IPC input structs near the existing memory IPC inputs**

Locate the existing `MemorySetInput` / `MemoryGetInput` definitions (around line 4150 of `tauri_commands.rs`). Append below them:

```rust
// ===== memory.unified.* (PR4 of 阶段 4) =====

#[derive(Debug, serde::Deserialize)]
pub struct MemoryUnifiedRecordInput {
    pub backend: Option<String>,
    pub namespace: String,
    pub key: String,
    pub content: String,
    pub category: crate::memory_adapter::MemoryCategory,
    pub session_id: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct MemoryUnifiedRecallInput {
    pub backend: Option<String>,
    pub namespace: String,
    pub query: String,
    pub limit: usize,
    pub opts: Option<crate::memory_adapter::RecallOptsIpc>,
}

#[derive(Debug, serde::Deserialize)]
pub struct MemoryUnifiedKeyInput {
    pub backend: Option<String>,
    pub namespace: String,
    pub key: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct MemoryUnifiedListInput {
    pub backend: Option<String>,
    pub namespace: Option<String>,
    pub category: Option<crate::memory_adapter::MemoryCategory>,
    pub limit: usize,
}

#[derive(Debug, serde::Deserialize)]
pub struct MemoryUnifiedClearInput {
    pub backend: Option<String>,
    pub namespace: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct MemoryUnifiedSetDefaultInput {
    pub backend: String,
}
```

- [ ] **Step 2: Add the 9 command handlers**

Append at the bottom of `tauri_commands.rs`:

```rust
// ===== memory.unified.* commands =====
// Thin wrappers: resolve backend via `router::resolve_backend`, call trait method,
// return result. Errors when the requested backend (or default) is not registered.

fn unified_backend_not_found_error(backend: &Option<String>, namespace: &str) -> Error {
    Error::NotFound(format!(
        "memory_adapter: no backend registered (explicit={:?}, namespace={:?})",
        backend, namespace
    ))
}

#[tauri::command]
pub async fn memory_unified_record(
    state: State<'_, AppState>,
    input: MemoryUnifiedRecordInput,
) -> Result<crate::memory_adapter::MemoryEntry, Error> {
    let resolved = crate::memory_adapter::resolve_backend(
        &state,
        input.backend.as_deref(),
        &input.namespace,
    )
    .ok_or_else(|| unified_backend_not_found_error(&input.backend, &input.namespace))?;

    resolved
        .adapter
        .store(
            &resolved.effective_namespace,
            &input.key,
            &input.content,
            input.category,
            input.session_id.as_deref(),
        )
        .await
        .map_err(|e| Error::Other(format!("memory_unified_record: {}", e)))
}

#[tauri::command]
pub async fn memory_unified_recall(
    state: State<'_, AppState>,
    input: MemoryUnifiedRecallInput,
) -> Result<Vec<crate::memory_adapter::MemoryEntry>, Error> {
    let opts = input.opts.unwrap_or_default();
    crate::memory_adapter::route_recall(
        &state,
        input.backend.as_deref(),
        &input.namespace,
        &input.query,
        input.limit,
        &opts,
    )
    .await
    .map_err(|e| Error::Other(format!("memory_unified_recall: {}", e)))
}

#[tauri::command]
pub async fn memory_unified_get(
    state: State<'_, AppState>,
    input: MemoryUnifiedKeyInput,
) -> Result<Option<crate::memory_adapter::MemoryEntry>, Error> {
    let resolved = crate::memory_adapter::resolve_backend(
        &state,
        input.backend.as_deref(),
        &input.namespace,
    )
    .ok_or_else(|| unified_backend_not_found_error(&input.backend, &input.namespace))?;

    resolved
        .adapter
        .get(&resolved.effective_namespace, &input.key)
        .await
        .map_err(|e| Error::Other(format!("memory_unified_get: {}", e)))
}

#[tauri::command]
pub async fn memory_unified_list(
    state: State<'_, AppState>,
    input: MemoryUnifiedListInput,
) -> Result<Vec<crate::memory_adapter::MemoryEntry>, Error> {
    // For list, we use the namespace from input.namespace directly as the backend-selection signal;
    // an empty namespace means "use default backend, list everything".
    let ns_hint = input.namespace.clone().unwrap_or_default();
    let resolved = crate::memory_adapter::resolve_backend(
        &state,
        input.backend.as_deref(),
        &ns_hint,
    )
    .ok_or_else(|| unified_backend_not_found_error(&input.backend, &ns_hint))?;

    let opts = crate::memory_adapter::RecallOpts {
        namespace: if input.namespace.is_some() {
            Some(resolved.effective_namespace.as_str())
        } else {
            None
        },
        category: input.category.clone(),
        session_id: None,
        min_score: None,
    };

    resolved
        .adapter
        .list(input.limit, &opts)
        .await
        .map_err(|e| Error::Other(format!("memory_unified_list: {}", e)))
}

#[tauri::command]
pub async fn memory_unified_delete(
    state: State<'_, AppState>,
    input: MemoryUnifiedKeyInput,
) -> Result<bool, Error> {
    let resolved = crate::memory_adapter::resolve_backend(
        &state,
        input.backend.as_deref(),
        &input.namespace,
    )
    .ok_or_else(|| unified_backend_not_found_error(&input.backend, &input.namespace))?;

    resolved
        .adapter
        .delete(&resolved.effective_namespace, &input.key)
        .await
        .map_err(|e| Error::Other(format!("memory_unified_delete: {}", e)))
}

#[tauri::command]
pub async fn memory_unified_clear_namespace(
    state: State<'_, AppState>,
    input: MemoryUnifiedClearInput,
) -> Result<usize, Error> {
    let resolved = crate::memory_adapter::resolve_backend(
        &state,
        input.backend.as_deref(),
        &input.namespace,
    )
    .ok_or_else(|| unified_backend_not_found_error(&input.backend, &input.namespace))?;

    resolved
        .adapter
        .clear_namespace(&resolved.effective_namespace)
        .await
        .map_err(|e| Error::Other(format!("memory_unified_clear_namespace: {}", e)))
}

#[tauri::command]
pub async fn memory_unified_namespace_summaries(
    state: State<'_, AppState>,
    backend: Option<String>,
) -> Result<Vec<crate::memory_adapter::NamespaceSummary>, Error> {
    // No namespace to feed resolver — pass "" and let it fall through to explicit/default.
    let resolved = crate::memory_adapter::resolve_backend(&state, backend.as_deref(), "")
        .ok_or_else(|| unified_backend_not_found_error(&backend, ""))?;

    resolved
        .adapter
        .namespace_summaries()
        .await
        .map_err(|e| Error::Other(format!("memory_unified_namespace_summaries: {}", e)))
}

#[tauri::command]
pub async fn memory_unified_list_backends(
    state: State<'_, AppState>,
) -> Result<Vec<String>, Error> {
    let mut names: Vec<String> = state.memory_adapters.keys().cloned().collect();
    names.sort();
    Ok(names)
}

#[tauri::command]
pub async fn memory_unified_set_default_backend(
    state: State<'_, AppState>,
    input: MemoryUnifiedSetDefaultInput,
) -> Result<String, Error> {
    if !state.memory_adapters.contains_key(&input.backend) {
        return Err(Error::NotFound(format!(
            "memory_unified_set_default_backend: backend '{}' not registered",
            input.backend
        )));
    }
    {
        let mut guard = state
            .default_memory_backend
            .write()
            .map_err(|e| Error::Other(format!("default_memory_backend poisoned: {}", e)))?;
        *guard = input.backend.clone();
    }
    Ok(input.backend)
}
```

**Adapter responsibility note**: The plan uses `Error::NotFound` and `Error::Other` — **verify these variants exist** in `crate::error::Error`. If the variants differ (e.g., the enum uses `Error::NotFound { what: String }` struct-form or `Error::Generic`), adapt the constructor calls. If `Error::Other` is named differently, use the codebase's existing "generic message" variant. The implementer must not invent error variants.

- [ ] **Step 3: Run build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -20`
Expected: zero `error[` lines.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "feat(memory_adapter): memory.unified.* Tauri commands (PR4.3 of 阶段 4)"
```

---

### Task 4: Register commands + flip default backend

**Files:**
- Modify: `src-tauri/src/main.rs` (invoke_handler registrations)
- Modify: `src-tauri/src/app.rs` (default backend literal)

- [ ] **Step 1: Register the 9 commands in `invoke_handler!`**

Locate the `// Memory` section in `main.rs` around line 982-992. Below `memory_list_namespaces`, add:

```rust
            // Memory adapter unified IPC (PR4 of 阶段 4)
            uclaw_core::tauri_commands::memory_unified_record,
            uclaw_core::tauri_commands::memory_unified_recall,
            uclaw_core::tauri_commands::memory_unified_get,
            uclaw_core::tauri_commands::memory_unified_list,
            uclaw_core::tauri_commands::memory_unified_delete,
            uclaw_core::tauri_commands::memory_unified_clear_namespace,
            uclaw_core::tauri_commands::memory_unified_namespace_summaries,
            uclaw_core::tauri_commands::memory_unified_list_backends,
            uclaw_core::tauri_commands::memory_unified_set_default_backend,
```

- [ ] **Step 2: Flip default backend in `app.rs`**

Edit `src-tauri/src/app.rs` around line 1011:

```rust
            default_memory_backend: std::sync::Arc::new(std::sync::RwLock::new(
                "legacy_kv".to_string(),
            )),
```

Replace `"bucket_seal".to_string()` with `"legacy_kv".to_string()`. Add a one-line comment above explaining the temporary state:

```rust
            // Temp default during stage 4 PRs 4-11; flips back to "bucket_seal"
            // when BucketSealAdapter is registered in PR12.
```

- [ ] **Step 3: Run full build + tests**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors.

Run: `cd src-tauri && cargo test --lib memory_adapter 2>&1 | tail -10`
Expected: all `memory_adapter` tests pass (Task 1+2 router tests + existing PR1-3 tests, total >=22+11=33).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/main.rs src-tauri/src/app.rs
git commit -m "feat(app): register memory.unified.* commands + flip default to legacy_kv (PR4.4 of 阶段 4)"
```

---

### Task 5: Verification + cleanup

**Files:** (no edits — verification only)

- [ ] **Step 1: Run clippy on PR4 files**

Run: `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "warning|error" | grep -E "router\.rs|tauri_commands\.rs" | head -20`
Expected: zero warnings/errors in `router.rs` or new `memory_unified_*` blocks of `tauri_commands.rs`. Pre-existing clippy issues elsewhere are not in scope.

- [ ] **Step 2: Run full `cargo test --lib` for regression check**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -20`
Expected: net new passes (~11 router tests). Pre-existing failures (e.g., 2 in `agent::`) unchanged — these are baseline noise from before PR4.

- [ ] **Step 3: Grep for stray TODO/FIXME introduced**

Run: `cd src-tauri && grep -nE "TODO|FIXME|XXX" src/memory_adapter/router.rs src/tauri_commands.rs | grep -E "memory_unified|router"`
Expected: zero hits. If any TODO landed during implementation, justify or remove before merge.

- [ ] **Step 4: Final commit (if any nits)**

If verification surfaces small cleanups (formatting, comment fixes, unused imports), apply them and commit:

```bash
git add -A
git commit -m "chore(memory_adapter): PR4 cleanup pass"
```

If nothing to clean, skip this step.

---

## Test plan summary

| Test type | Count | Where |
|---|---|---|
| `split_namespace_prefix` cases | 4 | `router.rs::tests` |
| `RecallOptsIpc` borrow round-trip | 1 | `router.rs::tests` |
| `resolve_backend` priority cases | 5 | `router.rs::tests` |
| `route_recall` end-to-end | 1 | `router.rs::tests` |
| **Total new tests** | **11** | — |

Tauri command handlers are intentionally thin (resolve → trait call → return). They're integration-tested implicitly via the `route_recall` + `resolve_backend` tests — the commands have no business logic of their own to test in isolation. UI-driven integration testing is the right surface for the commands themselves and lands in a follow-up if any UI consumer flakes.

---

## Self-Review Checklist

- ✅ **Spec coverage**: 9 commands listed at spec line 235 → all 9 implemented. `route_recall` helper at spec line 149 → implemented. Backend resolution priority at spec line 150-151 → implemented. Default backend flip → handled in Task 4.2.
- ✅ **Type consistency**: `RecallOptsIpc` ↔ `RecallOpts<'_>` borrow conversion. `ResolvedBackend` exposed pub for advanced callers but commands only use it internally.
- ✅ **No placeholders**: every step shows the actual code. Two `Adapter responsibility note` blocks acknowledge unknowns the implementer must verify against current source (`AppState::test_fixture_with_memory_adapters`, `Error` variants) — these are explicit verification asks, not TBDs.
- ✅ **Bisectability**: 4 task commits (router skeleton / resolver+route_recall / Tauri commands / registration+default-flip) + optional cleanup commit. Each compiles standalone.
- ✅ **No scope creep**: no UI changes, no schema migration, no agent-loop wiring (that's PR15), no MemUAdapter (PR14).
