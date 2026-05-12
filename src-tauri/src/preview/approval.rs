//! Write-approval flow for preview_write_text.
//!
//! When a write targets a path OUTSIDE every editable mount (workspace
//! mounts default editable=true; attached_dirs default false), the
//! command can't silently allow OR silently reject — both are bad UX.
//! Instead it queues a PendingApproval, emits a Tauri event, and awaits
//! a oneshot resolution from approve_preview_write.
//!
//! The frontend's WriteApprovalDialog consumes the event, presents
//! Allow/Deny, and dispatches the resolution.

use crate::app::{AppState, ApprovalResult};
use crate::error::Error;
use serde::Serialize;
use std::path::Path;
use tauri::Emitter;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteApprovalRequestPayload {
    pub approval_id: String,
    pub path: String,
    pub reason: String,
}

/// Queue an approval request, emit the event, and await the user's
/// decision. Returns `Ok(true)` when allowed, `Ok(false)` when denied.
///
/// `reason` is rendered verbatim in the dialog. Keep it short and
/// user-facing (e.g. "Write outside editable mounts").
pub async fn request_write_approval(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    abs_path: &Path,
    reason: &str,
) -> Result<bool, Error> {
    let approval_id = format!("preview-write-{}", Uuid::new_v4());

    let rx = state.pending_approvals.register(approval_id.clone());

    let payload = WriteApprovalRequestPayload {
        approval_id: approval_id.clone(),
        path: abs_path.display().to_string(),
        reason: reason.to_string(),
    };

    app_handle
        .emit("preview:write_approval_request", &payload)
        .map_err(|e| Error::Internal(format!("emit approval event: {}", e)))?;

    match rx.await {
        Ok(result) => Ok(result.approved),
        Err(_) => {
            // Channel closed without resolution — treat as deny.
            tracing::warn!(
                approval_id = %approval_id,
                "write approval channel closed without resolution; treating as deny"
            );
            Ok(false)
        }
    }
}

/// Resolve a pending write approval (called by approve_preview_write).
/// Returns `true` if the approval was found and resolved.
pub fn resolve_write_approval(
    state: &AppState,
    approval_id: &str,
    allowed: bool,
) -> bool {
    state.pending_approvals.resolve(
        approval_id,
        ApprovalResult {
            approved: allowed,
            always_allow: false,
            tool_name: None,
            path_scope: None,
            paths: None,
        },
    )
}
