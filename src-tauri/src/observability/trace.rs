/// 追踪服务
///
/// 提供简单的本地追踪能力，支持 trace / span 层级结构。
/// 不依赖 OpenTelemetry，所有数据保存在内存中。

use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// 数据结构
// ---------------------------------------------------------------------------

/// Span 状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpanStatus {
    /// 正常完成
    Ok,
    /// 发生错误
    Error(String),
}

/// 追踪 Span
///
/// 代表一个有始有终的操作单元，支持嵌套（通过 parent_span_id）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    /// 所属 trace 的 ID
    pub trace_id: String,
    /// 当前 span 的唯一 ID
    pub span_id: String,
    /// 父 span ID（顶层 span 为 None）
    pub parent_span_id: Option<String>,
    /// 操作名称
    pub operation: String,
    /// 开始时间（ISO 8601）
    pub start_time: String,
    /// 结束时间（ISO 8601，未结束时为 None）
    pub end_time: Option<String>,
    /// 持续时间（毫秒，未结束时为 None）
    pub duration_ms: Option<u64>,
    /// Span 状态
    pub status: SpanStatus,
    /// 附加属性
    pub attributes: serde_json::Value,
}

// ---------------------------------------------------------------------------
// TraceService
// ---------------------------------------------------------------------------

/// 追踪服务
///
/// 简单的本地追踪，将 span 保存在内存中。
/// 支持创建 trace、在 trace 下创建嵌套 span、结束 span 并收集。
pub struct TraceService {
    /// 是否启用追踪
    enabled: bool,
    /// 活跃的 span（span_id -> TraceSpan）
    active_spans: RwLock<HashMap<String, TraceSpan>>,
    /// 已完成的 span（span_id -> TraceSpan）
    completed_spans: RwLock<HashMap<String, TraceSpan>>,
}

impl TraceService {
    /// 创建追踪服务
    ///
    /// * `enabled` - 是否启用追踪。禁用时所有操作为空操作。
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            active_spans: RwLock::new(HashMap::new()),
            completed_spans: RwLock::new(HashMap::new()),
        }
    }

    /// 开始一个新的 trace
    ///
    /// 创建一个根 span 并返回 trace_id（同时也是根 span 的 span_id）。
    pub async fn start_trace(&self, operation: &str) -> String {
        let trace_id = Uuid::new_v4().to_string();
        if !self.enabled {
            return trace_id;
        }

        let span = TraceSpan {
            trace_id: trace_id.clone(),
            span_id: trace_id.clone(),
            parent_span_id: None,
            operation: operation.to_string(),
            start_time: Utc::now().to_rfc3339(),
            end_time: None,
            duration_ms: None,
            status: SpanStatus::Ok,
            attributes: serde_json::Value::Object(serde_json::Map::new()),
        };

        tracing::debug!(trace_id = %trace_id, operation, "trace started");

        let mut active = self.active_spans.write().await;
        active.insert(trace_id.clone(), span);
        trace_id
    }

    /// 在指定 trace 下创建子 span
    ///
    /// * `trace_id` - 所属 trace 的 ID
    /// * `operation` - 操作名称
    /// * `parent_span_id` - 父 span ID（None 表示直接挂在 trace 根下）
    ///
    /// 返回新创建的 span_id。
    pub async fn start_span(
        &self,
        trace_id: &str,
        operation: &str,
        parent_span_id: Option<&str>,
    ) -> String {
        let span_id = Uuid::new_v4().to_string();
        if !self.enabled {
            return span_id;
        }

        let span = TraceSpan {
            trace_id: trace_id.to_string(),
            span_id: span_id.clone(),
            parent_span_id: parent_span_id.map(|s| s.to_string()),
            operation: operation.to_string(),
            start_time: Utc::now().to_rfc3339(),
            end_time: None,
            duration_ms: None,
            status: SpanStatus::Ok,
            attributes: serde_json::Value::Object(serde_json::Map::new()),
        };

        tracing::debug!(
            trace_id = %trace_id,
            span_id = %span_id,
            operation,
            "span started"
        );

        let mut active = self.active_spans.write().await;
        active.insert(span_id.clone(), span);
        span_id
    }

    /// 为活跃 span 添加属性
    pub async fn set_attribute(&self, span_id: &str, key: &str, value: serde_json::Value) {
        if !self.enabled {
            return;
        }
        let mut active = self.active_spans.write().await;
        if let Some(span) = active.get_mut(span_id) {
            if let serde_json::Value::Object(ref mut map) = span.attributes {
                map.insert(key.to_string(), value);
            }
        }
    }

    /// 结束指定 span
    ///
    /// 将 span 从活跃列表移到已完成列表，记录结束时间和状态。
    pub async fn end_span(&self, span_id: &str, status: SpanStatus) {
        if !self.enabled {
            return;
        }

        let mut active = self.active_spans.write().await;
        if let Some(mut span) = active.remove(span_id) {
            let end_time = Utc::now();
            span.end_time = Some(end_time.to_rfc3339());

            // 计算持续时间
            if let Ok(start) = chrono::DateTime::parse_from_rfc3339(&span.start_time) {
                let duration = end_time.signed_duration_since(start);
                span.duration_ms = Some(duration.num_milliseconds().max(0) as u64);
            }

            span.status = status;

            tracing::debug!(
                trace_id = %span.trace_id,
                span_id = %span.span_id,
                operation = %span.operation,
                duration_ms = ?span.duration_ms,
                "span ended"
            );

            drop(active); // 释放活跃锁再获取已完成锁
            let mut completed = self.completed_spans.write().await;
            completed.insert(span.span_id.clone(), span);
        }
    }

    /// 结束 trace 并收集所有关联的 span（包括已完成和仍活跃的）
    ///
    /// 所有属于该 trace 的 span 都会被移除并返回。
    pub async fn end_trace(&self, trace_id: &str) -> Vec<TraceSpan> {
        if !self.enabled {
            return Vec::new();
        }

        let mut result = Vec::new();

        // 收集已完成的 span
        {
            let mut completed = self.completed_spans.write().await;
            let keys: Vec<String> = completed
                .iter()
                .filter(|(_, s)| s.trace_id == trace_id)
                .map(|(k, _)| k.clone())
                .collect();
            for key in keys {
                if let Some(span) = completed.remove(&key) {
                    result.push(span);
                }
            }
        }

        // 收集仍活跃的 span（强制结束）
        {
            let mut active = self.active_spans.write().await;
            let keys: Vec<String> = active
                .iter()
                .filter(|(_, s)| s.trace_id == trace_id)
                .map(|(k, _)| k.clone())
                .collect();
            for key in keys {
                if let Some(mut span) = active.remove(&key) {
                    let end_time = Utc::now();
                    span.end_time = Some(end_time.to_rfc3339());
                    if let Ok(start) = chrono::DateTime::parse_from_rfc3339(&span.start_time) {
                        let duration = end_time.signed_duration_since(start);
                        span.duration_ms = Some(duration.num_milliseconds().max(0) as u64);
                    }
                    result.push(span);
                }
            }
        }

        // 按开始时间排序
        result.sort_by(|a, b| a.start_time.cmp(&b.start_time));

        tracing::debug!(
            trace_id = %trace_id,
            span_count = result.len(),
            "trace ended"
        );

        result
    }

    /// 获取当前活跃 span 数量
    pub async fn active_span_count(&self) -> usize {
        self.active_spans.read().await.len()
    }

    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试基本的 trace 创建和结束
    #[tokio::test]
    async fn test_trace_lifecycle() {
        let svc = TraceService::new(true);

        // 开始 trace
        let trace_id = svc.start_trace("test_operation").await;
        assert!(!trace_id.is_empty());
        assert_eq!(svc.active_span_count().await, 1);

        // 结束 trace
        let spans = svc.end_trace(&trace_id).await;
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].operation, "test_operation");
        assert!(spans[0].end_time.is_some());
        assert_eq!(svc.active_span_count().await, 0);
    }

    /// 测试嵌套 span
    #[tokio::test]
    async fn test_nested_spans() {
        let svc = TraceService::new(true);

        let trace_id = svc.start_trace("root_op").await;
        let child1 = svc.start_span(&trace_id, "child_1", Some(&trace_id)).await;
        let child2 = svc.start_span(&trace_id, "child_2", Some(&trace_id)).await;

        assert_eq!(svc.active_span_count().await, 3); // root + 2 children

        // 结束子 span
        svc.end_span(&child1, SpanStatus::Ok).await;
        svc.end_span(&child2, SpanStatus::Error("timeout".into())).await;

        assert_eq!(svc.active_span_count().await, 1); // 只剩 root

        // 结束整个 trace
        let spans = svc.end_trace(&trace_id).await;
        assert_eq!(spans.len(), 3);
    }

    /// 测试 span 属性设置
    #[tokio::test]
    async fn test_span_attributes() {
        let svc = TraceService::new(true);

        let trace_id = svc.start_trace("op").await;
        svc.set_attribute(&trace_id, "user_id", serde_json::json!("u-123")).await;

        svc.end_span(&trace_id, SpanStatus::Ok).await;

        // 从已完成中取出验证
        let spans = svc.end_trace(&trace_id).await;
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].attributes["user_id"], "u-123");
    }

    /// 测试禁用时不记录
    #[tokio::test]
    async fn test_disabled_service() {
        let svc = TraceService::new(false);

        let trace_id = svc.start_trace("noop").await;
        assert!(!trace_id.is_empty()); // 仍然返回 ID
        assert_eq!(svc.active_span_count().await, 0); // 但不记录

        let spans = svc.end_trace(&trace_id).await;
        assert!(spans.is_empty());
    }

    /// 测试 end_span 状态记录
    #[tokio::test]
    async fn test_span_status() {
        let svc = TraceService::new(true);

        let trace_id = svc.start_trace("op").await;
        svc.end_span(&trace_id, SpanStatus::Error("fail".into())).await;

        let spans = svc.end_trace(&trace_id).await;
        assert_eq!(spans.len(), 1);
        match &spans[0].status {
            SpanStatus::Error(msg) => assert_eq!(msg, "fail"),
            _ => panic!("expected Error status"),
        }
    }
}
