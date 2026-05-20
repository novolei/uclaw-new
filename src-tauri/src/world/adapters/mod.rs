//! M4-T3 — Projection adapters.
//!
//! Adapters translate external-system events (filesystem inotify,
//! git poll, slack webhook, ...) into `ProjectionEvent`s that the
//! `ProjectionStore` ingests. This pilot ships the **filesystem**
//! adapter — the simplest concrete example to validate the adapter
//! pattern + provide a base for later git/web/chat adapters
//! (M4-T4 onwards).
//!
//! The filesystem adapter is **scan-based** in this pilot — it walks
//! a root dir + produces snapshot entities. Live inotify integration
//! lives in M4-T3 commit 2 (uses `uclaw-utils-file-watcher` crate).
//!
//! Layout:
//!
//! - [`filesystem`] — `FileSystemAdapter` + `scan_directory`

pub mod filesystem;

pub use filesystem::{scan_directory, FileSystemAdapter, ScanOptions, ScanResult};
