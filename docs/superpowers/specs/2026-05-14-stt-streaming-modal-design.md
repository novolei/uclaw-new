# STT 流式语音 Modal 设计文档

**日期**：2026-05-14
**作者**：uClaw 团队
**状态**：草案 → 待 review
**关联**：现有 STT 实现（PR #148/#154/#156/#157）、[2026-05-13-stt-design.md](./2026-05-13-stt-design.md)（首版批处理 STT 设计）

## 目的

把现有的 uClaw STT 从「内联录音条 + 停止后批处理转写」升级成「弹出 modal + 伪流式实时转写 + 静音超时自动填入 + 持续递增」，UI/UX 流程对齐 Proma 的语音输入体验，并在此基础上加：

- 静音超时后自动把当前段文本填入聊天输入框
- modal 保持打开，持续监听、分段把文本递增追加到输入框
- 给 modal 加 Claude Code 风格的辉光漫散射效果（辉光色绑定主题 token）

参考：Proma 的语音输入浮窗（Doubao 流式 ASR）、Claude Code app 的语音输入 modal 辉光效果。

## 背景：现有 STT 的现状

uClaw 已有一套完整可用的 STT（已合并）：

- **后端** `src-tauri/src/stt/`：本地 SenseVoice ONNX 管线，`stt_transcribe` 命令接收完整音频 base64、返回文本。`OpenFlowAsrEngine` 是懒加载单例，跨命令复用。
- **前端**：`SpeechButton`（两个 composer 的麦克风按钮）、`InlineRecorder`（内联录音条）、`FirstRunDialog`（首次下模型）、`SttSettings`（设置页）。
- **状态**：`recordingStateAtom` FSM、`activeComposerAtom` 跨 composer 锁、`sttSettingsAtom`、`modelStatusAtom`。
- **流程**：点麦克风 → 录音（内联条）→ 点停止 → 批处理转写 → 文本插入光标。

**结构性差异**（本设计要解决的）：现有实现是「内联条 + 批处理」，目标是「modal + 伪流式实时」。

## 设计原则

- **后端零改动**：伪流式纯前端编排——前端持续累积 PCM、定期把「目前为止的段 buffer」重新调一次现有的 `stt_transcribe`。`OpenFlowAsrEngine` 单例复用已加载模型。
- **复用现有基建**：`activeComposerAtom` 跨 composer 锁、`sttSettingsAtom`、`modelStatusAtom`、`FirstRunDialog`、`SpeechButton`、`SttSettings` 保留；只替换录音条 UI 和录音编排 hook。
- **伪流式 = 方案 C**（段内滚动预览 + 静音定稿）：段内每 ~1.5s 重转写当前段 buffer 显示实时预览；静音超时把该段定稿、追加、清空，继续下一段。buffer 被静音天然限界。
- **主题安全**：modal 内文字走主题 token；辉光色用 `hsl(var(--primary))` / `hsl(var(--accent))`，11 个主题自动适配。
- **优雅降级**：权限拒绝、模型未就绪、转写失败、LLM 标点失败都有明确分支或回退。

## 用户故事

1. 用户按 `Alt+S`（或点 composer 的麦克风按钮）→ 弹出语音 modal，开始监听。
2. 用户说话 → modal 的转写区实时显示滚动预览文本（段内每 ~1.5s 刷新一次）。
3. 用户停顿超过静音阈值（默认 1.8s）→ 当前段文本定稿、加标点、追加到聊天输入框；modal 不关，转写区清空，继续监听下一段。
4. 用户继续说话 → 重复 2-3，文本持续递增追加到输入框。
5. 用户再按 `Alt+S`（或点 modal 的 Stop 按钮）→ 定稿当前未完成段 → modal 关闭。
6. 用户按 `Esc` → 取消当前会话，丢弃未定稿段，modal 关闭。
7. 模型未下载时点麦克风 → 走现有 `FirstRunDialog` 下载流程，下完再开 modal。
8. 麦克风权限被拒 → modal 显示权限拒绝态 + 引导。

## 范围与文件结构

### 新增

```
ui/src/components/stt/
  SttModal.tsx              # 流式语音 modal（替代 InlineRecorder）
  SttModal.test.tsx
  SttModal.css              # 辉光漫散射 + 颗粒噪点（也可内联 style，见 §辉光实现）
ui/src/hooks/
  useSttStreamingSession.ts # 流式会话编排 FSM（替代 useSttRecording）
  useSttStreamingSession.test.tsx
ui/src/lib/stt/
  streaming-capture.ts      # 边录边暴露增长 PCM buffer 的音频采集（AudioWorklet）
  streaming-capture.test.ts
  punctuation.ts            # 标点规整 + 可选 LLM 润色
  punctuation.test.ts
ui/public/stt/
  pcm-worklet.js            # AudioWorklet processor（持续 post PCM 块）
```

### 改动

```
ui/src/atoms/stt-atoms.ts                  # recordingStateAtom → sttModalStateAtom；sttSettingsAtom 加字段
ui/src/atoms/stt-atoms.test.ts             # 对应测试更新
ui/src/components/ai-elements/speech-button.tsx  # 触发改为开/关 modal
ui/src/lib/shortcut-defaults.ts            # toggle-stt-recording: Cmd+Shift+M → Alt+S
ui/src/components/chat/ChatInput.tsx       # 移除 <InlineRecorder>，挂 <SttModal>
ui/src/components/agent/AgentView.tsx      # 同上（两个 composer 都要改 — CLAUDE.md 双 composer 规则）
ui/src/components/settings/SttSettings.tsx # 加静音阈值、标点模式设置；快捷键显示更新
```

### 删除

```
ui/src/components/stt/InlineRecorder.tsx        # 被 SttModal 替代
ui/src/components/stt/InlineRecorder.test.tsx
ui/src/hooks/useSttRecording.ts                 # 被 useSttStreamingSession 替代
ui/src/hooks/useSttRecording.test.tsx
```

`ui/src/lib/stt/audio-capture.ts` 保留但被 `streaming-capture.ts` 取代用途——若无其他引用则一并删除（计划阶段确认引用）。

### 后端

零改动。复用 `stt_transcribe`。

## 状态机

`sttModalStateAtom` 持有的 FSM（替代现有 `recordingStateAtom`）：

```typescript
export type SttModalState =
  | { kind: 'idle' }
  | { kind: 'requesting-permission' }
  | { kind: 'listening'; segmentStartedMs: number; volume: number; interimText: string }
  | { kind: 'finalizing'; volume: number }
  | { kind: 'permission-denied' }
  | { kind: 'error'; message: string }
```

转换：

```
idle ──(Alt+S / 点麦克风, 模型 ready)──▶ requesting-permission
requesting-permission ──(getUserMedia 成功)──▶ listening
requesting-permission ──(getUserMedia 拒绝)──▶ permission-denied
listening ──(段内 ~1.5s tick)──▶ listening (interimText 更新)
listening ──(静音 > 阈值 且 interimText 非空)──▶ finalizing
finalizing ──(最终转写 + 标点 + 追加 + 清空)──▶ listening (interimText 重置为 '')
listening ──(Alt+S 再按 / Stop 按钮)──▶ finalizing ──▶ idle (modal 关)
listening / finalizing ──(Esc)──▶ idle (丢弃未定稿段, modal 关)
任意 ──(转写抛错)──▶ error ──(1500ms)──▶ idle
permission-denied / error ──(用户关闭)──▶ idle
```

- modal 在 `state.kind !== 'idle'` 时挂载。
- `volume` 由音量采样驱动 5 段音量条。
- `interimText` 是当前段的滚动预览文本。
- 跨 composer 锁仍用 `activeComposerAtom`：开 modal 前 claim，关 modal 时 release。

## 伪流式引擎（方案 C）

### 音频采集 — `streaming-capture.ts`

现有 `audio-capture.ts` 用 `MediaRecorder` 录完出 blob，拿不到中途数据。新实现用 **AudioWorklet**：

- `getUserMedia` → `AudioContext`（16kHz）→ `AudioWorkletNode`（加载 `ui/public/stt/pcm-worklet.js`）
- worklet processor 持续把 PCM 块 `port.postMessage` 给主线程
- 主线程累积进一个增长的 `Int16Array` 段 buffer
- 同时保留 `AnalyserNode` 算实时音量（喂 5 段音量条 + 静音检测）

暴露接口：

```typescript
export interface StreamingCapture {
  start(deviceId: string | null): Promise<void>
  stop(): void
  getSegmentPcmBase64(): string   // 当前段 PCM16LE base64（喂 stt_transcribe）
  resetSegment(): void            // 清空当前段 buffer（段定稿后调）
  getVolume(): number             // 0..1 实时峰值音量
}
```

> AudioWorklet 是现代标准；若 Tauri webview 兼容性踩坑，回退用 `ScriptProcessorNode`（已弃用但 Proma 在用，可跑）。计划阶段验证。

### 重转写编排 — `useSttStreamingSession.ts`

`listening` 期间：

- `setInterval` ~1.5s 一拍。每拍：
  - in-flight 守卫：`if (transcribeInFlightRef.current) return`（上次没返回就跳过这拍，防止堆积）
  - `capture.getSegmentPcmBase64()` → `invoke('stt_transcribe', { request: { audio_bytes_base64, language, sample_rate: 16000 } })`
  - 成功 → 写 `interimText`（实时预览，**不加 LLM 标点**，只用 SenseVoice 原生输出）
- 音量采样：`AnalyserNode` 按 ~80ms 间隔取峰值（复用现有 InlineRecorder 的采样模式），同时喂 5 段音量条和静音检测；音量 > 阈值时刷新 `lastVoiceMs`。

### 静音定稿

`listening` 期间持续检查：`now - lastVoiceMs > silenceThresholdMs`（默认 1800ms）**且** `interimText.trim()` 非空 **且** 段时长 > 最小段长（默认 800ms，防气音误触发）→ 进 `finalizing`：

1. 取消段内 tick
2. `capture.getSegmentPcmBase64()` → 最后一次 `stt_transcribe`（对完整段，最准）
3. `punctuation.ts` 处理（见 §标点）
4. 智能拼接追加到当前 composer 的 draft（见 §增量追加）
5. `capture.resetSegment()` + `interimText = ''` + `lastVoiceMs = now`
6. 回 `listening`

会话结束（Alt+S / Stop）：同样走一次 `finalizing` 把未完成段定稿，然后 `idle`。`Esc` 直接 `idle` 丢弃。

## 行为层

### Alt+S 快捷键

`shortcut-defaults.ts` 的 `toggle-stt-recording` 从 `Cmd+Shift+M` / `Ctrl+Shift+M` 改为 `Alt+S`（Mac 上是 `Option+S`，Win/Linux 是 `Alt+S`）。

> Mac 上 `Option+S` 默认输入特殊字符（ß）；作为应用内快捷键被 `useShortcut` 捕获并 `preventDefault` 后不影响。计划阶段验证无冲突（检查 `shortcut-defaults.ts` 里其他 `Alt`/`Option` 组合）。

toggle 语义：modal 开着 → 结束会话（定稿+关）；关着 → claim 锁 + 开 modal + 开始监听。与现有 `SpeechButton` 里「只有 chat composer 实例响应快捷键」的去重逻辑一致。

### 增量追加

每段定稿文本追加到当前 composer 的 `agentSessionDraftsAtom`（两个 composer 共用此 atom，按 session/conversation id 键控）：

- composer 聚焦时：走 TipTap `editor.commands.insertContent(text)` 插在光标位置
- 未聚焦时：追加到 draft atom 的字符串末尾
- **智能拼接**（仿 Proma `joinTranscriptParts`）：拼接点两侧都是 ASCII 词字符时补一个空格；涉及中文/标点时不补。例如 `"hello"` + `"world"` → `"hello world"`；`"你好"` + `"世界"` → `"你好世界"`；`"。"` + `"下一句"` → `"。下一句"`。

### 标点

参考 Proma「ASR 层透明加标点」的理念。Proma 靠 Doubao 服务端 `enable_punc`；uClaw 的 SenseVoice 解码器（`decoder.rs::postprocess_tokens`）只拼 token + 规整空格，无标点逻辑——SenseVoice Small 原生会吐一些中文标点但不稳定。

`punctuation.ts` 两档，`sttSettingsAtom.punctuationMode` 控制：

- **`'native'`（默认）**：SenseVoice 原生输出 + 轻量规整——补句末标点（中文段末补「。」、英文段末补「.」，已有标点则不动）、全/半角规范化。本地、即时、零新依赖。
- **`'llm'`（可选）**：段定稿后，用 uClaw 已有的 LLM provider 跑一次「只加标点、不改字、不翻译」的润色 prompt。更准，但每段一次网络调用有延迟。失败时优雅回退到 `'native'` 规整。

实时预览（`interimText`）**始终**用 SenseVoice 原生输出，不等标点处理——标点只在段定稿时施加。

## Modal UI + 辉光漫散射

### 布局

居中 overlay modal，宽 `min(640px, 90vw)`。结构（仿 Claude Code + Proma）自上而下：

1. **状态行**：动效图标（复用 §1 的 ripple spinner 或 Claude Code 风格的星芒）+ 状态文字（`正在聆听… 停顿即录入` / `定稿中…` / `权限被拒` 等）
2. **实时转写区**：显示当前段 `interimText`，`whitespace-pre-wrap break-words`，`min-h` 固定避免跳动，空时显示占位符 `请开始说话`
3. **音量条**：5 段，高度由实时 `volume` 驱动，`transition-all duration-100`（复用现有 InlineRecorder 的波形逻辑）
4. **控制行**：Stop 按钮（红色方块）+ 提示文字 `Alt+S 结束 · Esc 取消`

段定稿瞬间，转写区文本做一个轻微的「上移淡出」过渡（暗示「飞进了输入框」），再清空。

### 辉光漫散射实现

- **辉光层**：modal 容器内绝对定位的一层，多个径向渐变叠加，色用主题 token：
  ```css
  background:
    radial-gradient(ellipse 80% 60% at 70% 20%, hsl(var(--primary) / 0.35), transparent 70%),
    radial-gradient(ellipse 70% 50% at 30% 80%, hsl(var(--accent) / 0.25), transparent 70%);
  filter: blur(40px);
  ```
  缓慢漂移动画（`@keyframes` 平移渐变中心，~20s 循环，`prefers-reduced-motion` 下停用）。
- **backdrop blur**：overlay 遮罩 `backdrop-filter: blur(12px)` 模糊身后内容。
- **颗粒噪点**：低透明度 SVG noise 叠层（`feTurbulence` data-URI 或一张小 noise PNG），`opacity: 0.04`，`mix-blend-mode: overlay`。
- 11 个主题下辉光自动适配（`--primary`/`--accent` 随主题变）。深色主题下辉光更含蓄、浅色主题下更明显——通过 token 的 alpha 自然达成，必要时按 `.dark` 微调 alpha。

## 设置变更

`sttSettingsAtom` 加两个字段（`atomWithStorage` 已自持久化）：

```typescript
export interface SttSettings {
  language: 'auto' | 'zh' | 'en' | ...   // 现有
  autoSend: boolean                       // 现有
  microphoneDeviceId: string | null       // 现有
  silenceThresholdMs: number              // 新增，默认 1800
  punctuationMode: 'native' | 'llm'       // 新增，默认 'native'
}
```

`SttSettings.tsx` 加：
- 静音阈值滑块（1000–3000ms，默认 1800）
- 标点模式选择（原生规整 / LLM 润色）
- 快捷键显示更新为 `Alt+S`

`autoSend` 语义在新流程下调整为：会话结束（modal 关）时若开启则自动发送消息。

## 数据流

```
[Alt+S / 麦克风按钮]
  → speech-button: claim activeComposerAtom → sttModalStateAtom = requesting-permission
[useSttStreamingSession]
  → streaming-capture.start() → AudioWorklet 持续累积 PCM
  → sttModalStateAtom = listening
  ↻ 每 ~1.5s: getSegmentPcmBase64() → invoke(stt_transcribe) → interimText 更新
  → 音量采样维护 lastVoiceMs
[静音 > 阈值]
  → finalizing → 最终转写 → punctuation.ts → 智能拼接追加 agentSessionDraftsAtom
  → resetSegment() → 回 listening
[SttModal] 订阅 sttModalStateAtom 渲染（转写区 / 音量条 / 辉光）
[Alt+S 再按 / Stop]
  → 定稿当前段 → release activeComposerAtom → sttModalStateAtom = idle → modal 卸载
```

## 错误处理

- `getUserMedia` 拒绝 → `permission-denied` 态，modal 显示引导文字
- 模型未就绪 → 点麦克风时先开 `FirstRunDialog`（现有逻辑），下完再开 modal
- `stt_transcribe` 抛错：段内 tick 抛错 → 跳过这拍、继续（不打断会话）；定稿转写抛错 → `error` 态 1500ms 后回 `idle`，已定稿的段不丢
- LLM 标点失败 → 回退 `native` 规整
- AudioWorklet 加载失败 → 回退 `ScriptProcessorNode`；都失败 → `error` 态
- 会话进行中切换 composer / 关闭 session → release 锁、stop capture、回 `idle`

## 测试策略

| 单元 | 测试方式 |
|---|---|
| `stt-atoms.ts` 新 FSM 默认值 + 设置字段 | Vitest 单元测试 |
| `useSttStreamingSession` FSM 转换 / 静音检测 / in-flight 守卫 / 段定稿追加 | Vitest + fake timers + mock `invoke` + mock capture |
| `streaming-capture.ts` PCM 累积 / resetSegment / 段 base64 | Vitest + mock AudioWorklet（jsdom shim，复用 `test-utils/stt-mocks.ts` 思路） |
| `punctuation.ts` native 规整 + 智能拼接 | Vitest 纯函数测试 |
| `SttModal` 渲染各状态 | RTL + mock `useSttStreamingSession` |
| `speech-button` 触发开/关 modal + 快捷键 | RTL + mock |

AudioWorklet 在 jsdom 下不可用——`streaming-capture` 测试 mock worklet，真实音频路径手动验证。

## 已知限制 / YAGNI

- 段内重转写有 CPU 成本；长时间不停顿的段 buffer 越来越大、每次转写越来越慢——靠静音天然限界，正常说话很少 >15s 不停顿。
- SenseVoice 在短/部分 buffer 上准确率低于完整句 → 实时预览会「抖动」（更多音频到达时文字会被修正）；这是伪流式固有特性，定稿转写用完整段保证最终质量。
- LLM 标点模式有网络延迟和成本——默认关闭。
- 不做：真·流式 ASR（需换引擎或云端）、外部 app 写入（Proma 有，uClaw 不需要）、interim/final 视觉区分（SenseVoice 没有 isFinal 概念）。
- AudioWorklet 兼容性若踩坑回退 ScriptProcessorNode。

## 决策记录

| 决策 | 选项 | 选择 | 理由 |
|---|---|---|---|
| 流式能力 | 真流式换引擎 / 伪流式 SenseVoice / 放弃实时 | **伪流式 SenseVoice** | 复用现有后端零改动；前端编排即可 |
| 伪流式机制 | 全量滚动 / 静音分段无预览 / 段内预览+静音定稿 | **段内预览+静音定稿（C）** | 实时 + buffer 有界 + 自然定稿点，本质即 Proma 行为 |
| Modal 生命周期 | 保持开分段清空 / 保持开累积显示 / 填完即关 | **保持开·分段清空** | 最接近 Proma + 用户要的「持续递增」 |
| UI 形态 | 独立窗口 / in-app modal | **in-app modal** | uClaw 只往自己输入框写，不需要 Proma 那种跨 app 独立窗口 |
| 快捷键 | Cmd+Shift+M（现有）/ Alt+S | **Alt+S** | 用户指定 |
| 标点 | SenseVoice 原生 / 原生+规整 / +LLM 润色（可选） | **原生+规整（默认）, LLM 可选** | 本地即时零依赖；LLM 留给要高质量的用户 |
| 音频采集 | MediaRecorder（现有）/ AudioWorklet / ScriptProcessor | **AudioWorklet（回退 ScriptProcessor）** | 现代标准、能拿中途 PCM；MediaRecorder 拿不到增长 buffer |
| 辉光实现 | 图片素材 / CSS 渐变+blur | **CSS 径向渐变 + blur + 主题 token** | 零素材、主题自适配 |

## 下一步

1. 用户 review 本 spec
2. 通过后调用 `superpowers:writing-plans` 生成实现 plan
3. plan 走 subagent-driven-development 执行（本 worktree 内）
