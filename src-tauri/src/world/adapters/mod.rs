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
