# HomeOffice 设计文档

**日期**：2026-05-14
**作者**：uClaw 团队
**状态**：草案 → 待 review
**关联**：PetWidget ([2026-05-13-pet-widget-design.md](./2026-05-13-pet-widget-design.md))、Focus Mode ([2026-05-13-focus-mode-design.md](./2026-05-13-focus-mode-design.md))

## 目的

在 uClaw 主窗口里开一个**全屏的"天空之城"工作空间**——一张 16:9 的 Miyazaki 水彩浮空岛，岛上 8 个 zone 把 agent 的状态、用户的输入、用户的技能/资源**可视化成有温度的场景**。Agent 不再是只在右边 chat panel 里的文字流，而是一个会**走到对应 zone 工作**的角色：

- thinking → 走去图书塔翻书
- typing → 坐在中央橡树下书桌写日记
- 调用工具 → 走去工坊烤炉
- idle → 趴在右下吊床上
- success → 火堆烧旺
- error → 火苗变小

UX 目标：把"agent 在干活"从 loading 条变成**一个真的角色在一个真的世界里做事**。这是 uClaw 区别于 ChatGPT/Claude.ai 的核心情感锚点。

参考：Star-Office-UI（Phaser 像素风办公场景）、宫崎骏《天空之城》《幽灵公主》。

## 范围与分期

本 spec **覆盖 4 个 sub-projects**，但 Phase 1（中 MVP）是首个可交付的可点击单元。后续 phase 在本 spec 内定接口、留 hook，但具体实现写各自的 plan：

| Phase | 内容 | 何时实现 |
|---|---|---|
| **1 · MVP** | 场景 + 8 zones + 1 角色 8 方向走动 + 4 交互 zones 全打通 + state 机接 agent 事件 + LeftSidebar 入口 | 本 spec 直接进 plan |
| **2 · 多 agent** | 多角色实例共存 + 按 Y 轴 z-sort + 简易避障 + 多 agent 消息路由 | Phase 1 落地后单独 spec/plan |
| **3 · 装饰** | 装饰物表 + inventory drawer + drag-and-drop + 昼夜变体 | Phase 1 落地后单独 spec/plan |
| **4 · Memo/Diary** | 便签持久化 + agent 日记历史 + proactive 定时巡检 scenario | Phase 1 落地后单独 spec/plan |

Phase 1 自身可独立上线、用户立刻能用——这是 MVP 的硬约束。

## 设计原则

- **PixiJS via pixi-react**：WebGL 2D 引擎，~150KB，专为多 sprite + 粒子设计。和 Jotai/React 状态融合通过 pixi-react 的 declarative 组件。
- **场景背景是静态 PNG**：v5 已锁定的 1920×1080 Imagen 渲染图直接当 BG 层。粒子（樱花/kodama）由 PixiJS 实时绘制叠加。
- **8 zones 是逻辑分区，不是独立场景**：用透明 hit-area 标定，hover 时显示发光描边，click 触发对应交互。
- **角色用 animated WebP**：和 PetWidget 同一管线（Veo 生成 + chromakey + img2webp）。PixiJS 把 WebP 当 Sprite 的 texture，每帧 swap。
- **Tauri 事件已经够用**：state 机复用 PetWidget 已经接好的 `chat:stream-*` 事件（[ui/src/hooks/usePetStateSync.ts](ui/src/hooks/usePetStateSync.ts)）。本 spec 不加新 Rust 命令。
- **Phase 1 单角色单 agent**：Phase 2 才上多角色——但数据结构和 zone 接口预留好。
- **可关、可换**：默认关闭。LeftSidebar 入口点开才加载场景，离开释放资源。
- **主题安全**：场景内文字（zone tooltip、modal）走主题 token，不硬编码颜色。背景图本身是固定 Miyazaki 调色，不参与主题切换（Phase 3 才上昼夜）。

## 用户故事

### Phase 1

1. 用户在 LeftSidebar 看到新按钮 `🏝️ Home Office`（在 `Automation` 下方），点击 → 主区切到全屏 HomeOffice 视图（其他 panel 收起，类似 Focus Mode）。
2. 场景加载：水彩天空之城浮空岛，云在飘，瀑布在流，樱花花瓣飘落，3 只 kodama 树精灵在岛上漂浮。
3. Lofi girl 角色站在中央橡树下书桌前（默认 idle 位置）。
4. 用户在右侧 chat panel 开始打字 → 角色走到橡树下，坐下，掏出日记本开始写（typing state）。
5. 用户回车发送 → 角色合上本子、起身、走去左上图书塔，坐在塔前翻书（thinking state）。
6. Agent 调工具 → 角色起身走去工坊（tool-activity state，复用 thinking 视觉但走到不同 zone）。
7. Agent 完成 → 角色走回中央广场，跳一下，火堆烧旺（success state）→ 4 秒后走去吊床趴下（回 idle）。
8. Agent 失败 → 角色走去火堆边坐下，火苗变小（error state）。
9. 用户点 🎵 音乐木亭 → 弹出播放器 modal，HTML5 audio 播放预设 lofi playlist。
10. 用户点 📌 石碑便签墙 → 弹出便签 modal，能加 / 删 / 列便签（Phase 1 in-memory，Phase 4 持久化）。
11. 用户点 ✍️ 橡树书桌 → 弹出 agent 日记 modal（Phase 1 只读，显示 in-memory 最近若干条；Phase 4 持久化 + 历史）。
12. 用户点 🌿 花园 → 跳转到 Skills 页面（复用现有 skills UI）。
13. 用户点 📚 图书塔 → 跳转到 session history 页面（复用现有 history UI）。
14. 用户离开 HomeOffice 视图（点别的 LeftSidebar 按钮）→ PixiJS app 销毁，资源释放。

### Phase 2-4（接口预留，本 spec 不实现细节）

- Phase 2：用户在 settings 启用 multi-agent，每个 active agent 在场景里有自己的角色实例，按 Y 轴遮挡排序。
- Phase 3：用户开 inventory drawer，把一盆植物拖到场景里某个位置，刷新后位置保持。昼夜按系统时间切换 BG 图。
- Phase 4：用户写的便签持久化；agent 日记按 session 归档；agent 定时巡检便签，遇到匹配触发器自动执行。

## 视觉资产

### 场景背景

- **路径**：`ui/public/home-office/scene-sky-v5.png`
- **来源**：v5 Imagen-4-fast 渲染（已在 `.superpowers/brainstorm/24317-1778687442/renders/scene-sky-v5-rich.png`）
- **尺寸**：1920×1080（16:9），后续 Phase 3 升清到 2K-4K 再说
- **内容**：Miyazaki 水彩 + 浮空岛 + 8 zones 视觉锚点（中央橡树 + 书桌、左上图书塔、左下花园、左中音乐亭、上方石碑、中下火堆、右下吊床、右中樱花树）+ 瀑布 + 远岛 + 云海

### 角色

Phase 1 只做 1 个角色：**Lofi girl**（参考 `.superpowers/brainstorm/24317-1778687442/renders/char-a-lofi-girl.png`）。

8 方向 walk + 5 state poses（idle / thinking / typing / success / error）。

```
方向枚举（顺时针起 N）：N / NE / E / SE / S / SW / W / NW
Mirror 优化：E = mirror(W)，NE = mirror(NW)，SE = mirror(SW)
实际生成方向：N / NW / W / SW / S（5 个）
```

资产清单（Phase 1）：

```
ui/public/home-office/sprites/lofi-girl/
  walk-N.webp     walk-NW.webp    walk-W.webp     walk-SW.webp    walk-S.webp
  pose-idle.webp        # 趴吊床 / 站立呼吸
  pose-thinking.webp    # 坐塔前翻书
  pose-typing.webp      # 桌前写字
  pose-success.webp     # 双手举起
  pose-error.webp       # 低头捂耳
```

5 walk + 5 pose = 10 个 WebP，每个 ~720×720，~1-2MB。

生成管线复用 PetWidget 的 `tools/`：Veo 绿幕视频 → chromakey → img2webp。脚本写一份 `scripts/gen-home-office-sprites.sh` 批量跑。

### 粒子

PixiJS 内绘制，不需要图片资源：

- **樱花花瓣**：5-10 片粉色椭圆，从画面顶部右侧飘落到底部左侧，drift + 旋转
- **kodama 树精灵**：3 个小白球，在岛上画 2D 漂浮路径（Lissajous 曲线），偶尔头偏一下
- **瀑布水雾**：场景右下 + 中下两道瀑布底部，喷出半透明白色圆点向上扩散
- **流星**：夜晚变体才需要（Phase 3）

### Audio

`ui/public/home-office/audio/`：lofi playlist 3-5 首 MP3。版权来源：用户自备，或用 CC0 lofi 资源（Phase 1 可以放占位音轨）。

## 架构

### 文件结构

```
ui/src/
  components/home-office/
    HomeOfficeView.tsx              # 全屏页面容器，full-window 模式，类 FocusMode
    scene/
      HomeOfficeScene.tsx           # pixi-react <Stage>，挂所有 layer
      layers/
        BackgroundLayer.tsx         # 静态 v5 BG
        ZoneLayer.tsx               # 8 zones hit-area + hover 高亮
        CharacterLayer.tsx          # 1 角色 sprite（Phase 1）
        ParticleLayer.tsx           # 樱花 + kodama + 瀑布雾
      hit-areas.ts                  # 8 zones 的归一化坐标（相对 1920x1080）
      sprite-loader.ts              # WebP → PIXI.Texture 加载 + 缓存
      animator.ts                   # WebP 帧定时驱动 PIXI.Sprite.texture swap
    zones/
      MusicGazeboModal.tsx          # 音乐播放器 modal
      StickyNoteModal.tsx           # 便签 modal（Phase 1 in-memory）
      DiaryDeskModal.tsx            # 日记 modal（Phase 1 只读 in-memory）
  atoms/
    home-office-atoms.ts            # 启用开关、当前 agent state、character 位置、便签列表、日记列表
  hooks/
    useHomeOfficeAgentSync.ts       # Tauri 事件 → 角色 state + 目的 zone
    useCharacterPath.ts             # state 切换 → 路径规划（A* lite 或简单 lerp）
ui/public/home-office/
  scene-sky-v5.png
  sprites/lofi-girl/*.webp
  audio/*.mp3
scripts/
  gen-home-office-sprites.sh        # 批量生成 sprite 的辅助脚本
```

### 依赖

```bash
npm i pixi.js@^8 @pixi/react@^8
```

Pixi v8 + @pixi/react v8 是当前稳定线，原生支持 React 18，WebGL2 默认开。

### 状态原子

[ui/src/atoms/home-office-atoms.ts](ui/src/atoms/home-office-atoms.ts)：

```typescript
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

// 全局开关 — settings 控制
export const homeOfficeEnabledAtom = atomWithStorage('uclaw.homeOffice.enabled', false)

// 当前是否打开（页面级）
export const homeOfficeViewActiveAtom = atom(false)

// Agent 当前状态（复用 PetWidget 的 5 态枚举）
export type HomeOfficeState =
  | 'idle'
  | 'thinking'
  | 'typing'
  | 'tool_activity'  // 视觉等价 thinking 但走到工坊
  | 'success'
  | 'error'

export const homeOfficeStateAtom = atom<HomeOfficeState>('idle')

// 角色当前坐标（归一化 0-1，相对场景 1920x1080）
export type Vec2 = { x: number; y: number }
export const characterPositionAtom = atom<Vec2>({ x: 0.5, y: 0.55 })  // 默认橡树前

// 角色当前朝向（用于选 walk sprite）
export type Direction = 'N' | 'NE' | 'E' | 'SE' | 'S' | 'SW' | 'W' | 'NW'
export const characterDirectionAtom = atom<Direction>('S')

// 角色当前是 walking 还是 posing（决定用 walk-X 还是 pose-X）
export const characterMotionAtom = atom<'walk' | 'pose'>('pose')

// Phase 1 in-memory，Phase 4 换 atomWithStorage 或 Tauri 持久化
export const stickyNotesAtom = atom<Array<{ id: string; text: string; at: number }>>([])
export const diaryEntriesAtom = atom<Array<{ id: string; text: string; at: number; sessionId: string }>>([])

// 当前打开的 zone modal（null = 无）
export type OpenZone = null | 'music' | 'sticky' | 'diary'
export const openZoneAtom = atom<OpenZone>(null)
```

### 状态机（agent state → 角色行为）

复用 PetWidget 的 `usePetStateSync` 逻辑，但**结果不止改表情，还驱动走路**：

和 [usePetStateSync.ts](ui/src/hooks/usePetStateSync.ts) 完全对齐，只是结果不再是改表情，而是切走路目标 zone + pose：

| Tauri 事件 | HomeOfficeState | 目标 zone（场景坐标） | 角色动作 |
|---|---|---|---|
| `chat:stream-chunk` | `typing`（agent 正在回复） | 中央橡树书桌 (0.50, 0.55) | 走到 → `pose-typing` |
| `chat:stream-tool-activity` | `tool_activity` | 工坊烤炉 (0.30, 0.70) | 走到 → `pose-thinking`（视觉等价 thinking，只是目的地不同） |
| 进入推理但无 chunk / 无 tool（reserved） | `thinking` | 图书塔 (0.78, 0.30) | 走到 → `pose-thinking` |
| `chat:stream-complete` | `success` | 当前位置 | 原地 `pose-success` 4s → walk 去吊床 → idle |
| `chat:stream-error` | `error` | 火堆 (0.42, 0.75) | 走到 → `pose-error` |
| `agent:stream-reset` | `idle` | 吊床 (0.82, 0.62) | 走到 → `pose-idle` |

> `thinking` 在 PetWidget 当前事件流里没有独立触发器（只走 `tool_activity` → 复用 thinking pose）。HomeOffice 保留 `thinking` 作为单独 zone（图书塔），等未来 Rust agent 发独立 "pre-tool reasoning" 事件时启用。Phase 1 实现里 `thinking` zone 是 dead code path——但保留枚举，避免 Phase 2 时重新洗状态机。

走路逻辑（`useCharacterPath`）：

1. state 变化触发 → 算当前位置 → 目标位置的方向向量
2. 选最近的 8 方向值 → 设 `characterDirectionAtom`
3. `characterMotionAtom = 'walk'`
4. 按 ~150ms/格的速度 lerp 位置（Phase 1 直线，不绕障碍）
5. 抵达目标 → `characterMotionAtom = 'pose'`，按 state 选对应 `pose-X.webp`

走路时 PIXI.Sprite 用 `walk-{direction}.webp` 的当前帧；切到 pose 时 swap 到 `pose-{state}.webp`。

### Animator（WebP 帧驱动）

PixiJS 不原生支持 animated WebP。我们自己拆帧 + 定时 swap：

```typescript
// scene/animator.ts
import { Texture, Sprite, Assets } from 'pixi.js'

export class WebpAnimator {
  private frames: Texture[] = []
  private currentFrame = 0
  private lastTickMs = 0
  private fps: number

  constructor(private sprite: Sprite, frames: Texture[], fps = 24) {
    this.frames = frames
    this.fps = fps
  }

  tick(deltaMs: number) {
    this.lastTickMs += deltaMs
    const frameDurationMs = 1000 / this.fps
    while (this.lastTickMs >= frameDurationMs) {
      this.lastTickMs -= frameDurationMs
      this.currentFrame = (this.currentFrame + 1) % this.frames.length
      this.sprite.texture = this.frames[this.currentFrame]
    }
  }

  swap(newFrames: Texture[]) {
    this.frames = newFrames
    this.currentFrame = 0
    this.lastTickMs = 0
    this.sprite.texture = newFrames[0]
  }
}
```

WebP 拆帧用 `<img>` + `<canvas>` 在加载时一次性预切，存进 sprite-loader 的缓存：

```typescript
// scene/sprite-loader.ts
async function loadAnimatedWebp(url: string): Promise<Texture[]> {
  if (!('ImageDecoder' in window)) {
    // 极端 fallback：返回单帧静态 texture（角色不动，但场景能开）
    const tex = await Assets.load(url)
    return [tex]
  }
  const res = await fetch(url)
  const decoder = new ImageDecoder({ data: res.body, type: 'image/webp' })
  await decoder.completed
  const frameCount = decoder.tracks.selectedTrack!.frameCount
  const textures: Texture[] = []
  for (let i = 0; i < frameCount; i++) {
    const { image } = await decoder.decode({ frameIndex: i })
    const bitmap = await createImageBitmap(image)
    textures.push(Texture.from(bitmap))
  }
  return textures
}
```

**Animated WebP 拆帧**：浏览器原生 `ImageDecoder` API（Chrome 94+、Safari 17+）支持迭代每帧，转 ImageBitmap → `Texture.from(bitmap)`。Tauri webview 是 WebKit (macOS) / WebView2 (Win) / WebKitGTK (Linux)，都支持。

### Zone 命中

8 zones 的坐标（归一化，参考 v5 PNG）：

```typescript
// scene/hit-areas.ts
export const ZONES = {
  garden:        { center: { x: 0.18, y: 0.78 }, w: 0.14, h: 0.18, kind: 'navigate', target: 'skills' },
  music:         { center: { x: 0.13, y: 0.45 }, w: 0.18, h: 0.32, kind: 'modal',    target: 'music' },
  sticky:        { center: { x: 0.36, y: 0.18 }, w: 0.16, h: 0.24, kind: 'modal',    target: 'sticky' },
  diary:         { center: { x: 0.50, y: 0.45 }, w: 0.20, h: 0.40, kind: 'modal',    target: 'diary' },
  library:       { center: { x: 0.68, y: 0.22 }, w: 0.13, h: 0.34, kind: 'navigate', target: 'history' },
  fire:          { center: { x: 0.42, y: 0.75 }, w: 0.14, h: 0.18, kind: 'state',    target: null },  // 不可点
  hammock:       { center: { x: 0.82, y: 0.62 }, w: 0.14, h: 0.18, kind: 'state',    target: null },
  sakura:        { center: { x: 0.70, y: 0.55 }, w: 0.14, h: 0.24, kind: 'state',    target: null },
} as const
```

ZoneLayer 给每个 hit-area 加 `interactive = true` + `hitArea = new Rectangle(...)`，pointer over → 发光描边（PIXI.Graphics 画 dashed rect 半透明 + 标签），click → 按 `kind` 触发：

- `navigate` → 调 `setActivePanel('skills')` 之类的 LeftSidebar action
- `modal` → 设 `openZoneAtom`，对应 modal 组件挂在 React 树
- `state` → 仅视觉，不响应 click

### Tauri 命令 / 后端改动

**Phase 1 零后端改动**。复用：

- `chat:stream-chunk`、`chat:stream-tool-activity`、`chat:stream-complete`、`chat:stream-error`、`agent:stream-reset`（PetWidget 已用）

Phase 4 才上后端：

- 新 Tauri 命令 `home_office_list_memos` / `home_office_add_memo` / `home_office_delete_memo`
- 新 Tauri 命令 `home_office_list_diary` / `home_office_add_diary_entry`
- 新 migration **V20**（注意：跟[active migration registry](../../CLAUDE.md) 同步——本 spec 写时占用最高 V19，本 spec Phase 4 占 V20）
- 新 proactive scenario `home_office_memo_poll`

### LeftSidebar 入口

`ui/src/components/sidebar/LeftSidebar.tsx`（或现有按钮列表所在文件）增加一项：

```tsx
<SidebarButton
  icon={<HomeOfficeIcon />}
  label="Home Office"
  active={activePanel === 'home-office'}
  onClick={() => setActivePanel('home-office')}
/>
```

挂在 `Automation` 按钮**下方**（用户明确指定）。点击设 `activePanel = 'home-office'`，主区切到 `<HomeOfficeView />`，类似 Focus Mode 的全屏方式但右侧 chat panel 保留（不像 Focus Mode 那样收起 chat——因为用户还需要看 agent 的对话）。

布局：

```
+--------+----------------------------+--------+
| Left   |                            | Chat   |
| Side   |   HomeOfficeView (Pixi)    | Panel  |
| bar    |                            | (右)   |
|        |                            |        |
+--------+----------------------------+--------+
```

LeftSidebar 不动，ChatPanel 不动，**中间主区**换成 HomeOfficeView。Focus Mode 是把侧栏全收，HomeOffice 不收——这是关键区别。

## 数据流

```
[Tauri Rust agent loop]
  ↓ emit chat:stream-chunk / tool-activity / complete / error / reset
[ui/src/hooks/useHomeOfficeAgentSync.ts]
  ↓ map event → HomeOfficeState
[homeOfficeStateAtom (jotai)]
  ↓ subscribe in useCharacterPath
[characterPositionAtom / characterDirectionAtom / characterMotionAtom]
  ↓ subscribe in CharacterLayer (pixi-react)
[PIXI.Sprite.texture swap via WebpAnimator + position lerp via Ticker]
```

并行：

```
[User clicks zone in PixiJS canvas]
  ↓ ZoneLayer pointerdown
[openZoneAtom 设值 / navigate / nothing]
  ↓
[Modal 组件 mount（React 层），或 LeftSidebar setActivePanel]
```

## 性能预算

- BG 静态 PNG 1920×1080，加载 ~500KB → texture upload 一次
- 角色 sprite ~10 个 WebP × 24 帧 × 720×720 RGBA = ~50MB GPU texture（懒加载，进入 HomeOfficeView 才加载，离开释放）
- 粒子最多 30 个 sprite（10 樱花 + 3 kodama + 17 水雾），不耗
- 目标 60fps，Pixi v8 WebGL 跑这个量级毫无压力

性能开关：用户在 settings 可以禁粒子（低端机）。

## 错误处理

- WebP 加载失败 → log warn + 用静态 PNG fallback（每个 sprite 同名 `.png` 第 0 帧）
- ImageDecoder API 不可用 → 降级用 `<img>` 当 single-frame texture（角色不动，但场景能开）
- 场景背景 PNG 缺失 → 显示纯色 BG + 错误 toast `Home Office assets not found, please reinstall`
- 用户网络下载音乐失败 → 静默，播放器显示 "no track"

## 测试策略

Vitest + jsdom 测试 React 部分，PixiJS 部分用集成测试或手动验证：

| 单元 | 测试方式 |
|---|---|
| `home-office-atoms.ts` 状态切换 | Vitest 单元测试 |
| `useHomeOfficeAgentSync` 事件 → state 映射 | Vitest mock Tauri event |
| `useCharacterPath` 方向选择 + lerp | Vitest 单元测试（pure 函数） |
| Zone modal mount / unmount | RTL 渲染测试 |
| LeftSidebar 入口 click → activePanel | RTL 集成测试 |
| PixiJS scene 渲染 | jsdom 跑不动 WebGL → 手动 + Playwright e2e 单独跑（Phase 1 不强求） |

PixiJS canvas 在 jsdom 下不可测——`HomeOfficeScene.tsx` 用 `Stage` 组件，**测试时 mock 成 `<div data-testid="pixi-stage" />`**。逻辑测试都在 hooks / atoms 层，渲染层手动验证。

## 已知限制 / YAGNI

- Phase 1 直线走路，不绕场景中央的橡树/书桌。用户能看出来角色穿过去——可接受，Phase 2 加 A*。
- Phase 1 in-memory 便签和日记，**刷新页面就丢**。这是 Phase 4 的活，明示在 UI 上（modal header 写 "暂存，重启丢失"）。
- 多 agent 在 Phase 1 不工作。只有 1 个角色对应当前 active agent（一般是 default agent）。
- 角色"走路"和"姿势"是分离 sprite，不无缝衔接——抵达目标瞬间会有视觉跳变。Phase 2 加 transition 帧再说。
- 昼夜变体在 Phase 1 不上，永远是白天。
- 音乐播放器 Phase 1 用占位音轨；正式音乐版权 Phase 3 处理。
- 角色 WebP 资产 Phase 1 只做 1 个角色（Lofi girl）。Scholar / Hooded 在 Phase 2 做。

## 与 PetWidget 的关系

PetWidget 留在 chat 输入框右上角（已上线），HomeOffice 是新页面。两者**并存不冲突**：

- 用户在普通 chat 视图：PetWidget 表演 state，HomeOffice 关闭
- 用户在 HomeOffice 视图：HomeOffice 角色表演 state，PetWidget 仍在右侧 chat panel 的输入框角落（不冲突，两个独立的视觉表现）

如果未来想统一，可以在 HomeOffice 打开时隐藏 PetWidget，但 Phase 1 不做。

## 风险与开放问题

1. **Pixi v8 + React 18 集成**：pixi-react v8 是 declarative，比 v7 命令式更易用，但生态较新。如果踩坑可降到 v7 imperative API（不破坏架构）。
2. **WebP 帧拆解性能**：ImageDecoder API 在大 WebP 上可能慢（24 帧 720×720 → 一次性解码 ~200ms）。如果卡，预热进度条 + 后台解。
3. **角色"穿模"**：直线走会从橡树后面穿过去而不是绕过——Phase 1 接受，但可以提前在每个 zone 之间画几条"路径锚点"用最近邻路径而非纯直线。
4. **Tauri 事件并发**：用户在 thinking 中再发一条消息——当前 PetWidget 用 `agent:stream-reset` 重置。HomeOffice 复用同一逻辑，角色会立刻切方向重新走，但这看起来可能很奇怪。等 Phase 2 多 agent 再处理（每个 agent 独立 state）。
5. **退出 HomeOffice 时 PIXI 资源释放**：必须在 unmount 时调 `app.destroy(true, { children: true, texture: true })`，否则内存泄漏。这是常见 Pixi-react 坑，在 plan 里要写明 cleanup test。

## 决策记录

| 决策 | 选项 | 选择 | 理由 |
|---|---|---|---|
| 引擎 | DOM/CSS / Pixi / Phaser / R3F | **PixiJS + pixi-react** | 2D WebGL 专用，~150KB，粒子 + 多 sprite + Z-sort 都现成，React 集成顺 |
| 行走方向 | 2 / 4 / 8 | **8** | 沿斜路走自然，mirror 优化后只生 5 方向，可接受成本 |
| MVP 范围 | 小 / 中 / 大 | **中** | 场景 + 1 角色 + 4 交互 zones 全打通，2-3 个 PR 可交付，用户立刻能用 |
| 美术风格 | 像素 / 卡通 / Miyazaki | **Miyazaki 水彩** | v5 已经 5 版迭代锁定 |
| 角色 sprite 格式 | PNG seq / sprite-sheet / WebP | **animated WebP** | 复用 PetWidget 管线，alpha 完美，Tauri webview 全支持 |
| 持久化 | Phase 1 持久化 / in-memory | **in-memory** | YAGNI，Phase 4 才需要持久化，DB migration 不预占 V20 |
| LeftSidebar 位置 | 顶部 / Automation 下方 / 设置内 | **Automation 下方** | 用户明确指定 |
| 与 chat panel 关系 | 全屏覆盖 / 主区切换 | **主区切换** | 用户还要看 agent 对话，不能像 Focus Mode 那样收 chat |

## 下一步

1. 用户 review 本 spec
2. 通过后调用 `superpowers:writing-plans` 生成 Phase 1 的实现 plan
3. Phase 1 plan 走 subagent-driven-development 执行
4. Phase 1 落地后再分别 spec Phase 2 / 3 / 4
