//! `WorldSnapshot` — read-only view of all known entities at one point.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::entity::{WorldEntity, WorldEntityKind};

/// What's in the snapshot for the UI / stats panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectionStats {
    pub total_entities: usize,
    pub tombstoned: usize,
    pub by_kind_id: BTreeMap<String, usize>,
}

impl ProjectionStats {
    pub fn alive(&self) -> usize {
        self.total_entities - self.tombstoned
    }
}

/// Read-only snapshot. Keys on `kind.id() :: ref.id` so two entities
/// with the same id but different kinds (rare, but possible across
/// plugins) don't collide. Sorted iteration via `BTreeMap`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSnapshot {
    /// RFC 3339 timestamp at which the snapshot was assembled.
    pub captured_at: String,
    /// `kind.id() :: ref.id` → entity. The double-colon delimiter
    /// avoids collisions with `:`-bearing ids (e.g. `"file:/path"`).
    pub entities: BTreeMap<String, WorldEntity>,
}

impl WorldSnapshot {
    pub fn new(captured_at: impl Into<String>) -> Self {
        Self {
            captured_at: captured_at.into(),
            entities: BTreeMap::new(),
        }
    }

    pub fn key_for(kind: &WorldEntityKind, id: &str) -> String {
        format!("{}::{}", kind.id(), id)
    }

    pub fn insert(&mut self, entity: WorldEntity) {
        let key = Self::key_for(&entity.kind, &entity.r#ref.id);
        self.entities.insert(key, entity);
    }

    pub fn get(&self, kind: &WorldEntityKind, id: &str) -> Option<&WorldEntity> {
        self.entities.get(&Self::key_for(kind, id))
    }

    /// All entities of one kind (alive + tombstoned).
    pub fn by_kind(&self, kind: &WorldEntityKind) -> Vec<&WorldEntity> {
        self.entities
            .values()
            .filter(|e| &e.kind == kind)
            .collect()
    }

    /// Stats summary.
    pub fn stats(&self) -> ProjectionStats {
        let mut s = ProjectionStats {
            total_entities: self.entities.len(),
            ..Default::default()
        };
        for e in self.entities.values() {
            if e.is_tombstoned() {
                s.tombstoned += 1;
            }
            *s.by_kind_id.entry(e.kind.id()).or_insert(0) += 1;
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::super::entity::{EntityRef, WorldEntityState};
    use super::*;

    fn ent(kind: WorldEntityKind, id: &str) -> WorldEntity {
        WorldEntity::new(EntityRef::new(id), kind, WorldEntityState::fresh("t0"))
    }

    // ── ProjectionStats ────────────────────────────────────────────

    #[test]
    fn stats_alive_subtracts_tombstoned() {
        let s = ProjectionStats {
            total_entities: 10,
            tombstoned: 3,
            by_kind_id: BTreeMap::new(),
        };
        assert_eq!(s.alive(), 7);
    }

    // ── snapshot ───────────────────────────────────────────────────

    #[test]
    fn empty_snapshot_has_zero_entities() {
        let s = WorldSnapshot::new("t0");
        assert_eq!(s.entities.len(), 0);
        assert_eq!(s.stats().total_entities, 0);
        assert_eq!(s.stats().alive(), 0);
    }

    #[test]
    fn insert_then_get_finds_entity() {
        let mut s = WorldSnapshot::new("t0");
        s.insert(ent(WorldEntityKind::File, "file:/a"));
        let got = s.get(&WorldEntityKind::File, "file:/a").unwrap();
        assert_eq!(got.r#ref.id, "file:/a");
    }

    #[test]
    fn get_returns_none_for_unknown() {
        let s = WorldSnapshot::new("t0");
        assert!(s.get(&WorldEntityKind::File, "nope").is_none());
    }

    #[test]
    fn same_id_different_kind_do_not_collide() {
        // E.g. plugin uses "x" for a custom kind while a File entity
        // also has id "x" — `key_for` namespacing prevents overlap.
        let mut s = WorldSnapshot::new("t0");
        s.insert(ent(WorldEntityKind::File, "x"));
        s.insert(ent(WorldEntityKind::Document, "x"));
        assert_eq!(s.entities.len(), 2);
        assert!(s.get(&WorldEntityKind::File, "x").is_some());
        assert!(s.get(&WorldEntityKind::Document, "x").is_some());
    }

    #[test]
    fn insert_with_same_kind_id_overwrites() {
        // Re-insertion is the upsert path. Newer state replaces older.
        let mut s = WorldSnapshot::new("t0");
        let mut e1 = ent(WorldEntityKind::File, "x");
        e1.state.version = 1;
        s.insert(e1);
        let mut e2 = ent(WorldEntityKind::File, "x");
        e2.state.version = 5;
        s.insert(e2);
        assert_eq!(s.entities.len(), 1);
        assert_eq!(
            s.get(&WorldEntityKind::File, "x").unwrap().state.version,
            5
        );
    }

    #[test]
    fn by_kind_filters_correctly() {
        let mut s = WorldSnapshot::new("t0");
        s.insert(ent(WorldEntityKind::File, "a"));
        s.insert(ent(WorldEntityKind::File, "b"));
        s.insert(ent(WorldEntityKind::Email, "c"));
        let files = s.by_kind(&WorldEntityKind::File);
        assert_eq!(files.len(), 2);
        let emails = s.by_kind(&WorldEntityKind::Email);
        assert_eq!(emails.len(), 1);
    }

    // ── stats ──────────────────────────────────────────────────────

    #[test]
    fn stats_counts_kinds_and_tombstones() {
        let mut s = WorldSnapshot::new("t0");
        s.insert(ent(WorldEntityKind::File, "a"));
        s.insert(ent(WorldEntityKind::File, "b"));
        // tombstoned file
        let mut tomb = ent(WorldEntityKind::File, "c");
        tomb.state = tomb.state.tombstoned("t1");
        s.insert(tomb);
        s.insert(ent(WorldEntityKind::Email, "d"));
        let stats = s.stats();
        assert_eq!(stats.total_entities, 4);
        assert_eq!(stats.tombstoned, 1);
        assert_eq!(stats.alive(), 3);
        assert_eq!(stats.by_kind_id.get("file"), Some(&3));
        assert_eq!(stats.by_kind_id.get("email"), Some(&1));
    }

    // ── key_for namespace ──────────────────────────────────────────

    #[test]
    fn key_for_uses_double_colon_namespace() {
        let k = WorldSnapshot::key_for(&WorldEntityKind::File, "file:/x");
        assert_eq!(k, "file::file:/x");
        // Other kind carries its own subkind prefix.
        let k2 = WorldSnapshot::key_for(
            &WorldEntityKind::Other("plugin.weather".into()),
            "abc",
        );
        assert_eq!(k2, "other:plugin.weather::abc");
    }
}
