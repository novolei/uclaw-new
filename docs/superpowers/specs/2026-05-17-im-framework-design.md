# IM Framework Design — hello-halo 完整移植

## Overview

将 hello-halo 的 IM 渠道框架完整移植到 uClaw。每个 IM 渠道实例绑定一个 Space，同时承担两种职责：（1）automation 通知与触发；（2）远程用户与 Agent 的实时双向对话。支持 6 种渠道：WeCom Bot（WebSocket 双向）、iLink 微信个人（HTTP long-poll 双向）、Email（SMTP 单向）、DingTalk（单向）、飞书（单向）、Webhook（单向）。

---

## Architecture

```
src-tauri/src/channels/
├── mod.rs               # re-exports + 模块入口
├── types.rs             # 所有核心类型（见 Type System 节）
├── manager.rs           # ImChannelManager：hot-reload，start/stop instances
├── session_registry.rs  # ImSessionRegistry：per-user session 持久化映射
├── dispatcher.rs        # dispatch_inbound()：权限检查 → 内容匹配路由
├── im/
│   ├── wecom.rs         # WeCom Bot WebSocket 双向
│   └── ilink.rs         # iLink 微信个人 HTTP long-poll 双向
└── notify/
    ├── email.rs         # SMTP 单向
    ├── dingtalk.rs      # 钉钉 outbound
    ├── feishu.rs        # 飞书 outbound
    └── webhook.rs       # Webhook（现有 WebhookSender 移入）

src-tauri/src/agent/
└── headless.rs          # HeadlessDelegate（从 AutomationDelegate 重构抽取）
```

**绑定模型**：每个 `im_channel_instances` 记录 → 一个 `space_id`（FK → spaces）。入站消息通过 `space_id` 找到 Space 上下文后做内容路由。

**双模式路由**（对标 hello-halo `dispatch-inbound.ts`）：
1. 权限检查（owners 白名单 + guestPolicy）
2. 消息内容以 spec 的 `trigger_phrase` 开头 AND 该 spec 在 `spec_channel_bindings` 中启用了此 channel → automation 路径
3. 未命中 → agent 对话路径（ImSession 长期 per-user session）

---

## Data Model（V27 迁移）

```sql
-- IM 渠道实例：每条记录对应一个配置好的渠道实例
CREATE TABLE IF NOT EXISTS im_channel_instances (
    id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,            -- FK → spaces(id)
    channel_type TEXT NOT NULL,        -- wecom_bot | wechat_ilink | email | dingtalk | feishu | webhook
    name TEXT NOT NULL,
    config_json TEXT NOT NULL,         -- 非敏感配置（endpoint、bot_id 等）
    enabled INTEGER NOT NULL DEFAULT 1,
    streaming INTEGER NOT NULL DEFAULT 0,
    reply_scope TEXT NOT NULL DEFAULT 'all',
    permission_enabled INTEGER NOT NULL DEFAULT 0,
    owners_json TEXT NOT NULL DEFAULT '[]',      -- chat_id 白名单 JSON 数组
    guest_policy_json TEXT NOT NULL DEFAULT '{}', -- { tool_allowlist: [], mcp_enabled: bool }
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- per-user 长期 agent session 映射（跨重启持久化）
CREATE TABLE IF NOT EXISTS im_sessions (
    id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    channel_type TEXT NOT NULL,
    chat_id TEXT NOT NULL,             -- 用户唯一标识（微信 openid / 企微 userid 等）
    agent_session_id TEXT NOT NULL,    -- FK → agent_sessions(id)
    created_at TEXT NOT NULL,
    last_active_at TEXT NOT NULL,
    UNIQUE(space_id, channel_type, chat_id)
);

-- spec 与 channel 实例的多对多绑定
CREATE TABLE IF NOT EXISTS spec_channel_bindings (
    spec_id TEXT NOT NULL,
    channel_instance_id TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (spec_id, channel_instance_id)
);

-- automation_specs 新增列
ALTER TABLE automation_specs ADD COLUMN trigger_phrase TEXT;  -- IM 触发前缀（如 /daily-report）
ALTER TABLE automation_specs ADD COLUMN system_prompt TEXT;   -- 覆盖 Space 级默认 prompt
ALTER TABLE automation_specs ADD COLUMN description TEXT;     -- 显示描述
```

敏感凭证（apiKey、secret、password）不存入 `config_json`，通过 `secrets::store(channel_instance_id, credential_json)` 加密存储，与 LLM provider key 机制一致。

---

## Type System（`channels/types.rs`）

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImChannelType {
    WecomBot,
    WechatIlink,
    Email,
    Dingtalk,
    Feishu,
    Webhook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImChannelInstanceConfig {
    pub id: String,
    pub space_id: String,
    pub channel_type: ImChannelType,
    pub name: String,
    pub config: serde_json::Value,   // 非敏感配置
    pub enabled: bool,
    pub streaming: bool,
    pub reply_scope: String,
    pub permission_enabled: bool,
    pub owners: Vec<String>,
    pub guest_policy: GuestPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuestPolicy {
    pub tool_allowlist: Vec<String>,
    pub mcp_enabled: bool,
}

// 所有渠道统一入站消息格式
#[derive(Debug, Clone)]
pub struct InboundMessage {
    pub instance_id: String,
    pub chat_id: String,
    pub sender_name: Option<String>,
    pub text: String,
    pub timestamp: i64,
    pub channel_ctx: Option<serde_json::Value>, // iLink: {context_token}; WeCom: {req_id, expires_at}
}

// 回复句柄（对标 hello-halo ReplyHandle）
#[derive(Clone)]
pub struct ReplyHandle {
    pub sender: Arc<dyn ChannelSender + Send + Sync>,
    pub channel_ctx: Option<serde_json::Value>,
    pub chat_id: String,
}

impl ReplyHandle {
    pub async fn send(&self, text: &str) -> Result<()>;
    pub async fn send_markdown(&self, text: &str) -> Result<()>;
}

// WeCom 流式句柄
pub struct StreamingHandle {
    pub wecom_sender: Arc<WecomSender>,
    pub chat_id: String,
    pub req_id: String,
    pub req_id_expires_at: DateTime<Utc>,
}

impl StreamingHandle {
    pub async fn update(&self, partial: &str) -> Result<()>;
    pub async fn finish(&self, final_text: &str) -> Result<()>;
}

// 统一 outbound trait
#[async_trait]
pub trait ChannelSender: Send + Sync {
    async fn send_text(&self, chat_id: &str, text: &str, ctx: Option<&serde_json::Value>) -> Result<()>;
    fn supports_streaming(&self) -> bool { false }
}
```

---

## ImChannelManager（`channels/manager.rs`）

对标 hello-halo `ImChannelManager`。注册为 `ServiceManager` Stage 3 服务。

```rust
pub struct ImChannelManager {
    instances: Arc<RwLock<HashMap<String, RunningInstance>>>,
    db: Arc<DbPool>,
    secrets: Arc<SecretsStore>,
    app_handle: tauri::AppHandle,
}

struct RunningInstance {
    config: ImChannelInstanceConfig,
    sender: Arc<dyn ChannelSender + Send + Sync>,
    abort_handle: Option<AbortHandle>,  // inbound poll task（WeCom / iLink 专用）
}
```

**关键方法**：
- `start_all()` — 启动时从 DB 加载所有 `enabled = 1` 实例
- `apply_config(new_configs)` — hot-reload：diff 新旧列表，stop 已删除，start 新增，restart 已变更
- `start_instance(config)` — 从 `secrets` 加载凭证，构建 `sender`，双向渠道额外启动 inbound tokio task
- `stop_instance(id)` — abort inbound task（如有）
- `send_to_channel(instance_id, chat_id, text)` — 供 `notify_user` 工具和 automation 通知调用

---

## Channel Implementations

### WeCom Bot（`channels/im/wecom.rs`）

- `start()` — 建立 WebSocket 连接到企微 API，在专用 tokio task 中持续接收消息
- 消息进来 → `channel_ctx = { req_id, expires_at: now + 5min }`
- `send_text()` — `req_id_expires_at > now` → reply API；否则 → `appchat/send` 主动推送降级
- `supports_streaming() → true` — `StreamingHandle.update()` 调用企微消息更新接口
- 重连策略 — 指数退避，上限 30 秒

### iLink 微信个人（`channels/im/ilink.rs`）

- `start()` — 专用 tokio task 跑 HTTP long-poll 循环（35 秒超时，完成立即重新请求）
- 消息进来 → `channel_ctx = { context_token }` （per-message，不跨消息复用）
- `send_text()` — POST 到 iLink reply endpoint，携带 `context_token`（从 `ctx` 取）
- `supports_streaming() → false` — 等 agent 完整输出后单次发送
- endpoint: `https://ilinkai.weixin.qq.com`；重连：网络错误指数退避

### Email（`channels/notify/email.rs`）

- `lettre` crate，SMTP with TLS
- `config`: smtp_host, port, from_address, to_addresses（JSON 数组）
- secret: password

### DingTalk（`channels/notify/dingtalk.rs`）

- HTTP POST Webhook；可选 HMAC-SHA256 加签（timestamp + signing_secret）
- `config`: webhook_url；secret: signing_secret（可选）

### 飞书（`channels/notify/feishu.rs`）

- HTTP POST Webhook；可选加签（同 DingTalk）
- `config`: webhook_url；secret: signing_secret（可选）

### Webhook（`channels/notify/webhook.rs`）

- 通用 HTTP POST，自定义 headers
- 现有 `WebhookSender` 移入此处，接口不变

---

## Inbound Dispatcher（`channels/dispatcher.rs`）

对标 hello-halo `dispatch-inbound.ts`：

```rust
pub async fn dispatch_inbound(
    msg: InboundMessage,
    instance: &ImChannelInstanceConfig,
    session_registry: Arc<ImSessionRegistry>,
    db: Arc<DbPool>,
    app_handle: tauri::AppHandle,
) -> Result<()> {
    // 1. 权限检查
    if instance.permission_enabled && !instance.owners.contains(&msg.chat_id) {
        build_reply_handle(&msg, instance, &app_handle)?
            .send("您没有权限使用此服务。").await?;
        return Ok(());
    }

    // 2. 构建 ReplyHandle（iLink: ctx 含 context_token；WeCom: ctx 含 req_id）
    let reply = build_reply_handle(&msg, instance, &app_handle)?;

    // 3. 内容匹配：trigger_phrase 前缀 + spec_channel_bindings 启用检查
    let matched_spec = find_matching_spec(msg.text.trim(), &instance.space_id, &instance.id, &db).await?;

    match matched_spec {
        Some(spec) => {
            // automation 路径：HeadlessDelegate + ReplyHandle 注入
            run_automation_via_im(spec, msg, reply, &app_handle).await?;
        }
        None => {
            // agent 对话路径：ImSession 长期 session
            run_agent_chat_via_im(msg, reply, instance, session_registry, &app_handle).await?;
        }
    }
    Ok(())
}
```

**`find_matching_spec`**：查 `automation_specs` WHERE `space_id = instance.space_id AND trigger_phrase IS NOT NULL`，JOIN `spec_channel_bindings` WHERE `channel_instance_id = instance.id AND enabled = 1`，过滤 `msg.text.trim().starts_with(spec.trigger_phrase)`，取第一个命中。

---

## ImSessionRegistry（`channels/session_registry.rs`）

```rust
pub struct ImSessionRegistry {
    cache: Arc<RwLock<HashMap<(String, String, String), String>>>,
    // key: (space_id, channel_type, chat_id) → agent_session_id
    db: Arc<DbPool>,
}

impl ImSessionRegistry {
    // 启动时从 im_sessions 全量加载缓存
    pub async fn load_from_db(&self) -> Result<()>;

    // 查找或新建 agent_session，同步写 im_sessions 和更新缓存
    pub async fn get_or_create_session(
        &self,
        space_id: &str,
        channel_type: &str,
        chat_id: &str,
        sender_name: Option<&str>,
    ) -> Result<String>; // → agent_session_id

    // 更新 last_active_at（每条入站消息后调用）
    pub async fn touch(&self, space_id: &str, channel_type: &str, chat_id: &str) -> Result<()>;
}
```

新建 `agent_session` 时：`title = "{sender_name} via {channel_type}"`，`origin = "im:{channel_type}:{chat_id}"`。

---

## HeadlessDelegate（`agent/headless.rs`）

从 2a 的 `AutomationDelegate` 重构抽取，统一服务 automation 和 IM 两种 headless 场景：

```rust
pub struct HeadlessDelegate {
    pub session_id: String,
    pub space_id: Option<String>,
    pub origin: String,
    // IM 触发时携带，notify_user 工具优先用此句柄回发
    pub reply_handle: Option<Arc<ReplyHandle>>,
    // WeCom 专用流式句柄
    pub streaming_handle: Option<Arc<StreamingHandle>>,
    // 覆盖 Space 默认 prompt（来自 spec.system_prompt）
    pub system_prompt_override: Option<String>,
}
```

**IM agent 对话路径**（`run_agent_chat_via_im`）：
1. `ImSessionRegistry.get_or_create_session()` → `agent_session_id`
2. 构建 `HeadlessDelegate`（`reply_handle` + `streaming_handle` 注入）
3. 向 session 注入用户消息（`inject_user_message`，写入 `agent_messages`）
4. 调用 `run_agentic_loop(delegate)`
5. loop 结束后，从 `agent_messages` 取最新一条 `role = 'assistant'` 的消息文本
6. 非流式渠道：`reply_handle.send(final_text)`；WeCom 流式：在 loop 期间 `streaming_handle.update(partial)` 逐批调用，loop 结束时 `streaming_handle.finish(final_text)`

**WeCom 流式拦截**：`HeadlessDelegate` 持有 `streaming_handle`，agent loop 每次 `emit_token` IPC 事件时，同步调用 `streaming_handle.update(token_batch)`；loop 完成时调用 `streaming_handle.finish()`。非 WeCom 渠道忽略 streaming_handle，等待 loop 完成后读 DB 取最终文本。

**IM automation 路径**（`run_automation_via_im`）：
与 2a `execute_run` 一致，额外传入 `reply_handle`，使 `notify_user` 工具能回到 IM 渠道。

---

## `notify_user` 工具升级

```rust
// 执行 notify_user 时检查 HeadlessDelegate
if let Some(delegate) = ctx.headless_delegate.as_ref() {
    if let Some(reply) = delegate.reply_handle.as_ref() {
        // IM 触发路径：回到原始渠道，形成完整闭环
        reply.send_markdown(&report_text).await?;
        return Ok(success_result());
    }
}
// 非 IM 触发（定时任务、手动触发等）：走现有 ChannelManager 通知路径
channel_manager.dispatch(notification).await?;
```

---

## 流式回复策略

| 渠道 | streaming 支持 | 默认行为 |
|---|---|---|
| WeCom Bot | ✅ 真流式 | `StreamingHandle.update()` 逐批发送 |
| iLink | ❌ | 等 agent 完整输出后单次 `ReplyHandle.send()` |
| Email / DingTalk / 飞书 / Webhook | ❌ | 等 agent 完整输出后单次发送 |

实例配置 `streaming: bool` 可覆盖渠道默认行为（如强制关闭 WeCom 流式）。

---

## Tauri Commands（`tauri_commands.rs`）

```rust
list_im_channels() → Vec<ImChannelInstanceConfig>
create_im_channel(config: ImChannelInstanceConfig, secret: String) → String  // → id
update_im_channel(id: String, config: ImChannelInstanceConfig, secret: Option<String>)
delete_im_channel(id: String)
toggle_im_channel(id: String, enabled: bool)

list_spec_channel_bindings(spec_id: String) → Vec<SpecChannelBinding>
update_spec_channel_bindings(spec_id: String, bindings: Vec<SpecChannelBinding>)
```

全部在 `main.rs` 的 `invoke_handler!` 宏中注册。`ImChannelManager` 在 `main.rs` Stage 3 注册为 `ServiceManager` 服务，Stage 4 `start_all()`。

---

## Frontend UI

### 入口 A — 全局设置「IM 渠道」面板（`ImChannelsSettings.tsx`）

- 渠道实例列表：类型图标 + 名称 + Space 标签 + enabled 开关 + 编辑 / 删除
- 「+ 新增渠道」按钮
- `ImChannelForm.tsx`：通用字段（名称、类型下拉、Space 下拉、streaming 开关）+ 权限区（permission_enabled → owners 输入 + guest_policy JSON 编辑器）+ 渠道专用凭证区（按 `channel_type` 条件渲染）：
  - **WeCom Bot**：Corp ID、Agent ID、Corp Secret（密码框）
  - **iLink**：App ID、API Key（密码框）
  - **Email**：SMTP Host、端口、用户名、密码、收件人列表
  - **DingTalk / 飞书**：Webhook URL、签名密钥（可选）
  - **Webhook**：URL、自定义 Headers（JSON 编辑器）

### 入口 B — Automation Spec 配置页新增区块（`AutomationSpecEditor.tsx`）

「**消息通道**」区块（对标 hello-halo 截图）：
- 列出所有已配置渠道实例（绿点 = 已启用；灰点 = 未启用）
- 每条渠道独立开关（写入 `spec_channel_bindings`）
- 底部「在设置中配置渠道 ↗」跳转链接
- 副标题："AI驱动：数字人决定何时以及通过配置的渠道通知什么内容。"

「**IM 触发**」字段：
- `trigger_phrase` 输入框，placeholder：`/daily-report`
- 说明："IM 消息以此关键词开头时触发本 automation"

「**开发者**」区块：
- 名称（`automation_specs.name`，已有）
- 描述（`automation_specs.description`，新增）
- 系统提示词（`automation_specs.system_prompt`，新增，多行文本框）——覆盖 Space 级默认 prompt

---

## 分期说明

本 spec 为单一完整实现，不分期。所有 6 个渠道、框架、数据库迁移、frontend UI 在同一 PR 内完成，任务粒度由 writing-plans 拆分为可独立提交的 tasks。

---

## 参考

- hello-halo 上游：`/Users/ryanliu/Documents/hello-halo/src/main/`
  - `im-channels/`: wecom-bot, weixin-ilink-bot
  - `notify-channels/`: email, dingtalk, feishu, webhook
  - `dispatch-inbound.ts`, `im-session-registry.ts`, `im-permission-registry.ts`
- uClaw 现有：`src-tauri/src/channels.rs`（现有 WebhookSender + ChannelManager，本 spec 重构并扩展）
- Phase 2a：`src-tauri/src/agent/session.rs`（AutomationDelegate 重构为 HeadlessDelegate）
- V24 迁移：`src-tauri/src/db/migrations.rs`（agent_sessions.archived_at，origin 字段 precedent）
