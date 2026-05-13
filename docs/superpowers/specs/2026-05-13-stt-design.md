# STT (Speech-to-Text) for uClaw — Design

**Date:** 2026-05-13
**Branch:** `worktree-stt-feature`
**Scope:** Single feature; one implementation plan suffices.

## 1. Goal

为 uClaw 的两个聊天输入框（ChatInput / AgentView）添加语音输入。点击 mic 按钮 → 录音 → 自动转写 → 文本插入光标位置。100% 本地、离线、隐私优先。

## 2. Sources

- **if2Ai** (`/Users/ryanliu/Documents/IfAI/if2Ai`) — 后端（SenseVoice ONNX）+ Tauri 命令骨架直接复用。
- **Proma** (`/Users/ryanliu/Documents/Proma`) — 输入框语音 UI/UX：mic 按钮、5 根真实音量驱动的波形条、内联展开式录音控件。
- **uClaw** (本仓库) — 集成宿主；遵循 CLAUDE.md 的「两个 composer 必须同步改」硬规矩。

## 3. Non-Goals (v0)

- ❌ 云端转写（Doubao / Whisper API / Groq）—— 仅本地 SenseVoice
- ❌ 独立悬浮窗 / 系统级跨应用听写 —— 仅 composer 内联
- ❌ 实时流式 partial 转写 —— 本地引擎为停止后转写
- ❌ FP16 模型变体 / 热词 / 自定义词表 —— v0 仅 quantized 230MB
- ❌ Linux / Windows 麦克风权限专项处理 —— 浏览器层 getUserMedia 自带

## 4. Architecture

```
┌─ Frontend (ui/) ──────────────────────────────────────────────┐
│  ┌──────────────────┐  click  ┌────────────────────────────┐ │
│  │  SpeechButton    │────────▶│  useSttRecording (FSM)     │ │
│  └──────────────────┘         └──────────┬─────────────────┘ │
│                                          │                    │
│             ┌────────────────────────────┼──────────────┐    │
│             ▼                            ▼              ▼    │
│   audio-capture.ts            InlineRecorder       FirstRun  │
│   • MediaRecorder             • 5-bar waveform     Dialog    │
│   • AudioContext              • timer 00:00        (★深度优 │
│   • AnalyserNode (★真实音量)  • cancel / stop      化点 #5) │
│   • PCM16LE base64 encode                                    │
│                                                              │
│   atoms/stt-atoms.ts                                         │
│   • recordingStateAtom   (idle / requesting / recording /    │
│                           transcribing / done / error)        │
│   • activeComposerAtom   (cross-composer single-session lock)│
│   • sttSettingsAtom      (atomWithStorage + backend sync)    │
│   • modelStatusAtom                                          │
└─────────────────────────┬─────────────────────────────────────┘
                          │ Tauri IPC
┌─────────────────────────▼─────────────────────────────────────┐
│ Backend (src-tauri/) — 直接复刻 if2Ai 的 stt 模块             │
│                                                               │
│  tauri_commands::                                             │
│    • stt_transcribe(audio_bytes_base64, lang, sample_rate)   │
│    • stt_model_status()                                       │
│    • stt_download_model(variant)                              │
│    • stt_get_settings() / stt_save_settings(req)             │
│    • stt_list_microphones()        (Tauri 端 enumeration)    │
│                                                               │
│  stt/                                                         │
│    openflow/                                                  │
│    ├─ engine.rs       OpenFlowAsrEngine (lazy singleton)     │
│    ├─ preprocess.rs   Fbank(80) + LFR(7,6) + CMVN            │
│    ├─ onnx_inference.rs  ort session, 4 threads, L3 opt     │
│    └─ decoder.rs      CTC greedy + token post-process        │
│    download.rs        HF + hf-mirror 双源 + progress event   │
│    settings.rs        load/save (~/.uclaw/stt_settings.json)  │
│                                                               │
│  Tauri events:                                                │
│    stt:download-progress  { file, downloaded, total, source }│
└───────────────────────────────────────────────────────────────┘
```

**模型路径**：`~/.uclaw/models/sensevoice/`（与 uClaw 配置同根；**不**与 if2Ai 的 `~/.if2ai/models/sensevoice/` 共享 —— 跨 app 路径耦合脆弱）。

**模型文件**（按需下载）：
- `model_quant.onnx` ≈ 230MB
- `tokens.json` ≈ 344KB
- `am.mvn` ≈ 11KB
- `config.yaml` ≈ 2KB

## 5. State Machine

```
                  ┌──────────────┐
            ┌─────│     idle     │◀────────────┐
            │     └──────┬───────┘             │
       click│            │ click               │ 300ms
          mic            │ mic                 │ flash
            │            ▼ (model not ready)   │  ↑
            │     ┌──────────────┐             │
            │     │ FirstRunDialog│            │
            │     │ (★ #5)        │            │
            │     └──────┬───────┘             │
            │            │ user downloads       │
            ▼            ▼ + start recording   │
       ┌─────────────────────────┐             │
       │  requesting-permission  │             │
       └────────┬────────────────┘             │
        granted │       │ denied                │
                ▼       ▼                       │
        ┌──────────┐  ┌─────────────────┐      │
        │ recording│  │permission-error │──────┘
        └────┬─────┘  └─────────────────┘
   cancel(X) │   │ stop(✓) / 60s auto / shortcut
             ▼   ▼
        idle    ┌────────────────┐
                │  transcribing  │
                └───────┬────────┘
              error │   │ ok(text)
                    ▼   ▼
              ┌────────┐  ┌──────────────────┐
              │ error  │  │ inject @ cursor  │
              └────────┘  │ + auto-send?(opt)│
                          └────────┬─────────┘
                                   ▼
                                  done ─→ idle
```

**关键不变式**：
- 单例 —— 跨 composer 同时只有 1 个录音会话（`activeComposerAtom`）。
- 录音中点击外部不打断（防误触）；只接受 X / Esc / ✓ / 快捷键 / 60s 超时。
- 录音上限 60s；50s 起波形条变黄警告。

## 6. UI — Inline Recorder（Proma 视觉风格）

idle 态：mic 按钮跟其他 composer 工具按钮等大（36×36 圆形）。

录音态：mic 按钮旁向右展开 ~280px pill：

```
┌─────────────────────────────────────────────────┐
│  🎤●   ▍▌▍▎▌▍▎    00:08    ✕      ✓           │
│  red   real-vol     timer   cancel  stop+send  │
└─────────────────────────────────────────────────┘
```

- **波形**：5 根条，每根宽 3px，间距 3px，高度由 `AnalyserNode.getByteFrequencyData()` 真实驱动（**深度优化 #1**：超过 if2Ai 的 RAF 模拟，对齐 Proma）；颜色 `bg-primary`，>50s 变 `text-amber-500` 警告。
- **计时**：`mm:ss` 单色字体；50s 黄，60s 自动 stop。
- **✕ cancel**：丢弃整段音频，回 idle，不做转写。
- **✓ stop**：进 transcribing 态；按钮变 spinner；完成后短 flash 然后塌回 idle。
- **转写中**：mic 区域显示 spinner + "转写中…"（pill 整体不变形）。
- **完成**：✓ 闪 300ms，文本注入 RichTextInput 光标位置（`editor.commands.insertContent(text)`），pill 塌回。
- **颜色**：全部用 token —— `bg-primary` / `text-foreground` / `bg-destructive` / `text-amber-500` —— 无 hardcoded。
- **动画**：使用 Framer Motion `AnimatePresence` + `layout`，展开/塌回 200ms `easeInOut`，与现有 composer 工具按钮一致。

## 7. 深度优化点 #5 — 优雅的首次下载引导（FirstRunDialog）

**问题**：if2Ai 的做法是模型未下载时 toast 一行字 +"去设置下载" —— 信息密度低、要离开当前页、用户得自己回来再点 mic、链路断。

**uClaw 的方案**：**首次点击 mic 即弹出 inline 引导弹层**（不是 toast、不是设置页跳转），当场说明 + 当场下载 + 下载完直接录音。整个流程不离开 composer 视野。

**触发**：
- 用户首次（或模型缺失/损坏）点击 SpeechButton。
- 检测 `stt_model_status()` → `not_downloaded` / `partial` / `corrupted`。

**Dialog 内容**（≈ 420 × 320px，Radix Dialog，主题色全 token）：

```
┌─ 启用语音输入 ──────────────────────────────────  ✕ ┐
│                                                     │
│   🎤                                                │
│   ───                                               │
│   SenseVoice (FunASR)                              │
│                                                    │
│   • 完全离线 — 录音永不离开你的设备                │
│   • 5 种语言 — 中 / 英 / 粤 / 日 / 韩 + 自动      │
│   • 自动标点 — 输出可直接发送                       │
│                                                    │
│   首次需下载 ~230MB 模型 · 一次性 · 永久离线       │
│                                                    │
│   ┌─────────────────────────────────────────────┐ │
│   │ 模型来源:  HuggingFace ✓  (含 hf-mirror     │ │
│   │            国内镜像自动 fallback)            │ │
│   └─────────────────────────────────────────────┘ │
│                                                    │
│                       [稍后]  [开始下载并录音] ◀━━┓│
└─────────────────────────────────────────────────━━┛
```

下载态（user 点了"开始下载并录音"）：

```
┌─ 启用语音输入 ──────────────────────────────────  ✕ ┐
│                                                     │
│   下载中 · HuggingFace → 自动 fallback hf-mirror   │
│                                                    │
│   ┌─────────────────────────────────────────────┐ │
│   │ model_quant.onnx                            │ │
│   │ ████████████████░░░░░░░░░░░  62%  142/230MB │ │
│   └─────────────────────────────────────────────┘ │
│                                                    │
│   ✓ tokens.json    344KB                          │
│   ✓ am.mvn         11KB                           │
│   ✓ config.yaml    2KB                            │
│                                                    │
│   预计剩余 ~1m 12s                                │
│                                                    │
│   下载完成后将自动开始录音 ━━━━━━━━━━━━━━━━━━━    │
│                                                    │
│                              [后台继续]  [取消]   │
└─────────────────────────────────────────────────────┘
```

下载完成（自动过渡）：

```
┌─ 启用语音输入 ──────────────────────────────────  ✕ ┐
│                                                     │
│         ✓  模型已就绪                              │
│                                                    │
│         3 秒后自动开始录音…                        │
│                                                    │
│                              [立即开始]  [取消]   │
└─────────────────────────────────────────────────────┘
```

**关键体验细节**：
1. **意图保留**：用户原本是想录音 → 我们一路把他带到"已开始录音"，不打断意图。下载完成后默认 3s 倒计时自动进入录音态（可点"立即开始"提速，或"取消"中止）。
2. **后台继续**：点"后台继续"后弹窗关闭，下载在 settings 页继续可见；composer 上 mic 按钮显示进度小环（圆形 progress ring 围绕 mic icon），告知"在下载中"。完成时 toast 提示 + mic 按钮自动恢复正常。
3. **失败处理**：HuggingFace 慢 / 超时 / 失败 → 自动切 hf-mirror 重试（**深度优化 #6**：if2Ai 的 fallback 是隐式的，uClaw 在 UI 显式高亮"已切到镜像"）。两源都失败 → 显示错误 + 重试按钮 + 链接到设置页的「手动指定模型路径」（v0 不实现手动路径，但留 hook）。
4. **网络估算**：基于已下载量 / 用时 计算预计剩余，每 2s 更新。
5. **第一次"稍后"按钮**：弹窗关闭，mic 按钮显示"待激活"小红点（pulse 1 次），引导用户知道功能已"半启用"；下次点 mic 还是这个弹窗（never naggy，每次 click 都重新邀请，不会自动弹）。
6. **诚实坦白**：不藏体积。"~230MB 一次性"显眼可见 —— 用户决策有依据。
7. **可达性**：所有按钮 keyboard 可达；Esc = 取消下载并关闭。

**设置页里**对应的状态视图：同一个组件复用 —— 如果模型已下载，显示"✓ 已就绪 · 模型路径 · 重新下载"；下载中显示同款进度条；未下载显示"立即下载"。

## 8. Settings 页（`ui/src/components/settings/SttSettings.tsx`）

| 区块 | 控件 | 默认值 |
|---|---|---|
| **模型状态** | 徽章（✓已下载 · ⚠未下载 · ⟳下载中）+ 路径展示 + 下载/重新下载按钮 | 未下载 |
| 模型来源 | HuggingFace（含 hf-mirror 自动 fallback；显示当前活跃源） | auto |
| **默认语言** | Select: 自动 / 中文 / 英文 / 粤语 / 日文 / 韩文 | 自动 |
| **麦克风设备** | Select（`navigator.mediaDevices.enumerateDevices()`） | 系统默认 |
| **转写完自动发送** | Toggle | 关 |
| **快捷键** | 当前绑定 + "去快捷键设置" 链接 | `Cmd/Ctrl+Shift+M` |
| **权限状态** | macOS 麦克风权限徽章 + "打开系统设置"按钮（denied 时） | — |

UI 使用现有 `SettingsCard` / `SettingsSection` / `SettingsRow` 三件套（`primitives/SettingsUIConstants.ts` 提供 `LABEL_CLASS` / `ROW_CLASS` / `CARD_CLASS`）。所有控件遵循 11 主题 token 化原则。

## 9. 快捷键

注册 `toggle-stt-recording` 到 `ui/src/lib/shortcut-defaults.ts`：

```ts
{
  id: 'toggle-stt-recording',
  label: '语音输入开/关',
  group: 'Agent',  // 与 toggle-focus-mode 同组
  mac: 'Cmd+Shift+M',
  win: 'Ctrl+Shift+M',
}
```

行为：
- idle 态按下 → 在当前聚焦的 composer（ChatInput 或 AgentView）开始录音。
- recording 态按下 → stop + 转写（等价于点 ✓）。
- 模型未下载 → 弹 FirstRunDialog（与点 mic 等价）。
- 无聚焦 composer → no-op。

## 10. 配置文件改动

**`src-tauri/tauri.conf.json`**：
- `bundle.macOS.infoPlist.NSMicrophoneUsageDescription = "uClaw 需要麦克风以转写您的语音输入"`
- `app.security.csp.connect-src` 追加 `https://huggingface.co` 和 `https://hf-mirror.com`（仅模型下载用，不用于转写）。

**`src-tauri/capabilities/default.json`**：无需变更 —— `getUserMedia` 走浏览器层，不经 Tauri permission。

**`Cargo.toml`**：新增依赖
- `ort = { version = "2.0.0-rc.10", features = ["load-dynamic"] }`
- `realfft = "3"`（Fbank FFT）
- if2Ai 已经验证这些依赖在 macOS / Linux / Windows 上工作；ort `load-dynamic` 不绑死 onnxruntime 版本，bundler 在运行时解析。
- **已知代价**：首次 `cargo build` cold compile +~3min。在 PR 说明里标注。

## 11. 跨 Composer 集成

CLAUDE.md 硬规矩：ChatInput 和 AgentView 必须同步改。

**ChatInput** (`ui/src/components/chat/ChatInput.tsx:360`)：
- 升级现有 `<SpeechButton />` 占位符 → 真正接 `useSttRecording`。
- `onTranscribe(text)` → 调 `editorRef.current?.commands.insertContent(text)`（或 RichTextInput 暴露的等价 API）。
- 录音态：在 SpeechButton 右侧渲染 `<InlineRecorder />`，挤掉其他工具按钮（动画过渡）。
- 自动发送（settings 开启时）：录音完插入文本后调 `onSend(content)`，复用现有发送路径。

**AgentView** (`ui/src/components/agent/AgentView.tsx`)：
- 在 line ~1546 附近（工具按钮行）新增 `<SpeechButton />`，与 ChatInput 位置对齐（在 attach paperclip 之后、ContextUsageBadge 之前）。
- 集成方式与 ChatInput 镜像。
- 自动发送复用 `handleSend()`。

两边的 `onTranscribe` 实现差异最小 —— 都是"取 editorRef → insertContent → 可选 send"。

## 12. 测试目标

- **UI**：当前 389（focus-mode merged 后）→ ~415（+26）
  - `atoms/stt-atoms.test.ts` — 4
  - `lib/stt/audio-capture.test.ts` — 4（MediaRecorder / AudioContext 全 mock）
  - `hooks/useSttRecording.test.ts` — 8（state machine）
  - `components/stt/InlineRecorder.test.tsx` — 4
  - `components/stt/FirstRunDialog.test.tsx` — 4（**深度优化 #5 必须有测试覆盖**）
  - `components/ai-elements/speech-button.test.tsx` — 2
- **Rust**：522 → ~550（+28）
  - `stt/openflow/preprocess.rs` — 8（Fbank / LFR / CMVN 各 unit）
  - `stt/openflow/decoder.rs` — 10（CTC greedy + post-process）
  - `stt/openflow/engine.rs` — 4（end-to-end with mock ONNX）
  - `stt/download.rs` — 3（HF / hf-mirror / fallback 切换）
  - `stt/settings.rs` — 3（load/save/默认值）

**jsdom 限制**：`MediaRecorder` / `AudioContext` / `navigator.mediaDevices` 在 vitest 环境必须 mock；遵循 `ui/src/test-utils/setup.ts` 已有 localStorage shim 的先例添加 mock。

## 13. 风险 & 注意事项

- **ort crate 冷编译 ~3min**：已知；不能压缩；PR 说明里前置提醒。
- **bundle 体积**：ONNX Runtime 动态链接，bundle 增 ~30MB；模型 230MB 按需下载不入包。
- **macOS 权限**：首次录音由浏览器弹出系统对话框（getUserMedia 触发），后续静默；denied 时设置页显示状态 + 跳转系统设置链接。Linux/Windows 无类似流程。
- **AgentView ⇄ ChatInput 必须双改**：Task 13 一并提交；任何 paste/drop/submit/attach 类历史 PR 都因为漏改了一边导致回归，这次防住。
- **TipTap insertContent API**：先确认 uClaw 当前 RichTextInput 是 TipTap 还是 placeholder textarea（CLAUDE.md 注脚提到是 placeholder，W4 才会替换为 TipTap）。若仍是 textarea，则用 native `selectionStart/End` + `setRangeText` 实现光标插入。Task 13 在实施前先验 RichTextInput 真实形态再选实现路径。
- **CSP**：huggingface.co 和 hf-mirror.com 加到 connect-src；不影响 LLM provider 列表（仍走 anthropic.com / openai.com）。
- **跨 composer 单例**：用 jotai atom 做锁 —— 录音中第二个 composer 点 mic 显示 toast"已在另一个输入框中录音"。
- **快捷键冲突**：`Cmd+Shift+M` 在主流 IDE/浏览器都不常占用；用户可在快捷键设置改键。

## 14. 实现顺序（提示给计划编写者）

按 spec § 4 架构图自底向上：
1. 配置文件（Info.plist + CSP）—— 不阻塞业务但必须先有，否则录音 / 下载都跑不通。
2. Backend：preprocess → onnx_inference → decoder → engine（4 步基本是 if2Ai 文件直拷）。
3. Backend：download + settings + Tauri 命令注册。
4. Frontend：atoms → audio-capture → useSttRecording → InlineRecorder + FirstRunDialog → SpeechButton 升级。
5. Frontend：两个 composer 集成 + SttSettings + 快捷键。
6. 整体 verification。

每步都要绿（tsc + vitest + cargo test），bisectable。

## 15. 验证方式

- `cd src-tauri && cargo test stt::` → 28+ 测试全过。
- `cd ui && npm test -- --run` → 26+ 新增测试全过，无 jsdom 报错。
- `cd src-tauri && cargo build` → 全绿（首次 cold +~3min for ort）。
- 手测脚本（写进 plan 最后一个任务）：
  1. 启动应用，进 settings → STT，确认显示"未下载"。
  2. 进 ChatInput，点 mic → FirstRunDialog 出现。
  3. "稍后" → 关闭，mic 按钮"半启用"小点。
  4. 再点 mic → 同样 dialog；点"开始下载并录音" → 进度条；
  5. 完成后 3s 自动进入录音；说一句中文。
  6. ✓ → 文本注入光标位置。
  7. 设置开启"自动发送"，重复 → 自动 onSend。
  8. AgentView 重复 1-7。
  9. 快捷键 `Cmd+Shift+M` 启动/停止录音。
  10. 录音中切到 AgentView 点 mic → 提示"已在另一个输入框录音"。
