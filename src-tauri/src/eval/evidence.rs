use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::eval::artifacts::{ArtifactStoreError, EvalArtifact};
use crate::eval::case::EvalSubject;
use crate::eval::episode::EvalEpisode;
use crate::eval::runtime::EvalRuntime;

pub const EVAL_EVIDENCE_SCHEMA: &str = "uclaw.eval.evidence.v1";
pub const EVAL_EVIDENCE_ARTIFACT_KIND: &str = "eval_evidence_report";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalEvidenceGateVerdict {
    Pass,
    FailClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalEvidenceCheckStatus {
    Pass,
    Missing,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalEvidenceRequirement {
    pub required_event_kinds: Vec<String>,
    pub required_artifact_kinds: Vec<String>,
}

impl EvalEvidenceRequirement {
    pub fn new<EventKinds, ArtifactKinds, EventKind, ArtifactKind>(
        required_event_kinds: EventKinds,
        required_artifact_kinds: ArtifactKinds,
    ) -> Self
    where
        EventKinds: IntoIterator<Item = EventKind>,
        ArtifactKinds: IntoIterator<Item = ArtifactKind>,
        EventKind: Into<String>,
        ArtifactKind: Into<String>,
    {
        Self {
            required_event_kinds: normalize_kinds(required_event_kinds),
            required_artifact_kinds: normalize_kinds(required_artifact_kinds),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.required_event_kinds.is_empty() && self.required_artifact_kinds.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalEvidenceRecord {
    pub kind: String,
    pub name: String,
    pub status: EvalEvidenceCheckStatus,
    pub message: String,
    pub evidence_refs: Vec<String>,
}

impl EvalEvidenceRecord {
    fn pass(kind: &str, name: &str, evidence_refs: Vec<String>) -> Self {
        Self {
            kind: kind.to_string(),
            name: name.to_string(),
            status: EvalEvidenceCheckStatus::Pass,
            message: "required evidence observed".to_string(),
            evidence_refs,
        }
    }

    fn missing(kind: &str, name: &str) -> Self {
        Self {
            kind: kind.to_string(),
            name: name.to_string(),
            status: EvalEvidenceCheckStatus::Missing,
            message: "required evidence missing".to_string(),
            evidence_refs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalEvidenceGateReport {
    pub schema: String,
    pub run_id: String,
    pub case_id: String,
    pub subject: EvalSubject,
    pub verdict: EvalEvidenceGateVerdict,
    pub observed_event_kinds: Vec<String>,
    pub observed_artifact_kinds: Vec<String>,
    pub missing_event_kinds: Vec<String>,
    pub missing_artifact_kinds: Vec<String>,
    pub records: Vec<EvalEvidenceRecord>,
    pub created_at_ms: i64,
}

pub fn gate_eval_evidence(
    episode: &EvalEpisode,
    requirement: &EvalEvidenceRequirement,
) -> EvalEvidenceGateReport {
    let observed_event_kinds = normalize_kinds(episode.trace.iter().map(|event| event.kind()));
    let observed_artifact_kinds = normalize_kinds(
        episode
            .artifacts
            .iter()
            .map(|artifact| artifact.kind.as_str()),
    );
    let observed_events = observed_event_kinds.iter().collect::<BTreeSet<_>>();
    let observed_artifacts = observed_artifact_kinds.iter().collect::<BTreeSet<_>>();

    let mut records = Vec::new();
    let mut missing_event_kinds = Vec::new();
    let mut missing_artifact_kinds = Vec::new();

    if requirement.is_empty() {
        records.push(EvalEvidenceRecord::missing("requirement", "non_empty"));
    }

    for required in &requirement.required_event_kinds {
        if observed_events.contains(required) {
            records.push(EvalEvidenceRecord::pass(
                "event_kind",
                required,
                vec![format!("event_kind:{required}")],
            ));
        } else {
            missing_event_kinds.push(required.clone());
            records.push(EvalEvidenceRecord::missing("event_kind", required));
        }
    }

    for required in &requirement.required_artifact_kinds {
        if observed_artifacts.contains(required) {
            let refs = episode
                .artifacts
                .iter()
                .filter(|artifact| artifact.kind == *required)
                .map(|artifact| artifact.id.clone())
                .collect();
            records.push(EvalEvidenceRecord::pass("artifact_kind", required, refs));
        } else {
            missing_artifact_kinds.push(required.clone());
            records.push(EvalEvidenceRecord::missing("artifact_kind", required));
        }
    }

    let verdict = if missing_event_kinds.is_empty()
        && missing_artifact_kinds.is_empty()
        && !requirement.is_empty()
    {
        EvalEvidenceGateVerdict::Pass
    } else {
        EvalEvidenceGateVerdict::FailClosed
    };

    EvalEvidenceGateReport {
        schema: EVAL_EVIDENCE_SCHEMA.to_string(),
        run_id: episode.run_id.clone(),
        case_id: episode.case_id.clone(),
        subject: episode.subject,
        verdict,
        observed_event_kinds,
        observed_artifact_kinds,
        missing_event_kinds,
        missing_artifact_kinds,
        records,
        created_at_ms: chrono::Utc::now().timestamp_millis(),
    }
}

pub fn attach_eval_evidence_report(
    runtime: &EvalRuntime,
    run_id: &str,
    report: &EvalEvidenceGateReport,
) -> Result<Option<EvalArtifact>, ArtifactStoreError> {
    runtime.attach_json_artifact(
        run_id,
        EVAL_EVIDENCE_ARTIFACT_KIND,
        &serde_json::to_value(report)?,
    )
}

fn normalize_kinds<Kinds, Kind>(kinds: Kinds) -> Vec<String>
where
    Kinds: IntoIterator<Item = Kind>,
    Kind: Into<String>,
{
    kinds
        .into_iter()
        .map(Into::into)
        .filter(|kind| !kind.trim().is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::case::{EvalBudget, EvalCase, EvalPolicy, EvalSubject};
    use crate::eval::episode::{EvalEpisode, EvalVerdict};
    use crate::eval::runtime::EvalRuntime;
    use crate::eval::trace::EvalEvent;
    use serde_json::json;

    fn episode_with_tool_evidence(tmp: &tempfile::TempDir) -> (EvalRuntime, EvalEpisode) {
        let runtime = EvalRuntime::new(tmp.path());
        let case = EvalCase {
            id: "case-1".into(),
            subject: EvalSubject::Tools,
            title: "Tool evidence".into(),
            prompt: "Run read_file".into(),
            setup: vec![],
            policy: EvalPolicy::default(),
            budgets: EvalBudget::default(),
            assertions: vec![],
            graders: vec![],
        };
        let episode = runtime.start_episode(&case);
        runtime.append_event(
            &episode.run_id,
            EvalEvent::ToolCall {
                ts: "2026-06-01T00:00:00Z".into(),
                tool_name: "read_file".into(),
                input_ref: "input-1".into(),
            },
        );
        runtime.append_event(
            &episode.run_id,
            EvalEvent::ToolResult {
                ts: "2026-06-01T00:00:01Z".into(),
                tool_name: "read_file".into(),
                output_ref: "artifact-output".into(),
                ok: true,
            },
        );
        runtime
            .attach_json_artifact(
                &episode.run_id,
                "tool_result",
                &json!({ "ok": true, "tool": "read_file" }),
            )
            .unwrap()
            .unwrap();
        runtime.finish_episode(&episode.run_id, EvalVerdict::Pass);
        let episode = runtime.get_episode(&episode.run_id).unwrap();
        (runtime, episode)
    }

    #[test]
    fn gate_passes_when_required_events_and_artifacts_are_present() {
        let tmp = tempfile::tempdir().unwrap();
        let (_runtime, episode) = episode_with_tool_evidence(&tmp);
        let requirement =
            EvalEvidenceRequirement::new(["tool_call", "tool_result"], ["tool_result"]);

        let report = gate_eval_evidence(&episode, &requirement);

        assert_eq!(report.schema, EVAL_EVIDENCE_SCHEMA);
        assert_eq!(report.verdict, EvalEvidenceGateVerdict::Pass);
        assert!(report.missing_event_kinds.is_empty());
        assert!(report.missing_artifact_kinds.is_empty());
        assert!(
            report
                .records
                .iter()
                .all(|record| { record.status == EvalEvidenceCheckStatus::Pass })
        );
    }

    #[test]
    fn gate_fails_closed_when_required_evidence_is_missing() {
        let episode = EvalEpisode::new("case-1", EvalSubject::Browser);
        let requirement =
            EvalEvidenceRequirement::new(["tool_result", "boundary_event"], ["browser_state"]);

        let report = gate_eval_evidence(&episode, &requirement);

        assert_eq!(report.verdict, EvalEvidenceGateVerdict::FailClosed);
        assert_eq!(
            report.missing_event_kinds,
            vec!["boundary_event".to_string(), "tool_result".to_string()]
        );
        assert_eq!(
            report.missing_artifact_kinds,
            vec!["browser_state".to_string()]
        );
        assert!(report.records.iter().any(|record| {
            record.status == EvalEvidenceCheckStatus::Missing
                && record.kind == "event_kind"
                && record.name == "tool_result"
        }));
    }

    #[test]
    fn gate_fails_closed_for_empty_requirements() {
        let episode = EvalEpisode::new("case-1", EvalSubject::AgentLoop);
        let requirement = EvalEvidenceRequirement::default();

        let report = gate_eval_evidence(&episode, &requirement);

        assert_eq!(report.verdict, EvalEvidenceGateVerdict::FailClosed);
        assert!(report.records.iter().any(|record| {
            record.status == EvalEvidenceCheckStatus::Missing
                && record.kind == "requirement"
                && record.name == "non_empty"
        }));
    }

    #[test]
    fn evidence_report_serializes_and_attaches_as_artifact() {
        let tmp = tempfile::tempdir().unwrap();
        let (runtime, episode) = episode_with_tool_evidence(&tmp);
        let requirement = EvalEvidenceRequirement::new(["tool_result"], ["tool_result"]);
        let report = gate_eval_evidence(&episode, &requirement);

        let artifact = attach_eval_evidence_report(&runtime, &episode.run_id, &report)
            .unwrap()
            .unwrap();

        assert_eq!(artifact.kind, "eval_evidence_report");
        let body = std::fs::read_to_string(&artifact.path).unwrap();
        assert!(body.contains(EVAL_EVIDENCE_SCHEMA), "{body}");
        assert!(body.contains("observedEventKinds"), "{body}");
        let stored = runtime.get_episode(&episode.run_id).unwrap();
        assert_eq!(stored.artifacts.len(), 2);
        assert!(
            stored
                .artifacts
                .iter()
                .any(|artifact| artifact.kind == EVAL_EVIDENCE_ARTIFACT_KIND)
        );
    }
}
