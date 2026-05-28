use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::eval::artifacts::{ArtifactStoreError, HarnessArtifact};
use crate::eval::case::EvalSubject;
use crate::eval::runtime::EvalRuntime;

pub const PERFORMANCE_SCORECARD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceVerdict {
    Pass,
    Warn,
    Fail,
}

impl PerformanceVerdict {
    pub fn from_value(value: f64, threshold: &PerformanceThreshold) -> Self {
        if !value.is_finite() || !threshold.is_valid() {
            return Self::Fail;
        }

        if threshold.lower_is_better {
            if value >= threshold.fail_at {
                Self::Fail
            } else if value >= threshold.warn_at {
                Self::Warn
            } else {
                Self::Pass
            }
        } else if value <= threshold.fail_at {
            Self::Fail
        } else if value <= threshold.warn_at {
            Self::Warn
        } else {
            Self::Pass
        }
    }

    pub fn combine(verdicts: impl IntoIterator<Item = PerformanceVerdict>) -> Self {
        let mut combined = Self::Pass;
        for verdict in verdicts {
            combined = match (combined, verdict) {
                (Self::Fail, _) | (_, Self::Fail) => Self::Fail,
                (Self::Warn, _) | (_, Self::Warn) => Self::Warn,
                _ => Self::Pass,
            };
        }
        combined
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceSample {
    pub metric: String,
    pub value: f64,
    pub unit: String,
    pub lower_is_better: bool,
}

impl PerformanceSample {
    pub fn new(
        metric: impl Into<String>,
        value: f64,
        unit: impl Into<String>,
        lower_is_better: bool,
    ) -> Self {
        Self {
            metric: metric.into(),
            value,
            unit: unit.into(),
            lower_is_better,
        }
    }

    pub fn milliseconds(metric: impl Into<String>, value: f64) -> Self {
        Self::new(metric, value, "ms", true)
    }

    pub fn bytes(metric: impl Into<String>, value: f64) -> Self {
        Self::new(metric, value, "bytes", true)
    }

    pub fn tokens(metric: impl Into<String>, value: f64) -> Self {
        Self::new(metric, value, "tokens", true)
    }

    fn has_finite_value(&self) -> bool {
        self.value.is_finite()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceMetricSummary {
    pub metric: String,
    pub unit: String,
    pub sample_count: usize,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

impl PerformanceMetricSummary {
    pub fn from_samples(metric: &str, samples: &[PerformanceSample]) -> Option<Self> {
        let matching = samples
            .iter()
            .filter(|sample| sample.metric == metric)
            .collect::<Vec<_>>();
        let first = matching.first()?;
        let unit = first.unit.clone();
        let lower_is_better = first.lower_is_better;
        if matching
            .iter()
            .any(|sample| sample.unit != unit || sample.lower_is_better != lower_is_better)
        {
            return None;
        }

        let mut values = samples
            .iter()
            .filter(|sample| sample.metric == metric)
            .filter(|sample| sample.has_finite_value())
            .map(|sample| sample.value)
            .collect::<Vec<_>>();
        if values.is_empty() {
            return None;
        }
        values.sort_by(f64::total_cmp);
        let sample_count = values.len();
        let sum = values.iter().sum::<f64>();
        Some(Self {
            metric: metric.to_string(),
            unit,
            sample_count,
            min: values[0],
            max: values[sample_count - 1],
            avg: sum / sample_count as f64,
            p50: percentile(&values, 0.50),
            p95: percentile(&values, 0.95),
            p99: percentile(&values, 0.99),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceThreshold {
    pub metric: String,
    pub warn_at: f64,
    pub fail_at: f64,
    pub lower_is_better: bool,
}

impl PerformanceThreshold {
    pub fn new(metric: impl Into<String>, warn_at: f64, fail_at: f64) -> Self {
        Self {
            metric: metric.into(),
            warn_at,
            fail_at,
            lower_is_better: true,
        }
    }

    pub fn higher_is_better(metric: impl Into<String>, warn_at: f64, fail_at: f64) -> Self {
        Self {
            metric: metric.into(),
            warn_at,
            fail_at,
            lower_is_better: false,
        }
    }

    pub fn try_new(metric: impl Into<String>, warn_at: f64, fail_at: f64) -> Option<Self> {
        let threshold = Self::new(metric, warn_at, fail_at);
        threshold.is_valid().then_some(threshold)
    }

    pub fn try_higher_is_better(
        metric: impl Into<String>,
        warn_at: f64,
        fail_at: f64,
    ) -> Option<Self> {
        let threshold = Self::higher_is_better(metric, warn_at, fail_at);
        threshold.is_valid().then_some(threshold)
    }

    pub fn is_valid(&self) -> bool {
        if !self.warn_at.is_finite() || !self.fail_at.is_finite() {
            return false;
        }

        if self.lower_is_better {
            self.warn_at <= self.fail_at
        } else {
            self.warn_at >= self.fail_at
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceCaseScore {
    pub case_id: String,
    pub subject: EvalSubject,
    pub samples: Vec<PerformanceSample>,
    pub summaries: Vec<PerformanceMetricSummary>,
    pub verdict: PerformanceVerdict,
}

impl PerformanceCaseScore {
    pub fn from_samples(
        case_id: impl Into<String>,
        subject: EvalSubject,
        samples: Vec<PerformanceSample>,
        thresholds: &[PerformanceThreshold],
    ) -> Self {
        let metrics = unique_metrics(&samples);
        let summaries = metrics
            .iter()
            .filter_map(|metric| PerformanceMetricSummary::from_samples(metric, &samples))
            .collect::<Vec<_>>();
        let mut threshold_verdicts = thresholds
            .iter()
            .filter_map(|threshold| {
                summaries
                    .iter()
                    .find(|summary| summary.metric == threshold.metric)
                    .map(|summary| PerformanceVerdict::from_value(summary.p95, threshold))
            })
            .collect::<Vec<_>>();
        if metrics
            .iter()
            .any(|metric| metric_has_invalid_samples(metric, &samples))
            || thresholds.iter().any(|threshold| !threshold.is_valid())
        {
            threshold_verdicts.push(PerformanceVerdict::Fail);
        }
        let verdict = if threshold_verdicts.is_empty() {
            PerformanceVerdict::Pass
        } else {
            PerformanceVerdict::combine(threshold_verdicts)
        };
        Self {
            case_id: case_id.into(),
            subject,
            samples,
            summaries,
            verdict,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceScorecard {
    pub schema_version: u32,
    pub suite_id: String,
    pub generated_at: String,
    pub commit: String,
    pub artifact_kind: String,
    pub cases: Vec<PerformanceCaseScore>,
    pub summary: PerformanceScorecardSummary,
}

impl PerformanceScorecard {
    pub fn new(
        suite_id: impl Into<String>,
        commit: impl Into<String>,
        cases: Vec<PerformanceCaseScore>,
    ) -> Self {
        let summary = PerformanceScorecardSummary::from_cases(&cases);
        Self {
            schema_version: PERFORMANCE_SCORECARD_SCHEMA_VERSION,
            suite_id: suite_id.into(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            commit: commit.into(),
            artifact_kind: "performance_scorecard".to_string(),
            cases,
            summary,
        }
    }

    pub fn to_json_value(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceScorecardSummary {
    pub case_count: usize,
    pub pass_count: usize,
    pub warn_count: usize,
    pub fail_count: usize,
    pub overall_verdict: PerformanceVerdict,
}

impl PerformanceScorecardSummary {
    pub fn from_cases(cases: &[PerformanceCaseScore]) -> Self {
        let pass_count = cases
            .iter()
            .filter(|case| case.verdict == PerformanceVerdict::Pass)
            .count();
        let warn_count = cases
            .iter()
            .filter(|case| case.verdict == PerformanceVerdict::Warn)
            .count();
        let fail_count = cases
            .iter()
            .filter(|case| case.verdict == PerformanceVerdict::Fail)
            .count();
        Self {
            case_count: cases.len(),
            pass_count,
            warn_count,
            fail_count,
            overall_verdict: PerformanceVerdict::combine(cases.iter().map(|case| case.verdict)),
        }
    }
}

pub fn attach_performance_scorecard(
    runtime: &EvalRuntime,
    run_id: &str,
    scorecard: &PerformanceScorecard,
) -> Result<Option<HarnessArtifact>, ArtifactStoreError> {
    let value = scorecard
        .to_json_value()
        .map_err(ArtifactStoreError::Serde)?;
    runtime.attach_json_artifact(run_id, "performance_scorecard", &value)
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    debug_assert!(!values.is_empty());
    let rank = ((percentile * values.len() as f64).ceil() as usize).saturating_sub(1);
    values[rank.min(values.len() - 1)]
}

fn unique_metrics(samples: &[PerformanceSample]) -> Vec<String> {
    let mut metrics = samples
        .iter()
        .map(|sample| sample.metric.clone())
        .collect::<Vec<_>>();
    metrics.sort();
    metrics.dedup();
    metrics
}

fn metric_has_invalid_samples(metric: &str, samples: &[PerformanceSample]) -> bool {
    let matching = samples
        .iter()
        .filter(|sample| sample.metric == metric)
        .collect::<Vec<_>>();
    let Some(first) = matching.first() else {
        return false;
    };

    matching.iter().any(|sample| {
        !sample.has_finite_value()
            || sample.unit != first.unit
            || sample.lower_is_better != first.lower_is_better
    })
}

#[cfg(test)]
#[path = "performance_scorecard_tests.rs"]
mod tests;
