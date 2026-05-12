//! W3: workspace file rail with notify-driven live refresh.
//!
//! Spec: `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §5

pub mod ignore;
pub mod types;
pub mod walker;
pub mod watcher;

pub use types::{
    ChangeKind, FileChange, FileNode, FilesRailChange, MountKind, MountRoot, NodeKind,
};
pub use watcher::FilesRailWatcher;
