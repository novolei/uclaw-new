# 子项目 A — gbrain 知识浏览器 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 复活 `WikiView.tsx`，把它的数据源从悬空的 `memory_wiki_*` 换成新的 `gbrain_*` Tauri 命令（内部走 MCP 代理到已连接的 gbrain stdio 服务），实现页面浏览/搜索/反向链接/编辑/版本史。

**Architecture:** 纯代理层 + 前端复活。新 Rust 模块 `src-tauri/src/gbrain/browse.rs` 把每个 gbrain MCP op 封装成强类型函数（参数组装 → MCP 调用 → 反序列化 → 结构化错误）；10 个薄 Tauri 命令转发；新 TS lib `ui/src/lib/gbrain-browse.ts` 镜像类型；`WikiView.tsx` 重写数据层、保留三区视觉骨架。无新 migration、无新依赖。

**Tech Stack:** Rust（tauri command、serde、serde_json、已有依赖 `serde_yml`）、React 18 + TypeScript + Jotai + Tailmind + react-markdown、Vitest。

---

## 关键事实（实现前必读 —— spec 的命令签名是理想化的，下面是 gbrain 真实 API）

侦察结果（全部来自 `src-tauri/gbrain-source/src/core/operations.ts` + `types.ts`，已逐行核对）：

1. **MCP 调用模式**：不要用 `McpManager::call_tool`（它跨网络 await 持有 `&self` 即 RwLock 读锁）。沿用既有 gbrain 调用样例 `call_gbrain_eval_tool`（`src-tauri/src/tauri_commands.rs:689-724`）的 **get_transport 模式**：`read().await` → `status("gbrain")` 检查 → `get_transport("gbrain")`（克隆 transport + req_id）→ **释放锁** → `transport.send(JsonRpcRequest::call_tool(...)).await`。server_id 是字面量 `"gbrain"`（不是 `state.gbrain_mcp_id`）。
2. **CallToolResult**（`src-tauri/src/mcp.rs:487-499`）：`{ content: Vec<ContentBlock>, is_error: bool }`；`ContentBlock::Text { text }`。文本是 JSON。提取方式见 §既有样例。
3. **`list_pages`** 参数：`type, tag, limit, updated_after, sort, include_deleted` —— **没有 offset，没有 after_date/before_date**。返回 `[{slug, type, title, updated_at, deleted_at?}]`。
4. **`get_page`** 返回整个 Page 展开 + `tags`：`{id, slug, source_id, type, title, compiled_truth, timeline?, frontmatter, content_hash?, created_at, updated_at, deleted_at?, effective_date?, tags}`。错误时 `is_error=true`（page_not_found）或返回 `{error:'ambiguous_slug', candidates}`。
5. **`search`** 参数 `query, limit, offset`，返回 `SearchResult[]`：`{slug, page_id, title, type, chunk_text, chunk_source, chunk_id, chunk_index, score, stale, source_id?}`。snippet=`chunk_text`，similarity=`score`。
6. **`get_backlinks`** 返回 `Link[]`：`{from_slug, to_slug, link_type, context, link_source?, ...}`。当前页是 `to_slug`，链接来源是 `from_slug`。
7. **`traverse_graph`** 无 `link_type`/`direction` 时返回 `GraphNode[]`：`{slug, title, type, depth, links:[{to_slug, link_type}]}`；设了任一则返回 `GraphPath[]`。**A 只建命令不渲染**（留给 C）。
8. **`get_versions`** 返回 `PageVersion[]`：`{id, page_id, compiled_truth, frontmatter, snapshot_at}`。version_id = `id`（**number**），时间 = `snapshot_at`，**没有 created_by**，`compiled_truth` 在版本里（预览免费）。
9. **`revert_version`** 参数 `slug, version_id`（**number**），返回 `{status:'reverted'}`（**不返回页**）。
10. **`put_page`** 参数 **只有 `slug` + `content`**（content = 含 YAML frontmatter 的完整 markdown）—— **没有** type/title/body/frontmatter 分字段。
11. **`get_stats`** `scope:'admin'`，返回 `BrainStats`：`{page_count, chunk_count, embedded_count, link_count, tag_count, timeline_entry_count, pages_by_type}`。嵌入覆盖率 = `embedded_count/chunk_count`（前端算）。
12. **`find_orphans`** `scope:'read'`，返回 `OrphanResult`：`{orphans:[{slug,title,domain}], total_orphans, total_linkable, total_pages, excluded}`。
13. **admin scope 风险**：uClaw 的 gbrain 是本地 stdio 子进程（无 OAuth scope 网关），既有 `call_gbrain_eval_tool` 已证明 read op 通。`get_stats`（唯一 admin op）大概率也通，但列为**手动验证 checkpoint** + 优雅降级（取不到 stats 就只显示列表计数）。
14. **编辑回写**：gbrain 不单独暴露"原始 body"。V1 策略 —— `get_page` 在 browse.rs 侧用 `serde_yml` 把 frontmatter 重组成 `---\n{yaml}---\n\n{compiled_truth}` 作为 `raw_markdown` 返回；编辑器编辑 `raw_markdown`（用户所见即所存）；保存时 `put_page(slug, content=编辑后的 raw_markdown)`，gbrain 重新解析+编译；browse.rs 随后 **re-fetch get_page** 返回新 PageDetail（不依赖 put_page/revert 的返回 shape）。

**验证命令**（每个 Task 末尾用）：
- Rust 编译：`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
- Rust 测试：`cd src-tauri && cargo test --lib gbrain_browse 2>&1 | tail -20`
- TS 检查：`cd ui && npx tsc --noEmit 2>&1 | head -10`
- Vitest：`cd ui && npm test -- --run WikiView 2>&1 | tail -20`

---

## 文件结构

| 文件 | 职责 |
|---|---|
| `src-tauri/src/gbrain/browse.rs` (新) | `GbrainError`、强类型结构、纯 `parse_*` 反序列化函数、`call_gbrain` 助手、10 个 typed async 函数、`build_raw_markdown` |
| `src-tauri/src/gbrain/mod.rs` (改) | 加 `pub mod browse;` |
| `src-tauri/src/tauri_commands.rs` (改) | 10 个 `#[tauri::command]` 薄封装 |
| `src-tauri/src/main.rs` (改) | `invoke_handler!` 注册 10 个命令 |
| `ui/src/lib/gbrain-browse.ts` (新) | TS 类型镜像 + 10 个 `invoke` 包装 |
| `ui/src/components/memory/WikiView.tsx` (重写) | 三区布局接 gbrain 数据源 + 编辑 + 版本史 |
| `ui/src/components/memory/WikiView.test.tsx` (新) | Vitest：列表/详情/反链/编辑/版本/空状态/搜索 |

---

## Task 1: browse.rs 脚手架 + 类型 + 读侧 parse 函数 + 单测

**Files:**
- Create: `src-tauri/src/gbrain/browse.rs`
- Modify: `src-tauri/src/gbrain/mod.rs`

- [ ] **Step 1: 在 `src-tauri/src/gbrain/mod.rs` 末尾加模块声明**

```rust
pub mod browse;
```

- [ ] **Step 2: 创建 `src-tauri/src/gbrain/browse.rs`，写入错误类型 + 强类型结构 + 纯 parse 函数**

```rust
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
        // r##"..."## delimiter: the JSON contains `"# Alice` whose `"#`
        // would prematurely close a plain r#"..."# raw string.
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
        // raw_markdown 应含 frontmatter 块 + body
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
        // 空 frontmatter → 只返回 body
        let empty = build_raw_markdown(&serde_json::Value::Null, "just body");
        assert_eq!(empty, "just body");
        let empty_obj = build_raw_markdown(&serde_json::json!({}), "just body");
        assert_eq!(empty_obj, "just body");
    }
}
```

- [ ] **Step 3: 编译 + 跑单测**

Run: `cd src-tauri && cargo test --lib gbrain::browse 2>&1 | tail -25`
Expected: 全部 PASS（约 11 个测试）。若 `serde_yml::to_string` 签名不符，确认 `serde_yml = "0.0.12"` 在 `Cargo.toml`（已确认在 line 116）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/gbrain/browse.rs src-tauri/src/gbrain/mod.rs
git commit -m "feat(gbrain): browse.rs types + read-side parsers + raw-markdown builder + tests"
```

---

## Task 2: browse.rs MCP 调用助手 + 读侧 async 函数

**Files:**
- Modify: `src-tauri/src/gbrain/browse.rs`

- [ ] **Step 1: 在 browse.rs 顶部 use 区加导入**

```rust
use crate::mcp::{ContentBlock, JsonRpcRequest, McpServerStatus, SharedMcpManager};
```

- [ ] **Step 2: 加 `call_gbrain` 助手（get_transport 模式，结构化错误）**

放在 `build_raw_markdown` 之后、`#[cfg(test)]` 之前：

```rust
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
```

- [ ] **Step 3: 加读侧 typed async 函数**

```rust
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
```

- [ ] **Step 4: 编译（这些 async 函数无单测——需真 gbrain，靠手动 E2E）**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无 error 输出。若 `JsonRpcRequest` / `McpServerStatus` / `ContentBlock` / `CallToolResult` 路径报错，确认它们都 `pub` 自 `crate::mcp`（已核对：mcp.rs:377/506/487/333）。`response.error` / `response.result` 字段来自 `JsonRpcResponse`——若字段名不符，`grep -n "struct JsonRpcResponse" src-tauri/src/mcp.rs` 核对。

- [ ] **Step 5: 跑既有单测确认未破坏**

Run: `cd src-tauri && cargo test --lib gbrain::browse 2>&1 | tail -10`
Expected: Task 1 的测试仍全 PASS。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/gbrain/browse.rs
git commit -m "feat(gbrain): browse.rs MCP call helper + read-side ops (get_transport pattern)"
```

---

## Task 3: browse.rs 写侧 async 函数（put_page + revert_version）

**Files:**
- Modify: `src-tauri/src/gbrain/browse.rs`

- [ ] **Step 1: 加写侧 async 函数（写后 re-fetch get_page，返回新 PageDetail）**

放在读侧函数之后：

```rust
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
```

- [ ] **Step 2: 编译**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无 error。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/gbrain/browse.rs
git commit -m "feat(gbrain): browse.rs write ops (put_page re-fetch + revert_version)"
```

---

## Task 4: 注册 10 个 gbrain_* Tauri 命令

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 在 `tauri_commands.rs` 末尾（任意命令区，建议靠近既有 gbrain 相关命令如 `restart_gbrain_mcp`）追加 10 个命令**

```rust
// ─── 子项目 A — gbrain 知识浏览器代理命令 ────────────────────────────────

#[tauri::command]
pub async fn gbrain_list_pages(
    state: State<'_, AppState>,
    limit: Option<u32>,
    sort: Option<String>,
    page_type: Option<String>,
    tag: Option<String>,
    updated_after: Option<String>,
) -> Result<Vec<crate::gbrain::browse::PageSummary>, String> {
    crate::gbrain::browse::list_pages(
        &state.mcp_manager,
        limit.unwrap_or(200),
        sort,
        page_type,
        tag,
        updated_after,
    )
    .await
    .map_err(|e| e.to_command_string())
}

#[tauri::command]
pub async fn gbrain_get_page(
    state: State<'_, AppState>,
    slug: String,
) -> Result<crate::gbrain::browse::PageDetail, String> {
    crate::gbrain::browse::get_page(&state.mcp_manager, &slug)
        .await
        .map_err(|e| e.to_command_string())
}

#[tauri::command]
pub async fn gbrain_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<crate::gbrain::browse::SearchHit>, String> {
    crate::gbrain::browse::search(
        &state.mcp_manager,
        &query,
        limit.unwrap_or(20),
        offset.unwrap_or(0),
    )
    .await
    .map_err(|e| e.to_command_string())
}

#[tauri::command]
pub async fn gbrain_get_backlinks(
    state: State<'_, AppState>,
    slug: String,
) -> Result<Vec<crate::gbrain::browse::Backlink>, String> {
    crate::gbrain::browse::get_backlinks(&state.mcp_manager, &slug)
        .await
        .map_err(|e| e.to_command_string())
}

#[tauri::command]
pub async fn gbrain_traverse_graph(
    state: State<'_, AppState>,
    slug: String,
    depth: Option<u32>,
    direction: Option<String>,
) -> Result<serde_json::Value, String> {
    crate::gbrain::browse::traverse_graph(&state.mcp_manager, &slug, depth.unwrap_or(2), direction)
        .await
        .map_err(|e| e.to_command_string())
}

#[tauri::command]
pub async fn gbrain_get_versions(
    state: State<'_, AppState>,
    slug: String,
) -> Result<Vec<crate::gbrain::browse::VersionMeta>, String> {
    crate::gbrain::browse::get_versions(&state.mcp_manager, &slug)
        .await
        .map_err(|e| e.to_command_string())
}

#[tauri::command]
pub async fn gbrain_revert_version(
    state: State<'_, AppState>,
    slug: String,
    version_id: i64,
) -> Result<crate::gbrain::browse::PageDetail, String> {
    crate::gbrain::browse::revert_version(&state.mcp_manager, &slug, version_id)
        .await
        .map_err(|e| e.to_command_string())
}

#[tauri::command]
pub async fn gbrain_put_page(
    state: State<'_, AppState>,
    slug: String,
    content: String,
) -> Result<crate::gbrain::browse::PageDetail, String> {
    crate::gbrain::browse::put_page(&state.mcp_manager, &slug, &content)
        .await
        .map_err(|e| e.to_command_string())
}

#[tauri::command]
pub async fn gbrain_get_stats(
    state: State<'_, AppState>,
) -> Result<crate::gbrain::browse::BrainStats, String> {
    crate::gbrain::browse::get_stats(&state.mcp_manager)
        .await
        .map_err(|e| e.to_command_string())
}

#[tauri::command]
pub async fn gbrain_find_orphans(
    state: State<'_, AppState>,
) -> Result<crate::gbrain::browse::OrphanSummary, String> {
    crate::gbrain::browse::find_orphans(&state.mcp_manager)
        .await
        .map_err(|e| e.to_command_string())
}
```

- [ ] **Step 2: 在 `main.rs` 的 `invoke_handler![tauri::generate_handler![...]]` 宏里注册（找现有 gbrain 命令如 `restart_gbrain_mcp` 附近插入）**

```rust
            uclaw_core::tauri_commands::gbrain_list_pages,
            uclaw_core::tauri_commands::gbrain_get_page,
            uclaw_core::tauri_commands::gbrain_search,
            uclaw_core::tauri_commands::gbrain_get_backlinks,
            uclaw_core::tauri_commands::gbrain_traverse_graph,
            uclaw_core::tauri_commands::gbrain_get_versions,
            uclaw_core::tauri_commands::gbrain_revert_version,
            uclaw_core::tauri_commands::gbrain_put_page,
            uclaw_core::tauri_commands::gbrain_get_stats,
            uclaw_core::tauri_commands::gbrain_find_orphans,
```

- [ ] **Step 3: 编译（确认命令签名 + 注册一致；漏注册编译过但运行时失败）**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无 error。若 `State` 未导入，确认 `tauri_commands.rs` 顶部已 `use tauri::State;`（既有命令已用，应已在）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(tauri): register 10 gbrain_* browser commands + invoke_handler"
```

---

## Task 5: TS IPC 包装 `ui/src/lib/gbrain-browse.ts`

**Files:**
- Create: `ui/src/lib/gbrain-browse.ts`

- [ ] **Step 1: 创建文件，写入类型镜像 + invoke 包装**

```ts
import { invoke } from '@tauri-apps/api/core'

// ─── 类型（镜像 Rust browse.rs）──────────────────────────────────────────

export interface PageSummary {
  slug: string
  title: string
  type: string
  updated_at: string | null
}

export interface PageDetail {
  slug: string
  title: string
  type: string
  compiled_truth: string
  frontmatter: unknown
  created_at: string | null
  updated_at: string | null
  tags: string[]
  raw_markdown: string
}

export interface SearchHit {
  slug: string
  title: string
  snippet: string
  similarity: number
}

export interface Backlink {
  from_slug: string
  link_type: string
}

export interface VersionMeta {
  id: number
  snapshot_at: string | null
  compiled_truth: string
}

export interface BrainStats {
  page_count: number
  chunk_count: number
  embedded_count: number
  link_count: number
  tag_count: number
}

export interface OrphanSummary {
  total_orphans: number
  total_pages: number
}

// 命令返回的稳定错误前缀（前端按此分支空状态）
export const GBRAIN_NOT_CONNECTED = 'gbrain_not_connected'

// ─── invoke 包装 ─────────────────────────────────────────────────────────

export const gbrainListPages = (params: {
  limit?: number
  sort?: string
  pageType?: string
  tag?: string
  updatedAfter?: string
}): Promise<PageSummary[]> =>
  invoke('gbrain_list_pages', {
    limit: params.limit,
    sort: params.sort,
    pageType: params.pageType,
    tag: params.tag,
    updatedAfter: params.updatedAfter,
  })

export const gbrainGetPage = (slug: string): Promise<PageDetail> =>
  invoke('gbrain_get_page', { slug })

export const gbrainSearch = (
  query: string,
  limit = 20,
  offset = 0,
): Promise<SearchHit[]> => invoke('gbrain_search', { query, limit, offset })

export const gbrainGetBacklinks = (slug: string): Promise<Backlink[]> =>
  invoke('gbrain_get_backlinks', { slug })

export const gbrainTraverseGraph = (
  slug: string,
  depth = 2,
  direction?: string,
): Promise<unknown> => invoke('gbrain_traverse_graph', { slug, depth, direction })

export const gbrainGetVersions = (slug: string): Promise<VersionMeta[]> =>
  invoke('gbrain_get_versions', { slug })

export const gbrainRevertVersion = (
  slug: string,
  versionId: number,
): Promise<PageDetail> =>
  invoke('gbrain_revert_version', { slug, versionId })

export const gbrainPutPage = (
  slug: string,
  content: string,
): Promise<PageDetail> => invoke('gbrain_put_page', { slug, content })

export const gbrainGetStats = (): Promise<BrainStats> =>
  invoke('gbrain_get_stats', {})

export const gbrainFindOrphans = (): Promise<OrphanSummary> =>
  invoke('gbrain_find_orphans', {})
```

> Tauri 的 `invoke` 参数名做 camelCase↔snake_case 自动转换：Rust 命令参数 `page_type` / `updated_after` / `version_id` 对应 TS 的 `pageType` / `updatedAfter` / `versionId`。`snake_case` 的 Rust 参数（如 `slug`、`query`、`limit`、`offset`、`content`、`depth`、`direction`、`tag`、`sort`）保持原样。

- [ ] **Step 2: TS 检查**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: 无 gbrain-browse.ts 相关错误。

- [ ] **Step 3: 提交**

```bash
git add ui/src/lib/gbrain-browse.ts
git commit -m "feat(ui): gbrain-browse.ts IPC wrapper + types (mirror Rust)"
```

---

## Task 6: WikiView 重写 — 浏览（overview + 列表 + 搜索 + 详情 + 反向链接）

**Files:**
- Modify: `ui/src/components/memory/WikiView.tsx`（整体重写数据层；保留三区视觉骨架 + 主题 token）

> 说明：旧 WikiView 的 export/sync/regenerate 逻辑绑定 memory_graph，对 gbrain 不适用，整体替换。保留：根容器 `cn('flex flex-col h-full bg-popover text-foreground', className)` + `data-testid="wiki-view"`、header 布局、`ScrollArea`/`Badge`/`Button`、`ReactMarkdown` 包在 `prose prose-sm dark:prose-invert max-w-none text-xs`、主题 token。挂载点 `MemoryModule.tsx` 的 `activeTab==='wiki'` 不变（props 仍 `{spaceId?, className?}`，spaceId 现忽略——gbrain 不分 workspace）。

- [ ] **Step 1: 整体重写 `WikiView.tsx`（本 Task 实现只读浏览，Task 7 加编辑/版本）**

```tsx
import * as React from 'react'
import ReactMarkdown from 'react-markdown'
import { Loader2, FileText, Search as SearchIcon, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'
import {
  gbrainListPages,
  gbrainGetPage,
  gbrainSearch,
  gbrainGetBacklinks,
  gbrainGetStats,
  gbrainFindOrphans,
  GBRAIN_NOT_CONNECTED,
  type PageSummary,
  type PageDetail,
  type SearchHit,
  type Backlink,
  type BrainStats,
  type OrphanSummary,
} from '@/lib/gbrain-browse'

interface WikiViewProps {
  spaceId?: string
  className?: string
}

function isNotConnected(e: unknown): boolean {
  return String(e).includes(GBRAIN_NOT_CONNECTED)
}

export function WikiView({ className }: WikiViewProps): React.ReactElement {
  const [pages, setPages] = React.useState<PageSummary[]>([])
  const [stats, setStats] = React.useState<BrainStats | null>(null)
  const [orphans, setOrphans] = React.useState<OrphanSummary | null>(null)
  const [selectedSlug, setSelectedSlug] = React.useState<string | null>(null)
  const [detail, setDetail] = React.useState<PageDetail | null>(null)
  const [backlinks, setBacklinks] = React.useState<Backlink[]>([])
  const [typeFilter, setTypeFilter] = React.useState<string>('')
  const [searchQuery, setSearchQuery] = React.useState<string>('')
  const [searchHits, setSearchHits] = React.useState<SearchHit[] | null>(null)
  const [loadingList, setLoadingList] = React.useState(true)
  const [loadingDetail, setLoadingDetail] = React.useState(false)
  const [notConnected, setNotConnected] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  // ─── 加载列表 + overview ───────────────────────────────────────────────
  const loadList = React.useCallback(async () => {
    setLoadingList(true)
    setError(null)
    try {
      const list = await gbrainListPages({ limit: 200, sort: 'updated_desc' })
      setPages(list)
      setNotConnected(false)
    } catch (e) {
      if (isNotConnected(e)) {
        setNotConnected(true)
      } else {
        setError(`加载页面列表失败: ${String(e)}`)
      }
    } finally {
      setLoadingList(false)
    }
    // overview 单独 try（get_stats 是 admin scope，失败不应阻断列表）
    try {
      setStats(await gbrainGetStats())
    } catch {
      setStats(null)
    }
    try {
      setOrphans(await gbrainFindOrphans())
    } catch {
      setOrphans(null)
    }
  }, [])

  React.useEffect(() => {
    void loadList()
  }, [loadList])

  // ─── 选中页 → 加载详情 + 反向链接 ──────────────────────────────────────
  const openPage = React.useCallback(async (slug: string) => {
    setSelectedSlug(slug)
    setLoadingDetail(true)
    setError(null)
    try {
      const [d, bl] = await Promise.all([
        gbrainGetPage(slug),
        gbrainGetBacklinks(slug).catch(() => [] as Backlink[]),
      ])
      setDetail(d)
      setBacklinks(bl)
    } catch (e) {
      setError(`加载页面失败: ${String(e)}`)
      setDetail(null)
    } finally {
      setLoadingDetail(false)
    }
  }, [])

  // ─── 搜索 ───────────────────────────────────────────────────────────────
  const runSearch = React.useCallback(async () => {
    const q = searchQuery.trim()
    if (!q) {
      setSearchHits(null)
      return
    }
    try {
      setSearchHits(await gbrainSearch(q, 30))
    } catch (e) {
      setError(`搜索失败: ${String(e)}`)
    }
  }, [searchQuery])

  // 列表按 type 过滤（gbrain list_pages 无 offset，V1 客户端过滤 200 上限）
  const types = React.useMemo(
    () => Array.from(new Set(pages.map((p) => p.type).filter(Boolean))).sort(),
    [pages],
  )
  const filteredPages = React.useMemo(
    () => (typeFilter ? pages.filter((p) => p.type === typeFilter) : pages),
    [pages, typeFilter],
  )

  // ─── 空状态：gbrain 未连接 ─────────────────────────────────────────────
  if (notConnected) {
    return (
      <div
        className={cn('flex flex-col items-center justify-center h-full bg-popover text-foreground gap-3', className)}
        data-testid="wiki-view"
      >
        <FileText className="size-8 text-muted-foreground" />
        <p className="text-sm text-muted-foreground">gbrain 未连接</p>
        <p className="text-xs text-muted-foreground">请到 设置 › 系统 检查 gbrain MCP 状态</p>
        <Button size="sm" variant="outline" onClick={() => void loadList()}>
          <RefreshCw className="size-3 mr-1" /> 重试
        </Button>
      </div>
    )
  }

  return (
    <div
      className={cn('flex flex-col h-full bg-popover text-foreground', className)}
      data-testid="wiki-view"
    >
      {/* Header + Overview */}
      <div className="px-3 py-2 border-b border-border/50">
        <div className="flex items-center gap-2 mb-2">
          <FileText className="size-4 text-muted-foreground" />
          <span className="text-xs font-medium">知识 Wiki · gbrain</span>
          {stats && (
            <span className="text-[10px] text-muted-foreground">
              {stats.page_count} 页 · {stats.chunk_count} 块 ·{' '}
              {stats.chunk_count > 0
                ? Math.round((stats.embedded_count / stats.chunk_count) * 100)
                : 0}
              % 已嵌入
            </span>
          )}
          {orphans && orphans.total_orphans > 0 && (
            <Badge variant="outline" className="text-[10px] px-1.5 py-0 border-amber-500/50 text-amber-500">
              {orphans.total_orphans} 孤儿页
            </Badge>
          )}
          <Button size="sm" variant="ghost" className="ml-auto h-7 text-xs gap-1" onClick={() => void loadList()}>
            <RefreshCw className="size-3" /> 刷新
          </Button>
        </div>
        {/* 搜索框 */}
        <div className="flex items-center gap-1">
          <SearchIcon className="size-3 text-muted-foreground" />
          <input
            className="flex-1 bg-muted/20 rounded px-2 py-1 text-xs outline-none focus:bg-muted/40"
            placeholder="搜索知识库…"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void runSearch()
            }}
            data-testid="wiki-search-input"
          />
          {searchHits !== null && (
            <Button size="sm" variant="ghost" className="h-6 text-[10px]" onClick={() => { setSearchQuery(''); setSearchHits(null) }}>
              清除
            </Button>
          )}
        </div>
      </div>

      {error && (
        <div className="px-3 py-1.5 bg-destructive/10 text-destructive text-xs">{error}</div>
      )}

      {/* 列表 + 详情 */}
      <div className="flex flex-1 min-h-0">
        {/* 左：列表 / 搜索结果 */}
        <div className="w-64 border-r border-border/50 flex flex-col min-h-0">
          {searchHits === null && (
            <div className="px-2 py-1.5 border-b border-border/40">
              <select
                className="w-full bg-muted/20 rounded px-1.5 py-1 text-xs outline-none"
                value={typeFilter}
                onChange={(e) => setTypeFilter(e.target.value)}
                data-testid="wiki-type-filter"
              >
                <option value="">全部类型 ({pages.length})</option>
                {types.map((t) => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
            </div>
          )}
          <ScrollArea className="flex-1">
            {loadingList ? (
              <div className="flex items-center justify-center p-4">
                <Loader2 className="size-4 animate-spin text-muted-foreground" />
              </div>
            ) : searchHits !== null ? (
              searchHits.length === 0 ? (
                <p className="p-3 text-xs text-muted-foreground">无搜索结果</p>
              ) : (
                searchHits.map((h) => (
                  <button
                    key={`${h.slug}-${h.snippet.slice(0, 8)}`}
                    className={cn(
                      'w-full text-left px-3 py-1.5 text-xs hover:bg-muted/60',
                      selectedSlug === h.slug && 'bg-accent text-accent-foreground',
                    )}
                    onClick={() => void openPage(h.slug)}
                  >
                    <div className="font-medium truncate">{h.title || h.slug}</div>
                    <div className="text-[10px] text-muted-foreground truncate">{h.snippet}</div>
                  </button>
                ))
              )
            ) : filteredPages.length === 0 ? (
              <p className="p-3 text-xs text-muted-foreground">无页面</p>
            ) : (
              filteredPages.map((p) => (
                <button
                  key={p.slug}
                  className={cn(
                    'w-full text-left px-3 py-1.5 text-xs hover:bg-muted/60',
                    selectedSlug === p.slug && 'bg-accent text-accent-foreground',
                  )}
                  onClick={() => void openPage(p.slug)}
                  data-testid="wiki-list-item"
                >
                  <div className="font-medium truncate">{p.title || p.slug}</div>
                  <div className="text-[10px] text-muted-foreground">{p.type}</div>
                </button>
              ))
            )}
          </ScrollArea>
        </div>

        {/* 右：详情 */}
        <div className="flex-1 flex flex-col min-h-0">
          {loadingDetail ? (
            <div className="flex items-center justify-center flex-1">
              <Loader2 className="size-5 animate-spin text-muted-foreground" />
            </div>
          ) : detail ? (
            <ScrollArea className="flex-1">
              <div className="p-4">
                <div className="flex items-center gap-2 mb-2">
                  <h2 className="text-sm font-semibold">{detail.title || detail.slug}</h2>
                  <Badge variant="outline" className="text-[10px]">{detail.type}</Badge>
                </div>
                <div className="prose prose-sm dark:prose-invert max-w-none text-xs" data-testid="wiki-detail-body">
                  <ReactMarkdown>{detail.compiled_truth}</ReactMarkdown>
                </div>
                {/* 反向链接 */}
                <div className="mt-4 pt-3 border-t border-border/40">
                  <div className="text-[10px] uppercase text-muted-foreground mb-1">反向链接</div>
                  {backlinks.length === 0 ? (
                    <p className="text-xs text-muted-foreground">无反向链接</p>
                  ) : (
                    <div className="flex flex-col gap-0.5" data-testid="wiki-backlinks">
                      {backlinks.map((b) => (
                        <button
                          key={`${b.from_slug}-${b.link_type}`}
                          className="text-left text-xs text-muted-foreground hover:text-foreground hover:underline"
                          onClick={() => void openPage(b.from_slug)}
                        >
                          · {b.from_slug} <span className="opacity-60">({b.link_type})</span>
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </ScrollArea>
          ) : (
            <div className="flex items-center justify-center flex-1">
              <p className="text-xs text-muted-foreground">选择一个页面查看</p>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: TS 检查**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -15`
Expected: 无 WikiView/gbrain-browse 相关错误。若 `ScrollArea`/`Badge`/`Button` 导入路径报错，确认它们存在于 `@/components/ui/`（旧 WikiView 已用，路径不变）。

- [ ] **Step 3: 提交**

```bash
git add ui/src/components/memory/WikiView.tsx
git commit -m "feat(ui): repurpose WikiView data source to gbrain (list/search/detail/backlinks/overview)"
```

---

## Task 7: WikiView 编辑 + 版本史

**Files:**
- Modify: `ui/src/components/memory/WikiView.tsx`

- [ ] **Step 1: 在 import 区补图标 + 命令**

把 Task 6 的 lucide 导入行改为：

```tsx
import { Loader2, FileText, Search as SearchIcon, RefreshCw, Pencil, History, X } from 'lucide-react'
```

把 gbrain-browse 导入补两行命令 + 一个类型：

```tsx
  gbrainPutPage,
  gbrainGetVersions,
  gbrainRevertVersion,
  type VersionMeta,
```

- [ ] **Step 2: 在组件内（state 区末尾）加编辑/版本 state**

```tsx
  const [editing, setEditing] = React.useState(false)
  const [draft, setDraft] = React.useState('')
  const [saving, setSaving] = React.useState(false)
  const [versionsOpen, setVersionsOpen] = React.useState(false)
  const [versions, setVersions] = React.useState<VersionMeta[]>([])
  const [loadingVersions, setLoadingVersions] = React.useState(false)
```

- [ ] **Step 3: 加编辑/版本 handler（放在 runSearch 之后）**

```tsx
  const startEdit = React.useCallback(() => {
    if (!detail) return
    setDraft(detail.raw_markdown)
    setEditing(true)
  }, [detail])

  const saveEdit = React.useCallback(async () => {
    if (!detail) return
    setSaving(true)
    setError(null)
    try {
      const updated = await gbrainPutPage(detail.slug, draft)
      setDetail(updated)
      setEditing(false)
      // 保存后反链可能变化，刷新
      gbrainGetBacklinks(detail.slug).then(setBacklinks).catch(() => {})
    } catch (e) {
      setError(`保存失败: ${String(e)}`)
    } finally {
      setSaving(false)
    }
  }, [detail, draft])

  const openVersions = React.useCallback(async () => {
    if (!detail) return
    setVersionsOpen(true)
    setLoadingVersions(true)
    try {
      setVersions(await gbrainGetVersions(detail.slug))
    } catch (e) {
      setError(`加载版本史失败: ${String(e)}`)
    } finally {
      setLoadingVersions(false)
    }
  }, [detail])

  const revertTo = React.useCallback(async (versionId: number) => {
    if (!detail) return
    try {
      const reverted = await gbrainRevertVersion(detail.slug, versionId)
      setDetail(reverted)
      setVersionsOpen(false)
    } catch (e) {
      setError(`回滚失败: ${String(e)}`)
    }
  }, [detail])
```

- [ ] **Step 4: 在详情 header（`<h2>...</h2>` 那行所在的 flex 容器）追加 编辑 / 版本史 按钮**

把 Task 6 详情区的 header div 改为：

```tsx
                <div className="flex items-center gap-2 mb-2">
                  <h2 className="text-sm font-semibold">{detail.title || detail.slug}</h2>
                  <Badge variant="outline" className="text-[10px]">{detail.type}</Badge>
                  <div className="ml-auto flex items-center gap-1">
                    {!editing && (
                      <>
                        <Button size="sm" variant="ghost" className="h-6 text-[10px] gap-1" onClick={startEdit} data-testid="wiki-edit-btn">
                          <Pencil className="size-3" /> 编辑
                        </Button>
                        <Button size="sm" variant="ghost" className="h-6 text-[10px] gap-1" onClick={() => void openVersions()} data-testid="wiki-versions-btn">
                          <History className="size-3" /> 版本史
                        </Button>
                      </>
                    )}
                  </div>
                </div>
```

- [ ] **Step 5: 在详情正文区——把 `<div className="prose ...">` 那段包成 编辑/渲染 二选一**

替换 Task 6 的正文渲染块为：

```tsx
                {editing ? (
                  <div className="flex flex-col gap-2" data-testid="wiki-editor">
                    <textarea
                      className="w-full min-h-[300px] bg-muted/20 rounded p-2 text-xs font-mono outline-none focus:bg-muted/30"
                      value={draft}
                      onChange={(e) => setDraft(e.target.value)}
                    />
                    <div className="flex items-center gap-2">
                      <Button size="sm" className="h-7 text-xs" onClick={() => void saveEdit()} disabled={saving} data-testid="wiki-save-btn">
                        {saving ? <Loader2 className="size-3 animate-spin mr-1" /> : null}
                        保存
                      </Button>
                      <Button size="sm" variant="ghost" className="h-7 text-xs" onClick={() => setEditing(false)} disabled={saving}>
                        取消
                      </Button>
                    </div>
                  </div>
                ) : (
                  <div className="prose prose-sm dark:prose-invert max-w-none text-xs" data-testid="wiki-detail-body">
                    <ReactMarkdown>{detail.compiled_truth}</ReactMarkdown>
                  </div>
                )}
```

- [ ] **Step 6: 在组件根 `</div>` 前加版本史抽屉（覆盖层）**

在最外层 `<div data-testid="wiki-view">` 闭合前插入：

```tsx
      {versionsOpen && (
        <div className="absolute inset-0 bg-background/80 flex justify-end" data-testid="wiki-version-drawer">
          <div className="w-80 h-full bg-popover border-l border-border/50 flex flex-col">
            <div className="flex items-center justify-between px-3 py-2 border-b border-border/50">
              <span className="text-xs font-medium">版本史</span>
              <Button size="sm" variant="ghost" className="h-6 w-6 p-0" onClick={() => setVersionsOpen(false)}>
                <X className="size-3" />
              </Button>
            </div>
            <ScrollArea className="flex-1">
              {loadingVersions ? (
                <div className="flex justify-center p-4"><Loader2 className="size-4 animate-spin text-muted-foreground" /></div>
              ) : versions.length === 0 ? (
                <p className="p-3 text-xs text-muted-foreground">无历史版本</p>
              ) : (
                versions.map((v) => (
                  <div key={v.id} className="px-3 py-2 border-b border-border/30">
                    <div className="flex items-center justify-between">
                      <span className="text-[10px] text-muted-foreground">{v.snapshot_at ?? `#${v.id}`}</span>
                      <Button size="sm" variant="ghost" className="h-5 text-[10px]" onClick={() => void revertTo(v.id)}>
                        回滚到此版本
                      </Button>
                    </div>
                    <div className="text-[10px] text-muted-foreground mt-1 line-clamp-2">{v.compiled_truth.slice(0, 120)}</div>
                  </div>
                ))
              )}
            </ScrollArea>
          </div>
        </div>
      )}
```

> 抽屉用 `absolute inset-0` 覆盖，根容器需 `relative`。把最外层 `<div className={cn('flex flex-col h-full bg-popover text-foreground', className)}` 改为加 `relative`：`cn('relative flex flex-col h-full bg-popover text-foreground', className)`。

- [ ] **Step 7: TS 检查 + 编译确认**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -15`
Expected: 无 WikiView 相关错误。

- [ ] **Step 8: 提交**

```bash
git add ui/src/components/memory/WikiView.tsx
git commit -m "feat(ui): WikiView edit (raw markdown) + version-history drawer + revert"
```

---

## Task 8: WikiView Vitest（mock invoke）

**Files:**
- Create: `ui/src/components/memory/WikiView.test.tsx`

- [ ] **Step 1: 创建测试文件**

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { WikiView } from './WikiView'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}))

function routeInvoke(overrides: Record<string, unknown> = {}) {
  invokeMock.mockImplementation((cmd: string) => {
    const table: Record<string, unknown> = {
      gbrain_list_pages: [
        { slug: 'person-alice', title: 'Alice', type: 'person', updated_at: '2026-05-10T00:00:00Z' },
        { slug: 'concept-fts', title: 'FTS', type: 'concept', updated_at: '2026-05-09T00:00:00Z' },
      ],
      gbrain_get_stats: { page_count: 2, chunk_count: 10, embedded_count: 8, link_count: 3, tag_count: 1 },
      gbrain_find_orphans: { total_orphans: 1, total_pages: 2 },
      gbrain_get_page: {
        slug: 'person-alice', title: 'Alice', type: 'person',
        compiled_truth: '# Alice\nFounder of Acme.', frontmatter: { type: 'person' },
        created_at: null, updated_at: null, tags: [], raw_markdown: '---\ntype: person\n---\n\n# Alice\nFounder of Acme.',
      },
      gbrain_get_backlinks: [{ from_slug: 'project-falcon', link_type: 'works_at' }],
      gbrain_search: [{ slug: 'concept-fts', title: 'FTS', snippet: 'full text search', similarity: 0.9 }],
      gbrain_get_versions: [{ id: 3, snapshot_at: '2026-05-05T00:00:00Z', compiled_truth: 'old body' }],
      gbrain_put_page: { slug: 'person-alice', title: 'Alice', type: 'person', compiled_truth: '# Alice\nEdited.', frontmatter: { type: 'person' }, created_at: null, updated_at: null, tags: [], raw_markdown: '---\ntype: person\n---\n\n# Alice\nEdited.' },
      gbrain_revert_version: { slug: 'person-alice', title: 'Alice', type: 'person', compiled_truth: 'old body', frontmatter: {}, created_at: null, updated_at: null, tags: [], raw_markdown: 'old body' },
      ...overrides,
    }
    const v = table[cmd]
    if (v instanceof Error) return Promise.reject(v)
    return Promise.resolve(v)
  })
}

describe('WikiView', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    routeInvoke()
  })

  it('renders page list from gbrain_list_pages', async () => {
    renderWithProviders(<WikiView />)
    expect(await screen.findByText('Alice')).toBeInTheDocument()
    expect(screen.getByText('FTS')).toBeInTheDocument()
  })

  it('shows overview stats + orphan badge', async () => {
    renderWithProviders(<WikiView />)
    await screen.findByText('Alice')
    expect(screen.getByText(/2 页/)).toBeInTheDocument()
    expect(screen.getByText(/1 孤儿页/)).toBeInTheDocument()
  })

  it('opens a page and renders markdown + backlinks', async () => {
    const { user } = renderWithProviders(<WikiView />)
    await user.click(await screen.findByText('Alice'))
    await waitFor(() => expect(screen.getByTestId('wiki-detail-body')).toHaveTextContent('Founder of Acme'))
    expect(screen.getByTestId('wiki-backlinks')).toHaveTextContent('project-falcon')
  })

  it('search switches list to result mode', async () => {
    const { user } = renderWithProviders(<WikiView />)
    await screen.findByText('Alice')
    const input = screen.getByTestId('wiki-search-input')
    await user.type(input, 'full text{Enter}')
    await waitFor(() => expect(screen.getByText('full text search')).toBeInTheDocument())
  })

  it('edit flow saves via gbrain_put_page', async () => {
    const { user } = renderWithProviders(<WikiView />)
    await user.click(await screen.findByText('Alice'))
    await user.click(await screen.findByTestId('wiki-edit-btn'))
    await user.click(screen.getByTestId('wiki-save-btn'))
    await waitFor(() => expect(screen.getByTestId('wiki-detail-body')).toHaveTextContent('Edited'))
    expect(invokeMock).toHaveBeenCalledWith('gbrain_put_page', expect.objectContaining({ slug: 'person-alice' }))
  })

  it('version drawer lists versions and reverts', async () => {
    const { user } = renderWithProviders(<WikiView />)
    await user.click(await screen.findByText('Alice'))
    await user.click(await screen.findByTestId('wiki-versions-btn'))
    const drawer = await screen.findByTestId('wiki-version-drawer')
    expect(drawer).toHaveTextContent('回滚到此版本')
    await user.click(screen.getByText('回滚到此版本'))
    await waitFor(() => expect(screen.getByTestId('wiki-detail-body')).toHaveTextContent('old body'))
  })

  it('shows not-connected empty state', async () => {
    routeInvoke({ gbrain_list_pages: new Error('gbrain_not_connected') })
    renderWithProviders(<WikiView />)
    expect(await screen.findByText('gbrain 未连接')).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: 跑 Vitest**

Run: `cd ui && npm test -- --run WikiView 2>&1 | tail -25`
Expected: 7 个测试全 PASS。若 `renderWithProviders` 返回值无 `user`，确认 `ui/src/test-utils/render.tsx` 暴露 `user`（CLAUDE.md 已说明它返回 `{container, user, store, ...screen, waitFor, fireEvent}`）。若 `react-markdown` 在 jsdom 报错，按既有测试惯例可不 mock（旧 memory 测试未 mock 它）。

- [ ] **Step 3: 提交**

```bash
git add ui/src/components/memory/WikiView.test.tsx
git commit -m "test(ui): WikiView vitest — list/detail/backlinks/search/edit/version/empty-state"
```

---

## 手动 E2E 验证清单（需真 gbrain 连接，写进 PR 描述）

1. 启动 `cargo tauri dev`（确保 `bunembed/` + `gbrain-source/` 已 bootstrap，gbrain MCP 已连接——设置 › 系统 看状态）。
2. 打开 万花筒 › 记忆 › Wiki tab → 列表应显示 gbrain 页面 + overview 统计 + 孤儿计数。
3. 点 type 过滤下拉 → 列表收窄。
4. 搜索框输入关键词回车 → 切换到搜索结果模式 → 点结果打开页。
5. 详情区渲染 compiled_truth markdown + 反向链接面板 → 点反链跳转。
6. **admin scope 验证**：确认 overview 的"N 页/N 块/N% 已嵌入"有数字（证明 `get_stats` 在 stdio 下通）。若空白但列表正常 → `get_stats` 被 admin gate 拦了，记录到 PR，按需后续在 gbrain stdio server 放开 admin scope（不在 A 范围）。
7. 点"编辑" → textarea 显示完整 markdown（frontmatter + 正文）→ 改几个字 → 保存 → 详情重渲染为新内容。
8. 点"版本史" → 抽屉列出版本（含刚才保存产生的新版本）→ 点"回滚到此版本" → 详情回到旧内容。
9. 断开 gbrain（停 MCP）后刷新 → 显示"gbrain 未连接"空状态卡片 + 重试按钮。

---

## 自检（写计划后对照 spec）

- **Spec 覆盖**：spec §3 的 10 命令 → Task 4 全部注册；§4 三区 → Task 6；§5 编辑+版本 → Task 7；§6 错误处理 → `GbrainError` + 空状态 + 各 try/catch；§7 测试 → Task 1（Rust parse）+ Task 8（Vitest）+ 手动 E2E 清单；§8 范围边界（traverse_graph 建命令不渲染、不碰 memory_nodes、不做摄入）→ 遵守。
- **占位符**：无（已清除 call_gbrain 里的临时标注）。
- **类型一致**：Rust `PageDetail.raw_markdown`/`page_type`(`#[serde(rename="type")]`) ↔ TS `PageDetail.raw_markdown`/`type`；`VersionMeta.id`(i64) ↔ TS `number`；`gbrain_revert_version` 参数 `version_id`(Rust) ↔ `versionId`(TS invoke camelCase)。命令名 snake_case 两侧一致。
- **范围**：单 PR，8 commit（≈ spec §9 的 7 commit 形状，多拆了读/写两步），无新 migration、无新依赖（`serde_yml` 已在）。
