# UCLAW Agent Autonomy Harness Design

> 目标：把 Browser 自治、Agent Loop 自治、Memory/gbrain 自我学习、Skill 提取和 Automation 执行统一到一套可观测、可评估、可恢复、可进化的 UCLAW Harness Runtime 中。

---

## 1. Executive Summary

uClaw 的最终目标不是单点实现一个 browser agent，而是构建一个能长期自治、自我学习、自我进化的 agent product runtime。Browser 能力、Memory OS、gbrain、Skill 提取、Automation、Permissions、Hooks、Tools、Prompts 都应该接入同一个 harness，而不是各自维护一套局部状态、局部日志和局部评估。

本设计选择 **reuse-first** 原则：

- 身份与会话管理复用 Playwright / browser-use 的 profile 与 storage state 机制，不自研 cookie/session 系统。
- OCR/视觉识别复用 PaddleOCR、EasyOCR、VLM grounding adapter，不自研 OCR。
- CAPTCHA 处理采用安全边界策略：检测、分类、授权交接、checkpoint resume；第三方真实站点不默认自动破解，只有自有测试环境或明确授权 allowlist provider 才允许自动化处理。
- Harness 参考 OpenHarness 的模块化思想，覆盖 `agent_loop`、`tools`、`skills`、`plugins`、`permissions`、`hooks`、`memory`、`gbrain`、`tasks`、`coordinator`、`prompts`。

核心判断：**自治能力不是让 agent 更大胆，而是让 agent 每一步更可观察、更可验证、更可恢复、更可学习。**

---

## 2. Design Principles

### 2.1 Reuse Before Build

uClaw 不应该单独造身份认证、session、OCR、captcha、eval runner 的大轮子。我们只做统一适配层：

- Browser identity state 使用 Playwright-compatible `storageState`。
- 真实登录态复用系统 Chrome profile 或 browser-use real browser profile。
- Secret 保存使用系统 keyring / macOS Keychain。
- OCR 使用 provider adapter 接 PaddleOCR / EasyOCR / local VLM。
- Harness 复用现有 agent/tool/memory/gbrain 运行时事件，不另起平行真相源。

### 2.2 One Harness, Many Subjects

Harness 的 subject 不是 browser，而是所有自治模块：

```ts
type HarnessSubject =
  | 'agent_loop'
  | 'browser'
  | 'tools'
  | 'skills'
  | 'plugins'
  | 'permissions'
  | 'hooks'
  | 'memory'
  | 'gbrain'
  | 'tasks'
  | 'coordinator'
  | 'prompts'
```

每个 subject 通过 adapter 接入同一套 case、episode、trace、artifact、grader、scorecard。

### 2.3 Boundary Is a Feature

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
  startedAt: string
  finishedAt?: string
  trace: HarnessEvent[]
  artifacts: HarnessArtifact[]
  scores: Record<string, number>
  verdict: 'pass' | 'fail' | 'partial' | 'blocked'
}

type HarnessEvent =
  | { kind: 'run_started'; ts: string; caseId: string }
  | { kind: 'model_turn'; ts: string; model: string; tokenUsage?: TokenUsage }
  | { kind: 'tool_call'; ts: string; toolName: string; inputRef: string }
  | { kind: 'tool_result'; ts: string; toolName: string; outputRef: string; ok: boolean }
  | { kind: 'permission_request'; ts: string; requestId: string; reason: string }
  | { kind: 'boundary_event'; ts: string; boundary: BrowserBoundaryEvent }
  | { kind: 'memory_write'; ts: string; target: 'memory' | 'gbrain'; artifactRef: string }
  | { kind: 'memory_recall'; ts: string; target: 'memory' | 'gbrain'; artifactRef: string }
  | { kind: 'checkpoint'; ts: string; checkpointRef: string }
  | { kind: 'run_finished'; ts: string; verdict: HarnessEpisode['verdict'] }
```

### 4.2 Runtime Modules

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

### 4.3 Memory Adapter and gbrain Adapter

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

The next task should be **writing-plans**, not brainstorming.

Reason:

- The architecture direction is already chosen.
- Major technical decisions are known.
- The risk is implementation sprawl, not lack of ideas.
- A `writing-plans` document should split PR 1 and PR 4 into executable, test-first tasks.

Recommended immediate plan:

1. Write `docs/superpowers/plans/2026-05-19-browser-identity-broker.md`.
2. Write `docs/superpowers/plans/2026-05-19-uclaw-harness-runtime-core.md`.
3. Implement PR 1 first if the next product focus is Browser autonomy.
4. Implement PR 4 first if the next product focus is whole-agent self-evolution.

My recommendation: **PR 4 first, then PR 1.**

Why: Harness Runtime Core becomes the evaluation substrate for Browser Identity, Challenge Broker, Memory/gbrain, and Skill promotion. If we build identity first without harness, we still rely on manual smoke tests. If we build harness first, every later Browser PR lands with a measurable regression suite.

---

## 8. Source References

- OpenAI Harness Engineering: https://openai.com/index/harness-engineering/
- OpenAI Agents SDK harness direction: https://openai.com/index/the-next-evolution-of-the-agents-sdk/
- OpenHarness: https://github.com/HKUDS/OpenHarness
- Browser Use authentication: https://docs.browser-use.com/open-source/customize/browser/authentication
- Playwright authentication/storage state: https://playwright.dev/docs/auth
- PaddleOCR: https://github.com/PaddlePaddle/PaddleOCR
- EasyOCR: https://github.com/JaidedAI/EasyOCR
