//! skill_parser — 从 LLM 响应中解析 <skill_report> XML 并存储为 Procedure 节点

use crate::memory_graph::models::{MemoryKeyword, MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus};
use crate::memory_graph::store::MemoryGraphStore;

/// 解析后的技能结构
#[derive(Debug, Clone)]
pub struct ParsedSkill {
    pub name: String,
    pub context: String,
    pub principles: String,
    pub steps: String,
    pub pitfalls: String,
}

/// 解析 <skill_report> XML 中的 <skill> 标签
///
/// 使用简单字符串解析，不引入 XML 库依赖。
/// 容错处理：LLM 输出不完美时不 panic，返回空 Vec。
pub fn parse_skill_report(xml_text: &str) -> Vec<ParsedSkill> {
    // 快速排除明显无内容的响应
    if xml_text.trim() == "[NO_MESSAGE]" || !xml_text.contains("<new_skills>") {
        return Vec::new();
    }

    // 找到 <new_skills>...</new_skills> 区域
    let new_skills_content = match extract_tag_content(xml_text, "new_skills") {
        Some(content) => content,
        None => return Vec::new(),
    };

    // 在该区域内找到所有 <skill>...</skill>
    let mut skills = Vec::new();
    let mut search_start = 0;

    loop {
        let remaining = &new_skills_content[search_start..];
        let skill_content = match extract_tag_content(remaining, "skill") {
            Some(content) => content,
            None => break,
        };

        // 计算下一个搜索起点（跳过当前 </skill>）
        if let Some(pos) = remaining.find("</skill>") {
            search_start += pos + "</skill>".len();
        } else {
            break;
        }

        // 从每个 <skill> 中提取字段
        let name = extract_tag_content(&skill_content, "name").unwrap_or_default();
        if name.is_empty() {
            continue; // name 是必须字段
        }

        let context = extract_tag_content(&skill_content, "context").unwrap_or_default();
        let principles = extract_tag_content(&skill_content, "principles").unwrap_or_default();
        let steps = extract_tag_content(&skill_content, "steps").unwrap_or_default();
        let pitfalls = extract_tag_content(&skill_content, "pitfalls").unwrap_or_default();

        skills.push(ParsedSkill {
            name,
            context,
            principles,
            steps,
            pitfalls,
        });
    }

    skills
}

/// 辅助函数：提取 XML 标签内容
///
/// 支持标签前后有空白符，内容可包含换行。
fn extract_tag_content(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);

    // 找到开始标签（支持 <tag> 或 <tag ...>）
    let start_pos = text.find(&open)?;
    let after_open = start_pos + open.len();

    // 找到开始标签的 '>'
    let gt_pos = text[after_open..].find('>')?;
    let content_start = after_open + gt_pos + 1;

    // 找到结束标签
    let end_pos = text[content_start..].find(&close)?;

    let content = &text[content_start..content_start + end_pos];
    Some(content.trim().to_string())
}

/// 将 ParsedSkill 存储为 MemoryNode(kind=Procedure) + MemoryVersion
///
/// 注意：MemoryGraphStore 的方法是同步的。
pub fn store_skill_as_procedure(
    store: &MemoryGraphStore,
    skill: &ParsedSkill,
    space_id: &str,
) -> anyhow::Result<MemoryNode> {
    let node_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let metadata = serde_json::json!({
        "skill_type": "learned",
        "context": skill.context,
        "principles": skill.principles,
        "steps": skill.steps,
        "pitfalls": skill.pitfalls,
        "source": "proactive_skill_extraction",
        "enabled": true,
        "usage_count": 0
    });

    let node = MemoryNode {
        id: node_id.clone(),
        space_id: space_id.to_string(),
        kind: MemoryNodeKind::Procedure,
        title: skill.name.clone(),
        metadata: Some(metadata),
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    store.create_node(&node)?;

    // 创建对应的 MemoryVersion 存储完整内容
    let version_content = format!(
        "# {}\n\n## 适用场景\n{}\n\n## 核心原则\n{}\n\n## 实现步骤\n{}\n\n## 常见陷阱\n{}",
        skill.name, skill.context, skill.principles, skill.steps, skill.pitfalls
    );

    let version = MemoryVersion {
        id: uuid::Uuid::new_v4().to_string(),
        node_id: node_id,
        supersedes_version_id: None,
        status: MemoryVersionStatus::Active,
        content: version_content,
        metadata: None,
        embedding_json: None,
        created_at: now.clone(),
    };

    store.create_version(&version)?;

    // ── Keyword index for L2 triggered recall ────────────────────────
    // Without this, learned skills can ONLY be recalled via FTS5 word
    // overlap (L3) — meaning a skill titled "SwiftData 项目分析" won't
    // surface when the user asks "看下这个 SwiftUI 项目". Writing
    // keywords plugs them into L2 keyword search where partial matches
    // and synonym-ish overlap can light them up.
    //
    // Keywords are pulled from name + context (the two fields most
    // likely to contain "what scenarios this skill applies to" tokens).
    // Best-effort: a failure here is logged but doesn't break the
    // create_version write — we'd rather have the skill stored without
    // keywords than abort skill extraction entirely.
    let keywords = extract_keywords(&skill.name, &skill.context);
    for kw in keywords {
        let kw_row = MemoryKeyword {
            id: uuid::Uuid::new_v4().to_string(),
            space_id: space_id.to_string(),
            node_id: node.id.clone(),
            keyword: kw,
            created_at: now.clone(),
        };
        if let Err(e) = store.create_keyword(&kw_row) {
            tracing::warn!(
                node_id = %node.id,
                err = %e,
                "skill_parser: keyword insert failed (skill stored OK)"
            );
        }
    }

    Ok(node)
}

/// Extract recall keywords from a skill's name + context.
///
/// Strategy: tokenize on whitespace + common CJK/ASCII punctuation,
/// drop tokens shorter than 2 characters (CJK) or 3 characters (ASCII),
/// drop a small Chinese stopword list, dedupe, cap at 8 keywords.
///
/// Conservative on purpose — keywords go into a LIKE '%kw%' search, so
/// a few short/noisy tokens spam the recall layer with false positives.
/// Better to miss recall than to spuriously inject the wrong skill.
pub fn extract_keywords(name: &str, context: &str) -> Vec<String> {
    let combined = format!("{} {}", name, context);
    let raw_tokens: Vec<&str> = combined
        .split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    ',' | '.'
                        | ';'
                        | ':'
                        | '/'
                        | '\\'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '<'
                        | '>'
                        | '"'
                        | '\''
                        | '!'
                        | '?'
                        | '、'
                        | '，'
                        | '。'
                        | '；'
                        | '：'
                        | '（'
                        | '）'
                        | '「'
                        | '」'
                        | '『'
                        | '』'
                        | '！'
                        | '？'
                )
        })
        .collect();

    const STOPWORDS: &[&str] = &[
        "的", "了", "和", "与", "是", "在", "对", "为", "中", "上", "下",
        "中文", "处理", "使用", "进行", "需要", "可以", "如果", "时候",
        "the", "and", "for", "with", "this", "that", "from", "into",
    ];

    // Two-pass extraction so bigrams from one CJK chunk don't squeeze
    // out distinct primary tokens from other chunks.
    //
    // Pass 1: every whitespace token gets one slot — preserves coverage
    //         across name + context.
    // Pass 2: fill remaining slots with CJK bigrams so partial Chinese
    //         queries can still match (without bigrams, "项目结构分析"
    //         wouldn't match a user typing just "项目").
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    let cap = 8usize;
    let push = |s: String,
                seen: &mut std::collections::HashSet<String>,
                out: &mut Vec<String>|
     -> bool {
        if out.len() >= cap {
            return false;
        }
        if STOPWORDS.iter().any(|w| *w == s.as_str()) {
            return false;
        }
        if seen.insert(s.clone()) {
            out.push(s);
        }
        true
    };

    // Pass 1: primary tokens.
    let mut cjk_chunks_for_bigrams: Vec<String> = Vec::new();
    for t in &raw_tokens {
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        let cjk_chars = t.chars().filter(|c| (*c as u32) >= 0x3000).count();
        let total_chars = t.chars().count();
        let cjk_dominant = cjk_chars >= total_chars / 2;

        if cjk_dominant {
            if total_chars < 2 {
                continue;
            }
            push(t.to_string(), &mut seen, &mut out);
            if total_chars >= 4 {
                cjk_chunks_for_bigrams.push(t.to_string());
            }
        } else {
            if total_chars < 3 {
                continue;
            }
            push(t.to_lowercase(), &mut seen, &mut out);
        }
    }

    // Pass 2: bigrams from CJK chunks ≥4 chars, until we hit the cap.
    'outer: for chunk in &cjk_chunks_for_bigrams {
        let chars: Vec<char> = chunk.chars().collect();
        for w in chars.windows(2) {
            let bigram: String = w.iter().collect();
            push(bigram, &mut seen, &mut out);
            if out.len() >= cap {
                break 'outer;
            }
        }
    }

    out
}

// ─── 单元测试 ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_report_single_skill() {
        let xml = r#"
<skill_report>
<success_patterns>有效策略</success_patterns>
<failure_lessons>失败教训</failure_lessons>
<new_skills>
<skill>
<name>Rust 错误处理模式</name>
<context>处理异步操作中的错误链</context>
<principles>使用 ? 操作符，自定义错误类型</principles>
<steps>1. 定义错误枚举
2. 实现 From trait
3. 使用 ? 传播</steps>
<pitfalls>不要 unwrap 生产代码</pitfalls>
</skill>
</new_skills>
<optimization_suggestions>建议</optimization_suggestions>
<tool_patterns>工具模式</tool_patterns>
</skill_report>"#;

        let skills = parse_skill_report(xml);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "Rust 错误处理模式");
        assert_eq!(skills[0].context, "处理异步操作中的错误链");
        assert_eq!(skills[0].principles, "使用 ? 操作符，自定义错误类型");
        assert!(skills[0].steps.contains("定义错误枚举"));
        assert_eq!(skills[0].pitfalls, "不要 unwrap 生产代码");
    }

    #[test]
    fn test_parse_skill_report_multiple_skills() {
        let xml = r#"
<skill_report>
<new_skills>
<skill>
<name>技能一</name>
<context>场景一</context>
<principles>原则一</principles>
<steps>步骤一</steps>
<pitfalls>陷阱一</pitfalls>
</skill>
<skill>
<name>技能二</name>
<context>场景二</context>
<principles>原则二</principles>
<steps>步骤二</steps>
<pitfalls>陷阱二</pitfalls>
</skill>
</new_skills>
</skill_report>"#;

        let skills = parse_skill_report(xml);
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "技能一");
        assert_eq!(skills[1].name, "技能二");
    }

    #[test]
    fn test_parse_skill_report_no_skills() {
        let xml = r#"
<skill_report>
<success_patterns>有效策略</success_patterns>
<new_skills>
</new_skills>
</skill_report>"#;

        let skills = parse_skill_report(xml);
        assert!(skills.is_empty());
    }

    #[test]
    fn test_parse_no_message() {
        let skills = parse_skill_report("[NO_MESSAGE]");
        assert!(skills.is_empty());
    }

    #[test]
    fn test_parse_no_new_skills_tag() {
        let skills = parse_skill_report("一些普通文本，没有 XML 标签");
        assert!(skills.is_empty());
    }

    #[test]
    fn test_parse_skill_missing_name() {
        let xml = r#"
<skill_report>
<new_skills>
<skill>
<context>场景</context>
<principles>原则</principles>
<steps>步骤</steps>
<pitfalls>陷阱</pitfalls>
</skill>
</new_skills>
</skill_report>"#;

        let skills = parse_skill_report(xml);
        assert!(skills.is_empty()); // name 是必须字段
    }

    #[test]
    fn test_extract_tag_content_basic() {
        assert_eq!(
            extract_tag_content("<name>hello</name>", "name"),
            Some("hello".to_string())
        );
    }

    #[test]
    fn test_extract_tag_content_with_whitespace() {
        assert_eq!(
            extract_tag_content("<name>  hello world  </name>", "name"),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn test_extract_tag_content_multiline() {
        let text = "<steps>\n1. First\n2. Second\n</steps>";
        let result = extract_tag_content(text, "steps");
        assert_eq!(result, Some("1. First\n2. Second".to_string()));
    }

    #[test]
    fn test_extract_tag_content_missing() {
        assert_eq!(extract_tag_content("no tags here", "name"), None);
    }

    #[test]
    fn test_store_skill_as_procedure() {
        // 创建内存数据库
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        let conn = std::sync::Arc::new(std::sync::Mutex::new(conn));
        let store = MemoryGraphStore::new(conn);

        let skill = ParsedSkill {
            name: "测试技能".to_string(),
            context: "测试场景".to_string(),
            principles: "测试原则".to_string(),
            steps: "测试步骤".to_string(),
            pitfalls: "测试陷阱".to_string(),
        };

        let node = store_skill_as_procedure(&store, &skill, "default").unwrap();
        assert_eq!(node.title, "测试技能");
        assert_eq!(node.kind, MemoryNodeKind::Procedure);
        assert_eq!(node.space_id, "default");

        // 验证 version 也被创建
        let version = store.get_active_version(&node.id).unwrap();
        assert!(version.is_some());
        let version = version.unwrap();
        assert!(version.content.contains("测试技能"));
        assert!(version.content.contains("测试场景"));
    }

    #[test]
    fn extract_keywords_zh_basic() {
        let kws = extract_keywords("SwiftData 项目结构分析", "适用于 SwiftUI 项目的数据层快速调研");
        // Should pick up SwiftData / SwiftUI / 项目 / 结构 / 分析 / 数据层 / 调研
        assert!(kws.contains(&"swiftdata".to_string()));
        assert!(kws.contains(&"swiftui".to_string()));
        assert!(kws.contains(&"项目".to_string()));
        // Stopwords + short tokens dropped
        assert!(!kws.contains(&"的".to_string()));
        assert!(!kws.contains(&"于".to_string()));
        assert!(kws.len() <= 8);
    }

    #[test]
    fn extract_keywords_dedupe_and_cap() {
        let kws = extract_keywords(
            "Rust Rust Rust",
            "Rust async tokio future stream channel mpsc oneshot RwLock Mutex",
        );
        assert!(kws.iter().all(|k| !k.is_empty()));
        // Dedupe: "rust" appears once max
        assert_eq!(kws.iter().filter(|k| k.as_str() == "rust").count(), 1);
        // Cap at 8
        assert!(kws.len() <= 8);
    }

    #[test]
    fn extract_keywords_drops_punctuation() {
        let kws = extract_keywords("调用 plan_update", "成功执行 done:true 之后,记得 commit。");
        // No tokens with leading/trailing punctuation
        for k in &kws {
            assert!(!k.starts_with(','));
            assert!(!k.starts_with(':'));
            assert!(!k.ends_with('。'));
        }
    }
}
