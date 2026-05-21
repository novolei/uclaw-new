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
