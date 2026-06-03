// SPDX-License-Identifier: Apache-2.0
//! Streaming manifest downloader: per-file source fallback, progress callback,
//! cooperative cancellation, atomic tmp→final rename, size + optional sha256
//! verification, and a single retry on verification failure.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::manifest::{Host, ManifestFile, ModelManifest};

/// Temp path for an in-progress download: append `.tmp` to the full filename
/// (NOT `with_extension`, which would replace the extension and let two files
/// sharing a stem collide on the same tmp path).
fn tmp_path(dest: &Path) -> std::path::PathBuf {
    let name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| format!("{n}.tmp"))
        .unwrap_or_else(|| "download.tmp".to_string());
    match dest.parent() {
        Some(p) => p.join(name),
        None => std::path::PathBuf::from(name),
    }
}

/// Progress tick for one file.
#[derive(Debug, Clone)]
pub struct Progress {
    pub file: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub host: Host,
    pub phase: &'static str, // "downloading" | "verifying" | "done"
}

/// Events the caller may surface (reserved for richer events without churn).
#[allow(dead_code)] // reserved for richer events (Slice D)
#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Progress(Progress),
}

/// Download every file in the manifest that isn't already present+valid.
/// `cancel` is polled cooperatively; setting it aborts with an error.
pub async fn download_manifest(
    manifest: &ModelManifest,
    cancel: Arc<AtomicBool>,
    on_progress: impl Fn(Progress) + Send + Sync,
) -> Result<(), String> {
    tokio::fs::create_dir_all(&manifest.cache_dir)
        .await
        .map_err(|e| format!("create cache dir {}: {e}", manifest.cache_dir.display()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1800))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("uclaw-backend/model-download")
        .build()
        .map_err(|e| format!("reqwest client build: {e}"))?;

    for file in &manifest.files {
        let dest = manifest.cache_dir.join(&file.dest_name);
        if file_is_valid(&dest, file).await {
            tracing::info!(file = %file.dest_name, "model file already present + valid, skipping");
            continue;
        }
        let mut last_err = String::new();
        let mut ok = false;
        for attempt in 0..2 {
            if cancel.load(Ordering::SeqCst) {
                return Err("download cancelled".to_string());
            }
            match download_file(&client, file, &dest, &cancel, &on_progress).await {
                Ok(()) => {
                    ok = true;
                    break;
                }
                Err(e) => {
                    last_err = e;
                    tracing::warn!(file = %file.dest_name, attempt, error = %last_err, "download attempt failed");
                    let _ = tokio::fs::remove_file(tmp_path(&dest)).await;
                }
            }
        }
        if !ok {
            return Err(format!("failed to download {}: {last_err}", file.dest_name));
        }
    }
    Ok(())
}

/// True if `dest` exists and matches expected_size (and sha256 if present).
async fn file_is_valid(dest: &Path, file: &ManifestFile) -> bool {
    let Ok(meta) = tokio::fs::metadata(dest).await else {
        return false;
    };
    if let Some(expected) = file.expected_size {
        if meta.len() != expected {
            return false;
        }
    }
    if let Some(want) = &file.sha256 {
        match sha256_file(dest).await {
            Ok(got) => got.eq_ignore_ascii_case(want),
            Err(_) => false,
        }
    } else {
        true
    }
}

/// Try each source in order until one succeeds, verifying after write.
async fn download_file(
    client: &reqwest::Client,
    file: &ManifestFile,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
    on_progress: &(impl Fn(Progress) + Send + Sync),
) -> Result<(), String> {
    let mut errors = Vec::new();
    for src in &file.sources {
        if cancel.load(Ordering::SeqCst) {
            return Err("download cancelled".to_string());
        }
        match stream_to_tmp(client, &src.url, src.host, file, dest, cancel, on_progress).await {
            Ok(()) => {
                let tmp = tmp_path(dest);
                if let Some(expected) = file.expected_size {
                    let len = tokio::fs::metadata(&tmp).await.map(|m| m.len()).unwrap_or(0);
                    if len != expected {
                        errors.push(format!("{} (size {len} != {expected})", src.url));
                        let _ = tokio::fs::remove_file(&tmp).await;
                        continue;
                    }
                }
                if let Some(want) = &file.sha256 {
                    on_progress(Progress {
                        file: file.dest_name.clone(),
                        downloaded: 0,
                        total: None,
                        host: src.host,
                        phase: "verifying",
                    });
                    match sha256_file(&tmp).await {
                        Ok(got) if got.eq_ignore_ascii_case(want) => {}
                        Ok(got) => {
                            errors.push(format!("{} (sha256 {got} != {want})", src.url));
                            let _ = tokio::fs::remove_file(&tmp).await;
                            continue;
                        }
                        Err(e) => {
                            errors.push(format!("{} (sha256 read: {e})", src.url));
                            let _ = tokio::fs::remove_file(&tmp).await;
                            continue;
                        }
                    }
                }
                tokio::fs::rename(&tmp, dest)
                    .await
                    .map_err(|e| format!("rename {} → {}: {e}", tmp.display(), dest.display()))?;
                on_progress(Progress {
                    file: file.dest_name.clone(),
                    downloaded: file.expected_size.unwrap_or(0),
                    total: file.expected_size,
                    host: src.host,
                    phase: "done",
                });
                return Ok(());
            }
            Err(e) => {
                errors.push(format!("{} ({e})", src.url));
                let _ = tokio::fs::remove_file(tmp_path(dest)).await;
                tracing::warn!(url = %src.url, error = %e, "source failed, trying next");
            }
        }
    }
    Err(format!("all sources failed:\n - {}", errors.join("\n - ")))
}

/// Stream one URL to `<dest>.tmp`, calling `on_progress` per chunk and aborting
/// promptly if `cancel` is set.
async fn stream_to_tmp(
    client: &reqwest::Client,
    url: &str,
    host: Host,
    file: &ManifestFile,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
    on_progress: &(impl Fn(Progress) + Send + Sync),
) -> Result<(), String> {
    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("request: {e}"))?
        .error_for_status()
        .map_err(|e| format!("status: {e}"))?;

    let total = resp.content_length().or(file.expected_size);
    let tmp = tmp_path(dest);
    let mut out = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| format!("create tmp: {e}"))?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("read stream: {e}"))? {
        if cancel.load(Ordering::SeqCst) {
            let _ = out.flush().await;
            drop(out);
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err("cancelled".to_string());
        }
        out.write_all(&chunk).await.map_err(|e| format!("write: {e}"))?;
        downloaded += chunk.len() as u64;
        on_progress(Progress {
            file: file.dest_name.clone(),
            downloaded,
            total,
            host,
            phase: "downloading",
        });
    }
    out.flush().await.map_err(|e| format!("flush: {e}"))?;
    Ok(())
}

/// Streaming sha256 of a file (avoids loading 688 MB into RAM).
async fn sha256_file(path: &Path) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let mut f = tokio::fs::File::open(path).await.map_err(|e| format!("open: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).await.map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_fetch::manifest::{FileSource, ModelManifest};

    fn manifest_with(dir: std::path::PathBuf, file: ManifestFile) -> ModelManifest {
        ModelManifest { cache_dir: dir, files: vec![file] }
    }

    #[tokio::test]
    async fn present_valid_file_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.bin"), b"hello").unwrap();
        let file = ManifestFile {
            dest_name: "a.bin".into(),
            sources: vec![FileSource { host: Host::HuggingFace, url: "http://127.0.0.1:1/nope".into() }],
            expected_size: Some(5),
            sha256: None,
        };
        let m = manifest_with(tmp.path().to_path_buf(), file);
        let cancel = Arc::new(AtomicBool::new(false));
        let res = download_manifest(&m, cancel, |_| {}).await;
        assert!(res.is_ok(), "valid present file should skip, got {res:?}");
    }

    #[tokio::test]
    async fn pre_cancelled_aborts() {
        let tmp = tempfile::tempdir().unwrap();
        let file = ManifestFile {
            dest_name: "b.bin".into(),
            sources: vec![FileSource { host: Host::HuggingFace, url: "http://127.0.0.1:1/nope".into() }],
            expected_size: Some(10),
            sha256: None,
        };
        let m = manifest_with(tmp.path().to_path_buf(), file);
        let cancel = Arc::new(AtomicBool::new(true));
        let res = download_manifest(&m, cancel, |_| {}).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("cancel"));
    }

    #[tokio::test]
    async fn sha256_of_known_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("x");
        std::fs::write(&p, b"abc").unwrap();
        let got = sha256_file(&p).await.unwrap();
        assert_eq!(got, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
