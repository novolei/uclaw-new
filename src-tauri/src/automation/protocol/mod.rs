//! Humane protocol layer — parses, validates, and normalises spec.yaml files.

pub mod humane_v1;
pub mod migrate_toml_v1;
pub mod normalize;
pub mod parse;

// Public re-exports
pub use humane_v1::HumaneAutomationSpec;
pub use normalize::normalize_to_json;
pub use parse::{ParseError, ParsedSpec, parse_humane_v1};
