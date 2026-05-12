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
