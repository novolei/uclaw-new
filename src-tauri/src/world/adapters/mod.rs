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
