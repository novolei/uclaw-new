//! Per-session context passed to ToolDescriptor builder closures.
//!
//! Builders are registered at boot (`AppState::new()` time) but only invoked
//! at session-build time. The `SessionContext` carries the live session-scoped
//! state (workspace, app handle, db handle, etc.) needed to construct concrete
//! `Box<dyn Tool>` instances.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::AppHandle;

/// Session-scoped context for tool construction.
///
/// Lifetime `'a` is the borrow of the AppState held by the session. Builder
/// closures dereference fields they need; they're free to `.clone()` the
/// `Arc`-typed fields out into the tool instance.
pub struct SessionContext<'a> {
    pub session_id: String,
    pub workspace: PathBuf,
    pub model: String,
    pub app_handle: AppHandle,
    pub llm: Arc<dyn crate::llm::LlmProvider>,
    pub app_state: &'a crate::app::AppState,
    /// item2 — resolved per-session project-check config. `Some` enables the
    /// best-effort post-edit project check on the descriptor-built `EditTool`;
    /// `None` (the default / unconfigured) keeps edits unchanged. Resolved from
    /// `memory_os.edit_project_check_*` in `build_tool_registry` (async) so the
    /// sync descriptor builder can read it without awaiting the config lock.
    pub edit_project_check: Option<crate::agent::tools::builtin::edit_verify::ProjectCheckCfg>,
}
