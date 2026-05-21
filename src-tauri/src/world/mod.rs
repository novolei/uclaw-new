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

pub use adapters::{tab_entity, BrowserAdapter, BrowserTabEvent};
pub use entity::{EntityRef, WorldEntity, WorldEntityKind, WorldEntityState};
pub use snapshot::{ProjectionStats, WorldSnapshot};
pub use store::{
    DuplicateSubscriberId, ProjectionEvent, ProjectionStore, ProjectionSubscriber,
    ProjectionSubscriberId,
};
