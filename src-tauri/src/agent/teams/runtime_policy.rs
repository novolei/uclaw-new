use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRuntimePolicy {
    pub max_supervisor_turns: u32,
    pub max_parallel_workers: usize,
    pub worker_max_iterations: usize,
    pub worker_await_timeout_secs: u64,
    pub drain_timeout_secs: u64,
}

impl Default for TeamRuntimePolicy {
    fn default() -> Self {
        Self {
            max_supervisor_turns: 20,
            max_parallel_workers: 4,
            worker_max_iterations: 8,
            worker_await_timeout_secs: 120,
            drain_timeout_secs: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TeamRuntimePolicyViolation {
    TooManyWorkers { active: usize, max: usize },
}

impl TeamRuntimePolicy {
    pub fn check_worker_fanout(
        &self,
        active_worker_count: usize,
    ) -> Result<(), TeamRuntimePolicyViolation> {
        if active_worker_count >= self.max_parallel_workers {
            Err(TeamRuntimePolicyViolation::TooManyWorkers {
                active: active_worker_count,
                max: self.max_parallel_workers,
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReviewGateDecision {
    Pending,
    Pass,
    Revise { feedback: String },
    Fail { reason: String },
    MaxCyclesReached,
}

impl ReviewGateDecision {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Pass => "pass",
            Self::Revise { .. } => "revise",
            Self::Fail { .. } => "fail",
            Self::MaxCyclesReached => "max_cycles_reached",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewGateState {
    pub last_decision: ReviewGateDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_result: Option<String>,
}

impl Default for ReviewGateState {
    fn default() -> Self {
        Self {
            last_decision: ReviewGateDecision::Pending,
            approved_result: None,
        }
    }
}

impl ReviewGateState {
    pub fn record_review(&mut self, decision: ReviewGateDecision) {
        self.last_decision = decision;
        self.approved_result = None;
    }

    pub fn record_reviewed_result(&mut self, decision: ReviewGateDecision, result: &str) {
        self.last_decision = decision.clone();
        self.approved_result =
            matches!(decision, ReviewGateDecision::Pass).then(|| result.to_string());
    }

    pub fn reset_for_new_work(&mut self) {
        self.last_decision = ReviewGateDecision::Pending;
        self.approved_result = None;
    }

    pub fn can_complete(&self) -> bool {
        matches!(self.last_decision, ReviewGateDecision::Pass) && self.approved_result.is_some()
    }

    pub fn completion_error(&self) -> Option<String> {
        self.completion_error_for_result(None)
    }

    pub fn completion_error_for_result(&self, result: Option<&str>) -> Option<String> {
        if self.can_complete() {
            match (result, self.approved_result.as_deref()) {
                (Some(candidate), Some(approved)) if candidate != approved => Some(
                    "Reviewer approval applies to a different result. Call request_review again."
                        .to_string(),
                ),
                _ => None,
            }
        } else if matches!(self.last_decision, ReviewGateDecision::Pending) {
            Some(
                "Reviewer approval required before complete_task. Call request_review first."
                    .to_string(),
            )
        } else {
            Some(format!(
                "Reviewer approval required before complete_task. Last review status: {}.",
                self.last_decision.id()
            ))
        }
    }
}

#[cfg(test)]
#[path = "runtime_policy_tests.rs"]
mod tests;
