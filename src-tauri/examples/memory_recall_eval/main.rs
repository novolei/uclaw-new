//! Memory recall eval — how often does the memory a question needs reach the
//! prompt? Ported from uclaw-pi (`examples/memory_recall_eval`), adapted to
//! this repo's recall stack.
//!
//! ```text
//! cargo run --example memory_recall_eval -- --verbose --out /tmp/recall.json
//! ```
//!
//! It snapshots `<data-dir>/uclaw.db` and `<data-dir>/bucket_seal/chunks.db`
//! with `VACUUM INTO` (live data untouched; migrations run on the snapshot
//! only) and scores every golden question against:
//!
//! - `graph_fts`          `MemoryGraphStore::fts_search` (trigram FTS)
//! - `graph_plan`         `MemoryRecallEngine::build_recall_plan` (triggered+relevant+expanded)
//! - `bucket_seal_fts`    `BucketSealAdapter::recall` (chunk FTS)
//! - `bucket_seal_hybrid` `BucketSealAdapter::recall_hybrid` (summaries + FTS)
//! - `memory_core`        segmented BM25 (`uclaw_core::memory_core`), when built
//! - prompt presence: what the agent path and the chat path actually inject.
//!
//! The golden set holds personal facts, so it lives outside the repo
//! (default `<data-dir>/eval/memory_recall_golden.jsonl`);
//! `golden.sample.jsonl` shows the format.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use serde::Serialize;

use uclaw_core::agent::memory_recall_block::{render_recall_block, BUCKET_SEAL_RECALL_MARKER};
use uclaw_core::eval::recall::{group_stats, parse_golden, GoldenItem, Matcher, RankedResult};
use uclaw_core::memory_adapter::{MemoryAdapter, RecallOpts};
use uclaw_core::memory_bucket_seal::score::embed::{build_embedder, Embedder, InertEmbedder};
use uclaw_core::memory_bucket_seal::store::BucketSealStore;
use uclaw_core::memory_bucket_seal::tree_source::{InertSummariser, Summariser};
use uclaw_core::memory_bucket_seal::BucketSealAdapter;
use uclaw_core::memory_graph::recall::{MemoryRecallConfig, MemoryRecallEngine};
use uclaw_core::memory_graph::store::MemoryGraphStore;

const K: usize = 10;
const SPACE_ID: &str = "default";
const BACKENDS: [&str; 4] = ["graph_fts", "graph_plan", "bucket_seal_fts", "bucket_seal_hybrid"];

struct Args {
    data_dir: PathBuf,
    golden: Option<PathBuf>,
    out: Option<PathBuf>,
    verbose: bool,
    dump_context: bool,
    no_embed: bool,
}

fn parse_args() -> Result<Args> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let mut a = Args {
        data_dir: PathBuf::from(home).join(".uclaw"),
        golden: None,
        out: None,
        verbose: false,
        dump_context: false,
        no_embed: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--data-dir" => a.data_dir = PathBuf::from(it.next().context("--data-dir needs a value")?),
            "--golden" => a.golden = Some(PathBuf::from(it.next().context("--golden needs a value")?)),
            "--out" => a.out = Some(PathBuf::from(it.next().context("--out needs a value")?)),
            "--verbose" | "-v" => a.verbose = true,
            "--dump-context" => a.dump_context = true,
            "--no-embed" => a.no_embed = true,
            other => bail!("unknown flag {other} (see the header of examples/memory_recall_eval/main.rs)"),
        }
    }
    Ok(a)
}

/// Read-only `VACUUM INTO` copy of a live SQLite DB.
fn snapshot(src: &Path, dst: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open {}", src.display()))?;
    conn.execute("VACUUM INTO ?1", [dst.to_string_lossy()])
        .with_context(|| format!("VACUUM INTO {}", dst.display()))?;
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize)]
struct BackendResult {
    first_hit_rank: Option<usize>,
    returned: usize,
    latency_ms: f64,
    error: Option<String>,
    top: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct QueryReport {
    id: String,
    lang: String,
    category: String,
    query: String,
    stored_in_graph: bool,
    stored_in_chunks: bool,
    backends: BTreeMap<String, BackendResult>,
    prompt_agent_hit: bool,
    prompt_agent_chars: usize,
    prompt_chat_hit: bool,
    prompt_chat_chars: usize,
}

fn snippet(s: &str) -> String {
    s.chars().take(90).collect::<String>().replace('\n', " ")
}

fn ranked(texts: &[String], m: &Matcher, latency_ms: f64) -> BackendResult {
    BackendResult {
        first_hit_rank: texts.iter().take(K).position(|t| m.hits(t)).map(|i| i + 1),
        returned: texts.len(),
        latency_ms,
        error: None,
        top: texts.iter().take(3).map(|t| snippet(t)).collect(),
    }
}

fn failed(e: impl std::fmt::Display, latency_ms: f64) -> BackendResult {
    BackendResult { latency_ms, error: Some(e.to_string()), ..Default::default() }
}

/// node_id → title + active content (what a graph hit "contains").
fn graph_corpus(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT n.id, n.title,
                COALESCE((SELECT group_concat(v.content, '\n') FROM memory_versions v
                           WHERE v.node_id = n.id AND v.status = 'active'), '')
           FROM memory_nodes n",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, format!("{}\n{}", r.get::<_, String>(1)?, r.get::<_, String>(2)?))))?
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(rows)
}

/// chunk id → full body (file under content_root, else the SQL preview).
fn chunk_corpus(chunks: &Connection, content_root: &Path) -> Result<HashMap<String, String>> {
    let mut stmt = chunks.prepare("SELECT id, source_id, content FROM mem_tree_chunks")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(id, source_id, preview)| {
            let path = uclaw_core::memory_bucket_seal::paths::chunk_abs_path(content_root, "document", &source_id, &id);
            let body = std::fs::read_to_string(path).unwrap_or(preview);
            (id, body)
        })
        .collect())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args()?;
    let golden_path = args.golden.clone().unwrap_or_else(|| args.data_dir.join("eval/memory_recall_golden.jsonl"));
    let golden_text = std::fs::read_to_string(&golden_path)
        .with_context(|| format!("read golden set {} (see golden.sample.jsonl)", golden_path.display()))?;
    let golden = parse_golden(&golden_text).map_err(|e| anyhow::anyhow!("{}: {e}", golden_path.display()))?;
    if golden.is_empty() {
        bail!("{} has no questions", golden_path.display());
    }

    let tmp = tempfile::tempdir()?;
    let main_db = tmp.path().join("uclaw.db");
    let chunks_db = tmp.path().join("chunks.db");
    snapshot(&args.data_dir.join("uclaw.db"), &main_db)?;
    snapshot(&args.data_dir.join("bucket_seal/chunks.db"), &chunks_db)?;
    let content_root = args.data_dir.join("bucket_seal/content");

    let conn = Connection::open(&main_db)?;
    uclaw_core::db::migrations::run(&conn)?;
    let graph_texts = graph_corpus(&conn)?;
    let db = Arc::new(Mutex::new(conn));
    let graph = Arc::new(MemoryGraphStore::new(Arc::clone(&db)));

    let bs_store = Arc::new(BucketSealStore::open(&chunks_db)?);
    bs_store.ensure_schema()?;
    let chunk_texts = chunk_corpus(&Connection::open(&chunks_db)?, &content_root)?;
    let cfg = uclaw_core::memubot_config::MemubotConfig::load(&args.data_dir);
    let model_ready = args.data_dir.join("models/bge-small-en-v1.5").exists();
    let embedder: Arc<dyn Embedder> = if !args.no_embed && model_ready {
        build_embedder(&cfg.embedding_endpoint, &args.data_dir)
    } else {
        Arc::new(InertEmbedder::new())
    };
    let summariser: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
    let bs = Arc::new(BucketSealAdapter::new(bs_store, content_root, embedder, summariser));
    let engine = MemoryRecallEngine::new(Arc::clone(&graph), Some(Arc::clone(&bs)), MemoryRecallConfig::default());

    eprintln!(
        "golden: {} questions · graph nodes: {} · chunks: {} · embedder: {}",
        golden.len(),
        graph_texts.len(),
        chunk_texts.len(),
        if !args.no_embed && model_ready { "bge-small-en-v1.5" } else { "inert" }
    );

    let mut reports = Vec::new();
    for item in &golden {
        reports.push(run_one(item, &args, &graph, &engine, &bs, &graph_texts, &chunk_texts).await);
    }

    print_summary(&reports);
    if let Some(out) = &args.out {
        std::fs::write(out, serde_json::to_string_pretty(&reports)?)?;
        eprintln!("wrote {}", out.display());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_one(
    item: &GoldenItem,
    args: &Args,
    graph: &Arc<MemoryGraphStore>,
    engine: &MemoryRecallEngine,
    bs: &Arc<BucketSealAdapter>,
    graph_texts: &HashMap<String, String>,
    chunk_texts: &HashMap<String, String>,
) -> QueryReport {
    let m = Matcher::new(item);
    let q = item.query.as_str();
    let mut backends = BTreeMap::new();
    let graph_text = |id: &str, fallback: String| graph_texts.get(id).cloned().unwrap_or(fallback);
    let chunk_text = |id: &str, fallback: String| chunk_texts.get(id).cloned().unwrap_or(fallback);

    let t = Instant::now();
    let r = match graph.fts_search(SPACE_ID, q, K) {
        Ok(hits) => {
            let texts: Vec<String> =
                hits.into_iter().map(|h| graph_text(&h.node_id, format!("{}\n{}", h.title, h.content_snippet))).collect();
            ranked(&texts, &m, ms(t))
        }
        Err(e) => failed(e, ms(t)),
    };
    backends.insert("graph_fts".to_string(), r);

    let t = Instant::now();
    let plan = engine.build_recall_plan(SPACE_ID, q, false).await;
    let r = match &plan {
        Ok(p) => {
            let texts: Vec<String> = p
                .triggered
                .iter()
                .chain(&p.relevant)
                .chain(&p.expanded)
                .map(|c| graph_text(&c.node_id, format!("{}\n{}", c.title, c.content)))
                .collect();
            ranked(&texts, &m, ms(t))
        }
        Err(e) => failed(e, ms(t)),
    };
    backends.insert("graph_plan".to_string(), r);

    let t = Instant::now();
    let opts = RecallOpts { namespace: None, category: None, session_id: None, min_score: None };
    let r = match bs.recall(q, K, opts).await {
        Ok(entries) => {
            let texts: Vec<String> = entries.into_iter().map(|e| chunk_text(&e.id, e.content)).collect();
            ranked(&texts, &m, ms(t))
        }
        Err(e) => failed(e, ms(t)),
    };
    backends.insert("bucket_seal_fts".to_string(), r);

    let t = Instant::now();
    let hybrid = bs.recall_hybrid(q, None, K).await;
    let texts: Vec<String> = hybrid.iter().map(|e| chunk_text(&e.id, e.content.clone())).collect();
    backends.insert("bucket_seal_hybrid".to_string(), ranked(&texts, &m, ms(t)));

    // Agent path: graph plan block + bucket_seal block (limit 6), as
    // send_agent_message composes them.
    let mut agent_ctx = String::new();
    if let Ok(p) = &plan {
        if p.boot.len() + p.triggered.len() + p.relevant.len() + p.expanded.len() + p.recent.len() > 0 {
            agent_ctx.push_str(&MemoryRecallEngine::format_recall_for_prompt(p, MemoryRecallConfig::default().token_budget));
        }
    }
    if let Some(block) = render_recall_block(BUCKET_SEAL_RECALL_MARKER, &hybrid.iter().take(6).cloned().collect::<Vec<_>>(), 1500) {
        agent_ctx.push_str("\n\n");
        agent_ctx.push_str(&block);
    }

    // Chat path (unified load_context, the default).
    let chat_ctx = uclaw_core::memory_adapter::load_context(&HashMap::new(), "bucket_seal", Some(bs), false, q, 8000, vec![]).await;

    if args.dump_context {
        eprintln!("──── {} agent ctx ────\n{agent_ctx}\n──── {} chat ctx ────\n{chat_ctx}", item.id, item.id);
    }
    let report = QueryReport {
        id: item.id.clone(),
        lang: item.lang.clone(),
        category: item.category.clone(),
        query: item.query.clone(),
        stored_in_graph: graph_texts.values().any(|t| m.hits(t)),
        stored_in_chunks: chunk_texts.values().any(|t| m.hits(t)),
        backends,
        prompt_agent_hit: m.hits(&agent_ctx),
        prompt_agent_chars: agent_ctx.chars().count(),
        prompt_chat_hit: m.hits(&chat_ctx),
        prompt_chat_chars: chat_ctx.chars().count(),
    };
    if args.verbose {
        let ranks: Vec<String> = BACKENDS
            .iter()
            .map(|b| format!("{b}={}", report.backends.get(*b).and_then(|r| r.first_hit_rank).map_or("-".into(), |r| r.to_string())))
            .collect();
        eprintln!(
            "[{}] {} | {} | agent={} chat={}",
            item.lang,
            item.id,
            ranks.join(" "),
            report.prompt_agent_hit,
            report.prompt_chat_hit
        );
    }
    report
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0)
}

fn groups(reports: &[QueryReport]) -> Vec<(String, Vec<&QueryReport>)> {
    let mut out = vec![("all".to_string(), reports.iter().collect::<Vec<_>>())];
    let mut by: BTreeMap<String, Vec<&QueryReport>> = BTreeMap::new();
    for r in reports {
        by.entry(format!("lang={}", r.lang)).or_default().push(r);
        if !r.category.is_empty() {
            by.entry(format!("cat={}", r.category)).or_default().push(r);
        }
    }
    out.extend(by);
    out
}

fn print_summary(reports: &[QueryReport]) {
    let mut names: Vec<String> = BACKENDS.iter().map(|s| s.to_string()).collect();
    for r in reports {
        for k in r.backends.keys() {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
    }
    println!("## Ranked recall (expected fact in top-k)\n");
    println!("| group | n | backend | hit@1 | hit@3 | hit@5 | hit@10 | MRR | errors | p50 ms |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for (g, rs) in groups(reports) {
        for b in &names {
            let results: Vec<RankedResult> = rs
                .iter()
                .filter_map(|r| r.backends.get(b))
                .map(|x| RankedResult { first_hit_rank: x.first_hit_rank, latency_ms: x.latency_ms })
                .collect();
            if results.is_empty() {
                continue;
            }
            let errors = rs.iter().filter_map(|r| r.backends.get(b)).filter(|x| x.error.is_some()).count();
            let s = group_stats(&results);
            println!(
                "| {g} | {} | {b} | {} | {} | {} | {} | {:.3} | {errors} | {:.1} |",
                s.n,
                pct(s.hit_at[0]),
                pct(s.hit_at[1]),
                pct(s.hit_at[2]),
                pct(s.hit_at[3]),
                s.mrr,
                s.p50_ms
            );
        }
    }
    println!("\n## What reaches the prompt\n");
    println!("| group | n | stored anywhere | agent prompt hit | chat prompt hit | agent hit / stored | avg agent chars | avg chat chars |");
    println!("|---|---|---|---|---|---|---|---|");
    for (g, rs) in groups(reports) {
        let n = rs.len() as f64;
        let stored = rs.iter().filter(|r| r.stored_in_graph || r.stored_in_chunks).count();
        let agent = rs.iter().filter(|r| r.prompt_agent_hit).count();
        let chat = rs.iter().filter(|r| r.prompt_chat_hit).count();
        let agent_stored = rs.iter().filter(|r| r.prompt_agent_hit && (r.stored_in_graph || r.stored_in_chunks)).count();
        println!(
            "| {g} | {} | {} | {} | {} | {} | {:.0} | {:.0} |",
            rs.len(),
            pct(stored as f64 / n),
            pct(agent as f64 / n),
            pct(chat as f64 / n),
            if stored == 0 { "-".to_string() } else { pct(agent_stored as f64 / stored as f64) },
            rs.iter().map(|r| r.prompt_agent_chars as f64).sum::<f64>() / n,
            rs.iter().map(|r| r.prompt_chat_chars as f64).sum::<f64>() / n,
        );
    }
}
