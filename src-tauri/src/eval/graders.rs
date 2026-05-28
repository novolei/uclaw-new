use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::eval::episode::{EvalEpisode, EvalVerdict};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalGraderSpec {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalGraderResult {
    pub grader_id: String,
    pub passed: bool,
    pub score: f64,
    pub message: String,
}

#[derive(Debug, Default, Clone)]
pub struct EvalGraderRegistry;

impl EvalGraderRegistry {
    pub fn grade_episode(
        &self,
        episode: &EvalEpisode,
        specs: &[EvalGraderSpec],
    ) -> Vec<EvalGraderResult> {
        specs
            .iter()
            .map(|spec| self.grade_one(episode, spec))
            .collect()
    }

    fn grade_one(&self, episode: &EvalEpisode, spec: &EvalGraderSpec) -> EvalGraderResult {
        match spec.kind.as_str() {
            "event_exists" => grade_event_exists(episode, spec),
            "verdict_is" => grade_verdict_is(episode, spec),
            other => EvalGraderResult {
                grader_id: spec.id.clone(),
                passed: false,
                score: 0.0,
                message: format!("unknown grader kind: {other}"),
            },
        }
    }
}

fn grade_event_exists(episode: &EvalEpisode, spec: &EvalGraderSpec) -> EvalGraderResult {
    let Some(kind) = spec.params.get("kind").and_then(Value::as_str) else {
        return EvalGraderResult {
            grader_id: spec.id.clone(),
            passed: false,
            score: 0.0,
            message: "event_exists requires params.kind".to_string(),
        };
    };
    let passed = episode.trace.iter().any(|event| event.kind() == kind);
    EvalGraderResult {
        grader_id: spec.id.clone(),
        passed,
        score: if passed { 1.0 } else { 0.0 },
        message: if passed {
            format!("found event kind {kind}")
        } else {
            format!("missing event kind {kind}")
        },
    }
}

fn grade_verdict_is(episode: &EvalEpisode, spec: &EvalGraderSpec) -> EvalGraderResult {
    let Some(verdict) = spec.params.get("verdict").and_then(Value::as_str) else {
        return EvalGraderResult {
            grader_id: spec.id.clone(),
            passed: false,
            score: 0.0,
            message: "verdict_is requires params.verdict".to_string(),
        };
    };
    let expected = match verdict {
        "pass" => EvalVerdict::Pass,
        "fail" => EvalVerdict::Fail,
        "partial" => EvalVerdict::Partial,
        "blocked" => EvalVerdict::Blocked,
        other => {
            return EvalGraderResult {
                grader_id: spec.id.clone(),
                passed: false,
                score: 0.0,
                message: format!("unknown verdict: {other}"),
            };
        }
    };
    let passed = episode.verdict == expected;
    EvalGraderResult {
        grader_id: spec.id.clone(),
        passed,
        score: if passed { 1.0 } else { 0.0 },
        message: format!("expected {expected:?}, got {:?}", episode.verdict),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::case::EvalSubject;
    use crate::eval::trace::EvalEvent;
    use serde_json::json;

    #[test]
    fn built_in_graders_score_events_and_verdicts() {
        let mut episode = EvalEpisode::new("case-1", EvalSubject::Tools);
        episode.append_event(EvalEvent::ToolCall {
            ts: "2026-05-19T00:00:00Z".into(),
            tool_name: "read_file".into(),
            input_ref: "input-1".into(),
        });
        episode.finish(EvalVerdict::Pass);

        let registry = EvalGraderRegistry;
        let results = registry.grade_episode(
            &episode,
            &[
                EvalGraderSpec {
                    id: "has-tool-call".into(),
                    kind: "event_exists".into(),
                    params: json!({ "kind": "tool_call" }),
                },
                EvalGraderSpec {
                    id: "passed".into(),
                    kind: "verdict_is".into(),
                    params: json!({ "verdict": "pass" }),
                },
            ],
        );

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| result.passed));
    }
}
