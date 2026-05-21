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
