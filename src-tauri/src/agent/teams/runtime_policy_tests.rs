use super::*;

#[test]
fn default_policy_matches_existing_team_runtime_limits() {
    let policy = TeamRuntimePolicy::default();

    assert_eq!(policy.max_supervisor_turns, 20);
    assert_eq!(policy.max_parallel_workers, 4);
    assert_eq!(policy.worker_max_iterations, 8);
    assert_eq!(policy.worker_await_timeout_secs, 120);
    assert_eq!(policy.drain_timeout_secs, 30);
}

#[test]
fn policy_blocks_worker_fanout_past_limit() {
    let policy = TeamRuntimePolicy {
        max_parallel_workers: 2,
        ..TeamRuntimePolicy::default()
    };

    assert!(policy.check_worker_fanout(0).is_ok());
    assert!(policy.check_worker_fanout(1).is_ok());
    assert_eq!(
        policy.check_worker_fanout(2).unwrap_err(),
        TeamRuntimePolicyViolation::TooManyWorkers { active: 2, max: 2 }
    );
}

#[test]
fn review_gate_blocks_completion_until_pass() {
    let mut gate = ReviewGateState::default();
    assert!(!gate.can_complete());
    assert_eq!(
        gate.completion_error(),
        Some(
            "Reviewer approval required before complete_task. Call request_review first."
                .to_string()
        )
    );

    gate.record_review(ReviewGateDecision::Revise {
        feedback: "Need citations".into(),
    });
    assert!(!gate.can_complete());

    gate.record_reviewed_result(ReviewGateDecision::Pass, "final answer");
    assert!(gate.can_complete());
    assert_eq!(gate.completion_error(), None);
}

#[test]
fn review_gate_pass_only_approves_reviewed_result() {
    let mut gate = ReviewGateState::default();
    gate.record_reviewed_result(ReviewGateDecision::Pass, "approved draft");

    assert_eq!(
        gate.completion_error_for_result(Some("approved draft")),
        None
    );
    assert_eq!(
        gate.completion_error_for_result(Some("different draft")),
        Some(
            "Reviewer approval applies to a different result. Call request_review again."
                .to_string()
        )
    );
}

#[test]
fn review_gate_resets_when_new_work_is_assigned() {
    let mut gate = ReviewGateState::default();
    gate.record_reviewed_result(ReviewGateDecision::Pass, "approved draft");
    assert!(gate.can_complete());

    gate.reset_for_new_work();

    assert!(!gate.can_complete());
    assert_eq!(gate.approved_result, None);
    assert_eq!(gate.last_decision, ReviewGateDecision::Pending);
}

#[test]
fn review_gate_records_max_cycles_without_approval() {
    let mut gate = ReviewGateState::default();
    gate.record_review(ReviewGateDecision::MaxCyclesReached);

    assert!(!gate.can_complete());
    assert_eq!(
        gate.completion_error(),
        Some(
            "Reviewer approval required before complete_task. Last review status: max_cycles_reached."
                .to_string()
        )
    );
}

#[test]
fn review_gate_decisions_have_stable_ids() {
    assert_eq!(ReviewGateDecision::Pending.id(), "pending");
    assert_eq!(ReviewGateDecision::Pass.id(), "pass");
    assert_eq!(
        ReviewGateDecision::Revise {
            feedback: "x".into()
        }
        .id(),
        "revise"
    );
    assert_eq!(ReviewGateDecision::Fail { reason: "x".into() }.id(), "fail");
    assert_eq!(
        ReviewGateDecision::MaxCyclesReached.id(),
        "max_cycles_reached"
    );
}
