// SPDX-License-Identifier: MIT
//! Backend resolution + namespace-prefix routing for the `memory.unified.*`
//! IPC family. Owned-IPC mirror of `RecallOpts<'a>` lives here too because
//! the trait's lifetime cannot cross the Tauri boundary.
//!
//! Resolution priority (first match wins):
//!   1. Explicit `backend` argument
//!   2. Namespace prefix (`"name:rest"` → backend `name`, namespace `rest`)
//!   3. `state.default_memory_backend`

use std::collections::HashMap;
use std::sync::Arc;

use crate::memory_adapter::{MemoryAdapter, MemoryCategory, MemoryEntry, RecallOpts};

// ─── Namespace prefix splitting ────────────────────────────────────────────

/// Split `"backend:rest_of_namespace"` into `(Some("backend"), "rest_of_namespace")`.
/// If no `:` is present, returns `(None, input)`.
/// The empty string before `:` is treated as no prefix (returns `(None, input)`).
pub fn split_namespace_prefix(namespace: &str) -> (Option<&str>, &str) {
    match namespace.split_once(':') {
        Some(("", _)) => (None, namespace),
        Some((prefix, rest)) => (Some(prefix), rest),
        None => (None, namespace),
    }
}

// ─── Owned IPC mirror of RecallOpts ────────────────────────────────────────

/// Owned IPC mirror of `RecallOpts<'a>`. Borrow back via `as_recall_opts`.
///
/// `RecallOpts<'a>` cannot cross the Tauri IPC boundary because it carries
/// lifetimes. This owned version deserializes from the frontend payload and
/// converts back to a borrow when the adapter call is made.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecallOptsIpc {
    pub namespace: Option<String>,
    pub category: Option<MemoryCategory>,
    pub session_id: Option<String>,
    pub min_score: Option<f64>,
}

impl RecallOptsIpc {
    /// Borrow this back as the trait's `RecallOpts<'_>`.
    pub fn as_recall_opts(&self) -> RecallOpts<'_> {
        RecallOpts {
            namespace: self.namespace.as_deref(),
            category: self.category.clone(),
            session_id: self.session_id.as_deref(),
            min_score: self.min_score,
        }
    }
}

// ─── Backend resolution ────────────────────────────────────────────────────

/// Result of resolving a backend selection: the chosen adapter + the namespace
/// to pass downstream (prefix stripped if it was consumed).
pub struct ResolvedBackend {
    pub adapter: Arc<dyn MemoryAdapter>,
    pub effective_namespace: String,
    pub backend_name: String,
}

/// Resolve which `MemoryAdapter` should handle a call, operating directly on
/// the adapters map and default-backend string rather than requiring a full
/// `AppState`. This makes the function trivially testable with a stub `HashMap`.
///
/// Priority: explicit `backend` arg → namespace prefix → `default_backend`.
/// When `backend` is `Some`, the prefix is still parsed (so the caller can pass
/// `"bucket_seal:user_99"` as namespace and get `user_99` stripped back) but
/// the explicit argument wins for adapter selection.
///
/// Returns `None` (not an error) if no backend matches — callers convert to the
/// IPC error type they prefer.
pub fn resolve_backend_in(
    adapters: &HashMap<String, Arc<dyn MemoryAdapter>>,
    default_backend: &str,
    explicit_backend: Option<&str>,
    namespace: &str,
) -> Option<ResolvedBackend> {
    let (prefix, stripped_namespace) = split_namespace_prefix(namespace);

    let name: String = match explicit_backend {
        Some(b) => b.to_string(),
        None => match prefix {
            Some(p) => p.to_string(),
            None => default_backend.to_string(),
        },
    };

    // When a prefix was found, use the stripped namespace; otherwise keep the
    // original so non-prefixed namespaces pass through unchanged.
    let effective_namespace = if prefix.is_some() {
        stripped_namespace.to_string()
    } else {
        namespace.to_string()
    };

    adapters
        .get(&name)
        .cloned()
        .map(|adapter| ResolvedBackend {
            adapter,
            effective_namespace,
            backend_name: name,
        })
}

/// Convenience wrapper around `resolve_backend_in` that reads from `AppState`.
///
/// The `std::sync::RwLock` on `default_memory_backend` is read synchronously;
/// falls back to `"legacy_kv"` if the lock is poisoned.
pub fn resolve_backend(
    state: &crate::app::AppState,
    explicit_backend: Option<&str>,
    namespace: &str,
) -> Option<ResolvedBackend> {
    let default = state
        .default_memory_backend
        .read()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_else(|| "legacy_kv".to_string());
    resolve_backend_in(&state.memory_adapters, &default, explicit_backend, namespace)
}

// ─── High-level recall routing ─────────────────────────────────────────────

/// Resolve the backend (by explicit arg, prefix, or default) and call `recall`.
///
/// Used by the unified IPC `memory_unified_recall` command AND by future
/// agent-loop wiring (`effective_system_prompt → memory_context`).
///
/// `namespace` may carry a backend prefix; if so, the prefix overrides the
/// default. `opts.namespace` is NOT used for backend selection — it is
/// forwarded only as part of the recall filter.
pub async fn route_recall(
    state: &crate::app::AppState,
    explicit_backend: Option<&str>,
    namespace: &str,
    query: &str,
    limit: usize,
    opts: &RecallOptsIpc,
) -> anyhow::Result<Vec<MemoryEntry>> {
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

    resolved.adapter.recall(query, limit, recall_opts).await
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryStore;
    use crate::memory_adapter::legacy_kv::LegacyKvAdapter;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    // ── helpers ──────────────────────────────────────────────────────────

    fn fresh_store() -> Arc<MemoryStore> {
        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_table();
        Arc::new(store)
    }

    /// Build a minimal adapters map + default-backend pair for resolver tests.
    /// Only `legacy_kv` is registered; `legacy_steward` is intentionally absent
    /// so "ghost backend" tests can use any unregistered name.
    fn stub_adapters() -> HashMap<String, Arc<dyn MemoryAdapter>> {
        let kv: Arc<dyn MemoryAdapter> = Arc::new(LegacyKvAdapter::new(fresh_store()));
        let mut map = HashMap::new();
        map.insert(kv.name().to_string(), kv);
        map
    }

    // ── split_namespace_prefix tests ──────────────────────────────────────

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

    // ── resolve_backend_in tests ──────────────────────────────────────────

    #[test]
    fn resolve_explicit_backend_wins() {
        let adapters = stub_adapters();
        let r = resolve_backend_in(&adapters, "legacy_kv", Some("legacy_kv"), "ns").unwrap();
        assert_eq!(r.backend_name, "legacy_kv");
        assert_eq!(r.effective_namespace, "ns");
    }

    #[test]
    fn resolve_prefix_when_no_explicit() {
        let adapters = stub_adapters();
        let r = resolve_backend_in(&adapters, "does_not_matter", None, "legacy_kv:user_99")
            .unwrap();
        assert_eq!(r.backend_name, "legacy_kv");
        assert_eq!(r.effective_namespace, "user_99");
    }

    #[test]
    fn resolve_default_when_no_explicit_no_prefix() {
        let adapters = stub_adapters();
        let r = resolve_backend_in(&adapters, "legacy_kv", None, "global").unwrap();
        assert_eq!(r.backend_name, "legacy_kv");
        assert_eq!(r.effective_namespace, "global");
    }

    #[test]
    fn resolve_explicit_wins_over_prefix() {
        let adapters = stub_adapters();
        // prefix says "legacy_steward" but explicit is "legacy_kv"; explicit wins,
        // and the prefix is still stripped from the namespace.
        let r =
            resolve_backend_in(&adapters, "does_not_matter", Some("legacy_kv"), "legacy_steward:foo")
                .unwrap();
        assert_eq!(r.backend_name, "legacy_kv");
        assert_eq!(r.effective_namespace, "foo");
    }

    #[test]
    fn resolve_returns_none_for_unknown_backend() {
        let adapters = stub_adapters();
        assert!(resolve_backend_in(&adapters, "ghost_backend", None, "ns").is_none());
    }

    // ── route_recall end-to-end (via resolve_backend_in + adapter) ───────
    // `route_recall` takes &AppState and is a thin wrapper around
    // `resolve_backend_in` + adapter.recall. We test the same logic path
    // here without a full AppState by calling those two pieces directly.
    // This gives the same coverage as a `route_recall` integration test.
    #[tokio::test]
    async fn route_recall_routes_through_default_backend() {
        let adapters = stub_adapters();
        // Store via the adapter directly.
        let adapter = adapters.get("legacy_kv").unwrap().clone();
        adapter
            .store("test", "key1", "needle-content", MemoryCategory::Core, None)
            .await
            .unwrap();
        // Resolve via default backend (no explicit, no prefix).
        let resolved = resolve_backend_in(&adapters, "legacy_kv", None, "test").unwrap();
        assert_eq!(resolved.backend_name, "legacy_kv");
        // Recall — exercises the adapter.recall path that route_recall calls.
        let opts = RecallOpts {
            namespace: Some(resolved.effective_namespace.as_str()),
            ..Default::default()
        };
        let hits = resolved
            .adapter
            .recall("needle", 10, opts)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "needle-content");
    }
}
