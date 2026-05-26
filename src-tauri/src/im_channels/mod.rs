//! M3-T7 — Instant Messaging channel adapter types (pilot).
//!
//! ADR §M3-T7 spec'd a uniform `ImChannelAdapter` trait so plugins
//! (Slack / Discord / Telegram / SMS / WeChat / Lark / Teams) can
//! ship as drop-in connectors. This pilot ships the type contract
//! the adapters implement:
//!
//! - `ImPlatform` enum — the platforms we know about up-front +
//!   `Custom(String)` for plugin-defined.
//! - `ImChannelRef` — opaque channel/thread pointer per platform.
//! - `ImMessage` — inbound message envelope.
//! - `MessageFlowEnvelope` — app-level communication envelope for
//!   global close-loop routing.
//! - `ImOutbound` — outbound message envelope (text / reaction / typing).
//! - `ImChannelAdapter` async trait — list / fetch / send.
//! - `ImEvent` — push-style inbound event (`MessageReceived`,
//!   `MessageEdited`, `MessageDeleted`, `ReactionAdded`).
//!
//! Adapter impls (slack-mcp / discord-mcp / ...) live in M3-T7
//! commit 2+.
//!
//! Layout:
//!
//! - [`types`] — `ImPlatform`, `ImChannelRef`, `ImMessage`, `ImEvent`,
//!   `MessageFlowEnvelope`, `ImOutbound`, `ImChannelAdapter`

pub mod types;

pub use types::{
    CloseLoopSink, ImChannelAdapter, ImChannelRef, ImEvent, ImMessage, ImOutbound, ImPlatform,
    ImSendResult, MessageCapabilityProfile, MessageFlowEnvelope, MessageFlowOrigin,
    MessageFlowTarget,
};
