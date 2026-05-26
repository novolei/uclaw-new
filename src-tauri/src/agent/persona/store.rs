use rusqlite::{params, Connection};

use super::types::{PersonaPresetId, PersonaScope, VoiceProfile};

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

#[allow(dead_code)]
fn _scope_name(scope: PersonaScope) -> &'static str {
    match scope {
        PersonaScope::Global => "global",
        PersonaScope::Workspace => "workspace",
        PersonaScope::Session => "session",
    }
}
