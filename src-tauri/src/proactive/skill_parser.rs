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
    /// 触发短语列表：LLM 在 <signals><signal>…</signal></signals> 里生成，
    /// 描述「哪类用户提问或错误消息」应该触发本技能。
    /// 若 LLM 未输出该块，则为空 Vec（向后兼容）。
    pub signals: Vec<String>,
    /// 从执行日志失败信息分类出的高层次错误信号（来自 failure_signals::classify_error）。
    /// 这是 signals[] 的经验对应版：「该技能是从哪类失败中被提炼出来的」。
    /// 若提取时无失败日志，则为空 Vec（向后兼容）。
    pub signals_seen: Vec<String>,
    /// 应用该技能后如何验证它真的有效（一句话，可选）。
    /// LLM 在 <validation_hint>…</validation_hint> 标签里输出；agent 看到后自行决定要不要验证。
    /// 若 LLM 未输出该标签，则为 None（向后兼容）。
    pub validation_hint: Option<String>,
    /// 技能类别：LLM 在 <category>repair|optimize|innovate</category> 里输出。
    /// 只接受这三个值（小写），其余无效值 → None（向后兼容）。
    pub category: Option<String>,
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

        // 提取可选的 <signals><signal>…</signal></signals> 块
        let signals: Vec<String> = if let Some(sigs_block) = extract_tag_content(&skill_content, "signals") {
            extract_repeated_tag_content(&sigs_block, "signal")
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        let validation_hint = extract_tag_content(&skill_content, "validation_hint")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // <category> is optional; only "repair", "optimize", "innovate" are valid.
        let category = extract_tag_content(&skill_content, "category")
            .map(|s| s.trim().to_lowercase())
            .filter(|s| matches!(s.as_str(), "repair" | "optimize" | "innovate"));

        skills.push(ParsedSkill {
            name,
            context,
            principles,
            steps,
            pitfalls,
            signals,
            signals_seen: Vec::new(), // populated by service layer from execution logs
            validation_hint,
            category,
        });
    }

    skills
}

/// 辅助函数：提取同名 XML 标签的所有内容（返回列表）
///
/// 用于提取 <signals> 块内的多个 <signal> 子标签等场景。
fn extract_repeated_tag_content(text: &str, tag: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut search_start = 0;
    loop {
        let remaining = &text[search_start..];
        match extract_tag_content(remaining, tag) {
            Some(content) => {
                results.push(content);
                // 推进到下一个同名标签结束位置
                let close = format!("</{}>", tag);
                if let Some(pos) = remaining.find(&close) {
                    search_start += pos + close.len();
                } else {
                    break;
                }
            }
            None => break,
        }
    }
    results
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

/// Build the Markdown body that is stored as a `MemoryVersion.content` for a
/// learned skill. Extracted so the embedding pipeline can reproduce the same
/// text without duplicating the format string.
pub fn build_version_content(skill: &ParsedSkill) -> String {
    format!(
        "# {}\n\n## 适用场景\n{}\n\n## 核心原则\n{}\n\n## 实现步骤\n{}\n\n## 常见陷阱\n{}",
        skill.name, skill.context, skill.principles, skill.steps, skill.pitfalls
    )
}

/// 将 ParsedSkill 存储为 MemoryNode(kind=Procedure) + MemoryVersion
///
/// **写入时去重 (D1)**：如果当前 space 里已经有一条 normalized title
/// 完全相同的 learned skill（lowercase + trim），不创建新节点，而是：
/// 1. deprecate 旧 active version
/// 2. 创建一条新的 active version 记录新内容
/// 3. 合并 keywords（旧 keyword 自动保留，新 keyword 增量插入）
/// 4. usage_count + 1（视作"该技能再次被强化"）
///
/// 这样旧的 node_id 保持稳定，召回引用 / boot mount / usage_count
/// 都不丢，但内容会持续被新提取强化。如果想要"模糊匹配同概念但不同
/// 措辞"，那是 D2 的活，本函数只做精确匹配。
///
/// 注意：MemoryGraphStore 的方法是同步的。
pub fn store_skill_as_procedure(
    store: &MemoryGraphStore,
    skill: &ParsedSkill,
    space_id: &str,
) -> anyhow::Result<MemoryNode> {
    let now = chrono::Utc::now().to_rfc3339();

    // ── D1: exact-after-normalize dedup ───────────────────────────
    let normalized = normalize_title_for_dedup(&skill.name);
    if !normalized.is_empty() {
        if let Ok(Some(existing)) = store.find_learned_skill_by_normalized_title(space_id, &normalized) {
            tracing::info!(
                node_id = %existing.id,
                title = %existing.title,
                "skill_parser: exact dedup hit — folding new extraction into existing node"
            );
            return upgrade_existing_skill(store, existing, skill, &now);
        }
    }

    // ── D2: fuzzy (bigram-Jaccard) dedup ──────────────────────────
    // Catches near-duplicates like "处理 edit 工具..." vs "edit 工具..."
    // (one extra prefix word) or "基于计划的增量式X工作流" vs the same
    // string with a single word inserted. Pure character bigrams on
    // normalized title — language-agnostic, no tokenizer dependency.
    //
    // Threshold 0.75 is conservative: high enough to reject "前端游戏
    // 开发" vs "基于计划的增量式游戏开发" (Jaccard ~0.24 — not the same
    // concept by string overlap), low enough to catch the genuine
    // "+1 word" near-dups the user keeps accumulating.
    //
    // Skip very short titles (< 4 chars) — bigram counts are too small
    // for the metric to mean anything.
    if normalized.chars().count() >= 4 {
        if let Ok(candidates) = store.list_top_learned_skills(space_id, 500) {
            let new_grams = title_bigrams(&normalized);
            let mut best: Option<(f32, crate::memory_graph::models::MemoryNode)> = None;
            for cand in candidates {
                let cand_norm = normalize_title_for_dedup(&cand.node.title);
                if cand_norm == normalized {
                    continue; // would have been caught by D1
                }
                let cand_grams = title_bigrams(&cand_norm);
                let sim = jaccard_similarity(&new_grams, &cand_grams);
                if sim >= FUZZY_DEDUP_THRESHOLD {
                    match &best {
                        Some((b, _)) if *b >= sim => {}
                        _ => best = Some((sim, cand.node)),
                    }
                }
            }
            if let Some((sim, existing)) = best {
                tracing::info!(
                    node_id = %existing.id,
                    title = %existing.title,
                    new_title = %skill.name,
                    similarity = sim,
                    "skill_parser: fuzzy dedup hit — folding new extraction into similar existing node"
                );
                return upgrade_existing_skill(store, existing, skill, &now);
            }
        }
    }

    let node_id = uuid::Uuid::new_v4().to_string();

    let mut metadata = serde_json::json!({
        "skill_type": "learned",
        "context": skill.context,
        "principles": skill.principles,
        "steps": skill.steps,
        "pitfalls": skill.pitfalls,
        "source": "proactive_skill_extraction",
        "enabled": true,
        "usage_count": 0
    });

    if !skill.signals.is_empty() {
        metadata["signals"] = serde_json::Value::Array(
            skill.signals.iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        );
    }

    if !skill.signals_seen.is_empty() {
        metadata["signals_seen"] = serde_json::Value::Array(
            skill.signals_seen.iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        );
    }

    if let Some(hint) = skill.validation_hint.as_ref() {
        metadata["validation_hint"] = serde_json::Value::String(hint.clone());
    }

    if let Some(cat) = skill.category.as_ref() {
        metadata["category"] = serde_json::Value::String(cat.clone());
    }

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
    let version_content = build_version_content(skill);

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

/// Threshold for fuzzy (bigram-Jaccard) dedup. ≥ this similarity →
/// fold into existing skill instead of creating a new node. Tuned
/// conservatively: catches "+1 word" near-dups but rejects
/// concept-level overlap (which is D3's territory, not D2's).
pub const FUZZY_DEDUP_THRESHOLD: f32 = 0.75;

/// Character bigrams of a string. Language-agnostic — works for CJK
/// without a tokenizer, and for ASCII without a stemmer.
///
/// Empty / 1-char strings produce empty sets; 2-char strings produce
/// a single bigram. Both are correctly handled by `jaccard_similarity`.
pub fn title_bigrams(s: &str) -> std::collections::HashSet<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut set = std::collections::HashSet::new();
    if chars.len() < 2 {
        return set;
    }
    for w in chars.windows(2) {
        set.insert(w.iter().collect::<String>());
    }
    set
}

/// Jaccard similarity (|A ∩ B| / |A ∪ B|) between two bigram sets.
/// Returns 0.0 for empty sets to avoid 0/0 NaN.
pub fn jaccard_similarity(
    a: &std::collections::HashSet<String>,
    b: &std::collections::HashSet<String>,
) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        return 0.0;
    }
    inter as f32 / union as f32
}

/// Normalize a skill title for dedup comparison.
///
/// Strategy: trim + lowercase + collapse whitespace + drop trailing
/// punctuation. Conservative — we only want to catch obvious duplicates
/// like "前端游戏开发项目工作流" appearing twice with different
/// casing or trailing colon. Fuzzy concept-level dedup is D2's job.
pub fn normalize_title_for_dedup(title: &str) -> String {
    let mut s = title.trim().to_lowercase();
    // Collapse runs of whitespace into single space.
    let collapsed: String = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    s = collapsed;
    // Drop trailing punctuation (Chinese + ASCII).
    s.trim_end_matches(|c: char| {
        matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | '。' | '，' | '；' | '：' | '！' | '？'
        )
    })
    .to_string()
}

/// Fold a newly-extracted skill into an existing node (D1 dedup path).
/// - Deprecates the old active version
/// - Inserts a new active version with the freshly-extracted content
/// - Bumps usage_count to reflect the "concept reinforced" signal
/// - Inserts any new keywords (existing ones are preserved)
fn upgrade_existing_skill(
    store: &MemoryGraphStore,
    existing: MemoryNode,
    skill: &ParsedSkill,
    now: &str,
) -> anyhow::Result<MemoryNode> {
    // Deprecate old active version (if any).
    if let Ok(Some(active)) = store.get_active_version(&existing.id) {
        if let Err(e) = store.deprecate_version(&active.id) {
            tracing::warn!(
                node_id = %existing.id,
                err = %e,
                "skill_parser: failed to deprecate old version (continuing with new version anyway)"
            );
        }
    }

    // New version content (same template as the create-fresh path).
    let version_content = build_version_content(skill);
    let new_version = MemoryVersion {
        id: uuid::Uuid::new_v4().to_string(),
        node_id: existing.id.clone(),
        supersedes_version_id: None,
        status: MemoryVersionStatus::Active,
        content: version_content,
        metadata: None,
        embedding_json: None,
        created_at: now.to_string(),
    };
    store.create_version(&new_version)?;

    // Update signals if the re-extraction produced any. Empty re-extraction
    // keeps the old signals (a re-extraction without signals shouldn't wipe
    // existing trigger phrases — that's worse than keeping stale ones).
    // Same rule applies to signals_seen — batch both updates into one
    // metadata write to avoid two separate round-trips.
    // validation_hint: only update if re-extraction provided one (None keeps old).
    let need_signals_update = !skill.signals.is_empty();
    let need_signals_seen_update = !skill.signals_seen.is_empty();
    let need_hint_update = skill.validation_hint.is_some();
    let need_category_update = skill.category.is_some();
    if need_signals_update || need_signals_seen_update || need_hint_update || need_category_update {
        let mut metadata = existing.metadata.clone().unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = metadata.as_object_mut() {
            if need_signals_update {
                obj.insert(
                    "signals".to_string(),
                    serde_json::Value::Array(
                        skill.signals.iter().map(|s| serde_json::Value::String(s.clone())).collect(),
                    ),
                );
            }
            if need_signals_seen_update {
                obj.insert(
                    "signals_seen".to_string(),
                    serde_json::Value::Array(
                        skill.signals_seen.iter().map(|s| serde_json::Value::String(s.clone())).collect(),
                    ),
                );
            }
            if let Some(hint) = skill.validation_hint.as_ref() {
                obj.insert(
                    "validation_hint".to_string(),
                    serde_json::Value::String(hint.clone()),
                );
            }
            if let Some(cat) = skill.category.as_ref() {
                obj.insert(
                    "category".to_string(),
                    serde_json::Value::String(cat.clone()),
                );
            }
        }
        if let Err(e) = store.update_node(&existing.id, None, None, Some(&metadata)) {
            tracing::warn!(
                node_id = %existing.id,
                err = %e,
                "skill_parser: signals/signals_seen/validation_hint update failed (continuing)"
            );
        }
    }

    // Bump usage_count — the LLM re-derived this skill from a fresh
    // session, which is itself a vote of confidence. Best-effort.
    if let Err(e) = store.bump_skill_usage(&[existing.id.as_str()]) {
        tracing::warn!(node_id = %existing.id, err = %e, "skill_parser: usage bump failed");
    }

    // Merge keywords — existing rows stay; new ones get inserted (the
    // create_keyword path doesn't dedup but a stray duplicate keyword
    // row is harmless for LIKE search).
    let existing_kw: std::collections::HashSet<String> = store
        .get_keywords_for_node(&existing.id)
        .unwrap_or_default()
        .into_iter()
        .collect();
    let fresh_kw = extract_keywords(&skill.name, &skill.context);
    for kw in fresh_kw {
        if existing_kw.contains(&kw) {
            continue;
        }
        let kw_row = MemoryKeyword {
            id: uuid::Uuid::new_v4().to_string(),
            space_id: existing.space_id.clone(),
            node_id: existing.id.clone(),
            keyword: kw,
            created_at: now.to_string(),
        };
        if let Err(e) = store.create_keyword(&kw_row) {
            tracing::warn!(
                node_id = %existing.id,
                err = %e,
                "skill_parser: keyword merge insert failed"
            );
        }
    }

    Ok(existing)
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
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
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
    fn normalize_title_collapses_case_whitespace_punctuation() {
        // Real example from user's DB: "前端游戏开发项目工作流" appeared
        // twice. Plus we want trailing ":" / case differences to fold.
        assert_eq!(
            normalize_title_for_dedup("前端游戏开发项目工作流"),
            normalize_title_for_dedup("  前端游戏开发项目工作流  "),
        );
        assert_eq!(
            normalize_title_for_dedup("Edit Tool Tips"),
            normalize_title_for_dedup("edit tool tips"),
        );
        assert_eq!(
            normalize_title_for_dedup("使用 edit 工具"),
            normalize_title_for_dedup("使用 edit 工具："),
        );
        // Multiple spaces collapse
        assert_eq!(
            normalize_title_for_dedup("a  b   c"),
            "a b c",
        );
        // Different titles remain different
        assert_ne!(
            normalize_title_for_dedup("edit 工具技巧"),
            normalize_title_for_dedup("edit 工具陷阱"),
        );
    }

    #[test]
    fn jaccard_similarity_basics() {
        let a = title_bigrams("处理 edit 工具");
        let b = title_bigrams("edit 工具");
        let sim = jaccard_similarity(&a, &b);
        // Mostly the same bigrams — should be high.
        assert!(sim > 0.6, "expected high similarity for prefix-only diff, got {}", sim);

        let c = title_bigrams("基于计划的增量式游戏前端开发工作流");
        let d = title_bigrams("基于计划的增量式游戏开发工作流");
        let sim2 = jaccard_similarity(&c, &d);
        // Single-word insertion ("前端") inside an otherwise identical
        // phrase — should fire D2.
        assert!(sim2 >= FUZZY_DEDUP_THRESHOLD, "expected fuzzy hit for inserted word, got {}", sim2);

        let e = title_bigrams("前端游戏开发项目工作流");
        let f = title_bigrams("基于计划的增量式游戏前端开发工作流");
        let sim3 = jaccard_similarity(&e, &f);
        // Different concepts that share some words — should NOT fire D2.
        // (D3 LLM-judgment is the right tool for this.)
        assert!(sim3 < FUZZY_DEDUP_THRESHOLD, "expected fuzzy miss for concept overlap, got {}", sim3);
    }

    #[test]
    fn fuzzy_dedup_folds_near_duplicate() {
        use crate::memory_graph::store::MemoryGraphStore;
        use rusqlite::Connection;
        use std::sync::{Arc, Mutex};

        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryGraphStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_tables();
        let space = "fuzz-space";

        let s1 = ParsedSkill {
            name: "edit 工具文本匹配错误的备选插入策略".into(),
            context: "v1".into(),
            principles: "v1".into(),
            steps: "v1".into(),
            pitfalls: "v1".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };
        let s2 = ParsedSkill {
            name: "处理 edit 工具文本匹配错误的备选插入策略".into(), // +1 word prefix
            context: "v2".into(),
            principles: "v2".into(),
            steps: "v2".into(),
            pitfalls: "v2".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };

        let n1 = store_skill_as_procedure(&store, &s1, space).unwrap();
        let n2 = store_skill_as_procedure(&store, &s2, space).unwrap();

        // Fuzzy dedup should have folded n2 into n1.
        assert_eq!(n1.id, n2.id, "fuzzy dedup should reuse node id");

        // Active version reflects v2 content.
        let active = store.get_active_version(&n1.id).unwrap().expect("active version");
        assert!(active.content.contains("v2"), "active version should be v2");
    }

    #[test]
    fn dedup_folds_into_existing_node() {
        use crate::memory_graph::store::MemoryGraphStore;
        use rusqlite::Connection;
        use std::sync::{Arc, Mutex};

        // Set up an in-memory store.
        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryGraphStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_tables();

        let space = "test-space";
        let s1 = ParsedSkill {
            name: "前端游戏开发项目工作流".into(),
            context: "v1 context".into(),
            principles: "v1 principles".into(),
            steps: "v1 steps".into(),
            pitfalls: "v1 pitfalls".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };
        let s2 = ParsedSkill {
            name: "  前端游戏开发项目工作流  ".into(), // same after normalize
            context: "v2 context".into(),
            principles: "v2 principles".into(),
            steps: "v2 steps".into(),
            pitfalls: "v2 pitfalls".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };

        let n1 = store_skill_as_procedure(&store, &s1, space).unwrap();
        let n2 = store_skill_as_procedure(&store, &s2, space).unwrap();

        // Same node_id — second call folded in.
        assert_eq!(n1.id, n2.id, "expected dedup to reuse node id");

        // Active version reflects v2 content.
        let active = store.get_active_version(&n1.id).unwrap().expect("active version");
        assert!(active.content.contains("v2 context"), "active version should have v2 content");
        assert!(!active.content.contains("v1 context"), "v1 should be deprecated");

        // History has both versions; v1 is deprecated, v2 is active.
        let all_versions = store.get_versions(&n1.id).unwrap();
        assert_eq!(all_versions.len(), 2, "should keep both versions in history");

        // usage_count bumped by the dedup path.
        let node = store.get_node(&n1.id).unwrap().unwrap();
        let count = node
            .metadata
            .as_ref()
            .and_then(|m| m.get("usage_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(count >= 1, "usage_count should be bumped on dedup hit");
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

    // ─── Task 1: signals[] extraction tests ──────────────────────────────

    #[test]
    fn parses_signals_array_from_skill_xml() {
        let xml = r#"<skill_report><new_skills><skill>
<name>api-key-rotation</name>
<context>API key auth failures</context>
<principles>Rotate keys when 401 persists</principles>
<steps>1. detect 401
2. swap key</steps>
<pitfalls>Don't retry indefinitely</pitfalls>
<signals>
<signal>401 unauthorized</signal>
<signal>token expired</signal>
<signal>authentication failed</signal>
</signals>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_report(xml);
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0].signals,
            vec!["401 unauthorized", "token expired", "authentication failed"]
        );
    }

    #[test]
    fn parses_skill_without_signals_block() {
        let xml = r#"<skill_report><new_skills><skill>
<name>basic-skill</name>
<context>x</context>
<principles>y</principles>
<steps>z</steps>
<pitfalls>w</pitfalls>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_report(xml);
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].signals.is_empty());
    }

    #[test]
    fn signals_persist_to_metadata_on_extraction() {
        use crate::memory_graph::store::MemoryGraphStore;
        use rusqlite::Connection;
        use std::sync::{Arc, Mutex};

        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryGraphStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_tables();

        let skill = ParsedSkill {
            name: "api-key-rotation".into(),
            context: "auth failures".into(),
            principles: "rotate on 401".into(),
            steps: "1. detect\n2. swap".into(),
            pitfalls: "don't loop".into(),
            signals: vec![
                "401 unauthorized".into(),
                "token expired".into(),
                "authentication failed".into(),
            ],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };

        let node = store_skill_as_procedure(&store, &skill, "default").unwrap();
        let stored = store.get_node(&node.id).unwrap().unwrap();
        let signals_val = stored.metadata.as_ref()
            .and_then(|m| m.get("signals"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        assert_eq!(signals_val, vec!["401 unauthorized", "token expired", "authentication failed"]);
    }

    #[test]
    fn empty_signals_not_written_to_metadata() {
        use crate::memory_graph::store::MemoryGraphStore;
        use rusqlite::Connection;
        use std::sync::{Arc, Mutex};

        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryGraphStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_tables();

        let skill = ParsedSkill {
            name: "no-signals-skill".into(),
            context: "ctx".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };

        let node = store_skill_as_procedure(&store, &skill, "default").unwrap();
        let stored = store.get_node(&node.id).unwrap().unwrap();
        let has_signals_key = stored.metadata.as_ref()
            .map(|m| m.get("signals").is_some())
            .unwrap_or(false);
        assert!(!has_signals_key, "signals key should be absent when signals is empty");
    }

    #[test]
    fn signals_persist_on_skill_upgrade() {
        use crate::memory_graph::store::MemoryGraphStore;
        use rusqlite::Connection;
        use std::sync::{Arc, Mutex};

        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryGraphStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_tables();

        let space = "default";

        // First extraction: skill with old signal.
        let s1 = ParsedSkill {
            name: "api-key-rotation".into(),
            context: "auth failures".into(),
            principles: "rotate on 401".into(),
            steps: "1. detect\n2. swap".into(),
            pitfalls: "don't loop".into(),
            signals: vec!["old-signal".into()],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };
        let n1 = store_skill_as_procedure(&store, &s1, space).unwrap();

        // Confirm initial signals stored.
        let stored = store.get_node(&n1.id).unwrap().unwrap();
        let init_signals = stored.metadata.as_ref()
            .and_then(|m| m.get("signals"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>())
            .unwrap_or_default();
        assert_eq!(init_signals, vec!["old-signal"]);

        // Second extraction: same skill (exact dedup), richer signals.
        let s2 = ParsedSkill {
            name: "api-key-rotation".into(),
            context: "auth failures v2".into(),
            principles: "rotate on 401".into(),
            steps: "1. detect\n2. swap".into(),
            pitfalls: "don't loop".into(),
            signals: vec!["new1".into(), "new2".into()],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };
        let n2 = store_skill_as_procedure(&store, &s2, space).unwrap();

        // Must fold into the same node.
        assert_eq!(n1.id, n2.id, "expected dedup to reuse node id");

        // Signals should be replaced with the new ones.
        let updated = store.get_node(&n1.id).unwrap().unwrap();
        let signals = updated.metadata.as_ref()
            .and_then(|m| m.get("signals"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>())
            .unwrap_or_default();
        assert_eq!(signals.len(), 2);
        assert_eq!(signals[0], "new1");
        assert_eq!(signals[1], "new2");
    }

    #[test]
    fn empty_signals_on_upgrade_keeps_old() {
        use crate::memory_graph::store::MemoryGraphStore;
        use rusqlite::Connection;
        use std::sync::{Arc, Mutex};

        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryGraphStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_tables();

        let space = "default";

        // First extraction: skill with existing signals.
        let s1 = ParsedSkill {
            name: "keep-signals-skill".into(),
            context: "ctx".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec!["keep-me".into()],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };
        let n1 = store_skill_as_procedure(&store, &s1, space).unwrap();

        // Second extraction: same skill, but re-extraction produced no signals.
        let s2 = ParsedSkill {
            name: "keep-signals-skill".into(),
            context: "ctx v2".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec![],  // empty — should NOT wipe existing signals
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };
        let n2 = store_skill_as_procedure(&store, &s2, space).unwrap();

        assert_eq!(n1.id, n2.id, "expected dedup to reuse node id");

        // Old signals should be preserved.
        let updated = store.get_node(&n1.id).unwrap().unwrap();
        let signals = updated.metadata.as_ref()
            .and_then(|m| m.get("signals"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>())
            .unwrap_or_default();
        assert_eq!(signals, vec!["keep-me"], "old signals must not be wiped by empty re-extraction");
    }

    // ─── Task 3: validation_hint tests ───────────────────────────────────

    #[test]
    fn parses_validation_hint_when_present() {
        let xml = r#"<skill_report><new_skills><skill>
<name>x</name><context>c</context><principles>p</principles>
<steps>s</steps><pitfalls>w</pitfalls>
<validation_hint>Run the command again and confirm exit 0.</validation_hint>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_report(xml);
        assert_eq!(parsed[0].validation_hint.as_deref(),
            Some("Run the command again and confirm exit 0."));
    }

    #[test]
    fn validation_hint_absent_yields_none() {
        let xml = r#"<skill_report><new_skills><skill>
<name>y</name><context>c</context><principles>p</principles>
<steps>s</steps><pitfalls>w</pitfalls>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_report(xml);
        assert!(parsed[0].validation_hint.is_none());
    }

    #[test]
    fn validation_hint_persists_on_upgrade() {
        use crate::memory_graph::store::MemoryGraphStore;
        use rusqlite::Connection;
        use std::sync::{Arc, Mutex};

        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryGraphStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_tables();
        let space = "default";

        // First extraction: skill with a hint.
        let s1 = ParsedSkill {
            name: "verify-skill".into(),
            context: "ctx".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: Some("Re-run and check exit 0.".into()),
            category: None,
        };
        let n1 = store_skill_as_procedure(&store, &s1, space).unwrap();

        // Confirm initial hint stored.
        let stored = store.get_node(&n1.id).unwrap().unwrap();
        let init_hint = stored.metadata.as_ref()
            .and_then(|m| m.get("validation_hint"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        assert_eq!(init_hint.as_deref(), Some("Re-run and check exit 0."));

        // Second extraction (same skill, dedup hit): new hint replaces old one.
        let s2 = ParsedSkill {
            name: "verify-skill".into(),
            context: "ctx v2".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: Some("Check the log output for 'success'.".into()),
            category: None,
        };
        let n2 = store_skill_as_procedure(&store, &s2, space).unwrap();
        assert_eq!(n1.id, n2.id, "expected dedup to reuse node id");

        let updated = store.get_node(&n1.id).unwrap().unwrap();
        let hint = updated.metadata.as_ref()
            .and_then(|m| m.get("validation_hint"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        assert_eq!(hint.as_deref(), Some("Check the log output for 'success'."));

        // Third extraction: no hint — should NOT wipe existing.
        let s3 = ParsedSkill {
            name: "verify-skill".into(),
            context: "ctx v3".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };
        let n3 = store_skill_as_procedure(&store, &s3, space).unwrap();
        assert_eq!(n1.id, n3.id);

        let final_node = store.get_node(&n1.id).unwrap().unwrap();
        let final_hint = final_node.metadata.as_ref()
            .and_then(|m| m.get("validation_hint"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        assert_eq!(final_hint.as_deref(), Some("Check the log output for 'success'."),
            "None re-extraction must not wipe existing validation_hint");
    }

    // ─── Task 6: category tag tests ──────────────────────────────────────

    #[test]
    fn parses_category_tag() {
        let xml = r#"<skill_report><new_skills><skill>
<name>bug-fixer</name>
<context>debugging sessions</context>
<principles>isolate then fix</principles>
<steps>1. repro 2. fix</steps>
<pitfalls>don't guess</pitfalls>
<category>repair</category>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_report(xml);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].category.as_deref(), Some("repair"));
    }

    #[test]
    fn parses_invalid_category_as_none() {
        let xml = r#"<skill_report><new_skills><skill>
<name>some-skill</name>
<context>ctx</context>
<principles>p</principles>
<steps>s</steps>
<pitfalls>pt</pitfalls>
<category>unknown-value</category>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_report(xml);
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].category.is_none(), "invalid category must be None");
    }

    #[test]
    fn category_persists_on_upgrade() {
        use crate::memory_graph::store::MemoryGraphStore;
        use rusqlite::Connection;
        use std::sync::{Arc, Mutex};

        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryGraphStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_tables();
        let space = "default";

        // First extraction: skill with category "repair".
        let s1 = ParsedSkill {
            name: "cat-skill".into(),
            context: "ctx".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: Some("repair".into()),
        };
        let n1 = store_skill_as_procedure(&store, &s1, space).unwrap();

        // Confirm initial category stored.
        let stored = store.get_node(&n1.id).unwrap().unwrap();
        let init_cat = stored.metadata.as_ref()
            .and_then(|m| m.get("category"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        assert_eq!(init_cat.as_deref(), Some("repair"));

        // Second extraction (dedup hit): new category replaces old one.
        let s2 = ParsedSkill {
            name: "cat-skill".into(),
            context: "ctx v2".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: Some("optimize".into()),
        };
        let n2 = store_skill_as_procedure(&store, &s2, space).unwrap();
        assert_eq!(n1.id, n2.id, "expected dedup to reuse node id");

        let updated = store.get_node(&n1.id).unwrap().unwrap();
        let cat = updated.metadata.as_ref()
            .and_then(|m| m.get("category"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        assert_eq!(cat.as_deref(), Some("optimize"), "category should be updated on upgrade");

        // Third extraction: no category — should NOT wipe existing.
        let s3 = ParsedSkill {
            name: "cat-skill".into(),
            context: "ctx v3".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
        };
        let n3 = store_skill_as_procedure(&store, &s3, space).unwrap();
        assert_eq!(n1.id, n3.id);

        let final_node = store.get_node(&n1.id).unwrap().unwrap();
        let final_cat = final_node.metadata.as_ref()
            .and_then(|m| m.get("category"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        assert_eq!(final_cat.as_deref(), Some("optimize"),
            "None re-extraction must not wipe existing category");
    }
}
