//! Tauri commands for the files-rail UI.

use super::types::{FileNode, MountRoot};
use super::walker::read_dir_layer;
use crate::app::AppState;
use crate::error::Error;
use tauri::State;

#[tauri::command]
pub async fn files_rail_list_mounts(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<Vec<MountRoot>, Error> {
    state.files_rail_list_mounts(session_id).await
}

#[tauri::command]
pub async fn files_rail_read_dir(
    state: State<'_, AppState>,
    mount_id: String,
    rel_path: String,
) -> Result<Vec<FileNode>, Error> {
    let (mount_root, target) = state.files_rail_resolve_dir(&mount_id, &rel_path).await?;
    let entries = read_dir_layer(&target, &mount_root)
        .map_err(|e| Error::Internal(format!("read_dir failed: {}", e)))?;
    Ok(entries)
}

#[tauri::command]
pub async fn files_rail_watch_start(
    state: State<'_, AppState>,
    mount_id: String,
) -> Result<(), Error> {
    let root = state.files_rail_mount_path(&mount_id).await?;
    state
        .files_rail_service
        .register_mount(mount_id, root)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn files_rail_watch_stop(
    state: State<'_, AppState>,
    mount_id: String,
) -> Result<(), Error> {
    state
        .files_rail_service
        .unregister_mount(&mount_id)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(())
}
