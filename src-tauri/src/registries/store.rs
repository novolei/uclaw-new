//! Generic `Registry<E>` container.

use std::collections::BTreeMap;

use super::entry::{RegistryEntry, RegistryError};

/// Ordered map keyed on `entry.id()`. Stable insertion ordering
/// (via `BTreeMap` key sort) so `list()` is deterministic.
#[derive(Debug, Clone)]
pub struct Registry<E: RegistryEntry> {
    entries: BTreeMap<String, E>,
}

impl<E: RegistryEntry> Registry<E> {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Register a new entry. Errors if its id is already in use.
    pub fn register(&mut self, entry: E) -> Result<(), RegistryError> {
        let id = entry.id().to_string();
        if self.entries.contains_key(&id) {
            return Err(RegistryError::DuplicateId(id));
        }
        self.entries.insert(id, entry);
        Ok(())
    }

    /// Look up by id. `None` if absent.
    pub fn get(&self, id: &str) -> Option<&E> {
        self.entries.get(id)
    }

    /// Look up by id, returning a typed error when missing.
    pub fn lookup(&self, id: &str) -> Result<&E, RegistryError> {
        self.entries
            .get(id)
            .ok_or_else(|| RegistryError::NotFound(id.into()))
    }

    /// All entries in id-sorted order.
    pub fn list(&self) -> Vec<&E> {
        self.entries.values().collect()
    }

    /// Filter by `kind`. id-sorted.
    pub fn by_kind(&self, kind: &str) -> Vec<&E> {
        self.entries
            .values()
            .filter(|e| e.kind() == kind)
            .collect()
    }

    /// Filter by exact tag key+value match. id-sorted.
    pub fn by_tag(&self, key: &str, value: &str) -> Vec<&E> {
        self.entries
            .values()
            .filter(|e| e.tags().get(key).map(|v| v == value).unwrap_or(false))
            .collect()
    }

    /// Remove an entry. Returns `true` if removed, `false` if absent.
    pub fn unregister(&mut self, id: &str) -> bool {
        self.entries.remove(id).is_some()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<E: RegistryEntry> Default for Registry<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::{
        connectors::ConnectorEntry, models::ModelEntry, skills::SkillEntry,
        themes::ThemeEntry, tools::ToolEntry,
    };
    use std::collections::BTreeMap;

    fn skill(id: &str, kind: &str) -> SkillEntry {
        SkillEntry {
            id: id.into(),
            kind: kind.into(),
            title: id.into(),
            description: String::new(),
            token_estimate: 100,
            tags: BTreeMap::new(),
        }
    }

    fn tagged_skill(id: &str, kind: &str, key: &str, value: &str) -> SkillEntry {
        let mut tags = BTreeMap::new();
        tags.insert(key.into(), value.into());
        SkillEntry {
            id: id.into(),
            kind: kind.into(),
            title: id.into(),
            description: String::new(),
            token_estimate: 100,
            tags,
        }
    }

    // ── Registry storage ────────────────────────────────────────────

    #[test]
    fn new_is_empty() {
        let r: Registry<SkillEntry> = Registry::new();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn register_grows_then_lookup() {
        let mut r = Registry::new();
        r.register(skill("a", "builtin")).unwrap();
        assert_eq!(r.len(), 1);
        let got = r.lookup("a").unwrap();
        assert_eq!(got.title, "a");
    }

    #[test]
    fn register_duplicate_returns_err() {
        let mut r = Registry::new();
        r.register(skill("a", "builtin")).unwrap();
        let err = r.register(skill("a", "plugin")).unwrap_err();
        assert_eq!(err, RegistryError::DuplicateId("a".into()));
        // First-registered version preserved.
        assert_eq!(r.lookup("a").unwrap().kind, "builtin");
    }

    #[test]
    fn lookup_missing_returns_not_found() {
        let r: Registry<SkillEntry> = Registry::new();
        let err = r.lookup("missing").unwrap_err();
        assert_eq!(err, RegistryError::NotFound("missing".into()));
    }

    #[test]
    fn get_missing_returns_none() {
        let r: Registry<SkillEntry> = Registry::new();
        assert!(r.get("nope").is_none());
    }

    #[test]
    fn list_returns_id_sorted() {
        let mut r = Registry::new();
        r.register(skill("z", "builtin")).unwrap();
        r.register(skill("a", "builtin")).unwrap();
        r.register(skill("m", "builtin")).unwrap();
        let ids: Vec<&str> = r.list().iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "m", "z"]);
    }

    #[test]
    fn by_kind_filters_correctly() {
        let mut r = Registry::new();
        r.register(skill("a", "builtin")).unwrap();
        r.register(skill("b", "plugin")).unwrap();
        r.register(skill("c", "builtin")).unwrap();
        let built = r.by_kind("builtin");
        assert_eq!(built.len(), 2);
        assert!(built.iter().all(|e| e.kind == "builtin"));
    }

    #[test]
    fn by_tag_filters_on_key_value() {
        let mut r = Registry::new();
        r.register(tagged_skill("a", "builtin", "lang", "rust"))
            .unwrap();
        r.register(tagged_skill("b", "builtin", "lang", "python"))
            .unwrap();
        r.register(tagged_skill("c", "builtin", "lang", "rust"))
            .unwrap();
        let rust = r.by_tag("lang", "rust");
        assert_eq!(rust.len(), 2);
        let py = r.by_tag("lang", "python");
        assert_eq!(py.len(), 1);
        // Missing tag.
        assert!(r.by_tag("os", "linux").is_empty());
    }

    #[test]
    fn unregister_returns_true_when_present() {
        let mut r = Registry::new();
        r.register(skill("a", "builtin")).unwrap();
        assert!(r.unregister("a"));
        assert!(r.is_empty());
        assert!(!r.unregister("a"));
    }

    // ── Per-type façade smoke tests ─────────────────────────────────

    #[test]
    fn registry_works_with_connector_entry() {
        let mut r: Registry<ConnectorEntry> = Registry::new();
        r.register(ConnectorEntry {
            id: "slack".into(),
            kind: "anthropic-mcp".into(),
            display_name: "Slack".into(),
            auth_kind: Some("oauth".into()),
            connected: true,
            tags: BTreeMap::new(),
        })
        .unwrap();
        assert_eq!(r.len(), 1);
        assert!(r.lookup("slack").unwrap().connected);
    }

    #[test]
    fn registry_works_with_tool_entry() {
        let mut r: Registry<ToolEntry> = Registry::new();
        r.register(ToolEntry {
            id: "shell".into(),
            kind: "builtin".into(),
            description: "Run shell".into(),
            schema: serde_json::json!({"type": "object"}),
            requires_permission: true,
            tags: BTreeMap::new(),
        })
        .unwrap();
        assert!(r.lookup("shell").unwrap().requires_permission);
    }

    #[test]
    fn registry_works_with_model_entry() {
        let mut r: Registry<ModelEntry> = Registry::new();
        r.register(ModelEntry {
            id: "anthropic::claude-sonnet-4-5".into(),
            kind: "anthropic".into(),
            display_name: "Sonnet 4.5".into(),
            context_window_tokens: 200_000,
            supports_images: true,
            supports_prompt_cache: true,
            input_cost_micros_per_mtok: Some(3_000_000),
            output_cost_micros_per_mtok: Some(15_000_000),
            tags: BTreeMap::new(),
        })
        .unwrap();
        let m = r.lookup("anthropic::claude-sonnet-4-5").unwrap();
        assert_eq!(m.context_window_tokens, 200_000);
        assert!(m.supports_prompt_cache);
    }

    #[test]
    fn registry_works_with_theme_entry() {
        let mut css = BTreeMap::new();
        css.insert("--bg".into(), "#111".into());
        css.insert("--fg".into(), "#fff".into());
        let mut r: Registry<ThemeEntry> = Registry::new();
        r.register(ThemeEntry {
            id: "neon-noir".into(),
            kind: "user".into(),
            display_name: "Neon Noir".into(),
            is_dark: true,
            css_vars: css,
            tags: BTreeMap::new(),
        })
        .unwrap();
        let t = r.lookup("neon-noir").unwrap();
        assert!(t.is_dark);
        assert_eq!(t.css_vars.get("--bg"), Some(&"#111".to_string()));
    }

    // ── RegistryError Display ──────────────────────────────────────

    #[test]
    fn registry_error_display() {
        let dup = RegistryError::DuplicateId("a".into());
        let nf = RegistryError::NotFound("b".into());
        assert!(dup.to_string().contains("a"));
        assert!(nf.to_string().contains("b"));
    }

    // ── serde roundtrip per entry type ─────────────────────────────

    #[test]
    fn skill_entry_serde_roundtrip() {
        let s = skill("rust-async", "builtin");
        let json = serde_json::to_string(&s).unwrap();
        let back: SkillEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn connector_entry_auth_kind_skipped_when_none() {
        let c = ConnectorEntry {
            id: "x".into(),
            kind: "anthropic-mcp".into(),
            display_name: "X".into(),
            auth_kind: None,
            connected: false,
            tags: BTreeMap::new(),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("authKind"));
    }
}
