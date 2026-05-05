/// 指标采集服务
///
/// 提供计数器（Counter）和直方图（Histogram）两种核心指标类型，
/// 用于采集 uClaw 运行时的关键业务指标。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// 预设指标名称常量
// ---------------------------------------------------------------------------

/// 预定义的指标名称，确保全局统一使用
pub mod metric_names {
    // ---- 消息处理 ----
    /// 已处理消息总数
    pub const MESSAGES_PROCESSED: &str = "messages_processed_total";
    /// 收到的消息总数
    pub const MESSAGES_INCOMING: &str = "messages_incoming_total";
    /// 发出的消息总数
    pub const MESSAGES_OUTGOING: &str = "messages_outgoing_total";

    // ---- 记忆提取 ----
    /// 记忆触发次数
    pub const MEMORIZATION_TRIGGERS: &str = "memorization_trigger_count";
    /// 记忆提取耗时（直方图，毫秒）
    pub const MEMORIZATION_DURATION: &str = "memorization_duration_ms";
    /// 提取的记忆条目数
    pub const MEMORIZATION_ITEMS: &str = "memorization_items_extracted";

    // ---- 主动服务 ----
    /// 主动轮询 tick 次数
    pub const PROACTIVE_TICKS: &str = "proactive_tick_count";
    /// 主动执行动作次数
    pub const PROACTIVE_ACTIONS: &str = "proactive_action_count";
    /// 主动轮询无消息次数
    pub const PROACTIVE_NO_MESSAGE: &str = "proactive_no_message_count";

    // ---- 记忆召唤 ----
    /// 记忆召唤延迟（直方图，毫秒）
    pub const RECALL_LATENCY: &str = "recall_latency_ms";
    /// 记忆召唤返回结果数
    pub const RECALL_RESULTS: &str = "recall_results_count";

    // ---- memU API ----
    /// memU API 延迟（直方图，毫秒）
    pub const MEMU_API_LATENCY: &str = "memu_api_latency_ms";
    /// memU API 错误总数
    pub const MEMU_API_ERRORS: &str = "memu_api_errors_total";

    // ---- Agent ----
    /// Agent 响应延迟（直方图，毫秒）
    pub const AGENT_RESPONSE_LATENCY: &str = "agent_response_latency_ms";
    /// Agent 工具调用总数
    pub const AGENT_TOOL_CALLS: &str = "agent_tool_calls_total";
    /// Agent 输入 token 总数
    pub const AGENT_TOKENS_INPUT: &str = "agent_tokens_input_total";
    /// Agent 输出 token 总数
    pub const AGENT_TOKENS_OUTPUT: &str = "agent_tokens_output_total";
}

// ---------------------------------------------------------------------------
// MetricsService
// ---------------------------------------------------------------------------

/// 指标服务
///
/// 提供计数器和直方图两种核心指标类型。
/// - 计数器：使用原子操作，无锁高性能递增。
/// - 直方图：记录数值分布，支持百分位查询。
pub struct MetricsService {
    /// 计数器映射（读写锁保护 HashMap，值为原子整数，读多写少场景高效）
    counters: StdRwLock<HashMap<String, AtomicU64>>,
    /// 直方图数据（异步读写锁，记录每个指标的采样值列表）
    histograms: Arc<RwLock<HashMap<String, Vec<f64>>>>,
    /// 服务启动时间
    started_at: Instant,
}

impl MetricsService {
    /// 创建新的指标服务实例
    pub fn new() -> Self {
        Self {
            counters: StdRwLock::new(HashMap::new()),
            histograms: Arc::new(RwLock::new(HashMap::new())),
            started_at: Instant::now(),
        }
    }

    /// 递增计数器（+1）
    ///
    /// 如果计数器不存在则自动创建并初始化为 1。
    pub fn increment(&self, name: &str) {
        self.add(name, 1);
    }

    /// 增加计数器指定值
    ///
    /// 如果计数器不存在则自动创建并初始化为 `value`。
    pub fn add(&self, name: &str, value: u64) {
        // 快速路径：读锁检查已存在的计数器
        {
            let counters = self.counters.read().unwrap();
            if let Some(counter) = counters.get(name) {
                counter.fetch_add(value, Ordering::Relaxed);
                return;
            }
        }
        // 慢速路径：写锁插入新计数器
        let mut counters = self.counters.write().unwrap();
        counters
            .entry(name.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(value, Ordering::Relaxed);
    }

    /// 获取计数器当前值
    ///
    /// 如果计数器不存在返回 0。
    pub fn get_counter(&self, name: &str) -> u64 {
        let counters = self.counters.read().unwrap();
        counters
            .get(name)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// 记录直方图值（如延迟毫秒数）
    pub async fn record_histogram(&self, name: &str, value: f64) {
        let mut histograms = self.histograms.write().await;
        histograms
            .entry(name.to_string())
            .or_insert_with(Vec::new)
            .push(value);
    }

    /// 记录持续时间（便捷方法，将 Duration 转为毫秒后写入直方图）
    pub async fn record_duration(&self, name: &str, duration: Duration) {
        self.record_histogram(name, duration.as_millis() as f64)
            .await;
    }

    /// 创建自动计时器（RAII 模式）
    ///
    /// 返回的 `MetricsTimer` 在 drop 时自动记录持续时间到对应直方图。
    pub fn start_timer(self: &Arc<Self>, name: &str) -> MetricsTimer {
        MetricsTimer {
            name: name.to_string(),
            start: Instant::now(),
            metrics: Arc::clone(self),
        }
    }

    /// 获取单个直方图的统计摘要
    ///
    /// 返回 min / max / avg / p50 / p95 / p99 / count。
    /// 如果指标不存在或无数据则返回 `None`。
    pub async fn get_histogram_summary(&self, name: &str) -> Option<HistogramSummary> {
        let histograms = self.histograms.read().await;
        let values = histograms.get(name)?;
        if values.is_empty() {
            return None;
        }
        Some(compute_summary(name, values))
    }

    /// 获取全部指标摘要（计数器 + 直方图）
    pub async fn get_summary(&self) -> MetricsSummary {
        // 收集计数器
        let counter_map = {
            let counters = self.counters.read().unwrap();
            counters
                .iter()
                .map(|(k, v)| (k.clone(), v.load(Ordering::Relaxed)))
                .collect::<HashMap<_, _>>()
        };

        // 收集直方图摘要
        let histogram_map = {
            let histograms = self.histograms.read().await;
            histograms
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, v)| (k.clone(), compute_summary(k, v)))
                .collect::<HashMap<_, _>>()
        };

        MetricsSummary {
            uptime_secs: self.started_at.elapsed().as_secs(),
            counters: counter_map,
            histograms: histogram_map,
        }
    }

    /// 获取服务运行时长
    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// 重置所有指标（主要用于测试）
    pub async fn reset(&self) {
        {
            let mut counters = self.counters.write().unwrap();
            counters.clear();
        }
        {
            let mut histograms = self.histograms.write().await;
            histograms.clear();
        }
    }
}

impl Default for MetricsService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// MetricsTimer — RAII 自动计时器
// ---------------------------------------------------------------------------

/// 自动计时器
///
/// 在创建时记录起始时间，drop 时自动将耗时写入对应的直方图。
/// 由于 `Drop` 不支持 async，内部使用 `tokio::spawn` 完成异步写入。
pub struct MetricsTimer {
    /// 指标名称
    name: String,
    /// 计时起点
    start: Instant,
    /// 指标服务的共享引用
    metrics: Arc<MetricsService>,
}

impl MetricsTimer {
    /// 手动结束计时并返回持续时间（毫秒）
    ///
    /// 调用此方法后 `Drop` 不会重复记录（因为 self 已被消费）。
    pub async fn finish(self) -> f64 {
        let elapsed_ms = self.start.elapsed().as_millis() as f64;
        self.metrics
            .record_histogram(&self.name, elapsed_ms)
            .await;
        // 阻止 Drop 再次记录：通过 mem::forget 跳过析构
        let name = self.name.clone();
        std::mem::forget(self);
        tracing::trace!(metric = %name, elapsed_ms, "timer finished (explicit)");
        elapsed_ms
    }
}

impl Drop for MetricsTimer {
    fn drop(&mut self) {
        let elapsed_ms = self.start.elapsed().as_millis() as f64;
        let name = self.name.clone();
        let metrics = Arc::clone(&self.metrics);
        // Drop 中无法 await，使用 tokio::spawn 异步写入
        tokio::spawn(async move {
            metrics.record_histogram(&name, elapsed_ms).await;
            tracing::trace!(metric = %name, elapsed_ms, "timer finished (drop)");
        });
    }
}

// ---------------------------------------------------------------------------
// 数据结构
// ---------------------------------------------------------------------------

/// 直方图统计摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramSummary {
    /// 指标名称
    pub name: String,
    /// 采样数量
    pub count: usize,
    /// 最小值
    pub min: f64,
    /// 最大值
    pub max: f64,
    /// 平均值
    pub avg: f64,
    /// 中位数（第 50 百分位）
    pub p50: f64,
    /// 第 95 百分位
    pub p95: f64,
    /// 第 99 百分位
    pub p99: f64,
}

/// 全部指标摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    /// 服务运行时长（秒）
    pub uptime_secs: u64,
    /// 所有计数器的当前值
    pub counters: HashMap<String, u64>,
    /// 所有直方图的统计摘要
    pub histograms: HashMap<String, HistogramSummary>,
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 计算直方图的统计摘要（min, max, avg, p50, p95, p99）
fn compute_summary(name: &str, values: &[f64]) -> HistogramSummary {
    let count = values.len();
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let min = sorted[0];
    let max = sorted[count - 1];
    let avg = sorted.iter().sum::<f64>() / count as f64;

    HistogramSummary {
        name: name.to_string(),
        count,
        min,
        max,
        avg,
        p50: percentile(&sorted, 50.0),
        p95: percentile(&sorted, 95.0),
        p99: percentile(&sorted, 99.0),
    }
}

/// 计算百分位值（线性插值）
fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let idx = (pct / 100.0) * (sorted.len() - 1) as f64;
    let lower = idx.floor() as usize;
    let upper = idx.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let frac = idx - lower as f64;
        sorted[lower] * (1.0 - frac) + sorted[upper] * frac
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// 测试计数器递增
    #[tokio::test]
    async fn test_counter_increment() {
        let svc = MetricsService::new();
        assert_eq!(svc.get_counter("test"), 0);

        svc.increment("test");
        assert_eq!(svc.get_counter("test"), 1);

        svc.increment("test");
        svc.increment("test");
        assert_eq!(svc.get_counter("test"), 3);
    }

    /// 测试计数器 add
    #[tokio::test]
    async fn test_counter_add() {
        let svc = MetricsService::new();
        svc.add("requests", 10);
        svc.add("requests", 5);
        assert_eq!(svc.get_counter("requests"), 15);
    }

    /// 测试直方图记录与摘要
    #[tokio::test]
    async fn test_histogram_summary() {
        let svc = MetricsService::new();
        let values = vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0];
        for v in &values {
            svc.record_histogram("latency", *v).await;
        }

        let summary = svc.get_histogram_summary("latency").await.unwrap();
        assert_eq!(summary.count, 10);
        assert!((summary.min - 10.0).abs() < f64::EPSILON);
        assert!((summary.max - 100.0).abs() < f64::EPSILON);
        assert!((summary.avg - 55.0).abs() < f64::EPSILON);
        // p50 ≈ 55.0 (线性插值在 index 4.5 → 50*0.5 + 60*0.5 = 55)
        assert!((summary.p50 - 55.0).abs() < 1.0);
    }

    /// 测试直方图不存在时返回 None
    #[tokio::test]
    async fn test_histogram_not_found() {
        let svc = MetricsService::new();
        assert!(svc.get_histogram_summary("nonexistent").await.is_none());
    }

    /// 测试 record_duration 便捷方法
    #[tokio::test]
    async fn test_record_duration() {
        let svc = MetricsService::new();
        svc.record_duration("api_latency", Duration::from_millis(123))
            .await;
        let summary = svc.get_histogram_summary("api_latency").await.unwrap();
        assert_eq!(summary.count, 1);
        assert!((summary.min - 123.0).abs() < f64::EPSILON);
    }

    /// 测试完整摘要输出
    #[tokio::test]
    async fn test_get_summary() {
        let svc = MetricsService::new();
        svc.increment("msg_in");
        svc.increment("msg_in");
        svc.increment("msg_out");
        svc.record_histogram("latency", 42.0).await;

        let summary = svc.get_summary().await;
        assert_eq!(summary.counters.get("msg_in"), Some(&2));
        assert_eq!(summary.counters.get("msg_out"), Some(&1));
        assert!(summary.histograms.contains_key("latency"));
    }

    /// 测试重置
    #[tokio::test]
    async fn test_reset() {
        let svc = MetricsService::new();
        svc.increment("counter");
        svc.record_histogram("hist", 1.0).await;

        svc.reset().await;
        assert_eq!(svc.get_counter("counter"), 0);
        assert!(svc.get_histogram_summary("hist").await.is_none());
    }

    /// 测试 start_timer（显式 finish）
    #[tokio::test]
    async fn test_timer_explicit_finish() {
        let svc = Arc::new(MetricsService::new());
        let timer = svc.start_timer("op_duration");
        // 模拟一些工作
        tokio::time::sleep(Duration::from_millis(10)).await;
        let elapsed = timer.finish().await;
        assert!(elapsed >= 10.0);

        let summary = svc.get_histogram_summary("op_duration").await.unwrap();
        assert_eq!(summary.count, 1);
    }

    /// 测试百分位计算
    #[test]
    fn test_percentile_single_value() {
        let sorted = vec![42.0];
        assert!((percentile(&sorted, 50.0) - 42.0).abs() < f64::EPSILON);
        assert!((percentile(&sorted, 99.0) - 42.0).abs() < f64::EPSILON);
    }

    /// 测试百分位线性插值
    #[test]
    fn test_percentile_interpolation() {
        let sorted = vec![0.0, 100.0];
        assert!((percentile(&sorted, 50.0) - 50.0).abs() < f64::EPSILON);
        assert!((percentile(&sorted, 25.0) - 25.0).abs() < f64::EPSILON);
    }
}
