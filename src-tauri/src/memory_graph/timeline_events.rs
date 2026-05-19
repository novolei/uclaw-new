//! Memory OS L3 §3.2.1 RETAINED — global `timeline_events` ledger.
//!
//! Per ADR 2026-05-20 §8: Timeline Engine is one of the RETAINED L3
//! components. gbrain has per-page timelines but no global timeline;
//! this module fills that gap.
//!
//! V44 (PR #271) shipped the `timeline_events` table. This module
//! ships the **write path**: a typed `TimelineEvent` struct, an
//! `insert_event` helper, and constructor helpers for the common
//! event kinds (entity page created, session started, etc.).
//!
//! ## Scope of this PR (Q2a)
//!
//! This module ships the **write API** + a single demonstration
//! caller (EntityPage create, hooked from
//! `tauri_commands::memory_entity_page_create`). It does NOT yet:
//! - Hook the session-start path
//! - Hook the memorize / fragment-review paths
//! - Read the timeline back out (read API comes alongside Q2c
//!   Temporal Query Classifier)
//! - Auto-aggregate into `temporal_aggregates` (Q2b)
//!
//! The other write hooks land in follow-up PRs to keep each commit
//! small and bisectable.

use rusqlite::{params, Connection};

/// Default importance assigned to a newly-recorded event before
/// Dream Cycle / Importance Decay updates it. Matches the V44
/// schema's `importance DEFAULT 0.5`.
pub const DEFAULT_EVENT_IMPORTANCE: f64 = 0.5;

/// Event kinds the timeline currently records. Wire format is
/// snake_case lowercase, matching `timeline_events.event_kind`
/// column convention. Caller can pass arbitrary strings via the
/// `Custom` variant for forward-compat — the read path tolerates
/// unknown kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineEventKind {
    /// A new EntityPage was created (subject_id = node_id).
    EntityPageCreated,
    /// A new EntityPage version was written (subject_id = node_id,
    /// payload may include the new version_id).
    EntityPageVersioned,
    /// A new agent session started (subject_id = session_id).
    SessionStart,
    /// A long-running review queue item was resolved (subject_id =
    /// review_queue_item.id; Cognitive Phase 13 — currently PAUSED
    /// but the kind is reserved for forward-compat).
    ReviewResolved,
    /// A skill was learned (subject_id = skill_id).
    SkillLearned,
    /// Caller-supplied kind. The string is stored verbatim; treat
    /// this as the extension point for plugins / new scenarios.
    Custom(String),
}

impl TimelineEventKind {
    /// Wire string for `timeline_events.event_kind`.
    pub fn as_str(&self) -> &str {
        match self {
            Self::EntityPageCreated => "entity_page_created",
            Self::EntityPageVersioned => "entity_page_versioned",
            Self::SessionStart => "session_start",
            Self::ReviewResolved => "review_resolved",
            Self::SkillLearned => "skill_learned",
            Self::Custom(s) => s.as_str(),
        }
    }
}

/// One row to insert into `timeline_events`. All fields except
/// `payload_json` and `related_entity_ids` are required; the two
/// JSON columns default to `None` / `[]` respectively.
#[derive(Debug, Clone)]
pub struct TimelineEvent {
    pub id: String,
    pub space_id: String,
    pub event_kind: TimelineEventKind,
    pub subject_id: Option<String>,
    pub title: String,
    pub payload_json: Option<String>,
    /// JSON array of related EntityPage node IDs. Empty array (or
    /// missing) means "no related entities". The V44 index
    /// `idx_timeline_events_entity` accelerates lookups by
    /// exact-string match (limited by SQLite's TEXT FTS shape; see
    /// V44 spec note for future improvements).
    pub related_entity_ids: Vec<String>,
    pub occurred_at: i64,
    pub importance: f64,
}

impl TimelineEvent {
    /// Helper: build an `entity_page_created` event with sensible
    /// defaults. Caller can adjust `importance` if known
    /// (otherwise [`DEFAULT_EVENT_IMPORTANCE`]).
    pub fn entity_page_created(
        space_id: impl Into<String>,
        node_id: impl Into<String>,
        title: impl Into<String>,
        occurred_at: i64,
    ) -> Self {
        let node_id = node_id.into();
        Self {
            id: format!("evt-{}", uuid::Uuid::new_v4()),
            space_id: space_id.into(),
            event_kind: TimelineEventKind::EntityPageCreated,
            subject_id: Some(node_id.clone()),
            title: title.into(),
            payload_json: None,
            related_entity_ids: vec![node_id],
            occurred_at,
            importance: DEFAULT_EVENT_IMPORTANCE,
        }
    }

    /// Helper: build a `session_start` event. Subject is the session
    /// id; `title` is the human-readable summary (e.g. "session: gbrain dev").
    pub fn session_start(
        space_id: impl Into<String>,
        session_id: impl Into<String>,
        title: impl Into<String>,
        occurred_at: i64,
    ) -> Self {
        Self {
            id: format!("evt-{}", uuid::Uuid::new_v4()),
            space_id: space_id.into(),
            event_kind: TimelineEventKind::SessionStart,
            subject_id: Some(session_id.into()),
            title: title.into(),
            payload_json: None,
            related_entity_ids: vec![],
            occurred_at,
            importance: DEFAULT_EVENT_IMPORTANCE,
        }
    }
}

/// Insert one event into `timeline_events`. Returns the inserted
/// row's `id` (matches input `event.id`) on success.
///
/// Failures are propagated as `rusqlite::Error` so the caller can
/// decide policy. Hot-path callers (e.g. EntityPage create) should
/// log + swallow errors with `tracing::warn!` — a timeline write
/// failure must NEVER fail the underlying user action.
pub fn insert_event(conn: &Connection, event: &TimelineEvent) -> rusqlite::Result<String> {
    let related_json = serde_json::to_string(&event.related_entity_ids)
        .unwrap_or_else(|_| "[]".to_string());
    let now_ms = chrono::Utc::now().timestamp_millis();
    let event_kind = event.event_kind.as_str().to_string();
    conn.execute(
        "INSERT INTO timeline_events
             (id, space_id, event_kind, subject_id, title, payload_json,
              related_entity_ids, occurred_at, importance, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            event.id,
            event.space_id,
            event_kind,
            event.subject_id,
            event.title,
            event.payload_json,
            related_json,
            event.occurred_at,
            event.importance,
            now_ms,
        ],
    )?;
    Ok(event.id.clone())
}

/// Best-effort insert: logs + swallows errors so the caller's main
/// action (e.g. EntityPage create) never fails because of a
/// timeline-write hiccup.
pub fn insert_event_best_effort(conn: &Connection, event: &TimelineEvent) {
    if let Err(e) = insert_event(conn, event) {
        tracing::warn!(
            event_id = %event.id,
            event_kind = %event.event_kind.as_str(),
            error = %e,
            "timeline_events: best-effort insert failed; user action still succeeds"
        );
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::OptionalExtension;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn entity_page_created_helper_carries_node_id_through() {
        let evt = TimelineEvent::entity_page_created(
            "default",
            "node-abc-123",
            "Test Entity Page",
            1_700_000_000_000,
        );
        assert_eq!(evt.event_kind, TimelineEventKind::EntityPageCreated);
        assert_eq!(evt.subject_id.as_deref(), Some("node-abc-123"));
        assert_eq!(evt.related_entity_ids, vec!["node-abc-123"]);
        assert_eq!(evt.title, "Test Entity Page");
        assert_eq!(evt.occurred_at, 1_700_000_000_000);
        assert_eq!(evt.importance, DEFAULT_EVENT_IMPORTANCE);
        assert!(
            evt.id.starts_with("evt-"),
            "default id should have evt- prefix, got {}",
            evt.id
        );
    }

    #[test]
    fn session_start_helper_has_empty_related_entities() {
        let evt = TimelineEvent::session_start(
            "default",
            "session-xyz",
            "session: gbrain dev",
            1_700_000_000_000,
        );
        assert_eq!(evt.event_kind, TimelineEventKind::SessionStart);
        assert!(
            evt.related_entity_ids.is_empty(),
            "session start has no related entities (session_id is in subject_id)"
        );
        assert_eq!(evt.subject_id.as_deref(), Some("session-xyz"));
    }

    #[test]
    fn insert_event_writes_row_to_timeline_events() {
        let conn = fresh_conn();
        let evt = TimelineEvent::entity_page_created(
            "default",
            "node-1",
            "Test",
            1_700_000_000_000,
        );
        let inserted_id = insert_event(&conn, &evt).unwrap();
        assert_eq!(inserted_id, evt.id);

        // Read back and verify fields.
        let row = conn
            .query_row(
                "SELECT event_kind, subject_id, title, related_entity_ids, importance
                 FROM timeline_events WHERE id = ?1",
                [&evt.id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, f64>(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, "entity_page_created");
        assert_eq!(row.1.as_deref(), Some("node-1"));
        assert_eq!(row.2, "Test");
        let related: Vec<String> = serde_json::from_str(&row.3).unwrap();
        assert_eq!(related, vec!["node-1"]);
        assert!((row.4 - DEFAULT_EVENT_IMPORTANCE).abs() < f64::EPSILON);
    }

    #[test]
    fn custom_kind_round_trips_verbatim() {
        // Forward-compat: a plugin or future scenario can pass any
        // string and the column stores it as-is, retrievable later.
        let conn = fresh_conn();
        let evt = TimelineEvent {
            id: "evt-custom-1".into(),
            space_id: "default".into(),
            event_kind: TimelineEventKind::Custom("agent_handoff_v2".into()),
            subject_id: None,
            title: "agent handoff".into(),
            payload_json: Some(r#"{"from":"main","to":"helper"}"#.into()),
            related_entity_ids: vec![],
            occurred_at: 1_700_000_000_000,
            importance: 0.7,
        };
        insert_event(&conn, &evt).unwrap();
        let kind: String = conn
            .query_row(
                "SELECT event_kind FROM timeline_events WHERE id = 'evt-custom-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kind, "agent_handoff_v2");
    }

    #[test]
    fn best_effort_insert_swallows_error_on_duplicate_id() {
        // Insert once; second insert with same primary key should
        // error per SQLite, but best_effort swallows it. The caller's
        // execution path continues, and the existing row remains
        // intact.
        let conn = fresh_conn();
        let evt = TimelineEvent::entity_page_created(
            "default",
            "node-dup",
            "dup test",
            1_700_000_000_000,
        );
        let id = evt.id.clone();
        insert_event(&conn, &evt).unwrap();
        // Second call with the same `id` — best-effort must not panic.
        insert_event_best_effort(&conn, &evt);
        // And the existing row stays.
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM timeline_events WHERE id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "exactly one row with this id should remain");
    }

    #[test]
    fn insert_handles_no_subject_no_related_entities() {
        let conn = fresh_conn();
        let evt = TimelineEvent {
            id: "evt-bare".into(),
            space_id: "default".into(),
            event_kind: TimelineEventKind::Custom("system_boot".into()),
            subject_id: None,
            title: "uClaw booted".into(),
            payload_json: None,
            related_entity_ids: vec![],
            occurred_at: 1_700_000_000_000,
            importance: 0.5,
        };
        insert_event(&conn, &evt).unwrap();
        let subject: Option<String> = conn
            .query_row(
                "SELECT subject_id FROM timeline_events WHERE id = 'evt-bare'",
                [],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
            .unwrap();
        assert!(subject.is_none(), "subject_id should be NULL");
    }

    #[test]
    fn insert_preserves_event_id_for_callers_that_need_it() {
        // The id returned from insert_event matches the caller's
        // event.id verbatim — callers can pass a deterministic id
        // (e.g. derived from session_id + timestamp) and use it for
        // dedup or cross-referencing without re-querying.
        let conn = fresh_conn();
        let evt = TimelineEvent::entity_page_created(
            "default",
            "node-deterministic",
            "deterministic id test",
            1_700_000_000_000,
        );
        let expected_id = evt.id.clone();
        let returned_id = insert_event(&conn, &evt).unwrap();
        assert_eq!(returned_id, expected_id);
    }
}
