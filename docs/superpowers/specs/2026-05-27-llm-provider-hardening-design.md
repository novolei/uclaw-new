# LLM Provider 层加固设计 (Sprint 4 ①)

**状态:** 设计已逐节批准,待 spec 评审 → writing-plans
**分支/worktree:** `codex/sprint4-provider-hardening`(base = main `e25a85ad`)
**前置:** ADR §15(保留多 provider 服务)、Pi 升级设计 §7(BYOK / 多 provider)。Sprint 4 ② 是 BYOK UI(下个 slice)。

---

## 0. 背景:Sprint 4 ① 大部分已实现

探查结论:`LlmProvider` trait + `AnthropicProvider` + `OpenAIProvider` + `create_provider` 工厂 + 按请求 provider 选择 + 29-provider registry **均已在生产**(Pi spec 的「无 provider 抽象」论断写于代码落地前)。故本 slice **不是**"建 trait + OpenAI",而是**加固现有 provider 层的两个真实 gap**(直接影响 BYOK 多 provider 可用性):

1. `create_provider` 只按 `provider_id == "anthropic"` 路由,**忽略 ApiType** → Anthropic-格式但 id≠"anthropic" 的 provider(`kimi-coding`/`minimax`)误走 OpenAI wire format → 运行时 4xx。
2. OpenAI 流式只取 `delta.tool_calls.first()` → 并行 tool calls 丢失除第一个外全部。

---

## 1. 目标与范围

**目标:** 让多 provider BYOK 真正可用 —— 工厂按 ApiType 正确路由、OpenAI 并行 tool calls 不丢、补跨 provider parity 测试。**行为保持**:`api=None` + `"anthropic"` 路径不变;OpenAI 单 tool 结果不变。

**范围内:**
- **工厂 ApiType 路由**:`LlmConfig` 加 `api: Option<ApiType>`;`provider_service` 解析时算 effective api(ProviderConfig.api ?? registry default_api)填入;`create_provider` 经 pure helper `resolve_api(provider_id, config_api) -> ApiType` 分派(`AnthropicMessages`→Anthropic;其余→OpenAI;`api=None && id=="anthropic"`→Anthropic 兜底)。
- **OpenAI 并行 tool calls**:SSE 解析按 `index` 聚合所有 `delta.tool_calls[]`,finish 前按序发出各 call 的 name+args delta(契合现有 `llm_stream` assembler,**不动共享 assembler / Anthropic 路径**)。
- **parity 测试**:`resolve_api` 单测;OpenAI 多/单 tool SSE fixture 解析单测;OpenAI 消息转换 parity(ToolUse→tool_calls / ToolResult→role:tool)。

**明确范围外(留后续):**
- OpenAI o1/o3 的 `reasoning_effort`/`thinking_enabled`(minor)。
- OpenAI Responses/Codex API(`OpenAiResponses`/`OpenAiCodexResponses` 暂走 chat/completions best-effort)。
- Gemini native 格式(Sprint 5;现走 OpenAI-compat base-url)。
- prompt caching(`cache_control`)保持 Anthropic-only(有意)。
- **BYOK UI**(Sprint 4 ②,下个 slice;后端 ProviderConfig/ProviderService/configure_provider 已就绪)。

---

## 2. 现状锚点(实现以此为准)

- `LlmProvider` trait(`src-tauri/src/llm/provider.rs:23-38`):`async fn complete(messages, tools, config) -> Result<RespondOutput, Error>`、`async fn stream(...) -> Result<Box<dyn Stream<Item=Result<StreamDelta,Error>>+Send+Unpin>, Error>`。`CompletionConfig{model, max_tokens, temperature, thinking_enabled}`。无 `provider_id(&self)`。
- 工厂(`llm/mod.rs:21-33`):`pub fn create_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, Error>`,现 `match config.provider.as_str() { "anthropic" => Anthropic, _ => OpenAI }`。
- `LlmConfig`(`config/llm.rs:6-13`):`{provider:String, model:String, api_key:String, base_url:Option<String>, max_tokens:Option<u32>, temperature:Option<f32>}`。**无 api 字段**。
- `ApiType`(`providers/types.rs`):`OpenAiCompletions | AnthropicMessages | OpenAiResponses | OpenAiCodexResponses`。`ProviderConfig{provider_id, display_name, api_key:Option, base_url:Option, api:Option<ApiType>}`。registry(`providers/registry.rs`)每 provider 有 `default_api`(`kimi-coding`/`minimax` = `AnthropicMessages`,id≠"anthropic")。
- 解析(`tauri_commands.rs:1841-1876` send_agent_message):`provider_service.get_provider_llm_config(pid,mid)` 或 `get_chat_llm_config()` → `(provider_id, model, api_key, base_url)` → 组 `LlmConfig` → `create_provider`。**当前不传 ApiType**。`ChatDelegate.llm: Arc<dyn LlmProvider>`(dispatcher.rs:24);call_llm 走 `llm_stream::stream_completion(self.llm.as_ref(), ...)`。
- `StreamDelta`(`agent/types.rs:291-297`,re-export uclaw_message_types):`TextDelta{text} | ThinkingDelta{thinking} | SignatureDelta{signature} | ToolCallDelta{id:String, name:Option<String>, input_json:Option<String>} | Done{finish_reason:Option<String>, usage:Option<TokenUsage>}`。
- `llm_stream::stream_completion`(`agent/llm_stream.rs:66-73`)驱动 provider stream → `RespondOutput`;assembler 跟踪 `current_tool: Option<(id,name,args)>`,遇带 name 的 `ToolCallDelta` flush 上一个 → 适配「按序 name+args」。
- **OpenAI provider**(`llm/providers/openai.rs`):`POST {base_url}/v1/chat/completions`,`Authorization: Bearer`,tools `{type:"function",function:{name,description,parameters}}`,`stream_options:{include_usage:true}`;SSE 行解析 `data:` + `[DONE]`;**tool 流式现 `delta.tool_calls.first()`(:724-742,bug)**;usage 在尾部 usage-only chunk(`pending_finish_reason`+`accumulated_usage` 延迟到 `[DONE]`)。`convert_messages` ToolUse→tool_calls / ToolResult→role:tool(orphan 检测)。
- **Anthropic provider**(`llm/providers/anthropic.rs`):`/v1/messages`;并行 tool 经**顺序** `content_block_start` 事件(不交错)→ 现 assembler OK。

---

## 3. 工厂 ApiType 路由(已批准 Section 1)

- `config/llm.rs` `LlmConfig` 加字段:`pub api: Option<crate::providers::types::ApiType>`(`#[serde(default)]`,向后兼容;旧 JSON 无此字段 → None)。
- `provider_service` 解析(`get_provider_llm_config` / `get_chat_llm_config`)算 effective api 填入 `LlmConfig.api`:`ProviderConfig.api`(用户/配置覆盖)`??` `registry::default_api(provider_id)`(29-provider 表)`??` None。读 provider_service 真实结构;若解析返回 tuple,扩成带 api 或在组 `LlmConfig` 处补 api。
- `llm/mod.rs`:加 pure helper(可单测):
```rust
pub(crate) fn resolve_api(provider_id: &str, config_api: Option<ApiType>) -> ApiType {
    config_api.unwrap_or_else(|| {
        if provider_id == "anthropic" { ApiType::AnthropicMessages } else { ApiType::OpenAiCompletions }
    })
}
```
`create_provider` 改:
```rust
match resolve_api(&config.provider, config.api) {
    ApiType::AnthropicMessages => Ok(Arc::new(AnthropicProvider::new(config.api_key.clone(), config.base_url.clone()))),
    _ => Ok(Arc::new(OpenAIProvider::new(config.api_key.clone(), config.base_url.clone()))),
}
```
> `OpenAiResponses`/`OpenAiCodexResponses` → OpenAIProvider(chat/completions best-effort,范围外做 native)。`api=None && id!="anthropic"` → OpenAiCompletions(同现行为)。`api=None && id=="anthropic"` → Anthropic(back-compat)。

---

## 4. OpenAI 并行 tool calls(已批准 Section 2)

- `openai.rs` SSE 处理:把现 `delta.tool_calls.first()` 改为遍历 `delta.tool_calls[]`,按 `index` 聚合进 `HashMap<u32, PartialToolCall { id:String, name:String, args:String }>`(`id`/`name` 取首个非空,`args` 累加 `function.arguments` 分片)。
- finish(尾部 `[DONE]`/usage-only chunk 触发 Done 前):按 index 升序,对每个 buffered call 发:`StreamDelta::ToolCallDelta{ id, name: Some(name), input_json: None }` 然后 `StreamDelta::ToolCallDelta{ id, name: None, input_json: Some(args) }`。assembler 遇下一个 name flush 上一个 → 得到全部 N 个 distinct call。
- 单 tool:buffer 1 个,发 1 个 → 结果同现状(仅 args 在 finish 一次性给出,非增量)。
- **不动** `llm_stream` assembler 与 Anthropic provider。
- tradeoff:OpenAI tool-call args 在 finish 给出而非增量(minor;tool_start/result 活动 UI 不受影响;text/thinking delta 仍实时)。

---

## 5. 测试(已批准 Section 3)

- `resolve_api` 单测(`llm/mod.rs` 或 tests):`(Some(AnthropicMessages),"kimi-coding")→AnthropicMessages`;`(None,"anthropic")→AnthropicMessages`;`(None,"deepseek")→OpenAiCompletions`;`(Some(OpenAiCompletions),"anthropic")→OpenAiCompletions`(显式覆盖胜)。
- **OpenAI 并行 tool SSE 单测(核心回归)**:把现 SSE 解析缝做成可单测(取 byte/line 流 → `Vec<StreamDelta>` 或驱动到 `RespondOutput`;若现为内联闭包,抽一个 `pub(crate) fn parse_openai_sse(...)` 薄函数)。喂 canned fixture:两个 tool_calls(index 0/1,跨 chunk 交错)+ usage-only chunk + `[DONE]` → 断言组装出**两个** call(id/name/arguments 正确)。+ 单 tool fixture(结果不变)+ usage 提取断言。
- OpenAI 消息转换 parity:`ToolUse`→assistant `tool_calls[]`;`ToolResult`→`role:"tool"`(无则补)。
- 验收 gate:`cargo build` 干净;`cargo test --lib "llm::" "openai" 2>&1` 全过;全量无新失败(~7 已知预存)。

---

## 6. 错误处理 + 行为保持

- `resolve_api` 总返回一个 ApiType(无 panic);未知 id + api=None → OpenAiCompletions(同现行为)。
- 工厂签名不变(`create_provider(&LlmConfig)`);`LlmConfig.api` 是新 `Option`(`serde default`)→ 旧配置/调用点零改动即编译(api=None)。仅 provider_service 解析处主动填 api。
- OpenAI 单 tool 结果不变;并行从「丢失」→「全保留」是纯修复。
- 现有 provider 测试保持绿。

---

## 7. 文件结构 + commit 序列(可二分)

| 文件 | 责任 |
|---|---|
| `src-tauri/src/config/llm.rs` | `LlmConfig.api: Option<ApiType>`(serde default)|
| `src-tauri/src/llm/mod.rs` | `resolve_api` helper + `create_provider` 按 ApiType 分派 + 单测 |
| `src-tauri/src/providers/...`(service)| 解析填 `LlmConfig.api`(effective = ProviderConfig.api ?? registry default_api)|
| `src-tauri/src/llm/providers/openai.rs` | 并行 tool_calls 按 index 聚合 + finish 发出 + SSE 解析可测 + 单测 |

commit 序列:`LlmConfig.api + resolve_api + create_provider 分派(单测)` → `provider_service 解析填 effective api` → `openai 并行 tool_calls 聚合(单测)` → `parity/转换测试收口`。
