//! M2-C — Context Fabric primitives.
//!
//! While [`super::contracts`] (M1-T1) defines the *task* contracts
//! (IntentSpec/TaskSpec/TaskEvent), this module introduces the
//! **context primitives** that M2 builds on:
//!
//! - [`ContextSource`] — which subsystem produced a piece of context
//!   (Conversation / Codebase / Memory / Browser / etc., 9 sources)
//! - [`ContextRef`] — a typed pointer to a fragment that lives somewhere
//!   else (DB row, file on disk, gbrain entity page). Lightweight enough
//!   to be passed across `TaskEvent::ContextAccess` without inlining the
//!   content.
//! - [`ContextFragment`] trait — the trait every concrete fragment type
//!   implements (`fetch()` to materialize, `token_estimate()` to budget,
//!   `topics()` to filter on)
//! - [`ContextArtifact`] — materialized content + citation + retrieval
//!   timestamp. Built by `ContextFragment::fetch()` when the fragment
//!   is actually injected into a prompt.
//!
//! M2-C ships the trait + 3 sample implementations (conversation
//! history, file contents, gbrain memory recall) as a **pilot**. The
//! remaining 25+ fragment types and the actual prompt-injection path
//! land in M2-D/F/G as that surface stabilizes. Nothing in this PR
//! plugs into production paths — it's purely additive scaffolding for
//! M2-F's 7 context tools.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Which uClaw subsystem produced the context. Mirrors the 9
/// "domains" the ADR §"Context Fabric" enumerates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSource {
    /// Current conversation messages (turn N-K..N).
    Conversation,
    /// Past task traces (TaskEvent rollout JSONL, `task_events_rollout`).
    TaskTrace,
    /// Source code in the workspace (file contents, grep results).
    Codebase,
    /// Active browser session DOM / screenshots / page text.
    Browser,
    /// Long-term knowledge (memory_graph for reads, gbrain for writes).
    Memory,
    /// User-authored artifacts (uploaded files, generated reports).
    Artifacts,
    /// Other agents' outputs in a team (M5+).
    Team,
    /// Automation run history + scheduled task results.
    Automation,
    /// Distributed cluster worker outputs (M9+).
    Cluster,
}

impl ContextSource {
    /// Stable string used in events / rollout / settings UI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::TaskTrace => "task_trace",
            Self::Codebase => "codebase",
            Self::Browser => "browser",
            Self::Memory => "memory",
            Self::Artifacts => "artifacts",
            Self::Team => "team",
            Self::Automation => "automation",
            Self::Cluster => "cluster",
        }
    }
}

/// A typed pointer to a fragment without inlining its content.
///
/// Cheap to clone and pass through events (e.g. `TaskEvent::ContextAccess`
/// carries one of these). The fragment may not actually exist when the
/// ref is constructed — `fetch()` is what hits storage. This is the
/// "promise" half of context retrieval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRef {
    /// Stable address within `source`. Conventional shapes:
    /// `"thread/<id>"`, `"file/<workspace-rel-path>"`,
    /// `"entity/<gbrain-slug>"`, `"trace/<task-id>"`.
    pub id: String,
    pub source: ContextSource,
    /// Optional human-readable label for UIs (e.g. "main.rs" instead
    /// of `"file/src/main.rs"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl ContextRef {
    pub fn new(source: ContextSource, id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            source,
            label: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// Citation entry — where in a fragment's body a piece of evidence came
/// from. Used by M2-G StructuredFold so the fold's claims can point back
/// to the underlying source. `evidence_ref` is opaque — typically a
/// chunk hash, a line range, or a memory_graph node id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub line: Option<u32>,
    pub evidence_ref: String,
}

/// Materialized fragment content — what `ContextFragment::fetch()` produces.
///
/// `content` is the actual text body to inject into a prompt (or fold,
/// or cite). `citations` is optional — fragments that don't carry
/// per-line provenance (e.g. plain conversation history) return an
/// empty Vec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextArtifact {
    pub r#ref: ContextRef,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<Citation>,
    /// RFC 3339 timestamp at the moment of `fetch()`. Used by M2-D
    /// diff-based re-injection to detect staleness.
    pub retrieval_ts: String,
}

/// The trait every concrete fragment type implements.
///
/// Fragments are stateless descriptors — they hold parameters
/// (conversation id, file path, query string), not content. Content is
/// materialized on demand via [`fetch`].
///
/// Implementations are **async** because most fragments hit storage
/// (DB, gbrain MCP, browser DOM serializer). Cheap synchronous
/// fragments (e.g. a constant prompt section) should still implement
/// the async trait — they just don't `await` anything inside.
#[async_trait]
pub trait ContextFragment: Send + Sync {
    /// The typed pointer to this fragment. Used for routing, dedup,
    /// and event emission. Returning `ContextRef::new` with a stable
    /// id ensures M2-D can compare "is this the same fragment I
    /// already injected last turn?".
    fn ref_(&self) -> ContextRef;

    /// Topic tags. M2-F `context.search("topic")` queries the registry
    /// of all known fragments by these tags. Mirrors `BaselineBlock::
    /// topics()` shape — kebab-lowercase strings.
    fn topics(&self) -> &'static [&'static str] {
        &[]
    }

    /// Best-effort token estimate WITHOUT performing a fetch. Used by
    /// M2-H L3 budget gating to decide whether to even attempt the
    /// fetch. Fragments that genuinely don't know their size (e.g. a
    /// search-by-query fragment) should overshoot — false positives
    /// cost a budget check, false negatives cost a real LLM call.
    fn token_estimate(&self) -> usize;

    /// Materialize the content. Hits storage. May return an error if
    /// the underlying resource is gone (file deleted, gbrain offline).
    async fn fetch(&self) -> Result<ContextArtifact, FragmentError>;
}

/// Failure modes for [`ContextFragment::fetch`].
#[derive(Debug, thiserror::Error)]
pub enum FragmentError {
    /// The fragment's target no longer exists (file deleted, message
    /// purged, etc.). Callers should drop the fragment from their
    /// set.
    #[error("fragment not found: {0}")]
    NotFound(String),

    /// Storage hit a transient error. Callers may retry.
    #[error("fragment fetch failed: {0}")]
    Storage(String),

    /// The fragment exists but was too big to materialize under the
    /// requested budget. M2-H L3 callers should respect this and skip
    /// the fragment.
    #[error("fragment exceeds budget ({needed} tokens > {budget} cap)")]
    BudgetExceeded { needed: usize, budget: usize },
}

// ────────────────────────────────────────────────────────────────────────
// Sample implementations — pilot only. M2-D/F land the production set.
// ────────────────────────────────────────────────────────────────────────

/// Inline conversation history snippet. Used to inject "the last K turns
/// in this thread" into context tools. Pilot — production wiring will
/// pull from `agent_messages` (V15 schema) via a real fetcher.
pub struct ConversationHistoryFragment {
    pub thread_id: String,
    pub turns: Vec<String>,
}

#[async_trait]
impl ContextFragment for ConversationHistoryFragment {
    fn ref_(&self) -> ContextRef {
        ContextRef::new(ContextSource::Conversation, format!("thread/{}", self.thread_id))
            .with_label(format!("conversation: {}", self.thread_id))
    }

    fn topics(&self) -> &'static [&'static str] {
        &["conversation", "history"]
    }

    fn token_estimate(&self) -> usize {
        // 4 chars/token heuristic, sum over turns.
        self.turns.iter().map(|t| t.chars().count() / 4).sum()
    }

    async fn fetch(&self) -> Result<ContextArtifact, FragmentError> {
        let content = self.turns.join("\n");
        Ok(ContextArtifact {
            r#ref: self.ref_(),
            content,
            citations: Vec::new(),
            retrieval_ts: chrono::Utc::now().to_rfc3339(),
        })
    }
}

/// File contents from the workspace, by relative path. Pilot — production
/// will respect `.gitignore` + size caps + binary detection.
pub struct WorkspaceFileFragment {
    pub workspace_rel_path: String,
    pub max_bytes: Option<usize>,
}

#[async_trait]
impl ContextFragment for WorkspaceFileFragment {
    fn ref_(&self) -> ContextRef {
        ContextRef::new(ContextSource::Codebase, format!("file/{}", self.workspace_rel_path))
            .with_label(self.workspace_rel_path.clone())
    }

    fn topics(&self) -> &'static [&'static str] {
        &["codebase", "file"]
    }

    fn token_estimate(&self) -> usize {
        // No content yet; use the max_bytes hint as the upper bound.
        // Production fragments would stat the file and use real size.
        self.max_bytes.unwrap_or(8 * 1024) / 4
    }

    async fn fetch(&self) -> Result<ContextArtifact, FragmentError> {
        let content = tokio::fs::read_to_string(&self.workspace_rel_path)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => FragmentError::NotFound(self.workspace_rel_path.clone()),
                _ => FragmentError::Storage(format!("{e}")),
            })?;
        let final_content = if let Some(cap) = self.max_bytes {
            if content.len() > cap {
                content.chars().take(cap).collect()
            } else {
                content
            }
        } else {
            content
        };
        let citation = Citation {
            line: None,
            evidence_ref: format!("file:{}", self.workspace_rel_path),
        };
        Ok(ContextArtifact {
            r#ref: self.ref_(),
            content: final_content,
            citations: vec![citation],
            retrieval_ts: chrono::Utc::now().to_rfc3339(),
        })
    }
}

/// Memory recall fragment — pulls the K most-relevant memory pages for
/// a query. Pilot — wraps an in-memory hash map; production will route
/// to gbrain MCP or memory_graph::recall depending on M2-D's decision.
pub struct MemoryRecallFragment {
    pub query: String,
    pub mock_hits: Vec<(String, String)>, // (page_id, body)
}

#[async_trait]
impl ContextFragment for MemoryRecallFragment {
    fn ref_(&self) -> ContextRef {
        ContextRef::new(
            ContextSource::Memory,
            format!("recall/{}", urlencoding_encode(&self.query)),
        )
        .with_label(format!("recall: {}", self.query))
    }

    fn topics(&self) -> &'static [&'static str] {
        &["memory", "recall"]
    }

    fn token_estimate(&self) -> usize {
        self.mock_hits.iter().map(|(_, body)| body.chars().count() / 4).sum()
    }

    async fn fetch(&self) -> Result<ContextArtifact, FragmentError> {
        if self.mock_hits.is_empty() {
            return Err(FragmentError::NotFound(format!("no recall hits for {}", self.query)));
        }
        let content = self
            .mock_hits
            .iter()
            .map(|(id, body)| format!("[{id}] {body}"))
            .collect::<Vec<_>>()
            .join("\n");
        let citations = self
            .mock_hits
            .iter()
            .map(|(id, _)| Citation {
                line: None,
                evidence_ref: format!("memory:{id}"),
            })
            .collect();
        Ok(ContextArtifact {
            r#ref: self.ref_(),
            content,
            citations,
            retrieval_ts: chrono::Utc::now().to_rfc3339(),
        })
    }
}

/// Minimal URL-encode for the recall query → ContextRef id. Production
/// would use the `url` crate; this avoids pulling it in just for the
/// pilot's id generation.
fn urlencoding_encode(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32 & 0xFF)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_source_strings_are_snake_case() {
        for src in [
            ContextSource::Conversation,
            ContextSource::TaskTrace,
            ContextSource::Codebase,
            ContextSource::Browser,
            ContextSource::Memory,
            ContextSource::Artifacts,
            ContextSource::Team,
            ContextSource::Automation,
            ContextSource::Cluster,
        ] {
            let s = src.as_str();
            assert!(
                s.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "source string '{s}' is not snake_case"
            );
        }
    }

    #[test]
    fn context_ref_with_label_round_trip() {
        let r = ContextRef::new(ContextSource::Codebase, "file/main.rs")
            .with_label("main.rs");
        let json = serde_json::to_string(&r).unwrap();
        let back: ContextRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn context_ref_serde_uses_camel_case() {
        let r = ContextRef::new(ContextSource::Browser, "page/abc");
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"source\""));
        assert!(json.contains("\"browser\""));
    }

    // ── ConversationHistoryFragment ──────────────────────────────

    #[tokio::test]
    async fn conversation_history_fetch_joins_turns() {
        let frag = ConversationHistoryFragment {
            thread_id: "t-1".into(),
            turns: vec!["hi".into(), "hello".into()],
        };
        let art = frag.fetch().await.unwrap();
        assert_eq!(art.content, "hi\nhello");
        assert_eq!(art.r#ref.source, ContextSource::Conversation);
        assert!(art.citations.is_empty());
    }

    #[test]
    fn conversation_history_token_estimate_sums_turns() {
        let frag = ConversationHistoryFragment {
            thread_id: "t-1".into(),
            turns: vec!["ab".repeat(8), "cd".repeat(8)],
        };
        // Each turn is 16 chars; 16/4 = 4 tokens each; total 8.
        assert_eq!(frag.token_estimate(), 8);
    }

    // ── WorkspaceFileFragment ────────────────────────────────────

    #[tokio::test]
    async fn workspace_file_returns_not_found_for_missing_file() {
        let frag = WorkspaceFileFragment {
            workspace_rel_path: "/tmp/uclaw-cowork-does-not-exist-zz.txt".into(),
            max_bytes: None,
        };
        let err = frag.fetch().await.unwrap_err();
        assert!(matches!(err, FragmentError::NotFound(_)));
    }

    #[tokio::test]
    async fn workspace_file_caps_content_at_max_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        tokio::fs::write(&path, "abcdefghij".repeat(100))
            .await
            .unwrap(); // 1000 chars
        let frag = WorkspaceFileFragment {
            workspace_rel_path: path.to_string_lossy().into_owned(),
            max_bytes: Some(50),
        };
        let art = frag.fetch().await.unwrap();
        assert!(art.content.chars().count() <= 50);
    }

    // ── MemoryRecallFragment ─────────────────────────────────────

    #[tokio::test]
    async fn memory_recall_returns_not_found_when_no_hits() {
        let frag = MemoryRecallFragment {
            query: "unknown".into(),
            mock_hits: vec![],
        };
        let err = frag.fetch().await.unwrap_err();
        assert!(matches!(err, FragmentError::NotFound(_)));
    }

    #[tokio::test]
    async fn memory_recall_assembles_content_and_citations() {
        let frag = MemoryRecallFragment {
            query: "topic A".into(),
            mock_hits: vec![
                ("page-1".into(), "page one body".into()),
                ("page-2".into(), "page two body".into()),
            ],
        };
        let art = frag.fetch().await.unwrap();
        assert_eq!(art.content, "[page-1] page one body\n[page-2] page two body");
        assert_eq!(art.citations.len(), 2);
        assert_eq!(art.citations[0].evidence_ref, "memory:page-1");
    }

    #[test]
    fn memory_recall_id_url_encodes_spaces() {
        let frag = MemoryRecallFragment {
            query: "find Rust traits".into(),
            mock_hits: vec![],
        };
        let r = frag.ref_();
        assert!(r.id.contains("recall/"));
        // Spaces become %20, no raw space in the id.
        assert!(!r.id.contains(' '));
    }
}
