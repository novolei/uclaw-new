//! Resolve a `(mount_id, rel_path)` pair into a concrete absolute path.
//!
//! Reuses `AppState::files_rail_list_mounts` to fetch the mount catalog,
//! then composes the absolute path with `..` and absolute-path guards.

use super::types::{PreviewBytes, MAX_PREVIEW_BYTES};
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

    let target = if rel_path.is_empty() || rel_path == "/" {
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
    if !path.exists() {
        return Err(Error::NotFound(format!("file not found: {}", path.display())));
    }
    let metadata = fs::metadata(path)
        .map_err(|e| Error::Internal(format!("metadata: {}", e)))?;
    if !metadata.is_file() {
        return Err(Error::InvalidInput(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    let size = metadata.len();
    let truncated = size > MAX_PREVIEW_BYTES;
    let to_read = if truncated { MAX_PREVIEW_BYTES } else { size };

    let mut file = fs::File::open(path).map_err(|e| Error::Internal(format!("open: {}", e)))?;
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
