# Pet Widget 设计文档

**日期**：2026-05-13
**作者**：uClaw 团队
**状态**：草案 → 待 review

## 目的

在 AgentView 的输入框右上角放置一个 **AI 桌面宠物**，作为 agent 状态的可视化伙伴：

- agent 在推理 / 调工具 / 完成 / 出错时，宠物用对应表情和动作回应
- 用户输入时，宠物专注地"和你一起打字"
- 用户把鼠标移到宠物上，宠物兴奋反应（hover 互动）
- 整体观感 chibi、可爱、印象深刻

UX 目标：让"看着 agent 干活"这件事**有温度**——agent 的状态不是冷冰冰的 loading 条，而是一个一直陪你的角色在认真做事。

## 设计原则

- **资源已就绪**：12 段 alpha WebP（2 角色 × 6 状态）+ 1 张参考基准 PNG。本 spec 只描述如何接入。
- **零后端改动**：现有 Tauri 事件已经够用（`chat:stream-*` / `agent:turn_cost` / `agent:stream-reset`）。Rust 侧不需要新事件。
- **可关、可换**：默认关闭。用户在 Settings 里启用 + 选角色后再加载资源。
- **主题安全**：11 个主题下都好看。alpha 通道保证零白边、零绿渍。
- **不打断关键状态**：hover 互动只在 idle 时生效；agent 正在 thinking / typing / success / error 期间，hover 不切走当前状态。
- **scope 优先级**：第一版只接 AgentView。Chat 模式（`ChatInput.tsx`）下一期再做（chat 没有 agent 推理，只有 idle / hover / typing 三态）。

## 用户故事

1. 用户在 Settings → Appearance 里勾选 "Enable desktop pet"，选 Astro 或 Clawby，AgentView 输入框右上角立刻出现 idle 状态的宠物。
2. 用户开始打字 → 宠物切换到 typing 状态（Astro 抱键盘敲、Clawby 拿铅笔写）。
3. 用户回车发送 → 宠物切换到 thinking（Astro 摸下巴 + loader、Clawby 仰头 + "?"）。
4. agent 完成回答 → 宠物切换到 success（双臂举起 + 星星 1.5 秒）→ 自动回 idle。
5. 失败或被中断 → 宠物切换到 error（皱眉 + 泪滴 / 摸耳 + 汗滴）→ 用户解决后回 idle。
6. 用户鼠标移到宠物身上（仅在 idle 时）→ 宠物兴奋互动（Astro 星星眼挥手、Clawby 偏头抬爪）→ 鼠标移开回 idle。
7. 用户不喜欢宠物 → Settings 取消勾选，宠物立刻消失，资源不再加载。

## 动画节奏设计

WebP 默认是匀速 loop。一直 4s 硬循环像紧张的玩偶——真实角色 idle 大部分时间在歇着，偶尔动一下。**通过在素材层追加"静止帧"调整每状态的动/静比例**，前端代码不变。

| 状态 | 节奏 | 动:静 | 设计意图 |
|---|---|---|---|
| idle | 4s 动 + 6s 静 = 10s loop | 40:60 | 像真实宠物大多数时间歇着，偶尔活动 |
| hover | 4s 动（不变） | 100:0 | 用户主动逗它的瞬间，要兴奋 |
| thinking | 4s 动 + 1s 静 = 5s loop | 80:20 | 在认真干活，有微停顿，避免机械循环 |
| typing | 4s 动（不变） | 100:0 | 敲键盘 / 写字本来就是**持续动作**，不能停 |
| success | 4s 动（4s 后由 timer 切回 idle，保证完整播放跳起+星星+笑脸） | 100:0 | 一次性庆祝，timer 长度 = 动画长度 |
| error | 4s 动 + 2s 静 = 6s loop | 67:33 | 悲伤姿态本就少动，多停顿更对路 |

### 实现：静止帧烤入 WebP

`tools/add-pauses.sh <in.webp> <out.webp> <pause_seconds>`：

1. PIL 把 WebP 拆为 96 帧 RGBA PNG
2. 末尾追加 N 份**第 0 帧**（Veo 起手姿势 = 最自然的"歇着"锚点）
3. `img2webp -m 2 -q 75 -mixed` 重组

得到的新 WebP 长度延长但仍然无缝 loop。`<img>` 标签 loop 节奏自动跟着变。

### 状态切换 vs loop 长度的解耦

切换时机**完全由 JS 决定**，与 WebP loop 长度无关：

- thinking: `chat:stream-chunk` 进入 → `chat:stream-complete` 退出，期间不管 loop 跑到第几秒都没关系，crossfade 立刻接管
- success: 1500ms 定时切回 idle，不等当前 loop 跑完
- typing: 用户停止打字 → 立刻切 idle

也就是说 **WebP loop 决定"节奏感"，前端 JS 决定"何时切换"**，互不干扰。当前的双层 `<img>` opacity crossfade（280ms）已经处理切换。

### 长 session 单调问题（已知限制）

即使 5s loop + 微停顿，30s+ thinking session 仍然会被看出循环。**真正治本**需要变体（thinking_A / thinking_B 两段不同动画，JS 随机切换）或 `<canvas>` 单帧控制 + 随机停顿长度。属于范围外，未来按需补。

## 资源清单

12 段 animated WebP，已生成 + 抠图 + chromakey + 静止帧节奏调整完毕。每段约 1.5–3 MB，720×720 RGBA，24fps：

```
ui/public/pet/
  astro-idle.webp       astro-hover.webp    astro-thinking.webp
  astro-typing.webp     astro-success.webp  astro-error.webp
  clawby-idle.webp      clawby-hover.webp   clawby-thinking.webp
  clawby-typing.webp    clawby-success.webp clawby-error.webp
```

源文件位于 `.superpowers/brainstorm/<session>/videos/webp/`，部署时复制到 `ui/public/pet/` 即可（Vite 把 `public/` 下的资源原样打包，相对 URL `/pet/*.webp` 可用）。

总大小约 24 MB。Vite 的 manualChunks 不影响 `public/` 资源；首次加载是按 `<img src>` 懒加载（见"性能"一节）。

---

## 架构

### 状态层（jotai）

新文件 `ui/src/atoms/pet-atoms.ts`：

```ts
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export type PetCharacter = 'astro' | 'clawby'
export type PetState = 'idle' | 'hover' | 'thinking' | 'typing' | 'success' | 'error'

// ───────── 用户偏好（持久化）─────────

export const petEnabledAtom = atomWithStorage<boolean>('pet.enabled', false)
export const petCharacterAtom = atomWithStorage<PetCharacter>('pet.character', 'astro')

// ───────── 主状态（由 agent 信号驱动）─────────

/**
 * 主状态。由 useAgentStateSync hook 写入。
 * - idle: 默认。没有 agent turn 在跑、用户没在打字、没有最近完成/错误。
 * - typing: 用户 composer 聚焦且有非空文本。
 * - thinking: agent turn 在跑（`chat:stream-chunk` 已收到第一帧、`chat:stream-complete` 还没到）。
 * - success: turn 刚完成（success 持续 1500ms 后自动回 idle）。
 * - error: turn 失败（保持直到下次用户交互）。
 */
export const petPrimaryStateAtom = atom<Exclude<PetState, 'hover'>>('idle')

// ───────── Hover override（瞬时）─────────

export const petHoverActiveAtom = atom<boolean>(false)

/**
 * 给 PetWidget 真正消费的派生 state。
 * - hover 只在 primary === 'idle' 时覆盖 → 'hover'
 * - 其他主状态期间，hover 不打断
 */
export const petDisplayStateAtom = atom<PetState>((get) => {
  const primary = get(petPrimaryStateAtom)
  const hovering = get(petHoverActiveAtom)
  return hovering && primary === 'idle' ? 'hover' : primary
})
```

为什么独立 atom 文件、不复用 `agent-atoms.ts`：宠物的状态机是 derived view，跟 agent 业务无关；保持解耦让以后 chat 模式接入更直白。

### 钩子层（`ui/src/hooks/`）

#### `usePetStateSync`

监听现有的 Tauri 事件 + 现有的 composer 焦点 atom，自动维护 `petPrimaryStateAtom`：

```ts
export function usePetStateSync(): void {
  const setPrimary = useSetAtom(petPrimaryStateAtom)
  const composerFocused = useAtomValue(composerFocusedAtom)
  const composerHasText = useAtomValue(composerHasTextAtom)

  // Tauri 事件 → primary state
  useEffect(() => {
    const unlistens: UnlistenFn[] = []
    listen('chat:stream-chunk', () => setPrimary('thinking')).then(u => unlistens.push(u))
    listen('chat:stream-complete', () => {
      setPrimary('success')
      setTimeout(() => setPrimary('idle'), 1500)  // success 持续 1.5s
    }).then(u => unlistens.push(u))
    listen('chat:stream-error', () => setPrimary('error')).then(u => unlistens.push(u))
    listen('agent:stream-reset', () => setPrimary('idle')).then(u => unlistens.push(u))
    return () => unlistens.forEach(u => u())
  }, [setPrimary])

  // typing 仅在 primary 不是关键 agent 状态时启用
  useEffect(() => {
    setPrimary((prev) => {
      if (prev === 'thinking' || prev === 'success' || prev === 'error') return prev
      return (composerFocused && composerHasText) ? 'typing' : 'idle'
    })
  }, [composerFocused, composerHasText, setPrimary])
}
```

注意：当前代码里 `RichTextInput` 的 focus / content state 还是 component-local（在 `AgentView.tsx` 里通过 hook 局部管理）。**实现 plan 的第一步是把它们抽到 atom**（`composerFocusedAtom` + `composerHasTextAtom`），这是 pet 接入的前置依赖。具体抽取点：在 `AgentView.tsx` 渲染 `RichTextInput` 的 `onFocus`/`onBlur`/`onChange` 回调里 setAtom。

#### `usePetHover`（轻量）

```ts
export function usePetHover() {
  const setHover = useSetAtom(petHoverActiveAtom)
  return {
    onMouseEnter: () => setHover(true),
    onMouseLeave: () => setHover(false),
  }
}
```

### 组件层

新文件 `ui/src/components/agent/PetWidget.tsx`：

```tsx
import { useAtomValue } from 'jotai'
import { useEffect, useRef, useState } from 'react'
import { petCharacterAtom, petDisplayStateAtom, petEnabledAtom, type PetState } from '@/atoms/pet-atoms'
import { usePetHover } from '@/hooks/usePetHover'

interface Props { className?: string }

export function PetWidget({ className }: Props) {
  const enabled = useAtomValue(petEnabledAtom)
  const character = useAtomValue(petCharacterAtom)
  const state = useAtomValue(petDisplayStateAtom)
  const hoverHandlers = usePetHover()

  const [activeLayer, setActiveLayer] = useState<'a' | 'b'>('a')
  const [layerASrc, setLayerASrc] = useState<PetState>('idle')
  const [layerBSrc, setLayerBSrc] = useState<PetState | null>(null)
  const lastShown = useRef<PetState>('idle')

  // Crossfade on state change
  useEffect(() => {
    if (state === lastShown.current) return
    const next = activeLayer === 'a' ? 'b' : 'a'
    if (next === 'a') setLayerASrc(state)
    else setLayerBSrc(state)
    // Wait a tick so the img element starts loading before we toggle opacity
    requestAnimationFrame(() => {
      setActiveLayer(next)
      lastShown.current = state
    })
  }, [state, activeLayer])

  if (!enabled) return null
  const src = (s: PetState | null) => s ? `/pet/${character}-${s}.webp` : ''

  return (
    <div
      className={`pet-widget ${className ?? ''}`}
      data-char={character}
      {...hoverHandlers}
    >
      <img
        className={`pet-layer ${activeLayer === 'a' ? 'active' : ''}`}
        src={src(layerASrc)} alt=""
      />
      {layerBSrc && (
        <img
          className={`pet-layer ${activeLayer === 'b' ? 'active' : ''}`}
          src={src(layerBSrc)} alt=""
        />
      )}
    </div>
  )
}
```

每角色独立 `feet-offset` 通过 `data-char` 属性 + CSS 选择器配置。

#### 样式（Tailwind 不够，需要少量原子 CSS）

新文件 `ui/src/components/agent/PetWidget.css`（已用 Tailwind 时也保留一个 .css 文件存 transform 和 feet-offset，因 Tailwind 不便表达 per-char 变量）：

```css
.pet-widget {
  position: absolute;
  right: 16px;
  bottom: 100%;          /* widget bottom 贴 input box top */
  width: 100px; height: 100px;
  z-index: 2;
  pointer-events: auto;  /* 接收 hover */
  cursor: pointer;
}
.pet-widget .pet-layer {
  position: absolute; inset: 0;
  width: 100%; height: 100%;
  object-fit: contain;
  opacity: 0;
  transition: opacity 280ms ease-in-out;
  pointer-events: none;
  /* 按角色补偿底部透明 alpha,让脚/爪正好落在 widget 底边 */
  transform: translateY(var(--feet-offset, 0%));
}
.pet-widget .pet-layer.active { opacity: 1; }
/* 测量自 idle 状态全 96 帧的最深底像素：
     Astro:  feet at row 673/720 → empty 6.39%
     Clawby: paws at row 591/720 → empty 17.78% */
.pet-widget[data-char="astro"]  .pet-layer { --feet-offset: 6.4%; }
.pet-widget[data-char="clawby"] .pet-layer { --feet-offset: 17.8%; }
```

#### AgentView 接入点

`ui/src/components/agent/AgentView.tsx` 渲染输入框时，在 RichTextInput 外层加一个 `position: relative` 容器（如果还没有），把 PetWidget 作为 sibling 加进去：

```tsx
// 在 AgentView 渲染 composer 的位置（搜 `RichTextInput` 的 JSX 用例）
<div className="composer-wrapper relative">
  <PetWidget />
  <RichTextInput ... />
</div>
```

`composer-wrapper.relative` 让 PetWidget 的 `position: absolute` 锚到 composer 容器。`right: 16px` 给宠物在右上角留间距。

#### App.tsx 全局 sync 钩子

`usePetStateSync()` 只能在一处 mount。因为它注册 Tauri 事件 listener，重复 mount 会导致多份 listener。放在 `App.tsx` 顶层 useEffect 里：

```tsx
function App() {
  usePetStateSync()  // 全局 mount 一次
  // ... 其他
}
```

---

## 状态机

```
                ┌───────[stream-chunk]──────┐
                │                            ▼
   ┌─── idle ◄──┴── stream-reset ──── thinking ──── stream-complete ───► success
   ▲     │  ▲                            │                                  │
   │     │  └──[on mouseleave]           │                                  │
   │     │                               ▼                                  ▼
   │     └─[hover override]──► hover    error ◄──── stream-error            └─(1.5s timer)─► idle
   │                                     │
   │             user types ──► typing   └── (next user interaction) ──► idle
   │                              │
   └──────────── user clears text or unfocuses ────────────────────────────┘
```

### 信号映射详表

| 触发 | 来源 | 切到 | 备注 |
|---|---|---|---|
| 用户聚焦 composer + 有文本 | atom 派生 | typing | 仅当当前不是 thinking/success/error |
| 用户清空文本或失焦 | atom 派生 | idle | 同上 |
| Agent 流式产 token | Tauri `chat:stream-chunk` | typing | "AI is typing" — 视觉和用户敲键盘一致 |
| Agent 调工具 / 推理 | Tauri `chat:stream-tool-activity` | thinking | 工具完成后下个 stream-chunk 再切回 typing |
| 流完成 | Tauri `chat:stream-complete` | success → 1.5s → idle | 含 timer |
| 流出错 | Tauri `chat:stream-error` | error | 持续到下次用户输入 |
| 流被重置 / cancel | Tauri `agent:stream-reset` | idle | |
| 鼠标进入 pet 区域 | DOM `mouseenter` | hover（仅当 primary=idle） | 派生层处理 |
| 鼠标离开 pet 区域 | DOM `mouseleave` | 回 primary | |

**为什么 chat:stream-chunk 是 typing 而不是 thinking**：动画视觉（chibi 键盘 / 铅笔写字）的语义是"有人正在打字"——agent 在产 token 和用户在敲 composer 是同一类事件（都在产出文本）。"thinking" 保留给 agent 调工具 / 推理的间歇（chibi 摸下巴 / "?"）。这种分工和 ChatGPT 的"AI is typing..."指示符语义对齐。

### 显示派生规则（`petDisplayStateAtom`）

```
display = hover_active && primary === 'idle' ? 'hover' : primary
```

这样 thinking/typing/success/error 任何主状态下，hover 都被忽略——关键状态绝不被打断。

---

## 锚点与样式

### 几何

- Widget 尺寸：100×100 px（桌面默认），56×56 px（移动端缩放）
- 容器（composer-wrapper）需要 `position: relative`
- Widget 位置：`right: 16px`、`bottom: 100%`（widget 底边贴 composer 顶边）
- 角色脚/爪通过 `transform: translateY(--feet-offset)` 下沉到 widget 底边
  - Astro: `--feet-offset: 6.4%`
  - Clawby: `--feet-offset: 17.8%`
- 结果：角色脚底/爪底**精确落在 composer 顶边线上**，无浮空、无沉入

### 主题安全

12 段视频都是 alpha 透明 WebP（VP8L + ALPH chunk）。在所有 11 主题下：
- 角色周围像素 alpha=0 → 显示后面的 composer / 页面背景
- 不需要主题感知逻辑，CSS 不需要按主题切换底色

### 浏览器兼容

- Tauri Windows / Linux（WebView2 / WebKitGTK）：原生 animated WebP 支持 ✓
- Tauri macOS（WebKit）：macOS 11+ 支持 animated WebP ✓（uClaw 当前 build target 是 macOS 13+，无问题）
- 老版本 fallback：若日后需要，可用 PIL 把每段 WebP 转 APNG（~25MB/段，体积大但 universally supported）。不在 v1 范围。

### 状态切换实现

两层 `<img>` 叠加，opacity transition 280ms。新状态的图先 `src=` 触发加载（浏览器缓存），加载完 `requestAnimationFrame` 切 opacity。组件层在 `useEffect([state])` 里管理 active-layer 状态机，逻辑见上面 `PetWidget.tsx`。

---

## Settings

复用现有 Settings 模板（`ui/src/components/settings/AppearanceSettings.tsx` 或类似），增加：

```
Pet (桌面宠物)
  [ ] Enable desktop pet
       ┌─────────────────────────┐
       │ ○ Astro                 │  // 选中时旁边显示 idle 预览
       │ ○ Clawby                │
       └─────────────────────────┘
```

写入 `petEnabledAtom` / `petCharacterAtom`（atomWithStorage → localStorage）。

切换到关闭：`PetWidget` 立刻 `return null`，资源不再请求；浏览器缓存里的 idle/hover 等图保留，下次启用时秒加载。

切换角色：所有 12 段图换 character 前缀（`/pet/clawby-*.webp` ↔ `/pet/astro-*.webp`），首次切换需要重新下载新角色的资源。

---

## 性能与可访问性

### 资源预加载

第一版用浏览器原生懒加载（`<img>` 默认行为）：

- App 启动后 → idle 立刻请求
- 鼠标 hover 到宠物 → hover 资源开始下载（可能延迟显示一次，之后浏览器缓存命中）
- 同理 thinking / typing / success / error 首次触发时下载

文件大小 1.5–2.5 MB，本地 dev 启动延迟可忽略；生产时可考虑在 idle mount 后 setTimeout 1000ms 预拉所有 6 个状态（一次性吃 ~12 MB 流量，换全程零延迟）。**v1 不做预拉**，按需就够。

### 防抖

- success → idle 的 1500ms timer 用 useRef 持有 timeoutId；如果中途 thinking 又来了，先 clearTimeout。
- thinking 切到 typing 不允许（agent 在干活时用户的打字不能盖掉 thinking 显示）。这一条在 `usePetStateSync` 的 effect 里已有 guard。

### 可访问性

- `<img alt="">`（装饰性，screen reader 跳过）。如果用户要求语音提示，可以改为 `alt="AI assistant is thinking"` 等动态文案。
- `prefers-reduced-motion: reduce` 媒体查询应该静止显示 idle 第一帧 → 实现方法：用 PIL 离线把每段 WebP 第 0 帧导出为 PNG，CSS `@media (prefers-reduced-motion: reduce) { .pet-layer { content: url(/pet/<char>-<state>-poster.png) } }`。**v1 不做**，但留 hook。
- 宠物**不接收键盘 focus**，纯装饰。不会污染 Tab 顺序。

### 性能预算

- 单段视频：4s × 24fps = 96 帧，720×720 RGBA。GPU 解码 + CSS compositing，约占 1-2% CPU on M1。可忽略。
- 同时两个 layer 解码：2-4% CPU during crossfade，280ms 后回到 1-2%。
- 内存：每段解码后约 720×720×4 × 96 / 压缩比 ≈ 30 MB／段，浏览器 LRU 自动释放未在 DOM 中的。

---

## 测试计划

### 单元

`ui/src/atoms/pet-atoms.test.ts`：
- `petDisplayStateAtom` 派生表（primary × hover）的所有组合
- hover 在 thinking/typing/success/error 下被忽略
- success 状态自动 1500ms 回 idle（用 vi.useFakeTimers）

`ui/src/hooks/usePetStateSync.test.ts`：
- mock Tauri `listen` → emit stream-chunk → primary 切到 thinking
- emit stream-complete → primary 切 success 然后 idle
- composer focus+text 时 primary 切 typing
- 在 thinking 期间 composer 变化不切换

### 组件

`ui/src/components/agent/PetWidget.test.tsx`：
- enabled=false 渲染 null
- enabled=true 渲染两个 img layer
- state 变化触发 crossfade（layer opacity 互换）
- hover handlers 调用 setHover

### 集成

手动验证清单：
- [ ] 11 个主题下宠物背景全部透明、无白方块
- [ ] Astro 标准锚点（脚踩在 composer 顶边、不浮空、不沉入）
- [ ] Clawby 标准锚点（爪扒在 composer 顶边）
- [ ] 完整流程：聚焦 → 打字 → 发送 → thinking → success → 回 idle
- [ ] 错误流程：发送 → thinking → 触发 error → 显示 error 直到下次用户输入
- [ ] Hover：idle 上能切 hover，thinking 上不打断
- [ ] Settings 切换角色：图换得过来
- [ ] Settings 关闭：widget 消失、无资源请求

---

## 文件清单（实现 plan 入口）

### 新建

- `ui/src/atoms/pet-atoms.ts`
- `ui/src/atoms/pet-atoms.test.ts`
- `ui/src/hooks/usePetStateSync.ts`
- `ui/src/hooks/usePetStateSync.test.ts`
- `ui/src/hooks/usePetHover.ts`
- `ui/src/components/agent/PetWidget.tsx`
- `ui/src/components/agent/PetWidget.css`
- `ui/src/components/agent/PetWidget.test.tsx`
- `ui/public/pet/` — 12 个 webp 文件，从 `.superpowers/brainstorm/<session>/videos/webp/` 复制
- `ui/src/components/settings/PetSettings.tsx`（或合并到 AppearanceSettings）

### 修改

- `ui/src/atoms/index.ts` — re-export pet-atoms
- `ui/src/App.tsx` — 调用 `usePetStateSync()`（全局 mount 一次）
- `ui/src/components/agent/AgentView.tsx` — composer 区域加 `.composer-wrapper.relative` + `<PetWidget />`
- `ui/src/components/settings/` 主页 — 入口链接到 PetSettings

### 不需要改

- Rust 后端：复用现有 IPC 事件
- 数据库：纯前端偏好，无 schema 影响
- Settings 持久化：`atomWithStorage` 自动 localStorage，无需 Tauri 命令

---

## 范围外（future）

1. **Chat 模式（ChatInput.tsx）**：chat 没有 thinking/success/error，只用 idle/hover/typing 三态。同一 PetWidget 组件能复用，只需要把 `usePetStateSync` 改成 chat-aware 版本，或在 ChatInput 包一层 `<PetWidget mode="chat" />`。
2. **更多角色**：纸鹤君（中式）、忆灵（果冻）——本次品牌探索时一起做过形象但未生成动画。需要时按相同流程生成 6 段视频 + chromakey。
3. **声音**：success/error 的音效。可加 `<audio>` 在状态切换时播放。需要做 mute 开关。
4. **更多状态**：典型扩展——`waiting-for-approval`（risky tool 等待用户授权时）、`offline`（网络断开时）。
5. **iOS / Web 部署**：当 uClaw 出 web/iOS 版本时，animated WebP 在 Mobile Safari 也支持，资源直接复用。
6. **自定义角色**：用户上传自己的素材包 → 6 段 alpha WebP + manifest.json 描述 feet-offset。
7. **Reduced-motion 静态海报**：见"可访问性"。
8. **预加载策略**：idle mount 后台预拉其他 5 段，换零延迟。

---

## 资源生成 pipeline 回溯（archive）

供未来生成新角色/新状态参考：

1. **角色绿底基准 PNG**：`nano-banana` 编辑原 base PNG，把白底换成 chroma green (#92EB57)。
2. **Veo 视频生成**：`tools/gen-video-vertex.sh "<prompt>" out.mp4 <green-base.png> 4`（Vertex AI Veo 3-fast，~$0.10/段）。Gemini API 配额紧（$5/月免费层不含 Veo），Vertex 配额宽松。
3. **固定 crop**：`ffmpeg ... -vf "crop=720:720:0:280" ...`。**不要用 `cropdetect`**——星星/sparkle/sweat-drop 这种 floating overlay 会让它误判。
4. **YUV chroma-key**：`tools/chromakey-alpha.sh in.mp4 out.webp`。Python NumPy YUV 色度距离（sim=0.20, soft=0.08）+ img2webp -m 2 -q 75 -mixed。
5. **裁掉 ramp-up 帧**：`tools/trim-frames.sh in.webp out.webp 16 95`。**Veo 几乎所有输出第 0 帧都是 reference PNG 的初始姿势**——例如 Astro typing 第 0 帧没有键盘、Clawby typing 第 0 帧没有铅笔。loop 时会反复看到"准备开始"的过渡。所有需要"持续动作"的状态（typing 必须、thinking 看情况）都应裁前 ~16 帧。
6. **节奏调整**：`tools/add-pauses.sh in.webp out.webp <pause_seconds>`。为 idle / thinking / error 在末尾追加锚点帧（默认锚 = 第 0 帧 = 起始 neutral 姿势），让 loop 有"动 → 静 → 动"的呼吸感。typing / hover / success 不加 pause。
7. **测量 feet-offset**：PIL 找每段 alpha 的最深底像素，跨所有帧取最深值，转 % 写进 CSS。当前值：Astro 6.4% / Clawby 17.8%（基于 idle 状态测量；如果某状态有更深的 bottom，可能要单独覆盖）。

### Prompt 模板要点

Veo 对"持续动作"类提示有偏弱倾向——倾向于做"起手 → 动作 → 收手"的 arc。要锁住"全程在做同一动作"，prompt 要：
- 反复强调 "THROUGHOUT THE ENTIRE 4 SECONDS"、"FROM START TO FINISH"、"NEVER stop"、"NEVER return to neutral pose"
- 列出**禁止动作**："NEVER waves"、"NEVER stands fully upright"
- 明确**唯一允许的动作**："The ONLY motion is: (1) ..., (2) ..."
- 即便如此，Veo 仍会有 5–15 帧的 ramp-up——靠 `trim-frames.sh` 兜底

所有脚本 + venv 都在 `.superpowers/brainstorm/41114-1778657173/` (gitignored)。生产前应该把 12 段 final WebP 复制到 `ui/public/pet/`，其他暂存物不入库。

### 资源指纹（v1 发版前的状态）

| 文件 | 帧数 | 时长 | 大小 |
|---|---|---|---|
| pet-astro-idle.webp | 238 | 10s loop (40:60) | 1.58 MB |
| pet-astro-hover.webp | 96 | 4s loop (100% motion) | 1.88 MB |
| pet-astro-thinking.webp | 119 | 5s loop (80:20) | 1.63 MB |
| pet-astro-typing.webp | 80 | 3.3s loop (100% typing) | 1.22 MB |
| pet-astro-success.webp | 96 | 4s one-shot, 1.5s 后 timer 切回 idle | 1.67 MB |
| pet-astro-error.webp | 143 | 6s loop (67:33) | 1.53 MB |
| pet-clawby-idle.webp | 238 | 10s loop (40:60) | 2.24 MB |
| pet-clawby-hover.webp | 96 | 4s loop | 2.14 MB |
| pet-clawby-thinking.webp | 119 | 5s loop (80:20) | 2.31 MB |
| pet-clawby-typing.webp | 80 | 3.3s loop (100% writing) | 1.73 MB |
| pet-clawby-success.webp | 96 | 4s one-shot | 2.37 MB |
| pet-clawby-error.webp | 142 | 6s loop (67:33) | 2.04 MB |

总计约 22 MB（两角色一起算，单角色 ~11 MB）。
