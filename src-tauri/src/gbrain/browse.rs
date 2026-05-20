//! 子项目 A — gbrain 知识浏览器代理层。
//!
//! 把 gbrain MCP op 封装成强类型 uClaw 函数：参数组装 → MCP 调用
//! （get_transport 模式，避免跨 await 持锁）→ 反序列化 → 结构化错误。
//! 纯 `parse_*` 函数与 IO 分离，便于单测（mock JSON 文本即可）。

use serde::{Deserialize, Serialize};
use crate::mcp::{ContentBlock, JsonRpcRequest, McpServerStatus, SharedMcpManager};

/// 代理层归一化错误。Tauri 命令把它转成稳定字符串返回前端。
#[derive(Debug, Clone)]
pub enum GbrainError {
    /// gbrain MCP server 未连接 / 未起。
    NotConnected,
    /// MCP 调用本身失败（传输/超时/server 返回 is_error）。
    CallFailed(String),
    /// gbrain 返回了意外的 JSON 形状。
    ParseFailed(String),
}

impl GbrainError {
    /// 稳定的命令层错误字符串（前端按这些前缀分支）。
    pub fn to_command_string(&self) -> String {
        match self {
            GbrainError::NotConnected => "gbrain_not_connected".to_string(),
            GbrainError::CallFailed(m) => format!("gbrain_call_failed: {m}"),
            GbrainError::ParseFailed(m) => format!("gbrain_response_parse_failed: {m}"),
        }
    }
}

// ─── 强类型 DTO（镜像 gbrain types.ts，只保留 WikiView 需要的字段）──────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSummary {
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(rename = "type", default)]
    pub page_type: String,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageDetail {
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(rename = "type", default)]
    pub page_type: String,
    #[serde(default)]
    pub compiled_truth: String,
    #[serde(default)]
    pub frontmatter: serde_json::Value,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// browse.rs 计算字段：frontmatter(YAML) + compiled_truth 重组的可编辑源。
    #[serde(default)]
    pub raw_markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(rename = "chunk_text", default)]
    pub snippet: String,
    #[serde(rename = "score", default)]
    pub similarity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backlink {
    pub from_slug: String,
    #[serde(default)]
    pub link_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMeta {
    /// gbrain PageVersion.id —— 数值，revert 时回传。
    pub id: i64,
    #[serde(default)]
    pub snapshot_at: Option<String>,
    /// 历史版本的 compiled_truth（用于预览，免费随 get_versions 返回）。
    #[serde(default)]
    pub compiled_truth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainStats {
    #[serde(default)]
    pub page_count: i64,
    #[serde(default)]
    pub chunk_count: i64,
    #[serde(default)]
    pub embedded_count: i64,
    #[serde(default)]
    pub link_count: i64,
    #[serde(default)]
    pub tag_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphanSummary {
    #[serde(default)]
    pub total_orphans: i64,
    #[serde(default)]
    pub total_pages: i64,
}

// ─── 纯反序列化函数（单测目标，mock JSON 文本）──────────────────────────────

/// 若 gbrain 返回顶层 `{error: "..."}`（如 ambiguous_slug），归一化为 CallFailed。
fn reject_error_envelope(v: &serde_json::Value) -> Result<(), GbrainError> {
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        return Err(GbrainError::CallFailed(err.to_string()));
    }
    Ok(())
}

pub fn parse_page_detail(json_text: &str) -> Result<PageDetail, GbrainError> {
    let v: serde_json::Value = serde_json::from_str(json_text)
        .map_err(|e| GbrainError::ParseFailed(e.to_string()))?;
    reject_error_envelope(&v)?;
    let mut detail: PageDetail = serde_json::from_value(v)
        .map_err(|e| GbrainError::ParseFailed(e.to_string()))?;
    detail.raw_markdown = build_raw_markdown(&detail.frontmatter, &detail.compiled_truth);
    Ok(detail)
}

pub fn parse_list_pages(json_text: &str) -> Result<Vec<PageSummary>, GbrainError> {
    serde_json::from_str(json_text).map_err(|e| GbrainError::ParseFailed(e.to_string()))
}

pub fn parse_search(json_text: &str) -> Result<Vec<SearchHit>, GbrainError> {
    serde_json::from_str(json_text).map_err(|e| GbrainError::ParseFailed(e.to_string()))
}

pub fn parse_backlinks(json_text: &str) -> Result<Vec<Backlink>, GbrainError> {
    serde_json::from_str(json_text).map_err(|e| GbrainError::ParseFailed(e.to_string()))
}

pub fn parse_versions(json_text: &str) -> Result<Vec<VersionMeta>, GbrainError> {
    serde_json::from_str(json_text).map_err(|e| GbrainError::ParseFailed(e.to_string()))
}

pub fn parse_stats(json_text: &str) -> Result<BrainStats, GbrainError> {
    serde_json::from_str(json_text).map_err(|e| GbrainError::ParseFailed(e.to_string()))
}

pub fn parse_orphans(json_text: &str) -> Result<OrphanSummary, GbrainError> {
    serde_json::from_str(json_text).map_err(|e| GbrainError::ParseFailed(e.to_string()))
}

/// frontmatter(object) + body → 可编辑的完整 markdown 源。
/// frontmatter 为空/null 时只返回 body。
pub fn build_raw_markdown(frontmatter: &serde_json::Value, body: &str) -> String {
    let is_empty = frontmatter.is_null()
        || frontmatter
            .as_object()
            .map(|m| m.is_empty())
            .unwrap_or(false);
    if is_empty {
        return body.to_string();
    }
    match serde_yml::to_string(frontmatter) {
        Ok(yaml) if !yaml.trim().is_empty() => format!("---\n{yaml}---\n\n{body}"),
        _ => body.to_string(),
    }
}

// ─── MCP 调用助手 ────────────────────────────────────────────────────────────

/// 调一个 gbrain MCP op，返回拼接后的文本内容（JSON）。
/// 沿用 tauri_commands.rs::call_gbrain_eval_tool 的 get_transport 模式：
/// 克隆 transport + req_id 后释放管理器读锁，再做网络 await。
async fn call_gbrain(
    mcp_manager: &SharedMcpManager,
    op: &str,
    arguments: serde_json::Value,
) -> Result<String, GbrainError> {
    let (transport, req_id) = {
        let manager = mcp_manager.read().await;
        if !matches!(manager.status("gbrain"), Some(McpServerStatus::Connected)) {
            return Err(GbrainError::NotConnected);
        }
        manager
            .get_transport("gbrain")
            .map_err(|_| GbrainError::NotConnected)?
    };
    let request = JsonRpcRequest::call_tool(req_id, op, arguments);
    let response = transport
        .send(&request)
        .await
        .map_err(|e| GbrainError::CallFailed(e.to_string()))?;
    if let Some(err) = response.error {
        return Err(GbrainError::CallFailed(format!("{err:?}")));
    }
    let result_value = response
        .result
        .ok_or_else(|| GbrainError::CallFailed("empty MCP result".to_string()))?;
    let result: crate::mcp::CallToolResult = serde_json::from_value(result_value)
        .map_err(|e| GbrainError::ParseFailed(e.to_string()))?;
    // gbrain 错误（page_not_found 等）→ is_error=true，在这里短路成 CallFailed，
    // 所以数组返回型 op 的 parser 永远不会看到 {error:...} 信封——只有 get_page
    // 的 ambiguous_slug（is_error=false + error 字段）需要 reject_error_envelope。
    if result.is_error {
        let text = join_text(&result.content);
        return Err(GbrainError::CallFailed(text));
    }
    Ok(join_text(&result.content))
}

fn join_text(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── 读取侧异步函数 ──────────────────────────────────────────────────────────

pub async fn list_pages(
    mcp: &SharedMcpManager,
    limit: u32,
    sort: Option<String>,
    page_type: Option<String>,
    tag: Option<String>,
    updated_after: Option<String>,
) -> Result<Vec<PageSummary>, GbrainError> {
    let mut args = serde_json::Map::new();
    args.insert("limit".into(), serde_json::json!(limit));
    if let Some(s) = sort { args.insert("sort".into(), serde_json::json!(s)); }
    if let Some(t) = page_type { args.insert("type".into(), serde_json::json!(t)); }
    if let Some(t) = tag { args.insert("tag".into(), serde_json::json!(t)); }
    if let Some(d) = updated_after { args.insert("updated_after".into(), serde_json::json!(d)); }
    let text = call_gbrain(mcp, "list_pages", serde_json::Value::Object(args)).await?;
    parse_list_pages(&text)
}

pub async fn get_page(mcp: &SharedMcpManager, slug: &str) -> Result<PageDetail, GbrainError> {
    let text = call_gbrain(mcp, "get_page", serde_json::json!({ "slug": slug, "fuzzy": true })).await?;
    parse_page_detail(&text)
}

pub async fn search(
    mcp: &SharedMcpManager,
    query: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<SearchHit>, GbrainError> {
    let text = call_gbrain(
        mcp,
        "search",
        serde_json::json!({ "query": query, "limit": limit, "offset": offset }),
    )
    .await?;
    parse_search(&text)
}

pub async fn get_backlinks(mcp: &SharedMcpManager, slug: &str) -> Result<Vec<Backlink>, GbrainError> {
    let text = call_gbrain(mcp, "get_backlinks", serde_json::json!({ "slug": slug })).await?;
    parse_backlinks(&text)
}

/// A 只建命令不渲染（留给 C）。返回原始 JSON 文本，前端 V1 不解析。
pub async fn traverse_graph(
    mcp: &SharedMcpManager,
    slug: &str,
    depth: u32,
    direction: Option<String>,
) -> Result<serde_json::Value, GbrainError> {
    let mut args = serde_json::Map::new();
    args.insert("slug".into(), serde_json::json!(slug));
    args.insert("depth".into(), serde_json::json!(depth));
    if let Some(d) = direction { args.insert("direction".into(), serde_json::json!(d)); }
    let text = call_gbrain(mcp, "traverse_graph", serde_json::Value::Object(args)).await?;
    serde_json::from_str(&text).map_err(|e| GbrainError::ParseFailed(e.to_string()))
}

pub async fn get_versions(mcp: &SharedMcpManager, slug: &str) -> Result<Vec<VersionMeta>, GbrainError> {
    let text = call_gbrain(mcp, "get_versions", serde_json::json!({ "slug": slug })).await?;
    parse_versions(&text)
}

pub async fn get_stats(mcp: &SharedMcpManager) -> Result<BrainStats, GbrainError> {
    let text = call_gbrain(mcp, "get_stats", serde_json::json!({})).await?;
    parse_stats(&text)
}

pub async fn find_orphans(mcp: &SharedMcpManager) -> Result<OrphanSummary, GbrainError> {
    let text = call_gbrain(mcp, "find_orphans", serde_json::json!({})).await?;
    parse_orphans(&text)
}

// ─── 知识图谱类型 ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeNode {
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(rename = "type", default)]
    pub page_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEdge {
    pub from_slug: String,
    pub to_slug: String,
    #[serde(default)]
    pub link_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    pub nodes: Vec<KnowledgeNode>,
    pub edges: Vec<KnowledgeEdge>,
}

/// 解析 get_links 的 JSON（Link[]，与 get_backlinks 同形）为出边。
pub fn parse_links(json_text: &str) -> Result<Vec<KnowledgeEdge>, GbrainError> {
    serde_json::from_str(json_text).map_err(|e| GbrainError::ParseFailed(e.to_string()))
}

/// 纯拼装：节点集 + 各页出边 → 图，丢弃指向未加载页的悬空边。单测目标。
pub fn assemble_graph(nodes: Vec<KnowledgeNode>, edges: Vec<KnowledgeEdge>) -> KnowledgeGraph {
    use std::collections::HashSet;
    let slugs: HashSet<&str> = nodes.iter().map(|n| n.slug.as_str()).collect();
    let edges = edges
        .into_iter()
        .filter(|e| slugs.contains(e.from_slug.as_str()) && slugs.contains(e.to_slug.as_str()))
        .collect();
    KnowledgeGraph { nodes, edges }
}

// ─── 知识图谱异步函数 ────────────────────────────────────────────────────────

pub async fn full_graph(
    mcp: &SharedMcpManager,
    limit: u32,
) -> Result<KnowledgeGraph, GbrainError> {
    let pages = list_pages(mcp, limit, Some("updated_desc".into()), None, None, None).await?;
    let nodes: Vec<KnowledgeNode> = pages
        .into_iter()
        .map(|p| KnowledgeNode { slug: p.slug, title: p.title, page_type: p.page_type })
        .collect();
    let mut all_edges: Vec<KnowledgeEdge> = Vec::new();
    for n in &nodes {
        // best-effort: a single page's get_links failure skips that page's edges, doesn't abort
        if let Ok(text) = call_gbrain(mcp, "get_links", serde_json::json!({ "slug": n.slug })).await {
            if let Ok(mut edges) = parse_links(&text) {
                all_edges.append(&mut edges);
            }
        }
    }
    Ok(assemble_graph(nodes, all_edges))
}

/// 保存编辑：put_page(slug, content=完整 markdown) → re-fetch 返回新页。
/// 不依赖 put_page 的返回 shape（gbrain put_page 返回 status，不是页）。
pub async fn put_page(
    mcp: &SharedMcpManager,
    slug: &str,
    content: &str,
) -> Result<PageDetail, GbrainError> {
    call_gbrain(
        mcp,
        "put_page",
        serde_json::json!({ "slug": slug, "content": content }),
    )
    .await?;
    get_page(mcp, slug).await
}

/// 回滚到某版本：revert_version(slug, version_id:number) → re-fetch 返回新页。
pub async fn revert_version(
    mcp: &SharedMcpManager,
    slug: &str,
    version_id: i64,
) -> Result<PageDetail, GbrainError> {
    call_gbrain(
        mcp,
        "revert_version",
        serde_json::json!({ "slug": slug, "version_id": version_id }),
    )
    .await?;
    get_page(mcp, slug).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_page_detail_from_gbrain_json() {
        let json = r##"{
            "id": 1, "slug": "person-alice", "type": "person",
            "title": "Alice", "compiled_truth": "# Alice\nFounder.",
            "frontmatter": {"type":"person","title":"Alice"},
            "created_at": "2026-05-01T00:00:00Z",
            "updated_at": "2026-05-10T00:00:00Z",
            "tags": ["founder","yc"]
        }"##;
        let d = parse_page_detail(json).expect("should parse");
        assert_eq!(d.slug, "person-alice");
        assert_eq!(d.page_type, "person");
        assert_eq!(d.title, "Alice");
        assert!(d.compiled_truth.contains("Founder"));
        assert_eq!(d.tags, vec!["founder", "yc"]);
        assert!(d.raw_markdown.starts_with("---\n"));
        assert!(d.raw_markdown.contains("# Alice"));
    }

    #[test]
    fn deserialize_list_pages_paginated() {
        let json = r#"[
            {"slug":"a","type":"concept","title":"A","updated_at":"2026-05-10T00:00:00Z"},
            {"slug":"b","type":"person","title":"B","updated_at":"2026-05-09T00:00:00Z"}
        ]"#;
        let pages = parse_list_pages(json).expect("should parse");
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].slug, "a");
        assert_eq!(pages[1].page_type, "person");
    }

    #[test]
    fn deserialize_search_maps_chunk_and_score() {
        let json = r#"[
            {"slug":"a","page_id":1,"title":"A","type":"concept",
             "chunk_text":"hello world","chunk_source":"compiled_truth",
             "chunk_id":1,"chunk_index":0,"score":0.87,"stale":false}
        ]"#;
        let hits = parse_search(json).expect("should parse");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].snippet, "hello world");
        assert!((hits[0].similarity - 0.87).abs() < 1e-6);
    }

    #[test]
    fn deserialize_backlinks() {
        let json = r#"[{"from_slug":"project-x","to_slug":"alice","link_type":"works_at","context":""}]"#;
        let bl = parse_backlinks(json).expect("should parse");
        assert_eq!(bl[0].from_slug, "project-x");
        assert_eq!(bl[0].link_type, "works_at");
    }

    #[test]
    fn deserialize_versions_keeps_numeric_id() {
        let json = r#"[{"id":7,"page_id":1,"compiled_truth":"old body","frontmatter":{},"snapshot_at":"2026-05-05T00:00:00Z"}]"#;
        let v = parse_versions(json).expect("should parse");
        assert_eq!(v[0].id, 7);
        assert_eq!(v[0].compiled_truth, "old body");
    }

    #[test]
    fn deserialize_stats() {
        let json = r#"{"page_count":42,"chunk_count":100,"embedded_count":90,"link_count":33,"tag_count":12,"timeline_entry_count":5,"pages_by_type":{}}"#;
        let s = parse_stats(json).expect("should parse");
        assert_eq!(s.page_count, 42);
        assert_eq!(s.embedded_count, 90);
    }

    #[test]
    fn deserialize_orphans() {
        let json = r#"{"orphans":[{"slug":"x","title":"X","domain":null}],"total_orphans":1,"total_linkable":40,"total_pages":42,"excluded":0}"#;
        let o = parse_orphans(json).expect("should parse");
        assert_eq!(o.total_orphans, 1);
        assert_eq!(o.total_pages, 42);
    }

    #[test]
    fn deserialize_handles_empty_results() {
        assert!(parse_list_pages("[]").unwrap().is_empty());
        assert!(parse_search("[]").unwrap().is_empty());
        assert!(parse_backlinks("[]").unwrap().is_empty());
        assert!(parse_versions("[]").unwrap().is_empty());
    }

    #[test]
    fn deserialize_handles_malformed_json() {
        assert!(matches!(parse_list_pages("not json"), Err(GbrainError::ParseFailed(_))));
        assert!(matches!(parse_page_detail("{bad"), Err(GbrainError::ParseFailed(_))));
    }

    #[test]
    fn page_detail_rejects_error_envelope() {
        let json = r#"{"error":"ambiguous_slug","candidates":["a","b"]}"#;
        assert!(matches!(parse_page_detail(json), Err(GbrainError::CallFailed(_))));
    }

    #[test]
    fn build_raw_markdown_with_and_without_frontmatter() {
        let fm = serde_json::json!({"type":"person","title":"Alice"});
        let md = build_raw_markdown(&fm, "# Alice");
        assert!(md.starts_with("---\n"));
        assert!(md.contains("# Alice"));
        let empty = build_raw_markdown(&serde_json::Value::Null, "just body");
        assert_eq!(empty, "just body");
        let empty_obj = build_raw_markdown(&serde_json::json!({}), "just body");
        assert_eq!(empty_obj, "just body");
    }
}

#[cfg(test)]
mod full_graph_tests {
    use super::*;

    #[test]
    fn parse_links_reads_from_to_type() {
        let json = r#"[{"from_slug":"a","to_slug":"b","link_type":"mentions","context":""}]"#;
        let e = parse_links(json).unwrap();
        assert_eq!(e[0].from_slug, "a");
        assert_eq!(e[0].to_slug, "b");
        assert_eq!(e[0].link_type, "mentions");
    }

    #[test]
    fn assemble_drops_dangling_edges() {
        let nodes = vec![
            KnowledgeNode { slug: "a".into(), title: "A".into(), page_type: "concept".into() },
            KnowledgeNode { slug: "b".into(), title: "B".into(), page_type: "person".into() },
        ];
        let edges = vec![
            KnowledgeEdge { from_slug: "a".into(), to_slug: "b".into(), link_type: "x".into() },
            KnowledgeEdge { from_slug: "a".into(), to_slug: "ghost".into(), link_type: "x".into() },
        ];
        let g = assemble_graph(nodes, edges);
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].to_slug, "b");
    }

    #[test]
    fn assemble_empty_is_safe() {
        let g = assemble_graph(vec![], vec![]);
        assert!(g.nodes.is_empty() && g.edges.is_empty());
    }

    #[test]
    fn parse_links_handles_empty_and_malformed() {
        assert!(parse_links("[]").unwrap().is_empty());
        assert!(matches!(parse_links("nope"), Err(GbrainError::ParseFailed(_))));
    }
}
