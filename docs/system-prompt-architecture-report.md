# 系统提示词(Prompt)设计框架深度调查报告

## 一、整体架构概览

系统提示词的完整生命周期涉及 **前端状态管理 → IPC桥接 → 后端分层组合 → LLM Provider发送** 四个环节。当前架构的核心矛盾是：**前端和后端各有一套独立且几乎互不相通的提示词机制**。

```
┌─────────────────────────────────────────────────────────────────┐
│  前端 (TypeScript/Jotai)                                        │
│  ┌──────────────────┐  ┌─────────────────────┐                 │
│  │ system-prompt-   │  │ PromptEditorSidebar │                 │
│  │ atoms.ts (状态)  │  │ (CRUD UI)           │                 │
│  └────────┬─────────┘  └──────────┬──────────┘                 │
│           │                       │                             │
│           │  tauri-bridge.ts      │ invoke()                    │
│           │  ┌────────────────────┴─────────┐                   │
│           │  │ getSystemPromptConfig()      │                   │
│           │  │ createSystemPrompt()         │  ← 全部 .catch()  │
│           │  │ updateSystemPrompt()         │     后端无实现     │
│           │  │ deleteSystemPrompt()         │                   │
│           │  │ setDefaultPrompt()           │                   │
│           │  └──────────────────────────────┘                   │
├───────────┼─────────────────────────────────────────────────────┤
│  后端 (Rust)                                                     │
│           │                                                     │
│  ┌────────┴──────────────────────────────────────────┐          │
│  │               ChatDelegate                         │          │
│  │                                                    │          │
│  │  system_prompt (hardcoded string - 构造时传入)      │          │
│  │       │                                            │          │
│  │       ▼                                            │          │
│  │  effective_system_prompt()                          │          │
│  │       │ 1. user base (system_prompt字段)            │          │
│  │       │ 2. memory_context (召回引擎)                │          │
│  │       ├── compose_system_prompt() ───┐              │          │
│  │       │   3. uclaw.md (工作区级)      │              │          │
│  │       │   4. [WORKSPACE] 路径块      │              │          │
│  │       │   5. KARPATHY_BASELINE       │  mode_prompts│          │
│  │       │   6. mode_addition (安全模式) │  .rs         │          │
│  │       └── 7. skills_manifest         │              │          │
│  │       ▼                                            │          │
│  │  call_llm()                                        │          │
│  │       │ + build_system_time_block()                │          │
│  │       │ + build_dynamic_context() (workspace root)  │          │
│  │       ▼                                            │          │
│  │  LLM Provider (Anthropic/OpenAI)                   │          │
│  └────────────────────────────────────────────────────┘          │
└─────────────────────────────────────────────────────────────────┘
```

---

## 二、前端状态管理 (`system-prompt-atoms.ts`)

### 2.1 状态架构

| Atom | 类型 | 持久化 | 用途 |
|------|------|--------|------|
| `promptConfigAtom` | `atom<SystemPromptConfig>` | ❌ 内存 | 完整配置（提示词列表+默认ID+追加开关） |
| `selectedPromptIdAtom` | `atomWithStorage<string>` | ✅ localStorage | 用户选中的提示词ID |
| `promptSidebarOpenAtom` | `atom<boolean>` | ❌ 内存 | 侧栏开/关 |
| `conversationPromptIdAtom` | `atom<Map<string, string>>` | ❌ 内存 | 每个对话独立的提示词选择 |

**派生只读状态**：
- `promptListAtom` — 从 config 派生提示词列表
- `defaultPromptIdAtom` — 从 config 派生默认ID
- `selectedPromptAtom` — 从 config + selectedId 派生当前提示词对象
- `resolvedSystemMessageAtom` — 派生最终 system message（含时间+用户名追加）

### 2.2 前端与后端交互机制

前端通过 `tauri-bridge.ts` 定义了以下IPC调用：

| 函数 | IPC命令 | 后端实现 |
|------|---------|----------|
| `getSystemPromptConfig()` | `get_system_prompt_config` | **❌ 不存在** |
| `createSystemPrompt()` | `create_system_prompt` | **❌ 不存在** |
| `updateSystemPrompt()` | `update_system_prompt` | **❌ 不存在** |
| `deleteSystemPrompt()` | `delete_system_prompt` | **❌ 不存在** |
| `setDefaultPrompt()` | `set_default_prompt` | **❌ 不存在** |
| `updateAppendSetting()` | `update_append_setting` | **❌ 不存在** |

**所有6个IPC命令均无后端实现**，全部通过 `.catch()` 静默降级为客户端本地 mock 值。这意味着：

1. `PromptEditorSidebar` 中创建/编辑/删除提示词的 UI 操作 **完全无效**
2. `SystemPromptSelector` 中的懒加载 `getSystemPromptConfig()` 永远返回 `{ prompts: [] }`
3. 追加日期时间开关切换后 **无任何后端效果**
4. 提示词持久化机制 **完全是客户端内存状态**，刷新即丢失

### 2.3 前端死代码

- **`resolvedSystemMessageAtom`**（第47行）：定义但从未被任何组件导入或消费
- **`resolveSystemMessage()`**（第77行）：在 `ChatView.tsx` 中被 import 但从未调用
- **`appendDateTimeAndUserName`**（第54行）：前端状态存在但后端不响应，实际时间注入已在后端 `build_system_time_block()` 中单独实现

---

## 三、后端提示词分层组合机制

### 3.1 两个独立的系统提示词入口

后端存在 **两个互不相通的系统提示词入口**，分别服务于 Chat 和 Agent 路径：

| 路径 | 入口函数 | 系统提示词来源 | 代码位置 |
|------|---------|--------------|----------|
| **Chat** | `send_message()` | `get_system_prompt()` — 约25行硬编码字符串 | `tauri_commands.rs:2005` |
| **Agent** | `send_agent_message()` | `AGENT_SYSTEM_PROMPT` — 约2行硬编码常量 | `tauri_commands.rs:5713` |

两者内容完全独立，Chat 版本更详细（含工具说明和行为指南），Agent 版本更简洁。**两者都不支持用户自定义**。

### 3.2 提示词分层组合顺序 (`mode_prompts.rs`)

`compose_system_prompt()` 按以下顺序逐层拼接，层间用 `\n\n---\n\n` 分隔：

```
第1层: user_global_base (硬编码字符串，非用户可配)
     ↓
第2层: workspace/uclaw.md (工作区级项目描述，每次调用实时读取)
     ↓
第3层: [WORKSPACE] 路径块 (告知 Agent 当前工作目录)
     ↓
第4层: KARPATHY_BASELINE (编译时嵌入的基线行为护栏)
     ↓
第5层: mode_addition (由 SafetyMode 决定：Ask/Plan/AcceptEdits/Bypass/Supervised)
```

**优先级/覆盖关系**：
- 按 LLM 提示词工程惯例，**越靠后优先级越高**（后面内容对模型行为约束力更强）
- KARPATHY_BASELINE 和 mode_addition 在最后，能覆盖用户 base 和 uclaw.md 中的行为设置
- 空层自动跳过，不产生多余分隔符

### 3.3 effective_system_prompt() 增强层

在 `compose_system_prompt()` 之前/之后，`dispatcher.rs:153` 的 `effective_system_prompt()` 额外处理：

**前置处理**（在 compose 之前）：
- `memory_context`（记忆召回引擎结果）拼接到 user base 之后

**后置处理**（在 compose 之后）：
- `skills_manifest_block`（技能清单，约800 tokens）追加到末尾
- 智能抑制：一旦 Agent 在循环中调用过 `skill_search`，后续调用自动跳过 manifest（节省 ~800 tokens/轮）

### 3.4 call_llm() 中的时间注入

```
effective_system_prompt()
       ↓
  + build_system_time_block()  ← 当前时间 + 中文星期 + "不要使用bash date"
       ↓
  = full_system_prompt
       ↓
  作为第一条 ChatMessage::system() 发送
```

时间块格式：
```
<system_info>
当前时间: 2026年5月16日 周六 15:30
注意: 以上时间由系统提供，你不需要使用工具（如 bash date）获取时间，直接使用此信息回答即可。
工作区路径: /Users/ryanliu/Documents/uclaw
</system_info>
```

每次 `call_llm()` 调用都重新生成时间戳（不持久化），确保 agent 循环中的多轮 LLM 调用都有准确时间。

---

## 四、UI 集成分析

### 4.1 PromptSettings 组件

位置：`ui/src/components/settings/PromptSettings.tsx`

当前状态：**占位组件（PLACEHOLDER）**

```typescript
const handleSave = () => {
    // [PLACEHOLDER - Tauri adaptation needed] Save custom system prompt
    setSaved(true)
    setTimeout(() => setSaved(false), 2000)
}
```

**功能**：显示一个 TextArea 让用户输入自定义系统提示词，但"保存"按钮仅仅设置 `saved=true` 2秒后恢复，**没有任何 IPC 调用或实际持久化**。

### 4.2 PromptEditorSidebar 组件

位置：`ui/src/components/chat/PromptEditorSidebar.tsx`

功能完备的提示词管理 UI：
- ✅ 新建/删除提示词
- ✅ 内联编辑名称和内容（500ms 防抖保存）
- ✅ 设为默认（星标）
- ✅ 追加日期时间/用户名开关
- ✅ 内置提示词只读保护

**但所有后端调用都失败**：`createSystemPrompt`, `updateSystemPrompt`, `deleteSystemPrompt` 等全部落到 `.catch()` 返回本地 mock 值，DB 无变化。

### 4.3 SystemPromptSelector 组件

位置：`ui/src/components/chat/SystemPromptSelector.tsx`

在 ChatHeader 中显示下拉选择器，支持：
- 选择当前对话使用的提示词
- 点击 "编辑提示词" 打开侧栏
- 懒加载配置（调用 `getSystemPromptConfig()` — 永远失败，返回空列表）

### 4.4 用户交互流程

```
用户操作                         实际效果
────────────────────────────────────────────────
在侧栏新建提示词          →  本地 state 更新, DB/后端无变化
编辑提示词内容            →  本地 state 更新, DB/后端无变化
删除提示词                →  本地 state 更新, DB/后端无变化
设为默认                  →  本地 state 更新, DB/后端无变化
选择不同提示词             →  conversationPromptIdAtom 更新
                           但 sendMessage 的 input 无 promptId 字段
                           后端 ChatDelegate 始终使用硬编码系统提示词
切换追加日期时间开关       →  本地 config 更新, 后端无变化
在 Settings 页面保存提示词  →  仅显示 "已保存" 2秒, 无实际动作
```

**结论：整套前端提示词管理功能是纯 UI 装饰（facade），无任何后端支撑。**

---

## 五、缺陷和优化机会

### 🔴 严重缺陷

**1. 前端提示词管理系统完全不可用**
- 6个IPC命令全部缺少后端实现
- 没有数据库表存储用户自定义提示词
- PromptEditorSidebar 的所有 CRUD 操作均静默失败
- `ChatView.sendInput` 不携带任何 promptId/systemMessage 字段
- `SendMessageInput` 结构体不包含 system_prompt 相关字段

**2. 系统提示词硬编码，用户不可配置**
- `get_system_prompt()` 返回25行硬编码字符串
- `AGENT_SYSTEM_PROMPT` 是编译时常量
- 无任何机制允许用户通过UI或配置文件修改

**3. Chat 和 Agent 路径使用不同的系统提示词**
- Chat: 详细，包含工具描述和行为指南
- Agent: 简洁，缺少很多 Chat 路径的 guardrails
- 两个版本独立维护，可能导致行为不一致

### 🟡 中等问题

**4. 时间注入存在两套机制**
- 后端 `build_system_time_block()` 注入到 system prompt（当前生效）
- 前端 `appendDateTimeAndUserName` 开关在 `resolvedSystemMessageAtom` 中（死代码，不生效）
- 两套机制容易让后续开发者困惑

**5. AutomationDelegate 缺少时间注入**
- `automation/runtime/execute.rs:95` 的 `call_llm()` 直接使用 `reason_ctx.system_prompt`
- 没有 `build_system_time_block()` 或类似机制
- 自动化场景下 Agent 可能不知道当前时间

**6. Proactive 场景缺少时间注入**
- `proactive/service.rs:734` 直接使用场景输出的 `system_prompt`
- 无时间上下文注入

**7. 前端死代码未清理**
- `resolvedSystemMessageAtom` — 定义但未使用
- `resolveSystemMessage()` — import 但未调用
- `updateAppendSetting()` — 调用但后端无实现
- `PromptSettings` — 占位组件，保存按钮无功能

### 🟢 优化机会

**8. 提示词性能优化空间**
- ✅ Anthropic prompt caching 已正确实现（system + tools 末尾标记 `cache_control: ephemeral`）
- ✅ 技能清单智能抑制（skill_search 后自动跳过，节省 ~800 tokens）
- ⚠️ 每次 `call_llm()` 都重新拼接完整 system prompt（`effective_system_prompt()` + `build_system_time_block()`），但内容高度重复，可以考虑缓存
- ⚠️ `uclaw.md` 每次调用都从磁盘读取，虽然有 OS 文件缓存，高频调用仍有开销

**9. CompletionConfig.system_prompt 字段冗余**
- `CompletionConfig` 有 `system_prompt: Option<String>` 字段
- 但 Anthropic 和 OpenAI provider 都从 `messages` 数组中提取 system 消息
- `config.system_prompt` 实际上被忽略（冗余字段）
- 同时存在两种传递方式增加混淆

**10. 安全性考虑**
- 当前系统提示词由后端完全控制，用户无法注入恶意提示词 — ✅ 安全
- 但如果开放用户自定义提示词，需要做内容过滤和长度限制
- uclaw.md 内容直接拼入系统提示词，应限制大小（当前无限制）

---

## 六、具体改进建议

### 6.1 短期修复（P0 - 立即执行）

#### 方案A：修复提示词管理系统（推荐）

**步骤1**：在数据库创建 `system_prompts` 表
```sql
CREATE TABLE system_prompts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    is_builtin INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
INSERT INTO system_prompts (id, name, content, is_builtin, sort_order, created_at, updated_at)
VALUES ('builtin-default', '默认', 'You are a helpful assistant.', 1, 0, 
        unixepoch() * 1000, unixepoch() * 1000);
```

**步骤2**：在 `tauri_commands.rs` 中实现6个缺失的IPC命令：
- `get_system_prompt_config` — 返回所有提示词 + 默认ID
- `create_system_prompt` — INSERT 新提示词
- `update_system_prompt` — UPDATE 提示词
- `delete_system_prompt` — DELETE 提示词（不可删除内置）
- `set_default_prompt` — 更新全局默认提示词ID
- `update_append_setting` — 更新追加设置

**步骤3**：将用户选择的提示词传递给后端
- `SendMessageInput` 添加 `prompt_id: Option<String>` 字段
- `ChatDelegate::new()` 接受 `prompt_id` 参数
- 修改 `send_message()` 读取用户提示词内容替换硬编码的 `get_system_prompt()`

**步骤4**：修复 `PromptSettings.tsx` 的保存按钮，连接真实IPC

#### 方案B：移除前端假功能（权宜之计）

如果短期无法实现后端，至少应该：
- 注释或隐藏 `PromptEditorSidebar` 的编辑/新建/删除按钮
- 在 `PromptSettings` 中标注 "Coming soon"
- 移除 `system-prompt-atoms.ts` 中的死代码

### 6.2 中期优化（P1）

**5. 统一 Chat/Agent 系统提示词**
- 提取公共基础提示词为常量
- Chat 和 Agent 路径共享相同的基础层，仅追加路径特定的指令

**6. 消除 CompletionConfig.system_prompt 冗余**
- 从 `CompletionConfig` 移除 `system_prompt` 字段
- 或改为强制使用 `config.system_prompt` 替代从 messages 中查找
- 选择一种方式并保持一致

**7. 为 AutomationDelegate 添加时间注入**
- 在 `automation/runtime/execute.rs:call_llm()` 中注入时间块
- 或让 `AutomationDelegate` 也使用 `ChatDelegate` 的基础设施

**8. 为 Proactive 场景添加时间注入**
- 在 `proactive/service.rs` 构建消息时注入当前时间

### 6.3 长期优化（P2）

**9. 系统提示词缓存**
- 对 `effective_system_prompt()` 结果做缓存（memoize）
- 失效条件：memory_context 变化、技能清单变化、安全模式变化、uclaw.md 修改
- `build_system_time_block()` 不应缓存（时间会变）

**10. uclaw.md 原子化**
- 添加文件 mtime 检查，仅在文件修改后重新读取
- 考虑限制 uclaw.md 大小（如 4096 字符），防止超大文件撑爆 token 预算

**11. 提示词模板系统**
- 支持 `{{user_name}}`、`{{current_time}}`、`{{workspace}}` 等变量
- 用户可在自定义提示词中使用这些变量

**12. 提示词版本控制**
- 支持提示词的版本历史和回滚
- 便于调试提示词变更对 Agent 行为的影响

---

## 七、总结

| 维度 | 当前状态 | 评分 |
|------|---------|------|
| 后端分层组合机制 | ✅ 设计良好，层次清晰 | ★★★★☆ |
| Anthropic 缓存优化 | ✅ 正确实现，节约成本 | ★★★★★ |
| 时间注入机制 | ✅ 已修复到 system prompt | ★★★★☆ |
| 技能清单智能抑制 | ✅ 巧妙优化 | ★★★★★ |
| 前端 CRUD 功能 | ❌ 纯UI装饰，完全不可用 | ☆☆☆☆☆ |
| 用户自定义提示词 | ❌ 硬编码，不支持 | ☆☆☆☆☆ |
| Chat/Agent 统一性 | ❌ 两套独立提示词 | ★★☆☆☆ |
| 前端死代码 | ❌ 多处未清理 | ★☆☆☆☆ |
| 自动化场景时间注入 | ❌ 缺失 | ★☆☆☆☆ |
| 提示词缓存 | ⚠️ 无缓存，每轮重新拼接 | ★★★☆☆ |

**核心结论**：后端提示词分层架构设计优秀，但前端管理功能完全是虚假的UI，且 Chat/Agent 路径使用独立硬编码提示词。**优先应实现后端 CRUD 并将前端选择结果实际传送到后端**，这是当前最大的功能缺口。
