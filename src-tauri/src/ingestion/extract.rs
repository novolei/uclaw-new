//! 文本块 → LLM 抽实体 → JSON 解析 → 跨块按 slug 累积。

use crate::ingestion::job::IngestError;
use serde::Deserialize;

/// LLM 抽出的一个知识实体。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ExtractedEntity {
    pub slug: String,
    #[serde(rename = "type", default)]
    pub page_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(rename = "compiled_truth_md", alias = "compiled_truth", default)]
    pub compiled_truth: String,
    #[serde(default)]
    pub links: Vec<String>,
}

/// 抽取 system prompt(返回 JSON 数组)。
pub const EXTRACT_SYSTEM: &str = "You extract distinct knowledge entities from a text chunk for a personal knowledge base. \
Return ONLY a JSON array, no prose. Each item: {\"slug\":\"domain/kebab-name\",\"type\":\"person|concept|org|reference|note\",\"title\":\"...\",\"compiled_truth_md\":\"a self-contained markdown summary of this entity, may use ## sections and [[wikilinks]]\",\"links\":[\"other-slug\"]}. \
Use domain-prefixed kebab slugs (e.g. people/jane-doe, concept/vector-search, org/acme). If nothing substantive, return [].";

/// 规范化 slug:小写、空格→`-`、去非法字符、折叠多重 `-`、限长。保留 `/`(域前缀)。
pub fn normalize_slug(raw: &str) -> String {
    let raw = raw.trim().to_lowercase();
    let mut out = String::new();
    let mut prev_dash = false;
    for c in raw.chars() {
        let mapped = match c {
            'a'..='z' | '0'..='9' | '/' => Some(c),
            ' ' | '_' | '-' => Some('-'),
            c if c as u32 > 127 => Some(c), // 保留 CJK 等
            _ => None,
        };
        match mapped {
            Some('-') => {
                if !prev_dash && !out.is_empty() {
                    out.push('-');
                    prev_dash = true;
                }
            }
            Some(ch) => {
                out.push(ch);
                prev_dash = false;
            }
            None => {}
        }
    }
    out.trim_matches('-').chars().take(120).collect()
}

/// 从 LLM 原始输出解析实体数组。鲁棒:剥 ```json fence、取首个 `[` 到末个 `]`。
pub fn parse_entities(raw: &str) -> Result<Vec<ExtractedEntity>, IngestError> {
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let start = cleaned.find('[');
    let end = cleaned.rfind(']');
    let json = match (start, end) {
        (Some(s), Some(e)) if e > s => &cleaned[s..=e],
        _ => return Err(IngestError::Parse("no JSON array in LLM output".into())),
    };
    let mut entities: Vec<ExtractedEntity> =
        serde_json::from_str(json).map_err(|e| IngestError::Parse(e.to_string()))?;
    for ent in entities.iter_mut() {
        ent.slug = normalize_slug(&ent.slug);
    }
    entities.retain(|e| !e.slug.is_empty());
    Ok(entities)
}

/// 跨块累积:同 slug 合并(标题取首个非空,compiled_truth 拼接,links 去重并集)。
pub fn accumulate(acc: &mut Vec<ExtractedEntity>, new: Vec<ExtractedEntity>) {
    for ent in new {
        if let Some(existing) = acc.iter_mut().find(|e| e.slug == ent.slug) {
            if existing.title.is_empty() {
                existing.title = ent.title;
            }
            if !ent.compiled_truth.trim().is_empty() {
                if !existing.compiled_truth.is_empty() {
                    existing.compiled_truth.push_str("\n\n");
                }
                existing.compiled_truth.push_str(&ent.compiled_truth);
            }
            for l in ent.links {
                if !existing.links.contains(&l) {
                    existing.links.push(l);
                }
            }
        } else {
            acc.push(ent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json() {
        let raw = r##"[{"slug":"people/jane-doe","type":"person","title":"Jane","compiled_truth_md":"# Jane","links":["org/acme"]}]"##;
        let ents = parse_entities(raw).unwrap();
        assert_eq!(ents.len(), 1);
        assert_eq!(ents[0].slug, "people/jane-doe");
        assert_eq!(ents[0].page_type, "person");
        assert!(ents[0].compiled_truth.contains("Jane"));
        assert_eq!(ents[0].links, vec!["org/acme"]);
    }

    #[test]
    fn parses_fenced_json_with_prose() {
        let raw = "Here you go:\n```json\n[{\"slug\":\"Concept/Vector Search\",\"title\":\"VS\"}]\n```\nDone.";
        let ents = parse_entities(raw).unwrap();
        assert_eq!(ents.len(), 1);
        assert_eq!(ents[0].slug, "concept/vector-search");
    }

    #[test]
    fn malformed_json_errs_not_panics() {
        assert!(parse_entities("not json at all").is_err());
        assert!(parse_entities("[ {bad } ]").is_err());
    }

    #[test]
    fn normalize_slug_rules() {
        assert_eq!(normalize_slug("People/Jane  Doe!!"), "people/jane-doe");
        assert_eq!(normalize_slug("  __weird__  "), "weird");
        assert_eq!(normalize_slug("人物/刘磊"), "人物/刘磊");
    }

    #[test]
    fn accumulate_merges_by_slug() {
        let mut acc = vec![ExtractedEntity {
            slug: "concept/x".into(), page_type: "concept".into(), title: "X".into(),
            compiled_truth: "first".into(), links: vec!["a".into()],
        }];
        accumulate(&mut acc, vec![ExtractedEntity {
            slug: "concept/x".into(), page_type: "concept".into(), title: "".into(),
            compiled_truth: "second".into(), links: vec!["a".into(), "b".into()],
        }]);
        assert_eq!(acc.len(), 1);
        assert!(acc[0].compiled_truth.contains("first") && acc[0].compiled_truth.contains("second"));
        assert_eq!(acc[0].links, vec!["a", "b"]);
    }
}
