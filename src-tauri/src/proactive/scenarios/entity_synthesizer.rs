//! EntitySynthesizer scenario — Memory OS Foundation Phase 6.2.
//!
//! Re-compiles an EntityPage's `compiled_truth` by feeding the LLM the
//! latest timeline + the existing compiled_truth and asking for a
//! refreshed, slightly longer summary plus an updated alias list.
//!
//! ## Design notes
//!
//! - Trait-object behind `Arc<dyn EntitySynthesizer>` so AppState can
//!   pick `RealEntitySynthesizer` (LLM-backed) when a provider is
//!   configured and `StubEntitySynthesizer` (deterministic) otherwise —
//!   same pattern as Phase 3 WikiSynthesizer and Phase 5 LintAnalyzer.
//! - LLM output is structured JSON so prompt-drift can't corrupt the
//!   write path. Parse failures fall back to keeping the existing
//!   compiled_truth and surfacing an error to the caller.
//! - New version is persisted via `MemoryGraphStore::create_version`,
//!   inheriting the Phase 2 auto-link post-hook (any new
//!   `[[entity:slug]]` references in the regenerated text automatically
//!   produce edges).
//! - The old active version is marked `deprecated` in the same
//!   transaction so `get_active_version` consistently returns the
//!   refreshed row.
//! - Metadata fields touched: `compiled_truth` (via memory_versions.content,
//!   not metadata), `aliases`, `last_synthesized_at`, `synthesis_source_count`.
//!   The `enrichment_tier` is NOT touched here — tier transitions are
//!   the tier_escalator scenario's responsibility (Phase 6.1).
//!
//! ## Cost
//!
//! Each call writes `cost_records.model = 'memory_entity_synth:<actual>'`.
//! No daily cap is enforced INSIDE this module; the upstream caller
//! (Phase 6.3 IPC + future tier-up auto-trigger) is responsible for
//! rate-limiting.

use async_trait::async_trait;
use rusqlite::params;
use serde::Serialize;
use std::sync::Arc;

use crate::memory_graph::entity_page::EntityPageMetadata;
use crate::memory_graph::memory_os_llm::MemoryOsLlm;
use crate::memory_graph::store::MemoryGraphStore;

// ─── Trait + DTOs ──────────────────────────────────────────────────────

/// Pluggable synthesizer that turns one EntityPage's history into a
/// refreshed compiled_truth + aliases bundle. Stub + Real implementations
/// live in this module; tests use the Stub directly to keep the suite
/// LLM-credential-free.
#[async_trait]
pub trait EntitySynthesizer: Send + Sync {
    /// Compile a refreshed page summary from the inputs.
    async fn synthesize(
        &self,
        input: EntitySynthInput<'_>,
    ) -> Result<EntitySynthOutput, EntitySynthError>;

    /// Telemetry/UI badge descriptor — `"stub:no-llm"` / `"real:memory_os_llm"`.
    fn descriptor(&self) -> &'static str;
}

#[derive(Debug, Clone)]
pub struct EntitySynthInput<'a> {
    pub node_id: &'a str,
    pub title: &'a str,
    pub subkind: Option<&'a str>,
    pub existing_compiled_truth: &'a str,
    pub existing_aliases: &'a [String],
    pub timeline: &'a [String],
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EntitySynthOutput {
    pub new_compiled_truth: String,
    pub new_aliases: Vec<String>,
    pub token_cost: u32,
    pub llm_model: Option<String>,
    pub source_count: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum EntitySynthError {
    #[error("synthesizer not configured (StubEntitySynthesizer would have produced output)")]
    Disabled,
    #[error("LLM returned a malformed response: {0}")]
    BadResponse(String),
    #[error("LLM call failed: {0}")]
    Llm(String),
    #[error("storage failure: {0}")]
    Storage(String),
    #[error("entity page not found: {0}")]
    NotFound(String),
}

// ─── Stub implementation ───────────────────────────────────────────────

/// Deterministic synthesizer that produces a clearly-labelled "stub"
/// output. Used as the default in AppState so the Phase 6.3 manual
/// synth button works end-to-end without LLM credentials.
pub struct StubEntitySynthesizer;

#[async_trait]
impl EntitySynthesizer for StubEntitySynthesizer {
    async fn synthesize(
        &self,
        input: EntitySynthInput<'_>,
    ) -> Result<EntitySynthOutput, EntitySynthError> {
        let mut md = String::new();
        md.push_str(&format!("[stub synthesis] {}\n\n", input.title));
        md.push_str(input.existing_compiled_truth.trim());
        md.push_str("\n\n_Recent timeline:_\n");
        if input.timeline.is_empty() {
            md.push_str("- (no timeline entries yet)\n");
        } else {
            for line in input.timeline.iter().take(5) {
                md.push_str(&format!("- {}\n", line));
            }
        }
        Ok(EntitySynthOutput {
            new_compiled_truth: md,
            new_aliases: input.existing_aliases.to_vec(),
            token_cost: 0,
            llm_model: None,
            source_count: input.timeline.len() as u32,
        })
    }

    fn descriptor(&self) -> &'static str {
        "stub:no-llm"
    }
}

// ─── Real LLM implementation ───────────────────────────────────────────

pub struct RealEntitySynthesizer {
    llm: Arc<dyn MemoryOsLlm>,
}

impl RealEntitySynthesizer {
    pub fn new(llm: Arc<dyn MemoryOsLlm>) -> Self {
        Self { llm }
    }

    pub(crate) fn system_prompt() -> &'static str {
        "You are the entity-page compiler for a personal AI knowledge wiki. \
         Given a single EntityPage's title, subkind, current compiled_truth, \
         aliases, and timeline, produce a refreshed compiled_truth (2-6 \
         sentences) plus a deduplicated alias list.\n\n\
         Output ONLY a single JSON object, no fences, no prose. Schema:\n\
         {\"compiled_truth\":\"<2-6 sentence summary>\",\"aliases\":[\"alias_a\",\"alias_b\"]}\n\n\
         Rules:\n\
         - Preserve facts from the existing compiled_truth UNLESS the timeline \
           contradicts them — in that case, prefer the timeline.\n\
         - Aliases must be canonical short names the user might also call this \
           entity. Drop duplicates and punctuation-only variants.\n\
         - Do not invent facts not in the input.\n\
         - The compiled_truth must read as third-person factual prose, not a \
           list."
    }

    pub(crate) fn build_user_prompt(input: &EntitySynthInput<'_>) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "title: {}\nsubkind: {}\nnode_id: {}\n\n",
            input.title,
            input.subkind.unwrap_or("entity"),
            input.node_id,
        ));
        out.push_str(&format!(
            "existing_compiled_truth:\n{}\n\n",
            input.existing_compiled_truth.trim()
        ));
        out.push_str(&format!(
            "existing_aliases: {}\n\n",
            if input.existing_aliases.is_empty() {
                "(none)".to_string()
            } else {
                input.existing_aliases.join(", ")
            }
        ));
        out.push_str("timeline (most recent first):\n");
        if input.timeline.is_empty() {
            out.push_str("  (empty)\n");
        } else {
            for entry in input.timeline.iter().take(50) {
                out.push_str(&format!("  - {}\n", entry));
            }
        }
        out.push_str("\nReply with the JSON object now.\n");
        out
    }
}

#[async_trait]
impl EntitySynthesizer for RealEntitySynthesizer {
    async fn synthesize(
        &self,
        input: EntitySynthInput<'_>,
    ) -> Result<EntitySynthOutput, EntitySynthError> {
        let source_count = input.timeline.len() as u32;
        let user_prompt = Self::build_user_prompt(&input);
        let out = self
            .llm
            .complete_text(
                "memory_entity_synth",
                Self::system_prompt(),
                &user_prompt,
                1500,
            )
            .await
            .map_err(|e| EntitySynthError::Llm(e.to_string()))?;

        let parsed = parse_synth_response(&out.text)
            .map_err(EntitySynthError::BadResponse)?;
        Ok(EntitySynthOutput {
            new_compiled_truth: parsed.compiled_truth,
            new_aliases: parsed.aliases,
            token_cost: out.input_tokens.saturating_add(out.output_tokens),
            llm_model: Some(out.model),
            source_count,
        })
    }

    fn descriptor(&self) -> &'static str {
        "real:memory_os_llm"
    }
}

// ─── Response parsing ──────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub(crate) struct SynthResponse {
    pub compiled_truth: String,
    pub aliases: Vec<String>,
}

/// Extract the first JSON object from `text`, deserialize as the synth
/// schema. Surfaces a structured error on every failure mode so the
/// caller can decide whether to log + retry or surface to the user.
pub(crate) fn parse_synth_response(text: &str) -> Result<SynthResponse, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty response".into());
    }
    let start = trimmed.find('{').ok_or_else(|| "no JSON object found".to_string())?;
    let end = trimmed.rfind('}').ok_or_else(|| "no closing brace".to_string())?;
    if end <= start {
        return Err("malformed brace pair".into());
    }
    let v: serde_json::Value = serde_json::from_str(&trimmed[start..=end])
        .map_err(|e| format!("invalid JSON: {}", e))?;
    let compiled_truth = v
        .get("compiled_truth")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "missing compiled_truth".to_string())?
        .trim()
        .to_string();
    if compiled_truth.is_empty() {
        return Err("compiled_truth is empty".into());
    }
    let aliases = v
        .get("aliases")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(SynthResponse {
        compiled_truth,
        aliases,
    })
}

// ─── Persist helper ────────────────────────────────────────────────────

/// Persist a fresh synth result to memory_versions + EntityPageMetadata.
///
/// Steps (all under one conn lock so the new version is observable
/// immediately):
/// 1. Resolve current active version_id to use as `supersedes_version_id`.
/// 2. UPDATE memory_versions SET status='deprecated' WHERE current active.
/// 3. INSERT a new active row via `create_version` (auto-link runs).
/// 4. UPDATE memory_nodes.metadata_json with new aliases +
///    last_synthesized_at + synthesis_source_count.
///
/// Returns the new version's id.
pub fn persist_synthesis(
    store: &MemoryGraphStore,
    node_id: &str,
    new_content: &str,
    new_aliases: &[String],
    source_count: u32,
) -> Result<String, EntitySynthError> {
    // Resolve current active version BEFORE we take the write lock —
    // create_version takes its own lock internally.
    let prev_active = store
        .get_active_version(node_id)
        .map_err(|e| EntitySynthError::Storage(e.to_string()))?;
    let prev_version_id = prev_active.as_ref().map(|v| v.id.clone());

    // Mark the prior version deprecated (no-op if there isn't one).
    if let Some(prev_id) = prev_version_id.as_ref() {
        let conn = store
            .conn
            .lock()
            .map_err(|e| EntitySynthError::Storage(format!("DB lock: {}", e)))?;
        conn.execute(
            "UPDATE memory_versions SET status = 'deprecated' WHERE id = ?1",
            params![prev_id],
        )
        .map_err(|e| EntitySynthError::Storage(e.to_string()))?;
        drop(conn);
    }

    // Create the new active version. `create_version` runs the Phase 2
    // auto-link hook so any new `[[entity:...]]` references in the
    // regenerated text get auto-edges for free.
    let new_version_id = uuid::Uuid::new_v4().to_string();
    let now_iso = chrono::Utc::now().to_rfc3339();
    let new_version = crate::memory_graph::models::MemoryVersion {
        id: new_version_id.clone(),
        node_id: node_id.to_string(),
        supersedes_version_id: prev_version_id,
        status: crate::memory_graph::models::MemoryVersionStatus::Active,
        content: new_content.to_string(),
        metadata: None,
        embedding_json: None,
        created_at: now_iso.clone(),
    };
    store
        .create_version(&new_version)
        .map_err(|e| EntitySynthError::Storage(e.to_string()))?;

    // Update node metadata: aliases + last_synthesized_at + source_count.
    update_metadata_after_synth(store, node_id, new_aliases, source_count, &now_iso)?;

    Ok(new_version_id)
}

fn update_metadata_after_synth(
    store: &MemoryGraphStore,
    node_id: &str,
    new_aliases: &[String],
    source_count: u32,
    now_iso: &str,
) -> Result<(), EntitySynthError> {
    let conn = store
        .conn
        .lock()
        .map_err(|e| EntitySynthError::Storage(format!("DB lock: {}", e)))?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM memory_nodes WHERE id = ?1",
            params![node_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    let value: serde_json::Value = raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
    let mut meta = EntityPageMetadata::from_value(&value);
    meta.aliases = dedup_aliases(new_aliases);
    meta.last_synthesized_at = Some(now_iso.to_string());
    meta.synthesis_source_count = Some(source_count);
    let new_json = serde_json::to_string(&meta.to_value())
        .map_err(|e| EntitySynthError::Storage(format!("serialise meta: {}", e)))?;
    conn.execute(
        "UPDATE memory_nodes SET metadata_json = ?1, updated_at = ?2 WHERE id = ?3",
        params![new_json, now_iso, node_id],
    )
    .map_err(|e| EntitySynthError::Storage(e.to_string()))?;
    Ok(())
}

fn dedup_aliases(aliases: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(aliases.len());
    for a in aliases {
        let canonical = a.trim().to_string();
        if !canonical.is_empty()
            && !out.iter().any(|existing| existing.eq_ignore_ascii_case(&canonical))
        {
            out.push(canonical);
        }
    }
    out
}

// ─── End-to-end facade ─────────────────────────────────────────────────

/// One-shot helper that reads the current page state, asks the
/// synthesizer for a refresh, persists the result, and returns the new
/// version id + token cost. Used by the Phase 6.3 IPC handler.
pub async fn synthesize_entity_now(
    store: Arc<MemoryGraphStore>,
    synthesizer: Arc<dyn EntitySynthesizer>,
    node_id: &str,
) -> Result<SynthesisOutcome, EntitySynthError> {
    // 1. Snapshot current state inside a short conn lock.
    let (title, subkind, existing_aliases, existing_compiled_truth, timeline) =
        load_synth_input_snapshot(&store, node_id)?;

    // 2. Run synthesizer (may call LLM — definitely async).
    let input = EntitySynthInput {
        node_id,
        title: &title,
        subkind: subkind.as_deref(),
        existing_compiled_truth: &existing_compiled_truth,
        existing_aliases: &existing_aliases,
        timeline: &timeline,
    };
    let out = synthesizer.synthesize(input).await?;

    // 3. Persist.
    let new_version_id = persist_synthesis(
        &store,
        node_id,
        &out.new_compiled_truth,
        &out.new_aliases,
        out.source_count,
    )?;

    Ok(SynthesisOutcome {
        new_version_id,
        token_cost: out.token_cost,
        llm_model: out.llm_model,
        synthesizer_descriptor: synthesizer.descriptor().to_string(),
        new_compiled_truth: out.new_compiled_truth,
        new_aliases: out.new_aliases,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SynthesisOutcome {
    pub new_version_id: String,
    pub token_cost: u32,
    pub llm_model: Option<String>,
    pub synthesizer_descriptor: String,
    pub new_compiled_truth: String,
    pub new_aliases: Vec<String>,
}

/// (title, subkind, aliases, current compiled_truth content, timeline_lines)
fn load_synth_input_snapshot(
    store: &MemoryGraphStore,
    node_id: &str,
) -> Result<
    (String, Option<String>, Vec<String>, String, Vec<String>),
    EntitySynthError,
> {
    let conn = store
        .conn
        .lock()
        .map_err(|e| EntitySynthError::Storage(format!("DB lock: {}", e)))?;
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT title, metadata_json FROM memory_nodes WHERE id = ?1 AND kind = 'entity_page'",
            params![node_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let (title, metadata_raw) = match row {
        Some(r) => r,
        None => return Err(EntitySynthError::NotFound(node_id.to_string())),
    };
    let value: serde_json::Value = metadata_raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
    let meta = EntityPageMetadata::from_value(&value);
    let subkind = meta.subkind.clone();
    let aliases = meta.aliases.clone();
    let timeline_lines: Vec<String> = meta
        .timeline
        .iter()
        .rev() // most recent first
        .map(|e| format!("{} — {}", e.date, e.text))
        .collect();

    let current_compiled: String = conn
        .query_row(
            "SELECT content FROM memory_versions \
             WHERE node_id = ?1 AND status = 'active' \
             ORDER BY created_at DESC LIMIT 1",
            params![node_id],
            |r| r.get(0),
        )
        .unwrap_or_default();

    Ok((title, subkind, aliases, current_compiled, timeline_lines))
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::entity_page::{EntityPageMetadata, TimelineEntry};
    use crate::memory_graph::memory_os_llm::MockMemoryOsLlm;
    use crate::memory_graph::store::MemoryGraphStore;
    use rusqlite::Connection;
    use std::sync::Mutex;

    fn fresh_store() -> Arc<MemoryGraphStore> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        conn.execute_batch(crate::db::migrations::V13_COST_RECORDS).unwrap();
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        Arc::new(MemoryGraphStore::new(Arc::new(Mutex::new(conn))))
    }

    fn insert_entity_page(
        store: &MemoryGraphStore,
        id: &str,
        title: &str,
        meta: EntityPageMetadata,
        compiled_truth: &str,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let meta_json = serde_json::to_string(&meta.to_value()).unwrap();
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title, metadata_json, created_at, updated_at) \
             VALUES (?1, 'default', 'entity_page', ?2, ?3, ?4, ?4)",
            params![id, title, meta_json, now],
        )
        .unwrap();
        let v_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO memory_versions \
             (id, node_id, supersedes_version_id, status, content, metadata_json, embedding_json, created_at) \
             VALUES (?1, ?2, NULL, 'active', ?3, NULL, NULL, ?4)",
            params![v_id, id, compiled_truth, now],
        )
        .unwrap();
    }

    // ─── Stub synthesizer ──────────────────────────────────────────

    #[tokio::test]
    async fn stub_synth_produces_marked_output() {
        let stub = StubEntitySynthesizer;
        let timeline = vec!["2026-05-15 — joined Acme".to_string()];
        let input = EntitySynthInput {
            node_id: "n1",
            title: "Alice",
            subkind: Some("person"),
            existing_compiled_truth: "An engineer.",
            existing_aliases: &["Allie".to_string()],
            timeline: &timeline,
        };
        let out = stub.synthesize(input).await.unwrap();
        assert!(out.new_compiled_truth.contains("[stub synthesis]"));
        assert!(out.new_compiled_truth.contains("Alice"));
        assert!(out.new_compiled_truth.contains("joined Acme"));
        assert_eq!(out.token_cost, 0);
        assert_eq!(out.new_aliases, vec!["Allie"]);
        assert_eq!(out.source_count, 1);
    }

    #[test]
    fn stub_descriptor() {
        assert_eq!(StubEntitySynthesizer.descriptor(), "stub:no-llm");
    }

    // ─── Real synthesizer prompt structure ────────────────────────

    #[test]
    fn real_synth_system_prompt_pins_schema() {
        let s = RealEntitySynthesizer::system_prompt();
        assert!(s.contains("compiled_truth"));
        assert!(s.contains("aliases"));
        assert!(s.contains("third-person factual prose"));
        assert!(s.contains("Do not invent facts"));
    }

    #[test]
    fn real_synth_user_prompt_carries_inputs() {
        let timeline = vec![
            "2026-05-15 — joined Acme".to_string(),
            "2026-04-01 — graduated MIT".to_string(),
        ];
        let aliases = vec!["Allie".to_string()];
        let input = EntitySynthInput {
            node_id: "n1",
            title: "Alice",
            subkind: Some("person"),
            existing_compiled_truth: "Existing summary.",
            existing_aliases: &aliases,
            timeline: &timeline,
        };
        let p = RealEntitySynthesizer::build_user_prompt(&input);
        assert!(p.contains("title: Alice"));
        assert!(p.contains("subkind: person"));
        assert!(p.contains("Existing summary."));
        assert!(p.contains("Allie"));
        assert!(p.contains("joined Acme"));
        assert!(p.contains("graduated MIT"));
    }

    // ─── parse_synth_response ──────────────────────────────────────

    #[test]
    fn parse_synth_handles_clean_json() {
        let r = parse_synth_response(
            r#"{"compiled_truth":"Senior engineer at Acme.","aliases":["Allie","A. Smith"]}"#,
        )
        .unwrap();
        assert_eq!(r.compiled_truth, "Senior engineer at Acme.");
        assert_eq!(r.aliases, vec!["Allie", "A. Smith"]);
    }

    #[test]
    fn parse_synth_handles_missing_aliases_array() {
        let r = parse_synth_response(r#"{"compiled_truth":"summary"}"#).unwrap();
        assert!(r.aliases.is_empty(), "missing aliases → empty vec, no error");
    }

    #[test]
    fn parse_synth_rejects_empty_compiled_truth() {
        let err = parse_synth_response(r#"{"compiled_truth":""}"#).unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn parse_synth_rejects_malformed_json() {
        let err = parse_synth_response("not even close to JSON").unwrap_err();
        assert!(err.contains("no JSON") || err.contains("invalid"));
    }

    #[test]
    fn parse_synth_tolerates_surrounding_prose() {
        let r = parse_synth_response(
            "Sure! {\"compiled_truth\":\"x\",\"aliases\":[\"a\"]} — happy to help.",
        )
        .unwrap();
        assert_eq!(r.compiled_truth, "x");
    }

    // ─── dedup_aliases ─────────────────────────────────────────────

    #[test]
    fn dedup_aliases_is_case_insensitive() {
        let out = dedup_aliases(&[
            "Alice".to_string(),
            "alice".to_string(),
            "Allie".to_string(),
            "  ".to_string(),
            "Allie".to_string(),
        ]);
        assert_eq!(out, vec!["Alice", "Allie"]);
    }

    // ─── End-to-end synthesize_entity_now ──────────────────────────

    #[tokio::test]
    async fn synthesize_entity_now_writes_new_version_and_metadata() {
        let store = fresh_store();
        let mut meta = EntityPageMetadata::default();
        meta.aliases = vec!["Allie".to_string()];
        meta.subkind = Some("person".into());
        meta.timeline = vec![
            TimelineEntry {
                date: "2026-05-15".into(),
                text: "joined Acme".into(),
                source_node_id: None,
                source_session_id: None,
            },
            TimelineEntry {
                date: "2026-05-01".into(),
                text: "graduated MIT".into(),
                source_node_id: None,
                source_session_id: None,
            },
        ];
        insert_entity_page(&store, "n1", "Alice", meta, "Old summary.");

        // Mock LLM returns a clean response.
        let mock = Arc::new(MockMemoryOsLlm {
            canned_text:
                r#"{"compiled_truth":"Alice is an engineer who joined Acme after graduating MIT.","aliases":["Allie","Alice S."]}"#
                    .into(),
            canned_input_tokens: 400,
            canned_output_tokens: 80,
            canned_model: "mock:claude-sonnet".into(),
        });
        let synth: Arc<dyn EntitySynthesizer> = Arc::new(RealEntitySynthesizer::new(mock));
        let outcome = synthesize_entity_now(store.clone(), synth, "n1")
            .await
            .unwrap();

        assert!(outcome.new_compiled_truth.contains("Acme"));
        assert_eq!(outcome.token_cost, 480);
        assert_eq!(outcome.llm_model.as_deref(), Some("mock:claude-sonnet"));
        assert_eq!(outcome.new_aliases, vec!["Allie", "Alice S."]);

        // Verify new active version exists with new content
        let active = store.get_active_version("n1").unwrap().unwrap();
        assert!(active.content.contains("Acme"));
        // Verify exactly one active version
        let conn = store.conn.lock().unwrap();
        let active_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_versions WHERE node_id = 'n1' AND status = 'active'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active_count, 1, "must have exactly one active version");
        let deprecated_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_versions WHERE node_id = 'n1' AND status = 'deprecated'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deprecated_count, 1, "old version must be deprecated");

        // Verify metadata has aliases + last_synthesized_at
        let meta_raw: String = conn
            .query_row(
                "SELECT metadata_json FROM memory_nodes WHERE id = 'n1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&meta_raw).unwrap();
        let m = EntityPageMetadata::from_value(&v);
        assert_eq!(m.aliases, vec!["Allie", "Alice S."]);
        assert!(m.last_synthesized_at.is_some());
        assert_eq!(m.synthesis_source_count, Some(2));
    }

    #[tokio::test]
    async fn synthesize_entity_now_returns_not_found_for_missing_node() {
        let store = fresh_store();
        let synth: Arc<dyn EntitySynthesizer> = Arc::new(StubEntitySynthesizer);
        let err = synthesize_entity_now(store, synth, "nonexistent")
            .await
            .unwrap_err();
        match err {
            EntitySynthError::NotFound(id) => assert_eq!(id, "nonexistent"),
            other => panic!("expected NotFound, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn synthesize_entity_now_with_stub_keeps_old_content_essentially() {
        let store = fresh_store();
        let mut meta = EntityPageMetadata::default();
        meta.timeline = vec![TimelineEntry {
            date: "2026-05-01".into(),
            text: "Existing thing.".into(),
            source_node_id: None,
            source_session_id: None,
        }];
        insert_entity_page(&store, "n2", "Stub Page", meta, "Original content.");

        let synth: Arc<dyn EntitySynthesizer> = Arc::new(StubEntitySynthesizer);
        let outcome = synthesize_entity_now(store.clone(), synth, "n2").await.unwrap();
        assert!(outcome.new_compiled_truth.contains("[stub synthesis]"));
        assert!(outcome.new_compiled_truth.contains("Original content."));
        assert_eq!(outcome.token_cost, 0);
        assert_eq!(outcome.synthesizer_descriptor, "stub:no-llm");
        assert!(outcome.llm_model.is_none());
    }

    #[tokio::test]
    async fn synthesize_entity_now_propagates_bad_llm_response() {
        let store = fresh_store();
        insert_entity_page(
            &store,
            "n3",
            "Bad",
            EntityPageMetadata::default(),
            "Old.",
        );
        // Mock LLM returns text that fails parse_synth_response.
        let mock = Arc::new(MockMemoryOsLlm {
            canned_text: "I refuse to JSON".into(),
            canned_input_tokens: 50,
            canned_output_tokens: 20,
            canned_model: "mock".into(),
        });
        let synth: Arc<dyn EntitySynthesizer> = Arc::new(RealEntitySynthesizer::new(mock));
        let err = synthesize_entity_now(store, synth, "n3").await.unwrap_err();
        match err {
            EntitySynthError::BadResponse(_) => {}
            other => panic!("expected BadResponse, got {:?}", other),
        }
    }
}
