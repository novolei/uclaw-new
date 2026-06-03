//! Pure markdown frontmatter utils for EntityPage content; relocated from
//! gbrain::browse in Step 2d.
//!
//! These functions have no network or I/O dependencies and operate purely on
//! in-memory data. They are placed here so that callers outside the gbrain
//! module can use them without depending on the gbrain transport layer.

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

/// Split a raw markdown page into (frontmatter, body) — the inverse of
/// `build_raw_markdown`. A leading `---\n … \n---\n` fence is parsed as YAML →
/// `serde_json::Value`; if the fence is absent or the YAML is malformed or not
/// a mapping, returns `(serde_json::Value::Null, the_full_input)`. Never panics.
pub fn split_frontmatter(markdown: &str) -> (serde_json::Value, String) {
    let null = serde_json::Value::Null;
    let Some(rest) = markdown.strip_prefix("---\n") else {
        return (null, markdown.to_string());
    };
    let Some(end) = rest.find("\n---\n") else {
        return (null, markdown.to_string());
    };
    let yaml = &rest[..end];
    let body = rest[end + "\n---\n".len()..].trim_start_matches('\n').to_string();
    match serde_yml::from_str::<serde_json::Value>(yaml) {
        Ok(v) if v.is_object() => (v, body),
        _ => (null, markdown.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn split_frontmatter_parses_yaml_block() {
        let md = "---\ntitle: Hello\npage_type: note\ntags:\n  - a\n  - b\n---\n\nbody text here";
        let (fm, body) = split_frontmatter(md);
        assert_eq!(fm.get("title").and_then(|v| v.as_str()), Some("Hello"));
        assert_eq!(fm.get("page_type").and_then(|v| v.as_str()), Some("note"));
        assert_eq!(fm.get("tags").and_then(|v| v.as_array()).map(|a| a.len()), Some(2));
        assert_eq!(body, "body text here");
    }

    #[test]
    fn split_frontmatter_no_fence_returns_null_and_full() {
        let md = "just a plain body, no frontmatter";
        let (fm, body) = split_frontmatter(md);
        assert!(fm.is_null());
        assert_eq!(body, md);
    }

    #[test]
    fn split_frontmatter_malformed_yaml_returns_null_and_full() {
        let md = "---\n: : not valid yaml : :\n---\nbody";
        let (fm, body) = split_frontmatter(md);
        assert!(fm.is_null());
        assert_eq!(body, md);
    }
}
