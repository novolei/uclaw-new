pub mod approval;
pub mod auto_continue;
pub mod chat_sessions;
pub mod cost;
pub mod execute;
pub mod prompt;
pub mod run_session;
pub mod service;
pub mod tool_registry;

pub use approval::AutomationApprovalHandler;

pub use service::{AppRuntimeService, EscalationRow};

/// Build the safety chokepoint plumbing for one automation run.
///
/// Returns `(tool_dispatcher, approval_handler)` — both wrapped as `Arc<...>`
/// ready to assign to `HeadlessDelegate`'s optional fields. The `ToolDispatcher`
/// shares `AppState`'s `SafetyManager`, `PendingApprovals`, and `HookBus`
/// singletons but uses the per-run tool registry passed in. The
/// `ApprovalHandler` is a fresh `AutomationApprovalHandler` bound to the
/// run's DB and app handle.
///
/// Production wire-up of the Slice 1b safety chokepoint (follow-up to PR #564).
pub fn build_automation_chokepoint(
    tools: std::sync::Arc<crate::agent::tools::tool::ToolRegistry>,
    app_handle: tauri::AppHandle,
    safety_manager: std::sync::Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>,
    pending_approvals: std::sync::Arc<crate::app::PendingApprovals>,
    hook_bus: std::sync::Arc<crate::agent::hook_bus::HookBus>,
    db: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> (
    std::sync::Arc<crate::agent::tool_dispatch::ToolDispatcher<tauri::Wry>>,
    std::sync::Arc<dyn crate::safety::ApprovalHandler>,
) {
    let approval_handler: std::sync::Arc<dyn crate::safety::ApprovalHandler> =
        std::sync::Arc::new(crate::automation::runtime::approval::AutomationApprovalHandler::new(
            db,
            Some(app_handle.clone()),
        ));
    let tool_dispatcher = std::sync::Arc::new(
        crate::agent::tool_dispatch::ToolDispatcher::new_with_approval_handler(
            tools,
            app_handle,
            safety_manager,
            approval_handler.clone(),
            pending_approvals,
            None,  // infra_service — automation doesn't use InfraService for dispatch
            None,  // trajectory_store — automation has its own
            None,  // tool_budget — automation has its own cost cap
            hook_bus,
            None,  // heartbeat — automation doesn't have a heartbeat supervisor
        ),
    );
    (tool_dispatcher, approval_handler)
}

use crate::automation::protocol::humane_v1::Permission;

/// Resolved permission state for one automation run.
///
/// `spec` holds the permissions declared in the YAML spec (treated as an
/// implicit grant if the user hasn't overridden them). `granted` / `denied`
/// are the user-level overrides stored in `automation_specs.permissions_granted`
/// / `permissions_denied`. The deny-list wins over both.
#[derive(Debug, Clone, Default)]
pub struct PermissionSet {
    pub spec: Vec<Permission>,
    pub granted: Vec<Permission>,
    pub denied: Vec<Permission>,
}

pub use auto_continue::{AutoContinueConfig, CompletionGate};
pub use cost::{CostCapConfig, CostCapState, CostCapDecision};
pub use execute::AutomationDelegate;
pub use prompt::{build_initial_message, build_initial_message_with_memory, build_system_prompt, EscalationResolution};

/// Map a tool name to its coarse-grained `Permission` category.
///
/// `PermissionSet` operates at category granularity (Shell/Filesystem/...).
/// This is the bridge from per-tool dispatch to per-category authorization.
/// Unknown tools map to `Permission::Unknown` and are treated as un-covered
/// by `PermissionSet::covers` (forcing FallThrough → SafetyManager `Ask`).
pub fn permission_for_tool(tool_name: &str) -> Permission {
    match tool_name {
        "bash" | "shell" => Permission::Shell,
        "edit" | "write_file" | "read_file" | "multi_edit" | "search_files"
            | "ls" | "glob" | "grep" | "get_file_skeleton" => Permission::Filesystem,
        "browser_task" => Permission::AiBrowser,
        "notify_user" => Permission::Notification,
        // TODO(Network permission): map "web_fetch", "http_request" to
        // Permission::Network once a Network category is wired into the
        // PermissionSet flow. Currently they fall through to Unknown →
        // SafetyManager ask.
        _ => Permission::Unknown,
    }
}

/// Result of checking whether a `PermissionSet` covers a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Tool's category is in `denied` — explicit deny.
    Denied,
    /// Tool's category is in `spec` ∪ `granted` (and not in `denied`) — auto-approve.
    Allowed,
    /// Tool's category is in neither — fall through to SafetyManager normal flow.
    FallThrough,
}

impl PermissionSet {
    /// Decide whether this set covers a per-call tool authorization decision.
    ///
    /// `denied` wins over `spec` and `granted`. Unknown-category tools always
    /// fall through (no permission category names them).
    pub fn covers(&self, tool_name: &str) -> Coverage {
        let cat = permission_for_tool(tool_name);
        if matches!(cat, Permission::Unknown) {
            return Coverage::FallThrough;
        }
        if self.denied.contains(&cat) {
            return Coverage::Denied;
        }
        if self.spec.contains(&cat) || self.granted.contains(&cat) {
            return Coverage::Allowed;
        }
        Coverage::FallThrough
    }
}

#[cfg(test)]
mod permission_set_covers_tests {
    use super::*;
    use crate::automation::protocol::humane_v1::Permission;

    fn perms(spec: Vec<Permission>, granted: Vec<Permission>, denied: Vec<Permission>) -> PermissionSet {
        PermissionSet { spec, granted, denied }
    }

    #[test]
    fn denied_wins_over_spec() {
        let p = perms(vec![Permission::Shell], vec![], vec![Permission::Shell]);
        assert_eq!(p.covers("bash"), Coverage::Denied);
    }

    #[test]
    fn spec_grant_allows() {
        let p = perms(vec![Permission::Filesystem], vec![], vec![]);
        assert_eq!(p.covers("edit"), Coverage::Allowed);
    }

    #[test]
    fn user_granted_allows() {
        let p = perms(vec![], vec![Permission::Filesystem], vec![]);
        assert_eq!(p.covers("write_file"), Coverage::Allowed);
    }

    #[test]
    fn neither_grants_nor_denies_falls_through() {
        let p = perms(vec![Permission::Notification], vec![], vec![]);
        assert_eq!(p.covers("bash"), Coverage::FallThrough);
    }

    #[test]
    fn unknown_tool_falls_through_even_when_all_categories_granted() {
        let p = perms(
            vec![Permission::Shell, Permission::Filesystem, Permission::Network],
            vec![Permission::AiBrowser, Permission::Notification],
            vec![],
        );
        assert_eq!(p.covers("some_unknown_tool"), Coverage::FallThrough);
    }
}

#[cfg(test)]
mod permission_for_tool_tests {
    use super::*;
    use crate::automation::protocol::humane_v1::Permission;

    #[test]
    fn maps_shell_tools_to_shell_permission() {
        assert_eq!(permission_for_tool("bash"), Permission::Shell);
        assert_eq!(permission_for_tool("shell"), Permission::Shell);
    }

    #[test]
    fn maps_file_tools_to_filesystem_permission() {
        assert_eq!(permission_for_tool("edit"), Permission::Filesystem);
        assert_eq!(permission_for_tool("write_file"), Permission::Filesystem);
        assert_eq!(permission_for_tool("read_file"), Permission::Filesystem);
        assert_eq!(permission_for_tool("multi_edit"), Permission::Filesystem);
    }

    #[test]
    fn maps_grep_to_filesystem_permission() {
        assert_eq!(permission_for_tool("grep"), Permission::Filesystem);
    }

    #[test]
    fn maps_get_file_skeleton_to_filesystem_permission() {
        assert_eq!(permission_for_tool("get_file_skeleton"), Permission::Filesystem);
    }

    #[test]
    fn maps_browser_tools_to_aibrowser_permission() {
        assert_eq!(permission_for_tool("browser_task"), Permission::AiBrowser);
    }

    #[test]
    fn maps_notify_user_to_notification_permission() {
        assert_eq!(permission_for_tool("notify_user"), Permission::Notification);
    }

    #[test]
    fn unknown_tool_maps_to_unknown() {
        assert_eq!(permission_for_tool("some_random_tool"), Permission::Unknown);
    }
}
