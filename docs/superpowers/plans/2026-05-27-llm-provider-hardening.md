# LLM Provider 层加固 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让多 provider BYOK 真正可用:`create_provider` 按 `ApiType` 正确路由(修 kimi-coding/minimax 误走 OpenAI),OpenAI 流式并行 tool calls 不丢。

**Architecture:** `LlmConfig` 加 `api: Option<ApiType>`;`provider_service` 解析回传 ProviderConfig.api;调用点算 effective api(override ?? registry default_api);`create_provider` 经 pure `resolve_api` 分派。OpenAI SSE 解析按 index 聚合 tool_calls,finish 时按序 flush。行为保持(api=None+"anthropic" 不变;单 tool 不变)。

**Tech Stack:** Rust,reqwest SSE,serde_json,async_trait。

**Spec:** `docs/superpowers/specs/2026-05-27-llm-provider-hardening-design.md`
**Branch/worktree:** `codex/sprint4-provider-hardening`(base = main `e25a85ad`)

---

## ⚠️ 规划期事实(实现以此为准)

- `LlmConfig`(`config/llm.rs:4-26`):`#[derive(Debug,Clone,Serialize,Deserialize)] #[serde(rename_all="camelCase")] { provider:String, model:String, api_key:String, base_url:Option<String>, max_tokens:Option<u32>, temperature:Option<f32> }` + `Default`(provider="anthropic", model="claude-sonnet-4-20250514", max_tokens=Some(16384), temp=Some(0.7))。
- `llm/mod.rs:1-53`:`pub use providers::{anthropic::AnthropicProvider, openai::OpenAIProvider}`;`pub fn create_provider(config:&LlmConfig)->Result<Arc<dyn LlmProvider>,Error>` 现 `match config.provider.as_str() { "anthropic"=>Anthropic, _=>OpenAI }`;`pub fn llm_config_from_provider(provider_id,model,api_key,base_url:&str,max_tokens:u32,temperature:f32)->LlmConfig`。`AnthropicProvider::new(String,Option<String>)`、`OpenAIProvider::new(String,Option<String>)`。
- `ApiType`(`providers/types.rs:76-91`):`#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)] #[serde(rename_all="kebab-case")] { OpenAiCompletions, AnthropicMessages, OpenAiResponses, OpenAiCodexResponses }`。`ProviderConfig{provider_id, display_name, api_key:Option, base_url:Option, api:Option<ApiType>}`(:217-232)。`KnownProvider{... default_api:ApiType ...}`(types.rs:152-170);`registry::find(provider_id:&str)->Option<KnownProvider>`(registry.rs:317);kimi-coding/minimax 的 default_api = AnthropicMessages。
- `provider_service`(`providers/service.rs:143-192`):`get_chat_llm_config()` / `get_provider_llm_config(pid,mid)` 均 `async -> Option<(String,String,String,String)>`(provider_id,model,api_key,base_url);内部 `configs.find_provider(pid)->Option<ProviderConfig>`(有 .api),但 api 被丢弃。
- 调用点(`tauri_commands.rs:1840-1876`):`let resolved = ... get_provider_llm_config / get_chat_llm_config`;`let llm_config = if let Some((provider_id,model,api_key,base_url))=resolved { llm::llm_config_from_provider(&provider_id,&model,&api_key,&base_url,max_tokens,temperature) } else { legacy_config.clone() };`;`let llm = llm::create_provider(&llm_config)?;`。
- OpenAI SSE(`llm/providers/openai.rs`):`async fn stream(...)`(:454)→ `OpenAISseStream::new(byte_stream, timeout)`;实际状态 `OpenAiSseState`,方法 `next_delta(&mut self)`(:552-642,async 驱动,行缓冲)、`extract_line`(:644)、`parse_chunk(&mut self, json:&Value)->Option<StreamDelta>`(:653-743,**私有**)。`[DONE]` 分支在 next_delta(:571-581)直接 emit `Done{finish_reason, usage:accumulated_usage.take()}`;usage-only chunk → `accumulated_usage=Some(...)` return None(:677-686);finish_reason+empty delta → `pending_finish_reason=Some(...)` return None(:700-703)。**tool bug**(:724-740):`if let Some(tc)=tool_calls.first() {...}` 只取第一个,未读 `index`。无 SSE 测试;现有测试(:787)`model_requires_fixed_temperature` + `convert_messages`。
- `StreamDelta`(`agent/types.rs:289-297`):`ToolCallDelta{id:String,name:Option<String>,input_json:Option<String>}` 等。assembler(`llm_stream.rs:185-216`):遇 `name=Some` flush 上一个 current_tool 并开新;`input_json=Some` 累加;`Done` flush 末个。⇒ 按序 `ToolCallDelta{id,Some(name),None}` + `ToolCallDelta{id,None,Some(args)}` × N → N 个 distinct ToolCall。`RespondOutput::ToolCalls{tool_calls,...}`。

---

## 验证命令
- 编译:`cd /Users/ryanliu/Documents/uclaw-worktrees/sprint4-provider-hardening/src-tauri && cargo build 2>&1 | grep -E "^error" | head`(空)
- 单测:同目录 `cargo test --lib <filter> 2>&1 | tail -15`
- 已知预存失败 ~7(daemon_approval / truncate_for_error / browser provider_execution×3 / runtime_pack_ipc / gbrain_eval_harness);stale tauri 产物报旧路径则清 `target/debug/build/tauri-*`+`uclaw-*`。

---

## File Structure
| 文件 | 责任 |
|---|---|
| `src-tauri/src/config/llm.rs` | `LlmConfig.api: Option<ApiType>`(serde default)+ Default 补 api:None |
| `src-tauri/src/llm/mod.rs` | `resolve_api` helper + `create_provider` 按 ApiType 分派 + `llm_config_from_provider` 加 api 参 + 单测 |
| `src-tauri/src/providers/service.rs` | `get_chat_llm_config`/`get_provider_llm_config` 回传 5-tuple(+ ProviderConfig.api)|
| `src-tauri/src/tauri_commands.rs` | 调用点算 effective api + 传入 `llm_config_from_provider` |
| `src-tauri/src/llm/providers/openai.rs` | `OpenAiSseState` tool_calls 按 index 聚合 + finish flush + 可测 + SSE 单测 |

---

## Task 1: LlmConfig.api + resolve_api + create_provider 分派

**Files:** Modify `src-tauri/src/config/llm.rs`, `src-tauri/src/llm/mod.rs`

- [ ] **Step 1: LlmConfig 加 api 字段** — `config/llm.rs`,在 `temperature` 后:
```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<crate::providers::types::ApiType>,
```
`Default` impl 补一行 `api: None,`。

- [ ] **Step 2: 失败测试(resolve_api)** — `llm/mod.rs` 底部加 test mod:
```rust
#[cfg(test)]
mod tests {
    use super::resolve_api;
    use crate::providers::types::ApiType;

    #[test]
    fn explicit_anthropic_messages_routes_anthropic() {
        assert_eq!(resolve_api("kimi-coding", Some(ApiType::AnthropicMessages)), ApiType::AnthropicMessages);
    }
    #[test]
    fn none_with_anthropic_id_backcompat() {
        assert_eq!(resolve_api("anthropic", None), ApiType::AnthropicMessages);
    }
    #[test]
    fn none_with_other_id_is_openai() {
        assert_eq!(resolve_api("deepseek", None), ApiType::OpenAiCompletions);
    }
    #[test]
    fn explicit_override_wins() {
        assert_eq!(resolve_api("anthropic", Some(ApiType::OpenAiCompletions)), ApiType::OpenAiCompletions);
    }
}
```

- [ ] **Step 3: 确认红** — `cargo test --lib llm::tests::resolve_api 2>&1 | grep -E "cannot find|error\[" | head`(`resolve_api` 未定义)。

- [ ] **Step 4: 实现 resolve_api + create_provider** — `llm/mod.rs`,加 import `use crate::providers::types::ApiType;` 和:
```rust
/// 决定某 provider 用哪种 wire API。config_api(ProviderConfig.api 覆盖)优先;
/// 否则 "anthropic" id → AnthropicMessages,其余 → OpenAiCompletions(同历史行为)。
pub(crate) fn resolve_api(provider_id: &str, config_api: Option<ApiType>) -> ApiType {
    config_api.unwrap_or_else(|| {
        if provider_id == "anthropic" { ApiType::AnthropicMessages } else { ApiType::OpenAiCompletions }
    })
}
```
改 `create_provider`:
```rust
pub fn create_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, crate::error::Error> {
    match resolve_api(&config.provider, config.api.clone()) {
        ApiType::AnthropicMessages => Ok(Arc::new(AnthropicProvider::new(
            config.api_key.clone(), config.base_url.clone(),
        ))),
        // OpenAiCompletions + Responses/Codex(best-effort 走 chat/completions,范围外做 native)
        _ => Ok(Arc::new(OpenAIProvider::new(
            config.api_key.clone(), config.base_url.clone(),
        ))),
    }
}
```
`llm_config_from_provider` 加 `api: Option<ApiType>` 参(放末位)并存入字面量 `api`:
```rust
pub fn llm_config_from_provider(
    provider_id: &str, model: &str, api_key: &str, base_url: &str,
    max_tokens: u32, temperature: f32, api: Option<ApiType>,
) -> LlmConfig {
    LlmConfig {
        provider: provider_id.to_string(), model: model.to_string(), api_key: api_key.to_string(),
        base_url: if base_url.is_empty() { None } else { Some(base_url.to_string()) },
        max_tokens: Some(max_tokens), temperature: Some(temperature), api,
    }
}
```
> `ApiType` 现无 `Clone`?——它 `#[derive(Debug,Clone,...)]` 有 Clone(types.rs:76)。`config.api.clone()` OK。

- [ ] **Step 5: 绿 + 编译** — `cargo test --lib llm::tests::resolve_api 2>&1 | tail -6`(4 passed);`cargo build 2>&1 | grep -E "^error" | head`。
> 注:本步 `llm_config_from_provider` 改签名会让其调用点(tauri_commands)暂时编译失败 —— Task 2 修。若要本任务独立编译绿,可在本步同时给调用点补 `None`(占位),Task 2 再换成 effective api。**实现者:本步顺手把 tauri_commands 调用点补一个 `None` 占位让全树编译过**(Task 2 替换为真值)。

- [ ] **Step 6: Commit**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-provider-hardening add src-tauri/src/config/llm.rs src-tauri/src/llm/mod.rs src-tauri/src/tauri_commands.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-provider-hardening commit -m "feat(llm): resolve_api + create_provider dispatch by ApiType

LlmConfig gains api: Option<ApiType> (serde default). create_provider routes via
resolve_api: AnthropicMessages -> AnthropicProvider, else OpenAI; api=None +
'anthropic' id -> Anthropic (back-compat). llm_config_from_provider takes api.
(tauri_commands call site stubbed None here; Task 2 wires effective api.)

Verification: cargo test --lib llm::tests::resolve_api -> 4 passed; build clean"
```

---

## Task 2: provider_service 回传 api + 调用点算 effective api

**Files:** Modify `src-tauri/src/providers/service.rs`, `src-tauri/src/tauri_commands.rs`

- [ ] **Step 1: service 回传 5-tuple** — `service.rs`:把 `get_provider_llm_config` 与 `get_chat_llm_config` 返回类型从 `Option<(String,String,String,String)>` 改为 `Option<(String,String,String,String,Option<crate::providers::types::ApiType>)>`,每个 `Some((...))` 末位补 `provider.api.clone()`(`provider` 是 `find_provider` 取到的 `ProviderConfig`,有 `.api`)。例如 `get_provider_llm_config`:
```rust
    Some((
        provider_id.to_string(), model_id.to_string(),
        provider.api_key.clone().unwrap_or_default(),
        provider.base_url.clone().unwrap_or_default(),
        provider.api.clone(),
    ))
```
`get_chat_llm_config` 的两个 `Some((...))`(role_models 分支 + active_model 分支)同样末位补 `provider.api.clone()`。

- [ ] **Step 2: 调用点算 effective api** — `tauri_commands.rs:1858-1862` 改:
```rust
    let llm_config = if let Some((provider_id, model, api_key, base_url, api_override)) = resolved {
        let effective_api = api_override.or_else(|| {
            crate::providers::registry::find(&provider_id).map(|k| k.default_api)
        });
        llm::llm_config_from_provider(&provider_id, &model, &api_key, &base_url, max_tokens, temperature, effective_api)
    } else {
        // legacy fallback(provider 多为 "anthropic";api=None → resolve_api 兜底)
        if legacy_config.api_key.is_empty() {
            return Err(Error::InvalidInput("No API key configured. Please set up your AI provider in Settings.".into()));
        }
        legacy_config.clone()
    };
```
> `registry::find` 路径:`crate::providers::registry::find`(registry.rs:317,返回 `Option<KnownProvider>`,`.default_api: ApiType`)。`KnownProvider` 需 import 或全路径;`k.default_api` 直接取。effective = ProviderConfig.api 覆盖 ?? registry default_api ?? None(None → resolve_api 用 id 兜底)。
> 若 Task 1 在本调用点放了 `None` 占位,这里替换为上面的真值。

- [ ] **Step 3: 编译 + 现有测试** — `cargo build 2>&1 | grep -E "^error" | head`(空);`cargo test --lib providers 2>&1 | tail -6`(无新失败)。

- [ ] **Step 4: (可选)effective api 集成测试** — 若 `provider_service` 可在测试中构造(读其是否有 test ctor;有则)加一个测试:配一个 `ProviderConfig{provider_id:"kimi-coding", api:None}` + active_model 指向它 → `get_chat_llm_config` 回 `api=None`;调用点 `registry::find("kimi-coding").default_api == AnthropicMessages`。若 service 难构造,跳过(resolve_api 单测 + registry::find 已覆盖逻辑),在 PR 说明。

- [ ] **Step 5: Commit**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-provider-hardening add src-tauri/src/providers/service.rs src-tauri/src/tauri_commands.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-provider-hardening commit -m "feat(providers): thread effective ApiType into LlmConfig at resolution

provider_service returns ProviderConfig.api; send_agent_message computes effective
api (override ?? registry default_api) and passes it to llm_config_from_provider,
so Anthropic-format providers (kimi-coding/minimax) route to AnthropicProvider.

Verification: cargo build clean; cargo test --lib providers -> no new failures"
```

---

## Task 3: OpenAI 并行 tool_calls 按 index 聚合 + finish flush

**Files:** Modify `src-tauri/src/llm/providers/openai.rs`

- [ ] **Step 1: 读现状** — 通读 `OpenAiSseState`(字段 + `next_delta` :552-642 + `parse_chunk` :653-743 + `[DONE]` 分支 :571-581 + usage-only :677 + finish_reason+empty :700)。确认字段含 `pending_finish_reason`、`accumulated_usage`、`done`、行缓冲。

- [ ] **Step 2: 给 OpenAiSseState 加缓冲 + 队列** — 在 struct 加:
```rust
    /// 并行 tool calls 按 index 聚合:index -> (id, name, args_concat)。
    tool_buf: std::collections::BTreeMap<u64, (String, String, String)>,
    /// finish 时把 tool_buf flush 成的待发 delta 队列(含末尾 Done)。
    pending: std::collections::VecDeque<StreamDelta>,
```
构造处(`OpenAiSseState::new` 或字面量)初始化 `tool_buf: BTreeMap::new(), pending: VecDeque::new()`。

- [ ] **Step 3: parse_chunk tool_calls 改按 index 聚合(不再 .first(),不再即时 emit)** — 把 :724-740 的 tool 块替换为:
```rust
        // Tool calls —— 聚合所有 index 的分片(OpenAI 并行 function calling 一个 chunk 可含多条;
        // 且同一 call 的 name/args 跨 chunk 分片到达,按 index 累加)。不即时 emit,finish 统一 flush。
        if let Some(tool_calls) = delta["tool_calls"].as_array() {
            for tc in tool_calls {
                let index = tc["index"].as_u64().unwrap_or(0);
                let entry = self.tool_buf.entry(index).or_insert_with(|| (String::new(), String::new(), String::new()));
                if let Some(id) = tc["id"].as_str() { if !id.is_empty() { entry.0 = id.to_string(); } }
                let func = &tc["function"];
                if let Some(name) = func["name"].as_str() { if !name.is_empty() { entry.1 = name.to_string(); } }
                if let Some(args) = func["arguments"].as_str() { entry.2.push_str(args); }
            }
            return None;   // 缓冲,finish flush
        }
```

- [ ] **Step 4: finish 时 flush tool_buf → pending,排在 Done 前** — 加一个私有 helper:
```rust
    /// 把聚合的 tool calls 按 index 升序展开成 (name then args) delta 推入 pending,再推 Done。
    fn flush_finish(&mut self, finish_reason: Option<String>) {
        let buf = std::mem::take(&mut self.tool_buf);
        for (_idx, (id, name, args)) in buf {
            self.pending.push_back(StreamDelta::ToolCallDelta { id: id.clone(), name: Some(name), input_json: None });
            self.pending.push_back(StreamDelta::ToolCallDelta { id, name: None, input_json: Some(args) });
        }
        self.pending.push_back(StreamDelta::Done { finish_reason, usage: self.accumulated_usage.take() });
    }
```
改 `[DONE]` 分支(:571-581):不再直接构造 Done,改为:
```rust
        if data == "[DONE]" {
            self.done = true;
            let finish_reason = self.pending_finish_reason.take().flatten().or_else(|| Some("stop".into()));
            self.flush_finish(finish_reason);
            return Some(Ok(self.pending.pop_front().expect("flush_finish pushes Done")));
        }
```

- [ ] **Step 5: next_delta 先 drain pending** — 在 `next_delta` 循环最前面加:
```rust
        if let Some(d) = self.pending.pop_front() {
            return Some(Ok(d));
        }
```
> 安全网:若流自然结束(无 `[DONE]`)且 `tool_buf` 非空,在 next_delta 检测到底层 byte stream 结束(现返回 None / 设 done)处,先 `if !self.tool_buf.is_empty() && self.pending.is_empty() { self.flush_finish(Some("stop".into())); return Some(Ok(self.pending.pop_front().unwrap())); }`。实现者按 next_delta 真实的"流结束"分支接入(OpenAI 正常发 `[DONE]`,此为兜底)。

- [ ] **Step 6: 让 SSE 解析可单测** — 把 `parse_chunk`、`flush_finish` 与构造器标 `pub(crate)`(或给 `OpenAiSseState` 加 `pub(crate) fn new_for_test() -> Self`)。drain 也暴露:测试通过「构造 state → 逐个喂 parse_chunk(serde_json 解析的 chunk JSON)→ 触发 flush_finish → drain pending」验证。

- [ ] **Step 7: 测试** — `openai.rs` test mod 加:
```rust
    fn chunk(v: serde_json::Value) -> serde_json::Value { v }

    #[test]
    fn parallel_tool_calls_all_preserved() {
        let mut st = OpenAiSseState::new_for_test();
        // chunk A: 两个 call 的 name(index 0/1)
        st.parse_chunk(&serde_json::json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"call_a","function":{"name":"tool_a"}},
            {"index":1,"id":"call_b","function":{"name":"tool_b"}}
        ]},"finish_reason":null}]}));
        // chunk B/C: 各自 args 分片(交错)
        st.parse_chunk(&serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"x\":"}}]}}]}));
        st.parse_chunk(&serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"{\"y\":"}}]}}]}));
        st.parse_chunk(&serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"1}"}}]}}]}));
        st.parse_chunk(&serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"2}"}}]}}]}));
        st.flush_finish(Some("tool_calls".into()));
        // drain pending,组装出两个 call
        let mut names = vec![]; let mut args = vec![];
        while let Some(d) = st.pending.pop_front() {
            if let StreamDelta::ToolCallDelta { name: Some(n), .. } = &d { names.push(n.clone()); }
            if let StreamDelta::ToolCallDelta { input_json: Some(a), .. } = &d { args.push(a.clone()); }
        }
        assert_eq!(names, vec!["tool_a", "tool_b"]);
        assert_eq!(args, vec!["{\"x\":1}", "{\"y\":2}"]);
    }

    #[test]
    fn single_tool_call_unchanged_shape() {
        let mut st = OpenAiSseState::new_for_test();
        st.parse_chunk(&serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"only"}}]}}]}));
        st.parse_chunk(&serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{}"}}]}}]}));
        st.flush_finish(Some("tool_calls".into()));
        let deltas: Vec<_> = st.pending.drain(..).collect();
        // [ToolCallDelta name, ToolCallDelta args, Done]
        assert_eq!(deltas.len(), 3);
        assert!(matches!(&deltas[2], StreamDelta::Done { .. }));
    }
```
> 字段访问(`st.pending` / `st.tool_buf`)需在同模块可见(test mod 在 openai.rs 内,私有字段可访问)。`OpenAiSseState::new_for_test()` 构造一个空状态(byte stream 用空/dummy;测试只调 parse_chunk/flush_finish 不驱动 next_delta)。若 `new` 需要 byte stream 参数,加一个仅置默认状态的 `new_for_test`。

- [ ] **Step 8: 编译 + 测试 + 提交** — `cargo build 2>&1 | grep -E "^error" | head`(空);`cargo test --lib openai 2>&1 | tail -10`(全过含 2 新)。
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-provider-hardening add src-tauri/src/llm/providers/openai.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-provider-hardening commit -m "fix(llm): OpenAI streaming preserves parallel tool calls (index-aggregated)

OpenAiSseState buffers all delta.tool_calls[] by index (was .first(), dropped
extras), flushes at finish as sequential name+args deltas so the llm_stream
assembler yields all N calls. Single-tool result unchanged. + SSE unit tests.

Verification: cargo test --lib openai -> pass (incl. parallel test); build clean"
```

---

## Task 4: 转换 parity 测试 + 验收收口

**Files:** Modify `src-tauri/src/llm/providers/openai.rs`(测试)

- [ ] **Step 1: 转换 parity 测试(补未覆盖)** — 确认现有 `valid_tool_result_stays_tool_role` 覆盖 ToolUse→tool_calls;补一个 ToolResult→`role:"tool"` 断言(若现有未直接断言):
```rust
    #[test]
    fn tool_result_becomes_tool_role() {
        let provider = OpenAIProvider::new("k".into(), None);
        let messages = vec![
            ChatMessage { role: MessageRole::Assistant, content: vec![ContentBlock::ToolUse { id: "call_1".into(), name: "t".into(), input: serde_json::json!({}) }], compacted: false },
            ChatMessage { role: MessageRole::User, content: vec![ContentBlock::ToolResult { tool_use_id: "call_1".into(), content: "ok".into(), is_error: Some(false) }], compacted: false },
        ];
        let converted = provider.convert_messages(&messages);
        // 末条应是 role:"tool",tool_call_id:"call_1"
        let last = converted.last().unwrap();
        assert_eq!(last["role"], "tool");
        assert_eq!(last["tool_call_id"], "call_1");
    }
```
> 读 `convert_messages` 真实输出形状对齐断言(orphan 处理:有匹配的前置 tool_call → role:tool;字段名 `tool_call_id`)。若形状不同,按真实输出改断言(目标:验证 ToolResult 映射到 OpenAI tool 角色)。

- [ ] **Step 2: 全量验收** — `cargo build`(空 error);`cargo test --lib "llm::" "openai" "providers" 2>&1 | tail -15`(全过);全量 `cargo test --lib 2>&1 | tail -6`(仅 ~7 已知预存失败,零新增)。

- [ ] **Step 3: Commit**
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-provider-hardening add src-tauri/src/llm/providers/openai.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/sprint4-provider-hardening commit -m "test(llm): OpenAI message-conversion parity (ToolResult -> tool role)

Verification: cargo test --lib -> no new failures (7 known pre-existing)"
```

---

## 最终验收
- [ ] `cargo build` → 空 error
- [ ] `llm::tests::resolve_api` 4/4;`openai` SSE/转换测试全过
- [ ] 全量 `cargo test --lib` → 仅 ~7 已知预存失败
- [ ] kimi-coding/minimax(api=None)经 `registry::find` 得 AnthropicMessages → AnthropicProvider(逻辑覆盖于 resolve_api + registry::find)
- [ ] 行为保持:`api=None`+"anthropic" → Anthropic;OpenAI 单 tool 结果不变

---

## Self-Review
**Spec coverage:** §3 工厂 ApiType 路由 → Task1(resolve_api+create_provider+LlmConfig.api)+Task2(service 回传+调用点 effective api);§4 OpenAI 并行 tool_calls → Task3(index 聚合+finish flush);§5 测试 → Task1(resolve_api)/Task3(SSE 多/单 tool)/Task4(转换 parity);§6 行为保持 → Task1(resolve_api 兜底)/Task3(单 tool 不变);§7 commit 序列 → Task1-4 顺序一致。

**Placeholder scan:** Task1 Step5 的"调用点补 None 占位让全树编译,Task2 替真值"是明确的跨任务编译策略(非 TBD)。Task2 Step4 / Task4 Step1 的"读真实形状对齐断言"是给实现者的对齐指令 + 具体断言骨架。Task3 Step5 的流结束兜底是带具体代码的边界处理。无 TODO/TBD。

**Type consistency:** `resolve_api(&str, Option<ApiType>) -> ApiType`(Task1)→ create_provider 调用一致;`LlmConfig.api: Option<ApiType>`(Task1)→ llm_config_from_provider 末参 + 字面量一致;`llm_config_from_provider(...,api:Option<ApiType>)`(Task1)↔ 调用点传 effective_api(Task2)一致;service 5-tuple `(...,Option<ApiType>)`(Task2)↔ 调用点 destructure 5 元一致;`registry::find(id)->Option<KnownProvider>` + `.default_api:ApiType`(Task2)与 registry 真实一致;`StreamDelta::ToolCallDelta{id,name:Option,input_json:Option}` / `Done{finish_reason,usage}`(Task3)与 types 一致;`tool_buf: BTreeMap<u64,(String,String,String)>` + `pending: VecDeque<StreamDelta>`(Task3)贯穿 parse_chunk/flush_finish/next_delta 一致。
