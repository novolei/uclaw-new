use rusqlite::{params, Connection};

use super::types::{
    AffinityFactors, PersonaEvent, PersonaEventKind, PersonaPresetId, PersonaScope,
    RecordPersonaEventInput, VoiceProfile,
};

pub struct PersonaStore<'a> {
    conn: &'a Connection,
}

impl<'a> PersonaStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get_global_voice_profile(&self) -> rusqlite::Result<Option<VoiceProfile>> {
        let mut stmt = self.conn.prepare(
            "SELECT preset_id, warmth, directness, challenge, playfulness, detail, initiative, structure, restraint, neutral_mode
             FROM persona_voice_profiles
             WHERE scope = 'global' AND scope_id IS NULL
             LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(
                VoiceProfile {
                    preset_id: parse_preset(row.get::<_, String>(0)?.as_str()),
                    warmth: row.get::<_, i64>(1)? as u8,
                    directness: row.get::<_, i64>(2)? as u8,
                    challenge: row.get::<_, i64>(3)? as u8,
                    playfulness: row.get::<_, i64>(4)? as u8,
                    detail: row.get::<_, i64>(5)? as u8,
                    initiative: row.get::<_, i64>(6)? as u8,
                    structure: row.get::<_, i64>(7)? as u8,
                    restraint: row.get::<_, i64>(8)? as u8,
                    neutral_mode: row.get::<_, i64>(9)? != 0,
                }
                .clamp(),
            ))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_global_voice_profile(&self, profile: &VoiceProfile) -> rusqlite::Result<()> {
        let profile = profile.clone().clamp();
        self.conn.execute(
            "INSERT INTO persona_voice_profiles
             (id, scope, scope_id, preset_id, warmth, directness, challenge, playfulness, detail, initiative, structure, restraint, neutral_mode, updated_at)
             VALUES ('global', 'global', NULL, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                preset_id = excluded.preset_id,
                warmth = excluded.warmth,
                directness = excluded.directness,
                challenge = excluded.challenge,
                playfulness = excluded.playfulness,
                detail = excluded.detail,
                initiative = excluded.initiative,
                structure = excluded.structure,
                restraint = excluded.restraint,
                neutral_mode = excluded.neutral_mode,
                updated_at = datetime('now')",
            params![
                format_preset(profile.preset_id),
                profile.warmth,
                profile.directness,
                profile.challenge,
                profile.playfulness,
                profile.detail,
                profile.initiative,
                profile.structure,
                profile.restraint,
                if profile.neutral_mode { 1 } else { 0 },
            ],
        )?;
        Ok(())
    }

    pub fn record_event(&self, input: &RecordPersonaEventInput) -> rusqlite::Result<PersonaEvent> {
        let id = uuid::Uuid::new_v4().to_string();
        let kind = format_event_kind(input.kind);
        let minutes = input.minutes.max(0);
        let weight = input.weight.max(0);
        let evidence_json =
            serde_json::to_string(&input.evidence).unwrap_or_else(|_| "[]".to_string());
        self.conn.execute(
            "INSERT INTO persona_events
             (id, kind, session_id, task_id, minutes, weight, note, evidence_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                kind,
                input.session_id.as_deref(),
                input.task_id.as_deref(),
                minutes,
                weight,
                input.note.as_deref(),
                evidence_json,
            ],
        )?;
        self.get_event(&id)
    }

    pub fn list_recent_events(&self, limit: i64) -> rusqlite::Result<Vec<PersonaEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, session_id, task_id, minutes, weight, note, evidence_json, created_at
             FROM persona_events
             ORDER BY datetime(created_at) DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit.max(0)], event_from_row)?;
        rows.collect()
    }

    pub fn affinity_factors_from_events(&self) -> rusqlite::Result<AffinityFactors> {
        let events = self.list_recent_events(i64::MAX)?;
        let mut factors = AffinityFactors::default();
        for event in events {
            let weight = event.weight.max(0);
            match event.kind {
                PersonaEventKind::CollaborationMinutes => {
                    factors.successful_minutes += event.minutes.max(0);
                }
                PersonaEventKind::TaskSucceeded => {
                    factors.successful_minutes += 30 * weight;
                }
                PersonaEventKind::PositiveFeedback => {
                    factors.positive_feedback += weight;
                }
                PersonaEventKind::StylePreferenceAccepted => {
                    factors.stable_style_fragments += weight;
                }
                PersonaEventKind::FailureRecovered => {
                    factors.recovered_failures += weight;
                }
                PersonaEventKind::InactivityDecay => {
                    factors.inactivity_days += weight;
                }
                PersonaEventKind::KeepsakeAccepted => {
                    factors.accepted_keepsakes += weight;
                }
                PersonaEventKind::CandidateRejected => {
                    factors.rejected_candidates += weight;
                }
                PersonaEventKind::FailureUnresolved => {
                    factors.unresolved_failures += weight;
                }
                PersonaEventKind::Correction => {
                    factors.correction_count += weight;
                }
            }
        }
        Ok(factors)
    }

    fn get_event(&self, id: &str) -> rusqlite::Result<PersonaEvent> {
        self.conn.query_row(
            "SELECT id, kind, session_id, task_id, minutes, weight, note, evidence_json, created_at
             FROM persona_events
             WHERE id = ?1",
            params![id],
            event_from_row,
        )
    }
}

fn parse_preset(value: &str) -> PersonaPresetId {
    match value {
        "muse" => PersonaPresetId::Muse,
        "anchor" => PersonaPresetId::Anchor,
        "critic" => PersonaPresetId::Critic,
        "operator" => PersonaPresetId::Operator,
        "companion" => PersonaPresetId::Companion,
        _ => PersonaPresetId::Clarity,
    }
}

pub fn format_preset(value: PersonaPresetId) -> &'static str {
    match value {
        PersonaPresetId::Clarity => "clarity",
        PersonaPresetId::Muse => "muse",
        PersonaPresetId::Anchor => "anchor",
        PersonaPresetId::Critic => "critic",
        PersonaPresetId::Operator => "operator",
        PersonaPresetId::Companion => "companion",
    }
}

fn parse_event_kind(value: &str) -> PersonaEventKind {
    match value {
        "collaboration_minutes" => PersonaEventKind::CollaborationMinutes,
        "task_succeeded" => PersonaEventKind::TaskSucceeded,
        "positive_feedback" => PersonaEventKind::PositiveFeedback,
        "style_preference_accepted" => PersonaEventKind::StylePreferenceAccepted,
        "failure_recovered" => PersonaEventKind::FailureRecovered,
        "inactivity_decay" => PersonaEventKind::InactivityDecay,
        "keepsake_accepted" => PersonaEventKind::KeepsakeAccepted,
        "candidate_rejected" => PersonaEventKind::CandidateRejected,
        "failure_unresolved" => PersonaEventKind::FailureUnresolved,
        "correction" => PersonaEventKind::Correction,
        _ => PersonaEventKind::Correction,
    }
}

pub fn format_event_kind(value: PersonaEventKind) -> &'static str {
    match value {
        PersonaEventKind::CollaborationMinutes => "collaboration_minutes",
        PersonaEventKind::TaskSucceeded => "task_succeeded",
        PersonaEventKind::PositiveFeedback => "positive_feedback",
        PersonaEventKind::StylePreferenceAccepted => "style_preference_accepted",
        PersonaEventKind::FailureRecovered => "failure_recovered",
        PersonaEventKind::InactivityDecay => "inactivity_decay",
        PersonaEventKind::KeepsakeAccepted => "keepsake_accepted",
        PersonaEventKind::CandidateRejected => "candidate_rejected",
        PersonaEventKind::FailureUnresolved => "failure_unresolved",
        PersonaEventKind::Correction => "correction",
    }
}

fn event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersonaEvent> {
    let evidence_json: String = row.get(7)?;
    let evidence = serde_json::from_str(&evidence_json).unwrap_or_default();
    Ok(PersonaEvent {
        id: row.get(0)?,
        kind: parse_event_kind(row.get::<_, String>(1)?.as_str()),
        session_id: row.get(2)?,
        task_id: row.get(3)?,
        minutes: row.get(4)?,
        weight: row.get(5)?,
        note: row.get(6)?,
        evidence,
        created_at: row.get(8)?,
    })
}

#[allow(dead_code)]
fn _scope_name(scope: PersonaScope) -> &'static str {
    match scope {
        PersonaScope::Global => "global",
        PersonaScope::Workspace => "workspace",
        PersonaScope::Session => "session",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn events_aggregate_into_affinity_factors() {
        let conn = store_conn();
        let store = PersonaStore::new(&conn);
        store
            .record_event(&RecordPersonaEventInput {
                kind: PersonaEventKind::CollaborationMinutes,
                session_id: Some("session-1".into()),
                task_id: Some("task-1".into()),
                minutes: 90,
                weight: 1,
                note: Some("worked together".into()),
                evidence: vec!["turn:1".into()],
            })
            .unwrap();
        store
            .record_event(&RecordPersonaEventInput {
                kind: PersonaEventKind::TaskSucceeded,
                session_id: Some("session-1".into()),
                task_id: Some("task-1".into()),
                minutes: 0,
                weight: 2,
                note: None,
                evidence: vec![],
            })
            .unwrap();
        store
            .record_event(&RecordPersonaEventInput {
                kind: PersonaEventKind::PositiveFeedback,
                session_id: None,
                task_id: None,
                minutes: 0,
                weight: 3,
                note: None,
                evidence: vec![],
            })
            .unwrap();
        store
            .record_event(&RecordPersonaEventInput {
                kind: PersonaEventKind::Correction,
                session_id: None,
                task_id: None,
                minutes: 0,
                weight: 1,
                note: None,
                evidence: vec![],
            })
            .unwrap();

        let factors = store.affinity_factors_from_events().unwrap();
        assert_eq!(factors.successful_minutes, 150);
        assert_eq!(factors.positive_feedback, 3);
        assert_eq!(factors.correction_count, 1);

        let recent = store.list_recent_events(10).unwrap();
        assert_eq!(recent.len(), 4);
        assert!(recent.iter().any(|event| event.evidence == vec!["turn:1"]));
    }
}
