use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
    /// Whether this message has been logically compacted (marked for removal
    /// from LLM context but kept in DB for UI replay). Default: false.
    #[serde(default)]
    pub compacted: bool,
}

impl ChatMessage {
    pub fn system(text: &str) -> Self {
        Self {
            role: MessageRole::System,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            compacted: false,
        }
    }
    pub fn user(text: &str) -> Self {
        Self {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            compacted: false,
        }
    }
    pub fn assistant(text: &str) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            compacted: false,
        }
    }
    pub fn assistant_with_tool_use(id: &str, name: &str, input: serde_json::Value) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input,
            }],
            compacted: false,
        }
    }
    pub fn user_tool_result(tool_use_id: &str, content: &str, is_error: bool) -> Self {
        Self {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: content.to_string(),
                is_error: Some(is_error),
            }],
            compacted: false,
        }
    }

    /// Build an Assistant message from a streaming LLM response's components.
    ///
    /// Encodes the canonical block order **Thinking → Text → ToolUse**.
    /// Replaces 6 nearly-identical inline assemblies in
    /// `agent::agentic_loop::run_loop` (P3-5b3 of the agent framework
    /// Pi-convergence remediation).
    ///
    /// - `thinking`: optional extended-thinking text. Empty strings are
    ///   treated as absent (no Thinking block is emitted).
    /// - `thinking_signature`: optional signature accompanying Thinking
    ///   (currently only Anthropic supplies one).
    /// - `text`: the assistant's visible text reply. Always included as a
    ///   single Text block, even if empty.
    /// - `tool_uses`: zero or more `(id, name, input)` tuples for tool
    ///   calls the model emitted. Iterated in the provided order.
    pub fn assistant_from_response(
        thinking: Option<&str>,
        thinking_signature: Option<String>,
        text: &str,
        tool_uses: impl IntoIterator<Item = (String, String, serde_json::Value)>,
    ) -> Self {
        let mut blocks = Vec::new();
        if let Some(t) = thinking {
            if !t.is_empty() {
                blocks.push(ContentBlock::Thinking {
                    thinking: t.to_string(),
                    signature: thinking_signature,
                });
            }
        }
        blocks.push(ContentBlock::Text {
            text: text.to_string(),
        });
        for (id, name, input) in tool_uses {
            blocks.push(ContentBlock::ToolUse { id, name, input });
        }
        Self {
            role: MessageRole::Assistant,
            content: blocks,
            compacted: false,
        }
    }
}

/// CJK-aware token estimation (fallback when tiktoken is unavailable).
pub fn estimate_tokens(text: &str) -> u32 {
    let mut tokens: f32 = 0.0;
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() {
            tokens += 0.25;
        } else if ch.is_ascii_digit() {
            tokens += 0.4;
        } else if is_cjk(ch) {
            tokens += 1.1;
        } else if ch == '\n' {
            tokens += 1.0;
        } else if ch.is_whitespace() {
            tokens += 0.15;
        } else {
            tokens += 0.5;
        }
    }
    tokens.ceil() as u32 + 4 // message overhead
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x4E00..=0x9FFF |   // CJK Unified Ideographs
        0x3400..=0x4DBF |   // CJK Extension A
        0x3000..=0x303F |   // CJK Symbols & Punctuation
        0x3040..=0x309F |   // Hiragana
        0x30A0..=0x30FF |   // Katakana
        0xAC00..=0xD7AF     // Hangul
    )
}

/// Estimate tokens for a single ChatMessage.
pub fn estimate_message_tokens(msg: &ChatMessage) -> u32 {
    msg.content
        .iter()
        .map(|b| match b {
            ContentBlock::Text { text } => estimate_tokens(text),
            ContentBlock::Thinking { thinking, .. } => estimate_tokens(thinking),
            ContentBlock::ToolUse { input, name, .. } => {
                estimate_tokens(name) + estimate_tokens(&input.to_string()) + 10
            }
            ContentBlock::ToolResult { content, .. } => estimate_tokens(content) + 5,
        })
        .sum()
}

#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
