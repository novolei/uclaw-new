# Slice C — Model Management + Smart Download Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto-fetch the MiniCPM5-1B GGUF + external `tokenizer.json`, pick the fastest of HuggingFace / hf-mirror / ModelScope by a lightweight ranged-GET probe, cache/verify/manage the files, and emit download progress events — landing the files exactly where Slice B's engine reads them (`~/.uclaw/models/minicpm5-1b/`).

**Architecture:** A new neutral `src-tauri/src/model_fetch/` module holds a **manifest-driven downloader** (manifest types + per-host URL builders + a `SourceProbe` trait + the streaming `download_manifest` core). `src-tauri/src/local_llm/model_manager.rs` builds the MiniCPM manifest, drives the downloader, and manages the cache (list/delete/cancel) via a module-static cancel registry — **no `AppState` change**. Five Tauri commands expose it. The existing bge embedder downloader (`memory_bucket_seal/score/embed/model_download.rs`) is refactored to build a bge manifest and call the same core (generalize, don't duplicate), gated by the bge tests so there is **no embedder regression**.

**Tech Stack:** Rust, `reqwest 0.12` (stream + Range header — already a dep), `sha2 0.10` (optional checksum — already a dep), `futures 0.3` (concurrent probes — already a dep), `sysinfo 0.31` (disk-space guard — already a dep), `tauri` events (`app.emit`), `std::sync::OnceLock`/`AtomicBool` (cancel registry, mirroring the existing `CONSOLIDATION_CANCELLED` pattern). Frontend bridge in `ui/src/lib/tauri-bridge.ts`.

---

## Boundary with adjacent slices (read before starting)

- **Slice B (shipped, PR #654, merge `9c3a1a54`)** is the downstream reader. It reads `~/.uclaw/models/minicpm5-1b/MiniCPM5-1B-Q4_K_M.gguf` + `tokenizer.json` via `crate::local_llm::{model_dir, model_paths, is_present, MODEL_ID, GGUF_FILENAME, TOKENIZER_FILENAME}`. **Slice C must write exactly those filenames to that directory.** Reuse those constants — do not redefine the paths.
- **candle uses an EXTERNAL `tokenizer.json`** (not the GGUF-embedded one). The GGUF repo `openbmb/MiniCPM5-1B-GGUF` may lack `tokenizer.json`; fetch it from the **base repo `openbmb/MiniCPM5-1B`**. So the two files come from **different repos** — the manifest design below carries per-file source URLs to handle this.
- **Slice D (next, not this PR)** builds the onboarding wizard that *calls* these commands and subscribes to the `minicpm://download-progress` event. Slice C must emit that event with the exact payload shape `{ model_id, file, downloaded, total, source, phase }`. D also adds `local_model_env_check` / `warmup` / `smoke_test` — **those are NOT in Slice C**. C ships only: `local_model_probe_sources`, `local_model_download`, `local_model_list`, `local_model_cancel`, `local_model_delete`.
- **No DB migration** (cache state lives on the filesystem, not SQLite).

## Verified facts (recon at plan time)

- bge downloader to generalize: `memory_bucket_seal/score/embed/model_download.rs` — has `model_dir(&Path)`, `is_present(&Path)`, `ensure_model(&Path)`, a `FILES: &[(&str,&str)]` const (`onnx/model.onnx`→`model.onnx`, `tokenizer.json`→`tokenizer.json`), `HF_BASE`/`MIRROR_BASE`, and `fetch_with_fallback`/`fetch_one` (streaming + tmp→final). Public API consumed by `onnx.rs` (`ensure_model`) and `factory.rs` (`model_dir`). **These public signatures must stay intact.**
- STT downloader pattern to mirror: `stt/openflow/downloader.rs` — `download_one` (streaming chunks + progress callback `Fn(&str,u64,Option<u64>)`, `content_length()`, tmp→final rename), pure URL builders with unit tests.
- Tauri emit: `app_handle.emit("event", serde_json::json!({...}))` with `app_handle: &tauri::AppHandle` (needs `use tauri::Emitter`).
- Existing cancel pattern: `static CONSOLIDATION_CANCELLED: AtomicBool` + a `cancel_*` command (`tauri_commands.rs`).
- `invoke_handler!` macro at `main.rs:846` (`tauri::generate_handler![ ... ]`), entries like `uclaw_core::tauri_commands::<name>`.
- Commands access the data dir via `state: tauri::State<'_, AppState>` → `state.data_dir` (PathBuf, `app.rs:158`).
- Deps present: `reqwest` (stream), `sha2 = "0.10"`, `futures = "0.3"`, `sysinfo = "0.31"` (features `["system"]` — for disk we may need to confirm a disk feature; see Task 5), `tempfile` (dev).

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/lib.rs` | `pub mod model_fetch;` declaration |
| `src-tauri/src/model_fetch/mod.rs` | module wiring + re-exports |
| `src-tauri/src/model_fetch/manifest.rs` | `ModelManifest`/`ManifestFile`/`FileSource`/`Host` + per-host URL builders (pure, unit-tested) |
| `src-tauri/src/model_fetch/probe.rs` | `SourceProbe` trait + `HttpProbe` (ranged GET) + pure `rank_hosts` (unit-tested with injected latencies) |
| `src-tauri/src/model_fetch/download.rs` | `download_manifest` streaming core: progress cb + cancel flag + tmp→final + size/sha256 verify + per-file source fallback + retry-once |
| `src-tauri/src/local_llm/model_manager.rs` | quant→filename map, `minicpm_manifest`, `ModelManager` (is_installed / missing / list / delete / download driver) + module-static cancel registry |
| `src-tauri/src/local_llm/mod.rs` | `pub mod model_manager;` |
| `src-tauri/src/tauri_commands.rs` | 5 `local_model_*` commands + `minicpm://download-progress` emit |
| `src-tauri/src/main.rs` | register the 5 commands in `invoke_handler!` |
| `ui/src/lib/tauri-bridge.ts` | typed wrappers for the 5 commands + progress-event payload type |
| `memory_bucket_seal/score/embed/model_download.rs` | refactored to build a bge manifest + call `model_fetch::download_manifest` (no API change) |

All new `.rs` files start with `// SPDX-License-Identifier: Apache-2.0`.

---

## Task 1: `model_fetch` manifest types + per-host URL builders (pure, unit-tested)

**Files:**
- Modify: `src-tauri/src/lib.rs` (add `pub mod model_fetch;` near `pub mod local_llm;`)
- Create: `src-tauri/src/model_fetch/mod.rs`
- Create: `src-tauri/src/model_fetch/manifest.rs`

- [ ] **Step 1: Declare the module in `lib.rs`**

After `pub mod local_llm;` add:
```rust
pub mod model_fetch;
```

- [ ] **Step 2: Create `src-tauri/src/model_fetch/mod.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Manifest-driven model downloader, shared by the bge embedder and the
//! local MiniCPM engine. A `ModelManifest` lists files (each with its own
//! ordered candidate source URLs); `download_manifest` streams them with
//! progress + cancellation + verification; `SourceProbe` ranks hosts by a
//! lightweight ranged-GET latency probe so the fastest reachable mirror wins.

pub mod download;
pub mod manifest;
pub mod probe;

pub use download::{download_manifest, DownloadEvent, Progress};
pub use manifest::{FileSource, Host, ManifestFile, ModelManifest};
pub use probe::{rank_hosts, HttpProbe, ProbeResult, SourceProbe};
```

- [ ] **Step 3: Write the failing tests in `manifest.rs`**

```rust
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
```

- [ ] **Step 4: Build + test**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no errors.
Run: `cd src-tauri && cargo test --lib model_fetch::manifest 2>&1 | tail -20` → `test result: ok. 4 passed`.

> Note: `mod.rs` references `download`/`probe` which don't exist yet. Create them as one-line stubs in this task so `mod.rs` compiles:
> - `src-tauri/src/model_fetch/download.rs`: `// SPDX-License-Identifier: Apache-2.0` + temporary `pub struct Progress; pub enum DownloadEvent {} pub async fn download_manifest() {}` — **NO**, that breaks the re-export signatures. Instead: make `mod.rs` only declare+re-export `manifest` in Task 1, and add the `download`/`probe` `pub mod` + re-exports in Tasks 2-3. Concretely, in this task `mod.rs` should be:
> ```rust
> // SPDX-License-Identifier: Apache-2.0
> //! Manifest-driven model downloader (see module docs).
> pub mod manifest;
> pub use manifest::{FileSource, Host, ManifestFile, ModelManifest};
> ```
> Then Task 2 adds `pub mod probe;` + its re-exports, Task 3 adds `pub mod download;` + its re-exports. This keeps every commit compiling.

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw/.claude/worktrees/<worktree>
git add src-tauri/src/lib.rs src-tauri/src/model_fetch/
git commit -m "feat(model_fetch): manifest types + per-host URL builders (pure)

Slice C Task 1. ModelManifest/ManifestFile/FileSource/Host + hf/hf-mirror/
modelscope URL construction, unit-tested. Per-file source lists so the GGUF
(GGUF repo) and tokenizer.json (base repo) can come from different repos."
```

---

## Task 2: `SourceProbe` trait + HTTP ranged-GET probe + host ranking (unit-tested with injected latencies)

**Files:**
- Create: `src-tauri/src/model_fetch/probe.rs`
- Modify: `src-tauri/src/model_fetch/mod.rs` (add `pub mod probe;` + re-exports)

- [ ] **Step 1: Write `probe.rs` with the trait, ranking fn, and tests**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Source probing: measure host reachability + first-byte latency with a
//! lightweight ranged GET (first few KB), then rank hosts fastest-first.
//! The probe is behind a trait so ranking is unit-testable with injected
//! latencies (no network).

use std::time::Duration;

use async_trait::async_trait;

use super::manifest::Host;

/// Outcome of probing one host.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub host: Host,
    /// `None` = unreachable / errored; `Some(latency)` = first bytes received.
    pub latency: Option<Duration>,
}

/// Abstracts the network probe so `rank_hosts` can be tested deterministically.
#[async_trait]
pub trait SourceProbe: Send + Sync {
    /// Probe one URL representing a host; return measured first-byte latency.
    async fn probe(&self, host: Host, url: &str) -> ProbeResult;
}

/// Rank probe results fastest-first; unreachable hosts (latency `None`) drop
/// to the end in their original relative order. Pure — unit-testable.
pub fn rank_hosts(mut results: Vec<ProbeResult>) -> Vec<Host> {
    results.sort_by(|a, b| match (a.latency, b.latency) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    results.into_iter().map(|r| r.host).collect()
}

/// Real HTTP probe: a ranged GET for the first `PROBE_BYTES` bytes, timing
/// how long until the response starts. Uses a short timeout so a dead mirror
/// fails fast.
pub struct HttpProbe {
    client: reqwest::Client,
}

const PROBE_BYTES: u64 = 16 * 1024; // 16 KB is enough to confirm a live byte stream
const PROBE_TIMEOUT_SECS: u64 = 6;

impl HttpProbe {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(PROBE_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent("uclaw-backend/model-probe")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

impl Default for HttpProbe {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SourceProbe for HttpProbe {
    async fn probe(&self, host: Host, url: &str) -> ProbeResult {
        let start = std::time::Instant::now();
        let req = self
            .client
            .get(url)
            .header(reqwest::header::RANGE, format!("bytes=0-{}", PROBE_BYTES - 1));
        match req.send().await {
            Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 206 => {
                ProbeResult { host, latency: Some(start.elapsed()) }
            }
            _ => ProbeResult { host, latency: None },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(host: Host, ms: Option<u64>) -> ProbeResult {
        ProbeResult { host, latency: ms.map(Duration::from_millis) }
    }

    #[test]
    fn ranks_fastest_first() {
        let ranked = rank_hosts(vec![
            r(Host::HuggingFace, Some(800)),
            r(Host::ModelScope, Some(120)),
            r(Host::HfMirror, Some(300)),
        ]);
        assert_eq!(ranked, vec![Host::ModelScope, Host::HfMirror, Host::HuggingFace]);
    }

    #[test]
    fn unreachable_hosts_sink_to_end() {
        let ranked = rank_hosts(vec![
            r(Host::HuggingFace, None),
            r(Host::ModelScope, Some(120)),
            r(Host::HfMirror, None),
        ]);
        assert_eq!(ranked[0], Host::ModelScope);
        assert!(ranked[1..].contains(&Host::HuggingFace));
        assert!(ranked[1..].contains(&Host::HfMirror));
    }

    #[test]
    fn all_unreachable_preserves_all() {
        let ranked = rank_hosts(vec![r(Host::HuggingFace, None), r(Host::ModelScope, None)]);
        assert_eq!(ranked.len(), 2);
    }
}
```

- [ ] **Step 2: Wire into `mod.rs`**

Add to `model_fetch/mod.rs`:
```rust
pub mod probe;
pub use probe::{rank_hosts, HttpProbe, ProbeResult, SourceProbe};
```

- [ ] **Step 3: Confirm `async_trait` is available**

Run: `cd src-tauri && grep -n '^async-trait\|async-trait =' Cargo.toml`
Expected: present (the embedder's `Embedder` trait uses it). If absent, add `async-trait = "0.1"`.

- [ ] **Step 4: Build + test**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no errors.
Run: `cd src-tauri && cargo test --lib model_fetch::probe 2>&1 | tail -20` → `test result: ok. 3 passed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/model_fetch/probe.rs src-tauri/src/model_fetch/mod.rs src-tauri/Cargo.toml
git commit -m "feat(model_fetch): SourceProbe trait + ranged-GET probe + host ranking

Slice C Task 2. rank_hosts (fastest-first, unreachable sinks to end) is pure +
unit-tested via injected latencies; HttpProbe does a 16KB ranged GET with a 6s
timeout so dead mirrors fail fast."
```

---

## Task 3: `download_manifest` streaming core (progress + cancel + verify + fallback + retry)

**Files:**
- Create: `src-tauri/src/model_fetch/download.rs`
- Modify: `src-tauri/src/model_fetch/mod.rs` (add `pub mod download;` + re-exports)

- [ ] **Step 1: Write `download.rs`**

```rust
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

/// Progress tick for one file.
#[derive(Debug, Clone)]
pub struct Progress {
    pub file: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub host: Host,
    pub phase: &'static str, // "downloading" | "verifying" | "done"
}

/// Events the caller may surface (here we only use the progress callback;
/// the enum reserves room for richer events without signature churn).
#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Progress(Progress),
}

/// Download every file in the manifest that isn't already present+valid.
/// `cancel` is polled cooperatively; setting it aborts with an error.
/// `on_progress` is called frequently during streaming.
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
        // Try once, then retry once on verification failure.
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
                    let _ = tokio::fs::remove_file(dest.with_extension("tmp")).await;
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

/// Try each source (in the order the manifest lists them — the manager sorts
/// them by probe rank beforehand) until one succeeds, verifying after write.
async fn download_file(
    client: &reqwest::Client,
    file: &ManifestFile,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
    on_progress: &(impl Fn(Progress) + Send + Sync),
) -> Result<(), String> {
    let mut errors = Vec::new();
    for src in &file.sources {
        match stream_to_tmp(client, &src.url, src.host, file, dest, cancel, on_progress).await {
            Ok(()) => {
                // Verify the freshly-downloaded tmp before promoting it.
                let tmp = dest.with_extension("tmp");
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
                let _ = tokio::fs::remove_file(dest.with_extension("tmp")).await;
                tracing::warn!(url = %src.url, error = %e, "source failed, trying next");
            }
        }
    }
    Err(format!("all sources failed:\n{}", errors.join("\n - ")))
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
    let tmp = dest.with_extension("tmp");
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
    let mut buf = vec![0u8; 1 << 20]; // 1 MB
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

    /// A file already present + size-matching must be skipped (no network).
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
        // Sources point at a dead URL; if it tried to download it'd fail. It must
        // skip because the file is present + size matches → Ok.
        let res = download_manifest(&m, cancel, |_| {}).await;
        assert!(res.is_ok(), "valid present file should skip, got {res:?}");
    }

    /// Cancel set before start → cancelled error (file absent so it would try).
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
        // sha256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let got = sha256_file(&p).await.unwrap();
        assert_eq!(got, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
```

- [ ] **Step 2: Wire into `mod.rs`**

Add to `model_fetch/mod.rs`:
```rust
pub mod download;
pub use download::{download_manifest, DownloadEvent, Progress};
```

- [ ] **Step 3: Build + test**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no errors.
Run: `cd src-tauri && cargo test --lib model_fetch::download 2>&1 | tail -20` → `test result: ok. 3 passed`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/model_fetch/download.rs src-tauri/src/model_fetch/mod.rs
git commit -m "feat(model_fetch): streaming download_manifest (cancel/verify/fallback)

Slice C Task 3. Per-file source fallback, progress callback, cooperative
cancel (Arc<AtomicBool>), atomic tmp→final, size + optional streaming sha256
verify, retry-once on failure. Present+valid files skip."
```

---

## Task 4: MiniCPM manifest + `ModelManager` (cache mgmt + cancel registry)

**Files:**
- Create: `src-tauri/src/local_llm/model_manager.rs`
- Modify: `src-tauri/src/local_llm/mod.rs` (add `pub mod model_manager;`)

- [ ] **Step 1: Write the failing tests + types in `model_manager.rs`**

```rust
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

use super::{model_dir, GGUF_FILENAME, MODEL_ID, TOKENIZER_FILENAME};

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

/// Build candidate sources for one file across all three hosts, in default
/// order (HF, hf-mirror, ModelScope). The manager re-sorts by probe rank.
fn sources_for(hf_repo: &str, ms_repo: &str, path: &str) -> Vec<FileSource> {
    use crate::model_fetch::manifest::{hf_mirror_url, hf_url, modelscope_url};
    vec![
        FileSource { host: Host::HuggingFace, url: hf_url(hf_repo, "main", path) },
        FileSource { host: Host::HfMirror, url: hf_mirror_url(hf_repo, "main", path) },
        FileSource { host: Host::ModelScope, url: modelscope_url(ms_repo, "master", path) },
    ]
}

/// Build the MiniCPM manifest for a quant. The GGUF comes from the GGUF repo;
/// `tokenizer.json` comes from the BASE repo (the GGUF repo may lack it, and
/// candle needs the external tokenizer).
pub fn minicpm_manifest(data_dir: &Path, quant: Quant) -> ModelManifest {
    let cache_dir = model_dir(data_dir);
    let gguf_name = quant.gguf_filename();
    ModelManifest {
        cache_dir,
        files: vec![
            ManifestFile {
                dest_name: gguf_name.to_string(),
                sources: sources_for(GGUF_REPO, GGUF_REPO_MS, gguf_name),
                expected_size: None, // filled by Slice C follow-up if exact sizes are pinned
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
    pub fn missing_files(&self, quant: Quant) -> Vec<String> {
        let m = minicpm_manifest(&self.data_dir, quant);
        m.files
            .into_iter()
            .filter(|f| !m_cache_join(&self.data_dir, &f.dest_name).exists())
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
    /// (from a probe; pass the default order if not probed). Progress is forwarded.
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

fn m_cache_join(data_dir: &Path, name: &str) -> PathBuf {
    model_dir(data_dir).join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Must equal Slice B's GGUF_FILENAME so the engine finds the file.
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
        // tokenizer comes from the BASE repo, not the GGUF repo
        assert!(tok.sources.iter().any(|s| s.url.contains("/openbmb/MiniCPM5-1B/resolve")
            || s.url.contains("/OpenBMB/MiniCPM5-1B/resolve")));
        // each file offers all three hosts
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
```

- [ ] **Step 2: Wire into `local_llm/mod.rs`**

Add (near `pub mod engine;`):
```rust
pub mod model_manager;
```

- [ ] **Step 3: Build + test**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no errors.
Run: `cd src-tauri && cargo test --lib local_llm::model_manager 2>&1 | tail -20` → `test result: ok. 6 passed`.

> **Implementer note (source verification):** the ModelScope repo path/revision (`OpenBMB/MiniCPM5-1B-GGUF`, revision `master`) and the exact GGUF filename for each quant should be confirmed against the live repos if possible. If a quant's exact filename differs (e.g. ModelScope names it differently), the manifest's per-host URLs may need per-host filenames — but for v1 the HF naming is authoritative and the default order tries HF first. Note any discrepancy in your report; do not block on it (the probe falls through to a reachable host).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/local_llm/model_manager.rs src-tauri/src/local_llm/mod.rs
git commit -m "feat(local_llm): MiniCPM ModelManager + manifest + cancel registry

Slice C Task 4. minicpm_manifest (GGUF from GGUF repo, tokenizer.json from base
repo, 3 hosts each); ModelManager is_installed/missing/list/delete/cancel via a
process-static cancel registry; download() sorts sources by probe rank. Default
quant filename asserted == Slice B's GGUF_FILENAME so the engine finds it."
```

---

## Task 5: Tauri commands + invoke_handler + bridge + progress event + disk guard

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (5 commands + emit helper)
- Modify: `src-tauri/src/main.rs` (register 5 in `invoke_handler!`)
- Modify: `ui/src/lib/tauri-bridge.ts` (typed wrappers + event payload type)

> **DMZ note (per uclaw-tauri-commands skill):** `tauri_commands.rs` is a DMZ file — the code-quality review of this task IS the required second-session read.

- [ ] **Step 1: Add the commands to `tauri_commands.rs`**

Find the imports/top of the file; ensure `use tauri::Emitter;` is in scope (grep — many emit sites exist, so it likely is). Add near other feature command groups:

```rust
// ─── Local model management (Slice C) ─────────────────────────────────

use crate::local_llm::model_manager::{InstalledModel, ModelManager, Quant};
use crate::model_fetch::manifest::Host;
use crate::model_fetch::{rank_hosts, HttpProbe, ProbeResult, SourceProbe};

#[derive(serde::Serialize)]
pub struct ProbedSource {
    pub host: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
}

/// Probe HF / hf-mirror / ModelScope concurrently with a small ranged GET and
/// return them fastest-first. Uses the GGUF repo's file as the probe target.
#[tauri::command]
pub async fn local_model_probe_sources() -> Result<Vec<ProbedSource>, String> {
    let probe = HttpProbe::new();
    // Representative probe URLs (the default-quant GGUF) per host.
    let m = crate::local_llm::model_manager::minicpm_manifest(
        std::path::Path::new("/nonexistent"),
        Quant::Q4KM,
    );
    let gguf_sources = m.files[0].sources.clone();
    let results: Vec<ProbeResult> = futures::future::join_all(
        gguf_sources
            .iter()
            .map(|s| probe.probe(s.host, &s.url)),
    )
    .await;
    let by_host: std::collections::HashMap<Host, ProbeResult> =
        results.iter().cloned().map(|r| (r.host, r)).collect();
    let ranked = rank_hosts(results.clone());
    Ok(ranked
        .into_iter()
        .map(|h| {
            let r = by_host.get(&h);
            ProbedSource {
                host: h.id().to_string(),
                reachable: r.map(|r| r.latency.is_some()).unwrap_or(false),
                latency_ms: r.and_then(|r| r.latency).map(|d| d.as_millis() as u64),
            }
        })
        .collect())
}

fn emit_download_progress(
    app: &tauri::AppHandle,
    model_id: &str,
    p: &crate::model_fetch::Progress,
) {
    let _ = app.emit(
        "minicpm://download-progress",
        serde_json::json!({
            "model_id": model_id,
            "file": p.file,
            "downloaded": p.downloaded,
            "total": p.total,
            "source": p.host.id(),
            "phase": p.phase,
        }),
    );
}

/// Download the MiniCPM model (default quant Q4_K_M). Probes sources unless
/// `source` is pinned, guards disk space, streams with progress events, and
/// resolves when complete. Cancellable via `local_model_cancel`.
#[tauri::command]
pub async fn local_model_download(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    quant: Option<String>,
    source: Option<String>,
) -> Result<(), String> {
    let quant = quant
        .as_deref()
        .map(|q| Quant::from_str_lenient(q).ok_or_else(|| format!("unknown quant: {q}")))
        .transpose()?
        .unwrap_or(Quant::Q4KM);

    let data_dir = state.data_dir.clone();
    let mgr = ModelManager::new(data_dir.clone());
    let model_id = crate::local_llm::MODEL_ID.to_string();

    // Determine host order: pinned source first, else probe-ranked, else default.
    let host_order: Vec<Host> = if let Some(src) = source.as_deref() {
        match src {
            "huggingface" => vec![Host::HuggingFace, Host::HfMirror, Host::ModelScope],
            "hf-mirror" => vec![Host::HfMirror, Host::HuggingFace, Host::ModelScope],
            "modelscope" => vec![Host::ModelScope, Host::HfMirror, Host::HuggingFace],
            other => return Err(format!("unknown source: {other}")),
        }
    } else {
        match local_model_probe_sources().await {
            Ok(ranked) => ranked
                .iter()
                .filter_map(|p| match p.host.as_str() {
                    "huggingface" => Some(Host::HuggingFace),
                    "hf-mirror" => Some(Host::HfMirror),
                    "modelscope" => Some(Host::ModelScope),
                    _ => None,
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    };

    // Disk-space guard: require a conservative headroom for the default quant.
    if let Some(free) = crate::local_llm::model_manager::free_disk_bytes(&data_dir) {
        const NEED_BYTES: u64 = 1_200_000_000; // ~1.2 GB headroom for the 688 MB quant + tmp
        if free < NEED_BYTES {
            return Err(format!(
                "insufficient disk space: {} MB free, need ~{} MB",
                free / 1_000_000,
                NEED_BYTES / 1_000_000
            ));
        }
    }

    let cancel = mgr.begin(&model_id);
    let app_for_cb = app.clone();
    let model_for_cb = model_id.clone();
    let result = mgr
        .download(quant, &host_order, cancel, move |p| {
            emit_download_progress(&app_for_cb, &model_for_cb, &p);
        })
        .await;
    mgr.finish(&model_id);
    result
}

/// List installed local models (single MiniCPM model in v1).
#[tauri::command]
pub async fn local_model_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<InstalledModel>, String> {
    Ok(ModelManager::new(state.data_dir.clone()).list())
}

/// Cancel an in-flight download. Returns true if a download was cancelled.
#[tauri::command]
pub async fn local_model_cancel(
    state: tauri::State<'_, AppState>,
    model_id: Option<String>,
) -> Result<bool, String> {
    let id = model_id.unwrap_or_else(|| crate::local_llm::MODEL_ID.to_string());
    Ok(ModelManager::new(state.data_dir.clone()).cancel(&id))
}

/// Delete the local model cache.
#[tauri::command]
pub async fn local_model_delete(
    state: tauri::State<'_, AppState>,
    model_id: Option<String>,
) -> Result<(), String> {
    let _ = model_id; // single model in v1
    ModelManager::new(state.data_dir.clone()).delete()
}
```

- [ ] **Step 2: Add `free_disk_bytes` to `model_manager.rs`**

`sysinfo 0.31` exposes disks via `sysinfo::Disks`. Add to `model_manager.rs`:
```rust
/// Best-effort free bytes on the volume containing `data_dir` (None if unknown).
/// Used as a pre-download disk-space guard.
pub fn free_disk_bytes(data_dir: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    // Pick the disk whose mount point is the longest prefix of data_dir.
    let mut best: Option<(usize, u64)> = None;
    let target = data_dir.to_string_lossy();
    for d in disks.list() {
        let mp = d.mount_point().to_string_lossy();
        if target.starts_with(mp.as_ref()) {
            let len = mp.len();
            if best.map(|(l, _)| len > l).unwrap_or(true) {
                best = Some((len, d.available_space()));
            }
        }
    }
    best.map(|(_, free)| free)
}
```
**Confirm the `sysinfo` feature for disks:** run `cd src-tauri && grep -n 'sysinfo' Cargo.toml`. The current features are `["system"]`; disks need the `disk` feature. Add it: `sysinfo = { version = "0.31", default-features = false, features = ["system", "disk"] }`. Verify `sysinfo::Disks` resolves after the feature add. If the API differs in 0.31 (e.g. `Disks::new_with_refreshed_list` vs `new()` + `refresh_list()`), adapt to the actual 0.31 API (read `~/.cargo/registry/src/*/sysinfo-0.31*/src/`).

- [ ] **Step 3: Register the 5 commands in `main.rs`**

In the `tauri::generate_handler![ ... ]` block (`main.rs:846`), add:
```rust
            uclaw_core::tauri_commands::local_model_probe_sources,
            uclaw_core::tauri_commands::local_model_download,
            uclaw_core::tauri_commands::local_model_list,
            uclaw_core::tauri_commands::local_model_cancel,
            uclaw_core::tauri_commands::local_model_delete,
```

- [ ] **Step 4: Verify both files reference each command (two-edit rule)**

Run: `cd src-tauri && for c in local_model_probe_sources local_model_download local_model_list local_model_cancel local_model_delete; do echo "$c:"; grep -c "$c" src/tauri_commands.rs src/main.rs; done`
Expected: each appears in BOTH files (count ≥ 1 each).

- [ ] **Step 5: Add the TypeScript bridge in `ui/src/lib/tauri-bridge.ts`**

Match the existing wrapper pattern (find a few `invoke(...)` wrappers to copy the style):
```typescript
// ── Local model management (Slice C) ──────────────────────────────────

export interface ProbedSource {
  host: string;
  reachable: boolean;
  latency_ms: number | null;
}

export interface InstalledFile {
  name: string;
  bytes: number;
}

export interface InstalledModel {
  model_id: string;
  installed: boolean;
  files: InstalledFile[];
  total_bytes: number;
}

/** Payload of the `minicpm://download-progress` Tauri event. */
export interface MiniCpmDownloadProgress {
  model_id: string;
  file: string;
  downloaded: number;
  total: number | null;
  source: string;
  phase: "downloading" | "verifying" | "done";
}

export function localModelProbeSources(): Promise<ProbedSource[]> {
  return invoke("local_model_probe_sources");
}

export function localModelDownload(opts?: { quant?: string; source?: string }): Promise<void> {
  return invoke("local_model_download", { quant: opts?.quant ?? null, source: opts?.source ?? null });
}

export function localModelList(): Promise<InstalledModel[]> {
  return invoke("local_model_list");
}

export function localModelCancel(modelId?: string): Promise<boolean> {
  return invoke("local_model_cancel", { modelId: modelId ?? null });
}

export function localModelDelete(modelId?: string): Promise<void> {
  return invoke("local_model_delete", { modelId: modelId ?? null });
}
```
(Confirm `invoke` is imported at the top of tauri-bridge.ts the same way other wrappers use it; match the file's existing import + export conventions exactly.)

- [ ] **Step 6: Build both sides**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no errors.
Run: `cd ui && npx tsc --noEmit 2>&1 | head -10` → no errors referencing the new wrappers.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs src-tauri/src/local_llm/model_manager.rs src-tauri/Cargo.toml ui/src/lib/tauri-bridge.ts
git commit -m "feat(tauri): local_model_* commands + probe/download/list/cancel/delete

Slice C Task 5. Five IPC commands (registered in both tauri_commands.rs and the
invoke_handler! macro per the two-edit rule); concurrent source probe, disk-space
guard (sysinfo), minicpm://download-progress events, cancel via the registry.
TS bridge wrappers + progress payload type."
```

---

## Task 6: Refactor the bge embedder downloader onto `model_fetch` (no regression)

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/score/embed/model_download.rs`

> **Risk note:** the bge embedder is load-bearing for memory recall. This task GENERALIZES it onto `model_fetch` to prove the shared core, but must keep the public API (`model_dir`, `is_present`, `ensure_model`) byte-identical in behavior. The existing tests (`model_dir_under_data`, `is_present_requires_both_files`) plus the `#[ignore]`d live parity test are the regression gates. If anything about bge behavior would change, STOP and report — do not alter the embedder's download semantics.

- [ ] **Step 1: Re-implement `ensure_model` over `model_fetch`, keeping the signature**

Replace the body of `ensure_model` (and the `fetch_with_fallback`/`fetch_one` helpers it used) with a manifest built from the existing bge constants, calling `model_fetch::download_manifest`. Keep `model_dir`, `is_present`, the `FILES`/`HF_BASE`/`MIRROR_BASE` consts, and the public `ensure_model(dir: &Path) -> anyhow::Result<()>` signature.

```rust
/// Download any missing bge files (HF then mirror) via the shared
/// manifest downloader. Public signature unchanged for the embedder callers.
pub async fn ensure_model(dir: &Path) -> anyhow::Result<()> {
    use crate::model_fetch::manifest::{hf_mirror_url, hf_url, FileSource, Host, ManifestFile, ModelManifest};
    if is_present(dir) {
        return Ok(());
    }
    // bge repo + revision (from the existing HF_BASE/MIRROR_BASE constants).
    const REPO: &str = "BAAI/bge-small-en-v1.5";
    let files = FILES
        .iter()
        .map(|(rel, local)| ManifestFile {
            dest_name: (*local).to_string(),
            sources: vec![
                FileSource { host: Host::HuggingFace, url: hf_url(REPO, "main", rel) },
                FileSource { host: Host::HfMirror, url: hf_mirror_url(REPO, "main", rel) },
            ],
            expected_size: None,
            sha256: None,
        })
        .collect();
    let manifest = ModelManifest { cache_dir: dir.to_path_buf(), files };
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    crate::model_fetch::download_manifest(&manifest, cancel, |_| {})
        .await
        .map_err(|e| anyhow::anyhow!("bge download: {e}"))
}
```
Delete the now-unused `fetch_with_fallback`/`fetch_one` helpers. Keep `model_dir`, `is_present`, `FILES`, and the `#[cfg(test)] mod tests` block (those tests must still pass unchanged).

> **Verify the bge URL is byte-identical to before.** Old `HF_BASE` = `https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main`, and `FILES` rel paths are `onnx/model.onnx` + `tokenizer.json`. So old URL = `https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/onnx/model.onnx`. `hf_url("BAAI/bge-small-en-v1.5", "main", "onnx/model.onnx")` = the same string. Confirm this equality holds for BOTH files before committing (the `dest_name` is the local name `model.onnx`, the URL uses the rel path `onnx/model.onnx` — make sure you pass `rel` to the URL builder and `local` to `dest_name`).

- [ ] **Step 2: Build + bge tests**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → no errors.
Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::embed::model_download 2>&1 | tail -20` → existing 2 tests still pass.
Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::embed 2>&1 | tail -20` → embedder suite green (live `#[ignore]`d tests skip).

- [ ] **Step 3: Sanity-check no other caller broke**

Run: `cd src-tauri && grep -rn "fetch_with_fallback\|fetch_one" src/` → no remaining references (they were private helpers).
Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/score/embed/model_download.rs
git commit -m "refactor(embed): bge downloader onto shared model_fetch manifest

Slice C Task 6. ensure_model now builds a bge ManifestFile list + calls
model_fetch::download_manifest (same HF→mirror URLs, same files, same tmp→final).
Public API (model_dir/is_present/ensure_model) unchanged; bge tests green. No
embedder regression — proves the manifest core generalizes."
```

---

## Final verification (before opening the PR)

- [ ] **Full backend build:** `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
- [ ] **All Slice C unit tests:** `cargo test --lib model_fetch 2>&1 | tail -20` (manifest 4 + probe 3 + download 3), `cargo test --lib local_llm::model_manager 2>&1 | tail -20` (6), `cargo test --lib memory_bucket_seal::score::embed::model_download 2>&1 | tail -20` (2) → all pass.
- [ ] **TS check:** `cd ui && npx tsc --noEmit 2>&1 | head -10` → clean.
- [ ] **Two-edit audit:** every `local_model_*` command appears in BOTH `tauri_commands.rs` and `main.rs`.
- [ ] **Clippy:** `cd src-tauri && cargo clippy --lib 2>&1 | grep -iE "model_fetch|model_manager|local_model" | head` → no warnings on new code.
- [ ] **GitNexus change check:** `gitnexus_detect_changes()` — confirm only expected symbols/flows changed.
- [ ] **Manual end-to-end (optional, real network):** `local_model_probe_sources()` returns ranked hosts; `local_model_download()` fetches both files into `~/.uclaw/models/minicpm5-1b/`; then Slice B's gated `smoke_generate_two_plus_two` runs green (closes the B↔C loop — first real-weights validation).

## PR body must call out
- **Two-edit rule applied** (5 new Tauri commands in both files); DMZ file `tauri_commands.rs` touched → needs the two-session review.
- **`sysinfo` `disk` feature added** for the disk-space guard.
- **bge embedder refactored onto the shared downloader** (Task 6) — load-bearing path; gated by bge tests; revertable as its own commit if a regression surfaces.
- **Known gaps:** `expected_size`/`sha256` are `None` in v1 (size/checksum pinning is a follow-up once exact upstream sizes are known — verification still catches truncated/partial files via the size check when populated); ModelScope repo path/revision + per-quant filenames assumed from HF naming (probe falls through to a reachable host if a host 404s). No real-network download exercised in CI (integration is gated/manual).
- **Commits (bisectable) table:** one row per Task 1–6.

## Self-review notes (plan-authoring time)
- **Spec coverage:** manifest core ✓ (T1), ranged-GET probe behind trait ✓ (T2), download with verify/atomic/retry/cancel/progress ✓ (T3), ModelManager + cache path contract to B + quant map ✓ (T4), 5 Tauri commands + event + disk guard ✓ (T5), bge generalize no-regression ✓ (T6). Cache-path contract honored via reuse of Slice B's `model_dir`/`GGUF_FILENAME`/`TOKENIZER_FILENAME`.
- **Type consistency:** `Host`/`FileSource`/`ManifestFile`/`ModelManifest`/`Progress` flow from `model_fetch` through `model_manager` into the commands; `Quant`/`InstalledModel`/`ProbedSource` consistent across Rust + TS bridge.
- **Open implementer confirmations (flagged inline):** `sysinfo` disk feature + `Disks` API shape (T5); ModelScope repo/revision/filenames (T4 note); bge URL byte-equality (T6 verify step); `async-trait` presence (T2); `use tauri::Emitter` in scope (T5).
</content>
</invoke>
