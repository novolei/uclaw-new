# WeChat iLink QR 绑定渠道重设计

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 uclaw 的 `wechat_ilink` 渠道从错误的 app_id/api_key 静态表单改造为参照 hello-halo 的 QR 码扫码绑定流程，绑定完成后自动启动 HTTP long-poll 收发消息。

**Architecture:** 后端新增独立的 `ilink_binding.rs` 模块代理 iLink QR API；`IlinkSender` 接入 `status_tx` 基础设施，会话 -14 时发 `NeedsRebind` 状态而非静默退出；前端新增 `WechatIlinkBindingPanel` 状态机组件，嵌入现有 `ImChannelAccordionRow` 的 `wechat_ilink` 展开区。

**Tech Stack:** Rust (reqwest, mockito for tests), React 18 + TypeScript, `qrcode` npm package (canvas rendering, `npm install qrcode @types/qrcode`), Jotai, Tauri invoke IPC

---

## 背景 & 现状分析

### 现有 `ilink.rs` 已经正确的部分

- `bot_token`（来自 credentials）和 `base_url`（来自 config，默认 `https://ilinkai.weixin.qq.com`）
- Long-poll：`POST /ilink/bot/getupdates`，40s 超时，`get_updates_buf` 携带
- Auth headers：`Authorization: Bearer {bot_token}`，`AuthorizationType: ilink_bot_token`，`X-WECHAT-UIN: random-uint32-base64`
- Context token 缓存（`HashMap<user_id, context_token>`）
- 发送：`POST /ilink/bot/sendmessage`，携带 `context_token`
- -14 检测：已有代码，但只是 `break`，没有状态通知

### 需要修复的部分

| 问题 | 修复 |
|---|---|
| `IlinkSender` 没有 `status_tx`，-14 时静默退出 | 加 `status_tx`，emit `NeedsRebind` |
| Manager 没有为 WechatIlink 创建 status relay task | 复用 WecomBot 的 relay 基础设施 |
| 没有 QR 绑定流程（获取/轮询/保存） | 新建 `ilink_binding.rs` + 4 个 Tauri commands |
| 前端表单用 `app_id`/`api_key`（协议不存在的字段） | 删除表单，替换为 `WechatIlinkBindingPanel` |
| `ChannelState` 没有 `NeedsRebind` 变体 | 新增 |

---

## 协议参考（来自 hello-halo 调查）

```
# QR 获取（无需认证）
GET https://ilinkai.weixin.qq.com/ilink/bot/get_bot_qrcode?bot_type=3
→ { qrcode: "xxx" }

# QR 状态轮询（需 header iLink-App-ClientVersion: 1）
GET /ilink/bot/get_qrcode_status?qrcode={qrcode}
→ { status: "wait"|"scaned"|"confirmed"|"expired",
    bot_token?: String, ilink_bot_id?: String }

# 长轮询（需 Bearer auth）
POST /ilink/bot/getupdates
Body: { get_updates_buf: String, base_info: { channel_version: "1.0.2" } }
→ { ret: 0, get_updates_buf: String, msgs: [...] }
   ret = -14 → session expired, must re-bind

# 发送消息（需 Bearer auth）
POST /ilink/bot/sendmessage
Body: { to_user_id, context_token, item_list: [{ type: 1, text_item: { text } }] }
```

---

## 文件结构

| 文件 | 操作 | 职责 |
|---|---|---|
| `src-tauri/src/channels/types.rs` | 修改 | 新增 `ChannelState::NeedsRebind` |
| `src-tauri/src/channels/im/ilink.rs` | 修改 | 加 `status_tx`，emit Online/NeedsRebind/Error |
| `src-tauri/src/channels/im/ilink_binding.rs` | 新建 | `fetch_qr`、`poll_qr_status`、QrStatus 类型 |
| `src-tauri/src/channels/im/mod.rs` | 修改 | 导出 `ilink_binding` |
| `src-tauri/src/channels/manager.rs` | 修改 | WechatIlink 分支加 status_tx + relay task |
| `src-tauri/src/tauri_commands.rs` | 修改 | 4 个新 IPC 命令 |
| `src-tauri/src/main.rs` | 修改 | `invoke_handler!` 注册 4 个新命令 |
| `ui/src/atoms/im-channel-atoms.ts` | 修改 | `ImChannelStatus.state` 新增 `'needs_rebind'` |
| `ui/src/components/settings/WechatIlinkBindingPanel.tsx` | 新建 | QR 状态机 UI 组件 |
| `ui/src/components/settings/ImChannelAccordionRow.tsx` | 修改 | wechat_ilink 展开区替换为 BindingPanel |
| `ui/src/components/settings/WechatIlinkBindingPanel.test.tsx` | 新建 | 4 个状态路径测试 |

---

## 详细设计

### 1. `ChannelState::NeedsRebind`（`types.rs`）

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelState {
    Online,
    Error,
    Offline,
    NeedsRebind,  // iLink session expired (-14); waiting for QR re-bind
}
```

前端对应：`'online' | 'error' | 'offline' | 'needs_rebind'`

### 2. `IlinkSender` 加 `status_tx`（`ilink.rs`）

**构造函数签名变更：**

```rust
pub fn new(
    instance_id: &str,
    config: &Value,
    credentials: &Value,
    status_tx: mpsc::UnboundedSender<ChannelRuntimeStatus>,
) -> Self
```

**`poll_loop` 出口变更：**

```rust
// 连接建立时（第一次成功长轮询前）发 Online
let _ = self.status_tx.send(ChannelRuntimeStatus {
    instance_id: self.instance_id.clone(),
    state: ChannelState::Online,
    last_error: None,
    connected_since_ms: Some(chrono::Utc::now().timestamp_millis()),
    message_count_today: 0,
});

// ret = -14
let _ = self.status_tx.send(ChannelRuntimeStatus {
    instance_id: self.instance_id.clone(),
    state: ChannelState::NeedsRebind,
    last_error: Some("iLink 会话已失效（-14），请重新扫码绑定".to_string()),
    connected_since_ms: None,
    message_count_today: 0,
});
break;

// 网络错误（已有退避逻辑）
let _ = self.status_tx.send(ChannelRuntimeStatus {
    state: ChannelState::Error,
    last_error: Some(e.to_string()),
    ...
});
```

`bot_token` 为空时直接返回，不发任何 status（channel 在 DB 中已启用但尚未绑定，显示为 offline）。

### 3. `ilink_binding.rs`（新建）

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const ILINK_BASE_URL: &str = "https://ilinkai.weixin.qq.com";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QrStatusKind {
    Wait,
    Scaned,
    Confirmed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrStatus {
    pub status: QrStatusKind,
    pub bot_token: Option<String>,
    pub account_id: Option<String>,   // ilink_bot_id from response
}

/// GET /ilink/bot/get_bot_qrcode?bot_type=3 — no auth required
pub async fn fetch_qr(base_url: &str) -> Result<String> { ... }

/// GET /ilink/bot/get_qrcode_status?qrcode={qrcode}
/// Header: iLink-App-ClientVersion: 1
pub async fn poll_qr_status(base_url: &str, qrcode: &str) -> Result<QrStatus> { ... }
```

### 4. Manager WechatIlink 分支（`manager.rs`）

```rust
ImChannelType::WechatIlink => {
    let (status_tx, status_rx) = mpsc::unbounded_channel::<ChannelRuntimeStatus>();
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();

    let ilink = Arc::new(IlinkSender::new(
        &config.id,
        &config.config,
        &config.credentials,
        status_tx,  // 新增
    ));
    let abort = if config.enabled { Some(ilink.clone().start(inbound_tx)) } else { None };
    let fanout_abort = if config.enabled {
        Some(self.spawn_fanout_loop(config.id.clone(), inbound_rx))
    } else { drop(inbound_rx); None };

    // 新增：relay status 到 Tauri event
    let status_relay = self.spawn_status_relay(status_rx);

    (Arc::new(IlinkNoopSender), abort, fanout_abort, Some(status_relay))
}
```

### 5. 4 个 Tauri Commands（`tauri_commands.rs`）

```rust
#[tauri::command]
pub async fn request_wechat_ilink_qrcode(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<serde_json::Value, String> {
    // 从 manager 获取 instance config → 读 base_url
    // 调用 ilink_binding::fetch_qr(base_url) → 返回 { qrcode }
}

#[tauri::command]
pub async fn poll_wechat_ilink_qrcode_status(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    qrcode: String,
) -> Result<serde_json::Value, String> {
    // 获取 base_url → 调用 ilink_binding::poll_qr_status → 返回 QrStatus
}

#[tauri::command]
pub async fn save_wechat_ilink_token(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    bot_token: String,
    account_id: String,
) -> Result<(), String> {
    // UPDATE im_channel_instances SET credentials_json = '{"bot_token":...,"account_id":...}' WHERE id = ?
    // manager.stop_instance(&instance_id) → manager.start_instance(updated_config)
}

#[tauri::command]
pub async fn disconnect_wechat_ilink(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), String> {
    // UPDATE credentials_json = '{}' WHERE id = ?
    // manager.stop_instance → manager.start_instance(cleared credentials)
}
```

### 6. `WechatIlinkBindingPanel.tsx`

**Props：**
```typescript
interface Props {
  instanceId: string
  baseUrl?: string           // from channel.config.base_url
  accountId?: string         // from channel.credentials.account_id (已绑定时)
  status: ImChannelStatus | undefined
  onSaved: () => void
  onDisconnect: () => void
}
```

**内部状态机：**
```typescript
type BindState =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'qr-shown'; qrcode: string }
  | { kind: 'scanning'; qrcode: string }
  | { kind: 'confirmed' }
  | { kind: 'qr-expired' }
  | { kind: 'error'; message: string }
```

**关键行为：**
- `useEffect([status?.state])` — `needs_rebind` 时自动调用 `fetchQr()`
- `fetchQr()` → `invoke('request_wechat_ilink_qrcode', { instanceId })` → 切 `qr-shown`，`startPolling()`
- `startPolling()` → `setInterval(2000)` 调 `poll_wechat_ilink_qrcode_status` → `scaned` 切 `scanning`，`confirmed` 调 `saveToken()`，`expired` 切 `qr-expired` 并 `clearInterval`
- `saveToken({ botToken, accountId })` → `invoke('save_wechat_ilink_token')` → `onSaved()`
- QR canvas 用 `qrcode` npm 包（`QRCode.toCanvas(canvasRef.current, qrcode, { width: 128 })`）
- `clearInterval` 在 `useEffect` cleanup 和所有终止状态中调用

**已绑定状态**：若 `accountId` 非空（credentials 中有 account_id），无论 status 是否已加载，都直接初始化为 `confirmed` 视图，不触发 QR 流程。QR 流程仅在 `accountId` 为空或 `status.state === 'needs_rebind'` 时触发。

### 7. `ImChannelAccordionRow.tsx` 修改

**关闭行 meta line（`wechat_ilink` 分支）：**
```typescript
case 'wechat_ilink': {
  const accountId = (channel?.credentials?.account_id as string) ?? ''
  return accountId ? `账号: ${accountId.slice(0, 16)}` : '未绑定'
}
```

**状态点颜色新增 `needs_rebind` → amber：**
```typescript
const dotColor =
  state === 'online' ? 'bg-green-500' :
  state === 'needs_rebind' ? 'bg-amber-400' :
  state === 'error' ? 'bg-red-500' :
  'bg-gray-400'
```

**展开区 `wechat_ilink` 替换：**
```tsx
{ch.channelType === 'wechat_ilink' ? (
  <WechatIlinkBindingPanel
    instanceId={ch.id}
    baseUrl={ch.config.base_url as string | undefined}
    accountId={ch.credentials?.account_id as string | undefined}
    status={status}
    onSaved={onSaved}
    onDisconnect={onSaved}
  />
) : (
  /* 现有 2 列表单 */
)}
```

删除：`appId`、`apiKey` state、`wechat_ilink` 的 `buildInput()` 分支（`save_wechat_ilink_token` 命令完全接管写入）。

**新建渠道流程**（`channel === undefined` 且 `newChannelType === 'wechat_ilink'`）：

`WechatIlinkBindingPanel` 只接受已有 `instanceId`，因此 ImChannelAccordionRow 在 `wechat_ilink` 新建模式下展示一个精简表单：只有"名称"输入框 + "创建实例"按钮（不显示 app_id/api_key）。用户点击"创建实例"后调用 `create_im_channel`（`config = {}`，`credentials = {}`），成功后父组件 `onSaved()` 刷新渠道列表，同时保持 accordion 展开状态，新实例行切换到编辑模式（`channel` 已存在），此时展开区自动渲染 `WechatIlinkBindingPanel`（`accountId` 为空 → `idle` 状态）。

若用户点"取消"不创建，`setAddingToType(null)` 即可，无孤儿行。

---

## 错误处理

| 场景 | 行为 |
|---|---|
| `fetch_qr` 网络失败 / 非 200 | 面板切 `error` 状态，显示错误文本，保留"重试"按钮回到 `idle` |
| 轮询超过 120s 仍无 confirmed | 切 `qr-expired` |
| `save_wechat_ilink_token` 失败 | toast 错误，面板回 `qr-shown` 继续显示 QR |
| iLink 运行中 -14 | emit `NeedsRebind` → 若面板已展开自动触发 `fetchQr()` |
| 断开后 `bot_token` 为空 | `poll_loop` 检测到空 token 直接 `return`，不发 status（channel 显示 offline） |

---

## 测试覆盖

**Rust 单元测试（`ilink_binding.rs`）：**
- `fetch_qr` 用 mockito mock `GET /ilink/bot/get_bot_qrcode?bot_type=3` 返回 `{ qrcode: "test_qr" }`，断言返回值
- `poll_qr_status` 分别 mock `wait`、`confirmed`（含 `bot_token`/`ilink_bot_id`）、`expired` 三种响应，断言 `QrStatus` 字段

**Rust 单元测试（`ilink.rs`）：**
- `-14` 时 `status_tx` 收到 `ChannelState::NeedsRebind`

**Vitest 测试（`WechatIlinkBindingPanel.test.tsx`）：**

| 测试 | 验证点 |
|---|---|
| idle 渲染 | 显示"获取二维码"按钮，不显示 canvas |
| qr-shown 渲染 | `request_wechat_ilink_qrcode` 被调用，canvas 存在，显示"微信扫码"文字 |
| scanning 渲染 | 轮询返回 `scaned` 后，界面切换至"已扫码，等待确认" |
| confirmed 保存 | 轮询返回 `confirmed` 后调用 `save_wechat_ilink_token`，`onSaved` 被调用 |

**Vitest 测试（`ImChannelAccordionRow.test.tsx` 追加）：**
- `wechat_ilink` 类型展开后不渲染 `app_id` 输入框

---

## 数据库影响

无新 migration 需要。`im_channel_instances.credentials_json` 已是 JSON blob，直接存 `{ "bot_token": "...", "account_id": "..." }`。  
`im_channel_instances.config_json` 存 `{ "base_url": "..." }`（可选覆盖，通常为空对象）。

已有 `wechat_ilink` 实例升级路径：`app_id`/`api_key` 在后端读取时会得到空字符串（`bot_token` 字段不存在），`poll_loop` 检测到 `bot_token` 为空直接返回，实例显示 offline，用户扫码重绑即可。无需数据迁移。
