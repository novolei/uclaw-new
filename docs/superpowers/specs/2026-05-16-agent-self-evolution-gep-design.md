# Agent 自进化引擎深度重构 — GEP 化设计

**Date:** 2026-05-16
**Status:** Draft
**Inspired by:** EvoMap/evolver (arXiv:2604.15097), GEP Protocol v1.5.0
**Builds on:** self_eval tool, ProactiveService, SkillExtractionScenario, MemoryGraph

---

## 1. Background and Problem Statement

### 1.1 核心发现：UClaw 的进化产出物形式与最优控制信号背道而驰

EvoMap/GEP 背后的核心论文 **"From Procedural Skills to Strategy Genes"**（4,590 次对照试验，45 个科学代码求解场景）得出以下定量结论：

| 经验表示 | 典型大小 | 对基准性能的影响 | 设计目标 |
|---|---|---|---|
| **Skill**（文档导向） | ~2,500 tokens | **-1.1pp**（退化） | 人类可读、文档完备 |
| **Gene**（控制导向） | ~230 tokens | **+3.0pp**（提升） | LLM 面向、控制信号密集 |

UClaw 当前 `SkillExtractionScenario` 的输出格式包含 11 个字段（name, description, context, principles, steps, anti_patterns, pitfalls, signals, validation_hint, category, tags）——是典型的**文档导向型 Skill 包**，即论文证明会降低 LLM 测试时表现的那种格式。

### 1.2 三个死数据回路

前序分析确认了 self_eval 工具的三个数据断路：

1. **DB 写入无读取**：`session_evals` 表只 INSERT，从未 SELECT
2. **无系统提示触发**：LLM 不知道应该调用 self_eval
3. **SkillLearned 事件无订阅者**：infra 发布后无人消费

但这些只是**症状**。根因是更深刻的：整个进化产出物（文档型 Skill）从设计理念上就跑偏了。

### 1.3 OKR

| Objective | Key Results |
|---|---|
| O1: 将进化产出物从文档型 Skill 转变为控制型 Gene | KR1: Gene 六元组替代 Skill 十一字段，token 压缩 70%+ |
| | KR2: 新增 Capsule 执行结果记录，形成 Gene→Capsule 成对闭环 |
| O2: 激活 self_eval 数据回路 | KR3: self_eval learnings 进入 ProactiveService gene_candidates 池 |
| | KR4: session_evals 数据被定期读取分析，不再死数据 |
| O3: 实现可审计的进化链 | KR5: 每次 Gene 应用产生 EvolutionEvent 事件记录 |
| | KR6: 引入 blast_radius 计算和 git 回滚能力 |

---

## 2. Evolver/GEP 核心理论摘要

### 2.1 Gene（策略基因）六元组

GEP 定义的 Gene 结构化表示为：

\[
g = (m, u, \pi, \alpha, c, v)
\]

| 字段 | 含义 | GEP JSON 键 | Token 预算 |
|---|---|---|---|
| **m** | 任务匹配信号（触发关键词/错误模式） | `signals_match` | ~30 |
| **u** | 紧凑摘要（一句话说清策略） | `summary` | ~40 |
| **π** | 战略步骤（3-4 步可执行动作） | `strategy` | ~80 |
| **α** | AVOID 提示（失败感知的"不可做"） | (implied in strategy) | ~40 |
| **c** | 执行约束（max_files, forbidden_paths） | `constraints` | ~20 |
| **v** | 验证钩子（可执行验证命令） | `validation` | ~20 |

总 token 预算 ≈ 230 tokens。对比 UClaw Skill 的 600-800 tokens，压缩率达 65-70%。

### 2.2 Capsule（执行胶囊）

Gene（策略）与 Capsule（执行结果）必须成对存在：

| 字段 | 含义 |
|---|---|
| `trigger` | 触发信号数组 |
| `gene` | 关联 Gene 的 SHA-256 asset_id |
| `summary` | 修复描述 |
| `confidence` | 置信度 0-1 |
| `blast_radius` | 影响范围 {files, lines} |
| `outcome` | 结果 {status, score} |
| `success_streak` | 连续成功次数 |
| `env_fingerprint` | 环境指纹 {rust_version, platform, arch} |

### 2.3 关键实验发现与 UClaw 的直接启示

| 论文发现 | UClaw 现状 | 改进方向 |
|---|---|---|
| **信息过载效应**：Skill 的额外文档降低性能 (-1.1pp) | Skill extraction 产出 11 字段完整文档 | 压缩为 Gene 六元组 |
| **打包效应**：重新附加文档材料会削弱 Gene | principles + context 稀释了核心信号 | 只保留 \(m, u, \pi, \alpha, c, v\) |
| **选择性积累**：失败信息蒸馏为紧凑警告最有效 | failure_lessons 作为独立块 | 蒸馏为 α（AVOID cues），≤3 条 |
| **边界复用**：太大或太小的经验包都无效 | 无适用范围概念 | 添加 signals_match + preconditions |
| **结构性鲁棒**：Gene 在结构扰动下保持竞争力 | 未测量 | 后续可引入结构扰动测试 |
| **"tokens rise then fall"**：推理被压缩为可复用基因 | 无压缩迹象 | Gene 蒸馏天然引入压缩 |

### 2.4 GEP vs MCP vs Skill 定位

| 协议 | 核心问题 | 类比 |
|---|---|---|
| MCP | 有什么工具可用？ | "这里有锤子和螺丝刀" |
| Skill | 怎么用这些工具？ | "用锤子钉钉子，步骤如下…" |
| **GEP** | **为什么这是最优解？** | "经过 100 次试验淘汰，这是验证过的最优方案，附带审计报告" |

---

## 3. 整体架构设计

```
                        ┌──────────────────────────────────────────┐
                        │          UClaw Agent Session              │
                        │                                          │
                        │  Think → Act → Observe                   │
                        │     │                    │               │
                        │     ▼                    ▼               │
                        │  self_eval()      ExecutionLog           │
                        │  (score,            (tool, input,         │
                        │   reasoning,         output, success,      │
                        │   learnings,         blast_radius)         │
                        │   blast_radius)                           │
                        └──────┬─────────────────┬─────────────────┘
                               │                 │
                               ▼                 ▼
                        ┌──────────────┐  ┌──────────────────────┐
                        │ session_evals│  │ InfraService          │
                        │ (DB table)   │  │ broadcast channel     │
                        └──────┬───────┘  └──────────┬───────────┘
                               │                     │
                               │    ┌────────────────┘
                               │    │
                               ▼    ▼
                        ┌──────────────────────────────────────────┐
                        │       ProactiveService                    │
                        │                                          │
                        │  context_listener                         │
                        │    ├─ MessageIncoming → window            │
                        │    ├─ MessageOutgoing → window            │
                        │    └─ SkillLearned    → [NEW] gene pool   │
                        │                                          │
                        │  tick_loop → ScenarioManager              │
                        │    ├─ ConversationLearning                │
                        │    ├─ SkillExtraction                     │
                        │    │    [REFACTOR] → GeneDistillation     │
                        │    ├─ MultimodalContext                   │
                        │    └─ [NEW] GeneEvolutionScenario        │
                        │                                          │
                        │  Gene Repository (assets/gep/)            │
                        │    ├─ repair/    (修复类 Gene)            │
                        │    ├─ optimize/  (优化类 Gene)            │
                        │    └─ innovate/  (创新类 Gene)            │
                        │    └─ capsules/  (每个 Gene 的 Capsule 历史) │
                        │    └─ events/    (EvolutionEvent 审计记录)  │
                        └──────────────────────────────────────────┘
```

---

## 4. 详细设计

### 4.1 P0-1: 改造 SkillExtraction → GeneDistillation（核心重构）

**目标**：将进化产出物从 11 字段文档型 Skill 改为 Gene 六元组 + Capsule 记录。

#### 4.1.1 Gene 输出格式定义

当前 SkillExtraction 输出（11 字段）：
```xml
<skill>
<name>kebab-case-skill-name</name>
<description>一句话，≤120 字符</description>
<context>适用场景描述</context>
<principles>核心原则 2-4 句</principles>
<steps>实现步骤</steps>
<anti_patterns>反模式</anti_patterns>
<pitfalls>常见陷阱</pitfalls>
<signals><signal>触发关键词</signal></signals>
<validation_hint>验证方法</validation_hint>
<category>repair|optimize|innovate</category>
<tags><tag>领域标签</tag></tags>
</skill>
```

新的 Gene 输出格式（六元组，~230 tokens）：
```xml
<gene>
<id>gene_stock_cross_validation</id>
<category>repair</category>
<signals_match>403, API key invalid, 数据源不可用</signals_match>
<summary>跨源校验股票财报，当 Yahoo 返回 403 时切换数据源</summary>
<strategy>
<step>识别失败的数据源 API 调用</step>
<step>从配置的备用源列表中选择下一个可用源</step>
<step>用新源重新发起请求，验证返回数据格式一致性</step>
</strategy>
<avoid>不要在 401 时立即重试同一 endpoint；不要假设单一数据源是权威</avoid>
<constraints>
<max_files>3</max_files>
<forbidden_paths>.env, secrets/</forbidden_paths>
</constraints>
<validation>验证切换后数据 schema 与原始源一致</validation>
</gene>
```

**Token 预算分析**：
- signals_match: ~30 tokens
- summary: ~40 tokens  
- strategy (3 steps): ~80 tokens
- avoid (2 cues): ~40 tokens
- constraints: ~20 tokens
- validation: ~20 tokens
- **总计: ~230 tokens**（vs 当前 ~600-800 tokens）

#### 4.1.2 改动的系统提示

需要修改 `skill_extraction.rs` 中的 `SKILL_EXTRACTION_SYSTEM_PROMPT` 常量，关键改动：

1. **标题**：从"抽取一个新的可复用 skill"改为"蒸馏一个新的控制型 Gene"
2. **输出格式**：从 11 字段 Skill XML 改为 6 字段 Gene XML
3. **去文档化约束**：
   - 砍掉 `<context>`（适用场景）、`<principles>`（世界观）、`<pitfalls>`（常见陷阱）、`<tags>`（领域标签）
   - `<avoid>` 合并了原 `<anti_patterns>` 和 `<pitfalls>` 的最高信号部分
4. **新增硬约束**：
   - `summary` ≤ 60 字符（vs 原 description 120 字符）
   - `strategy` ≤ 4 步（论文证明超出则信号稀释）
   - `avoid` ≤ 3 条（失败信息蒸馏）
   - `constraints` 必须包含 `max_files` 和 `forbidden_paths`

#### 4.1.3 MemoryGraph 存储适配

当前 skill 以 `memory_node` 形式存储在 MemoryGraph 中。Gene 需要新的存储结构：

```rust
struct GeneNode {
    id: String,                    // "gene_stock_cross_validation"
    category: GeneCategory,        // repair | optimize | innovate
    signals_match: Vec<String>,    // 触发关键词
    summary: String,               // 紧凑摘要 (≤60 chars)
    strategy: Vec<String>,         // 战略步骤 (≤4)
    avoid: Vec<String>,            // AVOID cues (≤3)
    constraints: GeneConstraints,  // {max_files, forbidden_paths}
    validation: String,           // 验证描述
    asset_id: String,             // SHA-256 hash
    created_at: i64,
    updated_at: i64,
    success_streak: u32,
}

struct GeneConstraints {
    max_files: u32,
    forbidden_paths: Vec<String>,
}
```

#### 4.1.4 Capsule 生成时机

当 Agent 应用一个 Gene 完成修复后，需要生成 Capsule。触发时机：

1. **self_eval 回调后自动生成**：Agent 调用 self_eval 时，如果 score ≥ 0.5 且当前 session 应用了某个 Gene，自动生成 Capsule
2. **GeneEvolutionScenario 中生成**：蒸馏出新 Gene 时，同时从候选 learnings 中生成初始 Capsule

Capsule 结构：
```rust
struct Capsule {
    id: String,
    gene_asset_id: String,         // 关联 Gene
    trigger: Vec<String>,          // 触发信号
    summary: String,               // 修复描述
    confidence: f32,               // 置信度 0-1
    blast_radius: BlastRadius,     // 影响范围
    outcome: CapsuleOutcome,       // 结果
    success_streak: u32,
    env_fingerprint: EnvFingerprint,
    created_at: i64,
}

struct BlastRadius {
    files: u32,
    lines: u32,
}

struct CapsuleOutcome {
    status: OutcomeStatus,  // success | partial | failed
    score: f32,
}

struct EnvFingerprint {
    rust_version: String,
    platform: String,
    arch: String,
}
```

---

### 4.2 P0-2: 打通 self_eval → ProactiveService 回路

**目标**：让 self_eval 的 learnings 进入 ProactiveService 的 gene_candidates 池，同时写 Capsule outcome。

#### 4.2.1 改动点：self_eval.rs

当前 `self_eval.rs` 在 DB 写入后发布 `SkillLearned` 事件，但无人消费。需要在 ProactiveService 侧增加消费逻辑。

**self_eval.rs 改动**（轻量）：在 `publish_skill_learned` 的 metadata 中附带更多上下文：

```rust
// Publish learnings with richer context for Capsule generation
if let Some(infra) = &self.infra_service {
    for learning in &learnings {
        infra.publish_skill_learned(
            "self_eval",
            learning,
            serde_json::json!({
                "session_id": self.session_id,
                "score": score,
                "reasoning": reasoning,
                "source": "self_eval",
                // [NEW] 附带执行上下文
                "capsule_hint": {
                    "trigger": [], // 由消费端从 reasoning 解析
                    "confidence": score,
                    "env_fingerprint": {
                        "rust_version": env!("CARGO_PKG_VERSION"),
                        "platform": std::env::consts::OS,
                        "arch": std::env::consts::ARCH,
                    }
                }
            }),
        ).await;
    }
}
```

#### 4.2.2 改动点：ProactiveService context_listener

在 `proactive/service.rs` 的 `start_context_listener` 中，增加对 `InfraEventType::SkillLearned` 的处理：

```rust
InfraEventType::SkillLearned { source, content, metadata } => {
    let mut pool = self.gene_candidate_pool.write().await;
    pool.push(GeneCandidate {
        source: source.clone(),
        content: content.clone(),
        score: metadata.get("score").and_then(|v| v.as_f64()),
        session_id: metadata.get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        reasoning: metadata.get("reasoning")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        timestamp: Utc::now(),
    });
    
    // 池满触发 GeneEvolutionScenario
    if pool.len() >= self.config.gene_distillation_threshold {
        self.new_gene_candidates.store(true, Ordering::Release);
    }
}
```

#### 4.2.3 新增数据结构：GeneCandidate

```rust
/// 待蒸馏的基因候选
#[derive(Debug, Clone)]
struct GeneCandidate {
    source: String,          // "self_eval"
    content: String,         // learning 原文
    score: Option<f64>,      // self_eval score
    session_id: Option<String>,
    reasoning: Option<String>,
    timestamp: DateTime<Utc>,
}
```

#### 4.2.4 改动点：ProactiveService 配置

在 `memubot_config.rs` 中增加相关配置：

```rust
pub struct ProactiveConfig {
    // ... existing fields ...
    
    /// Gene 蒸馏触发阈值（candidates 池达到多少条时触发蒸馏）
    pub gene_distillation_threshold: usize,  // default: 5
    
    /// Gene 蒸馏最小间隔（秒）
    pub gene_distillation_cooldown_secs: u64,  // default: 600 (10 min)
    
    /// 最大保留 Gene candidates 数
    pub max_gene_candidates: usize,  // default: 20
}
```

---

### 4.3 P1-1: 新增 GeneEvolutionScenario

**目标**：一个专门的 ProactiveScenario，定期从 gene_candidates 池中读取候选，通过 LLM 蒸馏为正式 Gene。

#### 4.3.1 触发条件

```rust
impl ProactiveScenario for GeneEvolutionScenario {
    async fn should_trigger(&self, ctx: &ScenarioContext) -> bool {
        let pool = self.gene_candidate_pool.read().await;
        
        // 条件 1: 候选池数量达标
        if pool.len() < self.config.gene_distillation_threshold {
            return false;
        }
        
        // 条件 2: 距离上次触发超过冷却时间
        if let Some(last) = ctx.last_trigger_at.get(self.name()) {
            if last.elapsed().as_secs() < self.config.gene_distillation_cooldown_secs {
                return false;
            }
        }
        
        true
    }
}
```

#### 4.3.2 Context 构建

```rust
async fn build_context(&self, ctx: &ScenarioContext) -> anyhow::Result<ScenarioOutput> {
    let pool = self.gene_candidate_pool.read().await;
    
    // 格式化候选为 LLM 可读文本
    let candidates_text = pool.iter()
        .map(|c| format!(
            "- [score={:.2}] {} (from {})",
            c.score.unwrap_or(0.0),
            c.content,
            c.source
        ))
        .collect::<Vec<_>>()
        .join("\n");
    
    // 获取已有 Gene 指纹（用于去重）
    let existing_fingerprints = self.get_existing_gene_fingerprints().await;
    
    Ok(ScenarioOutput {
        scenario_name: self.name().to_string(),
        system_prompt: GENE_DISTILLATION_PROMPT.to_string(),
        context_messages: vec![
            ("candidates".to_string(), candidates_text),
            ("existing_genes".to_string(), existing_fingerprints.join("\n")),
        ],
        memory_types: vec!["gene".to_string()],
        additional_instructions: None,
    })
}
```

#### 4.3.3 蒸馏 Prompt 设计

```
你是一个基因蒸馏器，将 Agent 的自我评估 learnings 蒸馏为紧凑的 GEP Gene。

## 核心原则

1. **紧凑优于完备**：Gene 总共不超过 250 tokens。不是缩短的 skill，是不同的抽象。
2. **AVOID cues 是最高价值信号**：从失败学到的比从成功复述的更有价值。
3. **可执行 > 可读**：Gene 是给 LLM 推理用的控制信号，不是人类文档。

## 输出格式

<gene>
<id>kebab-case-gene-id</id>
<category>repair|optimize|innovate</category>
<signals_match>逗号分隔的触发关键词/错误模式</signals_match>
<summary>一句话策略描述，≤60字符</summary>
<strategy>
<step>具体可执行步骤1</step>
<step>具体可执行步骤2</step>
<step>具体可执行步骤3</step>
</strategy>
<avoid>失败感知的不可做提示，≤3条</avoid>
<constraints>
<max_files>最大修改文件数</max_files>
<forbidden_paths>禁止修改路径，逗号分隔</forbidden_paths>
</constraints>
<validation>如何验证该 Gene 有效</validation>
</gene>

## 反模式（看到这些放弃蒸馏）

- learnings 太泛（"应该多注意细节"）→ 不是 Gene
- 已有 Gene 已覆盖 → 标记为 DUPLICATE
- 单次偶然成功事件 → 不是可复用策略

## 输出前自审

- [ ] summary ≤ 60 字符
- [ ] strategy ≤ 4 步
- [ ] avoid ≤ 3 条，且来自真实失败
- [ ] constraints 有具体数值
- [ ] 与已有 Gene 不重复

如果没有可蒸馏的 Gene，返回 [NO_GENE]。
```

#### 4.3.4 蒸馏后处理

```rust
async fn handle_distillation_result(&self, gene: Gene) -> anyhow::Result<()> {
    // 1. 计算 SHA-256 asset_id
    let asset_id = compute_gene_asset_id(&gene);
    
    // 2. 检查与已有 Gene 的重复度
    if let Some(existing) = self.find_duplicate_gene(&gene).await {
        // 更新已有 Gene 的 success_streak
        self.increment_success_streak(&existing.id).await;
        return Ok(());
    }
    
    // 3. 存储 Gene 到 assets/gep/<category>/
    self.store_gene(&gene, &asset_id).await?;
    
    // 4. 生成初始 Capsule
    let capsule = Capsule::from_gene_candidates(&gene, &self.drained_candidates);
    self.store_capsule(&capsule).await?;
    
    // 5. 写入 EvolutionEvent
    let event = EvolutionEvent {
        intent: gene.category.to_string(),
        capsule_id: capsule.id.clone(),
        genes_used: vec![asset_id],
        outcome: capsule.outcome.clone(),
        mutations_tried: self.drained_candidates.len() as u32,
        total_cycles: 1,
    };
    self.store_event(&event).await?;
    
    // 6. 清空已处理的 candidates
    self.gene_candidate_pool.write().await.clear();
    
    Ok(())
}
```

---

### 4.4 P1-2: Agent 注入 Gene 检索（test-time control）

**目标**：在每次 Agent 对话开始时，根据用户消息/上下文自动匹配适用的 Gene。

#### 4.4.1 检索时机

在 `dispatcher.rs` 或 `agentic_loop.rs` 的初始化阶段，在构建 system prompt 时：

```rust
// In agentic_loop::create_initial_context or dispatcher::build_system_prompt
let matched_genes = gene_repository.match_genes(
    &user_message,          // 用户消息
    &recent_tool_errors,    // 最近 N 个工具错误
    max_genes: 2,           // 最多注入 2 个 Gene
);

// 将匹配的 Gene 注入 system prompt
if !matched_genes.is_empty() {
    system_prompt.push_str("\n\n<active_genes>\n");
    for gene in &matched_genes {
        system_prompt.push_str(&gene.to_compact_prompt());
    }
    system_prompt.push_str("</active_genes>\n");
}
```

#### 4.4.2 匹配算法

```rust
fn match_genes(
    &self,
    user_message: &str,
    tool_errors: &[String],
    max_genes: usize,
) -> Vec<Gene> {
    let mut scored: Vec<(Gene, f64)> = self.genes.iter()
        .filter_map(|gene| {
            let score = self.compute_match_score(gene, user_message, tool_errors);
            if score > 0.0 { Some((gene.clone(), score)) } else { None }
        })
        .collect();
    
    // 按 (match_score × log(success_streak + 1)) 排序
    scored.sort_by(|a, b| {
        let score_a = a.1 * (a.0.success_streak as f64 + 1.0).ln();
        let score_b = b.1 * (b.0.success_streak as f64 + 1.0).ln();
        score_b.partial_cmp(&score_a).unwrap()
    });
    
    // 去重：同一 category 最多保留 1 个
    let mut seen_categories = HashSet::new();
    scored.into_iter()
        .filter(|(gene, _)| seen_categories.insert(gene.category.clone()))
        .take(max_genes)
        .map(|(gene, _)| gene)
        .collect()
}

fn compute_match_score(&self, gene: &Gene, msg: &str, errors: &[String]) -> f64 {
    let mut score = 0.0;
    let lower_msg = msg.to_lowercase();
    
    for signal in &gene.signals_match {
        let lower_signal = signal.to_lowercase();
        if lower_msg.contains(&lower_signal) {
            score += 1.0;
        }
        for err in errors {
            if err.to_lowercase().contains(&lower_signal) {
                score += 2.0; // 错误匹配权重更高
            }
        }
    }
    
    score
}
```

#### 4.4.3 Gene 的紧凑提示格式

```rust
impl Gene {
    fn to_compact_prompt(&self) -> String {
        format!(
            "GENE[{id}] ({category}): {summary}\n  Strategy: {strategy}\n  AVOID: {avoid}\n  Constraints: max {max_files} files, no {forbidden_paths}",
            id = self.id,
            category = self.category,
            summary = self.summary,
            strategy = self.strategy.join(" → "),
            avoid = self.avoid.join("; "),
            max_files = self.constraints.max_files,
            forbidden_paths = self.constraints.forbidden_paths.join(", "),
        )
    }
}
```

---

### 4.5 P1-3: Git 集成 — blast_radius 计算与回滚

**目标**：在 Capsule 生成时自动计算 blast_radius，支持 git 回滚。

#### 4.5.1 blast_radius 计算

利用现有的 [git 模块](file:///Users/ryanliu/Documents/uclaw/src-tauri/src/git/)，扩展：

```rust
/// 在 Capsule 生成时自动计算 blast_radius
fn compute_blast_radius(repo_path: &Path) -> anyhow::Result<BlastRadius> {
    let output = Command::new("git")
        .args(["-C", repo_path.to_str().unwrap(), "diff", "--stat", "HEAD"])
        .output()?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // 解析 git diff --stat 最后一行: 
    // "2 files changed, 34 insertions(+), 18 deletions(-)"
    let last_line = stdout.lines().last().unwrap_or("");
    
    let files = parse_files_changed(last_line);
    let lines = parse_total_changes(last_line);
    
    Ok(BlastRadius { files, lines })
}

fn parse_files_changed(line: &str) -> u32 {
    // "2 files changed" → 2
    line.split("file").next()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn parse_total_changes(line: &str) -> u32 {
    // "34 insertions(+), 18 deletions(-)" → 52
    let ins: u32 = line.split("insertion").next()
        .and_then(|s| s.split(',').last())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let del: u32 = line.split("deletion").next()
        .and_then(|s| s.split(',').last())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    ins + del
}
```

#### 4.5.2 回滚集成

```rust
/// 当 Capsule 标记为 failed 时，执行 git stash 回滚
async fn rollback_on_failure(capsule: &Capsule, repo_path: &Path) -> anyhow::Result<()> {
    if capsule.outcome.status == OutcomeStatus::Failed {
        let mode = std::env::var("GENE_ROLLBACK_MODE")
            .unwrap_or_else(|_| "stash".to_string());
        
        match mode.as_str() {
            "stash" => {
                Command::new("git")
                    .args(["-C", repo_path.to_str().unwrap(), "stash", "push", 
                           "--include-untracked", "-m", 
                           &format!("rollback: {}", capsule.id)])
                    .output()?;
            }
            "hard" => {
                Command::new("git")
                    .args(["-C", repo_path.to_str().unwrap(), "reset", "--hard", "HEAD"])
                    .output()?;
            }
            _ => {} // "none" — skip rollback
        }
    }
    Ok(())
}
```

---

## 5. 数据模型总览

### 5.1 新增存储路径（SHA-256 内容寻址）

```
.uclaw/gep/
├── genes/
│   └── <asset_id[0:2]>/<asset_id>.json          # Gene 文件体（SHA-256 hash 命名）
│       ├── e3/b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.json
│       └── a7/ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a.json
├── capsules/
│   └── <gene_id>/
│       ├── capsule_v1.json
│       ├── capsule_v2.json
│       └── capsule_v3_streak_5.json
├── events/
│   └── event_<timestamp>_<gene_id>.json
└── gene_candidates.jsonl                         # append-only 候选日志
```

MemoryGraph 中仅存 `GeneRef` 轻量索引节点（id, asset_id, signals_match, category, score, effective_streak），实际 Gene/Capsule/Event 正文均落文件系统。

### 5.2 与现有系统的关系

| 现有组件 | 改动类型 | 说明 |
|---|---|---|
| `self_eval.rs` | 轻量扩展 | 增加 blast_radius 和 env_fingerprint 到 metadata |
| `skill_extraction.rs` | **重构** | 输出格式从 Skill XML → Gene XML |
| `proactive/service.rs` | 扩展 | context_listener 消费 SkillLearned，维护 gene_candidates 池 |
| `proactive/scenarios/mod.rs` | 扩展 | 注册 GeneEvolutionScenario |
| `memubot_config.rs` | 扩展 | 新增 gene_distillation 相关配置 |
| `agentic_loop.rs` / `dispatcher.rs` | 扩展 | Gene 检索注入 system prompt |
| `git/` | 扩展 | blast_radius 计算 + 回滚 |
| `MemoryGraph` | 扩展 | Gene/Capsule/Event 节点类型 |

---

## 6. 实施路径

### Phase 0: 基础准备（1-2 天）
- [ ] 定义 Gene、Capsule、EvolutionEvent 数据结构（`types.rs`）
- [ ] 创建 `src-tauri/src/agent/gep/` 模块骨架
- [ ] 新增 `memubot_config.rs` 配置项
- [ ] 编写 Gene/Capsule 的序列化/反序列化测试

### Phase 1: P0-1 核心重构（2-3 天）
- [ ] 重写 `skill_extraction.rs` 的 `SKILL_EXTRACTION_SYSTEM_PROMPT`
- [ ] 将输出解析从 11 字段 Skill 改为 6 字段 Gene
- [ ] 新增 GeneRepository 存储层
- [ ] 编译测试 + 回归测试

### Phase 2: P0-2 数据回路（1-2 天）
- [ ] 扩展 `self_eval.rs` metadata
- [ ] ProactiveService context_listener 消费 SkillLearned
- [ ] 实现 gene_candidates 池（VecDeque + 容量控制）
- [ ] 集成测试

### Phase 3: P1-1 蒸馏场景（2-3 天）
- [ ] 实现 `GeneEvolutionScenario`
- [ ] 编写 `GENE_DISTILLATION_PROMPT`
- [ ] Capsule 生成逻辑
- [ ] EvolutionEvent 审计记录
- [ ] 端到端测试

### Phase 4: P1-2 检索注入（1-2 天）
- [ ] Gene 匹配算法
- [ ] system prompt 注入
- [ ] Agent 行为对比测试（with/without Gene）

### Phase 5: P1-3 Git 集成（1 天）
- [ ] blast_radius 计算
- [ ] git stash 回滚

---

## 7. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|---|---|---|
| Gene 蒸馏质量差（LLM 产出空泛内容） | 高 | 严格的前置检查（≤60 字符、≤4 步、must have AVOID cues）；没有就不产出 |
| 与已有 Skill 系统冲突 | 中 | Gene 和 Skill 并存过渡期，通过 `type` 字段区分 |
| MemoryGraph 存储格式不兼容 | 中 | Gene 作为新 `node_type`，不影响已有 skill nodes |
| 检索注入过多 Gene 导致 prompt 膨胀 | 低 | max_genes=2 硬限制，每个 Gene ≤80 tokens prompt |
| Git 回滚误操作 | 低 | 默认 stash 模式（可恢复），需要用户显式选择 hard |

---

## 8. 脑暴决策记录（Brainstorming Decisions）

以下决策通过 10 维度深度脑暴达成，作为设计文档的补充细化。

### 8.1 推理策略：约束优先 + Few-shot 锚定（Q1）

**决策**：选项 B — 在蒸馏 prompt 的**输出前自审**checklist 中调整思考顺序：
1. 先看 AVOID（失败信息）和 constraints（安全边界）
2. 再推导 strategy（步骤）
3. 最后定义 signals_match（触发词）

理由：约束优先迫使 LLM 从边界而非理想解出发，避免过度乐观策略。Few-shot 锚定用于防止 LLM 在不同 session 间漂移。

### 8.2 success_streak 计算：双轨制（Q2）

**决策**：选项 C — 双轨制。
- **raw_streak**：符合 GEP 协议（连续成功次数），存储在 Capsule 文件本体
- **effective_streak**：加权公式 `recency(0.5) × stability(0.3) × latest_score(0.2)`，用于排序

加权因子详解：
- recency：`exp(-days_since_last_capsule / 7)`，最近使用加分
- stability：`1.0 - variance_of_last_5_scores`，波动小的 Gene 更可信
- latest_score：直接取最近 Capsule 的 score

### 8.3 Gene 检索匹配：两阶段混合（Q3）

**决策**：选项 C — 两阶段混合。
- **Stage 1**（热路径）：精确 `signals_match` 子串匹配，O(n) 线性扫描，零延迟
- **Stage 2**（冷路径）：当 Stage 1 无命中时，用 `fastembed`（本地 embedding 模型）做语义向量搜索

理由：大多数场景 signals_match 足够，只有模糊场景才需要 embedding fallback。避免每次对话都做嵌入——节省延迟和计算。

### 8.4 多 Gene 冲突解决：全量注入 + 冲突标注（Q4）

**决策**：选项 B — 将所有匹配的 Gene 全量注入 system prompt，超量时带 CONFLICT NOTICE。

格式：
```
⚠️ CONFLICT NOTICE: 以下 Gene 存在潜在矛盾，请根据当前场景抉择：
GENE[gene_a]: 切换备用数据源（最大化可用性）
GENE[gene_b]: 移除冗余数据源调用（最小化外部依赖）
→ RESOLUTION HINT: 根据用户消息判断是数据获取失败（用 A）还是性能优化（用 B）
```

理由：Gene 冲突的本质是**场景依赖**，系统无法事先裁决（不是矛盾，是分场景的最优解不同）。让 LLM 基于当前上下文自行解决。

### 8.5 GEP 实体存储：文件系统主体 + MemoryGraph 引用索引（Q5）

**决策**：选项 B — 独立 GEP 文件存储 + MemoryGraph GeneRef 轻量索引。

存储布局：
```
.uclaw/gep/
├── genes/<asset_id[0:2]>/<asset_id>.json       # Gene 文件体（SHA-256 寻址）
├── capsules/<gene_id>/capsule_<id>.json        # Capsule 文件
└── events/event_<timestamp>_<gene_id>.json     # EvolutionEvent 审计文件
```

MemoryGraph 中只存 `GeneRef` 节点（id, asset_id, signals_match, category, score, streak），用于快速检索。
理由：文件系统本身就支持 SHA-256 内容寻址，不需要引入新存储引擎。MemoryGraph 只做轻量索引，Gene/Capsule/Event 主体落文件。

### 8.6 Gene 变异机制：Stage 1 先行（Q6）

**决策**：只实现 Stage 1（AVOID cues 增补），Stage 2-3 列入 backlog。

- **Stage 1**（P1 实施）：当同一 Gene 有 ≥2 个 Capsule 的 outcome 为 `failed` 或 `partial` 且出现新 failure 模式时，自动追加到 AVOID cues（≤5 条上限）。触发后 wait 3 天冷却，等 Capsule 积累再判断是否需要新一轮增补。
- **Stage 2**（P2 backlog）：success_streak 连续下降（如 5→3→1），LLM 分析原因并产出新版本 Gene。产生的是新版本（不覆盖原版本），便于 A/B 对比和回退。
- **Stage 3**（P3 backlog）：两个互补 Gene 手动触发融合，生成 hybrid Gene。

### 8.7 Gene 生命周期管理（Q7）

**决策**：引入 Gene 状态机 `active → stale → (retired | upgraded)`。

退役条件：
| 条件 | 阈值 |
|---|---|
| 连续失败 | 最近 3 个 Capsule 都是 `failed` |
| 长期未激活 | 180 天无新 Capsule 且无检索命中 |
| 被新版本替代 | 同 `gene_id` 有新版本且新版本 streak ≥ 3 |
| 环境指纹失效 | env_fingerprint 与当前环境不兼容 |
| 手动退役 | 用户通过 UI 标记 |

退役 ≠ 删除：保留所有 Capsule 和 Event 历史，前端显示为灰色，支持手动 reactivate。

### 8.8 learnings 中间表示：LearningCard 结构化标注（Q8）

**决策**：在 self_eval publish 前，用轻量规则分类器对 learnings 做结构化标注。

**P0**（基础过滤）：`is_generic_advice()` + score 阈值过滤噪声 learnings。
**P1**（完整 LearningCard）：增加 `card_type`（FailureLesson / SuccessPattern / OptimizationTip / Noise）、`failure_signal`、`tool_name`、`strategy_hint`（condition × action × reason）。
**P2**（聚类增强）：用 embedding 对同 session 的 LearningCard 聚类，自动归组。

LearningCard 结构：
```rust
struct LearningCard {
    raw: String,                          // 原始文本
    card_type: LearningCardType,          // 预分类
    failure_signal: Option<String>,       // "403", "timeout"
    tool_name: Option<String>,           // "read_file"
    strategy_hint: StrategyHint,          // condition × action × reason
    files_touched: Vec<String>,
    session_id: String, score: f32, timestamp: i64,
}
```

GeneEvolutionScenario 的蒸馏 prompt 消费的是结构化 LearningCard 而非原始自由文本——噪声已前置过滤，信号已预标注。

### 8.9 前端可视化：万花筒 Evolution Tab（Q9）

**决策**：在万花筒（Kaleidoscope）的 Skills 模块区域新增 "🧬 Evolution" Tab，与现有 "🧠 Skills" Tab 并列。

四个核心视图：
1. **Gene 列表**（左栏）：按 category（repair/optimize/innovate）过滤，streak+score 摘要
2. **Gene 进化树**（右栏）：版本分支（v1.0 → v1.1 AVOID → v2.0 planned）、跨 Gene 融合
3. **Capsule 时间轴**：选中 Gene 后下方展示，按时间倒序，success green / partial orange / failed red
4. **Gene 详情卡片**：六元组完整展示，AVOID 字段标注版本新增痕迹

布局策略：Phase 1 先放入万花筒 Tab，后续根据使用频率决定是否提升为独立面板。

### 8.10 与 Skill 系统共存策略（Q10）

**决策**：三阶段渐进——并行运行 → 逐步迁移 → 长期收敛。

**阶段 1（P0-P1，并行运行）**：
- Skill 系统**完全不变**（skill_extraction 继续产出文档型 Skill）
- Gene 系统**独立新增**（GeneDistillation 走独立管线，消费 LearningCard，产出 Gene 六元组）
- 用户感知：Skills Tab 不变，Evolution Tab 新功能

**阶段 2（P2，逐步迁移）**：
- 当 Gene 已覆盖场景时，skill_extraction 自动跳过
- Agent 同时检索 Skills（主动 tool call）和 Genes（系统自动注入）
- learned Skills 被 Gene 覆盖的标记为 `superseded_by_gene`

**阶段 3（P3，长期收敛，视数据反馈决定）**：
- 如 Gene 效果显著优于 Skill，可考虑将 Skill Extraction 完全替换
- built-in/user 手写 Skill **永久保留**（人类可读的知识传递价值不替代）

关键命名区分：Skill 叫"技能"，Gene 叫"策略基因"。

---

## 9. 关键设计原则（来自 GEP 论文）

1. **紧凑优于完备**：Gene ~230 tokens beats Skill ~2500 tokens。不求全，求信号密度。
2. **AVOID cues 是最高价值信号**：失败信息蒸馏成紧凑警告 > 朴素追加。
3. **Protocol-bound 优于 free-form**：严格 JSON schema 约束，可做 SHA-256 寻址和结构比较。
4. **控制导向优于文档导向**：Gene 为 LLM 推理设计，不为人类阅读设计。
5. **Bundle 规则**：Gene + Capsule 成对发布，形成可审计进化链。
6. **宁可空缺不可虚构**：`[NO_GENE]` 是合格输出，1 条好 Gene > 5 条水货。
