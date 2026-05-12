//! Data types for the files-rail subsystem.
//!
//! Wire format for the Tauri commands and the `files_rail:change` IPC channel.
//! All types are `Serialize` (out) and `Deserialize` (in for command inputs).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    /// Absolute path on disk.
    pub path: PathBuf,
    /// Path relative to the mount root (forward slashes regardless of OS).
    pub rel_path: String,
    /// Display name (last segment of path).
    pub name: String,
    pub kind: NodeKind,
    /// Size in bytes; 0 for directories.
    pub size: u64,
    /// Modification time, milliseconds since epoch.
    pub mtime_ms: i64,
    /// True if the node was filtered by ignore rules. Currently only used in
    /// changes-tab rendering — directory walks already drop ignored entries.
    pub is_ignored: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MountKind {
    /// `~/Documents/workground/<workspace>` — the workspace root.
    Workspace,
    /// `~/Documents/workground/<workspace>/<session>` — per-session subdir.
    Session,
    /// A directory the user attached via `attach_session_directory` etc.
    AttachedDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountRoot {
    /// Stable id used as the key in atom families and the watcher registry.
    /// Format: "workspace:<workspace_id>" / "session:<session_id>" /
    /// "attached:<sha1(path)>".
    pub id: String,
    /// Human-visible label (e.g. workspace name, "会话文件", attached dir basename).
    pub label: String,
    pub path: PathBuf,
    pub kind: MountKind,
    /// True if the user may rename / delete / write through W4's editors.
    /// AttachedDirs default to read-only; W4 adds an opt-in toggle.
    pub editable: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
    /// On macOS, notify often reports rename as a Created/Removed pair within
    /// the debounce window — `coalesce_pairs()` in watcher.rs merges them.
    Renamed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub kind: ChangeKind,
    pub rel_path: String,
    /// Only set for `Renamed`. The new relative path.
    pub new_rel_path: Option<String>,
    pub is_dir: bool,
}

/// Payload of the `files_rail:change` IPC event.
/// Batched: each emit contains up to 100 changes accumulated over a 16ms
/// debounce window per mount.
#[derive(Debug, Clone, Serialize)]
pub struct FilesRailChange {
    pub mount_id: String,
    pub changes: Vec<FileChange>,
}

impl FilesRailChange {
    pub const CHANNEL: &'static str = "files_rail:change";
}
