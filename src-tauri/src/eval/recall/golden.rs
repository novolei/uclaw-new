//! Golden set: one JSON object per line —
//! `{"id","lang","category"?,"query","expect_any"?:[…],"expect_word"?:[…]}`.
//! Blank lines and lines starting with `//` are skipped.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldenItem {
    pub id: String,
    pub lang: String,
    #[serde(default)]
    pub category: String,
    pub query: String,
    #[serde(default)]
    pub expect_any: Vec<String>,
    #[serde(default)]
    pub expect_word: Vec<String>,
}

/// Parse a JSONL golden set; the error names the 1-based line.
pub fn parse_golden(text: &str) -> Result<Vec<GoldenItem>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let item: GoldenItem = serde_json::from_str(line).map_err(|e| format!("golden line {}: {e}", i + 1))?;
        out.push(item);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_golden_reads_fields_and_skips_comments() {
        let text = "// comment\n\n{\"id\":\"a\",\"lang\":\"zh\",\"category\":\"profile\",\"query\":\"我家的猫叫什么\",\"expect_any\":[\"团子\"]}\n{\"id\":\"b\",\"lang\":\"en\",\"query\":\"what should you call me\",\"expect_word\":[\"AB\"]}\n";
        let items = parse_golden(text).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].category, "profile");
        assert_eq!(items[0].expect_any, vec!["团子"]);
        assert_eq!(items[1].category, "", "category optional");
        assert_eq!(items[1].expect_word, vec!["AB"]);
    }

    #[test]
    fn parse_golden_reports_bad_line_number() {
        let text = "{\"id\":\"a\",\"lang\":\"zh\",\"query\":\"q\"}\n{not json}\n";
        let err = parse_golden(text).unwrap_err();
        assert!(err.contains("line 2"), "got {err}");
        let missing = parse_golden("{\"id\":\"a\",\"lang\":\"zh\"}").unwrap_err();
        assert!(missing.contains("line 1"), "required field missing: {missing}");
    }
}
