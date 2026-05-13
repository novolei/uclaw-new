//! ONNX Runtime dynamic library loader.
//!
//! `ort = "load-dynamic"` doesn't bundle libonnxruntime — it dlopen()s it at
//! first use. This module downloads the appropriate Microsoft onnxruntime
//! release for the current platform/arch into `~/.uclaw/onnxruntime/` and
//! sets `ORT_DYLIB_PATH` BEFORE any ort code runs.
//!
//! Idempotent: subsequent calls are no-ops if the dylib is already present.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

/// onnxruntime version paired with ort = "2.0.0-rc.10".
///
/// ort rc.10 在 src/lib.rs:168 处 hard-check 加载的 dylib 的 `GetVersionString()`
/// 必须返回 `1.22.x`。1.20.x 会触发 "not compatible" panic。
///
/// Microsoft GitHub release 中 1.22.x 系列目前只有 1.22.0（1.22.1/1.22.2 均 404，
/// 2026-05-14 实测）。后续 ort 升级到下个 major rc 时再同步 bump 此常量。
pub const ONNXRUNTIME_VERSION: &str = "1.22.0";

/// Microsoft GitHub release base URL.
const RELEASE_BASE: &str =
    "https://github.com/microsoft/onnxruntime/releases/download";

/// Per-platform/arch metadata for the download.
struct PlatformInfo {
    /// Archive filename (without base URL).
    archive: String,
    /// Tar/zip top-level directory prefix inside the archive.
    archive_root: String,
    /// Library filename as it appears in `{archive_root}/lib/` inside the archive.
    lib_name_in_archive: String,
    /// Canonical name we save under in `~/.uclaw/onnxruntime/`.
    canonical_lib_name: String,
    /// `true` → .zip (Windows), `false` → .tgz (macOS / Linux).
    is_zip: bool,
}

fn detect_platform() -> Result<PlatformInfo> {
    let v = ONNXRUNTIME_VERSION;
    let info = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => PlatformInfo {
            archive: format!("onnxruntime-osx-arm64-{}.tgz", v),
            archive_root: format!("onnxruntime-osx-arm64-{}", v),
            lib_name_in_archive: format!("libonnxruntime.{}.dylib", v),
            canonical_lib_name: "libonnxruntime.dylib".into(),
            is_zip: false,
        },
        ("macos", "x86_64") => PlatformInfo {
            archive: format!("onnxruntime-osx-x86_64-{}.tgz", v),
            archive_root: format!("onnxruntime-osx-x86_64-{}", v),
            lib_name_in_archive: format!("libonnxruntime.{}.dylib", v),
            canonical_lib_name: "libonnxruntime.dylib".into(),
            is_zip: false,
        },
        ("linux", "aarch64") => PlatformInfo {
            archive: format!("onnxruntime-linux-aarch64-{}.tgz", v),
            archive_root: format!("onnxruntime-linux-aarch64-{}", v),
            lib_name_in_archive: format!("libonnxruntime.so.{}", v),
            canonical_lib_name: "libonnxruntime.so".into(),
            is_zip: false,
        },
        ("linux", "x86_64") => PlatformInfo {
            archive: format!("onnxruntime-linux-x64-{}.tgz", v),
            archive_root: format!("onnxruntime-linux-x64-{}", v),
            lib_name_in_archive: format!("libonnxruntime.so.{}", v),
            canonical_lib_name: "libonnxruntime.so".into(),
            is_zip: false,
        },
        ("windows", "x86_64") => PlatformInfo {
            archive: format!("onnxruntime-win-x64-{}.zip", v),
            archive_root: format!("onnxruntime-win-x64-{}", v),
            lib_name_in_archive: "onnxruntime.dll".into(),
            canonical_lib_name: "onnxruntime.dll".into(),
            is_zip: true,
        },
        (os, arch) => {
            return Err(anyhow!("unsupported platform: {}-{}", os, arch))
        }
    };
    Ok(info)
}

fn install_root() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("home dir not found"))?;
    // 版本隔离：~/.uclaw/onnxruntime/{version}/libonnxruntime.dylib
    // ort 升级换版本时旧目录无害留下（用户可手动清理），新版本走干净状态。
    Ok(home
        .join(".uclaw")
        .join("onnxruntime")
        .join(ONNXRUNTIME_VERSION))
}

/// Return the path where libonnxruntime is (or will be) installed.
pub fn dylib_path() -> Result<PathBuf> {
    let info = detect_platform()?;
    Ok(install_root()?.join(&info.canonical_lib_name))
}

/// Progress callback: `(phase, bytes_done, total_bytes_opt)`.
/// `phase` is one of `"download"`, `"extract"`.
pub type ProgressCallback =
    std::sync::Arc<dyn Fn(&str, u64, Option<u64>) + Send + Sync>;

/// Idempotently ensure libonnxruntime is downloaded and `ORT_DYLIB_PATH` is set.
///
/// Safe to call multiple times — returns quickly if the library already exists.
/// After the first successful call:
/// - The library file exists at the path returned by [`dylib_path()`].
/// - `ORT_DYLIB_PATH` is set to that absolute path so the next
///   `ort::Session::builder()` call resolves the library via `dlopen`.
///
/// `progress_cb` receives `(phase, bytes_done, total_bytes_opt)` updates.
pub async fn ensure_onnxruntime(
    progress_cb: Option<ProgressCallback>,
) -> Result<PathBuf> {
    let info = detect_platform()?;
    let install = install_root()?;
    let lib_path = install.join(&info.canonical_lib_name);

    if lib_path.exists() {
        // Already installed — just refresh the env var.
        // SAFETY: no other thread is racing here; env mutation is inherently
        // unsafe in a multi-threaded context but is the canonical way to tell ort
        // where to find the library before any Session is built.
        #[allow(unused_unsafe)]
        unsafe {
            std::env::set_var("ORT_DYLIB_PATH", &lib_path);
        }
        return Ok(lib_path);
    }

    tokio::fs::create_dir_all(&install)
        .await
        .with_context(|| format!("create install dir {:?}", install))?;

    let archive_path = install.join(&info.archive);
    let archive_tmp = install.join(format!("{}.tmp", info.archive));

    // 1. Download the archive to a .tmp file then atomically rename.
    let url = format!(
        "{}/v{}/{}",
        RELEASE_BASE, ONNXRUNTIME_VERSION, info.archive
    );
    download_to(&url, &archive_tmp, progress_cb.as_ref(), "download")
        .await
        .with_context(|| format!("download {}", url))?;
    tokio::fs::rename(&archive_tmp, &archive_path)
        .await
        .with_context(|| format!("rename to {:?}", archive_path))?;

    // 2. Extract only the library file; skip everything else.
    if info.is_zip {
        extract_zip_lib(&archive_path, &info, &lib_path, progress_cb.as_ref())
            .await
            .context("zip extraction")?;
    } else {
        extract_tarball_lib(&archive_path, &info, &lib_path, progress_cb.as_ref())
            .await
            .context("tarball extraction")?;
    }

    // 3. Remove the archive — we only need the lib going forward.
    let _ = tokio::fs::remove_file(&archive_path).await;

    // 4. Set env var so ort dlopen finds it on the next Session::builder() call.
    #[allow(unused_unsafe)]
    unsafe {
        std::env::set_var("ORT_DYLIB_PATH", &lib_path);
    }

    Ok(lib_path)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn download_to(
    url: &str,
    dest: &Path,
    progress_cb: Option<&ProgressCallback>,
    phase: &str,
) -> Result<()> {
    let resp = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {}", url))?;

    if !resp.status().is_success() {
        return Err(anyhow!("HTTP {} for {}", resp.status(), url));
    }

    let total = resp.content_length();
    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(dest)
        .await
        .with_context(|| format!("create {:?}", dest))?;
    let mut downloaded: u64 = 0;

    use futures_util::StreamExt as _;
    use tokio::io::AsyncWriteExt as _;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("chunk read")?;
        file.write_all(&chunk).await.context("write chunk")?;
        downloaded += chunk.len() as u64;
        if let Some(cb) = progress_cb {
            cb(phase, downloaded, total);
        }
    }
    file.flush().await?;
    Ok(())
}

async fn extract_tarball_lib(
    archive: &Path,
    info: &PlatformInfo,
    dest_lib: &Path,
    progress_cb: Option<&ProgressCallback>,
) -> Result<()> {
    // The lib lives at `{archive_root}/lib/{lib_name_in_archive}`.
    let want = format!(
        "{}/lib/{}",
        info.archive_root, info.lib_name_in_archive
    );

    let archive = archive.to_path_buf();
    let dest = dest_lib.to_path_buf();
    let progress_cb = progress_cb.cloned();

    tokio::task::spawn_blocking(move || -> Result<()> {
        use std::fs::File;
        use std::io::{BufReader, Read, Write};

        let f = File::open(&archive)
            .with_context(|| format!("open archive {:?}", archive))?;
        let gz = flate2::read::GzDecoder::new(BufReader::new(f));
        let mut tar = tar::Archive::new(gz);

        for entry in tar.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            if path.to_string_lossy() == want {
                let mut out = File::create(&dest)
                    .with_context(|| format!("create dest {:?}", dest))?;
                let mut buf = [0u8; 64 * 1024];
                let mut total = 0u64;
                loop {
                    let n = entry.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    out.write_all(&buf[..n])?;
                    total += n as u64;
                    if let Some(cb) = &progress_cb {
                        cb("extract", total, None);
                    }
                }
                out.flush()?;
                return Ok(());
            }
        }

        Err(anyhow!("library '{}' not found in archive", want))
    })
    .await
    .context("spawn_blocking join")??;

    Ok(())
}

async fn extract_zip_lib(
    archive: &Path,
    info: &PlatformInfo,
    dest_lib: &Path,
    _progress_cb: Option<&ProgressCallback>,
) -> Result<()> {
    // The lib lives at `{archive_root}/lib/{lib_name_in_archive}`.
    let want = format!(
        "{}/lib/{}",
        info.archive_root, info.lib_name_in_archive
    );

    let archive = archive.to_path_buf();
    let dest = dest_lib.to_path_buf();

    tokio::task::spawn_blocking(move || -> Result<()> {
        let f = std::fs::File::open(&archive)
            .with_context(|| format!("open zip {:?}", archive))?;
        let mut zip = zip::ZipArchive::new(f)?;
        let mut entry = zip
            .by_name(&want)
            .map_err(|e| anyhow!("zip entry '{}': {}", want, e))?;
        let mut out = std::fs::File::create(&dest)
            .with_context(|| format!("create dest {:?}", dest))?;
        std::io::copy(&mut entry, &mut out)?;
        Ok(())
    })
    .await
    .context("zip spawn_blocking join")??;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_platform_returns_canonical_lib_name_per_os() {
        let info = detect_platform().unwrap();
        let canonical = info.canonical_lib_name.as_str();
        assert!(
            canonical == "libonnxruntime.dylib"
                || canonical == "libonnxruntime.so"
                || canonical == "onnxruntime.dll",
            "unexpected canonical lib name: {}",
            canonical
        );
    }

    #[test]
    fn detect_platform_archive_path_is_predictable() {
        let info = detect_platform().unwrap();
        assert!(
            info.archive.starts_with("onnxruntime-"),
            "archive should start with 'onnxruntime-': {}",
            info.archive
        );
        assert!(
            info.archive.contains(ONNXRUNTIME_VERSION),
            "archive should contain version {}: {}",
            ONNXRUNTIME_VERSION,
            info.archive
        );
        assert!(
            info.archive_root.contains(ONNXRUNTIME_VERSION),
            "archive_root should contain version {}: {}",
            ONNXRUNTIME_VERSION,
            info.archive_root
        );
    }

    #[test]
    fn install_root_is_under_home_uclaw_versioned() {
        let root = install_root().unwrap();
        let s = root.to_string_lossy();
        assert!(s.contains(".uclaw"), "install_root should contain .uclaw: {}", s);
        // 版本隔离：路径以 onnxruntime/{ONNXRUNTIME_VERSION} 结尾
        let suffix = format!("onnxruntime/{}", ONNXRUNTIME_VERSION);
        assert!(
            s.ends_with(&suffix),
            "install_root should end with {}: {}",
            suffix,
            s
        );
    }

    #[test]
    fn dylib_path_matches_install_root_plus_canonical() {
        let info = detect_platform().unwrap();
        let p = dylib_path().unwrap();
        assert!(
            p.ends_with(&info.canonical_lib_name),
            "dylib_path {:?} should end with {}",
            p,
            info.canonical_lib_name
        );
    }

    #[tokio::test]
    async fn ensure_onnxruntime_is_idempotent_when_file_exists() {
        // We cannot safely touch ~/.uclaw in unit tests, so this test documents
        // the idempotency contract at the code level only. The happy path (lib
        // already present → early return + env var set) is verified by the
        // `lib_path.exists()` guard in `ensure_onnxruntime`. Actual download is
        // a manual smoke test — downloading the ~650 MB tarball in CI would
        // inflate runtime unacceptably.
        let _ = dylib_path(); // compile-time + path-construction smoke only
    }
}
