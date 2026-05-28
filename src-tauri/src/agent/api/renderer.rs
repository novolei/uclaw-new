//! Custom message renderer registration shape.
//!
//! A renderer takes a custom-typed message payload and returns a UI-displayable
//! string. The dispatcher invokes renderers keyed by `custom_type`.

use std::sync::Arc;

pub type RendererFn = Arc<
    dyn Fn(&serde_json::Value) -> Result<String, String> + Send + Sync,
>;

/// Wrapper around the function alias so callers can pass `Renderer { custom_type, render }`
/// instead of a bare tuple at the register site.
#[derive(Clone)]
pub struct Renderer {
    pub custom_type: &'static str,
    pub render: RendererFn,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("custom_type", &self.custom_type)
            .field("render", &"<fn>")
            .finish()
    }
}
