# UCLAW Agent Autonomy Harness Design

> 目标：把 Browser 自治、Agent Loop 自治、Memory/gbrain 自我学习、Skill 提取和 Automation 执行统一到一套可观测、可评估、可恢复、可进化的 UCLAW Harness Runtime 中。

---

## 1. Executive Summary

uClaw 的最终目标不是单点实现一个 browser agent，而是构建一个能长期自治、自我学习、自我进化的 agent product runtime。Browser 能力、Memory OS、gbrain、Skill 提取、Automation、Permissions、Hooks、Tools、Prompts 都应该接入同一个 harness，而不是各自维护一套局部状态、局部日志和局部评估。

本文中的 **Harness** 指的是全局 **UCLAW Agent Harness Control Plane**，不是 Browser 专用测试套件，也不是 System Diagnostics 里的几个按钮。Browser parity、Memory/gbrain eval、Agent control-plane eval、Self-improvement gates 都只是这个全局控制面的 subject adapter 或 early slice。

`browser-use/browser-harness` 补充了一个必须吸收的方向：Harness 不只是“测试 agent 的框架”，也可以是 agent 执行任务时可直接修补的最薄控制层。它的核心思想是：通过一个 CDP websocket 连接真实浏览器，把可编辑 helper workspace 暴露给 agent；当能力缺失时，agent 在任务中写出缺失 helper 或 domain skill，然后继续执行。uClaw 需要吸收这个方向，但不能把它误读成无审计、无权限、无 promotion gate 的生产自修改。

Harness 的职责是把所有 agentic runtime 统一成一条闭环：

`production trace -> episode -> eval -> learning candidate -> gate -> promotion -> regression replay -> production monitoring`

只有这条链路闭合后，才能诚实地说 uClaw 进入“越用越聪明”的阶段。当前实现已经有 runtime core、若干 fixture/eval adapter、System Diagnostics scorecard 入口和 self-improvement gate 雏形；还没有把所有生产事件自动纳入 episode，也没有自动把真实失败转成可 gated 的 skill/prompt/memory/hook/helper/domain-skill candidate。

本设计选择 **reuse-first** 原则：

- 身份与会话管理复用 Playwright / browser-use 的 profile 与 storage state 机制，不自研 cookie/session 系统。
- OCR/视觉识别复用 PaddleOCR、EasyOCR、VLM grounding adapter，不自研 OCR。
- CAPTCHA 处理采用安全边界策略：检测、分类、授权交接、checkpoint resume；第三方真实站点不默认自动破解，只有自有测试环境或明确授权 allowlist provider 才允许自动化处理。
- Harness 参考 OpenHarness 的模块化思想，覆盖 `agent_loop`、`tools`、`skills`、`plugins`、`permissions`、`hooks`、`memory`、`gbrain`、`tasks`、`coordinator`、`prompts`。
- Browser execution 参考 browser-harness 的 thin CDP lane：允许 agent 在受控 workspace 中补写 helper/domain skill，而不是把所有浏览器动作都预先框死在 Playwright 风格 wrapper 里。

核心判断：**自治能力不是让 agent 更大胆，而是让 agent 每一步更可观察、更可验证、更可恢复、更可学习。**

### 1.1 Current Implementation Truth

As of GitHub PR #285:

- Implemented and app-visible:
  - harness runtime core,
  - Browser parity scorecards,
  - Memory/gbrain inventory and recall scorecards,
  - Agent control-plane fixture scorecards,
  - Self-improvement gate fixture reports,
  - System Diagnostics buttons for `All`, `Browser`, `Memory`, `Agent`, and `Self`.
- Implemented but not fully live-looped:
  - `browser_task` production traces are persisted and some long-term browser events write to Memory/gbrain,
  - Browser harness has a real `BrowserAgentLoop` executor path in backend code,
  - System Diagnostics `Browser` currently uses deterministic `BrowserFixtureParityExecutor`, not a live arbitrary website run.
- Not implemented yet:
  - global production trace ingestion across every agent loop/message/tool/automation surface,
  - automatic production run mining into harness episodes,
  - automatic failure-to-candidate generation,
  - promotion that mutates production skills/prompts/hooks/memory/helpers/domain-skills after passing gates,
  - historical trend dashboards and regression delta analysis.

---

## 2. Design Principles

### 2.1 Reuse Before Build

uClaw 不应该单独造身份认证、session、OCR、captcha、eval runner 的大轮子。我们只做统一适配层：

- Browser identity state 使用 Playwright-compatible `storageState`。
- 真实登录态复用系统 Chrome profile 或 browser-use real browser profile。
- Secret 保存使用系统 keyring / macOS Keychain。
- OCR 使用 provider adapter 接 PaddleOCR / EasyOCR / local VLM。
- Harness 复用现有 agent/tool/memory/gbrain 运行时事件，不另起平行真相源。

### 2.2 One Global Harness, Many Subjects

Harness 的 subject 不是 browser，而是所有自治模块。Browser 是最早需要高强度 eval 的 subject，但不是 harness 的边界。

```ts
type HarnessSubject =
  | 'agent_loop'
  | 'agent_message'
  | 'context'
  | 'browser'
  | 'browser_cdp'
  | 'harness_workspace'
  | 'tools'
  | 'skills'
  | 'plugins'
  | 'permissions'
  | 'hooks'
  | 'memory'
  | 'gbrain'
  | 'automation'
  | 'tasks'
  | 'coordinator'
  | 'prompts'
  | 'ui_projection'
  | 'runtime_health'
```

每个 subject 通过 adapter 接入同一套 case、episode、trace、artifact、grader、scorecard。

### 2.2.1 HarnessSubject Coverage Matrix

| Subject | Production source | Eval purpose | Current state | Next required connector |
| --- | --- | --- | --- | --- |
| `agent_loop` | Main chat/agent run lifecycle | Tool/result pairing, final run status, stuck-loop recovery, checkpoint correctness | Fixture adapter exists | Ingest real agent run events into `HarnessEpisode` |
| `agent_message` | User/assistant/system message assembly | Prompt correctness, tool-call visibility, ask_user transcript, model output parseability | Not implemented | Capture normalized message envelope before provider call |
| `context` | Context builder / memory injection / compaction | Grounded context selection, token budget, stale context exclusion | Not implemented | Add context snapshot artifact and grader |
| `browser` | `browser_task`, browser tools, Browser Task Monitor | Browser-use parity, DOM action success, boundary precision, checkpoint resume | Adapter exists; UI uses deterministic fixture executor | Add live local-fixture smoke executor and production run mining |
| `browser_cdp` | Direct CDP connection, target/session/events, screenshots, coordinate input | Thin browser-harness lane, compositor-level actions, stale session recovery, CDP error repair | Not implemented | Add CDP daemon/client adapter with policy-scoped access |
| `harness_workspace` | Editable helpers, domain skills, generated patches | Self-healing helper generation, patch quality, promotion safety | Gate fixture only | Add writable workspace with candidate extraction and diff review |
| `tools` | Tool broker and tool renderers | Tool call/result pairing, crash recovery, output schema correctness | Partially represented through agent fixture | Add generic tool adapter over production tool events |
| `permissions` | Permission mode, ask_user, human boundary | Correct blocking, no silent unsafe action, visible user response | Partially represented through Browser ask_user and agent fixture | Add permission trace subject independent of Browser |
| `automation` | Cron/heartbeat automation runs | Schedule correctness, idempotency, checkpoint, run isolation | Not implemented | Adapter over automation run records |
| `tasks` | Long-running task monitor / checkpoints | Pause/resume correctness, cancellation truth, timeout behavior | Browser-specific only | Generic task episode adapter |
| `memory` | MemoryStore / memU | Write/recall precision, stale recall, correction adoption | Eval adapter exists | Link production memory reads/writes into episodes |
| `gbrain` | gbrain MCP pages/entities | Page grounding, entity consistency, recall traceability | Eval adapter exists | Add durable gbrain artifact refs and failure classification |
| `skills` | Skill extraction and skill registry | Skill usefulness, reuse rate, regression rate | Gate fixture only | Candidate extraction from failed episodes |
| `prompts` | System/developer/policy prompt revisions | Output correctness, safety, token/cost regression | Gate schema only | Prompt patch candidate + replay gate |
| `hooks` | Runtime hooks and policy hooks | Hook side effects, ordering, rollback | Gate schema only | Hook trace adapter |
| `plugins` | MCP/plugin connectors | Availability, tool exposure, schema drift | gbrain/memU only | Generic MCP/plugin health subject |
| `coordinator` | Multi-agent routing / delegation | Correct routing, no duplicate work, merge safety | Not implemented | Coordinator decision trace |
| `ui_projection` | Frontend running state, task monitor, tool-call UI | UI truth matches backend runtime | Not implemented | UI projection snapshot and event consistency grader |
| `runtime_health` | Diagnostics, bridge services, MCP health | Service availability, startup recovery, crash classification | System Diagnostics exists | Convert diagnostics snapshots into harness episodes |

### 2.2.2 Global Ownership Rule

Every agentic subsystem should answer three questions through the same harness interface:

1. **What happened?** A normalized trace event and artifact reference.
2. **Was it good?** A grader result with explicit pass/fail/blocked/partial semantics.
3. **What should change?** A candidate, checkpoint, or rollback-safe no-op.

If a subsystem cannot answer those questions, it is not yet fully harnessed.

### 2.3 Thin Writable Harness Principle

`browser-use/browser-harness` teaches a second ownership rule: a harness should be thin enough that the agent can inspect and repair the missing capability during the task. The upstream pattern is deliberately small:

- protected core maintains the browser/CDP connection,
- editable `agent_helpers.py` contains task-specific helper functions,
- optional domain skills capture durable site knowledge,
- the agent writes missing helper code mid-task,
- the next call immediately uses the new helper.

For uClaw, this becomes a general principle:

| Layer | Browser-harness pattern | uClaw global translation |
| --- | --- | --- |
| Thin execution lane | One CDP websocket to Chrome | Minimal raw capability lane per subsystem: CDP for Browser, broker calls for Tools, scheduler API for Automation, page/entity API for gbrain |
| Writable helper layer | `agent_helpers.py` | `harness_workspace/<subject>/helpers/*` as candidate patches |
| Durable learned playbooks | `domain-skills/<site>/` | subject-scoped skills for browser sites, tool APIs, automation flows, memory/gbrain schemas |
| Mid-task repair | Agent adds missing helper and continues | Agent may create a temporary helper/checkpoint within the same episode |
| Long-term improvement | Commit useful helper/domain skill | Promotion gate decides whether to persist helper, skill, prompt, hook, or memory change |

This is the core distinction:

- **Execution freedom:** during a bounded episode, the agent can write temporary helpers and use raw subsystem APIs to finish the task.
- **Production mutation:** durable changes still require redaction, trace evidence, replay, gate approval, rollback metadata, and user/policy approval when sensitive.

That gives the agent more autonomy without turning the product into an unreviewed self-modifying system.

### 2.4 Boundary Is a Feature

登录、密码、TOTP、SMS/email 2FA、CAPTCHA、支付、隐私敏感操作都不是“失败”，而是自治系统必须识别和处理的边界。边界的正确行为是：

1. 结构化检测。
2. 记录原因和上下文。
3. 触发 ask_user / permission / checkpoint。
4. 用户完成或授权后从同一 browser state 恢复。

### 2.4 Learning Must Be Gated

Memory/gbrain/skill extraction 的自我学习结果不能直接污染长期系统。所有候选学习都应该经过 harness episode 和 promotion gate：

- 这条记忆是否被正确召回？
- gbrain 是否结构化沉淀正确？
- 用户纠错是否覆盖旧知识？
- skill 是否真的减少成本、提高成功率？
- 新 prompt/hook 是否引入权限或 hallucination 回归？

---

## 3. Browser Autonomy Architecture

### 3.1 Browser Identity Broker

Browser Identity Broker 负责复用真实用户 profile 与同步 auth profile 数据。

```ts
type BrowserIdentityProfile = {
  id: string
  label: string
  originPattern: string
  kind: 'real_browser_profile' | 'storage_state' | 'cookie_jar' | 'bearer_token'
  provider: 'system_chrome' | 'playwright' | 'browser_use' | 'manual_import'
  scope: 'workspace' | 'session' | 'global'
  secretHandle: string
  createdAt: string
  lastVerifiedAt?: string
  expiresAt?: string
}
```

Responsibilities:

- Detect system Chrome profiles.
- Import/export Playwright-compatible `storageState`.
- Store sensitive state through keyring/Keychain, not repo files.
- Attach profile to a browser context by origin/workspace/session.
- Validate whether auth is still live with a lightweight page probe.
- Emit `auth_profile_stale` boundary events when login expires.

Backend placement:

- `src-tauri/src/browser/identity/mod.rs`
- `src-tauri/src/browser/identity/profile_store.rs`
- `src-tauri/src/browser/identity/playwright_state.rs`
- `src-tauri/src/browser/identity/keyring_store.rs`

Frontend placement:

- `ui/src/components/browser/identity/BrowserIdentitySettings.tsx`
- `ui/src/atoms/browser-identity-atoms.ts`

### 3.2 Visual Perception Adapter

Visual perception is a provider interface over mature OCR/VLM tools.

```ts
type VisualPerceptionProvider = 'paddleocr' | 'easyocr' | 'vlm_grounding'

type OcrTextBox = {
  text: string
  confidence: number
  box: { x: number; y: number; width: number; height: number }
  source: VisualPerceptionProvider
}

type VisualObservation = {
  screenshotRef: string
  ocrText: OcrTextBox[]
  detectedControls: VisualControlCandidate[]
}
```

Recommended path:

1. Start with a provider interface and a no-op/mock provider for tests.
2. Add EasyOCR sidecar for fast local validation.
3. Add PaddleOCR as production-grade local provider.
4. Add optional VLM grounding provider for visual-only controls.

Backend placement:

- `src-tauri/src/browser/perception/mod.rs`
- `src-tauri/src/browser/perception/provider.rs`
- `src-tauri/src/browser/perception/sidecar.rs`

### 3.3 Challenge Boundary Broker

Challenge Boundary Broker classifies sensitive and anti-abuse states.

```ts
type BrowserBoundaryKind =
  | 'login_required'
  | 'password_field'
  | 'totp_2fa'
  | 'email_or_sms_2fa'
  | 'captcha'
  | 'payment'
  | 'privacy_sensitive'
  | 'auth_profile_stale'

type BrowserBoundaryEvent = {
  id: string
  sessionId: string
  tabId: string
  kind: BrowserBoundaryKind
  url: string
  title: string
  evidence: BoundaryEvidence[]
  recommendedAction: 'ask_user' | 'use_authorized_profile' | 'checkpoint' | 'abort'
  canResume: boolean
}
```

CAPTCHA policy:

- Default third-party CAPTCHA behavior: detect and ask user.
- Allowed automatic behavior:
  - local fixtures and self-owned test pages,
  - explicitly configured allowlist domains,
  - enterprise/managed provider integrations with user consent and audit trail.
- Disallowed default behavior:
  - silent third-party CAPTCHA solving,
  - bypassing anti-abuse systems without domain authorization,
  - storing CAPTCHA challenge artifacts without trace retention policy.

This keeps the product useful for legitimate automation and QA while avoiding a brittle or unsafe “universal CAPTCHA breaker” path.

---

## 4. UCLAW Harness Runtime

Harness Runtime is the global control plane for evaluation and learning. Its runtime contract is broader than test execution:

- In **eval mode**, it runs deterministic fixtures or local live smoke tasks and emits scorecards.
- In **production observer mode**, it records normalized runtime events from real user sessions, automations, browser tasks, tool calls, memory writes, and UI projections.
- In **learning mode**, it turns failed or partial episodes into candidate changes.
- In **self-healing mode**, it allows scoped temporary helper/domain-skill patches inside an episode, then records the patch as a gated candidate instead of silently promoting it.
- In **gate mode**, it blocks or promotes candidates based on regression evidence, rollback metadata, and safety constraints.

### 4.1 Core Types

```ts
type HarnessCase = {
  id: string
  subject: HarnessSubject
  title: string
  prompt: string
  setup: HarnessFixture[]
  policy: HarnessPolicy
  budgets: HarnessBudget
  assertions: HarnessAssertion[]
  graders: HarnessGraderSpec[]
}

type HarnessEpisode = {
  runId: string
  caseId: string
  subject: HarnessSubject
  origin: 'fixture' | 'live_smoke' | 'production'
  parentRunId?: string
  startedAt: string
  finishedAt?: string
  trace: HarnessEvent[]
  artifacts: HarnessArtifact[]
  scores: Record<string, number>
  verdict: 'pass' | 'fail' | 'partial' | 'blocked'
}
```

### 4.2 Global Event Schema

Every module writes through a shared envelope. Module-specific payloads live in artifacts or typed sub-payloads, but the top-level identity fields stay stable so scorecards can compare Browser, Agent, Automation, Memory, and UI events.

```ts
type HarnessEventEnvelope<TPayload = unknown> = {
  id: string
  ts: string
  source:
    | 'agent_loop'
    | 'agent_message'
    | 'context'
    | 'browser'
    | 'browser_cdp'
    | 'harness_workspace'
    | 'tool_broker'
    | 'permission'
    | 'automation'
    | 'memory'
    | 'gbrain'
    | 'skill'
    | 'prompt'
    | 'hook'
    | 'plugin'
    | 'coordinator'
    | 'ui_projection'
    | 'runtime_health'
  subject: HarnessSubject
  sessionId?: string
  runId?: string
  parentRunId?: string
  taskId?: string
  correlationId?: string
  artifactRefs: string[]
  payload: TPayload
  privacy: {
    containsSecret: boolean
    redaction: 'none' | 'summary_only' | 'hash_only' | 'dropped'
  }
}
```

Canonical event kinds:

```ts
type HarnessEvent =
  | { kind: 'run_started'; caseId?: string; promptRef?: string }
  | { kind: 'agent_message'; role: 'system' | 'developer' | 'user' | 'assistant' | 'tool'; messageRef: string }
  | { kind: 'context_snapshot'; contextRef: string; tokenUsage?: TokenUsage }
  | { kind: 'model_turn'; model: string; inputRef: string; outputRef: string; tokenUsage?: TokenUsage }
  | { kind: 'tool_call'; toolName: string; callId: string; inputRef: string }
  | { kind: 'tool_result'; toolName: string; callId: string; outputRef: string; ok: boolean }
  | { kind: 'permission_request'; requestId: string; reason: string; mode: string }
  | { kind: 'permission_result'; requestId: string; decision: 'approved' | 'denied' | 'expired' }
  | { kind: 'browser_observation'; tabId: string; observationRef: string }
  | { kind: 'browser_action'; actionName: string; actionRef: string; ok: boolean }
  | { kind: 'cdp_call'; method: string; callId: string; inputRef: string }
  | { kind: 'cdp_result'; method: string; callId: string; outputRef: string; ok: boolean }
  | { kind: 'boundary_event'; boundaryRef: string; kind: string; canResume: boolean }
  | { kind: 'helper_missing'; helperName: string; subject: HarnessSubject; reasonRef: string }
  | { kind: 'helper_patch_created'; patchId: string; helperName: string; diffRef: string; temporary: boolean }
  | { kind: 'helper_patch_used'; patchId: string; callId: string; ok: boolean }
  | { kind: 'domain_skill_created'; skillId: string; subject: HarnessSubject; skillRef: string; temporary: boolean }
  | { kind: 'automation_tick'; automationId: string; scheduledFor: string }
  | { kind: 'automation_result'; automationId: string; ok: boolean; outputRef: string }
  | { kind: 'memory_write'; target: 'memory' | 'gbrain'; artifactRef: string }
  | { kind: 'memory_recall'; target: 'memory' | 'gbrain'; queryRef: string; resultRef: string }
  | { kind: 'candidate_created'; candidateId: string; candidateRef: string }
  | { kind: 'gate_result'; candidateId: string; verdict: 'promote' | 'hold' | 'reject'; score: number }
  | { kind: 'promotion_applied'; candidateId: string; rollbackRef: string }
  | { kind: 'checkpoint'; checkpointRef: string; resumable: boolean }
  | { kind: 'ui_projection'; surface: string; stateRef: string }
  | { kind: 'runtime_health'; service: string; status: 'running' | 'stopped' | 'failed' | 'degraded' }
  | { kind: 'run_finished'; verdict: HarnessEpisode['verdict']; finalRef?: string }
```

Rules:

- Raw secrets, cookies, bearer tokens, passwords, CAPTCHA images, and raw screenshot base64 must not be stored directly in ordinary trace payloads.
- Large values go into artifacts with redaction metadata; events carry refs.
- `tool_call` and `tool_result` must share a stable `callId`.
- `cdp_call` and `cdp_result` must share a stable `callId`, and raw CDP payloads must be redacted before ordinary trace storage.
- `helper_patch_created` and `domain_skill_created` are not promotions. They are episode-local repairs until a gate explicitly promotes them.
- UI state is an evaluable projection, not a truth source. Backend run state remains authoritative.

### 4.3 Production Trace to Promotion Loop

The intended closed loop is:

1. **Capture:** production runtime emits `HarnessEventEnvelope` for agent messages, context snapshots, model turns, tools, browser observations/actions, permissions, automation ticks, memory writes/recalls, and UI projections.
2. **Episode assembly:** event router groups related events by `sessionId`, `runId`, `taskId`, and `correlationId` into a `HarnessEpisode`.
3. **Evaluation:** subject adapters run graders over the episode and attach scorecards.
4. **Failure classification:** failed/partial/blocked episodes are labeled as browser DOM failure, bad prompt, missing memory, stale gbrain, tool crash, permission mismatch, automation schedule drift, UI projection drift, or coordinator routing error.
5. **Self-healing attempt:** when policy allows, the episode can spawn a temporary helper/domain-skill patch in a writable harness workspace, use it to continue, and record whether it worked.
6. **Candidate extraction:** classifiers generate proposed memory corrections, gbrain page updates, skill candidates, helper/domain-skill patches, prompt patches, hook changes, automation policy changes, or browser policy changes.
7. **Gate:** self-improvement gate requires evidence, passing regression suites, no blockers, and rollback refs.
8. **Promotion:** approved candidates mutate production registries only through typed promotion handlers.
9. **Regression replay:** promoted changes rerun relevant harness suites.
10. **Production monitoring:** future production episodes compare success rate, step count, ask_user frequency, tool failure rate, and cost against pre-promotion baseline.

Until steps 1-10 are wired end-to-end, Harness should be described as **evaluation infrastructure**, not as a completed self-improving intelligence loop.

### 4.4 Adapter Classes

Adapters fall into three classes:

| Class | Purpose | Example current implementation |
| --- | --- | --- |
| Fixture adapter | Deterministic, fast regression over synthetic traces or synthetic browser runs | Agent control-plane fixture, self-improvement fixture, System Diagnostics Browser fixture |
| Live smoke adapter | Bounded real runtime execution against local fixtures or allowlisted targets | BrowserAgentLoop executor path exists, but System Diagnostics does not use it yet |
| Production observer adapter | Converts real user/session/automation events into episodes | Browser task store and memory adapter exist; global event ingestion is not complete |
| Thin writable adapter | Gives the agent raw subsystem access plus an editable helper workspace | Not implemented; inspired by browser-harness CDP + editable helpers |

All three are useful, but they answer different questions:

- Fixture answers: did our contract and scoring code regress?
- Live smoke answers: does the real runtime work on bounded deterministic tasks?
- Production observer answers: are users getting better outcomes over time?
- Thin writable answers: can the agent repair a missing capability during the task without waiting for a human framework change?

### 4.5 Browser-Harness-Inspired CDP Lane

The Browser subject needs two execution lanes:

1. **Structured lane:** existing `browser_task`, Playwright-compatible context/profile resolution, boundary broker, memory adapter, and scorecards.
2. **Thin CDP lane:** a direct CDP client that can attach to a user-approved Chrome target, issue raw `Domain.method` commands, capture screenshots, dispatch coordinate input, inspect events, and recover stale sessions with minimal policy code between the agent and browser.

The thin lane should follow these browser-harness lessons:

- Prefer screenshot + coordinate action when visible UI is the target. This avoids selector brittleness and works across iframes/shadow DOM/cross-origin surfaces because hit-testing happens in the browser process.
- Drop to DOM/JS only when the visible target has no reliable geometry, such as hidden inputs or extraction tasks.
- Keep the protected core small: attach, session, target, event buffer, screenshot, keyboard/mouse dispatch, raw CDP send, and liveness.
- Put task-specific browser logic in editable helpers, not in the protected core.
- Use domain skills for durable site knowledge: stable URL patterns, private APIs, wait conditions, selectors, known traps, and iframe/shadow-DOM notes.
- After every meaningful action, observe again before assuming success.

uClaw should not copy browser-harness blindly:

- Browser-harness optimizes for direct agent freedom; uClaw must also preserve app UX, permissions, traceability, redaction, and promotion gates.
- CDP lane can be enabled for local dev, allowlisted Browser tasks, and explicit user-approved profile sessions first.
- Sensitive boundaries still route to ask_user/checkpoint; CDP freedom does not authorize credential entry, payment, CAPTCHA bypass, or privacy-sensitive mutation.

### 4.6 Unified Self-Healing Workspace

Self-healing should not be Browser-only. The same pattern applies across the system:

```text
agent hits missing capability
  -> emits helper_missing
  -> writes temporary helper/domain skill in harness workspace
  -> uses helper inside the same episode
  -> emits helper_patch_used
  -> grader scores outcome
  -> gate decides whether to promote, hold, or reject
```

Suggested workspace shape:

```text
harness_workspace/
  browser/
    helpers/
    domain-skills/
  tools/
    helpers/
    api-skills/
  automation/
    helpers/
    flow-skills/
  memory/
    schemas/
    recall-skills/
  gbrain/
    page-templates/
    relation-skills/
  prompts/
    candidates/
  hooks/
    candidates/
```

Policy:

- Episode-local helpers may be executed only in the sandbox/profile/workspace scope assigned to that episode.
- Promotion from workspace to production registry requires diff review, redaction scan, regression replay, rollback metadata, and a gate verdict.
- Rejected helpers remain as artifacts for debugging, not as executable production code.
- Domain skills should capture the durable map, not the diary: stable selectors, API routes, wait conditions, and traps; never secrets or one-off user data.

### 4.7 Current Implementation State

| Area | Current state | Honest interpretation |
| --- | --- | --- |
| Runtime core | `HarnessRuntime`, episodes, events, artifacts, graders exist | Good substrate, but in-memory episode registry and limited query/report APIs |
| Browser parity | 7 built-in cases, scorecards, fixture executor, real executor trait | Measures browser contract; UI path is deterministic fixture, not live web autonomy |
| Memory/gbrain eval | Inventory and recall probes via explicit command | Useful service/retrieval check; not yet continuous memory quality monitoring |
| Agent control-plane | Fixture traces for tool pairing, permissions, checkpoints, failure final state | Good contract test; not yet real agent-loop ingestion |
| System Diagnostics UI | Buttons for `All`, `Browser`, `Memory`, `Agent`, `Self` | Developer-facing runner, not full harness dashboard |
| Self-improvement gates | Candidate policy evaluator and fixture candidates | Gate semantics exist; automatic candidate generation and promotion handlers are not implemented |
| Browser production memory | Browser task events can write Memory/gbrain with cooldown and redaction | Useful retention path, but not guaranteed complete or used as eval input automatically |
| Browser CDP thin lane | Not implemented | Needed for browser-harness parity and full browser autonomy |
| Self-healing workspace | Gate fixture only | Missing writable helper/domain-skill workspace and mid-task repair events |
| Automation | No adapter yet | Must be added for global harness claim |
| Agent messages/context | No adapter yet | Major missing piece for prompt/context quality eval |
| UI projection | No adapter yet | Important because prior bugs included frontend running-state drift |

### 4.8 Runtime Modules

Backend:

- `src-tauri/src/harness/mod.rs`
- `src-tauri/src/harness/case.rs`
- `src-tauri/src/harness/episode.rs`
- `src-tauri/src/harness/trace.rs`
- `src-tauri/src/harness/artifacts.rs`
- `src-tauri/src/harness/graders.rs`
- `src-tauri/src/harness/adapters/mod.rs`

Adapters:

- `adapters/agent_loop.rs`
- `adapters/browser.rs`
- `adapters/tools.rs`
- `adapters/permissions.rs`
- `adapters/hooks.rs`
- `adapters/memory.rs`
- `adapters/gbrain.rs`
- `adapters/skills.rs`
- `adapters/tasks.rs`
- `adapters/prompts.rs`
- `adapters/coordinator.rs`

Frontend:

- `ui/src/components/harness/HarnessDashboard.tsx`
- `ui/src/components/harness/HarnessEpisodeView.tsx`
- `ui/src/components/harness/HarnessScorecard.tsx`
- `ui/src/atoms/harness-atoms.ts`

### 4.9 Memory Adapter and gbrain Adapter

Memory Adapter must connect both the Memory System and gbrain. They are separate truth layers with a unified grader.

```ts
type MemoryHarnessTarget = 'memory_system' | 'gbrain'

type MemoryEvalProbe = {
  id: string
  target: MemoryHarnessTarget
  writePrompt: string
  recallPrompt: string
  expectedFacts: string[]
  forbiddenFacts: string[]
  correctionPrompt?: string
}

type MemoryEvalResult = {
  probeId: string
  target: MemoryHarnessTarget
  recalledFacts: string[]
  missingFacts: string[]
  hallucinatedFacts: string[]
  staleFacts: string[]
  score: number
}
```

Memory System metrics:

- recall precision,
- recall coverage,
- stale recall rate,
- hallucinated memory rate,
- user correction adoption rate,
- cross-session persistence.

gbrain metrics:

- entity consistency,
- page grounding,
- relation/link correctness,
- duplicate page rate,
- wrong merge rate,
- recall-to-page traceability.

Unified Memory Grader answers:

- Did the agent remember the right fact?
- Did the fact land in the correct persistence layer?
- Did gbrain structure the fact without distorting it?
- Did later agent behavior actually use the remembered knowledge?
- Did user correction override old memory/gbrain state?

---

## 5. Evaluation Metrics

### Browser

- task success rate,
- action count,
- DOM/action state diff correctness,
- tab selection correctness,
- auth profile restore success,
- challenge boundary precision,
- checkpoint resume success.

### Agent Loop

- tool-call success rate,
- stuck-loop detection,
- permission correctness,
- final answer groundedness,
- cost per completed task,
- recovery-after-tool-error rate.

### Automation

- schedule correctness,
- idempotency,
- run isolation,
- durable checkpointing,
- side-effect auditability.

### Memory/gbrain

- recall precision and coverage,
- cross-session persistence,
- stale memory rate,
- gbrain entity/page consistency,
- correction adoption,
- downstream usage rate.

### Skills

- skill extraction quality,
- reuse rate,
- task success improvement after skill promotion,
- prompt/context cost reduction,
- regression rate after skill update.

---

## 6. Implementation Roadmap

### PR 0: Global Agent Harness Control Plane Definition

Goal: correct the abstraction so Harness is defined as the global agentic runtime control plane, not a Browser-only or diagnostics-only test surface.

Scope:

- global subject coverage,
- global event envelope,
- production trace to episode assembly,
- eval to candidate to gate to promotion loop,
- browser-harness-inspired thin CDP lane,
- unified self-healing workspace for helpers/domain skills across Browser and Agent subjects,
- current implementation truth table,
- explicit distinction between fixture, live smoke, and production observer adapters.

Verification:

- documentation review confirms Browser, Agent Loop, Agent Message, Context, Automation, Memory/gbrain, Skills, Prompts, Hooks, Permissions, Tools, Plugins, Coordinator, UI Projection, and Runtime Health are covered by the global model.
- no current fixture-only implementation is described as completed self-improvement.

### PR 1: Browser Identity Broker

Goal: reuse real browser profile and Playwright-compatible storage state.

Scope:

- profile detection,
- storage state import/export,
- keyring-backed secret handles,
- browser_task profile selection.

Verification:

- import a storage state fixture,
- restore a browser context,
- detect stale profile,
- ensure no secret value enters normal trace/log/chat.

### PR 2: Visual Perception Adapter

Goal: add OCR/VLM provider abstraction without binding the core browser loop to one provider.

Scope:

- provider trait,
- mock provider tests,
- screenshot-to-OCR artifact flow,
- optional EasyOCR sidecar prototype.

Verification:

- OCR fixture returns text boxes,
- visual observation is attached to browser observation,
- provider failure degrades to DOM-only observation.

### PR 3: Challenge Boundary Broker

Goal: detect login/password/2FA/CAPTCHA/payment/privacy boundaries and bridge them into ask_user/checkpoint.

Scope:

- boundary classifier,
- policy table,
- ask_user integration,
- checkpoint/resume after user action.

Verification:

- login fixture,
- password field fixture,
- CAPTCHA fixture detection,
- user intervention resume.

### PR 4: Harness Runtime Core

Goal: create generic harness runtime covering all current and future agent modules.

Scope:

- `HarnessCase`,
- `HarnessEpisode`,
- trace store,
- artifact store,
- grader registry,
- hook bus.

Verification:

- dry-run case,
- tool trace case,
- permission request case,
- persisted episode replay.

### PR 5: Memory and gbrain Evaluation Adapters

Goal: connect Memory System and gbrain into the same harness.

Scope:

- memory write/recall probes,
- gbrain page/entity probes,
- unified memory grader,
- correction adoption test cases.

Verification:

- write fact -> recall from memory,
- write fact -> structure in gbrain,
- correction prompt updates both,
- stale/wrong facts are scored as failures.

### PR 6: Browser Harness Suite

Goal: make browser-use parity measurable.

Scope:

- multi-tab fixture,
- auth restore fixture,
- file upload fixture,
- checkpoint resume fixture,
- challenge boundary fixture.

Verification:

- scorecard renders,
- trace replay works,
- failure report identifies exact broken adapter/tool/action.

### PR 7: Self-Improvement Promotion Gate

Goal: turn failures into learning candidates without silently polluting production memory/skills/prompts.

Scope:

- failure-to-learning-candidate report,
- skill candidate promotion gate,
- memory/gbrain promotion guard,
- prompt/hook change regression gate.

Verification:

- failed eval generates candidate,
- candidate is not promoted until grader passes,
- rollback is possible.

---

## 7. Recommended Next Step

The next implementation task should be **Global Trace Envelope and Subject Registry**, not another local Browser-only case.

Reason:

- Browser, Memory/gbrain, Agent control-plane, and Self gates now have early slices.
- The remaining architectural gap is that production events do not enter one global episode model.
- Without global trace ingestion, Harness remains a set of useful eval buttons rather than a learning control plane.

Recommended immediate plan:

1. Extend `HarnessSubject` in Rust to include `agent_message`, `context`, `automation`, `ui_projection`, `runtime_health`, and other global subjects from this spec.
2. Add a stable `HarnessEventEnvelope` Rust type with redaction metadata and artifact refs.
3. Add `helper_missing`, `helper_patch_created`, `helper_patch_used`, and `domain_skill_created` event kinds so self-healing is first-class rather than hidden in logs.
4. Add a lightweight event router that can receive production events without running graders synchronously.
5. Add one real production connector first: agent message/context snapshots or automation run records.
6. Start a separate Browser CDP thin-lane PR after the global event envelope exists, so browser-harness parity lands as a subject adapter instead of becoming another parallel browser framework.
7. Keep Browser live smoke as the next Browser-specific follow-up, but do not let Browser consume the entire Harness abstraction.

---

## 8. Source References

- OpenAI Harness Engineering: https://openai.com/index/harness-engineering/
- OpenAI Agents SDK harness direction: https://openai.com/index/the-next-evolution-of-the-agents-sdk/
- OpenHarness: https://github.com/HKUDS/OpenHarness
- Browser Use authentication: https://docs.browser-use.com/open-source/customize/browser/authentication
- browser-use/browser-harness: https://github.com/browser-use/browser-harness
- browser-harness README: https://github.com/browser-use/browser-harness/blob/main/README.md
- browser-harness setup and connection reference: https://github.com/browser-use/browser-harness/blob/main/install.md
- browser-harness skill instructions: https://github.com/browser-use/browser-harness/blob/main/SKILL.md
- Playwright authentication/storage state: https://playwright.dev/docs/auth
- PaddleOCR: https://github.com/PaddlePaddle/PaddleOCR
- EasyOCR: https://github.com/JaidedAI/EasyOCR
