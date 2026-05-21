//! `MemoryAdapter` trait — backend contract.

use async_trait::async_trait;

use super::types::{MemoryEdge, MemoryNode, MemoryQuery, MemoryQueryResult};

/// Errors any memory backend can surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryAdapterError {
    /// Backend not connected (gbrain offline, DB unreachable).
    BackendUnavailable,
    /// Node id not found.
    NotFound(String),
    /// Backend rejected the write (e.g. validation, quota).
    Rejected(String),
    /// Transport error.
    Transport(String),
    Other(String),
}

impl std::fmt::Display for MemoryAdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackendUnavailable => write!(f, "memory adapter: backend unavailable"),
            Self::NotFound(id) => write!(f, "memory adapter: not found: {id}"),
            Self::Rejected(m) => write!(f, "memory adapter: rejected: {m}"),
            Self::Transport(m) => write!(f, "memory adapter: transport: {m}"),
            Self::Other(m) => write!(f, "memory adapter: {m}"),
        }
    }
}

impl std::error::Error for MemoryAdapterError {}

/// Per-backend contract. One impl for gbrain (M6-T1 commit 2), one
/// for SurrealDB (M6-T2), one for in-memory test fixtures.
///
/// Operations:
///
/// - **`upsert_node`** — insert or update by `node.id`. Returns the
///   final node (with adapter-assigned `updated_at`).
/// - **`upsert_edge`** — insert or update by `(source, target, kind)`.
/// - **`delete_node`** — remove a node + its incident edges. Returns
///   `Ok(true)` if anything was deleted, `Ok(false)` if absent.
/// - **`query`** — full-text + vector + filter search.
/// - **`fetch_node`** — direct lookup by id.
#[async_trait]
pub trait MemoryAdapter: Send + Sync {
    async fn upsert_node(&self, node: MemoryNode) -> Result<MemoryNode, MemoryAdapterError>;
    async fn upsert_edge(&self, edge: MemoryEdge) -> Result<MemoryEdge, MemoryAdapterError>;
    async fn delete_node(&self, id: &str) -> Result<bool, MemoryAdapterError>;
    async fn query(
        &self,
        query: &MemoryQuery,
    ) -> Result<MemoryQueryResult, MemoryAdapterError>;
    async fn fetch_node(&self, id: &str) -> Result<MemoryNode, MemoryAdapterError>;
}

#[cfg(test)]
mod tests {
    use super::super::types::{MemoryNamespace, MemoryNodeKind};
    use super::*;
    use std::sync::Mutex;

    // In-memory fake for verifying the trait shape compiles + serves
    // as a template for the gbrain impl.
    #[derive(Default)]
    struct FakeMemory {
        nodes: Mutex<Vec<MemoryNode>>,
        edges: Mutex<Vec<MemoryEdge>>,
    }

    #[async_trait]
    impl MemoryAdapter for FakeMemory {
        async fn upsert_node(
            &self,
            node: MemoryNode,
        ) -> Result<MemoryNode, MemoryAdapterError> {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(existing) = nodes.iter_mut().find(|n| n.id == node.id) {
                *existing = node.clone();
            } else {
                nodes.push(node.clone());
            }
            Ok(node)
        }
        async fn upsert_edge(
            &self,
            edge: MemoryEdge,
        ) -> Result<MemoryEdge, MemoryAdapterError> {
            self.edges.lock().unwrap().push(edge.clone());
            Ok(edge)
        }
        async fn delete_node(&self, id: &str) -> Result<bool, MemoryAdapterError> {
            let mut nodes = self.nodes.lock().unwrap();
            let before = nodes.len();
            nodes.retain(|n| n.id != id);
            Ok(nodes.len() != before)
        }
        async fn query(
            &self,
            query: &MemoryQuery,
        ) -> Result<MemoryQueryResult, MemoryAdapterError> {
            let nodes = self.nodes.lock().unwrap();
            let lower = query.text.to_ascii_lowercase();
            let mut hits: Vec<super::super::types::MemoryHit> = nodes
                .iter()
                .filter(|n| n.body.to_ascii_lowercase().contains(&lower))
                .map(|n| super::super::types::MemoryHit {
                    node: n.clone(),
                    relevance: 0.5,
                })
                .collect();
            if query.top_k > 0 {
                hits.truncate(query.top_k as usize);
            }
            Ok(MemoryQueryResult {
                hits,
                scanned: nodes.len() as u32,
            })
        }
        async fn fetch_node(&self, id: &str) -> Result<MemoryNode, MemoryAdapterError> {
            self.nodes
                .lock()
                .unwrap()
                .iter()
                .find(|n| n.id == id)
                .cloned()
                .ok_or_else(|| MemoryAdapterError::NotFound(id.into()))
        }
    }

    fn n(id: &str, body: &str) -> MemoryNode {
        MemoryNode::new(
            id,
            MemoryNamespace::UserFacts,
            MemoryNodeKind::Fact,
            body,
            "t0",
        )
    }

    #[tokio::test]
    async fn upsert_then_fetch_roundtrips() {
        let m = FakeMemory::default();
        m.upsert_node(n("n1", "rust async")).await.unwrap();
        let got = m.fetch_node("n1").await.unwrap();
        assert_eq!(got.body, "rust async");
    }

    #[tokio::test]
    async fn fetch_unknown_returns_not_found() {
        let m = FakeMemory::default();
        let err = m.fetch_node("nope").await.unwrap_err();
        assert!(matches!(err, MemoryAdapterError::NotFound(_)));
    }

    #[tokio::test]
    async fn upsert_node_replaces_existing_id() {
        let m = FakeMemory::default();
        m.upsert_node(n("n1", "original")).await.unwrap();
        m.upsert_node(n("n1", "updated")).await.unwrap();
        let got = m.fetch_node("n1").await.unwrap();
        assert_eq!(got.body, "updated");
        assert_eq!(m.nodes.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn delete_node_returns_true_when_present() {
        let m = FakeMemory::default();
        m.upsert_node(n("n1", "x")).await.unwrap();
        assert!(m.delete_node("n1").await.unwrap());
        assert!(!m.delete_node("n1").await.unwrap());
    }

    #[tokio::test]
    async fn query_filters_by_text_substring() {
        let m = FakeMemory::default();
        m.upsert_node(n("a", "rust async")).await.unwrap();
        m.upsert_node(n("b", "python sync")).await.unwrap();
        m.upsert_node(n("c", "RUST traits")).await.unwrap();
        let result = m
            .query(&MemoryQuery {
                text: "rust".into(),
                top_k: 0,
                ..Default::default()
            })
            .await
            .unwrap();
        // "a" and "c" match (case-insensitive). scanned counts all 3.
        assert_eq!(result.hits.len(), 2);
        assert_eq!(result.scanned, 3);
    }

    #[tokio::test]
    async fn query_top_k_truncates() {
        let m = FakeMemory::default();
        for i in 0..5 {
            m.upsert_node(n(&format!("n{i}"), "rust async")).await.unwrap();
        }
        let result = m
            .query(&MemoryQuery {
                text: "rust".into(),
                top_k: 2,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(result.hits.len(), 2);
    }

    // ── Error Display ────────────────────────────────────────────

    #[test]
    fn adapter_error_display() {
        for (e, contains) in [
            (MemoryAdapterError::BackendUnavailable, "backend unavailable"),
            (MemoryAdapterError::NotFound("n1".into()), "not found: n1"),
            (
                MemoryAdapterError::Rejected("quota".into()),
                "rejected: quota",
            ),
            (
                MemoryAdapterError::Transport("dns".into()),
                "transport: dns",
            ),
        ] {
            assert!(e.to_string().contains(contains));
        }
    }
}
