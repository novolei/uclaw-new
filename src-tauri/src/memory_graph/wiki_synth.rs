//! AI Wiki synthesis — generates the `index.md` (SQL-only) and
//! `overview.md` (LLM-driven) artifacts that back the Phase 3 WikiView.
//!
//! Design ref: `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md`
//! §4.5 ("AI Wiki 构建策略") and the Phase 3 plan task list.
//!
//! ## Two artifacts, two cost tiers
//!
//! - **index.md** — produced by `regenerate_index`. Lists every
//!   EntityPage grouped by `metadata.subkind`. Pure SQL, zero LLM. Cheap
//!   to run every few ticks. Stored in `wiki_artifacts(kind="index")`.
//!
//! - **overview.md** — produced by `regenerate_overview`. Synthesizes
//!   a "what we currently know" narrative from the latest N pages and
//!   the longest M timelines. Calls an LLM via the [`WikiSynthesizer`]
//!   trait, so the test suite can pass a deterministic mock. Stored in
//!   `wiki_artifacts(kind="overview")`.
//!
//! Both kinds are upserted: the latest row per `(space_id, kind)`
//! supersedes its predecessor. Older rows stay in the table for audit /
//! diff but `wiki_get_*` IPC commands return only the freshest.
//!
//! ## LLM integration boundary — intentionally narrow
//!
//! Phase 3 does NOT wire up real LLM provider plumbing — that touches
//! AppState / ProviderService / cost tracking and is a chunk of work in
//! its own right. Instead the LLM call goes through [`WikiSynthesizer`],
//! which Phase 3 ships with:
//!
//! - [`StubSynthesizer`] — deterministic, marks overviews as "stub"
//!   so the WikiView can show a clear "needs real LLM" badge.
//!   Wired as the default in AppState so the feature is usable end-to-end.
//! - [`MockSynthesizer`] (in tests) — returns canned strings for the
//!   wiki_overview test cases.
//!
//! A follow-up PR (or Cognitive Phase 10's `wiki_compile`) replaces the
//! stub with the real Anthropic / OpenAI client without disturbing the
//! scenario / IPC / frontend code paths.

use async_trait::async_trait;
use rusqlite::params;
use std::collections::BTreeMap;
use std::sync::Arc;

use super::models::MemoryNodeKind;

// ─── LLM seam ──────────────────────────────────────────────────────────

/// Pluggable narrative synthesizer for the wiki overview. Implementations
/// receive the same input the scenario would feed an LLM (system prompt
/// + structured page snapshot) and return either the synthesized
/// markdown or an error explaining why the synthesis was skipped.
///
/// All implementors must be `Send + Sync` because the trait object lives
/// inside `AppState` which is shared across the Tokio runtime.
#[async_trait]
pub trait WikiSynthesizer: Send + Sync {
    /// Synthesize the overview body. Returns the markdown that will
    /// land in `wiki_artifacts.content`.
    async fn synthesize_overview(
        &self,
        input: WikiSynthesisInput<'_>,
    ) -> Result<WikiSynthesisOutput, WikiSynthesisError>;

    /// Short descriptor for telemetry / UI badges. Real LLM implementors
    /// return `"anthropic:claude-haiku-..."`; the stub returns
    /// `"stub:no-llm"` so the WikiView can show a "needs real LLM"
    /// indicator.
    fn descriptor(&self) -> &'static str;
}

/// Read-only structured snapshot of the wiki state used as LLM input.
/// All slices borrow from the caller — the synthesizer mustn't outlive
/// the regenerate call.
#[derive(Debug)]
pub struct WikiSynthesisInput<'a> {
    pub space_id: &'a str,
    pub recent_entity_pages: &'a [EntityPageSnapshot],
    pub total_entity_pages: usize,
    pub total_edges: usize,
    pub generated_at_iso: &'a str,
}

/// Minimal projection of an EntityPage for synthesis. Doesn't carry the
/// full `metadata_json` — the synthesizer never needs that detail and
/// keeping the shape narrow makes mocking easy.
#[derive(Debug, Clone)]
pub struct EntityPageSnapshot {
    pub node_id: String,
    pub title: String,
    pub slug: Option<String>,
    pub subkind: Option<String>,
    pub compiled_truth_excerpt: String,
    pub updated_at: String,
}

#[derive(Debug)]
pub struct WikiSynthesisOutput {
    pub markdown: String,
    /// Estimated token cost. The stub returns 0; real synthesizers fill
    /// this so the Phase 5 cost dashboard can roll it up.
    pub token_cost: u32,
    pub llm_model: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum WikiSynthesisError {
    #[error("no synthesizer configured")]
    NoSynthesizer,
    #[error("synthesizer disabled by config")]
    Disabled,
    #[error("synthesizer error: {0}")]
    Other(String),
}

// ─── Default stub synthesizer ──────────────────────────────────────────

/// No-op synthesizer used as the default until a real LLM client is
/// wired up. Returns deterministic markdown that mentions the page
/// titles and a clear "stub" header so users / the UI know this isn't
/// a real synthesis.
///
/// Crucially, this makes `regenerate_overview` succeed and produce a
/// valid `wiki_artifacts` row — so the end-to-end UI flow can be tested
/// without LLM credentials. When a real synthesizer lands, swapping it
/// in is one line in AppState bootstrap.
pub struct StubSynthesizer;

#[async_trait]
impl WikiSynthesizer for StubSynthesizer {
    async fn synthesize_overview(
        &self,
        input: WikiSynthesisInput<'_>,
    ) -> Result<WikiSynthesisOutput, WikiSynthesisError> {
        let mut md = String::new();
        md.push_str("# Wiki Overview (stub)\n\n");
        md.push_str(&format!(
            "_Generated at {} — workspace `{}`._\n\n",
            input.generated_at_iso, input.space_id
        ));
        md.push_str(&format!(
            "**Statistics:** {} entity pages, {} edges.\n\n",
            input.total_entity_pages, input.total_edges
        ));
        md.push_str(
            "> This overview is currently a placeholder produced by the \
             stub synthesizer. Once a real LLM provider is wired into \
             `WikiSynthesizer` the overview will become a true narrative \
             synthesis instead of the per-page listing below.\n\n",
        );

        md.push_str("## Recent activity\n\n");
        if input.recent_entity_pages.is_empty() {
            md.push_str("_No entity pages yet — create one with QuickCapture._\n");
        } else {
            for p in input.recent_entity_pages.iter().take(10) {
                let slug = p.slug.as_deref().unwrap_or("");
                let subkind = p.subkind.as_deref().unwrap_or("entity");
                md.push_str(&format!(
                    "- **{}** _({}_{}_)_ — {} _(last updated {})_\n",
                    p.title,
                    subkind,
                    if slug.is_empty() { String::new() } else { format!(", slug `{}`", slug) },
                    truncate_for_excerpt(&p.compiled_truth_excerpt, 140),
                    p.updated_at,
                ));
            }
        }

        Ok(WikiSynthesisOutput {
            markdown: md,
            token_cost: 0,
            llm_model: None,
        })
    }

    fn descriptor(&self) -> &'static str {
        "stub:no-llm"
    }
}

// ─── Real LLM synthesizer (Phase 6b) ───────────────────────────────────

/// Production wiki overview synthesizer — turns the structured snapshot
/// into a narrative paragraph or three by calling out to whichever LLM
/// is configured via [`crate::memory_graph::memory_os_llm::MemoryOsLlm`].
///
/// Cost goes into `cost_records.model = "memory_wiki:<actual_model>"`
/// — *not* matched by Phase 5's `LIKE 'memory_lint%'` cost guard, so
/// wiki regen has no per-day budget cap of its own. The cap is the
/// tick cadence (every ~5 min) + the upper bound on recent snapshots
/// fed into the prompt (capped in `read_recent_snapshots` at 20).
pub struct RealWikiSynthesizer {
    llm: Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>,
}

impl RealWikiSynthesizer {
    pub fn new(llm: Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>) -> Self {
        Self { llm }
    }

    /// System prompt — small, deterministic, narrative-focused. Kept as
    /// a method so test-mode `MockMemoryOsLlm` can assert on it if
    /// needed.
    pub(crate) fn system_prompt() -> &'static str {
        "You are the overview narrator for a personal AI knowledge wiki. \
         Your job is to summarize what the user currently knows about \
         their world — the people, projects, concepts, and themes that \
         show up across their entity pages. \
         \n\n\
         Write in a calm, observational voice. Aim for 2-4 short \
         paragraphs of markdown (≤ 350 words total). Reference at most \
         5-8 entity pages by **bold name**. End with a one-line \
         observation about the wiki's current shape (e.g. \"Mostly \
         people and projects so far; concepts will likely grow as \
         topics deepen.\"). Do not invent facts not in the input."
    }

    pub(crate) fn build_user_prompt(input: &WikiSynthesisInput<'_>) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Wiki space: `{}` (generated {}).\n\
             Total entity pages: {} · Total edges: {}.\n\n",
            input.space_id,
            input.generated_at_iso,
            input.total_entity_pages,
            input.total_edges,
        ));
        out.push_str("## Recent entity pages\n\n");
        if input.recent_entity_pages.is_empty() {
            out.push_str("_(none yet — the wiki is empty)_\n");
        } else {
            for p in input.recent_entity_pages.iter().take(20) {
                let slug = p.slug.as_deref().unwrap_or("");
                let subkind = p.subkind.as_deref().unwrap_or("entity");
                out.push_str(&format!(
                    "- **{}** _(subkind: {}{})_ — {} _(updated {})_\n",
                    p.title,
                    subkind,
                    if slug.is_empty() {
                        String::new()
                    } else {
                        format!(", slug `{}`", slug)
                    },
                    truncate_for_excerpt(&p.compiled_truth_excerpt, 200),
                    p.updated_at,
                ));
            }
        }
        out.push_str(
            "\n---\n\
             Write the overview now. Output ONLY the markdown body — \
             do not wrap in code fences, do not add a top-level header \
             (the WikiView already renders one).\n",
        );
        out
    }
}

#[async_trait]
impl WikiSynthesizer for RealWikiSynthesizer {
    async fn synthesize_overview(
        &self,
        input: WikiSynthesisInput<'_>,
    ) -> Result<WikiSynthesisOutput, WikiSynthesisError> {
        let user_prompt = Self::build_user_prompt(&input);
        let out = self
            .llm
            .complete_text("memory_wiki", Self::system_prompt(), &user_prompt, 1500)
            .await
            .map_err(|e| WikiSynthesisError::Other(e.to_string()))?;
        Ok(WikiSynthesisOutput {
            markdown: out.text,
            token_cost: out.input_tokens.saturating_add(out.output_tokens),
            llm_model: Some(out.model),
        })
    }

    fn descriptor(&self) -> &'static str {
        "real:memory_os_llm"
    }
}

fn truncate_for_excerpt(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return "_(no content yet)_".into();
    }
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= max {
        trimmed.to_string()
    } else {
        let mut out: String = chars.iter().take(max).collect();
        out.push('…');
        out
    }
}

// ─── Regenerate operations ─────────────────────────────────────────────

/// Trigger `kind` recorded in `wiki_artifacts.payload`-style metadata
/// so we can later distinguish ticks vs manual user clicks for cost
/// telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegenerateTrigger {
    /// ProactiveService tick (every N ticks). Cheap path only.
    Tick,
    /// Explicit `memory_wiki_regenerate` IPC call by user / agent.
    Manual,
}

impl RegenerateTrigger {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tick => "tick",
            Self::Manual => "manual",
        }
    }
}

/// Outcome of a single `regenerate_*` call. Returned so the caller can
/// surface success / skip information without re-querying the table.
#[derive(Debug)]
pub struct RegenerateOutcome {
    pub artifact_id: String,
    pub bytes_written: usize,
    pub token_cost: u32,
    pub llm_model: Option<String>,
}

/// Regenerate `wiki_artifacts(kind="index")` for the given space.
///
/// SQL-only — never calls LLM. Groups entity pages by `metadata.subkind`
/// (entity / concept / comparison / question / synthesis / decision / gap
/// / default-when-missing) and writes a sorted markdown listing.
///
/// Locks the conn for the duration so it's safe to call from anywhere
/// (including the proactive tick loop).
pub fn regenerate_index(
    conn: &rusqlite::Connection,
    space_id: &str,
    trigger: RegenerateTrigger,
) -> Result<RegenerateOutcome, crate::error::Error> {
    let now_iso = chrono::Utc::now().to_rfc3339();
    let now_ms = chrono::Utc::now().timestamp_millis();

    // Fetch every EntityPage in the space ordered by subkind then title.
    let mut stmt = conn
        .prepare(
            "SELECT id, title, \
                    COALESCE(json_extract(metadata_json, '$.slug'), '') AS slug, \
                    COALESCE(json_extract(metadata_json, '$.subkind'), '') AS subkind, \
                    updated_at \
             FROM memory_nodes \
             WHERE space_id = ?1 AND kind = 'entity_page' \
             ORDER BY \
               COALESCE(json_extract(metadata_json, '$.subkind'), 'default') ASC, \
               title ASC",
        )
        .map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(params![space_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(crate::error::Error::Database)?;

    // Group by subkind in display order. BTreeMap keeps subkinds sorted
    // deterministically; within each group, rows are already sorted by
    // title thanks to the SQL ORDER BY.
    let mut groups: BTreeMap<String, Vec<(String, String, String, String)>> = BTreeMap::new();
    let mut total: usize = 0;
    let mut node_ids: Vec<String> = Vec::new();
    for row in rows.flatten() {
        let (node_id, title, slug, subkind, updated_at) = row;
        let group_key = if subkind.is_empty() {
            "default".to_string()
        } else {
            subkind
        };
        node_ids.push(node_id.clone());
        groups
            .entry(group_key)
            .or_default()
            .push((node_id, title, slug, updated_at));
        total += 1;
    }
    // Drop stmt so memory_fts queries below (if any) can take the lock
    // — Phase 1 fix-up pattern, prevents E0597.
    drop(stmt);

    // Compose markdown. Display order: entity first, then concept,
    // comparison, question, synthesis, decision, gap, default — but
    // BTreeMap iterates alphabetically. We force the canonical order by
    // walking the known set first then appending the rest.
    let canonical_order = [
        "entity", "concept", "comparison", "question", "synthesis", "decision", "gap", "default",
    ];
    let mut md = String::new();
    md.push_str("# Wiki Index\n\n");
    md.push_str(&format!(
        "_Auto-generated {} — {} entity pages in workspace `{}`._\n\n",
        now_iso, total, space_id
    ));
    if total == 0 {
        md.push_str("_No entity pages yet._\n");
    } else {
        for kind in canonical_order {
            if let Some(rows) = groups.remove(kind) {
                md.push_str(&format!("## {} ({})\n\n", pretty_subkind(kind), rows.len()));
                for (node_id, title, slug, updated_at) in rows {
                    let handle = if slug.is_empty() {
                        format!("`{}`", node_id)
                    } else {
                        format!("`{}` ({})", slug, node_id)
                    };
                    md.push_str(&format!(
                        "- **{}** {} — _updated {}_\n",
                        title, handle, updated_at
                    ));
                }
                md.push('\n');
            }
        }
        // Any subkinds not in the canonical list (e.g. user-added).
        for (kind, rows) in groups {
            md.push_str(&format!("## {} ({})\n\n", pretty_subkind(&kind), rows.len()));
            for (node_id, title, slug, updated_at) in rows {
                let handle = if slug.is_empty() {
                    format!("`{}`", node_id)
                } else {
                    format!("`{}` ({})", slug, node_id)
                };
                md.push_str(&format!(
                    "- **{}** {} — _updated {}_\n",
                    title, handle, updated_at
                ));
            }
            md.push('\n');
        }
    }

    let outcome = upsert_wiki_artifact(
        conn,
        space_id,
        "index",
        &md,
        &node_ids,
        None,
        0,
        trigger,
        now_ms,
    )?;
    Ok(outcome)
}

/// Regenerate `wiki_artifacts(kind="overview")` via the given
/// synthesizer. The caller is responsible for picking a synthesizer
/// (stub vs real LLM).
///
/// Locks the conn while gathering input + writing output. The
/// synthesizer call itself happens OUTSIDE the lock — the input is
/// projected to owned data first, then the lock is dropped, the
/// synthesizer runs (potentially making network calls), and finally a
/// fresh lock is taken to write the artifact.
pub async fn regenerate_overview(
    store_conn: Arc<std::sync::Mutex<rusqlite::Connection>>,
    synthesizer: Arc<dyn WikiSynthesizer>,
    space_id: &str,
    trigger: RegenerateTrigger,
) -> Result<RegenerateOutcome, crate::error::Error> {
    let now_iso = chrono::Utc::now().to_rfc3339();
    let now_ms = chrono::Utc::now().timestamp_millis();

    // ─── Read phase: project to owned data, drop lock ─────────────
    let (recent, total_pages, total_edges) = {
        let conn = store_conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let recent = read_recent_snapshots(&conn, space_id, 20)?;
        let total_pages: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_nodes WHERE space_id = ?1 AND kind = 'entity_page'",
                params![space_id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize;
        let total_edges: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_edges WHERE space_id = ?1",
                params![space_id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize;
        (recent, total_pages, total_edges)
    };

    let node_ids: Vec<String> = recent.iter().map(|p| p.node_id.clone()).collect();

    // ─── Synthesis phase: outside the lock ────────────────────────
    let input = WikiSynthesisInput {
        space_id,
        recent_entity_pages: &recent,
        total_entity_pages: total_pages,
        total_edges,
        generated_at_iso: &now_iso,
    };
    let output = synthesizer
        .synthesize_overview(input)
        .await
        .map_err(|e| crate::error::Error::Internal(format!("synthesizer: {e}")))?;

    // ─── Write phase: fresh lock, upsert artifact ─────────────────
    let conn = store_conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    upsert_wiki_artifact(
        &conn,
        space_id,
        "overview",
        &output.markdown,
        &node_ids,
        output.llm_model.as_deref(),
        output.token_cost,
        trigger,
        now_ms,
    )
}

// ─── Helpers ───────────────────────────────────────────────────────────

fn read_recent_snapshots(
    conn: &rusqlite::Connection,
    space_id: &str,
    limit: usize,
) -> Result<Vec<EntityPageSnapshot>, crate::error::Error> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.title, \
                    COALESCE(json_extract(n.metadata_json, '$.slug'), ''), \
                    COALESCE(json_extract(n.metadata_json, '$.subkind'), ''), \
                    COALESCE(v.content, ''), \
                    n.updated_at \
             FROM memory_nodes n \
             LEFT JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active' \
             WHERE n.space_id = ?1 AND n.kind = 'entity_page' \
             ORDER BY n.updated_at DESC \
             LIMIT ?2",
        )
        .map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(params![space_id, limit as i64], |row| {
            Ok(EntityPageSnapshot {
                node_id: row.get::<_, String>(0)?,
                title: row.get::<_, String>(1)?,
                slug: {
                    let s: String = row.get(2)?;
                    if s.is_empty() { None } else { Some(s) }
                },
                subkind: {
                    let s: String = row.get(3)?;
                    if s.is_empty() { None } else { Some(s) }
                },
                compiled_truth_excerpt: row.get::<_, String>(4)?,
                updated_at: row.get::<_, String>(5)?,
            })
        })
        .map_err(crate::error::Error::Database)?;
    Ok(rows.flatten().collect())
}

#[allow(clippy::too_many_arguments)]
fn upsert_wiki_artifact(
    conn: &rusqlite::Connection,
    space_id: &str,
    kind: &str,
    content: &str,
    source_node_ids: &[String],
    llm_model: Option<&str>,
    token_cost: u32,
    trigger: RegenerateTrigger,
    now_ms: i64,
) -> Result<RegenerateOutcome, crate::error::Error> {
    let artifact_id = uuid::Uuid::new_v4().to_string();
    let source_node_ids_json =
        serde_json::to_string(source_node_ids).unwrap_or_else(|_| "[]".into());

    // Wiki_artifacts has no UNIQUE constraint on (space_id, kind) —
    // older versions stay as audit history. The "latest" row per
    // (space_id, kind) is identified by MAX(generated_at), and
    // memory_wiki_get_* in tauri_commands selects on that.
    //
    // For now we just INSERT a new row. Phase 11 (Cognitive SHA-256
    // incremental compile) will introduce a hash check that skips the
    // INSERT entirely when nothing changed.
    conn.execute(
        "INSERT INTO wiki_artifacts \
         (id, space_id, kind, content, generated_at, source_node_ids, llm_model, token_cost) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            artifact_id,
            space_id,
            kind,
            content,
            now_ms,
            source_node_ids_json,
            llm_model,
            token_cost as i64,
        ],
    )
    .map_err(crate::error::Error::Database)?;

    tracing::debug!(
        space_id, kind, trigger = trigger.as_str(),
        bytes = content.len(),
        token_cost,
        "memory_graph: wiki artifact written"
    );

    Ok(RegenerateOutcome {
        artifact_id,
        bytes_written: content.len(),
        token_cost,
        llm_model: llm_model.map(str::to_string),
    })
}

fn pretty_subkind(s: &str) -> &'static str {
    match s {
        "entity" => "Entity",
        "concept" => "Concept",
        "comparison" => "Comparison",
        "question" => "Question",
        "synthesis" => "Synthesis",
        "decision" => "Decision",
        "gap" => "Gap",
        "default" => "Other",
        _ => "Other",
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fresh_conn() -> Arc<std::sync::Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        Arc::new(std::sync::Mutex::new(conn))
    }

    fn insert_entity_page(
        conn: &Connection,
        id: &str,
        title: &str,
        slug: &str,
        subkind: &str,
        compiled_truth: &str,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let meta = serde_json::json!({"slug": slug, "subkind": subkind});
        conn.execute(
            "INSERT INTO memory_nodes \
             (id, space_id, kind, title, metadata_json, created_at, updated_at) \
             VALUES (?1, 'default', 'entity_page', ?2, ?3, ?4, ?4)",
            params![id, title, meta.to_string(), now],
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

    // ─── regenerate_index ─────────────────────────────────────────

    #[test]
    fn regenerate_index_groups_by_subkind_and_writes_artifact() {
        let store = fresh_conn();
        {
            let c = store.lock().unwrap();
            insert_entity_page(&c, "n1", "Alice",   "alice",   "entity",  "...");
            insert_entity_page(&c, "n2", "Bob",     "bob",     "entity",  "...");
            insert_entity_page(&c, "n3", "RAG",     "rag",     "concept", "...");
            insert_entity_page(&c, "n4", "RAG vs LLM Wiki", "rag-vs-wiki", "comparison", "...");
        }
        let outcome = {
            let c = store.lock().unwrap();
            regenerate_index(&c, "default", RegenerateTrigger::Manual).unwrap()
        };
        assert!(outcome.bytes_written > 0);
        assert_eq!(outcome.token_cost, 0, "index is zero-LLM");
        assert!(outcome.llm_model.is_none());

        let c = store.lock().unwrap();
        let row: (String, String, i64) = c
            .query_row(
                "SELECT kind, content, token_cost FROM wiki_artifacts \
                 WHERE space_id = 'default' AND id = ?1",
                params![outcome.artifact_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row.0, "index");
        // Layout assertions — group headers + entries in canonical order.
        assert!(row.1.contains("## Entity (2)"), "got:\n{}", row.1);
        assert!(row.1.contains("## Concept (1)"));
        assert!(row.1.contains("## Comparison (1)"));
        // Entries reference titles + slugs.
        assert!(row.1.contains("**Alice**"));
        assert!(row.1.contains("`rag-vs-wiki`"));
        assert_eq!(row.2, 0);
    }

    #[test]
    fn regenerate_index_handles_empty_space() {
        let store = fresh_conn();
        let outcome = {
            let c = store.lock().unwrap();
            regenerate_index(&c, "empty", RegenerateTrigger::Tick).unwrap()
        };
        // Still produces a row with the "No entity pages yet" sentinel.
        let c = store.lock().unwrap();
        let content: String = c
            .query_row(
                "SELECT content FROM wiki_artifacts WHERE id = ?1",
                params![outcome.artifact_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(content.contains("No entity pages yet"));
    }

    #[test]
    fn regenerate_index_excludes_non_entity_page_nodes() {
        let store = fresh_conn();
        {
            let c = store.lock().unwrap();
            insert_entity_page(&c, "ep1", "Page", "page", "entity", "...");
            // Insert a Procedure node — must NOT appear in the index.
            let now = chrono::Utc::now().to_rfc3339();
            c.execute(
                "INSERT INTO memory_nodes \
                 (id, space_id, kind, title, metadata_json, created_at, updated_at) \
                 VALUES ('pr1', 'default', 'procedure', 'Some Skill', NULL, ?1, ?1)",
                params![now],
            )
            .unwrap();
        }
        let outcome = {
            let c = store.lock().unwrap();
            regenerate_index(&c, "default", RegenerateTrigger::Manual).unwrap()
        };
        let c = store.lock().unwrap();
        let content: String = c
            .query_row(
                "SELECT content FROM wiki_artifacts WHERE id = ?1",
                params![outcome.artifact_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(content.contains("**Page**"));
        assert!(!content.contains("Some Skill"));
    }

    // ─── StubSynthesizer ──────────────────────────────────────────

    #[tokio::test]
    async fn stub_synthesizer_marks_output_as_stub() {
        let stub = StubSynthesizer;
        let input = WikiSynthesisInput {
            space_id: "default",
            recent_entity_pages: &[],
            total_entity_pages: 0,
            total_edges: 0,
            generated_at_iso: "2026-05-18T00:00:00Z",
        };
        let out = stub.synthesize_overview(input).await.unwrap();
        assert!(out.markdown.contains("stub"));
        assert!(out.markdown.contains("No entity pages yet"));
        assert_eq!(out.token_cost, 0);
        assert!(out.llm_model.is_none());
        assert_eq!(stub.descriptor(), "stub:no-llm");
    }

    #[tokio::test]
    async fn stub_synthesizer_renders_recent_pages() {
        let stub = StubSynthesizer;
        let pages = vec![
            EntityPageSnapshot {
                node_id: "n1".into(),
                title: "Acme".into(),
                slug: Some("acme".into()),
                subkind: Some("entity".into()),
                compiled_truth_excerpt: "A search startup.".into(),
                updated_at: "2026-05-18".into(),
            },
            EntityPageSnapshot {
                node_id: "n2".into(),
                title: "RAG".into(),
                slug: Some("rag".into()),
                subkind: Some("concept".into()),
                compiled_truth_excerpt: "Retrieval-augmented generation.".into(),
                updated_at: "2026-05-17".into(),
            },
        ];
        let input = WikiSynthesisInput {
            space_id: "default",
            recent_entity_pages: &pages,
            total_entity_pages: 2,
            total_edges: 1,
            generated_at_iso: "2026-05-18T00:00:00Z",
        };
        let out = stub.synthesize_overview(input).await.unwrap();
        assert!(out.markdown.contains("**Acme**"));
        assert!(out.markdown.contains("**RAG**"));
        assert!(out.markdown.contains("2 entity pages, 1 edges"));
    }

    // ─── regenerate_overview + mock synthesizer ──────────────────

    struct MockSynthesizer {
        descriptor: &'static str,
        canned_markdown: String,
        seen_pages: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait]
    impl WikiSynthesizer for MockSynthesizer {
        async fn synthesize_overview(
            &self,
            input: WikiSynthesisInput<'_>,
        ) -> Result<WikiSynthesisOutput, WikiSynthesisError> {
            let mut seen = self.seen_pages.lock().unwrap();
            for p in input.recent_entity_pages {
                seen.push(p.node_id.clone());
            }
            Ok(WikiSynthesisOutput {
                markdown: self.canned_markdown.clone(),
                token_cost: 123,
                llm_model: Some("mock-model".into()),
            })
        }
        fn descriptor(&self) -> &'static str {
            self.descriptor
        }
    }

    #[tokio::test]
    async fn regenerate_overview_writes_artifact_with_synthesizer_output() {
        let store = fresh_conn();
        {
            let c = store.lock().unwrap();
            insert_entity_page(&c, "n1", "Alice", "alice", "entity", "Senior eng.");
            insert_entity_page(&c, "n2", "Acme", "acme", "entity", "Search startup.");
        }
        let mock = Arc::new(MockSynthesizer {
            descriptor: "mock",
            canned_markdown: "# Overview\nHello world.".into(),
            seen_pages: Default::default(),
        });
        let outcome =
            regenerate_overview(store.clone(), mock.clone(), "default", RegenerateTrigger::Manual)
                .await
                .unwrap();
        assert_eq!(outcome.token_cost, 123);
        assert_eq!(outcome.llm_model.as_deref(), Some("mock-model"));

        // Verify artifact row.
        let c = store.lock().unwrap();
        let row: (String, String, Option<String>, i64) = c
            .query_row(
                "SELECT kind, content, llm_model, token_cost FROM wiki_artifacts \
                 WHERE id = ?1",
                params![outcome.artifact_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(row.0, "overview");
        assert_eq!(row.1, "# Overview\nHello world.");
        assert_eq!(row.2.as_deref(), Some("mock-model"));
        assert_eq!(row.3, 123);

        // Confirm the synthesizer saw both pages.
        let seen = mock.seen_pages.lock().unwrap().clone();
        assert!(seen.contains(&"n1".to_string()));
        assert!(seen.contains(&"n2".to_string()));
    }

    #[tokio::test]
    async fn regenerate_overview_propagates_synthesizer_error() {
        struct ErrSyn;
        #[async_trait]
        impl WikiSynthesizer for ErrSyn {
            async fn synthesize_overview(
                &self,
                _input: WikiSynthesisInput<'_>,
            ) -> Result<WikiSynthesisOutput, WikiSynthesisError> {
                Err(WikiSynthesisError::Other("boom".into()))
            }
            fn descriptor(&self) -> &'static str { "err" }
        }
        let store = fresh_conn();
        let err = regenerate_overview(
            store.clone(),
            Arc::new(ErrSyn) as Arc<dyn WikiSynthesizer>,
            "default",
            RegenerateTrigger::Manual,
        )
        .await
        .unwrap_err();
        assert!(format!("{}", err).contains("boom"));
        // No artifact written.
        let c = store.lock().unwrap();
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM wiki_artifacts WHERE kind = 'overview'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }

    // ─── pretty_subkind ────────────────────────────────────────────

    #[test]
    fn pretty_subkind_maps_known_values() {
        assert_eq!(pretty_subkind("entity"), "Entity");
        assert_eq!(pretty_subkind("concept"), "Concept");
        assert_eq!(pretty_subkind("comparison"), "Comparison");
        assert_eq!(pretty_subkind("question"), "Question");
        assert_eq!(pretty_subkind("synthesis"), "Synthesis");
        assert_eq!(pretty_subkind("decision"), "Decision");
        assert_eq!(pretty_subkind("gap"), "Gap");
        assert_eq!(pretty_subkind("default"), "Other");
        assert_eq!(pretty_subkind("some_future_subkind"), "Other");
    }

    #[test]
    fn truncate_for_excerpt_handles_empty_and_long() {
        assert_eq!(truncate_for_excerpt("   ", 10), "_(no content yet)_");
        assert_eq!(truncate_for_excerpt("short", 10), "short");
        let long: String = "a".repeat(200);
        let out = truncate_for_excerpt(&long, 50);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() <= 51);
    }

    // Suppress warning on unused field — we keep the kind constant in
    // case future telemetry wants to filter by trigger type.
    #[test]
    fn regenerate_trigger_renders() {
        assert_eq!(RegenerateTrigger::Tick.as_str(), "tick");
        assert_eq!(RegenerateTrigger::Manual.as_str(), "manual");
    }

    // ─── RealWikiSynthesizer (Phase 6b) ───────────────────────────

    /// The real synth's system prompt is intentionally narrow and
    /// versioned with the codebase, so this test asserts the substrings
    /// that guide both prompt review and downstream behaviour.
    #[test]
    fn real_wiki_system_prompt_includes_narrative_voice_cues() {
        let sys = RealWikiSynthesizer::system_prompt();
        assert!(
            sys.contains("personal AI knowledge wiki"),
            "system prompt missing wiki framing: {sys}"
        );
        assert!(
            sys.contains("Do not invent facts"),
            "system prompt missing anti-hallucination cue: {sys}"
        );
    }

    /// The user prompt must surface stats + recent pages + an explicit
    /// instruction to emit markdown WITHOUT the top-level header (the
    /// WikiView renders one already).
    #[test]
    fn real_wiki_user_prompt_carries_snapshot_signals() {
        let snaps = vec![
            EntityPageSnapshot {
                node_id: "n1".into(),
                title: "Alice".into(),
                slug: Some("alice".into()),
                subkind: Some("person".into()),
                compiled_truth_excerpt: "Senior engineer at Acme.".into(),
                updated_at: "2026-05-15".into(),
            },
            EntityPageSnapshot {
                node_id: "n2".into(),
                title: "RAG".into(),
                slug: Some("rag".into()),
                subkind: Some("concept".into()),
                compiled_truth_excerpt: "Retrieval-augmented generation.".into(),
                updated_at: "2026-05-16".into(),
            },
        ];
        let input = WikiSynthesisInput {
            space_id: "default",
            recent_entity_pages: &snaps,
            total_entity_pages: 2,
            total_edges: 1,
            generated_at_iso: "2026-05-18T10:00:00Z",
        };
        let p = RealWikiSynthesizer::build_user_prompt(&input);
        assert!(p.contains("Total entity pages: 2"));
        assert!(p.contains("Total edges: 1"));
        assert!(p.contains("**Alice**"));
        assert!(p.contains("**RAG**"));
        assert!(p.contains("slug `rag`"));
        assert!(p.contains("subkind: concept"));
        assert!(
            p.contains("do not add a top-level header"),
            "user prompt must avoid double H1: {p}"
        );
    }

    #[test]
    fn real_wiki_user_prompt_handles_empty_snapshots() {
        let input = WikiSynthesisInput {
            space_id: "default",
            recent_entity_pages: &[],
            total_entity_pages: 0,
            total_edges: 0,
            generated_at_iso: "2026-05-18T10:00:00Z",
        };
        let p = RealWikiSynthesizer::build_user_prompt(&input);
        assert!(p.contains("(none yet"), "got: {p}");
    }

    #[tokio::test]
    async fn real_wiki_returns_llm_text_and_token_cost() {
        use crate::memory_graph::memory_os_llm::MockMemoryOsLlm;
        let mock = Arc::new(MockMemoryOsLlm {
            canned_text:
                "## Recent activity\n\nAlice and Bob have been collaborating on the RAG project."
                    .into(),
            canned_input_tokens: 320,
            canned_output_tokens: 80,
            canned_model: "mock:claude-sonnet-4-test".into(),
        });
        let synth = RealWikiSynthesizer::new(mock);
        let input = WikiSynthesisInput {
            space_id: "default",
            recent_entity_pages: &[],
            total_entity_pages: 0,
            total_edges: 0,
            generated_at_iso: "2026-05-18T10:00:00Z",
        };
        let out = synth.synthesize_overview(input).await.unwrap();
        assert!(out.markdown.contains("Alice and Bob"));
        // token_cost = input + output, NOT the cost-tag prefixed string
        assert_eq!(out.token_cost, 320 + 80);
        assert_eq!(out.llm_model.as_deref(), Some("mock:claude-sonnet-4-test"));
    }

    #[tokio::test]
    async fn real_wiki_descriptor_marks_as_real() {
        use crate::memory_graph::memory_os_llm::MockMemoryOsLlm;
        let mock = Arc::new(MockMemoryOsLlm::default());
        let synth = RealWikiSynthesizer::new(mock);
        assert_eq!(synth.descriptor(), "real:memory_os_llm");
    }

    /// Sanity-check the trait-object swap: `Arc<dyn WikiSynthesizer>`
    /// holds either Stub or Real and dispatches correctly. This is the
    /// exact shape `AppState.wiki_synthesizer` uses.
    #[tokio::test]
    async fn real_wiki_swappable_via_trait_object() {
        use crate::memory_graph::memory_os_llm::MockMemoryOsLlm;
        let stub: Arc<dyn WikiSynthesizer> = Arc::new(StubSynthesizer);
        assert_eq!(stub.descriptor(), "stub:no-llm");

        let real: Arc<dyn WikiSynthesizer> =
            Arc::new(RealWikiSynthesizer::new(Arc::new(MockMemoryOsLlm::default())));
        assert_eq!(real.descriptor(), "real:memory_os_llm");

        let input = WikiSynthesisInput {
            space_id: "default",
            recent_entity_pages: &[],
            total_entity_pages: 0,
            total_edges: 0,
            generated_at_iso: "2026-05-18T10:00:00Z",
        };
        // Stub returns deterministic markdown
        let stub_out = stub.synthesize_overview(input).await.unwrap();
        assert!(stub_out.markdown.contains("stub"));
        // Real returns the mock's canned text
        let input2 = WikiSynthesisInput {
            space_id: "default",
            recent_entity_pages: &[],
            total_entity_pages: 0,
            total_edges: 0,
            generated_at_iso: "2026-05-18T10:00:00Z",
        };
        let real_out = real.synthesize_overview(input2).await.unwrap();
        assert!(real_out.markdown.contains("[mock"));
    }
}

// Use `MemoryNodeKind` to silence unused-import on builds where this
// module is referenced only for the trait export. The wiki_synth code
// path will eventually distinguish kinds (Phase 8 subkind taxonomy),
// at which point this import becomes load-bearing.
#[allow(dead_code)]
const _ENSURE_KIND_REFERENCED: Option<MemoryNodeKind> = None;
