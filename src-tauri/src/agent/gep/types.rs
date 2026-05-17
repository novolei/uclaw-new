//! GEP (Gene Evolution Protocol) core data types.
//!
//! Implements the Gene 六元组 (six-tuple), Capsule, EvolutionEvent, LearningCard,
//! and associated supporting types as defined by the GEP protocol v1.5.0.
//!
//! Reference: EvoMap/evolver (arXiv:2604.15097)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Gene ───────────────────────────────────────────────────────────────────

/// A strategy Gene — the core control signal (~230 tokens).
///
/// Gene six-tuple: g = (m, u, π, α, c, v)
/// - m: signals_match — trigger keywords/error patterns
/// - u: summary — one-line strategy description (≤60 chars)
/// - π: strategy — 3-4 executable steps
/// - α: avoid — failure-distilled "do NOT do" cues
/// - c: constraints — execution bounds
/// - v: validation — verification hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gene {
    /// Logical identifier (kebab-case, e.g. "gene_stock_cross_validation")
    pub gene_id: String,
    /// Semantic version of this gene
    pub version: String,
    /// Category of the gene
    pub category: GeneCategory,
    /// Comma-separated trigger keywords / error patterns
    pub signals_match: Vec<String>,
    /// One-line strategy summary (≤60 characters)
    pub summary: String,
    /// Executable strategy steps (≤4 steps)
    pub strategy: Vec<String>,
    /// Failure-distilled AVOID cues (≤5 after mutation, ≤3 initially)
    pub avoid: Vec<String>,
    /// Execution constraints
    pub constraints: GeneConstraints,
    /// Verification description
    pub validation: String,
    /// SHA-256 content hash for content-addressed storage
    pub asset_id: String,
    /// Lifecycle status
    #[serde(default = "default_gene_status")]
    pub status: GeneStatus,
    /// Unix timestamp of creation
    pub created_at: i64,
    /// Unix timestamp of last update
    pub updated_at: i64,
}

fn default_gene_status() -> GeneStatus {
    GeneStatus::Active
}

impl Gene {
    /// Compute the SHA-256 asset_id from the gene's content fields.
    ///
    /// The hash covers all semantic fields (gene_id, version, category,
    /// signals_match, summary, strategy, avoid, constraints, validation).
    /// Metadata fields (status, created_at, updated_at) are excluded so
    /// that lifecycle changes don't alter the identity.
    pub fn compute_asset_id(&self) -> String {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(self.gene_id.as_bytes());
        hasher.update(self.version.as_bytes());
        hasher.update(self.category.to_string().as_bytes());
        for s in &self.signals_match {
            hasher.update(s.as_bytes());
        }
        hasher.update(self.summary.as_bytes());
        for s in &self.strategy {
            hasher.update(s.as_bytes());
        }
        for a in &self.avoid {
            hasher.update(a.as_bytes());
        }
        hasher.update(self.constraints.max_files.to_le_bytes());
        for p in &self.constraints.forbidden_paths {
            hasher.update(p.as_bytes());
        }
        hasher.update(self.validation.as_bytes());

        format!("{:x}", hasher.finalize())
    }

    /// Format the Gene as a compact system-prompt injection block.
    ///
    /// Target: ≤80 tokens (~400 chars). This is injected into the agent's
    /// system prompt when the gene's signals_match patterns fire.
    pub fn to_compact_prompt(&self) -> String {
        format!(
            "GENE[{id}] ({category}, s:{streak}): {summary}\n  Strategy: {strategy}\n  AVOID: {avoid}\n  Constraints: max {max_files} files, no {forbidden_paths}",
            id = self.gene_id,
            category = self.category,
            streak = 0, // filled by caller with effective_streak
            summary = self.summary,
            strategy = self.strategy.join(" → "),
            avoid = self.avoid.join("; "),
            max_files = self.constraints.max_files,
            forbidden_paths = self.constraints.forbidden_paths.join(", "),
        )
    }

    /// Format the Gene as a compact prompt with actual streak value.
    pub fn to_compact_prompt_with_streak(&self, effective_streak: f32) -> String {
        format!(
            "GENE[{id}] ({category}, s:{streak:.1}): {summary}\n  Strategy: {strategy}\n  AVOID: {avoid}\n  Constraints: max {max_files} files, no {forbidden_paths}",
            id = self.gene_id,
            category = self.category,
            streak = effective_streak,
            summary = self.summary,
            strategy = self.strategy.join(" → "),
            avoid = self.avoid.join("; "),
            max_files = self.constraints.max_files,
            forbidden_paths = self.constraints.forbidden_paths.join(", "),
        )
    }
}

// ─── GeneCategory ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GeneCategory {
    /// Fixing broken things — error recovery, bug fixes, data validation
    #[serde(rename = "repair")]
    Repair,
    /// Making things better — performance, token efficiency, code quality
    #[serde(rename = "optimize")]
    Optimize,
    /// Exploring new territory — novel patterns, creative solutions
    #[serde(rename = "innovate")]
    Innovate,
}

impl std::fmt::Display for GeneCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeneCategory::Repair => write!(f, "repair"),
            GeneCategory::Optimize => write!(f, "optimize"),
            GeneCategory::Innovate => write!(f, "innovate"),
        }
    }
}

// ─── GeneConstraints ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneConstraints {
    /// Maximum number of files this gene is allowed to modify
    pub max_files: u32,
    /// Paths that must not be modified (e.g., ".env", "secrets/")
    #[serde(default)]
    pub forbidden_paths: Vec<String>,
}

// ─── GeneStatus ─────────────────────────────────────────────────────────────

/// Lifecycle status of a Gene.
///
/// State machine: Active → Stale → (Retired | Upgraded)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeneStatus {
    /// Gene is active and participates in retrieval
    #[serde(rename = "active")]
    Active,
    /// Gene has met retirement conditions but is not yet retired
    #[serde(rename = "stale")]
    Stale,
    /// Gene is retired — does not participate in retrieval, but history preserved
    #[serde(rename = "retired")]
    Retired,
    /// Gene has been superseded by a newer version
    #[serde(rename = "upgraded")]
    Upgraded,
}

// ─── Capsule ────────────────────────────────────────────────────────────────

/// A Capsule — execution result of applying a Gene.
///
/// Must be published as a bundle with its Gene. Each capsule records the
/// validated fix result, blast radius, confidence, and success streak.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capsule {
    /// Unique capsule identifier
    pub id: String,
    /// SHA-256 asset_id of the associated Gene
    pub gene_asset_id: String,
    /// The gene's logical id for human reference
    pub gene_id: String,
    /// Trigger signals that activated this gene
    pub trigger: Vec<String>,
    /// One-line description of the fix
    pub summary: String,
    /// Confidence score 0.0–1.0
    pub confidence: f32,
    /// Files and lines affected
    pub blast_radius: BlastRadius,
    /// Execution outcome
    pub outcome: CapsuleOutcome,
    /// Raw GEP-compatible consecutive success count
    pub raw_streak: u32,
    /// Weighted effective streak for ranking
    pub effective_streak: f32,
    /// Environment fingerprint at capsule creation
    pub env_fingerprint: EnvFingerprint,
    /// Unix timestamp
    pub created_at: i64,
    /// List of previous capsule IDs in this gene's lineage (for temporal scoring)
    #[serde(default)]
    pub lineage: Vec<String>,
}

impl Capsule {
    /// Compute the effective streak using the dual-track formula:
    ///
    /// effective = recency(0.5) × stability(0.3) × latest_score(0.2)
    ///
    /// - recency: exp(-days_since_last / 7)
    /// - stability: 1.0 - variance_of_last_5_scores
    /// - latest_score: the most recent capsule's score
    ///
    pub fn compute_effective_streak(
        &self,
        previous_capsules: &[Capsule],
        now_ts: i64,
    ) -> f32 {
        // Recency: exponential decay over 7 days
        let days_since_last = if let Some(last) = previous_capsules.first() {
            ((now_ts - last.created_at) as f64 / 86_400_000.0).max(0.0)
        } else {
            0.0
        };
        let recency = (-days_since_last / 7.0).exp();

        // Stability: 1.0 - variance of last 5 scores (or fewer if not enough)
        let mut scores: Vec<f64> = previous_capsules
            .iter()
            .take(5)
            .map(|c| c.outcome.score as f64)
            .collect();
        scores.push(self.outcome.score as f64);
        let stability: f64 = if scores.len() >= 2 {
            let mean = scores.iter().sum::<f64>() / scores.len() as f64;
            let variance =
                scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / scores.len() as f64;
            (1.0 - variance).max(0.0)
        } else {
            1.0 // single capsule = maximum stability
        };

        // Latest score: directly from this capsule
        let latest_score = self.outcome.score as f64;

        (0.5 * recency + 0.3 * stability + 0.2 * latest_score) as f32
    }
}

// ─── BlastRadius ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadius {
    /// Number of files modified
    pub files: u32,
    /// Total lines changed (insertions + deletions)
    pub lines: u32,
}

// ─── CapsuleOutcome ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsuleOutcome {
    /// Outcome status
    pub status: OutcomeStatus,
    /// Score from self_eval (0.0–1.0)
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutcomeStatus {
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "partial")]
    Partial,
    #[serde(rename = "failed")]
    Failed,
}

// ─── EnvFingerprint ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvFingerprint {
    /// Rust toolchain version (e.g., "1.80.0")
    pub rust_version: String,
    /// OS platform (e.g., "macos", "linux", "windows")
    pub platform: String,
    /// CPU architecture (e.g., "aarch64", "x86_64")
    pub arch: String,
}

impl Default for EnvFingerprint {
    fn default() -> Self {
        Self {
            rust_version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

// ─── EvolutionEvent ─────────────────────────────────────────────────────────

/// An EvolutionEvent records one complete cycle of gene application.
/// Forms the audit trail for the gene's evolutionary history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionEvent {
    /// Intent of the evolution cycle (matches GeneCategory)
    pub intent: String,
    /// Capsule ID produced in this cycle
    pub capsule_id: String,
    /// Gene asset_ids used in this cycle
    pub genes_used: Vec<String>,
    /// Number of candidate mutations tried
    pub mutations_tried: u32,
    /// Total evolution cycles for this gene
    pub total_cycles: u32,
    /// Unix timestamp
    pub created_at: i64,
}

// ─── LearningCard ───────────────────────────────────────────────────────────

/// Structured intermediate representation of a self_eval learning.
///
/// LearningCard transforms raw free-text learnings into pre-classified,
/// signal-annotated cards before they enter the gene candidate pool.
/// This improves LLM distillation quality by filtering noise upfront.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningCard {
    /// Original raw learning text (preserved for LLM context)
    pub raw: String,
    /// Pre-classification type
    pub card_type: LearningCardType,
    /// Extracted failure signal ("403", "timeout", "parse_error")
    #[serde(default)]
    pub failure_signal: Option<String>,
    /// Associated tool name
    #[serde(default)]
    pub tool_name: Option<String>,
    /// Structured strategy hint (condition × action × reason)
    pub strategy_hint: StrategyHint,
    /// Files touched in this session
    #[serde(default)]
    pub files_touched: Vec<String>,
    /// Session identifier
    pub session_id: String,
    /// self_eval score
    pub score: f32,
    /// Unix timestamp
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LearningCardType {
    /// Concrete failure lesson ("When Yahoo 403, switch to Alpha Vantage")
    #[serde(rename = "failure_lesson")]
    FailureLesson,
    /// Successful pattern ("Using search_codebase first is 3x faster")
    #[serde(rename = "success_pattern")]
    SuccessPattern,
    /// Optimization tip ("Batch reads save 40% tokens")
    #[serde(rename = "optimization_tip")]
    OptimizationTip,
    /// Noise to be filtered ("Should pay more attention")
    #[serde(rename = "noise")]
    Noise,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyHint {
    /// "When [condition] occurs"
    #[serde(default)]
    pub condition: Option<String>,
    /// "Do [action]"
    #[serde(default)]
    pub action: Option<String>,
    /// "Because [reason]"
    #[serde(default)]
    pub reason: Option<String>,
}

// ─── GeneRef ────────────────────────────────────────────────────────────────

/// Lightweight index node stored in MemoryGraph.
///
/// Points to the actual Gene JSON in the file system via asset_id.
/// Only stores fields needed for fast retrieval (avoid loading full Gene body).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneRef {
    /// Logical gene identifier
    pub gene_id: String,
    /// SHA-256 asset_id for content-addressed lookup
    pub asset_id: String,
    /// Trigger signals for substring matching
    pub signals_match: Vec<String>,
    /// Gene category
    pub category: GeneCategory,
    /// Latest effective streak
    pub effective_streak: f32,
    /// Latest capsule score
    pub score: f32,
    /// Lifecycle status
    pub status: GeneStatus,
    /// Current version
    pub version: String,
    /// Unix timestamp of last activity
    pub last_active_at: i64,
}

// ─── GeneCandidate ──────────────────────────────────────────────────────────

/// A candidate waiting to be distilled into a Gene.
///
/// Accumulated in the gene_candidate_pool within ProactiveService.
/// When the pool reaches the distillation threshold, GeneEvolutionScenario
/// is triggered.
#[derive(Debug, Clone)]
pub struct GeneCandidate {
    /// Source of the learning ("self_eval")
    pub source: String,
    /// Raw learning content
    pub content: String,
    /// Original LearningCard type preserved through the pool
    pub card_type: Option<LearningCardType>,
    /// self_eval score (if available)
    pub score: Option<f64>,
    /// Session that produced this candidate
    pub session_id: Option<String>,
    /// self_eval reasoning (if available)
    pub reasoning: Option<String>,
    /// Timestamp when the candidate was collected
    pub timestamp: DateTime<Utc>,
}

// ─── Gene Distillation Result ───────────────────────────────────────────────

/// Result of a gene distillation attempt.
#[derive(Debug, Clone)]
pub enum DistillationResult {
    /// Successfully distilled a new Gene
    NewGene {
        gene: Gene,
        capsule: Capsule,
        event: EvolutionEvent,
    },
    /// No distillable gene found
    NoGene,
    /// Duplicate of an existing gene
    Duplicate {
        existing_gene_id: String,
    },
    /// Distillation failed (LLM error, parse error, etc.)
    Failed {
        reason: String,
    },
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_gene() -> Gene {
        Gene {
            gene_id: "test_cross_validation".to_string(),
            version: "1.0".to_string(),
            category: GeneCategory::Repair,
            signals_match: vec!["403".to_string(), "API key invalid".to_string()],
            summary: "跨源校验数据，当主源失败时切换备用源".to_string(),
            strategy: vec![
                "识别失败的数据源API调用".to_string(),
                "从备用源列表中选择下一个可用源".to_string(),
                "用新源重新发起请求并验证schema".to_string(),
            ],
            avoid: vec![
                "不要在401时立即重试同一endpoint".to_string(),
            ],
            constraints: GeneConstraints {
                max_files: 3,
                forbidden_paths: vec![".env".to_string(), "secrets/".to_string()],
            },
            validation: "验证切换后数据schema一致".to_string(),
            asset_id: String::new(),
            status: GeneStatus::Active,
            created_at: 1715779200000,
            updated_at: 1715779200000,
        }
    }

    #[test]
    fn test_compute_asset_id_deterministic() {
        let mut gene = make_test_gene();
        let id1 = gene.compute_asset_id();
        let id2 = gene.compute_asset_id();
        assert_eq!(id1, id2, "asset_id must be deterministic");
        assert_eq!(id1.len(), 64, "SHA-256 produces 64 hex chars");
    }

    #[test]
    fn test_compute_asset_id_changes_with_content() {
        let mut gene1 = make_test_gene();
        let mut gene2 = make_test_gene();
        gene2.summary = "不同的摘要".to_string();

        let id1 = gene1.compute_asset_id();
        let id2 = gene2.compute_asset_id();
        assert_ne!(id1, id2, "different content must produce different asset_ids");
    }

    #[test]
    fn test_compute_asset_id_ignores_metadata() {
        let mut gene1 = make_test_gene();
        let mut gene2 = gene1.clone();
        gene2.created_at = 9999999999999;
        gene2.updated_at = 9999999999999;
        gene2.status = GeneStatus::Retired;

        let id1 = gene1.compute_asset_id();
        let id2 = gene2.compute_asset_id();
        assert_eq!(id1, id2, "metadata changes must not affect asset_id");
    }

    #[test]
    fn test_compact_prompt_token_budget() {
        let gene = make_test_gene();
        let prompt = gene.to_compact_prompt();
        // Rough token estimate: 1 token ≈ 4 chars for English
        let estimated_tokens = prompt.len() as f64 / 4.0;
        assert!(
            estimated_tokens <= 100.0,
            "compact prompt should be ≤100 tokens, got ~{:.0}: {}",
            estimated_tokens,
            prompt,
        );
    }

    #[test]
    fn test_effective_streak_all_success() {
        let capsule = Capsule {
            id: "cap_3".to_string(),
            gene_asset_id: "abc".to_string(),
            gene_id: "test".to_string(),
            trigger: vec!["403".to_string()],
            summary: "fix".to_string(),
            confidence: 0.9,
            blast_radius: BlastRadius { files: 1, lines: 10 },
            outcome: CapsuleOutcome { status: OutcomeStatus::Success, score: 0.95 },
            raw_streak: 3,
            effective_streak: 0.0,
            env_fingerprint: EnvFingerprint::default(),
            created_at: 1715779200000,
            lineage: vec![],
        };

        let prev = vec![
            Capsule {
                id: "cap_2".to_string(),
                gene_asset_id: "abc".to_string(),
                gene_id: "test".to_string(),
                trigger: vec!["403".to_string()],
                summary: "fix".to_string(),
                confidence: 0.9,
                blast_radius: BlastRadius { files: 1, lines: 8 },
                outcome: CapsuleOutcome { status: OutcomeStatus::Success, score: 0.90 },
                raw_streak: 2,
                effective_streak: 0.0,
                env_fingerprint: EnvFingerprint::default(),
                created_at: 1715779100000,
                lineage: vec![],
            },
            Capsule {
                id: "cap_1".to_string(),
                gene_asset_id: "abc".to_string(),
                gene_id: "test".to_string(),
                trigger: vec!["403".to_string()],
                summary: "fix".to_string(),
                confidence: 0.9,
                blast_radius: BlastRadius { files: 1, lines: 5 },
                outcome: CapsuleOutcome { status: OutcomeStatus::Success, score: 0.88 },
                raw_streak: 1,
                effective_streak: 0.0,
                env_fingerprint: EnvFingerprint::default(),
                created_at: 1715779000000,
                lineage: vec![],
            },
        ];

        let streak = capsule.compute_effective_streak(&prev, 1715779300000);
        assert!(streak > 0.0, "effective streak should be positive");
        assert!(streak <= 1.0, "effective streak should be ≤1.0");
        // With similar high scores and recent activity, streak should be high
        println!("effective_streak (all success): {:.3}", streak);
    }

    #[test]
    fn test_effective_streak_with_failures() {
        let capsule = Capsule {
            id: "cap_3".to_string(),
            gene_asset_id: "abc".to_string(),
            gene_id: "test".to_string(),
            trigger: vec!["403".to_string()],
            summary: "fix".to_string(),
            confidence: 0.3,
            blast_radius: BlastRadius { files: 5, lines: 200 },
            outcome: CapsuleOutcome { status: OutcomeStatus::Partial, score: 0.4 },
            raw_streak: 3,
            effective_streak: 0.0,
            env_fingerprint: EnvFingerprint::default(),
            created_at: 1715779200000,
            lineage: vec![],
        };

        let prev = vec![
            Capsule {
                id: "cap_2".to_string(),
                gene_asset_id: "abc".to_string(),
                gene_id: "test".to_string(),
                trigger: vec!["403".to_string()],
                summary: "fix".to_string(),
                confidence: 0.2,
                blast_radius: BlastRadius { files: 10, lines: 500 },
                outcome: CapsuleOutcome { status: OutcomeStatus::Failed, score: 0.2 },
                raw_streak: 2,
                effective_streak: 0.0,
                env_fingerprint: EnvFingerprint::default(),
                created_at: 1715779100000,
                lineage: vec![],
            },
            Capsule {
                id: "cap_1".to_string(),
                gene_asset_id: "abc".to_string(),
                gene_id: "test".to_string(),
                trigger: vec!["403".to_string()],
                summary: "fix".to_string(),
                confidence: 0.8,
                blast_radius: BlastRadius { files: 2, lines: 15 },
                outcome: CapsuleOutcome { status: OutcomeStatus::Success, score: 0.85 },
                raw_streak: 1,
                effective_streak: 0.0,
                env_fingerprint: EnvFingerprint::default(),
                created_at: 1715779000000,
                lineage: vec![],
            },
        ];

        let streak = capsule.compute_effective_streak(&prev, 1715779300000);
        // With mixed scores and recency, should be lower than all-success case
        assert!(streak < 1.0);
        println!("effective_streak (with failures): {:.3}", streak);
    }

    #[test]
    fn test_gene_category_display() {
        assert_eq!(GeneCategory::Repair.to_string(), "repair");
        assert_eq!(GeneCategory::Optimize.to_string(), "optimize");
        assert_eq!(GeneCategory::Innovate.to_string(), "innovate");
    }

    #[test]
    fn test_env_fingerprint_default() {
        let fp = EnvFingerprint::default();
        assert!(!fp.rust_version.is_empty());
        assert!(!fp.platform.is_empty());
        assert!(!fp.arch.is_empty());
    }
}
