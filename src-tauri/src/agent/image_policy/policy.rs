//! `strip_images` — replace image content blocks with placeholder
//! text when the active provider/model is image-blind.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Default placeholder content for a stripped image block. Matches
/// the wording suggested in ADR §M2-H L5 so the model gets a clear
/// signal that the user/agent attempted to share an image.
pub const DEFAULT_PLACEHOLDER: &str =
    "image content omitted because the current model does not support image input";

/// Per-provider/per-model capability snapshot. Only the
/// `supports_images` flag matters to L5; the other fields are kept
/// so callers can log decisions clearly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCaps {
    /// Provider id, e.g. "anthropic", "openai", "google", "ollama".
    pub provider: String,
    /// Model id within that provider, e.g. "claude-sonnet-4-5",
    /// "gpt-4o", "gemini-2.5-pro".
    pub model: String,
    /// `true` if this provider+model accepts image content blocks.
    /// When `false`, [`strip_images`] swaps each image block for a
    /// `{"type": "text", "text": <placeholder>}` block.
    pub supports_images: bool,
}

impl ProviderCaps {
    /// Convenience: a fully image-blind cap snapshot.
    pub fn image_blind(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            supports_images: false,
        }
    }

    /// Convenience: an image-capable cap snapshot.
    pub fn image_capable(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            supports_images: true,
        }
    }
}

/// Configuration for the stripper. Currently just the placeholder
/// text — kept as a struct so future settings (per-provider
/// override, attach-as-text-summary mode) slot in without API churn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePolicy {
    pub placeholder: String,
}

impl ImagePolicy {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
        }
    }
}

impl Default for ImagePolicy {
    fn default() -> Self {
        Self::new(DEFAULT_PLACEHOLDER)
    }
}

/// What the stripper found and replaced. Useful for the M2-J
/// token-budget UI ("4 images stripped because model is gpt-4o-mini").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StripStats {
    /// Number of Anthropic-style `{"type":"image"}` blocks replaced.
    pub anthropic_image_blocks: usize,
    /// Number of OpenAI-style `{"type":"image_url"}` blocks replaced.
    pub openai_image_url_blocks: usize,
}

impl StripStats {
    /// Total blocks the stripper replaced.
    pub fn total(&self) -> usize {
        self.anthropic_image_blocks + self.openai_image_url_blocks
    }

    /// `true` when nothing was stripped.
    pub fn is_noop(&self) -> bool {
        self.total() == 0
    }
}

/// Walk `value` recursively. If `caps.supports_images` is `true`,
/// return the input unchanged. Otherwise replace every detected
/// image content block with `{"type": "text", "text": <placeholder>}`.
///
/// **Shape recognition** — a block counts as image content when it
/// is an object AND has one of:
///
/// - `type == "image"` (Anthropic) — replaces the whole object
/// - `type == "image_url"` (OpenAI) — replaces the whole object
///
/// The replacement is in-place at the original position; surrounding
/// structure is untouched.
pub fn strip_images(
    value: Value,
    caps: &ProviderCaps,
    policy: &ImagePolicy,
) -> (Value, StripStats) {
    let mut stats = StripStats::default();
    if caps.supports_images {
        return (value, stats);
    }
    let out = visit(value, &policy.placeholder, &mut stats);
    (out, stats)
}

// ── internals ──────────────────────────────────────────────────────

fn visit(v: Value, placeholder: &str, stats: &mut StripStats) -> Value {
    match v {
        Value::Object(map) => {
            // First check: is this object itself an image block?
            if let Some(kind) = image_block_kind(&map) {
                match kind {
                    ImageKind::Anthropic => stats.anthropic_image_blocks += 1,
                    ImageKind::OpenAi => stats.openai_image_url_blocks += 1,
                }
                return text_placeholder(placeholder);
            }
            // Otherwise recurse into every value.
            let mut out = Map::with_capacity(map.len());
            for (k, val) in map.into_iter() {
                out.insert(k, visit(val, placeholder, stats));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| visit(item, placeholder, stats))
                .collect(),
        ),
        scalar => scalar,
    }
}

#[derive(Debug, Clone, Copy)]
enum ImageKind {
    /// `{"type": "image", "source": {...}}` — Anthropic + Gemini-ish.
    Anthropic,
    /// `{"type": "image_url", "image_url": {...}}` — OpenAI.
    OpenAi,
}

/// Detect whether `map` *is* an image content block. Returns `None`
/// for any other object shape (regular text block, tool_use, etc.).
fn image_block_kind(map: &Map<String, Value>) -> Option<ImageKind> {
    let ty = map.get("type")?.as_str()?;
    match ty {
        "image" => {
            // Be conservative — require a `source` field too so we
            // don't false-positive on a schema field named "type":
            // "image" that isn't a content block.
            if map.contains_key("source") {
                Some(ImageKind::Anthropic)
            } else {
                None
            }
        }
        "image_url" => {
            if map.contains_key("image_url") {
                Some(ImageKind::OpenAi)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn text_placeholder(placeholder: &str) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), Value::String("text".into()));
    m.insert("text".into(), Value::String(placeholder.into()));
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn blind() -> ProviderCaps {
        ProviderCaps::image_blind("anthropic", "claude-haiku-text-only")
    }

    fn seeing() -> ProviderCaps {
        ProviderCaps::image_capable("anthropic", "claude-sonnet-4-5")
    }

    // ── pass-through when supports_images = true ────────────────────

    #[test]
    fn passes_through_unchanged_when_model_supports_images() {
        let payload = json!({
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "look at this:"},
                        {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": "image/png",
                                "data": "iVBORw0KGgo..."
                            }
                        }
                    ]
                }
            ]
        });
        let (out, stats) = strip_images(payload.clone(), &seeing(), &ImagePolicy::default());
        assert!(stats.is_noop());
        assert_eq!(out, payload);
    }

    #[test]
    fn empty_payload_is_noop() {
        let payload = json!({});
        let (out, stats) = strip_images(payload.clone(), &blind(), &ImagePolicy::default());
        assert!(stats.is_noop());
        assert_eq!(out, payload);
    }

    #[test]
    fn pure_text_messages_are_noop() {
        let payload = json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "hello"}]}
            ]
        });
        let (out, stats) = strip_images(payload.clone(), &blind(), &ImagePolicy::default());
        assert!(stats.is_noop());
        assert_eq!(out, payload);
    }

    // ── Anthropic-shape stripping ───────────────────────────────────

    #[test]
    fn strips_anthropic_image_block_to_text_placeholder() {
        let payload = json!({
            "content": [
                {"type": "text", "text": "see image:"},
                {
                    "type": "image",
                    "source": {"type": "base64", "media_type": "image/png", "data": "abc"}
                }
            ]
        });
        let (out, stats) = strip_images(payload, &blind(), &ImagePolicy::default());
        assert_eq!(stats.anthropic_image_blocks, 1);
        assert_eq!(stats.openai_image_url_blocks, 0);
        assert_eq!(stats.total(), 1);
        // Image block replaced; text block untouched.
        let content = &out["content"];
        assert_eq!(content[0], json!({"type": "text", "text": "see image:"}));
        assert_eq!(
            content[1],
            json!({"type": "text", "text": DEFAULT_PLACEHOLDER})
        );
    }

    #[test]
    fn anthropic_image_without_source_is_not_stripped() {
        // Conservative recognition: an object with `type: "image"` but
        // no `source` field is treated as schema field, not content.
        let payload = json!({
            "type": "image",
            "label": "this is a JSON schema field, not a content block"
        });
        let (out, stats) = strip_images(payload.clone(), &blind(), &ImagePolicy::default());
        assert!(stats.is_noop());
        assert_eq!(out, payload);
    }

    // ── OpenAI-shape stripping ──────────────────────────────────────

    #[test]
    fn strips_openai_image_url_block() {
        let payload = json!({
            "content": [
                {"type": "text", "text": "look at:"},
                {
                    "type": "image_url",
                    "image_url": {"url": "data:image/png;base64,abc..."}
                }
            ]
        });
        let (out, stats) = strip_images(payload, &blind(), &ImagePolicy::default());
        assert_eq!(stats.openai_image_url_blocks, 1);
        assert_eq!(stats.anthropic_image_blocks, 0);
        assert_eq!(
            out["content"][1],
            json!({"type": "text", "text": DEFAULT_PLACEHOLDER})
        );
    }

    #[test]
    fn openai_image_url_without_payload_field_not_stripped() {
        let payload =
            json!({"type": "image_url", "note": "just a field named image_url somewhere"});
        let (out, stats) = strip_images(payload.clone(), &blind(), &ImagePolicy::default());
        assert!(stats.is_noop());
        assert_eq!(out, payload);
    }

    // ── multiple images / mixed shapes ──────────────────────────────

    #[test]
    fn strips_multiple_mixed_shape_images() {
        let payload = json!({
            "messages": [
                {
                    "content": [
                        {"type": "image", "source": {"type": "url", "url": "a"}},
                        {"type": "text", "text": "and"},
                        {"type": "image_url", "image_url": {"url": "b"}}
                    ]
                },
                {
                    "content": [
                        {"type": "image", "source": {"type": "base64", "data": "c"}}
                    ]
                }
            ]
        });
        let (_, stats) = strip_images(payload, &blind(), &ImagePolicy::default());
        assert_eq!(stats.anthropic_image_blocks, 2);
        assert_eq!(stats.openai_image_url_blocks, 1);
        assert_eq!(stats.total(), 3);
    }

    // ── nested locations ────────────────────────────────────────────

    #[test]
    fn finds_image_nested_deeply() {
        let payload = json!({
            "wrapper": {
                "deeper": {
                    "messages": [
                        {
                            "content": [
                                {"type": "image", "source": {"x": 1}}
                            ]
                        }
                    ]
                }
            }
        });
        let (out, stats) = strip_images(payload, &blind(), &ImagePolicy::default());
        assert_eq!(stats.anthropic_image_blocks, 1);
        assert_eq!(
            out["wrapper"]["deeper"]["messages"][0]["content"][0]["type"],
            "text"
        );
    }

    // ── custom placeholder ──────────────────────────────────────────

    #[test]
    fn custom_placeholder_used_in_replacement() {
        let policy = ImagePolicy::new("[image redacted]");
        let payload = json!({
            "content": [
                {"type": "image", "source": {"x": 1}}
            ]
        });
        let (out, stats) = strip_images(payload, &blind(), &policy);
        assert_eq!(stats.total(), 1);
        assert_eq!(out["content"][0]["text"], "[image redacted]");
    }

    // ── idempotency ─────────────────────────────────────────────────

    #[test]
    fn second_pass_is_noop() {
        let payload = json!({
            "content": [
                {"type": "image", "source": {"x": 1}},
                {"type": "image_url", "image_url": {"url": "a"}}
            ]
        });
        let (once, s1) = strip_images(payload, &blind(), &ImagePolicy::default());
        assert_eq!(s1.total(), 2);
        let (twice, s2) = strip_images(once.clone(), &blind(), &ImagePolicy::default());
        assert!(s2.is_noop(), "stripped output has no image blocks left");
        assert_eq!(once, twice);
    }

    // ── ProviderCaps convenience constructors ───────────────────────

    #[test]
    fn provider_caps_image_blind_factory() {
        let c = ProviderCaps::image_blind("ollama", "llama3");
        assert!(!c.supports_images);
        assert_eq!(c.provider, "ollama");
        assert_eq!(c.model, "llama3");
    }

    #[test]
    fn provider_caps_image_capable_factory() {
        let c = ProviderCaps::image_capable("openai", "gpt-4o");
        assert!(c.supports_images);
    }

    #[test]
    fn provider_caps_serde_roundtrip() {
        let c = ProviderCaps::image_blind("anthropic", "haiku");
        let json = serde_json::to_string(&c).unwrap();
        let back: ProviderCaps = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
