//! 工具流式输出领域事件 — Pi convergence Sprint 2。
//!
//! `ToolStreamSink` 让工具(当前仅 BashTool)在执行过程中把输出分块推给
//! dispatcher。工具只产生领域事件,不接触 Tauri —— 由 dispatcher 翻译成
//! `chat:stream-tool-activity` 事件并做合并节流。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// 输出来源管道。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolStream {
    Stdout,
    Stderr,
}

/// 一块工具输出。`seq` 跨两管道全局单调,保证前端可按顺序拼接。
#[derive(Debug, Clone)]
pub struct ToolStreamEvent {
    pub seq: u64,
    pub stream: ToolStream,
    pub bytes: Vec<u8>,
}

/// 流式输出 sink。`noop()` 用于非流式默认路径(零开销),
/// `channel()` 用于 dispatcher 接收端。
#[derive(Clone)]
pub struct ToolStreamSink {
    tx: Option<mpsc::Sender<ToolStreamEvent>>,
    seq: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
}

impl ToolStreamSink {
    /// 不投递任何事件的 sink(默认 `execute_streaming` 用)。
    pub fn noop() -> Self {
        Self {
            tx: None,
            seq: Arc::new(AtomicU64::new(0)),
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 建一个有界 channel,返回 sink + 接收端。
    pub fn channel(capacity: usize) -> (Self, mpsc::Receiver<ToolStreamEvent>) {
        let (tx, rx) = mpsc::channel(capacity);
        let sink = Self {
            tx: Some(tx),
            seq: Arc::new(AtomicU64::new(0)),
            dropped: Arc::new(AtomicU64::new(0)),
        };
        (sink, rx)
    }

    /// 非阻塞投递一块输出。channel 满时丢弃并计数(尽力而为;
    /// 最终结果 + temp 文件才是权威)。
    pub fn send(&self, stream: ToolStream, bytes: &[u8]) {
        let Some(tx) = &self.tx else { return };
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let event = ToolStreamEvent {
            seq,
            stream,
            bytes: bytes.to_vec(),
        };
        if tx.try_send(event).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// try_send 失败丢弃的块数。
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sink_assigns_monotonic_seq_and_delivers() {
        let (sink, mut rx) = ToolStreamSink::channel(16);
        sink.send(ToolStream::Stdout, b"a");
        sink.send(ToolStream::Stderr, b"b");
        let e0 = rx.recv().await.unwrap();
        let e1 = rx.recv().await.unwrap();
        assert_eq!(e0.seq, 0);
        assert_eq!(e1.seq, 1);
        assert!(matches!(e0.stream, ToolStream::Stdout));
        assert!(matches!(e1.stream, ToolStream::Stderr));
        assert_eq!(e0.bytes, b"a");
    }

    #[test]
    fn noop_sink_drops_silently_without_panicking() {
        let sink = ToolStreamSink::noop();
        sink.send(ToolStream::Stdout, b"ignored");
        assert_eq!(sink.dropped(), 0);
    }

    #[tokio::test]
    async fn full_channel_counts_dropped() {
        let (sink, _rx) = ToolStreamSink::channel(1);
        sink.send(ToolStream::Stdout, b"1");
        sink.send(ToolStream::Stdout, b"2");
        assert_eq!(sink.dropped(), 1);
    }
}
