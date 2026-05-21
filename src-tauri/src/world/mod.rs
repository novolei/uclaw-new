//! M4-T1 — World projection types (pilot).
//!
//! ADR §"World Projection" describes the agent's read-only model of
//! the external world — every entity uClaw has observed (file, GitHub
//! issue, Slack channel, Excel sheet, browser tab) gets a typed
//! `WorldEntity` with a stable id and a snapshot of state. Tasks
//! consult the projection at start time (to decide what's safe to
//! act on) and update it after every observed change.
//!
//! This pilot ships the **read-only entity + snapshot types**. The
//! projection store (M4-T2), subscriber pattern (M4-T3), and the
//! adapters that translate connector events into projection updates
//! (M4-T4-T8) live in follow-up PRs.
//!
//! Layout:
//!
//! - [`entity`] — `WorldEntity` + `WorldEntityKind` + supporting refs
//! - [`snapshot`] — `WorldSnapshot` + `ProjectionStats`

pub mod adapters;
pub mod entity;
pub mod snapshot;
pub mod store;

// Consolidated re-exports from all M4 adapters (#354 #356 #359 #360 #361 #362).
// Note: #356 (M4-T4 git) doesn't re-export from adapters::* but defines
// its symbols at adapters::git::*; they're accessible via the path.
pub use adapters::{
    branch_to_entity, calendar_event_to_entity, channel_to_entity, commit_to_entity,
    dataset_to_entity, document_to_entity, email_to_entity, parse_branch_listing,
    parse_log_one_line, parse_status_porcelain, scan_directory, tab_entity,
    wtchange_to_entity, BrowserAdapter, BrowserTabEvent, CalendarAdapter,
    CalendarChangeEvent, DatasetAdapter, DatasetEvent, DocEvent, DocumentAdapter,
    EmailEvent, FileSystemAdapter, GitBranch, GitCommit, GitWorkTreeChange,
    MailAdapter, ScanOptions, ScanResult, SlackAdapter, SlackEvent,
};
pub use entity::{EntityRef, WorldEntity, WorldEntityKind, WorldEntityState};
pub use snapshot::{ProjectionStats, WorldSnapshot};
pub use store::{
    DuplicateSubscriberId, ProjectionEvent, ProjectionStore, ProjectionSubscriber,
    ProjectionSubscriberId,
};
