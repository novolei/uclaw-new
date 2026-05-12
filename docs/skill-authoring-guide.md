# Skill 编写指南

uClaw 里 skill 是 agent 复用经验的最小单位——一段被命名、被检索、被注入到 LLM 上下文的可执行知识。本指南讲：什么样的 skill 值得写、怎么写、怎么验证它真的被召回了。

参考自 [mattpocock/skills](https://github.com/mattpocock/skills) 的 `write-a-skill`，叠加 uClaw 特有的 lifecycle、检索路径、和存储约束。

---

## 三种来源

uClaw 同时维护三类 skill，它们的写法、生命周期、和召回路径完全不同：

| 来源 | 存放位置 | 谁写 | lifecycle | 调用方式 |
|---|---|---|---|---|
| **静态注册** | `skills/<name>/SKILL.md` (项目内) 或 `~/.uclaw/skills/<name>/SKILL.md` | 你手工写 | 永远启用 | manifest 注入 + `/<name>` slash 命令 + 关键词/正则匹配 |
| **借用** | `skills/borrowed/<name>/SKILL.md` (从 mattpocock 等上游 vendor) | 上游写，本仓 vendor | 永远启用 | 同上 |
| **学得** | SQLite `memory_nodes` (kind=Procedure, skill_type=learned) | 后台 LLM 自动抽取 | `draft` → `promoted` → `deprecated` | `skill_search` 工具 + manifest top-30 (仅 promoted) |

学得 skill 的生命周期是本指南最容易踩坑的部分，单独成节（见 [Lifecycle](#lifecycle-学得-skill-的三阶段)）。

---

## SKILL.md 结构 (静态 / 借用)

```
skills/your-skill/
└── SKILL.md          # 必需。frontmatter 是 manifest，正文是 prompt
```

最小可解析的 SKILL.md：

```markdown
---
name: skill-name
description: 一句话说清能力 + 何时触发。≤1024 字，第三人称，含 "用于 X，当 Y 时"
---

# Skill Name

## Quick start

[最小可运行示例]

## Workflows

[复杂场景的步骤清单]
```

uClaw `SkillManifest` 实际识别的字段（`src-tauri/src/skills.rs` line 168）：

```yaml
---
name: skill-name           # 必需。kebab-case；slash 命令就是 /<name>
version: 0.1.0             # 默认 "0.1.0"
description: ...           # 见下节《Description 是一切》
author: ""
enabled: true              # 默认 true
category: ""               # 自由字符串，UI 分组用
activation:                # 可选；不写也能靠 slash 命令被显式调用
  keywords: ["..."]        # 上限见 MAX_KEYWORDS_PER_SKILL
  exclude_keywords: ["..."] # 任一命中即否决本 skill
  patterns: ["..."]        # 正则；编译失败会被静默丢弃
  tags: ["..."]
  max_context_tokens: ...  # 默认 DEFAULT_MAX_CONTEXT_TOKENS
parameters: []             # 若 skill 暴露工具调用入参
requires: []               # 依赖的其他 skill name
tools: []                  # 若 skill 提供 shell/HTTP 工具
---
```

> ⚠️ **不要塞额外字段**：未识别的 frontmatter 字段会被 serde 默默丢弃，看起来一切正常但运行时不生效。

---

## Description 是一切

`description` 是 agent 决定要不要 load 本 skill 的**唯一依据**。所有已注册 skill 的 description 会被并列展示在系统 prompt 里，router 只看这一句。

### 硬约束

- **第三人称**："Extract …" / "用于校验 …"，不要 "I help you …"。
- **首句说能力**，第二句以 `Use when …` / `当 X 时触发` 结构给触发条件。
- **学得 skill 的 description ≤120 字符**（由 extraction prompt 强制并在 `truncate_to_char_count` 软截断；静态 skill 上限 1024 字符）。
- 含具体关键词、错误消息、文件类型 —— 给 router 一个抓手。

### 对照

```
✅ Extract text and tables from PDF files, fill forms, merge documents. Use when working with PDF files or when user mentions PDFs, forms, or document extraction.

✅ 跨源校验股票财报，当 Yahoo Finance 返回 403 或 API key 失效时切换数据源。

❌ Helps with documents.
   理由：router 无法把它和其他文档类 skill 区分开。

❌ 提供完整的股票财报研究方法学指南。
   理由：自吹自夸 + 内容自描述，没有触发条件。

❌ 我可以帮你处理 PDF。
   理由：第一人称，缺触发条件。
```

`description` 写糟糕 → skill 永远不会被召回 → 等于没写。这条规则比 SKILL.md 正文质量更重要。

---

## 反模式先行

我们的 [extraction prompt](../src-tauri/src/proactive/scenarios/skill_extraction.rs) 显式列了 "看到这些请放弃当前 skill 抽取" 的反模式。手写 skill 时同样适用：

- **复述大模型常识**："多用 try-catch" / "写好测试" / "保持代码简洁" —— LLM 默认会做的事不是 skill。
- **单次成功的偶然事件**：一次性的 patch、配置改动、debug 输出 —— 下次不会再用。
- **过度泛化**："勤总结" / "及时复盘" —— 没有具体操作步骤就不是 skill。
- **重复修复同一个 bug 当作新技能**：3 次失败地处理同一个 API key 问题 = 1 个 skill，不是 3 个。
- **`description` 写成 `Helps with X` / `提供 X 的指南`** —— 违反触发条件硬约束。

写 `<anti_patterns>` 字段是高 ROI 动作：明确告诉 agent "执行本 skill 时**绝对不要做**" 的事，比堆原则更能避免回归。例：

> ❌ 不要在 401 时立即重试同一 endpoint；先检查 token 是否过期。
> ❌ 不要假设单一数据源是权威；至少跨源校验一次。

---

## 学得 skill 的内部结构

学得 skill 不写 SKILL.md，而是由后台 [extraction scenario](../src-tauri/src/proactive/scenarios/skill_extraction.rs) 从执行日志里抽出 `<skill_report>` XML，落到 `memory_nodes` 表。手工评审时按以下结构看：

```xml
<skill>
  <name>kebab-case-skill-name</name>
  <description>一句话，≤120 字符，第三人称，含 "用于 X，当 Y 时"</description>
  <context>简短描述适用场景（agent 调用前的状况）</context>
  <principles>核心原则，2-4 句</principles>
  <steps>实现步骤（编号或 markdown 列表，可执行的具体动作）</steps>
  <anti_patterns>执行时绝对不要做的事</anti_patterns>
  <pitfalls>常见陷阱（已观察 / 可预见的失败方式）</pitfalls>
  <signals>
    <signal>触发短语 / 错误关键词</signal>
    <signal>...</signal>
  </signals>
  <validation_hint>应用后如何在不依赖人工的情况下验证它真的有效（一句话）</validation_hint>
  <category>repair | optimize | innovate</category>
</skill>
```

字段映射到存储的 metadata JSON：`context`、`principles`、`steps`、`pitfalls`、`signals`、`signals_seen`、`validation_hint`、`category`、`anti_patterns`、`description`。空字段在 upgrade 路径上保留旧值（不会被空字符串覆盖）。

---

## Lifecycle (学得 skill 的三阶段)

PR #117 引入：

| 阶段 | 含义 | 进入 manifest top-30 | `skill_search` 可见 | 怎么转换 |
|---|---|---|---|---|
| `draft` | 刚抽取、未经使用验证 | ❌ | ✅ | 新抽取默认；手动 `set_skill_lifecycle` |
| `promoted` | `cited_count ≥ 3` 自动升级，或手动升 | ✅ | ✅ | `record_skill_cited` 跨阈值时翻转 |
| `deprecated` | 手动退役 | ❌ | ✅ (附 warning) | `set_skill_lifecycle` |

`PROMOTION_THRESHOLD = 3` 定义在 `tauri_commands.rs::record_skill_cited`。

**Pre-PR 117 抽出的旧记录** 没有 `lifecycle` 字段，SQL `COALESCE(..., 'promoted')` 默认当 `promoted` 处理，不需要 migration。

### 为什么要 draft 阶段

学得 skill 的召回质量取决于 description 是否精准。新抽取的 skill 没经过任何 agent 实际使用，注入到 manifest 会冲淡更可靠的 promoted skill。draft 阶段强制 "先在 search 里被找到、被引用 3 次、再进 manifest"——召回数据是免费的质量信号，不浪费。

---

## 工作流：从想法到上线

### A) 写一个静态 skill

1. **确认它够格**：跑一遍下方[审查清单](#审查清单)，特别是 "是否复述了大模型常识"。
2. **选位置**：通用工具放 `skills/<name>/`；个人工作流放 `~/.uclaw/skills/<name>/`。
3. **起 SKILL.md**：先写 frontmatter，重点是 `description`（≤1024 字符，含触发条件）。再写正文，控制在 100 行内；超出就拆 `REFERENCE.md`。
4. **本地验证**：
   ```bash
   cd src-tauri && cargo test --lib skills::
   ```
   重启 uClaw，在 Agent 模式输入 `/<your-skill-name>`，确认匹配。也用 `description` 里的触发关键词测自动召回。

### B) 借用上游 skill

1. 在 `skills/borrowed/<name>/SKILL.md` vendor 上游内容。
2. 更新 `skills/borrowed/ATTRIBUTION.md`：上游 commit SHA、license（必须 MIT/Apache 兼容）、目的。
3. **不要本地改正文**——重新 vendor 时会覆盖。要 patch 就放到独立的 `skills/<name>/SKILL.md` 引用 borrowed 版本。

### C) 学得 skill 的人工评审

后台 extraction 跑完后，进 Settings → 已学技能：

1. **看 description**：触发条件清楚吗？没有就直接 `set_skill_lifecycle = deprecated`。
2. **看 anti_patterns**：是真实失败的总结，还是脑补？
3. **看 signals**：和 description 里的触发条件呼应吗？没呼应说明 router 命中率会很低。
4. **看 validation_hint**：能自动验证吗？不能就警惕——agent 自己不知道何时该收手。
5. 通过 → 留 `draft`，等使用累计；不通过 → `deprecated`。

---

## 调用方式（PR #117 已落地 / PR #118 即将落地）

agent 召回 skill 有四条路径：

1. **Slash 命令**：`/<skill-name>` 在用户输入开头时，`SkillsRegistry::match_slash_command` 直接命中并把 prompt 注入到 system message。学得 skill 的 slash 命令支持在 PR #118 (Layer B-3) 落地。
2. **关键词 / 正则匹配**：`SkillsRegistry::match_skills` 根据 `activation` 字段打分。
3. **Manifest 注入**：每次 agent loop 开始前 `skills_manifest::build_active_skills` 选 top-30 (promoted-only) 注入到 system prompt。
4. **`skill_search` 工具**：agent 主动调用，按语义+关键词搜索，包含所有 lifecycle 阶段，draft/deprecated 在结果里带 `warnings[]` 标记（warnings 落地见 PR #118 follow-up）。

---

## 审查清单

写完 / 评审前逐条对照——任意一条不满足，重写或丢弃：

- [ ] **治一个具体的 agent 失败模式**（不是 "应该多用 X"）
- [ ] **`description` 含触发条件**："Use when …" / "当 X 时触发"，≤120 字符（学得）/ ≤1024 字符（静态）
- [ ] **第三人称写**，不要 "I help you …"
- [ ] **跟既有 skill 不重复**：明显主题/做法相似就不是新 skill，是同一个
- [ ] **能填出 2-3 条具体的 `steps`**（"建立工作流" 不算具体）
- [ ] **`anti_patterns` 来自真实观察**，不是脑补
- [ ] **没有时效性信息**（不要 "截至 2025/03 …"）
- [ ] **术语前后一致**
- [ ] **SKILL.md 正文 ≤100 行**；超出拆 `REFERENCE.md`
- [ ] **本地跑 `cargo test --lib skills::` 通过**

---

## 引用

- mattpocock/skills 上游：[github.com/mattpocock/skills](https://github.com/mattpocock/skills)
- 借用 skill 列表 + 出处：`skills/borrowed/ATTRIBUTION.md`
- 抽取 prompt：`src-tauri/src/proactive/scenarios/skill_extraction.rs:23`
- Manifest 注入路径：`src-tauri/src/skills_manifest.rs`
- Lifecycle 实现：`src-tauri/src/tauri_commands.rs::record_skill_cited` (auto-promote) / `::set_skill_lifecycle` (manual)
- 数据落点：`memory_nodes.metadata_json` (skill_type=learned)
