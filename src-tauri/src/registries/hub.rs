//! M3-T1 wire-up slice 1 — `RegistryHub` + Skills bridge.
//!
//! The M3-T1 pilot shipped 5 typed `Registry<E>` containers and an
//! M3-T2 resolver, but nothing in production *populated* them. This
//! module ships the **hub**: a single Arc-wrapped, RwLock-guarded
//! aggregate of all 5 registries that lives on `AppState`, plus the
//! `sync_skills_from_registry` bridge that mirrors entries from the
//! existing `SkillsRegistry` (disk-tier) into `Registry<SkillEntry>`
//! so the resolver can hit them.
//!
//! ## Why a separate hub instead of folding into SkillsRegistry?
//!
//! - `SkillsRegistry` (skills.rs) is **disk-tier** — it scans
//!   directories for SKILL.md files and tracks per-file provenance.
//!   Its model is "file → LoadedSkill". It owns the parse + watch
//!   + reload semantics.
//! - `Registry<SkillEntry>` (registries/skills.rs) is **mesh-tier** —
//!   it's the index the Capability Mesh resolver runs queries
//!   against. Its model is "id → typed entry with tags". It owns the
//!   scoring + lookup + listing API the resolver expects.
//!
//! Keeping them separate lets each evolve independently: disk-tier
//! can add new file-format support without touching mesh contracts;
//! mesh-tier can add new resolution strategies without rewriting the
//! disk parser. The `sync_skills_from_registry` function is the
//! one-way bridge.
//!
//! ## Slice 1 scope
//!
//! - Hub struct with all 5 `Arc<RwLock<Registry<E>>>` slots
//! - `sync_skills_from_registry` bridge — populates Skills slot from
//!   `SkillsRegistry::all_skills()`
//! - Call site in `AppState::new` after `skills_reg.discover()`
//! - Tests covering the sync path and the resolver running against
//!   the populated hub
//!
//! ## What's NOT in slice 1 (intentionally deferred)
//!
//! - **Tools wire-up** — `agent::tools::ToolRegistry` lives per-
//!   conversation in `tauri_commands.rs::send_agent_message`, not at
//!   AppState scope. Wiring it requires either making the hub
//!   per-session or registering tools eagerly at boot. Slice 2.
//! - **Connectors wire-up** — MCP manager has dynamic
//!   connect/disconnect; the hub needs a subscription, not a one-
//!   shot sync. Slice 3.
//! - **Models wire-up** — provider list comes from `ProviderService`
//!   which reads `providers.json`. Easy sync; Slice 2.
//! - **Themes wire-up** — no themes registry exists yet outside
//!   what's bundled in `theme-factory` skill. Slice 4 (or skip if
//!   themes stay UI-only).
//! - **Resolver invocation in production code paths** — slice 1 just
//!   makes the data available. Calling the resolver from
//!   `skill_search` / `load_skill` (and folding it into ranked
//!   results) is slice 2.

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::registries::{
    ConnectorEntry, ModelEntry, Registry, RegistryError, SkillEntry, ThemeEntry, ToolEntry,
};

/// Aggregate of the 5 typed registries the Capability Mesh resolves
/// against. Each slot is `Arc<RwLock<...>>` so the hub can be shared
/// across threads (the agent loop spawns onto tokio tasks) without
/// touching the surrounding `AppState` lock.
#[derive(Clone, Debug)]
pub struct RegistryHub {
    pub skills: Arc<RwLock<Registry<SkillEntry>>>,
    pub connectors: Arc<RwLock<Registry<ConnectorEntry>>>,
    pub tools: Arc<RwLock<Registry<ToolEntry>>>,
    pub models: Arc<RwLock<Registry<ModelEntry>>>,
    pub themes: Arc<RwLock<Registry<ThemeEntry>>>,
}

impl RegistryHub {
    /// Construct an empty hub. Call sites populate each registry via
    /// a sync function (e.g. `sync_skills_from_registry`) or by
    /// registering entries directly.
    pub fn new() -> Self {
        Self {
            skills: Arc::new(RwLock::new(Registry::new())),
            connectors: Arc::new(RwLock::new(Registry::new())),
            tools: Arc::new(RwLock::new(Registry::new())),
            models: Arc::new(RwLock::new(Registry::new())),
            themes: Arc::new(RwLock::new(Registry::new())),
        }
    }

    /// Snapshot of the current slot counts. Useful for diagnostics
    /// (Settings panel "Registry contents" view, log lines on boot,
    /// etc.) without forcing every reader to take 5 locks.
    pub async fn counts(&self) -> RegistryHubCounts {
        RegistryHubCounts {
            skills: self.skills.read().await.list().len(),
            connectors: self.connectors.read().await.list().len(),
            tools: self.tools.read().await.list().len(),
            models: self.models.read().await.list().len(),
            themes: self.themes.read().await.list().len(),
        }
    }
}

impl Default for RegistryHub {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-slot entry count. Returned by [`RegistryHub::counts`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RegistryHubCounts {
    pub skills: usize,
    pub connectors: usize,
    pub tools: usize,
    pub models: usize,
    pub themes: usize,
}

impl RegistryHubCounts {
    pub fn total(&self) -> usize {
        self.skills + self.connectors + self.tools + self.models + self.themes
    }
}

// ───────────────────────────────────────────────────────────────────
// Bridges — sync from existing uClaw subsystems into the hub
// ───────────────────────────────────────────────────────────────────

/// Mirror entries from `crate::skills::SkillsRegistry` into the
/// hub's `skills` slot. Idempotent — re-running clears the slot
/// first so a fresh disk scan replaces stale entries (the disk-tier
/// registry has its own discover() that's the source of truth).
///
/// Returns the number of entries written.
///
/// Why this lives here instead of `skills.rs`:
/// - keeps `skills.rs` free of the M3 registries dep (one-way
///   coupling: registries can import skills, not vice versa)
/// - the bridge is the only file that needs to know both schemas
pub async fn sync_skills_from_registry(
    hub: &RegistryHub,
    src: &crate::skills::SkillsRegistry,
) -> Result<usize, RegistryError> {
    let mut dst = hub.skills.write().await;
    // Idempotent semantics: wipe + repopulate. The set-of-entries is
    // small (typical install has < 50 skills); repopulation cost is
    // negligible relative to a registry.discover() disk walk.
    *dst = Registry::new();

    let mut count = 0usize;
    for skill in src.all_loaded_skills() {
        let manifest = &skill.manifest;
        let entry = SkillEntry {
            id: manifest.name.clone(),
            // SkillsRegistry's `SkillProvenance` (Bundled / User /
            // Project / Marketplace) maps cleanly to the registry
            // entry's `kind`. The resolver scores `+50` on kind match.
            kind: provenance_to_kind(&skill.provenance).to_string(),
            title: manifest.name.clone(),
            description: manifest.description.clone(),
            // Best-effort estimate; the SkillsRegistry doesn't track
            // a real token count today. The body length is a passable
            // proxy until we wire L3's per-turn skills top-K through
            // here (PR #312 era).
            token_estimate: skill.prompt_content.len() / 4,
            tags: extract_tags(skill),
        };
        // `register` errors on duplicate id; treat that as a soft
        // warning so a single bad SKILL.md doesn't break the whole
        // sync. Future hardening: collect into a `Vec<RegistryError>`
        // and surface to Settings.
        match dst.register(entry) {
            Ok(()) => count += 1,
            Err(e) => {
                tracing::warn!(
                    skill_id = %manifest.name,
                    error = %e,
                    "[M3-T1] skill sync skipped duplicate id"
                );
            }
        }
    }
    Ok(count)
}

fn provenance_to_kind(p: &crate::skills::SkillProvenance) -> &'static str {
    match p {
        crate::skills::SkillProvenance::Bundled => "bundled",
        crate::skills::SkillProvenance::User => "user",
        crate::skills::SkillProvenance::Project => "project",
        crate::skills::SkillProvenance::Marketplace => "marketplace",
    }
}

/// Extract the SKILL.md frontmatter tags into the hub-entry's
/// `BTreeMap<String, String>`. Activation tags become `tag:<value>=1`
/// entries the resolver can match on via `query.tags`. Keeps things
/// simple — domain-specific tag shapes can be added later without
/// breaking the schema (it's a string→string map).
fn extract_tags(
    skill: &crate::skills::LoadedSkill,
) -> std::collections::BTreeMap<String, String> {
    let mut tags = std::collections::BTreeMap::new();
    for tag in &skill.lowercased_tags {
        tags.insert(format!("tag:{}", tag), "1".to_string());
    }
    // Surface provenance as a tag too so resolver queries can use
    // both kind-match and tag-match (different score weights —
    // see resolver.rs).
    tags.insert(
        "provenance".to_string(),
        provenance_to_kind(&skill.provenance).to_string(),
    );
    tags
}

// ───────────────────────────────────────────────────────────────────
// Slice 2 — Models bridge: ProviderService → Registry<ModelEntry>
// ───────────────────────────────────────────────────────────────────

/// Mirror the user's configured models (from `ProviderService`) into
/// the hub's `models` slot. Idempotent — wipe + repopulate, same as
/// the skills bridge.
///
/// Each `(provider_id, model_id)` pair from
/// `ProviderService::get_all_configured_models` becomes one
/// `ModelEntry` with:
/// - `id` = `"{provider_id}::{model_id}"` (the canonical fully-
///   qualified form the resolver uses)
/// - `kind` = `provider_id` (so the resolver's `+50` kind-match
///   boost works on "find me an Anthropic model" queries)
/// - `display_name` = `model_id` (rich names live in
///   `providers/registry.rs`; not surfaced here in slice 2 to keep
///   the bridge sync — enrichment is a slice 3 follow-up)
/// - `supports_prompt_cache` = `provider_id == "anthropic"` (the
///   only provider with explicit cache_control today; OpenAI's
///   automatic prompt cache is opaque to our path)
/// - `tags`: `provider=<id>` so callers can also resolve by tag
///
/// Returns the number of entries written.
pub async fn sync_models_from_provider_service(
    hub: &RegistryHub,
    provider_service: &crate::providers::service::ProviderService,
) -> Result<usize, RegistryError> {
    let configured = provider_service.get_all_configured_models().await;
    let mut dst = hub.models.write().await;
    *dst = Registry::new();

    let mut count = 0usize;
    for (provider_id, model_ids) in configured {
        for model_id in model_ids {
            let id = format!("{provider_id}::{model_id}");
            let mut tags = std::collections::BTreeMap::new();
            tags.insert("provider".to_string(), provider_id.clone());

            let entry = ModelEntry {
                id: id.clone(),
                kind: provider_id.clone(),
                display_name: model_id.clone(),
                // Context window / image support / cost would come
                // from `providers/registry.rs::all()` lookup. Slice 2
                // ships the minimal sync; an enrichment pass on top
                // of `KnownProvider::supports_model_list` lands in
                // slice 3 alongside dynamic MCP-tool tracking.
                context_window_tokens: 0,
                supports_images: false,
                supports_prompt_cache: provider_id == "anthropic",
                input_cost_micros_per_mtok: None,
                output_cost_micros_per_mtok: None,
                tags,
            };
            match dst.register(entry) {
                Ok(()) => count += 1,
                Err(e) => {
                    tracing::warn!(
                        model_id = %id,
                        error = %e,
                        "[M3-T1 slice 2] model sync skipped duplicate id"
                    );
                }
            }
        }
    }
    Ok(count)
}

// ───────────────────────────────────────────────────────────────────
// Slice 2 — Tools bridge: builtin tool catalog → Registry<ToolEntry>
// ───────────────────────────────────────────────────────────────────

/// Tools live in `agent::tools::ToolRegistry` which is per-session
/// (rebuilt every `send_agent_message`). Plumbing the per-session
/// registry into the hub would force the whole hub to be per-session
/// or require an event-bus subscription. Slice 2 takes a simpler
/// route: register a **static catalog** of the well-known builtin
/// tools at boot. This gives the resolver enough information to
/// answer "is there a tool named X" / "what kind of tool is X"
/// without entangling the hub with the agent loop's lifecycle.
///
/// MCP-provided tools (dynamic) stay out of slice 2 — slice 3
/// (Connectors wire-up) adds an event subscription on MCP
/// connect/disconnect that mirrors changes into both Tools and
/// Connectors slots.
///
/// The tool descriptors here are kept in lockstep with
/// `agent/tools/builtin/mod.rs` — if you add a new builtin module
/// there, add a matching entry here.
pub async fn register_builtin_tools(hub: &RegistryHub) -> Result<usize, RegistryError> {
    let mut dst = hub.tools.write().await;
    *dst = Registry::new();

    let catalog = builtin_tool_catalog();
    let mut count = 0usize;
    for (id, kind, description, requires_permission, tag_keys) in catalog {
        let mut tags = std::collections::BTreeMap::new();
        tags.insert("builtin".to_string(), "1".to_string());
        for tag_key in tag_keys {
            tags.insert(format!("tag:{}", tag_key), "1".to_string());
        }
        let entry = ToolEntry {
            id: id.to_string(),
            kind: kind.to_string(),
            description: description.to_string(),
            // Slice 2 keeps schema as `null` — the real JSON Schema
            // lives on the per-session `Tool::parameters_schema()`.
            // The resolver doesn't score on schema content; if a
            // later slice wants schema-aware matching, it can pull
            // from `ToolRegistry::list_definitions()` and update
            // this slot at session start.
            schema: serde_json::Value::Null,
            requires_permission,
            tags,
        };
        match dst.register(entry) {
            Ok(()) => count += 1,
            Err(e) => {
                tracing::warn!(
                    tool_id = id,
                    error = %e,
                    "[M3-T1 slice 2] builtin tool register failed"
                );
            }
        }
    }
    Ok(count)
}

/// The hard-coded list of builtin tools surfaced to the hub. Tuple
/// shape: `(id, kind, description, requires_permission, &[tag_keys])`.
/// `requires_permission` mirrors the real `Tool::requires_approval`
/// shape from `agent/tools/tool.rs` — kept in lockstep with the
/// per-session ToolRegistry's actual approval gating. Update this
/// list when new builtin tools land under `agent/tools/builtin/`.
fn builtin_tool_catalog()
    -> Vec<(&'static str, &'static str, &'static str, bool, &'static [&'static str])>
{
    vec![
        ("skill_search", "skill",
            "Search learned and built-in skills by query.",
            false, &["search", "skill"]),
        ("load_skill", "skill",
            "Load a specific skill into the current agent context.",
            false, &["skill"]),
        ("skill_write", "skill",
            "Author a new skill file under the user or project scope.",
            true, &["skill", "author"]),  // user-scope writes require approval
        ("skill_marketplace_search", "skill",
            "Search the public agent-skills ecosystem on GitHub / skills.sh.",
            false, &["skill", "marketplace", "search"]),
        ("skill_install_from_marketplace", "skill",
            "Install a skill from a GitHub source into the marketplace tier.",
            true, &["skill", "marketplace", "install"]),
        ("ask_user", "interaction",
            "Pose a clarifying question to the user mid-task.",
            false, &["interaction"]),
        ("plan", "planning",
            "Draft / update a structured plan for the current task.",
            false, &["plan"]),
        ("exit_plan_mode", "planning",
            "Leave plan mode after the user approves the drafted plan.",
            false, &["plan"]),
        ("plan_mode", "planning",
            "Enter plan mode for non-trivial tasks before execution.",
            false, &["plan"]),
        ("search", "filesystem",
            "Search file content / paths in the workspace.",
            false, &["filesystem", "search"]),
        ("shell", "filesystem",
            "Run a shell command in the workspace sandbox.",
            true, &["filesystem", "shell"]),
        ("edit", "filesystem",
            "Edit a file with surgical search-and-replace.",
            true, &["filesystem", "edit"]),
        ("file", "filesystem",
            "Read or write a file in the workspace.",
            true, &["filesystem"]),
        ("web", "network",
            "Fetch HTTP(S) content for the agent to read.",
            false, &["network", "web"]),
        ("self_eval", "meta",
            "Run a structured self-evaluation on the recent turn.",
            false, &["meta", "evaluation"]),
    ]
}

// ───────────────────────────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::contracts::CapabilityQuery;

    #[tokio::test]
    async fn empty_hub_reports_zero_counts() {
        let hub = RegistryHub::new();
        let c = hub.counts().await;
        assert_eq!(c.total(), 0);
    }

    #[tokio::test]
    async fn registry_hub_counts_are_independent_per_slot() {
        let hub = RegistryHub::new();
        {
            let mut s = hub.skills.write().await;
            s.register(SkillEntry {
                id: "test-skill".into(),
                kind: "user".into(),
                title: "Test".into(),
                description: "Demo".into(),
                token_estimate: 10,
                tags: Default::default(),
            })
            .unwrap();
        }
        let c = hub.counts().await;
        assert_eq!(c.skills, 1);
        assert_eq!(c.connectors, 0);
        assert_eq!(c.tools, 0);
        assert_eq!(c.models, 0);
        assert_eq!(c.themes, 0);
        assert_eq!(c.total(), 1);
    }

    #[test]
    fn provenance_kind_mapping_is_exhaustive() {
        use crate::skills::SkillProvenance::*;
        assert_eq!(provenance_to_kind(&Bundled), "bundled");
        assert_eq!(provenance_to_kind(&User), "user");
        assert_eq!(provenance_to_kind(&Project), "project");
        assert_eq!(provenance_to_kind(&Marketplace), "marketplace");
    }

    #[tokio::test]
    async fn resolver_hits_hub_skills_after_sync() {
        // Build a hub, register two skills directly (bypassing the
        // bridge — we want to test the resolver, not the bridge),
        // and verify CapabilityQuery resolves against them.
        let hub = RegistryHub::new();
        {
            let mut s = hub.skills.write().await;
            s.register(SkillEntry {
                id: "lunar-converter".into(),
                kind: "user".into(),
                title: "Lunar Converter".into(),
                description: "Convert lunar to solar dates".into(),
                token_estimate: 50,
                tags: {
                    let mut t = std::collections::BTreeMap::new();
                    t.insert("tag:calendar".into(), "1".into());
                    t
                },
            })
            .unwrap();
            s.register(SkillEntry {
                id: "draft-email".into(),
                kind: "user".into(),
                title: "Draft Email".into(),
                description: "Write polite emails".into(),
                token_estimate: 80,
                tags: Default::default(),
            })
            .unwrap();
        }

        // Exact-id query — should score 100 + kind bonus.
        let q = CapabilityQuery {
            name: Some("lunar-converter".into()),
            kind: "user".into(),
            tags: Default::default(),
        };
        let result = crate::registries::resolve(&*hub.skills.read().await, &q);
        assert!(!result.is_empty());
        assert_eq!(result.best(), Some("lunar-converter"));
        // First match should have score 100 (id) + 50 (kind) = 150
        assert_eq!(result.matches[0].score, 150);

        // Tag-only query — kind must still be provided (it's required
        // on CapabilityQuery), but doesn't have to match anything in
        // the registry to be effective. Use the lunar-converter's
        // kind so we test the tag-add boost not just the kind-only
        // match.
        let q = CapabilityQuery {
            name: None,
            kind: "user".into(),
            tags: {
                let mut t = std::collections::BTreeMap::new();
                t.insert("tag:calendar".into(), "1".into());
                t
            },
        };
        let result = crate::registries::resolve(&*hub.skills.read().await, &q);
        assert_eq!(result.best(), Some("lunar-converter"));
    }

    // ── slice 2 — builtin tools registration ────────────────────────

    #[tokio::test]
    async fn register_builtin_tools_populates_tools_slot() {
        let hub = RegistryHub::new();
        let n = register_builtin_tools(&hub).await.unwrap();
        assert!(n >= 10, "expected at least 10 builtin tools, got {n}");
        let counts = hub.counts().await;
        assert_eq!(counts.tools, n);
        // Spot-check a few canonical entries the agent loop relies on.
        let tools = hub.tools.read().await;
        assert!(tools.lookup("skill_search").is_ok());
        assert!(tools.lookup("skill_write").is_ok());
        assert!(tools.lookup("shell").is_ok());
        // shell should be flagged as requires_permission=true.
        let shell = tools.lookup("shell").unwrap();
        assert!(shell.requires_permission);
        // skill_search is read-only, requires_permission=false.
        let search = tools.lookup("skill_search").unwrap();
        assert!(!search.requires_permission);
    }

    #[tokio::test]
    async fn register_builtin_tools_is_idempotent() {
        let hub = RegistryHub::new();
        let n1 = register_builtin_tools(&hub).await.unwrap();
        let n2 = register_builtin_tools(&hub).await.unwrap();
        assert_eq!(n1, n2);
        assert_eq!(hub.counts().await.tools, n1);
    }

    #[tokio::test]
    async fn resolver_finds_builtin_tool_by_kind_and_tag() {
        let hub = RegistryHub::new();
        register_builtin_tools(&hub).await.unwrap();

        // Find tools by `kind=skill`.
        let q = CapabilityQuery {
            name: None,
            kind: "skill".into(),
            tags: Default::default(),
        };
        let result = crate::registries::resolve(&*hub.tools.read().await, &q);
        // Expect at least skill_search, load_skill, skill_write,
        // skill_marketplace_search, skill_install_from_marketplace.
        assert!(result.matches.len() >= 5, "got {:?}", result);

        // Find tools by tag.
        let q = CapabilityQuery {
            name: None,
            kind: "filesystem".into(),
            tags: {
                let mut t = std::collections::BTreeMap::new();
                t.insert("tag:shell".into(), "1".into());
                t
            },
        };
        let result = crate::registries::resolve(&*hub.tools.read().await, &q);
        assert_eq!(result.best(), Some("shell"));
    }

    #[tokio::test]
    async fn sync_skills_idempotent_replaces_stale_entries() {
        // Re-syncing should wipe and repopulate, so a SkillsRegistry
        // that lost an entry between syncs doesn't leave stale rows.
        let hub = RegistryHub::new();
        {
            let mut s = hub.skills.write().await;
            s.register(SkillEntry {
                id: "stale-skill".into(),
                kind: "user".into(),
                title: "Stale".into(),
                description: "...".into(),
                token_estimate: 0,
                tags: Default::default(),
            })
            .unwrap();
        }
        assert_eq!(hub.counts().await.skills, 1);

        // Empty SkillsRegistry → resync should wipe the slot.
        let src = crate::skills::SkillsRegistry::new();
        let count = sync_skills_from_registry(&hub, &src).await.unwrap();
        assert_eq!(count, 0);
        assert_eq!(hub.counts().await.skills, 0);
    }
}
