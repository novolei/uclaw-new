//! Humane protocol layer — parses, validates, and normalises spec.yaml files.

pub mod humane_v1;
pub mod migrate_toml_v1;
pub mod normalize;
pub mod parse;

// Public re-exports populated in subsequent tasks:
//   pub use humane_v1::HumaneAutomationSpec;
//   pub use parse::{ParseError, parse_humane_v1};
