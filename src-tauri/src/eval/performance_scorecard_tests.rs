use serde_json::json;

use super::*;
use crate::eval::case::{HarnessBudget, HarnessCase, HarnessPolicy};

fn visible_ready_samples() -> Vec<PerformanceSample> {
    vec![
        PerformanceSample::milliseconds("startup.visible_ready", 10.0),
        PerformanceSample::milliseconds("startup.visible_ready", 20.0),
        PerformanceSample::milliseconds("startup.visible_ready", 30.0),
        PerformanceSample::milliseconds("startup.visible_ready", 40.0),
    ]
}

#[test]
fn metric_summary_uses_deterministic_nearest_rank_percentiles() {
    let samples = visible_ready_samples();

    let summary =
        PerformanceMetricSummary::from_samples("startup.visible_ready", &samples).unwrap();

    assert_eq!(summary.metric, "startup.visible_ready");
    assert_eq!(summary.unit, "ms");
    assert_eq!(summary.sample_count, 4);
    assert_eq!(summary.min, 10.0);
    assert_eq!(summary.max, 40.0);
    assert_eq!(summary.avg, 25.0);
    assert_eq!(summary.p50, 20.0);
    assert_eq!(summary.p95, 40.0);
    assert_eq!(summary.p99, 40.0);
}

#[test]
fn metric_summary_filters_by_metric() {
    let mut samples = visible_ready_samples();
    samples.push(PerformanceSample::bytes("tool.output_bytes", 500.0));

    let summary = PerformanceMetricSummary::from_samples("tool.output_bytes", &samples).unwrap();

    assert_eq!(summary.sample_count, 1);
    assert_eq!(summary.unit, "bytes");
    assert_eq!(summary.p95, 500.0);
    assert!(PerformanceMetricSummary::from_samples("missing", &samples).is_none());
}

#[test]
fn metric_summary_rejects_non_finite_samples() {
    let samples = vec![
        PerformanceSample::milliseconds("startup.visible_ready", 10.0),
        PerformanceSample::milliseconds("startup.visible_ready", f64::NAN),
    ];

    assert!(PerformanceMetricSummary::from_samples("startup.visible_ready", &samples).is_some());
    let case = PerformanceCaseScore::from_samples(
        "non-finite",
        HarnessSubject::Tasks,
        samples,
        &[PerformanceThreshold::new(
            "startup.visible_ready",
            50.0,
            100.0,
        )],
    );

    assert_eq!(case.verdict, PerformanceVerdict::Fail);
}

#[test]
fn metric_summary_rejects_mixed_units_or_directions() {
    let mixed_units = vec![
        PerformanceSample::milliseconds("tool.search.latency", 10.0),
        PerformanceSample::new("tool.search.latency", 20.0, "seconds", true),
    ];
    let mixed_directions = vec![
        PerformanceSample::new("cache.hit_rate", 0.9, "ratio", false),
        PerformanceSample::new("cache.hit_rate", 0.8, "ratio", true),
    ];

    assert!(PerformanceMetricSummary::from_samples("tool.search.latency", &mixed_units).is_none());
    assert!(PerformanceMetricSummary::from_samples("cache.hit_rate", &mixed_directions).is_none());
}

#[test]
fn metric_summary_serializes_camel_case() {
    let samples = visible_ready_samples();
    let summary =
        PerformanceMetricSummary::from_samples("startup.visible_ready", &samples).unwrap();

    let value = serde_json::to_value(&summary).unwrap();

    assert_eq!(value["sampleCount"], 4);
    assert_eq!(value["metric"], "startup.visible_ready");
    assert_eq!(value["p95"], 40.0);
}

#[test]
fn threshold_verdicts_support_lower_and_higher_is_better() {
    let lower = PerformanceThreshold::new("tool.search.latency_ms", 50.0, 100.0);
    assert_eq!(
        PerformanceVerdict::from_value(25.0, &lower),
        PerformanceVerdict::Pass
    );
    assert_eq!(
        PerformanceVerdict::from_value(75.0, &lower),
        PerformanceVerdict::Warn
    );
    assert_eq!(
        PerformanceVerdict::from_value(125.0, &lower),
        PerformanceVerdict::Fail
    );

    let higher = PerformanceThreshold::higher_is_better("cache.hit_rate", 0.80, 0.50);
    assert_eq!(
        PerformanceVerdict::from_value(0.90, &higher),
        PerformanceVerdict::Pass
    );
    assert_eq!(
        PerformanceVerdict::from_value(0.70, &higher),
        PerformanceVerdict::Warn
    );
    assert_eq!(
        PerformanceVerdict::from_value(0.40, &higher),
        PerformanceVerdict::Fail
    );
}

#[test]
fn threshold_verdicts_fail_closed_for_invalid_values_and_thresholds() {
    let lower = PerformanceThreshold::new("tool.search.latency_ms", 50.0, 100.0);

    assert_eq!(
        PerformanceVerdict::from_value(f64::NAN, &lower),
        PerformanceVerdict::Fail
    );
    assert_eq!(
        PerformanceVerdict::from_value(f64::INFINITY, &lower),
        PerformanceVerdict::Fail
    );
    assert_eq!(
        PerformanceVerdict::from_value(f64::NEG_INFINITY, &lower),
        PerformanceVerdict::Fail
    );
    assert!(PerformanceThreshold::try_new("latency", 50.0, 100.0).is_some());
    assert!(PerformanceThreshold::try_new("latency", 100.0, 50.0).is_none());
    assert!(PerformanceThreshold::try_higher_is_better("hit_rate", 0.80, 0.50).is_some());
    assert!(PerformanceThreshold::try_higher_is_better("hit_rate", 0.50, 0.80).is_none());
    assert!(PerformanceThreshold::try_new("latency", f64::NAN, 100.0).is_none());

    let invalid_threshold = PerformanceThreshold::new("latency", 100.0, 50.0);
    assert_eq!(
        PerformanceVerdict::from_value(10.0, &invalid_threshold),
        PerformanceVerdict::Fail
    );
}

#[test]
fn case_score_combines_thresholds_by_worst_verdict() {
    let samples = vec![
        PerformanceSample::milliseconds("tool.search.latency_ms", 20.0),
        PerformanceSample::milliseconds("tool.search.latency_ms", 75.0),
        PerformanceSample::bytes("tool.output_bytes", 1_000.0),
    ];
    let thresholds = vec![
        PerformanceThreshold::new("tool.search.latency_ms", 50.0, 100.0),
        PerformanceThreshold::new("tool.output_bytes", 8_000.0, 16_000.0),
    ];

    let case = PerformanceCaseScore::from_samples(
        "search-small",
        HarnessSubject::Tools,
        samples,
        &thresholds,
    );

    assert_eq!(case.case_id, "search-small");
    assert_eq!(case.subject, HarnessSubject::Tools);
    assert_eq!(case.summaries.len(), 2);
    assert_eq!(case.verdict, PerformanceVerdict::Warn);
}

#[test]
fn scorecard_summary_counts_case_verdicts() {
    let pass = PerformanceCaseScore::from_samples(
        "pass",
        HarnessSubject::Tools,
        vec![PerformanceSample::milliseconds("latency", 10.0)],
        &[PerformanceThreshold::new("latency", 50.0, 100.0)],
    );
    let fail = PerformanceCaseScore::from_samples(
        "fail",
        HarnessSubject::Tools,
        vec![PerformanceSample::milliseconds("latency", 150.0)],
        &[PerformanceThreshold::new("latency", 50.0, 100.0)],
    );

    let scorecard = PerformanceScorecard::new("tool-smoke", "commit-1", vec![pass, fail]);

    assert_eq!(
        scorecard.schema_version,
        PERFORMANCE_SCORECARD_SCHEMA_VERSION
    );
    assert_eq!(scorecard.artifact_kind, "performance_scorecard");
    assert_eq!(scorecard.summary.case_count, 2);
    assert_eq!(scorecard.summary.pass_count, 1);
    assert_eq!(scorecard.summary.fail_count, 1);
    assert_eq!(scorecard.summary.overall_verdict, PerformanceVerdict::Fail);
}

#[test]
fn performance_scorecard_attaches_as_harness_artifact() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = HarnessRuntime::new(tmp.path());
    let case = HarnessCase {
        id: "perf-case".into(),
        subject: HarnessSubject::Tools,
        title: "Tool performance smoke".into(),
        prompt: "Measure deterministic tool latency".into(),
        setup: vec![],
        policy: HarnessPolicy::default(),
        budgets: HarnessBudget::default(),
        assertions: vec![],
        graders: vec![],
    };
    let episode = runtime.start_episode(&case);
    let score = PerformanceCaseScore::from_samples(
        "tool-search",
        HarnessSubject::Tools,
        vec![PerformanceSample::milliseconds(
            "tool.search.latency_ms",
            42.0,
        )],
        &[PerformanceThreshold::new(
            "tool.search.latency_ms",
            50.0,
            100.0,
        )],
    );
    let scorecard = PerformanceScorecard::new("tool-smoke", "commit-1", vec![score]);

    let artifact = attach_performance_scorecard(&runtime, &episode.run_id, &scorecard)
        .unwrap()
        .unwrap();

    assert_eq!(artifact.kind, "performance_scorecard");
    let body = std::fs::read_to_string(&artifact.path).unwrap();
    assert!(body.contains("performance_scorecard"), "{body}");
    assert!(body.contains("tool.search.latency_ms"), "{body}");
    let stored = runtime.get_episode(&episode.run_id).unwrap();
    assert_eq!(stored.artifacts.len(), 1);
}

#[test]
fn scorecard_json_preserves_operational_fields() {
    let score = PerformanceCaseScore::from_samples(
        "projection-replay",
        HarnessSubject::Tasks,
        vec![PerformanceSample::milliseconds(
            "projection.replay.latency_ms",
            12.0,
        )],
        &[PerformanceThreshold::new(
            "projection.replay.latency_ms",
            25.0,
            50.0,
        )],
    );
    let scorecard = PerformanceScorecard::new("projection", "commit-1", vec![score]);

    let value = scorecard.to_json_value().unwrap();

    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["suiteId"], "projection");
    assert_eq!(value["artifactKind"], "performance_scorecard");
    assert_eq!(value["summary"]["overallVerdict"], "pass");
    assert_eq!(value["cases"][0]["subject"], "tasks");
    assert_ne!(value["generatedAt"], json!(null));
}
