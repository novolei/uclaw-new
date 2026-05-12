//! Tauri commands for the preview UI.

use super::resolver::{read_capped, resolve_path};
use super::types::PreviewBytes;
use crate::app::AppState;
use crate::error::Error;
use tauri::State;

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
