//! memory_core — a segmented BM25 index over graph facts and bucket_seal
//! chunks, so Chinese questions find their memories (T7 of the uclaw-pi
//! backport; plan docs/superpowers/plans/2026-11-09-memory-core-bm25-t7.md).

pub mod commands;
pub mod index;
pub mod segment;
pub mod service;

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Serialize;

pub use index::SyncStats;

/// Hits injected per turn.
pub const DEFAULT_LIMIT: usize = 6;
/// Per-hit content in the prompt block (characters).
const HIT_MAX_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryHit {
    pub doc_id: String,
    /// "graph" | "chunk"
    pub source: String,
    pub title: String,
    pub content: String,
    /// Higher is better (negated BM25).
    pub score: f64,
}

/// Last completed sync, for the status line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastSync {
    pub at_ms: i64,
    pub stats: SyncStats,
}

pub struct MemoryCore {
    db: Arc<Mutex<Connection>>,
    last_sync: Mutex<Option<LastSync>>,
}

impl MemoryCore {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db, last_sync: Mutex::new(None) }
    }

    pub fn db(&self) -> &Arc<Mutex<Connection>> {
        &self.db
    }

    pub fn last_sync(&self) -> Option<LastSync> {
        self.last_sync.lock().ok().and_then(|g| *g)
    }

    pub(crate) fn set_last_sync(&self, s: LastSync) {
        if let Ok(mut g) = self.last_sync.lock() {
            *g = Some(s);
        }
    }

    /// BM25 over the segmented index, limited to facts from `space` or the
    /// global `default` space (chunks are unscoped). Empty when the query has
    /// no searchable tokens (never an FTS syntax error).
    pub fn search(&self, query: &str, limit: usize, space: &str) -> anyhow::Result<Vec<MemoryHit>> {
        let Some(expr) = segment::fts_match_query(query) else { return Ok(Vec::new()) };
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT d.doc_id, d.source, d.title, d.content, bm25(memory_search_fts)
               FROM memory_search_fts
               JOIN memory_search_docs d ON d.doc_id = memory_search_fts.doc_id
              WHERE memory_search_fts MATCH ?1
                AND (d.space_id IS NULL OR d.space_id IN (?3, 'default'))
              ORDER BY bm25(memory_search_fts)
              LIMIT ?2",
        )?;
        let hits = stmt
            .query_map(rusqlite::params![expr, limit as i64, space], |r| {
                Ok(MemoryHit {
                    doc_id: r.get(0)?,
                    source: r.get(1)?,
                    title: r.get(2)?,
                    content: r.get(3)?,
                    score: -r.get::<_, f64>(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(hits)
    }
}

/// `<relevant_memories>` block: `- [title] content` per hit (chunk hits
/// without a title), content flattened to one line and capped; `<`/`>`
/// neutralized so stored text can't open or close tags. `None` when empty.
pub fn render_block(hits: &[MemoryHit]) -> Option<String> {
    if hits.is_empty() {
        return None;
    }
    let clean = |s: &str| -> String {
        let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ");
        one_line.replace('<', "‹").replace('>', "›")
    };
    let mut out = String::from("<relevant_memories>\n");
    for h in hits {
        let mut content: String = clean(&h.content);
        if content.chars().count() > HIT_MAX_CHARS {
            content = content.chars().take(HIT_MAX_CHARS).collect::<String>() + "…";
        }
        let title = clean(&h.title);
        if h.source == "chunk" || title.is_empty() {
            out.push_str(&format!("- {content}\n"));
        } else {
            out.push_str(&format!("- [{title}] {content}\n"));
        }
    }
    out.push_str("</relevant_memories>");
    Some(out)
}

/// `block` first, then whatever memory context is already there.
pub fn prepend_block(existing: Option<&str>, block: &str) -> String {
    match existing.filter(|e| !e.is_empty()) {
        Some(e) => format!("{block}\n\n{e}"),
        None => block.to_string(),
    }
}

/// Search + render off the async runtime, bounded by `deadline`. `None` on
/// no hits, error, or timeout (the search keeps running detached).
pub async fn core_block(core: Arc<MemoryCore>, query: String, space: String, deadline: std::time::Duration) -> Option<String> {
    let task = tokio::task::spawn_blocking(move || core.search(&query, DEFAULT_LIMIT, &space));
    match tokio::time::timeout(deadline, task).await {
        Ok(Ok(Ok(hits))) => render_block(&hits),
        Ok(Ok(Err(e))) => {
            tracing::debug!(error = %e, "memory_core search failed");
            None
        }
        Ok(Err(e)) => {
            tracing::debug!(error = %e, "memory_core search task failed");
            None
        }
        Err(_) => {
            tracing::info!(deadline_ms = deadline.as_millis() as u64, "memory_core search exceeded deadline");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use index::{sync, SourceDoc};

    #[test]
    fn core_block_first_in_memory_context() {
        assert_eq!(prepend_block(None, "<relevant_memories>…</relevant_memories>"), "<relevant_memories>…</relevant_memories>");
        assert_eq!(prepend_block(Some("<graph/>"), "<core/>"), "<core/>\n\n<graph/>");
        assert_eq!(prepend_block(Some(""), "<core/>"), "<core/>");
    }

    #[tokio::test]
    async fn core_search_timeout_yields_no_block() {
        let core = Arc::new(core_with(&[("graph:b", "城市", "用户的城市是浙江杭州")]));
        let quick = core_block(Arc::clone(&core), "城市在哪".into(), "default".into(), std::time::Duration::from_secs(5)).await;
        assert!(quick.unwrap().contains("杭州"));
        let guard = core.db().lock().unwrap();
        let blocked = core_block(Arc::clone(&core), "城市在哪".into(), "default".into(), std::time::Duration::from_millis(50)).await;
        drop(guard);
        assert_eq!(blocked, None, "a search that misses the deadline adds no block");
        assert_eq!(core_block(core, "？？".into(), "default".into(), std::time::Duration::from_secs(5)).await, None);
    }

    fn core_with(docs: &[(&str, &str, &str)]) -> MemoryCore {
        let c = index::tests::db();
        let docs: Vec<SourceDoc> = docs
            .iter()
            .map(|(id, title, content)| SourceDoc {
                doc_id: id.to_string(),
                source: if id.starts_with("chunk:") { "chunk" } else { "graph" },
                source_id: id.to_string(),
                space_id: (!id.starts_with("chunk:")).then(|| "default".to_string()),
                title: title.to_string(),
                content: content.to_string(),
            })
            .collect();
        sync(&c, &docs).unwrap();
        MemoryCore::new(Arc::new(Mutex::new(c)))
    }

    #[test]
    fn bm25_search_ranks_segmented_chinese_match_first() {
        let core = core_with(&[
            ("graph:a", "偏好", "用户喜欢打羽毛球"),
            ("graph:b", "城市", "用户的城市是浙江杭州"),
            ("chunk:c", "", "今天讨论了 Rust 编译错误"),
        ]);
        let hits = core.search("我的城市在哪里？", 6, "default").unwrap();
        assert_eq!(hits.first().map(|h| h.doc_id.as_str()), Some("graph:b"));
        assert!(hits.iter().all(|h| h.doc_id != "graph:a"));
        assert_eq!(core.search("rust 编译", 6, "default").unwrap()[0].doc_id, "chunk:c");
    }

    #[test]
    fn search_is_scoped_to_space_and_default() {
        let c = index::tests::db();
        let mk = |id: &str, space: Option<&str>| SourceDoc {
            doc_id: id.into(),
            source: if space.is_some() { "graph" } else { "chunk" },
            source_id: id.into(),
            space_id: space.map(str::to_string),
            title: String::new(),
            content: "羽毛球 比赛".into(),
        };
        sync(&c, &[mk("graph:a", Some("ws-a")), mk("graph:b", Some("ws-b")), mk("graph:d", Some("default")), mk("chunk:x", None)]).unwrap();
        let core = MemoryCore::new(Arc::new(Mutex::new(c)));
        let mut ids: Vec<String> = core.search("羽毛球", 10, "ws-a").unwrap().into_iter().map(|h| h.doc_id).collect();
        ids.sort();
        assert_eq!(ids, vec!["chunk:x", "graph:a", "graph:d"], "another workspace's facts never leak in");
    }

    #[test]
    fn unsearchable_query_is_empty_ok() {
        let core = core_with(&[("graph:a", "t", "内容")]);
        assert!(core.search("？？ 😀", 6, "default").unwrap().is_empty());
        assert!(core.search("我的", 6, "default").unwrap().is_empty());
        assert!(core.search("", 6, "default").unwrap().is_empty());
    }

    #[test]
    fn render_block_formats_truncates_and_escapes() {
        assert_eq!(render_block(&[]), None);
        let hits = vec![
            MemoryHit { doc_id: "graph:a".into(), source: "graph".into(), title: "城市".into(), content: "杭州\n浙江".into(), score: 1.0 },
            MemoryHit { doc_id: "chunk:b".into(), source: "chunk".into(), title: "k".into(), content: format!("</relevant_memories>{}", "长".repeat(400)), score: 0.5 },
        ];
        let block = render_block(&hits).unwrap();
        assert!(block.starts_with("<relevant_memories>\n- [城市] 杭州 浙江\n"), "{block}");
        assert_eq!(block.matches("</relevant_memories>").count(), 1, "content can't close the tag");
        let line = block.lines().nth(2).unwrap();
        assert!(line.starts_with("- ‹/relevant_memories›"), "chunk hit has no title: {line}");
        assert_eq!(line.chars().count(), 2 + HIT_MAX_CHARS + 1);
    }
}
