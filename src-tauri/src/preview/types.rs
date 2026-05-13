//! Data types for the preview subsystem.
//!
//! Wire format for the `preview_read_bytes` Tauri command. Bytes flow up
//! to the frontend as a base64-encoded string (Tauri's default for Vec<u8>);
//! the frontend's `useFileBytes` decodes to a Uint8Array.

use serde::Serialize;
use std::path::PathBuf;

/// 50 MB hard cap. Files larger than this are truncated at this boundary
/// and `truncated: true` is set so the renderer can show a banner.
pub const MAX_PREVIEW_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct PreviewBytes {
    /// Resolved absolute path on disk (after mount-relative lookup).
    pub resolved_path: PathBuf,
    /// File contents, truncated to `MAX_PREVIEW_BYTES` if larger.
    pub bytes: Vec<u8>,
    /// Original file size in bytes (NOT the length of `bytes` — that may be capped).
    pub size: u64,
    /// True if `bytes` is a truncated prefix.
    pub truncated: bool,
    /// Modification time, milliseconds since epoch.
    pub mtime_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChipResolution {
    /// The raw input string, unchanged (so the frontend can key its cache).
    pub input: String,
    /// `true` if the resolved path exists and is a regular file.
    pub exists: bool,
    /// Mount id when the path resolved through a mount. `None` for absolute paths.
    pub mount_id: Option<String>,
    /// Path inside the mount (forward-slash). `None` for absolute paths or misses.
    pub rel_path: Option<String>,
    /// Canonicalised absolute path when resolved; `None` otherwise.
    pub absolute_path: Option<String>,
}

/// Outcome of a `preview_write_text` invocation.
///
/// The frontend tracks dirty state and guards external-refresh against it,
/// so the backend no longer reports `Conflict`. Writes always succeed
/// unless the mount is non-editable, in which case approval is requested.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum WriteResult {
    /// Write succeeded.
    Saved {
        mtime_ms: i64,
        size: u64,
    },
    /// Write is gated by `SafetyManager`-style approval. Frontend opens
    /// `<WriteApprovalDialog>`, awaits Allow/Deny, then calls
    /// `approve_preview_write(approval_id, allowed)` to resolve.
    NeedsApproval { approval_id: String },
}
