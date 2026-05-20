# 《uclaw 代码库升级改造实施方案》

> **版本：v2.0（ADR Agent OS v2 北极星对齐版）**
> 日期：2026-05-20
> 配套：《uclaw-codex 对比分析与架构改进设计文档》v2.0
> 北极星：`docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`

---

## v2.0 关键变更说明

1. **Phase 重映射到 ADR M0-M9**：v1.0-v1.2 的 Phase 0-5 全部归入 ADR Milestone 体系
2. **License 决策落地**：Apache-2.0（首章具体执行步骤）
3. **承认 uclaw 现状**：harness/learning/browser v2/channels/extensions/symphony_graph 已就位，工作量重估
4. **总时长缩短**：M1-M8（核心）**8-11 月**（中位 9.5）；M1-M9（含远期）**13-15 月**（中位 14）；3 人团队，远小于 v1.1 估的 15-18 月
5. **新增任务**：HookBus 13 events / Capability Cards / WorldProjection / Evolution Factory pipeline / Isolation profiles

---

## 目录

1. [总览与里程碑](#1-总览与里程碑)
2. [Milestone M0：ADR Lock + License + Workspace（已完成 + Phase 0.5）](#2-m0)
3. [Milestone M1：Runtime Contracts（2-3 周）](#3-m1)
4. [Milestone M2：Context Fabric（5-7 周）](#4-m2)
5. [Milestone M3：Capability Mesh（6-8 周）](#5-m3)
6. [Milestone M4：World Projection（3-4 周）](#6-m4)
7. [Milestone M5：Policy Hooks + Isolation（4-5 周）](#7-m5)
8. [Milestone M6：Browser Provider 抽象（3-4 周）](#8-m6)
9. [Milestone M7：Evolution Factory（6-8 周）](#9-m7)
10. [Milestone M8：Teams v1（5-7 周）](#10-m8)
11. [Milestone M9：Cluster v1（12-16 周，远期）](#11-m9)
12. [依赖关系图与时间线](#12-依赖与时间线)
13. [测试与验证策略](#13-测试)
14. [回滚与紧急预案](#14-回滚)
15. [资源与时间估算](#15-资源)
16. [PR 流程与 commit 粒度](#16-pr-流程)
17. [Crate 复制操作（Phase 0.5）](#17-crate-复制)
18. [附录 A：迁移注册表](#附录-a迁移注册表)
19. [附录 B：测试矩阵](#附录-b测试矩阵)

---

## 1. 总览与里程碑 <a id="1-总览与里程碑"></a>

```
M0 ADR Lock (已完成) + Phase 0.5 Workspace + License + Crate 复制 (3-5 天)
   │
M1 Runtime Contracts (2-3 周)
   │
M2 Context Fabric (5-7 周)
   ├── M2-A Prompt 12 block 重写
   ├── M2-B UCLAW.md 项目级指令
   ├── M2-C 30+ ContextFragment
   ├── M2-D Diff-based re-injection
   ├── M2-E Template 引擎
   ├── M2-F 7 Context Tools
   ├── M2-G 8 字段 Structured Fold
   ├── M2-H 7 层 Token 防线
   ├── M2-I Prompt caching 优化
   └── M2-J Token budget UI
   │
M3 Capability Mesh (6-8 周)
   │
M4 World Projection (3-4 周)  ← 可与 M3 并行
   │
M5 Policy Hooks + Isolation (4-5 周)  ← 可与 M4 并行
   │
M6 Browser Provider (3-4 周)  ← 可与 M5 并行
   │
M7 Evolution Factory (6-8 周)
   │
M8 Teams v1 (5-7 周)
   │
M9 Cluster v1 (12-16 周, 远期)
```

**总时长**（**单一权威值**，与对比文档 §2.4 同步）：
- **M1-M8（核心）**：约 **34-46 周 = 8-11 月（3 人团队，中位 ~9.5 月）**
- **M1-M9（含 cluster 远期）**：约 **46-62 周 = 13-15 月（3 人团队，中位 ~14 月）**

### 1.1 关键里程碑

| 序 | 里程碑 | 大致月份 | 主要交付 |
|---|---|---|---|
| MS1 | Phase 0.5 完成 | M0.2 | License + workspace + 17 个 codex utils crate |
| MS2 | M1 完成 | M1 | IntentSpec/TaskSpec/TaskEvent + adapters |
| MS3 | M2 完成 | M3 | Context Fabric 完整 + Token 节约 60-75% |
| MS4 | M3 完成 | M4.5 | 5 Registry + Capability Cards + Plugin |
| MS5 | M4 完成 | M5 | WorldProjection materialized view |
| MS6 | M5 完成 | M6 | HookBus 13 events + Isolation profiles |
| MS7 | M6 完成 | M7 | BrowserProvider trait + 多 provider plugin |
| MS8 | M7 完成 | M8.5 | Evolution Factory pipeline + User Review UI |
| MS9 | M8 完成 | M10 | Teams v1 with ReviewGate |
| MS10 | M9 完成 | M13+ | Cluster v1 with WorkerNode |

### 1.2 总体优先级（按用户感知 + ADR）

**P0（必须，立即）**：
1. **License → Apache-2.0** + Workspace 改造 + 复制 17+1 个 codex utils crate（Phase 0.5）
2. **M2-H L1 TruncationPolicy** —— 立竿见影 token 节约
3. **M2-H L2 ToolExposure** —— 立竿见影
4. **M2-A baseline.md 12 block 重写** —— 输出质量肉眼提升

**P1（高优）**：
5. **M1 Runtime Contracts** —— 全部后续 milestone 基础
6. **M2-D + M2-H L7 三档 compaction** —— 解决"context 爆掉"+ 长会话节约
7. **M3 五大 Registry** —— Capability Mesh 基座
8. **M5 HookBus 13 events** —— 安全可见性

**P2（中优）**：
9. **M4 WorldProjection** —— UI 体验飞跃
10. **M6 BrowserProvider** trait —— 现有 v2 抽象化
11. **M7 Evolution Factory** —— 现有 learning/harness/proactive 收纳
12. **M8 Teams v1** —— agent/teams + channels 升级

**P3（远期）**：
13. **M9 Cluster v1**

---

## 2. Milestone M0：ADR Lock + License + Workspace <a id="2-m0"></a>

### 2.1 M0 状态

| 任务 | 状态 | 备注 |
|---|---|---|
| ADR `2026-05-20-uclaw-agent-platform-north-star.md` accepted | ✅ 已完成 | CLAUDE.md L73 已引用 |
| Sibling ADR `gbrain-primary-freeze-l2-cognitive.md` accepted | ✅ 已完成 | |
| License 决策（Apache-2.0） | ⏳ 本方案推荐 | Phase 0.5-T1 执行 |
| Workspace 改造 | ⏳ | Phase 0.5-T2 执行 |
| 17+1 codex utils crate 复制 | ⏳ | Phase 0.5-T3 ~ T4 执行 |

### 2.2 Phase 0.5：License + Workspace + Crate 复制（3-5 天）

#### Phase 0.5-T1：License 落地（0.5 天）

**Commit 1 — 添加 LICENSE 文件**

```bash
cp /Users/ryanliu/Documents/Hero/codex/LICENSE \
   /Users/ryanliu/Documents/uclaw/LICENSE
# 编辑：把 OpenAI copyright 改为 uclaw 团队（保留 Apache-2.0 全文）
```

**Commit 2 — 添加 NOTICE + licenses/**

```bash
mkdir -p /Users/ryanliu/Documents/uclaw/licenses
cp /Users/ryanliu/Documents/Hero/codex/LICENSE \
   /Users/ryanliu/Documents/uclaw/licenses/apache-2.0.txt

cat > /Users/ryanliu/Documents/uclaw/NOTICE <<'EOF'
uclaw

Copyright (c) 2026 uclaw contributors.
Licensed under the Apache License, Version 2.0.

This product includes software developed at OpenAI (https://openai.com/)
under the Apache License, Version 2.0.

The following crates are derived (with or without modifications) from
the openai/codex repository (https://github.com/openai/codex):

  - uclaw-utils-template       (from codex-rs/utils/template)
  - uclaw-utils-string         (from codex-rs/utils/string)
  - uclaw-utils-cache          (from codex-rs/utils/cache)
  - uclaw-utils-fuzzy          (from codex-rs/utils/fuzzy-match)
  - uclaw-async-utils          (from codex-rs/async-utils)
  - uclaw-file-watcher         (from codex-rs/file-watcher)
  - uclaw-utils-output-truncation (from codex-rs/utils/output-truncation; modified)
  # 后续添加更多

Upstream codex commit: <填入今日 commit hash>
The original codex source is licensed under Apache License 2.0.
See licenses/apache-2.0.txt for the full license text.
EOF
```

**Commit 3 — Cargo.toml license 字段**

`src-tauri/Cargo.toml` 加：
```toml
[package]
license = "Apache-2.0"
```

**Commit 4 — docs/THIRD_PARTY.md**

新建 `docs/THIRD_PARTY.md` 写明衍生流程：每个外部代码引入须更新 NOTICE + 加 SPDX header + 文件头注明 "Derived from ..."。

**Commit 5 — CI lint**

每个 `src-tauri/uclaw-*` 子 crate 顶部必须含 `// SPDX-License-Identifier: Apache-2.0`。

**DoD**：
- [ ] LICENSE + NOTICE + licenses/apache-2.0.txt 三个文件就位
- [ ] Cargo.toml license 字段
- [ ] docs/THIRD_PARTY.md 规则文档
- [ ] CI lint 工作

#### Phase 0.5-T2：Workspace 改造（0.5 天）

**新建** `/Users/ryanliu/Documents/uclaw/Cargo.toml`（之前**无此文件**，必须新建）：

```toml
[workspace]
resolver = "2"
members = [
    "src-tauri",
    # 第一批最高 ROI（T3 完成后填入）
    "src-tauri/uclaw-utils-template",
    "src-tauri/uclaw-utils-string",
    "src-tauri/uclaw-utils-cache",
    "src-tauri/uclaw-utils-fuzzy",
    "src-tauri/uclaw-async-utils",
    "src-tauri/uclaw-file-watcher",
    "src-tauri/uclaw-utils-output-truncation",
    # 后续按需添加（T5 第二批 11 个）
]

[workspace.package]
edition = "2024"
license = "Apache-2.0"
authors = ["uclaw contributors"]
repository = "https://github.com/<owner>/uclaw"

[workspace.dependencies]
# 第三方依赖统一管理（避免 src-tauri 与子 crate 重复指定）
tokio = { version = "1", features = ["full"] }
tokio-util = "0.7"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
regex-lite = "0.1"
lru = "0.12"
sha1 = "0.10"
notify = "6"
async-trait = "0.1"
thiserror = "1"
time = "0.3"
ignore = "0.4"
nucleo = "0.5"
crossbeam-channel = "0.5"
clap = { version = "4", features = ["derive"] }
anyhow = "1"
dirs = "5"
dunce = "1"
schemars = "0.8"
ts-rs = { version = "10", features = ["serde-json-impl", "no-serde-warnings"] }
tempfile = "3"
image = { version = "0.25", features = ["jpeg", "png", "gif", "webp"] }
base64 = "0.22"
mime_guess = "2"
portable-pty = "0.8"
similar = "2"
walkdir = "2"
chrono = "0.4"
tracing = "0.1"
pretty_assertions = "1"

[workspace.lints.rust]
unsafe_code = "warn"

[workspace.lints.clippy]
# 按团队偏好
```

**修改** `src-tauri/Cargo.toml`：

```diff
- [package]
- name = "uclaw"
- version = "0.1.0"
- edition = "2024"
+ [package]
+ name = "uclaw"
+ version = "0.1.0"
+ edition.workspace = true
+ license.workspace = true
```

**验证**：

```bash
cd /Users/ryanliu/Documents/uclaw
cargo build 2>&1 | tee /tmp/build.log | tail -30
# 应无错误
```

**DoD**：
- [ ] 顶层 Cargo.toml 就绪
- [ ] `cargo build` 通过
- [ ] CI 通过（如 CI 用 `cd src-tauri && cargo build`，改为根目录）

#### Phase 0.5-T3：第一批 6 个 crate 复制（0.5 天）

```bash
cd /Users/ryanliu/Documents/uclaw

# 1. 复制源码
cp -r /Users/ryanliu/Documents/Hero/codex/codex-rs/utils/template     src-tauri/uclaw-utils-template
cp -r /Users/ryanliu/Documents/Hero/codex/codex-rs/utils/string       src-tauri/uclaw-utils-string
cp -r /Users/ryanliu/Documents/Hero/codex/codex-rs/utils/cache        src-tauri/uclaw-utils-cache
cp -r /Users/ryanliu/Documents/Hero/codex/codex-rs/utils/fuzzy-match  src-tauri/uclaw-utils-fuzzy
cp -r /Users/ryanliu/Documents/Hero/codex/codex-rs/async-utils        src-tauri/uclaw-async-utils
cp -r /Users/ryanliu/Documents/Hero/codex/codex-rs/file-watcher       src-tauri/uclaw-file-watcher

# 2. sed 改名 codex-* → uclaw-*
for d in src-tauri/uclaw-utils-template src-tauri/uclaw-utils-string \
         src-tauri/uclaw-utils-cache src-tauri/uclaw-utils-fuzzy \
         src-tauri/uclaw-async-utils src-tauri/uclaw-file-watcher; do
  name=$(basename "$d")
  sed -i '' "s/^name = \"codex-[^\"]*\"/name = \"$name\"/" "$d/Cargo.toml"
done

# 3. 每个 crate src/lib.rs 顶部加 SPDX header
# 示例（每个文件手工 + git diff 校验）：
# // SPDX-License-Identifier: Apache-2.0
# // Derived from codex-rs/utils/<name> (https://github.com/openai/codex).
# // Copyright (c) OpenAI. Licensed under Apache License 2.0.
# // See NOTICE in the repository root.

# 4. src-tauri/Cargo.toml 引用
# [dependencies]
# uclaw-utils-template = { path = "./uclaw-utils-template" }
# uclaw-utils-string   = { path = "./uclaw-utils-string" }
# uclaw-utils-cache    = { path = "./uclaw-utils-cache" }
# uclaw-utils-fuzzy    = { path = "./uclaw-utils-fuzzy" }
# uclaw-async-utils    = { path = "./uclaw-async-utils" }
# uclaw-file-watcher   = { path = "./uclaw-file-watcher" }

# 5. 验证
cargo build
cargo test -p uclaw-utils-template
cargo test -p uclaw-utils-string
cargo test -p uclaw-utils-cache
cargo test -p uclaw-utils-fuzzy
cargo test -p uclaw-async-utils
cargo test -p uclaw-file-watcher
```

**DoD**：
- [ ] 6 个 crate 编译通过
- [ ] 6 个 crate 测试通过（codex 原带测试）
- [ ] src-tauri 仍可编译
- [ ] NOTICE 列出 6 个 crate

#### Phase 0.5-T4：output-truncation 微改动复制（0.5 天）

```bash
cd /Users/ryanliu/Documents/uclaw

# 1. 复制
cp -r /Users/ryanliu/Documents/Hero/codex/codex-rs/utils/output-truncation \
      src-tauri/uclaw-utils-output-truncation
```

**新建** `src-tauri/uclaw-utils-output-truncation/src/types.rs`（从 codex protocol/src/protocol.rs 摘出 + 改命名空间）：

```rust
// SPDX-License-Identifier: Apache-2.0
// Types extracted from codex_protocol/protocol.rs and models.rs.
// Modified for uclaw to remove codex_protocol dependency.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TruncationPolicy {
    Bytes(usize),
    Tokens(usize),
}

impl TruncationPolicy {
    pub fn byte_budget(&self) -> usize {
        match self {
            Self::Bytes(b) => *b,
            Self::Tokens(t) => *t * 4, // approx 4 bytes/token
        }
    }
    pub fn token_budget(&self) -> usize {
        match self {
            Self::Bytes(b) => *b / 4,
            Self::Tokens(t) => *t,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FunctionCallOutputContentItem {
    InputText { text: String },
    InputImage { image_url: String, detail: ImageDetail },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageDetail {
    Auto,
    Low,
    High,
}
```

**修改** `src-tauri/uclaw-utils-output-truncation/src/lib.rs` 顶部 imports：

```diff
- use codex_protocol::models::FunctionCallOutputContentItem;
- pub use codex_protocol::protocol::TruncationPolicy;
- pub use codex_utils_string::approx_bytes_for_tokens;
- pub use codex_utils_string::approx_token_count;
- pub use codex_utils_string::approx_tokens_from_byte_count;
- use codex_utils_string::truncate_middle_chars;
- use codex_utils_string::truncate_middle_with_token_budget;
+ mod types;
+ pub use types::{FunctionCallOutputContentItem, ImageDetail, TruncationPolicy};
+ pub use uclaw_utils_string::approx_bytes_for_tokens;
+ pub use uclaw_utils_string::approx_token_count;
+ pub use uclaw_utils_string::approx_tokens_from_byte_count;
+ use uclaw_utils_string::truncate_middle_chars;
+ use uclaw_utils_string::truncate_middle_with_token_budget;
```

**修改 Cargo.toml**：

```diff
- [package]
- name = "codex-utils-output-truncation"
+ [package]
+ name = "uclaw-utils-output-truncation"
  edition.workspace = true
  license.workspace = true

  [dependencies]
- codex-protocol = { workspace = true }
- codex-utils-string = { workspace = true }
+ uclaw-utils-string = { path = "../uclaw-utils-string" }
+ serde = { workspace = true, features = ["derive"] }
```

**workspace.members + src-tauri 引用**：

```toml
# 顶层 Cargo.toml
members = [
    ...
    "src-tauri/uclaw-utils-output-truncation",
]

# src-tauri/Cargo.toml
uclaw-utils-output-truncation = { path = "./uclaw-utils-output-truncation" }
```

**验证**：

```bash
cargo build
cargo test -p uclaw-utils-output-truncation
```

**DoD**：
- [ ] output-truncation 编译 + 测试通过
- [ ] NOTICE 已加这条
- [ ] 文件头注明 "modified from codex-rs/utils/output-truncation"

#### Phase 0.5-T5：第二批 11 个 crate（可选，0.5-1 天）

复制顺序（注意依赖）：

1. `utils/absolute-path` → `uclaw-utils-abs-path`（无依赖）
2. `utils/home-dir` → `uclaw-utils-home`（依赖 #13 abs-path）
3. `utils/path-utils` → `uclaw-utils-path`（依赖 abs-path）
4. `utils/elapsed` → `uclaw-utils-elapsed`（无依赖）
5. `utils/readiness` → `uclaw-utils-readiness`（无依赖）
6. `utils/sleep-inhibitor` → `uclaw-utils-sleep`（平台 deps）
7. `utils/stream-parser` → `uclaw-utils-stream`（无依赖）
8. `utils/image` → `uclaw-utils-image`（依赖 cache）
9. `utils/json-to-toml` → `uclaw-utils-json-toml`（无 codex deps）
10. `file-search` → `uclaw-file-search`（无 codex deps）
11. `utils/pty` → `uclaw-utils-pty`（无 codex deps）

每个完全同 Phase 0.5-T3 的 5 步流程。

**DoD**：
- [ ] 11 个 crate 编译 + 测试通过
- [ ] NOTICE 完整
- [ ] CI 通过

### 2.3 Phase 0.5 完成定义

- [ ] License 落地（Apache-2.0 + NOTICE + licenses/）
- [ ] Cargo workspace 改造
- [ ] 第一批 6 个 codex utils crate 复制
- [ ] output-truncation 微改动复制
- [ ] CI lint 工作

**完成 T1-T4 即可启动 M1**。T5 第二批可在 M1 期间并行。

---

## 3. Milestone M1：Runtime Contracts（2-3 周）<a id="3-m1"></a>

### 3.1 ADR §16 M1 规约

**交付**：Rust 类型 `IntentSpec`、`TaskSpec`、`TaskEvent`、`PolicySpec`、`BudgetSpec`、`CapabilityProfile`、`CheckpointRef` + 适配器把 agent/browser/automation 现有事件转 TaskEvent + harness 摄取统一事件流。

**Exit criteria**：one chat task + one browser task + one automation run 产生 comparable traces。

### 3.2 任务清单

#### M1-T1：定义 Rust 类型（1 周）

**Commit 1 — 新模块 `src-tauri/src/runtime/contracts.rs`**

```rust
// IntentSpec、TaskSpec、TaskEvent、PolicySpec、BudgetSpec、
// CapabilityProfile、CheckpointPolicy、AutonomyLevel、RiskClass、
// IntentOrigin、Constraint、ContextRef、CapabilityQuery、ArtifactRef、
// HookDecision、BoundaryRef、CheckpointRef、WorkerId、TaskVerdict
// 全部按 ADR §7 schema 定义

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentSpec {
    pub id: IntentId,
    pub origin: IntentOrigin,
    pub user_goal: String,
    pub acceptance_criteria: Vec<String>,
    pub constraints: Vec<Constraint>,
    pub autonomy_target: AutonomyLevel,
    pub risk_class: RiskClass,
    pub context_refs: Vec<ContextRef>,
    pub requested_capabilities: Vec<CapabilityQuery>,
    pub created_at_ms: i64,
}

// ... 其余 17 类型
```

**Commit 2 — TaskEvent enum 13 variants**（对位 ADR §7.3）

**Commit 3 — Unit tests 全覆盖（每类型 ≥3 happy + 3 error）**

#### M1-T2：SessionTask trait + 抢占式调度（1 周）

**Commit 1 — 新 `src-tauri/src/agent/task.rs`**

```rust
pub trait SessionTask: Send + Sync + 'static {
    fn kind(&self) -> TaskKind;
    fn span_name(&self) -> &'static str;
    async fn run(
        self: Arc<Self>,
        session: Arc<SessionContext>,
        ctx: Arc<TurnContext>,
        input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String>;
    async fn abort(&self, session: Arc<SessionContext>, ctx: Arc<TurnContext>) {}
}

pub enum TaskKind { Regular, Compact, Review, UserShell, BrowserTask, AutomationRun, TeamWork }
```

**Commit 2 — Session 加 spawn_task / abort_all_tasks**

```rust
impl Session {
    pub async fn spawn_task<T: SessionTask>(self: &Arc<Self>, ctx, input, task) {
        self.abort_all_tasks(TurnAbortReason::Replaced).await;  // 抢占
        let token = CancellationToken::new();
        let task = Arc::new(task);
        let handle = tokio::spawn(/* task.run + on_task_finished */);
        self.active_turn.lock().await.replace(ActiveTurn { handle, task_kind: task.kind(), token, ... });
    }
    pub async fn abort_all_tasks(self, reason: TurnAbortReason) {
        // 100ms 优雅退出窗口
    }
}
```

**Commit 3 — 把 `agentic_loop::run_agentic_loop` 包成 RegularTask**

保留现有 6 阶段逻辑，仅外壳改为 trait 实现。所有现有调用点改为 `Session::spawn_task(RegularTask)`。

**Commit 4 — e2e 测试**：抢占性测试 3 个 + 优雅退出 + 资源清理。

#### M1-T3：HarnessEvent 升格为通用 TaskEvent（0.5 周）

**Commit 1 — `harness/trace.rs::HarnessEvent` 重命名为 `TaskEvent`（在 runtime 模块）**

`harness/` 仍可使用，但作为 `pub use` 而非定义。

**Commit 2 — `harness/case.rs::HarnessSubject` 12 个 subject 作为 TaskEvent 的 `source` 字段维度**

#### M1-T4：多域适配器（0.5-1 周）

**Commit 1 — Agent loop adapter** —— `agent/agentic_loop` 产出的工具调用 / LLM 响应 / 审批转 TaskEvent

**Commit 2 — Browser adapter** —— `browser/agent_loop.rs` 产出的 browser 步骤转 TaskEvent（含 boundary events）

**Commit 3 — Automation adapter** —— `automation/runtime/` 产出的 activity status 转 TaskEvent

**Commit 4 — 统一 rollout 写入**

#### M1-T5：Rollout JSONL（0.5 周）

**Commit 1 — 新模块 `src-tauri/src/agent/rollout/recorder.rs`**

借鉴 codex `rollout/src/recorder.rs`。tokio task + Sender<RolloutCmd> + 串行化写盘。每 TaskEvent 实时 append 到 `~/.uclaw/sessions/rollout-<TS>-<UUID>.jsonl`。

**Commit 2 — V44 迁移**：`task_events_rollout(kind, payload_json, task_id, thread_id, ts, ...)` 作为 SQLite 索引（编号以提交时为准）。

**Commit 3 — `replayer.rs`** CLI / 维护命令，扫描 JSONL → 重建 SQLite。

#### M1-T6：6 维 token + Turn span（0.5 周）

**Commit 1 — TokenUsage 5 字段**（input / cached_input / output / reasoning_output / total）

**Commit 2 — V55 迁移**：`cost_records` 加 `cached_input_tokens` + `reasoning_output_tokens` 列。

**Commit 3 — `observability/` 加 4 个 metric**：
- `uclaw_turn_e2e_duration_ms`
- `uclaw_turn_tool_call_count`
- `uclaw_turn_token_usage`（按 token_type 5 维 histogram）
- `uclaw_turn_memory_metric`

**Commit 4 — `info_span!("turn", thread.id, turn.id, model, ...)` 全 turn 一档**

#### M1-T7：Prewarm LLM 连接（0.5 周）

**Commit 1 — `main.rs::Stage 3` spawn 预热 task**：预建 keepalive HTTP/2 + 预协商 SSE。

### 3.3 M1 完成定义

- [ ] M1-T1 ~ T7 全合并
- [ ] **ADR M1 Exit criteria**：one chat task + one browser task + one automation run 产生 comparable traces
- [ ] cargo build + cargo test 全通过
- [ ] 性能基线未退化

---

## 4. Milestone M2：Context Fabric（5-7 周）<a id="4-m2"></a>

### 4.1 ADR §16 M2 规约

**交付**：ContextRef + ContextArtifact schema + context.* 7 工具 + source-cited folding 格式 + context budget 核算 + UI context reads trace。

**Exit criteria**：agent 可按需检索 code / memory / browser / prior trace 上下文，不预加载全部。

### 4.2 任务清单

> 复用 v1.1 PR-S/PR-T 详细设计（已实地核对 codex 实现）。这里按 M2-A ~ M2-J 重新组织。

#### M2-A：baseline.md 12 block 完整重写（2 周）

**Commit 1 — 12 block 大纲 + 同行评审**（团队 reviewer）
**Commit 2 — 完整内容写入**（~8K 字 / ~2K tokens）
- Block A: 身份与角色
- Block B: 默认人格
- Block C: UCLAW.md 规约
- Block D: Preamble（8 例句）
- Block E: Planning（3 好 + 3 坏 plan 对比）
- Block F: 任务执行（10+ 硬规约 + apply_patch syntax）
- Block G: 验证 (specific → broad)
- Block H: Ambition vs Precision
- Block I: Progress (8-10 词)
- Block J: Final answer formatting (~3K 字 style guide)
- Block K: 工具指南
- Block L: NEVER 列表（≥ 12 条）

**Commit 3 — include_str! 引入 + 组装链替换**
**Commit 4 — Snapshot 测试 + Feature flag 灰度**
**Commit 5 — 默认开启 + 移除旧 baseline 字面量**

#### M2-B：UCLAW.md 项目级指令注入（1 周）

**Commit 1 — `ProjectDocManager` 实现**（仿 codex `AgentsMdManager`）：从工作区根（`.git` / `.uclaw/` marker）向 cwd 收集 `UCLAW.md`，`UCLAW.override.md` 优先，separator `\n\n--- project-doc ---\n\n`。

**Commit 2 — baseline.md Block C 节明文告知 LLM"UCLAW.md 已注入"**

**Commit 3 — file-watcher 监听 UCLAW.md 改动 + 热重载**

**Commit 4 — UI 编辑器 + 用户文档**

#### M2-C：30+ Context Fragment 抽象（2 周）

**Commit 1 — trait 定义**

```rust
// src-tauri/src/agent/context/mod.rs
pub trait ContextFragment: Send + Sync {
    fn key(&self) -> &'static str;
    fn render(&self) -> String;
    fn token_estimate(&self) -> usize;
    fn build_update_item(&self, prev: Option<&TurnContextItem>) -> Option<ResponseItem>;
}
```

**Commit 2 — P0 fragment 10 个**：EnvironmentContext / PermissionsInstructions / PersonalitySpecInstructions / AvailableSkillsInstructions / AvailablePluginsInstructions / GoalContext / SubagentNotification / UserInstructions / ModelSwitchInstructions / TurnAborted

**Commit 3 — P1 fragment 10 个**：HookAdditionalContext / AppsInstructions / ImageGenerationInstructions / UserShellCommand / NetworkRuleSaved / ApprovedCommandPrefixSaved / GuardianFollowupReviewReminder / LegacyApplyPatchExecCommandWarning / LegacyModelMismatchWarning / LegacyUnifiedExecProcessLimitWarning

**Commit 4 — Snapshot 测试每个 fragment**

**Commit 5 — `PromptAssembler` 替代旧 `effective_system_prompt`**

#### M2-D：Diff-based re-injection（1.5 周）

**Commit 1 — ContextManager.reference_context_item 字段**

**Commit 2 — 每 fragment 实现 build_update_item**

**Commit 3 — Assembler 集成（仅变化时注入）**

**Commit 4 — Snapshot + benchmark 100-turn 会话**

#### M2-E：Template 引擎（0.5 周）

**Commit 1 — uclaw-utils-template 已在 Phase 0.5-T3 复制完成**

**Commit 2 — 现有 prompt 字符串拼接全部用 Template**

#### M2-F：7 Context Tools（1 周）

**Commit 1 — 新 handler 7 个**：context.search / context.read / context.fold / context.cite / context.compare / context.pin / context.release

**Commit 2 — 每次调用 emit TaskEvent::ContextRead/Write/Pinned/Released**

**Commit 3 — UI 可视化 context reads 列表**

#### M2-G：8 字段 Structured Fold（0.5 周）

**Commit 1 — 新模板 `prompts/templates/compact/structured_fold.md`** 含 8 字段（facts / decisions / unresolved / evidence / failed_attempts / constraints / next_actions / rollback_points）

**Commit 2 — 替换现有 compact prompt**

#### M2-H：7 层 Token 防线（2 周）

**L1 TruncationPolicy**（依赖 Phase 0.5-T4 已复制的 uclaw-utils-output-truncation）：每 handler 出口 `formatted_truncate_text`。默认 budget：shell/exec 8K / file 4K / search 4K / web 6K / mcp 5K。用户配置 `[tool_output_budgets]`。UI 显示"已截断"。

**L2 ToolExposure**：枚举 Always/OnDemand/Hidden + normalize_tool_schema（去 description.examples / 合并 enum / 裁剪 nested ≥3 层）。MCP server 级配置。

**L3 Per-turn skills**：top-K（默认 5）+ default_skill_metadata_budget=1500。

**L4 Diff updates**：M2-D 已完成。

**L5 Image stripping**：每 provider `supports_images: bool`，不支持时占位 "image content omitted because the current model does not support image input"。

**L6 ensure_call_outputs_present**：ContextManager.for_prompt 阶段扫描合法性，孤儿 FunctionCall 合成 "aborted" 占位。

**L7 三档 compaction**：
- 新模块 `src-tauri/src/agent/compact/` (mod, local, remote, prompts/, analytics, budget)
- `local.rs` 重构现有 compress_context
- `remote.rs` 新增（部分 provider 支持 Anthropic Haiku 4.5 等小模型做 summary）
- InitialContextInjection 双模式（BeforeLastUserMessage / DoNotInject）
- `COMPACT_USER_MESSAGE_MAX_TOKENS = 20_000` 硬限
- Pre/Post-compact hooks（与 M5 共享）
- CompactionAnalytics 5 维（trigger / reason / implementation / phase / status）

**Buffer**：`effective_context_window_percent = 92`（uclaw 推理型 LLM 较多，比 codex 95% 更保守）。

#### M2-I：Prompt caching 优化（0.5 周）

**Commit 1 — 前缀稳定**：baseline + UCLAW.md + skill manifest 放最前；变化项放末尾。

**Commit 2 — `cached_input_tokens` 命中率 metric + A/B 验证**：目标 ≥ 50% 命中率。

#### M2-J：Token budget UI dashboard（1 周）

**Commit 1 — Settings → Token Usage 页面**：context 占用 progress bar / 累计 cost（今日/本周/本月）/ 工具 token top-10 / 接近 context window 告警 toast。

### 4.3 M2 完成定义

- [ ] 全部 M2-A ~ M2-J 合并
- [ ] **ADR M2 Exit criteria**：agent 可按需检索 code/memory/browser/prior trace 上下文，不预加载全部
- [ ] **Benchmark**：50-turn 会话 token 节约 60-75%
- [ ] **Cached token 命中率 ≥ 50%**
- [ ] 月度成本下降 ≥ 60%
- [ ] 输出格式一致性主观评分 +1.5/5 以上

---

## 5. Milestone M3：Capability Mesh（6-8 周）<a id="5-m3"></a>

### 5.1 ADR §16 M3 规约

**交付**：ToolRegistry / ProviderRegistry / PluginRegistry / CapabilityProfileRegistry + plugin manifest parser + Hermes-style bundled/user/project plugin discovery + 显式 override policy + provider health TTL。

**Exit criteria**：本地 browser + gbrain 注册为 provider；至少 1 个 bundled plugin 可发现但 disabled；1 个 task 以受限 capability profile 运行。

### 5.2 任务清单

#### M3-T1：五大 Registry 类型定义（1 周）

新模块 `src-tauri/src/capabilities/`：

```
capabilities/
├── mod.rs
├── card.rs                    # CapabilityCard 类型
├── tool_registry.rs
├── provider_registry.rs
├── plugin_registry.rs
├── capability_profile_registry.rs
├── worker_registry.rs
├── gateway.rs                 # ToolGateway
└── card_yaml.rs               # YAML serde
```

`CapabilityCard` 含 id / kind / family / description / permissions / cost / reliability(harnessScore + lastEvaluatedAt) / failureModes / humanBoundaries。

#### M3-T2：把现有 tools 注册到 ToolRegistry（1 周）

把 `agent/tools/builtin/` 13 个 tool + MCP tools + memU tools + skill-as-tools 全部注册。每个 tool 提供 CapabilityCard。

#### M3-T3：把 MCP / providers / gbrain / memU 注册到 ProviderRegistry（1 周）

- `mcp.rs::SharedMcpManager` → MCP backend provider
- `providers/` LLM providers → model-provider plugin kind
- `gbrain` → exclusive memory provider
- `memU` → auxiliary memory provider（disabled by default）

每个 provider 含 health TTL（30s 默认）。

#### M3-T4：新增缺失工具（0.5 周）

借鉴 codex：`mcp_resource` / `request_permissions` / `request_plugin_install` / `view_image` / `tool_search` / `unified_exec`。V47 迁移 `agent_jobs` + `agent_job_items` 表（编号以提交时为准）。

#### M3-T5：Skill 作用域 + per-turn 注入 + budget（M2-H L3 已落地）

V43 迁移：`skill_scope ENUM('User','Repo','Workspace','System')`。

#### M3-T6：PluginRegistry + Plugin manifest（2 周）

**Commit 1 — Plugin manifest schema**（YAML）+ 解析器

`src-tauri/src/capabilities/plugin_manifest.rs` 解析 ADR §9.3 格式。

**Commit 2 — 4 source 发现**：bundled（嵌入二进制）+ user（`~/.uclaw/plugins/`）+ project-trusted（项目 `.uclaw/plugins/`）+ external（远程 marketplace）

**Commit 3 — 5 kind 实现**：standalone / backend / exclusive / platform / model-provider

**Commit 4 — install/update/uninstall/list API**

**Commit 5 — V43 迁移 `installed_plugins` 表**

**Commit 6 — UI 插件管理面板**

#### M3-T7：把现有扩展迁移为 plugin（1 周）

- memU → `~/.uclaw/plugins/memu/plugin.toml`（exclusive memory provider，disabled by default）
- gbrain → `~/.uclaw/plugins/gbrain/`（exclusive memory provider，enabled）
- automation_installed_skills（V22）→ 转 plugin manifest

#### M3-T8：CapabilityProfileRegistry + mode 重构（1 周）

把 plan_mode / ask_mode / bypass_mode / accept_edits_mode 重构为 TOML profile（`~/.uclaw/profiles/<name>.toml`）。包含 autonomyMax / allowedToolsets / deniedCapabilities / budget / requiresApproval。

ToolExposure（M2-H L2）纳入 CapabilityProfile.allowedToolsets。

#### M3-T9：MCP server 暴露 uclaw 能力（1 周）

**Commit 1 — 新模块 `src-tauri/src/mcp/server/`** 用 `rmcp` crate

**Commit 2 — 暴露选定 tool**：list_threads / read_thread / start_automation / query_memory

**Commit 3 — `~/.uclaw/mcp_server.toml` 配置 + 认证 token**

**Commit 4 — UI Settings → MCP Server 开关**

#### M3-T10：Consequential templates（0.5 周）

`~/.uclaw/mcp_consequential_templates.json` —— destructive 工具的审批文案模板。SafetyManager 加载。

#### M3-T11：LLM 主动 request_plugin_install（0.5 周）

LLM 调用 `request_plugin_install(plugin_id, marketplace_name)` → 弹窗给用户确认。

### 5.3 M3 完成定义

- [ ] 五大 Registry 完整 + Capability Cards 全部生成
- [ ] Plugin manifest 解析 + 4 source 发现工作
- [ ] memU / gbrain 已迁移为 plugin
- [ ] **ADR M3 Exit criteria**：本地 browser + gbrain 注册为 provider；至少 1 个 bundled plugin discoverable but disabled；1 个 task 以受限 capability profile 运行
- [ ] uclaw 作为 MCP server 文档发布

---

## 6. Milestone M4：World Projection（3-4 周）<a id="6-m4"></a>

### 6.1 ADR §16 M4 规约

**交付**：materialized task projection from TaskEvent streams + pending boundary projection + active provider/worker projection + browser/task/team state 消费 projection。

**Exit criteria**：UI 能回答 agent 在做什么 / 等什么 / 用什么 / 能否 resume。

### 6.2 任务清单

#### M4-T1：WorldProjection 类型 + apply_event（1 周）

```rust
// src-tauri/src/projection/world.rs
pub struct WorldProjection {
    pub intent: Option<IntentSpec>,
    pub current_plan: Option<PlanSnapshot>,
    pub task_state: TaskState,
    pub history: Vec<TaskEventSummary>,
    pub waiting_on: Option<BoundaryRef>,
    pub active_capabilities: Vec<CapabilityCard>,
    pub active_workers: Vec<WorkerSnapshot>,
    pub context_reads: Vec<ContextReadSummary>,
    pub memory_writes: Vec<MemoryWriteReceipt>,
    pub boundaries_hit: Vec<BoundaryEvent>,
    pub resume_checkpoints: Vec<CheckpointRef>,
    pub latest_harness_score: Option<HarnessVerdict>,
    pub action_affordances: Vec<UserAffordance>,
    pub version: u64,
}
```

#### M4-T2：projection subscriber（1 周）

订阅 TaskEvent 流 + 增量更新 + diff_since(version) 接口。V53 迁移 `world_projection_snapshots` 表（用于 resume）。

#### M4-T3：前端 useWorldProjection hook（1 周）

```ts
function useWorldProjection(taskId: string): WorldProjection
```

替代 20+ store 的混合订阅。UI 各 panel 从 projection 直读。

#### M4-T4：消费方迁移（1 周）

- chat panel 用 projection.intent + .task_state + .history
- browser panel 用 projection.active_workers + .boundaries_hit
- automation panel 用 projection.active_workers + .resume_checkpoints
- timeline 用 projection.history

#### M4-T5：OTLP turn span export（0.5 周）

`observability/` 加 OTel exporter（与 projection 状态对齐）。

### 6.3 M4 完成定义

- [ ] WorldProjection 类型 + subscriber 工作
- [ ] **ADR M4 Exit criteria**：UI 能回答 agent 在做什么 / 等什么 / 用什么 / 能否 resume

---

## 7. Milestone M5：Policy Hooks + Isolation（4-5 周）<a id="7-m5"></a>

### 7.1 ADR §16 M5 规约

**交付**：HookBus with trace-visible lifecycle events + policy hooks for tools/memory/browser/subagent/worker/promotion + task isolation profiles + dirty-worktree/worktree policy。

**Exit criteria**：hook 可阻止 unsafe action 且 rejection 出现在 task trace。

### 7.2 任务清单

#### M5-T1：HookBus + 13 event 类型（1 周）

```rust
// src-tauri/src/safety/hook_bus.rs
pub enum HookEventName {
    UserPromptSubmit, IntentClassified,
    PreContextRead, PostContextRead,
    PreToolUse, PostToolUse,
    PreMemoryWrite, PreBrowserAction,
    PermissionRequest,
    SubagentStart, WorkerAssignment,
    PrePromotion, SessionEnd,
}
```

13 event 的 can_block / can_mutate / must_emit_event 矩阵（对位 ADR §10.1）。

#### M5-T2：HookRegistry + 配置（0.5 周）

V51 迁移：`hook_configs(id, event, matcher_regex, command_argv, execution_mode, trust_status, ...)`。

`~/.uclaw/hooks.toml` + plugin-declared hooks。

#### M5-T3：13 集成点接线（1.5 周）

每个事件接到对应代码位置：
- UserPromptSubmit → agent loop 入口
- IntentClassified → IntentSpec 构造完成
- PreContextRead + PostContextRead → context.* tools
- PreToolUse + PostToolUse → ToolOrchestrator
- PreMemoryWrite → memory.rs / gbrain MCP writes
- PreBrowserAction → browser/agent_loop
- PermissionRequest → SafetyManager
- SubagentStart → subagent spawn
- WorkerAssignment → teams orchestrator
- PrePromotion → Evolution Factory（M7 配套）
- SessionEnd → session.rs 退出

#### M5-T4：IsolationProfile trait + 7 工作类型（1 周）

```rust
pub enum IsolationScope {
    ConversationScope { thread_id: ThreadId },
    GitWorktree { base_branch: String, worktree_path: PathBuf },
    DirtyTreePolicy { allowed_paths: Vec<PathPattern> },
    BrowserSession { profile_id: BrowserProfileId },
    SubagentContext { parent: SessionId, restricted_tools: Vec<ToolName> },
    AutomationRun { run_id: RunId, ledger_path: PathBuf },
    TeamRoleContext { role: WorkerRole, channel: ChannelId },
    RemoteWorker { worker_id: WorkerId, locality: DataLocality },
}

pub trait IsolationProfile {
    fn allowed_writes(&self) -> Vec<PathPattern>;
    fn allowed_reads(&self) -> Vec<PathPattern>;
    fn allowed_network(&self) -> NetworkPolicy;
    fn allowed_tools(&self) -> Vec<ToolName>;
    fn entry_event(&self) -> TaskEvent;
    fn exit_event(&self, verdict: TaskVerdict) -> TaskEvent;
}
```

#### M5-T5：Git worktree 自动创建（0.5 周）

针对 coding task：自动 `git worktree add` + 退出时 cleanup / merge。

#### M5-T6：Human Boundary 处理（0.5 周）

7 类 boundary 检测：credentials / CAPTCHA / payment / destructive / external messages / publishing / private data exposure / autonomy escalation。

每个触发对应 hook + UI 弹窗。

#### M5-T7：UI hook 管理面板（0.5 周）

Settings → Hooks：列表 / 启停 / 历史。

### 7.3 M5 完成定义

- [ ] 13 个 hook 全部接入
- [ ] 7 个 isolation profile 工作
- [ ] **ADR M5 Exit criteria**：hook 可阻止 unsafe action 且 rejection 出现在 trace

---

## 8. Milestone M6：Browser Provider 抽象（3-4 周）<a id="8-m6"></a>

### 8.1 ADR §16 M6 规约

**交付**：BrowserProvider trait + LocalChromiumProvider adapter + provider-independent browser harness + Browser Use / Browserbase / Firecrawl provider plugin stubs + site workflow script contract。

**Exit criteria**：同一 browser harness case 可针对 local provider 和 mock external provider 运行。

### 8.2 任务清单

#### M6-T1：BrowserProvider trait（1 周）

```rust
// src-tauri/src/browser/provider.rs
pub trait BrowserProvider: Send + Sync {
    async fn create_session(&self, config: BrowserSessionConfig) -> Result<BrowserSession>;
    async fn snapshot(&self, session: &BrowserSession) -> Result<BrowserSnapshot>;
    async fn action(&self, session: &BrowserSession, action: BrowserAction) -> Result<BrowserActionResult>;
    async fn detect_boundary(&self, session: &BrowserSession) -> Option<BrowserBoundaryEvent>;
    async fn checkpoint(&self, session: &BrowserSession) -> Result<CheckpointRef>;
    async fn close_session(&self, session: BrowserSession) -> Result<()>;
    fn capability_card(&self) -> CapabilityCard;
}
```

#### M6-T2：LocalChromiumProvider adapter（1 周）

把 `browser/context_manager.rs::BrowserContextManager` 适配为 LocalChromiumProvider。保留所有现有 v2 功能（identity broker / loop_detector / recovery / memory_adapter / perception 等）。

`BrowserService` 标 sunset note，仅作 compat surface。

#### M6-T3：Provider plugin stubs（1 周）

- Browser Use plugin stub
- Browserbase plugin stub
- Firecrawl plugin stub

每个含 manifest + 占位实现 + harness cases。

#### M6-T4：Browser harness（0.5 周）

`harness/cases/browser/` 加 case set：navigate / login / extract / form-fill / captcha-detect / payment-block 等。

同一 case 可对 local + mock external provider 运行。

#### M6-T5：Site workflow script contract（0.5 周）

定义 browser script artifact 格式（Evolution Factory candidate kind=browser_script 配套）。

### 8.3 M6 完成定义

- [ ] BrowserProvider trait 工作
- [ ] LocalChromiumProvider 替代 BrowserService
- [ ] 至少 1 个 external provider plugin stub
- [ ] **ADR M6 Exit criteria**：同一 harness case 可 local + mock external

---

## 9. Milestone M7：Evolution Factory（6-8 周）<a id="9-m7"></a>

### 9.1 ADR §16 M7 规约

**交付**：learning artifact schema + reflection generator + candidate builder + harness promotion gate + user review surface + rollback/disable path。

**Exit criteria**：已完成的 task 能提议 skill/SOP/browser script，跑 gate，等待用户批准。

### 9.2 任务清单

#### M7-T1：evolution/ 模块统一 pipeline（1 周）

```
src-tauri/src/evolution/
├── mod.rs
├── pipeline.rs              # Reflection → Candidate → Simulation → Harness → User Review → Promotion → gbrain
├── reflection/              # 把 proactive scenarios 升格为 reflection generator
│   ├── conversation.rs (现有 conversation_learning)
│   ├── skill.rs            (现有 skill_extraction)
│   ├── multimodal.rs       (现有 multimodal_context)
│   ├── gep.rs              (现有 gene_evolution)
│   ├── plan_mode.rs        (现有 plan_mode_calibration)
│   ├── memory_health.rs    (现有 memory_health)
│   └── memory_lint.rs      (现有 memory_lint)
├── candidate/               # 9 种 candidate type
│   ├── gbrain_page_update.rs
│   ├── skill_or_sop.rs
│   ├── browser_script.rs
│   ├── prompt_patch.rs
│   ├── planner_heuristic.rs
│   ├── capability_profile_adjustment.rs
│   ├── policy_hook.rs
│   ├── failure_memory.rs
│   └── regression_harness_case.rs
├── simulation.rs            # dry-run / safe simulation（新增层）
├── promotion.rs             # 包装 harness/self_improvement.rs
├── review.rs                # User Review surface
└── registry.rs              # Promotion Registry（版本化 + 可回滚）
```

#### M7-T2：把现有 7 proactive scenarios 适配为 reflection generator（1 周）

每个 scenario 改实现 `ReflectionGenerator` trait，输出 `LearningCandidate`。

加 Feature flag 控制开关 + 优先级。

#### M7-T3：9 candidate type 实现（2 周）

每种 candidate 含：source_trace_ref / proposed_scope / safety_impact / 自带 simulation 函数 / rollback_plan。

#### M7-T4：Simulation 阶段补全（1 周）

每个 candidate type 写 dry-run：
- browser_script candidate 在 sandboxed browser 跑
- prompt_patch candidate 在 LLM eval set 跑
- skill_or_sop candidate 在 harness 子集跑
- ...

#### M7-T5：Promotion Gate（包装 harness/self_improvement.rs）（0.5 周）

7 字段强制 check：source trace / proposed scope / safety impact / benchmark result / rollback plan / owner / version id。

6 类禁止 promotion 的硬性 check：silent permission widening / secret capture / prompt mutation without regression / memory writes without evidence / provider enablement without user config / autonomy escalation without policy approval。

#### M7-T6：User Review UI（1.5 周）

Settings → Evolution Queue：
- 列表待审 candidates
- 每个含 source trace + evidence + harness score + safety impact + rollback plan
- 一键 approve / edit / reject

#### M7-T7：Promotion Registry + V52 迁移（0.5 周）

`evolution_promotions(id, candidate_kind, version, status, source_trace_ref, harness_result_json, safety_impact_json, rollback_plan_json, owner, approved_at, promoted_at)`。

#### M7-T8：把 learning Sprint 1 (user_profile_facets V39) 作为第一个完整 candidate 跑通（0.5 周）

### 9.3 M7 完成定义

- [ ] Pipeline 完整工作
- [ ] **ADR M7 Exit criteria**：完成 task → 提议 skill/SOP/browser script → 跑 gate → 等待用户批准

---

## 10. Milestone M8：Teams v1（5-7 周）<a id="10-m8"></a>

### 10.1 ADR §16 M8 规约

**交付**：TeamSpec + role registry + coordinator + typed team channels + reviewer gate + team harness episode。

**Exit criteria**：team run 产生 role outputs + reviewer verdict + verified final artifact。

### 10.2 任务清单

#### M8-T1：把现有 agent/teams/ 用 IntentSpec/TaskEvent 包装（1 周）

worker / reviewer / supervisor 各跑独立 SessionTask + 独立 CapabilityProfile。

#### M8-T2：SubAgent MVP（2 周）

借鉴 codex codex_delegate：
- `SessionSource::SubAgent { parent_thread_id, depth }`
- 共享 services 句柄但独立 session_state
- child CancellationToken 树
- approval 上浮父 session
- 先支持 depth ≤ 1
- V41 迁移 `agent_session_spawn_edges`

#### M8-T3：spawn 工具暴露给 LLM（1 周）

`spawn_subagent(task, role, max_turns)` / `wait_subagent` / `get_subagent_result` / `list_active_subagents`

#### M8-T4：Role-as-config TOML（1 周）

把 worker/reviewer/supervisor + explorer/awaiter/planner 重构为 TOML（`~/.uclaw/roles/<name>.toml`）。借鉴 codex `apply_role_to_config`。每个 TOML 含 model / reasoning_effort / developer_instructions。

#### M8-T5：Nickname pool（0.5 周）

100 个中文化命名（墨子 / 张衡 / 徐霞客 / 毕昇 / 华罗庚 / ...）。重名加序数。

#### M8-T6：TeamSpec + Coordinator + TeamChannel + ReviewGate（1.5 周）

```rust
pub struct TeamSpec {
    pub id: String,
    pub roles: Vec<TeamRole>,
    pub coordinator: CoordinatorSpec,
    pub channel: ChannelSpec,
    pub review_gate: ReviewGateSpec,
    pub budget: BudgetSpec,
    pub policy: PolicySpec,
    pub output_contract: OutputContract,
}
```

reviewer 可 block 完成。coordinator 不可 bypass policy hooks。

#### M8-T7：Team harness episode（0.5 周）

`harness/case.rs::HarnessSubject::Coordinator` + `subject=Tasks` 双重评估。

### 10.3 M8 完成定义

- [ ] SubAgent MVP（depth=1）工作
- [ ] TeamSpec + Coordinator + ReviewGate 工作
- [ ] **ADR M8 Exit criteria**：team run 产生 role outputs + reviewer verdict + verified final artifact

---

## 11. Milestone M9：Cluster v1（12-16 周，远期）<a id="11-m9"></a>

### 11.1 ADR §16 M9 规约

**交付**：worker registry + heartbeat + capability routing + load-aware assignment + data locality policy + checkpoint/failover + 远程 event ingestion。

**Exit criteria**：local worker + mock remote worker 跑 comparable tasks，含 unified trace + recovery。

### 11.2 任务清单（远期，简述）

- M9-T1：WorkerRegistry + WorkerNode 类型（8 kind: local/subagent/worktree/remote/container/mobile/cloud + heartbeat）
- M9-T2：Capability routing 算法
- M9-T3：Load-aware assignment
- M9-T4：Data locality policy
- M9-T5：Checkpoint / failover 协议
- M9-T6：Mock remote worker 实现
- M9-T7：远程 event 摄取到本地 projection

### 11.3 M9 完成定义

- [ ] **ADR M9 Exit criteria**：local + mock remote worker 跑 comparable tasks + unified trace + recovery

---

## 12. 依赖关系图与时间线 <a id="12-依赖与时间线"></a>

```
Phase 0.5 (License + Workspace + Crate copy)
   │   ✓ License Apache-2.0
   │   ✓ Cargo workspace
   │   ✓ 6 第一批 crate + output-truncation
   ↓
M1 Runtime Contracts (2-3 w)
   │   依赖 Phase 0.5
   ↓
M2 Context Fabric (5-7 w)
   │   依赖 M1（TaskEvent 类型）+ Phase 0.5（uclaw-utils-template / output-truncation）
   ↓
   ┌─────────────────┐
   ↓                 ↓
M3 Capability Mesh   M4 World Projection
(6-8 w)              (3-4 w)
依赖 M1+M2           依赖 M1 + M3 部分
   ↓                 ↓
   └────────┬────────┘
            ↓
M5 Policy Hooks + Isolation (4-5 w)
依赖 M3（registry）+ M2（context）
            ↓
M6 Browser Provider (3-4 w)
依赖 M3（plugin）+ M5（isolation）
            ↓
M7 Evolution Factory (6-8 w)
依赖 M1 + M2 + M5（PrePromotion hook）
            ↓
M8 Teams v1 (5-7 w)
依赖 M3 + M5 + M7
            ↓
M9 Cluster v1 (12-16 w，远期)
依赖全部
```

### 12.1 总体时间线

| 阶段 | 时长 | 累计 |
|---|---|---|
| Phase 0.5 | 3-5 天 | 0.2 月 |
| M1 | 2-3 周 | 1 月 |
| M2 | 5-7 周 | 3 月 |
| M3 + M4 并行 | 6-8 周 | 4.5 月 |
| M5 | 4-5 周 | 5.5 月 |
| M6 | 3-4 周 | 6.5 月 |
| M7 | 6-8 周 | 8 月 |
| M8 | 5-7 周 | 9.5 月 |
| M9（远期） | 12-16 周 | 13 月 |

**M1-M8 总时长（核心）**：**8-11 月（3 人团队全职，中位 ~9.5 月）**
**M1-M9（含远期）**：**13-15 月（中位 ~14 月）**

（与对比文档 §2.4 + 本文档 §1 完全一致）

### 12.2 不同团队规模

| 团队规模 | M1-M8（核心）| 含 M9（远期）|
|---|---|---|
| 1 人全职 | 16-22 月 | 24-30 月 |
| 2 人全职 | 10-14 月 | 16-20 月 |
| **3 人全职**（**推荐**） | **8-11 月（中位 9.5）** | **13-15 月（中位 14）** |
| 4+ 人 | 6-9 月 | 10-13 月 |

---

## 13. 测试与验证策略 <a id="13-测试"></a>

### 13.1 测试层次

**单元测试**：每个 module `#[cfg(test)] mod tests`，覆盖率 ≥70%（cargo-tarpaulin）。

**集成测试**：`src-tauri/tests/` integration tests。跨模块流程（spawn_task → tool dispatch → safety → cost record）。

**E2E 测试**：完整 turn 流（用户 input → LLM → tool → 输出）。异常路径（中断、网络故障、token 超限）。多 turn 序列。

**前端测试**：Vitest + RTL + JotaiProvider。`renderWithProviders` from `ui/src/test-utils/render.tsx`。Recharts mock。

**Harness 测试**：复用现有 `harness/` 11 文件 + 12 HarnessSubject。每个 Milestone 加对应 subject 的 case set。

**性能测试**：每 Milestone 完成后跑 50 个 turn benchmark。

### 13.2 关键 benchmark

- **冷启动**："app 就绪"耗时（M1-T7 Prewarm 后应下降）
- **首字延迟**：用户发消息 → LLM 第一个 token（M1-T7 Prewarm 后应 -200~400ms）
- **Turn 完成时间**：消息发出 → 全部工具调用完成（M2-H 并行 tools 后应 -30%）
- **Token 节约**：50-turn 会话 token 总量（M2 完成后 -60~75%）
- **Cached token 命中率**：cached_input_tokens / input_tokens（M2-I 后 ≥ 50%）
- **CPU 占用**：（M2-D diff 注入后无显著上升）

### 13.3 验证矩阵（每改进项必含）

| 类型 | 内容 |
|---|---|
| Unit | ≥3 happy + ≥3 error 单测 |
| Integration | ≥1 跨模块场景 |
| E2E | ≥1 用户视角 |
| Snapshot | 影响输出格式的必含 |
| Harness | 关联 HarnessSubject 必加 case |
| Benchmark | 涉及热点必含 |
| Manual | 涉及 UI 必含 screenshot + steps |

---

## 14. 回滚与紧急预案 <a id="14-回滚"></a>

### 14.1 数据库迁移回滚

每个新 V 号必须 forward + tolerant：
- `CREATE TABLE IF NOT EXISTS`
- `ALTER TABLE ... ADD COLUMN ...` 配 try/swallow
- 不破坏旧 schema 的 SELECT

紧急回滚：reverse migration（如 V100：undo_V44）。

### 14.2 Feature flag 策略

每个大改动加 feature flag（参考 codex `features` crate / Phase 0.5-T5 复制版 19）：

```rust
pub enum UclawFeature {
    AdrAgentOsV2Runtime,         // M1 总开关
    ContextFabricToolset,        // M2-F 7 tools
    CapabilityMeshRegistries,    // M3
    WorldProjectionV1,           // M4
    HooksBus13Events,            // M5-T1
    BrowserProviderTrait,        // M6
    EvolutionFactoryPipeline,    // M7
    TeamsV1,                     // M8
    ClusterV1,                   // M9
    NewBaselinePrompt,           // M2-A
    DiffBasedContextUpdates,     // M2-D
    ThreeTierCompaction,         // M2-H L7
    // ...
}
```

启动时从 config 读，每个改动同时支持新旧路径，flag off 即走旧。

### 14.3 灰度策略

- M1-M2 完成 → 内部 alpha
- M3-M5 完成 → 内部 beta + 选定用户
- M6-M8 完成 → 全员

### 14.4 紧急 patch 流程

CLAUDE.md 已规定 "hotfixes with an obvious root cause and a ≤ 1-file fix" 可跳过 superpowers 流程。保留此豁免。

### 14.5 回滚预演

每个 Milestone 完成前演练一次："如果发现严重问题，30 分钟内如何回滚到上个版本？"。

---

## 15. 资源与时间估算 <a id="15-资源"></a>

### 15.1 总时间（含 v2.0 修正）

| Milestone | 时长 | 人周 |
|---|---|---|
| Phase 0.5 | 0.2 月 | 1-2 |
| M1 | 2-3 周 | 6-9 |
| M2 | 5-7 周 | 15-21 |
| M3 | 6-8 周 | 18-24 |
| M4 | 3-4 周 | 9-12（可与 M3 并行）|
| M5 | 4-5 周 | 12-15 |
| M6 | 3-4 周 | 9-12 |
| M7 | 6-8 周 | 18-24 |
| M8 | 5-7 周 | 15-21 |
| **M1-M8 小计** | **34-46 周** | **102-138 人周** |
| M9 | 12-16 周 | 36-48（远期） |

### 15.2 与 v1.1 对比

| 项 | v1.1 估计 | v2.0 修正 |
|---|---|---|
| 总时长 | 15-18 月（3 人） | **8-11 月（3 人，含 M9 13-15 月）** |
| 总人周 | 96-136 | **102-138** |
| 主要减少原因 | 多个模块（harness/learning/browser v2）已有骨架 | — |

### 15.3 外部依赖

- Mock LLM provider（wiremock + 固定响应集）
- OTLP collector（可选，自建或 SaaS）
- 代码签名（M6 plugin signing 时需 macOS 开发者证书）

---

## 16. PR 流程与 commit 粒度 <a id="16-pr-流程"></a>

按 CLAUDE.md 现有规约 + ADR §18 增强：

### 16.1 一 Milestone 一 plan，一 task 一 PR

每个 M*-T* 对应一份 plan（`docs/superpowers/plans/<name>.md`），一份 PR。

PR title 格式：`[M<N>-T<M>] <代号> <名称>`，如：
- `[M1-T2] SessionTask trait + 抢占式调度`
- `[M5-T1] HookBus 13 events`

### 16.2 ADR §18 11 问必答

每份 spec 必答 ADR §18 11 问（已嵌入对比文档每章末尾）：
1. 用户 intent
2. autonomy 等级
3. canonical truth source
4. emit 什么 TaskEvent
5. 读什么 context，怎么 cite
6. 添加/消费 capability card
7. 什么 policy hook 能 block
8. UI 渲染什么 world projection
9. 什么 harness case 证明
10. rollback / disable 路径
11. 不拥有什么

### 16.3 Commits 必须 bisectable

每 commit 独立可编译可测。PR body 必带 `## Commits (bisectable)` 表。

### 16.4 ADR + 迁移注册表同步更新

涉及迁移的 PR 必须在 CLAUDE.md 的 *Active migration registry* 加行（status `in progress` → 合并后 `merged`）。

### 16.5 测试为提交一等公民

测试必须在 PR 中。不允许"先合代码后补测试"。测试不通过禁止合并。

---

## 17. Crate 复制操作（Phase 0.5）<a id="17-crate-复制"></a>

> 详细步骤已在 §2.2 Phase 0.5-T1 ~ T5 给出。这里给速查表。

### 17.1 第一档：直接复制零改动（17 个）

按 ROI 排序：

| # | codex 路径 | uclaw 路径 | Phase |
|---|---|---|---|
| 1 | `utils/template` | `uclaw-utils-template` | T3 ★★★★★ |
| 2 | `utils/string` | `uclaw-utils-string` | T3 ★★★★★ |
| 3 | `utils/cache` | `uclaw-utils-cache` | T3 ★★★★ |
| 4 | `utils/fuzzy-match` | `uclaw-utils-fuzzy` | T3 ★★★★ |
| 10 | `async-utils` | `uclaw-async-utils` | T3 ★★★★ |
| 11 | `file-watcher` | `uclaw-file-watcher` | T3 ★★★★★ |
| 5 | `utils/elapsed` | `uclaw-utils-elapsed` | T5 ★★★ |
| 7 | `utils/readiness` | `uclaw-utils-readiness` | T5 ★★★★ |
| 8 | `utils/sleep-inhibitor` | `uclaw-utils-sleep` | T5 ★★★★ |
| 12 | `file-search` | `uclaw-file-search` | T5 ★★★★ |
| 13 | `utils/absolute-path` | `uclaw-utils-abs-path` | T5 ★★★★ |
| 14 | `utils/home-dir` | `uclaw-utils-home` | T5 ★★★ |
| 15 | `utils/path-utils` | `uclaw-utils-path` | T5 ★★★ |
| 16 | `utils/image` | `uclaw-utils-image` | T5 ★★★ |
| 6 | `utils/json-to-toml` | `uclaw-utils-json-toml` | T5 ★★ |
| 9 | `utils/stream-parser` | `uclaw-utils-stream` | T5 ★★★ |
| 17 | `utils/pty` | `uclaw-utils-pty` | M6 配套 ★★★ |

### 17.2 第二档：复制 + 微改动（3 个）

| # | codex 路径 | uclaw 路径 | Phase |
|---|---|---|---|
| 18 | `utils/output-truncation` | `uclaw-utils-output-truncation` | T4 ★★★★★ |
| 19 | `features` | `uclaw-features` | M3 配套 ★★★★ |
| 20 | `utils/oss` | `uclaw-utils-oss` | 可选 ★★ |

### 17.3 第三档：选择性复制（2 个 + ansi-escape 跳过）

| # | codex 路径 | uclaw 路径 | Phase |
|---|---|---|---|
| 21 | `apply-patch` | `uclaw-apply-patch` | M3 配套，~2 周 |
| 22 | `git-utils` | `uclaw-git-utils` | M3 配套，~1.5 周 |
| 23 | `ansi-escape` | — | **跳过**（uclaw 不用 ratatui）|

### 17.4 风险与缓解

| 风险 | 缓解 |
|---|---|
| Apache-2.0 NOTICE 遗漏 | CI lint 每个 `uclaw-*` 子 crate 必须有 SPDX header + NOTICE 自动校验 |
| codex 上游更新 drift | NOTICE 固化 commit hash + 季度对比 |
| workspace 改造破坏 src-tauri | T2 后必跑 `cargo build` + `cargo test --workspace` |
| 外部 deps 版本冲突 | `workspace.dependencies` 统一管理 + 定期 `cargo tree --duplicates` |
| 编译时间增加 | workspace 并行编译，实测影响 < 10% |

---

## 附录 A：迁移注册表 <a id="附录-a迁移注册表"></a>

uclaw 实际最新 V 号为 V40（MCP audit table 已预订）。建议 V41+ 占位：

| V | 内容 | 关联 Milestone |
|---|---|---|
| V41 | `agent_session_spawn_edges` | M8 |
| V42 | `thread_goals` | M1 |
| V43 | `installed_plugins` | M3 |
| V44 | `task_events_rollout` | M1 |
| V45 | `agent_sessions.parent_thread_id + spawn_depth` | M8 |
| V46 | `automation_dead_letter` | M3 |
| V47 | `agent_jobs + agent_job_items` | M3 + M8 |
| V48 | `personality_profile_id` + `personality_evolved` columns | M2 |
| V49 | logs DB 拆分 | M4 |
| V50 | cost DB 拆分 | M4 |
| V51 | `hook_configs` | M5 |
| V52 | `evolution_promotions` | M7 |
| V53 | `world_projection_snapshots` | M4 |
| V54 | `capability_cards_cache` | M3 |
| V55 | `cost_records` 加 cached + reasoning 列 | M1 |

**每次开 PR 必须按 CLAUDE.md *Active migration registry* 流程查实时号**。

---

## 附录 B：测试矩阵 <a id="附录-b测试矩阵"></a>

| Milestone | Unit | Integration | E2E | Snapshot | Harness | Bench | Manual |
|---|---|---|---|---|---|---|---|
| M1 | ✓ | ✓ | ✓ | — | ✓ | ✓ | — |
| M2 | ✓ | ✓ | ✓ | ✓ | ✓ (Prompts subject) | ✓ (核心) | ✓ |
| M3 | ✓ | ✓ | ✓ | ✓ | ✓ (Tools/Plugins) | ✓ | ✓ |
| M4 | ✓ | ✓ | ✓ | ✓ | — | — | ✓ |
| M5 | ✓ | ✓ | ✓ | ✓ | ✓ (Hooks/Permissions) | — | ✓ |
| M6 | ✓ | ✓ | ✓ | ✓ | ✓ (Browser) | ✓ | ✓ |
| M7 | ✓ | ✓ | ✓ | ✓ | ✓ (全 12 subject) | — | ✓ |
| M8 | ✓ | ✓ | ✓ | ✓ | ✓ (Coordinator) | — | ✓ |
| M9 | ✓ | ✓ | ✓ | ✓ | ✓ (Tasks) | ✓ | ✓ |

---

## v2.0 文档版本变更说明

**v1.2 → v2.0 重大变更**：

1. 全面重写 —— 全部 Phase 重映射到 ADR Milestone M0-M9
2. License 决策落地（Apache-2.0 + Phase 0.5-T1 具体执行）
3. 承认 uclaw 现状（harness/learning/browser v2 等已就位）
4. 总时长从 15-18 月（3 人）修正为 **8-11 月（3 人，M1-M8 核心）/ 13-15 月（含 M9 远期）**
5. 新增 13-event HookBus / Capability Cards / WorldProjection / Evolution Factory pipeline / Isolation profiles 任务
6. 每章末尾按 ADR §18 11 问回答（在对比文档对应章节）

**对应对比文档**：v1.2 → v2.0 重写为 ADR Agent OS v2 北极星对齐版（第 1-23 章 + 附录）。

**总价值**：完成 M1-M8 后，uclaw 在 ADR 11 层运行时模型上与 Agent OS v2 北极星完全对齐，**月度成本下降 60-70%**，输出质量主观评分 +1.5/5，长会话 token 节约 60-75%，并保留 uclaw 独有的 harness 评估、learning Sprint 1 自学习、browser v2 自治浏览、Symphony workflow、IM 集成、proactive scenarios 等优势。

---

# 第二部分：Crate 集成任务清单（v2.1 新增）

> **本部分目的**：v2.0 留下的最大缺口 —— **17 个复制过来的 codex crate 必须真实被使用，不能成为孤儿**。本部分把每个 crate 的集成动作映射到具体 Milestone 任务内，与对比文档 §24 一一对位。

---

## 18. Crate 集成任务嵌入（M0-M9 全 Milestone）<a id="18-crate-集成任务"></a>

### 18.1 集成总原则

**3 条硬性规则**（详见对比文档 §24.5）：

1. **30 天落地**：每 crate 复制后 30 天内必有第一个生产使用者
2. **M2-H L1 强制**：所有新增 tool handler PR 必须 import `uclaw_utils_output_truncation`
3. **M0 Sweep**：Phase 0.5 收尾后立即做 `uclaw_utils_home` 全仓 sweep

### 18.2 Phase 0.5 收尾追加任务

#### Phase 0.5-T6：`uclaw_utils_home` 全仓 sweep（0.5 天）★

**目标**：把所有 `dirs::home_dir().unwrap().join(".uclaw")` 散落代码一次性替换为 `uclaw_home()` 单一函数。

**实测可替换位置**（grep 结果）：

| 文件 | 行号 | 现状 | 替换为 |
|---|---|---|---|
| `tauri_commands.rs` | 872 | `dirs::home_dir()` | `uclaw_utils_home::uclaw_home()` |
| `tauri_commands.rs` | 943 | `dirs::home_dir().unwrap().join(".uclaw")` | `uclaw_utils_home::uclaw_home()` |
| `tauri_commands.rs` | 4571-4573 | `dirs::home_dir().unwrap().join(".uclaw").join("skills")` | `uclaw_utils_home::uclaw_skills_dir()` |
| `tauri_commands.rs` | 11857-11931 | `.uclaw` 硬编码 | `uclaw_home()` |
| `memubot_config.rs` | 741, 764 | data_dir 拼接 | `uclaw_home()` |

**Commit 1 — 定义 `uclaw_utils_home::uclaw_home()` API**

仿 codex `codex_utils_home_dir::codex_home()`：

```rust
// src-tauri/uclaw-utils-home/src/lib.rs (在已复制基础上扩展)
pub fn uclaw_home() -> AbsolutePathBuf {
    dirs::home_dir()
        .expect("home dir must exist")
        .join(".uclaw")
        .try_into()
        .expect("valid absolute path")
}

pub fn uclaw_skills_dir() -> AbsolutePathBuf { uclaw_home().join("skills") }
pub fn uclaw_sessions_dir() -> AbsolutePathBuf { uclaw_home().join("sessions") }
pub fn uclaw_plugins_dir() -> AbsolutePathBuf { uclaw_home().join("plugins") }
pub fn uclaw_secrets_dir() -> AbsolutePathBuf { uclaw_home().join("secrets") }
pub fn uclaw_logs_dir() -> AbsolutePathBuf { uclaw_home().join("logs") }
```

**Commit 2 — sed sweep 替换**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri
# 半自动 sed + 手工审查
grep -rln "dirs::home_dir\(\).*\.join(\"\.uclaw\")" src/ | while read f; do
  # 加 use uclaw_utils_home::uclaw_home;
  # 替换调用
done
# 手工 review 每处替换
```

**Commit 3 — CI lint**

新增 lint：禁止 `src-tauri/src/` 出现 `dirs::home_dir` 字样（必走 uclaw_utils_home）。

**DoD**：
- [ ] 5+ 处替换完成
- [ ] `grep "dirs::home_dir" src-tauri/src/` 返回空
- [ ] CI lint 工作

### 18.3 M1 Crate 集成（嵌入 M1 任务）

#### M1-T2 增量：使用 `uclaw-async-utils::OrCancelExt`

新增 `agent/task.rs::SessionTask` 实现时，**所有取消点**使用 `.or_cancel(&token)` 而不是 `tokio::select! { _ = token.cancelled() ... }`：

```rust
// 之前
tokio::select! {
    _ = cancellation_token.cancelled() => return None,
    res = task_future => res,
}

// 之后（一行 + 类型化错误）
match task_future.or_cancel(&cancellation_token).await {
    Ok(res) => Some(res),
    Err(CancelErr::Cancelled) => None,
}
```

适用：codex_delegate 等价（M8）、所有 SessionTask::run、browser/agent_loop 内的取消点。

#### M1-T6 增量：使用 `uclaw-utils-string::approx_token_count`

在 TokenUsage 估算和 6 维 token 直方图代码中，统一使用 `uclaw_utils_string::approx_token_count`。

#### M1-T7 增量：使用 `uclaw-utils-elapsed::format_duration`

UI 端 turn 时长展示统一用 `format_duration`。`harness/episode.rs` duration_ms 渲染同样。

#### M1-T8 增量：使用 `uclaw-utils-stream` 重构 SSE

**强烈推荐重构**：

- `llm/providers/anthropic.rs:521-659` 自实现 SSE state machine → `uclaw_utils_stream::SseParser`
- `llm/providers/openai.rs:253+` 同上
- 其他 provider 同步

**Commit 1**：定义 `uclaw_utils_stream::SseParser` 接口（基于已复制 stream-parser）

**Commit 2**：anthropic.rs 重构（保留分层超时 connect=15s/stall=45s/total=120s）

**Commit 3**：openai.rs 重构

**Commit 4**：其他 provider 同步

**Commit 5**：回归测试 + 流式响应 e2e 验证

**DoD**：
- [ ] 所有 LLM provider 共用 `uclaw_utils_stream::SseParser`
- [ ] 0 个手写 SSE state machine 剩留

#### M1-T5 增量：使用 `uclaw-file-search`

Rollout JSONL 文件发现 + 列表 → 用 `uclaw_file_search::find_files` 替代 ad-hoc walkdir。

### 18.4 M2 Crate 集成（最大集成密度）

#### M2-A 增量：使用 `uclaw-utils-template`

baseline.md 12 block 重写时，所有占位符必须用 `{{ name }}` Template 语法：

```rust
// 之前
let prompt = format!("{}\n{}\n{}", BASE, user_section, env_context);

// 之后
const TEMPLATE: &str = include_str!("templates/baseline.md");
static BASELINE: LazyLock<Template> = LazyLock::new(|| {
    Template::parse(TEMPLATE).expect("baseline template must parse")
});

let prompt = BASELINE.render([
    ("user_section", &user_section),
    ("env_context", &env_context),
    ("cwd", cwd),
])?;
```

**LazyLock 编译时校验** —— 模板错直接 panic，避免运行时静默。

#### M2-B 增量：使用 `uclaw-file-watcher`

UCLAW.md 项目级指令注入要求 hot reload：

```rust
// agent/context/project_doc.rs
use uclaw_file_watcher::{FileWatcher, FileWatcherEvent, WatchPath};

let watcher = FileWatcher::new()?;
watcher.subscribe(WatchPath::recursive(project_root, "UCLAW.md")).await?;
// 监听到变更 → 触发 ContextManager.reference_context_item 清空 → 下轮全量重注入
```

**同步统一**：把 `skills.rs`、`mcp.rs` 等模块的 file watch 全部归入 `uclaw_file_watcher`，避免每个模块各起 `notify`。

#### M2-C 增量：使用 `uclaw-utils-cache`

ContextFragment 解析 + AGENTS.md 等价物缓存：

```rust
// agent/context/agents_md_cache.rs（新增）
use uclaw_utils_cache::{BlockingLruCache, sha1_digest};

static AGENTS_MD_CACHE: LazyLock<BlockingLruCache<String, Arc<ParsedAgentsMd>>> =
    LazyLock::new(|| BlockingLruCache::new(NonZeroUsize::new(64).unwrap()));

pub async fn parse_or_cached(content: &str) -> Arc<ParsedAgentsMd> {
    let key = sha1_digest(content.as_bytes());
    AGENTS_MD_CACHE.get_or_insert_with(&key, || Arc::new(parse(content))).await
}
```

**同步替换** `automation/filters.rs:7` REGEX_CACHE：

```diff
- static REGEX_CACHE: Lazy<Mutex<HashMap<String, Regex>>> = ...;
+ static REGEX_CACHE: LazyLock<BlockingLruCache<String, Regex>> =
+     LazyLock::new(|| BlockingLruCache::new(NonZeroUsize::new(256).unwrap()));
```

#### M2-E 增量：Template 引擎全面替代 format!

scan & sweep 所有 `agent/prompts/` 字面量拼接：

```bash
grep -rn 'format!.*\\n' src-tauri/src/agent/prompts/ | review
```

逐个替换为 `.md` + `include_str!` + `Template::parse`。

#### M2-F 增量：使用 `uclaw-utils-string`

7 个 Context Tools 内部使用：

```rust
// context.read handler 实现
use uclaw_utils_string::{approx_token_count, take_bytes_at_char_boundary};

let content = read_artifact(&ref).await?;
let budget = invocation.budget.unwrap_or(4_000);
let truncated = if approx_token_count(&content) > budget {
    take_bytes_at_char_boundary(&content, budget * 4)
} else {
    &content
};
```

#### M2-H L1 增量：使用 `uclaw-utils-output-truncation`（**M2 主线**）

**所有 13 个 builtin tool handler** 必须改造：

```rust
// agent/tools/builtin/shell.rs（示例）
use uclaw_utils_output_truncation::{formatted_truncate_text, TruncationPolicy};

let output = run_command(...).await?;
let policy = TruncationPolicy::Tokens(
    self.config.tool_output_budgets
        .get("shell")
        .copied()
        .unwrap_or(8_000)
);
let truncated = formatted_truncate_text(&output.stdout, policy);
ToolOutput::Text(truncated)
```

**13 个 handler 批量改造清单**：

| Handler | 默认 budget tokens | PR commit |
|---|---|---|
| `ask_user.rs` | N/A（用户输入不截）| skip |
| `edit.rs` | 4_000（diff 输出）| commit 1 |
| `exit_plan_mode.rs` | N/A | skip |
| `file.rs` | 4_000 | commit 2 |
| `load_skill.rs` | 2_000 | commit 3 |
| `plan.rs` | N/A | skip |
| `plan_mode.rs` | N/A | skip |
| `search.rs` | 4_000 | commit 4 |
| `self_eval.rs` | 2_000 | commit 5 |
| `shell.rs` | 8_000 | commit 6 |
| `skill_search.rs` | 2_000 | commit 7 |
| `web.rs` | 6_000 | commit 8 |
| MCP wrapper | 5_000 | commit 9 |
| memU wrapper | 3_000 | commit 10 |

#### M2-H L7 增量：使用 `uclaw-utils-sleep`

长 compaction 任务期间防 OS sleep：

```rust
// agent/compact/local.rs / remote.rs
use uclaw_utils_sleep_inhibitor::SleepInhibitor;

let _inhibitor = SleepInhibitor::new(true);
// 跑压缩
// inhibitor drop 时自动释放
```

### 18.5 M3 Crate 集成（Capability Mesh 配套）

#### M3-T2 增量：使用 `uclaw-utils-fuzzy`

```rust
// agent/skills_manifest.rs 或 capabilities/tool_registry.rs
use uclaw_utils_fuzzy::fuzzy_match;

pub fn search_tools(query: &str, all: &[ToolName]) -> Vec<(ToolName, i64)> {
    all.iter()
        .filter_map(|t| {
            fuzzy_match(t.as_str(), query).map(|(score, _)| (t.clone(), score))
        })
        .collect()
}
```

UI 通过 Tauri command 暴露 + slash command palette / skill picker / @ mention 都用。

#### M3-T3 增量：使用 `uclaw-utils-readiness`（**uclaw 首个采用者**）

```rust
// services/manager.rs
use uclaw_utils_readiness::{Readiness, ReadinessSignal};

#[async_trait]
impl Readiness for GbrainService {
    async fn signal(&self) -> ReadinessSignal { ... }
}

// ServiceManager 启动期间统一展示
let signals = manager.collect_readiness().await;
for sig in signals {
    println!("{}: {}", sig.name, sig.status);
}
```

UI 启动 splash 显示各 service 状态。

#### M3-T4 增量：使用 `uclaw-utils-pty` + `uclaw-utils-image` + `uclaw-file-search`

新 builtin tools：

```rust
// agent/tools/builtin/unified_exec.rs
use uclaw_utils_pty::PtySession;
// PTY 模式长进程 + stdin 写入

// agent/tools/builtin/view_image.rs
use uclaw_utils_image::{encode_base64_with_mime};

// agent/tools/builtin/file_search.rs
use uclaw_file_search::FileSearcher;
```

#### M3-T6 增量：使用 `uclaw-utils-template` + `uclaw-utils-json-toml`

Plugin manifest 解析：

```rust
// capabilities/plugin_manifest.rs
use uclaw_utils_template::Template;
use uclaw_utils_json_to_toml::convert;

// manifest.yaml 中支持 {{ env.HOME }} 等占位符
// 跨格式（YAML/TOML/JSON）支持
```

#### M3-T9 增量：使用 `uclaw-file-watcher`

MCP server 注册到 `uclaw_file_watcher` 监听 config 变更，触发 hot reload。

### 18.6 M5 Crate 集成

#### M5-T1 增量：使用 `uclaw-async-utils`

HookBus hook 执行 timeout：

```rust
// safety/hook_bus.rs
use uclaw_async_utils::OrCancelExt;

let timeout_token = CancellationToken::new();
// 设置 timeout
match hook_command.spawn().or_cancel(&timeout_token).await {
    Ok(result) => ...,
    Err(CancelErr::Cancelled) => HookResult::Timeout,
}
```

### 18.7 M6 Crate 集成

#### M6-T2 增量：使用 `uclaw-utils-pty` + `uclaw-utils-image` + `uclaw-utils-sleep`

LocalChromiumProvider 适配：
- 浏览器截图统一走 `uclaw_utils_image::encode_base64_with_mime`
- 长 browser task 期间 `SleepInhibitor`
- 如果将来需要 headless terminal 控制 → `uclaw_utils_pty`

### 18.8 M7 Crate 集成

#### M7-T3 增量：使用 `uclaw-utils-template`

prompt_patch candidate 类型用 Template 描述：

```rust
// evolution/candidate/prompt_patch.rs
pub struct PromptPatchCandidate {
    template_diff: Template,  // 描述要 patch 的 prompt 部分
    ...
}
```

#### M7-T4 增量：使用 `uclaw-utils-sleep` + `uclaw-utils-output-truncation`

Simulation 阶段长跑期间防 sleep + candidate 描述截断。

### 18.9 持续集成（跨全 Milestone）

#### `uclaw-utils-abs-path` 渐进迁移

新规则：所有新代码的 path 参数**必须** `AbsolutePathBuf`，不允许 `PathBuf`。

旧代码 sweep：分 7 批，每批一个 module（safety / automation / browser / workspace / files_rail / git / agent）。每批一个独立 PR。

#### `uclaw-utils-path` 渐进迁移

类似 abs-path。所有 `canonicalize` 散落替换为 `uclaw_utils_path::normalize_for_path_comparison`。

### 18.10 Crate 集成 DoD 矩阵

每个 Milestone 完成 DoD 必含对应 crate 集成项：

| Milestone | Crate 集成 DoD |
|---|---|
| Phase 0.5 收尾 | uclaw-utils-home 全仓替换 `dirs::home_dir().unwrap().join(".uclaw")` |
| M1 | async-utils 用于所有 SessionTask cancel + stream 替换 ≥1 个 provider SSE |
| M2-A/B/C/E/F | template 用于所有 prompt + cache 用于 fragment + file-watcher 监听 UCLAW.md |
| M2-H L1 | output-truncation 用于 ≥10 个 tool handler |
| M2-H L7 | sleep-inhibitor 用于 compaction |
| M3-T2 | fuzzy 用于 tool/skill search |
| M3-T3 | readiness 用于 ≥3 个 service |
| M3-T4 | pty + image + file-search 各新增 1 个 builtin tool |
| M3-T6 | template + json-toml 用于 plugin manifest |
| M5 | async-utils OrCancelExt 全 hook 执行 |
| M6 | pty / image / sleep 用于 browser |
| M7 | template / sleep 用于 evolution pipeline |
| 持续 | abs-path / path 渐进迁移完成度 ≥80% |

---

## 19. 跨文档一致性核查规则（v2.1 新增）<a id="19-一致性核查"></a>

为避免"二次真相"（同概念在两份文档定义不一致），本节列出**所有需要在两份文档同步的关键事实**。

### 19.1 单一真相清单（与对比文档 §25 同步）

| 概念 | 唯一定义位置 | 实施方案引用规则 |
|---|---|---|
| ADR 11 层模型 | ADR `2026-05-20-uclaw-agent-platform-north-star.md` §6 | §1 引用而非重述 |
| ADR 9 Milestone | ADR §16 | 完全沿用 M0-M9 命名 |
| License = Apache-2.0 | 对比文档 §3.1 | §2.2 P0.5-T1 引用 |
| 17 crate 列表 | 对比文档 §17.2 + §24.1 | §17 表对齐 |
| 13 个 HookEventName | 对比文档 §10.1 + ADR §10.1 | §7.2 M5-T1 实现 |
| 7 个 Context Tools | 对比文档 §7.3 + ADR §8.3 | §4.2 M2-F 实现 |
| 9 个 Candidate Type | 对比文档 §11.2 + ADR §13.1 | §9.2 M7-T3 实现 |
| 7 个 Isolation Profile | 对比文档 §10.3 + ADR §10.3 | §7.2 M5-T4 实现 |
| 5 大 Registry | 对比文档 §8.2 + ADR §9.2 | §5.2 M3 实现 |
| 迁移注册表 V41-V55 | 本文档 §17（附录）| 对比文档 §23（附录 C）同步 |

### 19.2 已修正的潜在矛盾点

| 矛盾点 | v2.1 修正 |
|---|---|
| 总时长（v1.1 15-18 月 vs v2.0 8-10 月 vs 实施方案 6-8 月） | **v2.1 单一权威值**：M1-M8 约 **8-11 月**（中位 9.5）；含 M9 约 **13-15 月**（中位 14）。3 人团队 |
| V40 占用（v1.0 spawn_edges vs 实际 mcp_audit） | V40 = mcp_audit；spawn_edges 占 V41 |
| Crate 数（v1.2 17 vs v2.0 17+3 vs 实际复制 17+1） | Phase 0.5 实做 17+1 = 18 个；v2.1 文档明确 |
| Prompt baseline 字数（估 vs 实测） | ~10K 字 / ~2K tokens / 12 个 block |

### 19.3 更新流程

未来更新两份文档时：

1. **更新 ADR 任何条目** → 两份文档同步更新引用
2. **更新 17 crate 列表** → 对比文档 §17.2 + §24.1 + 本文档 §17 / §18 三处同步
3. **更新 Milestone 任务** → 本文档 §M*-T* 为唯一源；对比文档仅引用
4. **更新 Crate 集成动作** → 本文档 §18 + 对比文档 §24 必须同步
5. **更新 License** → 对比文档 §3 为唯一源；本文档 §2 仅引用执行步骤
6. **更新 codex 实地源码引用** → 必须 grep 验证后再写入

### 19.4 ADR Baseline 对齐复核

本实施方案 Phase 全部对位 ADR Milestone：

| 本文档 Phase / 章 | ADR Milestone | 对齐状态 |
|---|---|---|
| §2 M0 (ADR Lock + Phase 0.5) | ADR M0 | ✅ |
| §3 M1 (Runtime Contracts) | ADR M1 | ✅ |
| §4 M2 (Context Fabric) | ADR M2 | ✅ |
| §5 M3 (Capability Mesh) | ADR M3 | ✅ |
| §6 M4 (World Projection) | ADR M4 | ✅ |
| §7 M5 (Policy Hooks + Isolation) | ADR M5 | ✅ |
| §8 M6 (Browser Provider) | ADR M6 | ✅ |
| §9 M7 (Evolution Factory) | ADR M7 | ✅ |
| §10 M8 (Teams v1) | ADR M8 | ✅ |
| §11 M9 (Cluster v1) | ADR M9 | ✅ |
| §18 Crate 集成 | 服务于 M0-M9（横切） | ✅ |

**所有 Phase 命名、任务编号、依赖关系均统一在 ADR Agent OS v2 北极星之下**。无独立 Milestone、无并行 Phase 轴、无替代 lifecycle —— 100% baseline 对齐。

---

## v2.1 文档版本变更说明

**v2.0 → v2.1 关键变更**：

1. **新增第 18 章 Crate 集成任务清单** —— 17 个 codex crate 在各 Milestone 内的具体集成动作，确保每个 crate **真实使用** 而非孤儿
2. **新增 Phase 0.5-T6** —— `uclaw_utils_home` 全仓 sweep（实测 5+ 处可替换）
3. **新增 M1-T8 增量** —— `uclaw_utils_stream` 重构 LLM provider SSE state machine（替换 anthropic.rs:521-659 + openai.rs:253+ 自实现）
4. **新增 M2-H L1 13 个 tool handler 批量改造清单**（含每个 handler 默认 budget）
5. **新增第 19 章 跨文档一致性核查规则** —— 单一真相清单 + 更新流程 + ADR baseline 对齐复核

**对应对比文档变更**：v2.0 → v2.1 新增 §24 crate 集成映射 + §25 跨文档一致性核查。

**核心价值**：完成 v2.1 描述的工作后，uclaw 中：

- **0 个 `dirs::home_dir().unwrap().join(".uclaw")` 散落**
- **0 个手写 SSE state machine**
- **0 个 `format!()` 拼接 prompt**
- **0 个 tool handler 不截断输出**
- **0 个 ad-hoc `tokio::select! { cancel }`**
- **0 个孤儿 crate**

**统一标准**：所有改进、Phase、任务、Crate 集成均在 ADR Agent OS v2 北极星之下，与对比文档严格对位，无二次真相。

---

# 第三部分：v2.2 自审驱动的任务补充（v2.2 新增）

> **本部分背景**：v2.1 完成后，用户委托做严格自审。3 个 subagent 并行审查后发现：
> - **文档对 ADR 忠诚度**：8/10（已通过对比文档 §26.3 中 7 处修复达 9.5/10）
> - **codebase 对 ADR 实现度**：**22%**（远低于 v2.0 估计）
> - **实施风险等级**：MEDIUM-HIGH，72% 成功率中位
>
> 本部分把审查发现的 P0/P1/P2 修复任务**真正写入对应 Milestone**，与对比文档 §26 严格对位。

---

## 20. Audit 驱动的 Milestone 任务补充 <a id="20-audit-补充"></a>

### 20.1 Phase 0.5 补充任务

#### Phase 0.5-T7：memory_graph 防御性冻结（**P0，0.5 天**）

**触发**：对比文档 §13.2 + ADR §11.2 明文规定 memory_graph 冻结。当前 codebase 仍允许写入，是 R-3 风险源（CRITICAL）。

**任务**：

```rust
// src-tauri/src/memory_graph/mod.rs 顶部增加
#[cfg(not(feature = "uclaw_memory_graph_legacy_migration"))]
pub fn write_entity_page(...) -> ! {
    panic!(
        "memory_graph is FROZEN (ADR §11.2). Use gbrain instead. \
         If this is migration code, enable `uclaw_memory_graph_legacy_migration` feature."
    );
}
```

**Commit 1** — panic 防御 + feature flag
**Commit 2** — CI lint：检查新 PR 不允许出现 `memory_graph::write*` 字样（白名单仅迁移代码）
**Commit 3** — 文档：docs/THIRD_PARTY.md 加 "Memory Graph Frozen" 段

**DoD**：
- [ ] panic 防御就位
- [ ] CI lint 工作
- [ ] 任何意外写入立即 panic（而非静默漂移）

### 20.2 M1 补充任务（Runtime Contracts 完整化）

#### M1-T1 增量：AutonomyLevel + RiskClass 完整定义（**P0**）

**触发**：subagent 2 发现 AutonomyLevel L0-L6 enum 完全不存在（0/10 实现度）。

**Commit**：在 `src-tauri/src/runtime/contracts.rs` 加：

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub enum AutonomyLevel { L0, L1, L2, L3, L4, L5, L6 }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskClass { Low, Medium, High, Restricted }

#[derive(Debug, Clone)]
pub struct AutonomyResolver {
    cap_profile_registry: Arc<CapabilityProfileRegistry>,
    provider_health: Arc<ProviderHealthRegistry>,
}

impl AutonomyResolver {
    pub fn resolve_effective(
        &self,
        requested: AutonomyLevel,
        task: &TaskSpec,
    ) -> (AutonomyLevel, Vec<DowngradeReason>) {
        // 1. CapabilityProfile.autonomyMax cap
        // 2. risk_class High → ≤ L2
        // 3. provider health → ≤ L1
        // ...
    }
}
```

**DoD**：
- [ ] L0-L6 完整 enum + Ord trait
- [ ] AutonomyResolver 含 3 类下调规则
- [ ] task.spawn_task 入口 assert autonomy_target 已声明

#### M1-T2 增量：agentic_loop 状态归属审计（**R-1 + R-6 缓解，P0**）

**触发**：subagent 3 标记 agentic_loop.rs 882 行重构是 R-1 风险（HIGH）+ R-6（state 跨 turn 泄漏，HIGH）。

**Commit 1** — 状态审计文档：在 `docs/superpowers/specs/2026-05-20-agentic-loop-state-audit.md` 列出 agentic_loop 所有 local var，标记 per-turn / per-session：

| 变量 | 当前归属 | 目标归属 | 备注 |
|---|---|---|---|
| iteration | local | TurnContext | per-turn |
| truncation_count | local | TurnContext | per-turn，**必须 reset** |
| consecutive_tool_intent_nudges | local | TurnContext | per-turn，**必须 reset** |
| messages | reason_ctx | ContextManager | per-turn 累积 |
| partial_code_buffer | reason_ctx | TurnContext | per-turn |
| cost_record | DB | Session.cost_store | per-session |
| skills_recall_history | proactive | Session | per-session |
| ... | ... | ... | ... |

**Commit 2** — SessionTask 包装时**显式 reset** per-turn 字段：
```rust
impl RegularTask {
    pub async fn run(self: Arc<Self>, session: Arc<SessionContext>, ctx: Arc<TurnContext>, ...) {
        debug_assert!(ctx.truncation_count.load(Ordering::Acquire) == 0);
        debug_assert!(ctx.consecutive_tool_intent_nudges.load(Ordering::Acquire) == 0);
        // ...
    }
}
```

**Commit 3** — 测试：10 turn loop，每 turn 中段 inject 不同消息，verify per-turn state 完全独立。

**DoD**：
- [ ] 状态审计文档完整
- [ ] debug_assert 守卫到位
- [ ] 跨 turn 污染测试通过

### 20.3 M2 补充任务（Context Fabric）

#### M2-H 增量：双重压缩状态机（**R-7 缓解，P0**）

**触发**：subagent 3 标记 R-7 —— Context Fabric 三档 compaction + 现有 `compress_context_if_needed` 可能冲突。

**Commit 1** — 定义压缩状态机：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionState {
    Idle,                   // context 完整未压缩
    LegacyCompacted,        // 走 compress_context_if_needed
    StructuredFolded,       // 走 M2-G 8 字段 fold
    DiffInjected,           // 走 M2-D diff-based
}

impl ContextManager {
    pub fn transition_to(&mut self, from: CompressionState, to: CompressionState) -> Result<()> {
        // 禁止 LegacyCompacted → DiffInjected 直接跳转
        // ...
    }
}
```

**Commit 2** — M2-D 依赖 M2-A 完成才能启动（lockfile-style 任务依赖）

**Commit 3** — Integration test：50 turn full conversation，每 turn assert `context_window` 占用率单调（除 compaction 边界）+ ≤ 95%

**DoD**：
- [ ] 状态机定义 + 转换 guard
- [ ] M2-D 启动前 M2-A baseline 必须就绪
- [ ] 50-turn test 通过

#### M2-G 增量：8 字段 StructuredFold 类型化（**P1**）

**触发**：subagent 1 发现 §M2-G 未给出 Rust 类型定义。

**Commit** — 在 `src-tauri/src/agent/compact/` 加：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredFold {
    pub facts: Vec<FactWithEvidence>,
    pub decisions: Vec<DecisionWithRationale>,
    pub unresolved_questions: Vec<String>,
    pub evidence_refs: Vec<ArtifactRef>,
    pub failed_attempts: Vec<FailedAttempt>,
    pub active_constraints: Vec<Constraint>,
    pub next_actions: Vec<String>,
    pub rollback_points: Vec<CheckpointRef>,
}
```

**DoD**：
- [ ] StructuredFold 类型 + serde
- [ ] M2-G prompt template 引用此 schema

### 20.4 M3 补充任务（Capability Mesh）

#### M3-T1 增量：5 Registry DAG 初始化检查（**R-8 缓解，P0**）

**触发**：subagent 3 标记 R-8 —— 5 个 Registry 之间有循环依赖（CRITICAL，可能 panic 启动）。

**Commit 1** — DAG 分层初始化：

```rust
// src-tauri/src/capabilities/init.rs
pub async fn init_capability_mesh() -> Result<CapabilityMesh> {
    // Layer 0: 并行
    let (tools, providers) = tokio::join!(
        ToolRegistry::init(),
        ProviderRegistry::init(),
    );

    // Layer 1: 依赖 Layer 0
    let plugins = PluginRegistry::init(&tools?, &providers?).await?;

    // Layer 2: 依赖 Layer 0 + Layer 1
    let profiles = CapabilityProfileRegistry::init(&tools?, &plugins).await?;

    // Layer 3: 依赖 Layer 0 + Layer 2
    let workers = WorkerRegistry::init(&providers?, &profiles).await?;

    Ok(CapabilityMesh { tools, providers, plugins, profiles, workers })
}
```

**Commit 2** — `once_cell::sync::Lazy<OnceCell<Arc<...>>>` 而非 Mutex（避免 deadlock）

**Commit 3** — CI build script：parse 源码 + 建 dep DAG + cycle detection（fail build on cycle）

**DoD**：
- [ ] 4 层初始化清晰
- [ ] CI 拒绝循环依赖

### 20.5 M5 补充任务（Hooks + Isolation）

#### M5-T1 增量：HookBus 性能预算（**R-5 缓解，P1**）

**触发**：subagent 3 标记 R-5 —— 13 个 hook 嵌入 agent loop 可能拖慢 turn（P99 +60%）。

**Commit 1** — HookBus 设计含性能 budget：每 hook < 10ms，全 13 hooks < 50ms。

```rust
pub struct HookBus {
    hooks: HashMap<HookEventName, Vec<RegisteredHook>>,
    per_hook_budget_ms: u64,        // 10
    total_per_event_budget_ms: u64, // 50
}

impl HookBus {
    pub async fn dispatch(&self, event: HookEventName, payload: &mut HookPayload) -> HookResult {
        // 每 hook 超 budget 强制 cancel + WARN log
        // 总时间超 budget → 后续 hook 跳过 + ERROR log
    }
}
```

**Commit 2** — Async dispatch 选项：非 blocking hook 收集到 background task

**Commit 3** — Baseline metric + CI compare P50/P99

**DoD**：
- [ ] 性能预算实施
- [ ] benchmark：50 turn 全 hook 开 vs 关，P99 delta ≤ 10%

#### M5-T7 新增：现有 isolation audit table（**R-10 缓解，P1**）

**触发**：subagent 3 标记 R-10 —— 现有 browser session / git worktree / subagent / automation 的 isolation 可能未映射到 M5 profiles。

**Commit 1** — 在 `docs/superpowers/specs/2026-05-20-isolation-audit.md` 列出现有所有隔离点：

| 隔离点 | 现有位置 | M5 IsolationProfile 映射 |
|---|---|---|
| browser session | `browser/context_manager.rs::per_profile` | BrowserSession |
| git worktree | `automation/runtime/activity_runner.rs` | GitWorktree |
| subagent context | （未实现） | SubagentContext |
| automation run ledger | `automation/runtime/service.rs` | AutomationRun |
| ... | ... | ... |

**Commit 2** — M5-T4 IsolationProfile trait 必须 1:1 覆盖上表

**DoD**：
- [ ] audit 完整
- [ ] M5-T4 类型枚举与现有点完全对应

### 20.6 M6 补充任务（BrowserProvider）

#### M6-T1 增量：现有 BrowserContextManager 字段分类（**R-11 缓解，P2**）

**触发**：subagent 3 标记 R-11 —— BrowserContextManager 字段在抽 trait 时归属不清。

**Commit 1** — 字段分类表（设计文档）：

| 字段 | 类别 | 归属 |
|---|---|---|
| session_id / url | trait 方法参数 | BrowserProvider::action(session, url, ...) |
| driver_state | impl struct | LocalChromiumProvider 私有 |
| identity_profile | shared Arc | 跨 provider 共享 IdentityContext |
| perception_cache | impl struct | provider-specific |
| cookie_store | shared Arc | 跨 provider 共享 |
| ... | ... | ... |

**Commit 2** — LocalChromiumProvider impl + snapshot test：现有行为 100% 一致

**DoD**：
- [ ] 字段分类完整
- [ ] snapshot test 通过

### 20.7 M7 补充任务（Evolution Factory）

#### M7-T5 拆分 → M7-T5a 6 类 Forbidden Promotion Detector（**P1**）

**触发**：subagent 1 发现 §M7-T5 未给出 6 类禁止 promotion 的具体检测方法。

**Commit 1** — 在 `src-tauri/src/evolution/promotion.rs` 加 6 个独立 detector：

```rust
pub trait ForbiddenCheck: Send + Sync {
    fn name(&self) -> &'static str;
    fn check(&self, candidate: &Candidate) -> Result<(), ForbiddenReason>;
}

pub struct SilentPermissionWideningDetector;  // 检查 candidate 是否扩大 capability 权限但无 review
pub struct SecretCaptureDetector;              // 检查 candidate 是否含 secret / token
pub struct PromptMutationWithoutRegressionDetector;  // 必须有 regression case
pub struct MemoryWriteWithoutEvidenceDetector;
pub struct ProviderEnablementWithoutUserConfigDetector;
pub struct AutonomyEscalationWithoutPolicyApprovalDetector;
```

**Commit 2** — Promotion pipeline 调用全部 6 个 detector，任一 fail 即拒绝

**Commit 3** — 6 个 detector 每个 ≥ 3 个 unit test

**DoD**：
- [ ] 6 个 detector 实现 + 测试
- [ ] Pipeline 集成

#### M7-T9 新增：GEP ↔ Evolution Factory 显式映射（**P2**）

**触发**：subagent 2 发现现有 GEP 系统（`agent/gep/types.rs`）与 ADR Evolution Factory 无对接。

**Commit** — 在 `src-tauri/src/evolution/gep_adapter.rs` 加适配器：

```rust
impl From<GepCapsule> for SelfImprovementCandidate {
    fn from(capsule: GepCapsule) -> Self { ... }
}
```

GEP 进化产物可作为 Candidate 进入 promotion gate。

### 20.8 M9 补充任务（Cluster v1）

#### M9-T0 新增：WorkerNode + 8 kind 完整 Rust types（**P2**）

**触发**：subagent 1 标记 M9 缺类型定义任务。

**Commit** — 在 `src-tauri/src/workers/node.rs`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerKind {
    Local,
    Subagent { parent: SessionId },
    Worktree { path: PathBuf },
    Remote { endpoint: Url },
    Container { image: String, runtime: String },
    Mobile { platform: String, device_id: String },
    Cloud { provider: String, instance_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerNode {
    pub id: WorkerId,
    pub kind: WorkerKind,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub status: WorkerStatus,
    pub load: WorkerLoad,
    pub policy: PolicySpec,
    pub locality: DataLocalitySpec,
    pub last_heartbeat_at: i64,
}
```

#### M9-T0b 新增：Data Locality Policy 设计（**R-10 + ADR §17 #10 缓解，P2**）

数据本地性策略：remote worker 只收到声明的 context refs，禁止隐式传播。设计文档放 `docs/superpowers/specs/2026-05-20-cluster-data-locality.md`。

### 20.9 跨 Milestone 补充

#### V41-V55 迁移 3-Phase 双写策略（**R-13 缓解，P0**）

**触发**：subagent 3 标记 R-13 —— V41-V55 数据库迁移期间双写可能不一致。

**统一规则**（适用所有新增 V 号 schema 涉及现有表的）：

| Phase | 内容 | 时长 |
|---|---|---|
| Phase 1 (schema) | 建新表/列 + 不写新表 | pre-deployment |
| Phase 2 (dual-write) | 新代码新旧并写 + nightly 对账 | 1 周 |
| Phase 3 (cutoff) | 停止旧写 + 新表唯一真相 | 验证后 |

**Commit** — `src-tauri/src/db/migrations.rs` 每个 V41+ 迁移按此模式实施。SQL trigger 保证原子性。

**应用范围**：V44 task_events_rollout、V45 agent_sessions.parent_thread_id、V55 cost_records.cached_input_tokens 等所有涉及现有表的迁移。

### 20.10 用户感知优先级（精简自 subagent 3）

按用户感知 + R-1/R-7/R-13 风险窗口排序：

| 序 | 任务 | 触发 milestone | 用户感知 ROI |
|---|---|---|---|
| 1 | Phase 0.5-T1（License）+ T2-T4（crate copy）+ **T6 home sweep**（替换 5+ 处）+ **T7 memory_graph freeze** | Phase 0.5 | ★★★★★ |
| 2 | M2-H L1 TruncationPolicy 13 handler 批量改（**token 立竿见影**）| M2-H | ★★★★★ |
| 3 | M2-H L2 ToolExposure | M2-H | ★★★★ |
| 4 | M2-A baseline 12 block 重写 + UCLAW.md | M2-A/B | ★★★★（输出质量肉眼可见）|
| 5 | M1-T1 IntentSpec/TaskSpec/TaskEvent/**AutonomyLevel** 类型定义 | M1 | ★★★★（架构基石）|
| 6 | M2-D Diff-based context updates（**长会话最大节约**）| M2-D | ★★★★ |
| 7 | M2-H L7 三档 compaction | M2-H | ★★★★ |
| 8 | M3-T1 5 Registry + **DAG 初始化 check** | M3 | ★★★（基础设施）|
| 9 | M5-T1 HookBus 13 events + **性能预算** | M5 | ★★★ |

### 20.11 实施成功率提升策略（精简自 subagent 3）

为把 72% 中位成功率推到 80%+：

1. **M1 前做 agentic_loop 完整 state audit**（1 day，见 M1-T2 增量）
2. **M1 + M2 严格串行**（不并行，降低 R-1+R-7 叠加风险）
3. **每 milestone 留 feature flag**（灾难回滚能力）
4. **M1 exit criteria 外部审查**（请 codex / Claude Code 工程师审 SessionTask 实现）
5. **每 milestone 写应急方案**（最坏情况下 degraded mode 仍可用）

---

## 21. v2.2 自审驱动的版本说明

### v2.1 → v2.2 变更

**直接修复对比文档 v2.1 的 7 处偏差**（应用在对比文档 §10.1 / §11.2 / §13.2 / §26 各处）：

1. ✅ HookEventName 完整 13×3 矩阵
2. ✅ Candidate Type 补第 9 类 "regression harness case"
3. ✅ memory_graph 冻结明文 + panic 防御
4. ✅ ADR §17 11 类风险 × uclaw 缓解矩阵
5. ✅ 17 crate Priority 标注（P0/P1/P2）
6. ✅ 3 类知识（Factual/Evidential/Executable）在代码组织对应
7. ✅ ADR §18 11 问完整回答标准

**实施方案新增 7 类任务**：

| 类别 | 新增任务 | 关联风险 |
|---|---|---|
| Phase 0.5 | T7 memory_graph 防御冻结 | R-3 (CRITICAL) |
| M1 | T1 AutonomyLevel + Resolver | gap 0/10 |
| M1 | T2 agentic_loop state audit | R-1 + R-6 (HIGH) |
| M2 | H 双重压缩状态机 + G StructuredFold 类型 | R-7 (HIGH) |
| M3 | T1 5 Registry DAG check | R-8 (CRITICAL) |
| M5 | T1 HookBus 性能预算 + T7 isolation audit | R-5 + R-10 |
| M6 | T1 BrowserContextManager 字段分类 | R-11 |
| M7 | T5a 6 类 Forbidden Promotion Detector + T9 GEP 适配 | gap |
| M9 | T0 WorkerNode types + T0b Data Locality Policy | gap + ADR §17 #10 |
| 跨 M | V41-V55 3-Phase 双写策略 | R-13 (HIGH) |

**单一真相维护**：本文档（实施方案）的所有 Milestone 任务为唯一源。对比文档 §19 / §26 仅引用代号映射。

**ADR baseline 对齐**：100%。新增任务全部归入 M0-M9 现有结构，无独立 Milestone。

### v2.2 后总评

**文档健康度**：9.5/10（v2.1 8/10 + 7 处修复）
**codebase 真实状态**：22% 实现度（未变，但任务列表已对应每个 gap）
**实施成功率预测**：80%+（v2.2 风险缓解措施 + 严格 M1+M2 串行 + feature flag 工程后）

**接下来唯一权威动作清单**：

**第 1 天**：执行 §20.10 序号 1（Phase 0.5 完整 5 步含 T6 sweep + T7 freeze）
**第 2-4 周**：M1-T1（含 AutonomyLevel）+ M1-T2（state audit）
**第 2-3 月**：M2 完整（含 §20.3 双重压缩状态机 + §20.7 StructuredFold 类型）
**之后**：按 §20.10 + 风险窗口推进 M3-M9

按此路径，**18-24 个月达到 ADR 100% 实现 + 最佳商业可行性**。
