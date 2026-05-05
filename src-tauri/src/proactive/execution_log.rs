use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::proactive::scenarios::types::ExecutionLog;

const MAX_EXECUTION_LOGS: usize = 100;

/// 执行日志收集器 - 维护最近的执行日志环形缓冲区
#[derive(Clone)]
pub struct ExecutionLogCollector {
    logs: Arc<RwLock<VecDeque<ExecutionLog>>>,
    max_size: usize,
}

impl ExecutionLogCollector {
    pub fn new() -> Self {
        Self {
            logs: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_EXECUTION_LOGS))),
            max_size: MAX_EXECUTION_LOGS,
        }
    }

    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            logs: Arc::new(RwLock::new(VecDeque::with_capacity(max_size))),
            max_size,
        }
    }

    /// 添加一条执行日志
    pub async fn push(&self, log: ExecutionLog) {
        let mut logs = self.logs.write().await;
        if logs.len() >= self.max_size {
            logs.pop_front();
        }
        logs.push_back(log);
    }

    /// 获取最近 N 条日志
    pub async fn recent(&self, count: usize) -> Vec<ExecutionLog> {
        let logs = self.logs.read().await;
        logs.iter().rev().take(count).cloned().collect::<Vec<_>>().into_iter().rev().collect()
    }

    /// 获取自指定时间戳以来的日志
    pub async fn since(&self, timestamp: i64) -> Vec<ExecutionLog> {
        let logs = self.logs.read().await;
        logs.iter().filter(|l| l.timestamp >= timestamp).cloned().collect()
    }

    /// 获取失败的日志
    pub async fn failures(&self, count: usize) -> Vec<ExecutionLog> {
        let logs = self.logs.read().await;
        logs.iter()
            .rev()
            .filter(|l| !l.success)
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// 获取当前日志数量
    pub async fn count(&self) -> usize {
        self.logs.read().await.len()
    }

    /// 清空日志
    pub async fn clear(&self) {
        self.logs.write().await.clear();
    }

    /// 统计成功/失败数
    pub async fn stats(&self) -> (usize, usize) {
        let logs = self.logs.read().await;
        let success = logs.iter().filter(|l| l.success).count();
        let failure = logs.iter().filter(|l| !l.success).count();
        (success, failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_log(tool_name: &str, success: bool, timestamp: i64) -> ExecutionLog {
        ExecutionLog {
            session_id: "test-session".to_string(),
            iteration: 1,
            tool_name: tool_name.to_string(),
            tool_input: serde_json::json!({}),
            tool_output: serde_json::json!({"result": "ok"}),
            success,
            duration_ms: 100,
            timestamp,
            context_summary: "test context".to_string(),
        }
    }

    #[tokio::test]
    async fn test_push_and_count() {
        let collector = ExecutionLogCollector::new();
        assert_eq!(collector.count().await, 0);

        collector.push(make_log("read_file", true, 1000)).await;
        collector.push(make_log("write_file", true, 2000)).await;
        assert_eq!(collector.count().await, 2);
    }

    #[tokio::test]
    async fn test_ring_buffer_eviction() {
        let collector = ExecutionLogCollector::with_max_size(3);

        collector.push(make_log("tool_a", true, 1000)).await;
        collector.push(make_log("tool_b", true, 2000)).await;
        collector.push(make_log("tool_c", true, 3000)).await;
        collector.push(make_log("tool_d", true, 4000)).await;

        assert_eq!(collector.count().await, 3);
        let recent = collector.recent(10).await;
        assert_eq!(recent[0].tool_name, "tool_b");
        assert_eq!(recent[2].tool_name, "tool_d");
    }

    #[tokio::test]
    async fn test_recent_returns_ordered() {
        let collector = ExecutionLogCollector::new();
        collector.push(make_log("a", true, 1000)).await;
        collector.push(make_log("b", true, 2000)).await;
        collector.push(make_log("c", true, 3000)).await;

        let recent = collector.recent(2).await;
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].tool_name, "b");
        assert_eq!(recent[1].tool_name, "c");
    }

    #[tokio::test]
    async fn test_since_filters_by_timestamp() {
        let collector = ExecutionLogCollector::new();
        collector.push(make_log("old", true, 1000)).await;
        collector.push(make_log("mid", true, 2000)).await;
        collector.push(make_log("new", true, 3000)).await;

        let since = collector.since(2000).await;
        assert_eq!(since.len(), 2);
        assert_eq!(since[0].tool_name, "mid");
        assert_eq!(since[1].tool_name, "new");
    }

    #[tokio::test]
    async fn test_failures_and_stats() {
        let collector = ExecutionLogCollector::new();
        collector.push(make_log("ok1", true, 1000)).await;
        collector.push(make_log("fail1", false, 2000)).await;
        collector.push(make_log("ok2", true, 3000)).await;
        collector.push(make_log("fail2", false, 4000)).await;

        let failures = collector.failures(10).await;
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].tool_name, "fail1");
        assert_eq!(failures[1].tool_name, "fail2");

        let (success, failure) = collector.stats().await;
        assert_eq!(success, 2);
        assert_eq!(failure, 2);
    }

    #[tokio::test]
    async fn test_clear() {
        let collector = ExecutionLogCollector::new();
        collector.push(make_log("a", true, 1000)).await;
        collector.push(make_log("b", true, 2000)).await;
        assert_eq!(collector.count().await, 2);

        collector.clear().await;
        assert_eq!(collector.count().await, 0);
    }
}
