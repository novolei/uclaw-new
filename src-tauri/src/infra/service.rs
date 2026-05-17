//! InfraService — 中央消息总线
//!
//! 基于 `tokio::sync::broadcast` 实现的发布/订阅消息总线，
//! 作为所有后台服务间通信的中枢。对标 memUBot 的 InfraService (EventEmitter 模式)。
//!
//! ## 设计要点
//! - 单一 broadcast channel，订阅者自行按 `event_type` 过滤
//! - 循环缓冲区保留最近 100 条事件，支持历史查询
//! - `AtomicU64` 生成全局递增事件 ID，无锁写入
//! - 线程安全：`Arc<RwLock<VecDeque>>` 保护缓冲区

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use super::types::*;

/// 循环缓冲区容量（保留最近 N 条事件）
const BUFFER_SIZE: usize = 100;

/// broadcast channel 容量（允许的最大未消费消息数）
const CHANNEL_CAPACITY: usize = 256;

/// 中央消息总线
///
/// 所有后台服务（Agent、Memorization、Proactive 等）通过该服务收发事件。
/// 调用 `subscribe()` 获取接收端，调用 `publish()` 或快捷方法发布事件。
pub struct InfraService {
    /// 广播发送端（所有事件类型共用一个 channel）
    sender: broadcast::Sender<InfraEvent>,
    /// 循环缓冲区，保留最近事件供历史查询
    buffer: Arc<RwLock<VecDeque<InfraEvent>>>,
    /// 事件 ID 原子计数器
    next_id: AtomicU64,
}

impl InfraService {
    /// 创建新的 InfraService 实例
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            sender,
            buffer: Arc::new(RwLock::new(VecDeque::with_capacity(BUFFER_SIZE))),
            next_id: AtomicU64::new(1),
        }
    }

    /// 发布事件到所有订阅者
    ///
    /// 1. 分配自增事件 ID
    /// 2. 写入循环缓冲区（超出容量时淘汰最旧事件）
    /// 3. 通过 broadcast channel 广播给所有订阅者
    pub async fn publish(&self, mut event: InfraEvent) {
        // 分配唯一事件 ID
        event.id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // 写入循环缓冲区
        {
            let mut buf = self.buffer.write().await;
            if buf.len() >= BUFFER_SIZE {
                buf.pop_front(); // 淘汰最旧事件
            }
            buf.push_back(event.clone());
        }

        // 广播给所有订阅者（即使没有订阅者也不会报错）
        let _ = self.sender.send(event);
    }

    /// 订阅消息总线
    ///
    /// 返回 `broadcast::Receiver`，调用者可自行按 `event_type` 过滤感兴趣的事件。
    pub fn subscribe(&self) -> broadcast::Receiver<InfraEvent> {
        self.sender.subscribe()
    }

    /// 查询最近的事件
    ///
    /// - `event_type`: 可选过滤条件，`None` 表示返回所有类型
    /// - `limit`: 最多返回的事件数量
    ///
    /// 返回结果按时间顺序排列（最旧在前）。
    pub async fn get_recent(
        &self,
        event_type: Option<InfraEventType>,
        limit: usize,
    ) -> Vec<InfraEvent> {
        let buf = self.buffer.read().await;
        buf.iter()
            .filter(|e| match event_type {
                Some(t) => e.event_type == t,
                None => true,
            })
            .rev()          // 从最新开始取
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()          // 恢复时间正序
            .collect()
    }

    // ─── 快捷发布方法 ─────────────────────────────────────────────────

    /// 快捷方法：发布「用户消息到达」事件
    pub async fn publish_incoming(
        &self,
        platform: &str,
        content: &str,
        metadata: serde_json::Value,
    ) {
        let event = InfraEvent {
            id: 0, // 由 publish 分配
            event_type: InfraEventType::MessageIncoming,
            platform: platform.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: ConversationMessage {
                role: "user".to_string(),
                content: content.to_string(),
            },
            metadata,
            trace_id: None,
        };
        self.publish(event).await;
    }

    /// 快捷方法：发布「Bot 响应发出」事件
    pub async fn publish_outgoing(
        &self,
        platform: &str,
        content: &str,
        metadata: serde_json::Value,
    ) {
        let event = InfraEvent {
            id: 0,
            event_type: InfraEventType::MessageOutgoing,
            platform: platform.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: ConversationMessage {
                role: "assistant".to_string(),
                content: content.to_string(),
            },
            metadata,
            trace_id: None,
        };
        self.publish(event).await;
    }

    /// 快捷方法：发布「消息处理完成」事件
    pub async fn publish_processed(
        &self,
        platform: &str,
        metadata: serde_json::Value,
    ) {
        let event = InfraEvent {
            id: 0,
            event_type: InfraEventType::MessageProcessed,
            platform: platform.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: ConversationMessage {
                role: "system".to_string(),
                content: String::new(),
            },
            metadata,
            trace_id: None,
        };
        self.publish(event).await;
    }

    /// 快捷方法：发布「工具执行」事件
    pub async fn publish_tool_executed(
        &self,
        platform: &str,
        tool_name: &str,
        tool_output: &str,
        metadata: serde_json::Value,
    ) {
        let event = InfraEvent {
            id: 0,
            event_type: InfraEventType::ToolExecuted,
            platform: platform.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: ConversationMessage {
                role: "tool".to_string(),
                content: tool_output.to_string(),
            },
            metadata: {
                let mut m = metadata;
                m["tool_name"] = serde_json::Value::String(tool_name.to_string());
                m
            },
            trace_id: None,
        };
        self.publish(event).await;
    }

    /// 快捷方法：发布「循环完成」事件
    pub async fn publish_loop_completed(
        &self,
        platform: &str,
        summary: &str,
        metadata: serde_json::Value,
    ) {
        let event = InfraEvent {
            id: 0,
            event_type: InfraEventType::LoopCompleted,
            platform: platform.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: ConversationMessage {
                role: "system".to_string(),
                content: summary.to_string(),
            },
            metadata,
            trace_id: None,
        };
        self.publish(event).await;
    }

    /// 快捷方法：发布「循环失败」事件
    pub async fn publish_loop_failed(
        &self,
        platform: &str,
        error: &str,
        metadata: serde_json::Value,
    ) {
        let event = InfraEvent {
            id: 0,
            event_type: InfraEventType::LoopFailed,
            platform: platform.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: ConversationMessage {
                role: "system".to_string(),
                content: error.to_string(),
            },
            metadata,
            trace_id: None,
        };
        self.publish(event).await;
    }

    /// 快捷方法：发布「多模态摄入」事件
    pub async fn publish_multimodal_ingested(
        &self,
        platform: &str,
        source_type: &str,
        metadata: serde_json::Value,
    ) {
        let event = InfraEvent {
            id: 0,
            event_type: InfraEventType::MultimodalIngested,
            platform: platform.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: ConversationMessage {
                role: "system".to_string(),
                content: source_type.to_string(),
            },
            metadata,
            trace_id: None,
        };
        self.publish(event).await;
    }

    /// 快捷方法：发布「记忆提取完成」事件
    pub async fn publish_memory_extracted(
        &self,
        platform: &str,
        summary: &str,
        metadata: serde_json::Value,
    ) {
        let event = InfraEvent {
            id: 0,
            event_type: InfraEventType::MemoryExtracted,
            platform: platform.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: ConversationMessage {
                role: "system".to_string(),
                content: summary.to_string(),
            },
            metadata,
            trace_id: None,
        };
        self.publish(event).await;
    }

    /// 快捷方法：发布「技能学习」事件
    pub async fn publish_skill_learned(
        &self,
        platform: &str,
        skill_name: &str,
        metadata: serde_json::Value,
    ) {
        let event = InfraEvent {
            id: 0,
            event_type: InfraEventType::SkillLearned,
            platform: platform.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: ConversationMessage {
                role: "system".to_string(),
                content: skill_name.to_string(),
            },
            metadata,
            trace_id: None,
        };
        self.publish(event).await;
    }

    /// 快捷方法：发布「Capsule 生成」事件
    pub async fn publish_capsule_created(
        &self,
        platform: &str,
        gene_id: &str,
        metadata: serde_json::Value,
    ) {
        let event = InfraEvent {
            id: 0,
            event_type: InfraEventType::CapsuleCreated,
            platform: platform.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: ConversationMessage {
                role: "system".to_string(),
                content: gene_id.to_string(),
            },
            metadata,
            trace_id: None,
        };
        self.publish(event).await;
    }
}

// ─── 单元测试 ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// 辅助方法：构造一个测试用的 InfraEvent
    fn make_event(event_type: InfraEventType, content: &str) -> InfraEvent {
        InfraEvent {
            id: 0,
            event_type,
            platform: "test".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: ConversationMessage {
                role: "user".to_string(),
                content: content.to_string(),
            },
            metadata: serde_json::json!({}),
            trace_id: None,
        }
    }

    /// 测试：publish 后事件 ID 自动递增
    #[tokio::test]
    async fn test_publish_assigns_incremental_ids() {
        let svc = InfraService::new();

        let e1 = make_event(InfraEventType::MessageIncoming, "hello");
        let e2 = make_event(InfraEventType::MessageIncoming, "world");

        svc.publish(e1).await;
        svc.publish(e2).await;

        let recent = svc.get_recent(None, 10).await;
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].id, 1);
        assert_eq!(recent[1].id, 2);
    }

    /// 测试：subscribe 能接收到 publish 的事件
    #[tokio::test]
    async fn test_subscribe_receives_events() {
        let svc = InfraService::new();
        let mut rx = svc.subscribe();

        let event = make_event(InfraEventType::MessageIncoming, "test msg");
        svc.publish(event).await;

        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, 1);
        assert_eq!(received.message.content, "test msg");
        assert_eq!(received.event_type, InfraEventType::MessageIncoming);
    }

    /// 测试：get_recent 按类型过滤
    #[tokio::test]
    async fn test_get_recent_filters_by_type() {
        let svc = InfraService::new();

        svc.publish(make_event(InfraEventType::MessageIncoming, "in1")).await;
        svc.publish(make_event(InfraEventType::MessageOutgoing, "out1")).await;
        svc.publish(make_event(InfraEventType::MessageIncoming, "in2")).await;
        svc.publish(make_event(InfraEventType::MessageProcessed, "done")).await;

        // 只取 Incoming
        let incoming = svc.get_recent(Some(InfraEventType::MessageIncoming), 10).await;
        assert_eq!(incoming.len(), 2);
        assert_eq!(incoming[0].message.content, "in1");
        assert_eq!(incoming[1].message.content, "in2");

        // 不过滤，取全部
        let all = svc.get_recent(None, 10).await;
        assert_eq!(all.len(), 4);
    }

    /// 测试：get_recent 的 limit 参数
    #[tokio::test]
    async fn test_get_recent_respects_limit() {
        let svc = InfraService::new();

        for i in 0..5 {
            svc.publish(make_event(InfraEventType::MessageIncoming, &format!("msg{}", i))).await;
        }

        let recent = svc.get_recent(None, 3).await;
        assert_eq!(recent.len(), 3);
        // 应该返回最新的 3 条（id 3, 4, 5）
        assert_eq!(recent[0].id, 3);
        assert_eq!(recent[2].id, 5);
    }

    /// 测试：循环缓冲区溢出淘汰最旧事件
    #[tokio::test]
    async fn test_buffer_overflow_evicts_oldest() {
        let svc = InfraService::new();

        // 发布 BUFFER_SIZE + 10 条事件
        for i in 0..(BUFFER_SIZE + 10) {
            svc.publish(make_event(
                InfraEventType::MessageIncoming,
                &format!("msg{}", i),
            )).await;
        }

        let all = svc.get_recent(None, 200).await;
        assert_eq!(all.len(), BUFFER_SIZE);
        // 最旧的应该是 id=11（前 10 条被淘汰）
        assert_eq!(all[0].id, 11);
    }

    /// 测试：快捷方法 publish_incoming / publish_outgoing / publish_processed
    #[tokio::test]
    async fn test_convenience_publish_methods() {
        let svc = InfraService::new();
        let mut rx = svc.subscribe();

        svc.publish_incoming("local", "用户消息", serde_json::json!({"cid": "c1"})).await;
        svc.publish_outgoing("local", "Bot回复", serde_json::json!({"cid": "c1"})).await;
        svc.publish_processed("local", serde_json::json!({"cid": "c1"})).await;

        let e1 = rx.recv().await.unwrap();
        assert_eq!(e1.event_type, InfraEventType::MessageIncoming);
        assert_eq!(e1.message.role, "user");
        assert_eq!(e1.message.content, "用户消息");
        assert_eq!(e1.platform, "local");

        let e2 = rx.recv().await.unwrap();
        assert_eq!(e2.event_type, InfraEventType::MessageOutgoing);
        assert_eq!(e2.message.role, "assistant");

        let e3 = rx.recv().await.unwrap();
        assert_eq!(e3.event_type, InfraEventType::MessageProcessed);
        assert_eq!(e3.message.role, "system");
    }

    /// 测试：多个订阅者都能收到同一事件
    #[tokio::test]
    async fn test_multiple_subscribers() {
        let svc = InfraService::new();
        let mut rx1 = svc.subscribe();
        let mut rx2 = svc.subscribe();

        svc.publish(make_event(InfraEventType::MessageIncoming, "broadcast")).await;

        let r1 = rx1.recv().await.unwrap();
        let r2 = rx2.recv().await.unwrap();
        assert_eq!(r1.id, r2.id);
        assert_eq!(r1.message.content, "broadcast");
        assert_eq!(r2.message.content, "broadcast");
    }
}
