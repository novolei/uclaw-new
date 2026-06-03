// SPDX-License-Identifier: Apache-2.0
//! MiniCPM ChatML prompt rendering — pure functions, no model/tokenizer.
//!
//! MiniCPM5-1B ships no `chat_template` in its tokenizer_config; it uses the
//! canonical ChatML layout with `<|im_start|>`/`<|im_end|>` role markers
//! (verified from openbmb/MiniCPM5-1B/tokenizer_config.json). We render that
//! layout and lock it here; a wrong template surfaces as garbage in the gated
//! engine smoke test.

/// One chat message in role/content form (matches the OpenAI wire shape).
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Render messages into MiniCPM ChatML and open the assistant turn.
///
/// Layout (per role): `<|im_start|>{role}\n{content}<|im_end|>\n`, then a final
/// `<|im_start|>assistant\n` to prompt generation. The `<s>` BOS is added by the
/// tokenizer at encode time (`add_special_tokens = true`), not here.
pub fn render_chatml(messages: &[ChatMessage]) -> String {
    let mut out = String::new();
    for m in messages {
        out.push_str("<|im_start|>");
        out.push_str(&m.role);
        out.push('\n');
        out.push_str(&m.content);
        out.push_str("<|im_end|>\n");
    }
    out.push_str("<|im_start|>assistant\n");
    out
}

/// Like `render_chatml` but prefills `<think>\n</think>\n` in the assistant
/// turn to suppress chain-of-thought reasoning (budget_tokens=0 technique).
///
/// MiniCPM5-1B is a reasoning model: left unconstrained, the first tokens it
/// generates are always `<think>…</think>` before the actual answer.  For
/// short-budget utility calls (title generation, summarisation) we want the
/// answer directly.  Prefilling the empty thinking block forces the model to
/// skip CoT and emit the answer immediately, just as `budget_tokens=0` does in
/// the Python reference implementation.
pub fn render_chatml_no_think(messages: &[ChatMessage]) -> String {
    let mut out = render_chatml(messages);
    out.push_str("<think>\n</think>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.into(), content: content.into() }
    }

    #[test]
    fn single_user_turn_opens_assistant() {
        let out = render_chatml(&[msg("user", "hi")]);
        assert_eq!(
            out,
            "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n"
        );
    }

    #[test]
    fn system_then_user() {
        let out = render_chatml(&[msg("system", "You are clawby."), msg("user", "2+2=")]);
        assert_eq!(
            out,
            "<|im_start|>system\nYou are clawby.<|im_end|>\n\
             <|im_start|>user\n2+2=<|im_end|>\n\
             <|im_start|>assistant\n"
        );
    }

    #[test]
    fn multi_turn_includes_prior_assistant() {
        let out = render_chatml(&[
            msg("user", "hi"),
            msg("assistant", "hello!"),
            msg("user", "bye"),
        ]);
        assert_eq!(
            out,
            "<|im_start|>user\nhi<|im_end|>\n\
             <|im_start|>assistant\nhello!<|im_end|>\n\
             <|im_start|>user\nbye<|im_end|>\n\
             <|im_start|>assistant\n"
        );
    }

    #[test]
    fn unknown_role_passes_through_verbatim() {
        let out = render_chatml(&[msg("tool", "result=4")]);
        assert_eq!(
            out,
            "<|im_start|>tool\nresult=4<|im_end|>\n<|im_start|>assistant\n"
        );
    }

    #[test]
    fn empty_messages_still_opens_assistant() {
        assert_eq!(render_chatml(&[]), "<|im_start|>assistant\n");
    }

    #[test]
    fn no_think_appends_empty_think_block() {
        let out = render_chatml_no_think(&[msg("user", "2+2=")]);
        assert_eq!(
            out,
            "<|im_start|>user\n2+2=<|im_end|>\n<|im_start|>assistant\n<think>\n</think>\n"
        );
    }
}
