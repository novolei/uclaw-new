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
        "get_page" => get_page_to_json(args, &cleaned),
        "search" => search_to_json(&cleaned),
        "get_versions" => versions_to_json(&cleaned),
        "get_links" => links_from_graph(args, &cleaned),
        "revert_version" => Ok("{\"status\":\"reverted\"}".to_string()),
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

/// `get`:`---`YAML`---`\n正文 → {slug,type,title,compiled_truth,frontmatter,tags}。
/// slug 取自入参(CLI get 输出不含 slug)。
fn get_page_to_json(args: &serde_json::Value, cleaned: &str) -> Result<String, McpError> {
    let slug = args.get("slug").and_then(|v| v.as_str()).unwrap_or("");
    let (frontmatter, body) = split_frontmatter(cleaned);
    let fm: serde_json::Value = if frontmatter.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_yml::from_str::<serde_json::Value>(frontmatter)
            .ok()
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
/// 注:N 是否就是 revert 接受的 version-id,Task 4/5 实跑验证。
fn versions_to_json(cleaned: &str) -> Result<String, McpError> {
    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut cur: Option<(i64, String, String)> = None;
    for line in cleaned.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix('#') {
            // `N  ISO  preview` — use split_whitespace to skip multiple spaces,
            // then reconstruct preview as everything after "N  ISO  ".
            let mut sw = rest.split_whitespace();
            if let (Some(n_str), Some(iso_str)) = (sw.next(), sw.next()) {
                if let Ok(id) = n_str.parse::<i64>() {
                    if let Some((pid, piso, pprev)) = cur.take() {
                        out.push(mk_ver(pid, piso, pprev));
                    }
                    // Reconstruct preview: trim past "N" and "ISO" in original rest.
                    let preview = {
                        let after_n = rest.trim_start().trim_start_matches(n_str).trim_start();
                        let after_iso = after_n.trim_start().trim_start_matches(iso_str).trim_start();
                        after_iso.to_string()
                    };
                    cur = Some((id, iso_str.to_string(), preview));
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
        assert_eq!(v[1]["slug"], "people/刘磊");
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
}
