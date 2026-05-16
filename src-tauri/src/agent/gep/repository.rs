//! GEP Gene Repository — content-addressed file-system storage.
//!
//! Genes, Capsules, and EvolutionEvents are stored in .uclaw/gep/ using
//! SHA-256 content addressing. MemoryGraph holds only lightweight GeneRef
//! index nodes for fast retrieval.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use super::types::*;

/// Gene Repository handles all file-system I/O for GEP entities.
pub struct GeneRepository {
    base_path: PathBuf,
}

impl GeneRepository {
    /// Create a new GeneRepository rooted at `base_path`.
    /// Ensures the directory structure exists.
    pub fn new(base_path: PathBuf) -> Result<Self> {
        let repo = Self { base_path };
        repo.ensure_dirs()?;
        debug!("GeneRepository initialized at {:?}", repo.base_path);
        Ok(repo)
    }

    /// Create the subdirectory structure if missing.
    fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(self.genes_dir())?;
        std::fs::create_dir_all(self.capsules_dir())?;
        std::fs::create_dir_all(self.events_dir())?;
        Ok(())
    }

    fn genes_dir(&self) -> PathBuf {
        self.base_path.join("genes")
    }
    fn capsules_dir(&self) -> PathBuf {
        self.base_path.join("capsules")
    }
    fn events_dir(&self) -> PathBuf {
        self.base_path.join("events")
    }

    // ─── Gene CRUD ──────────────────────────────────────────────────────

    /// Compute the file path for a Gene by its asset_id.
    /// Uses the first 2 hex chars as a sharding prefix.
    pub fn gene_path(&self, asset_id: &str) -> PathBuf {
        let prefix = &asset_id[..2.min(asset_id.len())];
        self.genes_dir().join(prefix).join(format!("{}.json", asset_id))
    }

    /// Store a Gene to disk.
    /// Automatically computes asset_id before writing.
    pub fn store_gene(&mut self, gene: &mut Gene) -> Result<()> {
        gene.asset_id = gene.compute_asset_id();
        let path = self.gene_path(&gene.asset_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(gene)?;
        std::fs::write(&path, &json)?;
        info!("Gene stored: {} at {}", gene.gene_id, path.display());
        Ok(())
    }

    /// Load a Gene from disk by asset_id.
    pub fn load_gene(&self, asset_id: &str) -> Result<Gene> {
        let path = self.gene_path(asset_id);
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("Gene not found: {}", asset_id))?;
        let gene: Gene = serde_json::from_str(&json)?;
        Ok(gene)
    }

    /// List all active genes.
    pub fn list_active_genes(&self) -> Result<Vec<Gene>> {
        let mut genes = self.list_all_genes()?;
        genes.retain(|g| g.status == GeneStatus::Active);
        Ok(genes)
    }

    /// List all genes (active + retired + upgraded).
    pub fn list_all_genes(&self) -> Result<Vec<Gene>> {
        let mut genes = Vec::new();
        self.scan_json_files(&self.genes_dir(), &mut genes)?;
        Ok(genes)
    }

    /// Update the lifecycle status of a Gene.
    pub fn update_gene_status(
        &mut self,
        asset_id: &str,
        status: GeneStatus,
    ) -> Result<()> {
        let mut gene = self.load_gene(asset_id)?;
        gene.status = status;
        gene.updated_at = chrono::Utc::now().timestamp_millis();
        let path = self.gene_path(asset_id);
        let json = serde_json::to_string_pretty(&gene)?;
        std::fs::write(&path, &json)?;
        info!("Gene {} status updated to {:?}", gene.gene_id, status);
        Ok(())
    }

    /// Retire a gene with a reason annotation.
    pub fn retire_gene(&mut self, asset_id: &str, reason: &str) -> Result<()> {
        warn!("Retiring gene {}: {}", asset_id, reason);
        self.update_gene_status(asset_id, GeneStatus::Retired)
    }

    // ─── Capsule CRUD ───────────────────────────────────────────────────

    /// Store a Capsule to disk.
    pub fn store_capsule(&self, capsule: &Capsule) -> Result<()> {
        let dir = self.capsules_dir().join(&capsule.gene_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("capsule_{}.json", capsule.id));
        let json = serde_json::to_string_pretty(capsule)?;
        std::fs::write(&path, &json)?;
        debug!("Capsule stored: {} at {}", capsule.id, path.display());
        Ok(())
    }

    /// List all Capsules for a specific Gene.
    pub fn list_capsules(&self, gene_id: &str) -> Result<Vec<Capsule>> {
        let dir = self.capsules_dir().join(gene_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut capsules = Vec::new();
        self.scan_json_files(&dir, &mut capsules)?;
        // Sort by created_at descending (newest first)
        capsules.sort_by(|a: &Capsule, b: &Capsule| b.created_at.cmp(&a.created_at));
        Ok(capsules)
    }

    // ─── EvolutionEvent CRUD ────────────────────────────────────────────

    /// Store an EvolutionEvent to disk.
    pub fn store_event(&self, event: &EvolutionEvent) -> Result<()> {
        let dir = self.events_dir();
        std::fs::create_dir_all(&dir)?;
        let filename = format!(
            "event_{}_{}.json",
            event.created_at,
            &event.capsule_id[..8.min(event.capsule_id.len())]
        );
        let path = dir.join(filename);
        let json = serde_json::to_string_pretty(event)?;
        std::fs::write(&path, &json)?;
        debug!("EvolutionEvent stored at {}", path.display());
        Ok(())
    }

    // ─── Helpers ────────────────────────────────────────────────────────

    /// Recursively scan a directory for .json files and deserialize them.
    fn scan_json_files<T: serde::de::DeserializeOwned>(
        &self,
        dir: &Path,
        out: &mut Vec<T>,
    ) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                self.scan_json_files(&path, out)?;
            } else if path.extension().map_or(false, |ext| ext == "json") {
                match std::fs::read_to_string(&path) {
                    Ok(json) => {
                        if let Ok(item) = serde_json::from_str::<T>(&json) {
                            out.push(item);
                        } else {
                            warn!("Failed to deserialize: {}", path.display());
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read {}: {}", path.display(), e);
                    }
                }
            }
        }
        Ok(())
    }
}
