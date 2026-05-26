//! 分层上下文管理器
//!
//! 实现 L0/L1/L2 三层上下文策略，在有限的 token 预算内最大化相关上下文的包含。
//!
//! - **L0 (Recent)**: 最近消息，最高优先级
//! - **L1 (Archive)**: 压缩的历史摘要
//! - **L2 (Retrieved)**: 语义检索的相关记忆（来自 recall engine）

use serde::{Deserialize, Serialize};

// ─── 上下文层级 ──────────────────────────────────────────────────────────

/// 上下文层级标识
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextLayer {
    /// L0: 最近消息 — 最高优先级，保留完整对话上下文
    Recent,
    /// L1: 档案摘要 — 由 LLM 压缩的历史会话摘要
    Archive,
    /// L2: 检索记忆 — 从 recall engine 语义检索的相关记忆
    Retrieved,
}

impl ContextLayer {
    /// 返回层级的字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            ContextLayer::Recent => "recent",
            ContextLayer::Archive => "archive",
            ContextLayer::Retrieved => "retrieved",
        }
    }
}

// ─── 上下文条目 ──────────────────────────────────────────────────────────

/// 单条上下文条目，携带元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    /// 所属层级: "recent" / "archive" / "retrieved"
    pub layer: String,
    /// 消息角色: "user" / "assistant" / "system"
    pub role: String,
    /// 文本内容
    pub content: String,
    /// 估算的 token 数
    pub estimated_tokens: usize,
    /// 优先级 0.0 ~ 1.0，越高越优先
    pub priority: f32,
    /// 来源标识（如 "recall_boot"、"session_archive" 等）
    pub source: Option<String>,
}

// ─── 分层上下文配置 ──────────────────────────────────────────────────────

/// 分层上下文的 token 预算配置
#[derive(Debug, Clone)]
pub struct LayeredContextConfig {
    /// 上下文中包含的最大消息数
    pub max_context_messages: usize,
    /// 上下文总 token 上限
    pub max_context_tokens: usize,
    /// L0 层（最近消息）的 token 预算
    pub l0_target_tokens: usize,
    /// L1 层（档案/压缩摘要）的 token 预算
    pub l1_target_tokens: usize,
    /// L2 层（检索记忆）的 token 预算 — 自动计算为剩余空间
    pub l2_target_tokens: usize,
    /// 用户提示的最大 token 数
    pub max_prompt_tokens: usize,
}

impl LayeredContextConfig {
    /// 基于模型上下文窗口大小自动规划分层预算。
    pub fn from_model_window(window: u32) -> Self {
        let budget_factor = if window >= 1_000_000 {
            0.75
        } else if window >= 200_000 {
            0.75
        } else if window >= 128_000 {
            0.80
        } else {
            0.65
        };

        let max_context_tokens = (window as f64 * budget_factor) as usize;
        let l0 = (max_context_tokens as f64 * 0.50) as usize;
        let l1 = (max_context_tokens as f64 * 0.25) as usize;
        let l2 = (max_context_tokens as f64 * 0.15) as usize;
        let max_context_messages = (window as usize / 4_000).max(10);
        let max_prompt_tokens = ((l2 as f64 * 0.6) as usize).max(1000);

        Self {
            max_context_messages,
            max_context_tokens,
            l0_target_tokens: l0,
            l1_target_tokens: l1,
            l2_target_tokens: l2,
            max_prompt_tokens,
        }
    }
}

impl Default for LayeredContextConfig {
    fn default() -> Self {
        let max_context_tokens = 12000;
        let l0 = 4000;
        let l1 = 4000;
        Self {
            max_context_messages: 40,
            max_context_tokens,
            l0_target_tokens: l0,
            l1_target_tokens: l1,
            l2_target_tokens: max_context_tokens.saturating_sub(l0 + l1),
            max_prompt_tokens: 2000,
        }
    }
}

// ─── Token 使用统计 ──────────────────────────────────────────────────────

/// 各层 token 使用统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTokenStats {
    /// L0（最近消息）已用 token 数
    pub l0_recent_tokens: usize,
    /// L1（档案摘要）已用 token 数
    pub l1_archive_tokens: usize,
    /// L2（检索记忆）已用 token 数
    pub l2_retrieved_tokens: usize,
    /// 三层总计 token
    pub total_tokens: usize,
    /// 剩余 token 预算
    pub budget_remaining: usize,
    /// L0 层包含的消息数
    pub l0_message_count: usize,
    /// L2 层包含的记忆条目数
    pub l2_memory_count: usize,
}

// ─── 分层上下文构建器 ────────────────────────────────────────────────────

/// 分层上下文构建器
///
/// 在有限的 token 预算内，按优先级组装最相关的上下文。
/// 使用方式：
/// 1. 创建构建器 `LayeredContextBuilder::new(config)`
/// 2. 按需添加各层内容: `add_recent_messages`, `add_archive`, `add_retrieved_memories`
/// 3. 调用 `build()` 获取最终消息列表，或 `build_system_context()` 获取系统提示注入文本
pub struct LayeredContextBuilder {
    config: LayeredContextConfig,
    /// L0: 最近消息（按时间顺序，最早在前）
    l0_entries: Vec<ContextEntry>,
    /// L1: 档案摘要
    l1_entries: Vec<ContextEntry>,
    /// L2: 检索到的记忆
    l2_entries: Vec<ContextEntry>,
    /// 各层已用 token 统计
    l0_used_tokens: usize,
    l1_used_tokens: usize,
    l2_used_tokens: usize,
}

impl LayeredContextBuilder {
    /// 创建新的分层上下文构建器
    pub fn new(config: LayeredContextConfig) -> Self {
        Self {
            config,
            l0_entries: Vec::new(),
            l1_entries: Vec::new(),
            l2_entries: Vec::new(),
            l0_used_tokens: 0,
            l1_used_tokens: 0,
            l2_used_tokens: 0,
        }
    }

    // ── 添加各层内容 ─────────────────────────────────────────────────────

    /// 添加最近消息 (L0 层)
    ///
    /// `messages` 按时间正序排列 `Vec<(role, content)>`（最旧在前）。
    /// 从最新消息开始向前遍历，直到 L0 token 预算用完，
    /// 最终保留的消息按原始时间顺序排列。
    pub fn add_recent_messages(&mut self, messages: Vec<(String, String)>) {
        let mut selected: Vec<ContextEntry> = Vec::new();
        let mut tokens_used = 0usize;
        let budget = self.config.l0_target_tokens;
        let max_msgs = self.config.max_context_messages;

        // 从最新到最旧遍历，优先保留最近的消息
        for (role, content) in messages.iter().rev() {
            if selected.len() >= max_msgs {
                break;
            }
            let est = estimate_tokens(content);
            if tokens_used + est > budget {
                break;
            }
            tokens_used += est;
            // 优先级: 越新的消息优先级越高 (1.0 → 0.0)
            let priority = 1.0 - (selected.len() as f32 / max_msgs.max(1) as f32);
            selected.push(ContextEntry {
                layer: ContextLayer::Recent.as_str().to_string(),
                role: role.clone(),
                content: content.clone(),
                estimated_tokens: est,
                priority,
                source: Some("recent_conversation".to_string()),
            });
        }

        // 反转回时间正序（最旧在前）
        selected.reverse();

        self.l0_used_tokens = tokens_used;
        self.l0_entries = selected;
    }

    /// 添加历史摘要 (L1 层)
    ///
    /// 接受由 LLM 压缩的前一个会话摘要文本。
    /// 如果摘要超出 L1 token 预算，会进行截断。
    pub fn add_archive(&mut self, archive_summary: &str) {
        if archive_summary.is_empty() {
            return;
        }
        let est = estimate_tokens(archive_summary);
        let budget = self.config.l1_target_tokens;

        // 如果超出预算，截断到预算以内
        let (content, final_tokens) = if est > budget {
            let truncated = truncate_to_token_budget(archive_summary, budget);
            let new_est = estimate_tokens(&truncated);
            (truncated, new_est)
        } else {
            (archive_summary.to_string(), est)
        };

        self.l1_entries.push(ContextEntry {
            layer: ContextLayer::Archive.as_str().to_string(),
            role: "system".to_string(),
            content,
            estimated_tokens: final_tokens,
            priority: 0.8, // 档案摘要优先级高于检索记忆
            source: Some("session_archive".to_string()),
        });
        self.l1_used_tokens = final_tokens;
    }

    /// 添加检索到的记忆 (L2 层)
    ///
    /// `memories` 格式: `Vec<(content, relevance_score)>`
    /// 按相关度排序（已由 recall engine 排好），填充剩余 token 预算。
    pub fn add_retrieved_memories(&mut self, memories: Vec<(String, f32)>) {
        let budget = self.config.l2_target_tokens;
        let mut tokens_used = 0usize;

        for (content, score) in memories {
            let est = estimate_tokens(&content);
            if tokens_used + est > budget {
                // 尝试截断最后一条以充分利用预算
                let remaining = budget.saturating_sub(tokens_used);
                if remaining > 20 {
                    // 至少留 20 token 才值得截断
                    let truncated = truncate_to_token_budget(&content, remaining);
                    let trunc_est = estimate_tokens(&truncated);
                    self.l2_entries.push(ContextEntry {
                        layer: ContextLayer::Retrieved.as_str().to_string(),
                        role: "system".to_string(),
                        content: truncated,
                        estimated_tokens: trunc_est,
                        priority: score.clamp(0.0, 1.0),
                        source: Some("recall_engine".to_string()),
                    });
                    tokens_used += trunc_est;
                }
                break;
            }
            tokens_used += est;
            self.l2_entries.push(ContextEntry {
                layer: ContextLayer::Retrieved.as_str().to_string(),
                role: "system".to_string(),
                content,
                estimated_tokens: est,
                priority: score.clamp(0.0, 1.0),
                source: Some("recall_engine".to_string()),
            });
        }

        self.l2_used_tokens = tokens_used;
    }

    // ── 构建输出 ─────────────────────────────────────────────────────────

    /// 构建最终的上下文消息列表
    ///
    /// 返回格式: `Vec<(role, content)>`
    /// 组装顺序:
    /// 1. L1 档案摘要（作为 system context）
    /// 2. L2 检索记忆（作为 system context）
    /// 3. L0 最近消息（保持原始时间顺序）
    pub fn build(&self) -> Vec<(String, String)> {
        let mut result: Vec<(String, String)> = Vec::new();

        // 1. L1 — 档案摘要
        for entry in &self.l1_entries {
            result.push((entry.role.clone(), entry.content.clone()));
        }

        // 2. L2 — 检索记忆（合并为单条 system 消息，避免消息过多）
        if !self.l2_entries.is_empty() {
            let mut combined = String::from("[Retrieved Memories]\n");
            for entry in &self.l2_entries {
                combined.push_str(&entry.content);
                combined.push('\n');
            }
            result.push(("system".to_string(), combined));
        }

        // 3. L0 — 最近消息（保持时间正序）
        for entry in &self.l0_entries {
            result.push((entry.role.clone(), entry.content.clone()));
        }

        result
    }

    /// 构建为格式化的系统上下文字符串
    ///
    /// 适用于注入到 system prompt 中，作为上下文补充信息。
    /// 不包含 L0 最近消息（L0 会作为独立的对话消息传入）。
    pub fn build_system_context(&self) -> String {
        let mut out = String::from("<layered_context>\n");

        // L1 — Session Archive
        if !self.l1_entries.is_empty() {
            out.push_str("## Session Archive\n");
            for entry in &self.l1_entries {
                out.push_str(&entry.content);
                out.push('\n');
            }
            out.push('\n');
        }

        // L2 — Retrieved Memories
        if !self.l2_entries.is_empty() {
            out.push_str("## Retrieved Memories\n");
            for (i, entry) in self.l2_entries.iter().enumerate() {
                let score_display = format!("{:.2}", entry.priority);
                out.push_str(&format!(
                    "### Memory {} (relevance: {})\n{}\n\n",
                    i + 1,
                    score_display,
                    entry.content,
                ));
            }
        }

        out.push_str("</layered_context>");
        out
    }

    // ── 统计信息 ─────────────────────────────────────────────────────────

    /// 获取各层的 token 使用统计
    pub fn get_token_stats(&self) -> ContextTokenStats {
        let total = self.l0_used_tokens + self.l1_used_tokens + self.l2_used_tokens;
        ContextTokenStats {
            l0_recent_tokens: self.l0_used_tokens,
            l1_archive_tokens: self.l1_used_tokens,
            l2_retrieved_tokens: self.l2_used_tokens,
            total_tokens: total,
            budget_remaining: self.config.max_context_tokens.saturating_sub(total),
            l0_message_count: self.l0_entries.len(),
            l2_memory_count: self.l2_entries.len(),
        }
    }
}

// ─── Token 估算工具函数 ──────────────────────────────────────────────────

/// Token 估算（粗略）
///
/// 针对中英文混合文本的 token 数估算：
/// - CJK 字符: 约 1.5 字符/token（即每个 CJK 字符约 0.67 token）
/// - ASCII 字符: 约 4 字符/token（即每个 ASCII 字符约 0.25 token）
///
/// 这是一种快速近似方法，不依赖 tokenizer 库。
pub fn estimate_tokens(text: &str) -> usize {
    let mut cjk_chars = 0usize;
    let mut ascii_chars = 0usize;

    for ch in text.chars() {
        if ch > '\u{2E7F}' {
            // CJK 及扩展字符（包含中日韩统一表意文字等）
            cjk_chars += 1;
        } else {
            ascii_chars += 1;
        }
    }

    let cjk_tokens = (cjk_chars as f64 / 1.5).ceil() as usize;
    let ascii_tokens = (ascii_chars as f64 / 4.0).ceil() as usize;
    cjk_tokens + ascii_tokens
}

/// 将文本截断到指定 token 预算以内
///
/// 从头开始遍历字符，累计估算 token 数，到达预算后截断并添加省略标记。
fn truncate_to_token_budget(text: &str, max_tokens: usize) -> String {
    let mut cjk_chars = 0usize;
    let mut ascii_chars = 0usize;
    let mut char_count = 0usize;

    for ch in text.chars() {
        if ch > '\u{2E7F}' {
            cjk_chars += 1;
        } else {
            ascii_chars += 1;
        }

        let current_tokens =
            (cjk_chars as f64 / 1.5).ceil() as usize + (ascii_chars as f64 / 4.0).ceil() as usize;

        if current_tokens >= max_tokens {
            break;
        }
        char_count += 1;
    }

    let truncated: String = text.chars().take(char_count).collect();
    if char_count < text.chars().count() {
        format!("{}...", truncated)
    } else {
        truncated
    }
}

// ─── 单元测试 ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Token 估算测试 ───────────────────────────────────────────────────

    #[test]
    fn test_estimate_tokens_ascii_only() {
        // "hello world" = 11 ASCII 字符 → ceil(11/4) = 3 tokens
        let tokens = estimate_tokens("hello world");
        assert_eq!(tokens, 3);
    }

    #[test]
    fn test_estimate_tokens_cjk_only() {
        // "你好世界" = 4 CJK 字符 → ceil(4/1.5) = 3 tokens
        let tokens = estimate_tokens("你好世界");
        assert_eq!(tokens, 3);
    }

    #[test]
    fn test_estimate_tokens_mixed() {
        // "Hello 你好" = 6 ASCII + 2 CJK
        // ASCII: ceil(6/4) = 2, CJK: ceil(2/1.5) = 2 → total = 4
        let tokens = estimate_tokens("Hello 你好");
        assert_eq!(tokens, 4);
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    // ── L0 最近消息测试 ─────────────────────────────────────────────────

    #[test]
    fn test_add_recent_messages_within_budget() {
        let config = LayeredContextConfig {
            l0_target_tokens: 100,
            max_context_messages: 10,
            ..Default::default()
        };
        let mut builder = LayeredContextBuilder::new(config);

        let messages = vec![
            ("user".to_string(), "Hello".to_string()),
            ("assistant".to_string(), "Hi there!".to_string()),
            ("user".to_string(), "How are you?".to_string()),
        ];
        builder.add_recent_messages(messages);

        // 所有消息都应被包含
        assert_eq!(builder.l0_entries.len(), 3);
        // 验证顺序正确（时间正序）
        assert_eq!(builder.l0_entries[0].content, "Hello");
        assert_eq!(builder.l0_entries[2].content, "How are you?");
    }

    #[test]
    fn test_add_recent_messages_budget_overflow() {
        let config = LayeredContextConfig {
            l0_target_tokens: 5, // 非常小的预算
            max_context_messages: 10,
            ..Default::default()
        };
        let mut builder = LayeredContextBuilder::new(config);

        let messages = vec![
            (
                "user".to_string(),
                "First message that is very long".to_string(),
            ),
            ("assistant".to_string(), "Second".to_string()),
            ("user".to_string(), "Third message".to_string()),
        ];
        builder.add_recent_messages(messages);

        // 预算很小，应该只包含部分最新消息
        assert!(builder.l0_entries.len() < 3);
        // 包含的应该是最新的消息
        if !builder.l0_entries.is_empty() {
            let last = builder.l0_entries.last().unwrap();
            assert!(last.content == "Third message" || last.content == "Second");
        }
    }

    // ── L1 档案摘要测试 ─────────────────────────────────────────────────

    #[test]
    fn test_add_archive_normal() {
        let config = LayeredContextConfig::default();
        let mut builder = LayeredContextBuilder::new(config);

        builder.add_archive("This is a session summary from earlier conversations.");

        assert_eq!(builder.l1_entries.len(), 1);
        assert_eq!(builder.l1_entries[0].role, "system");
        assert!(builder.l1_used_tokens > 0);
    }

    #[test]
    fn test_add_archive_empty() {
        let config = LayeredContextConfig::default();
        let mut builder = LayeredContextBuilder::new(config);

        builder.add_archive("");

        assert!(builder.l1_entries.is_empty());
        assert_eq!(builder.l1_used_tokens, 0);
    }

    #[test]
    fn test_add_archive_truncation() {
        let config = LayeredContextConfig {
            l1_target_tokens: 5, // 非常小的预算
            ..Default::default()
        };
        let mut builder = LayeredContextBuilder::new(config);

        let long_summary = "A".repeat(200); // 200 ASCII → 50 tokens，远超预算
        builder.add_archive(&long_summary);

        assert_eq!(builder.l1_entries.len(), 1);
        assert!(builder.l1_used_tokens <= 6); // 截断后应接近预算
    }

    // ── L2 检索记忆测试 ─────────────────────────────────────────────────

    #[test]
    fn test_add_retrieved_memories() {
        let config = LayeredContextConfig {
            l2_target_tokens: 100,
            ..Default::default()
        };
        let mut builder = LayeredContextBuilder::new(config);

        let memories = vec![
            ("Memory about user preferences".to_string(), 0.95),
            ("Memory about project setup".to_string(), 0.85),
            ("Memory about coding style".to_string(), 0.70),
        ];
        builder.add_retrieved_memories(memories);

        assert_eq!(builder.l2_entries.len(), 3);
        // 第一条相关度最高
        assert_eq!(builder.l2_entries[0].priority, 0.95);
    }

    #[test]
    fn test_add_retrieved_memories_budget_limit() {
        let config = LayeredContextConfig {
            l2_target_tokens: 10, // 很小的预算
            ..Default::default()
        };
        let mut builder = LayeredContextBuilder::new(config);

        let memories = vec![
            ("Short".to_string(), 0.95),
            (
                "A much longer memory content that should exceed the small budget".to_string(),
                0.85,
            ),
            ("Third memory".to_string(), 0.70),
        ];
        builder.add_retrieved_memories(memories);

        // 不应包含所有记忆
        assert!(builder.l2_entries.len() < 3);
        assert!(builder.l2_used_tokens <= 12); // 允许一些溢出（截断精度）
    }

    // ── 构建输出测试 ─────────────────────────────────────────────────────

    #[test]
    fn test_build_output_order() {
        let config = LayeredContextConfig::default();
        let mut builder = LayeredContextBuilder::new(config);

        builder.add_archive("Session archive content");
        builder.add_retrieved_memories(vec![("Retrieved memory 1".to_string(), 0.9)]);
        builder.add_recent_messages(vec![
            ("user".to_string(), "Hello".to_string()),
            ("assistant".to_string(), "Hi".to_string()),
        ]);

        let result = builder.build();

        // 输出顺序: L1 → L2 → L0
        assert!(result.len() >= 4); // 1 archive + 1 retrieved + 2 recent
        assert_eq!(result[0].0, "system"); // L1 archive
        assert_eq!(result[1].0, "system"); // L2 retrieved
        assert_eq!(result[2].0, "user"); // L0 first msg
        assert_eq!(result[3].0, "assistant"); // L0 second msg
    }

    #[test]
    fn test_build_system_context_format() {
        let config = LayeredContextConfig::default();
        let mut builder = LayeredContextBuilder::new(config);

        builder.add_archive("Previous session summary");
        builder.add_retrieved_memories(vec![
            ("User likes Rust".to_string(), 0.95),
            ("Project uses Tauri".to_string(), 0.80),
        ]);

        let ctx = builder.build_system_context();

        assert!(ctx.starts_with("<layered_context>"));
        assert!(ctx.ends_with("</layered_context>"));
        assert!(ctx.contains("## Session Archive"));
        assert!(ctx.contains("## Retrieved Memories"));
        assert!(ctx.contains("User likes Rust"));
        assert!(ctx.contains("Project uses Tauri"));
        assert!(ctx.contains("relevance: 0.95"));
    }

    #[test]
    fn test_build_system_context_empty() {
        let config = LayeredContextConfig::default();
        let builder = LayeredContextBuilder::new(config);

        let ctx = builder.build_system_context();

        // 空状态下不应有具体段落，只有外层标签
        assert!(ctx.starts_with("<layered_context>"));
        assert!(ctx.ends_with("</layered_context>"));
        assert!(!ctx.contains("## Session Archive"));
        assert!(!ctx.contains("## Retrieved Memories"));
    }

    // ── Token 统计测试 ───────────────────────────────────────────────────

    #[test]
    fn test_token_stats() {
        let config = LayeredContextConfig {
            max_context_tokens: 6000,
            ..Default::default()
        };
        let mut builder = LayeredContextBuilder::new(config);

        builder.add_recent_messages(vec![("user".to_string(), "Hello world".to_string())]);
        builder.add_archive("Summary of past events");
        builder.add_retrieved_memories(vec![("Memory content".to_string(), 0.9)]);

        let stats = builder.get_token_stats();
        assert!(stats.l0_recent_tokens > 0);
        assert!(stats.l1_archive_tokens > 0);
        assert!(stats.l2_retrieved_tokens > 0);
        assert_eq!(
            stats.total_tokens,
            stats.l0_recent_tokens + stats.l1_archive_tokens + stats.l2_retrieved_tokens
        );
        assert_eq!(stats.budget_remaining, 6000 - stats.total_tokens);
        assert_eq!(stats.l0_message_count, 1);
        assert_eq!(stats.l2_memory_count, 1);
    }

    // ── 截断工具函数测试 ─────────────────────────────────────────────────

    #[test]
    fn test_truncate_to_token_budget() {
        let long_text = "A".repeat(400); // 400 ASCII → 100 tokens
        let truncated = truncate_to_token_budget(&long_text, 10);
        let est = estimate_tokens(&truncated);
        // 截断后的 token 数应接近目标（含省略号开销）
        assert!(est <= 15);
    }

    #[test]
    fn test_truncate_to_token_budget_cjk() {
        let cjk_text = "你".to_string().repeat(100); // 100 CJK → ~67 tokens
        let truncated = truncate_to_token_budget(&cjk_text, 10);
        let est = estimate_tokens(&truncated);
        assert!(est <= 15);
    }

    #[test]
    fn test_truncate_short_text_no_change() {
        let short = "Hello";
        let result = truncate_to_token_budget(short, 100);
        assert_eq!(result, "Hello"); // 不需要截断
    }
}
