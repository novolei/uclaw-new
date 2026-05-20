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
}
