//! Projection adapters.
//!
//! Each adapter translates external-system events into `WorldEntity`
//! upserts/tombstones on the `ProjectionStore` (M4-T2). Adapters are
//! independent of each other — installing the git adapter does not
//! require the slack adapter.
//!
//! M4 adapter inventory:
//!
//! - [`filesystem`] — file/dir tree → `File` entities (#354)
//! - [`git`] — `git log/branch/status` parsers → `GitObject` (#356)
//! - [`browser`] — Chrome MCP tab events → `BrowserPage` (#359)
//! - [`slack`] — Slack/IM channel events → `ChatThread` (#360)
//! - [`mail`] — email events → `Email` (#361)
//! - [`calendar`] — calendar events → `CalendarEvent` (#361)
//! - [`document`] — Google Doc/Notion/Office → `Document` (#362)
//! - [`dataset`] — DB tables/CSV → `Dataset` (#362)

pub mod browser;
pub mod calendar;
pub mod dataset;
pub mod document;
pub mod filesystem;
pub mod git;
pub mod mail;
pub mod slack;

pub use browser::{tab_entity, BrowserAdapter, BrowserTabEvent};
pub use calendar::{calendar_event_to_entity, CalendarAdapter, CalendarChangeEvent};
pub use dataset::{dataset_to_entity, DatasetAdapter, DatasetEvent};
pub use document::{document_to_entity, DocEvent, DocumentAdapter};
pub use filesystem::{scan_directory, FileSystemAdapter, ScanOptions, ScanResult};
pub use git::{
    branch_to_entity, commit_to_entity, parse_branch_listing, parse_log_one_line,
    parse_status_porcelain, wtchange_to_entity, GitBranch, GitCommit, GitWorkTreeChange,
};
pub use mail::{email_to_entity, EmailEvent, MailAdapter};
pub use slack::{channel_to_entity, SlackAdapter, SlackEvent};
