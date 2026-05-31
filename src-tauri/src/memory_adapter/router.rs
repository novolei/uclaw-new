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
        // poison-fallback to the canonical default, not the legacy adapter
        .unwrap_or_else(|| "bucket_seal".to_string());
    resolve_backend_in(&state.memory_adapters, &default, explicit_backend, namespace)
}

// ─── High-level recall routing ─────────────────────────────────────────────

/// Core recall routing logic, operating directly on the adapters map and
/// default-backend string. Testable without a full `AppState`.
///
/// `namespace` may carry a backend prefix; if so, the prefix overrides the
/// default. `opts.namespace` is NOT used for backend selection — it is
/// forwarded only as part of the recall filter.
pub async fn route_recall_in(
    adapters: &HashMap<String, Arc<dyn MemoryAdapter>>,
    default_backend: &str,
    explicit_backend: Option<&str>,
    namespace: &str,
    query: &str,
    limit: usize,
    opts: &RecallOptsIpc,
) -> anyhow::Result<Vec<MemoryEntry>> {
    let resolved =
        resolve_backend_in(adapters, default_backend, explicit_backend, namespace).ok_or_else(
            || {
                anyhow::anyhow!(
                    "memory_adapter::route_recall: backend not found (explicit={:?}, namespace={:?})",
                    explicit_backend,
                    namespace
                )
            },
        )?;

    let backend_name = resolved.backend_name.clone();
    let recall_opts = RecallOpts {
        namespace: Some(resolved.effective_namespace.as_str()),
        category: opts.category.clone(),
        session_id: opts.session_id.as_deref(),
        min_score: opts.min_score,
    };

    resolved
        .adapter
        .recall(query, limit, recall_opts)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "memory_adapter::route_recall: backend '{}' failed: {}",
                backend_name,
                e
            )
        })
}

/// Convenience wrapper around `route_recall_in` that reads from `AppState`.
///
/// Used by the unified IPC `memory_unified_recall` command AND by future
/// agent-loop wiring (`effective_system_prompt → memory_context`).
pub async fn route_recall(
    state: &crate::app::AppState,
    explicit_backend: Option<&str>,
    namespace: &str,
    query: &str,
    limit: usize,
    opts: &RecallOptsIpc,
) -> anyhow::Result<Vec<MemoryEntry>> {
    let default = state
        .default_memory_backend
        .read()
        .ok()
        .map(|g| g.clone())
        // poison-fallback to the canonical default, not the legacy adapter
        .unwrap_or_else(|| "bucket_seal".to_string());
    route_recall_in(
        &state.memory_adapters,
        &default,
        explicit_backend,
        namespace,
        query,
        limit,
        opts,
    )
    .await
}

// ─── Merge / dedupe / budget / format helpers ─────────────────────────────

/// Merge recall candidates: dedup by `content` (keep highest score), sort by
/// `score` desc (None=0.0), truncate so cumulative `content` chars ≤ `budget`.
pub fn merge_dedupe_budget(mut entries: Vec<MemoryEntry>, budget: usize) -> Vec<MemoryEntry> {
    entries.sort_by(|a, b| {
        b.score
            .unwrap_or(0.0)
            .partial_cmp(&a.score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen = std::collections::HashSet::new();
    entries.retain(|e| seen.insert(e.content.clone()));
    let mut used = 0usize;
    entries
        .into_iter()
        .take_while(|e| {
            let n = e.content.chars().count();
            if used + n <= budget {
                used += n;
                true
            } else {
                false
            }
        })
        .collect()
}

/// Render budgeted entries into a prompt block (highest-score first). Empty → "".
pub fn format_entries(entries: &[MemoryEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut s = String::from("<memory_context>\n");
    for e in entries {
        s.push_str("- ");
        s.push_str(&e.content);
        s.push('\n');
    }
    s.push_str("</memory_context>");
    s
}

/// Unified recall + assembly. Recalls via the default backend + gbrain, merges
/// with caller-supplied `extra` (proactive/session), dedups/budgets/formats.
/// Best-effort: a failing/missing backend contributes nothing. Decoupled from
/// proactive/session services (they arrive as `extra`).
pub async fn load_context(
    adapters: &HashMap<String, std::sync::Arc<dyn MemoryAdapter>>,
    default_backend: &str,
    query: &str,
    budget: usize,
    extra: Vec<MemoryEntry>,
) -> String {
    let mut all = extra;
    let mut sources = vec![default_backend];
    if default_backend != "gbrain" {
        sources.push("gbrain");
    }
    for name in sources {
        if let Some(ad) = adapters.get(name) {
            match ad.recall(query, 6, RecallOpts::default()).await {
                Ok(mut hits) => all.append(&mut hits),
                Err(e) => tracing::debug!(
                    backend = name,
                    error = %e,
                    "load_context: recall failed; skipping"
                ),
            }
        }
    }
    format_entries(&merge_dedupe_budget(all, budget))
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

    // ── Test A: resolve_backend_in returns None for unknown default ───────

    #[test]
    fn resolve_returns_none_for_unknown_default_too() {
        // Even when there's no explicit/prefix and the default points to a non-registered backend.
        let adapters = stub_adapters();
        let result = resolve_backend_in(&adapters, "ghost_backend", None, "ns");
        assert!(result.is_none(), "unknown default backend should resolve to None");
    }

    // ── Test: bucket_seal is the canonical default ────────────────────────

    #[test]
    fn resolve_bucket_seal_as_canonical_default() {
        let kv: Arc<dyn MemoryAdapter> = Arc::new(LegacyKvAdapter::new(fresh_store()));
        let bucket: Arc<dyn MemoryAdapter> = Arc::new(LegacyKvAdapter::new(fresh_store()));
        let mut adapters: HashMap<String, Arc<dyn MemoryAdapter>> = HashMap::new();
        adapters.insert("legacy_kv".to_string(), kv);
        adapters.insert("bucket_seal".to_string(), bucket);
        let r = resolve_backend_in(&adapters, "bucket_seal", None, "global").unwrap();
        assert_eq!(r.backend_name, "bucket_seal");
        assert_eq!(r.effective_namespace, "global");
    }

    // ── Test B: default-backend flip persists across resolves ─────────────

    /// Build a two-entry adapters map using two distinct `LegacyKvAdapter`
    /// instances registered under different names. This verifies the lock-flip
    /// test without requiring a heavier `LegacyStewardAdapter` setup.
    fn stub_adapters_with_two_kinds() -> HashMap<String, Arc<dyn MemoryAdapter>> {
        use crate::memory_adapter::legacy_kv::LegacyKvAdapter;
        let kv_a: Arc<dyn MemoryAdapter> = Arc::new(LegacyKvAdapter::new(fresh_store()));
        let kv_b_raw = LegacyKvAdapter::new(fresh_store());
        // Register kv_b under a different name via a newtype wrapper that lies about name().
        // Simpler: just register the same type under the second name by inserting directly.
        let kv_b: Arc<dyn MemoryAdapter> = Arc::new(kv_b_raw);
        let mut map = HashMap::new();
        map.insert("legacy_kv".to_string(), kv_a);
        map.insert("legacy_steward".to_string(), kv_b);
        map
    }

    #[tokio::test]
    async fn default_backend_flip_via_lock_persists() {
        use std::sync::{Arc, RwLock};
        let adapters = stub_adapters_with_two_kinds();
        let default = Arc::new(RwLock::new("legacy_kv".to_string()));

        // Before flip
        let r1 = {
            let d = default.read().unwrap().clone();
            resolve_backend_in(&adapters, &d, None, "ns").unwrap()
        };
        assert_eq!(r1.backend_name, "legacy_kv");

        // Flip
        *default.write().unwrap() = "legacy_steward".to_string();

        // After flip
        let r2 = {
            let d = default.read().unwrap().clone();
            resolve_backend_in(&adapters, &d, None, "ns").unwrap()
        };
        assert_eq!(r2.backend_name, "legacy_steward");
    }

    // ── Test C: route_recall_in end-to-end ────────────────────────────────

    #[tokio::test]
    async fn route_recall_in_routes_through_default() {
        let adapters = stub_adapters();
        let adapter = adapters.get("legacy_kv").unwrap().clone();
        adapter
            .store("test", "key1", "needle-content", MemoryCategory::Core, None)
            .await
            .unwrap();

        let opts = RecallOptsIpc {
            namespace: Some("test".to_string()),
            ..Default::default()
        };
        let hits = route_recall_in(&adapters, "legacy_kv", None, "test", "needle", 10, &opts)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "needle-content");
    }

    #[tokio::test]
    async fn route_recall_in_errors_when_backend_missing() {
        let adapters = stub_adapters();
        let opts = RecallOptsIpc::default();
        let err = route_recall_in(&adapters, "ghost_backend", None, "ns", "q", 5, &opts)
            .await
            .unwrap_err();
        // Error message should mention the missing backend name (fix #1 validated here too).
        let msg = err.to_string();
        assert!(
            msg.contains("backend not found") || msg.contains("ghost_backend"),
            "unexpected error: {}",
            msg
        );
    }

    // ── A.3: merge_dedupe_budget + format_entries + load_context ─────────

    fn mk_entry(id: &str, content: &str, score: Option<f64>) -> MemoryEntry {
        MemoryEntry {
            id: id.to_string(),
            key: id.to_string(),
            content: content.to_string(),
            namespace: None,
            category: MemoryCategory::Core,
            timestamp: String::new(),
            session_id: None,
            score,
        }
    }

    #[test]
    fn merge_dedupe_budget_sorts_dedups_and_truncates() {
        let entries = vec![
            mk_entry("a", "high score fact", Some(0.9)),
            mk_entry("b", "low score fact", Some(0.2)),
            mk_entry("a2", "high score fact", Some(0.5)),
            mk_entry("c", "mid fact", Some(0.6)),
        ];
        let out = merge_dedupe_budget(entries.clone(), 10_000);
        assert_eq!(out.len(), 3);
        assert!(out[0].score.unwrap() >= out[1].score.unwrap());
        let tiny = merge_dedupe_budget(entries, 20);
        let chars: usize = tiny.iter().map(|e| e.content.chars().count()).sum();
        assert!(chars <= 20);
    }

    #[test]
    fn format_entries_empty_is_empty_string() {
        assert_eq!(format_entries(&[]), "");
    }

    #[test]
    fn format_entries_renders_content() {
        assert!(format_entries(&[mk_entry("a", "remember X", Some(0.9))]).contains("remember X"));
    }
}
