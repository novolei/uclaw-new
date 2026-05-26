# Bash 工具完整流式输出管线 — 设计

> Pi 框架融合 Sprint 2。把 Bash 工具的「执行完一次性返回」升级为 **stdout/stderr 实时交错流式输出**,前端聊天页实时显示(类终端体验),完整日志按需从落盘文件加载。
>
> **状态**:已通过 brainstorming 评审,待转 writing-plans。
> **日期**:2026-05-26
> **前置**:Pi Sprint 1 Task 1(`RollingTailBuffer` + temp 落盘,已合并 `8b584c0b`)。

---

## 1. 背景与现状

调查结论(`shell.rs` / `dispatcher.rs` / 前端):**今天 bash 输出不流式**。

- **后端**:`BashTool::execute()` 用 `tokio::join!` 把 stdout/stderr `read_to_end()` 读到结束,塞进 `RollingTailBuffer`(32KB tail),返回**一个**最终 `ToolOutput`。执行期间无任何中间事件。
- **Tool trait**(`agent/tools/tool.rs:207-241`):`async fn execute(&self, params) -> Result<ToolOutput, ToolError>`,无 callback/channel/sender。**146 个实现 + MCP 适配器** 都实现它。
- **事件管线**:`ChatDelegate` 持 `app_handle`;LLM token 走 `chat:stream-chunk { conversationId, delta, seq }`;工具活动走 `chat:stream-tool-activity`(按 `toolCallId` 关联),仅 `tool_start` / `tool_result` 两个一次性事件。
- **前端**:`agent-atoms.ts` 按 `toolUseId` 合并 start/result;`ToolActivityItem.tsx` / `bash-result.tsx` 把完整 `result` 渲染成静态 `<pre>`。无流式、无死代码假象。
- **持久化**:工具输出存在 SDK message content 块里;实时事件是临时的,reload 走持久化结果。

**目标**:让 `npm install`、长测试、`tail -f` 等长命令的输出在聊天页实时滚动显示;不改变 agent 决策逻辑(LLM 仍在工具完成后拿最终结果)。

---

## 2. 关键决策(brainstorming 已定)

| 决策 | 选择 |
|---|---|
| 架构(流式如何接线) | **方案 A**:可选 trait 方法 + mpsc channel;工具产生领域事件,dispatcher 负责 Tauri emit + 节流 |
| 范围 | bash 全保真逐块实时;机制通用可选,当前仅 bash 接入 |
| 实时视图边界 | **有界窗口(~256KB)+「加载完整日志」按钮**(从 temp 文件按需读取) |
| stdout/stderr | **真实交错 + 颜色区分**(`tokio::select!` 同读两管道,chunk 带 `stream` 标记;最终结果也按真实执行顺序) |

被否方案:B(把 AppHandle 传进工具 → 耦合 Tauri,破坏纯 Rust 核心)、C(ToolRegistry 层通用包装 → 过度设计,trait 提取是 Sprint 3)。

---

## 3. 端到端数据流

```
bash 进程 stdout/stderr 两条管道
  └─ shell.rs execute_streaming():tokio::select! 同时分块读两管道(8KB read buffer)
       每读到一块 chunk:
         ① tail_buf.push_bytes(chunk)         → 32KB rolling tail(最终给 LLM,Task 1 不变)
         ② OverflowSink 增量 append 到 temp     → 完整日志落盘(>32KB 才开文件)
         ③ sink.send(stream, chunk)           → 推入 mpsc channel(领域事件,不碰 Tauri)
  └─ dispatcher drain 任务:合并节流(~50ms 或 8KB 先到先发,保持 seq + stream 分组)
       emit chat:stream-tool-activity { type:"tool_output_chunk", toolCallId, seq, stream, chunk }
  └─ 前端 atoms listener:append 到 ToolActivity.liveOutput(256KB 有界环,按 stream 着色)
  └─ tool_result(现有事件):标记 done,最终结果块为准
  └─「加载完整日志」(仅当丢过头部):invoke read_bash_log(path) → 读 temp 文件
```

交错天然产生:`tokio::select!` 谁就绪先读谁,tail_buf / temp / sink 拿到的都是真实到达顺序。

---

## 4. 后端设计

### 4.1 核心类型(新建 `src-tauri/src/agent/tools/stream.rs`)

```rust
#[derive(Debug, Clone, Copy, Serialize)]
pub enum ToolStream { Stdout, Stderr }

#[derive(Debug, Clone)]
pub struct ToolStreamEvent {
    pub seq: u64,            // 单调序号(跨两管道全局递增,保证前端排序)
    pub stream: ToolStream,
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
pub struct ToolStreamSink {
    tx: Option<tokio::sync::mpsc::Sender<ToolStreamEvent>>,  // None = no-op
    seq: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,   // try_send 失败计数(尽力而为)
}

impl ToolStreamSink {
    pub fn noop() -> Self;                                       // 默认 trait 用
    pub fn channel(cap: usize) -> (Self, mpsc::Receiver<ToolStreamEvent>);  // dispatcher 用
    pub fn send(&self, stream: ToolStream, bytes: &[u8]);        // try_send;满了丢弃 + 计数
}
```

### 4.2 Tool trait 改动(附加,非破坏)

```rust
#[async_trait]
pub trait Tool {
    // ... 现有 name/description/parameters_schema/execute 全不动 ...

    /// 流式变体。默认忽略 sink、直接委托 execute()。
    /// 只有增量输出的工具(BashTool)override 它。
    async fn execute_streaming(
        &self,
        params: Value,
        _sink: ToolStreamSink,
    ) -> Result<ToolOutput, ToolError> {
        self.execute(params).await
    }
}
```

→ 146 个实现 + MCP 适配器零改动。`BashTool::execute()` 反过来调 `self.execute_streaming(params, ToolStreamSink::noop())`,统一成一条读取路径。

### 4.3 shell.rs:`execute_streaming` 读循环

替换现有 `tokio::join!`(`shell.rs:579-614`):

```rust
let mut stdout = child.stdout.take()...;  let mut stderr = child.stderr.take()...;
let mut buf_out = [0u8; 8192];  let mut buf_err = [0u8; 8192];
let mut tail_buf = RollingTailBuffer::new(CONTEXT_LIMIT);
let mut overflow = OverflowSink::new(self.resolve_temp_dir());
let (mut out_open, mut err_open) = (true, true);

loop {
    tokio::select! {
        r = stdout.read(&mut buf_out), if out_open => match r {
            Ok(0) => out_open = false,
            Ok(n) => on_chunk(ToolStream::Stdout, &buf_out[..n], &mut tail_buf, &mut overflow, &sink),
            Err(_) => out_open = false,
        },
        r = stderr.read(&mut buf_err), if err_open => match r {
            Ok(0) => err_open = false,
            Ok(n) => on_chunk(ToolStream::Stderr, &buf_err[..n], &mut tail_buf, &mut overflow, &sink),
            Err(_) => err_open = false,
        },
        else => break,
    }
}
let status = child.wait().await?;
overflow.finish();
let combined = tail_buf.to_truncated_string(overflow.path());
```

`on_chunk` 对每块依次:`tail_buf.push_bytes` → `overflow.write` → `sink.send(stream, chunk)`。

**daemon 模式**(`spawn_daemon`,`shell.rs:376-439`)独立路径,不流式,立即返回 PID,不变。

### 4.4 temp 文件增量落盘(`OverflowSink`,扩展 Task 1)

```rust
struct OverflowSink { dir: Option<PathBuf>, file: Option<File>, path: Option<PathBuf>, total: usize }

impl OverflowSink {
    fn write(&mut self, tail_buf: &RollingTailBuffer, chunk: &[u8]) {
        self.total += chunk.len();
        if self.file.is_none() && self.total > CONTEXT_LIMIT {
            // 首次越过 32KB:开文件,先把 tail_buf 当前内容(此刻还没丢)刷进去
            self.open_and_flush(tail_buf.as_bytes());
        } else if let Some(f) = &mut self.file {
            let _ = f.write_all(chunk);   // 之后每块直接 append
        }
    }
    fn finish(&mut self);     // flush + drop file
    fn path(&self) -> Option<&Path>;
}
```

→ <32KB 不开文件(零额外 I/O);>32KB 全量落盘。写失败 `tracing::warn!`(保留 Task 1 行为)。

### 4.5 dispatcher 节流 drain 任务(仅串行路径)

bash 不是 parallel-safe(不在 Task 4 并行批次),流式只需接串行路径一处:

```rust
let (sink, mut rx) = ToolStreamSink::channel(256);
let app = self.app_handle.clone();  let id = tc.id.clone();
let coalescer = tokio::spawn(async move {
    let mut pending: Vec<ToolStreamEvent> = vec![];  let mut pending_bytes = 0usize;
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Some(e) => { pending_bytes += e.bytes.len(); pending.push(e);
                             if pending_bytes >= 8192 { flush(&app, &id, &mut pending, &mut pending_bytes); } }
                None => { flush(&app, &id, &mut pending, &mut pending_bytes); break; }
            },
            _ = tick.tick() => flush(&app, &id, &mut pending, &mut pending_bytes),
        }
    }
});
let output = tool.execute_streaming(params, sink).await;  // sink drop → channel 关
coalescer.await.ok();
self.emit_tool_result(&name, &id, &output);               // 现有收尾事件不变
```

`flush` 把累积事件按 stream 合并成连续段,`from_utf8_lossy`,emit
`chat:stream-tool-activity { conversationId, activity: { type:"tool_output_chunk", toolCallId, seq, stream, chunk } }`。

### 4.6 `read_bash_log` Tauri 命令(新增)

```rust
#[tauri::command]
async fn read_bash_log(path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    let temp = uclaw_home_pathbuf().map_err(|e| e.to_string())?.join("temp");
    if !p.starts_with(&temp) { return Err("path outside temp dir".into()); }  // 安全:限 temp 内
    read_capped(&p, 5 * 1024 * 1024)   // 5MB 上限,超出加自己的截断头
}
```

在 `main.rs` 的 `invoke_handler!` 注册(漏注册编译过但运行时失败)。

---

## 5. 前端设计

### 5.1 监听新事件(`agent-atoms.ts:565-601` 加分支)

```ts
interface ToolActivity {
  // ... 现有 ...
  liveOutput?: LiveOutput;
}
interface LiveOutput {
  segments: { stream: 'stdout' | 'stderr'; text: string }[];  // 连续同 stream 合并成段
  bytes: number;
  droppedHead: boolean;     // 丢过头部 → 显示「加载完整日志」
  logPath?: string;         // 从 tool_result 截断头注解析出的 temp 路径
}
```

`type === "tool_output_chunk"` → 按 `toolUseId` 找 `ToolActivity` → append 到 `liveOutput`;`bytes > 256KB` 时从头部 segments 删除并置 `droppedHead = true`。

### 5.2 渲染 + 着色 + 自动滚动

bash 结果组件(`bash-result.tsx` / `ToolActivityItem.tsx`):流式期间渲染 `liveOutput.segments`,stderr 用 `text-destructive`(主题 token,不硬编码),stdout 默认色;`<pre>` 在用户未手动上滚时自动贴底;`done` 后切换到最终结果块(持久化 SDK content 为准)。

### 5.3「加载完整日志」

`droppedHead && logPath` 时显示 `⋯ 早期输出已截断 — [加载完整日志]`,点击 `invoke('read_bash_log', { path })` 把完整内容填进展开区。

### 5.4 前端节流渲染

后端已 ~50ms 合并;前端再用 `requestAnimationFrame` 批量 flush append,避免每事件一次 re-render。

### 5.5 双 composer

`ToolActivityList` 共享,渲染改动一处。CLAUDE.md 双 composer 规则针对输入框(paste/drop/submit),不涉及活动列表;实现时仍验证 bash 渲染组件在 Chat + Agent 两侧走同一个。

---

## 6. 错误处理 / 边界

| 情况 | 处理 |
|---|---|
| channel 满(生产快于消费) | `try_send` 丢弃 + 计数;实时视图尽力而为,**最终结果 + temp 文件为权威** |
| UTF-8 跨 chunk 边界 | coalescer 每 stream 累积后 `from_utf8_lossy`;边界偶现替换符,可接受(可选优化:暂留尾部不完整字节) |
| daemon 模式 | 独立路径,不流式,不变 |
| 中断/取消 | 子进程被 kill;已 append 的 liveOutput 留显;结果为已缓冲部分 |
| 非 bash 工具 | 默认 trait 方法 = 不流式,零行为变化 |
| reload 历史会话 | 无 liveOutput(实时事件临时);显示持久化最终结果块 +(若截断)加载按钮 |

---

## 7. 测试

**Rust 单元**
- `ToolStreamSink`:send 后 seq 单调递增;channel 满时 `try_send` 丢弃 + `dropped` 计数。
- `OverflowSink`:<32KB 不开文件;>32KB 首次越界 flush tail_buf 当前内容 + 后续 append;写失败 warn。
- `execute_streaming`:用同时写 stdout+stderr 的命令(如 `sh -c 'echo out; echo err >&2; echo out2'`),收集 sink 事件,断言**交错顺序合理 + seq 单调 + stream 标记正确**。
- coalescer:喂事件,断言时间(50ms)/大小(8KB)flush 边界。
- `read_bash_log`:路径在 temp 外被拒;temp 内正常返回;>5MB 截断。

**前端 vitest**
- listener:`tool_output_chunk` append 到 liveOutput;有界环丢头部置 `droppedHead`;连续同 stream 合并段。
- 渲染:流式期间显示 segments、stderr 着色、自动贴底;`done` 后切最终结果。
- 加载按钮:`droppedHead` 时出现,点击 invoke `read_bash_log`。

---

## 8. ADR §18 子集(运行时变更,涉及 Tool trait)

- **Intent**:bash 长命令实时可见,提升 agent 可观测性;不改 agent 决策逻辑。
- **Truth source**:最终结果(32KB tail)+ temp 文件为权威;实时流是 UI affordance,可丢。
- **Capability**:新增 `read_bash_log` 命令(限 `~/.uclaw/temp/`)。
- **Harness/测试**:见 §7。
- **Rollback**:trait 方法附加、默认 no-op;`git revert` 即回滚,无 schema 变更。
- **不拥有**:不改 LLM 流式(`chat:stream-chunk` 不变);不改 approval/并行批次;不动 daemon 模式;不做通用工具流式框架(Sprint 3)。

---

## 9. 范围边界(YAGNI)

✅ 做:bash stdout/stderr 实时交错流、有界实时窗口、temp 增量落盘、加载完整日志、着色。
❌ 不做:其他工具流式(默认 no-op 占位)、ANSI 转义/终端模拟、实时搜索过滤、把流式喂给 LLM。

---

## 10. 文件清单(实现时)

| 文件 | 改动 |
|---|---|
| `src-tauri/src/agent/tools/stream.rs` | **新建** — `ToolStream` / `ToolStreamEvent` / `ToolStreamSink` |
| `src-tauri/src/agent/tools/mod.rs` | `pub mod stream;` |
| `src-tauri/src/agent/tools/tool.rs` | Tool trait 加默认 `execute_streaming` |
| `src-tauri/src/agent/tools/builtin/shell.rs` | `execute_streaming` select 读循环;`OverflowSink` 增量;`execute` 委托 |
| `src-tauri/src/agent/dispatcher.rs` | 串行路径节流 drain 任务 + emit `tool_output_chunk` |
| `src-tauri/src/tauri_commands.rs` | `read_bash_log` 命令 |
| `src-tauri/src/main.rs` | `invoke_handler!` 注册 `read_bash_log` |
| `ui/src/atoms/agent-atoms.ts` | listener 加 `tool_output_chunk` 分支 + `LiveOutput` |
| `ui/src/components/agent/ToolActivityItem.tsx` / `ui/src/components/chat/bash-result.tsx` | 流式渲染 + 着色 + 自动滚动 + 加载按钮 |
