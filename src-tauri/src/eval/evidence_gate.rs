use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::eval::case::EvalSubject;
use crate::eval::episode::EvalEpisode;
use crate::eval::evidence::{
    EvalEvidenceGateReport, EvalEvidenceGateVerdict, EvalEvidenceRequirement, gate_eval_evidence,
};

pub const EVAL_EVIDENCE_MANIFEST_SCHEMA: &str = "uclaw.eval.evidence_manifest.v1";

#[derive(Debug)]
pub enum EvalEvidenceGateError {
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidManifest(String),
}

impl fmt::Display for EvalEvidenceGateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "evidence gate IO error: {err}"),
            Self::Json(err) => write!(f, "evidence gate JSON error: {err}"),
            Self::InvalidManifest(message) => write!(f, "invalid evidence manifest: {message}"),
        }
    }
}

impl std::error::Error for EvalEvidenceGateError {}

impl From<std::io::Error> for EvalEvidenceGateError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for EvalEvidenceGateError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalEvidenceManifest {
    pub schema: String,
    pub manifest_id: String,
    pub cases: Vec<EvalEvidenceManifestCase>,
}

impl EvalEvidenceManifest {
    pub fn from_json_str(input: &str) -> Result<Self, EvalEvidenceGateError> {
        let manifest: Self = serde_json::from_str(input)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn case(&self, case_id: &str, subject: EvalSubject) -> Option<&EvalEvidenceManifestCase> {
        self.cases
            .iter()
            .find(|case| case.case_id == case_id && case.subject == subject)
    }

    fn validate(&self) -> Result<(), EvalEvidenceGateError> {
        if self.schema != EVAL_EVIDENCE_MANIFEST_SCHEMA {
            return Err(EvalEvidenceGateError::InvalidManifest(format!(
                "expected schema {EVAL_EVIDENCE_MANIFEST_SCHEMA}, got {}",
                self.schema
            )));
        }
        if self.manifest_id.trim().is_empty() {
            return Err(EvalEvidenceGateError::InvalidManifest(
                "manifestId is required".to_string(),
            ));
        }
        if self.cases.is_empty() {
            return Err(EvalEvidenceGateError::InvalidManifest(
                "at least one case is required".to_string(),
            ));
        }
        for case in &self.cases {
            if case.case_id.trim().is_empty() {
                return Err(EvalEvidenceGateError::InvalidManifest(
                    "caseId is required".to_string(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalEvidenceManifestCase {
    pub case_id: String,
    pub subject: EvalSubject,
    #[serde(default)]
    pub required_event_kinds: Vec<String>,
    #[serde(default)]
    pub required_artifact_kinds: Vec<String>,
}

impl EvalEvidenceManifestCase {
    pub fn requirement(&self) -> EvalEvidenceRequirement {
        EvalEvidenceRequirement::new(
            self.required_event_kinds.clone(),
            self.required_artifact_kinds.clone(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalEvidenceGateCommandOutcome {
    pub exit_code: i32,
    pub report: EvalEvidenceGateReport,
}

pub fn gate_eval_evidence_manifest(
    episode: &EvalEpisode,
    manifest: &EvalEvidenceManifest,
) -> EvalEvidenceGateReport {
    let requirement = manifest
        .case(&episode.case_id, episode.subject)
        .map(EvalEvidenceManifestCase::requirement)
        .unwrap_or_default();
    gate_eval_evidence(episode, &requirement)
}

pub fn run_eval_evidence_gate_files(
    manifest_path: &Path,
    episode_path: &Path,
    report_path: Option<&Path>,
) -> Result<EvalEvidenceGateCommandOutcome, EvalEvidenceGateError> {
    let manifest = EvalEvidenceManifest::from_json_str(&fs::read_to_string(manifest_path)?)?;
    let episode: EvalEpisode = serde_json::from_slice(&fs::read(episode_path)?)?;
    let report = gate_eval_evidence_manifest(&episode, &manifest);

    if let Some(path) = report_path {
        fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }

    let exit_code = if report.verdict == EvalEvidenceGateVerdict::Pass {
        0
    } else {
        1
    };

    Ok(EvalEvidenceGateCommandOutcome { exit_code, report })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::artifacts::EvalArtifact;
    use crate::eval::case::EvalSubject;
    use crate::eval::episode::EvalEpisode;
    use crate::eval::trace::EvalEvent;
    use serde_json::json;

    #[test]
    fn evidence_manifest_parses_case_requirements() {
        let manifest = EvalEvidenceManifest::from_json_str(
            r#"
            {
              "schema": "uclaw.eval.evidence_manifest.v1",
              "manifestId": "agent-os-smoke",
              "cases": [
                {
                  "caseId": "case-1",
                  "subject": "tools",
                  "requiredEventKinds": ["tool_call", "tool_result"],
                  "requiredArtifactKinds": ["tool_result"]
                }
              ]
            }
            "#,
        )
        .unwrap();

        let case = manifest.case("case-1", EvalSubject::Tools).unwrap();
        assert_eq!(
            case.requirement().required_event_kinds,
            ["tool_call", "tool_result"]
        );
        assert_eq!(case.requirement().required_artifact_kinds, ["tool_result"]);
    }

    #[test]
    fn file_gate_fails_closed_when_required_artifact_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest_path = tmp.path().join("manifest.json");
        let episode_path = tmp.path().join("episode.json");
        let report_path = tmp.path().join("report.json");

        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&json!({
                "schema": "uclaw.eval.evidence_manifest.v1",
                "manifestId": "agent-os-smoke",
                "cases": [{
                    "caseId": "case-1",
                    "subject": "tools",
                    "requiredEventKinds": ["tool_call", "tool_result"],
                    "requiredArtifactKinds": ["tool_result"]
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        let mut episode = EvalEpisode::new("case-1", EvalSubject::Tools);
        episode.run_id = "run-1".into();
        episode.append_event(EvalEvent::ToolCall {
            ts: "2026-06-01T00:00:00Z".into(),
            tool_name: "read_file".into(),
            input_ref: "input-1".into(),
        });
        episode.append_event(EvalEvent::ToolResult {
            ts: "2026-06-01T00:00:01Z".into(),
            tool_name: "read_file".into(),
            output_ref: "output-1".into(),
            ok: true,
        });
        std::fs::write(&episode_path, serde_json::to_vec_pretty(&episode).unwrap()).unwrap();

        let outcome =
            run_eval_evidence_gate_files(&manifest_path, &episode_path, Some(&report_path))
                .unwrap();

        assert_eq!(outcome.exit_code, 1);
        assert_eq!(outcome.report.missing_artifact_kinds, ["tool_result"]);
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
        assert_eq!(report["verdict"], "fail_closed");
    }

    #[test]
    fn file_gate_passes_when_manifest_evidence_is_present() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest_path = tmp.path().join("manifest.json");
        let episode_path = tmp.path().join("episode.json");

        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&json!({
                "schema": "uclaw.eval.evidence_manifest.v1",
                "manifestId": "agent-os-smoke",
                "cases": [{
                    "caseId": "case-1",
                    "subject": "tools",
                    "requiredEventKinds": ["tool_call", "tool_result"],
                    "requiredArtifactKinds": ["tool_result"]
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        let mut episode = EvalEpisode::new("case-1", EvalSubject::Tools);
        episode.run_id = "run-1".into();
        episode.append_event(EvalEvent::ToolCall {
            ts: "2026-06-01T00:00:00Z".into(),
            tool_name: "read_file".into(),
            input_ref: "input-1".into(),
        });
        episode.append_event(EvalEvent::ToolResult {
            ts: "2026-06-01T00:00:01Z".into(),
            tool_name: "read_file".into(),
            output_ref: "output-1".into(),
            ok: true,
        });
        episode.attach_artifact(EvalArtifact {
            id: "artifact-1".into(),
            run_id: "run-1".into(),
            kind: "tool_result".into(),
            path: "/tmp/tool-result.json".into(),
            mime_type: "application/json".into(),
            created_at_ms: 0,
            metadata: serde_json::Value::Null,
        });
        std::fs::write(&episode_path, serde_json::to_vec_pretty(&episode).unwrap()).unwrap();

        let outcome = run_eval_evidence_gate_files(&manifest_path, &episode_path, None).unwrap();

        assert_eq!(outcome.exit_code, 0);
        assert_eq!(
            outcome.report.verdict,
            crate::eval::evidence::EvalEvidenceGateVerdict::Pass
        );
    }
}
