// SPDX-License-Identifier: Apache-2.0
//! `MemUAdapter` — wraps the memU bridge (item-based memory) behind the
//! `MemoryAdapter` trait. Marshalling layer over `MemUClient`
//! (`create_item` / `retrieve_with_context` / `list_items`). memU items are
//! id-addressed, so `get`/`delete`/`clear_namespace` by `(namespace, key)`
//! are no-ops (no key→item-id mapping); the adapter's value is recall +
//! append + list. Mirrors `gbrain.rs`.
//!
//! # Encoding conventions
//! - `MemoryCategory` → memU `memory_type` string (`core`/`daily`/`conversation`/`custom:X`).
//! - `namespace` → first element of `memory_categories` on `create_item`;
//!   recovered from `categories[0]` on read (best-effort).
//! - `id` — memU `EnrichedMemoryItem` has no stable `id` field; a fresh
//!   UUID is generated at unmarshal time. `ListItemsResult.items` are
//!   `serde_json::Value`; the `id` field is extracted if present.
//! - `score` — `EnrichedMemoryItem.relevance_score` mapped directly.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::memu::client::MemUClient;
use crate::memory::EnrichedMemoryItem;
use crate::memory_adapter::traits::MemoryAdapter;
use crate::memory_adapter::types::{
    MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts,
};

// ─── Category encoding ───────────────────────────────────────────────────────

fn category_to_str(c: &MemoryCategory) -> String {
    match c {
        MemoryCategory::Core => "core".to_string(),
        MemoryCategory::Daily => "daily".to_string(),
        MemoryCategory::Conversation => "conversation".to_string(),
        MemoryCategory::Custom(s) => format!("custom:{s}"),
    }
}

fn category_from_str(s: &str) -> MemoryCategory {
    match s {
        "core" => MemoryCategory::Core,
        "daily" => MemoryCategory::Daily,
        "conversation" => MemoryCategory::Conversation,
        other => match other.strip_prefix("custom:") {
            Some(c) => MemoryCategory::Custom(c.to_string()),
            None => MemoryCategory::Conversation,
        },
    }
}

// ─── Pure marshalling fns ─────────────────────────────────────────────────────

/// Map an `EnrichedMemoryItem` (from `retrieve_with_context`) to a `MemoryEntry`.
///
/// `EnrichedMemoryItem` has no stable `id` — a fresh UUID is generated.
/// `namespace` is recovered from `categories[0]` (best-effort; often absent on
/// the fast path where `include_categories=false`).
fn enriched_item_to_entry(item: &EnrichedMemoryItem) -> MemoryEntry {
    let namespace = item.categories.first().map(|c| c.clone());
    MemoryEntry {
        id: Uuid::new_v4().to_string(),
        key: item.memory_type.clone(),
        content: item.content.clone(),
        namespace,
        category: category_from_str(&item.memory_type),
        timestamp: item
            .created_at
            .clone()
            .unwrap_or_else(|| Utc::now().to_rfc3339()),
        session_id: None,
        score: Some(item.relevance_score),
    }
}

/// Map a `serde_json::Value` item element from `ListItemsResult.items` to a
/// `MemoryEntry`. Parses defensively — missing fields fall back to defaults.
fn list_item_to_entry(v: &serde_json::Value) -> MemoryEntry {
    let id = v
        .get("id")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let id = if id.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        id
    };

    let memory_type = v
        .get("memory_type")
        .and_then(|x| x.as_str())
        .unwrap_or("conversation")
        .to_string();

    let content = v
        .get("memory_content")
        .or_else(|| v.get("content"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();

    // Recover namespace from the first element of `memory_categories`.
    let namespace = v
        .get("memory_categories")
        .and_then(|x| x.as_array())
        .and_then(|arr| arr.first())
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());

    let timestamp = v
        .get("created_at")
        .or_else(|| v.get("updated_at"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let timestamp = if timestamp.is_empty() {
        Utc::now().to_rfc3339()
    } else {
        timestamp
    };

    MemoryEntry {
        id,
        key: memory_type.clone(),
        content,
        namespace,
        category: category_from_str(&memory_type),
        timestamp,
        session_id: None,
        score: v.get("relevance_score").and_then(|x| x.as_f64()),
    }
}

/// Derive `Vec<NamespaceSummary>` from a flat list of `MemoryEntry` items by
/// grouping on the `namespace` field. Namespace-less items are skipped.
fn summaries_from_items(items: &[MemoryEntry]) -> Vec<NamespaceSummary> {
    use std::collections::BTreeMap;
    let mut acc: BTreeMap<String, (usize, Option<String>)> = BTreeMap::new();
    for item in items {
        let Some(ref ns) = item.namespace else {
            continue;
        };
        let entry = acc.entry(ns.clone()).or_insert((0, None));
        entry.0 += 1;
        // Track the lexicographically-latest RFC3339 timestamp (string sort
        // is correct for RFC3339).
        if !item.timestamp.is_empty() {
            if entry
                .1
                .as_deref()
                .map(|cur| item.timestamp.as_str() > cur)
                .unwrap_or(true)
            {
                entry.1 = Some(item.timestamp.clone());
            }
        }
    }
    acc.into_iter()
        .map(|(namespace, (count, last_updated))| NamespaceSummary {
            namespace,
            count,
            last_updated,
        })
        .collect()
}

// ─── MemUAdapter ─────────────────────────────────────────────────────────────

/// memU wrapped as a `MemoryAdapter`. Holds an `Option<Arc<MemUClient>>` —
/// `None` when the bridge is not running (registered always; methods return
/// `Err` when absent). Mirrors `GbrainAdapter`.
pub struct MemUAdapter {
    client: Option<Arc<MemUClient>>,
}

impl MemUAdapter {
    pub fn new(client: Option<Arc<MemUClient>>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl MemoryAdapter for MemUAdapter {
    fn name(&self) -> &str {
        "memu"
    }

    async fn store(
        &self,
        namespace: &str,
        _key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("memu bridge not available"))?;

        // memU items are id-addressed; `key` has no home. We fold the
        // namespace into `memory_categories` so `list_items(category=ns)`
        // can filter by it later.
        let user_scope = session_id.map(|s| serde_json::json!({ "session_id": s }));
        client
            .create_item(
                &category_to_str(&category),
                content,
                vec![namespace.to_string()],
                user_scope,
            )
            .await
            .map_err(|e| anyhow::anyhow!("memu create_item: {e}"))?;
        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("memu bridge not available"))?;

        // Fast path: include_categories=false avoids the 90s LLM-enrichment
        // timeout. Categories are not returned but we filter by them
        // post-fetch using the memory_type field.
        let items = client
            .retrieve_with_context(query, None, limit, false)
            .await
            .map_err(|e| anyhow::anyhow!("memu retrieve_with_context: {e}"))?;

        let mut out = Vec::new();
        for item in &items {
            let entry = enriched_item_to_entry(item);
            if let Some(ns) = opts.namespace {
                if entry.namespace.as_deref() != Some(ns) {
                    continue;
                }
            }
            if let Some(min) = opts.min_score {
                if entry.score.map(|s| s < min).unwrap_or(false) {
                    continue;
                }
            }
            if let Some(ref cat) = opts.category {
                if &entry.category != cat {
                    continue;
                }
            }
            out.push(entry);
        }
        Ok(out)
    }

    /// memU is id-addressed; there is no key→item-id mapping.
    /// Always returns `Ok(None)`.
    async fn get(&self, _namespace: &str, _key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        Ok(None)
    }

    async fn list(
        &self,
        namespace: Option<&str>,
        _category: Option<&MemoryCategory>,
        _session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("memu bridge not available"))?;

        // Use namespace as a memU `category` filter — items are stored with
        // `memory_categories = [namespace]`, so `list_items(category=ns)` narrows by it.
        let result = client
            .list_items(namespace, None, Some(200), Some(0), None)
            .await
            .map_err(|e| anyhow::anyhow!("memu list_items: {e}"))?;

        Ok(result.items.iter().map(list_item_to_entry).collect())
    }

    /// memU has no key→item-id mapping; delete by `(namespace, key)` is a no-op.
    async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<bool> {
        tracing::warn!(
            namespace = %namespace,
            key = %key,
            "memu has no key→item-id mapping — delete is a no-op"
        );
        Ok(false)
    }

    /// memU has no key→item-id mapping; clear by namespace is a no-op.
    async fn clear_namespace(&self, namespace: &str) -> anyhow::Result<u64> {
        tracing::warn!(
            namespace = %namespace,
            "memu has no key→item-id mapping — clear_namespace is a no-op"
        );
        Ok(0)
    }

    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("memu bridge not available"))?;

        let result = client
            .list_items(None, None, Some(200), Some(0), None)
            .await
            .map_err(|e| anyhow::anyhow!("memu list_items: {e}"))?;

        let entries: Vec<MemoryEntry> = result.items.iter().map(list_item_to_entry).collect();
        Ok(summaries_from_items(&entries))
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── category_to_str / category_from_str round-trip ───────────────────

    #[test]
    fn category_round_trips_core() {
        let s = category_to_str(&MemoryCategory::Core);
        assert_eq!(s, "core");
        assert!(matches!(category_from_str(&s), MemoryCategory::Core));
    }

    #[test]
    fn category_round_trips_daily() {
        let s = category_to_str(&MemoryCategory::Daily);
        assert_eq!(s, "daily");
        assert!(matches!(category_from_str(&s), MemoryCategory::Daily));
    }

    #[test]
    fn category_round_trips_conversation() {
        let s = category_to_str(&MemoryCategory::Conversation);
        assert_eq!(s, "conversation");
        assert!(matches!(
            category_from_str(&s),
            MemoryCategory::Conversation
        ));
    }

    #[test]
    fn category_round_trips_custom() {
        let s = category_to_str(&MemoryCategory::Custom("notes".into()));
        assert_eq!(s, "custom:notes");
        assert!(matches!(
            category_from_str(&s),
            MemoryCategory::Custom(ref v) if v == "notes"
        ));
    }

    #[test]
    fn category_from_str_unknown_falls_back_to_conversation() {
        let cat = category_from_str("unknown_type");
        assert!(matches!(cat, MemoryCategory::Conversation));
    }

    // ── enriched_item_to_entry ────────────────────────────────────────────

    #[test]
    fn enriched_item_maps_content_and_score() {
        let item = EnrichedMemoryItem {
            content: "User prefers dark mode".into(),
            memory_type: "core".into(),
            relevance_score: 0.95,
            categories: vec!["settings".into()],
            metadata: serde_json::json!({}),
            created_at: Some("2026-05-30T10:00:00Z".into()),
        };
        let entry = enriched_item_to_entry(&item);
        assert_eq!(entry.content, "User prefers dark mode");
        assert!(matches!(entry.category, MemoryCategory::Core));
        assert_eq!(entry.score, Some(0.95));
        assert_eq!(entry.timestamp, "2026-05-30T10:00:00Z");
        // namespace from categories[0]
        assert_eq!(entry.namespace.as_deref(), Some("settings"));
        // key is memory_type
        assert_eq!(entry.key, "core");
        // id is a non-empty UUID string
        assert!(!entry.id.is_empty());
    }

    #[test]
    fn enriched_item_no_categories_gives_none_namespace() {
        let item = EnrichedMemoryItem {
            content: "some content".into(),
            memory_type: "daily".into(),
            relevance_score: 0.5,
            categories: vec![],
            metadata: serde_json::json!({}),
            created_at: None,
        };
        let entry = enriched_item_to_entry(&item);
        assert!(entry.namespace.is_none());
        assert!(matches!(entry.category, MemoryCategory::Daily));
        // timestamp falls back to now (non-empty)
        assert!(!entry.timestamp.is_empty());
        assert_eq!(entry.score, Some(0.5));
    }

    #[test]
    fn enriched_item_custom_type_maps_to_conversation_fallback() {
        let item = EnrichedMemoryItem {
            content: "x".into(),
            memory_type: "behavior".into(), // not a known category string
            relevance_score: 0.0,
            categories: vec![],
            metadata: serde_json::json!({}),
            created_at: None,
        };
        let entry = enriched_item_to_entry(&item);
        assert!(matches!(entry.category, MemoryCategory::Conversation));
    }

    // ── list_item_to_entry ────────────────────────────────────────────────

    #[test]
    fn list_item_maps_all_fields() {
        let v = serde_json::json!({
            "id": "item-123",
            "memory_type": "daily",
            "memory_content": "Did morning standup",
            "memory_categories": ["work"],
            "created_at": "2026-05-30T08:00:00Z",
            "relevance_score": 0.7
        });
        let entry = list_item_to_entry(&v);
        assert_eq!(entry.id, "item-123");
        assert_eq!(entry.key, "daily");
        assert_eq!(entry.content, "Did morning standup");
        assert_eq!(entry.namespace.as_deref(), Some("work"));
        assert_eq!(entry.timestamp, "2026-05-30T08:00:00Z");
        assert!(matches!(entry.category, MemoryCategory::Daily));
        assert_eq!(entry.score, Some(0.7));
    }

    #[test]
    fn list_item_falls_back_to_content_field() {
        let v = serde_json::json!({
            "memory_type": "core",
            "content": "fallback content",
        });
        let entry = list_item_to_entry(&v);
        assert_eq!(entry.content, "fallback content");
    }

    #[test]
    fn list_item_missing_id_gets_uuid() {
        let v = serde_json::json!({ "memory_type": "core", "memory_content": "x" });
        let entry = list_item_to_entry(&v);
        assert!(!entry.id.is_empty());
    }

    #[test]
    fn list_item_missing_timestamp_falls_back_to_now() {
        let v = serde_json::json!({ "memory_type": "core" });
        let entry = list_item_to_entry(&v);
        assert!(!entry.timestamp.is_empty());
    }

    #[test]
    fn list_item_no_categories_gives_none_namespace() {
        let v = serde_json::json!({ "memory_type": "core", "memory_categories": [] });
        let entry = list_item_to_entry(&v);
        assert!(entry.namespace.is_none());
    }

    // ── summaries_from_items ──────────────────────────────────────────────

    #[test]
    fn summaries_group_by_namespace() {
        let items = vec![
            MemoryEntry {
                id: "1".into(),
                key: "k1".into(),
                content: "c1".into(),
                namespace: Some("work".into()),
                category: MemoryCategory::Core,
                timestamp: "2026-05-30T01:00:00Z".into(),
                session_id: None,
                score: None,
            },
            MemoryEntry {
                id: "2".into(),
                key: "k2".into(),
                content: "c2".into(),
                namespace: Some("work".into()),
                category: MemoryCategory::Daily,
                timestamp: "2026-05-30T02:00:00Z".into(),
                session_id: None,
                score: None,
            },
            MemoryEntry {
                id: "3".into(),
                key: "k3".into(),
                content: "c3".into(),
                namespace: Some("personal".into()),
                category: MemoryCategory::Conversation,
                timestamp: "2026-05-29T12:00:00Z".into(),
                session_id: None,
                score: None,
            },
        ];
        let mut s = summaries_from_items(&items);
        s.sort_by(|a, b| a.namespace.cmp(&b.namespace));
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].namespace, "personal");
        assert_eq!(s[0].count, 1);
        assert_eq!(s[1].namespace, "work");
        assert_eq!(s[1].count, 2);
        // latest timestamp in "work"
        assert_eq!(
            s[1].last_updated.as_deref(),
            Some("2026-05-30T02:00:00Z")
        );
    }

    #[test]
    fn summaries_skip_namespace_less_items() {
        let items = vec![
            MemoryEntry {
                id: "1".into(),
                key: "k".into(),
                content: "c".into(),
                namespace: None, // no namespace — skipped
                category: MemoryCategory::Core,
                timestamp: "2026-05-30T00:00:00Z".into(),
                session_id: None,
                score: None,
            },
            MemoryEntry {
                id: "2".into(),
                key: "k2".into(),
                content: "c2".into(),
                namespace: Some("ns".into()),
                category: MemoryCategory::Daily,
                timestamp: "2026-05-30T00:00:00Z".into(),
                session_id: None,
                score: None,
            },
        ];
        let s = summaries_from_items(&items);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].namespace, "ns");
    }

    #[test]
    fn summaries_empty_input_returns_empty() {
        let s = summaries_from_items(&[]);
        assert!(s.is_empty());
    }
}
