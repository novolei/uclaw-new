//! M2-F pilot — `ContextToolSet`: the seven context-on-demand tools an
//! agent invokes during a turn.
//!
//! Per ADR §"Context Fabric" the agent has 7 first-class operations
//! over context:
//!
//! | Tool | Purpose |
//! |---|---|
//! | `search` | "find me fragments about X" — discover by topic |
//! | `read` | "give me the content of this ref" — materialize one fragment |
//! | `fold` | "summarize N fragments into a StructuredFold" — M2-G |
//! | `cite` | "show citations for this artifact" |
//! | `compare` | "diff two refs" |
//! | `pin` | "keep this fragment hot across turns" |
//! | `release` | "drop a pinned fragment" |
//!
//! This pilot ships **`search` + `read`** as working implementations
//! and provides the other 5 as `Err(unimplemented)` stubs so the API
//! surface is locked in. Later M2-G/D PRs replace the stubs with
//! real implementations.
//!
//! Design choice: `ContextToolSet` **holds the fragment set itself**
//! (as `Arc<dyn ContextFragment>`). A fragment registry / discovery
//! mechanism (M2-Cfollow-up) decides which fragments enter the set;
//! tools operate over whatever is in there. This avoids coupling the
//! tools to a global registry while M2-D works out the fragment
//! lifecycle (when does a fragment get added/removed/refreshed).

use std::sync::Arc;

use crate::runtime::context::{
    ContextArtifact, ContextFragment, ContextRef, FragmentError,
};

/// A bag of context-on-demand operations. Holds a fragment set,
/// answers `search`/`read` against it, and stubs out the rest of the
/// 7-tool surface for M2-G/D follow-ups.
#[derive(Default)]
pub struct ContextToolSet {
    /// Fragments currently visible to the agent. M2-D will manage
    /// add/remove lifecycle; for now callers populate this directly.
    available: Vec<Arc<dyn ContextFragment>>,
    /// Fragments the agent has explicitly pinned (kept hot across turns).
    /// Mirrors `available` storage shape — distinct field so `pin` /
    /// `release` are observable in tests.
    pinned: Vec<Arc<dyn ContextFragment>>,
}

impl ContextToolSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a fragment so subsequent `search` / `read` can find it.
    /// This is the "available set" the agent works with — typically
    /// populated by the M2-D context manager before the agent loop
    /// hands off to the LLM.
    pub fn add(&mut self, fragment: Arc<dyn ContextFragment>) {
        self.available.push(fragment);
    }

    /// Bulk-add convenience.
    pub fn add_all<I>(&mut self, fragments: I)
    where
        I: IntoIterator<Item = Arc<dyn ContextFragment>>,
    {
        self.available.extend(fragments);
    }

    /// Number of fragments currently in the available set (excludes pinned).
    pub fn available_len(&self) -> usize {
        self.available.len()
    }

    pub fn pinned_len(&self) -> usize {
        self.pinned.len()
    }

    // ── Tool 1: search ──────────────────────────────────────────────

    /// Return the refs of every fragment whose `topics()` includes
    /// `topic`. Case-sensitive exact match — caller is expected to
    /// pass kebab-lowercase (matching the convention established in
    /// M2-A `BaselineBlock::topics()`).
    ///
    /// Implementation choice: linear scan over `available + pinned`.
    /// uClaw's typical context set is < 100 fragments per turn, so
    /// O(n) is fine. If profiling later shows hot path, a topic →
    /// indexer can be added without changing the API.
    pub fn search(&self, topic: &str) -> Vec<ContextRef> {
        self.available
            .iter()
            .chain(self.pinned.iter())
            .filter(|f| f.topics().iter().any(|t| *t == topic))
            .map(|f| f.ref_())
            .collect()
    }

    // ── Tool 2: read ────────────────────────────────────────────────

    /// Materialize the fragment matching `target` and return its
    /// `ContextArtifact`. Looks in `available` first, then `pinned`.
    /// Returns `FragmentError::NotFound` if no fragment matches.
    pub async fn read(&self, target: &ContextRef) -> Result<ContextArtifact, FragmentError> {
        let frag = self
            .available
            .iter()
            .chain(self.pinned.iter())
            .find(|f| f.ref_() == *target)
            .ok_or_else(|| FragmentError::NotFound(target.id.clone()))?;
        frag.fetch().await
    }

    // ── Tool 3: pin (working — see release below) ───────────────────

    /// Move a fragment from `available` to `pinned`. Returns
    /// `FragmentError::NotFound` if no available fragment matches.
    /// Idempotent — re-pinning a pinned fragment is a no-op.
    pub fn pin(&mut self, target: &ContextRef) -> Result<(), FragmentError> {
        // Idempotency check first
        if self.pinned.iter().any(|f| f.ref_() == *target) {
            return Ok(());
        }
        let idx = self
            .available
            .iter()
            .position(|f| f.ref_() == *target)
            .ok_or_else(|| FragmentError::NotFound(target.id.clone()))?;
        let f = self.available.swap_remove(idx);
        self.pinned.push(f);
        Ok(())
    }

    /// Inverse of `pin` — moves the fragment back from `pinned` to
    /// `available`. `FragmentError::NotFound` if no pinned fragment
    /// matches.
    pub fn release(&mut self, target: &ContextRef) -> Result<(), FragmentError> {
        let idx = self
            .pinned
            .iter()
            .position(|f| f.ref_() == *target)
            .ok_or_else(|| FragmentError::NotFound(target.id.clone()))?;
        let f = self.pinned.swap_remove(idx);
        self.available.push(f);
        Ok(())
    }

    // ── Tools 4-6: stubs (M2-G + M2-D follow-ups) ──────────────────

    /// `fold` produces a StructuredFold from N fragments. **Unimplemented**
    /// here; M2-G ships the 8-field structured fold body. Returning
    /// `Err(FragmentError::Storage)` instead of panicking lets callers
    /// graceful-degrade (e.g. fall back to raw concat) while the M2-G
    /// design lands.
    pub async fn fold(
        &self,
        _refs: &[ContextRef],
    ) -> Result<ContextArtifact, FragmentError> {
        Err(FragmentError::Storage(
            "context.fold is M2-G — not implemented in pilot".into(),
        ))
    }

    /// `cite` extracts citations from a previously-fetched artifact.
    /// Pilot implementation: returns the citations the artifact
    /// already carries. (The trait-level Citation field is
    /// already populated by `fetch()`.) Replace with richer
    /// citation logic in M2-G.
    pub fn cite(&self, artifact: &ContextArtifact) -> Vec<crate::runtime::context::Citation> {
        artifact.citations.clone()
    }

    /// `compare` diffs two refs. **Unimplemented** — M2-G's
    /// StructuredFold encodes diffing semantics and we don't want
    /// to commit a half-baked diff format here.
    pub async fn compare(
        &self,
        _a: &ContextRef,
        _b: &ContextRef,
    ) -> Result<String, FragmentError> {
        Err(FragmentError::Storage(
            "context.compare is M2-G — not implemented in pilot".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::context::{
        ContextSource, ConversationHistoryFragment, MemoryRecallFragment,
        WorkspaceFileFragment,
    };

    fn sample_conversation() -> Arc<dyn ContextFragment> {
        Arc::new(ConversationHistoryFragment {
            thread_id: "t-1".into(),
            turns: vec!["hi".into(), "world".into()],
        })
    }

    fn sample_memory() -> Arc<dyn ContextFragment> {
        Arc::new(MemoryRecallFragment {
            query: "rust".into(),
            mock_hits: vec![("page-1".into(), "rust traits".into())],
        })
    }

    fn sample_file() -> Arc<dyn ContextFragment> {
        Arc::new(WorkspaceFileFragment {
            workspace_rel_path: "/tmp/uclaw-test-file-does-not-exist.txt".into(),
            max_bytes: Some(512),
        })
    }

    // ── search ──────────────────────────────────────────────────────

    #[test]
    fn search_returns_empty_when_no_match() {
        let mut t = ContextToolSet::new();
        t.add(sample_conversation());
        let hits = t.search("nonexistent-topic");
        assert!(hits.is_empty());
    }

    #[test]
    fn search_finds_by_topic() {
        let mut t = ContextToolSet::new();
        t.add(sample_conversation());
        t.add(sample_memory());
        // "conversation" matches only the conversation fragment
        let hits = t.search("conversation");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, ContextSource::Conversation);
        // "memory" matches only the memory fragment
        let hits = t.search("memory");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, ContextSource::Memory);
        // "history" matches only the conversation fragment (also tagged)
        let hits = t.search("history");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn search_is_case_sensitive() {
        let mut t = ContextToolSet::new();
        t.add(sample_conversation());
        assert_eq!(t.search("conversation").len(), 1);
        assert_eq!(t.search("Conversation").len(), 0);
        assert_eq!(t.search("CONVERSATION").len(), 0);
    }

    #[test]
    fn search_includes_pinned_fragments() {
        let mut t = ContextToolSet::new();
        let frag = sample_conversation();
        let ref_ = frag.ref_();
        t.add(frag);
        t.pin(&ref_).unwrap();
        // Now the conversation is in `pinned`, not `available`.
        assert_eq!(t.available_len(), 0);
        assert_eq!(t.pinned_len(), 1);
        // search still finds it.
        let hits = t.search("conversation");
        assert_eq!(hits.len(), 1);
    }

    // ── read ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn read_materializes_known_fragment() {
        let mut t = ContextToolSet::new();
        let frag = sample_conversation();
        let r = frag.ref_();
        t.add(frag);
        let art = t.read(&r).await.unwrap();
        assert_eq!(art.content, "hi\nworld");
        assert_eq!(art.r#ref, r);
    }

    #[tokio::test]
    async fn read_returns_not_found_for_unknown_ref() {
        let t = ContextToolSet::new();
        let unknown =
            ContextRef::new(ContextSource::Codebase, "file/missing.rs");
        let err = t.read(&unknown).await.unwrap_err();
        assert!(matches!(err, FragmentError::NotFound(_)));
    }

    #[tokio::test]
    async fn read_finds_pinned_fragment_after_pinning() {
        let mut t = ContextToolSet::new();
        let frag = sample_memory();
        let r = frag.ref_();
        t.add(frag);
        t.pin(&r).unwrap();
        // available is empty after pin; read must still find it.
        let art = t.read(&r).await.unwrap();
        assert_eq!(art.r#ref.source, ContextSource::Memory);
    }

    // ── pin / release ───────────────────────────────────────────────

    #[test]
    fn pin_moves_fragment_to_pinned() {
        let mut t = ContextToolSet::new();
        let frag = sample_conversation();
        let r = frag.ref_();
        t.add(frag);
        assert_eq!(t.available_len(), 1);
        assert_eq!(t.pinned_len(), 0);

        t.pin(&r).unwrap();
        assert_eq!(t.available_len(), 0);
        assert_eq!(t.pinned_len(), 1);
    }

    #[test]
    fn pin_is_idempotent() {
        let mut t = ContextToolSet::new();
        let frag = sample_conversation();
        let r = frag.ref_();
        t.add(frag);
        t.pin(&r).unwrap();
        // Second pin call is a no-op (Ok, no error)
        assert!(t.pin(&r).is_ok());
        assert_eq!(t.pinned_len(), 1, "duplicate pin should not duplicate");
    }

    #[test]
    fn pin_returns_not_found_when_target_not_in_available() {
        let mut t = ContextToolSet::new();
        let unknown = ContextRef::new(ContextSource::Browser, "page/zz");
        let err = t.pin(&unknown).unwrap_err();
        assert!(matches!(err, FragmentError::NotFound(_)));
    }

    #[test]
    fn release_moves_fragment_back_to_available() {
        let mut t = ContextToolSet::new();
        let frag = sample_conversation();
        let r = frag.ref_();
        t.add(frag);
        t.pin(&r).unwrap();
        assert_eq!(t.pinned_len(), 1);

        t.release(&r).unwrap();
        assert_eq!(t.available_len(), 1);
        assert_eq!(t.pinned_len(), 0);
    }

    #[test]
    fn release_returns_not_found_when_not_pinned() {
        let mut t = ContextToolSet::new();
        let frag = sample_conversation();
        let r = frag.ref_();
        t.add(frag);
        // Not pinned yet
        let err = t.release(&r).unwrap_err();
        assert!(matches!(err, FragmentError::NotFound(_)));
    }

    // ── cite / fold / compare stubs ────────────────────────────────

    #[tokio::test]
    async fn cite_returns_citations_from_artifact() {
        let mut t = ContextToolSet::new();
        let frag = sample_memory();
        let r = frag.ref_();
        t.add(frag);
        let art = t.read(&r).await.unwrap();
        let citations = t.cite(&art);
        assert_eq!(citations.len(), 1);
        assert!(citations[0].evidence_ref.starts_with("memory:"));
    }

    #[tokio::test]
    async fn fold_returns_unimplemented_error() {
        let t = ContextToolSet::new();
        let r = ContextRef::new(ContextSource::Codebase, "file/x.rs");
        let err = t.fold(&[r]).await.unwrap_err();
        match err {
            FragmentError::Storage(msg) => assert!(msg.contains("M2-G")),
            other => panic!("expected Storage stub, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn compare_returns_unimplemented_error() {
        let t = ContextToolSet::new();
        let a = ContextRef::new(ContextSource::Codebase, "file/a.rs");
        let b = ContextRef::new(ContextSource::Codebase, "file/b.rs");
        let err = t.compare(&a, &b).await.unwrap_err();
        assert!(matches!(err, FragmentError::Storage(_)));
    }

    // ── add_all + len helpers ───────────────────────────────────────

    #[test]
    fn add_all_bulk_loads_fragments() {
        let mut t = ContextToolSet::new();
        t.add_all([sample_conversation(), sample_memory(), sample_file()]);
        assert_eq!(t.available_len(), 3);
        assert_eq!(t.pinned_len(), 0);
    }
}
