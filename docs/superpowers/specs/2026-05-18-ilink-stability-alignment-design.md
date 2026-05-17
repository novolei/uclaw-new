# WeChat iLink 稳定性重构与 hello-halo 对齐设计

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 WeChat iLink IM 消息流程中已确认的所有 bug，将架构与 hello-halo 参考实现对齐，同时明确群聊不在 iLink 协议支持范围内。

**Architecture:** 提取 `IlinkSharedState`（`Arc` 包裹的共享运行时状态），让 `IlinkSender`（轮询端）和 `IlinkReplySender`（发送端）共享 context_tokens LRU 缓存、session_active 原子标志和 seen_msg_ids 去重缓冲区；同时修复 dispatcher.rs 中三处无关架构的 bug。

**Tech Stack:** Rust (tokio, reqwest, lru crate)，Tauri AppHandle emit，rusqlite

---

## 背景与 hello-halo 差距分析

本 spec 基于对 hello-halo `weixin-ilink.provider.ts` 的完整逆向调查，识别出以下类别的问题：

### 已修复（不在本 spec 范围）
- sendmessage `msg` 包装层及所有必填字段
- `X-WECHAT-UIN` 头
- 消息去重 `seen_msg_ids` VecDeque（cap 200）
- `start_idx` 在 user message push 之前捕获
- `chat:stream-complete` IPC emit
- `TextAction::Return` on first IM text response
- `iLink-App-ClientVersion: 1` header（已在 ilink_binding.rs）
- `ilink_bot_id` → account_id 提取（已正确）

### 本 spec 修复的问题

| 优先级 | 问题 | 位置 | 影响 |
|---|---|---|---|
| Critical | `send_text` 错误静默丢弃 | dispatcher.rs | 用户手机收不到回复，无任何错误日志 |
| Critical | sendmessage -14 不回传 IlinkSender | ilink.rs | 窗口期内 channel 显示 Online 但发送全部失败 |
| Critical | 空消息未过滤 | dispatcher.rs | 空文本触发 agent 空跑 |
| High | context_tokens map 无界增长 | ilink.rs | 长期运行内存泄漏 |
| High | 回复无长度限制 | ilink.rs | 超长回复被 iLink 服务器静默丢弃（hello-halo 限制 4000 字符）|
| High | 多 text block 只取最后一条 | dispatcher.rs | 多工具调用后回复内容不完整 |
| Medium | NeedsRebind 时不清理共享缓存 | ilink.rs | 重新认证后旧 context_tokens 残留 |
| Minor | group_id 消息无防御性过滤 | ilink.rs | 将来 iLink 万一推送群事件会误触发 agent |

### 群聊结论（明确不做）

**个人微信 iLink Bot 在协议层面不支持群聊：**
- Bot 身份为 `xxx@im.bot`，无法被邀请进群
- `getupdates` 在群聊场景下 `msgs` 数组永远为空（Hermes issue #17094 实测）
- `group_id` 字段在 SDK 结构体中存在但实际响应中从不填充
- hello-halo 的 `WEIXIN_GROUP_POLICY` 配置项是未来占位符，当前无效
- **群聊场景正确路径：使用 WeCom（企业微信）渠道**，uclaw 已有 WecomBot 实现

---

## 文件变动总表

| 文件 | 变动性质 |
|---|---|
| `src-tauri/src/channels/im/ilink.rs` | 重构（提取 IlinkSharedState）+ 修复（-14 传播、LRU、长度截断、group_id 过滤）|
| `src-tauri/src/channels/dispatcher.rs` | 修复（空消息过滤、text block 拼接、send_text 错误日志）|
| `src-tauri/src/channels/im/ilink_binding.rs` | 不变（已对齐）|
| `src-tauri/src/channels/manager.rs` | 微改（IlinkSender::new 签名不变，外部接口零变化）|
| `Cargo.toml` | 不变（`lru` crate 已存在）|

**不在本 spec 范围：**
- 主动推送 pushToChat（YAGNI）
- per-space 系统提示词（需单独 schema migration）
- WeCom 群聊（独立 issue）
- message_count_today 计数（独立小 task）

---

## 详细设计

### 1. IlinkSharedState — 共享运行时状态

```rust
// src-tauri/src/channels/im/ilink.rs

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::VecDeque;
use tokio::sync::{Mutex, RwLock};

const CONTEXT_TOKEN_CACHE_CAP: usize = 500;
const DEDUP_CAPACITY: usize = 200;
const MAX_REPLY_CHARS: usize = 4_000;

/// 被 IlinkSender（轮询端）和 IlinkReplySender（发送端）共享的运行时状态。
/// 通过 Arc 共享，任意一端触发 -14 均可立即停止轮询。
pub(super) struct IlinkSharedState {
    /// user_id → 最新 context_token，LRU 上限 500 条。
    pub context_tokens: RwLock<LruCache<String, String>>,
    /// false = -14 已在任意路径触发，poll_loop 下次迭代检测后退出。
    pub session_active: AtomicBool,
    /// 已见过的 message_id 去重缓冲，迁移至此供两端统一管理。
    pub seen_msg_ids: Mutex<VecDeque<String>>,
}

impl IlinkSharedState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            context_tokens: RwLock::new(
                LruCache::new(NonZeroUsize::new(CONTEXT_TOKEN_CACHE_CAP).unwrap())
            ),
            session_active: AtomicBool::new(true),
            seen_msg_ids: Mutex::new(VecDeque::with_capacity(DEDUP_CAPACITY)),
        })
    }

    pub async fn invalidate(&self) {
        self.session_active.store(false, Ordering::Relaxed);
        self.context_tokens.write().await.clear();
        self.seen_msg_ids.lock().await.clear();
    }
}
```

### 2. IlinkSender — 使用共享状态

```rust
pub struct IlinkSender {
    instance_id: String,
    bot_token: String,
    base_url: String,
    shared: Arc<IlinkSharedState>,   // 替代原来的 context_tokens + seen_msg_ids
    client: reqwest::Client,
    status_tx: UnboundedSender<ChannelRuntimeStatus>,
}

impl IlinkSender {
    pub fn new(
        instance_id: &str,
        config: &Value,
        credentials: &Value,
        status_tx: UnboundedSender<ChannelRuntimeStatus>,
    ) -> Self {
        Self {
            instance_id: instance_id.to_string(),
            bot_token: credentials["bot_token"].as_str().unwrap_or("").to_string(),
            base_url: config["base_url"].as_str().filter(|s| !s.is_empty())
                .unwrap_or(ILINK_BASE_URL).to_string(),
            shared: IlinkSharedState::new(),
            client: reqwest::Client::new(),
            status_tx,
        }
    }
}
```

### 3. poll_loop — session_active 提前检测

```rust
async fn poll_loop(&self, inbound_tx: mpsc::UnboundedSender<...>) {
    // ...
    loop {
        // 检测 sendmessage 路径已触发 -14 的情况
        if !self.shared.session_active.load(Ordering::Relaxed) {
            tracing::warn!(
                "[IlinkBot:{}] session_active=false (set by sendmessage -14), stopping poll",
                self.instance_id
            );
            let _ = self.status_tx.send(ChannelRuntimeStatus {
                instance_id: self.instance_id.clone(),
                state: ChannelState::NeedsRebind,
                last_error: Some("会话已失效（-14），请重新扫码绑定".to_string()),
                connected_since_ms: None,
                message_count_today: 0,
            });
            break;
        }

        match self.single_poll(&mut updates_buf, &inbound_tx).await {
            Ok(true)  => { connected = true; attempt = 0; }
            Ok(false) => {
                // getupdates 返回 -14，single_poll 内已调用 shared.invalidate()
                let _ = self.status_tx.send(ChannelRuntimeStatus {
                    state: ChannelState::NeedsRebind, ..
                });
                break;
            }
            Err(e) => { /* 指数退避，与现有逻辑相同 */ }
        }
    }
}
```

### 4. single_poll — -14 时调用 invalidate

```rust
async fn single_poll(&self, updates_buf: &mut String, inbound_tx: &...) -> Result<bool> {
    // ... HTTP 请求不变 ...
    let ret_code = resp["ret"].as_i64().unwrap_or(resp["errcode"].as_i64().unwrap_or(0));
    if ret_code == SESSION_EXPIRED_CODE {
        self.shared.invalidate().await;   // 清理 context_tokens + seen_msg_ids
        return Ok(false);
    }
    // ... 其余逻辑不变 ...
}
```

### 5. handle_inbound — group_id 防御过滤 + 使用共享状态

```rust
async fn handle_inbound(&self, msg: &Value, inbound_tx: &...) {
    // 1. group_id 防御性过滤（iLink DM-only；群消息 skip 并 debug log）
    if let Some(gid) = msg["group_id"].as_str().filter(|s| !s.is_empty()) {
        tracing::debug!(
            "[IlinkBot:{}] group message (group_id={gid}) — iLink is DM-only, skipping",
            self.instance_id
        );
        return;
    }

    // 2. user_id 提取（不变）
    let user_id = match msg["from_user_id"].as_str() {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => return,
    };

    // 3. 去重（从 IlinkSharedState 读取，原逻辑不变）
    let dedup_key = /* 同现有逻辑 */;
    if let Some(ref key) = dedup_key {
        let mut seen = self.shared.seen_msg_ids.lock().await;
        if seen.contains(key) { return; }
        if seen.len() >= DEDUP_CAPACITY { seen.pop_front(); }
        seen.push_back(key.clone());
    }

    // 4. context_token 写入 LRU 缓存
    let context_token = msg["context_token"].as_str().map(String::from);
    if let Some(ref ct) = context_token {
        self.shared.context_tokens.write().await.put(user_id.clone(), ct.clone());
    }

    // 5. 构建 InboundMessage + IlinkReplySender（传入 shared）
    let sender_arc: Arc<dyn ImChannelSender> = Arc::new(IlinkReplySender {
        instance_id: self.instance_id.clone(),
        bot_token: self.bot_token.clone(),
        base_url: self.base_url.clone(),
        client: self.client.clone(),
        shared: self.shared.clone(),   // 共享状态传入
    });
    // ... 其余不变 ...
}
```

### 6. IlinkReplySender — -14 回传 + 长度截断 + 错误日志

```rust
struct IlinkReplySender {
    instance_id: String,
    bot_token: String,
    base_url: String,
    client: reqwest::Client,
    shared: Arc<IlinkSharedState>,   // 新增
}

#[async_trait]
impl ImChannelSender for IlinkReplySender {
    async fn send_text(&self, chat_id: &str, text: &str, ctx: Option<&Value>) -> Result<(), String> {
        let context_token = ctx
            .and_then(|c| c["context_token"].as_str())
            .map(String::from)
            .or_else(|| {
                // Fallback：从 LRU 缓存查最近一条（channel_ctx 意外丢失时）
                self.shared.context_tokens.try_read().ok()
                    .and_then(|cache| cache.peek(chat_id).cloned())
            })
            .ok_or_else(|| format!(
                "[IlinkBot:{}] 无 context_token，无法发送给 {chat_id}",
                self.instance_id
            ))?;

        // 长度截断（对齐 hello-halo 4000 字符限制）
        let text_to_send: String = if text.chars().count() > MAX_REPLY_CHARS {
            let truncated: String = text.chars().take(MAX_REPLY_CHARS).collect();
            format!("{truncated}\n\n…（内容已截断）")
        } else {
            text.to_string()
        };

        // ... 构建 request body（不变）...

        let ret = resp_body["ret"].as_i64().unwrap_or(resp_body["errcode"].as_i64().unwrap_or(0));

        if ret == SESSION_EXPIRED_CODE {
            // 写回共享 flag，poll_loop 下次迭代将停止
            self.shared.invalidate().await;
            return Err(format!(
                "[IlinkBot:{}] sendmessage 会话失效（-14），已通知 poll_loop 停止",
                self.instance_id
            ));
        }
        if ret != 0 {
            return Err(format!(
                "[IlinkBot:{}] sendmessage 错误 ret={ret}: {}",
                self.instance_id,
                resp_body["errmsg"].as_str().unwrap_or("unknown")
            ));
        }
        Ok(())
    }
}
```

### 7. dispatcher.rs — 三处修复

```rust
// --- 修复 1：空消息过滤（run_agent_chat_via_im 顶部）---
if msg.text.trim().is_empty() {
    tracing::debug!("[IM dispatcher] empty inbound text from {}, skipping agent", msg.chat_id);
    return Ok(());
}

// --- 修复 2：多 text block 拼接（替换 find_map）---
// 旧：只取最后一个 Assistant text block
// 新：收集所有 Assistant text block，按顺序拼接
let final_assistant_text: String = reason_ctx.messages
    .iter()
    .filter(|m| m.role == MessageRole::Assistant)
    .flat_map(|m| m.content.iter())
    .filter_map(|b| match b {
        ContentBlock::Text { text } if !text.trim().is_empty() => Some(text.as_str()),
        _ => None,
    })
    .collect::<Vec<_>>()
    .join("\n\n")
    .trim()
    .to_string();

// --- 修复 3：send_text 错误不静默（替换 let _ = ...）---
if let Some(ref sh) = delegate.streaming_handle {
    if let Err(e) = sh.finish(&final_assistant_text).await {
        tracing::error!("[IM] streaming finish failed for session {session_id}: {e}");
    }
} else if let Some(ref rh) = delegate.reply_handle {
    if let Err(e) = rh.sender.send_text(
        &rh.chat_id, &final_assistant_text, rh.channel_ctx.as_ref()
    ).await {
        tracing::error!("[IM] reply send failed for {} (session {session_id}): {e}", rh.chat_id);
        // 不尝试二次发送（避免循环），错误已记录
    }
}
```

---

## 测试策略

每个修复对应一个独立的 `#[tokio::test]`，均使用 mockito mock HTTP 端点：

| 测试名 | 验证内容 |
|---|---|
| `session_active_false_stops_poll_before_getupdates` | IlinkReplySender 收到 -14 → `session_active=false` → poll_loop 在下次迭代 break（不再发 HTTP）|
| `sendmessage_14_calls_invalidate` | mock sendmessage 返回 `{"ret":-14}` → `session_active=false` + context_tokens 清空 + seen_msg_ids 清空 |
| `getupdates_14_calls_invalidate` | mock getupdates 返回 `{"ret":-14}` → same as above |
| `reply_truncated_at_4000_chars` | 4001 字符文本 → 发出的 `item_list[0].text_item.text` ≤ 4000 字符 + 包含截断标记 |
| `context_token_lru_evicts_at_capacity` | 插入 501 条 → LRU 自动 evict，容量保持 500 |
| `context_token_fallback_from_lru` | `ctx=None` 但 LRU 中有该 user_id 的缓存 → send_text 使用缓存 token 成功 |
| `group_id_message_skipped` | msg 含非空 `group_id` → `handle_inbound` 立即返回，inbound_tx 不收到消息 |
| `empty_text_message_not_dispatched` | `InboundMessage.text=""` → `dispatch_inbound` 提前返回，agent loop 不运行 |
| `multi_text_block_reply_joined` | reason_ctx 含 2 个 Assistant text block → `final_assistant_text` = block1 + "\n\n" + block2 |
| `send_text_error_logged_not_panicked` | mock sendmessage 返回 `{"ret":1,"errmsg":"err"}` → send_text 返回 Err，dispatcher 记录 error log 后继续（不 panic，不重试）|
| `invalidate_clears_all_shared_state` | 调用 `IlinkSharedState::invalidate()` → session_active=false, context_tokens 为空, seen_msg_ids 为空 |

---

## 实现约束与注意事项

1. **`lru` crate 已在 Cargo.toml 中**（被 `memory_graph` 模块使用），无需新增依赖。检查 `lru::LruCache` 的实际版本 API（`.put()` / `.peek()` / `.clear()`）。

2. **`IlinkSender::new()` 对外签名不变**，`IlinkSharedState::new()` 在内部构造，`manager.rs` 无需改动。

3. **`IlinkReplySender` 新增 `shared` 字段**，在 `handle_inbound` 中通过 `self.shared.clone()` 传入，`Arc::clone` 成本极低。

4. **`Ordering::Relaxed` 用于 `session_active`**：这是一个单向 flag（true→false），无需 acquire/release 内存序。poll_loop 发现 false 后 break 即可，不存在数据竞争。

5. **测试中 `IlinkSender::new()` 调用**：现有测试（`status_tx_receives_needs_rebind_on_session_expired` 等）的 new() 签名不变，无需修改测试构造代码。

6. **`context_token` fallback 路径**（`try_read().ok()`）：这是一个非阻塞尝试，不是 `expect`。`try_read` 在锁竞争时返回 None，fallback 到错误路径，不阻塞发送线程。

---

## 群聊扩展路径（本 spec 范围外，仅作参考）

如果未来需要微信群聊支持，正确路径是：

1. **WeCom（企业微信）渠道扩展**：uclaw 已有 `WecomBot` 实现，WeCom 协议原生支持群消息（`chattype: "group"`）
2. 在 `WecomBot` 的 `handle_inbound` 中区分 `chatType: direct` vs `group`，用 `group_id` 作为 `chat_id`
3. `ImSessionRegistry.get_or_create_session()` 的 key 为 `(space_id, channel_type, chat_id)`，群 ID 作为 chat_id 天然适配，无需改 session 层
4. **不应尝试通过 iLink 实现群聊**：协议层硬限制，无论配置如何均无法收到群消息
