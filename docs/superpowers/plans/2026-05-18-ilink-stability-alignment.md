# iLink 稳定性重构与 hello-halo 对齐实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 WeChat iLink IM 消息流程中已确认的所有 bug，将架构与 hello-halo 参考实现对齐。

**Architecture:** 提取 `IlinkSharedState`（私有结构体，通过 `Arc` 共享），让 `IlinkSender`（轮询端）和 `IlinkReplySender`（回复端）共享 context_tokens LRU 缓存、`session_active` 原子标志和 `seen_msg_ids` 去重缓冲区；同时修复 `dispatcher.rs` 中三处独立 bug。

**Tech Stack:** Rust, tokio, reqwest, `lru = "0.12"`（需新增依赖）, mockito 0.32.5

---

## 文件变动总表

| 文件 | 变动性质 |
|---|---|
| `src-tauri/Cargo.toml` | 新增 `lru = "0.12"` |
| `src-tauri/src/channels/im/ilink.rs` | 重构 + 修复（5 项）|
| `src-tauri/src/channels/dispatcher.rs` | 修复（3 项）|

---

## Task 1: 新增 `lru` 依赖 + 定义 `IlinkSharedState`

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/channels/im/ilink.rs`

- [ ] **Step 1: 写失败测试 `invalidate_clears_all_shared_state`**

在 `ilink.rs` 的 `#[cfg(test)] mod tests` 块末尾追加：

```rust
#[tokio::test]
async fn invalidate_clears_all_shared_state() {
    let shared = IlinkSharedState::new();
    shared.context_tokens.write().await.put("u1".to_string(), "ct1".to_string());
    shared.seen_msg_ids.lock().await.push_back("id1".to_string());
    assert!(shared.session_active.load(std::sync::atomic::Ordering::Relaxed));

    shared.invalidate().await;

    assert!(!shared.session_active.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(shared.context_tokens.read().await.len(), 0);
    assert_eq!(shared.seen_msg_ids.lock().await.len(), 0);
}
```

- [ ] **Step 2: 运行测试，确认编译错误（IlinkSharedState 不存在）**

```bash
cd src-tauri && cargo test --lib channels::im::ilink::tests::invalidate_clears_all_shared_state 2>&1 | grep -E "^error" | head -5
```

Expected: `error[E0422]: cannot find struct ... IlinkSharedState`

- [ ] **Step 3: 在 `Cargo.toml` 新增 `lru` 依赖**

在 `[dependencies]` 段落中，找到 `rand = "0.8"` 附近，插入：

```toml
lru = "0.12"
```

- [ ] **Step 4: 在 `ilink.rs` 顶部追加 use 声明**

在现有 use 块后追加（`use std::collections::VecDeque;` 已存在，不要重复）：

```rust
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
```

- [ ] **Step 5: 在 `ilink.rs` 现有常量后追加两个新常量**

紧跟 `const DEDUP_CAPACITY: usize = 200;` 之后：

```rust
const CONTEXT_TOKEN_CACHE_CAP: usize = 500;
const MAX_REPLY_CHARS: usize = 4_000;
```

- [ ] **Step 6: 在 `pub struct IlinkSender` 定义之前插入 `IlinkSharedState`**

```rust
struct IlinkSharedState {
    /// user_id → 最新 context_token，LRU 上限 500 条。
    context_tokens: RwLock<LruCache<String, String>>,
    /// false = -14 已触发，poll_loop 下次迭代后退出。
    session_active: AtomicBool,
    /// 已见过的消息去重缓冲。
    seen_msg_ids: Mutex<VecDeque<String>>,
}

impl IlinkSharedState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            context_tokens: RwLock::new(
                LruCache::new(NonZeroUsize::new(CONTEXT_TOKEN_CACHE_CAP).unwrap()),
            ),
            session_active: AtomicBool::new(true),
            seen_msg_ids: Mutex::new(VecDeque::with_capacity(DEDUP_CAPACITY)),
        })
    }

    async fn invalidate(&self) {
        self.session_active.store(false, Ordering::Relaxed);
        self.context_tokens.write().await.clear();
        self.seen_msg_ids.lock().await.clear();
    }
}
```

- [ ] **Step 7: 运行测试，确认通过**

```bash
cd src-tauri && cargo test --lib channels::im::ilink::tests::invalidate_clears_all_shared_state 2>&1 | tail -5
```

Expected: `test channels::im::ilink::tests::invalidate_clears_all_shared_state ... ok`

- [ ] **Step 8: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/channels/im/ilink.rs
git commit -m "feat(ilink): add lru dep + IlinkSharedState struct with invalidate()"
```

---

## Task 2: 将 `IlinkSender` 迁移至 `IlinkSharedState`

**Files:**
- Modify: `src-tauri/src/channels/im/ilink.rs`

- [ ] **Step 1: 写失败测试 `group_id_message_skipped`**

在 `mod tests` 块末尾追加：

```rust
#[tokio::test]
async fn group_id_message_skipped() {
    let (status_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let (inbound_tx, mut inbound_rx) = tokio::sync::mpsc::unbounded_channel();
    let sender = Arc::new(IlinkSender::new(
        "test",
        &serde_json::json!({}),
        &serde_json::json!({ "bot_token": "tok" }),
        status_tx,
    ));

    let msg = serde_json::json!({
        "from_user_id": "user1",
        "group_id": "group123",
        "context_token": "ct1",
        "item_list": [{ "type": 1, "text_item": { "text": "hello" } }]
    });
    sender.handle_inbound(&msg, &inbound_tx).await;

    assert!(inbound_rx.try_recv().is_err(), "group message must not be dispatched");
}
```

- [ ] **Step 2: 运行测试，确认失败（group_id 未过滤）**

```bash
cd src-tauri && cargo test --lib channels::im::ilink::tests::group_id_message_skipped 2>&1 | tail -5
```

Expected: FAILED（惨败或 group 消息被错误派发）

- [ ] **Step 3: 替换 `IlinkSender` 结构体定义**

将现有 `pub struct IlinkSender { ... }` 替换为：

```rust
pub struct IlinkSender {
    instance_id: String,
    bot_token: String,
    base_url: String,
    shared: Arc<IlinkSharedState>,
    client: reqwest::Client,
    status_tx: tokio::sync::mpsc::UnboundedSender<ChannelRuntimeStatus>,
}
```

- [ ] **Step 4: 替换 `IlinkSender::new()` 实现**

将现有 `pub fn new(...)` 的 `Self { ... }` 体替换为：

```rust
pub fn new(
    instance_id: &str,
    config: &Value,
    credentials: &Value,
    status_tx: tokio::sync::mpsc::UnboundedSender<ChannelRuntimeStatus>,
) -> Self {
    let bot_token = credentials["bot_token"].as_str().unwrap_or("").to_string();
    let base_url = config["base_url"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(ILINK_BASE_URL)
        .to_string();
    Self {
        instance_id: instance_id.to_string(),
        bot_token,
        base_url,
        shared: IlinkSharedState::new(),
        client: reqwest::Client::new(),
        status_tx,
    }
}
```

- [ ] **Step 5: 替换 `handle_inbound` 方法**

将整个 `async fn handle_inbound(...)` 方法替换为：

```rust
async fn handle_inbound(
    &self,
    msg: &Value,
    inbound_tx: &mpsc::UnboundedSender<(InboundMessage, Arc<ReplyHandle>)>,
) {
    // iLink 是个人号 DM-only；群消息在协议层面不可达，防御性过滤。
    if let Some(gid) = msg["group_id"].as_str().filter(|s| !s.is_empty()) {
        tracing::debug!(
            "[IlinkBot:{}] group message (group_id={gid}) — iLink DM-only, skipping",
            self.instance_id
        );
        return;
    }

    let user_id = match msg["from_user_id"].as_str() {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => return,
    };

    // 去重：iLink 在 ACK 前重传同一条消息，跳过已见过的 ID。
    let dedup_key = msg["message_id"]
        .as_i64()
        .map(|id| id.to_string())
        .or_else(|| {
            msg["context_token"]
                .as_str()
                .map(|ct| format!("{}:{}", user_id, ct))
        });
    if let Some(ref key) = dedup_key {
        let mut seen = self.shared.seen_msg_ids.lock().await;
        if seen.contains(key) {
            tracing::debug!(
                "[IlinkBot:{}] duplicate msg key={key}, skipping",
                self.instance_id
            );
            return;
        }
        if seen.len() >= DEDUP_CAPACITY {
            seen.pop_front();
        }
        seen.push_back(key.clone());
    }

    let context_token = msg["context_token"].as_str().map(String::from);
    if let Some(ref ct) = context_token {
        self.shared
            .context_tokens
            .write()
            .await
            .put(user_id.clone(), ct.clone());
    }

    let items = &msg["item_list"];
    let text = Self::extract_text(items);

    tracing::info!(
        "[IlinkBot:{}] inbound from={user_id} len={}",
        self.instance_id,
        text.len()
    );

    let channel_ctx = context_token
        .as_ref()
        .map(|ct| json!({ "context_token": ct }));

    let inbound = InboundMessage {
        instance_id: self.instance_id.clone(),
        chat_id: user_id.clone(),
        sender_name: Some(user_id.clone()),
        text,
        timestamp: chrono::Utc::now().timestamp_millis(),
        channel_ctx: channel_ctx.clone(),
    };

    let sender_arc: Arc<dyn ImChannelSender> = Arc::new(IlinkReplySender {
        instance_id: self.instance_id.clone(),
        bot_token: self.bot_token.clone(),
        base_url: self.base_url.clone(),
        client: self.client.clone(),
        shared: self.shared.clone(),
    });
    let reply = Arc::new(ReplyHandle {
        sender: sender_arc,
        channel_ctx,
        chat_id: user_id,
    });

    let _ = inbound_tx.send((inbound, reply));
}
```

- [ ] **Step 6: 运行所有 ilink 测试，确认全部通过**

```bash
cd src-tauri && cargo test --lib channels::im::ilink 2>&1 | tail -10
```

Expected: 全部 ok，包括原有的 4 个测试和新的 `group_id_message_skipped`

- [ ] **Step 7: 写测试 `context_token_lru_evicts_at_capacity` 并运行**

在 `mod tests` 末尾追加：

```rust
#[tokio::test]
async fn context_token_lru_evicts_at_capacity() {
    let shared = IlinkSharedState::new();
    for i in 0..=CONTEXT_TOKEN_CACHE_CAP {
        shared
            .context_tokens
            .write()
            .await
            .put(format!("user{i}"), format!("ct{i}"));
    }
    // LRU 自动 evict 最旧条目，容量保持 CONTEXT_TOKEN_CACHE_CAP
    assert_eq!(
        shared.context_tokens.read().await.len(),
        CONTEXT_TOKEN_CACHE_CAP
    );
}
```

```bash
cd src-tauri && cargo test --lib channels::im::ilink::tests::context_token_lru_evicts_at_capacity 2>&1 | tail -5
```

Expected: `... ok`

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/channels/im/ilink.rs
git commit -m "refactor(ilink): migrate IlinkSender to IlinkSharedState; add group_id defense filter"
```

---

## Task 3: 更新 `IlinkReplySender`（-14 回传 + 截断 + LRU fallback）

**Files:**
- Modify: `src-tauri/src/channels/im/ilink.rs`

- [ ] **Step 1: 写失败测试 `sendmessage_14_calls_invalidate`**

在 `mod tests` 末尾追加：

```rust
#[tokio::test]
async fn sendmessage_14_calls_invalidate() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/ilink/bot/sendmessage")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ret":-14}"#)
        .create_async()
        .await;

    let shared = IlinkSharedState::new();
    shared.context_tokens.write().await.put("user1".to_string(), "ct1".to_string());
    shared.seen_msg_ids.lock().await.push_back("id1".to_string());

    let reply_sender = IlinkReplySender {
        instance_id: "test".to_string(),
        bot_token: "tok".to_string(),
        base_url: server.url(),
        client: reqwest::Client::new(),
        shared: shared.clone(),
    };

    let ctx = serde_json::json!({ "context_token": "ct1" });
    let result = reply_sender.send_text("user1", "hello", Some(&ctx)).await;

    assert!(result.is_err(), "sendmessage -14 must return Err");
    assert!(
        !shared.session_active.load(Ordering::Relaxed),
        "session_active must be false after -14"
    );
    assert_eq!(shared.context_tokens.read().await.len(), 0, "context_tokens must be cleared");
    assert_eq!(shared.seen_msg_ids.lock().await.len(), 0, "seen_msg_ids must be cleared");
}
```

- [ ] **Step 2: 运行测试，确认编译失败（IlinkReplySender 缺 shared 字段）**

```bash
cd src-tauri && cargo test --lib channels::im::ilink::tests::sendmessage_14_calls_invalidate 2>&1 | grep -E "^error" | head -5
```

Expected: `error[E0560]: struct ... IlinkReplySender has no field named 'shared'`

- [ ] **Step 3: 替换 `IlinkReplySender` 结构体定义**

将现有 `struct IlinkReplySender { ... }` 替换为：

```rust
struct IlinkReplySender {
    instance_id: String,
    bot_token: String,
    base_url: String,
    client: reqwest::Client,
    shared: Arc<IlinkSharedState>,
}
```

- [ ] **Step 4: 在 `IlinkReplySender` impl 块中添加截断辅助方法**

在 `#[async_trait] impl ImChannelSender for IlinkReplySender {` 之前插入：

```rust
impl IlinkReplySender {
    fn truncate_reply(text: &str) -> String {
        if text.chars().count() > MAX_REPLY_CHARS {
            let truncated: String = text.chars().take(MAX_REPLY_CHARS).collect();
            format!("{truncated}\n\n…（内容已截断）")
        } else {
            text.to_string()
        }
    }
}
```

- [ ] **Step 5: 替换 `send_text` 实现**

将整个 `async fn send_text(...)` 方法替换为：

```rust
async fn send_text(
    &self,
    chat_id: &str,
    text: &str,
    ctx: Option<&Value>,
) -> Result<(), String> {
    let context_token = ctx
        .and_then(|c| c["context_token"].as_str())
        .map(String::from)
        .or_else(|| {
            // Fallback：channel_ctx 意外丢失时从 LRU 缓存读最近一条。
            self.shared
                .context_tokens
                .try_read()
                .ok()
                .and_then(|cache| cache.peek(chat_id).cloned())
        })
        .ok_or_else(|| {
            format!(
                "[IlinkBot:{}] Cannot reply to {chat_id}: missing context_token",
                self.instance_id
            )
        })?;

    let text_to_send = Self::truncate_reply(text);

    use base64::Engine;
    let n: u32 = rand::random();
    let uin =
        base64::engine::general_purpose::STANDARD.encode(n.to_string().as_bytes());

    let url = format!("{}/ilink/bot/sendmessage", self.base_url);
    let body = json!({
        "msg": {
            "from_user_id": "",
            "to_user_id": chat_id,
            "client_id": uuid::Uuid::new_v4().to_string(),
            "message_type": 2,
            "message_state": 2,
            "context_token": context_token,
            "item_list": [{
                "type": 1,
                "text_item": { "text": text_to_send }
            }]
        },
        "base_info": { "channel_version": CHANNEL_VERSION }
    });

    let resp = self
        .client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("AuthorizationType", "ilink_bot_token")
        .header("Authorization", format!("Bearer {}", self.bot_token))
        .header("X-WECHAT-UIN", uin)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let http_status = resp.status();
    let resp_body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
    let ret = resp_body["ret"]
        .as_i64()
        .unwrap_or(resp_body["errcode"].as_i64().unwrap_or(0));

    if !http_status.is_success() {
        return Err(format!("iLink sendmessage HTTP {http_status} ret={ret}"));
    }
    if ret == SESSION_EXPIRED_CODE {
        self.shared.invalidate().await;
        return Err(format!(
            "[IlinkBot:{}] sendmessage session expired (-14), poll_loop notified",
            self.instance_id
        ));
    }
    if ret != 0 {
        return Err(format!(
            "iLink sendmessage error ret={ret}: {}",
            resp_body["errmsg"].as_str().unwrap_or("")
        ));
    }
    Ok(())
}
```

- [ ] **Step 6: 运行所有 ilink 测试**

```bash
cd src-tauri && cargo test --lib channels::im::ilink 2>&1 | tail -10
```

Expected: 全部 ok

- [ ] **Step 7: 追加剩余两个测试并运行**

```rust
#[test]
fn truncate_reply_at_4000_chars() {
    let input = "x".repeat(4001);
    let result = IlinkReplySender::truncate_reply(&input);
    let first_line: &str = result.split('\n').next().unwrap();
    assert_eq!(first_line.chars().count(), MAX_REPLY_CHARS);
    assert!(result.contains("内容已截断"));
}

#[test]
fn no_truncation_under_limit() {
    let input = "x".repeat(MAX_REPLY_CHARS);
    assert_eq!(IlinkReplySender::truncate_reply(&input), input);
}

#[tokio::test]
async fn context_token_fallback_from_lru() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/ilink/bot/sendmessage")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ret":0}"#)
        .create_async()
        .await;

    let shared = IlinkSharedState::new();
    // 预置 LRU 缓存，模拟之前收过消息时已存 context_token
    shared
        .context_tokens
        .write()
        .await
        .put("user1".to_string(), "cached_ct".to_string());

    let reply_sender = IlinkReplySender {
        instance_id: "test".to_string(),
        bot_token: "tok".to_string(),
        base_url: server.url(),
        client: reqwest::Client::new(),
        shared,
    };

    // ctx=None → 应从 LRU fallback，不报 "missing context_token"
    let result = reply_sender.send_text("user1", "hi", None).await;
    assert!(result.is_ok(), "LRU fallback should succeed: {:?}", result);
}
```

```bash
cd src-tauri && cargo test --lib channels::im::ilink 2>&1 | tail -10
```

Expected: 全部 ok

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/channels/im/ilink.rs
git commit -m "feat(ilink): IlinkReplySender — -14 propagation, reply truncation, LRU ctx fallback"
```

---

## Task 4: 更新 `poll_loop` + `single_poll` 支持 `session_active` 检测

**Files:**
- Modify: `src-tauri/src/channels/im/ilink.rs`

- [ ] **Step 1: 写失败测试 `session_active_false_stops_poll_loop`**

在 `mod tests` 末尾追加：

```rust
#[tokio::test]
async fn session_active_false_stops_poll_loop() {
    // 不设置任何 HTTP mock — 若 poll_loop 发出 HTTP 请求将会连接失败，
    // 测试通过表示 session_active=false 在 HTTP 之前被检测到并 break。
    let (status_tx, mut status_rx) = tokio::sync::mpsc::unbounded_channel();
    let (inbound_tx, _) = tokio::sync::mpsc::unbounded_channel();

    let sender = Arc::new(IlinkSender::new(
        "test-stop",
        &serde_json::json!({ "base_url": "http://127.0.0.1:1" }), // 无法连接的地址
        &serde_json::json!({ "bot_token": "tok" }),
        status_tx,
    ));

    // 在 start 之前将 session 标记为失效
    sender.shared.session_active.store(false, Ordering::Relaxed);

    let abort = sender.clone().start(inbound_tx);

    let status = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        status_rx.recv(),
    )
    .await
    .expect("timeout: NeedsRebind 未在 3s 内发出")
    .expect("channel closed");

    abort.abort();
    assert_eq!(status.state, crate::channels::types::ChannelState::NeedsRebind);
    assert_eq!(status.instance_id, "test-stop");
}
```

- [ ] **Step 2: 运行测试，确认失败（poll_loop 不检测 session_active）**

```bash
cd src-tauri && cargo test --lib channels::im::ilink::tests::session_active_false_stops_poll_loop 2>&1 | tail -5
```

Expected: FAILED（超时或收到 Error 而非 NeedsRebind）

- [ ] **Step 3: 更新 `poll_loop`，在循环顶部添加 `session_active` 检测**

将现有 `poll_loop` 中的 `loop {` 开头替换为（在 `match self.single_poll(...)` 之前插入）：

```rust
loop {
    // sendmessage 路径已触发 -14 时提前退出，不再发 HTTP。
    if !self.shared.session_active.load(Ordering::Relaxed) {
        tracing::warn!(
            "[IlinkBot:{}] session_active=false (set via sendmessage -14), stopping poll",
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
        // ... 其余不变 ...
```

- [ ] **Step 4: 更新 `single_poll`，-14 时调用 `shared.invalidate()`**

找到 `single_poll` 中：

```rust
if ret_code == SESSION_EXPIRED_CODE {
    return Ok(false);
}
```

替换为：

```rust
if ret_code == SESSION_EXPIRED_CODE {
    self.shared.invalidate().await;
    return Ok(false);
}
```

- [ ] **Step 5: 追加测试 `getupdates_14_calls_invalidate` 并运行所有 ilink 测试**

```rust
#[tokio::test]
async fn getupdates_14_calls_invalidate() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/ilink/bot/getupdates")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ret":-14}"#)
        .create_async()
        .await;

    let (status_tx, mut status_rx) = tokio::sync::mpsc::unbounded_channel();
    let (inbound_tx, _) = tokio::sync::mpsc::unbounded_channel();

    let sender = Arc::new(IlinkSender::new(
        "test-gtu14",
        &serde_json::json!({ "base_url": server.url() }),
        &serde_json::json!({ "bot_token": "tok" }),
        status_tx,
    ));
    // 预置 shared 状态
    sender.shared.context_tokens.write().await.put("u1".to_string(), "ct1".to_string());

    let abort = sender.clone().start(inbound_tx);

    let status = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        status_rx.recv(),
    )
    .await
    .expect("timeout")
    .expect("channel closed");

    abort.abort();
    assert_eq!(status.state, crate::channels::types::ChannelState::NeedsRebind);
    // invalidate() 应已清空缓存
    assert!(!sender.shared.session_active.load(Ordering::Relaxed));
    assert_eq!(sender.shared.context_tokens.read().await.len(), 0);
}
```

```bash
cd src-tauri && cargo test --lib channels::im::ilink 2>&1 | tail -10
```

Expected: 全部 ok（共 ~12 个测试）

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/channels/im/ilink.rs
git commit -m "feat(ilink): poll_loop session_active guard; single_poll calls invalidate on -14"
```

---

## Task 5: 修复 `dispatcher.rs` 三处 Bug

**Files:**
- Modify: `src-tauri/src/channels/dispatcher.rs`

### Bug 1：空消息未过滤（agent 空跑）
### Bug 2：只取最后一个 text block（多工具调用后回复不完整）
### Bug 3：`send_text` 错误静默丢弃（用户收不到回复但无日志）

- [ ] **Step 1: 写失败测试 `empty_text_not_dispatched` 和 `extract_final_text_joins_all_blocks`**

在 `dispatcher.rs` 的 `#[cfg(test)] mod tests` 末尾追加：

```rust
#[test]
fn empty_text_not_dispatched_guard() {
    // 直接测试过滤谓词，不依赖 AppHandle。
    let empty_inputs = ["", "   ", "\t\n"];
    for input in empty_inputs {
        assert!(
            input.trim().is_empty(),
            "should be treated as empty: {:?}", input
        );
    }
    let non_empty = "hello";
    assert!(!non_empty.trim().is_empty());
}

#[test]
fn extract_final_text_joins_all_blocks() {
    let messages = vec![
        crate::agent::types::ChatMessage {
            role: crate::agent::types::MessageRole::User,
            content: vec![crate::agent::types::ContentBlock::Text { text: "q".into() }],
            compacted: false,
        },
        crate::agent::types::ChatMessage {
            role: crate::agent::types::MessageRole::Assistant,
            content: vec![crate::agent::types::ContentBlock::Text { text: "first reply".into() }],
            compacted: false,
        },
        crate::agent::types::ChatMessage {
            role: crate::agent::types::MessageRole::Assistant,
            content: vec![crate::agent::types::ContentBlock::Text { text: "second reply".into() }],
            compacted: false,
        },
    ];
    let result = extract_final_assistant_text(&messages);
    assert_eq!(result, "first reply\n\nsecond reply");
}
```

- [ ] **Step 2: 运行测试，确认 `extract_final_text_joins_all_blocks` 编译失败（函数不存在）**

```bash
cd src-tauri && cargo test --lib channels::dispatcher::tests::extract_final_text_joins_all_blocks 2>&1 | grep -E "^error" | head -3
```

Expected: `error[E0425]: cannot find function 'extract_final_assistant_text'`

- [ ] **Step 3: 在 `dispatcher.rs` 中提取辅助函数 `extract_final_assistant_text`**

在 `pub async fn dispatch_inbound(...)` 定义之前插入：

```rust
/// 收集 messages 中所有 Assistant text block，按出现顺序拼接。
/// 替代原先只取最后一个 block 的 find_map 逻辑。
pub(super) fn extract_final_assistant_text(messages: &[ChatMessage]) -> String {
    messages
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
        .to_string()
}
```

- [ ] **Step 4: 运行测试，确认通过**

```bash
cd src-tauri && cargo test --lib channels::dispatcher::tests 2>&1 | tail -10
```

Expected: 全部 ok

- [ ] **Step 5: 在 `run_agent_chat_via_im` 函数开头添加空消息过滤**

找到：

```rust
async fn run_agent_chat_via_im(
    msg: InboundMessage,
    ...
) -> Result<()> {
    let channel_type_str = instance.channel_type.as_str().to_string();
```

在 `let channel_type_str = ...` 之前插入：

```rust
if msg.text.trim().is_empty() {
    tracing::debug!("[IM] empty inbound text from {}, skipping agent run", msg.chat_id);
    return Ok(());
}
```

- [ ] **Step 6: 将 `final_assistant_text` 提取改为使用新辅助函数**

找到现有的（约 329–343 行）：

```rust
let final_assistant_text = reason_ctx
    .messages
    .iter()
    .rev()
    .find_map(|m| {
        if m.role == MessageRole::Assistant {
            m.content.iter().find_map(|b| match b {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
        } else {
            None
        }
    })
    .unwrap_or_default();
```

替换为：

```rust
let final_assistant_text = extract_final_assistant_text(&reason_ctx.messages);
```

- [ ] **Step 7: 将 `send_text` 的静默丢弃替换为错误日志**

找到现有（约 345–349 行）：

```rust
if let Some(ref sh) = delegate.streaming_handle {
    let _ = sh.finish(&final_assistant_text).await;
} else if let Some(ref rh) = delegate.reply_handle {
    let _ = rh.sender.send_text(&rh.chat_id, &final_assistant_text, rh.channel_ctx.as_ref()).await;
}
```

替换为：

```rust
if let Some(ref sh) = delegate.streaming_handle {
    if let Err(e) = sh.finish(&final_assistant_text).await {
        tracing::error!("[IM] streaming finish failed for session {session_id}: {e}");
    }
} else if let Some(ref rh) = delegate.reply_handle {
    if let Err(e) = rh
        .sender
        .send_text(&rh.chat_id, &final_assistant_text, rh.channel_ctx.as_ref())
        .await
    {
        tracing::error!(
            "[IM] reply send failed for {} (session {session_id}): {e}",
            rh.chat_id
        );
    }
}
```

- [ ] **Step 8: 运行全量测试**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -15
```

Expected: 全部 ok，无 regression（基准 842 tests passing）

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/channels/dispatcher.rs
git commit -m "fix(dispatcher): empty text guard, join all assistant blocks, log send_text errors"
```

---

## 收尾

- [ ] **全量验证**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib 2>&1 | grep -E "FAILED|error\[" | head -10
```

Expected: 0 errors, 0 failures

- [ ] **最终 Commit（如前几个 task 已各自 commit 则跳过）**

此时所有改动已通过 Task 1–5 的逐步 commit 提交，无需额外 commit。
