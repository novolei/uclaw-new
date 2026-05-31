//! Unified safety decision interface.
//!
//! This module owns the decision shape for tool calls. Callers can still keep
//! their origin-specific approval handlers, but policy/rule/permission coverage
//! resolution crosses one interface before any tool executes.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::agent::tools::tool::ApprovalRequirement;
use crate::safety::{permissions, ApprovalDecision, SafetyMode, SafetyPolicy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPermissionCoverage {
    Denied,
    Allowed,
    FallThrough,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyDecisionSource {
    PermissionCoverageDenied,
    PermissionCoverageAllowed,
    DbRules,
    LegacyPolicy,
}

#[derive(Debug, Clone)]
pub struct SafetyToolDecision {
    pub decision: ApprovalDecision,
    pub source: SafetyDecisionSource,
}

pub struct SafetyToolDecisionRequest<'a> {
    pub db: Option<&'a Arc<Mutex<Connection>>>,
    pub session_id: &'a str,
    pub tool_name: &'a str,
    pub arguments: &'a serde_json::Value,
    pub tool_approval: &'a ApprovalRequirement,
    pub mode_override: Option<&'a SafetyMode>,
    pub permission_coverage: Option<ToolPermissionCoverage>,
}

pub fn decide_tool_call(
    policy: &SafetyPolicy,
    request: SafetyToolDecisionRequest<'_>,
) -> SafetyToolDecision {
    match request.permission_coverage {
        Some(ToolPermissionCoverage::Denied) => {
            return SafetyToolDecision {
                decision: ApprovalDecision::Block {
                    reason: "tool denied by spec".to_string(),
                },
                source: SafetyDecisionSource::PermissionCoverageDenied,
            };
        }
        Some(ToolPermissionCoverage::Allowed) => {
            return SafetyToolDecision {
                decision: ApprovalDecision::AutoApprove,
                source: SafetyDecisionSource::PermissionCoverageAllowed,
            };
        }
        Some(ToolPermissionCoverage::FallThrough) | None => {}
    }

    if let Some(db) = request.db {
        return SafetyToolDecision {
            decision: permissions::resolve_decision(
                db,
                policy,
                request.session_id,
                request.tool_name,
                request.arguments,
                request.tool_approval,
                request.mode_override,
            ),
            source: SafetyDecisionSource::DbRules,
        };
    }

    SafetyToolDecision {
        decision: legacy_policy_decision(
            policy,
            request.tool_name,
            request.tool_approval,
            request.mode_override,
        ),
        source: SafetyDecisionSource::LegacyPolicy,
    }
}

fn legacy_policy_decision(
    policy: &SafetyPolicy,
    tool_name: &str,
    tool_approval: &ApprovalRequirement,
    mode_override: Option<&SafetyMode>,
) -> ApprovalDecision {
    if policy.blocked_tools.contains(tool_name) {
        tracing::warn!("Tool '{}' is blocked by safety policy", tool_name);
        return ApprovalDecision::Block {
            reason: format!("Tool '{}' is blocked by safety policy", tool_name),
        };
    }

    if *tool_approval == ApprovalRequirement::Never {
        tracing::debug!(
            "Tool '{}' auto-approved (requires_approval=Never)",
            tool_name
        );
        return ApprovalDecision::AutoApprove;
    }

    if policy.auto_approved_tools.contains(tool_name) {
        tracing::debug!("Tool '{}' auto-approved via whitelist", tool_name);
        return ApprovalDecision::AutoApprove;
    }

    let effective_mode = mode_override
        .or_else(|| policy.tool_overrides.get(tool_name))
        .unwrap_or(&policy.global_mode);

    tracing::info!(
        tool = %tool_name,
        effective_mode = ?effective_mode,
        tool_approval = ?tool_approval,
        session_override = ?mode_override,
        global_mode = ?policy.global_mode,
        "Safety decision inputs"
    );

    match effective_mode {
        SafetyMode::Yolo => ApprovalDecision::AutoApprove,
        SafetyMode::Ask => ApprovalDecision::RequireApproval {
            reason: format!("Safety mode requires approval for tool '{}'", tool_name),
        },
        SafetyMode::AcceptEdits => {
            if matches!(tool_name, "edit" | "write_file") {
                ApprovalDecision::AutoApprove
            } else {
                ApprovalDecision::RequireApproval {
                    reason: format!(
                        "Accept-edits mode: tool '{}' is not an edit tool, requires approval",
                        tool_name
                    ),
                }
            }
        }
        SafetyMode::Plan => match tool_approval {
            ApprovalRequirement::Never => ApprovalDecision::AutoApprove,
            _ => ApprovalDecision::Block {
                reason: format!(
                    "Plan mode — execution blocked for tool '{}'. Use exit_plan_mode to propose plan.",
                    tool_name
                ),
            },
        },
        SafetyMode::Supervised => match tool_approval {
            ApprovalRequirement::Always => ApprovalDecision::RequireApproval {
                reason: format!("Tool '{}' requires approval (high-risk)", tool_name),
            },
            ApprovalRequirement::UnlessAutoApproved | ApprovalRequirement::Never => {
                ApprovalDecision::AutoApprove
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request<'a>(
        tool_name: &'a str,
        approval: &'a ApprovalRequirement,
    ) -> SafetyToolDecisionRequest<'a> {
        SafetyToolDecisionRequest {
            db: None,
            session_id: "session-1",
            tool_name,
            arguments: &serde_json::Value::Null,
            tool_approval: approval,
            mode_override: None,
            permission_coverage: None,
        }
    }

    #[test]
    fn permission_denied_wins_over_yolo_mode() {
        let policy = SafetyPolicy {
            global_mode: SafetyMode::Yolo,
            ..SafetyPolicy::default()
        };
        let mut req = request("bash", &ApprovalRequirement::Always);
        req.permission_coverage = Some(ToolPermissionCoverage::Denied);

        let decision = decide_tool_call(&policy, req);

        assert!(matches!(decision.decision, ApprovalDecision::Block { .. }));
        assert_eq!(
            decision.source,
            SafetyDecisionSource::PermissionCoverageDenied
        );
    }

    #[test]
    fn permission_allowed_wins_over_ask_mode() {
        let policy = SafetyPolicy {
            global_mode: SafetyMode::Ask,
            ..SafetyPolicy::default()
        };
        let mut req = request("bash", &ApprovalRequirement::Always);
        req.permission_coverage = Some(ToolPermissionCoverage::Allowed);

        let decision = decide_tool_call(&policy, req);

        assert!(matches!(decision.decision, ApprovalDecision::AutoApprove));
        assert_eq!(
            decision.source,
            SafetyDecisionSource::PermissionCoverageAllowed
        );
    }

    #[test]
    fn fallthrough_uses_legacy_policy() {
        let policy = SafetyPolicy {
            global_mode: SafetyMode::Ask,
            ..SafetyPolicy::default()
        };
        let decision = decide_tool_call(&policy, request("bash", &ApprovalRequirement::Always));

        assert!(matches!(
            decision.decision,
            ApprovalDecision::RequireApproval { .. }
        ));
        assert_eq!(decision.source, SafetyDecisionSource::LegacyPolicy);
    }
}
