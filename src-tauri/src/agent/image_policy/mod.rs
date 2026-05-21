//! M2-H L5 — Image stripping for image-blind providers.
//!
//! Each LLM provider+model has a `supports_images` capability. When
//! the agent has uploaded screenshots, attachments, or browser
//! captures into the conversation but the active model can't see
//! images, the request fails (or worse, silently ignores the image
//! and the model hallucinates). L5 catches this before the request
//! goes out: detect image content blocks in any message, replace
//! them with a short placeholder string that tells the model "an
//! image was here but the current model can't see it".
//!
//! Why "stripping" matters even when the request would succeed:
//!
//! - Base64-encoded images can be **megabytes**. A single screenshot
//!   easily exceeds the per-request body limit on some providers.
//! - Even when the request succeeds, transmitting + paying for the
//!   image bytes when the model is image-blind is pure waste.
//!
//! Two image content shapes are recognized (these cover Anthropic +
//! OpenAI + Gemini message formats, which uClaw routes through):
//!
//! 1. **Anthropic-style**: `{"type": "image", "source": {...}}`
//! 2. **OpenAI-style**:   `{"type": "image_url", "image_url": {...}}`
//!
//! The replacement is **shape-preserving**: where the original block
//! sat, a `{"type": "text", "text": <placeholder>}` block takes its
//! place. The message structure around the block is left intact.
//!
//! Layout:
//!
//! - [`policy`] — `ProviderCaps`, `ImagePolicy`, `strip_images`, `StripStats`

pub mod policy;

pub use policy::{
    strip_images, ImagePolicy, ProviderCaps, StripStats, DEFAULT_PLACEHOLDER,
};
