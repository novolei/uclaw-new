//! Gene Retrieval — two-stage hybrid matching pipeline.
//!
//! Stage 1 (hot path): Exact signals_match substring matching, O(n), zero latency.
//! Stage 2 (cold path): Semantic embedding vector search via fastembed.
//!   Only activated when Stage 1 produces no hits.
//!
//! Brainstomed decision (Q3): Two-stage hybrid retrieval.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tracing::debug;

use crate::memu::client::MemUClient;
use crate::memu::embedding::cosine_sim;

use super::types::*;

/// Handles Gene retrieval for injection into the agent's system prompt.
pub struct GeneRetriever {
    /// All genes available for retrieval
    genes: Vec<Gene>,
    /// Whether Stage 2 (semantic search) is enabled
    semantic_fallback_enabled: bool,
    /// memU client for embedding (None if fastembed unavailable)
    memu_client: Option<Arc<MemUClient>>,
    /// Cached gene embedding vectors (gene_id → vector)
    gene_embeddings: Mutex<HashMap<String, Vec<f32>>>,
    /// Cached effective streaks for ranking (gene_id → streak, computed from Capsule history)
    gene_effective_streaks: HashMap<String, f32>,
}

impl GeneRetriever {
    /// Create a new retriever with the given genes.
    pub fn new(
        genes: Vec<Gene>,
        semantic_fallback_enabled: bool,
        memu_client: Option<Arc<MemUClient>>,
    ) -> Self {
        Self {
            genes,
            semantic_fallback_enabled,
            memu_client,
            gene_embeddings: Mutex::new(HashMap::new()),
            gene_effective_streaks: HashMap::new(),
        }
    }

    /// Match genes against user message and recent tool errors.
    ///
    /// Returns up to `max_genes` matched genes, ranked by
    /// `effective_streak × match_score`, with one gene per category.
    pub async fn match_genes(
        &self,
        user_message: &str,
        tool_errors: &[String],
        max_genes: usize,
    ) -> Vec<GeneMatch> {
        // Stage 1: Exact substring match on signals_match
        let mut matches = self.stage1_exact_match(user_message, tool_errors);

        // Stage 2: Semantic fallback (only if Stage 1 produced nothing)
        if matches.is_empty() && self.semantic_fallback_enabled && self.memu_client.is_some() {
            debug!("Stage 1 no hits — falling back to semantic search");
            matches = self.stage2_semantic_match(user_message).await;
        }

        // Rank by effective_streak × match_score
        self.rank_matches(matches, max_genes)
    }

    /// Stage 1: Exact signals_match substring matching.
    ///
    /// - User message match: weight 1.0
    /// - Tool error match: weight 2.0 (higher priority)
    fn stage1_exact_match(
        &self,
        user_message: &str,
        tool_errors: &[String],
    ) -> Vec<GeneMatchCandidate> {
        let lower_msg = user_message.to_lowercase();
        let mut candidates = Vec::new();

        for gene in &self.genes {
            if gene.status != GeneStatus::Active {
                continue;
            }

            let mut score = 0.0;

            for signal in &gene.signals_match {
                let lower_signal = signal.to_lowercase();

                // Check user message
                if lower_msg.contains(&lower_signal) {
                    score += 1.0;
                }

                // Check tool errors (higher weight)
                for err in tool_errors {
                    if err.to_lowercase().contains(&lower_signal) {
                        score += 2.0;
                    }
                }
            }

            if score > 0.0 {
                candidates.push(GeneMatchCandidate {
                    gene: gene.clone(),
                    match_score: score,
                });
            }
        }

        debug!(
            "Stage 1 exact match: {} hits from {} genes",
            candidates.len(),
            self.genes.len()
        );
        candidates
    }

    /// Stage 2: Semantic embedding fallback via fastembed.
    ///
    /// Embeds the user message and compares with gene summary+signals
    /// embedding vectors via cosine similarity. Gene embeddings are
    /// lazily computed and cached.
    async fn stage2_semantic_match(&self, user_message: &str) -> Vec<GeneMatchCandidate> {
        let client = match &self.memu_client {
            Some(c) => c,
            None => return Vec::new(),
        };

        let preview = &user_message[..60.min(user_message.len())];
        debug!("Stage 2 semantic match: embedding query '{}...'", preview);

        // Embed user query
        let query_embedding = match client.embed_text(&[user_message]).await {
            Ok(mut vecs) if !vecs.is_empty() => vecs.remove(0),
            _ => {
                tracing::warn!("[GeneRetriever] Stage 2: failed to embed user query");
                return Vec::new();
            }
        };

        let mut candidates = Vec::new();

        for gene in &self.genes {
            if gene.status != GeneStatus::Active {
                continue;
            }

            // Build gene text for embedding: summary + signals
            let gene_text = format!("{} {}", gene.summary, gene.signals_match.join(" "));

            // Check cache first
            let gene_embedding = {
                let cache = self.gene_embeddings.lock().unwrap();
                cache.get(&gene.gene_id).cloned()
            };

            let gene_embedding = match gene_embedding {
                Some(emb) => emb,
                None => {
                    // Embed and cache
                    match client.embed_text(&[&gene_text]).await {
                        Ok(mut vecs) if !vecs.is_empty() => {
                            let emb = vecs.remove(0);
                            if let Ok(mut cache) = self.gene_embeddings.lock() {
                                cache.insert(gene.gene_id.clone(), emb.clone());
                            }
                            emb
                        }
                        _ => continue,
                    }
                }
            };

            let sim = cosine_sim(&query_embedding, &gene_embedding);

            // Only include matches above threshold (0.5 = moderate semantic similarity)
            if sim > 0.5 {
                candidates.push(GeneMatchCandidate {
                    gene: gene.clone(),
                    match_score: sim as f64 * 2.0, // scale similarity to [1.0, 2.0] range
                });
            }
        }

        debug!(
            "Stage 2 semantic match: {} hits from {} active genes",
            candidates.len(),
            self.genes.iter().filter(|g| g.status == GeneStatus::Active).count()
        );

        candidates
    }

    /// Set effective streaks for ranking, computed from Capsule history.
    pub fn set_streaks(&mut self, streaks: HashMap<String, f32>) {
        self.gene_effective_streaks = streaks;
    }

    /// Rank and deduplicate matches.
    ///
    /// Sorts by `match_score × log(effective_streak + 1)`,
    /// keeps at most one gene per category, and returns up to max_genes.
    fn rank_matches(
        &self,
        candidates: Vec<GeneMatchCandidate>,
        max_genes: usize,
    ) -> Vec<GeneMatch> {
        let mut scored: Vec<GeneMatch> = candidates
            .into_iter()
            .map(|c| {
                let effective_streak = self.gene_effective_streaks
                    .get(&c.gene.gene_id)
                    .copied()
                    .unwrap_or(1.0) as f64; // default 1.0 if no Capsule history
                let rank_score = c.match_score * (effective_streak + 1.0).ln();
                GeneMatch {
                    gene: c.gene,
                    match_score: c.match_score,
                    rank_score,
                }
            })
            .collect();

        // Sort by rank_score descending
        scored.sort_by(|a, b| b.rank_score.partial_cmp(&a.rank_score).unwrap());

        // Deduplicate: one gene per category
        let mut seen_categories = HashSet::new();
        let mut result = Vec::new();

        for gm in scored {
            if seen_categories.insert(gm.gene.category.clone()) {
                result.push(gm);
            }
            if result.len() >= max_genes {
                break;
            }
        }

        result
    }

    /// Reload genes from a repository (for updates after distillation).
    pub fn reload(&mut self, genes: Vec<Gene>) {
        self.genes = genes;
    }
}

/// Intermediate match candidate before ranking.
struct GeneMatchCandidate {
    gene: Gene,
    match_score: f64,
}

/// A matched Gene with its match and rank scores.
#[derive(Debug, Clone)]
pub struct GeneMatch {
    /// The matched gene
    pub gene: Gene,
    /// Raw match score from Stage 1 or 2
    pub match_score: f64,
    /// Final rank score (match_score × log(streak + 1))
    pub rank_score: f64,
}

impl GeneMatch {
    /// Format this match as a compact prompt for system prompt injection.
    pub fn to_compact_prompt(&self) -> String {
        self.gene.to_compact_prompt_with_streak(self.rank_score as f32)
    }
}

/// Format multiple GeneMatches into a system prompt block with conflict notices.
pub fn format_gene_injection(matches: &[GeneMatch], max_genes: usize) -> String {
    if matches.is_empty() {
        return String::new();
    }

    let mut output = String::from("\n\n<active_genes>\n");

    // Check for potential conflicts (genes in the same category)
    let category_counts: std::collections::HashMap<&GeneCategory, usize> = {
        let mut counts = std::collections::HashMap::new();
        for m in matches {
            *counts.entry(&m.gene.category).or_insert(0) += 1;
        }
        counts
    };

    let has_conflicts = category_counts.values().any(|&c| c > 1);

    if has_conflicts && matches.len() > 1 {
        output.push_str("⚠️ CONFLICT NOTICE: 以下 Gene 存在潜在矛盾，请根据当前场景抉择：\n");
    }

    for (i, gm) in matches.iter().enumerate() {
        output.push_str(&gm.to_compact_prompt());
        output.push('\n');
        if i < matches.len() - 1 {
            output.push('\n');
        }
    }

    if has_conflicts && matches.len() > 1 {
        output.push_str("→ RESOLUTION HINT: 根据用户消息判断优先级最高的场景，选择匹配度最高的 Gene\n");
    }

    output.push_str("</active_genes>\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_genes() -> Vec<Gene> {
        vec![
            Gene {
                gene_id: "stock-fallback".to_string(),
                version: "1.0".to_string(),
                category: GeneCategory::Repair,
                signals_match: vec!["403".to_string(), "api error".to_string()],
                summary: "跨源校验".to_string(),
                strategy: vec!["step1".to_string()],
                avoid: vec!["don't retry".to_string()],
                constraints: GeneConstraints { max_files: 3, forbidden_paths: vec![] },
                validation: "test".to_string(),
                asset_id: String::new(),
                status: GeneStatus::Active,
                created_at: 0,
                updated_at: 0,
            },
            Gene {
                gene_id: "batch-read".to_string(),
                version: "1.0".to_string(),
                category: GeneCategory::Optimize,
                signals_match: vec!["大量文件".to_string(), "grep".to_string()],
                summary: "批量读取".to_string(),
                strategy: vec!["step1".to_string()],
                avoid: vec![],
                constraints: GeneConstraints { max_files: 10, forbidden_paths: vec![] },
                validation: "test".to_string(),
                asset_id: String::new(),
                status: GeneStatus::Active,
                created_at: 0,
                updated_at: 0,
            },
            Gene {
                gene_id: "retired-gene".to_string(),
                version: "1.0".to_string(),
                category: GeneCategory::Innovate,
                signals_match: vec!["test".to_string()],
                summary: "retired".to_string(),
                strategy: vec!["step".to_string()],
                avoid: vec![],
                constraints: GeneConstraints { max_files: 1, forbidden_paths: vec![] },
                validation: "test".to_string(),
                asset_id: String::new(),
                status: GeneStatus::Retired,
                created_at: 0,
                updated_at: 0,
            },
        ]
    }

    #[tokio::test]
    async fn test_stage1_match_user_message() {
        let genes = make_test_genes();
        let retriever = GeneRetriever::new(genes, false, None);

        let matches = retriever.match_genes("遇到403错误", &[], 5).await;
        assert!(!matches.is_empty());
        assert_eq!(matches[0].gene.gene_id, "stock-fallback");
    }

    #[tokio::test]
    async fn test_stage1_match_tool_error() {
        let genes = make_test_genes();
        let retriever = GeneRetriever::new(genes, false, None);

        let matches = retriever.match_genes(
            "something else",
            &["grep command failed".to_string()],
            5,
        ).await;
        // grep should match batch-read
        assert!(matches.iter().any(|m| m.gene.gene_id == "batch-read"));
    }

    #[tokio::test]
    async fn test_no_match() {
        let genes = make_test_genes();
        let retriever = GeneRetriever::new(genes, false, None);

        let matches = retriever.match_genes("completely unrelated", &[], 5).await;
        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn test_retired_gene_excluded() {
        let genes = make_test_genes();
        let retriever = GeneRetriever::new(genes, false, None);

        let matches = retriever.match_genes("test something", &[], 5).await;
        // "test" matches retired-gene's signals_match, but retired genes are excluded
        assert!(matches.iter().all(|m| m.gene.gene_id != "retired-gene"));
    }

    #[tokio::test]
    async fn test_category_dedup() {
        // Two repair genes with overlapping signals
        let genes = vec![
            Gene {
                gene_id: "repair-a".to_string(),
                version: "1.0".to_string(),
                category: GeneCategory::Repair,
                signals_match: vec!["403".to_string()],
                summary: "a".to_string(),
                strategy: vec!["s".to_string()],
                avoid: vec![],
                constraints: GeneConstraints { max_files: 1, forbidden_paths: vec![] },
                validation: "v".to_string(),
                asset_id: String::new(),
                status: GeneStatus::Active,
                created_at: 0,
                updated_at: 0,
            },
            Gene {
                gene_id: "repair-b".to_string(),
                version: "1.0".to_string(),
                category: GeneCategory::Repair,
                signals_match: vec!["403".to_string(), "timeout".to_string()],
                summary: "b".to_string(),
                strategy: vec!["s".to_string()],
                avoid: vec![],
                constraints: GeneConstraints { max_files: 1, forbidden_paths: vec![] },
                validation: "v".to_string(),
                asset_id: String::new(),
                status: GeneStatus::Active,
                created_at: 0,
                updated_at: 0,
            },
        ];

        let retriever = GeneRetriever::new(genes, false, None);
        let matches = retriever.match_genes("403 error", &[], 5).await;
        // Should only return one repair gene (deduped by category)
        let repair_count = matches.iter().filter(|m| m.gene.category == GeneCategory::Repair).count();
        assert_eq!(repair_count, 1, "Should deduplicate by category");
    }

    #[test]
    fn test_format_gene_injection() {
        let matches = vec![GeneMatch {
            gene: Gene {
                gene_id: "test-gene".to_string(),
                version: "1.0".to_string(),
                category: GeneCategory::Repair,
                signals_match: vec!["403".to_string()],
                summary: "测试摘要".to_string(),
                strategy: vec!["步骤1".to_string(), "步骤2".to_string()],
                avoid: vec!["不要重试".to_string()],
                constraints: GeneConstraints { max_files: 3, forbidden_paths: vec![".env".to_string()] },
                validation: "test".to_string(),
                asset_id: String::new(),
                status: GeneStatus::Active,
                created_at: 0,
                updated_at: 0,
            },
            match_score: 1.0,
            rank_score: 0.5,
        }];

        let injection = format_gene_injection(&matches, 2);
        assert!(injection.contains("<active_genes>"));
        assert!(injection.contains("GENE[test-gene]"));
        assert!(injection.contains("AVOID:"));
        assert!(injection.contains("</active_genes>"));
    }
}
