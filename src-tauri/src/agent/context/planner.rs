//! 模型感知的上下文自动规划器 (Model-Aware Context Planner)
//!
//! 基于 AI 模型的上下文窗口大小，自动计算最优的 token 预算分配方案。
//! 这是 "openhanako" 风格的分层上下文自动规划核心 —— 不再使用硬编码的
//! 静态预算值，而是根据每个模型的实际能力按比例动态分配。
//!
//! ## 设计原理
//!
//! ### 为什么需要模型感知规划？
//!
//! 不同模型的上下文窗口差异巨大（128K ~ 1M tokens）。如果使用固定预算：
//! - 1M 模型：200K 预算仅占 20%，浪费了 80% 的上下文空间
//! - 128K 模型：200K 预算超出模型上限，导致压缩阈值失效
//!
//! ### 算法核心
//!
//! 采用**分层比例分配**策略：
//!
//! 1. **有效预算因子**：根据窗口大小采用不同的预算利用率
//!    - 1M+ 窗口：75%（留 25% 给响应输出和系统开销）
//!    - 200K 窗口：75%
//!    - 128K 窗口：80%（平衡上下文利用与响应空间）
//!    - <128K 窗口：65%
//!
//! 2. **三层比例分配**：
//!    - L0 (Recent):   50% — 活跃对话，最高优先级
//!    - L1 (Archive):  25% — 压缩历史摘要
//!    - L2 (Retrieved): 15% — 语义检索记忆
//!    - System Reserve: 10% — 系统提示词/skills manifest/时间块（隐式）
//!
//! 3. **自适应参数**：
//!    - keep_turns: W / 18000（1M→55轮, 128K→10轮 min）
//!    - max_messages: W / 4000（1M→250条, 128K→32条）
//!
//! ### 与现有系统的关系
//!
//! - `get_model_context_length()`: 提供模型→窗口的静态映射（保持不变）
//! - `AgenticLoopConfig`: 通过 `from_model()` 使用规划结果
//! - `LayeredContextConfig`: 通过 `from_model_window()` 使用规划结果
//! - `compress_context_if_needed()`: 自动使用规划后的预算和阈值

use crate::agent::types::get_model_context_length;

// ─── 模型上下文规划结果 ──────────────────────────────────────────────────

/// 模型感知的上下文预算规划结果
///
/// 包含针对特定模型计算的所有上下文预算参数。
/// 通过 [`plan_context_for_model`] 生成。
#[derive(Debug, Clone)]
pub struct ModelContextPlan {
    /// 模型原始上下文窗口大小（token 数）
    pub model_context_length: u32,

    /// 有效 token 预算 = model_context_length × budget_factor
    /// 这是压缩判断的基准线
    pub token_budget: usize,

    /// 软压缩阈值比例（0.0–1.0），达到此比例触发 L0/L1 分层压缩
    pub compression_threshold: f32,

    /// 硬截断阈值比例（0.0–1.0），达到此比例触发逐条删除
    pub hard_truncation_threshold: f32,

    /// 压缩时保留的最近轮次数
    pub compression_keep_turns: usize,

    /// 上下文中最大消息数（L0 层限制）
    pub max_context_messages: usize,

    // ── 分层预算 ──────────────────────────────────────────────────────
    /// L0 层（最近消息）目标 token 数
    pub l0_target_tokens: usize,

    /// L1 层（档案摘要）目标 token 数
    pub l1_target_tokens: usize,

    /// L2 层（检索记忆）目标 token 数
    pub l2_target_tokens: usize,

    /// 系统提示词最大 token 数
    pub max_prompt_tokens: usize,
}

// ─── 规划函数 ────────────────────────────────────────────────────────────

/// 为指定模型规划上下文预算。
///
/// # 参数
/// - `model`: 模型名称字符串（如 "claude-sonnet-4-20250514"）
///
/// # 返回
/// 包含所有预算参数的 [`ModelContextPlan`]
///
/// # 算法
///
/// ```text
/// W = get_model_context_length(model)
///
/// // 第1步：确定预算因子
/// factor = match W {
///     >= 1_000_000 => 0.75,  // 1M 模型：750K 可用
///     >= 200_000   => 0.75,  // 200K 模型：150K 可用
///     >= 128_000   => 0.70,  // 128K 模型：~90K 可用
///     _            => 0.65,  // 更小窗口：保守
/// }
///
/// // 第2步：计算总预算
/// budget = W × factor
///
/// // 第3步：三层比例分配
/// l0 = budget × 0.50  // 最近消息
/// l1 = budget × 0.25  // 历史摘要
/// l2 = budget × 0.15  // 检索记忆
/// // 剩余 10% 留给系统提示词/skills/time block
///
/// // 第4步：自适应参数
/// keep_turns = max(10, W / 18000)
/// max_msgs   = max(10, W / 4000)
/// ```
///
/// # 示例
///
/// ```
/// // Claude Sonnet 4 (1M 窗口):
/// //   budget=750K, L0=375K, L1=187.5K, L2=112.5K, keep=40
///
/// // GPT-4o (128K 窗口):
/// //   budget=102.4K, L0=51.2K, L1=25.6K, L2=15.4K, keep=10
/// ```
pub fn plan_context_for_model(model: &str) -> ModelContextPlan {
    let window = get_model_context_length(model);

    // 第1步：预算因子 — 大窗口可以更激进地使用上下文
    let budget_factor = if window >= 1_000_000 {
        0.75 // 1M 窗口：750K 用于上下文，250K 留给响应
    } else if window >= 200_000 {
        0.75 // 200K 窗口：150K 用于上下文，50K 留给响应
    } else if window >= 128_000 {
        0.80 // 128K 窗口：平衡上下文利用与响应空间（原 0.70 过于保守，60%窗口就触发压缩）
    } else {
        0.65 // 小窗口：尽可能多地留给响应
    };

    // 第2步：有效 token 预算
    let token_budget = (window as f64 * budget_factor) as usize;

    // 第3步：三层比例分配
    // L0（最近消息）— 50%，承载活跃对话
    let l0_target_tokens = (token_budget as f64 * 0.50) as usize;
    // L1（档案摘要）— 25%，压缩历史
    let l1_target_tokens = (token_budget as f64 * 0.25) as usize;
    // L2（检索记忆）— 15%，recall engine 结果
    let l2_target_tokens = (token_budget as f64 * 0.15) as usize;
    // System Reserve — 10%，系统提示词/skills manifest/time block（隐式）

    // 第4步：自适应参数
    // keep_turns: 大窗口保留更多轮次，小窗口至少 10 轮
    // （原公式 W/25000 对 128K 模型仅保留 5 轮，过于激进；
    //  改为 W/18000 使 128K→10 轮，1M→55 轮）
    let compression_keep_turns = (window as usize / 18_000).max(10);
    // max_messages: 控制 L0 层消息数上限
    let max_context_messages = (window as usize / 4_000).max(10);
    // max_prompt_tokens: 系统提示词预算（取 L2 的 60% 或至少 1000）
    let max_prompt_tokens = ((l2_target_tokens as f64 * 0.6) as usize).max(1000);

    // 压缩阈值：固定比例
    // compression_threshold: 0.90 — 给响应留 10% 余量后再触发压缩
    //   （原 0.85 对 128K 模型仅在 ~60% 窗口就触发，过于频繁）
    let compression_threshold = 0.90; // 90% 触发软压缩
    let hard_truncation_threshold = 0.98; // 98% 触发硬截断

    ModelContextPlan {
        model_context_length: window,
        token_budget,
        compression_threshold,
        hard_truncation_threshold,
        compression_keep_turns,
        max_context_messages,
        l0_target_tokens,
        l1_target_tokens,
        l2_target_tokens,
        max_prompt_tokens,
    }
}

// ─── 单元测试 ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── 1M 模型测试 ─────────────────────────────────────────────────────

    #[test]
    fn test_plan_for_1m_model() {
        // Claude Sonnet 4 带 1M 上下文
        let plan = plan_context_for_model("claude-sonnet-4-20250514");
        assert_eq!(plan.model_context_length, 1_000_000);
        assert_eq!(plan.token_budget, 750_000); // 75% of 1M
        assert_eq!(plan.l0_target_tokens, 375_000); // 50% of budget
        assert_eq!(plan.l1_target_tokens, 187_500); // 25% of budget
        assert_eq!(plan.l2_target_tokens, 112_500); // 15% of budget
        assert_eq!(plan.compression_keep_turns, 55); // 1M / 18000 = 55
        assert_eq!(plan.max_context_messages, 250); // 1M / 4000
        assert!(plan.compression_threshold > 0.8);
        assert!(plan.hard_truncation_threshold > 0.95);
    }

    #[test]
    fn test_plan_for_opus_5() {
        let plan = plan_context_for_model("claude-opus-5-20250514");
        assert_eq!(plan.model_context_length, 1_000_000);
        assert_eq!(plan.token_budget, 750_000);
        assert_eq!(plan.compression_keep_turns, 55);
    }

    // ── 200K 模型测试 ───────────────────────────────────────────────────

    #[test]
    fn test_plan_for_200k_model() {
        // Claude 3.5 Sonnet / Haiku
        let plan = plan_context_for_model("claude-3-5-sonnet-20241022");
        assert_eq!(plan.model_context_length, 200_000);
        assert_eq!(plan.token_budget, 150_000); // 75% of 200K
        assert_eq!(plan.l0_target_tokens, 75_000);
        assert_eq!(plan.l1_target_tokens, 37_500);
        assert_eq!(plan.l2_target_tokens, 22_500);
        assert_eq!(plan.compression_keep_turns, 11); // 200K / 18000 = 11
        assert_eq!(plan.max_context_messages, 50); // 200K / 4000
    }

    // ── 128K 模型测试 ───────────────────────────────────────────────────

    #[test]
    fn test_plan_for_gpt4o() {
        let plan = plan_context_for_model("gpt-4o");
        assert_eq!(plan.model_context_length, 128_000);
        assert_eq!(plan.token_budget, 102_400); // 80% of 128K
        assert_eq!(plan.l0_target_tokens, 51_200);
        assert_eq!(plan.l1_target_tokens, 25_600);
        assert_eq!(plan.l2_target_tokens, 15_360);
        assert_eq!(plan.compression_keep_turns, 10); // max(10, 128K/18000=7)
        assert_eq!(plan.max_context_messages, 32); // 128K / 4000
    }

    #[test]
    fn test_plan_for_deepseek() {
        let plan = plan_context_for_model("deepseek-r1");
        assert_eq!(plan.model_context_length, 128_000);
        assert_eq!(plan.token_budget, 102_400);
        assert_eq!(plan.compression_keep_turns, 10);
    }

    #[test]
    fn test_plan_for_qwen() {
        let plan = plan_context_for_model("qwen-max");
        assert_eq!(plan.model_context_length, 131_072);
        // 131072 >= 128000 → factor 0.80
        assert_eq!(plan.token_budget, 104_857); // 80% of 131072 ≈ 104857
        assert!(plan.l0_target_tokens > 0);
        assert!(plan.l1_target_tokens > 0);
        assert!(plan.l2_target_tokens > 0);
    }

    // ── 小模型测试 ──────────────────────────────────────────────────────

    #[test]
    fn test_plan_for_small_model() {
        // 假设一个 64K 模型（通过默认路径返回 200K，这里测的是因子逻辑）
        // 直接测试 < 128K 分支：get_model_context_length 对所有未知模型返回 200K
        // 所以这里测试的是预算分配的数学正确性
        let plan = plan_context_for_model("claude-3-5-sonnet-20241022");
        // 200K → budget = 150K
        assert_eq!(plan.token_budget, 150_000);
        // L0+L1+L2 ≤ budget (三者之和应 ≤ budget)
        let layered_total = plan.l0_target_tokens + plan.l1_target_tokens + plan.l2_target_tokens;
        assert!(layered_total <= plan.token_budget);
    }

    // ── 预算一致性测试 ──────────────────────────────────────────────────

    #[test]
    fn test_budget_consistency_across_models() {
        let models = vec![
            "claude-sonnet-4-20250514",   // 1M
            "claude-opus-4-6-20250514",   // 1M
            "claude-3-5-sonnet-20241022", // 200K
            "gpt-4o",                      // 128K
            "deepseek-r1",                 // 128K
            "qwen-max",                    // 131K
        ];

        for model in models {
            let plan = plan_context_for_model(model);

            // 基本不变量
            assert!(plan.token_budget > 0, "model={model}: budget must be positive");
            assert!(plan.token_budget <= plan.model_context_length as usize,
                "model={model}: budget {budget} exceeds window {window}",
                budget = plan.token_budget, window = plan.model_context_length);

            // 分层预算不变量
            let layered_total = plan.l0_target_tokens + plan.l1_target_tokens + plan.l2_target_tokens;
            assert!(layered_total <= plan.token_budget,
                "model={model}: layered total {layered} exceeds budget {budget}",
                layered = layered_total, budget = plan.token_budget);

            // L0 应该最大
            assert!(plan.l0_target_tokens >= plan.l1_target_tokens,
                "model={model}: L0 should be >= L1");
            assert!(plan.l1_target_tokens >= plan.l2_target_tokens,
                "model={model}: L1 should be >= L2");

            // 自适应参数合理
            assert!(plan.compression_keep_turns >= 10,
                "model={model}: keep_turns too small");
            assert!(plan.max_context_messages >= 10,
                "model={model}: max_messages too small");
            assert!(plan.max_prompt_tokens >= 1000,
                "model={model}: max_prompt_tokens too small");

            // 阈值合理性
            assert!(plan.compression_threshold >= 0.90 && plan.compression_threshold < 1.0);
            assert!(plan.hard_truncation_threshold > plan.compression_threshold);
            assert!(plan.hard_truncation_threshold < 1.0);
        }
    }

    // ── 边界测试 ────────────────────────────────────────────────────────

    #[test]
    fn test_plan_empty_model_name() {
        // 空模型名走默认路径 → 200K
        let plan = plan_context_for_model("");
        assert_eq!(plan.model_context_length, 200_000);
        assert_eq!(plan.token_budget, 150_000);
    }

    #[test]
    fn test_layered_budget_sum_is_reasonable() {
        // 验证三层预算 + system reserve 的和接近总预算
        let plan = plan_context_for_model("claude-sonnet-4-20250514");
        let layered_sum = plan.l0_target_tokens + plan.l1_target_tokens + plan.l2_target_tokens;
        let system_reserve = plan.token_budget - layered_sum;
        // System reserve 应在 8%–12% 范围
        let reserve_ratio = system_reserve as f64 / plan.token_budget as f64;
        assert!(reserve_ratio >= 0.08 && reserve_ratio <= 0.12,
            "System reserve ratio {:.2}% out of expected 8-12% range",
            reserve_ratio * 100.0);
    }

    #[test]
    fn test_keep_turns_scales_with_window() {
        let large = plan_context_for_model("claude-sonnet-4-20250514");
        let medium = plan_context_for_model("claude-3-5-sonnet-20241022");
        let small = plan_context_for_model("gpt-4o");

        // 大窗口应保留更多轮次
        assert!(large.compression_keep_turns > medium.compression_keep_turns);
        assert!(medium.compression_keep_turns >= small.compression_keep_turns);
    }

    #[test]
    fn test_max_messages_scales_with_window() {
        let large = plan_context_for_model("claude-sonnet-4-20250514");
        let medium = plan_context_for_model("claude-3-5-sonnet-20241022");
        let small = plan_context_for_model("gpt-4o");

        // 大窗口应有更高消息上限
        assert!(large.max_context_messages > medium.max_context_messages);
        assert!(medium.max_context_messages >= small.max_context_messages);
    }
}
