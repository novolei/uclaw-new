use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::eval::artifacts::{ArtifactStoreError, HarnessArtifact, HarnessArtifactStore};
use crate::eval::case::EvalCase;
use crate::eval::episode::{HarnessEpisode, HarnessVerdict};
use crate::eval::graders::{HarnessGraderRegistry, HarnessGraderResult};
use crate::eval::trace::EvalEvent;

#[derive(Clone)]
pub struct EvalRuntime {
    artifact_store: HarnessArtifactStore,
    grader_registry: HarnessGraderRegistry,
    episodes: Arc<Mutex<HashMap<String, HarnessEpisode>>>,
}

impl EvalRuntime {
    pub fn new(artifact_root: impl AsRef<std::path::Path>) -> Self {
        Self {
            artifact_store: HarnessArtifactStore::new(artifact_root),
            grader_registry: HarnessGraderRegistry,
            episodes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start_episode(&self, case: &EvalCase) -> HarnessEpisode {
        let episode = HarnessEpisode::new(case.id.clone(), case.subject);
        self.episodes
            .lock()
            .unwrap()
            .insert(episode.run_id.clone(), episode.clone());
        episode
    }

    pub fn append_event(&self, run_id: &str, event: EvalEvent) -> Option<HarnessEpisode> {
        let mut episodes = self.episodes.lock().unwrap();
        let episode = episodes.get_mut(run_id)?;
        episode.append_event(event);
        Some(episode.clone())
    }

    pub fn attach_json_artifact(
        &self,
        run_id: &str,
        kind: &str,
        value: &Value,
    ) -> Result<Option<HarnessArtifact>, ArtifactStoreError> {
        let artifact = self.artifact_store.write_json(run_id, kind, value)?;
        let mut episodes = self.episodes.lock().unwrap();
        let Some(episode) = episodes.get_mut(run_id) else {
            return Ok(None);
        };
        episode.attach_artifact(artifact.clone());
        Ok(Some(artifact))
    }

    pub fn finish_episode(&self, run_id: &str, verdict: HarnessVerdict) -> Option<HarnessEpisode> {
        let mut episodes = self.episodes.lock().unwrap();
        let episode = episodes.get_mut(run_id)?;
        episode.finish(verdict);
        Some(episode.clone())
    }

    pub fn grade_case_episode(
        &self,
        case: &EvalCase,
        run_id: &str,
    ) -> Option<Vec<HarnessGraderResult>> {
        let episodes = self.episodes.lock().unwrap();
        let episode = episodes.get(run_id)?;
        Some(self.grader_registry.grade_episode(episode, &case.graders))
    }

    pub fn get_episode(&self, run_id: &str) -> Option<HarnessEpisode> {
        self.episodes.lock().unwrap().get(run_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::case::{HarnessBudget, HarnessPolicy, EvalSubject};
    use crate::eval::graders::HarnessGraderSpec;
    use serde_json::json;

    #[test]
    fn runtime_records_artifacts_events_and_grades_episode() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = EvalRuntime::new(tmp.path());
        let case = EvalCase {
            id: "case-1".into(),
            subject: EvalSubject::Browser,
            title: "Browser task trace".into(),
            prompt: "Open a page".into(),
            setup: vec![],
            policy: HarnessPolicy::default(),
            budgets: HarnessBudget::default(),
            assertions: vec![],
            graders: vec![
                HarnessGraderSpec {
                    id: "has-tool-result".into(),
                    kind: "event_exists".into(),
                    params: json!({ "kind": "tool_result" }),
                },
                HarnessGraderSpec {
                    id: "passed".into(),
                    kind: "verdict_is".into(),
                    params: json!({ "verdict": "pass" }),
                },
            ],
        };

        let episode = runtime.start_episode(&case);
        runtime.append_event(
            &episode.run_id,
            EvalEvent::ToolResult {
                ts: "2026-05-19T00:00:00Z".into(),
                tool_name: "browser_get_state".into(),
                output_ref: "state-1".into(),
                ok: true,
            },
        );
        let artifact = runtime
            .attach_json_artifact(
                &episode.run_id,
                "browser_state",
                &json!({ "url": "https://example.com" }),
            )
            .unwrap()
            .unwrap();
        assert_eq!(artifact.kind, "browser_state");

        runtime.finish_episode(&episode.run_id, HarnessVerdict::Pass);
        let results = runtime.grade_case_episode(&case, &episode.run_id).unwrap();
        assert!(results.iter().all(|result| result.passed));

        let stored = runtime.get_episode(&episode.run_id).unwrap();
        assert_eq!(stored.artifacts.len(), 1);
        assert_eq!(stored.verdict, HarnessVerdict::Pass);
    }
}
