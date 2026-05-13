# Settings Page Redesign — Design

**Date:** 2026-05-13
**Branch:** `worktree-settings-redesign`
**Scope:** UI/UX overhaul of the in-app settings dialog. No new backend behavior.

## 1. Goal

把当前的 15-tab、900×600 固定尺寸的设置弹窗改造为：

- **9 个 tab**（中度合并语义相近的功能）
- **响应式 1200×800**（在小屏幕自动收缩，content 区放宽到 800px）
- **左 nav 加分组分隔 + 搜索框**
- **顶部 sticky breadcrumb** 显示当前 tab / 子区块
- **顺手补 STT 漏挂的 bug**（[`SttSettings.tsx`](../../../ui/src/components/settings/SttSettings.tsx) 上次 PR 写完了组件但忘了挂进 nav）
- **删 Bot stub**（占位实现，YAGNI）

## 2. Sources

- 现状清单见 [docs/superpowers/specs/.../prior]（本次扫描结果，spec § 6 列出原 15 tab）
- 主要文件：
  - [`ui/src/components/settings/SettingsDialog.tsx`](../../../ui/src/components/settings/SettingsDialog.tsx) — 弹窗尺寸（`w-[900px] h-[600px]`）
  - [`ui/src/components/settings/SettingsPanel.tsx`](../../../ui/src/components/settings/SettingsPanel.tsx) — TABS 数组、左 nav 渲染、内容路由 switch
  - [`ui/src/atoms/settings-tab.ts`](../../../ui/src/atoms/settings-tab.ts) — `SettingsTab` union type + `settingsTabAtom`
  - [`ui/src/components/settings/SttSettings.tsx`](../../../ui/src/components/settings/SttSettings.tsx) — 待挂入 nav
  - 现有 15 个 `<Tab>Settings.tsx`：channels / models / general / appearance / usage / agent / tools / permissions / prompts / skills / bots / shortcuts / proxy / pet / about
  - 设置原语：`SettingsCard` / `SettingsSection` / `SettingsRow`（[primitives/](../../../ui/src/components/settings/primitives/)）

## 3. Non-Goals

- ❌ 重写任何 `<Tab>Settings.tsx` 内部逻辑 —— 仅把它们作为 child 塞进新的 wrapper tab
- ❌ 后端改动（零 Rust）
- ❌ 设置数据格式变化 / 迁移
- ❌ 国际化（保持中文 UI）
- ❌ Bot tab 的实际功能 —— v0 删除占位符即可（如未来需要再加回，复活成本 < 10min）
- ❌ 动态 tab order / 用户自定义 nav

## 4. 新 Tab 结构（9 个）

| ID | 标签 | 子区块（用 SettingsSection 分） | 实现策略 |
|---|---|---|---|
| **`connectivity`** | 服务商与用量 | ① 服务商 ② 用量与预算 | 新 wrapper：纵向 stack `<ChannelSettings />` + `<UsageSettings />` |
| **`intelligence`** | 智能 | ① 模型分配 ② Agent 行为 ③ 提示词 | 新 wrapper：stack `<ModelSettings />` + `<AgentSettings />` + `<PromptsSettings />` |
| **`tools`** | 工具与能力 | ① 工具与 MCP ② 工具权限 ③ 已学技能 | 新 wrapper：stack `<ToolSettings />` + `<PermissionsSettings />` + `<SkillsSettings />` |
| **`general`** | 通用与外观 | ① 通用偏好 ② 主题与字体 | 新 wrapper：stack `<GeneralSettings />` + `<AppearanceSettings />` |
| **`stt`** | 输入（语音） | ① 模型 ② 转写 ③ 快捷键 | **直接挂 `<SttSettings />`** —— 修上次 PR 漏挂 |
| **`shortcuts`** | 快捷键 | — | 原样 `<ShortcutSettings />` |
| **`pet`** | 桌面宠物 | — | 原样 `<PetSettings />` |
| **`proxy`** | 代理 | — | 原样 `<ProxySetting />` |
| **`about`** | 关于 | — | 原样 `<AboutSettings />` |

**删除**：`bots`（stub，无实际逻辑）。

### 4.1 关键点：不重写既有组件

新增 4 个 wrapper 组件：

```tsx
// ConnectivityTab.tsx — 纯组合
export function ConnectivityTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <ChannelSettings />
      <UsageSettings />
    </div>
  )
}
```

每个原 `<Tab>Settings.tsx` 已经自带 SettingsSection 标题 + 描述，所以拼接后视觉上自然出现"二级分区"。如果某个子组件没标题，再外面套一层 `<SettingsSection title="...">` 即可。

## 5. Tab Nav 分组（左侧）

新 nav 按"主要功能 / 偏好 / 系统"轻分组：

```
┌─ 设置 ──────────────────  ✕ ┐
│ ┌─ Search ────────────┐    │
│ │ 🔍 搜索…              │   │
│ └────────────────────┘     │
│                            │
│ 主要功能                   │
│  📡 服务商与用量            │
│  🧠 智能                   │
│  🛠 工具与能力              │
│                            │
│ 偏好                       │
│  ⚙️ 通用与外观              │
│  🎤 输入（语音）            │
│  ⌨️ 快捷键                  │
│  🐾 桌面宠物                │
│                            │
│ 系统                       │
│  🌐 代理                   │
│  ℹ️ 关于                    │
└────────────────────────────┘
```

### 5.1 Nav 视觉规范
- 宽度 `w-[160px]` → `w-[200px]`（更舒适，icon + 标签不挤）
- 分组标题：`text-[10.5px] uppercase tracking-wider text-muted-foreground/70 px-3 py-2`（参考 ShortcutSettings v3 的 group-name 样式）
- 分组间距：`mt-4 first:mt-0`
- 各 tab item 维持现有 `px-3 py-2 rounded-md` 但加强 hover/active：
  - hover：`bg-muted/60`（现在是 `bg-muted/50`）
  - active：`bg-muted` + 左边一条 2px primary 竖线（subtle indicator）

### 5.2 搜索框
- 顶部位置（nav 内的第一个 child）
- `<input>` 占满 nav 宽度 `bg-muted/50 rounded-md px-2.5 py-1.5 text-xs`
- 输入触发本地 filter：tab 的 label + 该 tab 内 SettingsSection title 都参与匹配
- 命中的 tab 高亮，无命中的 tab 半透明 `opacity-40`
- 9 个 tab 不算多，搜索框定位是"老用户键盘流"加速，不是"主导航"

### 5.3 STT / about 等位置约定
- 红点提醒（hasUpdate）原本只在 `about`，保留
- STT 模型未下载时，nav 上「输入（语音）」也加红点提示 → 用 `modelStatusAtom.kind === 'not-downloaded'` 派生

## 6. Sticky Breadcrumb 顶栏

把现在的 12px 高 header 改为 sticky 顶栏，展示三段：

```
设置  /  智能  /  Agent 行为
                                              ✕
```

- "设置" = static 标题
- "智能" = 当前激活 tab label
- "Agent 行为" = 滚动到第 N 个子 SettingsSection 时同步显示（用 IntersectionObserver 观察每个 section 标题）
- 子段对 9 个 tab 中只有 4 个有意义（合并的几个），其他 tab 末段省略
- 高度 `h-12`（不变）
- 滚动时该顶栏 sticky（贴在 dialog 顶端）

## 7. Dialog 尺寸响应式

[`SettingsDialog.tsx`](../../../ui/src/components/settings/SettingsDialog.tsx) 的 `motion.div`：

```tsx
// before
className="w-[900px] h-[600px] ..."

// after
style={{
  width: 'min(85vw, 1200px)',
  height: 'min(85vh, 800px)',
}}
className="bg-background shadow-2xl rounded-2xl overflow-hidden"
```

- 13" MBA（约 1440×900）：1200×720 ≈ 84vw×80vh
- 27" 5K：饱和在 1200×800（不会无限拉大）
- 13" MBP 1280×800：1088×720（依然占绝大部分屏幕，比现在多）

### 7.1 内容区 max-width
[`SettingsPanel.tsx`](../../../ui/src/components/settings/SettingsPanel.tsx) 右栏的 `max-w-[640px]` 改为 `max-w-[800px]`，让模型表格 / 用量图表 / 长设置行有呼吸空间。

## 8. 密度微调

- `ROW_CLASS` `py-3` → `py-3.5`（多 2px 上下，跟苹果系统设置一致）
- SettingsSection `space-y-3` → `space-y-5`（区块之间多 8px）
- Icon size：nav 用 `size={16}`（现在 15）；行内 icon 统一 14
- 字体保持现状（label `text-sm`，desc `text-sm text-muted-foreground`）—— 不变更字号

## 9. 类型 + Atom 改动

[`ui/src/atoms/settings-tab.ts`](../../../ui/src/atoms/settings-tab.ts)：

```ts
export type SettingsTab =
  | 'connectivity'   // 新（合并 channels + usage）
  | 'intelligence'   // 新（合并 models + agent + prompts）
  | 'tools'          // 复用 id（扩为 tools + permissions + skills）
  | 'general'        // 复用 id（扩为 general + appearance）
  | 'stt'            // 新
  | 'shortcuts'
  | 'pet'
  | 'proxy'
  | 'about'

export const settingsTabAtom = atom<SettingsTab>('connectivity')
```

**注意**：原 type 中的 `channels / models / appearance / usage / agent / prompts / permissions / skills / bots / tutorial` 移除。**编译期必须**确认无其它 .ts 文件硬编码这些字符串值（grep 验证）。

实际上：用户可以通过其他入口跳转到设置某 tab（如「去设置 → 快捷键」link）。Plan 任务里要 grep `setActiveTab`、`settingsTabAtom` 找出所有调用点；非 9 个新 ID 的全部映射到新 ID 或删除。

## 10. 文件结构

新建：
```
ui/src/components/settings/
├── ConnectivityTab.tsx        (Create)  — wrapper for channels + usage
├── IntelligenceTab.tsx        (Create)  — wrapper for models + agent + prompts
├── ToolsTab.tsx               (Create)  — wrapper for tools + permissions + skills
├── GeneralTab.tsx             (Create)  — wrapper for general + appearance
├── SettingsNav.tsx            (Create)  — extracted left nav with groups + search
├── SettingsBreadcrumb.tsx     (Create)  — sticky breadcrumb header
```

修改：
```
ui/src/components/settings/
├── SettingsDialog.tsx         (Modify)  — responsive sizing
├── SettingsPanel.tsx          (Modify)  — use new nav + breadcrumb + new tab routing
├── BotDefaultSettings.tsx     (Delete)  — stub
```

修改 atoms：
```
ui/src/atoms/settings-tab.ts   (Modify)  — type union 改 9 个 id
```

保留不动：
- `<ChannelSettings />`, `<ModelSettings />`, `<AgentSettings />`, `<PromptsSettings />`, `<ToolSettings />`, `<PermissionsSettings />`, `<SkillsSettings />`, `<GeneralSettings />`, `<AppearanceSettings />`, `<UsageSettings />`, `<SttSettings />`, `<ShortcutSettings />`, `<PetSettings />`, `<ProxySetting />`, `<AboutSettings />`

## 11. 测试

- UI 测试基线：463（STT 合并后）→ 目标约 **475**（+12）
  - `SettingsNav.test.tsx` — 4 测试：渲染 9 个 tab、搜索 filter 缩小列表、分组标题渲染、active 状态视觉
  - `SettingsBreadcrumb.test.tsx` — 2 测试：单段 vs 多段、IntersectionObserver mock
  - `ConnectivityTab.test.tsx` — 1 烟雾测试：渲染两个子组件
  - `IntelligenceTab.test.tsx` — 1 烟雾测试：渲染三个子组件
  - `ToolsTab.test.tsx` — 1 烟雾测试
  - `GeneralTab.test.tsx` — 1 烟雾测试
  - 现有 `SttSettings.test.tsx` 应继续通过
  - 现有任何"settings tab"层级的测试都需修改 tab id 引用

- 零 Rust 测试影响（基线 556 不变）

## 12. 风险 & 注意事项

- **跳转链接**：可能存在 `setActiveTab('models')` 之类的调用点。Plan 第一步先 grep。
- **`tutorial` 在原 type 里**但 nav 中没有 — 检查是否真的有跳到 `tutorial` 的代码。若有，决定迁移目标；若无，删除。
- **现有 SttSettings 内 `getShortcutForPlatform('toggle-stt-recording')`** —— 快捷键设置依赖 shortcut-defaults.ts（已注册），新 nav 不影响。
- **IntersectionObserver in jsdom**：测试需 mock（`global.IntersectionObserver = vi.fn()`）。
- **Nav 搜索过滤**：v0 只匹配 tab label（不全文搜索 section 标题，避免复杂度）。
- **Bot stub 删除**：导入清理 + `bots` 从 type 移除。Plan 验证编译期无报错。
- **`Cmd+,` 打开设置 + Esc 关闭**：现有行为保留，不修改。

## 13. 验证清单

- [ ] 9 个 tab 全部可见可点
- [ ] Dialog 在 1440×900 屏幕约 1200×720，无溢出
- [ ] 搜索框输入"模型" → 「智能」高亮，其他半透明
- [ ] 滚动「智能」tab 时 breadcrumb 末段切换（模型分配 → Agent 行为 → 提示词）
- [ ] 「输入（语音）」点入 → SttSettings 完整显示
- [ ] 模型未下载时 nav 上「输入（语音）」有红点
- [ ] 旧的 `setActiveTab('models')` 等调用点全部迁移到新 id（或删除）
- [ ] `tsc --noEmit` 干净
- [ ] `npm test -- --run` 全绿（基线 463 → ~475）

## 14. 实现顺序（提示给 plan 编写者）

1. 类型 + atom 改动 + grep 所有 tab id 引用
2. Dialog 响应式尺寸 + 内容区 max-width
3. SettingsNav（带 group + 搜索）
4. SettingsBreadcrumb（含 IntersectionObserver）
5. ConnectivityTab + IntelligenceTab（俩 wrapper）
6. ToolsTab + GeneralTab（俩 wrapper）
7. 挂 STT + 删 Bot + 改 SettingsPanel routing + 密度微调
8. 整体验证
