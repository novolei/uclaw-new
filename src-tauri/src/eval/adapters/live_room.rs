use serde::{Deserialize, Serialize};

use crate::eval::adapters::{HarnessAdapter, LIVE_ROOM_ADAPTER_ID};
use crate::eval::case::{HarnessBudget, HarnessCase, HarnessPolicy, HarnessSubject};
use crate::eval::episode::HarnessVerdict;
use crate::eval::runtime::HarnessRuntime;

pub const BUILTIN_LIVE_ROOM_CASES: &[&str] =
    &[include_str!("../cases/live_room/douyin-moderator-fixture.json")];

#[derive(Debug, Default, Clone)]
pub struct LiveRoomHarnessAdapter;

impl HarnessAdapter for LiveRoomHarnessAdapter {
    fn subject(&self) -> HarnessSubject {
        HarnessSubject::Browser
    }

    fn adapter_id(&self) -> &'static str {
        LIVE_ROOM_ADAPTER_ID
    }
}

impl LiveRoomHarnessAdapter {
    pub fn load_builtin_cases() -> Result<Vec<LiveRoomHarnessCase>, serde_json::Error> {
        BUILTIN_LIVE_ROOM_CASES
            .iter()
            .map(|raw| serde_json::from_str(raw))
            .collect()
    }

    pub fn run_fixture_suite(
        &self,
        runtime: &HarnessRuntime,
    ) -> anyhow::Result<LiveRoomSuiteReport> {
        let cases = Self::load_builtin_cases()?;
        let traces = cases
            .iter()
            .map(LiveRoomHarnessTrace::passing_fixture_for_case)
            .collect();
        self.run_suite(runtime, cases, traces)
    }

    pub fn run_suite(
        &self,
        runtime: &HarnessRuntime,
        cases: Vec<LiveRoomHarnessCase>,
        traces: Vec<LiveRoomHarnessTrace>,
    ) -> anyhow::Result<LiveRoomSuiteReport> {
        let mut run_ids = Vec::new();
        let mut scorecards = Vec::new();

        for case in cases {
            let trace = traces
                .iter()
                .find(|trace| trace.case_id == case.id)
                .cloned()
                .unwrap_or_else(|| LiveRoomHarnessTrace::empty(&case.id));
            let harness_case = case.to_harness_case();
            let episode = runtime.start_episode(&harness_case);
            run_ids.push(episode.run_id.clone());
            runtime.attach_json_artifact(
                &episode.run_id,
                "live_room_trace",
                &serde_json::to_value(&trace)?,
            )?;
            let scorecard = grade_live_room_trace(&trace);
            runtime.attach_json_artifact(
                &episode.run_id,
                "live_room_scorecard",
                &serde_json::to_value(&scorecard)?,
            )?;
            runtime.finish_episode(
                &episode.run_id,
                if scorecard.passed {
                    HarnessVerdict::Pass
                } else {
                    HarnessVerdict::Fail
                },
            );
            scorecards.push(scorecard);
        }

        let average_score = if scorecards.is_empty() {
            0.0
        } else {
            scorecards.iter().map(|card| card.score).sum::<f64>() / scorecards.len() as f64
        };
        Ok(LiveRoomSuiteReport {
            passed: scorecards.iter().all(|card| card.passed),
            average_score,
            run_ids,
            scorecards,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRoomHarnessCase {
    pub id: String,
    pub title: String,
    pub platform: String,
    pub rooms: Vec<String>,
    pub prompt: String,
    #[serde(default)]
    pub assertions: Vec<String>,
}

impl LiveRoomHarnessCase {
    fn to_harness_case(&self) -> HarnessCase {
        HarnessCase {
            id: self.id.clone(),
            subject: HarnessSubject::Browser,
            title: self.title.clone(),
            prompt: self.prompt.clone(),
            setup: Vec::new(),
            policy: HarnessPolicy {
                permission_mode: "bypass".to_string(),
                allowed_tools: vec![
                    "browser_run_script".to_string(),
                    "browser_task".to_string(),
                    "gbrain_room_search".to_string(),
                    "gbrain_room_put_page".to_string(),
                ],
                allow_network: false,
                allow_memory_writes: true,
            },
            budgets: HarnessBudget {
                max_steps: 20,
                max_seconds: 120,
                max_tokens: Some(8000),
            },
            assertions: Vec::new(),
            graders: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRoomHarnessTrace {
    pub case_id: String,
    pub room_entered: bool,
    pub comments_scanned: bool,
    pub separate_concurrent_state: bool,
    pub scoped_gbrain_recall: bool,
    pub scoped_gbrain_write: bool,
    pub mute_after_two_warnings: bool,
    pub severe_remove_enabled: bool,
    pub auto_stopped_on_room_end: bool,
    pub user_stop_report_written: bool,
    pub final_report_written: bool,
    pub leaked_auth_material: bool,
}

impl LiveRoomHarnessTrace {
    fn empty(case_id: &str) -> Self {
        Self {
            case_id: case_id.to_string(),
            ..Self::default()
        }
    }

    fn passing_fixture_for_case(case: &LiveRoomHarnessCase) -> Self {
        Self {
            case_id: case.id.clone(),
            room_entered: true,
            comments_scanned: true,
            separate_concurrent_state: true,
            scoped_gbrain_recall: true,
            scoped_gbrain_write: true,
            mute_after_two_warnings: true,
            severe_remove_enabled: true,
            auto_stopped_on_room_end: true,
            user_stop_report_written: true,
            final_report_written: true,
            leaked_auth_material: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRoomGrade {
    pub verdict: &'static str,
    pub passed: bool,
    pub score: f64,
    pub failed_checks: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRoomSuiteReport {
    pub passed: bool,
    pub average_score: f64,
    pub run_ids: Vec<String>,
    pub scorecards: Vec<LiveRoomGrade>,
}

pub fn grade_live_room_trace(trace: &LiveRoomHarnessTrace) -> LiveRoomGrade {
    let checks = [
        ("room_entered", trace.room_entered),
        ("comments_scanned", trace.comments_scanned),
        ("separate_concurrent_state", trace.separate_concurrent_state),
        ("scoped_gbrain_recall", trace.scoped_gbrain_recall),
        ("scoped_gbrain_write", trace.scoped_gbrain_write),
        ("mute_after_two_warnings", trace.mute_after_two_warnings),
        ("severe_remove_enabled", trace.severe_remove_enabled),
        ("auto_stopped_on_room_end", trace.auto_stopped_on_room_end),
        ("user_stop_report_written", trace.user_stop_report_written),
        ("final_report_written", trace.final_report_written),
        ("auth_material_absent", !trace.leaked_auth_material),
    ];
    let failed_checks = checks
        .iter()
        .filter_map(|(id, passed)| if *passed { None } else { Some(*id) })
        .collect::<Vec<_>>();
    let score = (checks.len() - failed_checks.len()) as f64 / checks.len() as f64;
    let passed = failed_checks.is_empty();
    LiveRoomGrade {
        verdict: if passed { "pass" } else { "fail" },
        passed,
        score,
        failed_checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scorecard_requires_room_scope_and_moderation_evidence() {
        let trace = LiveRoomHarnessTrace {
            case_id: "live-room/douyin-moderator-fixture".to_string(),
            room_entered: true,
            comments_scanned: true,
            separate_concurrent_state: true,
            scoped_gbrain_recall: true,
            scoped_gbrain_write: true,
            mute_after_two_warnings: true,
            severe_remove_enabled: true,
            auto_stopped_on_room_end: true,
            user_stop_report_written: true,
            final_report_written: true,
            leaked_auth_material: false,
        };

        let grade = grade_live_room_trace(&trace);
        assert_eq!(grade.verdict, "pass");
        assert!(grade.failed_checks.is_empty());
    }

    #[test]
    fn scorecard_fails_when_auth_material_leaks() {
        let trace = LiveRoomHarnessTrace {
            case_id: "live-room/douyin-moderator-fixture".to_string(),
            leaked_auth_material: true,
            ..LiveRoomHarnessTrace::passing_fixture_for_case(&LiveRoomHarnessCase {
                id: "live-room/douyin-moderator-fixture".to_string(),
                title: "fixture".to_string(),
                platform: "douyin".to_string(),
                rooms: vec!["room-a".to_string()],
                prompt: String::new(),
                assertions: Vec::new(),
            })
        };

        let grade = grade_live_room_trace(&trace);
        assert_eq!(grade.verdict, "fail");
        assert!(grade.failed_checks.contains(&"auth_material_absent"));
    }

    #[test]
    fn loads_builtin_fixture_case() {
        let cases = LiveRoomHarnessAdapter::load_builtin_cases().unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].platform, "douyin");
        assert!(cases[0].rooms.contains(&"room-a".to_string()));
        assert!(cases[0].rooms.contains(&"room-b".to_string()));
    }
}
