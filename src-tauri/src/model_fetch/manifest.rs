// SPDX-License-Identifier: Apache-2.0
//! Manifest data model + per-host URL construction (pure — no network/IO).

use std::path::PathBuf;

/// A download host, used to rank candidate URLs by probe latency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Host {
    HuggingFace,
    HfMirror,
    ModelScope,
}

impl Host {
    pub fn id(&self) -> &'static str {
        match self {
            Host::HuggingFace => "huggingface",
            Host::HfMirror => "hf-mirror",
            Host::ModelScope => "modelscope",
        }
    }
}

/// One fully-resolved candidate URL for a file, tagged with its host.
#[derive(Debug, Clone)]
pub struct FileSource {
    pub host: Host,
    pub url: String,
}

/// One file to fetch: destination filename + ordered candidate sources +
/// optional integrity checks.
#[derive(Debug, Clone)]
pub struct ManifestFile {
    pub dest_name: String,
    pub sources: Vec<FileSource>,
    pub expected_size: Option<u64>,
    pub sha256: Option<String>,
}

/// A complete model: where to cache it + the files that compose it.
#[derive(Debug, Clone)]
pub struct ModelManifest {
    pub cache_dir: PathBuf,
    pub files: Vec<ManifestFile>,
}

/// HuggingFace resolve URL: `https://huggingface.co/{repo}/resolve/{revision}/{path}`.
pub fn hf_url(repo: &str, revision: &str, path: &str) -> String {
    format!("https://huggingface.co/{repo}/resolve/{revision}/{path}")
}

/// hf-mirror.com resolve URL (same path shape as HF).
pub fn hf_mirror_url(repo: &str, revision: &str, path: &str) -> String {
    format!("https://hf-mirror.com/{repo}/resolve/{revision}/{path}")
}

/// ModelScope resolve URL: `https://modelscope.cn/models/{repo}/resolve/{revision}/{path}`.
pub fn modelscope_url(repo: &str, revision: &str, path: &str) -> String {
    format!("https://modelscope.cn/models/{repo}/resolve/{revision}/{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hf_url_shape() {
        assert_eq!(
            hf_url("openbmb/MiniCPM5-1B-GGUF", "main", "MiniCPM5-1B-Q4_K_M.gguf"),
            "https://huggingface.co/openbmb/MiniCPM5-1B-GGUF/resolve/main/MiniCPM5-1B-Q4_K_M.gguf"
        );
    }

    #[test]
    fn hf_mirror_url_shape() {
        assert_eq!(
            hf_mirror_url("openbmb/MiniCPM5-1B", "main", "tokenizer.json"),
            "https://hf-mirror.com/openbmb/MiniCPM5-1B/resolve/main/tokenizer.json"
        );
    }

    #[test]
    fn modelscope_url_shape() {
        assert_eq!(
            modelscope_url("OpenBMB/MiniCPM5-1B-GGUF", "master", "MiniCPM5-1B-Q4_K_M.gguf"),
            "https://modelscope.cn/models/OpenBMB/MiniCPM5-1B-GGUF/resolve/master/MiniCPM5-1B-Q4_K_M.gguf"
        );
    }

    #[test]
    fn host_ids_are_stable() {
        assert_eq!(Host::HuggingFace.id(), "huggingface");
        assert_eq!(Host::HfMirror.id(), "hf-mirror");
        assert_eq!(Host::ModelScope.id(), "modelscope");
    }
}
