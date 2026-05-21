//! M2-H L5 capability lookup — does `(provider, model)` accept image input?
//!
//! Returns `false` for known text-only models so the dispatcher can
//! strip image content blocks BEFORE the request goes out. Defaults to
//! `true` (assume vision-capable) for unknown ids — uClaw's call path
//! must be able to handle a 400 cleanly if the model turns out to be
//! blind, but the cost of a false-positive strip (lost vision turn) is
//! higher than a false-negative (one failed request, then we add the
//! model to the deny list).
//!
//! The match is **lowercase-and-substring** so common model id
//! variations ("deepseek-v4-pro", "deepseek-chat", "DeepSeek-V3") all
//! resolve consistently.

/// `true` if `(provider, model)` accepts image content blocks in
/// requests. Provider/model are matched case-insensitively as
/// substrings of the lowercased model id when the provider doesn't
/// pin the answer.
pub fn supports_images(provider: &str, model: &str) -> bool {
    let provider = provider.trim().to_lowercase();
    let model = model.trim().to_lowercase();

    // Provider-level decisions first. These take priority and short-
    // circuit the per-model check below.
    match provider.as_str() {
        // Anthropic: every modern Claude (3+, 4.x, 4.5, 4.6) supports
        // images. Older "claude-instant" was text-only but is EOL — we
        // assume any "claude-*" id today is image-capable.
        "anthropic" => return true,

        // OpenAI: only the gpt-4o / gpt-4-vision family + o1/o3/o4
        // reasoning models accept images. Older gpt-4 turbo + gpt-3.5
        // are text-only.
        "openai" => {
            // Image-capable substrings.
            if model.contains("gpt-4o")
                || model.contains("gpt-4.1")
                || model.contains("gpt-4-vision")
                || model.contains("o1")
                || model.contains("o3")
                || model.contains("o4")
            {
                return true;
            }
            // Known text-only.
            return false;
        }

        // DeepSeek: as of 2025, all officially supported models are
        // text-only. Strip aggressively.
        "deepseek" => return false,

        // Google Gemini: 1.5 + 2.5 + flash family all support vision.
        "google" | "gemini" => return true,

        // Moonshot Kimi: latest moonshot-v1-vision models support
        // images, the base chat models don't.
        "moonshot" => return model.contains("vision"),

        // Alibaba Cloud Qwen: qwen-vl-* are vision; qwen-max / qwen-turbo
        // / qwen-plus are text-only.
        "aliyun" | "qwen" => return model.contains("vl") || model.contains("vision"),

        // Mistral: pixtral-* is vision; plain mistral / codestral are
        // text-only.
        "mistral" => return model.contains("pixtral"),

        // Local OSS (ollama / llama.cpp): assume blind unless explicit
        // multimodal marker — bigger Llava/Bakllava are vision, raw
        // llama is text-only.
        "ollama" => return model.contains("llava") || model.contains("bakllava"),

        // ZAI / Zhipu GLM: glm-4v* is vision.
        "zai" | "zhipu" => return model.contains("4v") || model.contains("vision"),

        // xAI Grok: grok-2-vision exists; default grok models are
        // text-only.
        "xai" => return model.contains("vision"),

        _ => {}
    }

    // Unknown provider — fall back to inferring from the model id.
    if model.starts_with("claude") {
        return true;
    }
    if model.contains("gpt-4o")
        || model.contains("gpt-4.1")
        || model.contains("o1")
        || model.contains("o3")
        || model.contains("o4")
        || model.contains("gemini")
        || model.contains("pixtral")
        || model.contains("llava")
        || model.contains("vision")
    {
        return true;
    }
    if model.starts_with("deepseek") {
        return false;
    }

    // Default: assume vision-capable. Better to ship the image and
    // catch the 400 than silently drop a screenshot the user expects
    // the model to read.
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_always_supports_images() {
        assert!(supports_images("anthropic", "claude-sonnet-4-6"));
        assert!(supports_images("anthropic", "claude-haiku-4-5"));
        assert!(supports_images("anthropic", "claude-opus-4-6"));
    }

    #[test]
    fn openai_only_vision_capable_substrings_pass() {
        assert!(supports_images("openai", "gpt-4o"));
        assert!(supports_images("openai", "gpt-4o-mini"));
        assert!(supports_images("openai", "gpt-4.1"));
        assert!(supports_images("openai", "o1-preview"));
        assert!(supports_images("openai", "o3-mini"));
        assert!(!supports_images("openai", "gpt-3.5-turbo"));
        assert!(!supports_images("openai", "gpt-4-turbo"));
    }

    #[test]
    fn deepseek_is_blind() {
        assert!(!supports_images("deepseek", "deepseek-v4-pro"));
        assert!(!supports_images("deepseek", "deepseek-chat"));
        assert!(!supports_images("deepseek", "DeepSeek-V3"));
    }

    #[test]
    fn google_gemini_supports_images() {
        assert!(supports_images("google", "gemini-2.5-pro"));
        assert!(supports_images("gemini", "gemini-1.5-flash"));
    }

    #[test]
    fn moonshot_only_vision_variants() {
        assert!(supports_images("moonshot", "moonshot-v1-vision"));
        assert!(!supports_images("moonshot", "moonshot-v1-8k"));
        assert!(!supports_images("moonshot", "kimi-k1"));
    }

    #[test]
    fn case_insensitive_matching() {
        assert!(supports_images("Anthropic", "Claude-Sonnet-4-6"));
        assert!(!supports_images("DEEPSEEK", "DeepSeek-V3"));
    }

    #[test]
    fn unknown_provider_infers_from_model_id() {
        // claude-* on a generic OpenAI-compatible endpoint
        assert!(supports_images("", "claude-sonnet-4-6"));
        // Some custom DeepSeek deployment
        assert!(!supports_images("custom", "deepseek-v3-chat"));
    }

    #[test]
    fn unknown_provider_unknown_model_defaults_vision_capable() {
        // Better to ship and see 400 than silently drop screenshots.
        assert!(supports_images("brandnew-co", "some-flagship-model"));
    }
}
