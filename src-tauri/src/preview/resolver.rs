//! Resolve a `(mount_id, rel_path)` pair into a concrete absolute path.
//!
//! Reuses `AppState::files_rail_list_mounts` to fetch the mount catalog,
//! then composes the absolute path with `..` and absolute-path guards.

use super::types::{ChipResolution, PreviewBytes, MAX_PREVIEW_BYTES};
use crate::app::AppState;
use crate::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Resolve `(mount_id, rel_path)` to a concrete absolute path.
///
/// `session_id` is optional; passed through to `files_rail_list_mounts`
/// so workspace mounts scoped to a non-default space resolve correctly
/// (same threading as W3 `files_rail_read_dir`).
pub async fn resolve_path(
    state: &AppState,
    mount_id: &str,
    rel_path: &str,
    session_id: Option<String>,
) -> Result<PathBuf, Error> {
    // Reject absolute and traversal segments BEFORE consulting mounts so a
    // malformed input fails fast.
    if rel_path.starts_with('/') {
        return Err(Error::InvalidInput(
            "rel_path must be relative".into(),
        ));
    }
    if rel_path.split('/').any(|seg| seg == "..") {
        return Err(Error::InvalidInput(
            "rel_path must not contain '..' segments".into(),
        ));
    }

    let mounts = state.files_rail_list_mounts(session_id).await?;
    let mount = mounts
        .into_iter()
        .find(|m| m.id == mount_id)
        .ok_or_else(|| Error::Internal(format!("mount not found: {}", mount_id)))?;

    // Empty rel_path = mount root. We don't need to handle `"/"` separately
    // because the absolute-path guard above (line 27) already rejected it.
    let target = if rel_path.is_empty() {
        mount.path.clone()
    } else {
        mount.path.join(rel_path)
    };

    // Defense-in-depth: even after path traversal guard, ensure final
    // canonicalised path stays under the mount root. `canonicalize` requires
    // the file to exist; for read operations this is correct.
    if let (Ok(canon_target), Ok(canon_root)) = (target.canonicalize(), mount.path.canonicalize()) {
        if !canon_target.starts_with(&canon_root) {
            return Err(Error::InvalidInput(format!(
                "resolved path escapes mount: {}",
                target.display()
            )));
        }
    }

    Ok(target)
}

/// Read up to `MAX_PREVIEW_BYTES` from `path`. Returns `PreviewBytes` with
/// `truncated = true` when the file exceeds the cap.
pub fn read_capped(path: &Path) -> Result<PreviewBytes, Error> {
    // Single stat call: map ENOENT to NotFound, everything else to Internal.
    // Avoids the TOCTOU window of a separate `exists()` pre-check.
    let metadata = fs::metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::NotFound(format!("file not found: {}", path.display()))
        } else {
            Error::Internal(format!("metadata: {}", e))
        }
    })?;
    if !metadata.is_file() {
        return Err(Error::InvalidInput(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    let size = metadata.len();
    let truncated = size > MAX_PREVIEW_BYTES;
    let to_read = if truncated { MAX_PREVIEW_BYTES } else { size };

    let file = fs::File::open(path).map_err(|e| Error::Internal(format!("open: {}", e)))?;
    let mut bytes = Vec::with_capacity(to_read as usize);
    file.take(to_read)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Internal(format!("read: {}", e)))?;

    let mtime_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    Ok(PreviewBytes {
        resolved_path: path.to_path_buf(),
        bytes,
        size,
        truncated,
        mtime_ms,
    })
}

/// Strip an optional `:line:col` suffix from a chip-candidate input.
/// Returns `(bare_path, line, col)`. Mirrors `parseLineCol` in TS.
///
/// Marked `pub(super)` so the test module in `tests.rs` can call it.
pub(super) fn strip_line_col(input: &str) -> (&str, Option<u32>, Option<u32>) {
    // Try to peel one or two trailing `:N` segments where N >= 1.
    let mut tail_path = input;
    let mut first: Option<u32> = None;
    let mut second: Option<u32> = None;

    if let Some((head, tail)) = tail_path.rsplit_once(':') {
        if let Ok(n) = tail.parse::<u32>() {
            if n >= 1 {
                first = Some(n);
                tail_path = head;
            }
        }
    }
    if first.is_some() {
        if let Some((head, tail)) = tail_path.rsplit_once(':') {
            if let Ok(n) = tail.parse::<u32>() {
                if n >= 1 {
                    second = Some(n);
                    tail_path = head;
                }
            }
        }
    }

    // When we peeled exactly one number, it's the line.
    // When we peeled two, the first peel was the col and the second was the line.
    match (second, first) {
        (Some(line), Some(col)) => (tail_path, Some(line), Some(col)),
        (None, Some(line)) => (tail_path, Some(line), None),
        _ => (input, None, None),
    }
}

/// Resolve a single chip candidate against mounts + absolute-path fallback.
///
/// Returns a fully-populated `ChipResolution`. Never errors — malformed or
/// missing inputs yield `exists: false`.
pub async fn resolve_chip_candidate(
    state: &AppState,
    raw: &str,
    session_id: Option<String>,
) -> ChipResolution {
    let (bare, _line, _col) = strip_line_col(raw);

    // Absolute paths take the express lane.
    if bare.starts_with('/') {
        let p = Path::new(bare);
        let exists = fs::metadata(p).map(|m| m.is_file()).unwrap_or(false);
        return ChipResolution {
            input: raw.to_string(),
            exists,
            mount_id: None,
            rel_path: None,
            absolute_path: if exists {
                p.canonicalize().ok().map(|c| c.to_string_lossy().into_owned())
            } else {
                None
            },
        };
    }

    // Relative path — reject `..` segments defensively.
    if bare.split('/').any(|seg| seg == "..") {
        return ChipResolution {
            input: raw.to_string(),
            exists: false,
            mount_id: None,
            rel_path: None,
            absolute_path: None,
        };
    }

    let mounts = match state.files_rail_list_mounts(session_id).await {
        Ok(m) => m,
        Err(_) => Vec::new(),
    };
    for mount in mounts {
        let candidate = mount.path.join(bare);
        if let Ok(meta) = fs::metadata(&candidate) {
            if meta.is_file() {
                let abs = candidate
                    .canonicalize()
                    .ok()
                    .map(|c| c.to_string_lossy().into_owned());
                return ChipResolution {
                    input: raw.to_string(),
                    exists: true,
                    mount_id: Some(mount.id.clone()),
                    rel_path: Some(bare.to_string()),
                    absolute_path: abs,
                };
            }
        }
    }
    ChipResolution {
        input: raw.to_string(),
        exists: false,
        mount_id: None,
        rel_path: None,
        absolute_path: None,
    }
}

/// Atomically write `content` to `path` using the rename-tempfile pattern.
///
/// Steps:
/// 1. Create a tempfile in the SAME directory as `path` (so rename is
///    atomic on POSIX — cross-filesystem rename would fail).
/// 2. Write `content` to the tempfile + fsync.
/// 3. `fs::rename(tempfile, path)` — atomic replacement.
/// 4. `stat` the result to verify size matches `content.len()`.
/// 5. Return `(mtime_ms, size)`.
///
/// On cross-filesystem rename failure (EXDEV), falls back to direct
/// write-then-fsync (less atomic but reliable).
///
/// 50 MB cap mirrors `MAX_PREVIEW_BYTES` — reject larger writes upfront.
pub fn write_atomic(path: &Path, content: &str) -> Result<(i64, u64), Error> {
    use std::fs::File;
    use std::io::Write;

    let dir = path.parent().ok_or_else(|| {
        Error::InvalidInput(format!("path has no parent dir: {}", path.display()))
    })?;

    if content.len() as u64 > MAX_PREVIEW_BYTES {
        return Err(Error::InvalidInput(format!(
            "content exceeds {} bytes cap",
            MAX_PREVIEW_BYTES
        )));
    }

    let tmp = tempfile::Builder::new()
        .prefix(".uclaw-preview-write-")
        .tempfile_in(dir)
        .map_err(|e| Error::Internal(format!("tempfile create: {}", e)))?;

    {
        let mut f = tmp.as_file();
        f.write_all(content.as_bytes())
            .map_err(|e| Error::Internal(format!("tempfile write: {}", e)))?;
        f.sync_all()
            .map_err(|e| Error::Internal(format!("tempfile fsync: {}", e)))?;
    }

    // Atomic rename. On cross-filesystem failure, fall back.
    let persisted = match tmp.persist(path) {
        Ok(f) => f,
        Err(_persist_err) => {
            // Cross-filesystem? Write directly to the target.
            let mut f = File::create(path)
                .map_err(|err| Error::Internal(format!("fallback create: {}", err)))?;
            f.write_all(content.as_bytes())
                .map_err(|err| Error::Internal(format!("fallback write: {}", err)))?;
            f.sync_all()
                .map_err(|err| Error::Internal(format!("fallback fsync: {}", err)))?;
            // _persist_err's tempfile is dropped automatically (auto-cleaned).
            f
        }
    };

    let metadata = persisted
        .metadata()
        .map_err(|e| Error::Internal(format!("post-write stat: {}", e)))?;
    let size = metadata.len();
    if size != content.len() as u64 {
        return Err(Error::Internal(format!(
            "post-write size mismatch: expected {} got {}",
            content.len(),
            size
        )));
    }
    let mtime_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    Ok((mtime_ms, size))
}
