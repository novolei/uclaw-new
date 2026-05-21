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
    /// 应用本 skill 时**绝对不要做**的事 — LLM 在 `<anti_patterns>...</anti_patterns>`
    /// 标签里输出。mattpocock-borrow PR 引入的字段，让 skill 同时承载"做什么"与"不做什么"。
    /// 若 LLM 未输出该标签或内容为空，则为 None（向后兼容）。
    pub anti_patterns: Option<String>,
    /// 简短一句话描述（≤120 chars），用于 skill_search router 决定召回 — LLM 在
    /// `<description>...</description>` 标签里输出。是 mattpocock 体例最关键的字段。
    /// 若 LLM 未输出，则为 None；持久化时回退到 `<context>` 作为旧 schema 兼容。
    pub description: Option<String>,
    /// 领域标签（V19 per-workspace 作用域过滤的"skill 侧" 输入）。LLM 在
    /// `<tags><tag>...</tag></tags>` 里输出 0-3 个。空 Vec 表示「全局可用」——
    /// 配合 V19 的"未打标 = 全局"规则保护跨域通用 skill 的覆盖面。
    ///
    /// 已在 parse 时规范化：trim + lowercase + dedup（保持首次出现顺序），与
    /// `tauri_commands::normalize_skill_tags` 的工作区侧规范化口径一致，
    /// 这样 manifest filter 做 intersect 时不会因大小写错位。
    pub tags: Vec<String>,
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

        // <anti_patterns> is optional — content trimmed; empty body → None.
        let anti_patterns = extract_tag_content(&skill_content, "anti_patterns")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // <description> is optional but strongly encouraged. ≤120 chars enforced
        // as a soft cap — over-long descriptions silently truncate on persist
        // to keep `description` searchable by a single-sentence router.
        let description = extract_tag_content(&skill_content, "description")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // <tags><tag>...</tag></tags> is optional. Normalize at parse time
        // (trim + lowercase + dedup) to match the workspace-side normalization
        // in `tauri_commands::normalize_skill_tags`. Order preserved so the
        // first occurrence wins (matches workspace-side behavior).
        let tags: Vec<String> = if let Some(tags_block) = extract_tag_content(&skill_content, "tags") {
            let raw = extract_repeated_tag_content(&tags_block, "tag");
            let mut seen = std::collections::HashSet::new();
            let mut out = Vec::with_capacity(raw.len());
            for r in raw {
                let cleaned = r.trim().to_lowercase();
                if cleaned.is_empty() { continue; }
                if seen.insert(cleaned.clone()) {
                    out.push(cleaned);
                }
            }
            out
        } else {
            Vec::new()
        };

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
            anti_patterns,
            description,
            tags,
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
    let mut out = format!(
        "# {}\n\n## 适用场景\n{}\n\n## 核心原则\n{}\n\n## 实现步骤\n{}",
        skill.name, skill.context, skill.principles, skill.steps
    );
    if let Some(ap) = skill.anti_patterns.as_ref() {
        out.push_str("\n\n## 反模式\n");
        out.push_str(ap);
    }
    if !skill.pitfalls.is_empty() {
        out.push_str("\n\n## 常见陷阱\n");
        out.push_str(&skill.pitfalls);
    }
    out
}

/// Truncate a string to at most `max_chars` Unicode chars, preserving char
/// boundaries (mirrors PR `66a7711`'s code-rescue fix for CJK content). Used
/// to cap `description` at the mattpocock-borrow soft limit (120 chars).
pub(super) fn truncate_to_char_count(s: &str, max_chars: usize) -> String {
    let total = s.chars().count();
    if total <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
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
    // 双层检测：
    //   层 A — 标题字符 bigram Jaccard（语言无关，无需分词器）。
    //   层 B — description 语义相似度（标题接近但未达阈值时的兜底）。
    //
    // CJK 自适应阈值：
    //   - CJK 占比 ≥50% → threshold=0.65（字符 bigram 对单字插入敏感，需放宽）
    //   - 否则 → threshold=0.75（原保守值，ASCII 语言 bigram 更精准）
    //
    // 层 B 触发条件：标题 similarity ∈ [0.50, threshold)，且双方均有 description。
    //   使用 word-bigram Jaccard（≥0.70 视为重复），弥补字符 bigram 对
    //   单词语义不敏感的问题（如 "游戏开发工作流" vs "开发工作流"）。
    if normalized.chars().count() >= 4 {
        if let Ok(candidates) = store.list_top_learned_skills(space_id, 500) {
            let new_grams = title_bigrams(&normalized);
            let cjk_ratio = cjk_char_ratio(&normalized);
            let threshold: f32 = if cjk_ratio >= 0.5 { 0.65 } else { FUZZY_DEDUP_THRESHOLD };
            let desc_threshold: f32 = 0.70; // description word-bigram Jaccard

            let mut best: Option<(f32, crate::memory_graph::models::MemoryNode)> = None;
            for cand in candidates {
                let cand_norm = normalize_title_for_dedup(&cand.node.title);
                if cand_norm == normalized {
                    continue; // would have been caught by D1
                }
                let cand_grams = title_bigrams(&cand_norm);
                let sim = jaccard_similarity(&new_grams, &cand_grams);
                if sim >= threshold {
                    match &best {
                        Some((b, _)) if *b >= sim => {}
                        _ => best = Some((sim, cand.node)),
                    }
                } else if sim >= 0.50 && sim < threshold {
                    // 层 B：标题接近但未达阈值 → 检查 description 语义相似度
                    if let (Some(ref new_desc), Some(ref cand_desc)) = (
                        skill.description.as_deref(),
                        cand.node.metadata.as_ref()
                            .and_then(|m| m.get("description"))
                            .and_then(|v| v.as_str()),
                    ) {
                        let new_desc_grams = word_bigrams(new_desc);
                        let cand_desc_grams = word_bigrams(cand_desc);
                        let desc_sim = jaccard_similarity(&new_desc_grams, &cand_desc_grams);
                        if desc_sim >= desc_threshold {
                            tracing::info!(
                                node_id = %cand.node.id,
                                title = %cand.node.title,
                                new_title = %skill.name,
                                title_sim = sim,
                                desc_sim = desc_sim,
                                cjk_ratio = cjk_ratio,
                                "skill_parser: description dedup hit (title marginal) — folding"
                            );
                            best = Some((sim, cand.node));
                            break; // description hit is definitive
                        }
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
        "usage_count": 0,
        // PR-mattpocock-3 lifecycle gate: newly-extracted skills start as
        // 'draft'. They flip to 'promoted' once cited_count crosses the
        // PROMOTION_THRESHOLD (see record_skill_cited in tauri_commands.rs).
        // Drafts stay searchable via skill_search but DON'T enter the
        // manifest top-30 — prevents unvalidated noise drowning out builtins.
        "lifecycle": "draft"
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

    if let Some(ap) = skill.anti_patterns.as_ref() {
        metadata["anti_patterns"] = serde_json::Value::String(ap.clone());
    }

    // V19 per-workspace scoping (PR #126): persist domain tags into
    // `metadata.tags`. Empty Vec is conformant — the manifest filter's
    // "untagged = global" rule keeps cross-domain skills visible without
    // a tags column. Already normalized at parse time.
    if !skill.tags.is_empty() {
        metadata["tags"] = serde_json::Value::Array(
            skill.tags.iter()
                .map(|t| serde_json::Value::String(t.clone()))
                .collect(),
        );
    }

    // description: take what the LLM provided; if absent, derive a short
    // summary from `context` so skill_search's tri-tier match_reasons still
    // has a non-empty single-line string to show in the manifest.
    // Soft cap at 120 chars (PR-mattpocock spec).
    let desc_raw = skill.description.clone()
        .or_else(|| if skill.context.trim().is_empty() { None } else { Some(skill.context.clone()) });
    if let Some(d) = desc_raw {
        let truncated = truncate_to_char_count(&d, 120);
        metadata["description"] = serde_json::Value::String(truncated);
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

// ───────────────────────────────────────────────────────────────────
// Bundle 22 — persist learned skills to disk as SKILL.md
// ───────────────────────────────────────────────────────────────────

/// Bundle 22 — write a learned skill out as a SKILL.md so the
/// `SkillsRegistry` (disk-tier loader) can scan it on the next
/// session start and the agent can `skill_search` it like any other
/// installed skill.
///
/// Until Bundle 22 the extraction pipeline stored learned skills as
/// `Procedure` nodes in `memory_graph` only — the registry never saw
/// them, so the LLM never reached for them as first-class skills.
/// This function closes that loop.
///
/// Path: `<data_dir>/skills/_auto_extracted/<slug>/SKILL.md`. The
/// `_auto_extracted` subtree shows up as a third sub-tier under the
/// existing User scan dir (`<data_dir>/skills/`) — same provenance,
/// no schema change in `SkillsRegistry`.
///
/// Returns the written path on success. Idempotent: if the same slug
/// already has a SKILL.md, the function overwrites it (we trust the
/// extraction LLM to produce the same-or-better content; D1/D2 dedup
/// at the Procedure-node layer already caught true duplicates before
/// we reached this point).
///
/// Safe on partial failure: if mkdir or write fails the function
/// returns the error and the caller logs + continues. The Procedure
/// node already landed in memory_graph, so the skill is still
/// discoverable via the original recall path — disk persistence is a
/// bonus, not the source of truth.
pub fn persist_learned_skill_to_disk(
    data_dir: &std::path::Path,
    skill: &ParsedSkill,
) -> std::io::Result<std::path::PathBuf> {
    // Refuse empty name upfront. Slugify's CJK-fallback would still
    // produce a hash-suffix name from an empty input, but a learned
    // skill with no human-readable name is almost certainly an
    // extraction bug — fail loudly so the caller logs it.
    if skill.name.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "ParsedSkill.name is empty — refusing to persist".to_string(),
        ));
    }

    // Slug — same normalization as the dedup layer, but kept ASCII +
    // filesystem-safe (lowercase, hyphens, digits only). The dedup
    // layer already collapsed variants of the same skill into one
    // Procedure node; here we just need a stable directory name.
    let slug = slugify_for_filesystem(&skill.name);
    if slug.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("could not slugify skill name {:?}", skill.name),
        ));
    }

    let skill_dir = data_dir
        .join("skills")
        .join("_auto_extracted")
        .join(&slug);
    std::fs::create_dir_all(&skill_dir)?;

    let skill_md_path = skill_dir.join("SKILL.md");
    let content = compose_learned_skill_md(skill, &slug);
    std::fs::write(&skill_md_path, content)?;
    Ok(skill_md_path)
}

/// Slugify a (possibly Chinese / mixed) skill name into a filesystem-safe
/// kebab-case identifier. ASCII-only output (Chinese characters get a
/// short hash suffix). This is more permissive than Bundle 21-A's
/// `is_kebab_case` — extraction-generated names sometimes include CJK
/// and we don't want to refuse to persist them; we just need a stable
/// directory name.
fn slugify_for_filesystem(name: &str) -> String {
    // ASCII fast path — keep letters/digits, replace whitespace
    // and separators with hyphens, lowercase, trim/collapse hyphens.
    let mut buf = String::with_capacity(name.len());
    let mut prev_was_sep = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            buf.push(c.to_ascii_lowercase());
            prev_was_sep = false;
        } else if c.is_whitespace() || matches!(c, '-' | '_' | '/' | '.' | ':' | '|') {
            if !prev_was_sep && !buf.is_empty() {
                buf.push('-');
                prev_was_sep = true;
            }
        }
        // Non-ASCII (e.g. CJK) is dropped — handled below as a hash.
    }
    while buf.ends_with('-') {
        buf.pop();
    }

    // If the input was mostly non-ASCII or pure punctuation, the ASCII
    // slug may be empty / digits-only. Append a short stable hash of
    // the original name to disambiguate. We KEEP whatever ASCII we
    // extracted as a human-readable prefix (e.g. "lunar-converter"
    // stays even if the upstream name was "lunar转换器").
    //
    // Don't apply the hash fallback purely on "short ASCII slug" —
    // e.g. "foo" (3 chars) is a perfectly fine slug. Only fall back
    // when buf is empty OR has no ASCII letters at all (only digits /
    // separators that survived the filter).
    if buf.is_empty() || !buf.chars().any(|c| c.is_ascii_alphabetic()) {
        use std::hash::Hasher;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        h.write(name.as_bytes());
        let suffix = format!("{:x}", h.finish());
        let suffix_short = &suffix[..8.min(suffix.len())];
        if buf.is_empty() {
            buf.push_str("skill-");
        } else {
            buf.push('-');
        }
        buf.push_str(suffix_short);
    }

    // Cap length so we don't end up with pathological filesystem paths.
    if buf.len() > 80 {
        buf.truncate(80);
        while buf.ends_with('-') {
            buf.pop();
        }
    }
    buf
}

/// Compose a SKILL.md from a `ParsedSkill`. Frontmatter mirrors the
/// `skills::parse_skill_md` reader's expectations (`name`,
/// `description`). Body is built from the structured fields the LLM
/// emitted; absent fields are omitted rather than rendered as empty
/// sections (clutter would hurt the LLM's read pass).
fn compose_learned_skill_md(skill: &ParsedSkill, slug: &str) -> String {
    // Description must be present + sane for the registry's reader to
    // accept the skill. If the LLM forgot it, synthesize one from the
    // name so the file is still loadable; an ugly description is
    // better than a parse error.
    let description = skill
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(escape_frontmatter_value)
        .unwrap_or_else(|| format!("Auto-extracted skill: {}", skill.name));

    let mut out = String::with_capacity(2048);
    out.push_str("---\n");
    out.push_str("name: ");
    out.push_str(slug);
    out.push('\n');
    out.push_str("description: ");
    out.push_str(&description);
    out.push('\n');
    out.push_str("---\n\n");

    out.push_str("# ");
    out.push_str(skill.name.trim());
    out.push_str("\n\n");
    out.push_str(
        "_This skill was auto-extracted by uClaw's `skill_extraction` \
         pipeline from prior execution logs. Quality depends on the \
         underlying LLM run — review before relying on it for critical \
         work._\n\n",
    );

    if !skill.context.trim().is_empty() {
        out.push_str("## Context\n\n");
        out.push_str(skill.context.trim());
        out.push_str("\n\n");
    }
    if !skill.principles.trim().is_empty() {
        out.push_str("## Principles\n\n");
        out.push_str(skill.principles.trim());
        out.push_str("\n\n");
    }
    if !skill.steps.trim().is_empty() {
        out.push_str("## Steps\n\n");
        out.push_str(skill.steps.trim());
        out.push_str("\n\n");
    }
    if let Some(ap) = skill.anti_patterns.as_deref() {
        let ap = ap.trim();
        if !ap.is_empty() {
            out.push_str("## Anti-patterns\n\n");
            out.push_str(ap);
            out.push_str("\n\n");
        }
    }
    if !skill.pitfalls.trim().is_empty() {
        out.push_str("## Pitfalls\n\n");
        out.push_str(skill.pitfalls.trim());
        out.push_str("\n\n");
    }
    if !skill.signals.is_empty() {
        out.push_str("## Trigger Signals\n\n");
        for sig in &skill.signals {
            out.push_str("- ");
            out.push_str(sig.trim());
            out.push('\n');
        }
        out.push('\n');
    }
    if !skill.signals_seen.is_empty() {
        out.push_str("## Failure Signals Seen\n\n");
        out.push_str("This skill was extracted from runs that exhibited these failure modes:\n\n");
        for sig in &skill.signals_seen {
            out.push_str("- `");
            out.push_str(sig.trim());
            out.push_str("`\n");
        }
        out.push('\n');
    }
    if let Some(hint) = skill.validation_hint.as_deref() {
        let hint = hint.trim();
        if !hint.is_empty() {
            out.push_str("## Validation Hint\n\n");
            out.push_str(hint);
            out.push_str("\n\n");
        }
    }
    if let Some(cat) = skill.category.as_deref() {
        let cat = cat.trim();
        if !cat.is_empty() {
            out.push_str("## Category\n\n");
            out.push_str(cat);
            out.push_str("\n\n");
        }
    }
    if !skill.tags.is_empty() {
        out.push_str("## Tags\n\n");
        let cleaned: Vec<&str> = skill.tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).collect();
        out.push_str(&cleaned.join(", "));
        out.push_str("\n\n");
    }

    out
}

fn escape_frontmatter_value(s: &str) -> String {
    let one_line = s.replace('\n', " ").replace('\r', "");
    if one_line.contains(": ") {
        let q = one_line.replace('"', "\\\"");
        format!("\"{}\"", q)
    } else {
        one_line
    }
}

#[cfg(test)]
mod bundle22_tests {
    use super::*;

    fn sample_skill(name: &str) -> ParsedSkill {
        ParsedSkill {
            name: name.to_string(),
            context: "When the agent sees X, it should do Y.".into(),
            principles: "1. Verify before acting.\n2. Cite sources.".into(),
            steps: "1. Read the input.\n2. Check rule A.\n3. Apply rule B.".into(),
            pitfalls: "Beware of the off-by-one.".into(),
            signals: vec!["err_x".into(), "timeout".into()],
            signals_seen: vec!["TimeoutError".into()],
            validation_hint: Some("Re-run the case and verify it passes.".into()),
            category: Some("repair".into()),
            anti_patterns: Some("Don't retry blindly on 401.".into()),
            description: Some(
                "Tightens recovery on transient API failures. Use when a tool call returns timeout / 5xx."
                    .into(),
            ),
            tags: vec!["recovery".into(), "api".into()],
        }
    }

    #[test]
    fn slugify_ascii_lowercases_and_hyphenates() {
        assert_eq!(slugify_for_filesystem("Hello World"), "hello-world");
        assert_eq!(slugify_for_filesystem("FOO_BAR.baz"), "foo-bar-baz");
        assert_eq!(slugify_for_filesystem("retry-tool-on-403"), "retry-tool-on-403");
    }

    #[test]
    fn slugify_collapses_separators() {
        assert_eq!(slugify_for_filesystem("foo   bar"), "foo-bar");
        assert_eq!(slugify_for_filesystem("foo / bar / baz"), "foo-bar-baz");
        assert_eq!(slugify_for_filesystem("---foo---"), "foo");
    }

    #[test]
    fn slugify_cjk_falls_back_to_hash_suffix() {
        let slug = slugify_for_filesystem("农历转阳历");
        assert!(slug.starts_with("skill-"), "got: {slug}");
        assert!(slug.len() > 8);
    }

    #[test]
    fn slugify_mixed_keeps_ascii_prefix_plus_hash() {
        let slug = slugify_for_filesystem("lunar 转换 helper");
        assert!(slug.starts_with("lunar"), "got: {slug}");
        assert!(slug.contains("helper"), "got: {slug}");
        // Mixed has ASCII letters so no fallback hash; pure-CJK does.
    }

    #[test]
    fn slugify_caps_at_80_chars() {
        let long = "a".repeat(200);
        let slug = slugify_for_filesystem(&long);
        assert!(slug.len() <= 80);
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn compose_md_emits_frontmatter() {
        let skill = sample_skill("Retry on 403");
        let md = compose_learned_skill_md(&skill, "retry-on-403");
        assert!(md.starts_with("---\nname: retry-on-403\n"));
        assert!(md.contains("description: "));
        assert!(md.contains("\n---\n\n"));
    }

    #[test]
    fn compose_md_includes_all_populated_sections() {
        let skill = sample_skill("Retry on 403");
        let md = compose_learned_skill_md(&skill, "retry-on-403");
        for section in &[
            "## Context",
            "## Principles",
            "## Steps",
            "## Anti-patterns",
            "## Pitfalls",
            "## Trigger Signals",
            "## Failure Signals Seen",
            "## Validation Hint",
            "## Category",
            "## Tags",
        ] {
            assert!(md.contains(section), "missing section {section}");
        }
    }

    #[test]
    fn compose_md_omits_empty_sections() {
        let mut skill = sample_skill("Retry on 403");
        skill.pitfalls = String::new();
        skill.anti_patterns = None;
        skill.signals.clear();
        skill.tags.clear();
        let md = compose_learned_skill_md(&skill, "retry-on-403");
        assert!(!md.contains("## Pitfalls"));
        assert!(!md.contains("## Anti-patterns"));
        assert!(!md.contains("## Trigger Signals"));
        assert!(!md.contains("## Tags"));
    }

    #[test]
    fn compose_md_synthesizes_description_when_missing() {
        let mut skill = sample_skill("My Skill");
        skill.description = None;
        let md = compose_learned_skill_md(&skill, "my-skill");
        assert!(md.contains("Auto-extracted skill: My Skill"));
    }

    #[test]
    fn compose_md_quotes_description_containing_colon() {
        let mut skill = sample_skill("Foo");
        skill.description = Some("Like Z: do W. Use when Y.".into());
        let md = compose_learned_skill_md(&skill, "foo");
        assert!(
            md.contains("description: \"Like Z: do W. Use when Y.\""),
            "got: {md}"
        );
    }

    #[test]
    fn persist_writes_skill_md_under_auto_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let skill = sample_skill("Retry on 403");
        let path = persist_learned_skill_to_disk(dir.path(), &skill).unwrap();
        assert!(path.ends_with("SKILL.md"));
        let parent = path.parent().unwrap();
        assert!(parent.starts_with(dir.path().join("skills/_auto_extracted")));
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("# Retry on 403"));
        assert!(body.contains("auto-extracted"));
    }

    #[test]
    fn persist_is_idempotent_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let mut skill = sample_skill("Foo");
        let first = persist_learned_skill_to_disk(dir.path(), &skill).unwrap();
        skill.principles = "Updated principles".into();
        let second = persist_learned_skill_to_disk(dir.path(), &skill).unwrap();
        assert_eq!(first, second);
        let body = std::fs::read_to_string(&second).unwrap();
        assert!(body.contains("Updated principles"));
    }

    #[test]
    fn persist_rejects_unsluggable_name() {
        let dir = tempfile::tempdir().unwrap();
        // Pure punctuation collapses to empty + has no chars to hash off of.
        // (Our impl hashes the original name as fallback, so this case
        // actually still produces something. Verify behaviour rather than
        // forcing a hard rejection.)
        let mut skill = sample_skill("---");
        let result = persist_learned_skill_to_disk(dir.path(), &skill);
        // We accept either: success with a hash slug, OR InvalidInput.
        // What matters is we don't write to a wonky path.
        if let Ok(path) = result {
            assert!(!path.to_string_lossy().contains("///"));
        }
        skill.name = "".into();
        let err = persist_learned_skill_to_disk(dir.path(), &skill);
        assert!(err.is_err(), "empty name must be rejected");
    }
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

/// Estimate the proportion of CJK characters in a string.
///
/// Counts characters in the Unicode ranges:
///   - CJK Unified Ideographs (U+4E00–U+9FFF)
///   - CJK Extension A (U+3400–U+4DBF)
///   - CJK Compatibility Ideographs (U+F900–U+FAFF)
///   - Hiragana (U+3040–U+309F), Katakana (U+30A0–U+30FF)
///   - Hangul (U+AC00–U+D7AF)
///
/// Returns 0.0 for empty strings and strings without CJK characters.
pub fn cjk_char_ratio(s: &str) -> f32 {
    let total = s.chars().count();
    if total == 0 {
        return 0.0;
    }
    let cjk_count = s.chars().filter(|c| is_cjk_char(*c)).count();
    cjk_count as f32 / total as f32
}

/// Check if a single character falls within CJK Unicode ranges.
fn is_cjk_char(c: char) -> bool {
    matches!(
        c,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}'  // CJK Extension A
        | '\u{F900}'..='\u{FAFF}'  // CJK Compatibility Ideographs
        | '\u{3040}'..='\u{309F}'  // Hiragana
        | '\u{30A0}'..='\u{30FF}'  // Katakana
        | '\u{AC00}'..='\u{D7AF}'  // Hangul Syllables
    )
}

/// Word-level bigrams for mixed CJK/ASCII text.
///
/// Splitting strategy:
///   - ASCII text: split on whitespace/punctuation, each word becomes a token
///   - CJK text (Han/Kana/Hangul): each CJK character is its own token
///     (no dictionary needed; CJK characters carry meaning individually)
///   - Punctuation and whitespace are discarded as tokens, but act as boundary
///     markers between ASCII words.
///
/// This produces better semantic bigrams than pure character bigrams because
/// "游戏开发" tokenizes to ["游","戏","开","发"] → bigrams ["游戏","戏开","开发"]
/// while "开发游戏" tokenizes to ["开","发","游","戏"] → ["开发","发游","游戏"] —
/// both share "开发" and "游戏" bigrams.
pub fn word_bigrams(s: &str) -> std::collections::HashSet<String> {
    let tokens = tokenize_mixed(s);
    if tokens.len() < 2 {
        return std::collections::HashSet::new();
    }
    let mut set = std::collections::HashSet::new();
    for w in tokens.windows(2) {
        set.insert(format!("{}{}", w[0], w[1]));
    }
    set
}

/// Split mixed CJK/ASCII text into word-like tokens.
///
/// Heuristic:
///   - Run of CJK characters: each char is a separate token
///   - Run of ASCII alphanumeric: accumulated then emitted as one token
///   - Whitespace and punctuation: discarded (act as token boundaries)
fn tokenize_mixed(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut ascii_buf = String::new();

    let flush_ascii = |buf: &mut String, out: &mut Vec<String>| {
        if !buf.is_empty() {
            out.push(buf.clone());
            buf.clear();
        }
    };

    for c in s.chars() {
        if c.is_whitespace() || c.is_ascii_punctuation() {
            flush_ascii(&mut ascii_buf, &mut tokens);
            // whitespace/punctuation discarded
        } else if is_cjk_char(c) {
            flush_ascii(&mut ascii_buf, &mut tokens);
            tokens.push(c.to_string());
        } else if c.is_alphanumeric() {
            ascii_buf.push(c);
        }
        // Other Unicode (emoji, symbols, etc.) — discard
    }
    flush_ascii(&mut ascii_buf, &mut tokens);
    tokens
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
    let need_anti_patterns_update = skill.anti_patterns.is_some();
    let need_description_update = skill.description.is_some();
    let need_tags_update = !skill.tags.is_empty();
    if need_signals_update || need_signals_seen_update || need_hint_update
        || need_category_update || need_anti_patterns_update || need_description_update
        || need_tags_update
    {
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
            if let Some(ap) = skill.anti_patterns.as_ref() {
                obj.insert(
                    "anti_patterns".to_string(),
                    serde_json::Value::String(ap.clone()),
                );
            }
            if let Some(desc) = skill.description.as_ref() {
                obj.insert(
                    "description".to_string(),
                    serde_json::Value::String(truncate_to_char_count(desc, 120)),
                );
            }
            if need_tags_update {
                // V19 tags: union with existing instead of overwrite.
                // Rationale: tags are stable domain labels — a second
                // extraction might surface a tag the first one missed,
                // and unioning monotonically expands the skill's
                // workspace coverage. Empty re-extraction preserves old
                // (same pattern as signals).
                //
                // Future user-facing tag editor should bypass this fn
                // and write directly via a dedicated IPC if "narrow"
                // semantics are needed.
                let existing_tags: Vec<String> = obj
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let mut seen: std::collections::HashSet<String> = existing_tags.iter().cloned().collect();
                let mut merged = existing_tags;
                for t in &skill.tags {
                    if seen.insert(t.clone()) {
                        merged.push(t.clone());
                    }
                }
                obj.insert(
                    "tags".to_string(),
                    serde_json::Value::Array(
                        merged.into_iter().map(serde_json::Value::String).collect(),
                    ),
                );
            }
        }
        if let Err(e) = store.update_node(&existing.id, None, None, Some(&metadata)) {
            tracing::warn!(
                node_id = %existing.id,
                err = %e,
                "skill_parser: metadata update on upgrade failed (continuing)"
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
    fn store_skill_defaults_lifecycle_to_draft() {
        // PR-mattpocock-3: newly-extracted skills must start as 'draft' so
        // they don't enter the manifest top-30 until they've been cited
        // enough times to prove useful (see PROMOTION_THRESHOLD in
        // record_skill_cited).
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        let conn = std::sync::Arc::new(std::sync::Mutex::new(conn));
        let store = MemoryGraphStore::new(conn);

        let skill = ParsedSkill {
            name: "draft 默认技能".to_string(),
            context: "ctx".to_string(),
            principles: "p".to_string(),
            steps: "s".to_string(),
            pitfalls: "x".to_string(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
            anti_patterns: None,
            description: None,
            tags: vec![],
        };

        let node = store_skill_as_procedure(&store, &skill, "default").unwrap();
        let meta = node.metadata.expect("metadata should be set");
        let lifecycle = meta.get("lifecycle").and_then(|v| v.as_str());
        assert_eq!(lifecycle, Some("draft"),
            "newly stored skill should default to lifecycle=draft, got {:?}", lifecycle);
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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
        anti_patterns: None,
        description: None,
            tags: vec![],
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

    // ─── PR-mattpocock-A: anti_patterns + description tag tests ─────────

    #[test]
    fn parses_anti_patterns_tag() {
        let xml = r#"<skill_report><new_skills><skill>
<name>api-retry-strategy</name>
<context>外部 API 频繁失败时的重试策略</context>
<principles>区分瞬时与永久错误</principles>
<steps>1. 检测状态码 2. 指数退避</steps>
<pitfalls>不要无限重试</pitfalls>
<anti_patterns>不要在 4xx (除 429) 上重试 — 这是客户端错误，重试只会重复失败</anti_patterns>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_report(xml);
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].anti_patterns.as_ref().unwrap().contains("4xx"));
    }

    #[test]
    fn empty_anti_patterns_yields_none() {
        let xml = r#"<skill_report><new_skills><skill>
<name>no-ap-skill</name>
<context>ctx</context>
<principles>p</principles>
<steps>s</steps>
<pitfalls>pt</pitfalls>
<anti_patterns></anti_patterns>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_report(xml);
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].anti_patterns.is_none(), "empty <anti_patterns> must yield None");
    }

    #[test]
    fn parses_description_field() {
        let xml = r#"<skill_report><new_skills><skill>
<name>desc-skill</name>
<description>跨源校验股票财报，当 Yahoo 返回 403 时切换源</description>
<context>股票研究</context>
<principles>p</principles>
<steps>s</steps>
<pitfalls>pt</pitfalls>
</skill></new_skills></skill_report>"#;
        let parsed = parse_skill_report(xml);
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].description.as_deref().unwrap().contains("403"));
    }

    #[test]
    fn description_truncates_to_120_chars_on_persist() {
        // Pure-function test of the truncate helper. The persistence layer
        // applies it via metadata["description"] = truncate_to_char_count(...)
        // (see store_skill_as_procedure + upgrade_existing_skill).
        let long = "用".repeat(200); // 200 CJK chars; UTF-8 bytes > char count
        let truncated = truncate_to_char_count(&long, 120);
        assert_eq!(truncated.chars().count(), 120,
            "truncate_to_char_count must return exactly max_chars chars");
        assert!(truncated.ends_with('…'),
            "truncated string must end with ellipsis marker");
    }

    #[test]
    fn description_passes_through_when_short() {
        let s = "短描述";
        let same = truncate_to_char_count(s, 120);
        assert_eq!(same, s, "short strings must pass through unchanged");
    }

    #[test]
    fn anti_patterns_and_description_persist_on_upgrade() {
        let store = MemoryGraphStore::new(std::sync::Arc::new(
            std::sync::Mutex::new(rusqlite::Connection::open_in_memory().unwrap()),
        ));
        let _ = store.conn.lock().unwrap().execute_batch(crate::db::migrations::V4_MEMORY_GRAPH);

        let space = "default";
        let s1 = ParsedSkill {
            name: "upgrade-target".into(),
            context: "v1".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
            anti_patterns: Some("v1 anti-pattern".into()),
            description: Some("v1 description".into()),
            tags: vec![],
        };
        let n1 = store_skill_as_procedure(&store, &s1, space).unwrap();

        // Re-extract with new anti_patterns + description → both replaced.
        let s2 = ParsedSkill {
            name: "upgrade-target".into(),
            context: "v2".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
            anti_patterns: Some("v2 anti-pattern".into()),
            description: Some("v2 description".into()),
            tags: vec![],
        };
        let n2 = store_skill_as_procedure(&store, &s2, space).unwrap();
        assert_eq!(n1.id, n2.id, "expected dedup to reuse node id");

        let updated = store.get_node(&n1.id).unwrap().unwrap();
        let ap = updated.metadata.as_ref()
            .and_then(|m| m.get("anti_patterns"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let desc = updated.metadata.as_ref()
            .and_then(|m| m.get("description"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        assert_eq!(ap.as_deref(), Some("v2 anti-pattern"));
        assert_eq!(desc.as_deref(), Some("v2 description"));

        // Now: re-extract with None → existing values preserved (empty-keeps-old).
        let s3 = ParsedSkill {
            name: "upgrade-target".into(),
            context: "v3".into(),
            principles: "p".into(),
            steps: "s".into(),
            pitfalls: "pt".into(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
            anti_patterns: None,
            description: None,
            tags: vec![],
        };
        let _ = store_skill_as_procedure(&store, &s3, space).unwrap();
        let final_node = store.get_node(&n1.id).unwrap().unwrap();
        let final_ap = final_node.metadata.as_ref()
            .and_then(|m| m.get("anti_patterns"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        assert_eq!(final_ap.as_deref(), Some("v2 anti-pattern"),
            "None re-extraction must not wipe existing anti_patterns");
    }

    // ─── V19 tags: parse + persist + upgrade ────────────────────────────

    /// `<tags><tag>…</tag></tags>` must be parsed into a normalized Vec<String>:
    /// trim + lowercase + dedup, preserving the first occurrence order. The
    /// normalization mirrors `tauri_commands::normalize_skill_tags` so the
    /// manifest filter's bare string comparison works.
    #[test]
    fn parse_tags_block_normalizes_and_dedups() {
        let xml = r#"
<skill_report>
<new_skills>
<skill>
<name>test-skill</name>
<context>ctx</context>
<principles>p</principles>
<steps>s</steps>
<pitfalls>x</pitfalls>
<tags>
  <tag>Engineering</tag>
  <tag>  testing  </tag>
  <tag>ENGINEERING</tag>
  <tag></tag>
  <tag>testing</tag>
</tags>
</skill>
</new_skills>
</skill_report>"#;
        let skills = parse_skill_report(xml);
        assert_eq!(skills.len(), 1);
        assert_eq!(
            skills[0].tags,
            vec!["engineering".to_string(), "testing".to_string()],
            "tags must be lowercased + deduped + empty-dropped, preserving first-occurrence order"
        );
    }

    /// Skills with no `<tags>` block stay at empty Vec → V19's "untagged =
    /// global" rule keeps them visible in every workspace. This is the
    /// backwards-compat default for all pre-tags learned skills.
    #[test]
    fn parse_skill_without_tags_block_yields_empty_vec() {
        let xml = r#"
<skill_report>
<new_skills>
<skill>
<name>untagged-skill</name>
<context>ctx</context>
<principles>p</principles>
<steps>s</steps>
<pitfalls>x</pitfalls>
</skill>
</new_skills>
</skill_report>"#;
        let skills = parse_skill_report(xml);
        assert_eq!(skills.len(), 1);
        assert!(skills[0].tags.is_empty(),
            "no <tags> block → empty Vec, NOT a sentinel value");
    }

    /// Non-empty `skill.tags` lands in `metadata.tags` as a JSON array.
    /// Empty `skill.tags` must NOT write the key at all — V19's filter
    /// uses presence of the array, and writing `[]` would be conformant
    /// but wasteful (and visually noisy in Settings → 已学技能).
    #[test]
    fn store_persists_tags_into_metadata() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        let conn = std::sync::Arc::new(std::sync::Mutex::new(conn));
        let store = MemoryGraphStore::new(conn);

        let skill = ParsedSkill {
            name: "tagged-skill".to_string(),
            context: "ctx".to_string(),
            principles: "p".to_string(),
            steps: "s".to_string(),
            pitfalls: "x".to_string(),
            signals: vec![],
            signals_seen: vec![],
            validation_hint: None,
            category: None,
            anti_patterns: None,
            description: None,
            tags: vec!["engineering".to_string(), "testing".to_string()],
        };
        let node = store_skill_as_procedure(&store, &skill, "default").unwrap();
        let meta = node.metadata.expect("metadata set");
        let tags = meta.get("tags").and_then(|v| v.as_array()).expect("tags array");
        let got: Vec<&str> = tags.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(got, vec!["engineering", "testing"]);
    }

    #[test]
    fn store_omits_tags_key_when_empty() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        let conn = std::sync::Arc::new(std::sync::Mutex::new(conn));
        let store = MemoryGraphStore::new(conn);

        let skill = ParsedSkill {
            name: "untagged-skill".to_string(),
            context: "ctx".to_string(), principles: "p".to_string(),
            steps: "s".to_string(), pitfalls: "x".to_string(),
            signals: vec![], signals_seen: vec![],
            validation_hint: None, category: None,
            anti_patterns: None, description: None,
            tags: vec![],
        };
        let node = store_skill_as_procedure(&store, &skill, "default").unwrap();
        let meta = node.metadata.expect("metadata set");
        assert!(meta.get("tags").is_none(),
            "empty tags Vec must omit the metadata key entirely (not write [])");
    }

    /// Upgrade path: a re-extraction with new tags must **union** with the
    /// existing set (not overwrite). Rationale: tags are stable domain
    /// labels, and unioning monotonically expands the workspace coverage —
    /// safer than narrowing it accidentally.
    #[test]
    fn upgrade_unions_tags_with_existing() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        let conn = std::sync::Arc::new(std::sync::Mutex::new(conn));
        let store = MemoryGraphStore::new(conn);

        // First extraction: ["engineering"]
        let v1 = ParsedSkill {
            name: "shared-name".to_string(),
            context: "ctx".to_string(), principles: "p".to_string(),
            steps: "s".to_string(), pitfalls: "x".to_string(),
            signals: vec![], signals_seen: vec![],
            validation_hint: None, category: None,
            anti_patterns: None, description: None,
            tags: vec!["engineering".to_string()],
        };
        let node1 = store_skill_as_procedure(&store, &v1, "default").unwrap();

        // Second extraction (same name → upgrade path): ["testing", "engineering"]
        let v2 = ParsedSkill {
            name: "shared-name".to_string(),
            context: "ctx2".to_string(), principles: "p".to_string(),
            steps: "s".to_string(), pitfalls: "x".to_string(),
            signals: vec![], signals_seen: vec![],
            validation_hint: None, category: None,
            anti_patterns: None, description: None,
            tags: vec!["testing".to_string(), "engineering".to_string()],
        };
        let node2 = store_skill_as_procedure(&store, &v2, "default").unwrap();
        assert_eq!(node1.id, node2.id, "dedup must reuse the same node id");

        // Refresh the node to read post-upgrade metadata.
        let refreshed = store.get_node(&node1.id).unwrap().expect("node exists");
        let meta = refreshed.metadata.expect("metadata set");
        let tags = meta.get("tags").and_then(|v| v.as_array()).expect("tags array");
        let got: Vec<&str> = tags.iter().filter_map(|v| v.as_str()).collect();
        assert!(got.contains(&"engineering"),
            "union must keep the original tag; got: {:?}", got);
        assert!(got.contains(&"testing"),
            "union must add the new tag; got: {:?}", got);
        assert_eq!(got.len(), 2,
            "no spurious duplicates from union; got: {:?}", got);
    }

    /// Upgrade path: empty re-extraction tags must NOT wipe the existing
    /// set. Matches the "empty preserves old" convention used for signals
    /// + signals_seen + validation_hint + anti_patterns.
    #[test]
    fn upgrade_with_empty_tags_preserves_existing() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        let conn = std::sync::Arc::new(std::sync::Mutex::new(conn));
        let store = MemoryGraphStore::new(conn);

        let v1 = ParsedSkill {
            name: "shared-name".to_string(),
            context: "ctx".to_string(), principles: "p".to_string(),
            steps: "s".to_string(), pitfalls: "x".to_string(),
            signals: vec![], signals_seen: vec![],
            validation_hint: None, category: None,
            anti_patterns: None, description: None,
            tags: vec!["research".to_string()],
        };
        store_skill_as_procedure(&store, &v1, "default").unwrap();

        let v2 = ParsedSkill {
            name: "shared-name".to_string(),
            context: "ctx2".to_string(), principles: "p".to_string(),
            steps: "s".to_string(), pitfalls: "x".to_string(),
            signals: vec![], signals_seen: vec![],
            validation_hint: None, category: None,
            anti_patterns: None, description: None,
            tags: vec![],  // empty re-extraction
        };
        let node = store_skill_as_procedure(&store, &v2, "default").unwrap();
        let refreshed = store.get_node(&node.id).unwrap().expect("node exists");
        let meta = refreshed.metadata.expect("metadata set");
        let tags = meta.get("tags").and_then(|v| v.as_array()).expect("tags array still present");
        let got: Vec<&str> = tags.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(got, vec!["research"],
            "empty re-extraction must preserve the existing tag set");
    }
}
