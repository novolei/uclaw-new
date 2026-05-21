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

    // Absolute paths: try to match against a mount root so the auto-preview
    // listener can build a PreviewFileTarget with mountId + relPath. Falls
    // back to a mountless resolution (existing behavior) if no mount contains
    // the path.
    if bare.starts_with('/') {
        let p = Path::new(bare);
        let exists = fs::metadata(p).map(|m| m.is_file()).unwrap_or(false);
        let abs_canonical = if exists {
            p.canonicalize().ok().map(|c| c.to_string_lossy().into_owned())
        } else {
            // For not-yet-existing paths (write_file before its tool_result),
            // canonicalize fails. Use the input path verbatim so callers still
            // get an absolute string to work with.
            Some(bare.to_string())
        };

        // Try mount matching only for paths that look like they could be
        // under a mount root. We compare against both the raw mount path and
        // its canonical form to handle symlinked workspace roots.
        let mounts = match state.files_rail_list_mounts(session_id.clone()).await {
            Ok(m) => m,
            Err(_) => Vec::new(),
        };
        for mount in &mounts {
            let mount_path = &mount.path;
            let candidates: [Option<std::path::PathBuf>; 2] = [
                Some(mount_path.clone()),
                mount_path.canonicalize().ok(),
            ];
            for cand in candidates.iter().flatten() {
                if let Ok(rel) = p.strip_prefix(cand) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    if rel_str.is_empty() {
                        continue;
                    }
                    return ChipResolution {
                        input: raw.to_string(),
                        exists,
                        mount_id: Some(mount.id.clone()),
                        rel_path: Some(rel_str),
                        absolute_path: abs_canonical,
                    };
                }
            }
        }

        return ChipResolution {
            input: raw.to_string(),
            exists,
            mount_id: None,
            rel_path: None,
            absolute_path: if exists { abs_canonical } else { None },
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

    let mounts = match state.files_rail_list_mounts(session_id.clone()).await {
        Ok(m) => m,
        Err(_) => Vec::new(),
    };
    for mount in &mounts {
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

    // Fallback chain for relative paths the LLM wrote into a non-mount
    // directory (e.g. .memory.md in ~/.uclaw, dropped via the agent's
    // own `write_file` to a path outside the workspace). Without this
    // the chip strikes through and the user thinks the file was lost.
    //
    // Each fallback returns the absolute path only — mount_id stays
    // None so the preview tab opens in "ad-hoc absolute" mode, which
    // the renderer already handles.
    for fallback in fallback_dirs(state, session_id.as_deref()) {
        let candidate = fallback.join(bare);
        if let Ok(meta) = fs::metadata(&candidate) {
            if meta.is_file() {
                let abs = candidate
                    .canonicalize()
                    .ok()
                    .map(|c| c.to_string_lossy().into_owned())
                    .unwrap_or_else(|| candidate.to_string_lossy().into_owned());
                return ChipResolution {
                    input: raw.to_string(),
                    exists: true,
                    // No mount_id — the preview tab handles this as an
                    // absolute-path resource, which doesn't need a mount
                    // for read-only display.
                    mount_id: None,
                    rel_path: None,
                    absolute_path: Some(abs),
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

/// Directories the chip resolver tries AFTER the mount catalog comes
/// up empty. Ordering matters — most-specific first (session workspace)
/// → broad (home dir).
///
/// Order:
///   1. Session's space.path from the DB (the workspace the agent loop
///      considers "cwd"). When the agent uses a relative `write_file`
///      this is the actual parent directory.
///   2. ~/.uclaw — uclaw's persistent data dir; agent skills, memory,
///      and rollout sidecar files all live under here.
///   3. ~ (home) — last-resort for paths the agent dropped to the user
///      home (e.g. ~/.bashrc edits triggered by a skill).
fn fallback_dirs(state: &AppState, session_id: Option<&str>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::with_capacity(3);

    if let Some(sid) = session_id {
        if let Some(ws) = session_space_path_blocking(state, sid) {
            out.push(ws);
        }
    }

    // ~/.uclaw — use the dedicated home-resolution crate so we honor
    // whatever override the user's UCLAW_HOME env var is set to.
    if let Ok(uclaw_home) = uclaw_utils_home::uclaw_home_pathbuf() {
        out.push(uclaw_home);
    }

    if let Some(home) = dirs::home_dir() {
        out.push(home);
    }

    out
}

/// Synchronous DB lookup of `spaces.path` for the session.
///
/// Mirrors `tauri_commands::session_workspace_root` but lives here so
/// the resolver can stay in the preview crate without an upward
/// dependency on the Tauri command surface.
fn session_space_path_blocking(state: &AppState, session_id: &str) -> Option<PathBuf> {
    let conn = state.db.lock().ok()?;
    let space_id: String = conn
        .query_row(
            "SELECT space_id FROM agent_sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get::<_, String>(0),
        )
        .ok()?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT path FROM spaces WHERE id = ?1",
            rusqlite::params![space_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    raw.filter(|s| !s.trim().is_empty()).map(PathBuf::from)
}

/// Write `content` to `path` using direct write + fsync.
///
/// Name retained as `write_atomic` for compatibility with existing callers,
/// but the implementation is now direct write+fsync. The previous
/// tempfile+rename pattern triggered a panic in notify-rs's macOS kqueue
/// backend: replacing a watched inode via rename caused kqueue to lose state
/// and call Option::unwrap() on None inside kqueue-1.1.1. The panic killed
/// the watcher thread, leaving the files rail unable to auto-refresh until a
/// manual click. Direct write keeps the same inode; the file watcher fires a
/// single Modify event without panicking.
///
/// Atomicity tradeoff: a crash mid-write could leave a partial file. For
/// editor-saves with small content, this is far less impactful than the
/// file-watcher panic.
///
/// 50 MB cap mirrors `MAX_PREVIEW_BYTES` — reject larger writes upfront.
pub fn write_atomic(path: &Path, content: &str) -> Result<(i64, u64), Error> {
    use std::fs::File;
    use std::io::Write;

    if content.len() as u64 > MAX_PREVIEW_BYTES {
        return Err(Error::InvalidInput(format!(
            "content exceeds {} bytes cap",
            MAX_PREVIEW_BYTES
        )));
    }

    let mut f = File::create(path)
        .map_err(|e| Error::Internal(format!("create: {}", e)))?;
    f.write_all(content.as_bytes())
        .map_err(|e| Error::Internal(format!("write: {}", e)))?;
    f.sync_all()
        .map_err(|e| Error::Internal(format!("fsync: {}", e)))?;

    let metadata = f
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
