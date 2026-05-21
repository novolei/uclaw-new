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
//! Projection adapters.
//!
//! Each adapter translates external-system events into
//! `WorldEntity` upserts/tombstones on the `ProjectionStore`. This
//! branch ships the **browser tab** adapter (M4-T5); the filesystem
//! and git adapters live in #354/#356 (separate prep branches that
//! can land in any order — adapters do not depend on each other).
//!
//! Layout:
//!
//! - [`browser`] — `BrowserTabEvent` + `BrowserAdapter`

pub mod browser;

pub use browser::{tab_entity, BrowserAdapter, BrowserTabEvent};
//! Projection adapters.
//!
//! Each adapter translates external-system events into `WorldEntity`
//! upserts/tombstones on the `ProjectionStore`. This branch ships the
//! **Slack/IM channel** adapter (M4-T6); the filesystem / git / browser
//! adapters live in #354/#356/#359 (separate prep branches that can
//! land in any order — adapters do not depend on each other).
//!
//! Layout:
//!
//! - [`slack`] — `SlackEvent` + `SlackAdapter`

pub mod slack;

pub use slack::{channel_to_entity, SlackAdapter, SlackEvent};
//! Projection adapters.
//!
//! Each adapter translates external-system events into `WorldEntity`
//! upserts/tombstones on the `ProjectionStore`. This branch ships
//! **mail + calendar** adapters (M4-T7); independent of #354/#356/
//! #359/#360 (siblings under world/adapters/).
//!
//! Layout:
//!
//! - [`mail`] — `EmailEvent` + `MailAdapter` (Email entity)
//! - [`calendar`] — `CalendarChangeEvent` + `CalendarAdapter`
//!   (CalendarEvent entity)

pub mod calendar;
pub mod mail;

pub use calendar::{
    calendar_event_to_entity, CalendarAdapter, CalendarChangeEvent,
};
pub use mail::{email_to_entity, EmailEvent, MailAdapter};
//! Projection adapters.
//!
//! This branch ships **document + dataset** adapters (M4-T8) —
//! the final M4 pilot, completing the World Projection type surface.
//!
//! Independent of #354/#356/#359/#360/#361 (siblings under
//! world/adapters/).
//!
//! Layout:
//!
//! - [`document`] — `DocEvent` + `DocumentAdapter` (Document entity)
//! - [`dataset`] — `DatasetEvent` + `DatasetAdapter` (Dataset entity)

pub mod dataset;
pub mod document;

pub use dataset::{
    dataset_to_entity, DatasetAdapter, DatasetEvent,
};
pub use document::{document_to_entity, DocEvent, DocumentAdapter};
