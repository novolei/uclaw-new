//! The growth passes: reflection (every N agent turns), promotion (user
//! model from facts), consolidation (merge reflections + ground the model).
//!
//! All passes go through the learning LLM under their own daily budget
//! (`memory_growth*` cost rows) and a single-flight guard. The DB mutex is
//! never held across an LLM await.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Serialize;

use super::{parse, prompts, render, store};
use crate::learning::cache::FacetCache;
use crate::memory_graph::memory_os_llm::MemoryOsLlm;

pub const COST_TAG_REFLECTION: &str = "memory_growth_reflection";
pub const COST_TAG_PROMOTION: &str = "memory_growth_promotion";
pub const COST_TAG_CONSOLIDATION: &str = "memory_growth_consolidation";
const REFLECTION_MAX_TOKENS: u32 = 2048;
const PROMOTION_MAX_TOKENS: u32 = 2048;
const CONSOLIDATION_MAX_TOKENS: u32 = 4096;
/// Messages read per reflection (the newest ones after the watermark).
const TRANSCRIPT_WINDOW: usize = 50;
/// Consolidate after every Nth reflection, once enough are live.
const CONSOLIDATE_EVERY_N_REFLECTIONS: i64 = 5;
const MIN_LIVE_TO_CONSOLIDATE: usize = 8;
const CONSOLIDATION_READ_CAP: usize = 40;
/// Promotion input caps.
const DIGEST_MAX_FACTS: usize = 60;
const DIGEST_MAX_CHARS: usize = 6000;
const DIGEST_LINE_MAX_CHARS: usize = 400;

/// What the passes need from `AppState` (cloned before spawning).
#[derive(Clone)]
pub struct GrowthDeps {
    pub db: Arc<Mutex<Connection>>,
    pub llm: Option<Arc<dyn MemoryOsLlm>>,
    pub facets: Arc<FacetCache>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrowthSettings {
    pub enabled: bool,
    pub every_n_turns: u32,
    pub daily_budget: u32,
}

impl GrowthSettings {
    pub fn from_config(c: &crate::memubot_config::MemoryOsConfig) -> Self {
        Self { enabled: c.growth_enabled, every_n_turns: c.growth_every_n_turns, daily_budget: c.growth_daily_token_budget }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum GrowthOutcome {
    /// `reason`: "disabled" | "no_llm" | "budget" | "not_due" | "busy" | "error"
    Skipped { reason: &'static str },
    Ran { reflection: bool, promoted: bool, consolidated: bool },
}

pub struct GrowthService {
    running: AtomicBool,
}

impl Default for GrowthService {
    fn default() -> Self {
        Self::new()
    }
}

impl GrowthService {
    pub fn new() -> Self {
        Self { running: AtomicBool::new(false) }
    }

    /// Turn-tail entry: run when ≥ `every_n_turns` new assistant turns
    /// arrived since the last reflection.
    pub async fn maybe_run(&self, deps: &GrowthDeps, s: &GrowthSettings) -> GrowthOutcome {
        let llm = match gate(deps, s) {
            Ok(l) => l,
            Err(reason) => return GrowthOutcome::Skipped { reason },
        };
        let due = {
            let Ok(conn) = deps.db.lock() else { return GrowthOutcome::Skipped { reason: "error" } };
            let wm = store::latest_watermark(&conn).unwrap_or(0);
            store::count_assistant_messages_since(&conn, wm).unwrap_or(0) >= i64::from(s.every_n_turns.max(1))
        };
        if !due {
            return GrowthOutcome::Skipped { reason: "not_due" };
        }
        let Some(_guard) = RunGuard::acquire(&self.running) else {
            return GrowthOutcome::Skipped { reason: "busy" };
        };
        let reflection = log_err("reflection", run_reflection(deps, &llm).await);
        let promoted = budget_ok(deps, s) && log_err("promotion", run_promotion(deps, &llm).await);
        let consolidated = reflection && consolidation_due(deps) && log_err("consolidation", run_consolidation(deps, s).await);
        GrowthOutcome::Ran { reflection, promoted, consolidated }
    }

    /// 立即刷新: reflect on any new turns, re-promote, consolidate when due —
    /// ignores the N-turn cadence but keeps every other gate.
    pub async fn run_now(&self, deps: &GrowthDeps, s: &GrowthSettings) -> GrowthOutcome {
        let llm = match gate(deps, s) {
            Ok(l) => l,
            Err(reason) => return GrowthOutcome::Skipped { reason },
        };
        let Some(_guard) = RunGuard::acquire(&self.running) else {
            return GrowthOutcome::Skipped { reason: "busy" };
        };
        let reflection = log_err("reflection", run_reflection(deps, &llm).await);
        let promoted = budget_ok(deps, s) && log_err("promotion", run_promotion(deps, &llm).await);
        let consolidated = budget_ok(deps, s) && log_err("consolidation", run_consolidation(deps, s).await);
        GrowthOutcome::Ran { reflection, promoted, consolidated }
    }
}

/// Holds the single-flight flag for one pass; released on drop.
struct RunGuard<'a>(&'a AtomicBool);

impl<'a> RunGuard<'a> {
    fn acquire(flag: &'a AtomicBool) -> Option<Self> {
        (!flag.swap(true, Ordering::AcqRel)).then_some(Self(flag))
    }
}

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn log_err(pass: &str, r: anyhow::Result<bool>) -> bool {
    r.unwrap_or_else(|e| {
        tracing::warn!(pass, error = %e, "memory growth pass failed");
        false
    })
}

fn budget_ok(deps: &GrowthDeps, s: &GrowthSettings) -> bool {
    s.daily_budget > 0 && crate::cost_store::today_growth_tokens(&deps.db) < s.daily_budget
}

/// Gates shared by every entry point; returns the LLM to use.
fn gate(deps: &GrowthDeps, s: &GrowthSettings) -> Result<Arc<dyn MemoryOsLlm>, &'static str> {
    if !s.enabled {
        return Err("disabled");
    }
    let llm = deps.llm.clone().ok_or("no_llm")?;
    if !budget_ok(deps, s) {
        return Err("budget");
    }
    Ok(llm)
}

fn consolidation_due(deps: &GrowthDeps) -> bool {
    let Ok(conn) = deps.db.lock() else { return false };
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM growth_reflections", [], |r| r.get(0)).unwrap_or(0);
    total > 0 && total % CONSOLIDATE_EVERY_N_REFLECTIONS == 0
}

/// Distill the messages after the watermark into one reflection.
async fn run_reflection(deps: &GrowthDeps, llm: &Arc<dyn MemoryOsLlm>) -> anyhow::Result<bool> {
    let msgs = {
        let conn = deps.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        let wm = store::latest_watermark(&conn)?;
        store::messages_since(&conn, wm, TRANSCRIPT_WINDOW)?
    };
    let Some(last) = msgs.last() else { return Ok(false) };
    let watermark = last.created_at;
    let transcript = render::render_transcript(&msgs);
    let out = llm
        .complete_text(COST_TAG_REFLECTION, prompts::REFLECTION_SYSTEM, &prompts::reflection_user(&transcript), REFLECTION_MAX_TOKENS)
        .await?;
    let Some((insight, confidence)) = parse::parse_reflection(&out.text) else { return Ok(false) };
    let conn = deps.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
    store::insert_reflection(&conn, &insight, confidence, msgs.len() as i64, watermark)?;
    tracing::info!(messages = msgs.len(), "memory growth: reflection stored");
    Ok(true)
}

/// Re-synthesize the user model from the profile digest.
async fn run_promotion(deps: &GrowthDeps, llm: &Arc<dyn MemoryOsLlm>) -> anyhow::Result<bool> {
    let digest = {
        let conn = deps.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        build_profile_digest(&conn, &deps.facets)
    };
    let Some(digest) = digest else { return Ok(false) };
    let out = llm
        .complete_text(COST_TAG_PROMOTION, prompts::PROMOTION_SYSTEM, &prompts::promotion_user(&digest), PROMOTION_MAX_TOKENS)
        .await?;
    let summary = out.text.trim();
    if summary.is_empty() {
        return Ok(false);
    }
    let conn = deps.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
    store::set_user_model(&conn, summary)?;
    Ok(true)
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}

/// Facts the user model may rest on: active facets + not-archived
/// user_profile/identity/value/directive nodes (active version text),
/// most-reinforced first. `None` when there is nothing.
pub fn build_profile_digest(conn: &Connection, facets: &FacetCache) -> Option<String> {
    let mut lines: Vec<String> = facets
        .all_active()
        .into_iter()
        .map(|f| format!("- [{}] {}: {}", f.class.as_str(), f.name, truncate_chars(&f.value, DIGEST_LINE_MAX_CHARS)))
        .collect();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT COALESCE(
                    (SELECT v.content FROM memory_versions v
                      WHERE v.node_id = n.id AND v.status = 'active'
                      ORDER BY v.created_at DESC LIMIT 1),
                    n.title)
           FROM memory_nodes n
          WHERE n.kind IN ('user_profile', 'identity', 'value', 'directive')
            AND n.archived_at IS NULL
          ORDER BY COALESCE(json_extract(n.metadata_json, '$.reinforced_count'), 0) DESC, n.updated_at DESC
          LIMIT ?1",
    ) {
        if let Ok(rows) = stmt.query_map([DIGEST_MAX_FACTS as i64], |r| r.get::<_, String>(0)) {
            lines.extend(
                rows.filter_map(Result::ok)
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .map(|t| format!("- {}", truncate_chars(&t, DIGEST_LINE_MAX_CHARS))),
            );
        }
    }
    let mut out = String::new();
    for line in lines.into_iter().take(DIGEST_MAX_FACTS) {
        if out.chars().count() + line.chars().count() + 1 > DIGEST_MAX_CHARS {
            break;
        }
        out.push_str(&line);
        out.push('\n');
    }
    (!out.is_empty()).then_some(out)
}

/// One consolidation pass over the newest live reflections. Archives only
/// the rows it read; rejected output leaves the database untouched.
pub(crate) async fn run_consolidation(deps: &GrowthDeps, s: &GrowthSettings) -> anyhow::Result<bool> {
    let Some(llm) = deps.llm.clone() else { return Ok(false) };
    if !budget_ok(deps, s) {
        return Ok(false);
    }
    let (live, user_model, digest) = {
        let conn = deps.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        let live = store::list_reflections(&conn, CONSOLIDATION_READ_CAP, false)?;
        if live.len() < MIN_LIVE_TO_CONSOLIDATE {
            return Ok(false);
        }
        let um = store::get_user_model(&conn)?.map(|m| m.summary);
        (live, um, build_profile_digest(&conn, &deps.facets))
    };
    let out = llm
        .complete_text(
            COST_TAG_CONSOLIDATION,
            prompts::CONSOLIDATION_SYSTEM,
            &prompts::consolidation_user(&live, user_model.as_deref(), digest.as_deref()),
            CONSOLIDATION_MAX_TOKENS,
        )
        .await?;
    let Some(result) = parse::parse_consolidation(&out.text) else { return Ok(false) };
    if !parse::consolidation_applies(live.len(), &result) {
        return Ok(false);
    }
    let mut conn = deps.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
    let tx = conn.transaction()?;
    let ids: Vec<String> = live.iter().map(|r| r.id.clone()).collect();
    store::archive_reflections(&tx, &ids)?;
    for (insight, confidence) in &result.reflections {
        store::insert_reflection(&tx, insight, *confidence, 0, 0)?;
    }
    if let Some(um) = &result.user_model {
        store::set_user_model(&tx, um)?;
    }
    tx.commit()?;
    tracing::info!(read = live.len(), kept = result.reflections.len(), "memory growth: consolidated");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::growth::store::tests::{add_message, db as test_db};
    use crate::memory_graph::memory_os_llm::{MemoryOsLlmError, MemoryOsLlmOutput};
    use std::collections::HashMap;

    /// Replies per cost tag; records every call's tag.
    struct ScriptedLlm {
        replies: HashMap<&'static str, String>,
        calls: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl MemoryOsLlm for ScriptedLlm {
        async fn complete_text(&self, cost_tag: &str, _s: &str, _u: &str, _m: u32) -> Result<MemoryOsLlmOutput, MemoryOsLlmError> {
            self.calls.lock().unwrap().push(cost_tag.to_string());
            let text = self.replies.get(cost_tag).cloned().unwrap_or_default();
            Ok(MemoryOsLlmOutput { text, input_tokens: 10, output_tokens: 5, model: "m".into() })
        }
        fn descriptor(&self) -> String {
            "scripted".into()
        }
    }

    fn llm(replies: &[(&'static str, &str)]) -> Arc<ScriptedLlm> {
        Arc::new(ScriptedLlm {
            replies: replies.iter().map(|(k, v)| (*k, v.to_string())).collect(),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn deps(conn: Connection, llm: Option<Arc<ScriptedLlm>>) -> GrowthDeps {
        GrowthDeps {
            db: Arc::new(Mutex::new(conn)),
            llm: llm.map(|l| l as Arc<dyn MemoryOsLlm>),
            facets: Arc::new(FacetCache::new()),
        }
    }

    fn settings(n: u32) -> GrowthSettings {
        GrowthSettings { enabled: true, every_n_turns: n, daily_budget: 20_000 }
    }

    fn seed_turns(conn: &Connection, turns: i64, start: i64) {
        for i in 0..turns {
            add_message(conn, "user", &format!("question {i}"), start + i * 10);
            add_message(conn, "assistant", &format!("answer {i}"), start + i * 10 + 1);
        }
    }

    fn seed_fact(conn: &Connection, kind: &str, text: &str) {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title, created_at, updated_at) VALUES (?1, 'default', ?2, ?3, '2026-01-01', '2026-01-01')",
            rusqlite::params![id, kind, text],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_versions (id, node_id, status, content, created_at) VALUES (?1, ?2, 'active', ?3, '2026-01-01')",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), id, text],
        )
        .unwrap();
    }

    const REFLECTION_JSON: &str = r#"{"insight":"Prefers small PRs","confidence":0.8}"#;

    #[tokio::test]
    async fn no_llm_or_disabled_or_over_budget_does_nothing() {
        let svc = GrowthService::new();
        let c = test_db();
        seed_turns(&c, 30, 1000);
        let d = deps(c, None);
        assert_eq!(svc.maybe_run(&d, &settings(20)).await, GrowthOutcome::Skipped { reason: "no_llm" });

        let c = test_db();
        seed_turns(&c, 30, 1000);
        let l = llm(&[(COST_TAG_REFLECTION, REFLECTION_JSON)]);
        let d = deps(c, Some(l.clone()));
        let off = GrowthSettings { enabled: false, ..settings(20) };
        assert_eq!(svc.maybe_run(&d, &off).await, GrowthOutcome::Skipped { reason: "disabled" });
        let zero = GrowthSettings { daily_budget: 0, ..settings(20) };
        assert_eq!(svc.maybe_run(&d, &zero).await, GrowthOutcome::Skipped { reason: "budget" });
        assert!(l.calls.lock().unwrap().is_empty(), "no LLM call when gated");
        let conn = d.db.lock().unwrap();
        assert!(store::list_reflections(&conn, 10, true).unwrap().is_empty());
    }

    #[tokio::test]
    async fn below_n_new_turns_skips() {
        let svc = GrowthService::new();
        let c = test_db();
        seed_turns(&c, 19, 1000);
        let l = llm(&[(COST_TAG_REFLECTION, REFLECTION_JSON)]);
        let d = deps(c, Some(l.clone()));
        assert_eq!(svc.maybe_run(&d, &settings(20)).await, GrowthOutcome::Skipped { reason: "not_due" });
        assert!(l.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn n_turns_creates_reflection_with_watermark_then_promotes() {
        let svc = GrowthService::new();
        let c = test_db();
        seed_turns(&c, 20, 1000);
        seed_fact(&c, "user_profile", "Speaks Chinese");
        let l = llm(&[(COST_TAG_REFLECTION, REFLECTION_JSON), (COST_TAG_PROMOTION, "A Chinese-speaking engineer who ships small PRs.")]);
        let d = deps(c, Some(l.clone()));
        let out = svc.maybe_run(&d, &settings(20)).await;
        assert_eq!(out, GrowthOutcome::Ran { reflection: true, promoted: true, consolidated: false });
        {
            let conn = d.db.lock().unwrap();
            let rows = store::list_reflections(&conn, 10, false).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].insight, "Prefers small PRs");
            assert_eq!(store::latest_watermark(&conn).unwrap(), 1000 + 19 * 10 + 1, "watermark = newest message read");
            assert_eq!(store::get_user_model(&conn).unwrap().unwrap().summary, "A Chinese-speaking engineer who ships small PRs.");
        }
        assert_eq!(*l.calls.lock().unwrap(), vec![COST_TAG_REFLECTION.to_string(), COST_TAG_PROMOTION.to_string()]);
        // Nothing new since the watermark → not due.
        assert_eq!(svc.maybe_run(&d, &settings(20)).await, GrowthOutcome::Skipped { reason: "not_due" });
    }

    #[tokio::test]
    async fn concurrent_trigger_is_dropped() {
        let svc = GrowthService::new();
        let c = test_db();
        seed_turns(&c, 20, 1000);
        let l = llm(&[(COST_TAG_REFLECTION, REFLECTION_JSON)]);
        let d = deps(c, Some(l.clone()));
        svc.running.store(true, Ordering::SeqCst);
        assert_eq!(svc.maybe_run(&d, &settings(20)).await, GrowthOutcome::Skipped { reason: "busy" });
        assert!(l.calls.lock().unwrap().is_empty());
        svc.running.store(false, Ordering::SeqCst);
        assert!(matches!(svc.maybe_run(&d, &settings(20)).await, GrowthOutcome::Ran { .. }));
        assert!(!svc.running.load(Ordering::SeqCst), "guard released after a run");
    }

    #[tokio::test]
    async fn consolidation_archives_only_rows_it_read() {
        let c = test_db();
        for i in 0..41 {
            store::insert_reflection(&c, &format!("r{i}"), 0.5, 1, i).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let oldest = store::list_reflections(&c, 100, false).unwrap().last().unwrap().id.clone();
        let l = llm(&[(COST_TAG_CONSOLIDATION, r#"{"reflections":[{"insight":"merged","confidence":0.9}],"user_model":"grounded"}"#)]);
        let d = deps(c, Some(l));
        assert!(run_consolidation(&d, &settings(20)).await.unwrap());
        let conn = d.db.lock().unwrap();
        let live = store::list_reflections(&conn, 100, false).unwrap();
        let ids: Vec<&str> = live.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(live.len(), 2, "merged + the one beyond the read cap");
        assert!(ids.contains(&oldest.as_str()), "unread row stays live");
        assert!(live.iter().any(|r| r.insight == "merged"));
        assert_eq!(store::get_user_model(&conn).unwrap().unwrap().summary, "grounded");
    }

    #[tokio::test]
    async fn consolidation_rejected_output_leaves_db_untouched() {
        for reply in ["not json", r#"{"reflections":[]}"#] {
            let c = test_db();
            for i in 0..8 {
                store::insert_reflection(&c, &format!("r{i}"), 0.5, 1, i).unwrap();
            }
            let l = llm(&[(COST_TAG_CONSOLIDATION, reply)]);
            let d = deps(c, Some(l));
            assert!(!run_consolidation(&d, &settings(20)).await.unwrap());
            let conn = d.db.lock().unwrap();
            assert_eq!(store::list_reflections(&conn, 100, false).unwrap().len(), 8);
        }
        // Fewer than the minimum live rows: no LLM call at all.
        let c = test_db();
        store::insert_reflection(&c, "only", 0.5, 1, 1).unwrap();
        let l = llm(&[(COST_TAG_CONSOLIDATION, "x")]);
        let d = deps(c, Some(l.clone()));
        assert!(!run_consolidation(&d, &settings(20)).await.unwrap());
        assert!(l.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn growth_spend_counts_against_budget() {
        let c = test_db();
        c.execute(
            "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, created_at) VALUES ('c1', 'memory_os', 'memory_growth_reflection:m', 300, 200, ?1)",
            [chrono::Utc::now().timestamp_millis()],
        )
        .unwrap();
        c.execute(
            "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, created_at) VALUES ('c2', 'memory_os', 'memory_learning:m', 999, 0, ?1)",
            [chrono::Utc::now().timestamp_millis()],
        )
        .unwrap();
        seed_turns(&c, 20, 1000);
        let l = llm(&[(COST_TAG_REFLECTION, REFLECTION_JSON)]);
        let d = deps(c, Some(l.clone()));
        assert_eq!(crate::cost_store::today_growth_tokens(&d.db), 500);
        let tight = GrowthSettings { daily_budget: 500, ..settings(20) };
        assert_eq!(GrowthService::new().maybe_run(&d, &tight).await, GrowthOutcome::Skipped { reason: "budget" });
        assert!(l.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn profile_digest_reads_facts_and_is_none_when_empty() {
        let c = test_db();
        assert_eq!(build_profile_digest(&c, &FacetCache::new()), None);
        seed_fact(&c, "user_profile", "Uses Rust and Tauri");
        seed_fact(&c, "episode", "Fixed a bug yesterday");
        let digest = build_profile_digest(&c, &FacetCache::new()).unwrap();
        assert!(digest.contains("Uses Rust and Tauri"));
        assert!(!digest.contains("Fixed a bug"), "episodes are not profile facts");
    }
}
