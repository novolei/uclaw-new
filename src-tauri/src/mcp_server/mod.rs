//! M3-T9 — MCP server types (pilot).
//!
//! uClaw exposes a subset of its capabilities as an MCP server so
//! external agents (Cursor, Codex, peer uClaw instances) can call in.
//! The spec covers 4 initial tools:
//!
//! - `list_threads` — enumerate chat threads
//! - `read_thread` — fetch one thread's messages
//! - `start_automation` — kick off a named automation
//! - `query_memory` — gbrain memory graph query
//!
//! This pilot ships the **typed request/response payloads** + a
//! `ServerToolKind` enum that tags each tool. The actual `rmcp`
//! binding + `~/.uclaw/mcp_server.toml` auth token surface live in
//! M3-T9 commit 2 + commit 3.
//!
//! Layout:
//!
//! - [`types`] — `ServerToolKind`, request/response structs,
//!   `ServerConfig`, `AuthToken`

pub mod types;

pub use types::{
    AuthToken, ListThreadsRequest, ListThreadsResponse, QueryMemoryRequest,
    QueryMemoryResponse, ReadThreadRequest, ReadThreadResponse, ServerConfig,
    ServerToolKind, StartAutomationRequest, StartAutomationResponse, ThreadSummary,
};
