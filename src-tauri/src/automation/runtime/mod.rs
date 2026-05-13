pub mod auto_continue;
pub mod execute;
pub mod prompt;
pub mod service;

pub use service::{AppRuntimeService, EscalationRow};

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
pub use execute::AutomationDelegate;
pub use prompt::{build_initial_message, build_system_prompt, EscalationResolution};
