// SPDX-License-Identifier: Apache-2.0
//! Context continuity P1 — per-iteration transcript persistence with a
//! tool-call pairing guard.
//!
//! `flush_transcript` appends the not-yet-persisted tail of
//! `ReasoningContext.messages` to the transcript via
//! `LoopDelegate::persist_transcript`, but never past an assistant message
//! whose `ToolUse` blocks are still missing results (`complete_prefix_len`).
//! `finalize_transcript` closes a run: dangling tool calls get a synthesized
//! `[工具调用已取消]` error result so the persisted transcript is always a
//! valid tool_use → tool_result sequence.

use crate::agent::types::{
    ChatMessage, ContentBlock, LoopDelegate, LoopOutcome, MessageRole, ReasoningContext,
};
use rusqlite::Connection;

/// Result text synthesized for a `ToolUse` that never got a result.
pub const CANCELLED_TOOL_RESULT: &str = "[工具调用已取消]";

/// Tool-use ids still awaiting a result after scanning `msgs`, in order of
/// appearance, plus the longest prefix length at which none were pending.
/// `compacted` messages are neutral (neither open nor close calls).
fn scan(msgs: &[ChatMessage]) -> (usize, Vec<String>) {
    let mut pending: Vec<String> = Vec::new();
    let mut complete = 0;
    for (i, m) in msgs.iter().enumerate() {
        if !m.compacted {
            for b in &m.content {
                match b {
                    ContentBlock::ToolUse { id, .. } => pending.push(id.clone()),
                    ContentBlock::ToolResult { tool_use_id, .. } => {
                        pending.retain(|p| p != tool_use_id)
                    }
                    _ => {}
                }
            }
        }
        if pending.is_empty() {
            complete = i + 1;
        }
    }
    (complete, pending)
}

/// Length of the longest prefix of `msgs` that ends with no `ToolUse`
/// lacking its matching `ToolResult` (parallel calls need ALL results).
pub fn complete_prefix_len(msgs: &[ChatMessage]) -> usize {
    scan(msgs).0
}

/// For `ToolUse` blocks left dangling at the end of `msgs`, build one
/// `user` message carrying an `is_error = true` `[工具调用已取消]` result per
/// dangling call. `None` when every call already has a result.
pub fn synthesize_missing_tool_results(msgs: &[ChatMessage]) -> Option<ChatMessage> {
    let (_, pending) = scan(msgs);
    if pending.is_empty() {
        return None;
    }
    Some(ChatMessage {
        role: MessageRole::User,
        content: pending
            .into_iter()
            .map(|id| ContentBlock::ToolResult {
                tool_use_id: id,
                content: CANCELLED_TOOL_RESULT.to_string(),
                is_error: Some(true),
            })
            .collect(),
        compacted: false,
    })
}

/// Persist `messages[transcript_persisted_len .. +complete_prefix_len(tail)]`
/// (skipping `compacted` messages) and advance the counter on success. A
/// persist failure is logged and the counter is left alone so the next flush
/// retries; it never aborts the loop.
pub async fn flush_transcript(delegate: &dyn LoopDelegate, ctx: &mut ReasoningContext) {
    let start = ctx.transcript_persisted_len;
    let Some(tail) = ctx.messages.get(start..) else {
        tracing::warn!(
            persisted = start,
            len = ctx.messages.len(),
            "transcript flush: persisted watermark beyond history; skipping"
        );
        return;
    };
    let n = complete_prefix_len(tail);
    if n == 0 {
        return;
    }
    let to_write: Vec<ChatMessage> = tail[..n].iter().filter(|m| !m.compacted).cloned().collect();
    if !to_write.is_empty() {
        if let Err(e) = delegate.persist_transcript(&to_write).await {
            tracing::warn!(error = %e, count = to_write.len(), "transcript flush failed; will retry next flush");
            return;
        }
    }
    ctx.transcript_persisted_len = start + n;
}

/// End-of-run flush: append synthesized results for any dangling tool calls,
/// then `flush_transcript`.
pub async fn finalize_transcript(delegate: &dyn LoopDelegate, ctx: &mut ReasoningContext) {
    if let Some(results) = synthesize_missing_tool_results(&ctx.messages) {
        ctx.messages.push(results);
    }
    flush_transcript(delegate, ctx).await;
}

/// Agent-mode history for a new turn: imports a pre-upgrade session's legacy
/// `agent_messages` text rows once (in full — no head+tail window), then
/// returns every active transcript message and `persisted_len =
/// messages.len()` (`messages[i]` is the i-th active transcript row). The
/// caller appends this turn's user message (`append_current_user_turn`).
pub fn build_agent_history(
    conn: &Connection,
    session_id: &str,
) -> anyhow::Result<(Vec<ChatMessage>, usize)> {
    crate::agent::transcript::import_legacy_text_rows(conn, session_id)?;
    let messages: Vec<ChatMessage> = crate::agent::transcript::load_active(conn, session_id)?
        .into_iter()
        .map(|r| r.message)
        .collect();
    let persisted_len = messages.len();
    Ok((messages, persisted_len))
}

/// Build this turn's user message — a `/skill` prompt merged in front of the
/// user text exactly as `history_window::replay_history_messages` merges a
/// persisted `system` row — and append it to the transcript tagged
/// `turn_id` (the user row's `agent_messages.id`). Returns the message.
pub fn append_current_user_turn(
    conn: &Connection,
    session_id: &str,
    turn_id: &str,
    skill_prompt: Option<&str>,
    user_text: &str,
) -> anyhow::Result<ChatMessage> {
    let rows = skill_prompt
        .map(|p| ("system", p, ()))
        .into_iter()
        .chain(std::iter::once(("user", user_text, ())));
    let (msg, ()) = crate::agent::history_window::merge_skill_rows(rows)
        .pop()
        .ok_or_else(|| anyhow::anyhow!("merge produced no user message"))?;
    crate::agent::transcript::append_within(
        conn,
        session_id,
        Some(turn_id),
        crate::agent::transcript::TranscriptKind::Message,
        std::slice::from_ref(&msg),
    )?;
    Ok(msg)
}

/// Record the start of a transcript-mode turn inside the caller's
/// transaction `tx`: the `/skill` `system` row (created_at = `now - 1`, so
/// it sorts first), the `user` row with id `user_msg_id`, the session's
/// `message_count` bump, and the transcript append of the merged user
/// message (`append_current_user_turn`). Any error leaves it to the caller
/// to drop `tx` (rollback), so either all of it lands or none.
pub fn persist_current_user_turn(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    user_msg_id: &str,
    skill_prompt: Option<&str>,
    user_text: &str,
    now: i64,
) -> anyhow::Result<ChatMessage> {
    if let Some(skill) = skill_prompt {
        tx.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1,?2,'system',?3,?4)",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), session_id, skill, now - 1],
        )?;
    }
    tx.execute(
        "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1,?2,'user',?3,?4)",
        rusqlite::params![user_msg_id, session_id, user_text, now],
    )?;
    let bump = if skill_prompt.is_some() { 2 } else { 1 };
    tx.execute(
        "UPDATE agent_sessions SET message_count = message_count + ?2, updated_at = ?1 WHERE id = ?3",
        rusqlite::params![now, bump, session_id],
    )?;
    append_current_user_turn(tx, session_id, user_msg_id, skill_prompt, user_text)
}

/// The messages the current turn added after its user message:
/// `messages[current_turn_start + 1..]`. Indexed (not text-matched) so a
/// later message with the same text can't move it; `transcript_compact`
/// keeps the index valid across rebuilds. `None` when not tracked.
pub fn current_turn_slice(ctx: &ReasoningContext) -> Option<&[ChatMessage]> {
    ctx.current_turn_start.and_then(|i| ctx.messages.get(i + 1..))
}

/// Every message of the current turn for post-loop extraction (tool cards,
/// reasoning, partial text): messages a mid-turn compaction folded away
/// (`compacted_turn_messages`) followed by `current_turn_slice`. `None` when
/// the turn start is not tracked.
pub fn current_turn_messages(ctx: &ReasoningContext) -> Option<Vec<ChatMessage>> {
    let tail = current_turn_slice(ctx)?;
    let mut all = ctx.compacted_turn_messages.clone();
    all.extend_from_slice(tail);
    Some(all)
}

/// The bare `[turn ended: …]` status line for a turn that ended without a
/// final reply (the partial text is already in the transcript). `None` for
/// `Response` / `ToolResult` outcomes.
pub fn turn_ended_note(outcome: &LoopOutcome) -> Option<String> {
    match outcome {
        LoopOutcome::Response { .. } | LoopOutcome::ToolResult { .. } => None,
        LoopOutcome::Cancelled { .. } => {
            LoopOutcome::Cancelled { partial_code: None }.persisted_assistant_content("")
        }
        other => other.persisted_assistant_content(""),
    }
}

/// After a turn that did not end with an assistant message (cancel /
/// timeout / failure before any reply), append `note` as an assistant
/// message tagged `turn_id` so the transcript keeps user/assistant
/// alternation — mirrors the `[turn ended: …]` row in `agent_messages`.
/// Returns whether a row was appended.
pub fn close_turn_in_transcript(
    conn: &Connection,
    session_id: &str,
    turn_id: &str,
    note: &str,
) -> anyhow::Result<bool> {
    match crate::agent::transcript::last_active(conn, session_id)? {
        Some(r) if r.message.role != MessageRole::Assistant => {
            crate::agent::transcript::append(
                conn,
                session_id,
                Some(turn_id),
                crate::agent::transcript::TranscriptKind::Message,
                &[ChatMessage::assistant(note)],
            )?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::turn::TurnSnapshot;
    use crate::agent::types::{
        LoopOutcome, LoopSignal, RespondOutput, ResponseMetadata, TextAction, ToolCall,
    };
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingDelegate {
        written: Mutex<Vec<ChatMessage>>,
    }

    #[async_trait::async_trait]
    impl LoopDelegate for RecordingDelegate {
        async fn check_signals(&self) -> LoopSignal {
            LoopSignal::Continue
        }
        async fn before_llm_call(&self, _: &mut ReasoningContext, _: usize) -> Option<LoopOutcome> {
            None
        }
        async fn call_llm(
            &self,
            _: &mut ReasoningContext,
            _: &TurnSnapshot,
            _: usize,
        ) -> Result<RespondOutput, crate::error::Error> {
            Ok(RespondOutput::Text {
                text: String::new(),
                thinking: None,
                thinking_signature: None,
                metadata: ResponseMetadata { model: String::new(), finish_reason: None, usage: None },
            })
        }
        async fn handle_text_response(
            &self,
            _: &str,
            _: ResponseMetadata,
            _: &mut ReasoningContext,
        ) -> TextAction {
            TextAction::Return(LoopOutcome::Stopped)
        }
        async fn execute_tool_calls(
            &self,
            _: Vec<ToolCall>,
            _: &mut ReasoningContext,
        ) -> Result<Option<LoopOutcome>, crate::error::Error> {
            Ok(None)
        }
        async fn persist_transcript(&self, new_msgs: &[ChatMessage]) -> anyhow::Result<()> {
            self.written.lock().unwrap().extend_from_slice(new_msgs);
            Ok(())
        }
    }

    fn assistant_calls(ids: &[&str]) -> ChatMessage {
        ChatMessage::assistant_from_response(
            None,
            None,
            "",
            ids.iter().map(|id| (id.to_string(), "noop".to_string(), json!({}))),
        )
    }

    fn result_ids(m: &ChatMessage) -> Vec<(String, String, Option<bool>)> {
        m.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                    Some((tool_use_id.clone(), content.clone(), *is_error))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn complete_prefix_excludes_dangling_tool_use() {
        let mut msgs = vec![ChatMessage::user("hi"), assistant_calls(&["a"])];
        assert_eq!(complete_prefix_len(&msgs), 1);
        msgs.push(ChatMessage::user_tool_result("a", "ok", false));
        assert_eq!(complete_prefix_len(&msgs), 3);
    }

    #[test]
    fn complete_prefix_requires_all_parallel_results() {
        let msgs = vec![
            ChatMessage::user("hi"),
            assistant_calls(&["a", "b"]),
            ChatMessage::user_tool_result("a", "ok", false),
        ];
        assert_eq!(complete_prefix_len(&msgs), 1);
    }

    #[test]
    fn synthesize_is_none_when_all_paired() {
        let msgs = vec![
            ChatMessage::user("hi"),
            assistant_calls(&["a"]),
            ChatMessage::user_tool_result("a", "ok", false),
        ];
        assert!(synthesize_missing_tool_results(&msgs).is_none());
    }

    #[tokio::test]
    async fn finalize_synthesizes_results_for_dangling_tool_use() {
        let delegate = RecordingDelegate::default();
        let mut ctx = ReasoningContext::new("sys".into());
        ctx.messages = vec![ChatMessage::user("hi"), assistant_calls(&["a", "b"])];

        finalize_transcript(&delegate, &mut ctx).await;

        let written = delegate.written.lock().unwrap().clone();
        assert_eq!(written.len(), 3, "user + assistant + synthesized results");
        let last = written.last().unwrap();
        assert!(matches!(last.role, MessageRole::User));
        assert_eq!(
            result_ids(last),
            vec![
                ("a".to_string(), CANCELLED_TOOL_RESULT.to_string(), Some(true)),
                ("b".to_string(), CANCELLED_TOOL_RESULT.to_string(), Some(true)),
            ]
        );
        assert_eq!(ctx.transcript_persisted_len, ctx.messages.len());
    }

    #[tokio::test]
    async fn flush_holds_back_unpaired_tail_and_skips_compacted() {
        let delegate = RecordingDelegate::default();
        let mut ctx = ReasoningContext::new("sys".into());
        let mut old = ChatMessage::user("old");
        old.compacted = true;
        ctx.messages = vec![old, ChatMessage::user("hi"), assistant_calls(&["a"])];

        flush_transcript(&delegate, &mut ctx).await;
        assert_eq!(ctx.transcript_persisted_len, 2, "stops before the unpaired assistant");
        assert_eq!(delegate.written.lock().unwrap().len(), 1, "compacted message skipped");

        ctx.messages.push(ChatMessage::user_tool_result("a", "ok", false));
        flush_transcript(&delegate, &mut ctx).await;
        assert_eq!(ctx.transcript_persisted_len, 4);
        assert_eq!(delegate.written.lock().unwrap().len(), 3);
    }

    struct FailingDelegate(RecordingDelegate);

    #[async_trait::async_trait]
    impl LoopDelegate for FailingDelegate {
        async fn check_signals(&self) -> LoopSignal {
            self.0.check_signals().await
        }
        async fn before_llm_call(&self, c: &mut ReasoningContext, i: usize) -> Option<LoopOutcome> {
            self.0.before_llm_call(c, i).await
        }
        async fn call_llm(
            &self,
            c: &mut ReasoningContext,
            s: &TurnSnapshot,
            i: usize,
        ) -> Result<RespondOutput, crate::error::Error> {
            self.0.call_llm(c, s, i).await
        }
        async fn handle_text_response(
            &self,
            t: &str,
            m: ResponseMetadata,
            c: &mut ReasoningContext,
        ) -> TextAction {
            self.0.handle_text_response(t, m, c).await
        }
        async fn execute_tool_calls(
            &self,
            t: Vec<ToolCall>,
            c: &mut ReasoningContext,
        ) -> Result<Option<LoopOutcome>, crate::error::Error> {
            self.0.execute_tool_calls(t, c).await
        }
        async fn persist_transcript(&self, _: &[ChatMessage]) -> anyhow::Result<()> {
            anyhow::bail!("db locked")
        }
    }

    // ── build_agent_history / current-turn helpers (Task 7) ─────────────

    use crate::agent::compaction_policy::{is_session_block, SESSION_BLOCK_HEADER};
    use crate::agent::dispatcher::TranscriptSink;
    use crate::agent::transcript::{self, TranscriptKind};
    use rusqlite::Connection;
    use std::sync::Arc;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at) VALUES ('s','global','t',0,0)",
            [],
        )
        .ok();
        conn
    }

    fn legacy_row(conn: &Connection, id: &str, role: &str, content: &str, at: i64) {
        conn.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at, compacted) VALUES (?1,'s',?2,?3,?4,0)",
            rusqlite::params![id, role, content, at],
        )
        .unwrap();
    }

    fn text(m: &ChatMessage) -> String {
        m.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    fn json(msgs: &[ChatMessage]) -> String {
        serde_json::to_string(msgs).unwrap()
    }

    /// 60 legacy rows (30 turns), ~700 tokens each (≈42K total, over the old
    /// 32K window). Row text starts with "legacy {i} ".
    fn sixty_legacy_rows(conn: &Connection) {
        for i in 0..60 {
            let role = if i % 2 == 0 { "user" } else { "assistant" };
            legacy_row(conn, &format!("m{i}"), role, &format!("legacy {i} {}", "x".repeat(2800)), i as i64 + 1);
        }
    }

    #[test]
    fn new_session_history_is_empty() {
        let conn = db();
        let (msgs, persisted) = build_agent_history(&conn, "s").unwrap();
        assert!(msgs.is_empty());
        assert_eq!(persisted, 0);
        assert!(!transcript::has_transcript(&conn, "s").unwrap());
    }

    #[test]
    fn legacy_session_replays_all_text_then_transcript() {
        let conn = db();
        sixty_legacy_rows(&conn);
        let (msgs, persisted) = build_agent_history(&conn, "s").unwrap();
        assert_eq!(msgs.len(), 60);
        assert_eq!(persisted, 60);
        assert_eq!(transcript::load_active(&conn, "s").unwrap().len(), 60);
        assert!(msgs.iter().all(|m| !text(m).starts_with("[Earlier context omitted")));
        for (i, m) in msgs.iter().enumerate() {
            assert!(text(m).starts_with(&format!("legacy {i} ")), "row {i} out of order");
        }

        // A new turn, recorded the transcript-mode way.
        legacy_row(&conn, "u-new", "user", "new question", 1000);
        append_current_user_turn(&conn, "s", "u-new", None, "new question").unwrap();
        transcript::append(&conn, "s", Some("u-new"), TranscriptKind::Message, &[ChatMessage::assistant("new answer")])
            .unwrap();

        let (msgs, persisted) = build_agent_history(&conn, "s").unwrap();
        assert_eq!(msgs.len(), 62);
        assert_eq!(persisted, 62);
        assert!(text(&msgs[0]).starts_with("legacy 0 "));
        assert!(text(&msgs[59]).starts_with("legacy 59 "));
        assert_eq!(text(&msgs[60]), "new question");
        assert_eq!(text(&msgs[61]), "new answer");
    }

    #[test]
    fn legacy_import_runs_once() {
        let conn = db();
        sixty_legacy_rows(&conn);
        build_agent_history(&conn, "s").unwrap();
        // A later agent_messages row (e.g. this turn's user row) must not be
        // re-imported once the transcript exists.
        legacy_row(&conn, "late", "user", "late row", 2000);
        let (msgs, _) = build_agent_history(&conn, "s").unwrap();
        assert_eq!(msgs.len(), 60);
        assert_eq!(transcript::load_active(&conn, "s").unwrap().len(), 60);
    }

    /// DB-backed delegate: persists and compacts through the same sink
    /// helpers `ChatDelegate` uses; the fold "LLM" is a fixed fold.
    struct DbDelegate {
        sink: TranscriptSink,
    }

    impl DbDelegate {
        fn new(conn: Connection, turn_id: &str) -> Self {
            Self {
                sink: TranscriptSink {
                    db: Arc::new(std::sync::Mutex::new(conn)),
                    session_id: "s".into(),
                    turn_id: turn_id.into(),
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl LoopDelegate for DbDelegate {
        async fn check_signals(&self) -> LoopSignal {
            LoopSignal::Continue
        }
        async fn before_llm_call(&self, _: &mut ReasoningContext, _: usize) -> Option<LoopOutcome> {
            None
        }
        async fn call_llm(
            &self,
            _: &mut ReasoningContext,
            _: &TurnSnapshot,
            _: usize,
        ) -> Result<RespondOutput, crate::error::Error> {
            unreachable!()
        }
        async fn handle_text_response(
            &self,
            _: &str,
            _: ResponseMetadata,
            _: &mut ReasoningContext,
        ) -> TextAction {
            TextAction::Return(LoopOutcome::Stopped)
        }
        async fn execute_tool_calls(
            &self,
            _: Vec<ToolCall>,
            _: &mut ReasoningContext,
        ) -> Result<Option<LoopOutcome>, crate::error::Error> {
            Ok(None)
        }
        async fn summarize_to_fold(&self, _: &[ChatMessage]) -> Option<crate::agent::compact::StructuredFold> {
            Some(
                crate::agent::compact::StructuredFold::default()
                    .with_unresolved_questions(vec!["fixed fold".to_string()]),
            )
        }
        async fn persist_transcript(&self, new_msgs: &[ChatMessage]) -> anyhow::Result<()> {
            crate::agent::dispatcher::persist_to_sink(Some(&self.sink), new_msgs)
        }
        async fn replace_with_session_block(
            &self,
            block: &ChatMessage,
            superseded_through: usize,
            _threshold_tokens: usize,
            _trigger: crate::agent::types::CompactionTrigger,
        ) -> anyhow::Result<()> {
            crate::agent::dispatcher::replace_session_block_in_sink(&self.sink, block, superseded_through)
                .map(|_| ())
        }
    }

    /// Seed a ReasoningContext the way `send_agent_message` does.
    fn boot_ctx(conn: &Connection) -> ReasoningContext {
        let (msgs, persisted) = build_agent_history(conn, "s").unwrap();
        let mut ctx = ReasoningContext::new("sys".into());
        ctx.messages = msgs;
        ctx.transcript_persisted_len = persisted;
        ctx
    }

    #[tokio::test]
    async fn compaction_after_legacy_import_does_not_duplicate() {
        let conn = db();
        sixty_legacy_rows(&conn);
        let mut ctx = boot_ctx(&conn);
        let delegate = DbDelegate::new(conn, "m58");

        assert!(crate::agent::transcript_compaction::transcript_compact(&delegate, &mut ctx, 128_000, crate::agent::types::CompactionTrigger::Auto)
            .await
            .unwrap());

        let conn = delegate.sink.db.lock().unwrap();
        let (msgs, persisted) = build_agent_history(&conn, "s").unwrap();
        assert!(is_session_block(&msgs[0]));
        assert!(text(&msgs[0]).starts_with(SESSION_BLOCK_HEADER));
        assert!(!msgs.iter().any(|m| text(m).starts_with("legacy 0 ")), "compacted legacy text replayed");
        assert_eq!(msgs.iter().filter(|m| text(m).starts_with("legacy 59 ")).count(), 1);
        assert_eq!(persisted, msgs.len());
        assert_eq!(json(&msgs), json(&ctx.messages));
    }

    #[test]
    fn legacy_compact_fold_row_kept_in_order() {
        let conn = db();
        legacy_row(&conn, "u0", "user", "first", 1);
        legacy_row(&conn, "a0", "assistant", "reply", 2);
        legacy_row(&conn, "fold", "user", "## Earlier conversation (compacted)\n- stuff", 3);
        legacy_row(&conn, "u1", "user", "next", 4);
        legacy_row(&conn, "a1", "assistant", "reply2", 5);
        let (msgs, _) = build_agent_history(&conn, "s").unwrap();
        let texts: Vec<String> = msgs.iter().map(text).collect();
        assert_eq!(
            texts,
            vec!["first", "reply", "## Earlier conversation (compacted)\n- stuff", "next", "reply2"]
        );
    }

    #[tokio::test]
    async fn history_after_restart_matches_transcript() {
        let conn = db();
        sixty_legacy_rows(&conn);
        let mut ctx = boot_ctx(&conn);
        legacy_row(&conn, "u-cur", "user", "current", 5000);
        let cur = append_current_user_turn(&conn, "s", "u-cur", None, "current").unwrap();
        ctx.messages.push(cur);
        ctx.transcript_persisted_len = ctx.messages.len();
        ctx.messages.push(ChatMessage::assistant_with_tool_use("t1", "read", serde_json::json!({})));
        ctx.messages.push(ChatMessage::user_tool_result("t1", "ok", false));
        let delegate = DbDelegate::new(conn, "u-cur");

        assert!(crate::agent::transcript_compaction::transcript_compact(&delegate, &mut ctx, 128_000, crate::agent::types::CompactionTrigger::Auto)
            .await
            .unwrap());
        // An unpersisted (dangling) tail stays out of the transcript.
        ctx.messages.push(assistant_calls(&["t2"]));
        flush_transcript(&delegate, &mut ctx).await;

        let conn = delegate.sink.db.lock().unwrap();
        let (msgs, persisted) = build_agent_history(&conn, "s").unwrap();
        assert_eq!(persisted, ctx.transcript_persisted_len);
        assert_eq!(json(&msgs), json(&ctx.messages[..ctx.transcript_persisted_len]));
    }

    #[test]
    fn append_current_user_turn_merges_skill_prompt() {
        let conn = db();
        let skill = "<skill name=\"tdd\" version=\"1\">\nWrite the test first.\n</skill>";
        let m = append_current_user_turn(&conn, "s", "u1", Some(skill), "/tdd fix it").unwrap();
        assert_eq!(m.role, MessageRole::User);
        assert_eq!(text(&m), format!("{skill}\n\n/tdd fix it"));
        let rows = transcript::load_active(&conn, "s").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].turn_id.as_deref(), Some("u1"));
        assert_eq!(json(&[rows[0].message.clone()]), json(&[m]));

        let plain = append_current_user_turn(&conn, "s", "u2", None, "hello").unwrap();
        assert_eq!(text(&plain), "hello");
        assert_eq!(transcript::load_active(&conn, "s").unwrap()[1].turn_id.as_deref(), Some("u2"));
    }

    /// Ruling 12(2): the current turn is tracked by index, so a later
    /// steering/follow-up message with the SAME text doesn't move the slice,
    /// before or after a compaction rebuild.
    #[tokio::test]
    async fn current_turn_slice_tracks_index_across_compaction_with_duplicate_text() {
        let conn = db();
        let mut ctx = ReasoningContext::new("sys".into());
        for i in 0..5 {
            ctx.messages.push(ChatMessage::user(&"a".repeat(60_000)));
            ctx.messages.push(ChatMessage::assistant(&format!("old reply {i}")));
        }
        ctx.messages.push(ChatMessage::user("same text"));
        ctx.current_turn_start = Some(ctx.messages.len() - 1);
        ctx.messages.push(ChatMessage::assistant("first reply"));
        ctx.messages.push(ChatMessage::user("same text")); // steering, same text
        ctx.messages.push(ChatMessage::assistant("second reply"));

        let before = current_turn_slice(&ctx).unwrap();
        assert_eq!(before.len(), 3);
        assert_eq!(text(&before[0]), "first reply");

        let delegate = DbDelegate::new(conn, "u-cur");
        assert!(crate::agent::transcript_compaction::transcript_compact(&delegate, &mut ctx, 128_000, crate::agent::types::CompactionTrigger::Auto)
            .await
            .unwrap());
        assert!(is_session_block(&ctx.messages[0]));
        let after = current_turn_slice(&ctx).unwrap();
        assert_eq!(after.len(), 3);
        assert_eq!(text(&after[0]), "first reply");
        assert_eq!(text(&ctx.messages[ctx.current_turn_start.unwrap()]), "same text");
    }

    /// Tool-use / tool-result ids of `msgs`, in order ("u:id" / "r:id").
    fn tool_trace(msgs: &[ChatMessage]) -> Vec<String> {
        msgs.iter()
            .flat_map(|m| m.content.iter())
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, .. } => Some(format!("u:{id}")),
                ContentBlock::ToolResult { tool_use_id, .. } => Some(format!("r:{tool_use_id}")),
                _ => None,
            })
            .collect()
    }

    /// One tool step + an in-turn nudge (user TEXT, like TOOL_INTENT_NUDGE).
    fn tool_step(ctx: &mut ReasoningContext, id: &str, result_tokens: usize) {
        ctx.messages.push(ChatMessage::assistant_with_tool_use(id, "read", serde_json::json!({})));
        ctx.messages.push(ChatMessage::user_tool_result(id, &"r".repeat(result_tokens * 4), false));
        ctx.messages.push(ChatMessage::user(&format!("nudge after {id}")));
    }

    /// Ruling 13(a): a long single turn whose in-turn nudges make
    /// `choose_cut_index` cut INSIDE the turn.
    #[tokio::test]
    async fn mid_turn_cut_keeps_whole_turn_for_extraction() {
        let conn = db();
        let mut ctx = ReasoningContext::new("sys".into());
        ctx.messages.push(ChatMessage::user("old question"));
        ctx.messages.push(ChatMessage::assistant("old answer"));
        ctx.messages.push(ChatMessage::user("do the long task"));
        ctx.current_turn_start = Some(2);
        tool_step(&mut ctx, "t1", 100);
        tool_step(&mut ctx, "t2", 25_000); // >20K tokens of tool output
        tool_step(&mut ctx, "t3", 100);
        tool_step(&mut ctx, "t4", 100);
        ctx.messages.push(ChatMessage::assistant("done"));
        let delegate = DbDelegate::new(conn, "u-cur");

        assert!(crate::agent::transcript_compaction::transcript_compact(&delegate, &mut ctx, 128_000, crate::agent::types::CompactionTrigger::Auto)
            .await
            .unwrap());

        assert_eq!(ctx.current_turn_start, Some(0), "turn start folded into the block");
        let turn = current_turn_messages(&ctx).unwrap();
        assert_eq!(text(&turn[0]), "do the long task");
        assert_eq!(
            tool_trace(&turn),
            vec!["u:t1", "r:t1", "u:t2", "r:t2", "u:t3", "r:t3", "u:t4", "r:t4"]
        );
        assert_eq!(text(turn.last().unwrap()), "done");
        assert!(!turn.iter().any(|m| is_session_block(m)));
    }

    /// Ruling 13(b): two compactions within the same turn accumulate.
    #[tokio::test]
    async fn two_mid_turn_compactions_accumulate() {
        let conn = db();
        let mut ctx = ReasoningContext::new("sys".into());
        ctx.messages.push(ChatMessage::user("old question"));
        ctx.messages.push(ChatMessage::assistant("old answer"));
        ctx.messages.push(ChatMessage::user("do the long task"));
        ctx.current_turn_start = Some(2);
        tool_step(&mut ctx, "t1", 100);
        tool_step(&mut ctx, "t2", 25_000);
        tool_step(&mut ctx, "t3", 100);
        tool_step(&mut ctx, "t4", 100);
        let delegate = DbDelegate::new(conn, "u-cur");
        assert!(crate::agent::transcript_compaction::transcript_compact(&delegate, &mut ctx, 128_000, crate::agent::types::CompactionTrigger::Auto)
            .await
            .unwrap());

        tool_step(&mut ctx, "t5", 25_000);
        tool_step(&mut ctx, "t6", 100);
        tool_step(&mut ctx, "t7", 100);
        tool_step(&mut ctx, "t8", 100);
        ctx.messages.push(ChatMessage::assistant("done"));
        assert!(crate::agent::transcript_compaction::transcript_compact(&delegate, &mut ctx, 128_000, crate::agent::types::CompactionTrigger::Auto)
            .await
            .unwrap());

        assert_eq!(ctx.current_turn_start, Some(0));
        let turn = current_turn_messages(&ctx).unwrap();
        assert_eq!(text(&turn[0]), "do the long task");
        let expected: Vec<String> = (1..=8)
            .flat_map(|i| [format!("u:t{i}"), format!("r:t{i}")])
            .collect();
        assert_eq!(tool_trace(&turn), expected, "every tool call/result exactly once, in order");
        assert!(!turn.iter().any(|m| is_session_block(m)));
        assert_eq!(text(turn.last().unwrap()), "done");
    }

    #[test]
    fn current_turn_slice_none_without_index() {
        let mut ctx = ReasoningContext::new("sys".into());
        ctx.messages.push(ChatMessage::user("x"));
        assert!(current_turn_slice(&ctx).is_none());
    }

    /// Ruling 12(1): agent_messages rows and the transcript append commit
    /// together or not at all.
    #[test]
    fn persist_current_user_turn_rolls_back_on_transcript_failure() {
        let conn = db();
        conn.execute_batch(
            "CREATE TRIGGER boom BEFORE INSERT ON agent_transcript BEGIN SELECT RAISE(ABORT, 'boom'); END;",
        )
        .unwrap();
        {
            let tx = conn.unchecked_transaction().unwrap();
            let r = persist_current_user_turn(&tx, "s", "u1", Some("<skill>x</skill>"), "/x go", 100);
            assert!(r.is_err());
            // tx dropped without commit → rollback
        }
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_messages WHERE session_id = 's'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "user/system rows must roll back with the transcript append");
        let t: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_transcript WHERE session_id = 's'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(t, 0);
    }

    #[test]
    fn persist_current_user_turn_writes_rows_and_transcript() {
        let conn = db();
        let tx = conn.unchecked_transaction().unwrap();
        let m = persist_current_user_turn(&tx, "s", "u1", Some("<skill>x</skill>"), "/x go", 100).unwrap();
        tx.commit().unwrap();
        assert_eq!(text(&m), "<skill>x</skill>\n\n/x go");
        let rows: Vec<(String, String, i64)> = conn
            .prepare("SELECT role, content, created_at FROM agent_messages WHERE session_id = 's' ORDER BY created_at")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("system".to_string(), "<skill>x</skill>".to_string(), 99),
                ("user".to_string(), "/x go".to_string(), 100),
            ]
        );
        let user_id: String = conn
            .query_row("SELECT id FROM agent_messages WHERE role = 'user'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(user_id, "u1");
        let t = transcript::load_active(&conn, "s").unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].turn_id.as_deref(), Some("u1"));
    }

    /// Ruling 12(4): the transcript note is only the bare status line.
    #[test]
    fn turn_ended_note_is_bare_status_line() {
        assert_eq!(
            turn_ended_note(&LoopOutcome::Cancelled { partial_code: Some("fn x() {}".into()) }).as_deref(),
            Some("[turn ended: cancelled by user]")
        );
        assert_eq!(
            turn_ended_note(&LoopOutcome::Failure { error: "timed out after 5s".into() }).as_deref(),
            Some("[turn ended: error — timed out after 5s]")
        );
        assert_eq!(turn_ended_note(&LoopOutcome::Stopped).as_deref(), Some("[turn ended: stopped]"));
        assert!(turn_ended_note(&LoopOutcome::Response {
            text: "hi".into(),
            usage: None,
            truncated: false,
            model: None,
        })
        .is_none());
    }

    #[test]
    fn close_turn_appends_note_only_after_non_assistant() {
        let conn = db();
        transcript::append(&conn, "s", Some("u1"), TranscriptKind::Message, &[ChatMessage::user("q")]).unwrap();
        assert!(close_turn_in_transcript(&conn, "s", "u1", "[turn ended: cancelled]").unwrap());
        let rows = transcript::load_active(&conn, "s").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].message.role, MessageRole::Assistant);
        assert_eq!(rows[1].turn_id.as_deref(), Some("u1"));
        // Already closed by an assistant message → no-op.
        assert!(!close_turn_in_transcript(&conn, "s", "u1", "again").unwrap());
        assert_eq!(transcript::load_active(&conn, "s").unwrap().len(), 2);
    }

    #[tokio::test]
    async fn flush_failure_does_not_advance() {
        let delegate = FailingDelegate(RecordingDelegate::default());
        let mut ctx = ReasoningContext::new("sys".into());
        ctx.messages = vec![ChatMessage::user("hi")];
        flush_transcript(&delegate, &mut ctx).await;
        assert_eq!(ctx.transcript_persisted_len, 0);
    }
}
