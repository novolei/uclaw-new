//! M2-H L2 — Tool exposure + schema shaping.
//!
//! Layer-2 of the 7-layer Token Defense. Where L1 truncates a tool's
//! *output*, L2 prunes a tool's *schema* before it ever reaches the
//! LLM:
//!
//! - **`ToolExposure`** decides whether a tool is announced to the LLM
//!   at all this turn (`Always` / `OnDemand` / `Hidden`). MCP servers
//!   that ship 40+ tools blow the system prompt budget — flipping
//!   most of them to `OnDemand` keeps only the agent-relevant ones
//!   visible per turn.
//! - **`normalize_tool_schema`** rewrites a JSON-schema tool definition
//!   to drop high-cost / low-signal sub-fields:
//!     - removes `description.examples` (LLMs rarely need full examples;
//!       a short description is enough)
//!     - merges adjacent `enum` arrays (`["a","b"]` + `["b","c"]` →
//!       `["a","b","c"]`) to dedupe overlap from MCP schema unions
//!     - prunes nested objects deeper than 3 levels, replacing them
//!       with `{"truncated": true, "original_depth": N}` so the LLM
//!       still knows "something exists here" without burning tokens
//!       on multi-level boilerplate
//!
//! This pilot ships the **policy + normalizer**. Wire-up into MCP
//! tool registration / dispatcher.rs schema-rendering lives in a
//! follow-up PR.
//!
//! Layout:
//!
//! - [`exposure`]  — `ToolExposure` + `ToolExposurePolicy`
//! - [`normalize`] — `normalize_tool_schema` + helpers

pub mod exposure;
pub mod normalize;

pub use exposure::{ToolExposure, ToolExposurePolicy};
pub use normalize::{normalize_tool_schema, NormalizeStats, DEFAULT_MAX_NESTING_DEPTH};
