//! W4a: preview engine — backend.
//!
//! Spec: `docs/superpowers/specs/2026-05-12-proma-preview-port-design.md` §6

pub mod approval;
pub mod commands;
pub mod resolver;
pub mod types;

#[cfg(test)]
mod tests;

pub use types::{PreviewBytes, MAX_PREVIEW_BYTES};
