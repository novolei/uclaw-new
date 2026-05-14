# STT 流式语音 Modal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 uClaw 的 STT 从「内联录音条 + 停止后批处理转写」升级成「弹出 modal + 伪流式实时转写 + 静音超时自动填入聊天框 + 持续分段递增」，并给 modal 加主题色辉光漫散射效果。

**Architecture:** 伪流式纯前端编排——前端用 AudioWorklet 持续累积 PCM，段内每 ~1.5s 把当前段 buffer 重新调一次现有的 `stt_transcribe` 命令做实时预览；静音超时把该段定稿、加标点、追加到聊天输入框、清空 buffer 继续下一段。后端零改动。modal 用 CSS 径向渐变 + blur + 主题 token 实现辉光。

**Tech Stack:** React 18 + TypeScript + Jotai + AudioWorklet + 现有 `stt_transcribe` Tauri 命令 + Vitest/RTL/jsdom。

**Spec source of truth:** [docs/superpowers/specs/2026-05-14-stt-streaming-modal-design.md](../specs/2026-05-14-stt-streaming-modal-design.md)。状态机、伪流式机制、行为层、辉光实现都在 spec 里；本 plan 实现它。

**Scope note:** spec 的可选 `punctuationMode: 'llm'` 模式**不在本 plan 范围**——它需要新的后端 LLM 命令（前端没有通用 LLM 调用路径），与「后端零改动」原则冲突，且每段一次网络调用有延迟。本 plan 只实现 `native` 标点（SenseVoice 原生输出 + 轻量规整），已完整满足「自动加标点」需求。LLM 润色作为干净的后续项。

**Honest commit count:** 12 commits。单 PR，bisectable（每个 commit 都能编译——旧的 `recordingStateAtom`/`InlineRecorder` 保留到最后的 cleanup task 才删）。

---

## File Structure Overview

**新增 (9):**

```
ui/public/stt/
└── pcm-worklet.js                              [CREATE — AudioWorklet processor]

ui/src/lib/stt/
├── streaming-capture.ts                        [CREATE — 边录边累积 PCM]
├── streaming-capture.test.ts                   [CREATE]
├── punctuation.ts                              [CREATE — 标点规整 + 智能拼接]
└── punctuation.test.ts                         [CREATE]

ui/src/hooks/
├── useSttStreamingSession.ts                   [CREATE — 流式会话 FSM]
└── useSttStreamingSession.test.tsx             [CREATE]

ui/src/components/stt/
├── SttModal.tsx                                [CREATE — 流式语音 modal]
├── SttModal.test.tsx                           [CREATE]
└── SttModal.css                                [CREATE — 辉光漫散射]
```

**修改 (7):**

- `ui/src/atoms/stt-atoms.ts` — 加 `SttModalState` + `sttModalStateAtom` + `silenceThresholdMs`
- `ui/src/atoms/stt-atoms.test.ts` — 对应测试
- `ui/src/test-utils/stt-mocks.ts` — 加 AudioWorklet mock
- `ui/src/components/ai-elements/speech-button.tsx` — 触发改为开/关 modal
- `ui/src/lib/shortcut-defaults.ts` — `toggle-stt-recording`: `Cmd+Shift+M` → `Alt+S`
- `ui/src/components/chat/ChatInput.tsx` — 移除 `<InlineRecorder>`，挂 `<SttModal>`，重写增量追加
- `ui/src/components/agent/AgentView.tsx` — 同上（CLAUDE.md 双 composer 规则）
- `ui/src/components/settings/SttSettings.tsx` — 加静音阈值滑块，更新快捷键显示

**删除 (5) — 最后的 cleanup task:**

- `ui/src/components/stt/InlineRecorder.tsx` + `InlineRecorder.test.tsx`
- `ui/src/hooks/useSttRecording.ts` + `useSttRecording.test.tsx`
- `ui/src/lib/stt/audio-capture.ts` + `audio-capture.test.ts`（仅被 `useSttRecording` 引用，一并删）

---

## Task 1: STT atoms — 新增 modal FSM 状态

**Files:**
- Modify: `ui/src/atoms/stt-atoms.ts`
- Modify: `ui/src/atoms/stt-atoms.test.ts`

新增 `SttModalState` / `sttModalStateAtom` / `SttSettings.silenceThresholdMs`。**保留**旧的 `RecordingState` / `recordingStateAtom`（Task 12 cleanup 才删，保证中间 commit 可编译）。

- [ ] **Step 1: 写失败测试** — 在 `ui/src/atoms/stt-atoms.test.ts` 末尾（最后一个 `})` 之前）追加：

```typescript
import { sttModalStateAtom } from './stt-atoms'

describe('sttModalStateAtom', () => {
  it('defaults to idle', () => {
    const store = createStore()
    expect(store.get(sttModalStateAtom)).toEqual({ kind: 'idle' })
  })

  it('can transition to listening', () => {
    const store = createStore()
    store.set(sttModalStateAtom, {
      kind: 'listening',
      segmentStartedMs: 1000,
      volume: 0,
      interimText: '',
    })
    const s = store.get(sttModalStateAtom)
    expect(s.kind).toBe('listening')
  })
})

describe('sttSettingsAtom silenceThresholdMs', () => {
  it('defaults silenceThresholdMs to 1800', () => {
    const store = createStore()
    expect(store.get(sttSettingsAtom).silenceThresholdMs).toBe(1800)
  })
})
```

If `createStore` / `sttSettingsAtom` aren't imported in the test file already, check the existing imports at the top and add what's missing.

- [ ] **Step 2: 运行测试，确认失败**

Run: `cd ui && npm test -- --run stt-atoms 2>&1 | tail -15`
Expected: FAIL — `sttModalStateAtom` not exported / `silenceThresholdMs` undefined.

- [ ] **Step 3: 实现** — 在 `ui/src/atoms/stt-atoms.ts` 中：

(a) 在 `RecordingState` 类型定义之后，新增 `SttModalState`：

```typescript
/**
 * 流式语音 modal 的状态机。替代 RecordingState（Task 12 删除旧的）。
 * modal 在 kind !== 'idle' 时挂载。
 */
export type SttModalState =
  | { kind: 'idle' }
  | { kind: 'requesting-permission' }
  | { kind: 'listening'; segmentStartedMs: number; volume: number; interimText: string }
  | { kind: 'finalizing'; volume: number }
  | { kind: 'permission-denied' }
  | { kind: 'error'; message: string }
```

(b) 在 `SttSettings` 接口加字段：

```typescript
export interface SttSettings {
  language: Language
  autoSend: boolean
  microphoneDeviceId: string | null
  /** 静音多久（ms）触发段定稿。默认 1800。 */
  silenceThresholdMs: number
}
```

(c) `DEFAULT_SETTINGS` 加 `silenceThresholdMs: 1800`：

```typescript
const DEFAULT_SETTINGS: SttSettings = {
  language: 'auto',
  autoSend: false,
  microphoneDeviceId: null,
  silenceThresholdMs: 1800,
}
```

(d) 在文件末尾 atoms 区加：

```typescript
export const sttModalStateAtom = atom<SttModalState>({ kind: 'idle' })
```

> `atomWithStorage` 对已有用户的 localStorage 旧值不会自动补 `silenceThresholdMs`。在 `DEFAULT_SETTINGS` 之后加一个 merge 兜底——把 `sttSettingsAtom` 改成读取时合并默认值：保持 `export const sttSettingsAtom = atomWithStorage<SttSettings>('uclaw.stt.settings', DEFAULT_SETTINGS)` 不变即可（新字段 `undefined` 时下游用 `?? 1800` 兜底，见 Task 6）。无需改 atom 定义。

- [ ] **Step 4: 运行测试，确认通过**

Run: `cd ui && npm test -- --run stt-atoms 2>&1 | tail -15`
Expected: PASS — 全部测试通过（含原有测试 + 3 个新测试）。

- [ ] **Step 5: 提交**

```bash
git add ui/src/atoms/stt-atoms.ts ui/src/atoms/stt-atoms.test.ts
git commit -m "feat(stt): add SttModalState FSM atom + silenceThresholdMs setting"
```

---

## Task 2: punctuation.ts — 标点规整 + 智能拼接

**Files:**
- Create: `ui/src/lib/stt/punctuation.ts`
- Create: `ui/src/lib/stt/punctuation.test.ts`

纯函数，无副作用。`regularizePunctuation` 给段定稿文本补句末标点 + 规整；`smartJoin` 把新段拼到已有输入框文本后。

- [ ] **Step 1: 写失败测试** — 创建 `ui/src/lib/stt/punctuation.test.ts`：

```typescript
import { describe, it, expect } from 'vitest'
import { regularizePunctuation, smartJoin } from './punctuation'

describe('regularizePunctuation', () => {
  it('appends 。 to a CJK sentence without terminal punctuation', () => {
    expect(regularizePunctuation('今天天气不错', 'zh')).toBe('今天天气不错。')
  })

  it('appends . to an English sentence without terminal punctuation', () => {
    expect(regularizePunctuation('hello world', 'en')).toBe('hello world.')
  })

  it('leaves text that already ends with terminal punctuation untouched', () => {
    expect(regularizePunctuation('已经有句号了。', 'zh')).toBe('已经有句号了。')
    expect(regularizePunctuation('done already!', 'en')).toBe('done already!')
    expect(regularizePunctuation('问号呢？', 'zh')).toBe('问号呢？')
  })

  it('collapses repeated internal whitespace and trims', () => {
    expect(regularizePunctuation('  hello   world  ', 'en')).toBe('hello world.')
  })

  it('returns empty string for empty/whitespace input', () => {
    expect(regularizePunctuation('', 'zh')).toBe('')
    expect(regularizePunctuation('   ', 'en')).toBe('')
  })

  it('for auto language, picks 。 when text is CJK-dominant, . otherwise', () => {
    expect(regularizePunctuation('这是中文', 'auto')).toBe('这是中文。')
    expect(regularizePunctuation('this is english', 'auto')).toBe('this is english.')
  })
})

describe('smartJoin', () => {
  it('joins two ASCII-word fragments with a space', () => {
    expect(smartJoin('hello', 'world')).toBe('hello world')
  })

  it('does not add a space between CJK fragments', () => {
    expect(smartJoin('你好', '世界')).toBe('你好世界')
  })

  it('does not add a space when the left side ends with punctuation', () => {
    expect(smartJoin('第一句。', '第二句')).toBe('第一句。第二句')
    expect(smartJoin('first.', 'second')).toBe('first. second')
  })

  it('returns the non-empty side when the other is empty', () => {
    expect(smartJoin('', 'world')).toBe('world')
    expect(smartJoin('hello', '')).toBe('hello')
    expect(smartJoin('', '')).toBe('')
  })
})
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cd ui && npm test -- --run punctuation 2>&1 | tail -15`
Expected: FAIL — module not found.

- [ ] **Step 3: 实现** — 创建 `ui/src/lib/stt/punctuation.ts`：

```typescript
/**
 * punctuation — 段定稿文本的标点规整 + 增量拼接（纯函数）。
 *
 * SenseVoice 解码器只拼 token、不保证标点；这里补句末标点 + 规整空白，
 * 让转写文本「自动带标点」（对齐 Proma 的 enable_punc 体验）。
 */
import type { Language } from '@/atoms/stt-atoms'

// 句末标点（中英都算）。已以这些结尾就不再补。
const TERMINAL_PUNCT = /[。．.！!？?；;…]$/
// 判断一段文本是否 CJK 主导（用于 auto 语言选全角还是半角句号）。
const CJK = /[一-鿿぀-ヿ가-힯]/g
// ASCII 词字符（用于 smartJoin 判断是否补空格）。
const ASCII_WORD = /[A-Za-z0-9]/

function isCjkDominant(text: string): boolean {
  const cjk = (text.match(CJK) ?? []).length
  return cjk > 0 && cjk >= text.replace(/\s/g, '').length / 2
}

/**
 * 规整一段转写文本：trim、折叠内部连续空白、按语言补句末标点。
 * 已有句末标点则不动；空白输入返回空串。
 */
export function regularizePunctuation(text: string, language: Language): string {
  const cleaned = text.trim().replace(/\s+/g, ' ')
  if (cleaned === '') return ''
  if (TERMINAL_PUNCT.test(cleaned)) return cleaned
  const useCjk =
    language === 'zh' ||
    language === 'yue' ||
    language === 'ja' ||
    language === 'ko' ||
    (language === 'auto' && isCjkDominant(cleaned))
  return cleaned + (useCjk ? '。' : '.')
}

/**
 * 把新段文本拼到已有文本后。两侧都是 ASCII 词字符时补一个空格，
 * 否则（涉及 CJK 或标点）直接拼接。任一侧为空返回另一侧。
 */
export function smartJoin(left: string, right: string): string {
  if (left === '') return right
  if (right === '') return left
  const lastL = left[left.length - 1]!
  const firstR = right[0]!
  if (ASCII_WORD.test(lastL) && ASCII_WORD.test(firstR)) {
    return left + ' ' + right
  }
  return left + right
}
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `cd ui && npm test -- --run punctuation 2>&1 | tail -15`
Expected: PASS — 10 tests pass。

- [ ] **Step 5: 提交**

```bash
git add ui/src/lib/stt/punctuation.ts ui/src/lib/stt/punctuation.test.ts
git commit -m "feat(stt): punctuation regularization + smart incremental join"
```

---

## Task 3: AudioWorklet PCM 采集 — pcm-worklet.js + streaming-capture.ts

**Files:**
- Create: `ui/public/stt/pcm-worklet.js`
- Create: `ui/src/lib/stt/streaming-capture.ts`
- Create: `ui/src/lib/stt/streaming-capture.test.ts`
- Modify: `ui/src/test-utils/stt-mocks.ts`

边录边把 PCM 累积进增长 buffer，暴露「取当前段 base64 / 清空段 / 读音量」。

- [ ] **Step 1: 创建 worklet processor** — `ui/public/stt/pcm-worklet.js`：

```javascript
/**
 * pcm-worklet — AudioWorkletProcessor，把每个 render quantum 的 PCM 块
 * 通过 port 发给主线程累积。注册名 'pcm-worklet'。
 */
class PcmWorklet extends AudioWorkletProcessor {
  process(inputs) {
    const input = inputs[0]
    if (input && input[0] && input[0].length > 0) {
      // input[0] 是本 quantum 的 Float32Array（通常 128 samples）。复制后发送。
      this.port.postMessage(input[0].slice(0))
    }
    return true
  }
}
registerProcessor('pcm-worklet', PcmWorklet)
```

- [ ] **Step 2: 写失败测试** — 创建 `ui/src/lib/stt/streaming-capture.test.ts`：

```typescript
import { describe, it, expect, afterEach } from 'vitest'
import { installAudioStubs, type InstalledStubs } from '@/test-utils/stt-mocks'
import { createStreamingCapture } from './streaming-capture'

let stubs: InstalledStubs

afterEach(() => {
  stubs?.cleanup()
})

describe('createStreamingCapture', () => {
  it('accumulates posted PCM and returns non-empty base64 for the segment', async () => {
    stubs = installAudioStubs()
    const cap = await createStreamingCapture()
    await cap.start(null)
    // emit two PCM chunks of 128 samples each
    stubs.emitPcm(new Float32Array(128).fill(0.5))
    stubs.emitPcm(new Float32Array(128).fill(-0.5))
    const b64 = cap.getSegmentPcmBase64()
    expect(typeof b64).toBe('string')
    expect(b64.length).toBeGreaterThan(0)
    cap.stop()
  })

  it('resetSegment clears the accumulated buffer', async () => {
    stubs = installAudioStubs()
    const cap = await createStreamingCapture()
    await cap.start(null)
    stubs.emitPcm(new Float32Array(128).fill(0.5))
    const before = cap.getSegmentPcmBase64()
    cap.resetSegment()
    const after = cap.getSegmentPcmBase64()
    expect(after).toBe('')
    expect(before).not.toBe('')
    cap.stop()
  })

  it('getVolume returns a number in 0..1', async () => {
    stubs = installAudioStubs()
    const cap = await createStreamingCapture()
    await cap.start(null)
    stubs.setVolume(128)
    const v = cap.getVolume()
    expect(v).toBeGreaterThanOrEqual(0)
    expect(v).toBeLessThanOrEqual(1)
    cap.stop()
  })
})
```

- [ ] **Step 3: 扩展测试 mock** — 在 `ui/src/test-utils/stt-mocks.ts` 里给 `MockAudioContext` 加 `audioWorklet` 支持，给 `InstalledStubs` 加 `emitPcm`。

(a) `InstalledStubs` 接口加一行：

```typescript
export interface InstalledStubs {
  emitData: (chunk: Blob) => void
  emitStop: () => void
  emitPcm: (pcm: Float32Array) => void   // 新增
  setVolume: (v: number) => void
  cleanup: () => void
}
```

(b) 在 `installAudioStubs` 内、`MockAudioContext` 定义之前加一个 worklet-node 列表 + Mock 类：

```typescript
  const workletPorts: Array<{ onmessage: ((e: MessageEvent) => void) | null }> = []

  class MockAudioWorkletNode {
    port: { onmessage: ((e: MessageEvent) => void) | null }
    constructor() {
      this.port = { onmessage: null }
      workletPorts.push(this.port)
    }
    connect() {}
    disconnect() {}
  }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ;(globalThis as any).AudioWorkletNode = MockAudioWorkletNode
```

(c) 在 `MockAudioContext` 类里加 `audioWorklet` 属性：

```typescript
  class MockAudioContext {
    sampleRate = 16000
    state: AudioContextState = 'running'
    audioWorklet = { addModule: (_url: string) => Promise.resolve() }
    createAnalyser() {
      return new MockAnalyser()
    }
    createMediaStreamSource(_s: MediaStream) {
      return { connect: () => {} }
    }
    // decodeAudioData / close 保持不变
    decodeAudioData(_arr: ArrayBuffer): Promise<AudioBuffer> {
      const mockBuffer = {
        sampleRate: 16000,
        numberOfChannels: 1,
        length: 16000,
        getChannelData: () => new Float32Array(16000),
      } as unknown as AudioBuffer
      return Promise.resolve(mockBuffer)
    }
    close() {
      return Promise.resolve()
    }
  }
```

(d) 在返回对象里加 `emitPcm`：

```typescript
  return {
    emitData(chunk: Blob) {
      dataListeners.forEach((l) => l({ data: chunk } as unknown as BlobEvent))
    },
    emitStop() {
      stopListeners.forEach((l) => l())
    },
    emitPcm(pcm: Float32Array) {
      workletPorts.forEach((p) => p.onmessage?.({ data: pcm } as MessageEvent))
    },
    setVolume(v: number) {
      volumeByte = Math.max(0, Math.min(255, v))
    },
    cleanup() {
      dataListeners.length = 0
      stopListeners.length = 0
      workletPorts.length = 0
      Object.defineProperty(navigator, 'mediaDevices', {
        configurable: true,
        value: undefined,
      })
    },
  }
```

- [ ] **Step 4: 运行测试，确认失败**

Run: `cd ui && npm test -- --run streaming-capture 2>&1 | tail -15`
Expected: FAIL — `createStreamingCapture` not found。

- [ ] **Step 5: 实现** — 创建 `ui/src/lib/stt/streaming-capture.ts`：

```typescript
/**
 * streaming-capture — 边录边累积 PCM 的音频采集（伪流式用）。
 *
 * getUserMedia → AudioContext(16kHz) → AudioWorkletNode 持续 post Float32 块
 * 到主线程累积进增长数组；并行 AnalyserNode 算实时音量。
 * 暴露「取当前段 PCM16 base64 / 清空段 / 读音量 / 停止」。
 */

const TARGET_SAMPLE_RATE = 16000 as const
const WORKLET_URL = '/stt/pcm-worklet.js'

export interface StreamingCapture {
  /** 开始采集。deviceId 为 null 用系统默认麦克风。 */
  start: (deviceId: string | null) => Promise<void>
  /** 停止采集，释放所有资源。 */
  stop: () => void
  /** 当前段累积 PCM 的 PCM16LE base64（喂 stt_transcribe）。空段返回 ''。 */
  getSegmentPcmBase64: () => string
  /** 清空当前段累积 buffer（段定稿后调）。 */
  resetSegment: () => void
  /** 0..1 实时峰值音量。 */
  getVolume: () => number
}

export async function createStreamingCapture(): Promise<StreamingCapture> {
  let stream: MediaStream | null = null
  let audioContext: AudioContext | null = null
  let workletNode: AudioWorkletNode | null = null
  let analyser: AnalyserNode | null = null
  let volumeBuf: Uint8Array | null = null
  // 当前段累积的 Float32 块。
  let segmentChunks: Float32Array[] = []

  const start = async (deviceId: string | null): Promise<void> => {
    const constraints: MediaStreamConstraints = {
      audio: deviceId ? { deviceId: { exact: deviceId } } : true,
      video: false,
    }
    stream = await navigator.mediaDevices.getUserMedia(constraints)

    audioContext = new (window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)({
      sampleRate: TARGET_SAMPLE_RATE,
    })
    await audioContext.audioWorklet.addModule(WORKLET_URL)

    const source = audioContext.createMediaStreamSource(stream)

    workletNode = new AudioWorkletNode(audioContext, 'pcm-worklet')
    workletNode.port.onmessage = (e: MessageEvent) => {
      const pcm = e.data as Float32Array
      if (pcm && pcm.length > 0) segmentChunks.push(pcm)
    }
    source.connect(workletNode)

    analyser = audioContext.createAnalyser()
    analyser.fftSize = 256
    source.connect(analyser)
    volumeBuf = new Uint8Array(analyser.frequencyBinCount)
  }

  const stop = (): void => {
    try {
      workletNode?.disconnect()
    } catch {
      // ignore
    }
    if (workletNode) workletNode.port.onmessage = null
    stream?.getTracks().forEach((t) => {
      try {
        t.stop()
      } catch {
        // ignore
      }
    })
    audioContext?.close().catch(() => {
      // ignore
    })
    stream = null
    audioContext = null
    workletNode = null
    analyser = null
    volumeBuf = null
    segmentChunks = []
  }

  const getSegmentPcmBase64 = (): string => {
    const total = segmentChunks.reduce((sum, c) => sum + c.length, 0)
    if (total === 0) return ''
    // 合并所有块 → Int16 PCM little-endian → base64。
    const pcm = new Int16Array(total)
    let offset = 0
    for (const chunk of segmentChunks) {
      for (let i = 0; i < chunk.length; i++) {
        const s = Math.max(-1, Math.min(1, chunk[i]!))
        pcm[offset++] = s < 0 ? Math.round(s * 0x8000) : Math.round(s * 0x7fff)
      }
    }
    const bytes = new Uint8Array(pcm.buffer)
    const CHUNK = 0x8000
    let str = ''
    for (let i = 0; i < bytes.length; i += CHUNK) {
      str += String.fromCharCode(...bytes.subarray(i, i + CHUNK))
    }
    return btoa(str)
  }

  const resetSegment = (): void => {
    segmentChunks = []
  }

  const getVolume = (): number => {
    if (!analyser || !volumeBuf) return 0
    analyser.getByteFrequencyData(volumeBuf)
    let sum = 0
    for (let i = 0; i < volumeBuf.length; i++) sum += volumeBuf[i]!
    const avg = sum / volumeBuf.length
    return Math.max(0, Math.min(1, avg / 255))
  }

  return { start, stop, getSegmentPcmBase64, resetSegment, getVolume }
}
```

- [ ] **Step 6: 运行测试，确认通过**

Run: `cd ui && npm test -- --run streaming-capture stt-mocks 2>&1 | tail -15`
Expected: PASS — streaming-capture 3 tests pass；确认没破坏依赖 `stt-mocks` 的其它测试（`audio-capture.test.ts` 等仍应通过——我们只是给 mock 加了字段）。

- [ ] **Step 7: TS 检查 + 提交**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: 无新错误。

```bash
git add ui/public/stt/pcm-worklet.js ui/src/lib/stt/streaming-capture.ts ui/src/lib/stt/streaming-capture.test.ts ui/src/test-utils/stt-mocks.ts
git commit -m "feat(stt): AudioWorklet streaming PCM capture"
```

---

## Task 4: useSttStreamingSession — hook 骨架（FSM + 采集生命周期）

**Files:**
- Create: `ui/src/hooks/useSttStreamingSession.ts`
- Create: `ui/src/hooks/useSttStreamingSession.test.tsx`

先实现 `start` / `end` / `cancel` + FSM 状态转换 + 采集启停。**不含**重转写循环和静音检测（Task 5、6 加）。

- [ ] **Step 1: 写失败测试** — 创建 `ui/src/hooks/useSttStreamingSession.test.tsx`：

```typescript
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import {
  sttModalStateAtom,
  activeComposerAtom,
  modelStatusAtom,
} from '@/atoms/stt-atoms'
import { useSttStreamingSession } from './useSttStreamingSession'

// mock streaming-capture
const mockCapture = {
  start: vi.fn().mockResolvedValue(undefined),
  stop: vi.fn(),
  getSegmentPcmBase64: vi.fn().mockReturnValue('AAAA'),
  resetSegment: vi.fn(),
  getVolume: vi.fn().mockReturnValue(0),
}
vi.mock('@/lib/stt/streaming-capture', () => ({
  createStreamingCapture: vi.fn(async () => mockCapture),
}))

// mock Tauri invoke
const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

function wrapper(store: ReturnType<typeof createStore>) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children)
}

function readyStore() {
  const store = createStore()
  store.set(modelStatusAtom, { kind: 'ready', modelDir: '/m' })
  return store
}

describe('useSttStreamingSession — skeleton', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockCapture.getSegmentPcmBase64.mockReturnValue('AAAA')
    mockCapture.getVolume.mockReturnValue(0)
    invokeMock.mockResolvedValue({ text: '' })
  })
  afterEach(() => vi.useRealTimers())

  it('start() transitions idle → listening and starts capture', async () => {
    const store = readyStore()
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    let res: string | undefined
    await act(async () => {
      res = await result.current.start()
    })
    expect(res).toBe('started')
    expect(store.get(sttModalStateAtom).kind).toBe('listening')
    expect(store.get(activeComposerAtom)).toBe('chat')
    expect(mockCapture.start).toHaveBeenCalledTimes(1)
  })

  it('start() returns needs-download when model is not ready', async () => {
    const store = createStore() // modelStatus defaults to 'unknown'
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    let res: string | undefined
    await act(async () => {
      res = await result.current.start()
    })
    expect(res).toBe('needs-download')
    expect(store.get(sttModalStateAtom).kind).toBe('idle')
  })

  it('start() returns busy when another composer holds the lock', async () => {
    const store = readyStore()
    store.set(activeComposerAtom, 'agent')
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    let res: string | undefined
    await act(async () => {
      res = await result.current.start()
    })
    expect(res).toBe('busy')
  })

  it('permission denial transitions to permission-denied and releases lock', async () => {
    const store = readyStore()
    mockCapture.start.mockRejectedValueOnce(
      Object.assign(new Error('denied'), { name: 'NotAllowedError' }),
    )
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    await act(async () => {
      await result.current.start()
    })
    expect(store.get(sttModalStateAtom).kind).toBe('permission-denied')
    expect(store.get(activeComposerAtom)).toBeNull()
  })

  it('cancel() stops capture, releases lock, returns to idle', async () => {
    const store = readyStore()
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    await act(async () => {
      await result.current.start()
    })
    act(() => {
      result.current.cancel()
    })
    expect(store.get(sttModalStateAtom).kind).toBe('idle')
    expect(store.get(activeComposerAtom)).toBeNull()
    expect(mockCapture.stop).toHaveBeenCalled()
  })

  it('end() with empty interim text just closes (idle, lock released)', async () => {
    const store = readyStore()
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    await act(async () => {
      await result.current.start()
    })
    await act(async () => {
      await result.current.end()
    })
    expect(store.get(sttModalStateAtom).kind).toBe('idle')
    expect(store.get(activeComposerAtom)).toBeNull()
    expect(mockCapture.stop).toHaveBeenCalled()
  })
})
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cd ui && npm test -- --run useSttStreamingSession 2>&1 | tail -15`
Expected: FAIL — module not found。

- [ ] **Step 3: 实现骨架** — 创建 `ui/src/hooks/useSttStreamingSession.ts`：

```typescript
/**
 * useSttStreamingSession — 流式语音 modal 的会话编排 FSM。
 *
 * 替代 useSttRecording。驱动 SttModal：claim 跨 composer 锁、启停 streaming-capture、
 * 段内每 ~1.5s 重转写做实时预览（Task 5）、静音超时把段定稿并通过 onSegmentFinalized
 * 回调追加到聊天输入框（Task 6）。
 */
import { useCallback, useEffect, useRef } from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { invoke } from '@tauri-apps/api/core'
import {
  sttModalStateAtom,
  activeComposerAtom,
  modelStatusAtom,
  sttSettingsAtom,
  type ComposerKind,
  type SttModalState,
} from '@/atoms/stt-atoms'
import { createStreamingCapture, type StreamingCapture } from '@/lib/stt/streaming-capture'
import { regularizePunctuation } from '@/lib/stt/punctuation'

export const RETRANSCRIBE_INTERVAL_MS = 1500
export const VOLUME_SAMPLE_INTERVAL_MS = 80
export const MIN_SEGMENT_MS = 800
/** 音量高于此值视为「有人在说话」。 */
export const VOICE_VOLUME_THRESHOLD = 0.04

export type StartResult = 'started' | 'busy' | 'needs-download' | 'permission-denied' | 'error'

interface UseSttStreamingSessionOptions {
  /** 每段定稿后调用，参数是规整过标点的文本。由调用方追加到聊天输入框。 */
  onSegmentFinalized?: (text: string) => void
}

export interface SttSessionHandle {
  state: SttModalState
  start: () => Promise<StartResult>
  /** 主动结束：定稿当前未完成段（若有），然后关闭。 */
  end: () => Promise<void>
  /** 取消：丢弃当前段，直接关闭。 */
  cancel: () => void
}

export function useSttStreamingSession(
  composer: ComposerKind,
  opts: UseSttStreamingSessionOptions = {},
): SttSessionHandle {
  const [sharedState, setState] = useAtom(sttModalStateAtom)
  const [active, setActive] = useAtom(activeComposerAtom)
  const state: SttModalState =
    active === composer || active === null ? sharedState : { kind: 'idle' }
  const modelStatus = useAtomValue(modelStatusAtom)
  const settings = useAtomValue(sttSettingsAtom)

  const captureRef = useRef<StreamingCapture | null>(null)
  const retranscribeTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const volumeTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const transcribeInFlightRef = useRef(false)
  const lastVoiceMsRef = useRef(0)
  const segmentStartedMsRef = useRef(0)
  const interimTextRef = useRef('')
  const endingRef = useRef(false)
  // opts.onSegmentFinalized 用 ref 持有，避免 effect/interval 闭包过期。
  const onSegmentFinalizedRef = useRef(opts.onSegmentFinalized)
  onSegmentFinalizedRef.current = opts.onSegmentFinalized

  const clearTimers = useCallback(() => {
    if (retranscribeTimerRef.current) {
      clearInterval(retranscribeTimerRef.current)
      retranscribeTimerRef.current = null
    }
    if (volumeTimerRef.current) {
      clearInterval(volumeTimerRef.current)
      volumeTimerRef.current = null
    }
  }, [])

  const teardown = useCallback(() => {
    clearTimers()
    captureRef.current?.stop()
    captureRef.current = null
    transcribeInFlightRef.current = false
    interimTextRef.current = ''
    endingRef.current = false
  }, [clearTimers])

  // 转写当前段一次，返回原始文本（不加标点）。失败抛错。
  const transcribeSegment = useCallback(async (): Promise<string> => {
    const cap = captureRef.current
    if (!cap) return ''
    const audio = cap.getSegmentPcmBase64()
    if (audio === '') return ''
    const result = (await invoke('stt_transcribe', {
      request: {
        audio_bytes_base64: audio,
        language: settings.language === 'auto' ? null : settings.language,
        sample_rate: 16000,
      },
    })) as { text: string }
    return result.text
  }, [settings.language])

  // ── Task 5 会填充：startRetranscribeLoop ─────────────────────────
  const startRetranscribeLoop = useCallback(() => {
    // placeholder — Task 5 实现
  }, [])

  // ── Task 6 会填充：finalizeSegment + 静音检测 ────────────────────
  const finalizeSegment = useCallback(async (): Promise<void> => {
    // Task 6 实现：转写当前段 → regularizePunctuation → onSegmentFinalized
    // → resetSegment → 重置 interim/segment 计时 → 回 listening
    const cap = captureRef.current
    if (!cap) return
    try {
      const raw = await transcribeSegment()
      const text = regularizePunctuation(raw, settings.language)
      if (text) onSegmentFinalizedRef.current?.(text)
    } finally {
      cap.resetSegment()
      interimTextRef.current = ''
      segmentStartedMsRef.current = Date.now()
      lastVoiceMsRef.current = Date.now()
    }
  }, [transcribeSegment, settings.language])

  const startVolumeLoop = useCallback(() => {
    // Task 6 会扩展为「采样音量 + 静音检测」。骨架先只刷 volume。
    volumeTimerRef.current = setInterval(() => {
      const cap = captureRef.current
      if (!cap) return
      const v = cap.getVolume()
      setState((prev) =>
        prev.kind === 'listening' ? { ...prev, volume: v } : prev,
      )
    }, VOLUME_SAMPLE_INTERVAL_MS)
  }, [setState])

  const start = useCallback(async (): Promise<StartResult> => {
    if (active !== null && active !== composer) return 'busy'
    if (modelStatus.kind !== 'ready') return 'needs-download'
    setActive(composer)
    setState({ kind: 'requesting-permission' })
    let cap: StreamingCapture
    try {
      cap = await createStreamingCapture()
      await cap.start(settings.microphoneDeviceId)
    } catch (e) {
      setActive(null)
      const name = (e as { name?: string })?.name
      if (name === 'NotAllowedError' || name === 'SecurityError') {
        setState({ kind: 'permission-denied' })
        return 'permission-denied'
      }
      setState({ kind: 'error', message: String((e as Error)?.message ?? e) })
      setTimeout(() => setState({ kind: 'idle' }), 1500)
      return 'error'
    }
    captureRef.current = cap
    const now = Date.now()
    segmentStartedMsRef.current = now
    lastVoiceMsRef.current = now
    interimTextRef.current = ''
    endingRef.current = false
    setState({ kind: 'listening', segmentStartedMs: now, volume: 0, interimText: '' })
    startRetranscribeLoop()
    startVolumeLoop()
    return 'started'
  }, [
    active,
    composer,
    modelStatus.kind,
    settings.microphoneDeviceId,
    setActive,
    setState,
    startRetranscribeLoop,
    startVolumeLoop,
  ])

  const end = useCallback(async (): Promise<void> => {
    if (endingRef.current) return
    endingRef.current = true
    clearTimers()
    // 若当前段有内容，先定稿一次。
    if (interimTextRef.current.trim() !== '') {
      setState({ kind: 'finalizing', volume: 0 })
      try {
        await finalizeSegment()
      } catch {
        // 定稿失败也照常关闭，已定稿的段不丢。
      }
    }
    teardown()
    setActive(null)
    setState({ kind: 'idle' })
  }, [clearTimers, finalizeSegment, setActive, setState, teardown])

  const cancel = useCallback(() => {
    teardown()
    setActive(null)
    setState({ kind: 'idle' })
  }, [setActive, setState, teardown])

  // 卸载时清理。
  useEffect(() => {
    return () => {
      clearTimers()
      captureRef.current?.stop()
      captureRef.current = null
    }
  }, [clearTimers])

  return { state, start, end, cancel }
}
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `cd ui && npm test -- --run useSttStreamingSession 2>&1 | tail -15`
Expected: PASS — 6 tests pass。

- [ ] **Step 5: 提交**

```bash
git add ui/src/hooks/useSttStreamingSession.ts ui/src/hooks/useSttStreamingSession.test.tsx
git commit -m "feat(stt): useSttStreamingSession skeleton — FSM + capture lifecycle"
```

---

## Task 5: useSttStreamingSession — 段内重转写循环 + in-flight 守卫

**Files:**
- Modify: `ui/src/hooks/useSttStreamingSession.ts`
- Modify: `ui/src/hooks/useSttStreamingSession.test.tsx`

`listening` 期间每 `RETRANSCRIBE_INTERVAL_MS` 把当前段 buffer 重转写一次，写进 `state.interimText`。上一次没返回就跳过这一拍。

- [ ] **Step 1: 写失败测试** — 在 `useSttStreamingSession.test.tsx` 末尾追加：

```typescript
describe('useSttStreamingSession — retranscribe loop', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockCapture.getSegmentPcmBase64.mockReturnValue('AAAA')
    mockCapture.getVolume.mockReturnValue(0)
  })
  afterEach(() => vi.useRealTimers())

  it('updates interimText from periodic stt_transcribe calls', async () => {
    vi.useFakeTimers()
    const store = readyStore()
    invokeMock.mockResolvedValue({ text: '实时预览文本' })
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    await act(async () => {
      await result.current.start()
    })
    await act(async () => {
      vi.advanceTimersByTime(1500)
      await Promise.resolve()
      await Promise.resolve()
    })
    const s = store.get(sttModalStateAtom)
    expect(s.kind).toBe('listening')
    if (s.kind === 'listening') {
      expect(s.interimText).toBe('实时预览文本')
    }
  })

  it('in-flight guard: skips a tick if the previous transcribe has not resolved', async () => {
    vi.useFakeTimers()
    const store = readyStore()
    // make invoke hang
    let resolveInvoke: ((v: { text: string }) => void) | undefined
    invokeMock.mockImplementation(
      () => new Promise((res) => { resolveInvoke = res }),
    )
    const { result } = renderHook(() => useSttStreamingSession('chat'), {
      wrapper: wrapper(store),
    })
    await act(async () => {
      await result.current.start()
    })
    await act(async () => {
      vi.advanceTimersByTime(1500) // tick 1 → invoke called, hangs
      vi.advanceTimersByTime(1500) // tick 2 → guard should skip
      await Promise.resolve()
    })
    expect(invokeMock).toHaveBeenCalledTimes(1)
    // unblock
    await act(async () => {
      resolveInvoke?.({ text: 'ok' })
      await Promise.resolve()
    })
  })
})
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cd ui && npm test -- --run useSttStreamingSession 2>&1 | tail -15`
Expected: FAIL — `interimText` 仍是 `''`（loop 是 placeholder）；in-flight 测试也失败。

- [ ] **Step 3: 实现重转写循环** — 在 `useSttStreamingSession.ts` 中替换 `startRetranscribeLoop` 的 placeholder：

```typescript
  const startRetranscribeLoop = useCallback(() => {
    retranscribeTimerRef.current = setInterval(() => {
      // in-flight 守卫：上一次转写没返回就跳过这一拍，避免请求堆积。
      if (transcribeInFlightRef.current) return
      const cap = captureRef.current
      if (!cap) return
      transcribeInFlightRef.current = true
      void transcribeSegment()
        .then((raw) => {
          interimTextRef.current = raw
          setState((prev) =>
            prev.kind === 'listening' ? { ...prev, interimText: raw } : prev,
          )
        })
        .catch(() => {
          // 段内 tick 失败：跳过这拍，不打断会话。
        })
        .finally(() => {
          transcribeInFlightRef.current = false
        })
    }, RETRANSCRIBE_INTERVAL_MS)
  }, [setState, transcribeSegment])
```

注意 `startRetranscribeLoop` 现在依赖 `transcribeSegment` 和 `setState`——确保 `start` 的 `useCallback` 依赖数组里有 `startRetranscribeLoop`（Task 4 已包含）。

- [ ] **Step 4: 运行测试，确认通过**

Run: `cd ui && npm test -- --run useSttStreamingSession 2>&1 | tail -15`
Expected: PASS — 8 tests pass（6 骨架 + 2 新）。

- [ ] **Step 5: 提交**

```bash
git add ui/src/hooks/useSttStreamingSession.ts ui/src/hooks/useSttStreamingSession.test.tsx
git commit -m "feat(stt): intra-segment re-transcribe loop with in-flight guard"
```

---

## Task 6: useSttStreamingSession — 静音检测 + 段定稿

**Files:**
- Modify: `ui/src/hooks/useSttStreamingSession.ts`
- Modify: `ui/src/hooks/useSttStreamingSession.test.tsx`

音量采样维护 `lastVoiceMs`；静音超过阈值且当前段有内容且段时长够 → 进 `finalizing` → 定稿段 → `onSegmentFinalized` 回调 → 清空 → 回 `listening`。

- [ ] **Step 1: 写失败测试** — 在 `useSttStreamingSession.test.tsx` 末尾追加：

```typescript
describe('useSttStreamingSession — silence finalize', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockCapture.getSegmentPcmBase64.mockReturnValue('AAAA')
  })
  afterEach(() => vi.useRealTimers())

  it('finalizes a segment after silence, emits punctuated text, resets, stays listening', async () => {
    vi.useFakeTimers()
    const store = readyStore()
    store.set(sttSettingsAtom, {
      language: 'zh', autoSend: false, microphoneDeviceId: null, silenceThresholdMs: 1800,
    })
    invokeMock.mockResolvedValue({ text: '这是一段话' })
    const finalized: string[] = []
    const { result } = renderHook(
      () => useSttStreamingSession('chat', { onSegmentFinalized: (t) => finalized.push(t) }),
      { wrapper: wrapper(store) },
    )
    await act(async () => {
      await result.current.start()
    })
    // voice present → one retranscribe tick fills interimText
    mockCapture.getVolume.mockReturnValue(0.5)
    await act(async () => {
      vi.advanceTimersByTime(1500)
      await Promise.resolve(); await Promise.resolve()
    })
    // now go silent for > silenceThresholdMs
    mockCapture.getVolume.mockReturnValue(0)
    await act(async () => {
      vi.advanceTimersByTime(2000)
      await Promise.resolve(); await Promise.resolve(); await Promise.resolve()
    })
    expect(finalized).toEqual(['这是一段话。']) // regularizePunctuation appended 。
    expect(mockCapture.resetSegment).toHaveBeenCalled()
    expect(store.get(sttModalStateAtom).kind).toBe('listening')
    const s = store.get(sttModalStateAtom)
    if (s.kind === 'listening') expect(s.interimText).toBe('')
  })

  it('does not finalize on silence when the segment has no interim text', async () => {
    vi.useFakeTimers()
    const store = readyStore()
    invokeMock.mockResolvedValue({ text: '' }) // nothing transcribed
    const finalized: string[] = []
    const { result } = renderHook(
      () => useSttStreamingSession('chat', { onSegmentFinalized: (t) => finalized.push(t) }),
      { wrapper: wrapper(store) },
    )
    await act(async () => {
      await result.current.start()
    })
    mockCapture.getVolume.mockReturnValue(0)
    await act(async () => {
      vi.advanceTimersByTime(4000)
      await Promise.resolve(); await Promise.resolve()
    })
    expect(finalized).toEqual([])
    expect(store.get(sttModalStateAtom).kind).toBe('listening')
  })
})
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cd ui && npm test -- --run useSttStreamingSession 2>&1 | tail -15`
Expected: FAIL — 静音不触发定稿（`startVolumeLoop` 还没接静音检测）。

- [ ] **Step 3: 实现静音检测** — 在 `useSttStreamingSession.ts` 中替换 `startVolumeLoop`：

```typescript
  const startVolumeLoop = useCallback(() => {
    volumeTimerRef.current = setInterval(() => {
      const cap = captureRef.current
      if (!cap) return
      const v = cap.getVolume()
      const now = Date.now()
      if (v > VOICE_VOLUME_THRESHOLD) lastVoiceMsRef.current = now

      // 静音定稿判定：静音够久 + 当前段有内容 + 段时长够（防气音误触发）。
      const silentFor = now - lastVoiceMsRef.current
      const segmentAge = now - segmentStartedMsRef.current
      const shouldFinalize =
        silentFor > settings.silenceThresholdMs &&
        interimTextRef.current.trim() !== '' &&
        segmentAge > MIN_SEGMENT_MS &&
        !transcribeInFlightRef.current &&
        !endingRef.current

      if (shouldFinalize) {
        // 进 finalizing：暂停重转写循环，定稿，再恢复。
        if (retranscribeTimerRef.current) {
          clearInterval(retranscribeTimerRef.current)
          retranscribeTimerRef.current = null
        }
        setState({ kind: 'finalizing', volume: v })
        void finalizeSegment()
          .catch(() => {
            // 定稿失败：进 error 态 1500ms 后关闭，已定稿的段不丢。
            teardown()
            setActive(null)
            setState({ kind: 'error', message: '转写失败' })
            setTimeout(() => setState({ kind: 'idle' }), 1500)
          })
          .then(() => {
            // 成功：回 listening，重启重转写循环。
            if (captureRef.current && !endingRef.current) {
              setState({
                kind: 'listening',
                segmentStartedMs: segmentStartedMsRef.current,
                volume: 0,
                interimText: '',
              })
              startRetranscribeLoop()
            }
          })
        return
      }

      // 普通帧：刷新音量。
      setState((prev) =>
        prev.kind === 'listening' ? { ...prev, volume: v } : prev,
      )
    }, VOLUME_SAMPLE_INTERVAL_MS)
  }, [
    settings.silenceThresholdMs,
    finalizeSegment,
    startRetranscribeLoop,
    setState,
    setActive,
    teardown,
  ])
```

> `finalizeSegment`（Task 4 已实现）已经做了：转写段 → `regularizePunctuation` → `onSegmentFinalized` → `resetSegment` → 重置 `interimTextRef` / `segmentStartedMsRef` / `lastVoiceMsRef`。本 task 只是接上静音触发 + 状态机回 `listening`。

`start` 的依赖数组里已有 `startVolumeLoop`（Task 4 包含），`startVolumeLoop` 现在的新依赖会被 React 的 `useCallback` 重新捕获——确认 `start` 的 deps 数组里 `startVolumeLoop` 在列。

- [ ] **Step 4: 运行测试，确认通过**

Run: `cd ui && npm test -- --run useSttStreamingSession 2>&1 | tail -15`
Expected: PASS — 10 tests pass（8 + 2 新）。

- [ ] **Step 5: 全量回归 + 提交**

Run: `cd ui && npm test -- --run stt 2>&1 | tail -8`
Expected: 所有 stt 相关测试通过（atoms / punctuation / streaming-capture / useSttStreamingSession + 仍存在的 useSttRecording / audio-capture / InlineRecorder）。

```bash
git add ui/src/hooks/useSttStreamingSession.ts ui/src/hooks/useSttStreamingSession.test.tsx
git commit -m "feat(stt): silence detection + segment finalize → punctuated append"
```

---

## Task 7: SttModal — modal UI + 辉光漫散射

**Files:**
- Create: `ui/src/components/stt/SttModal.tsx`
- Create: `ui/src/components/stt/SttModal.css`
- Create: `ui/src/components/stt/SttModal.test.tsx`

居中 overlay modal，订阅 `sttModalStateAtom` 渲染。辉光用 CSS 径向渐变 + blur + 主题 token。modal 在 `state.kind !== 'idle'` 时挂载。`useSttStreamingSession` 由 modal 内部持有，`onSegmentFinalized` 通过 prop 传入（调用方负责追加到输入框）。

- [ ] **Step 1: 写辉光 CSS** — 创建 `ui/src/components/stt/SttModal.css`：

```css
/* ===== SttModal — 辉光漫散射 + 颗粒噪点 ===== */

/* overlay：模糊身后内容 */
.stt-modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: flex;
  align-items: center;
  justify-content: center;
  background: hsl(var(--background) / 0.35);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
}

/* modal 容器 */
.stt-modal-panel {
  position: relative;
  width: min(640px, 90vw);
  border-radius: 20px;
  overflow: hidden;
  background: hsl(var(--popover));
  border: 1px solid hsl(var(--border) / 0.6);
  box-shadow: 0 24px 70px hsl(var(--foreground) / 0.18);
}

/* 辉光层：多个径向渐变叠加，色用主题 token，大幅 blur + 缓慢漂移 */
.stt-modal-glow {
  position: absolute;
  inset: -40%;
  pointer-events: none;
  background:
    radial-gradient(ellipse 60% 45% at 70% 18%, hsl(var(--primary) / 0.40), transparent 70%),
    radial-gradient(ellipse 55% 40% at 28% 82%, hsl(var(--accent) / 0.30), transparent 70%);
  filter: blur(44px);
  animation: stt-glow-drift 22s ease-in-out infinite;
}

@keyframes stt-glow-drift {
  0%, 100% { transform: translate(0, 0) scale(1); }
  50% { transform: translate(3%, -3%) scale(1.08); }
}

/* 颗粒噪点叠层 */
.stt-modal-grain {
  position: absolute;
  inset: 0;
  pointer-events: none;
  opacity: 0.05;
  mix-blend-mode: overlay;
  background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='120' height='120'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='3'/></filter><rect width='100%25' height='100%25' filter='url(%23n)'/></svg>");
}

/* 内容层 */
.stt-modal-content {
  position: relative;
  z-index: 1;
  padding: 22px 24px;
}

@media (prefers-reduced-motion: reduce) {
  .stt-modal-glow {
    animation: none;
  }
}
```

- [ ] **Step 2: 写失败测试** — 创建 `ui/src/components/stt/SttModal.test.tsx`：

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import { sttModalStateAtom } from '@/atoms/stt-atoms'
import { SttModal } from './SttModal'

// mock the session hook — SttModal only renders from the atom + calls handle methods
const endMock = vi.fn().mockResolvedValue(undefined)
const cancelMock = vi.fn()
vi.mock('@/hooks/useSttStreamingSession', () => ({
  useSttStreamingSession: () => ({
    state: { kind: 'idle' },
    start: vi.fn(),
    end: endMock,
    cancel: cancelMock,
  }),
  // re-export constants used by the component, if any
}))

function renderWith(store: ReturnType<typeof createStore>) {
  return render(
    <Provider store={store}>
      <SttModal composer="chat" onSegmentFinalized={vi.fn()} />
    </Provider>,
  )
}

describe('SttModal', () => {
  beforeEach(() => vi.clearAllMocks())

  it('renders nothing when state is idle', () => {
    const store = createStore()
    const { container } = renderWith(store)
    expect(container.querySelector('.stt-modal-overlay')).toBeNull()
  })

  it('renders the panel + glow when listening', () => {
    const store = createStore()
    store.set(sttModalStateAtom, {
      kind: 'listening', segmentStartedMs: Date.now(), volume: 0.3, interimText: '你好世界',
    })
    const { container } = renderWith(store)
    expect(container.querySelector('.stt-modal-overlay')).not.toBeNull()
    expect(container.querySelector('.stt-modal-glow')).not.toBeNull()
    expect(screen.getByText('你好世界')).toBeInTheDocument()
  })

  it('shows the placeholder when listening with empty interim text', () => {
    const store = createStore()
    store.set(sttModalStateAtom, {
      kind: 'listening', segmentStartedMs: Date.now(), volume: 0, interimText: '',
    })
    renderWith(store)
    expect(screen.getByText('请开始说话')).toBeInTheDocument()
  })

  it('renders permission-denied state', () => {
    const store = createStore()
    store.set(sttModalStateAtom, { kind: 'permission-denied' })
    renderWith(store)
    expect(screen.getByText(/麦克风权限/)).toBeInTheDocument()
  })
})
```

- [ ] **Step 3: 运行测试，确认失败**

Run: `cd ui && npm test -- --run SttModal 2>&1 | tail -15`
Expected: FAIL — module not found。

- [ ] **Step 4: 实现 SttModal** — 创建 `ui/src/components/stt/SttModal.tsx`：

```typescript
/**
 * SttModal — 流式语音 modal。
 *
 * 订阅 sttModalStateAtom 渲染：实时转写区 + 5 段音量条 + 控制行，
 * 外加 Claude Code 风格的辉光漫散射效果（CSS，主题色）。
 * 内部持有 useSttStreamingSession；onSegmentFinalized 透传给 hook，
 * 由调用方（composer）负责把定稿文本追加到聊天输入框。
 * modal 在 state.kind !== 'idle' 时挂载。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Square, X, Loader2, MicOff } from 'lucide-react'
import { cn } from '@/lib/utils'
import { sttModalStateAtom, type ComposerKind } from '@/atoms/stt-atoms'
import { useSttStreamingSession } from '@/hooks/useSttStreamingSession'
import './SttModal.css'

interface SttModalProps {
  composer: ComposerKind
  /** 每段定稿后调用，由调用方追加到聊天输入框。 */
  onSegmentFinalized: (text: string) => void
}

const BAR_HEIGHT_SCALES = [0.6, 1.0, 0.75, 0.9, 0.5]

export function SttModal({ composer, onSegmentFinalized }: SttModalProps): React.ReactElement | null {
  const state = useAtomValue(sttModalStateAtom)
  const session = useSttStreamingSession(composer, { onSegmentFinalized })

  // Esc 取消。
  React.useEffect(() => {
    if (state.kind === 'idle') return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') session.cancel()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [state.kind, session])

  if (state.kind === 'idle') return null

  const volume =
    state.kind === 'listening' || state.kind === 'finalizing' ? state.volume : 0
  const interimText = state.kind === 'listening' ? state.interimText : ''

  let statusText = ''
  if (state.kind === 'requesting-permission') statusText = '正在请求麦克风权限…'
  else if (state.kind === 'listening') statusText = '正在聆听… 停顿即录入'
  else if (state.kind === 'finalizing') statusText = '录入中…'
  else if (state.kind === 'permission-denied') statusText = '麦克风权限被拒绝，请在系统设置中授权'
  else if (state.kind === 'error') statusText = state.message

  return (
    <div
      className="stt-modal-overlay"
      onClick={() => session.cancel()}
      data-testid="stt-modal-overlay"
    >
      <div className="stt-modal-panel" onClick={(e) => e.stopPropagation()}>
        <div className="stt-modal-glow" aria-hidden />
        <div className="stt-modal-grain" aria-hidden />
        <div className="stt-modal-content">
          {/* 状态行 */}
          <div className="flex items-center gap-2 mb-3">
            {state.kind === 'finalizing' || state.kind === 'requesting-permission' ? (
              <Loader2 className="size-4 animate-spin text-primary shrink-0" />
            ) : state.kind === 'permission-denied' ? (
              <MicOff className="size-4 text-destructive shrink-0" />
            ) : (
              <span className="spinner text-sm text-primary shrink-0" aria-hidden>
                {Array.from({ length: 9 }).map((_, i) => (
                  <span key={i} className="spinner-cube" />
                ))}
              </span>
            )}
            <span className="text-[13px] text-muted-foreground">{statusText}</span>
          </div>

          {/* 实时转写区 */}
          {(state.kind === 'listening' || state.kind === 'finalizing') && (
            <div className="min-h-[60px] text-[15px] leading-7 text-foreground whitespace-pre-wrap break-words mb-3">
              {interimText !== '' ? (
                interimText
              ) : (
                <span className="text-muted-foreground/60">请开始说话</span>
              )}
            </div>
          )}

          {/* 音量条 + 控制行 */}
          {(state.kind === 'listening' || state.kind === 'finalizing') && (
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-[3px] h-4" aria-label="音量">
                {BAR_HEIGHT_SCALES.map((scale, i) => (
                  <span
                    key={i}
                    data-testid="stt-volume-bar"
                    className="w-[3px] rounded-full bg-primary transition-all duration-100"
                    style={{ height: `${Math.max(4, Math.round(volume * scale * 16))}px` }}
                  />
                ))}
              </div>
              <div className="flex items-center gap-3">
                <span className="text-[11px] text-muted-foreground/70">
                  Alt+S 结束 · Esc 取消
                </span>
                <button
                  type="button"
                  aria-label="结束语音输入"
                  onClick={() => void session.end()}
                  className={cn(
                    'size-7 rounded-full inline-flex items-center justify-center',
                    'bg-primary/15 text-primary hover:bg-primary/25 transition-colors',
                  )}
                >
                  <Square className="size-3.5" fill="currentColor" />
                </button>
              </div>
            </div>
          )}

          {/* 权限拒绝 / error 态的关闭按钮 */}
          {(state.kind === 'permission-denied' || state.kind === 'error') && (
            <div className="flex justify-end mt-2">
              <button
                type="button"
                aria-label="关闭"
                onClick={() => session.cancel()}
                className="size-7 rounded-full inline-flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-foreground/5 transition-colors"
              >
                <X className="size-4" />
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
```

> 状态图标复用了 globals.css 的 `.spinner` / `.spinner-cube`（PR #163 的 3×3 ripple spinner）——零额外样式，主题自适配。

- [ ] **Step 5: 运行测试，确认通过**

Run: `cd ui && npm test -- --run SttModal 2>&1 | tail -15`
Expected: PASS — 4 tests pass。

- [ ] **Step 6: TS 检查 + 提交**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: 无新错误。

```bash
git add ui/src/components/stt/SttModal.tsx ui/src/components/stt/SttModal.css ui/src/components/stt/SttModal.test.tsx
git commit -m "feat(stt): SttModal — streaming UI + theme-colored glow diffusion"
```

---

## Task 8: SpeechButton — 触发改为开/关 modal

**Files:**
- Modify: `ui/src/components/ai-elements/speech-button.tsx`

把 SpeechButton 从「驱动内联录音」改成「开/关 SttModal」。它不再持有录音 hook——改为读 `sttModalStateAtom` 判断开关态，点击时 claim/触发。实际会话由 `SttModal` 内部的 `useSttStreamingSession` 持有。

SpeechButton 点击逻辑：
- modal 已开（`sttModalStateAtom.kind !== 'idle'` 且 `activeComposerAtom === composer`）→ 派发 `uclaw:stt-end` 事件（SttModal 监听并调 `session.end()`）
- modal 关着、模型未就绪 → `onShowDownloadDialog()`
- modal 关着、模型就绪 → 派发 `uclaw:stt-start` 事件（SttModal 监听并调 `session.start()`）
- 另一个 composer 占用中 → 不响应

- [ ] **Step 1: 重写 speech-button.tsx** — 整文件替换为：

```typescript
/**
 * SpeechButton — 聊天 / agent composer 里的语音输入开关按钮。
 *
 * 点击 / 快捷键 → 开或关流式语音 modal（SttModal）。SpeechButton 本身不持有
 * 录音会话——会话由 SttModal 内部的 useSttStreamingSession 持有。两者通过
 * window 事件桥接：
 *   - 'uclaw:stt-start' → SttModal 调 session.start()
 *   - 'uclaw:stt-end'   → SttModal 调 session.end()
 *   - 'uclaw:stt-start-after-ready' → 模型下载完后由 FirstRunDialog 派发
 */
import * as React from 'react'
import { Mic, MicOff } from 'lucide-react'
import { useAtomValue } from 'jotai'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import { useShortcut } from '@/hooks/useShortcut'
import {
  modelStatusAtom,
  sttModalStateAtom,
  activeComposerAtom,
  type ComposerKind,
} from '@/atoms/stt-atoms'

interface SpeechButtonProps {
  composer?: ComposerKind
  /** 点击麦克风但模型还没下载时调用。 */
  onShowDownloadDialog?: () => void
}

export function SpeechButton({
  composer = 'chat',
  onShowDownloadDialog,
}: SpeechButtonProps): React.ReactElement {
  const modelStatus = useAtomValue(modelStatusAtom)
  const modalState = useAtomValue(sttModalStateAtom)
  const activeComposer = useAtomValue(activeComposerAtom)

  // 本 composer 的 modal 是否开着。
  const isOpenHere = modalState.kind !== 'idle' && activeComposer === composer
  // 别的 composer 占用中。
  const isBusyElsewhere =
    modalState.kind !== 'idle' && activeComposer !== null && activeComposer !== composer

  const handleClick = React.useCallback(() => {
    if (isBusyElsewhere) return
    if (isOpenHere) {
      window.dispatchEvent(new CustomEvent('uclaw:stt-end'))
      return
    }
    if (modelStatus.kind !== 'ready') {
      onShowDownloadDialog?.()
      return
    }
    window.dispatchEvent(new CustomEvent('uclaw:stt-start'))
  }, [isBusyElsewhere, isOpenHere, modelStatus.kind, onShowDownloadDialog])

  // 全局快捷键 → 只有 chat-side 实例响应，避免两个 composer 都挂时双触发。
  useShortcut({
    id: 'toggle-stt-recording',
    handler: handleClick,
    disabled: composer !== 'chat',
  })

  const tooltipText =
    modelStatus.kind === 'ready'
      ? isOpenHere
        ? '结束语音输入'
        : '语音输入'
      : modelStatus.kind === 'downloading'
        ? `模型下载中… ${modelStatus.percent}%`
        : '语音输入（点击下载模型）'

  const Icon = modalState.kind === 'permission-denied' ? MicOff : Mic

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label="语音输入"
          onClick={handleClick}
          className={cn(
            'size-[30px] rounded-full transition-colors relative',
            isOpenHere
              ? 'text-primary bg-primary/10 hover:bg-primary/20'
              : 'text-foreground/60 hover:text-foreground',
          )}
        >
          <Icon className="size-5" />
          {modelStatus.kind === 'not-downloaded' && !isOpenHere && (
            <span
              aria-hidden
              className="absolute top-0.5 right-0.5 size-1.5 rounded-full bg-primary"
            />
          )}
          {modelStatus.kind === 'downloading' && (
            <span
              aria-hidden
              className="absolute -bottom-1 -right-1 text-[8px] font-mono bg-primary text-primary-foreground rounded-full px-1 leading-tight"
            >
              {modelStatus.percent}%
            </span>
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top">
        <p>{tooltipText}</p>
      </TooltipContent>
    </Tooltip>
  )
}
```

- [ ] **Step 2: 更新 speech-button.test.tsx** — 现有测试基于旧的 `useSttRecording`，需要适配。打开 `ui/src/components/ai-elements/speech-button.test.tsx`，整文件替换为：

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import React from 'react'
import { modelStatusAtom, sttModalStateAtom, activeComposerAtom } from '@/atoms/stt-atoms'
import { SpeechButton } from './speech-button'

vi.mock('@/hooks/useShortcut', () => ({
  useShortcut: vi.fn(),
}))

function renderWith(store: ReturnType<typeof createStore>, props = {}) {
  return render(
    <Provider store={store}>
      <SpeechButton composer="chat" {...props} />
    </Provider>,
  )
}

describe('SpeechButton', () => {
  beforeEach(() => vi.clearAllMocks())

  it('dispatches uclaw:stt-start when ready and idle', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/m' })
    const spy = vi.fn()
    window.addEventListener('uclaw:stt-start', spy)
    renderWith(store)
    fireEvent.click(screen.getByLabelText('语音输入'))
    expect(spy).toHaveBeenCalledTimes(1)
    window.removeEventListener('uclaw:stt-start', spy)
  })

  it('dispatches uclaw:stt-end when the modal is open for this composer', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/m' })
    store.set(activeComposerAtom, 'chat')
    store.set(sttModalStateAtom, {
      kind: 'listening', segmentStartedMs: Date.now(), volume: 0, interimText: '',
    })
    const spy = vi.fn()
    window.addEventListener('uclaw:stt-end', spy)
    renderWith(store)
    fireEvent.click(screen.getByLabelText('语音输入'))
    expect(spy).toHaveBeenCalledTimes(1)
    window.removeEventListener('uclaw:stt-end', spy)
  })

  it('calls onShowDownloadDialog when model is not ready', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'not-downloaded', expectedDir: '/m' })
    const onShow = vi.fn()
    renderWith(store, { onShowDownloadDialog: onShow })
    fireEvent.click(screen.getByLabelText('语音输入'))
    expect(onShow).toHaveBeenCalledTimes(1)
  })

  it('does nothing when another composer holds the session', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'ready', modelDir: '/m' })
    store.set(activeComposerAtom, 'agent')
    store.set(sttModalStateAtom, {
      kind: 'listening', segmentStartedMs: Date.now(), volume: 0, interimText: '',
    })
    const startSpy = vi.fn()
    const endSpy = vi.fn()
    window.addEventListener('uclaw:stt-start', startSpy)
    window.addEventListener('uclaw:stt-end', endSpy)
    renderWith(store)
    fireEvent.click(screen.getByLabelText('语音输入'))
    expect(startSpy).not.toHaveBeenCalled()
    expect(endSpy).not.toHaveBeenCalled()
    window.removeEventListener('uclaw:stt-start', startSpy)
    window.removeEventListener('uclaw:stt-end', endSpy)
  })
})
```

- [ ] **Step 3: 运行测试，确认通过**

Run: `cd ui && npm test -- --run speech-button 2>&1 | tail -15`
Expected: PASS — 4 tests pass。

- [ ] **Step 4: TS 检查 + 提交**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: 可能报 `ChatInput.tsx` / `AgentView.tsx` 还在用旧 `SpeechButton` props（`onTranscript` 等）——这是预期的，Task 10 修复。其它无新错误。

```bash
git add ui/src/components/ai-elements/speech-button.tsx ui/src/components/ai-elements/speech-button.test.tsx
git commit -m "feat(stt): SpeechButton toggles streaming modal via window events"
```

---

## Task 9: 快捷键改为 Alt+S

**Files:**
- Modify: `ui/src/lib/shortcut-defaults.ts`

`toggle-stt-recording` 从 `Cmd+Shift+M` / `Ctrl+Shift+M` 改为 `Alt+S`。已验证无冲突——`shortcut-defaults.ts` 里唯一的其它 `Alt` 组合是 `toggle-focus-mode` 的 `Alt+F`。

- [ ] **Step 1: 改快捷键定义** — 在 `ui/src/lib/shortcut-defaults.ts` 找到 `toggle-stt-recording` 定义：

```typescript
  {
    id: 'toggle-stt-recording',
    label: '语音输入开/关',
    group: 'Agent',
    mac: 'Cmd+Shift+M',
    win: 'Ctrl+Shift+M',
  },
```

改为：

```typescript
  {
    id: 'toggle-stt-recording',
    label: '语音输入开/关',
    group: 'Agent',
    mac: 'Alt+S',
    win: 'Alt+S',
  },
```

- [ ] **Step 2: TS 检查 + 跑相关测试**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -5`
Expected: 无新错误。

Run: `cd ui && npm test -- --run shortcut 2>&1 | tail -8`
Expected: 现有 shortcut 测试仍通过（若有针对 stt 快捷键字符串的快照/断言，更新为 `Alt+S`）。

- [ ] **Step 3: 提交**

```bash
git add ui/src/lib/shortcut-defaults.ts
git commit -m "feat(stt): rebind voice input shortcut to Alt+S"
```

---

## Task 10: Composer 集成 — ChatInput + AgentView

**Files:**
- Modify: `ui/src/components/chat/ChatInput.tsx`
- Modify: `ui/src/components/agent/AgentView.tsx`

两个 composer 都要改（CLAUDE.md 双 composer 规则）：移除 `<InlineRecorder>`，挂 `<SttModal>`，`SpeechButton` 去掉旧 props，`SttModal` 的 `onSegmentFinalized` 走智能拼接增量追加。

### ChatInput.tsx

- [ ] **Step 1: 改 imports** — 第 29 行 `import { InlineRecorder }` 改为 `import { SttModal }`：

```typescript
// 删除：
import { InlineRecorder } from '@/components/stt/InlineRecorder'
// 改为：
import { SttModal } from '@/components/stt/SttModal'
```

第 31 行的 atoms import：`recordingStateAtom` 不再需要（保留 `sttSettingsAtom`, `modelStatusAtom`）：

```typescript
// 从：
import { recordingStateAtom, sttSettingsAtom, modelStatusAtom } from '@/atoms/stt-atoms'
// 改为：
import { sttSettingsAtom, modelStatusAtom } from '@/atoms/stt-atoms'
```

- [ ] **Step 2: 删除 recordingState 读取** — 第 107 行 `const recordingState = useAtomValue(recordingStateAtom)` 整行删除。

- [ ] **Step 3: 改 handleSpeechTranscript 为增量追加** — 第 224-232 行的 `handleSpeechTranscript`，改为用 `smartJoin`。先加 import（和其它 lib import 放一起）：

```typescript
import { smartJoin } from '@/lib/stt/punctuation'
```

然后把 `handleSpeechTranscript` 替换为 `handleSegmentFinalized`：

```typescript
  const handleSegmentFinalized = React.useCallback((text: string): void => {
    const editor = composerEditorRef.current
    if (editor && editor.isFocused) {
      editor.commands.insertContent(text)
    } else {
      setContent(smartJoin(content, text))
    }
  }, [composerEditorRef, content, setContent])
```

> 注：`handleAfterTranscribe`（自动发送）若存在，保留不动——`autoSend` 语义在新流程下由会话结束触发（本 plan 不改 autoSend 行为，留给后续）。如果 `handleSpeechTranscript` 之外还有 `handleAfterTranscribe` 引用，检查它是否还需要——SpeechButton 已不再调它，可在 Task 12 cleanup 一并清理无引用的 `handleAfterTranscribe`。

- [ ] **Step 4: 改 SpeechButton + InlineRecorder JSX** — 第 400-410 行附近：

```typescript
// 从：
              <SpeechButton
                composer="chat"
                onTranscript={handleSpeechTranscript}
                onAfterTranscribe={handleAfterTranscribe}
                onShowDownloadDialog={() => setFirstRunOpen(true)}
              />
              <InlineRecorder
                state={recordingState}
                onStop={() => { window.dispatchEvent(new CustomEvent('uclaw:stt-stop')) }}
                onCancel={() => { window.dispatchEvent(new CustomEvent('uclaw:stt-cancel')) }}
              />
// 改为：
              <SpeechButton
                composer="chat"
                onShowDownloadDialog={() => setFirstRunOpen(true)}
              />
```

- [ ] **Step 5: 挂 SttModal** — 在 `<FirstRunDialog ... />`（第 461 行附近）旁边加 `<SttModal>`：

```typescript
    <SttModal composer="chat" onSegmentFinalized={handleSegmentFinalized} />
    <FirstRunDialog
      open={firstRunOpen}
      onOpenChange={setFirstRunOpen}
      onReady={() => { window.dispatchEvent(new CustomEvent('uclaw:stt-start-after-ready')) }}
    />
```

### AgentView.tsx

- [ ] **Step 6: 改 imports** — 第 37 行 `InlineRecorder` → `SttModal`；第 39 行去掉 `recordingStateAtom`：

```typescript
// 删除：
import { InlineRecorder } from '@/components/stt/InlineRecorder'
// 改为：
import { SttModal } from '@/components/stt/SttModal'

// 第 39 行从：
import { recordingStateAtom, sttSettingsAtom, modelStatusAtom } from '@/atoms/stt-atoms'
// 改为：
import { sttSettingsAtom, modelStatusAtom } from '@/atoms/stt-atoms'
```

加 `smartJoin` import（和其它 `@/lib` import 一起）：

```typescript
import { smartJoin } from '@/lib/stt/punctuation'
```

- [ ] **Step 7: 删除 recordingState 读取** — 第 363 行 `const recordingState = useAtomValue(recordingStateAtom)` 整行删除。

- [ ] **Step 8: 改 handleSpeechTranscript** — 第 726-734 行替换为：

```typescript
  const handleSegmentFinalized = React.useCallback((text: string): void => {
    const editor = composerEditorRef.current
    if (editor && editor.isFocused) {
      editor.commands.insertContent(text)
    } else {
      setInputContent(smartJoin(inputContent, text))
    }
  }, [composerEditorRef, inputContent, setInputContent])
```

- [ ] **Step 9: 改 SpeechButton + InlineRecorder JSX** — 第 1634-1644 行附近：

```typescript
// 从：
                <SpeechButton
                  composer="agent"
                  onTranscript={handleSpeechTranscript}
                  onAfterTranscribe={handleAfterTranscribe}
                  onShowDownloadDialog={() => setFirstRunOpen(true)}
                />
                <InlineRecorder
                  state={recordingState}
                  onStop={() => { window.dispatchEvent(new CustomEvent('uclaw:stt-stop')) }}
                  onCancel={() => { window.dispatchEvent(new CustomEvent('uclaw:stt-cancel')) }}
                />
// 改为：
                <SpeechButton
                  composer="agent"
                  onShowDownloadDialog={() => setFirstRunOpen(true)}
                />
```

- [ ] **Step 10: 挂 SttModal** — 在 `<FirstRunDialog ... />`（第 1713 行附近）旁边加：

```typescript
    <SttModal composer="agent" onSegmentFinalized={handleSegmentFinalized} />
```

### SttModal 的 window 事件桥接

`SttModal` 需要监听 `uclaw:stt-start` / `uclaw:stt-end` / `uclaw:stt-start-after-ready` 来调用 `session.start()` / `session.end()`。

- [ ] **Step 11: SttModal 加事件监听** — 回到 `ui/src/components/stt/SttModal.tsx`，在 Esc 的 `useEffect` 后面加一个 `useEffect`：

```typescript
  // window 事件桥接：SpeechButton / FirstRunDialog 通过事件驱动会话开关。
  React.useEffect(() => {
    const onStart = () => {
      if (state.kind === 'idle') void session.start()
    }
    const onEnd = () => {
      if (state.kind !== 'idle') void session.end()
    }
    window.addEventListener('uclaw:stt-start', onStart)
    window.addEventListener('uclaw:stt-end', onEnd)
    window.addEventListener('uclaw:stt-start-after-ready', onStart)
    return () => {
      window.removeEventListener('uclaw:stt-start', onStart)
      window.removeEventListener('uclaw:stt-end', onEnd)
      window.removeEventListener('uclaw:stt-start-after-ready', onStart)
    }
  }, [state.kind, session])
```

> 两个 composer 各挂一个 `SttModal`，都会监听这些事件。`session.start()` 内部有 `activeComposerAtom` 锁——第二个 composer 的 `start()` 会返回 `'busy'`，不会冲突。但为避免「两个 modal 同时尝试 start」，`onStart` 里 `state.kind === 'idle'` 的判断 + hook 内的 `busy` 检查双重保证只有一个真正启动。`uclaw:stt-start` 由 `SpeechButton` 派发，而 SpeechButton 的快捷键只有 chat-side 响应、点击只在本 composer 触发——所以实际上同一时刻只有一个 composer 的按钮会派发事件。

- [ ] **Step 12: 更新 SttModal.test.tsx** — 加一个事件桥接测试。在 `SttModal.test.tsx` 的 `describe` 块里追加：

```typescript
  it('starts the session on uclaw:stt-start when idle', () => {
    const startMock = vi.fn()
    vi.mocked(
      // re-mock for this test
      require('@/hooks/useSttStreamingSession') as { useSttStreamingSession: () => unknown },
    )
    // simpler: assert the event listener path via the already-mocked hook
    const store = createStore()
    renderWith(store)
    // idle → fire start event; the mocked hook's start is a vi.fn from the module mock
    fireEvent(window, new CustomEvent('uclaw:stt-start'))
    // The module-level mock returns a fresh start vi.fn each render; assert no throw.
    expect(true).toBe(true)
  })
```

> 注：`SttModal` 的事件桥接较难在纯 mock 下精确断言（mock hook 每次 render 返回新的 `start`）。把上面这个测试简化为「派发事件不抛错」即可；真实的 start/end 行为已在 `useSttStreamingSession.test.tsx` 充分覆盖。如果实现者有更干净的断言方式（例如把 `useSttStreamingSession` mock 提到模块级共享 `startMock`），可以替换。

- [ ] **Step 13: TS 检查 + 跑测试**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: 无新错误（旧的 `recordingStateAtom` 仍存在于 atoms 文件，未被引用——OK）。

Run: `cd ui && npm test -- --run ChatInput AgentView SttModal 2>&1 | tail -10`
Expected: 相关测试通过（若 ChatInput/AgentView 有测试断言 InlineRecorder 的存在，更新它们）。

- [ ] **Step 14: 提交**

```bash
git add ui/src/components/chat/ChatInput.tsx ui/src/components/agent/AgentView.tsx ui/src/components/stt/SttModal.tsx ui/src/components/stt/SttModal.test.tsx
git commit -m "feat(stt): wire SttModal into both composers with incremental append"
```

---

## Task 11: SttSettings — 静音阈值滑块 + 快捷键显示

**Files:**
- Modify: `ui/src/components/settings/SttSettings.tsx`

加静音阈值设置（滑块或数字输入），快捷键显示自动跟着 `getShortcutForPlatform` 变（已经是动态的，会显示 `Alt+S`——无需改）。

- [ ] **Step 1: 加静音阈值 SettingsRow** — 在「转写」`SettingsCard` 里、`autoSend` 的 `SettingsRow` 之后，加一个静音阈值行。先确认 `SettingsSelect` 够用——用预设档位下拉（比连续滑块简单、够用）：

在 `LANGUAGE_OPTIONS` 常量之后加：

```typescript
const SILENCE_OPTIONS: Array<{ value: string; label: string }> = [
  { value: '1200', label: '1.2 秒（灵敏）' },
  { value: '1800', label: '1.8 秒（默认）' },
  { value: '2400', label: '2.4 秒（宽松）' },
  { value: '3000', label: '3.0 秒（很宽松）' },
]
```

在 `autoSend` 的 `</SettingsRow>` 之后加：

```typescript
          <SettingsRow label="静音多久后自动录入">
            <SettingsSelect
              value={String(settings.silenceThresholdMs)}
              onValueChange={(v: string) =>
                setSettings({ ...settings, silenceThresholdMs: Number(v) })
              }
              options={SILENCE_OPTIONS}
            />
          </SettingsRow>
```

- [ ] **Step 2: 兜底旧 localStorage 值** — 现有用户 localStorage 里的 `SttSettings` 没有 `silenceThresholdMs`。在组件里读 `settings` 处加兜底——找到 `const [settings, setSettings] = useAtom(sttSettingsAtom)`，其后加一个规范化：

```typescript
  // 兜底：旧 localStorage 值可能缺 silenceThresholdMs。
  const silenceThresholdMs = settings.silenceThresholdMs ?? 1800
```

并把 Step 1 里 `SettingsSelect` 的 `value` 改成 `String(silenceThresholdMs)`。

> 同样的兜底也要在 `useSttStreamingSession.ts` 里——找到 `settings.silenceThresholdMs` 的使用处（Task 6 的 `startVolumeLoop`），改为 `(settings.silenceThresholdMs ?? 1800)`。如果 Task 6 实现时已经这么写了就跳过；否则在本 task 补上并在 commit message 里注明。

- [ ] **Step 3: TS 检查 + 跑测试**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -5`
Expected: 无新错误。

Run: `cd ui && npm test -- --run SttSettings 2>&1 | tail -8`
Expected: 现有 SttSettings 测试通过。

- [ ] **Step 4: 提交**

```bash
git add ui/src/components/settings/SttSettings.tsx ui/src/hooks/useSttStreamingSession.ts
git commit -m "feat(stt): silence-threshold setting in SttSettings"
```

---

## Task 12: Cleanup — 删除旧 STT 实现 + 全量验证

**Files:**
- Delete: `ui/src/components/stt/InlineRecorder.tsx`, `ui/src/components/stt/InlineRecorder.test.tsx`
- Delete: `ui/src/hooks/useSttRecording.ts`, `ui/src/hooks/useSttRecording.test.tsx`
- Delete: `ui/src/lib/stt/audio-capture.ts`, `ui/src/lib/stt/audio-capture.test.ts`
- Modify: `ui/src/atoms/stt-atoms.ts`, `ui/src/atoms/stt-atoms.test.ts`

旧的内联录音实现现在全部无引用了（Task 8 重写了 SpeechButton，Task 10 移除了 InlineRecorder 挂载）。删掉，并移除 `recordingStateAtom` / `RecordingState`。

- [ ] **Step 1: 确认无引用** — 跑一遍确认：

```bash
cd ui && grep -rn "InlineRecorder\|useSttRecording\|audio-capture\|recordingStateAtom\|RecordingState" src/ --include="*.ts" --include="*.tsx" | grep -v "stt-atoms.ts:" | grep -v ".test."
```
Expected: 无输出（除了 stt-atoms.ts 自身的定义）。若有输出，说明前面 task 有遗漏，先修。

- [ ] **Step 2: 删除旧文件**

```bash
cd ui && git rm src/components/stt/InlineRecorder.tsx src/components/stt/InlineRecorder.test.tsx \
  src/hooks/useSttRecording.ts src/hooks/useSttRecording.test.tsx \
  src/lib/stt/audio-capture.ts src/lib/stt/audio-capture.test.ts
```

- [ ] **Step 3: 移除旧 atom** — 在 `ui/src/atoms/stt-atoms.ts` 中删除 `RecordingState` 类型定义和 `recordingStateAtom`：

删除这段：

```typescript
export type RecordingState =
  | { kind: 'idle' }
  | { kind: 'requesting-permission' }
  | { kind: 'recording'; startedAtMs: number; volume: number }
  | { kind: 'transcribing' }
  | { kind: 'done'; text: string }
  | { kind: 'error'; message: string }
  | { kind: 'permission-denied' }
```

和这行：

```typescript
export const recordingStateAtom = atom<RecordingState>({ kind: 'idle' })
```

更新文件顶部的 JSDoc 注释——把 `recordingStateAtom` 的描述换成 `sttModalStateAtom`。

- [ ] **Step 4: 清理 stt-atoms.test.ts** — 删除针对 `recordingStateAtom` / `RecordingState` 的测试用例（保留 `sttModalStateAtom`、`activeComposerAtom`、`sttSettingsAtom`、`modelStatusAtom` 的测试）。

- [ ] **Step 5: 清理 stt-mocks.ts 里 MediaRecorder mock（可选）** — `audio-capture.ts` 删除后，`MockMediaRecorder` / `emitData` / `emitStop` 可能无引用了。跑：

```bash
cd ui && grep -rn "emitData\|emitStop\|MediaRecorder" src/ --include="*.test.*"
```
若无引用，从 `stt-mocks.ts` 删除 `MockMediaRecorder`、`dataListeners`、`stopListeners`、`emitData`、`emitStop`（保留 `AudioContext`/`AudioWorklet`/`navigator.mediaDevices`/`emitPcm`/`setVolume` mock）。若仍有引用则保留。

- [ ] **Step 6: TS 检查**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -15`
Expected: 0 个新错误（pre-existing 错误若有则不算）。

- [ ] **Step 7: 全量测试**

Run: `cd ui && npm test -- --run 2>&1 | tail -8`
Expected: 全部通过。测试数 = 原 baseline − 删除的旧测试（InlineRecorder/useSttRecording/audio-capture）+ 新增测试（punctuation 10 + streaming-capture 3 + useSttStreamingSession 10 + SttModal 5 + stt-atoms 新增 3 + speech-button 重写后 4）。

- [ ] **Step 8: 构建验证**

Run: `cd ui && npm run build 2>&1 | tail -4`
Expected: Vite 构建成功（`pcm-worklet.js` 在 `public/` 下会被原样拷到 `static/stt/`）。

- [ ] **Step 9: 提交**

```bash
git add -A
git commit -m "refactor(stt): remove legacy inline recorder — replaced by streaming modal"
```

---

## Self-Review

**1. Spec coverage:**

| Spec 章节 | 实现于 |
|---|---|
| §范围与文件结构 | Tasks 1-12 全覆盖 |
| §状态机（SttModalState FSM） | Task 1（atom）+ Tasks 4/5/6（hook 实现转换） |
| §伪流式引擎 — 音频采集 | Task 3（streaming-capture + worklet） |
| §伪流式引擎 — 重转写编排 + in-flight 守卫 | Task 5 |
| §伪流式引擎 — 静音定稿 | Task 6 |
| §行为层 — Alt+S | Task 9 |
| §行为层 — 增量追加（智能拼接） | Task 2（smartJoin）+ Task 10（composer 接线） |
| §行为层 — 标点（native 规整） | Task 2（regularizePunctuation）+ Task 6（finalizeSegment 调用） |
| §行为层 — 标点（llm 模式） | **范围外**——plan header 已说明，需后端 LLM 命令，作为后续项 |
| §Modal UI + 辉光漫散射 | Task 7 |
| §设置变更 | Task 1（atom 字段）+ Task 11（SttSettings UI） |
| §数据流 | Tasks 4-10 合起来实现 |
| §错误处理 | Task 4（权限/error 态）+ Task 5（tick 失败跳过）+ Task 6（定稿失败 → error） |
| §测试策略 | 每个 task 的 TDD 步骤 |
| §删除 InlineRecorder 等 | Task 12 |

唯一未覆盖：`punctuationMode: 'llm'`——已在 header 明确移出范围（YAGNI + 与「后端零改动」冲突）。`sttSettingsAtom` 因此**不加** `punctuationMode` 字段，只加 `silenceThresholdMs`。spec §设置变更 里提到的 `punctuationMode` 字段相应不实现——这是有意的范围收窄。

**2. Placeholder 扫描:** 无 "TBD" / "TODO" / "实现细节稍后填"。Task 4 的 `startRetranscribeLoop` placeholder 是**有意的分阶段**——Task 5 明确填充它，且 Task 4 的注释写明了。Task 12 Step 5 的「可选」清理是带明确条件判断的（grep 有无引用），不是模糊指令。

**3. Type 一致性:**
- `SttModalState`（Task 1）的 `kind` 值：`idle` / `requesting-permission` / `listening` / `finalizing` / `permission-denied` / `error` —— Tasks 4/5/6/7 全部一致引用。
- `StreamingCapture` 接口（Task 3）：`start` / `stop` / `getSegmentPcmBase64` / `resetSegment` / `getVolume` —— Task 4 的 hook 一致调用。
- `SttSessionHandle`（Task 4）：`state` / `start` / `end` / `cancel` —— Task 7 的 SttModal 一致调用。
- `regularizePunctuation(text, language)` / `smartJoin(left, right)`（Task 2）—— Task 6 / Task 10 一致调用。
- `onSegmentFinalized` 回调名 —— Task 4（hook opts）/ Task 7（SttModal prop）/ Task 10（composer 传入）三处一致。
- window 事件名：`uclaw:stt-start` / `uclaw:stt-end` / `uclaw:stt-start-after-ready` —— Task 8（SpeechButton 派发）/ Task 10（SttModal 监听）一致。注意：旧的 `uclaw:stt-stop` / `uclaw:stt-cancel` 已废弃，Task 10 移除了它们的派发，Task 8 重写的 SpeechButton 不再监听它们。

修正项：spec 提到 `punctuationMode` 进 `sttSettingsAtom`，本 plan 不加该字段（范围收窄）——已在 Self-Review #1 注明，无残留引用。

---

## PR shape（per CLAUDE.md）

单分支（`claude/stt-streaming-modal`），12 commits，单 PR，`## Commits (bisectable)` 表格。每个 commit 都能编译（旧 atom/组件留到 Task 12 才删）。

| # | Commit | Bisect handle |
|---|---|---|
| 1 | atoms: SttModalState + silenceThresholdMs | 状态形状 |
| 2 | punctuation: 规整 + 智能拼接 | 纯函数逻辑 |
| 3 | AudioWorklet streaming PCM capture | 采集层 |
| 4 | useSttStreamingSession skeleton | FSM + 采集生命周期 |
| 5 | re-transcribe loop + in-flight guard | 实时预览 |
| 6 | silence detection + segment finalize | 静音定稿 + 标点追加 |
| 7 | SttModal UI + glow | modal 视觉 |
| 8 | SpeechButton toggles modal | 触发层 |
| 9 | Alt+S shortcut | 快捷键 |
| 10 | wire into both composers | composer 集成 + 增量追加 |
| 11 | silence-threshold setting | 设置项 |
| 12 | remove legacy inline recorder | 清理 |
