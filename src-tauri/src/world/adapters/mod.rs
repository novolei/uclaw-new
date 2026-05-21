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
