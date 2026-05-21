//! M2-H L6 — Tool call/output audit + orphan synthesis.
//!
//! Most LLM providers (Anthropic / OpenAI / Gemini) **require** that
//! every `tool_use` (a.k.a. `function_call`) message be followed by a
//! matching `tool_result` (a.k.a. `function_call_output`) before the
//! next assistant turn. Otherwise the API returns a 400 along the
//! lines of:
//!
//! - Anthropic: *"messages.N: `tool_use` ids were found without
//!   `tool_result` blocks immediately after"*
//! - OpenAI:    *"function call requires a function_call_output"*
//!
//! Orphans get into the history when:
//!
//! 1. The agent was cancelled mid-turn (M1-T2d cancellation token)
//! 2. A handler crashed before producing output
//! 3. A previous session ended uncleanly and the rollout was replayed
//! 4. Context compaction (M2-G fold) dropped the output but kept the call
//!
//! L6 detects orphans at `ContextManager.for_prompt` time and synthesizes
//! a placeholder `tool_result` with `"aborted"` content so the API call
//! succeeds and the model sees that the tool didn't complete.
//!
//! This pilot ships:
//!
//! - **`AuditMessage`** — a minimal, provider-agnostic message shape
//!   (`UserText` / `AssistantText` / `ToolCall { call_id, name, args }` /
//!   `ToolResult { call_id, content, is_aborted }`). The real wire-up
//!   bridges from whichever provider message shape the dispatcher is
//!   currently producing.
//! - **`audit_call_outputs`** — pure function: takes
//!   `Vec<AuditMessage>`, returns `(Vec<AuditMessage>, EnsureStats)`
//!   with synthesized `"aborted"` placeholders for every orphan call.
//! - **`EnsureStats`** — observability for the M2-J token-budget UI:
//!   how many orphans were patched, where they were, and which call
//!   ids triggered the synthesis.
//!
//! Layout:
//!
//! - [`audit`] — `AuditMessage`, `audit_call_outputs`, `EnsureStats`

pub mod audit;
pub mod chat_history;

pub use audit::{audit_call_outputs, AuditMessage, EnsureStats, OrphanCall};
pub use chat_history::audit_chat_history;
