//! `ModelEntry` — LLM provider + model capability snapshot.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::entry::RegistryEntry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    /// Fully-qualified id: `"provider::model"`, e.g.
    /// `"anthropic::claude-sonnet-4-5"`.
    pub id: String,
    /// Provider id, kept duplicated for `kind()` indexing.
    pub kind: String,
    /// Short model display name, e.g. "Sonnet 4.5".
    pub display_name: String,
    /// Maximum context window in tokens.
    pub context_window_tokens: u64,
    /// `true` if the model accepts image content blocks (M2-H L5).
    pub supports_images: bool,
    /// `true` if the model exposes prompt caching (M2-I).
    pub supports_prompt_cache: bool,
    /// Per-million-token cost in micro-USD. Optional — set to None for
    /// local / self-hosted models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_cost_micros_per_mtok: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_cost_micros_per_mtok: Option<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
}

impl RegistryEntry for ModelEntry {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &str {
        &self.kind
    }
    fn tags(&self) -> &BTreeMap<String, String> {
        &self.tags
    }
}
