// SPDX-License-Identifier: Apache-2.0
//! Persisted per-session prompt frame (context continuity P3, V67
//! `agent_session_frames`).
//!
//! In transcript mode the system prompt is frozen per session: it is built
//! once and reused verbatim until its [`FrameKey`] changes (template text,
//! workspace, frame version or the compile-time static text). Mode, persona,
//! skills registry, rule files and model changes do NOT rebuild the frame —
//! they surface in the per-turn dynamic block instead.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const FRAME_VERSION: u32 = 1;

/// What the frozen prompt was built from. Any field change rebuilds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameKey {
    pub version: u32,
    /// sha256 (lowercase hex) of the resolved base prompt template text.
    pub prompt_sha: String,
    /// Workspace root path (`""` when none).
    pub workspace: String,
    /// sha256 (lowercase hex) of the compile-time static text: the baseline
    /// rendered with `InjectionContext::baseline()` plus every mode addition.
    pub static_sha: String,
}

fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Compile-time static prompt text: baseline + all mode additions.
fn static_text() -> String {
    use crate::agent::baseline_blocks::{render_with_context, InjectionContext};
    use crate::agent::mode_prompts::mode_addition;
    use crate::safety::SafetyMode;
    let mut s = render_with_context(&InjectionContext::baseline());
    for mode in [
        SafetyMode::Ask,
        SafetyMode::AcceptEdits,
        SafetyMode::Plan,
        SafetyMode::Supervised,
        SafetyMode::Yolo,
    ] {
        s.push_str("\n\u{0}\n");
        s.push_str(mode_addition(&mode));
    }
    s
}

impl FrameKey {
    pub fn compute(base_prompt: &str, workspace_root: Option<&Path>) -> Self {
        use std::sync::OnceLock;
        static STATIC_SHA: OnceLock<String> = OnceLock::new();
        Self {
            version: FRAME_VERSION,
            prompt_sha: sha256_hex(base_prompt),
            workspace: workspace_root
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            static_sha: STATIC_SHA.get_or_init(|| sha256_hex(&static_text())).clone(),
        }
    }
}

/// snake_case name of a safety mode as stored in [`SessionFrame::mode`]
/// (and rendered in `<mode_update mode="…">`).
pub fn mode_name(mode: &crate::safety::SafetyMode) -> &'static str {
    use crate::safety::SafetyMode;
    match mode {
        SafetyMode::Ask => "ask",
        SafetyMode::AcceptEdits => "accept_edits",
        SafetyMode::Plan => "plan",
        SafetyMode::Supervised => "supervised",
        SafetyMode::Yolo => "yolo",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameRebuildReason {
    New,
    PromptChanged,
    WorkspaceChanged,
    VersionChanged,
}

impl FrameRebuildReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::PromptChanged => "prompt_changed",
            Self::WorkspaceChanged => "workspace_changed",
            Self::VersionChanged => "version_changed",
        }
    }

    fn parse(s: &str) -> anyhow::Result<Self> {
        Ok(match s {
            "new" => Self::New,
            "prompt_changed" => Self::PromptChanged,
            "workspace_changed" => Self::WorkspaceChanged,
            "version_changed" => Self::VersionChanged,
            other => anyhow::bail!("unknown frame rebuild reason: {other}"),
        })
    }
}

/// Skill manifest sizing recorded when the frame was built.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestStats {
    pub listed: u32,
    pub total: u32,
    pub tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionFrame {
    pub session_id: String,
    pub key: FrameKey,
    pub system_prompt: String,
    pub mode: String,
    pub rules_hash: String,
    /// Tool names announced to the model with this frame, in order.
    pub tool_names: Vec<String>,
    pub manifest: ManifestStats,
    pub rebuild_reason: FrameRebuildReason,
    pub rebuilt_turn_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Why the frame must be (re)built, or `None` to reuse `existing`.
/// Priority: missing → New; version/static text → VersionChanged;
/// workspace → WorkspaceChanged; template text → PromptChanged.
pub fn rebuild_reason(existing: Option<&SessionFrame>, key: &FrameKey) -> Option<FrameRebuildReason> {
    let Some(frame) = existing else {
        return Some(FrameRebuildReason::New);
    };
    let old = &frame.key;
    if old.version != key.version || old.static_sha != key.static_sha {
        Some(FrameRebuildReason::VersionChanged)
    } else if old.workspace != key.workspace {
        Some(FrameRebuildReason::WorkspaceChanged)
    } else if old.prompt_sha != key.prompt_sha {
        Some(FrameRebuildReason::PromptChanged)
    } else {
        None
    }
}

pub fn load(conn: &Connection, session_id: &str) -> anyhow::Result<Option<SessionFrame>> {
    let raw = conn
        .query_row(
            "SELECT frame_key_json, system_prompt, mode, rules_hash, tool_names_json,
                    manifest_json, rebuild_reason, rebuilt_turn_id, created_at, updated_at
             FROM agent_session_frames WHERE session_id = ?1",
            [session_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, i64>(8)?,
                    r.get::<_, i64>(9)?,
                ))
            },
        )
        .optional()?;
    let Some((key, system_prompt, mode, rules_hash, tools, manifest, reason, rebuilt_turn_id, created_at, updated_at)) = raw
    else {
        return Ok(None);
    };
    Ok(Some(SessionFrame {
        session_id: session_id.to_string(),
        key: serde_json::from_str(&key)?,
        system_prompt,
        mode,
        rules_hash,
        tool_names: serde_json::from_str(&tools)?,
        manifest: serde_json::from_str(&manifest)?,
        rebuild_reason: FrameRebuildReason::parse(&reason)?,
        rebuilt_turn_id,
        created_at,
        updated_at,
    }))
}

/// Insert or fully replace the frame of `frame.session_id`.
pub fn upsert(conn: &Connection, frame: &SessionFrame) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO agent_session_frames
            (session_id, frame_key_json, system_prompt, mode, rules_hash, tool_names_json,
             manifest_json, rebuild_reason, rebuilt_turn_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(session_id) DO UPDATE SET
            frame_key_json = excluded.frame_key_json,
            system_prompt = excluded.system_prompt,
            mode = excluded.mode,
            rules_hash = excluded.rules_hash,
            tool_names_json = excluded.tool_names_json,
            manifest_json = excluded.manifest_json,
            rebuild_reason = excluded.rebuild_reason,
            rebuilt_turn_id = excluded.rebuilt_turn_id,
            created_at = excluded.created_at,
            updated_at = excluded.updated_at",
        params![
            frame.session_id,
            serde_json::to_string(&frame.key)?,
            frame.system_prompt,
            frame.mode,
            frame.rules_hash,
            serde_json::to_string(&frame.tool_names)?,
            serde_json::to_string(&frame.manifest)?,
            frame.rebuild_reason.as_str(),
            frame.rebuilt_turn_id,
            frame.created_at,
            frame.updated_at,
        ],
    )?;
    Ok(())
}

pub fn update_tool_names(conn: &Connection, session_id: &str, names: &[String], now: i64) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE agent_session_frames SET tool_names_json = ?2, updated_at = ?3 WHERE session_id = ?1",
        params![session_id, serde_json::to_string(names)?, now],
    )?;
    Ok(())
}

pub fn update_rules_hash(conn: &Connection, session_id: &str, hash: &str, now: i64) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE agent_session_frames SET rules_hash = ?2, updated_at = ?3 WHERE session_id = ?1",
        params![session_id, hash, now],
    )?;
    Ok(())
}

/// Copy `from`'s frame to `to` (fork). Returns whether a frame was copied.
/// Replaces any frame `to` already has. No own transaction.
pub fn copy_to(conn: &Connection, from: &str, to: &str) -> anyhow::Result<bool> {
    let n = conn.execute(
        "INSERT OR REPLACE INTO agent_session_frames
            (session_id, frame_key_json, system_prompt, mode, rules_hash, tool_names_json,
             manifest_json, rebuild_reason, rebuilt_turn_id, created_at, updated_at)
         SELECT ?2, frame_key_json, system_prompt, mode, rules_hash, tool_names_json,
                manifest_json, rebuild_reason, rebuilt_turn_id, created_at, updated_at
         FROM agent_session_frames WHERE session_id = ?1",
        params![from, to],
    )?;
    Ok(n > 0)
}

/// Drop the frame of `session_id`. No own transaction.
pub fn delete_session(conn: &Connection, session_id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM agent_session_frames WHERE session_id = ?1", [session_id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::transcript::{self, TranscriptKind};
    use uclaw_message_types::ChatMessage;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn key(prompt: &str, ws: &str) -> FrameKey {
        FrameKey::compute(prompt, Some(Path::new(ws)))
    }

    fn frame(session_id: &str, k: FrameKey) -> SessionFrame {
        SessionFrame {
            session_id: session_id.to_string(),
            key: k,
            system_prompt: format!("prompt of {session_id}"),
            mode: "supervised".into(),
            rules_hash: "rh0".into(),
            tool_names: vec!["zeta".into(), "alpha".into(), "mid".into()],
            manifest: ManifestStats { listed: 3, total: 9, tokens: 120 },
            rebuild_reason: FrameRebuildReason::New,
            rebuilt_turn_id: Some("u1".into()),
            created_at: 100,
            updated_at: 100,
        }
    }

    fn is_lower_hex64(s: &str) -> bool {
        s.len() == 64 && s.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    }

    #[test]
    fn frame_key_is_stable_and_hex() {
        let a = key("base prompt", "/ws");
        let b = key("base prompt", "/ws");
        assert_eq!(a, b);
        assert_eq!(a.version, FRAME_VERSION);
        assert_eq!(a.workspace, "/ws");
        assert!(is_lower_hex64(&a.prompt_sha), "{}", a.prompt_sha);
        assert!(is_lower_hex64(&a.static_sha), "{}", a.static_sha);
        assert_ne!(a.prompt_sha, key("other prompt", "/ws").prompt_sha);
        assert_eq!(FrameKey::compute("x", None).workspace, "");
    }

    #[test]
    fn rebuild_reason_priority() {
        let k = key("p", "/ws");
        assert_eq!(rebuild_reason(None, &k), Some(FrameRebuildReason::New));
        let f = frame("s", k.clone());
        assert_eq!(rebuild_reason(Some(&f), &k), None);
        assert_eq!(
            rebuild_reason(Some(&f), &key("p2", "/ws")),
            Some(FrameRebuildReason::PromptChanged)
        );
        // Workspace outranks prompt.
        assert_eq!(
            rebuild_reason(Some(&f), &key("p2", "/ws2")),
            Some(FrameRebuildReason::WorkspaceChanged)
        );
        // Version (or static text) outranks everything else.
        let mut v = key("p2", "/ws2");
        v.version += 1;
        assert_eq!(rebuild_reason(Some(&f), &v), Some(FrameRebuildReason::VersionChanged));
        let mut st = k.clone();
        st.static_sha = "0".repeat(64);
        assert_eq!(rebuild_reason(Some(&f), &st), Some(FrameRebuildReason::VersionChanged));
    }

    #[test]
    fn frame_roundtrip_upsert_load() {
        let conn = db();
        assert_eq!(load(&conn, "s").unwrap(), None);
        let f = frame("s", key("p", "/ws"));
        upsert(&conn, &f).unwrap();
        let got = load(&conn, "s").unwrap().unwrap();
        assert_eq!(got, f);
        assert_eq!(got.tool_names, vec!["zeta", "alpha", "mid"]);
        // Upsert replaces.
        let mut g = f.clone();
        g.system_prompt = "rebuilt".into();
        g.rebuild_reason = FrameRebuildReason::PromptChanged;
        g.rebuilt_turn_id = None;
        g.updated_at = 200;
        upsert(&conn, &g).unwrap();
        assert_eq!(load(&conn, "s").unwrap().unwrap(), g);
    }

    #[test]
    fn update_tool_names_and_rules_hash_touch_only_those_columns() {
        let conn = db();
        let f = frame("s", key("p", "/ws"));
        upsert(&conn, &f).unwrap();
        upsert(&conn, &frame("other", key("p", "/ws"))).unwrap();
        let names = vec!["b".to_string(), "a".to_string()];
        update_tool_names(&conn, "s", &names, 300).unwrap();
        let mut want = f.clone();
        want.tool_names = names;
        want.updated_at = 300;
        assert_eq!(load(&conn, "s").unwrap().unwrap(), want);
        update_rules_hash(&conn, "s", "rh1", 400).unwrap();
        want.rules_hash = "rh1".into();
        want.updated_at = 400;
        assert_eq!(load(&conn, "s").unwrap().unwrap(), want);
        assert_eq!(load(&conn, "other").unwrap().unwrap(), frame("other", key("p", "/ws")));
    }

    #[test]
    fn copy_to_and_delete_session() {
        let conn = db();
        assert!(!copy_to(&conn, "missing", "t").unwrap());
        upsert(&conn, &frame("s", key("p", "/ws"))).unwrap();
        assert!(copy_to(&conn, "s", "t").unwrap());
        let t = load(&conn, "t").unwrap().unwrap();
        assert_eq!(t.session_id, "t");
        assert_eq!(t.system_prompt, "prompt of s");
        delete_session(&conn, "s").unwrap();
        assert_eq!(load(&conn, "s").unwrap(), None);
        assert!(load(&conn, "t").unwrap().is_some());
    }

    #[test]
    fn fork_copies_frame() {
        let conn = db();
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at) VALUES ('src','default','S','{}',0,0,0,1000,1000)",
            [],
        )
        .unwrap();
        for (id, role, at) in [("u1", "user", 1002), ("a1", "assistant", 1003)] {
            conn.execute(
                "INSERT INTO agent_messages (id, session_id, role, content, created_at, compacted) VALUES (?1,'src',?2,?1,?3,0)",
                params![id, role, at],
            )
            .unwrap();
        }
        transcript::append(
            &conn,
            "src",
            Some("u1"),
            TranscriptKind::Message,
            &[ChatMessage::user("u1"), ChatMessage::assistant("a1")],
        )
        .unwrap();
        upsert(&conn, &frame("src", key("p", "/ws"))).unwrap();

        let res = crate::agent::transcript_branch::fork_with_transcript(&conn, "src", "a1").unwrap();
        let forked = load(&conn, &res.id).unwrap().expect("fork has a frame");
        assert_eq!(forked.system_prompt, "prompt of src");
        assert_eq!(forked.tool_names, vec!["zeta", "alpha", "mid"]);
        assert_eq!(forked.key, key("p", "/ws"));
    }
}
