//! Sprint 2.4 — gbrain-specific subsystems that aren't part of the MCP
//! protocol layer (which lives in `crate::mcp`). Today this is just the
//! chat extractor; future Sprint 2.5+ work (size optimization, content
//! ingestion path) will live alongside it.

pub mod chat_extractor;
pub mod browse;
pub mod scoped;
pub mod cli_format;
