// SPDX-License-Identifier: Apache-2.0
//! Rewind / fork that keep the agent transcript in sync with `agent_messages`
//! (context continuity P1, spec §6.9).
//!
//! Both run the `session_tree` change and the transcript change in ONE SQLite
//! transaction. Callers hold the session's turn lock (and the DB lock).
//!
//! Mapping: the commands receive an `agent_messages` id (the last message of
//! an assistant turn). `session_tree` keeps every row with
//! `created_at <= target.created_at`; the transcript is cut at turn
//! granularity through the latest kept `agent_messages` row whose id is a
//! transcript `turn_id` (= that turn's user row; compaction display rows and
//! assistant rows never are). Sessions without a transcript are untouched.

use crate::agent::session_tree::{self, ForkResult, RewindResult};
use crate::agent::session_turn_lock::SessionTurnLocks;
use tokio::sync::OwnedMutexGuard;
use crate::agent::file_working_set;
use crate::agent::transcript;
use crate::error::Error;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;

fn internal(e: anyhow::Error) -> Error {
    Error::Internal(format!("transcript sync: {e}"))
}

/// Latest `agent_messages` row of `session_id` with `created_at <= at` whose
/// id tags transcript rows, i.e. the transcript turn that row range ends in.
fn latest_transcript_turn(conn: &Connection, session_id: &str, at: i64) -> Result<Option<String>, Error> {
    Ok(conn
        .query_row(
            "SELECT m.id FROM agent_messages m
             WHERE m.session_id = ?1 AND m.created_at <= ?2
               AND m.id IN (SELECT turn_id FROM agent_transcript WHERE session_id = ?1)
             ORDER BY m.created_at DESC, m.rowid DESC LIMIT 1",
            params![session_id, at],
            |r| r.get(0),
        )
        .optional()?)
}

fn message_created_at(conn: &Connection, session_id: &str, message_id: &str) -> Result<i64, Error> {
    conn.query_row(
        "SELECT created_at FROM agent_messages WHERE id = ?1 AND session_id = ?2",
        params![message_id, session_id],
        |r| r.get(0),
    )
    .map_err(|_| Error::NotFound(format!("message {message_id} not in session {session_id}")))
}

/// `session_tree::rewind_to` plus the matching transcript truncation, atomically.
/// If no kept row maps to a transcript turn, every transcript row belongs to a
/// deleted turn: the transcript is cleared and the next send re-imports the
/// kept `agent_messages` (legacy import path).
pub fn rewind_with_transcript(
    conn: &Connection,
    session_id: &str,
    target_message_id: &str,
) -> Result<RewindResult, Error> {
    let tx = conn.unchecked_transaction()?;
    let res = session_tree::rewind_to(&tx, session_id, target_message_id)?;
    // The V52 fold baseline may summarise turns being rewound away.
    crate::agent::compact::delete_baseline(&tx, session_id)?;
    // P2: file events of rewound turns stay (their disk changes remain) but
    // are marked `rewound`. Kept turn ids = the surviving agent_messages ids.
    let kept: HashSet<String> = {
        let mut stmt = tx.prepare("SELECT id FROM agent_messages WHERE session_id = ?1")?;
        let ids = stmt
            .query_map([session_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        ids
    };
    file_working_set::mark_rewound_after(&tx, session_id, &kept).map_err(internal)?;
    if transcript::has_transcript(&tx, session_id).map_err(internal)? {
        match latest_transcript_turn(&tx, session_id, i64::MAX)? {
            Some(turn) => {
                transcript::truncate_after_turn_within(&tx, session_id, &turn).map_err(internal)?;
            }
            None => {
                // Every transcript row belongs to a deleted turn: clear it so the
                // next send re-imports the kept agent_messages. Invariant: this
                // never re-imports a compaction display row (whose summarized
                // originals stay at compacted = 0, so importing both would
                // duplicate content). A display row is inserted just before its
                // anchor turn's user row (created_at - 1), and that anchor turn
                // has active kept-tail transcript rows; rewind targets are
                // assistant rows, so keeping the display row keeps the anchor
                // user row too, which maps to a transcript turn and takes the
                // `Some` branch above instead.
                tx.execute("DELETE FROM agent_transcript WHERE session_id = ?1", params![session_id])?;
            }
        }
    }
    tx.commit()?;
    Ok(res)
}

/// `session_tree::fork_at` plus the matching transcript clone, atomically.
/// The fork's transcript is the source's as a rewind to the fork point would
/// leave it (`transcript::clone_through_turn_within`), with turn ids mapped to
/// the fork's own `agent_messages` ids. If the fork point maps to no
/// transcript turn the fork gets no transcript and imports its copied
/// `agent_messages` on first send.
pub fn fork_with_transcript(
    conn: &Connection,
    source_session: &str,
    up_to_message_id: &str,
) -> Result<ForkResult, Error> {
    let tx = conn.unchecked_transaction()?;
    let fork_ts = message_created_at(&tx, source_session, up_to_message_id)?;
    let res = session_tree::fork_at(&tx, source_session, up_to_message_id)?;
    if let Some(turn) = latest_transcript_turn(&tx, source_session, fork_ts)? {
        transcript::clone_through_turn_within(
            &tx,
            source_session,
            &res.id,
            &turn,
            &res.message_id_map,
        )
        .map_err(internal)?;
    }
    // P2: the fork inherits the file working set; events after the fork
    // point are copied as rewound (their disk changes remain).
    file_working_set::clone_for_fork(&tx, source_session, &res.id, &res.message_id_map)
        .map_err(internal)?;
    tx.commit()?;
    Ok(res)
}

/// How long rewind/fork wait for a busy session turn lock before refusing.
pub const BRANCH_TURN_LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// Acquire `session_id`'s turn lock within `wait`, or refuse with `refusal`
/// (rewind/fork never cancel or outwait a running turn).
pub async fn lock_idle_session(
    locks: &SessionTurnLocks,
    session_id: &str,
    wait: std::time::Duration,
    refusal: &str,
) -> Result<OwnedMutexGuard<()>, Error> {
    tokio::time::timeout(wait, locks.acquire(session_id))
        .await
        .map_err(|_| Error::InvalidInput(refusal.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::transcript::{self, TranscriptKind, TranscriptRow};
    use rusqlite::params;
    use uclaw_message_types::{ChatMessage, ContentBlock};

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn session(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at) VALUES (?1,'default','S','{}',0,0,0,1000,1000)",
            params![id],
        )
        .unwrap();
    }

    fn msg(conn: &Connection, session_id: &str, id: &str, role: &str, at: i64) {
        conn.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at, compacted) VALUES (?1,?2,?3,?4,?5,0)",
            params![id, session_id, role, id, at],
        )
        .unwrap();
    }

    /// Turn `n`: agent_messages `u{n}`/`a{n}` (created_at 1000+2n / +1) and
    /// transcript rows `u{n}`, `a{n}` tagged with turn id `u{n}`.
    fn turn(conn: &Connection, s: &str, n: i64) {
        let (u, a) = (format!("u{n}"), format!("a{n}"));
        msg(conn, s, &u, "user", 1000 + 2 * n);
        msg(conn, s, &a, "assistant", 1001 + 2 * n);
        transcript::append(
            conn,
            s,
            Some(&u),
            TranscriptKind::Message,
            &[ChatMessage::user(&u), ChatMessage::assistant(&a)],
        )
        .unwrap();
    }

    fn texts(rows: &[TranscriptRow]) -> Vec<String> {
        rows.iter()
            .map(|r| match &r.message.content[0] {
                ContentBlock::Text { text } => text.clone(),
                _ => String::new(),
            })
            .collect()
    }

    fn active(conn: &Connection, s: &str) -> Vec<String> {
        texts(&transcript::load_active(conn, s).unwrap())
    }

    fn message_ids(conn: &Connection, s: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT id FROM agent_messages WHERE session_id = ?1 ORDER BY created_at, rowid")
            .unwrap();
        stmt.query_map([s], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    }

    fn user_id_with_content(conn: &Connection, s: &str, content: &str) -> String {
        conn.query_row(
            "SELECT id FROM agent_messages WHERE session_id = ?1 AND role = 'user' AND content = ?2",
            params![s, content],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// Turns 1-2 complete; turn 3's user row written, then a compaction during
    /// turn 3 folds turns 1-2 into block `b` (tagged `u3`, kept tail = `u3`),
    /// with the display row before `u3`; then `a3`. Active: [b, u3, a3].
    fn compacted_session(conn: &Connection, s: &str) {
        session(conn, s);
        turn(conn, s, 1);
        turn(conn, s, 2);
        msg(conn, s, "u3", "user", 1006);
        transcript::append(conn, s, Some("u3"), TranscriptKind::Message, &[ChatMessage::user("u3")])
            .unwrap();
        transcript::replace_prefix_with_session_block(conn, s, Some("u3"), &ChatMessage::user("b"), 4)
            .unwrap();
        msg(conn, s, "display", "user", 1005);
        msg(conn, s, "a3", "assistant", 1007);
        transcript::append(conn, s, Some("u3"), TranscriptKind::Message, &[ChatMessage::assistant("a3")])
            .unwrap();
        assert_eq!(active(conn, s), vec!["b", "u3", "a3"]);
    }

    #[test]
    fn rewind_truncates_transcript_to_turn() {
        let conn = db();
        session(&conn, "s");
        for n in 1..=3 {
            turn(&conn, "s", n);
        }
        let res = rewind_with_transcript(&conn, "s", "a2").unwrap();
        assert_eq!(res.deleted, 2);
        assert_eq!(message_ids(&conn, "s"), vec!["u1", "a1", "u2", "a2"]);
        assert_eq!(active(&conn, "s"), vec!["u1", "a1", "u2", "a2"]);
    }

    #[test]
    fn rewind_before_compaction_restores_raw_rows() {
        let conn = db();
        compacted_session(&conn, "s");
        rewind_with_transcript(&conn, "s", "a1").unwrap();
        assert_eq!(message_ids(&conn, "s"), vec!["u1", "a1"]);
        assert_eq!(active(&conn, "s"), vec!["u1", "a1"], "raw rows back, block dropped");
        let blocks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_transcript WHERE session_id = 's' AND kind = 'session_block'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(blocks, 0);
    }

    #[test]
    fn fork_copies_transcript_through_turn() {
        let conn = db();
        session(&conn, "s");
        for n in 1..=3 {
            turn(&conn, "s", n);
        }
        let res = fork_with_transcript(&conn, "s", "a2").unwrap();
        assert_eq!(res.message_count, 4);
        let rows = transcript::load_active(&conn, &res.id).unwrap();
        assert_eq!(texts(&rows), vec!["u1", "a1", "u2", "a2"]);
        // Turn ids point at the fork's own agent_messages user rows.
        let nu1 = user_id_with_content(&conn, &res.id, "u1");
        let nu2 = user_id_with_content(&conn, &res.id, "u2");
        let turns: Vec<_> = rows.iter().map(|r| r.turn_id.clone().unwrap()).collect();
        assert_eq!(turns, vec![nu1.clone(), nu1, nu2.clone(), nu2]);
        // Source untouched.
        assert_eq!(active(&conn, "s"), vec!["u1", "a1", "u2", "a2", "u3", "a3"]);
    }

    #[test]
    fn fork_through_fully_compacted_turn_restores_raw_rows() {
        let conn = db();
        compacted_session(&conn, "s");

        // Turn 1 lives only inside block `b` (which also summarizes turn 2).
        let early = fork_with_transcript(&conn, "s", "a1").unwrap();
        assert_eq!(active(&conn, &early.id), vec!["u1", "a1"], "no summary of later turns");

        // A fork after the compaction keeps the block, and its superseded
        // originals too: rewinding inside the fork can still restore them.
        let late = fork_with_transcript(&conn, "s", "a3").unwrap();
        assert_eq!(active(&conn, &late.id), vec!["b", "u3", "a3"]);
        let na1 = conn
            .query_row(
                "SELECT id FROM agent_messages WHERE session_id = ?1 AND content = 'a1'",
                [&late.id],
                |r| r.get::<_, String>(0),
            )
            .unwrap();
        rewind_with_transcript(&conn, &late.id, &na1).unwrap();
        assert_eq!(active(&conn, &late.id), vec!["u1", "a1"]);
        // Source untouched.
        assert_eq!(active(&conn, "s"), vec!["b", "u3", "a3"]);
    }

    #[test]
    fn rewind_leaving_block_tagged_with_deleted_turn_then_second_rewind() {
        let conn = db();
        session(&conn, "s");
        turn(&conn, "s", 1);
        turn(&conn, "s", 2);
        // Turn 3 compacts turn 1 (block tagged u3, tail = turn 2 + u3).
        msg(&conn, "s", "u3", "user", 1006);
        transcript::append(&conn, "s", Some("u3"), TranscriptKind::Message, &[ChatMessage::user("u3")])
            .unwrap();
        transcript::replace_prefix_with_session_block(&conn, "s", Some("u3"), &ChatMessage::user("b"), 2)
            .unwrap();
        msg(&conn, "s", "a3", "assistant", 1007);
        transcript::append(&conn, "s", Some("u3"), TranscriptKind::Message, &[ChatMessage::assistant("a3")])
            .unwrap();
        assert_eq!(active(&conn, "s"), vec!["b", "u2", "a2", "u3", "a3"]);

        // Rewind to turn 2: the block survives, tagged with deleted turn u3.
        rewind_with_transcript(&conn, "s", "a2").unwrap();
        assert_eq!(message_ids(&conn, "s"), vec!["u1", "a1", "u2", "a2"]);
        assert_eq!(active(&conn, "s"), vec!["b", "u2", "a2"]);

        // A new turn, then rewinds again: still consistent.
        turn(&conn, "s", 4);
        assert_eq!(active(&conn, "s"), vec!["b", "u2", "a2", "u4", "a4"]);
        rewind_with_transcript(&conn, "s", "a2").unwrap();
        assert_eq!(active(&conn, "s"), vec!["b", "u2", "a2"]);
        let fork = fork_with_transcript(&conn, "s", "a1").unwrap();
        assert_eq!(active(&conn, &fork.id), vec!["u1", "a1"]);
        rewind_with_transcript(&conn, "s", "a1").unwrap();
        assert_eq!(active(&conn, "s"), vec!["u1", "a1"]);
    }

    #[test]
    fn rewind_to_turn_missing_from_transcript_clears_it() {
        // Turn 1 predates the transcript (e.g. legacy folded rows); only turn 2 is in it.
        let conn = db();
        session(&conn, "s");
        msg(&conn, "s", "u1", "user", 1002);
        msg(&conn, "s", "a1", "assistant", 1003);
        turn(&conn, "s", 2);
        assert_eq!(active(&conn, "s"), vec!["u2", "a2"]);
        rewind_with_transcript(&conn, "s", "a1").unwrap();
        assert_eq!(message_ids(&conn, "s"), vec!["u1", "a1"]);
        assert!(
            !transcript::has_transcript(&conn, "s").unwrap(),
            "next send re-imports the kept agent_messages"
        );
    }

    #[test]
    fn legacy_session_rewind_and_fork_unaffected() {
        let conn = db();
        session(&conn, "s");
        for (id, role, at) in [("u1", "user", 1), ("a1", "assistant", 2), ("u2", "user", 3), ("a2", "assistant", 4)] {
            msg(&conn, "s", id, role, at);
        }
        let fork = fork_with_transcript(&conn, "s", "a1").unwrap();
        assert_eq!(fork.message_count, 2);
        assert!(!transcript::has_transcript(&conn, &fork.id).unwrap());
        let res = rewind_with_transcript(&conn, "s", "a1").unwrap();
        assert_eq!(res.deleted, 2);
        assert_eq!(message_ids(&conn, "s"), vec!["u1", "a1"]);
        assert!(!transcript::has_transcript(&conn, "s").unwrap());
    }

    #[tokio::test]
    async fn lock_idle_session_refuses_when_turn_lock_held() {
        let locks = SessionTurnLocks::default();
        let _held = locks.acquire("s").await;
        let started = std::time::Instant::now();
        let res = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            lock_idle_session(&locks, "s", std::time::Duration::from_millis(50), "先停止 agent 再回溯"),
        )
        .await
        .expect("must refuse within the bound, not wait for the turn");
        match res {
            Err(Error::InvalidInput(m)) => assert_eq!(m, "先停止 agent 再回溯"),
            other => panic!("expected refusal, got {:?}", other.map(|_| ())),
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[tokio::test]
    async fn lock_idle_session_proceeds_when_lock_free() {
        let locks = SessionTurnLocks::default();
        let guard = lock_idle_session(&locks, "s", std::time::Duration::from_millis(50), "x")
            .await
            .expect("free lock is acquired");
        // The guard really holds the session's lock.
        assert!(locks.mutex_for("s").try_lock().is_err());
        drop(guard);
        assert!(locks.mutex_for("s").try_lock().is_ok());
    }

    fn history(conn: &Connection, s: &str) -> Vec<String> {
        let (msgs, _) = crate::agent::transcript_history::build_agent_history(conn, s).unwrap();
        msgs.iter()
            .map(|m| match &m.content[0] {
                ContentBlock::Text { text } => text.clone(),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn history_after_rewind_is_exactly_kept_turns() {
        let conn = db();
        session(&conn, "s");
        for n in 1..=3 {
            turn(&conn, "s", n);
        }
        rewind_with_transcript(&conn, "s", "a2").unwrap();
        assert_eq!(history(&conn, "s"), vec!["u1", "a1", "u2", "a2"]);

        // After a compaction: rewinding into the compacted range replays raw
        // rows; rewinding past it keeps [block, kept turn].
        let conn = db();
        compacted_session(&conn, "s");
        turn(&conn, "s", 4);
        rewind_with_transcript(&conn, "s", "a3").unwrap();
        assert_eq!(history(&conn, "s"), vec!["b", "u3", "a3"]);
        rewind_with_transcript(&conn, "s", "a2").unwrap();
        assert_eq!(history(&conn, "s"), vec!["u1", "a1", "u2", "a2"]);

        // Cleared transcript: the kept agent_messages are re-imported, nothing after.
        let conn = db();
        session(&conn, "s");
        msg(&conn, "s", "u1", "user", 1002);
        msg(&conn, "s", "a1", "assistant", 1003);
        turn(&conn, "s", 2);
        rewind_with_transcript(&conn, "s", "a1").unwrap();
        assert_eq!(history(&conn, "s"), vec!["u1", "a1"]);
    }

    /// Rewind drops the session's V52 fold baseline (it can summarise rewound
    /// turns); a fork never inherits one.
    #[test]
    fn rewind_drops_fold_baseline_and_fork_does_not_inherit_it() {
        let conn = db();
        compacted_session(&conn, "s");
        let fold = crate::agent::compact::StructuredFold::default()
            .with_unresolved_questions(vec!["covers turns 1-2".into()]);
        crate::agent::compact::upsert_baseline(&conn, "s", &fold).unwrap();

        let fork = fork_with_transcript(&conn, "s", "a1").unwrap();
        assert!(crate::agent::compact::load_baseline(&conn, &fork.id).is_none());
        assert!(crate::agent::compact::load_baseline(&conn, "s").is_some(), "fork leaves source alone");

        rewind_with_transcript(&conn, "s", "a1").unwrap();
        assert!(crate::agent::compact::load_baseline(&conn, "s").is_none());
    }

    fn edit(conn: &Connection, s: &str, turn_id: &str, path: &str) {
        use crate::agent::file_working_set::{record, FileEvent, FileOp};
        record(conn, s, Some(turn_id), &[FileEvent { path: path.into(), op: FileOp::Edit, detail: format!("edit {path}") }])
            .unwrap();
    }

    fn rewound_by_path(conn: &Connection, s: &str) -> Vec<(String, bool, u32)> {
        let mut v: Vec<_> = crate::agent::file_working_set::load(conn, s)
            .unwrap()
            .modified
            .into_iter()
            .map(|m| (m.path, m.rewound, m.edit_count))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn rewind_marks_events_and_fork_copies_marks() {
        let conn = db();
        session(&conn, "s");
        for (n, path) in [(1, "a.rs"), (2, "b.rs"), (3, "c.rs")] {
            turn(&conn, "s", n);
            edit(&conn, "s", &format!("u{n}"), path);
        }
        // An event without a turn id is never touched by rewind.
        crate::agent::file_working_set::record(
            &conn,
            "s",
            None,
            &[crate::agent::file_working_set::FileEvent {
                path: "loose.rs".into(),
                op: crate::agent::file_working_set::FileOp::Edit,
                detail: String::new(),
            }],
        )
        .unwrap();

        // Fork from the full session first: events after the fork point
        // (turn 3) are copied too, marked rewound.
        let fork = fork_with_transcript(&conn, "s", "a2").unwrap();
        assert_eq!(
            rewound_by_path(&conn, &fork.id),
            vec![
                ("a.rs".into(), false, 1),
                ("b.rs".into(), false, 1),
                ("c.rs".into(), true, 1),
                ("loose.rs".into(), false, 1),
            ]
        );
        // Turn ids point at the fork's own user rows.
        let nu1 = user_id_with_content(&conn, &fork.id, "u1");
        let a_turn: String = conn
            .query_row(
                "SELECT turn_id FROM agent_file_events WHERE session_id = ?1 AND path = 'a.rs'",
                [&fork.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(a_turn, nu1);
        // Source untouched by the fork.
        assert!(rewound_by_path(&conn, "s").iter().all(|(_, rw, _)| !rw));

        // Rewind the source to turn 2: c.rs's event is marked, not deleted.
        rewind_with_transcript(&conn, "s", "a2").unwrap();
        assert_eq!(
            rewound_by_path(&conn, "s"),
            vec![
                ("a.rs".into(), false, 1),
                ("b.rs".into(), false, 1),
                ("c.rs".into(), true, 1),
                ("loose.rs".into(), false, 1),
            ]
        );

        // Fork after the rewind carries the rewound mark over.
        let fork2 = fork_with_transcript(&conn, "s", "a2").unwrap();
        assert_eq!(rewound_by_path(&conn, &fork2.id), rewound_by_path(&conn, "s"));

        // Editing c.rs again in the fork: still flagged (a rewound change
        // exists on disk), edit count accumulates.
        msg(&conn, &fork2.id, "fu", "user", 2000);
        edit(&conn, &fork2.id, "fu", "c.rs");
        let c = rewound_by_path(&conn, &fork2.id).into_iter().find(|(p, ..)| p == "c.rs").unwrap();
        assert_eq!(c, ("c.rs".into(), true, 2));
    }

    #[test]
    fn failed_rewind_does_not_mark_file_events() {
        let conn = db();
        session(&conn, "s");
        for (n, path) in [(1, "a.rs"), (2, "b.rs")] {
            turn(&conn, "s", n);
            edit(&conn, "s", &format!("u{n}"), path);
        }
        // Make the transcript truncation (after the marking) fail.
        conn.execute_batch(
            "CREATE TRIGGER fail_truncate BEFORE DELETE ON agent_transcript
             BEGIN SELECT RAISE(ABORT, 'boom'); END;
             CREATE TRIGGER fail_truncate_u BEFORE UPDATE ON agent_transcript
             BEGIN SELECT RAISE(ABORT, 'boom'); END;",
        )
        .unwrap();
        assert!(rewind_with_transcript(&conn, "s", "a1").is_err());
        assert!(rewound_by_path(&conn, "s").iter().all(|(_, rw, _)| !rw), "rolled back");
        assert_eq!(message_ids(&conn, "s"), vec!["u1", "a1", "u2", "a2"]);
    }

    #[test]
    fn rewound_file_changes_reach_the_next_request_after_rewind() {
        use crate::agent::dispatcher::{
            build_request_messages, rewound_changes_from_sink, with_rewound_changes, TranscriptSink,
        };
        let conn = db();
        session(&conn, "s");
        for (n, path) in [(1, "a.rs"), (2, "b.rs"), (3, "c.rs")] {
            turn(&conn, "s", n);
            edit(&conn, "s", &format!("u{n}"), path);
        }
        let sink = TranscriptSink {
            db: std::sync::Arc::new(std::sync::Mutex::new(conn)),
            session_id: "s".into(),
            turn_id: "next".into(),
        };
        assert_eq!(rewound_changes_from_sink(Some(&sink)), "", "nothing rewound → no block");
        assert_eq!(with_rewound_changes("<dyn>".into(), ""), "<dyn>", "no block → dynamic unchanged");

        rewind_with_transcript(&sink.db.lock().unwrap(), "s", "a2").unwrap();
        let block = rewound_changes_from_sink(Some(&sink));
        assert!(block.contains("- c.rs（修改 1 次） [来自已回退的轮次，磁盘上仍保留]"), "{block}");
        assert!(block.contains("· edit c.rs"), "{block}");
        assert!(!block.contains("a.rs") && !block.contains("b.rs"), "{block}");

        let mut ctx = crate::agent::types::ReasoningContext::new("base".into());
        ctx.messages = vec![ChatMessage::user("which files did you change?")];
        let snap = crate::agent::turn::TurnSnapshot {
            turn_index: 1,
            model: "m".into(),
            system_prompt: std::sync::Arc::new("SYS".into()),
            dynamic_context: with_rewound_changes("<dyn>now</dyn>".into(), &block),
            tools: std::sync::Arc::new(Vec::new()),
            force_text: false,
        };
        let sent = build_request_messages(&ctx, &snap, true);
        let ContentBlock::Text { text } = &sent.last().unwrap().content[0] else { panic!("text") };
        assert!(text.starts_with("<dyn>now</dyn>\n\n<rewound_file_changes>"), "{text}");
        assert!(text.contains("c.rs") && text.contains("来自已回退的轮次，磁盘上仍保留"), "{text}");
        assert!(text.ends_with("which files did you change?"));

        assert_eq!(rewound_changes_from_sink(None), "", "no sink (non-transcript mode) → no block");
    }
}
