//! Memory lint scenario — LLM-driven semantic checks over the
//! memory_graph subsystem.
//!
//! Memory OS Foundation Phase 5 (spec §3.2 B2).
//!
//! Sibling to `memory_health` (Phase 4, zero-LLM). The split mirrors
//! gbrain's dream-cycle / llm-wiki-agent's health.py vs lint.py:
//!
//! - `memory_health` runs every ~30 min, all SQL, free, catches
//!   structural integrity (orphans / phantom rows / index drift).
//!
//! - `memory_lint` runs every ~15 EntityPage writes (capped at 4/day),
//!   uses an LLM, costs tokens, catches **semantic** issues that
//!   require natural-language understanding:
//!
//!   1. **hub_stub**       — node is heavily referenced (high backlink
//!                            count) but its compiled_truth is short.
//!                            Worth enriching with synthesis.
//!   2. **phantom_hub**     — a `[[entity:X]]` slug appears in >= 3
//!                            EntityPage timelines but no entity_page
//!                            with that slug exists. Worth creating.
//!   3. **stale_summary**   — compiled_truth was synthesized > 7 days
//!                            ago AND timeline added >= 5 entries
//!                            since. Worth re-synthesizing.
//!   4. **contradiction**   — two timeline entries on the same page
//!                            disagree about a fact. Surfaced as a
//!                            finding AND persisted into
//!                            EntityPageMetadata.contradictions[]
//!                            (Phase 1 schema already has this field).
//!
//! ## LLM seam
//!
//! Mirrors Phase 3's WikiSynthesizer / StubSynthesizer pattern: a
//! `LintAnalyzer` trait with a `StubAnalyzer` default that produces
//! deterministic placeholder findings clearly labelled "stub" so the
//! frontend can surface them without LLM credentials. Swapping in a
//! real Anthropic/OpenAI client later is a single AppState change.
//!
//! ## Cost guard
//!
//! `run_lint_checks` accepts a `daily_token_budget` parameter; the
//! caller (ProactiveService tick, IPC) sums today's
//! `cost_records.model LIKE 'memory_lint%'` and skips when exceeded.
//! StubAnalyzer reports 0 tokens so the budget check is a no-op in
//! Phase 5 unit tests.

use async_trait::async_trait;
use rusqlite::params;
use serde::Serialize;
use std::sync::Arc;

use crate::memory_graph::entity_page::{Contradiction, EntityPageMetadata};

// ─── LLM seam ──────────────────────────────────────────────────────────

/// Pluggable LLM client for semantic lint checks.
#[async_trait]
pub trait LintAnalyzer: Send + Sync {
    /// Given a `LintCandidate` (one page-level concern), produce zero
    /// or more findings. The analyzer is free to skip any candidate
    /// (e.g. if it judges the issue trivial).
    async fn analyze(
        &self,
        candidate: LintCandidate,
    ) -> Result<LintAnalysisOutput, LintAnalysisError>;

    fn descriptor(&self) -> &'static str;
}

/// What gets fed to the analyzer for one check on one page.
#[derive(Debug, Clone)]
pub struct LintCandidate {
    pub check_kind: LintCheckKind,
    pub node_id: String,
    pub title: String,
    pub compiled_truth: String,
    pub timeline_summary: Vec<String>,
    /// Auxiliary context. Hub-stub passes `backlink_count`; phantom-hub
    /// passes `mention_count`; stale_summary passes `days_since_synth`
    /// + `timeline_entries_since`; contradiction passes the candidate
    /// pair of entries.
    pub aux_json: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LintCheckKind {
    HubStub,
    PhantomHub,
    StaleSummary,
    Contradiction,
}

impl LintCheckKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HubStub => "hub_stub",
            Self::PhantomHub => "phantom_hub",
            Self::StaleSummary => "stale_summary",
            Self::Contradiction => "contradiction",
        }
    }
}

#[derive(Debug)]
pub struct LintAnalysisOutput {
    /// Markdown / human-readable description for the finding row.
    pub message: String,
    /// Severity to record on the finding ("error" / "warn" / "info").
    pub severity: String,
    /// Tokens consumed — `cost_records` rolls this up under
    /// "memory_lint:<kind>" so the cost dashboard separates lint from
    /// other LLM users.
    pub token_cost: u32,
    pub llm_model: Option<String>,
    /// Only Some when `check_kind == Contradiction`. The orchestrator
    /// uses this to populate EntityPageMetadata.contradictions[].
    pub contradiction: Option<Contradiction>,
}

#[derive(Debug, thiserror::Error)]
pub enum LintAnalysisError {
    #[error("analyzer disabled")]
    Disabled,
    #[error("analyzer error: {0}")]
    Other(String),
}

// ─── Default stub analyzer ─────────────────────────────────────────────

/// Deterministic placeholder. Produces a clearly-labelled "stub" finding
/// for each candidate so the Health panel + EntityPage view can be
/// exercised without LLM credentials.
pub struct StubAnalyzer;

#[async_trait]
impl LintAnalyzer for StubAnalyzer {
    async fn analyze(
        &self,
        candidate: LintCandidate,
    ) -> Result<LintAnalysisOutput, LintAnalysisError> {
        let (severity, message) = match candidate.check_kind {
            LintCheckKind::HubStub => (
                "warn",
                format!(
                    "[stub] '{}' is heavily referenced but has a short compiled_truth.",
                    candidate.title
                ),
            ),
            LintCheckKind::PhantomHub => (
                "warn",
                format!(
                    "[stub] Slug referenced multiple times but no EntityPage exists for it: {}",
                    candidate.node_id
                ),
            ),
            LintCheckKind::StaleSummary => (
                "info",
                format!(
                    "[stub] '{}' compiled_truth may be stale relative to recent timeline activity.",
                    candidate.title
                ),
            ),
            LintCheckKind::Contradiction => (
                "warn",
                format!(
                    "[stub] '{}' may contain two timeline entries that disagree.",
                    candidate.title
                ),
            ),
        };

        // Phase 5 stub does not synthesize an actual contradiction
        // payload — that's a real LLM's job. We surface the finding so
        // the UI can be wired, but leave the metadata.contradictions[]
        // untouched.
        Ok(LintAnalysisOutput {
            message,
            severity: severity.into(),
            token_cost: 0,
            llm_model: None,
            contradiction: None,
        })
    }

    fn descriptor(&self) -> &'static str {
        "stub:no-llm"
    }
}

// ─── Real LLM analyzer (Phase 6c) ──────────────────────────────────────

/// Production lint analyzer — routes each `LintCandidate` to the
/// configured LLM via [`crate::memory_graph::memory_os_llm::MemoryOsLlm`].
///
/// Replies are constrained to a small JSON schema so we never depend
/// on the LLM matching free-form English exactly. Any parse failure
/// falls back to recording the raw text as a `warn` finding — that's
/// better than dropping the candidate silently.
///
/// Cost lands in `cost_records.model = "memory_lint:<actual_model>"`
/// — the existing daily-budget computation (`WHERE model LIKE
/// 'memory_lint%'`) already consumes this prefix, so the Phase 5 cost
/// guard keeps working unchanged.
pub struct RealLintAnalyzer {
    llm: Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>,
}

impl RealLintAnalyzer {
    pub fn new(llm: Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>) -> Self {
        Self { llm }
    }

    /// System prompt — pinned in source so test-mode mocks can verify
    /// it, and so review during prompt tuning has one canonical home.
    pub(crate) fn system_prompt() -> &'static str {
        "You are the lint analyzer for a personal AI knowledge wiki. \
         You receive ONE page-level concern at a time and decide whether \
         it warrants surfacing to the user.\n\n\
         Output ONLY a single JSON object on one line, no fences, no \
         prose around it. Schema:\n\
         {\"verdict\":\"report\"|\"skip\",\"severity\":\"error\"|\"warn\"|\"info\",\"message\":\"<≤200 chars>\",\"contradiction\":{\"claim_a\":\"...\",\"claim_b\":\"...\"}}\n\n\
         - `verdict=\"skip\"` means the candidate is not actually a problem (drop it).\n\
         - `message` describes the user-visible finding succinctly.\n\
         - `contradiction` is REQUIRED only when check_kind=contradiction and verdict=report; otherwise omit.\n\
         - Do not invent facts that are not in the input.\n\
         - Do not output anything outside the JSON object."
    }

    pub(crate) fn build_user_prompt(candidate: &LintCandidate) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "check_kind: {}\n\
             page_title: {}\n\
             page_node_id: {}\n\
             compiled_truth_excerpt: {}\n\n",
            candidate.check_kind.as_str(),
            candidate.title,
            candidate.node_id,
            truncate(&candidate.compiled_truth, 800),
        ));
        out.push_str("timeline_summary:\n");
        if candidate.timeline_summary.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for line in candidate.timeline_summary.iter().take(20) {
                out.push_str(&format!("  - {}\n", truncate(line, 240)));
            }
        }
        out.push_str(&format!("aux: {}\n", candidate.aux_json));
        out.push_str("\nReply with the JSON object now.\n");
        out
    }
}

#[async_trait]
impl LintAnalyzer for RealLintAnalyzer {
    async fn analyze(
        &self,
        candidate: LintCandidate,
    ) -> Result<LintAnalysisOutput, LintAnalysisError> {
        let check_kind = candidate.check_kind;
        let user_prompt = Self::build_user_prompt(&candidate);
        let out = self
            .llm
            .complete_text("memory_lint", Self::system_prompt(), &user_prompt, 600)
            .await
            .map_err(|e| LintAnalysisError::Other(e.to_string()))?;

        let token_cost = out.input_tokens.saturating_add(out.output_tokens);
        let llm_model = Some(out.model);

        // Parse the structured response. We accept either a clean JSON
        // line or "any JSON object somewhere in the text" — some models
        // are stubborn about wrapping the object in commentary.
        let parsed = parse_lint_response(&out.text);
        let (severity, message, contradiction_pair) = match parsed {
            LintResponseParse::Skip => {
                return Err(LintAnalysisError::Disabled);
            }
            LintResponseParse::Report {
                severity,
                message,
                contradiction,
            } => (severity, message, contradiction),
            LintResponseParse::Unparseable(raw) => {
                tracing::warn!(
                    "RealLintAnalyzer: failed to parse LLM reply, falling back to raw text"
                );
                ("warn".to_string(), truncate(&raw, 200), None)
            }
        };

        let contradiction = if check_kind == LintCheckKind::Contradiction {
            contradiction_pair.map(|(a, b)| Contradiction {
                between_source_ids: vec![candidate.node_id.clone()],
                claim_a: a,
                claim_b: b,
                noticed_at: chrono::Utc::now().to_rfc3339(),
            })
        } else {
            None
        };

        Ok(LintAnalysisOutput {
            message,
            severity,
            token_cost,
            llm_model,
            contradiction,
        })
    }

    fn descriptor(&self) -> &'static str {
        "real:memory_os_llm"
    }
}

/// Outcome of parsing the LLM's structured reply.
#[derive(Debug, PartialEq)]
pub(crate) enum LintResponseParse {
    /// LLM said this isn't actually a problem.
    Skip,
    /// LLM produced a finding.
    Report {
        severity: String,
        message: String,
        contradiction: Option<(String, String)>,
    },
    /// LLM reply didn't match our schema. Caller falls back to a raw warn.
    Unparseable(String),
}

/// Pull the first JSON object out of `text` and try to interpret it as
/// the lint schema. Tolerates surrounding prose, but not malformed JSON
/// inside the braces.
pub(crate) fn parse_lint_response(text: &str) -> LintResponseParse {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return LintResponseParse::Unparseable(String::new());
    }

    // Find the first '{' and the matching '}' (greedy — relies on the
    // schema being one object, no nested objects with sibling braces).
    let start = match trimmed.find('{') {
        Some(s) => s,
        None => return LintResponseParse::Unparseable(trimmed.to_string()),
    };
    let end = match trimmed.rfind('}') {
        Some(e) => e,
        None => return LintResponseParse::Unparseable(trimmed.to_string()),
    };
    if end <= start {
        return LintResponseParse::Unparseable(trimmed.to_string());
    }
    let object_slice = &trimmed[start..=end];
    let v: serde_json::Value = match serde_json::from_str(object_slice) {
        Ok(v) => v,
        Err(_) => return LintResponseParse::Unparseable(trimmed.to_string()),
    };

    let verdict = v.get("verdict").and_then(|x| x.as_str()).unwrap_or("");
    if verdict == "skip" {
        return LintResponseParse::Skip;
    }

    let severity = v
        .get("severity")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .filter(|s| matches!(s.as_str(), "error" | "warn" | "info"))
        .unwrap_or_else(|| "warn".to_string());
    let message = v
        .get("message")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "(no message)".to_string());

    let contradiction = v.get("contradiction").and_then(|c| {
        let a = c.get("claim_a")?.as_str()?.to_string();
        let b = c.get("claim_b")?.as_str()?.to_string();
        Some((a, b))
    });

    LintResponseParse::Report {
        severity,
        message,
        contradiction,
    }
}

/// Chop a string to at most `max` chars and append `…` if truncated.
/// Char-aware so we don't slice in the middle of a multi-byte rune.
pub(crate) fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let mut out: String = chars.iter().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

// ─── Run config + outcome ──────────────────────────────────────────────

/// Knobs that the caller (ProactiveService / IPC) controls. The
/// orchestrator itself never reads memubot_config directly — keeps the
/// scenario testable without faking config layers.
#[derive(Debug, Clone)]
pub struct LintRunConfig {
    pub hub_stub_min_backlinks: i64,
    pub hub_stub_max_content_len: usize,
    pub phantom_hub_min_mentions: usize,
    pub stale_summary_days_threshold: i64,
    pub stale_summary_min_timeline_entries: usize,
    pub max_candidates_per_run: usize,
    /// `cost_records.model` filter for the daily-budget computation.
    /// Caller (ProactiveService) provides today's already-spent total
    /// and the budget cap; orchestrator stops short if consuming the
    /// next candidate would exceed the cap.
    pub daily_token_budget: u32,
}

impl Default for LintRunConfig {
    fn default() -> Self {
        Self {
            hub_stub_min_backlinks: 5,
            hub_stub_max_content_len: 500,
            phantom_hub_min_mentions: 3,
            stale_summary_days_threshold: 7,
            stale_summary_min_timeline_entries: 5,
            max_candidates_per_run: 8,
            daily_token_budget: 50_000,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LintRunOutcome {
    pub hub_stub: u32,
    pub phantom_hub: u32,
    pub stale_summary: u32,
    pub contradiction: u32,
    pub total_inserted: u32,
    pub total_tokens: u32,
    pub skipped_due_to_budget: u32,
    pub duration_ms: u64,
    pub analyzer_descriptor: String,
}

// ─── Orchestrator ──────────────────────────────────────────────────────

/// Run the four lint checks against `space_id`. Each check produces
/// candidates; each candidate is analyzed by the LLM (or stub); each
/// produced finding lands in `memory_health_findings.is_lint = 1`.
///
/// The caller passes:
///   - `today_spent_tokens` — sum of cost_records for today filtered to
///     memory_lint:* models. Used together with
///     `cfg.daily_token_budget` to bail out before exceeding the cap.
///
/// Lock contract: the orchestrator acquires the conn lock multiple
/// times (once per candidate batch fetch + once per finding write).
/// Caller should run on `tokio::spawn_blocking` so the runtime keeps
/// moving while the analyzer's LLM calls are in flight.
pub async fn run_lint_checks(
    store: Arc<crate::memory_graph::store::MemoryGraphStore>,
    analyzer: Arc<dyn LintAnalyzer>,
    space_id: &str,
    cfg: &LintRunConfig,
    today_spent_tokens: u32,
) -> Result<LintRunOutcome, crate::error::Error> {
    let started = std::time::Instant::now();
    let mut out = LintRunOutcome {
        analyzer_descriptor: analyzer.descriptor().to_string(),
        ..Default::default()
    };
    let mut remaining_budget = cfg.daily_token_budget.saturating_sub(today_spent_tokens);

    // Gather candidates (zero-LLM, pure SQL — same lock pattern as
    // memory_health). Capped at max_candidates_per_run so a misbehaving
    // workspace can't accumulate hundreds of analyzer calls in one
    // tick.
    let candidates = fetch_candidates(&store, space_id, cfg)?;

    for cand in candidates.into_iter().take(cfg.max_candidates_per_run) {
        // Budget check — stop before consuming a candidate that would
        // push us over the daily cap. Per-candidate consumption isn't
        // known in advance so we use a conservative estimate (4096
        // tokens, matching a typical Haiku context window) plus the
        // current remaining budget.
        if remaining_budget < 4096 {
            out.skipped_due_to_budget += 1;
            continue;
        }

        let kind = cand.check_kind;
        match analyzer.analyze(cand.clone()).await {
            Ok(result) => {
                out.total_tokens = out.total_tokens.saturating_add(result.token_cost);
                remaining_budget = remaining_budget.saturating_sub(result.token_cost.max(0));
                if let Err(e) = persist_finding(&store, space_id, kind, &cand, &result) {
                    tracing::warn!(
                        check = kind.as_str(),
                        node_id = %cand.node_id,
                        error = %e,
                        "memory_lint: failed to persist finding (non-fatal)"
                    );
                    continue;
                }
                // Side-effect: contradiction finding also writes into
                // EntityPageMetadata.contradictions[].
                if let (LintCheckKind::Contradiction, Some(contra)) =
                    (kind, result.contradiction.as_ref())
                {
                    if let Err(e) = append_contradiction(&store, &cand.node_id, contra) {
                        tracing::warn!(
                            node_id = %cand.node_id,
                            error = %e,
                            "memory_lint: failed to append contradiction to metadata (non-fatal)"
                        );
                    }
                }
                match kind {
                    LintCheckKind::HubStub => out.hub_stub += 1,
                    LintCheckKind::PhantomHub => out.phantom_hub += 1,
                    LintCheckKind::StaleSummary => out.stale_summary += 1,
                    LintCheckKind::Contradiction => out.contradiction += 1,
                }
                out.total_inserted += 1;
            }
            Err(LintAnalysisError::Disabled) => {
                // Analyzer self-disabled mid-run. Stop early; user
                // toggled the flag or hit an upstream rate limit.
                break;
            }
            Err(LintAnalysisError::Other(msg)) => {
                tracing::warn!(
                    check = kind.as_str(),
                    error = msg,
                    "memory_lint: analyzer error (skipping candidate)"
                );
            }
        }
    }

    out.duration_ms = started.elapsed().as_millis() as u64;
    Ok(out)
}

// ─── Candidate fetchers (SQL-only, no LLM) ─────────────────────────────

fn fetch_candidates(
    store: &crate::memory_graph::store::MemoryGraphStore,
    space_id: &str,
    cfg: &LintRunConfig,
) -> Result<Vec<LintCandidate>, crate::error::Error> {
    let mut out = Vec::new();
    out.extend(fetch_hub_stub_candidates(store, space_id, cfg)?);
    out.extend(fetch_stale_summary_candidates(store, space_id, cfg)?);
    out.extend(fetch_contradiction_candidates(store, space_id)?);
    // phantom_hub left as a follow-up: it requires NER-style scanning of
    // free-text references inside compiled_truth, which the auto-link
    // hook only catches when the slug already exists (Phase 2 contract).
    // Phase 15 (Engines NER) lands the real detection; until then we
    // simply emit no phantom_hub candidates from this function.
    Ok(out)
}

fn fetch_hub_stub_candidates(
    store: &crate::memory_graph::store::MemoryGraphStore,
    space_id: &str,
    cfg: &LintRunConfig,
) -> Result<Vec<LintCandidate>, crate::error::Error> {
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let sql = "
        SELECT n.id, n.title, COALESCE(v.content, '') AS content, \
               (SELECT COUNT(*) FROM memory_edges e WHERE e.child_node_id = n.id) AS backlinks \
        FROM memory_nodes n \
        LEFT JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active' \
        WHERE n.space_id = ?1 AND n.kind = 'entity_page' \
          AND COALESCE(LENGTH(v.content), 0) <= ?2 \
          AND (SELECT COUNT(*) FROM memory_edges e WHERE e.child_node_id = n.id) >= ?3 \
        ORDER BY backlinks DESC \
        LIMIT 50";
    let mut stmt = conn.prepare(sql).map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(
            params![
                space_id,
                cfg.hub_stub_max_content_len as i64,
                cfg.hub_stub_min_backlinks,
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            },
        )
        .map_err(crate::error::Error::Database)?;
    let collected: Vec<_> = rows.flatten().collect();
    drop(stmt);

    Ok(collected
        .into_iter()
        .map(|(id, title, content, backlinks)| LintCandidate {
            check_kind: LintCheckKind::HubStub,
            node_id: id,
            title,
            compiled_truth: content,
            timeline_summary: Vec::new(),
            aux_json: serde_json::json!({ "backlinks": backlinks }),
        })
        .collect())
}

fn fetch_stale_summary_candidates(
    store: &crate::memory_graph::store::MemoryGraphStore,
    space_id: &str,
    cfg: &LintRunConfig,
) -> Result<Vec<LintCandidate>, crate::error::Error> {
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let sql = "
        SELECT n.id, n.title, COALESCE(v.content, ''), n.metadata_json \
        FROM memory_nodes n \
        LEFT JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active' \
        WHERE n.space_id = ?1 AND n.kind = 'entity_page' \
        LIMIT 100";
    let mut stmt = conn.prepare(sql).map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(params![space_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(crate::error::Error::Database)?;
    let collected: Vec<_> = rows.flatten().collect();
    drop(stmt);

    let now = chrono::Utc::now();
    let threshold = chrono::Duration::days(cfg.stale_summary_days_threshold);
    let min_entries = cfg.stale_summary_min_timeline_entries;

    Ok(collected
        .into_iter()
        .filter_map(|(id, title, content, meta_json)| {
            // Parse metadata to inspect timeline + last_synthesized_at.
            let meta_value: serde_json::Value = meta_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Null);
            let meta = EntityPageMetadata::from_value(&meta_value);

            // Skip if we don't have a last_synthesized_at — the page
            // was never auto-synthesized so there's nothing to be
            // "stale" relative to.
            let last_syn = meta.last_synthesized_at.as_deref()?;
            let last_syn_dt = chrono::DateTime::parse_from_rfc3339(last_syn).ok()?;
            let age = now.signed_duration_since(last_syn_dt.with_timezone(&chrono::Utc));
            if age < threshold {
                return None;
            }
            // Count timeline entries newer than last_syn.
            let entries_since: usize = meta
                .timeline
                .iter()
                .filter(|t| t.date.as_str() > &last_syn[..10])
                .count();
            if entries_since < min_entries {
                return None;
            }

            Some(LintCandidate {
                check_kind: LintCheckKind::StaleSummary,
                node_id: id,
                title,
                compiled_truth: content,
                timeline_summary: meta
                    .timeline
                    .iter()
                    .rev()
                    .take(10)
                    .map(|t| format!("{} — {}", t.date, t.text))
                    .collect(),
                aux_json: serde_json::json!({
                    "days_since_synth": age.num_days(),
                    "timeline_entries_since": entries_since,
                }),
            })
        })
        .collect())
}

fn fetch_contradiction_candidates(
    store: &crate::memory_graph::store::MemoryGraphStore,
    space_id: &str,
) -> Result<Vec<LintCandidate>, crate::error::Error> {
    // Per-page candidate. Any EntityPage with `timeline.len() >= 2` AND
    // no already-recorded contradiction is a candidate; the analyzer
    // (real LLM or stub) decides whether the entries actually disagree.
    //
    // Two consequences worth being aware of:
    //
    // 1. A page can simultaneously be a candidate for multiple checks
    //    (e.g. a stale_summary page with 6 today-dated entries will
    //    also show up here). That's by design — each check writes a
    //    distinct finding with a distinct check_kind, and the stub
    //    analyzer treats every candidate as a real issue. In
    //    production, a real LLM analyzer will return zero findings
    //    for pages whose entries don't actually contradict, so the
    //    overlap fades out.
    //
    // 2. Tests that want each fixture page to surface in exactly one
    //    check should either pre-seed `metadata.contradictions` so
    //    this finder skips the page, or keep their timeline at length
    //    1. The `run_lint_with_stub_writes_findings_for_each_kind`
    //    test uses the former.
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let sql = "
        SELECT n.id, n.title, COALESCE(v.content, ''), n.metadata_json \
        FROM memory_nodes n \
        LEFT JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active' \
        WHERE n.space_id = ?1 AND n.kind = 'entity_page' \
        LIMIT 100";
    let mut stmt = conn.prepare(sql).map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(params![space_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(crate::error::Error::Database)?;
    let collected: Vec<_> = rows.flatten().collect();
    drop(stmt);

    Ok(collected
        .into_iter()
        .filter_map(|(id, title, content, meta_json)| {
            let meta_value: serde_json::Value = meta_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Null);
            let meta = EntityPageMetadata::from_value(&meta_value);
            if meta.timeline.len() < 2 {
                return None;
            }
            // Skip pages that already have a recorded contradiction —
            // surfacing again would create duplicate findings.
            if !meta.contradictions.is_empty() {
                return None;
            }
            Some(LintCandidate {
                check_kind: LintCheckKind::Contradiction,
                node_id: id,
                title,
                compiled_truth: content,
                timeline_summary: meta
                    .timeline
                    .iter()
                    .map(|t| format!("{} — {}", t.date, t.text))
                    .collect(),
                aux_json: serde_json::json!({ "timeline_len": meta.timeline.len() }),
            })
        })
        .collect())
}

// ─── Persistence ───────────────────────────────────────────────────────

fn persist_finding(
    store: &crate::memory_graph::store::MemoryGraphStore,
    space_id: &str,
    kind: LintCheckKind,
    cand: &LintCandidate,
    output: &LintAnalysisOutput,
) -> Result<(), crate::error::Error> {
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

    // Dedup with Phase 4 contract: same (space_id, subject, check_kind)
    // already-open row → skip insert. Dismissed rows do NOT block
    // re-detection.
    let open_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_health_findings \
             WHERE space_id = ?1 AND subject = ?2 AND check_kind = ?3 \
               AND dismissed = 0",
            params![space_id, cand.node_id, kind.as_str()],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if open_count > 0 {
        return Ok(());
    }

    let id = uuid::Uuid::new_v4().to_string();
    let payload = serde_json::json!({
        "title": cand.title,
        "message": output.message,
        "aux": cand.aux_json,
        "tokens": output.token_cost,
        "model": output.llm_model,
    });
    conn.execute(
        "INSERT INTO memory_health_findings \
         (id, space_id, severity, check_kind, subject, payload_json, is_lint, discovered_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)",
        params![
            id,
            space_id,
            output.severity,
            kind.as_str(),
            cand.node_id,
            payload.to_string(),
            chrono::Utc::now().timestamp_millis(),
        ],
    )
    .map_err(crate::error::Error::Database)?;
    Ok(())
}

fn append_contradiction(
    store: &crate::memory_graph::store::MemoryGraphStore,
    node_id: &str,
    contradiction: &Contradiction,
) -> Result<(), crate::error::Error> {
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let metadata_raw: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM memory_nodes WHERE id = ?1",
            params![node_id],
            |r| r.get(0),
        )
        .ok();
    let value: serde_json::Value = metadata_raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
    let mut meta = EntityPageMetadata::from_value(&value);
    meta.contradictions.push(contradiction.clone());
    let new_json = serde_json::to_string(&meta.to_value())
        .map_err(crate::error::Error::Serde)?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE memory_nodes SET metadata_json = ?1, updated_at = ?2 WHERE id = ?3",
        params![new_json, now, node_id],
    )
    .map_err(crate::error::Error::Database)?;
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::entity_page::TimelineEntry;
    use crate::memory_graph::store::MemoryGraphStore;
    use rusqlite::Connection;
    use std::sync::Mutex;

    fn fresh_store() -> Arc<MemoryGraphStore> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        Arc::new(MemoryGraphStore::new(Arc::new(Mutex::new(conn))))
    }

    fn insert_entity_page(
        store: &MemoryGraphStore,
        id: &str,
        title: &str,
        content: &str,
        metadata: EntityPageMetadata,
    ) {
        let conn = store.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO memory_nodes \
             (id, space_id, kind, title, metadata_json, created_at, updated_at) \
             VALUES (?1, 'default', 'entity_page', ?2, ?3, ?4, ?4)",
            params![id, title, metadata.to_value().to_string(), now],
        )
        .unwrap();
        let v_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO memory_versions \
             (id, node_id, supersedes_version_id, status, content, created_at) \
             VALUES (?1, ?2, NULL, 'active', ?3, ?4)",
            params![v_id, id, content, now],
        )
        .unwrap();
    }

    fn insert_backlinks(store: &MemoryGraphStore, child_id: &str, count: usize) {
        let conn = store.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for i in 0..count {
            let parent_id = format!("parent-{}-{}", child_id, i);
            // Parent is a separate Episode node so it doesn't itself
            // become a hub_stub candidate.
            conn.execute(
                "INSERT INTO memory_nodes (id, space_id, kind, title, created_at, updated_at) \
                 VALUES (?1, 'default', 'episode', ?2, ?3, ?3)",
                params![parent_id, format!("Parent {}", i), now],
            )
            .unwrap();
            let edge_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO memory_edges \
                 (id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, created_at, updated_at) \
                 VALUES (?1, 'default', ?2, ?3, 'relates_to', 'private', 0, ?4, ?4)",
                params![edge_id, parent_id, child_id, now],
            )
            .unwrap();
        }
    }

    // ─── StubAnalyzer ──────────────────────────────────────────────

    #[tokio::test]
    async fn stub_analyzer_emits_warn_finding_with_zero_tokens() {
        let stub = StubAnalyzer;
        let candidate = LintCandidate {
            check_kind: LintCheckKind::HubStub,
            node_id: "n1".into(),
            title: "Hub".into(),
            compiled_truth: "short".into(),
            timeline_summary: vec![],
            aux_json: serde_json::json!({"backlinks": 7}),
        };
        let result = stub.analyze(candidate).await.unwrap();
        assert_eq!(result.severity, "warn");
        assert!(result.message.starts_with("[stub]"));
        assert_eq!(result.token_cost, 0);
        assert!(result.contradiction.is_none());
        assert_eq!(stub.descriptor(), "stub:no-llm");
    }

    // ─── fetch_hub_stub_candidates ─────────────────────────────────

    #[test]
    fn hub_stub_finds_short_content_with_high_backlinks() {
        let store = fresh_store();
        // Short content + 5 backlinks → candidate.
        insert_entity_page(&store, "hub1", "Hub", "tiny", EntityPageMetadata::default());
        insert_backlinks(&store, "hub1", 5);
        // Long content + 5 backlinks → NOT a candidate.
        insert_entity_page(
            &store,
            "long1",
            "Long",
            &"x".repeat(600),
            EntityPageMetadata::default(),
        );
        insert_backlinks(&store, "long1", 5);
        // Short content + 2 backlinks → NOT a candidate (under threshold).
        insert_entity_page(&store, "low1", "Low", "tiny", EntityPageMetadata::default());
        insert_backlinks(&store, "low1", 2);

        let cfg = LintRunConfig::default();
        let cands = fetch_hub_stub_candidates(&store, "default", &cfg).unwrap();
        let ids: Vec<&str> = cands.iter().map(|c| c.node_id.as_str()).collect();
        assert_eq!(ids, vec!["hub1"]);
    }

    // ─── fetch_stale_summary_candidates ────────────────────────────

    #[test]
    fn stale_summary_finds_old_synth_with_recent_timeline() {
        let store = fresh_store();
        let old = chrono::Utc::now()
            - chrono::Duration::days(15);
        let old_iso = old.to_rfc3339();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        let mut meta = EntityPageMetadata {
            last_synthesized_at: Some(old_iso),
            ..Default::default()
        };
        for i in 0..6 {
            meta.timeline.push(TimelineEntry {
                date: today.clone(),
                text: format!("Recent event {}", i),
                source_node_id: None,
                source_session_id: None,
            });
        }
        insert_entity_page(&store, "stale1", "Stale", "old content", meta);

        // Fresh synth, lots of timeline — NOT stale.
        let mut fresh_meta = EntityPageMetadata {
            last_synthesized_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        };
        for _ in 0..6 {
            fresh_meta.timeline.push(TimelineEntry {
                date: today.clone(),
                text: "x".into(),
                source_node_id: None,
                source_session_id: None,
            });
        }
        insert_entity_page(&store, "fresh1", "Fresh", "fresh content", fresh_meta);

        let cfg = LintRunConfig::default();
        let cands = fetch_stale_summary_candidates(&store, "default", &cfg).unwrap();
        let ids: Vec<&str> = cands.iter().map(|c| c.node_id.as_str()).collect();
        assert_eq!(ids, vec!["stale1"]);
    }

    #[test]
    fn stale_summary_skips_pages_without_last_synthesized_at() {
        let store = fresh_store();
        // No metadata at all — not a candidate even with timeline.
        insert_entity_page(&store, "never1", "Never", "x", EntityPageMetadata::default());
        let cands = fetch_stale_summary_candidates(&store, "default", &LintRunConfig::default()).unwrap();
        assert!(cands.is_empty());
    }

    // ─── fetch_contradiction_candidates ────────────────────────────

    #[test]
    fn contradiction_requires_multi_entry_timeline() {
        let store = fresh_store();
        // Single-entry timeline — not a candidate.
        let mut single = EntityPageMetadata::default();
        single.timeline.push(TimelineEntry {
            date: "2026-05-01".into(),
            text: "Only".into(),
            source_node_id: None,
            source_session_id: None,
        });
        insert_entity_page(&store, "single", "Single", "x", single);

        // Two-entry timeline — candidate.
        let mut multi = EntityPageMetadata::default();
        for i in 0..2 {
            multi.timeline.push(TimelineEntry {
                date: format!("2026-05-0{}", i + 1),
                text: format!("Entry {}", i),
                source_node_id: None,
                source_session_id: None,
            });
        }
        insert_entity_page(&store, "multi", "Multi", "x", multi);

        let cands = fetch_contradiction_candidates(&store, "default").unwrap();
        let ids: Vec<&str> = cands.iter().map(|c| c.node_id.as_str()).collect();
        assert_eq!(ids, vec!["multi"]);
    }

    #[test]
    fn contradiction_skips_pages_with_existing_contradictions() {
        let store = fresh_store();
        let mut meta = EntityPageMetadata::default();
        for i in 0..3 {
            meta.timeline.push(TimelineEntry {
                date: format!("2026-05-0{}", i + 1),
                text: "x".into(),
                source_node_id: None,
                source_session_id: None,
            });
        }
        meta.contradictions.push(Contradiction {
            between_source_ids: vec!["a".into(), "b".into()],
            claim_a: "x".into(),
            claim_b: "y".into(),
            noticed_at: "2026-05-18T00:00:00Z".into(),
        });
        insert_entity_page(&store, "already", "Already", "x", meta);

        let cands = fetch_contradiction_candidates(&store, "default").unwrap();
        assert!(cands.is_empty(), "pages with existing contradictions should be skipped");
    }

    // ─── persist_finding + dedup ───────────────────────────────────

    #[test]
    fn persist_finding_dedupes_within_open_window() {
        let store = fresh_store();
        insert_entity_page(&store, "n1", "N1", "x", EntityPageMetadata::default());

        let cand = LintCandidate {
            check_kind: LintCheckKind::HubStub,
            node_id: "n1".into(),
            title: "N1".into(),
            compiled_truth: "x".into(),
            timeline_summary: vec![],
            aux_json: serde_json::json!({}),
        };
        let output = LintAnalysisOutput {
            message: "msg".into(),
            severity: "warn".into(),
            token_cost: 0,
            llm_model: None,
            contradiction: None,
        };

        persist_finding(&store, "default", LintCheckKind::HubStub, &cand, &output).unwrap();
        persist_finding(&store, "default", LintCheckKind::HubStub, &cand, &output).unwrap();

        let conn = store.conn.lock().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_health_findings WHERE subject = 'n1' AND check_kind = 'hub_stub'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "dedup must prevent duplicate inserts");
    }

    // ─── append_contradiction ──────────────────────────────────────

    #[test]
    fn append_contradiction_writes_to_metadata() {
        let store = fresh_store();
        insert_entity_page(&store, "n1", "N1", "x", EntityPageMetadata::default());

        let contradiction = Contradiction {
            between_source_ids: vec!["src-a".into(), "src-b".into()],
            claim_a: "He works at Acme".into(),
            claim_b: "He works at Beta".into(),
            noticed_at: "2026-05-18T00:00:00Z".into(),
        };
        append_contradiction(&store, "n1", &contradiction).unwrap();

        // Verify metadata.contradictions[] now contains exactly one entry.
        let conn = store.conn.lock().unwrap();
        let raw: String = conn
            .query_row(
                "SELECT metadata_json FROM memory_nodes WHERE id = 'n1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let meta = EntityPageMetadata::from_value(&v);
        assert_eq!(meta.contradictions.len(), 1);
        assert_eq!(meta.contradictions[0].claim_a, "He works at Acme");
    }

    // ─── run_lint_checks orchestration ─────────────────────────────

    #[tokio::test]
    async fn run_lint_with_stub_writes_findings_for_each_kind() {
        let store = fresh_store();

        // Set up: one fixture page per check kind. The contradiction
        // finder operates per-page (any EntityPage with >= 2 timeline
        // entries is a candidate unless it ALREADY has a recorded
        // contradiction), so to keep this test single-purpose per
        // fixture we pre-seed `metadata.contradictions` on the stale
        // page — that's how a production page would look once a real
        // LLM analyzer had previously flagged it.
        //
        // Without the pre-seeded contradiction the stale fixture would
        // *also* show up as a contradiction candidate (6 timeline
        // entries >= 2). The stub treats every candidate as a real
        // contradiction, which would double-count. See the comment on
        // `fetch_contradiction_candidates` for the by-design rationale.

        // hub: hub_stub only — short content + 6 backlinks.
        insert_entity_page(&store, "hub", "Hub", "tiny", EntityPageMetadata::default());
        insert_backlinks(&store, "hub", 6);

        // stale: stale_summary only — old synth + 6 recent entries +
        // pre-seeded contradiction so the contradiction finder skips it.
        let old = (chrono::Utc::now() - chrono::Duration::days(20)).to_rfc3339();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let mut stale_meta = EntityPageMetadata {
            last_synthesized_at: Some(old),
            contradictions: vec![Contradiction {
                between_source_ids: vec!["pre-existing".into(), "noted-earlier".into()],
                claim_a: "(seeded so the contradiction finder skips this fixture)".into(),
                claim_b: "(seeded so the contradiction finder skips this fixture)".into(),
                noticed_at: "2026-01-01T00:00:00Z".into(),
            }],
            ..Default::default()
        };
        for i in 0..6 {
            stale_meta.timeline.push(TimelineEntry {
                date: today.clone(),
                text: format!("e{}", i),
                source_node_id: None,
                source_session_id: None,
            });
        }
        insert_entity_page(&store, "stale", "Stale", "x", stale_meta);

        // contra: contradiction only — exactly 2 entries, no prior
        // contradictions recorded, no last_synthesized_at so it's not
        // a stale_summary candidate either.
        let mut contra_meta = EntityPageMetadata::default();
        for i in 0..2 {
            contra_meta.timeline.push(TimelineEntry {
                date: format!("2026-05-0{}", i + 1),
                text: format!("e{}", i),
                source_node_id: None,
                source_session_id: None,
            });
        }
        insert_entity_page(&store, "contra", "Contra", "x", contra_meta);

        let stub = Arc::new(StubAnalyzer) as Arc<dyn LintAnalyzer>;
        let outcome = run_lint_checks(store, stub, "default", &LintRunConfig::default(), 0)
            .await
            .unwrap();
        assert_eq!(outcome.hub_stub, 1);
        assert_eq!(outcome.stale_summary, 1);
        assert_eq!(outcome.contradiction, 1);
        assert_eq!(outcome.total_inserted, 3);
        assert_eq!(outcome.total_tokens, 0);
        assert_eq!(outcome.analyzer_descriptor, "stub:no-llm");
    }

    #[tokio::test]
    async fn run_lint_overlapping_candidate_pages_get_one_finding_per_kind() {
        // Positive coverage for the per-page-per-kind contract from
        // `fetch_contradiction_candidates`'s doc: a single page that
        // matches multiple finders produces one finding per matching
        // kind (not deduplicated, not coalesced). When a real LLM
        // analyzer concludes the candidate ISN'T actually a problem
        // it would emit no finding for that kind; the stub assumes
        // every candidate is real so this test captures the worst
        // (loudest) case.
        let store = fresh_store();
        let old = (chrono::Utc::now() - chrono::Duration::days(20)).to_rfc3339();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        // One page that simultaneously:
        //   - has short content (will NOT be hub_stub — 0 backlinks)
        //   - is stale (6 recent entries, old last_synthesized_at)
        //   - has >= 2 timeline entries AND no pre-recorded
        //     contradictions → contradiction candidate
        let mut meta = EntityPageMetadata {
            last_synthesized_at: Some(old),
            ..Default::default()
        };
        for i in 0..6 {
            meta.timeline.push(TimelineEntry {
                date: today.clone(),
                text: format!("e{}", i),
                source_node_id: None,
                source_session_id: None,
            });
        }
        insert_entity_page(&store, "double", "Double", "x", meta);

        let stub = Arc::new(StubAnalyzer) as Arc<dyn LintAnalyzer>;
        let outcome = run_lint_checks(store, stub, "default", &LintRunConfig::default(), 0)
            .await
            .unwrap();
        // Same page produces 2 findings: one stale_summary, one
        // contradiction. hub_stub does NOT fire because the page has
        // 0 backlinks (below the threshold).
        assert_eq!(outcome.hub_stub, 0, "no backlinks → no hub_stub");
        assert_eq!(outcome.stale_summary, 1);
        assert_eq!(outcome.contradiction, 1);
        assert_eq!(outcome.total_inserted, 2);
    }

    #[tokio::test]
    async fn run_lint_respects_daily_token_budget() {
        // Force the candidate count > 0 then set today_spent_tokens at
        // the budget so no candidates are analyzed.
        let store = fresh_store();
        insert_entity_page(&store, "hub", "Hub", "tiny", EntityPageMetadata::default());
        insert_backlinks(&store, "hub", 6);

        let stub = Arc::new(StubAnalyzer) as Arc<dyn LintAnalyzer>;
        let cfg = LintRunConfig {
            daily_token_budget: 100,
            ..Default::default()
        };
        // Spent already equals the budget — remaining = 0 < 4096 → skip.
        let outcome = run_lint_checks(store, stub, "default", &cfg, 100).await.unwrap();
        assert_eq!(outcome.total_inserted, 0);
        assert!(outcome.skipped_due_to_budget > 0);
    }

    // ─── RealLintAnalyzer (Phase 6c) ─────────────────────────────

    #[test]
    fn real_lint_system_prompt_pins_schema() {
        let sys = RealLintAnalyzer::system_prompt();
        // Schema fields the analyzer relies on
        assert!(sys.contains("verdict"));
        assert!(sys.contains("severity"));
        assert!(sys.contains("message"));
        assert!(sys.contains("contradiction"));
        // Anti-hallucination cue
        assert!(
            sys.contains("Do not invent facts"),
            "system prompt must include anti-hallucination cue"
        );
    }

    #[test]
    fn real_lint_user_prompt_carries_check_context() {
        let candidate = LintCandidate {
            check_kind: LintCheckKind::HubStub,
            node_id: "n42".into(),
            title: "Alice".into(),
            compiled_truth: "Short bio.".into(),
            timeline_summary: vec!["2026-05-01 — joined Acme".into()],
            aux_json: serde_json::json!({"backlink_count": 7}),
        };
        let p = RealLintAnalyzer::build_user_prompt(&candidate);
        assert!(p.contains("check_kind: hub_stub"));
        assert!(p.contains("page_title: Alice"));
        assert!(p.contains("page_node_id: n42"));
        assert!(p.contains("Short bio."));
        assert!(p.contains("joined Acme"));
        assert!(p.contains("backlink_count"));
    }

    #[test]
    fn parse_lint_response_handles_clean_json() {
        let resp = r#"{"verdict":"report","severity":"warn","message":"Hub is stubby"}"#;
        let parsed = parse_lint_response(resp);
        match parsed {
            LintResponseParse::Report { severity, message, contradiction } => {
                assert_eq!(severity, "warn");
                assert_eq!(message, "Hub is stubby");
                assert!(contradiction.is_none());
            }
            _ => panic!("expected Report, got {:?}", parsed),
        }
    }

    #[test]
    fn parse_lint_response_handles_skip() {
        let resp = r#"{"verdict":"skip","severity":"info","message":"actually fine"}"#;
        assert_eq!(parse_lint_response(resp), LintResponseParse::Skip);
    }

    #[test]
    fn parse_lint_response_extracts_contradiction_pair() {
        let resp = r#"{"verdict":"report","severity":"warn","message":"Two conflicting claims about Acme.","contradiction":{"claim_a":"Alice works at Acme","claim_b":"Alice works at Beta"}}"#;
        match parse_lint_response(resp) {
            LintResponseParse::Report { contradiction: Some((a, b)), .. } => {
                assert_eq!(a, "Alice works at Acme");
                assert_eq!(b, "Alice works at Beta");
            }
            other => panic!("expected Report with contradiction, got {:?}", other),
        }
    }

    #[test]
    fn parse_lint_response_tolerates_surrounding_prose() {
        let resp = "Here's my analysis:\n{\"verdict\":\"report\",\"severity\":\"info\",\"message\":\"ok\"}\nDone.";
        match parse_lint_response(resp) {
            LintResponseParse::Report { severity, .. } => assert_eq!(severity, "info"),
            other => panic!("expected Report through surrounding prose, got {:?}", other),
        }
    }

    #[test]
    fn parse_lint_response_defaults_invalid_severity_to_warn() {
        let resp = r#"{"verdict":"report","severity":"critical","message":"x"}"#;
        match parse_lint_response(resp) {
            LintResponseParse::Report { severity, .. } => assert_eq!(severity, "warn"),
            other => panic!("expected Report with default severity, got {:?}", other),
        }
    }

    #[test]
    fn parse_lint_response_handles_malformed_text() {
        let parsed = parse_lint_response("not json at all");
        match parsed {
            LintResponseParse::Unparseable(s) => assert!(s.contains("not json")),
            other => panic!("expected Unparseable, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn real_lint_returns_finding_with_token_cost() {
        use crate::memory_graph::memory_os_llm::MockMemoryOsLlm;
        let mock = Arc::new(MockMemoryOsLlm {
            canned_text:
                r#"{"verdict":"report","severity":"warn","message":"Hub 'Alice' needs enrichment."}"#
                    .into(),
            canned_input_tokens: 200,
            canned_output_tokens: 60,
            canned_model: "mock:claude-haiku-test".into(),
        });
        let analyzer = RealLintAnalyzer::new(mock);
        let candidate = LintCandidate {
            check_kind: LintCheckKind::HubStub,
            node_id: "n1".into(),
            title: "Alice".into(),
            compiled_truth: "Short.".into(),
            timeline_summary: vec![],
            aux_json: serde_json::json!({"backlink_count": 6}),
        };
        let out = analyzer.analyze(candidate).await.unwrap();
        assert_eq!(out.severity, "warn");
        assert!(out.message.contains("Alice"));
        assert_eq!(out.token_cost, 200 + 60);
        assert!(out.contradiction.is_none(), "hub_stub doesn't carry contradiction");
        assert_eq!(out.llm_model.as_deref(), Some("mock:claude-haiku-test"));
    }

    #[tokio::test]
    async fn real_lint_populates_contradiction_for_contradiction_kind() {
        use crate::memory_graph::memory_os_llm::MockMemoryOsLlm;
        let mock = Arc::new(MockMemoryOsLlm {
            canned_text: r#"{"verdict":"report","severity":"warn","message":"Conflicting employer claims.","contradiction":{"claim_a":"Alice at Acme","claim_b":"Alice at Beta"}}"#.into(),
            canned_input_tokens: 250,
            canned_output_tokens: 90,
            canned_model: "mock:sonnet".into(),
        });
        let analyzer = RealLintAnalyzer::new(mock);
        let candidate = LintCandidate {
            check_kind: LintCheckKind::Contradiction,
            node_id: "n-alice".into(),
            title: "Alice".into(),
            compiled_truth: "Senior engineer.".into(),
            timeline_summary: vec![
                "2026-05-01 — works at Acme".into(),
                "2026-05-15 — works at Beta".into(),
            ],
            aux_json: serde_json::json!({"entries": 2}),
        };
        let out = analyzer.analyze(candidate).await.unwrap();
        let c = out.contradiction.expect("contradiction kind must produce a Contradiction");
        assert_eq!(c.claim_a, "Alice at Acme");
        assert_eq!(c.claim_b, "Alice at Beta");
        assert_eq!(c.between_source_ids, vec!["n-alice"]);
        assert!(!c.noticed_at.is_empty());
    }

    #[tokio::test]
    async fn real_lint_skips_when_llm_says_skip() {
        use crate::memory_graph::memory_os_llm::MockMemoryOsLlm;
        let mock = Arc::new(MockMemoryOsLlm {
            canned_text: r#"{"verdict":"skip","severity":"info","message":"actually fine"}"#.into(),
            canned_input_tokens: 100,
            canned_output_tokens: 20,
            canned_model: "mock".into(),
        });
        let analyzer = RealLintAnalyzer::new(mock);
        let candidate = LintCandidate {
            check_kind: LintCheckKind::StaleSummary,
            node_id: "n1".into(),
            title: "RAG".into(),
            compiled_truth: "Retrieval-augmented generation.".into(),
            timeline_summary: vec![],
            aux_json: serde_json::json!({}),
        };
        // `skip` → caller receives `LintAnalysisError::Disabled`, which
        // run_lint_checks interprets as "drop this candidate, do not
        // insert a finding".
        let err = analyzer.analyze(candidate).await.unwrap_err();
        match err {
            LintAnalysisError::Disabled => {}
            other => panic!("expected Disabled for skip verdict, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn real_lint_falls_back_to_raw_text_on_parse_failure() {
        use crate::memory_graph::memory_os_llm::MockMemoryOsLlm;
        let mock = Arc::new(MockMemoryOsLlm {
            canned_text: "I'm a bad LLM and I refuse to use JSON.".into(),
            canned_input_tokens: 50,
            canned_output_tokens: 30,
            canned_model: "mock".into(),
        });
        let analyzer = RealLintAnalyzer::new(mock);
        let candidate = LintCandidate {
            check_kind: LintCheckKind::HubStub,
            node_id: "n1".into(),
            title: "X".into(),
            compiled_truth: "x".into(),
            timeline_summary: vec![],
            aux_json: serde_json::json!({}),
        };
        let out = analyzer.analyze(candidate).await.unwrap();
        assert_eq!(out.severity, "warn", "fallback uses warn severity");
        assert!(
            out.message.contains("bad LLM"),
            "fallback surfaces raw text; got: {}",
            out.message
        );
    }

    #[tokio::test]
    async fn real_lint_descriptor_marks_as_real() {
        use crate::memory_graph::memory_os_llm::MockMemoryOsLlm;
        let analyzer = RealLintAnalyzer::new(Arc::new(MockMemoryOsLlm::default()));
        assert_eq!(analyzer.descriptor(), "real:memory_os_llm");
    }

    #[test]
    fn truncate_helper_handles_unicode_safely() {
        assert_eq!(truncate("hello", 10), "hello");
        let cut = truncate("一二三四五六七八九十", 5);
        // 5 chars including the ellipsis → 4 source chars + '…'
        assert_eq!(cut.chars().count(), 5);
        assert!(cut.ends_with('…'));
    }
}
