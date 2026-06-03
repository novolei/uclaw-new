// SPDX-License-Identifier: Apache-2.0
//! MiniCPM model management: build the download manifest (GGUF from the GGUF
//! repo + tokenizer.json from the base repo), drive `model_fetch`, and manage
//! the on-disk cache (installed?/missing/list/delete) with cooperative cancel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};

use crate::model_fetch::manifest::{FileSource, Host, ManifestFile, ModelManifest};
use crate::model_fetch::Progress;

use super::{model_dir, MODEL_ID, TOKENIZER_FILENAME};

/// Supported quantizations. Q4_K_M is the default (688 MB).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Quant {
    #[serde(rename = "Q4_K_M")]
    Q4KM,
    #[serde(rename = "Q8_0")]
    Q8_0,
    #[serde(rename = "F16")]
    F16,
}

impl Quant {
    pub fn from_str_lenient(s: &str) -> Option<Quant> {
        match s.to_ascii_uppercase().replace('-', "_").as_str() {
            "Q4_K_M" | "Q4KM" => Some(Quant::Q4KM),
            "Q8_0" | "Q80" => Some(Quant::Q8_0),
            "F16" | "FP16" => Some(Quant::F16),
            _ => None,
        }
    }

    /// The GGUF filename for this quant (matches Slice B's GGUF_FILENAME for Q4KM).
    pub fn gguf_filename(&self) -> &'static str {
        match self {
            Quant::Q4KM => "MiniCPM5-1B-Q4_K_M.gguf",
            Quant::Q8_0 => "MiniCPM5-1B-Q8_0.gguf",
            Quant::F16 => "MiniCPM5-1B-F16.gguf",
        }
    }
}

const GGUF_REPO: &str = "openbmb/MiniCPM5-1B-GGUF";
const GGUF_REPO_MS: &str = "OpenBMB/MiniCPM5-1B-GGUF";
const BASE_REPO: &str = "openbmb/MiniCPM5-1B";
const BASE_REPO_MS: &str = "OpenBMB/MiniCPM5-1B";

/// Build candidate sources for one file across all three hosts, default order.
fn sources_for(hf_repo: &str, ms_repo: &str, path: &str) -> Vec<FileSource> {
    use crate::model_fetch::manifest::{hf_mirror_url, hf_url, modelscope_url};
    vec![
        FileSource { host: Host::HuggingFace, url: hf_url(hf_repo, "main", path) },
        FileSource { host: Host::HfMirror, url: hf_mirror_url(hf_repo, "main", path) },
        FileSource { host: Host::ModelScope, url: modelscope_url(ms_repo, "master", path) },
    ]
}

/// Build the MiniCPM manifest for a quant. GGUF from the GGUF repo;
/// `tokenizer.json` from the BASE repo (candle needs the external tokenizer).
pub fn minicpm_manifest(data_dir: &Path, quant: Quant) -> ModelManifest {
    let cache_dir = model_dir(data_dir);
    let gguf_name = quant.gguf_filename();
    ModelManifest {
        cache_dir,
        files: vec![
            ManifestFile {
                dest_name: gguf_name.to_string(),
                sources: sources_for(GGUF_REPO, GGUF_REPO_MS, gguf_name),
                expected_size: None,
                sha256: None,
            },
            ManifestFile {
                dest_name: TOKENIZER_FILENAME.to_string(),
                sources: sources_for(BASE_REPO, BASE_REPO_MS, TOKENIZER_FILENAME),
                expected_size: None,
                sha256: None,
            },
        ],
    }
}

/// Summary of one installed model on disk.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledModel {
    pub model_id: String,
    pub installed: bool,
    pub files: Vec<InstalledFile>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledFile {
    pub name: String,
    pub bytes: u64,
}

/// Process-wide registry of in-flight download cancel flags, keyed by model_id.
fn cancels() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static CANCELS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    CANCELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Lightweight cache manager; constructed per-command from the data dir.
pub struct ModelManager {
    data_dir: PathBuf,
}

impl ModelManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// True if the default-quant GGUF + tokenizer are present (delegates to
    /// Slice B's `is_present` so the readiness contract stays single-sourced).
    pub fn is_installed(&self) -> bool {
        super::is_present(&self.data_dir)
    }

    /// Files in the manifest that are missing on disk.
    #[allow(dead_code)] // used by local_model_* commands (Task 5)
    pub fn missing_files(&self, quant: Quant) -> Vec<String> {
        let m = minicpm_manifest(&self.data_dir, quant);
        let dir = model_dir(&self.data_dir);
        m.files
            .into_iter()
            .filter(|f| !dir.join(&f.dest_name).exists())
            .map(|f| f.dest_name)
            .collect()
    }

    /// List the installed MiniCPM model (single model in v1).
    pub fn list(&self) -> Vec<InstalledModel> {
        let dir = model_dir(&self.data_dir);
        let mut files = Vec::new();
        let mut total = 0u64;
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for entry in rd.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        let bytes = meta.len();
                        total += bytes;
                        files.push(InstalledFile {
                            name: entry.file_name().to_string_lossy().to_string(),
                            bytes,
                        });
                    }
                }
            }
        }
        vec![InstalledModel {
            model_id: MODEL_ID.to_string(),
            installed: super::is_present(&self.data_dir),
            files,
            total_bytes: total,
        }]
    }

    /// Delete the entire model cache dir. Idempotent.
    pub fn delete(&self) -> Result<(), String> {
        let dir = model_dir(&self.data_dir);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("delete {}: {e}", dir.display()))?;
        }
        Ok(())
    }

    /// Register + return a fresh cancel flag for `model_id`, replacing any prior.
    #[allow(dead_code)] // used by local_model_* commands (Task 5)
    pub fn begin(&self, model_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        cancels().lock().unwrap().insert(model_id.to_string(), flag.clone());
        flag
    }

    /// Signal cancellation for an in-flight download. Returns true if one existed.
    pub fn cancel(&self, model_id: &str) -> bool {
        if let Some(flag) = cancels().lock().unwrap().get(model_id) {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// Clear the cancel registration for `model_id` (call when a download ends).
    pub fn finish(&self, model_id: &str) {
        cancels().lock().unwrap().remove(model_id);
    }

    /// Drive a download for `quant`, ordering each file's sources by `host_order`
    /// (from a probe; pass empty for default order). Progress is forwarded.
    #[allow(dead_code)] // used by local_model_* commands (Task 5)
    pub async fn download(
        &self,
        quant: Quant,
        host_order: &[Host],
        cancel: Arc<AtomicBool>,
        on_progress: impl Fn(Progress) + Send + Sync,
    ) -> Result<(), String> {
        let mut manifest = minicpm_manifest(&self.data_dir, quant);
        if !host_order.is_empty() {
            for file in &mut manifest.files {
                file.sources.sort_by_key(|s| {
                    host_order.iter().position(|h| *h == s.host).unwrap_or(usize::MAX)
                });
            }
        }
        crate::model_fetch::download_manifest(&manifest, cancel, on_progress).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::GGUF_FILENAME;

    #[test]
    fn quant_parsing_lenient() {
        assert_eq!(Quant::from_str_lenient("q4_k_m"), Some(Quant::Q4KM));
        assert_eq!(Quant::from_str_lenient("Q4-K-M"), Some(Quant::Q4KM));
        assert_eq!(Quant::from_str_lenient("Q8_0"), Some(Quant::Q8_0));
        assert_eq!(Quant::from_str_lenient("fp16"), Some(Quant::F16));
        assert_eq!(Quant::from_str_lenient("garbage"), None);
    }

    #[test]
    fn default_quant_filename_matches_slice_b() {
        assert_eq!(Quant::Q4KM.gguf_filename(), GGUF_FILENAME);
    }

    #[test]
    fn manifest_has_gguf_and_tokenizer_from_correct_repos() {
        let m = minicpm_manifest(Path::new("/tmp/uclaw"), Quant::Q4KM);
        assert_eq!(m.files.len(), 2);
        let gguf = &m.files[0];
        assert_eq!(gguf.dest_name, "MiniCPM5-1B-Q4_K_M.gguf");
        assert!(gguf.sources.iter().any(|s| s.url.contains("MiniCPM5-1B-GGUF")));
        let tok = &m.files[1];
        assert_eq!(tok.dest_name, "tokenizer.json");
        assert!(tok.sources.iter().any(|s| s.url.contains("/openbmb/MiniCPM5-1B/resolve")
            || s.url.contains("/OpenBMB/MiniCPM5-1B/resolve")));
        assert_eq!(gguf.sources.len(), 3);
    }

    #[test]
    fn cache_path_under_data_dir() {
        let m = minicpm_manifest(Path::new("/tmp/uclaw"), Quant::Q4KM);
        assert!(m.cache_dir.ends_with("models/minicpm5-1b"));
    }

    #[test]
    fn list_and_delete_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ModelManager::new(tmp.path().to_path_buf());
        assert!(!mgr.is_installed());
        let dir = model_dir(tmp.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("MiniCPM5-1B-Q4_K_M.gguf"), b"x").unwrap();
        std::fs::write(dir.join("tokenizer.json"), b"yy").unwrap();
        assert!(mgr.is_installed());
        let listed = mgr.list();
        assert_eq!(listed[0].total_bytes, 3);
        assert!(listed[0].installed);
        mgr.delete().unwrap();
        assert!(!mgr.is_installed());
        mgr.delete().unwrap(); // idempotent
    }

    #[test]
    fn cancel_registry_roundtrip() {
        let mgr = ModelManager::new(PathBuf::from("/tmp/uclaw-cancel-test"));
        let id = "minicpm5-1b-canceltest";
        assert!(!mgr.cancel(id), "no flag yet");
        let flag = mgr.begin(id);
        assert!(!flag.load(std::sync::atomic::Ordering::SeqCst));
        assert!(mgr.cancel(id), "flag exists");
        assert!(flag.load(std::sync::atomic::Ordering::SeqCst), "cancel set the flag");
        mgr.finish(id);
        assert!(!mgr.cancel(id), "removed after finish");
    }
}
