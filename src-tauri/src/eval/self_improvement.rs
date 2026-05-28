use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelfImprovementCandidateKind {
    Memory,
    Gbrain,
    Skill,
    Prompt,
    Hook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelfImprovementGateVerdict {
    Promote,
    Hold,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfImprovementCandidate {
    pub id: String,
    pub kind: SelfImprovementCandidateKind,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub rollback_ref: Option<String>,
    #[serde(default)]
    pub evidence: Vec<SelfImprovementEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfImprovementEvidence {
    pub suite_id: String,
    pub passed: bool,
    pub average_score: f64,
    #[serde(default)]
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfImprovementGatePolicy {
    pub min_average_score: f64,
    pub require_rollback_ref: bool,
    #[serde(default)]
    pub required_suites: Vec<String>,
}

impl Default for SelfImprovementGatePolicy {
    fn default() -> Self {
        Self {
            min_average_score: 0.9,
            require_rollback_ref: true,
            required_suites: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfImprovementGateReport {
    pub candidate_id: String,
    pub kind: SelfImprovementCandidateKind,
    pub verdict: SelfImprovementGateVerdict,
    pub score: f64,
    pub checks: Vec<SelfImprovementGateCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfImprovementGateCheck {
    pub id: String,
    pub passed: bool,
    pub message: String,
}

pub fn evaluate_self_improvement_candidate(
    candidate: &SelfImprovementCandidate,
    policy: &SelfImprovementGatePolicy,
) -> SelfImprovementGateReport {
    let mut checks = Vec::new();
    let score = if candidate.evidence.is_empty() {
        0.0
    } else {
        candidate
            .evidence
            .iter()
            .map(|evidence| evidence.average_score)
            .sum::<f64>()
            / candidate.evidence.len() as f64
    };

    checks.push(check(
        "has_evidence",
        !candidate.evidence.is_empty(),
        format!("evidence suites={}", candidate.evidence.len()),
    ));
    checks.push(check(
        "score_threshold",
        score >= policy.min_average_score,
        format!(
            "average score {:.2} must be >= {:.2}",
            score, policy.min_average_score
        ),
    ));

    for suite in &policy.required_suites {
        checks.push(check(
            format!("required_suite:{suite}"),
            candidate
                .evidence
                .iter()
                .any(|evidence| evidence.suite_id == *suite && evidence.passed),
            format!("required passing suite: {suite}"),
        ));
    }

    let blockers = candidate
        .evidence
        .iter()
        .flat_map(|evidence| evidence.blockers.iter())
        .collect::<Vec<_>>();
    checks.push(check(
        "no_blockers",
        blockers.is_empty(),
        format!("blockers={:?}", blockers),
    ));

    let needs_rollback = policy.require_rollback_ref
        && matches!(
            candidate.kind,
            SelfImprovementCandidateKind::Memory
                | SelfImprovementCandidateKind::Gbrain
                | SelfImprovementCandidateKind::Skill
                | SelfImprovementCandidateKind::Prompt
                | SelfImprovementCandidateKind::Hook
        );
    if needs_rollback {
        checks.push(check(
            "rollback_ref",
            candidate
                .rollback_ref
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            format!("rollback_ref={:?}", candidate.rollback_ref),
        ));
    }

    let all_passed = checks.iter().all(|check| check.passed);
    let verdict = if all_passed {
        SelfImprovementGateVerdict::Promote
    } else if candidate
        .evidence
        .iter()
        .any(|evidence| !evidence.blockers.is_empty())
    {
        SelfImprovementGateVerdict::Reject
    } else {
        SelfImprovementGateVerdict::Hold
    };

    SelfImprovementGateReport {
        candidate_id: candidate.id.clone(),
        kind: candidate.kind,
        verdict,
        score,
        checks,
    }
}

pub fn run_self_improvement_gate_fixture_suite() -> Vec<SelfImprovementGateReport> {
    let policy = SelfImprovementGatePolicy {
        min_average_score: 0.9,
        require_rollback_ref: true,
        required_suites: vec![
            "memory_gbrain_eval".to_string(),
            "agent_control_plane".to_string(),
        ],
    };
    vec![
        evaluate_self_improvement_candidate(
            &SelfImprovementCandidate {
                id: "candidate.memory.safe_profile_fact".into(),
                kind: SelfImprovementCandidateKind::Memory,
                title: "Promote corrected profile fact".into(),
                summary: "A corrected user profile fact passed memory/gbrain and agent-loop gates."
                    .into(),
                rollback_ref: Some("memory:people/ryanliu@prev".into()),
                evidence: vec![
                    SelfImprovementEvidence {
                        suite_id: "memory_gbrain_eval".into(),
                        passed: true,
                        average_score: 0.97,
                        blockers: Vec::new(),
                    },
                    SelfImprovementEvidence {
                        suite_id: "agent_control_plane".into(),
                        passed: true,
                        average_score: 1.0,
                        blockers: Vec::new(),
                    },
                ],
            },
            &policy,
        ),
        evaluate_self_improvement_candidate(
            &SelfImprovementCandidate {
                id: "candidate.skill.unsafe_shell".into(),
                kind: SelfImprovementCandidateKind::Skill,
                title: "Promote unsafe shell fallback".into(),
                summary: "A learned skill proposes bypassing a guarded shell boundary.".into(),
                rollback_ref: Some("skill:unsafe_shell@prev".into()),
                evidence: vec![SelfImprovementEvidence {
                    suite_id: "agent_control_plane".into(),
                    passed: false,
                    average_score: 0.42,
                    blockers: vec!["permission boundary regression".into()],
                }],
            },
            &policy,
        ),
    ]
}

fn check(
    id: impl Into<String>,
    passed: bool,
    message: impl Into<String>,
) -> SelfImprovementGateCheck {
    SelfImprovementGateCheck {
        id: id.into(),
        passed,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promotes_candidate_only_when_required_suites_pass_and_rollback_exists() {
        let reports = run_self_improvement_gate_fixture_suite();
        let promoted = reports
            .iter()
            .find(|report| report.candidate_id == "candidate.memory.safe_profile_fact")
            .unwrap();

        assert_eq!(promoted.verdict, SelfImprovementGateVerdict::Promote);
        assert!(promoted.checks.iter().all(|check| check.passed));
    }

    #[test]
    fn rejects_candidate_with_blocking_regression() {
        let reports = run_self_improvement_gate_fixture_suite();
        let rejected = reports
            .iter()
            .find(|report| report.candidate_id == "candidate.skill.unsafe_shell")
            .unwrap();

        assert_eq!(rejected.verdict, SelfImprovementGateVerdict::Reject);
        assert!(rejected
            .checks
            .iter()
            .any(|check| check.id == "no_blockers" && !check.passed));
    }

    #[test]
    fn holds_candidate_missing_required_suite() {
        let policy = SelfImprovementGatePolicy {
            required_suites: vec!["memory_gbrain_eval".into()],
            ..SelfImprovementGatePolicy::default()
        };
        let candidate = SelfImprovementCandidate {
            id: "candidate.prompt.partial".into(),
            kind: SelfImprovementCandidateKind::Prompt,
            title: "Prompt tweak".into(),
            summary: "Prompt candidate with partial evidence".into(),
            rollback_ref: Some("prompt:main@prev".into()),
            evidence: vec![SelfImprovementEvidence {
                suite_id: "agent_control_plane".into(),
                passed: true,
                average_score: 1.0,
                blockers: Vec::new(),
            }],
        };

        let report = evaluate_self_improvement_candidate(&candidate, &policy);

        assert_eq!(report.verdict, SelfImprovementGateVerdict::Hold);
        assert!(report
            .checks
            .iter()
            .any(|check| check.id == "required_suite:memory_gbrain_eval" && !check.passed));
    }
}
