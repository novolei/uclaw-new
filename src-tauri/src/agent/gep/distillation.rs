//! Gene Distillation — LLM-based distillation of LearningCards into GEP Genes.
//!
//! Handles the prompt engineering, XML parsing, validation, and
//! duplicate detection for the Gene distillation pipeline.
//!
//! Distillation prompt follows the "constraint-first" approach:
//! 1. Analyze AVOID cues (failure info) and constraints (safety bounds)
//! 2. Derive strategy steps
//! 3. Define signals_match triggers

use anyhow::{bail, Result};
use tracing::debug;

use super::types::*;

/// The Gene distillation system prompt.
///
/// Follows the constraint-first approach (Q1 brainstomed decision):
/// LLM is instructed to first consider AVOID cues and constraints before
/// deriving strategy, preventing over-optimistic solutions.
pub const GENE_DISTILLATION_SYSTEM_PROMPT: &str = include_str!("distillation_prompt.txt");

/// Format LearningCards as structured LLM input.
///
/// Noise cards are filtered out. Remaining cards are grouped by type
/// and formatted with their pre-extracted strategy hints.
pub fn format_learning_cards(cards: &[LearningCard]) -> String {
    let mut output = String::from("## 待蒸馏候选\n\n");

    // Group by type
    let failures: Vec<_> = cards
        .iter()
        .filter(|c| c.card_type == LearningCardType::FailureLesson)
        .collect();
    let successes: Vec<_> = cards
        .iter()
        .filter(|c| c.card_type == LearningCardType::SuccessPattern)
        .collect();
    let tips: Vec<_> = cards
        .iter()
        .filter(|c| c.card_type == LearningCardType::OptimizationTip)
        .collect();

    // Format failures first (highest signal)
    for (i, card) in failures.iter().enumerate() {
        output.push_str(&format_card(card, i + 1, "FAILURE_LESSON"));
    }

    for (i, card) in successes.iter().enumerate() {
        output.push_str(&format_card(card, i + 1, "SUCCESS_PATTERN"));
    }

    for (i, card) in tips.iter().enumerate() {
        output.push_str(&format_card(card, i + 1, "OPTIMIZATION_TIP"));
    }

    if output == "## 待蒸馏候选\n\n" {
        output.push_str("(无有效候选)\n");
    }

    output
}

fn format_card(card: &LearningCard, idx: usize, type_label: &str) -> String {
    let mut s = format!(
        "[{} | {} | score={:.2}]\n{}\n",
        idx, type_label, card.score, card.raw
    );

    if let Some(signal) = &card.failure_signal {
        s.push_str(&format!("  → signal: {}\n", signal));
    }
    if let Some(tool) = &card.tool_name {
        s.push_str(&format!("  → tool: {}\n", tool));
    }
    if let Some(condition) = &card.strategy_hint.condition {
        s.push_str(&format!("  → condition: {}\n", condition));
    }
    if let Some(action) = &card.strategy_hint.action {
        s.push_str(&format!("  → action: {}\n", action));
    }
    if let Some(reason) = &card.strategy_hint.reason {
        s.push_str(&format!("  → reason: {}\n", reason));
    }

    s.push('\n');
    s
}

/// Parse LLM output XML into a Gene struct.
///
/// Expected format:
/// ```xml
/// <gene>
/// <id>kebab-case-id</id>
/// <category>repair|optimize|innovate</category>
/// <signals_match>comma,separated,signals</signals_match>
/// <summary>One-line summary ≤60 chars</summary>
/// <strategy>
/// <step>Step 1</step>
/// <step>Step 2</step>
/// </strategy>
/// <avoid>avoid cue 1</avoid>
/// <constraints>
/// <max_files>3</max_files>
/// <forbidden_paths>.env,secrets/</forbidden_paths>
/// </constraints>
/// <validation>How to validate</validation>
/// </gene>
/// ```
pub fn parse_gene_xml(xml: &str) -> Result<Gene> {
    // Quick check for NO_GENE
    if xml.trim().is_empty() || xml.contains("[NO_GENE]") {
        bail!("No distillable gene found");
    }

    let gene_id = extract_tag(xml, "id")?;
    let category_str = extract_tag(xml, "category")?;
    let signals_str = extract_tag(xml, "signals_match")?;
    let summary = extract_tag(xml, "summary")?;
    let strategy_str = extract_xml_section(xml, "strategy")?;
    let avoid_str = extract_tag_optional(xml, "avoid").unwrap_or_default();
    let constraints_str = extract_xml_section(xml, "constraints")?;
    let validation = extract_tag_optional(xml, "validation").unwrap_or_default();

    let category = match category_str.to_lowercase().as_str() {
        "repair" => GeneCategory::Repair,
        "optimize" => GeneCategory::Optimize,
        "innovate" => GeneCategory::Innovate,
        _ => bail!("Unknown gene category: {}", category_str),
    };

    let signals_match: Vec<String> = signals_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let strategy: Vec<String> = extract_tag_values(&strategy_str, "step");

    let avoid: Vec<String> = avoid_str
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let max_files = extract_tag_optional(&constraints_str, "max_files")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let forbidden_paths: Vec<String> = extract_tag_optional(&constraints_str, "forbidden_paths")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let now = chrono::Utc::now().timestamp_millis();
    let mut gene = Gene {
        gene_id,
        version: "1.0".to_string(),
        category,
        signals_match,
        summary,
        strategy,
        avoid,
        constraints: GeneConstraints {
            max_files,
            forbidden_paths,
        },
        validation,
        asset_id: String::new(),
        status: GeneStatus::Active,
        created_at: now,
        updated_at: now,
    };

    gene.asset_id = gene.compute_asset_id();
    Ok(gene)
}

/// Validate a Gene against hard constraints.
pub fn validate_gene(gene: &Gene) -> Result<()> {
    if gene.summary.len() > 60 {
        bail!("Summary too long: {} chars (max 60)", gene.summary.len());
    }
    if gene.strategy.len() > 4 {
        bail!("Strategy has {} steps (max 4)", gene.strategy.len());
    }
    if gene.avoid.len() > 3 {
        bail!(
            "AVOID has {} cues (max 3 for initial gene)",
            gene.avoid.len()
        );
    }
    if gene.constraints.max_files == 0 {
        bail!("Constraints must specify max_files > 0");
    }
    if gene.signals_match.is_empty() {
        bail!("Gene must have at least one signals_match");
    }
    if gene.strategy.is_empty() {
        bail!("Gene must have at least one strategy step");
    }
    Ok(())
}

/// Check if a new gene duplicates an existing one.
///
/// Duplicate detection is based on signals_match overlap:
/// if more than 50% of signals_match overlap, it's considered a duplicate.
pub fn check_duplicate(gene: &Gene, existing: &[Gene]) -> Option<String> {
    for existing_gene in existing {
        let overlap = gene
            .signals_match
            .iter()
            .filter(|s| {
                existing_gene
                    .signals_match
                    .iter()
                    .any(|es| es.to_lowercase() == s.to_lowercase())
            })
            .count();

        let total = gene
            .signals_match
            .len()
            .max(existing_gene.signals_match.len());
        if total > 0 && overlap as f64 / total as f64 > 0.5 {
            debug!(
                "Gene {} duplicates {} ({}% signal overlap)",
                gene.gene_id,
                existing_gene.gene_id,
                (overlap as f64 / total as f64 * 100.0) as u32,
            );
            return Some(existing_gene.gene_id.clone());
        }
    }
    None
}

// ─── XML Parsing Helpers ──────────────────────────────────────────────────

fn extract_tag(xml: &str, tag: &str) -> Result<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);

    let start = xml
        .find(&open)
        .ok_or_else(|| anyhow::anyhow!("Missing <{}> tag in gene XML", tag))?;
    let end = xml[start..]
        .find(&close)
        .ok_or_else(|| anyhow::anyhow!("Missing </{}> tag in gene XML", tag))?;

    let value = &xml[start + open.len()..start + end];
    Ok(value.trim().to_string())
}

fn extract_tag_optional(xml: &str, tag: &str) -> Option<String> {
    extract_tag(xml, tag).ok()
}

fn extract_xml_section(xml: &str, section: &str) -> Result<String> {
    let open = format!("<{}>", section);
    let close = format!("</{}>", section);

    let start = xml
        .find(&open)
        .ok_or_else(|| anyhow::anyhow!("Missing <{}> section in gene XML", section))?;
    let end = xml[start..]
        .find(&close)
        .ok_or_else(|| anyhow::anyhow!("Missing </{}> section in gene XML", section))?;

    Ok(xml[start + open.len()..start + end].to_string())
}

fn extract_tag_values(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let mut values = Vec::new();
    let mut remaining = xml;

    while let Some(start) = remaining.find(&open) {
        if let Some(end) = remaining[start..].find(&close) {
            let value = &remaining[start + open.len()..start + end];
            values.push(value.trim().to_string());
            remaining = &remaining[start + end + close.len()..];
        } else {
            break;
        }
    }

    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_learning_cards() {
        let cards = vec![
            LearningCard {
                raw: "Yahoo 403时切换Alpha Vantage成功".to_string(),
                card_type: LearningCardType::FailureLesson,
                failure_signal: Some("403".to_string()),
                tool_name: Some("read_file".to_string()),
                strategy_hint: StrategyHint {
                    condition: Some("Yahoo 403".to_string()),
                    action: Some("切换到Alpha Vantage".to_string()),
                    reason: Some("schema兼容".to_string()),
                },
                files_touched: vec!["stock.py".to_string()],
                session_id: "s1".to_string(),
                score: 0.3,
                timestamp: 1715779200000,
            },
            LearningCard {
                raw: "应该多注意细节".to_string(),
                card_type: LearningCardType::Noise,
                failure_signal: None,
                tool_name: None,
                strategy_hint: StrategyHint::default(),
                files_touched: vec![],
                session_id: "s1".to_string(),
                score: 0.7,
                timestamp: 1715779200000,
            },
        ];

        let formatted = format_learning_cards(&cards);
        // Noise should be filtered
        assert!(!formatted.contains("应该多注意细节"));
        // Failure lesson should be present
        assert!(formatted.contains("FAILURE_LESSON"));
        assert!(formatted.contains("Yahoo 403"));
    }

    #[test]
    fn test_parse_gene_xml_valid() {
        let xml = r#"<gene>
<id>test-gene</id>
<category>repair</category>
<signals_match>403,timeout</signals_match>
<summary>测试基因摘要</summary>
<strategy>
<step>步骤1</step>
<step>步骤2</step>
</strategy>
<avoid>不要重试</avoid>
<constraints>
<max_files>3</max_files>
<forbidden_paths>.env</forbidden_paths>
</constraints>
<validation>验证schema</validation>
</gene>"#;

        let gene = parse_gene_xml(xml).unwrap();
        assert_eq!(gene.gene_id, "test-gene");
        assert_eq!(gene.category, GeneCategory::Repair);
        assert_eq!(gene.signals_match, vec!["403", "timeout"]);
        assert_eq!(gene.summary, "测试基因摘要");
        assert_eq!(gene.strategy.len(), 2);
        assert_eq!(gene.avoid, vec!["不要重试"]);
        assert_eq!(gene.constraints.max_files, 3);
        assert_eq!(gene.constraints.forbidden_paths, vec![".env"]);
        assert_eq!(gene.validation, "验证schema");
        // Asset ID should be computed
        assert_eq!(gene.asset_id.len(), 64);
    }

    #[test]
    fn test_parse_gene_xml_no_gene() {
        let result = parse_gene_xml("[NO_GENE]");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No distillable gene"));
    }

    #[test]
    fn test_validate_gene_passes() {
        let gene = Gene {
            gene_id: "test".to_string(),
            version: "1.0".to_string(),
            category: GeneCategory::Repair,
            signals_match: vec!["403".to_string()],
            summary: "short summary".to_string(),
            strategy: vec!["step1".to_string(), "step2".to_string()],
            avoid: vec!["don't retry".to_string()],
            constraints: GeneConstraints {
                max_files: 3,
                forbidden_paths: vec![],
            },
            validation: "test".to_string(),
            asset_id: String::new(),
            status: GeneStatus::Active,
            created_at: 0,
            updated_at: 0,
        };
        assert!(validate_gene(&gene).is_ok());
    }

    #[test]
    fn test_validate_gene_summary_too_long() {
        let mut gene = Gene {
            gene_id: "test".to_string(),
            version: "1.0".to_string(),
            category: GeneCategory::Repair,
            signals_match: vec!["403".to_string()],
            summary: "a".repeat(61),
            strategy: vec!["step".to_string()],
            avoid: vec![],
            constraints: GeneConstraints {
                max_files: 1,
                forbidden_paths: vec![],
            },
            validation: "test".to_string(),
            asset_id: String::new(),
            status: GeneStatus::Active,
            created_at: 0,
            updated_at: 0,
        };
        assert!(validate_gene(&gene).is_err());
    }

    #[test]
    fn test_check_duplicate() {
        let gene1 = Gene {
            gene_id: "g1".to_string(),
            version: "1.0".to_string(),
            category: GeneCategory::Repair,
            signals_match: vec![
                "403".to_string(),
                "timeout".to_string(),
                "api_error".to_string(),
            ],
            summary: "s1".to_string(),
            strategy: vec!["s1".to_string()],
            avoid: vec![],
            constraints: GeneConstraints {
                max_files: 1,
                forbidden_paths: vec![],
            },
            validation: "v".to_string(),
            asset_id: String::new(),
            status: GeneStatus::Active,
            created_at: 0,
            updated_at: 0,
        };
        let gene2 = Gene {
            gene_id: "g2".to_string(),
            version: "1.0".to_string(),
            category: GeneCategory::Repair,
            signals_match: vec![
                "403".to_string(),
                "timeout".to_string(),
                "other".to_string(),
            ],
            summary: "s2".to_string(),
            strategy: vec!["s2".to_string()],
            avoid: vec![],
            constraints: GeneConstraints {
                max_files: 1,
                forbidden_paths: vec![],
            },
            validation: "v".to_string(),
            asset_id: String::new(),
            status: GeneStatus::Active,
            created_at: 0,
            updated_at: 0,
        };

        let existing = vec![gene2];
        // 50% overlap (403 matches, timeout vs api_error not)
        let dup = check_duplicate(&gene1, &existing);
        assert!(dup.is_some());
    }
}
