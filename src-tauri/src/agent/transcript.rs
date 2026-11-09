// SPDX-License-Identifier: Apache-2.0
//! Append-only per-session transcript of the exact messages sent to the model.
//!
//! Each row stores `serde_json::to_string(&ChatMessage)` so replay is
//! byte-identical to what was written. Compaction never rewrites rows: a
//! `session_block` summary row is appended and the rows it replaces are marked
//! `superseded` (and remember which block did so, so a rewind can undo it).

use rusqlite::{params, Connection};
use uclaw_message_types::{estimate_message_tokens, ChatMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptKind {
    Message,
    SessionBlock,
}

impl TranscriptKind {
    fn as_str(self) -> &'static str {
        match self {
            TranscriptKind::Message => "message",
            TranscriptKind::SessionBlock => "session_block",
        }
    }

    fn parse(s: &str) -> Self {
        if s == "session_block" {
            TranscriptKind::SessionBlock
        } else {
            TranscriptKind::Message
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptRow {
    pub id: i64,
    pub seq: i64,
    pub turn_id: Option<String>,
    pub kind: TranscriptKind,
    pub message: ChatMessage,
    pub est_tokens: u32,
}

fn next_seq(conn: &Connection, session_id: &str) -> anyhow::Result<i64> {
    let max: Option<i64> = conn.query_row(
        "SELECT MAX(seq) FROM agent_transcript WHERE session_id = ?1",
        [session_id],
        |r| r.get(0),
    )?;
    Ok(max.unwrap_or(0) + 1)
}

fn insert_row(
    conn: &Connection,
    session_id: &str,
    seq: i64,
    turn_id: Option<&str>,
    kind: TranscriptKind,
    msg: &ChatMessage,
) -> anyhow::Result<i64> {
    conn.execute(
        "INSERT INTO agent_transcript
         (session_id, seq, turn_id, kind, message_json, est_tokens, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            session_id,
            seq,
            turn_id,
            kind.as_str(),
            serde_json::to_string(msg)?,
            estimate_message_tokens(msg) as i64,
            chrono::Utc::now().timestamp_millis(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Append messages with contiguous seq numbers (single transaction). Returns row ids.
pub fn append(
    conn: &Connection,
    session_id: &str,
    turn_id: Option<&str>,
    kind: TranscriptKind,
    msgs: &[ChatMessage],
) -> anyhow::Result<Vec<i64>> {
    let tx = conn.unchecked_transaction()?;
    let mut seq = next_seq(&tx, session_id)?;
    let mut ids = Vec::with_capacity(msgs.len());
    for m in msgs {
        ids.push(insert_row(&tx, session_id, seq, turn_id, kind, m)?);
        seq += 1;
    }
    tx.commit()?;
    Ok(ids)
}

/// Atomically supersede every active row with `seq <= supersedes_through_seq`
/// and append `msg` as a `session_block` row. The superseded rows remember the
/// block's id so `truncate_after_turn` can restore them. Returns the block's row id.
pub fn append_session_block(
    conn: &Connection,
    session_id: &str,
    turn_id: Option<&str>,
    msg: &ChatMessage,
    supersedes_through_seq: i64,
) -> anyhow::Result<i64> {
    let tx = conn.unchecked_transaction()?;
    let seq = next_seq(&tx, session_id)?;
    let block_id = insert_row(&tx, session_id, seq, turn_id, TranscriptKind::SessionBlock, msg)?;
    tx.execute(
        "UPDATE agent_transcript SET superseded = 1, superseded_by = ?3
         WHERE session_id = ?1 AND superseded = 0 AND seq <= ?2 AND id != ?3",
        params![session_id, supersedes_through_seq, block_id],
    )?;
    tx.commit()?;
    Ok(block_id)
}

/// Compaction with a kept tail: atomically supersede EVERY active row
/// (recording the block's id), append `block` as a `session_block` row tagged
/// `turn_id`, then re-append the active rows after the first
/// `superseded_through` (the kept tail) as fresh rows with their original turn
/// ids and kinds — so `load_active` reads `[block, tail...]` in seq order.
/// Rows are never rewritten; removing the block (`truncate_after_turn`)
/// restores the originals. Errors (and changes nothing) unless
/// `1 <= superseded_through <= active rows`. Returns the block's row id.
pub fn replace_prefix_with_session_block(
    conn: &Connection,
    session_id: &str,
    turn_id: Option<&str>,
    block: &ChatMessage,
    superseded_through: usize,
) -> anyhow::Result<i64> {
    let tx = conn.unchecked_transaction()?;
    let rows = load_active(&tx, session_id)?;
    if superseded_through == 0 || superseded_through > rows.len() {
        anyhow::bail!(
            "session_block would supersede {superseded_through} rows but {} are active",
            rows.len()
        );
    }
    let last_seq = rows[rows.len() - 1].seq;
    let mut seq = next_seq(&tx, session_id)?;
    let block_id = insert_row(&tx, session_id, seq, turn_id, TranscriptKind::SessionBlock, block)?;
    tx.execute(
        "UPDATE agent_transcript SET superseded = 1, superseded_by = ?3
         WHERE session_id = ?1 AND superseded = 0 AND seq <= ?2 AND id != ?3",
        params![session_id, last_seq, block_id],
    )?;
    for r in &rows[superseded_through..] {
        seq += 1;
        insert_row(&tx, session_id, seq, r.turn_id.as_deref(), r.kind, &r.message)?;
    }
    tx.commit()?;
    Ok(block_id)
}

/// Non-superseded rows ordered by seq.
pub fn load_active(conn: &Connection, session_id: &str) -> anyhow::Result<Vec<TranscriptRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, seq, turn_id, kind, message_json, est_tokens FROM agent_transcript
         WHERE session_id = ?1 AND superseded = 0 ORDER BY seq",
    )?;
    let raw = stmt
        .query_map([session_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    raw.into_iter()
        .map(|(id, seq, turn_id, kind, json, est)| {
            Ok(TranscriptRow {
                id,
                seq,
                turn_id,
                kind: TranscriptKind::parse(&kind),
                message: serde_json::from_str(&json)?,
                est_tokens: est.max(0) as u32,
            })
        })
        .collect()
}

pub fn has_transcript(conn: &Connection, session_id: &str) -> anyhow::Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM agent_transcript WHERE session_id = ?1",
        [session_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Mark active rows with `seq <= max_seq_inclusive` superseded. Returns rows changed.
pub fn supersede_up_to(
    conn: &Connection,
    session_id: &str,
    max_seq_inclusive: i64,
) -> anyhow::Result<usize> {
    Ok(conn.execute(
        "UPDATE agent_transcript SET superseded = 1
         WHERE session_id = ?1 AND superseded = 0 AND seq <= ?2",
        params![session_id, max_seq_inclusive],
    )?)
}

/// One-time import of legacy `agent_messages` text rows (user/assistant/
/// system, not compacted) when the session has no transcript yet. `system`
/// rows (`/skill` prompts) are merged into the following user message by the
/// same rule as `history_window::replay_history_messages`. Turn ids: a user
/// message (merged or not) = the user row's id; an orphan system group = its
/// first row's id; an assistant inherits the preceding user message's turn.
/// Returns transcript rows written.
pub fn import_legacy_text_rows(conn: &Connection, session_id: &str) -> anyhow::Result<usize> {
    if has_transcript(conn, session_id)? {
        return Ok(0);
    }
    let mut stmt = conn.prepare(
        "SELECT id, role, content FROM agent_messages
         WHERE session_id = ?1 AND compacted = 0 AND role IN ('user', 'assistant', 'system')
         ORDER BY created_at, rowid",
    )?;
    let rows = stmt
        .query_map([session_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let merged = crate::agent::history_window::merge_skill_rows(
        rows.iter().map(|(id, role, content)| (role.as_str(), content.as_str(), id.as_str())),
    );

    let tx = conn.unchecked_transaction()?;
    let mut seq = 1;
    let mut current_turn: Option<&str> = None;
    for (msg, id) in &merged {
        if msg.role == uclaw_message_types::MessageRole::User {
            current_turn = Some(id);
        }
        insert_row(&tx, session_id, seq, current_turn, TranscriptKind::Message, msg)?;
        seq += 1;
    }
    tx.commit()?;
    Ok(merged.len())
}

/// Delete rows after the last row of `keep_through_turn_id`; rows superseded by
/// a deleted `session_block` become active again. Returns rows deleted
/// (0 and no change if the turn is unknown).
pub fn truncate_after_turn(
    conn: &Connection,
    session_id: &str,
    keep_through_turn_id: &str,
) -> anyhow::Result<usize> {
    let keep_max: Option<i64> = conn.query_row(
        "SELECT MAX(seq) FROM agent_transcript WHERE session_id = ?1 AND turn_id = ?2",
        params![session_id, keep_through_turn_id],
        |r| r.get(0),
    )?;
    let Some(keep_max) = keep_max else {
        return Ok(0);
    };
    let tx = conn.unchecked_transaction()?;
    let block_ids: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT id FROM agent_transcript
             WHERE session_id = ?1 AND seq > ?2 AND kind = 'session_block'",
        )?;
        let ids = stmt
            .query_map(params![session_id, keep_max], |r| r.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        ids
    };
    for id in &block_ids {
        tx.execute(
            "UPDATE agent_transcript SET superseded = 0, superseded_by = NULL
             WHERE session_id = ?1 AND superseded_by = ?2",
            params![session_id, id],
        )?;
    }
    let n = tx.execute(
        "DELETE FROM agent_transcript WHERE session_id = ?1 AND seq > ?2",
        params![session_id, keep_max],
    )?;
    tx.commit()?;
    Ok(n)
}

/// Copy the active rows of `from_session` up to the last row of `through_turn_id`
/// into `to_session` (re-sequenced from the destination's next seq). Returns rows copied.
pub fn copy_through_turn(
    conn: &Connection,
    from_session: &str,
    to_session: &str,
    through_turn_id: &str,
) -> anyhow::Result<usize> {
    let through_max: Option<i64> = conn.query_row(
        "SELECT MAX(seq) FROM agent_transcript WHERE session_id = ?1 AND turn_id = ?2",
        params![from_session, through_turn_id],
        |r| r.get(0),
    )?;
    let Some(through_max) = through_max else {
        return Ok(0);
    };
    let rows: Vec<TranscriptRow> = load_active(conn, from_session)?
        .into_iter()
        .filter(|r| r.seq <= through_max)
        .collect();
    let tx = conn.unchecked_transaction()?;
    let mut seq = next_seq(&tx, to_session)?;
    for r in &rows {
        insert_row(&tx, to_session, seq, r.turn_id.as_deref(), r.kind, &r.message)?;
        seq += 1;
    }
    tx.commit()?;
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serde_json::json;
    use uclaw_message_types::{ContentBlock, MessageRole};

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn seqs(rows: &[TranscriptRow]) -> Vec<i64> {
        rows.iter().map(|r| r.seq).collect()
    }

    #[test]
    fn v63_creates_agent_transcript() {
        let conn = db();
        crate::db::migrations::run(&conn).expect("idempotent");
        for col in [
            "id", "session_id", "seq", "turn_id", "kind", "message_json", "est_tokens",
            "superseded", "created_at",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('agent_transcript') WHERE name = ?1",
                    [col],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "missing column {col}");
        }
    }

    #[test]
    fn roundtrip_is_byte_identical() {
        let conn = db();
        let msgs = vec![
            ChatMessage::user("你好，请读取文件"),
            ChatMessage {
                role: MessageRole::Assistant,
                content: vec![
                    ContentBlock::Text { text: "好的".into() },
                    ContentBlock::ToolUse {
                        id: "t1".into(),
                        name: "read_file".into(),
                        input: json!({"path": "/tmp/中文.txt", "opts": {"a": [1, 2, {"b": null}]}}),
                    },
                ],
                compacted: false,
            },
            ChatMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "内容\n\"quoted\"".into(),
                    is_error: Some(false),
                }],
                compacted: false,
            },
        ];
        append(&conn, "s1", Some("turn1"), TranscriptKind::Message, &msgs).unwrap();
        let rows = load_active(&conn, "s1").unwrap();
        assert_eq!(rows.len(), 3);
        for (r, m) in rows.iter().zip(&msgs) {
            assert_eq!(
                serde_json::to_string(&r.message).unwrap(),
                serde_json::to_string(m).unwrap()
            );
            assert_eq!(r.turn_id.as_deref(), Some("turn1"));
            assert_eq!(r.kind, TranscriptKind::Message);
        }
        assert!(rows[1].est_tokens > 0);
    }

    #[test]
    fn append_assigns_contiguous_seq_per_session() {
        let conn = db();
        let m = ChatMessage::user("x");
        append(&conn, "a", None, TranscriptKind::Message, &[m.clone(), m.clone()]).unwrap();
        append(&conn, "b", None, TranscriptKind::Message, &[m.clone()]).unwrap();
        append(&conn, "a", None, TranscriptKind::Message, &[m.clone()]).unwrap();
        append(&conn, "b", None, TranscriptKind::Message, &[m.clone(), m.clone()]).unwrap();
        assert_eq!(seqs(&load_active(&conn, "a").unwrap()), vec![1, 2, 3]);
        assert_eq!(seqs(&load_active(&conn, "b").unwrap()), vec![1, 2, 3]);
        assert!(has_transcript(&conn, "a").unwrap());
        assert!(!has_transcript(&conn, "zzz").unwrap());
    }

    #[test]
    fn supersede_hides_rows() {
        let conn = db();
        let m = ChatMessage::user("x");
        append(&conn, "s", None, TranscriptKind::Message, &[m.clone(), m.clone(), m.clone(), m.clone(), m]).unwrap();
        assert_eq!(supersede_up_to(&conn, "s", 3).unwrap(), 3);
        assert_eq!(seqs(&load_active(&conn, "s").unwrap()), vec![4, 5]);
    }

    #[test]
    fn truncate_after_turn_restores_rows_superseded_by_removed_block() {
        let conn = db();
        let m = ChatMessage::user("x");
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[m.clone()]).unwrap();
        append(&conn, "s", Some("t2"), TranscriptKind::Message, &[m.clone()]).unwrap();
        append_session_block(&conn, "s", Some("t3"), &ChatMessage::user("summary"), 2).unwrap();
        append(&conn, "s", Some("t3"), TranscriptKind::Message, &[m.clone()]).unwrap();
        assert_eq!(seqs(&load_active(&conn, "s").unwrap()), vec![3, 4]);

        let removed = truncate_after_turn(&conn, "s", "t2").unwrap();
        assert_eq!(removed, 2);
        assert_eq!(seqs(&load_active(&conn, "s").unwrap()), vec![1, 2]);
    }

    #[test]
    fn truncate_keeps_rows_superseded_by_earlier_surviving_block() {
        let conn = db();
        let m = ChatMessage::user("x");
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[m.clone(), m.clone()]).unwrap();
        append_session_block(&conn, "s", Some("t1"), &ChatMessage::user("b1"), 2).unwrap(); // seq 3
        append(&conn, "s", Some("t2"), TranscriptKind::Message, &[m.clone()]).unwrap(); // seq 4
        append_session_block(&conn, "s", Some("t3"), &ChatMessage::user("b2"), 4).unwrap(); // seq 5
        assert_eq!(seqs(&load_active(&conn, "s").unwrap()), vec![5]);
        truncate_after_turn(&conn, "s", "t2").unwrap();
        // b1 and t2's row restored; rows 1-2 stay hidden behind b1.
        assert_eq!(seqs(&load_active(&conn, "s").unwrap()), vec![3, 4]);
    }

    #[test]
    fn copy_through_turn_copies_active_rows_only() {
        let conn = db();
        let m = ChatMessage::user("x");
        append(&conn, "src", Some("t1"), TranscriptKind::Message, &[m.clone(), m.clone()]).unwrap();
        append(&conn, "src", Some("t2"), TranscriptKind::Message, &[m.clone()]).unwrap();
        append(&conn, "src", Some("t3"), TranscriptKind::Message, &[m.clone()]).unwrap();
        supersede_up_to(&conn, "src", 1).unwrap();
        let n = copy_through_turn(&conn, "src", "dst", "t2").unwrap();
        assert_eq!(n, 2);
        let rows = load_active(&conn, "dst").unwrap();
        assert_eq!(seqs(&rows), vec![1, 2]);
        assert_eq!(rows[0].turn_id.as_deref(), Some("t1"));
        assert_eq!(rows[1].turn_id.as_deref(), Some("t2"));
    }

    #[test]
    fn import_legacy_text_rows_imports_once() {
        let conn = db();
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at) VALUES ('s','global','t',0,0)",
            [],
        )
        .ok();
        let ins = |id: &str, role: &str, content: &str, at: i64, compacted: i64| {
            conn.execute(
                "INSERT INTO agent_messages (id, session_id, role, content, created_at, compacted) VALUES (?1,'s',?2,?3,?4,?5)",
                rusqlite::params![id, role, content, at, compacted],
            )
            .unwrap();
        };
        ins("u1", "user", "hi", 1, 0);
        ins("a1", "assistant", "hello", 2, 0);
        ins("sys", "system", "ignore", 3, 0);
        ins("u2", "user", "old", 4, 1);
        ins("u3", "user", "again", 5, 0);
        ins("a3", "assistant", "yes", 6, 0);
        assert_eq!(import_legacy_text_rows(&conn, "s").unwrap(), 4);
        let rows = load_active(&conn, "s").unwrap();
        let turns: Vec<_> = rows.iter().map(|r| r.turn_id.clone().unwrap()).collect();
        assert_eq!(turns, vec!["u1", "u1", "u3", "u3"]);
        assert_eq!(rows[1].message.role, MessageRole::Assistant);
        assert_eq!(import_legacy_text_rows(&conn, "s").unwrap(), 0);
    }

    #[test]
    fn legacy_import_merges_skill_system_rows() {
        let conn = db();
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at) VALUES ('s','global','t',0,0)",
            [],
        )
        .unwrap();
        let ins = |id: &str, role: &str, content: &str, at: i64| {
            conn.execute(
                "INSERT INTO agent_messages (id, session_id, role, content, created_at, compacted) VALUES (?1,'s',?2,?3,?4,0)",
                rusqlite::params![id, role, content, at],
            )
            .unwrap();
        };
        let skill = "<skill name=\"tdd\" version=\"1\">\nWrite the test first.\n</skill>";
        ins("u1", "user", "earlier", 1);
        ins("a1", "assistant", "ok", 2);
        ins("sk", "system", skill, 3);
        ins("u2", "user", "/tdd fix the parser", 4);
        ins("a2", "assistant", "done", 5);
        ins("orph", "system", "<command name=\"x\">\nbody\n</command>", 6);
        ins("a3", "assistant", "after orphan", 7);
        ins("tail", "system", "trailing", 8);

        assert_eq!(import_legacy_text_rows(&conn, "s").unwrap(), 7);
        let rows = load_active(&conn, "s").unwrap();
        let roles: Vec<_> = rows.iter().map(|r| r.message.role.clone()).collect();
        assert_eq!(
            roles,
            vec![
                MessageRole::User,
                MessageRole::Assistant,
                MessageRole::User,
                MessageRole::Assistant,
                MessageRole::User,
                MessageRole::Assistant,
                MessageRole::User,
            ]
        );
        let t = texts(&rows);
        assert_eq!(t[2], format!("{skill}\n\n/tdd fix the parser"));
        assert!(t[4].contains("body"));
        assert_eq!(t[6], "trailing");
        let turns: Vec<_> = rows.iter().map(|r| r.turn_id.clone().unwrap()).collect();
        assert_eq!(turns, vec!["u1", "u1", "u2", "u2", "orph", "orph", "tail"]);
    }

    #[test]
    fn replace_prefix_with_session_block_puts_block_before_kept_tail() {
        let conn = db();
        let u = |t: &str| ChatMessage::user(t);
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[u("u1"), u("a1")]).unwrap();
        append(&conn, "s", Some("t2"), TranscriptKind::Message, &[u("u2"), u("a2")]).unwrap();
        let block_id =
            replace_prefix_with_session_block(&conn, "s", Some("t3"), &u("block"), 2).unwrap();

        let rows = load_active(&conn, "s").unwrap();
        let texts: Vec<_> = rows
            .iter()
            .map(|r| match &r.message.content[0] {
                ContentBlock::Text { text } => text.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(texts, vec!["block", "u2", "a2"]);
        assert_eq!(rows[0].id, block_id);
        assert_eq!(rows[0].kind, TranscriptKind::SessionBlock);
        assert_eq!(rows[0].turn_id.as_deref(), Some("t3"));
        assert_eq!(rows[1].turn_id.as_deref(), Some("t2"), "tail keeps its turn ids");
        let by_block: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_transcript WHERE session_id = 's' AND superseded_by = ?1",
                [block_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(by_block, 4, "every original row remembers the block");

        // Rewinding to before the block restores the originals.
        truncate_after_turn(&conn, "s", "t1").unwrap();
        assert_eq!(seqs(&load_active(&conn, "s").unwrap()), vec![1, 2]);
    }

    #[test]
    fn replace_prefix_with_session_block_rejects_bad_cut() {
        let conn = db();
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[ChatMessage::user("x")]).unwrap();
        let b = ChatMessage::user("block");
        assert!(replace_prefix_with_session_block(&conn, "s", None, &b, 0).is_err());
        assert!(replace_prefix_with_session_block(&conn, "s", None, &b, 2).is_err());
        assert_eq!(seqs(&load_active(&conn, "s").unwrap()), vec![1]);
    }

    fn texts(rows: &[TranscriptRow]) -> Vec<String> {
        rows.iter()
            .map(|r| match &r.message.content[0] {
                ContentBlock::Text { text } => text.clone(),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn truncate_to_kept_tail_turn_keeps_block_and_copies() {
        let conn = db();
        let u = |t: &str| ChatMessage::user(t);
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[u("t1")]).unwrap();
        append(&conn, "s", Some("t2"), TranscriptKind::Message, &[u("t2")]).unwrap();
        replace_prefix_with_session_block(&conn, "s", Some("t3"), &u("b1"), 1).unwrap();
        append(&conn, "s", Some("t3"), TranscriptKind::Message, &[u("t3")]).unwrap();

        truncate_after_turn(&conn, "s", "t2").unwrap();
        assert_eq!(texts(&load_active(&conn, "s").unwrap()), vec!["b1", "t2"], "no duplicate t2");
    }

    #[test]
    fn truncate_after_nested_compactions() {
        // t1 t2 | b1@t3 (supersedes t1, tail t2c) t3 t4 | b2@t5 (supersedes b1 t2c t3, tail t4c)
        let build = || {
            let conn = db();
            let u = |t: &str| ChatMessage::user(t);
            append(&conn, "s", Some("t1"), TranscriptKind::Message, &[u("t1")]).unwrap();
            append(&conn, "s", Some("t2"), TranscriptKind::Message, &[u("t2")]).unwrap();
            replace_prefix_with_session_block(&conn, "s", Some("t3"), &u("b1"), 1).unwrap();
            append(&conn, "s", Some("t3"), TranscriptKind::Message, &[u("t3")]).unwrap();
            append(&conn, "s", Some("t4"), TranscriptKind::Message, &[u("t4")]).unwrap();
            replace_prefix_with_session_block(&conn, "s", Some("t5"), &u("b2"), 3).unwrap();
            assert_eq!(texts(&load_active(&conn, "s").unwrap()), vec!["b2", "t4"]);
            conn
        };

        let conn = build();
        truncate_after_turn(&conn, "s", "t3").unwrap();
        assert_eq!(texts(&load_active(&conn, "s").unwrap()), vec!["b1", "t2", "t3"]);

        let conn = build();
        truncate_after_turn(&conn, "s", "t2").unwrap();
        assert_eq!(texts(&load_active(&conn, "s").unwrap()), vec!["b1", "t2"]);

        let conn = build();
        truncate_after_turn(&conn, "s", "t1").unwrap();
        // Keep-through-t1: every later row (t2's original included) is removed.
        assert_eq!(texts(&load_active(&conn, "s").unwrap()), vec!["t1"]);
    }
}
