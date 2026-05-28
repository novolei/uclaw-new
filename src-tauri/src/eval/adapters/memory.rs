use serde::{Deserialize, Serialize};

use crate::eval::adapters::{HarnessAdapter, MEMORY_ADAPTER_ID};
use crate::eval::case::{HarnessBudget, HarnessCase, HarnessPolicy, HarnessSubject};
use crate::eval::episode::HarnessVerdict;
use crate::eval::memory_inventory::{
    InventoryProbeStatus, MemoryInventorySmokeReport, MemoryInventoryTargetReport,
};
use crate::eval::runtime::EvalRuntime;
use crate::eval::trace::{EvalEvent, MemoryEvalTarget};

pub const BUILTIN_MEMORY_GBRAIN_CASES: &[&str] = &[
    include_str!("../cases/memory/memu-inventory.json"),
    include_str!("../cases/memory/gbrain-inventory.json"),
    include_str!("../cases/memory/gbrain-tooling.json"),
    include_str!("../cases/memory/dual-inventory-health.json"),
    include_str!("../cases/memory/memory-policy-freeze.json"),
    include_str!("../cases/memory/memory-policy-degraded.json"),
];

pub const BUILTIN_MEMORY_GBRAIN_RECALL_CASES: &[&str] = &[
    include_str!("../cases/memory/grounded-recall.json"),
    include_str!("../cases/memory/no-hallucinated-recall.json"),
    include_str!("../cases/memory/gbrain-page-grounding.json"),
];

#[derive(Debug, Default, Clone)]
pub struct MemoryGbrainEvalAdapter;

impl HarnessAdapter for MemoryGbrainEvalAdapter {
    fn subject(&self) -> HarnessSubject {
        HarnessSubject::Memory
    }

    fn adapter_id(&self) -> &'static str {
        MEMORY_ADAPTER_ID
    }
}

impl MemoryGbrainEvalAdapter {
    pub fn load_builtin_cases() -> Result<Vec<MemoryGbrainEvalCase>, serde_json::Error> {
        BUILTIN_MEMORY_GBRAIN_CASES
            .iter()
            .map(|raw| serde_json::from_str(raw))
            .collect()
    }

    pub fn load_builtin_recall_cases() -> Result<Vec<MemoryGbrainEvalCase>, serde_json::Error> {
        BUILTIN_MEMORY_GBRAIN_RECALL_CASES
            .iter()
            .map(|raw| serde_json::from_str(raw))
            .collect()
    }

    pub fn score_report(
        &self,
        case: MemoryGbrainEvalCase,
        report: &MemoryInventorySmokeReport,
    ) -> MemoryGbrainScorecard {
        score_memory_gbrain_case(case, report)
    }

    pub fn score_eval(
        &self,
        case: MemoryGbrainEvalCase,
        input: &MemoryGbrainEvalInput,
    ) -> MemoryGbrainScorecard {
        score_memory_gbrain_case_with_evidence(case, input)
    }

    pub fn run_inventory_suite(
        &self,
        runtime: &EvalRuntime,
        report: &MemoryInventorySmokeReport,
    ) -> anyhow::Result<MemoryGbrainSuiteReport> {
        let cases = Self::load_builtin_cases()?;
        self.run_suite(
            runtime,
            &MemoryGbrainEvalInput {
                inventory: report.clone(),
                evidence: MemoryGbrainEvalEvidence::default(),
            },
            cases,
        )
    }

    pub fn run_eval_suite(
        &self,
        runtime: &EvalRuntime,
        input: &MemoryGbrainEvalInput,
    ) -> anyhow::Result<MemoryGbrainSuiteReport> {
        let mut cases = Self::load_builtin_cases()?;
        cases.extend(Self::load_builtin_recall_cases()?);
        self.run_suite(runtime, input, cases)
    }

    pub fn run_suite(
        &self,
        runtime: &EvalRuntime,
        input: &MemoryGbrainEvalInput,
        cases: Vec<MemoryGbrainEvalCase>,
    ) -> anyhow::Result<MemoryGbrainSuiteReport> {
        let mut scorecards = Vec::new();
        let mut run_ids = Vec::new();
        for case in cases {
            let harness_case = case.to_harness_case();
            let episode = runtime.start_episode(&harness_case);
            run_ids.push(episode.run_id.clone());
            runtime.append_event(
                &episode.run_id,
                EvalEvent::MemoryRecall {
                    ts: chrono::Utc::now().to_rfc3339(),
                    target: case.memory_target(),
                    artifact_ref: "memory_gbrain_eval_input".to_string(),
                },
            );
            runtime.attach_json_artifact(
                &episode.run_id,
                "memory_gbrain_eval_input",
                &serde_json::to_value(input)?,
            )?;
            let scorecard = self.score_eval(case, input);
            runtime.attach_json_artifact(
                &episode.run_id,
                "memory_gbrain_scorecard",
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
        Ok(MemoryGbrainSuiteReport {
            passed: scorecards.iter().all(|card| card.passed),
            average_score,
            run_ids,
            scorecards,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGbrainEvalCase {
    pub id: String,
    pub title: String,
    pub target: MemoryGbrainEvalTarget,
    pub prompt: String,
    #[serde(default)]
    pub allow_empty: bool,
    #[serde(default)]
    pub require_connected: bool,
    #[serde(default)]
    pub min_tool_count: Option<u64>,
    #[serde(default)]
    pub require_sample_keys: bool,
    #[serde(default)]
    pub require_write_receipt: bool,
    #[serde(default)]
    pub require_recall_evidence: bool,
    #[serde(default)]
    pub expected_facts: Vec<String>,
    #[serde(default)]
    pub forbidden_facts: Vec<String>,
}

impl MemoryGbrainEvalCase {
    fn to_harness_case(&self) -> HarnessCase {
        HarnessCase {
            id: self.id.clone(),
            subject: match self.target {
                MemoryGbrainEvalTarget::Memu => HarnessSubject::Memory,
                MemoryGbrainEvalTarget::Gbrain => HarnessSubject::Gbrain,
                MemoryGbrainEvalTarget::Both => HarnessSubject::Memory,
            },
            title: self.title.clone(),
            prompt: self.prompt.clone(),
            setup: Vec::new(),
            policy: HarnessPolicy {
                permission_mode: "bypass".to_string(),
                allowed_tools: vec![
                    "memu_memory".to_string(),
                    "mcp__gbrain__list_pages".to_string(),
                ],
                allow_network: false,
                allow_memory_writes: false,
            },
            budgets: HarnessBudget {
                max_steps: 4,
                max_seconds: 30,
                max_tokens: None,
            },
            assertions: Vec::new(),
            graders: Vec::new(),
        }
    }

    fn memory_target(&self) -> MemoryEvalTarget {
        match self.target {
            MemoryGbrainEvalTarget::Gbrain => MemoryEvalTarget::Gbrain,
            MemoryGbrainEvalTarget::Memu | MemoryGbrainEvalTarget::Both => {
                MemoryEvalTarget::MemorySystem
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryGbrainEvalTarget {
    Memu,
    Gbrain,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGbrainEvalInput {
    pub inventory: MemoryInventorySmokeReport,
    #[serde(default)]
    pub evidence: MemoryGbrainEvalEvidence,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGbrainEvalEvidence {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_receipts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memu_recall_texts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gbrain_recall_texts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gbrain_page_texts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub correction_adoption_texts: Vec<String>,
}

impl MemoryGbrainEvalEvidence {
    fn searchable_text_for_target(&self, target: MemoryGbrainEvalTarget) -> String {
        let mut chunks = Vec::new();
        match target {
            MemoryGbrainEvalTarget::Memu => {
                chunks.extend(self.memu_recall_texts.iter().map(String::as_str));
            }
            MemoryGbrainEvalTarget::Gbrain => {
                chunks.extend(self.gbrain_recall_texts.iter().map(String::as_str));
                chunks.extend(self.gbrain_page_texts.iter().map(String::as_str));
            }
            MemoryGbrainEvalTarget::Both => {
                chunks.extend(self.memu_recall_texts.iter().map(String::as_str));
                chunks.extend(self.gbrain_recall_texts.iter().map(String::as_str));
                chunks.extend(self.gbrain_page_texts.iter().map(String::as_str));
            }
        }
        chunks.extend(self.correction_adoption_texts.iter().map(String::as_str));
        chunks.join("\n").to_lowercase()
    }

    fn has_recall_for_target(&self, target: MemoryGbrainEvalTarget) -> bool {
        match target {
            MemoryGbrainEvalTarget::Memu => !self.memu_recall_texts.is_empty(),
            MemoryGbrainEvalTarget::Gbrain => {
                !self.gbrain_recall_texts.is_empty() || !self.gbrain_page_texts.is_empty()
            }
            MemoryGbrainEvalTarget::Both => {
                !self.memu_recall_texts.is_empty()
                    && (!self.gbrain_recall_texts.is_empty() || !self.gbrain_page_texts.is_empty())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGbrainSuiteReport {
    pub passed: bool,
    pub average_score: f64,
    pub run_ids: Vec<String>,
    pub scorecards: Vec<MemoryGbrainScorecard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGbrainScorecard {
    pub case_id: String,
    pub title: String,
    pub target: MemoryGbrainEvalTarget,
    pub passed: bool,
    pub score: f64,
    pub checks: Vec<MemoryGbrainCheckResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGbrainCheckResult {
    pub id: String,
    pub passed: bool,
    pub score: f64,
    pub message: String,
}

pub fn score_memory_gbrain_case(
    case: MemoryGbrainEvalCase,
    report: &MemoryInventorySmokeReport,
) -> MemoryGbrainScorecard {
    score_memory_gbrain_case_with_evidence(
        case,
        &MemoryGbrainEvalInput {
            inventory: report.clone(),
            evidence: MemoryGbrainEvalEvidence::default(),
        },
    )
}

pub fn score_memory_gbrain_case_with_evidence(
    case: MemoryGbrainEvalCase,
    input: &MemoryGbrainEvalInput,
) -> MemoryGbrainScorecard {
    let mut checks = Vec::new();
    match case.target {
        MemoryGbrainEvalTarget::Memu => score_target(&case, &input.inventory.memu, &mut checks),
        MemoryGbrainEvalTarget::Gbrain => score_target(&case, &input.inventory.gbrain, &mut checks),
        MemoryGbrainEvalTarget::Both => {
            score_target(&case, &input.inventory.memu, &mut checks);
            score_target(&case, &input.inventory.gbrain, &mut checks);
            checks.push(check(
                "overall_report_ok",
                input.inventory.ok,
                format!("inventory smoke ok={}", input.inventory.ok),
            ));
        }
    }
    score_evidence(&case, &input.evidence, &mut checks);

    let score = if checks.is_empty() {
        0.0
    } else {
        checks.iter().map(|check| check.score).sum::<f64>() / checks.len() as f64
    };
    let passed = checks.iter().all(|check| check.passed);
    MemoryGbrainScorecard {
        case_id: case.id,
        title: case.title,
        target: case.target,
        passed,
        score,
        checks,
    }
}

fn score_evidence(
    case: &MemoryGbrainEvalCase,
    evidence: &MemoryGbrainEvalEvidence,
    checks: &mut Vec<MemoryGbrainCheckResult>,
) {
    if case.require_write_receipt {
        checks.push(check(
            "write_receipt",
            !evidence.write_receipts.is_empty(),
            format!("write_receipts={}", evidence.write_receipts.len()),
        ));
    }

    if case.require_recall_evidence {
        checks.push(check(
            "recall_evidence",
            evidence.has_recall_for_target(case.target),
            format!(
                "memu_recall={}, gbrain_recall={}, gbrain_pages={}",
                evidence.memu_recall_texts.len(),
                evidence.gbrain_recall_texts.len(),
                evidence.gbrain_page_texts.len()
            ),
        ));
    }

    if !case.expected_facts.is_empty() {
        let haystack = evidence.searchable_text_for_target(case.target);
        for fact in &case.expected_facts {
            let normalized = fact.to_lowercase();
            checks.push(check(
                format!("expected_fact:{fact}"),
                haystack.contains(&normalized),
                format!("expected fact present: {fact}"),
            ));
        }
    }

    if !case.forbidden_facts.is_empty() {
        let haystack = evidence.searchable_text_for_target(case.target);
        for fact in &case.forbidden_facts {
            let normalized = fact.to_lowercase();
            checks.push(check(
                format!("forbidden_fact:{fact}"),
                !haystack.contains(&normalized),
                format!("forbidden fact absent: {fact}"),
            ));
        }
    }
}

fn score_target(
    case: &MemoryGbrainEvalCase,
    target: &MemoryInventoryTargetReport,
    checks: &mut Vec<MemoryGbrainCheckResult>,
) {
    let reachable = match target.status {
        InventoryProbeStatus::Pass => true,
        InventoryProbeStatus::Empty => case.allow_empty,
        InventoryProbeStatus::Unavailable | InventoryProbeStatus::Error => false,
    };
    checks.push(check(
        format!("{}:reachable", target.target),
        reachable,
        format!(
            "{} status={:?}, item_count={}",
            target.target, target.status, target.item_count
        ),
    ));

    if case.require_connected {
        checks.push(check(
            format!("{}:connected", target.target),
            !matches!(
                target.status,
                InventoryProbeStatus::Unavailable | InventoryProbeStatus::Error
            ),
            target
                .detail
                .clone()
                .unwrap_or_else(|| format!("{} status={:?}", target.target, target.status)),
        ));
    }

    if let Some(min_tool_count) = case.min_tool_count.filter(|_| target.tool_count.is_some()) {
        let tool_count = target.tool_count.unwrap_or(0);
        checks.push(check(
            format!("{}:tool_count", target.target),
            tool_count >= min_tool_count,
            format!("expected >= {min_tool_count} tools, got {tool_count}"),
        ));
    }

    if case.require_sample_keys {
        checks.push(check(
            format!("{}:sample_keys", target.target),
            !target.sample_keys.is_empty(),
            format!("sample_keys={:?}", target.sample_keys),
        ));
    }
}

fn check(
    id: impl Into<String>,
    passed: bool,
    message: impl Into<String>,
) -> MemoryGbrainCheckResult {
    MemoryGbrainCheckResult {
        id: id.into(),
        passed,
        score: if passed { 1.0 } else { 0.0 },
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn target(
        name: &str,
        status: InventoryProbeStatus,
        item_count: u64,
        tool_count: Option<u64>,
        samples: Vec<&str>,
    ) -> MemoryInventoryTargetReport {
        MemoryInventoryTargetReport {
            target: name.to_string(),
            status,
            item_count,
            category_count: None,
            tool_count,
            sample_keys: samples.into_iter().map(ToString::to_string).collect(),
            detail: None,
        }
    }

    fn report() -> MemoryInventorySmokeReport {
        MemoryInventorySmokeReport {
            ok: true,
            generated_at: "2026-05-20T00:00:00Z".into(),
            memu: target("memu", InventoryProbeStatus::Empty, 0, None, vec![]),
            gbrain: target(
                "gbrain",
                InventoryProbeStatus::Pass,
                4,
                Some(5),
                vec!["people/ryanliu"],
            ),
            observations: vec![],
        }
    }

    #[test]
    fn loads_builtin_memory_gbrain_eval_cases() {
        let cases = MemoryGbrainEvalAdapter::load_builtin_cases().unwrap();
        assert_eq!(cases.len(), 6);
        assert!(cases
            .iter()
            .any(|case| matches!(case.target, MemoryGbrainEvalTarget::Both)));
        assert!(cases.iter().any(|case| case.id == "memory.policy.freeze"));
        assert!(cases.iter().any(|case| case.id == "memory.policy.degraded"));
    }

    #[test]
    fn scores_empty_memu_as_pass_when_allowed() {
        let case = MemoryGbrainEvalCase {
            id: "memu.inventory".into(),
            title: "memU inventory".into(),
            target: MemoryGbrainEvalTarget::Memu,
            prompt: "Check memU inventory".into(),
            allow_empty: true,
            require_connected: true,
            min_tool_count: None,
            require_sample_keys: false,
            require_write_receipt: false,
            require_recall_evidence: false,
            expected_facts: Vec::new(),
            forbidden_facts: Vec::new(),
        };
        let scorecard = score_memory_gbrain_case(case, &report());
        assert!(scorecard.passed, "{scorecard:#?}");
    }

    #[test]
    fn catches_gbrain_tool_exposure_regression() {
        let mut report = report();
        report.gbrain.tool_count = Some(0);
        let case = MemoryGbrainEvalCase {
            id: "gbrain.tooling".into(),
            title: "gbrain tooling".into(),
            target: MemoryGbrainEvalTarget::Gbrain,
            prompt: "Check gbrain tools".into(),
            allow_empty: true,
            require_connected: true,
            min_tool_count: Some(4),
            require_sample_keys: false,
            require_write_receipt: false,
            require_recall_evidence: false,
            expected_facts: Vec::new(),
            forbidden_facts: Vec::new(),
        };
        let scorecard = score_memory_gbrain_case(case, &report);
        assert!(!scorecard.passed);
        assert!(scorecard
            .checks
            .iter()
            .any(|check| check.id == "gbrain:tool_count" && !check.passed));
    }

    #[test]
    fn run_suite_records_memory_recall_trace_and_scorecard_artifact() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = EvalRuntime::new(tmp.path());
        let adapter = MemoryGbrainEvalAdapter;
        let case = MemoryGbrainEvalCase {
            id: "gbrain.inventory".into(),
            title: "gbrain inventory".into(),
            target: MemoryGbrainEvalTarget::Gbrain,
            prompt: "Check gbrain inventory".into(),
            allow_empty: true,
            require_connected: true,
            min_tool_count: Some(4),
            require_sample_keys: true,
            require_write_receipt: false,
            require_recall_evidence: false,
            expected_facts: Vec::new(),
            forbidden_facts: Vec::new(),
        };

        let suite = adapter
            .run_suite(
                &runtime,
                &MemoryGbrainEvalInput {
                    inventory: report(),
                    evidence: MemoryGbrainEvalEvidence::default(),
                },
                vec![case],
            )
            .unwrap();

        assert!(suite.passed, "{suite:#?}");
        let episode = runtime.get_episode(&suite.run_ids[0]).unwrap();
        assert_eq!(episode.verdict, HarnessVerdict::Pass);
        assert!(episode
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == "memory_gbrain_scorecard"));
        assert!(episode
            .trace
            .iter()
            .any(|event| event.kind() == "memory_recall"));
        let scorecard_artifact = episode
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == "memory_gbrain_scorecard")
            .unwrap();
        let artifact = std::fs::read_to_string(&scorecard_artifact.path).unwrap();
        assert!(artifact.contains("people/ryanliu"));
        assert!(json!(suite.scorecards[0]).is_object());
    }

    #[test]
    fn recall_grounding_case_requires_written_and_recalled_fact() {
        let case = MemoryGbrainEvalCase {
            id: "memory.recall.grounded".into(),
            title: "Grounded recall".into(),
            target: MemoryGbrainEvalTarget::Both,
            prompt: "Write and recall a known fact".into(),
            allow_empty: true,
            require_connected: true,
            min_tool_count: Some(4),
            require_sample_keys: false,
            require_write_receipt: true,
            require_recall_evidence: true,
            expected_facts: vec!["Browser task observed Apple homepage".into()],
            forbidden_facts: vec!["User lives on Mars".into()],
        };
        let scorecard = score_memory_gbrain_case_with_evidence(
            case,
            &MemoryGbrainEvalInput {
                inventory: report(),
                evidence: MemoryGbrainEvalEvidence {
                    write_receipts: vec!["memu:ok".into(), "gbrain:ok".into()],
                    memu_recall_texts: vec!["Browser task observed Apple homepage".into()],
                    gbrain_recall_texts: vec!["Browser task observed Apple homepage".into()],
                    ..Default::default()
                },
            },
        );

        assert!(scorecard.passed, "{scorecard:#?}");
    }

    #[test]
    fn recall_grounding_case_rejects_hallucinated_fact() {
        let case = MemoryGbrainEvalCase {
            id: "memory.recall.no_hallucination".into(),
            title: "No hallucinated recall".into(),
            target: MemoryGbrainEvalTarget::Memu,
            prompt: "Recall only stored facts".into(),
            allow_empty: true,
            require_connected: false,
            min_tool_count: None,
            require_sample_keys: false,
            require_write_receipt: false,
            require_recall_evidence: true,
            expected_facts: vec!["天津大学毕业".into()],
            forbidden_facts: vec!["北京大学毕业".into()],
        };
        let scorecard = score_memory_gbrain_case_with_evidence(
            case,
            &MemoryGbrainEvalInput {
                inventory: report(),
                evidence: MemoryGbrainEvalEvidence {
                    memu_recall_texts: vec!["用户是天津大学毕业，也可能是北京大学毕业".into()],
                    ..Default::default()
                },
            },
        );

        assert!(!scorecard.passed);
        assert!(scorecard
            .checks
            .iter()
            .any(|check| check.id == "forbidden_fact:北京大学毕业" && !check.passed));
    }
}
