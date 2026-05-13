//! Tauri commands for the preview UI.

use super::approval::{request_write_approval, resolve_write_approval};
use super::resolver::{read_capped, resolve_chip_candidate, resolve_path, write_atomic};
use super::types::{ChipResolution, PreviewBytes, WriteResult};
use crate::app::AppState;
use crate::error::Error;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tauri::State;
use tokio::sync::Mutex as TokioMutex;

/// Per-target-path async mutex map. Serialises `preview_write_text`
/// invocations against the same file so concurrent auto-saves never
/// observe one another mid-truncate. Lock granularity is per resolved
/// target path — unrelated files never block each other.
///
/// Note: we no longer perform optimistic concurrency control here
/// (no `expected_mtime_ms`). The frontend tracks dirty state and
/// guards external-refresh against it; the backend writes unconditionally
/// (modulo the safety/approval gate for non-editable mounts). This
/// mirrors if2Ai's preview-panel design and eliminates the false-positive
/// "file changed on disk" warnings that the mtime check kept producing
/// against macOS's coarse mtime resolution + React's commit timing window.
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

#[tauri::command]
pub async fn preview_write_text(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    mount_id: String,
    rel_path: String,
    session_id: Option<String>,
    content: String,
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
        // Non-editable mount → request approval before proceeding.
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
    }

    // Acquire the per-file write lock — held through write_atomic so
    // concurrent saves to the same path serialize, never one mid-truncate.
    let path_lock = acquire_write_lock(&target).await;
    let _write_guard = path_lock.lock().await;

    // Ensure parent dir exists (first save to a brand-new file).
    if let Some(parent) = target.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(Error::Io)?;
        }
    }

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
