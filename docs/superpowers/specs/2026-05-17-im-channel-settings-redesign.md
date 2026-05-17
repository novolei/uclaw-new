# IM 渠道设置 UI 重设计 — Design Spec

> 参考设计：Proma「远程连接」UI 范式，全面翻新 `ImChannelsSettings` 体验

**日期：** 2026-05-17  
**状态：** 待实施  
**关联 PR：** feat+im-framework（PR #178）

---

## 1. 背景与目标

现有 `ImChannelsSettings.tsx` + `ImChannelForm.tsx` 采用「列表 + 全页换屏表单」模式：点击「编辑」后整个面板被表单替换，没有连接状态可见性，增删改操作步骤分散。

重设计目标：
1. **渠道类型 Tab 导航**：多类型渠道按类型分 Tab，Tab 上显示该类的实例数量徽章；
2. **手风琴行内展开**：点击实例行直接原地展开编辑表单，无需切换页面；
3. **连接感知**：展开区顶部显示运行时连接状态（已连接/错误/离线），错误原因一眼可见；
4. **行内开关**：启用/停用不需要展开，toggle 即生效；
5. **错误提前暴露**：折叠状态就能看到「认证失败」徽章，不必逐一展开检查；
6. **保存即重连**：修改凭证后「保存并重连」一键完成，不需要额外操作。

---

## 2. 整体架构

### 2.1 文件拆分

| 文件 | 职责 | 操作 |
|------|------|------|
| `ui/src/components/settings/ImChannelsSettings.tsx` | 顶层容器：Tab 导航 + 实例列表 | 重写 |
| `ui/src/components/settings/ImChannelAccordionRow.tsx` | 单个实例行：收起态 + 展开态（手风琴） | 新建 |
| `ui/src/components/settings/ImChannelForm.tsx` | 渠道类型特定字段（WeCom/iLink/Email…） | 保留，内部复用 |
| `ui/src/atoms/im-channel-atoms.ts` | 添加 `ImChannelStatus` 类型 + `imChannelStatusesAtom` | 修改 |
| `src-tauri/src/channels/manager.rs` | 添加 `ChannelRuntimeStatus` 状态追踪 + 事件发射 | 修改 |
| `src-tauri/src/tauri_commands.rs` | 添加 `get_im_channel_statuses` 命令 | 修改 |
| `src-tauri/src/main.rs` | 注册新命令 | 修改 |

`ImChannelForm.tsx`（渠道字段组件）保持现有接口不变，被 `ImChannelAccordionRow` 内联复用，**不删除**。

---

## 3. 数据模型

### 3.1 后端：`ChannelRuntimeStatus`

在 `src-tauri/src/channels/manager.rs` 新增：

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChannelRuntimeStatus {
    pub instance_id: String,
    pub state: ChannelState,
    pub last_error: Option<String>,
    pub connected_since_ms: Option<i64>,   // epoch ms，online 时有值
    pub message_count_today: u32,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelState {
    Online,   // WS 已连接 / iLink 轮询正常
    Error,    // 认证失败、握手超时等
    Offline,  // enabled=false 或未运行
}
```

`ImChannelManager` 新增：
```rust
pub statuses: Arc<RwLock<HashMap<String, ChannelRuntimeStatus>>>,
```

WecomBot 的 `connection_loop` 在状态变化时调用辅助函数：
```rust
manager.update_status(instance_id, ChannelRuntimeStatus { state: Online, .. });
// 后紧跟：
app_handle.emit("im_channel_status_changed", &status).ok();
```

### 3.2 新 Tauri 命令：`get_im_channel_statuses`

```rust
#[tauri::command]
pub async fn get_im_channel_statuses(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ChannelRuntimeStatus>, Error> {
    let statuses = state.im_channel_manager.statuses.read().await;
    Ok(statuses.values().cloned().collect())
}
```

注册到 `main.rs` 的 `invoke_handler!` 宏。

### 3.3 前端类型扩展

`ui/src/atoms/im-channel-atoms.ts` 新增：

```typescript
export interface ImChannelStatus {
  instanceId: string
  state: 'online' | 'error' | 'offline'
  lastError?: string
  connectedSinceMs?: number
  messageCountToday?: number
}

// 按 instanceId 索引的状态 map
export const imChannelStatusesAtom = atom<Record<string, ImChannelStatus>>({})

// 拉取一次全量状态
export const fetchImChannelStatusesAtom = atom(null, async (_get, set) => {
  const statuses = await invoke<ImChannelStatus[]>('get_im_channel_statuses')
  const map: Record<string, ImChannelStatus> = {}
  for (const s of statuses) map[s.instanceId] = s
  set(imChannelStatusesAtom, map)
})
```

IPC 实时订阅（挂在 `ImChannelsSettings` 的 `useEffect`）：

```typescript
import { listen } from '@tauri-apps/api/event'

useEffect(() => {
  const unlisten = listen<ImChannelStatus>('im_channel_status_changed', ({ payload }) => {
    setStatuses(prev => ({ ...prev, [payload.instanceId]: payload }))
  })
  return () => { unlisten.then(fn => fn()) }
}, [])
```

---

## 4. 组件设计

### 4.1 `ImChannelsSettings.tsx`（重写）

**职责：** Tab 导航容器，不承载具体行逻辑。

```
┌─────────────────────────────────────────────────────┐
│  [企业微信 ②] [微信 iLink ①] [邮件] [Webhook] [+新增渠道类型]  │
├─────────────────────────────────────────────────────┤
│  企业微信 Bot 通过 WebSocket 长连接收发消息…              │
│                                                     │
│  [实例行 A — 在线，折叠]                               │
│  [实例行 B — 认证失败，展开]                            │
│  [+ 新增企业微信实例（虚线）]                            │
└─────────────────────────────────────────────────────┘
```

关键逻辑：
- `activeTab` state 追踪当前 Tab
- 计算各 Tab 实例数：`channelsByType[type].length`
- 徽章颜色：有 `error` 状态的类型显示 `bg-destructive`，全部在线显示 `bg-success`，无实例不显示徽章
- `+ 新增渠道类型` 占位（Phase 1 不实现，仅显示 disabled 状态）
- `openRowId` state 控制手风琴：只允许一行展开，点击已展开行收起

```typescript
const CHANNEL_DESCRIPTIONS: Record<string, string> = {
  wecom_bot: '企业微信 Bot 通过 WebSocket 长连接收发消息，每个实例对应一个独立的 Corp App。',
  wechat_ilink: '微信 iLink 通过 HTTP 长轮询桥接个人微信账号，需配合 iLink 桥接服务运行。',
  email: '通过 SMTP 发送邮件通知，适用于低频告警场景。',
  dingtalk: '钉钉 Webhook 通知，不支持双向对话。',
  feishu: '飞书 Webhook 通知，不支持双向对话。',
  webhook: '通用 HTTP Webhook，POST JSON 到目标 URL。',
}
```

### 4.2 `ImChannelAccordionRow.tsx`（新建）

**Props：**
```typescript
interface Props {
  channel: ImChannelRow
  status: ImChannelStatus | undefined
  spaces: { id: string; name: string }[]
  open: boolean
  onToggleOpen: () => void
  onToggleEnabled: (enabled: boolean) => void
  onSaved: () => void
  onDeleted: () => void
}
```

**折叠态（closed）：**

```
┌──────────────────────────────────────────────────┐
│ ● 产品组机器人  [工作区]              [toggle] ›  │
│   corp_id: wx12abc · 在线 3h 21m · 今日 47 条消息  │
└──────────────────────────────────────────────────┘
```

状态点颜色：
- `online` → `bg-success` + 脉冲阴影（仅 WS/长轮询类型）
- `error` → `bg-destructive`
- `offline` / `undefined` → `bg-muted-foreground`

错误时，名称右侧追加红色徽章：`认证失败` / `连接超时` / `未知错误`（取 `status.lastError` 的首句摘要）。

元数据行（第二行）规则：
- `online`：`corp_id: {prefix} · 在线 {duration} · 今日 {count} 条消息`
- `error`：`corp_id: {prefix} · {lastError的简短描述}`
- `offline`：`corp_id: {prefix} · 已停用` 或 `已停止`
- 若 `status` 为 undefined（channel 类型为 notify-only 如 webhook）：`url: {config.url}`

Inline toggle 逻辑：
- 使用 CSS toggle switch（`div` + `position:absolute` thumb），不用 `<input type="checkbox">`
- 点击事件调用 `onToggleEnabled`，optimistic update 在父组件（`ImChannelsSettings`）处理
- toggle click 不触发手风琴展开

**展开态（open）：**

展开区上方首先渲染「连接状态块」：

```
┌─ 状态块 ──────────────────────────────────────┐
│  ● WebSocket 认证失败                 [重连]  │  ← error 态
│    corp_secret 已过期或被重置。请更新凭证。      │
└────────────────────────────────────────────────┘
```

```
┌─ 状态块 ──────────────────────────────────────┐
│  ● WebSocket 已连接                  [断开]  │  ← online 态
│    在线 3h 21m · 今日 47 条消息               │
└────────────────────────────────────────────────┘
```

状态块样式：
- `online` → `bg-success/10` + `border-success/30` + `text-success`
- `error` → `bg-destructive/10` + `border-destructive/30` + `text-destructive`
- `offline` / undefined → `bg-muted` + `text-muted-foreground`，文字「未连接」

状态块右侧按钮：
- `online` 态：「停用」按钮 → 调用 `toggle_im_channel(id, false)`（持久化 enabled=false，行内 toggle 也同步更新）
- `error` 态：「重连」按钮 → 调用 `update_im_channel(id, currentInput)`（以当前字段原值触发 restart，不修改任何数据）
- `offline` 态：「启用」按钮 → 调用 `toggle_im_channel(id, true)`

状态块之后渲染凭证字段（两列网格）。错误态时，`lastError` 关联的字段（如 corp_secret）border 高亮为 `border-destructive`，label 追加 `*` 红色标记。

**底部操作行：**

```
[删除实例（text，destructive）]   [取消]  [保存 / 保存并重连]
```

按钮文字规则（`dirty` = 任何字段发生变化）：
- `dirty` = false → 「保存」disabled
- `dirty` = true + `status?.state === 'online'` → 「保存并重连」（因为 `update_im_channel` 内部总会 restart）
- `dirty` = true + 其他状态 → 「保存」

注意：`update_im_channel` 总是在保存后调用 `restart_instance_by_id`，无论字段是否实质改变。所以即使只修改了名称也会触发重连——这是预期行为，不是 bug。

「删除实例」使用 `confirm()` 二次确认，确认后调用 `delete_im_channel`。

**脏状态追踪：**

展开时，`ImChannelAccordionRow` 从 `channel` 派生初始字段值。任何字段变化置 `dirty=true`。  
「取消」还原所有字段到初始值，`dirty=false`。  
「保存（并重连）」成功后调 `onSaved()`，父组件重新拉取列表 + 状态。

### 4.3 字段布局（两列网格）

各渠道类型的字段展示在两列网格中，顺序和分组沿用 `ImChannelForm.tsx` 的现有字段集，仅将布局从 stacked 改为 `grid grid-cols-2 gap-x-3 gap-y-2`：

**企业微信 Bot（wecom_bot）：**
- 列1: Corp ID（只读显示，前缀截断）
- 列2: Agent ID（只读显示）
- 列1+2 span: Corp Secret（密码输入，error 态边框高亮）
- 列1: 绑定 Space（select）
- 列2: WebSocket URL（可选，高级）

**微信 iLink（wechat_ilink）：**
- 列1: App ID
- 列2: API Key（密码输入）
- 列1+2: 绑定 Space

**Email：**
- 列1: SMTP Host
- 列2: 端口
- 列1: 用户名
- 列2: 密码（密码输入）
- 列1+2: 收件人（逗号分隔）
- 列1+2: 绑定 Space

**Webhook / 钉钉 / 飞书：**
- 列1+2: URL
- 列1+2: 签名密钥（可选）
- 列1+2: 绑定 Space

额外选项行（在字段网格下方，独立一行）：
```
[checkbox] 流式回复    [checkbox] 开启权限控制
```

权限控制展开后追加：
- Owners 白名单输入框
- [checkbox] Guest 允许 MCP 工具

---

## 5. 交互流程

### 5.1 展开/收起

```
用户点击实例行（非 toggle 区域）
  → openRowId === this.id ? openRowId = null : openRowId = this.id
  → CSS max-height 过渡 展开/收起（不使用 layout animation）
```

展开时拉取最新状态：`fetchImChannelStatusesAtom` 在 `ImChannelsSettings` mount 时调用一次，之后靠 IPC 事件实时更新，展开不需要单独拉取。

### 5.2 Toggle 开关

```
用户点击 toggle
  → optimistic: setChannels(prev => prev.map(ch => ch.id === id ? {...ch, enabled} : ch))
  → await invoke('toggle_im_channel', { id, enabled })
  → 成功：无操作（optimistic 已生效）
  → 失败：fetchChannels() 还原 + toast 报错（替换现有 alert）
```

### 5.3 保存

```
用户点击「保存 / 保存并重连」
  → setSaving(true)
  → await invoke('update_im_channel', { id, input })
    （update_im_channel 内部已调用 restart_instance_by_id，无需额外重连命令）
  → 成功：onSaved() → 父组件 fetchChannels() + fetchStatuses()
  → 失败：setError(e)，展示在表单内（替换现有 alert）
```

注意：「重连」按钮（状态块右侧）调用的也是 `update_im_channel`，但传的是当前未修改的 input（仅触发 restart），**不更新任何字段**。

### 5.4 新增实例

点击「+ 新增 {类型} 实例」虚线行：
- 展开一个空白的 `ImChannelAccordionRow`，`channel` 为 undefined 状态（新建模式）
- 新建模式下状态块不显示（无 status）
- 保存调 `create_im_channel`，成功后关闭新建行并刷新列表

---

## 6. 错误处理

| 场景 | 处理 |
|------|------|
| toggle 失败 | toast（替换 alert），还原 optimistic |
| save 失败 | 表单内 `<p className="text-sm text-destructive">` |
| delete 失败 | toast 报错 |
| 后端 status 事件缺失 | status 为 undefined → 显示「状态未知」，灰色点 |
| SSRF 拒绝（后端 400） | save 失败，error 显示后端返回的 URL 字段描述 |

全面替换现有 `alert()` 调用为 `sonner` toast（`import { toast } from 'sonner'`），保持与 uClaw 其他地方的一致性。

---

## 7. 样式约束

- 严格使用 CSS 变量 token（`bg-success`、`text-destructive`、`border-border` 等），禁止硬编码颜色（`bg-green-500`、`text-red-600`）
- 展开区内边距：`px-3 py-2.5`，字段 label `text-xs text-muted-foreground mb-1`
- 手风琴过渡：`overflow-hidden transition-[max-height] duration-200 ease-out`，max-height 从 0 到 `1000px`（足够容纳最长的展开内容）
- Toggle switch 尺寸：`w-8 h-4`（32×16px），thumb `w-3 h-3`，on = `bg-success`，off = `bg-muted`
- 状态点（小圆点）：`w-2 h-2 rounded-full flex-shrink-0`
- online 态脉冲：`animate-pulse`（仅限 WS/长轮询类型，webhook/email 不需要）

---

## 8. 测试要点

| 测试 | 类型 |
|------|------|
| Tab 渲染 + 徽章数量与实例列表一致 | Vitest unit |
| error 状态实例行显示红色徽章 | Vitest unit |
| toggle optimistic 更新 → 失败还原 | Vitest unit |
| 展开行 open 态渲染 status block | Vitest unit |
| dirty 追踪：初始无修改 → 按钮为「保存」disabled；修改后启用 | Vitest unit |
| 新增实例表单提交调 create_im_channel | Vitest unit |
| `get_im_channel_statuses` 返回所有已知状态 | Rust unit |
| `im_channel_status_changed` 事件在 WecomBot 状态变化时发射 | Rust unit |

---

## 9. 不在本次范围内

- **「+新增渠道类型」Tab 实现**：占位显示，不可点击；Webhook / 邮件等类型扩展是独立任务。
- **连接时长精确计时**（`在线 3h 21m`）：后端记录 `connected_since_ms`，前端计算显示；不做实时倒计时，仅 mount 时计算。
- **每日消息数统计**：`message_count_today` 由后端维护（计数器在内存，重启归零）；不做持久化。
- **权限控制 UI 深度改造**：owners / guest policy 字段保持现有逻辑，仅迁入手风琴布局。

---

## 10. 迁移说明

本次重写**向下兼容**：
- 所有后端命令（`list_im_channels`、`create_im_channel`、`update_im_channel`、`delete_im_channel`、`toggle_im_channel`）接口不变
- 新增 `get_im_channel_statuses` 命令、`ChannelRuntimeStatus` 结构、`im_channel_status_changed` 事件
- `ImChannelRow` 类型不变（status 信息单独存储在 `imChannelStatusesAtom`）
- `ImChannelForm.tsx` 字段逻辑代码被 `ImChannelAccordionRow` 内联复用后可逐步废弃，但不在本次删除
