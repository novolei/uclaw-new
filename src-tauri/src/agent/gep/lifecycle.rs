//! Gene Lifecycle Manager — monitors gene health and applies lifecycle changes.
//!
//! State machine: Active → Stale → (Retired | Upgraded)
//!
//! Also handles Stage 1 mutation: AVOID cues augmentation when failure patterns emerge.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use tracing::{info, warn};

use super::repository::GeneRepository;
use super::types::*;

/// Manages the lifecycle of Genes: retirement, upgrading, and Stage 1 mutation.
pub struct GeneLifecycleManager {
    /// Base path for GEP storage
    base_path: PathBuf,
    /// Timestamp of the last mutation check per gene_id
    last_mutation_check: HashMap<String, i64>,
}

impl GeneLifecycleManager {
    /// Create a new lifecycle manager.
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            last_mutation_check: HashMap::new(),
        }
    }

    /// Check all active genes for lifecycle transitions.
    ///
    /// This should be called periodically by ProactiveService's tick_loop.
    /// Returns a summary of actions taken.
    pub fn check_all_genes(
        &mut self,
        repo: &GeneRepository,
        config: &crate::memubot_config::GeneEvolutionConfig,
    ) -> Result<LifecycleReport> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut report = LifecycleReport::default();

        let active_genes = match repo.list_active_genes() {
            Ok(genes) => genes,
            Err(e) => {
                warn!("Failed to list active genes: {}", e);
                return Ok(report);
            }
        };

        for gene in &active_genes {
            let capsules = repo.list_capsules(&gene.gene_id).unwrap_or_default();

            // ─── Check retirement conditions ───────────────────────
            self.check_retirement(repo, gene, &capsules, config, &mut report, now)?;

            // ─── Check Stage 1 mutation (AVOID augmentation) ──────
            self.check_avoid_augmentation(repo, gene, &capsules, config, &mut report, now)?;
        }

        Ok(report)
    }

    /// Check if a gene should be marked stale or retired.
    fn check_retirement(
        &self,
        repo: &GeneRepository,
        gene: &Gene,
        capsules: &[Capsule],
        config: &crate::memubot_config::GeneEvolutionConfig,
        report: &mut LifecycleReport,
        now: i64,
    ) -> Result<()> {
        // Check 1: Consecutive failures
        let recent_failures: Vec<&Capsule> = capsules
            .iter()
            .filter(|c| c.outcome.status == OutcomeStatus::Failed)
            .take(config.gene_retire_consecutive_failures as usize)
            .collect();

        if recent_failures.len() >= config.gene_retire_consecutive_failures as usize {
            warn!(
                "Gene {} has {} consecutive failures → marking stale",
                gene.gene_id,
                recent_failures.len()
            );
            let mut repo_mut = GeneRepository::new(self.base_path.clone())?;
            repo_mut.update_gene_status(&gene.asset_id, GeneStatus::Stale)?;
            report.stale_count += 1;
            return Ok(());
        }

        // Check 2: Long inactivity
        let last_activity = capsules
            .first()
            .map(|c| c.created_at)
            .unwrap_or(gene.created_at);
        let days_inactive = (now - last_activity) / 86_400_000;
        if days_inactive > config.gene_retire_inactive_days as i64 {
            warn!(
                "Gene {} inactive for {} days → marking stale",
                gene.gene_id, days_inactive
            );
            let mut repo_mut = GeneRepository::new(self.base_path.clone())?;
            repo_mut.update_gene_status(&gene.asset_id, GeneStatus::Stale)?;
            report.stale_count += 1;
            return Ok(());
        }

        // Check 3: Environment fingerprint mismatch (simplified)
        let current_env = EnvFingerprint::default();
        if let Some(last_capsule) = capsules.first() {
            if last_capsule.env_fingerprint.platform != current_env.platform
                || last_capsule.env_fingerprint.arch != current_env.arch
            {
                warn!(
                    "Gene {} env mismatch ({} vs {}) → marking stale",
                    gene.gene_id, last_capsule.env_fingerprint.platform, current_env.platform
                );
                let mut repo_mut = GeneRepository::new(self.base_path.clone())?;
                repo_mut.update_gene_status(&gene.asset_id, GeneStatus::Stale)?;
                report.stale_count += 1;
            }
        }

        Ok(())
    }

    /// Stage 1 mutation: Augment AVOID cues when new failure patterns emerge.
    fn check_avoid_augmentation(
        &mut self,
        repo: &GeneRepository,
        gene: &Gene,
        capsules: &[Capsule],
        config: &crate::memubot_config::GeneEvolutionConfig,
        report: &mut LifecycleReport,
        now: i64,
    ) -> Result<()> {
        // Check mutation cooldown
        if let Some(last_check) = self.last_mutation_check.get(&gene.gene_id) {
            let elapsed_secs = (now - last_check) / 1000;
            if elapsed_secs < config.gene_mutation_cooldown_secs as i64 {
                return Ok(()); // still in cooldown
            }
        }

        // Count failed/partial capsules
        let failure_capsules: Vec<&Capsule> = capsules
            .iter()
            .filter(|c| {
                c.outcome.status == OutcomeStatus::Failed
                    || c.outcome.status == OutcomeStatus::Partial
            })
            .collect();

        if failure_capsules.len() < config.gene_avoid_augment_min_failures as usize {
            return Ok(());
        }

        // Extract new failure patterns from capsule summaries
        // (simplified: just adds any unique failure summary as a new AVOID cue)
        let existing_avoids: std::collections::HashSet<&str> =
            gene.avoid.iter().map(|s| s.as_str()).collect();

        let mut new_avoids = Vec::new();
        for capsule in failure_capsules.iter().take(3) {
            let cue = format!("不要 {}", capsule.summary.trim());
            if !existing_avoids.contains(cue.as_str())
                && gene.avoid.len() + new_avoids.len() < config.gene_max_avoid_cues
            {
                new_avoids.push(cue);
            }
        }

        if !new_avoids.is_empty() {
            info!(
                "Gene {} AVOID augmentation: adding {} new cues",
                gene.gene_id,
                new_avoids.len()
            );

            // Load gene, append avoids, save
            let mut repo_mut = GeneRepository::new(self.base_path.clone())?;
            let mut updated_gene = repo_mut.load_gene(&gene.asset_id)?;
            updated_gene.avoid.extend(new_avoids);
            updated_gene.avoid.truncate(config.gene_max_avoid_cues);
            updated_gene.updated_at = now;
            // Bump minor version for AVOID augmentation
            updated_gene.version = bump_minor(&updated_gene.version);
            repo_mut.store_gene(&mut updated_gene)?;

            report.mutations_performed += 1;
            self.last_mutation_check.insert(gene.gene_id.clone(), now);
        }

        Ok(())
    }
}

/// Bump the minor version: "1.0" → "1.1", "2.3" → "2.4"
fn bump_minor(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() == 2 {
        if let Ok(minor) = parts[1].parse::<u32>() {
            return format!("{}.{}", parts[0], minor + 1);
        }
    }
    format!("{}.1", version)
}

/// Summary report of lifecycle actions taken.
#[derive(Debug, Default)]
pub struct LifecycleReport {
    /// Number of genes marked stale
    pub stale_count: u32,
    /// Number of genes retired
    pub retired_count: u32,
    /// Number of AVOID augmentations performed (Stage 1 mutation)
    pub mutations_performed: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bump_minor() {
        assert_eq!(bump_minor("1.0"), "1.1");
        assert_eq!(bump_minor("2.3"), "2.4");
        assert_eq!(bump_minor("10.9"), "10.10");
        assert_eq!(bump_minor("unknown"), "unknown.1");
    }
}
