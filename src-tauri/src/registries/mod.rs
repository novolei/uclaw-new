//! M3-T1 — Five-registry skeleton.
//!
//! ADR §"Capability Mesh" calls for **five Registry surfaces** the
//! capability mesh resolves against:
//!
//! | Registry | What lives here |
//! |---|---|
//! | `skills`     | uClaw skills (built-in + plugin-installed) |
//! | `connectors` | MCP servers + chrome/excel/native connectors |
//! | `tools`      | Individual tool descriptors (resolved by name) |
//! | `models`     | LLM providers/models + their capabilities |
//! | `themes`     | UI themes (light/dark/custom) for plugin-extended UIs |
//!
//! All five share a tiny common shape: `id` → entry, with `kind` +
//! optional `tags` for filtering. This pilot ships the **generic
//! `Registry<E>` container** + a `RegistryEntry` trait + a typed
//! façade per registry so callers get strongly-typed entries while
//! the storage layer stays uniform.
//!
//! What's NOT in this PR:
//!
//! - **Wire-up** into a global RegistryHub that the agent dispatcher
//!   queries — that lives in M3-T1 commit 2.
//! - **Hot-reload** from `~/.uclaw/registries/*.toml` — M3-T1 commit 3.
//! - **Capability mesh resolver** that turns `CapabilityQuery`
//!   (M1-T1) into a concrete `RegistryEntry` — that's M3-T2.
//!
//! Layout:
//!
//! - [`entry`]   — `RegistryEntry` trait + `RegistryError`
//! - [`store`]   — generic `Registry<E>` container
//! - [`skills`]  — typed `SkillEntry`
//! - [`connectors`] — typed `ConnectorEntry`
//! - [`tools`]   — typed `ToolEntry`
//! - [`models`]  — typed `ModelEntry`
//! - [`themes`]  — typed `ThemeEntry`

pub mod connectors;
pub mod entry;
pub mod models;
pub mod skills;
pub mod store;
pub mod themes;
pub mod tools;

pub use connectors::ConnectorEntry;
pub use entry::{RegistryEntry, RegistryError};
pub use models::ModelEntry;
pub use skills::SkillEntry;
pub use store::Registry;
pub use themes::ThemeEntry;
pub use tools::ToolEntry;
