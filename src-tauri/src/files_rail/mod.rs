//! W3: workspace file rail with notify-driven live refresh.
//!
//! Spec: `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §5

pub mod types;

pub use types::{
    ChangeKind, FileChange, FileNode, FilesRailChange, MountKind, MountRoot, NodeKind,
};
