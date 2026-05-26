# Bash 流式输出管线 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Bash 工具的 stdout/stderr 实时交错流式输出到聊天页(类终端体验),完整日志按需从落盘文件加载。

**Architecture:** 方案 A——`Tool` trait 加可选 `execute_streaming`(默认委托 `execute`,blast radius 1);`shell.rs` 用 `tokio::select!` 分块读两管道,每块推入 `mpsc` channel + 32KB rolling tail + 增量 temp 落盘;dispatcher 串行路径起一个合并节流任务,emit `chat:stream-tool-activity` 的新 `tool_output_chunk` 事件;前端 listener append 到 `ToolActivity.liveOutput`(256KB 有界环),`BashStreamView` 实时着色渲染 + 自动滚动 + 「加载完整日志」。

**Tech Stack:** Rust + Tokio(`tokio::select!`、`tokio::sync::mpsc`、`AtomicU64`)、Tauri events、React + Jotai。

**Spec:** `docs/superpowers/specs/2026-05-26-bash-streaming-output-design.md`
**Branch:** `codex/pi-sprint2-bash-streaming`(base = `main` @ `be4f2b08`)

---

## 验证命令速查

- 后端编译:`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空 = 通过)
- 后端单测:`cd src-tauri && cargo test --lib <filter> 2>&1 | tail -15`
- 前端类型:`cd ui && npx tsc --noEmit 2>&1 | head -10`
- 前端单测:`cd ui && npm test -- --run <file> 2>&1 | tail -15`

> 注意:`cargo test -p uclaw_core` 在本仓库**不工作**(包名不匹配),一律在 `src-tauri/` 下用 `cargo test`。

---

## File Structure

| 文件 | 责任 |
|---|---|
| `src-tauri/src/agent/tools/stream.rs` | **新建** — `ToolStream` / `ToolStreamEvent` / `ToolStreamSink`(领域事件 + channel,不碰 Tauri) |
| `src-tauri/src/agent/tools/mod.rs` | 注册 `pub mod stream;` + re-export |
| `src-tauri/src/agent/tools/tool.rs` | `Tool` trait 加 `supports_streaming` + `execute_streaming` 默认方法;新增 `execute_streaming_with_context` |
| `src-tauri/src/agent/tools/builtin/shell.rs` | `OverflowSink` 增量落盘;`execute_streaming` select 读循环;`execute` 委托 |
| `src-tauri/src/agent/dispatcher.rs` | 串行路径合并节流 drain 任务 + emit `tool_output_chunk` |
| `src-tauri/src/tauri_commands.rs` | `read_bash_log` 命令 |
| `src-tauri/src/main.rs` | `invoke_handler!` 注册 `read_bash_log` |
| `ui/src/atoms/agent-atoms.ts` | `LiveOutput` 类型 + `ToolActivity.liveOutput` + 有界环 append 纯函数 |
| `ui/src/hooks/useGlobalAgentListeners.ts` | listener 加 `tool_output_chunk` 分支 |
| `ui/src/components/agent/tool-renderers/BashStreamView.tsx` | **新建** — 实时着色渲染 + 自动滚动 + 加载完整日志 |
| `ui/src/components/agent/ToolActivityItem.tsx` | bash 运行中 + 有 liveOutput 时渲染 `BashStreamView` |

任务顺序:后端自底向上(类型 → trait → 落盘 → 读循环 → dispatcher → 命令),再前端(状态 → listener → 渲染)。每个 Task 独立编译、独立提交。

---

## Task 1: ToolStream / ToolStreamEvent / ToolStreamSink

**Files:**
- Create: `src-tauri/src/agent/tools/stream.rs`
- Modify: `src-tauri/src/agent/tools/mod.rs`

- [ ] **Step 1: 写失败测试** — 在新建文件 `src-tauri/src/agent/tools/stream.rs` 末尾:

```rust
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
        sink.send(ToolStream::Stdout, b"ignored"); // no receiver, must not panic
        assert_eq!(sink.dropped(), 0); // noop never counts drops (no channel)
    }

    #[tokio::test]
    async fn full_channel_counts_dropped() {
        let (sink, _rx) = ToolStreamSink::channel(1);
        sink.send(ToolStream::Stdout, b"1"); // fills capacity 1
        sink.send(ToolStream::Stdout, b"2"); // try_send fails -> dropped
        assert_eq!(sink.dropped(), 1);
    }
}
```

- [ ] **Step 2: 运行确认红**

Run: `cd src-tauri && cargo test --lib tools::stream 2>&1 | grep -E "^error|cannot find" | head`
Expected: `cannot find ... ToolStreamSink`(模块未注册 / 类型未定义)

- [ ] **Step 3: 实现** — 在 `stream.rs` 测试模块之前写:

```rust
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
        Self { tx: None, seq: Arc::new(AtomicU64::new(0)), dropped: Arc::new(AtomicU64::new(0)) }
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
        let event = ToolStreamEvent { seq, stream, bytes: bytes.to_vec() };
        if tx.try_send(event).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// try_send 失败丢弃的块数。
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}
```

- [ ] **Step 4: 注册模块** — 在 `src-tauri/src/agent/tools/mod.rs` 现有 `pub mod` 列表里加一行(放在 `pub mod tool;` 附近):

```rust
pub mod stream;
```

- [ ] **Step 5: 运行确认绿**

Run: `cd src-tauri && cargo test --lib tools::stream 2>&1 | tail -8`
Expected: `test result: ok. 3 passed`

- [ ] **Step 6: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/agent/tools/stream.rs src-tauri/src/agent/tools/mod.rs
git commit -m "$(cat <<'EOF'
feat(tools): ToolStreamSink domain events for streaming output

New agent/tools/stream.rs: ToolStream / ToolStreamEvent / ToolStreamSink.
Tools push output chunks into an mpsc channel with a monotonic seq; the
dispatcher (not the tool) translates them to Tauri events. noop() sink
is a zero-cost default for non-streaming tools.

Verification: cargo test --lib tools::stream → ok, 3 passed
EOF
)"
```

---

## Task 2: Tool trait 流式方法(附加,非破坏)

**Files:**
- Modify: `src-tauri/src/agent/tools/tool.rs:206-249`

- [ ] **Step 1: 写失败测试** — 在 `tool.rs` 末尾的 `#[cfg(test)]` 块中(若无则新建)添加。这个测试验证默认 `execute_streaming` 委托到 `execute`,且默认 `supports_streaming` 为 false:

```rust
#[cfg(test)]
mod stream_default_tests {
    use super::*;
    use crate::agent::tools::stream::ToolStreamSink;

    struct DummyTool;
    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str { "dummy" }
        fn description(&self) -> &str { "" }
        fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({}) }
        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::new(serde_json::json!({"ok": true}), 0))
        }
    }

    #[tokio::test]
    async fn default_execute_streaming_delegates_to_execute() {
        let tool = DummyTool;
        assert!(!tool.supports_streaming());
        let out = tool.execute_streaming(serde_json::json!({}), ToolStreamSink::noop()).await.unwrap();
        assert_eq!(out.result["ok"], serde_json::json!(true));
    }
}
```

- [ ] **Step 2: 运行确认红**

Run: `cd src-tauri && cargo test --lib tool::stream_default 2>&1 | grep -E "^error|no method|cannot find" | head`
Expected: `no method named 'supports_streaming'` / `execute_streaming`

- [ ] **Step 3: 给 trait 加默认方法** — 在 `tool.rs` 的 `Tool` trait 内,`execute` 声明(第 213 行)之后插入:

```rust
    /// 是否支持流式输出。默认 false —— 只有增量产出的工具(BashTool)override。
    /// dispatcher 用它决定是否为本次调用搭建合并节流 drain 任务。
    fn supports_streaming(&self) -> bool { false }

    /// 流式变体。默认忽略 sink、委托 `execute()`。
    /// 只有 `supports_streaming() == true` 的工具才 override 它。
    async fn execute_streaming(
        &self,
        params: serde_json::Value,
        _sink: crate::agent::tools::stream::ToolStreamSink,
    ) -> Result<ToolOutput, ToolError> {
        self.execute(params).await
    }
```

- [ ] **Step 4: 加 `execute_streaming_with_context` 自由函数** — 在 `execute_tool_with_context`(第 243-249 行)之后插入:

```rust
/// 流式版本的 `execute_tool_with_context`。dispatcher 串行路径在工具
/// `supports_streaming()` 时调用它,把 sink 传进工具。
pub async fn execute_streaming_with_context(
    tool: &dyn Tool,
    params: serde_json::Value,
    _ctx: &ToolExecutionContext,
    sink: crate::agent::tools::stream::ToolStreamSink,
) -> Result<ToolOutput, ToolError> {
    tool.execute_streaming(params, sink).await
}
```

- [ ] **Step 5: 运行确认绿**

Run: `cd src-tauri && cargo test --lib tool::stream_default 2>&1 | tail -6`
Expected: `test result: ok. 1 passed`

- [ ] **Step 6: 全量编译**(确认 146 个实现未被破坏)

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无输出

- [ ] **Step 7: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/agent/tools/tool.rs
git commit -m "$(cat <<'EOF'
feat(tools): additive Tool::execute_streaming + supports_streaming

Default execute_streaming delegates to execute() (blast radius: 0 — all
146 existing impls + MCP adapter inherit the no-op). supports_streaming
defaults false. Adds execute_streaming_with_context mirror.

Verification: cargo test --lib tool::stream_default → ok; cargo build → no errors
EOF
)"
```

---

## Task 3: OverflowSink 增量 temp 落盘

**Files:**
- Modify: `src-tauri/src/agent/tools/builtin/shell.rs`(在 `RollingTailBuffer` 之后、`BashTool` 之前加 `OverflowSink`;给 `RollingTailBuffer` 加 `as_bytes`)

- [ ] **Step 1: 给 RollingTailBuffer 加 `as_bytes`** — `OverflowSink` 越界时要把当前 tail 刷进文件。在 `shell.rs` 的 `impl RollingTailBuffer`(第 124 行起)里,`to_truncated_string` 之前加:

```rust
    /// 当前缓冲区内容(连续字节)。供首次越界时刷入 temp 文件。
    fn as_bytes(&self) -> Vec<u8> {
        let (a, b) = self.buf.as_slices();
        let mut v = Vec::with_capacity(a.len() + b.len());
        v.extend_from_slice(a);
        v.extend_from_slice(b);
        v
    }
```

- [ ] **Step 2: 写失败测试** — 在 `shell.rs` 的 `#[cfg(test)] mod tests`(第 674 行起)中加:

```rust
    #[test]
    fn overflow_sink_no_file_under_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let mut tail = RollingTailBuffer::new(32 * 1024);
        let mut sink = OverflowSink::new(Some(dir.path().to_path_buf()));
        let chunk = vec![b'x'; 1000];
        tail.push_bytes(&chunk);
        sink.write(&tail, &chunk);
        sink.finish();
        assert!(sink.path().is_none(), "should not open a file under 32KB");
    }

    #[test]
    fn overflow_sink_writes_full_content_over_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let mut tail = RollingTailBuffer::new(32 * 1024);
        let mut sink = OverflowSink::new(Some(dir.path().to_path_buf()));
        // 写 40KB,分两块:第一块 30KB(未越界),第二块 15KB(越界)
        let c1 = vec![b'a'; 30 * 1024];
        let c2 = vec![b'b'; 15 * 1024];
        tail.push_bytes(&c1); sink.write(&tail, &c1);
        tail.push_bytes(&c2); sink.write(&tail, &c2);
        sink.finish();
        let path = sink.path().expect("file should exist over 32KB").to_path_buf();
        let written = std::fs::read(&path).unwrap();
        assert_eq!(written.len(), 45 * 1024, "temp file must hold the FULL output, not the 32KB tail");
        assert_eq!(&written[..30 * 1024], &c1[..]);
        assert_eq!(&written[30 * 1024..], &c2[..]);
    }
```

- [ ] **Step 3: 运行确认红**

Run: `cd src-tauri && cargo test --lib shell::tests::overflow_sink 2>&1 | grep -E "^error|cannot find" | head`
Expected: `cannot find ... OverflowSink`

- [ ] **Step 4: 实现 OverflowSink** — 在 `shell.rs` 的 `RollingTailBuffer` impl 之后、`BashTool` 定义(第 177 行)之前插入:

```rust
use std::io::Write as _;

/// 增量溢出落盘:首次累计字节越过 `CONTEXT_LIMIT` 时开 temp 文件,
/// 先把彼时的 rolling tail(尚未丢弃)刷进去,之后每块直接 append。
/// 内存恒定,完整输出在磁盘,供前端「加载完整日志」。
struct OverflowSink {
    dir: Option<PathBuf>,
    file: Option<std::fs::File>,
    path: Option<PathBuf>,
    total: usize,
}

impl OverflowSink {
    fn new(dir: Option<PathBuf>) -> Self {
        Self { dir, file: None, path: None, total: 0 }
    }

    fn write(&mut self, tail_buf: &RollingTailBuffer, chunk: &[u8]) {
        self.total += chunk.len();
        if self.file.is_none() {
            if self.total <= CONTEXT_LIMIT {
                return; // 仍在内存可容范围,不落盘
            }
            // 首次越界:开文件 + 刷入当前 tail(此刻还没开始丢字节)
            let Some(dir) = self.dir.clone() else { return };
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join(format!("bash-{}.log", uuid::Uuid::new_v4()));
            match std::fs::File::create(&path) {
                Ok(mut f) => {
                    if let Err(e) = f.write_all(&tail_buf.as_bytes()) {
                        tracing::warn!(path = %path.display(), error = %e, "bash: temp flush failed");
                        return;
                    }
                    self.file = Some(f);
                    self.path = Some(path);
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "bash: failed to open overflow log");
                }
            }
        } else if let Some(f) = &mut self.file {
            if let Err(e) = f.write_all(chunk) {
                tracing::warn!(error = %e, "bash: temp append failed");
            }
        }
    }

    fn finish(&mut self) {
        if let Some(f) = &mut self.file {
            let _ = f.flush();
        }
        self.file = None;
    }

    fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }
}
```

> 注:`CONTEXT_LIMIT`(模块级 `const = 32 * 1024`,Task 1 of Sprint 1 已存在)、`PathBuf`、`uuid` 均已在 `shell.rs` 作用域。`tempfile` 在 dev-dependencies(测试已用过)。

- [ ] **Step 5: 运行确认绿**

Run: `cd src-tauri && cargo test --lib shell::tests::overflow_sink 2>&1 | tail -6`
Expected: `test result: ok. 2 passed`

- [ ] **Step 6: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/agent/tools/builtin/shell.rs
git commit -m "$(cat <<'EOF'
feat(bash): OverflowSink for incremental temp-file logging

Replaces Task-1's write-on-completion temp logic with a streaming-friendly
incremental writer: opens ~/.uclaw/temp/bash-<uuid>.log lazily on the first
chunk that crosses 32KB, flushes the rolling tail (not yet dropped), then
appends every subsequent chunk. Memory stays bounded; the full log lives
on disk for the frontend's load-full-log button.

Verification: cargo test --lib shell::tests::overflow_sink → ok, 2 passed
EOF
)"
```

---

## Task 4: shell.rs execute_streaming select 读循环

**Files:**
- Modify: `src-tauri/src/agent/tools/builtin/shell.rs:500-672`(把 `execute` 主体迁到 `execute_streaming`,`execute` 委托;impl `Tool` 加 `supports_streaming` + `execute_streaming`)

- [ ] **Step 1: 写失败集成测试** — 验证流式 sink 收到交错的 stdout/stderr 块、seq 单调、最终结果含两者。在 `shell.rs` `#[cfg(test)] mod tests` 中加:

```rust
    #[tokio::test]
    async fn execute_streaming_emits_interleaved_chunks() {
        use crate::agent::tools::stream::{ToolStream, ToolStreamSink};
        let tool = BashTool::new(std::path::PathBuf::from("/tmp"));
        let (sink, mut rx) = ToolStreamSink::channel(64);
        let params = serde_json::json!({
            "command": "printf 'OUT1\\n'; printf 'ERR1\\n' 1>&2; printf 'OUT2\\n'"
        });
        let out = tool.execute_streaming(params, sink).await.unwrap();

        let mut events = vec![];
        while let Ok(e) = rx.try_recv() { events.push(e); }
        assert!(!events.is_empty(), "streaming sink should have received chunks");
        // seq 单调递增
        for w in events.windows(2) { assert!(w[1].seq > w[0].seq); }
        // 两个管道都出现过
        assert!(events.iter().any(|e| e.stream == ToolStream::Stdout));
        assert!(events.iter().any(|e| e.stream == ToolStream::Stderr));
        // 最终结果同时含 stdout 和 stderr 内容
        let output = out.result["output"].as_str().unwrap();
        assert!(output.contains("OUT1") && output.contains("OUT2"));
        assert!(output.contains("ERR1"));
    }

    #[tokio::test]
    async fn execute_still_works_without_streaming() {
        let tool = BashTool::new(std::path::PathBuf::from("/tmp"));
        let out = tool.execute(serde_json::json!({ "command": "echo hello" })).await.unwrap();
        assert!(out.result["output"].as_str().unwrap().contains("hello"));
        assert_eq!(out.result["exit_code"], serde_json::json!(0));
    }
```

- [ ] **Step 2: 运行确认红**

Run: `cd src-tauri && cargo test --lib shell::tests::execute_streaming 2>&1 | grep -E "^error|no method" | head`
Expected: `no method named 'execute_streaming' found for ... BashTool`

- [ ] **Step 3: 把 execute 主体改造为 execute_streaming** — 替换 `shell.rs` 第 500-672 行的整个 `async fn execute(...)`。新结构:`execute` 变薄(委托),真实逻辑搬进 `execute_streaming`,读循环改 `tokio::select!`。

先把第 500 行的签名改为内部方法(注意:`execute`/`execute_streaming` 都是 trait 方法,要放进 `impl Tool for BashTool`。这里把核心逻辑抽成一个 `BashTool` 的固有方法 `run_streaming`,两个 trait 方法都调它)。

在 `impl BashTool`(`resolve_temp_dir` 之后)新增固有方法,内容为原 `execute` 主体改造版:

```rust
    /// 核心执行逻辑,带可选流式 sink。`execute` / `execute_streaming` 都委托到这里。
    async fn run(&self, params: serde_json::Value, sink: crate::agent::tools::stream::ToolStreamSink)
        -> Result<ToolOutput, ToolError>
    {
        use crate::agent::tools::stream::ToolStream;
        use tokio::io::AsyncReadExt;

        let start = std::time::Instant::now();

        let command = params["command"].as_str()
            .ok_or_else(|| ToolError::InvalidParams("command is required".into()))?;
        let working_dir = match params["working_dir"].as_str() {
            Some(dir) => { let p = PathBuf::from(dir); if p.is_absolute() { p } else { self.workspace_root.join(p) } }
            None => self.workspace_root.clone(),
        };
        let timeout = params["timeout_ms"].as_u64().map(Duration::from_millis).unwrap_or(DEFAULT_TIMEOUT);
        let daemon = params["daemon"].as_bool().unwrap_or(false);

        debug!(command = %command, working_dir = %working_dir.display(), timeout_ms = %timeout.as_millis(), daemon = %daemon, "bash: executing command");

        // --- Safety checks (与原实现一致) ---
        if let Some(reason) = Self::check_blocked(command) {
            warn!(command = %command, reason = %reason, "bash: command blocked");
            return Ok(ToolOutput::error(&format!("Command blocked: {reason}"), start.elapsed().as_millis() as u64));
        }
        if let Some(reason) = Self::check_working_dir(&working_dir) {
            warn!(working_dir = %working_dir.display(), reason = %reason, "bash: working directory blocked");
            return Ok(ToolOutput::error(&format!("Working directory blocked: {reason}"), start.elapsed().as_millis() as u64));
        }
        if !working_dir.exists() {
            return Ok(ToolOutput::error(&format!("Working directory does not exist: {}", working_dir.display()), start.elapsed().as_millis() as u64));
        }

        // --- Daemon 分支:不流式,原样返回 ---
        if daemon {
            return Self::spawn_daemon(command, &working_dir, start);
        }

        let mut child = Command::new("sh")
            .arg("-c").arg(command)
            .current_dir(&working_dir)
            .stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                let kind = if e.kind() == std::io::ErrorKind::NotFound { ToolErrorKind::ResourceNotFound }
                    else if e.kind() == std::io::ErrorKind::PermissionDenied { ToolErrorKind::PermissionDenied }
                    else { ToolErrorKind::Other };
                ToolError::kinded_with_source(kind, "Failed to spawn process", e.to_string())
            })?;

        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let mut tail_buf = RollingTailBuffer::new(CONTEXT_LIMIT);
        let mut overflow = OverflowSink::new(self.resolve_temp_dir());

        let result = tokio::time::timeout(timeout, async {
            let mut buf_out = [0u8; 8192];
            let mut buf_err = [0u8; 8192];
            let (mut out_open, mut err_open) = (stdout.is_some(), stderr.is_some());

            while out_open || err_open {
                tokio::select! {
                    r = async { stdout.as_mut().unwrap().read(&mut buf_out).await }, if out_open => {
                        match r {
                            Ok(0) | Err(_) => out_open = false,
                            Ok(n) => {
                                let chunk = &buf_out[..n];
                                tail_buf.push_bytes(chunk);
                                overflow.write(&tail_buf, chunk);
                                sink.send(ToolStream::Stdout, chunk);
                            }
                        }
                    },
                    r = async { stderr.as_mut().unwrap().read(&mut buf_err).await }, if err_open => {
                        match r {
                            Ok(0) | Err(_) => err_open = false,
                            Ok(n) => {
                                let chunk = &buf_err[..n];
                                tail_buf.push_bytes(chunk);
                                overflow.write(&tail_buf, chunk);
                                sink.send(ToolStream::Stderr, chunk);
                            }
                        }
                    },
                }
            }
            child.wait().await
        }).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(status) => {
                overflow.finish();
                let combined = tail_buf.to_truncated_string(overflow.path());
                let exit_code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
                debug!(exit_code = %exit_code, output_len = %combined.len(), "bash: command completed");
                let result = serde_json::json!({ "ok": exit_code == 0, "exit_code": exit_code, "output": combined });
                Ok(ToolOutput::new(result, duration_ms))
            }
            Err(_) => {
                warn!(command = %command, timeout_ms = %timeout.as_millis(), "bash: command timed out, killing process");
                overflow.finish();
                Ok(ToolOutput::error(&format!("Command timed out after {}ms", timeout.as_millis()), duration_ms))
            }
        }
    }
```

> 关键差异 vs 原实现:不再用 `tokio::join!(read_to_end, read_to_end)`,改 `tokio::select!` 分块读;stdout/stderr **按真实到达顺序**交错进 `tail_buf` / `overflow` / `sink`(满足"真实交错");溢出落盘由 `OverflowSink` 增量完成(替代原 617-643 行一次性写)。`select!` 分支用 `async { reader.as_mut().unwrap().read(..).await }` + `if open` guard,EOF/Err 关闭该管道。

- [ ] **Step 4: 把两个 trait 方法委托到 `run`** — 在 `impl Tool for BashTool` 中,把原 `async fn execute(...)` 整体(第 500-671 行)替换为:

```rust
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        self.run(params, crate::agent::tools::stream::ToolStreamSink::noop()).await
    }

    fn supports_streaming(&self) -> bool { true }

    async fn execute_streaming(
        &self,
        params: serde_json::Value,
        sink: crate::agent::tools::stream::ToolStreamSink,
    ) -> Result<ToolOutput, ToolError> {
        self.run(params, sink).await
    }
```

> `path_args`(第 496-498 行)保持不动。

- [ ] **Step 5: 运行确认绿**

Run: `cd src-tauri && cargo test --lib shell::tests 2>&1 | tail -12`
Expected: `test result: ok.` 全过(含 `execute_streaming_emits_interleaved_chunks`、`execute_still_works_without_streaming`,及 Sprint 1 已有 bash 测试)

- [ ] **Step 6: 全量编译**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无输出

- [ ] **Step 7: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/agent/tools/builtin/shell.rs
git commit -m "$(cat <<'EOF'
feat(bash): streaming read loop with interleaved stdout/stderr

Rewrite the bash read path from tokio::join!(read_to_end) to a
tokio::select! chunked loop. Each 8KB chunk feeds the 32KB rolling tail,
the incremental OverflowSink, and the ToolStreamSink in true arrival
order. execute() delegates to run() with a noop sink; execute_streaming()
passes the real sink; supports_streaming() = true.

Verification: cargo test --lib shell::tests → ok, all passed; cargo build → no errors
EOF
)"
```

---

## Task 5: dispatcher 合并节流 + emit tool_output_chunk

**Files:**
- Modify: `src-tauri/src/agent/dispatcher.rs:2664-2708`(串行路径)+ import(第 8 行)

- [ ] **Step 1: import streaming 函数** — 在 `dispatcher.rs` 第 8 行的 tools import 里追加 `execute_streaming_with_context`:

```rust
    execute_tool_with_context, execute_streaming_with_context, ToolExecutionContext, ToolRegistry, ToolOutput,
```

- [ ] **Step 2: 加 emit 方法** — 在 `dispatcher.rs` 的 `emit_tool_result`(第 1055 行)之后插入一个发送分块的方法:

```rust
    /// Emit 一段流式工具输出块到前端(合并节流后调用)。
    fn emit_tool_output_chunk(&self, id: &str, seq: u64, stream: &str, chunk: &str) {
        let _ = self.app_handle.emit("chat:stream-tool-activity", serde_json::json!({
            "conversationId": self.conversation_id,
            "activity": {
                "type": "tool_output_chunk",
                "toolCallId": id,
                "seq": seq,
                "stream": stream,   // "stdout" | "stderr"
                "chunk": chunk,
            }
        }));
    }
```

- [ ] **Step 3: 在串行路径搭建 sink + 合并节流 drain** — 替换 `dispatcher.rs` 第 2665-2698 行(`let tool_start = ...` 到 `execute_result` 的 `match ... .await { ... };`)。改造点:仅当 `supports_streaming()` 时建 channel + coalescer,并走 `execute_streaming_with_context`;否则保持原 `execute_tool_with_context` 路径不变。

```rust
                    let tool_start = std::time::Instant::now();

                    // 该工具是否支持流式输出(BashTool = true)。
                    let wants_stream = self.tools.get(&tc.name).map(|t| t.supports_streaming()).unwrap_or(false);

                    // 仅流式工具搭建合并节流 drain(~50ms 或 8KB 先到先发)。
                    let coalescer = if wants_stream {
                        let (sink, mut rx) = crate::agent::tools::stream::ToolStreamSink::channel(256);
                        let app = self.app_handle.clone();
                        let conv = self.conversation_id.clone();
                        let id = tc.id.clone();
                        let handle = tokio::spawn(async move {
                            // 按 stream 累积,定时/超量 flush
                            let mut buf_out = String::new();
                            let mut buf_err = String::new();
                            let mut last_seq: u64 = 0;
                            let mut tick = tokio::time::interval(std::time::Duration::from_millis(50));
                            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                            let flush = |app: &tauri::AppHandle, conv: &str, id: &str, last_seq: u64,
                                         buf_out: &mut String, buf_err: &mut String| {
                                if !buf_out.is_empty() {
                                    let _ = app.emit("chat:stream-tool-activity", serde_json::json!({
                                        "conversationId": conv,
                                        "activity": { "type": "tool_output_chunk", "toolCallId": id,
                                                      "seq": last_seq, "stream": "stdout", "chunk": std::mem::take(buf_out) }
                                    }));
                                }
                                if !buf_err.is_empty() {
                                    let _ = app.emit("chat:stream-tool-activity", serde_json::json!({
                                        "conversationId": conv,
                                        "activity": { "type": "tool_output_chunk", "toolCallId": id,
                                                      "seq": last_seq, "stream": "stderr", "chunk": std::mem::take(buf_err) }
                                    }));
                                }
                            };
                            loop {
                                tokio::select! {
                                    ev = rx.recv() => match ev {
                                        Some(e) => {
                                            last_seq = e.seq;
                                            let s = String::from_utf8_lossy(&e.bytes);
                                            match e.stream {
                                                crate::agent::tools::stream::ToolStream::Stdout => buf_out.push_str(&s),
                                                crate::agent::tools::stream::ToolStream::Stderr => buf_err.push_str(&s),
                                            }
                                            if buf_out.len() + buf_err.len() >= 8192 {
                                                flush(&app, &conv, &id, last_seq, &mut buf_out, &mut buf_err);
                                            }
                                        }
                                        None => { flush(&app, &conv, &id, last_seq, &mut buf_out, &mut buf_err); break; }
                                    },
                                    _ = tick.tick() => flush(&app, &conv, &id, last_seq, &mut buf_out, &mut buf_err),
                                }
                            }
                        });
                        Some((sink, handle))
                    } else {
                        None
                    };

                    let tool_name_for_panic = tc.name.clone();
                    let tools_arc = Arc::clone(&self.tools);
                    let sink_for_spawn = coalescer.as_ref().map(|(s, _)| s.clone());
                    let execute_result = match tokio::task::spawn(async move {
                        match tools_arc.get(&tool_name_for_panic) {
                            Some(t) => match sink_for_spawn {
                                Some(sink) => execute_streaming_with_context(t, tool_args_for_spawn, &tool_context_for_spawn, sink).await,
                                None => execute_tool_with_context(t, tool_args_for_spawn, &tool_context_for_spawn).await,
                            },
                            None => Err(crate::agent::tools::tool::ToolError::NotFound(tool_name_for_panic)),
                        }
                    }).await {
                        Ok(Ok(out)) => Ok(out),
                        Ok(Err(e)) => Err(e),
                        Err(join_err) if join_err.is_panic() => {
                            tracing::error!(tool = %tc.name, "tool panicked");
                            Err(crate::agent::tools::tool::ToolError::Execution(format!(
                                "Tool '{}' crashed unexpectedly. See ~/.uclaw/logs/crashes/ for details.", tc.name,
                            )))
                        }
                        Err(join_err) => {
                            tracing::error!(tool = %tc.name, %join_err, "tool join error");
                            Err(crate::agent::tools::tool::ToolError::Execution(format!("Tool join error: {}", join_err)))
                        }
                    };

                    // 工具结束 → 关 sink(drop) → coalescer 收尾 flush 后退出。
                    if let Some((sink, handle)) = coalescer {
                        drop(sink);
                        let _ = handle.await;
                    }
```

> 说明:`sink_for_spawn` 是 sink 的 clone 移入执行闭包;原始 sink 留在 `coalescer` 元组里,执行完后显式 `drop` → channel 关闭 → coalescer 的 `rx.recv()` 返回 `None` → 收尾 flush 后 break。`emit_tool_output_chunk`(Step 2)目前未被 coalescer 直接用(coalescer 内联了 emit 以避免 `&self` 跨 spawn);保留它供将来非闭包路径复用,或在 Step 2 改为不加该方法 —— 二选一,默认保留 inline emit、删掉 Step 2 的方法以避免 dead_code 警告。

**修订 Step 2 决定**:为避免 `dead_code`,**不**新增 `emit_tool_output_chunk` 方法(coalescer 内联 emit 已足够)。跳过 Step 2,直接做 Step 3。

- [ ] **Step 4: 全量编译**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: 无输出

- [ ] **Step 5: 回归测试**(确认串行/并行工具路径未坏)

Run: `cd src-tauri && cargo test --lib dispatcher 2>&1 | tail -10`
Expected: `test result: ok.` 全过

- [ ] **Step 6: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/agent/dispatcher.rs
git commit -m "$(cat <<'EOF'
feat(agent): coalesce + emit bash output chunks in serial path

When a tool supports_streaming() (bash), the serial execute path builds an
mpsc channel + a coalescer task that batches chunks (~50ms / 8KB) and emits
chat:stream-tool-activity tool_output_chunk events. Non-streaming tools keep
the existing execute_tool_with_context path untouched. Bash is never in the
parallel JoinSet batch (not parallel-safe), so serial-only wiring is sufficient.

Verification: cargo build → no errors; cargo test --lib dispatcher → ok
EOF
)"
```

---

## Task 6: read_bash_log Tauri 命令

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`(新增命令)
- Modify: `src-tauri/src/main.rs`(`invoke_handler!` 注册)

- [ ] **Step 1: 写失败测试** — 在 `tauri_commands.rs` 末尾 `#[cfg(test)]` 块(若无则新建)加纯函数路径校验测试。先把可测逻辑抽成自由函数 `read_capped_in_temp`:

```rust
#[cfg(test)]
mod read_bash_log_tests {
    use super::*;

    #[test]
    fn rejects_path_outside_temp() {
        let temp = std::path::PathBuf::from("/home/u/.uclaw/temp");
        let res = read_capped_in_temp(&temp, "/etc/passwd", 1024);
        assert!(res.is_err(), "must reject paths outside temp dir");
    }

    #[test]
    fn reads_file_inside_temp_capped() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bash-x.log");
        std::fs::write(&p, vec![b'a'; 100]).unwrap();
        let content = read_capped_in_temp(dir.path(), p.to_str().unwrap(), 50).unwrap();
        assert!(content.len() <= 50 + 200, "content should be capped (+truncation note)");
        assert!(content.contains("aaaa"));
    }
}
```

- [ ] **Step 2: 运行确认红**

Run: `cd src-tauri && cargo test --lib read_bash_log 2>&1 | grep -E "^error|cannot find" | head`
Expected: `cannot find function 'read_capped_in_temp'`

- [ ] **Step 3: 实现** — 在 `tauri_commands.rs` 加自由函数 + 命令:

```rust
/// 读取 temp 目录内的 bash 日志文件,限制在 `temp_dir` 内,内容上限 `cap` 字节。
fn read_capped_in_temp(temp_dir: &std::path::Path, path: &str, cap: usize) -> Result<String, String> {
    let p = std::path::PathBuf::from(path);
    // 规范化后必须落在 temp_dir 内(防目录穿越)
    let canon_temp = temp_dir.canonicalize().unwrap_or_else(|_| temp_dir.to_path_buf());
    let canon_p = p.canonicalize().unwrap_or_else(|_| p.clone());
    if !canon_p.starts_with(&canon_temp) {
        return Err("path outside temp dir".into());
    }
    let bytes = std::fs::read(&canon_p).map_err(|e| e.to_string())?;
    if bytes.len() > cap {
        let tail = &bytes[bytes.len() - cap..];
        Ok(format!(
            "[日志过大:共 {} 字节,仅显示最后 {} 字节]\n\n{}",
            bytes.len(), cap, String::from_utf8_lossy(tail)
        ))
    } else {
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// 读取 bash 溢出日志(前端「加载完整日志」按钮)。限 ~/.uclaw/temp/,上限 5MB。
#[tauri::command]
pub async fn read_bash_log(path: String) -> Result<String, String> {
    let temp = uclaw_utils_home::uclaw_home_pathbuf()
        .map_err(|e| e.to_string())?
        .join("temp");
    read_capped_in_temp(&temp, &path, 5 * 1024 * 1024)
}
```

> 注:`tauri_commands.rs` 顶部若未引入 `uclaw_utils_home`,直接用全限定路径(如上)即可,无需加 `use`。

- [ ] **Step 4: 注册命令** — 在 `src-tauri/src/main.rs` 的 `tauri::generate_handler![` / `invoke_handler!` 列表里加 `tauri_commands::read_bash_log,`(放在其它 `tauri_commands::` 项附近)。

Run 定位:`grep -n "tauri_commands::" src-tauri/src/main.rs | head -3`

- [ ] **Step 5: 运行确认绿 + 编译**

Run: `cd src-tauri && cargo test --lib read_bash_log 2>&1 | tail -6 && cargo build 2>&1 | grep -E "^error" | head`
Expected: `test result: ok. 2 passed`;编译无 error

- [ ] **Step 6: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "$(cat <<'EOF'
feat(api): read_bash_log command for load-full-log button

Reads a bash overflow log from ~/.uclaw/temp/ (path-traversal guarded via
canonicalize + starts_with), capped at 5MB with a truncation note. Backs
the frontend "load full log" affordance when the live window dropped head.

Verification: cargo test --lib read_bash_log → ok, 2 passed; cargo build → no errors
EOF
)"
```

---

## Task 7: 前端 LiveOutput 状态 + 有界环 append

**Files:**
- Modify: `ui/src/atoms/agent-atoms.ts`(`ToolActivity` 加字段 + `LiveOutput` 类型 + `appendLiveOutput` 纯函数)
- Test: `ui/src/atoms/agent-atoms.live-output.test.ts`(新建)

- [ ] **Step 1: 写失败测试** — 新建 `ui/src/atoms/agent-atoms.live-output.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { appendLiveOutput, type LiveOutput } from './agent-atoms'

describe('appendLiveOutput', () => {
  it('creates segments on first chunk', () => {
    const out = appendLiveOutput(undefined, 'stdout', 'hello')
    expect(out.segments).toEqual([{ stream: 'stdout', text: 'hello' }])
    expect(out.bytes).toBe(5)
    expect(out.droppedHead).toBe(false)
  })

  it('merges consecutive same-stream chunks into one segment', () => {
    let out = appendLiveOutput(undefined, 'stdout', 'foo')
    out = appendLiveOutput(out, 'stdout', 'bar')
    expect(out.segments).toEqual([{ stream: 'stdout', text: 'foobar' }])
  })

  it('starts a new segment when the stream switches', () => {
    let out = appendLiveOutput(undefined, 'stdout', 'out')
    out = appendLiveOutput(out, 'stderr', 'err')
    expect(out.segments).toEqual([
      { stream: 'stdout', text: 'out' },
      { stream: 'stderr', text: 'err' },
    ])
  })

  it('drops head and sets droppedHead when exceeding 256KB', () => {
    let out: LiveOutput | undefined = undefined
    const big = 'x'.repeat(100 * 1024)
    out = appendLiveOutput(out, 'stdout', big)
    out = appendLiveOutput(out, 'stdout', big)
    out = appendLiveOutput(out, 'stdout', big) // 300KB total > 256KB
    expect(out.droppedHead).toBe(true)
    expect(out.bytes).toBeLessThanOrEqual(256 * 1024)
  })
})
```

- [ ] **Step 2: 运行确认红**

Run: `cd ui && npm test -- --run agent-atoms.live-output 2>&1 | tail -10`
Expected: FAIL — `appendLiveOutput` is not a function / not exported

- [ ] **Step 3: 加类型 + 字段** — 在 `agent-atoms.ts` 的 `ToolActivity` 接口(第 16-32 行)`imageAttachments` 之后加字段,并在其前定义 `LiveOutput`:

```ts
/** Bash 等流式工具的实时输出(临时,仅 live 会话;reload 不重建) */
export interface LiveOutput {
  segments: { stream: 'stdout' | 'stderr'; text: string }[]
  bytes: number
  droppedHead: boolean
}
```

在 `ToolActivity` 接口内加:

```ts
  /** 流式工具的实时输出窗口(有界 256KB);done 后由持久化 result 接管 */
  liveOutput?: LiveOutput
```

- [ ] **Step 4: 实现 appendLiveOutput** — 在 `agent-atoms.ts` 顶部常量区加上限,并在 `appendToolHistory`(第 545 行)附近加纯函数:

```ts
/** 实时输出窗口上限 */
const LIVE_OUTPUT_MAX_BYTES = 256 * 1024

/**
 * 向 LiveOutput 追加一块输出(纯函数)。
 * - 连续同 stream 合并进同一段
 * - 超过 256KB 从头部丢弃并置 droppedHead
 */
export function appendLiveOutput(
  prev: LiveOutput | undefined,
  stream: 'stdout' | 'stderr',
  text: string,
): LiveOutput {
  const segments = prev ? prev.segments.map((s) => ({ ...s })) : []
  const last = segments[segments.length - 1]
  if (last && last.stream === stream) {
    last.text += text
  } else {
    segments.push({ stream, text })
  }
  let bytes = (prev?.bytes ?? 0) + text.length
  let droppedHead = prev?.droppedHead ?? false

  // 超量:从头部段裁剪
  while (bytes > LIVE_OUTPUT_MAX_BYTES && segments.length > 0) {
    const head = segments[0]!
    const over = bytes - LIVE_OUTPUT_MAX_BYTES
    if (head.text.length <= over) {
      bytes -= head.text.length
      segments.shift()
    } else {
      head.text = head.text.slice(over)
      bytes -= over
    }
    droppedHead = true
  }

  return { segments, bytes, droppedHead }
}
```

- [ ] **Step 5: 运行确认绿 + 类型检查**

Run: `cd ui && npm test -- --run agent-atoms.live-output 2>&1 | tail -8 && npx tsc --noEmit 2>&1 | head -5`
Expected: `4 passed`;tsc 无输出

- [ ] **Step 6: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/atoms/agent-atoms.ts ui/src/atoms/agent-atoms.live-output.test.ts
git commit -m "$(cat <<'EOF'
feat(ui): LiveOutput state + bounded-ring appendLiveOutput

ToolActivity.liveOutput holds streamed bash output as stream-tagged
segments, bounded to 256KB (drops head + sets droppedHead beyond that).
appendLiveOutput is a pure reducer: merges consecutive same-stream chunks,
trims the head ring on overflow.

Verification: npm test -- --run agent-atoms.live-output → 4 passed; tsc clean
EOF
)"
```

---

## Task 8: listener 接 tool_output_chunk

**Files:**
- Modify: `ui/src/hooks/useGlobalAgentListeners.ts:384-422`

- [ ] **Step 1: 在主 tool-activity listener 加分支** — 在 `useGlobalAgentListeners.ts` 第 401 行的 `} else if (ev.type === 'tool_result') {` **之前**插入 `tool_output_chunk` 分支(import `appendLiveOutput`):

文件顶部 import 区(找到从 `@/atoms/agent-atoms` 的 import)追加 `appendLiveOutput`:

```ts
import { appendLiveOutput } from '@/atoms/agent-atoms'
```

> 若已有从 `@/atoms/agent-atoms` 的具名 import,合并进去而非新增一行。

在 listener 的 `if (ev.type === 'tool_start') { ... }` 之后、`else if (ev.type === 'tool_result')` 之前插入:

```ts
        } else if (ev.type === 'tool_output_chunk') {
          const idx = activities.findIndex((a) => a.toolUseId === ev.toolCallId)
          if (idx >= 0) {
            const streamKind = ev.stream === 'stderr' ? 'stderr' : 'stdout'
            activities[idx] = {
              ...activities[idx]!,
              liveOutput: appendLiveOutput(activities[idx]!.liveOutput, streamKind, String(ev.chunk ?? '')),
            }
          }
```

> 注意原代码用 `if (...) { } else if (...) { }` 链(第 391/401 行)。把上面这段插成链中的一段:即把第 401 行 `} else if (ev.type === 'tool_result') {` 改为先接上面这段、再接 `tool_result`。最终结构:`if tool_start {} else if tool_output_chunk {} else if tool_result {}`。

- [ ] **Step 2: 类型检查**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -8`
Expected: 无输出(`ev` 是 `any`,`appendLiveOutput` 已导出)

- [ ] **Step 3: 写 listener 行为测试**(可选但推荐)— 若该 hook 已有测试套件则加用例;否则跳过(逻辑已由 Task 7 的纯函数测试覆盖,listener 仅做路由)。本步以 tsc 通过为准。

- [ ] **Step 4: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/hooks/useGlobalAgentListeners.ts
git commit -m "$(cat <<'EOF'
feat(ui): route tool_output_chunk events into ToolActivity.liveOutput

The chat:stream-tool-activity listener now handles the new
tool_output_chunk activity type, appending to the matching activity's
liveOutput via appendLiveOutput (keyed by toolCallId).

Verification: npx tsc --noEmit → clean
EOF
)"
```

---

## Task 9: BashStreamView 实时渲染 + 接入 ToolActivityItem

**Files:**
- Create: `ui/src/components/agent/tool-renderers/BashStreamView.tsx`
- Modify: `ui/src/components/agent/ToolActivityItem.tsx`(在 bash 运行中 + 有 liveOutput 时渲染 BashStreamView)
- Test: `ui/src/components/agent/tool-renderers/BashStreamView.test.tsx`(新建)

- [ ] **Step 1: 写失败测试** — 新建 `BashStreamView.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BashStreamView } from './BashStreamView'
import type { LiveOutput } from '@/atoms/agent-atoms'

vi.mock('@/lib/tauri-bridge', () => ({ invoke: vi.fn() }))

describe('BashStreamView', () => {
  it('renders stdout and stderr segments', () => {
    const live: LiveOutput = {
      segments: [
        { stream: 'stdout', text: 'building...\n' },
        { stream: 'stderr', text: 'warning: x\n' },
      ],
      bytes: 22,
      droppedHead: false,
    }
    render(<BashStreamView command="npm run build" live={live} logPath={undefined} />)
    expect(screen.getByText(/building/)).toBeInTheDocument()
    expect(screen.getByText(/warning: x/)).toBeInTheDocument()
  })

  it('shows truncation affordance when droppedHead', () => {
    const live: LiveOutput = { segments: [{ stream: 'stdout', text: 'tail' }], bytes: 4, droppedHead: true }
    render(<BashStreamView command="cat big" live={live} logPath="/home/u/.uclaw/temp/bash-x.log" />)
    expect(screen.getByText(/早期输出已截断/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /加载完整日志/ })).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: 运行确认红**

Run: `cd ui && npm test -- --run BashStreamView 2>&1 | tail -8`
Expected: FAIL — cannot find module `./BashStreamView`

- [ ] **Step 3: 实现 BashStreamView** — 新建 `ui/src/components/agent/tool-renderers/BashStreamView.tsx`:

```tsx
import * as React from 'react'
import { cn } from '@/lib/utils'
import { invoke } from '@/lib/tauri-bridge'
import type { LiveOutput } from '@/atoms/agent-atoms'

interface Props {
  command: string
  live: LiveOutput
  /** 从 tool_result 截断头注解析出的 temp 路径;有值才显示「加载完整日志」 */
  logPath?: string
}

/**
 * Bash 实时流式输出视图。stdout 默认色、stderr 用 text-destructive;
 * 流式期间自动滚动到底部;droppedHead 时提供「加载完整日志」(读 temp 文件)。
 */
export function BashStreamView({ command, live, logPath }: Props): React.ReactElement {
  const scrollRef = React.useRef<HTMLPreElement>(null)
  const [fullLog, setFullLog] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(false)

  // 流式期间自动贴底(用户未手动上滚时)
  React.useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40
    if (nearBottom) el.scrollTop = el.scrollHeight
  }, [live.bytes])

  const handleLoadFull = React.useCallback(async () => {
    if (!logPath) return
    setLoading(true)
    try {
      const content = await invoke<string>('read_bash_log', { path: logPath })
      setFullLog(content)
    } catch (e) {
      setFullLog(`加载失败: ${String(e)}`)
    } finally {
      setLoading(false)
    }
  }, [logPath])

  return (
    <div className="rounded-md bg-zinc-950 text-zinc-100 font-mono text-xs p-3 overflow-x-auto">
      <div className="text-emerald-400 mb-1.5">$ {command}</div>
      {live.droppedHead && (
        <div className="text-amber-400/80 mb-1 flex items-center gap-2">
          <span>⋯ 早期输出已截断</span>
          {logPath && (
            <button
              type="button"
              onClick={handleLoadFull}
              disabled={loading}
              className="px-1.5 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-50"
            >
              {loading ? '加载中…' : '加载完整日志'}
            </button>
          )}
        </div>
      )}
      <pre ref={scrollRef} className="whitespace-pre-wrap break-all max-h-[320px] overflow-y-auto">
        {fullLog !== null
          ? fullLog
          : live.segments.map((seg, i) => (
              <span key={i} className={cn(seg.stream === 'stderr' && 'text-red-400')}>
                {seg.text}
              </span>
            ))}
      </pre>
    </div>
  )
}
```

- [ ] **Step 4: 运行确认绿**

Run: `cd ui && npm test -- --run BashStreamView 2>&1 | tail -8`
Expected: `2 passed`

- [ ] **Step 5: 接入 ToolActivityItem** — bash 运行中且有 `liveOutput` 时,在该行下方渲染 `BashStreamView`。在 `ToolActivityItem.tsx` 的 `ActivityRow`(第 191 行)return 处理中,把 `rowContent` 包裹后追加 live 视图。

最小改法:在 `ActivityRow` 函数体内、`return (` 之前计算:

```tsx
  const liveBash =
    activity.toolName === 'bash' && !activity.done && activity.liveOutput && activity.liveOutput.segments.length > 0
      ? activity.liveOutput
      : null
```

import `BashStreamView`(文件顶部):

```tsx
import { BashStreamView } from './tool-renderers/BashStreamView'
```

然后把 `ActivityRow` 最外层 return 的 `<div ...>{canExpand ? (...) : (...)}</div>` 改为在其后追加 live 视图。即把结尾:

```tsx
  return (
    <div className={cn(animate && '...')} style={...}>
      {canExpand ? (...) : (...)}
    </div>
  )
```

改为:

```tsx
  return (
    <div className={cn(animate && 'animate-in fade-in slide-in-from-left-2 duration-200 fill-mode-both')}
         style={animate ? { animationDelay: delay } : undefined}>
      {canExpand ? (
        <div role="button" tabIndex={0}
          className={cn('group/row w-full flex items-center gap-2 px-2.5 rounded-lg cursor-pointer transition-colors duration-100 hover:bg-muted/50', SIZE.row)}
          onClick={(e) => { e.stopPropagation(); onOpenDetails!(activity) }}
          onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.stopPropagation(); onOpenDetails!(activity) } }}>
          {rowContent}
        </div>
      ) : (
        <div className={cn('group/row flex items-center gap-2 px-2.5 rounded-lg', SIZE.row)}>
          {rowContent}
        </div>
      )}
      {liveBash && (
        <div className="mx-2 mt-1">
          <BashStreamView command={(activity.input.command as string) ?? ''} live={liveBash} logPath={undefined} />
        </div>
      )}
    </div>
  )
```

> `logPath` 流式期间为 `undefined`(temp 路径在 `done` 后从 result 头注得到);`droppedHead` 期间显示截断提示但按钮在 `done` 前不出现(`logPath` 为空)。这符合 spec:button 在结果落定后激活。

- [ ] **Step 6: 类型检查 + 全量前端测试**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -8 && npm test -- --run BashStreamView agent-atoms.live-output 2>&1 | tail -8`
Expected: tsc 无输出;测试全过

- [ ] **Step 7: 提交**

```bash
cd /Users/ryanliu/Documents/uclaw
git add ui/src/components/agent/tool-renderers/BashStreamView.tsx \
        ui/src/components/agent/tool-renderers/BashStreamView.test.tsx \
        ui/src/components/agent/ToolActivityItem.tsx
git commit -m "$(cat <<'EOF'
feat(ui): live bash streaming view in ToolActivityItem

BashStreamView renders liveOutput segments with stderr color-coding,
auto-scrolls to bottom while streaming, and offers a load-full-log button
(invokes read_bash_log) when the live window dropped its head. Wired into
ActivityRow: shown under a running bash row that has liveOutput.

Verification: npm test -- --run BashStreamView agent-atoms.live-output → passed; tsc clean
EOF
)"
```

---

## 最终验收

- [ ] 后端全量编译:`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → 无输出
- [ ] 后端相关单测:`cd src-tauri && cargo test --lib "tools::stream" "shell::tests" "tool::stream_default" "read_bash_log" 2>&1 | tail -15` → 全过
- [ ] 前端类型:`cd ui && npx tsc --noEmit 2>&1 | head` → 无输出
- [ ] 前端单测:`cd ui && npm test -- --run agent-atoms.live-output BashStreamView 2>&1 | tail -10` → 全过
- [ ] 手动 smoke(`cargo tauri dev`):Agent 模式执行 `for i in $(seq 1 200); do echo "line $i"; sleep 0.01; done; echo oops 1>&2`,观察:输出实时逐行滚动、stderr `oops` 红色、命令结束后展开行显示最终结果块。
- [ ] 手动 smoke 大输出:执行 `seq 1 100000`,观察实时窗口出现「早期输出已截断」、命令结束后「加载完整日志」可点开完整内容(读 `~/.uclaw/temp/bash-*.log`)。

---

## Self-Review(写计划后自查)

**Spec coverage:**
- §4.1 类型 → Task 1 ✓;§4.2 trait → Task 2 ✓;§4.3 读循环 → Task 4 ✓;§4.4 OverflowSink → Task 3 ✓;§4.5 dispatcher 节流 → Task 5 ✓;§4.6 read_bash_log → Task 6 ✓;§5.1 LiveOutput/listener → Task 7+8 ✓;§5.2-5.4 渲染/着色/滚动/加载/节流 → Task 9 ✓;§6 错误边界(channel 满计数 Task 1、UTF-8 lossy Task 5、daemon 不流式 Task 4、非 bash 零变化 Task 2、reload 走持久化 Task 7 注释)✓;§7 测试散落各 Task ✓。
- 前端 rAF 节流(§5.4):后端已 50ms 合并,前端按 `live.bytes` 触发 effect;若实测仍抖动可在 Task 9 加 rAF 批处理 —— 当前不预先实现(YAGNI),留待 smoke 观察。

**Placeholder scan:** 无 TBD/TODO;每个代码步均含完整代码;Step 2(emit_tool_output_chunk)已显式改为"跳过以避免 dead_code",非占位。

**Type consistency:** `LiveOutput` / `appendLiveOutput`(Task 7)→ listener(Task 8)→ `BashStreamView` props(Task 9)一致;`ToolStreamSink::channel/noop/send/dropped`(Task 1)→ trait(Task 2)→ shell(Task 4)→ dispatcher(Task 5)一致;`ToolStream::Stdout/Stderr` 全程一致;`read_bash_log`(Task 6)↔ `invoke('read_bash_log', { path })`(Task 9)参数名一致。
