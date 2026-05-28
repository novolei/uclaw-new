use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::eval::artifacts::HarnessArtifact;
use crate::eval::case::EvalSubject;
use crate::eval::trace::EvalEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessVerdict {
    Pass,
    Fail,
    Partial,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessEpisode {
    pub run_id: String,
    pub case_id: String,
    pub subject: EvalSubject,
    pub started_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<i64>,
    pub trace: Vec<EvalEvent>,
    pub artifacts: Vec<HarnessArtifact>,
    pub scores: BTreeMap<String, f64>,
    pub verdict: HarnessVerdict,
}

impl HarnessEpisode {
    pub fn new(case_id: impl Into<String>, subject: EvalSubject) -> Self {
        let case_id = case_id.into();
        Self {
            run_id: format!("run-{}", uuid::Uuid::new_v4()),
            case_id: case_id.clone(),
            subject,
            started_at_ms: chrono::Utc::now().timestamp_millis(),
            finished_at_ms: None,
            trace: vec![EvalEvent::RunStarted {
                ts: chrono::Utc::now().to_rfc3339(),
                case_id,
            }],
            artifacts: Vec::new(),
            scores: BTreeMap::new(),
            verdict: HarnessVerdict::Partial,
        }
    }

    pub fn append_event(&mut self, event: EvalEvent) {
        self.trace.push(event);
    }

    pub fn attach_artifact(&mut self, artifact: HarnessArtifact) {
        self.artifacts.push(artifact);
    }

    pub fn finish(&mut self, verdict: HarnessVerdict) {
        self.verdict = verdict;
        self.finished_at_ms = Some(chrono::Utc::now().timestamp_millis());
        self.trace.push(EvalEvent::RunFinished {
            ts: chrono::Utc::now().to_rfc3339(),
            verdict,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::trace::EvalEvent;

    #[test]
    fn episode_starts_and_finishes_with_trace_events() {
        let mut episode = HarnessEpisode::new("case-1", EvalSubject::AgentLoop);
        assert_eq!(episode.verdict, HarnessVerdict::Partial);
        assert_eq!(episode.trace[0].kind(), "run_started");

        episode.append_event(EvalEvent::PermissionRequest {
            ts: "2026-05-19T00:00:00Z".into(),
            request_id: "ask-1".into(),
            reason: "needs approval".into(),
        });
        episode.finish(HarnessVerdict::Blocked);

        assert_eq!(episode.verdict, HarnessVerdict::Blocked);
        assert!(episode.finished_at_ms.is_some());
        assert_eq!(episode.trace.last().unwrap().kind(), "run_finished");
        let value = serde_json::to_value(&episode).unwrap();
        assert_eq!(value["caseId"], "case-1");
        assert_eq!(value["verdict"], "blocked");
    }
}
