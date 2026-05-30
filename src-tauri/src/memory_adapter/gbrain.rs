// SPDX-License-Identifier: Apache-2.0
//! `GbrainAdapter` — wraps gbrain (page-oriented knowledge-graph MCP server)
//! behind the `MemoryAdapter` trait. Marshalling layer over
//! `crate::gbrain::browse::*`; slug = `{namespace}/{key}`; delete/clear are
//! graceful no-ops (gbrain has no delete tool).

use async_trait::async_trait;
use chrono::Utc;

use crate::gbrain::browse::{self, PageDetail, PageSummary, SearchHit};
use crate::mcp::SharedMcpManager;
use crate::memory_adapter::traits::MemoryAdapter;
use crate::memory_adapter::types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

const META_PREFIX: &str = "<!-- uclaw-meta:";

/// Build a gbrain page slug from a flat namespace + key.
fn slug_for(namespace: &str, key: &str) -> String {
    format!("{namespace}/{key}")
}

/// Recover (namespace, key) from a slug, splitting on the FIRST '/'.
/// No slash → (None, whole slug).
fn split_slug(slug: &str) -> (Option<String>, String) {
    match slug.split_once('/') {
        Some((ns, key)) => (Some(ns.to_string()), key.to_string()),
        None => (None, slug.to_string()),
    }
}

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

/// Prepend a recoverable meta marker to the page content.
// Edge case: if caller `content` itself begins with `<!-- uclaw-meta:`, a second marker is
// prepended and `parse_page_meta` strips only the outer one (inner remains as body). Accepted —
// real-world risk is negligible (callers should not embed raw HTML comment markers in content).
fn build_page_body(content: &str, category: &MemoryCategory, session: Option<&str>) -> String {
    format!(
        "{META_PREFIX} category={}; session={} -->\n\n{}",
        category_to_str(category),
        session.unwrap_or(""),
        content
    )
}

/// Parse the meta marker (if present) off the first line; return
/// (category, session, stripped_content). Defaults to Conversation/None
/// when the marker is absent.
fn parse_page_meta(text: &str) -> (MemoryCategory, Option<String>, String) {
    if let Some(first_line_end) = text.find('\n') {
        let first = &text[..first_line_end];
        if let Some(rest) = first.trim().strip_prefix(META_PREFIX) {
            // rest looks like ` category=core; session=abc -->`
            let inner = rest.trim_end_matches("-->").trim();
            let mut cat = MemoryCategory::Conversation;
            let mut session: Option<String> = None;
            for kv in inner.split(';') {
                let kv = kv.trim();
                if let Some(v) = kv.strip_prefix("category=") {
                    cat = category_from_str(v.trim());
                } else if let Some(v) = kv.strip_prefix("session=") {
                    let v = v.trim();
                    if !v.is_empty() {
                        session = Some(v.to_string());
                    }
                }
            }
            // Strip the marker line + one following blank line (the "\n\n" we prepend).
            let body = text[first_line_end + 1..]
                .trim_start_matches('\n')
                .to_string();
            return (cat, session, body);
        }
    }
    (MemoryCategory::Conversation, None, text.to_string())
}

fn search_hit_to_entry(hit: &SearchHit) -> MemoryEntry {
    let (namespace, key) = split_slug(&hit.slug);
    MemoryEntry {
        id: hit.slug.clone(),
        key,
        content: hit.snippet.clone(),
        namespace,
        category: MemoryCategory::Conversation, // search snippets carry no meta
        timestamp: Utc::now().to_rfc3339(),
        session_id: None,
        score: Some(hit.similarity),
    }
}

fn page_detail_to_entry(p: &PageDetail) -> MemoryEntry {
    let (namespace, key) = split_slug(&p.slug);
    // Prefer compiled_truth as it is the canonical body; fall back to raw_markdown.
    // Both fields preserve the HTML comment marker line written by build_page_body,
    // since gbrain stores content verbatim (no HTML stripping).
    let source = if !p.compiled_truth.is_empty() {
        &p.compiled_truth
    } else {
        &p.raw_markdown
    };
    let (category, session_id, content) = parse_page_meta(source);
    MemoryEntry {
        id: p.slug.clone(),
        key,
        content,
        namespace,
        category,
        timestamp: p.updated_at.clone().unwrap_or_else(|| Utc::now().to_rfc3339()),
        session_id,
        score: None,
    }
}

fn page_summary_to_entry(s: &PageSummary) -> MemoryEntry {
    let (namespace, key) = split_slug(&s.slug);
    MemoryEntry {
        id: s.slug.clone(),
        key,
        content: s.title.clone(),
        namespace,
        category: MemoryCategory::Conversation,
        timestamp: s.updated_at.clone().unwrap_or_else(|| Utc::now().to_rfc3339()),
        session_id: None,
        score: None,
    }
}

/// One NamespaceSummary per first-segment prefix: count + latest updated_at.
fn summaries_from_pages(pages: &[PageSummary]) -> Vec<NamespaceSummary> {
    use std::collections::BTreeMap;
    let mut acc: BTreeMap<String, (usize, Option<String>)> = BTreeMap::new();
    for p in pages {
        let (ns, _key) = split_slug(&p.slug);
        let Some(ns) = ns else { continue }; // skip namespace-less (agent) pages
        let entry = acc.entry(ns).or_insert((0, None));
        entry.0 += 1;
        // Track the lexicographically-latest RFC3339 updated_at (string sort
        // is correct for RFC3339).
        if let Some(ts) = &p.updated_at {
            if entry.1.as_deref().map(|cur| ts.as_str() > cur).unwrap_or(true) {
                entry.1 = Some(ts.clone());
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

// ─── GbrainAdapter ───────────────────────────────────────────────────────────

/// gbrain wrapped as a `MemoryAdapter`. Holds a clone of the app's MCP
/// manager; all ops delegate to `crate::gbrain::browse::*`.
pub struct GbrainAdapter {
    mcp: SharedMcpManager,
}

impl GbrainAdapter {
    pub fn new(mcp: SharedMcpManager) -> Self {
        Self { mcp }
    }
}

#[async_trait]
impl MemoryAdapter for GbrainAdapter {
    fn name(&self) -> &str {
        "gbrain"
    }

    async fn store(
        &self,
        namespace: &str,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let slug = slug_for(namespace, key);
        let body = build_page_body(content, &category, session_id);
        browse::put_page(&self.mcp, &slug, &body)
            .await
            .map_err(|e| anyhow::anyhow!("gbrain put_page: {}", e.to_command_string()))?;
        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let hits = browse::search(&self.mcp, query, limit as u32, 0)
            .await
            .map_err(|e| anyhow::anyhow!("gbrain search: {}", e.to_command_string()))?;
        let mut out = Vec::new();
        for hit in &hits {
            let entry = search_hit_to_entry(hit);
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
            if let Some(cat) = &opts.category {
                if &entry.category != cat {
                    continue;
                }
            }
            out.push(entry);
        }
        Ok(out)
    }

    async fn get(&self, namespace: &str, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let slug = slug_for(namespace, key);
        match browse::get_page(&self.mcp, &slug).await {
            Ok(page) => Ok(Some(page_detail_to_entry(&page))),
            Err(e) => {
                // page-not-found → None; other errors → propagate.
                let msg = e.to_command_string();
                if msg.contains("page_not_found") || msg.contains("not found") {
                    Ok(None)
                } else {
                    Err(anyhow::anyhow!("gbrain get_page: {}", msg))
                }
            }
        }
    }

    async fn list(
        &self,
        namespace: Option<&str>,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let pages = browse::list_pages(&self.mcp, 200, None, None, None, None)
            .await
            .map_err(|e| anyhow::anyhow!("gbrain list_pages: {}", e.to_command_string()))?;
        // category/session come from page meta, not the summary — list
        // returns summaries (no body), so category/session filters here
        // are best-effort: a summary has no meta, so only namespace
        // filtering is reliable. Documented limitation.
        let _ = (category, session_id);
        let mut out = Vec::new();
        for p in &pages {
            let entry = page_summary_to_entry(p);
            if let Some(ns) = namespace {
                if entry.namespace.as_deref() != Some(ns) {
                    continue;
                }
            }
            out.push(entry);
        }
        Ok(out)
    }

    async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<bool> {
        tracing::warn!(
            namespace = %namespace, key = %key,
            "gbrain has no delete tool — delete is a no-op"
        );
        Ok(false)
    }

    async fn clear_namespace(&self, namespace: &str) -> anyhow::Result<u64> {
        tracing::warn!(
            namespace = %namespace,
            "gbrain has no delete tool — clear_namespace is a no-op"
        );
        Ok(0)
    }

    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
        let pages = browse::list_pages(&self.mcp, 200, None, None, None, None)
            .await
            .map_err(|e| anyhow::anyhow!("gbrain list_pages: {}", e.to_command_string()))?;
        Ok(summaries_from_pages(&pages))
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gbrain::browse::{PageDetail, PageSummary, SearchHit};

    #[test]
    fn slug_round_trips() {
        assert_eq!(slug_for("ns", "key"), "ns/key");
        assert_eq!(
            split_slug("ns/key"),
            (Some("ns".to_string()), "key".to_string())
        );
        // first-slash split: key may itself contain slashes
        assert_eq!(
            split_slug("ns/a/b"),
            (Some("ns".to_string()), "a/b".to_string())
        );
        // no slash → no namespace
        assert_eq!(split_slug("loose"), (None, "loose".to_string()));
    }

    #[test]
    fn build_and_parse_meta_round_trips() {
        let body = build_page_body("hello world", &MemoryCategory::Core, Some("sess1"));
        let (cat, session, content) = parse_page_meta(&body);
        assert!(matches!(cat, MemoryCategory::Core));
        assert_eq!(session.as_deref(), Some("sess1"));
        assert_eq!(content.trim(), "hello world");
    }

    #[test]
    fn parse_meta_defaults_when_absent() {
        let (cat, session, content) = parse_page_meta("just content, no marker");
        assert!(matches!(cat, MemoryCategory::Conversation));
        assert!(session.is_none());
        assert_eq!(content, "just content, no marker");
    }

    #[test]
    fn build_and_parse_custom_category() {
        let body = build_page_body("x", &MemoryCategory::Custom("notes".into()), None);
        let (cat, session, _) = parse_page_meta(&body);
        assert!(matches!(cat, MemoryCategory::Custom(ref s) if s == "notes"));
        assert!(session.is_none());
    }

    #[test]
    fn search_hit_becomes_entry() {
        let hit = SearchHit {
            slug: "ns/k".into(),
            title: "T".into(),
            snippet: "snip".into(),
            similarity: 0.9,
        };
        let e = search_hit_to_entry(&hit);
        assert_eq!(e.id, "ns/k");
        assert_eq!(e.namespace.as_deref(), Some("ns"));
        assert_eq!(e.key, "k");
        assert_eq!(e.content, "snip");
        assert_eq!(e.score, Some(0.9));
    }

    #[test]
    fn page_detail_becomes_entry() {
        let p = PageDetail {
            slug: "ns/k".into(),
            title: "T".into(),
            page_type: "note".into(),
            compiled_truth: build_page_body("the body", &MemoryCategory::Daily, None),
            frontmatter: serde_json::json!({}),
            created_at: None,
            updated_at: Some("2026-05-30T00:00:00Z".into()),
            tags: vec![],
            raw_markdown: String::new(),
        };
        let e = page_detail_to_entry(&p);
        assert_eq!(e.id, "ns/k");
        assert_eq!(e.namespace.as_deref(), Some("ns"));
        assert_eq!(e.content.trim(), "the body");
        assert!(matches!(e.category, MemoryCategory::Daily));
        assert_eq!(e.timestamp, "2026-05-30T00:00:00Z");
    }

    #[test]
    fn summaries_group_by_first_segment() {
        let pages = vec![
            PageSummary {
                slug: "a/1".into(),
                title: "".into(),
                page_type: "".into(),
                updated_at: Some("2026-05-30T01:00:00Z".into()),
            },
            PageSummary {
                slug: "a/2".into(),
                title: "".into(),
                page_type: "".into(),
                updated_at: Some("2026-05-30T02:00:00Z".into()),
            },
            PageSummary {
                slug: "b/1".into(),
                title: "".into(),
                page_type: "".into(),
                updated_at: Some("2026-05-29T00:00:00Z".into()),
            },
        ];
        let mut s = summaries_from_pages(&pages);
        s.sort_by(|x, y| x.namespace.cmp(&y.namespace));
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].namespace, "a");
        assert_eq!(s[0].count, 2);
        assert_eq!(
            s[0].last_updated.as_deref(),
            Some("2026-05-30T02:00:00Z")
        ); // latest in 'a'
        assert_eq!(s[1].namespace, "b");
        assert_eq!(s[1].count, 1);
    }

    #[test]
    fn page_summary_becomes_entry() {
        let s = PageSummary {
            slug: "ns/mykey".into(),
            title: "My Title".into(),
            page_type: "note".into(),
            updated_at: Some("2026-05-30T12:00:00Z".into()),
        };
        let e = page_summary_to_entry(&s);
        assert_eq!(e.id, "ns/mykey");
        assert_eq!(e.namespace.as_deref(), Some("ns"));
        assert_eq!(e.key, "mykey");
        assert_eq!(e.content, "My Title");
        assert_eq!(e.timestamp, "2026-05-30T12:00:00Z");
        assert!(matches!(e.category, MemoryCategory::Conversation));
    }

    #[test]
    fn summaries_skip_namespaceless_pages() {
        let pages = vec![
            PageSummary {
                slug: "loose".into(), // no '/' — no namespace
                title: "".into(),
                page_type: "".into(),
                updated_at: None,
            },
            PageSummary {
                slug: "a/1".into(),
                title: "".into(),
                page_type: "".into(),
                updated_at: Some("2026-05-30T00:00:00Z".into()),
            },
        ];
        let s = summaries_from_pages(&pages);
        // "loose" is skipped; only "a" produces a NamespaceSummary.
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].namespace, "a");
        assert_eq!(s[0].count, 1);
    }

    #[test]
    fn parse_meta_malformed_marker_falls_back() {
        // Marker is present but missing the closing `-->`.
        // trim_end_matches("-->") is a no-op, so KV parsing still proceeds
        // on the raw suffix and successfully extracts category=core + session=x.
        let input = "<!-- uclaw-meta: category=core; session=x\nbody here";
        let (cat, session, content) = parse_page_meta(input);
        // Does not panic; category is parsed as Core (not the Conversation fallback).
        assert!(matches!(cat, MemoryCategory::Core));
        assert_eq!(session.as_deref(), Some("x"));
        assert_eq!(content.trim(), "body here");
    }
}
