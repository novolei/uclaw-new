//! Tauri commands for the preview UI.

use super::approval::{request_write_approval, resolve_write_approval};
use super::resolver::{read_capped, resolve_chip_candidate, resolve_path, write_atomic};
use super::types::{ChipResolution, PreviewBytes, WriteResult, MAX_PREVIEW_BYTES};
use crate::app::AppState;
use crate::error::Error;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;
use tauri::State;
use tokio::sync::Mutex as TokioMutex;

/// Per-target-path async mutex map used to serialise `preview_write_text`
/// invocations against the same file. Without this lock, two concurrent
/// auto-saves race: save A's `File::create` truncates the file while
/// save B is reading metadata + content for its conflict response, so
/// save B observed an empty file and reported `current_content: ""` to
/// the frontend (which then surfaced an empty diff and corrupted the
/// editor when the user clicked "discard mine"). Lock granularity is
/// per resolved target path so unrelated files never block each other.
static WRITE_LOCKS: OnceLock<TokioMutex<HashMap<PathBuf, Arc<TokioMutex<()>>>>> = OnceLock::new();

async fn acquire_write_lock(path: &Path) -> Arc<TokioMutex<()>> {
    let map = WRITE_LOCKS.get_or_init(|| TokioMutex::new(HashMap::new()));
    let mut guard = map.lock().await;
    guard
        .entry(path.to_path_buf())
        .or_insert_with(|| Arc::new(TokioMutex::new(())))
        .clone()
}

#[tauri::command]
pub async fn preview_read_bytes(
    state: State<'_, AppState>,
    mount_id: String,
    rel_path: String,
    session_id: Option<String>,
) -> Result<PreviewBytes, Error> {
    let target = resolve_path(&state, &mount_id, &rel_path, session_id).await?;
    let bytes = read_capped(&target)?;
    Ok(bytes)
}

#[tauri::command]
pub async fn preview_resolve_chips(
    state: State<'_, AppState>,
    paths: Vec<String>,
    session_id: Option<String>,
) -> Result<Vec<ChipResolution>, Error> {
    // Cap input length to prevent abuse — a normal chat message has ≪ 100 chips.
    const MAX_PATHS: usize = 256;
    let mut out = Vec::with_capacity(paths.len().min(MAX_PATHS));
    for raw in paths.into_iter().take(MAX_PATHS) {
        out.push(resolve_chip_candidate(&state, &raw, session_id.clone()).await);
    }
    Ok(out)
}

/// `expected_mtime_ms` sentinel: file does not exist yet (caller wants
/// to create it).
const NEW_FILE_MTIME_SENTINEL: i64 = -1;
/// `expected_mtime_ms` sentinel: skip the optimistic-concurrency check
/// and overwrite whatever's on disk. Used by the conflict banner's
/// 覆盖 button — the user has explicitly chosen "I don't care what's
/// on disk now, write my version".
const FORCE_OVERWRITE_SENTINEL: i64 = -2;

#[tauri::command]
pub async fn preview_write_text(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    mount_id: String,
    rel_path: String,
    session_id: Option<String>,
    content: String,
    expected_mtime_ms: i64,
) -> Result<WriteResult, Error> {
    // 1. Resolve path through the W3-aware resolver (handles user-customized
    //    workspace paths via session_id threading).
    let target = resolve_path(&state, &mount_id, &rel_path, session_id.clone()).await?;

    // 2. Determine if the mount is editable.
    let mounts = state.files_rail_list_mounts(session_id).await?;
    let mount = mounts
        .into_iter()
        .find(|m| m.id == mount_id)
        .ok_or_else(|| Error::Internal(format!("mount not found: {}", mount_id)))?;

    if !mount.editable {
        // 3. Non-editable mount → request approval before proceeding.
        let allowed = request_write_approval(
            &state,
            &app_handle,
            &target,
            &format!("Write to read-only mount '{}'", mount.label),
        )
        .await?;
        if !allowed {
            return Err(Error::Internal(format!(
                "write to '{}' was denied by user",
                target.display()
            )));
        }
        // Approved — proceed to step 4.
    }

    // Acquire the per-file write lock. From here through write_atomic
    // we hold it, so concurrent saves against the same path serialize:
    // no save observes another save mid-truncate, and the conflict-read
    // path (below) always sees a complete file.
    let path_lock = acquire_write_lock(&target).await;
    let _write_guard = path_lock.lock().await;

    // Force-overwrite sentinel: skip the mtime check entirely. Used by
    // the conflict banner's 覆盖 action.
    if expected_mtime_ms == FORCE_OVERWRITE_SENTINEL {
        // Ensure the parent dir exists (rare, but possible if the file
        // was deleted while the conflict banner was open).
        if let Some(parent) = target.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(Error::Io)?;
            }
        }
        let (mtime_ms, size) = write_atomic(&target, &content)?;
        return Ok(WriteResult::Saved { mtime_ms, size });
    }

    // 4. Check existing mtime against expected (optimistic concurrency).
    let existing_mtime = match fs::metadata(&target) {
        Ok(meta) => meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File doesn't exist yet. Only allow if caller signals "new file"
            // via expected_mtime_ms == -1.
            if expected_mtime_ms != NEW_FILE_MTIME_SENTINEL {
                return Err(Error::Internal(format!(
                    "file does not exist and expected_mtime_ms != -1: {}",
                    target.display()
                )));
            }
            // Proceed to write.
            let (mtime_ms, size) = write_atomic(&target, &content)?;
            return Ok(WriteResult::Saved { mtime_ms, size });
        }
        Err(e) => {
            return Err(Error::Internal(format!(
                "metadata for '{}': {}",
                target.display(),
                e
            )));
        }
    };

    if existing_mtime != expected_mtime_ms {
        // Conflict — return current content for the banner's "View diff".
        // Reading INSIDE the lock guarantees we see a fully-flushed file
        // (no mid-truncate race vs another save).
        let mut current = String::new();
        let f = fs::File::open(&target)
            .map_err(|e| Error::Internal(format!("conflict read open: {}", e)))?;
        // Cap at MAX_PREVIEW_BYTES — same as preview_read_bytes.
        f.take(MAX_PREVIEW_BYTES)
            .read_to_string(&mut current)
            .map_err(|e| Error::Internal(format!("conflict read: {}", e)))?;
        return Ok(WriteResult::Conflict {
            current_mtime_ms: existing_mtime,
            current_content: current,
        });
    }

    // 5. mtime matches — atomic write.
    let (mtime_ms, size) = write_atomic(&target, &content)?;
    Ok(WriteResult::Saved { mtime_ms, size })
}

#[tauri::command]
pub async fn approve_preview_write(
    state: State<'_, AppState>,
    approval_id: String,
    allowed: bool,
) -> Result<bool, Error> {
    Ok(resolve_write_approval(&state, &approval_id, allowed))
}
