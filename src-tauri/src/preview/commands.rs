//! Tauri commands for the preview UI.

use super::resolver::{read_capped, resolve_chip_candidate, resolve_path};
use super::types::{ChipResolution, PreviewBytes};
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
