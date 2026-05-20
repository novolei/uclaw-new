//! 子项目 A — gbrain 知识浏览器代理层。
//!
//! 把 gbrain MCP op 封装成强类型 uClaw 函数：参数组装 → MCP 调用
//! （get_transport 模式，避免跨 await 持锁）→ 反序列化 → 结构化错误。
//! 纯 `parse_*` 函数与 IO 分离，便于单测（mock JSON 文本即可）。

use serde::{Deserialize, Serialize};

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
