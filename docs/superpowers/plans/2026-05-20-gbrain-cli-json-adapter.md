# gbrain CLI→MCP-JSON 适配器(Path 2)实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新增 `gbrain/cli_format.rs` 适配器,把 `GbrainCliTransport` 跑 gbrain CLI 得到的真实输出(部分文本、部分 JSON)归一成 A 的 `browse::parse_*` 期望的 MCP JSON,并给 `call_cli` 补全缺失 op 映射,使 A(wiki)+ C(双星云)的 gbrain 功能真正可用。

**Architecture:** `call_cli` 拿到 CLI stdout 后经 `cli_format::to_mcp_json(op, args, stdout)` 归一成 JSON 文本 → 包进现有 `{content:[{text}]}` 信封;`browse.rs` parser 完全不动(transport 无关)。fragile 的 CLI 格式知识全部隔离在 `cli_format.rs`,用真实 fixture 单测兜底。

**Tech Stack:** Rust（serde_json、serde_yml、tokio process）。

---

## 已核对的事实(真实 CLI 输出 — 实跑抓取)

`GBRAIN_HOME=~/.uclaw/gbrain bunembed/bun gbrain-source/src/cli.ts <cmd>`(brain 有 10 页):

- **list** `-n 3`:每行 `slug\ttype\tYYYY-MM-DD\ttitle`(TAB 分隔)。如 `people/ryanliu\tperson\t2026-05-20\tRyan Liu`。`--json` **无效**(输出相同)。
- **stats**:
  ```
  Pages:     10
  Chunks:    10
  Embedded:  0
  Links:     1
  Tags:      21
  Timeline:  0

  By type:
    reference: 3
    ...
  ```
- **get `<slug>`**:`---`\nYAML(type/title/aliases/tags)\n`---`\n\n# 正文markdown…(正文=compiled_truth)。
- **search `<q>`**:每条 `[0.3648] people/ryanliu -- # Ryan Liu (刘磊)\n…snippet 跨行…`。
- **backlinks `<slug>`**:**JSON** `[{"from_slug","to_slug","link_type","context","link_source","origin_slug","origin_field"}]`。
- **graph `<slug>` --depth 1**:**JSON** `[{"slug","title","type","depth","links":[{"to_slug","link_type"}]}]`。
- **orphans --json**:**JSON** `{"orphans":[{"slug","title","domain"}],...}`(前有 `[orphans.scan] start/done` 进度行)。
- **history `<slug>`**:每条 `#6  2026-05-20T04:33:40  # Ryan Liu (刘磊)…预览…`。
- **revert `<slug> <version-id>`**:命令存在(help 1567)。**无 `links`/`get-links` CLI 命令** → `get_links` 用 `graph <slug> --depth 1`。
- 噪声 `[ai.gateway] recipe "google"…`(每次首行)+ `[orphans.scan] …` 在 **stderr**;`call_cli` 取 stdout(`if stdout.is_empty() && !stderr.is_empty()` 才回 stderr)。仍防御性 strip。

**`call_cli`(mcp.rs ~1163)结构**:`match tool { "search"|"query"|"list_pages"|"think"|"get_page"|"put_page" => 构 argv … , other => Err }`,末尾跑 `cmd.output()`,`let stdout = ...trim()`,成功返回 `Ok(stdout)`(或 stderr 兜底)。helper:`push_number_flag(&mut argv,&args,"k","--flag")`、`push_string_flag`、`push_bool_flag`、`required_string(&args,"k")->Result<String>`、`optional_string`。`send`(~1379)把 call_cli 的返回包成 `{content:[{type:"text",text}],isError:false}`。

**browse.rs op 名(transport 收到的)**:`list_pages/get_page/search/get_backlinks/traverse_graph/get_versions/revert_version/put_page/get_stats/find_orphans/get_links`。已映射 6 个,需补 7 个。

**browse parser 期望的 JSON 形状(目标)**:
- PageSummary `{slug,title,type,updated_at}`;PageDetail `{slug,title,type,compiled_truth,frontmatter,tags,...}`;SearchHit `{slug,title,chunk_text,score}`(rename snippet←chunk_text, similarity←score);Backlink `{from_slug,link_type}`(parse_backlinks 取这俩,余字段忽略);VersionMeta `{id,snapshot_at,compiled_truth}`;BrainStats `{page_count,chunk_count,embedded_count,link_count,tag_count}`;OrphanSummary `{total_orphans,total_pages}`(orphans JSON 有 `orphans[]` 但**没有** total_* —— 见 Task 1 注);KnowledgeEdge `{from_slug,to_slug,link_type}`。

**验证命令**：`cd src-tauri && cargo test --lib gbrain::cli_format > /tmp/cf.txt 2>&1; grep "test result" /tmp/cf.txt`；`cargo build > /tmp/b.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/b.txt|head`。IRON RULE:重定向到文件再 grep。

---

## 文件结构

| 文件 | 职责 |
|---|---|
| `src-tauri/src/gbrain/cli_format.rs` (新) | `strip_noise` + `to_mcp_json(op,args,stdout)` 分派 + 各 op 文本/JSON→MCP JSON + fixture 单测 |
| `src-tauri/src/gbrain/mod.rs` (改) | 加 `pub mod cli_format;` |
| `src-tauri/src/mcp.rs` (改) | `call_cli` 补 7 op→argv 映射 + 末尾经 `cli_format::to_mcp_json` 归一 |

---

## Task 1: cli_format.rs 骨架 + strip_noise + JSON 透传 + list/stats 转换 + 测试

**Files:** Create `src-tauri/src/gbrain/cli_format.rs`; Modify `src-tauri/src/gbrain/mod.rs`.

- [ ] **Step 1: mod.rs 加 `pub mod cli_format;`**

- [ ] **Step 2: cli_format.rs — 写骨架 + strip_noise + 分派 + list/stats + orphans/passthrough**

```rust
//! gbrain CLI 输出 → MCP JSON 适配器。GbrainCliTransport 跑 gbrain CLI 得到的
//! 输出部分是人类文本、部分已是 JSON;这里统一归一成 browse::parse_* 期望的
//! JSON 文本。fragile 的 CLI 格式知识集中于此,真实 fixture 单测兜底。

use crate::mcp::McpError;

/// 去掉 gbrain 的日志/进度噪声行(`[ai.gateway] …`、`[orphans.scan] …`)。
/// 判据:行首 `[tag]` 且 tag 含 ASCII 字母(日志标签)。search 的 `[0.36]`
/// 是纯数字 tag → 不误删;JSON 的 `[` 后跟 `{`/`]`/空白 → 不误删。
pub fn strip_noise(s: &str) -> String {
    s.lines()
        .filter(|line| !is_log_line(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_log_line(line: &str) -> bool {
    let t = line.trim_start();
    if !t.starts_with('[') {
        return false;
    }
    let Some(close) = t.find(']') else { return false; };
    let tag = &t[1..close];
    !tag.is_empty()
        && tag.chars().any(|c| c.is_ascii_alphabetic())
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// 取 stdout 中第一个 JSON 值(`[` 或 `{` 起)到末尾,用于 JSON-native op。
fn extract_json(stdout: &str) -> Result<&str, McpError> {
    let s = stdout.trim_start();
    let start = s
        .find(|c| c == '[' || c == '{')
        .ok_or_else(|| McpError::Server("gbrain CLI: no JSON in output".into()))?;
    Ok(s[start..].trim_end())
}

/// 入口:把某 op 的 CLI stdout 归一成 browse::parse_* 期望的 JSON 文本。
pub fn to_mcp_json(
    op: &str,
    args: &serde_json::Value,
    stdout: &str,
) -> Result<String, McpError> {
    let cleaned = strip_noise(stdout);
    match op {
        // JSON-native:CLI 已吐 JSON,取出原样返回(parse_* 直接用)。
        "get_backlinks" | "traverse_graph" | "find_orphans" => {
            Ok(extract_json(&cleaned)?.to_string())
        }
        "list_pages" => list_to_json(&cleaned),
        "get_stats" => stats_to_json(&cleaned),
        // 后续 Task 2/3 接入:get_page / search / get_versions / get_links / revert_version
        // 其余(search/query/think/put_page 等)Task 2/3 前先原样返回,避免破坏:
        _ => Ok(cleaned),
    }
}

/// `list`:每行 `slug\ttype\tYYYY-MM-DD\ttitle` → `[{slug,type,title,updated_at}]`。
fn list_to_json(cleaned: &str) -> Result<String, McpError> {
    let rows: Vec<serde_json::Value> = cleaned
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let mut it = l.splitn(4, '\t');
            let slug = it.next()?.trim();
            if slug.is_empty() {
                return None;
            }
            let page_type = it.next().unwrap_or("").trim();
            let date = it.next().unwrap_or("").trim();
            let title = it.next().unwrap_or("").trim();
            Some(serde_json::json!({
                "slug": slug,
                "type": page_type,
                "title": title,
                "updated_at": date,
            }))
        })
        .collect();
    serde_json::to_string(&rows).map_err(|e| McpError::Server(e.to_string()))
}

/// `stats`:标签行 `Pages: N` … → `{page_count,chunk_count,embedded_count,link_count,tag_count}`。
fn stats_to_json(cleaned: &str) -> Result<String, McpError> {
    let mut get = |label: &str| -> i64 {
        cleaned
            .lines()
            .find_map(|l| {
                let l = l.trim();
                l.strip_prefix(label)
                    .and_then(|rest| rest.trim().split_whitespace().next())
                    .and_then(|n| n.parse::<i64>().ok())
            })
            .unwrap_or(0)
    };
    let obj = serde_json::json!({
        "page_count": get("Pages:"),
        "chunk_count": get("Chunks:"),
        "embedded_count": get("Embedded:"),
        "link_count": get("Links:"),
        "tag_count": get("Tags:"),
    });
    serde_json::to_string(&obj).map_err(|e| McpError::Server(e.to_string()))
}
```

> **orphans 注**:browse 的 `OrphanSummary` 期望 `{total_orphans,total_pages}`,但 CLI `orphans --json` 输出 `{orphans:[...], total_orphans, total_linkable, total_pages, excluded}`(spec §1 fixture 只截了 orphans[]，但 orphans.ts 返回含 total_*)。透传即可;若实跑发现 CLI 的 orphans JSON 缺 total_*,Task 3 加一个 orphans 专转换器从 `orphans.len()` 补 `total_orphans`。**实现时跑一次 `orphans --json` 确认有无 total_* 字段。**

- [ ] **Step 3: 加这些转换器的 fixture 单测(用真实输出)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_noise_drops_logs_keeps_search_score() {
        let s = "[ai.gateway] recipe \"google\" ...\n[0.36] people/ryanliu -- hi\ndata";
        let out = strip_noise(s);
        assert!(!out.contains("ai.gateway"));
        assert!(out.contains("[0.36] people/ryanliu"));
        assert!(out.contains("data"));
    }

    #[test]
    fn list_tab_text_to_json() {
        let stdout = "people/ryanliu\tperson\t2026-05-20\tRyan Liu\npeople/刘磊\talias\t2026-05-20\t刘磊 (别名页)";
        let json = to_mcp_json("list_pages", &serde_json::json!({}), stdout).unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["slug"], "people/ryanliu");
        assert_eq!(v[0]["type"], "person");
        assert_eq!(v[0]["title"], "Ryan Liu");
        assert_eq!(v[0]["updated_at"], "2026-05-20");
        assert_eq!(v[1]["slug"], "people/刘磊"); // CJK 不丢
    }

    #[test]
    fn list_empty_is_empty_array() {
        assert_eq!(to_mcp_json("list_pages", &serde_json::json!({}), "").unwrap(), "[]");
    }

    #[test]
    fn stats_labeled_lines_to_json() {
        let stdout = "Pages:     10\nChunks:    10\nEmbedded:  0\nLinks:     1\nTags:      21\nTimeline:  0\n\nBy type:\n  reference: 3";
        let json = to_mcp_json("get_stats", &serde_json::json!({}), stdout).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["page_count"], 10);
        assert_eq!(v["chunk_count"], 10);
        assert_eq!(v["embedded_count"], 0);
        assert_eq!(v["link_count"], 1);
        assert_eq!(v["tag_count"], 21);
    }

    #[test]
    fn backlinks_json_passthrough() {
        let stdout = "[ai.gateway] noise\n[{\"from_slug\":\"a\",\"to_slug\":\"b\",\"link_type\":\"mentions\",\"context\":\"\"}]";
        let json = to_mcp_json("get_backlinks", &serde_json::json!({}), stdout).unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(v[0]["from_slug"], "a");
        assert_eq!(v[0]["link_type"], "mentions");
    }
}
```

- [ ] **Step 4: 编译 + 测试**
  - `cd src-tauri && cargo test --lib gbrain::cli_format > /tmp/cf1.txt 2>&1; grep "test result" /tmp/cf1.txt`(5 passed)
  - `cargo build > /tmp/cfb1.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/cfb1.txt | head`(EXIT=0;此时 cli_format 未被 call_cli 调用 → 可能 dead_code 警告,Task 4 接入)

- [ ] **Step 5: 提交**
  ```bash
  git add src-tauri/src/gbrain/cli_format.rs src-tauri/src/gbrain/mod.rs
  git commit -m "feat(gbrain): cli_format.rs — strip_noise + list/stats converters + JSON passthrough + tests"
  ```

---

## Task 2: get_page(markdown) + search + get_versions 转换器 + 测试

**Files:** Modify `src-tauri/src/gbrain/cli_format.rs`.

- [ ] **Step 1: to_mcp_json 分派加 3 个 op,并实现转换器**（在 match 里把 `get_page`/`search`/`get_versions` 从 `_ =>` 移到专门分支）

```rust
        "get_page" => get_page_to_json(args, &cleaned),
        "search" => search_to_json(&cleaned),
        "get_versions" => versions_to_json(&cleaned),
```

```rust
/// `get`:`---`YAML`---`\n正文 → {slug,type,title,compiled_truth,frontmatter,tags}。
/// slug 取自入参(CLI get 输出不含 slug)。
fn get_page_to_json(args: &serde_json::Value, cleaned: &str) -> Result<String, McpError> {
    let slug = args.get("slug").and_then(|v| v.as_str()).unwrap_or("");
    let (frontmatter, body) = split_frontmatter(cleaned);
    let fm: serde_json::Value = if frontmatter.trim().is_empty() {
        serde_json::json!({})
    } else {
        // serde_yml → serde_json::Value
        serde_yml::from_str::<serde_yml::Value>(frontmatter)
            .ok()
            .and_then(|y| serde_json::to_value(y).ok())
            .unwrap_or_else(|| serde_json::json!({}))
    };
    let title = fm.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let page_type = fm.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let tags: Vec<String> = fm
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let obj = serde_json::json!({
        "slug": slug,
        "type": page_type,
        "title": title,
        "compiled_truth": body,
        "frontmatter": fm,
        "tags": tags,
    });
    serde_json::to_string(&obj).map_err(|e| McpError::Server(e.to_string()))
}

/// 切出 `---\n…\n---\n` 之间的 frontmatter 与其后的正文。无 frontmatter 时 ("", 全文)。
fn split_frontmatter(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            let after = &rest[end + 4..]; // 跳过 "\n---"
            let body = after.trim_start_matches('\n').trim_start();
            return (fm, body);
        }
    }
    ("", s)
}

/// `search`:每条 `[score] slug -- snippet…` → [{slug,title,chunk_text,score}]。
/// snippet 可能跨行;以下一条 `[score] ` 行为边界。CLI 无 title → 用 slug 兜底。
fn search_to_json(cleaned: &str) -> Result<String, McpError> {
    let mut hits: Vec<serde_json::Value> = Vec::new();
    let mut cur: Option<(f64, String, String)> = None; // (score, slug, snippet)
    let header = |line: &str| -> Option<(f64, String, String)> {
        let t = line.trim_start();
        let close = t.strip_prefix('[')?.find(']')?;
        let score: f64 = t[1..1 + close].parse().ok()?;
        let after = t[1 + close + 1..].trim_start();
        let (slug, snippet) = after.split_once(" -- ")?;
        Some((score, slug.trim().to_string(), snippet.trim().to_string()))
    };
    for line in cleaned.lines() {
        if let Some(h) = header(line) {
            if let Some((sc, sl, sn)) = cur.take() {
                hits.push(mk_hit(sc, sl, sn));
            }
            cur = Some(h);
        } else if let Some((_, _, snippet)) = cur.as_mut() {
            if !line.trim().is_empty() {
                snippet.push(' ');
                snippet.push_str(line.trim());
            }
        }
    }
    if let Some((sc, sl, sn)) = cur.take() {
        hits.push(mk_hit(sc, sl, sn));
    }
    serde_json::to_string(&hits).map_err(|e| McpError::Server(e.to_string()))
}

fn mk_hit(score: f64, slug: String, snippet: String) -> serde_json::Value {
    // chunk_text/score 对齐 SearchResult;title 用 slug 兜底(CLI 无独立 title)。
    let snippet = snippet.chars().take(200).collect::<String>();
    serde_json::json!({ "slug": slug, "title": slug, "chunk_text": snippet, "score": score })
}

/// `history`:每条 `#N  <ISO>  <preview>` → [{id:N, snapshot_at:ISO, compiled_truth:preview}]。
/// 注:N 是否就是 revert 接受的 version-id,Task 4 实跑验证。
fn versions_to_json(cleaned: &str) -> Result<String, McpError> {
    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut cur: Option<(i64, String, String)> = None;
    for line in cleaned.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix('#') {
            // `N  ISO  preview`
            let mut it = rest.splitn(3, char::is_whitespace).filter(|s| !s.is_empty());
            if let (Some(n), Some(iso)) = (it.next(), it.next()) {
                if let Ok(id) = n.parse::<i64>() {
                    if let Some((pid, piso, pprev)) = cur.take() {
                        out.push(mk_ver(pid, piso, pprev));
                    }
                    let preview = it.next().unwrap_or("").to_string();
                    cur = Some((id, iso.to_string(), preview));
                    continue;
                }
            }
        }
        if let Some((_, _, prev)) = cur.as_mut() {
            if !line.trim().is_empty() {
                prev.push(' ');
                prev.push_str(line.trim());
            }
        }
    }
    if let Some((id, iso, prev)) = cur.take() {
        out.push(mk_ver(id, iso, prev));
    }
    serde_json::to_string(&out).map_err(|e| McpError::Server(e.to_string()))
}

fn mk_ver(id: i64, iso: String, preview: String) -> serde_json::Value {
    let preview = preview.chars().take(200).collect::<String>();
    serde_json::json!({ "id": id, "snapshot_at": iso, "compiled_truth": preview })
}
```

- [ ] **Step 2: 加 fixture 单测**

```rust
    #[test]
    fn get_markdown_to_json() {
        let stdout = "---\ntype: person\ntitle: Ryan Liu\naliases:\n  - 刘磊\ntags:\n  - myself\n  - user\n---\n\n# Ryan Liu (刘磊)\n\n## 基本信息\n- 中文名: 刘磊";
        let json = to_mcp_json("get_page", &serde_json::json!({"slug":"people/ryanliu"}), stdout).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["slug"], "people/ryanliu");
        assert_eq!(v["type"], "person");
        assert_eq!(v["title"], "Ryan Liu");
        assert!(v["compiled_truth"].as_str().unwrap().contains("# Ryan Liu"));
        assert!(v["compiled_truth"].as_str().unwrap().contains("基本信息"));
        assert_eq!(v["tags"][0], "myself");
    }

    #[test]
    fn search_text_to_json() {
        let stdout = "[0.3648] people/ryanliu -- # Ryan Liu (刘磊)\n## 基本信息\n[0.2432] personal/ryanliu-edu -- 姓名 刘磊";
        let json = to_mcp_json("search", &serde_json::json!({}), stdout).unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["slug"], "people/ryanliu");
        assert!((v[0]["score"].as_f64().unwrap() - 0.3648).abs() < 1e-6);
        assert!(v[0]["chunk_text"].as_str().unwrap().contains("Ryan Liu"));
        assert_eq!(v[1]["slug"], "personal/ryanliu-edu");
    }

    #[test]
    fn history_text_to_json() {
        let stdout = "#6  2026-05-20T04:33:40  # Ryan Liu (刘磊)\n## 基本信息\n#4  2026-05-20T04:32:35  # Ryan Liu";
        let json = to_mcp_json("get_versions", &serde_json::json!({}), stdout).unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["id"], 6);
        assert_eq!(v[0]["snapshot_at"], "2026-05-20T04:33:40");
        assert_eq!(v[1]["id"], 4);
    }
```

- [ ] **Step 3: 测试 + 编译**
  - `cd src-tauri && cargo test --lib gbrain::cli_format > /tmp/cf2.txt 2>&1; grep "test result" /tmp/cf2.txt`(8 passed)
  - 确认 `serde_yml::from_str` / `serde_yml::Value` 可用(serde_yml 在 deps;若类型名是 `serde_yml::Value` 之外,按真实 API 调整 —— 用 `serde_yml::from_str::<serde_json::Value>(frontmatter)` 直接转 serde_json 也行,二选一)。

- [ ] **Step 4: 提交**
  ```bash
  git add src-tauri/src/gbrain/cli_format.rs
  git commit -m "feat(gbrain): cli_format get_page(markdown)/search/get_versions converters + tests"
  ```

---

## Task 3: get_links(graph→Link) + revert/orphans 收尾 + 测试

**Files:** Modify `src-tauri/src/gbrain/cli_format.rs`.

- [ ] **Step 1: to_mcp_json 分派加 `get_links` / `revert_version`**

```rust
        "get_links" => links_from_graph(args, &cleaned),
        "revert_version" => Ok("{\"status\":\"reverted\"}".to_string()),
```

```rust
/// `get_links` 用 `graph <slug> --depth 1` 实现:graph 输出 GraphNode[]
/// `[{slug,links:[{to_slug,link_type}]}]`,找 slug==入参的节点,其 links
/// → [{from_slug:入参slug, to_slug, link_type}](browse::parse_links 形状)。
fn links_from_graph(args: &serde_json::Value, cleaned: &str) -> Result<String, McpError> {
    let slug = args.get("slug").and_then(|v| v.as_str()).unwrap_or("");
    let nodes: serde_json::Value = serde_json::from_str(extract_json(cleaned)?)
        .map_err(|e| McpError::Server(format!("graph json: {e}")))?;
    let mut edges: Vec<serde_json::Value> = Vec::new();
    if let Some(arr) = nodes.as_array() {
        for node in arr {
            if node.get("slug").and_then(|s| s.as_str()) == Some(slug) {
                if let Some(links) = node.get("links").and_then(|l| l.as_array()) {
                    for l in links {
                        let to = l.get("to_slug").and_then(|s| s.as_str()).unwrap_or("");
                        let lt = l.get("link_type").and_then(|s| s.as_str()).unwrap_or("");
                        edges.push(serde_json::json!({
                            "from_slug": slug, "to_slug": to, "link_type": lt,
                        }));
                    }
                }
            }
        }
    }
    serde_json::to_string(&edges).map_err(|e| McpError::Server(e.to_string()))
}
```

> revert:browse::revert_version 调 CLI 成功后会 re-fetch get_page,不依赖此返回 shape;返回 `{"status":"reverted"}` 占位即可。**orphans total_* 兜底**:若 Task 1 Step 2 注里的实跑确认 CLI `orphans --json` 缺 `total_orphans`,在此加 `find_orphans` 专转换器从 `orphans` 数组长度补 `{total_orphans: len, total_pages: <从另一处或 0>}`;若 CLI 已含 total_*,保持透传不动。

- [ ] **Step 2: 加测试**

```rust
    #[test]
    fn get_links_from_graph_node() {
        let stdout = "[{\"slug\":\"a\",\"title\":\"A\",\"type\":\"concept\",\"depth\":0,\"links\":[{\"to_slug\":\"b\",\"link_type\":\"mentions\"},{\"to_slug\":\"c\",\"link_type\":\"refs\"}]}]";
        let json = to_mcp_json("get_links", &serde_json::json!({"slug":"a"}), stdout).unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["from_slug"], "a");
        assert_eq!(v[0]["to_slug"], "b");
        assert_eq!(v[0]["link_type"], "mentions");
        assert_eq!(v[1]["to_slug"], "c");
    }

    #[test]
    fn get_links_no_match_is_empty() {
        let stdout = "[{\"slug\":\"x\",\"links\":[]}]";
        let json = to_mcp_json("get_links", &serde_json::json!({"slug":"a"}), stdout).unwrap();
        assert_eq!(json, "[]");
    }
```

- [ ] **Step 3: 测试**
  `cd src-tauri && cargo test --lib gbrain::cli_format > /tmp/cf3.txt 2>&1; grep "test result" /tmp/cf3.txt`(10 passed)

- [ ] **Step 4: 提交**
  ```bash
  git add src-tauri/src/gbrain/cli_format.rs
  git commit -m "feat(gbrain): cli_format get_links via graph + revert status + tests"
  ```

---

## Task 4: call_cli 补 op 映射 + 接入 cli_format

**Files:** Modify `src-tauri/src/mcp.rs`.

- [ ] **Step 1: 在 `call_cli` 的 `match tool` 里,`other =>` 之前补 7 个分支**

```rust
            "get_backlinks" => {
                let slug = required_string(&arguments, "slug")?;
                argv.push("backlinks".to_string());
                argv.push(slug);
            }
            "traverse_graph" => {
                let slug = required_string(&arguments, "slug")?;
                argv.push("graph".to_string());
                argv.push(slug);
                push_number_flag(&mut argv, &arguments, "depth", "--depth");
                push_string_flag(&mut argv, &arguments, "direction", "--direction");
            }
            "get_links" => {
                let slug = required_string(&arguments, "slug")?;
                argv.push("graph".to_string());
                argv.push(slug);
                argv.push("--depth".to_string());
                argv.push("1".to_string());
            }
            "get_versions" => {
                let slug = required_string(&arguments, "slug")?;
                argv.push("history".to_string());
                argv.push(slug);
            }
            "get_stats" => {
                argv.push("stats".to_string());
            }
            "find_orphans" => {
                argv.push("orphans".to_string());
                argv.push("--json".to_string());
            }
            "revert_version" => {
                let slug = required_string(&arguments, "slug")?;
                let vid = arguments
                    .get("version_id")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| McpError::Server("revert_version: version_id (number) required".into()))?;
                argv.push("revert".to_string());
                argv.push(slug);
                argv.push(vid.to_string());
            }
```

- [ ] **Step 2: 末尾把成功 stdout 经 cli_format 归一**
  找到 call_cli 末尾的成功返回(`if stdout.is_empty() && !stderr.is_empty() { Ok(stderr) } else { Ok(stdout) }`),改为:
```rust
        if stdout.is_empty() && !stderr.is_empty() {
            return Ok(stderr);
        }
        crate::gbrain::cli_format::to_mcp_json(tool, &arguments, &stdout)
            .map_err(|e| e) // McpError 直接传
```
  > `tool` 是 `&str`(match 的对象);`arguments` 是 `&serde_json::Value`(call_cli 入参)。确认这两个绑定名(读 call_cli 签名:`async fn call_cli(&self, tool: &str, arguments: serde_json::Value)` → 用 `&arguments`)。

- [ ] **Step 3: 编译 + 全 gbrain 测试**
  - `cd src-tauri && cargo build > /tmp/cf4.txt 2>&1; echo EXIT=$?; grep -E "^error" /tmp/cf4.txt | head`(EXIT=0)
  - `cargo test --lib gbrain:: > /tmp/cf4t.txt 2>&1; grep "test result" /tmp/cf4t.txt`(cli_format 10 + browse 15 等全过)

- [ ] **Step 4: 提交**
  ```bash
  git add src-tauri/src/mcp.rs
  git commit -m "feat(mcp): map 7 gbrain ops in call_cli + route stdout through cli_format::to_mcp_json"
  ```

---

## Task 5: 真实验证(orphans/版本-id)+ smoke + 手动 E2E

**Files:** 视验证结果可能微调 `cli_format.rs`;`tauri_commands.rs`(smoke 已存在,确认)。

- [ ] **Step 1: 实跑确认两处真实形状**
  ```bash
  cd src-tauri && export GBRAIN_HOME=/Users/ryanliu/.uclaw/gbrain
  ./bunembed/bun gbrain-source/src/cli.ts orphans --json 2>/dev/null | tail -8   # 有无 total_orphans/total_pages?
  ./bunembed/bun gbrain-source/src/cli.ts history people/ryanliu 2>/dev/null | head -3  # 记下某 #N
  ./bunembed/bun gbrain-source/src/cli.ts revert people/ryanliu <那个N> 2>&1 | head   # #N 是否被 revert 接受?
  ```
  - 若 `orphans --json` **缺** `total_orphans`/`total_pages` → 在 cli_format 加 `find_orphans` 专转换器:`{total_orphans: orphans.len(), total_pages: <stats 另取或 0>}`(browse::OrphanSummary 只用这俩)。补一个单测。
  - 若 `revert <slug> <#N>` **报错/不接受 #N** → `history` 的 id 来源改正(可能 version-id 是另一个字段);调 `versions_to_json` + 补测。**revert 是写操作,验证后请 revert 回最新版以免改坏数据。**

- [ ] **Step 2: 确认 smoke 命令经 CLI 现在通**(`gbrain_serve_smoke` 已在 9e39bb2;现在它走 CLI 垫片 + cli_format)
  - 手动:`cargo tauri dev` → devtools `await window.__TAURI__.core.invoke('gbrain_serve_smoke')` → 期望 `{listPagesOk:true, listPagesCount:10, getStatsOk:true, error:null}`。

- [ ] **Step 3: 手动 E2E 清单(写进 PR)**
  `cargo tauri dev`(gbrain 已连):
  1. Wiki tab → 列表出真页(10 页,含 CJK)、点页详情 markdown、搜索出结果、反向链接、版本史列表。
  2. 双星云 tab → 知识层有节点(list)+ 连线(get_links via graph)。
  3. 编辑保存(put_page)→ 重渲染;版本史回滚(revert,#N 已验证)。
  4. 空 brain/异常 → 优雅(空态/错误 toast,不崩)。

- [ ] **Step 4: (若 Step 1 有微调)提交**
  ```bash
  git add src-tauri/src/gbrain/cli_format.rs
  git commit -m "fix(gbrain): orphans total_* + version-id mapping per real CLI output"
  ```

---

## 自检(对照 spec)

- **Spec 覆盖**:§2 架构(cli_format + call_cli 接入)→ Task 1/4;§3 各 op 转换器 → Task 1(list/stats/passthrough)+ 2(get/search/history)+ 3(get_links/revert);§4 call_cli op 映射 → Task 4;§5 错误处理(解析失败 → McpError + 原始片段)→ 各转换器返回 McpError;§6 测试(真实 fixture 单测 + smoke + 手动)→ Task 1/2/3 单测 + Task 5;§8 风险(#N=version-id、orphans total_*)→ Task 5 实跑验证。
- **占位符**:无 TBD。Task 5 的"实跑验证"是真实运行步骤(orphans/version-id 的真实形状只能跑出来),非含糊;`_ => Ok(cleaned)` 是有意的"未专门处理 op 原样返回"兜底(query/think/put_page 等)。
- **类型一致**:`to_mcp_json(op,args,stdout)->Result<String,McpError>` 全任务一致;各转换器产出的 JSON 字段对齐 §"browse parser 期望形状"(list→{slug,type,title,updated_at};get→{slug,type,title,compiled_truth,frontmatter,tags};search→{slug,title,chunk_text,score};versions→{id,snapshot_at,compiled_truth};get_links→{from_slug,to_slug,link_type})。call_cli arms 用真实 helper(required_string/push_number_flag/push_string_flag)。
- **范围**:单 PR ~5 commit,无新 migration、无新依赖,不改 browse parser / 前端 / serve。
