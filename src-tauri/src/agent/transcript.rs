// SPDX-License-Identifier: Apache-2.0
//! Append-only per-session transcript of the exact messages sent to the model.
//!
//! Each row stores `serde_json::to_string(&ChatMessage)` so replay is
//! byte-identical to what was written. Compaction never rewrites rows: a
//! `session_block` summary row is appended and the rows it replaces are marked
//! `superseded` (and remember which block did so, so a rewind can undo it).
//!
//! Tool-result clearing (P2, `tool_result_clearing`) writes an overlay: the
//! cleared message goes to `cleared_json` / `cleared_est_tokens` and every
//! read (`load_active`, `last_active`) prefers it, while `message_json` keeps
//! the original. The overlay travels with the row when compaction re-appends
//! a kept tail and when a fork clones the transcript. A rewind does not undo
//! it: old results kept after the rewind stay placeholders (the model can
//! re-read the file).
//!
//! The per-turn dynamic block (P3, `<turn_context>…</turn_context>`) is a
//! second overlay: `dynamic_prefix` on the turn's first user row, written at
//! most once (`set_dynamic_prefix`) and merged in front of the message's first
//! Text block on every read. It travels with the row like the clearing overlay.

use rusqlite::{params, Connection, OptionalExtension};
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
    /// Message as replayed: clearing overlay applied and `dynamic_prefix` merged in.
    pub message: ChatMessage,
    /// Includes `estimate_tokens(dynamic_prefix)` when a prefix is merged
    /// (the `"\n\n"` separator is not counted).
    pub est_tokens: u32,
    /// The row's `<turn_context>` overlay (already merged into `message`).
    pub dynamic_prefix: Option<String>,
}

/// Put `prefix` in front of `msg`'s first Text block as `"{prefix}\n\n{text}"`.
/// With no Text block the message is returned untouched: inserting a Text
/// block could put text before `tool_result` blocks, which providers reject.
/// (`set_dynamic_prefix` never stores a prefix on such a row; this is defensive.)
pub fn merge_dynamic_prefix(prefix: &str, mut msg: ChatMessage) -> ChatMessage {
    merge_in_place(prefix, &mut msg);
    msg
}

/// `merge_dynamic_prefix` in place; returns whether the prefix was merged.
fn merge_in_place(prefix: &str, msg: &mut ChatMessage) -> bool {
    use uclaw_message_types::ContentBlock;
    match msg.content.iter_mut().find_map(|b| match b {
        ContentBlock::Text { text } => Some(text),
        _ => None,
    }) {
        Some(text) => {
            *text = format!("{prefix}\n\n{text}");
            true
        }
        None => false,
    }
}

/// A row may carry a dynamic prefix only if it is a plain prompt: at least
/// one Text block and no ToolResult block.
fn is_prompt_message(msg: &ChatMessage) -> bool {
    use uclaw_message_types::ContentBlock;
    msg.content.iter().any(|b| matches!(b, ContentBlock::Text { .. }))
        && !msg.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }))
}

/// Store `prefix` as the dynamic-prefix overlay of `turn_id`'s prompt row —
/// its first active user message row (lowest seq, `kind = message`,
/// `role = user`, not superseded) — only when that row has none yet.
/// `message_json` is never touched. Only that row is considered (never a
/// later steering / tool-result row). Returns whether a value was written;
/// false (nothing written) when there is no such row, it already has a
/// prefix, or it is not a plain prompt (has a ToolResult block or no Text
/// block — e.g. the prompt row was superseded and the first active user row
/// is a tool result; a prefix there would put text before `tool_result`
/// blocks, and the prefix is write-once).
pub fn set_dynamic_prefix(conn: &Connection, session_id: &str, turn_id: &str, prefix: &str) -> anyhow::Result<bool> {
    let rows: Vec<(i64, String, Option<String>)> = {
        let mut stmt = conn.prepare(
            "SELECT id, message_json, dynamic_prefix FROM agent_transcript
             WHERE session_id = ?1 AND turn_id = ?2 AND kind = 'message' AND superseded = 0
             ORDER BY seq",
        )?;
        let rows = stmt
            .query_map(params![session_id, turn_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (id, json, existing) in rows {
        let msg: ChatMessage = serde_json::from_str(&json)?;
        if msg.role != uclaw_message_types::MessageRole::User {
            continue;
        }
        if existing.is_some() || !is_prompt_message(&msg) {
            return Ok(false);
        }
        let n = conn.execute(
            "UPDATE agent_transcript SET dynamic_prefix = ?2 WHERE id = ?1 AND dynamic_prefix IS NULL",
            params![id, prefix],
        )?;
        return Ok(n > 0);
    }
    Ok(false)
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
    insert_row_json(conn, session_id, seq, turn_id, kind, msg, &serde_json::to_string(msg)?)
}

/// JSON key, next to the `ChatMessage` fields of a `session_block` row's
/// `message_json`, holding the `StructuredFold` the block was rendered from.
/// `ChatMessage` deserialisation ignores it, so replay is unaffected.
const BLOCK_FOLD_KEY: &str = "fold";

fn insert_row_json(
    conn: &Connection,
    session_id: &str,
    seq: i64,
    turn_id: Option<&str>,
    kind: TranscriptKind,
    msg: &ChatMessage,
    message_json: &str,
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
            message_json,
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
    let ids = append_within(&tx, session_id, turn_id, kind, msgs)?;
    tx.commit()?;
    Ok(ids)
}

/// `append` without its own transaction: for callers that already hold one
/// (SQLite transactions don't nest) and must commit it together with other
/// writes. Returns row ids.
pub fn append_within(
    conn: &Connection,
    session_id: &str,
    turn_id: Option<&str>,
    kind: TranscriptKind,
    msgs: &[ChatMessage],
) -> anyhow::Result<Vec<i64>> {
    let mut seq = next_seq(conn, session_id)?;
    let mut ids = Vec::with_capacity(msgs.len());
    for m in msgs {
        ids.push(insert_row(conn, session_id, seq, turn_id, kind, m)?);
        seq += 1;
    }
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
    replace_prefix_with_session_block_folded(conn, session_id, turn_id, block, None, superseded_through)
}

/// `replace_prefix_with_session_block` that also stores `fold` (the
/// `StructuredFold` `block` was rendered from) on the block row, so the next
/// compaction's incremental base is always the fold of the block that is
/// actually active (`leading_block_fold`) — a rewind that drops the block
/// drops its fold with it.
pub fn replace_prefix_with_session_block_folded(
    conn: &Connection,
    session_id: &str,
    turn_id: Option<&str>,
    block: &ChatMessage,
    fold: Option<&crate::agent::compact::StructuredFold>,
    superseded_through: usize,
) -> anyhow::Result<i64> {
    let mut block_json = serde_json::to_value(block)?;
    if let (Some(fold), Some(obj)) = (fold, block_json.as_object_mut()) {
        obj.insert(BLOCK_FOLD_KEY.to_string(), serde_json::to_value(fold)?);
    }
    let block_json = serde_json::to_string(&block_json)?;
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
    let block_id =
        insert_row_json(&tx, session_id, seq, turn_id, TranscriptKind::SessionBlock, block, &block_json)?;
    tx.execute(
        "UPDATE agent_transcript SET superseded = 1, superseded_by = ?3
         WHERE session_id = ?1 AND superseded = 0 AND seq <= ?2 AND id != ?3",
        params![session_id, last_seq, block_id],
    )?;
    for r in &rows[superseded_through..] {
        seq += 1;
        copy_row(&tx, r.id, session_id, seq, r.turn_id.as_deref())?;
    }
    tx.commit()?;
    Ok(block_id)
}

/// Copy row `id` (original `message_json` + clearing and dynamic-prefix
/// overlays, kind, est tokens) as a fresh row of `to_session` at `seq` tagged `turn_id`.
fn copy_row(conn: &Connection, id: i64, to_session: &str, seq: i64, turn_id: Option<&str>) -> anyhow::Result<i64> {
    conn.execute(
        "INSERT INTO agent_transcript
         (session_id, seq, turn_id, kind, message_json, est_tokens, created_at, cleared_json, cleared_est_tokens, dynamic_prefix)
         SELECT ?2, ?3, ?4, kind, message_json, est_tokens, ?5, cleared_json, cleared_est_tokens, dynamic_prefix
         FROM agent_transcript WHERE id = ?1",
        params![id, to_session, seq, turn_id, chrono::Utc::now().timestamp_millis()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Store cleared versions of active rows: `cleared` = (index into
/// `load_active`, cleared message). Writes only the overlay columns
/// (`cleared_json`, `cleared_est_tokens`); `message_json` is never rewritten.
/// All-or-nothing; an out-of-range index is an error. Rows carrying a
/// `dynamic_prefix` are skipped: `load_active` returns them with the prefix
/// merged, so clearing one would bake the prefix into `cleared_json` and
/// replay it twice (prefix rows are prompts, never tool results anyway).
pub fn set_cleared(conn: &Connection, session_id: &str, cleared: &[(usize, ChatMessage)]) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    let ids: Vec<(i64, bool)> = {
        let mut stmt = tx.prepare(
            "SELECT id, dynamic_prefix IS NOT NULL FROM agent_transcript
             WHERE session_id = ?1 AND superseded = 0 ORDER BY seq",
        )?;
        let ids = stmt.query_map([session_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        ids
    };
    for (index, msg) in cleared {
        let Some(&(id, has_prefix)) = ids.get(*index) else {
            anyhow::bail!("set_cleared: active index {index} out of range ({} active rows)", ids.len());
        };
        if has_prefix {
            continue;
        }
        tx.execute(
            "UPDATE agent_transcript SET cleared_json = ?2, cleared_est_tokens = ?3 WHERE id = ?1",
            params![id, serde_json::to_string(msg)?, estimate_message_tokens(msg) as i64],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Tool-call ids whose results were cleared in `session_id` (any row, active
/// or superseded — the UI marks the card either way), first occurrence
/// order, deduplicated (compaction re-appends carry the overlay).
pub fn cleared_tool_call_ids(conn: &Connection, session_id: &str) -> anyhow::Result<Vec<String>> {
    use uclaw_message_types::ContentBlock;
    let mut stmt = conn.prepare(
        "SELECT cleared_json FROM agent_transcript
         WHERE session_id = ?1 AND cleared_json IS NOT NULL ORDER BY seq, id",
    )?;
    let jsons = stmt.query_map([session_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    let mut out: Vec<String> = Vec::new();
    for json in jsons {
        let msg: ChatMessage = serde_json::from_str(&json)?;
        for b in msg.content {
            if let ContentBlock::ToolResult { tool_use_id, content, .. } = b {
                if content.starts_with(crate::agent::tool_result_clearing::CLEARED_PREFIX)
                    && !out.contains(&tool_use_id)
                {
                    out.push(tool_use_id);
                }
            }
        }
    }
    Ok(out)
}

type RawRow = (i64, i64, Option<String>, String, String, i64, Option<String>);

/// Replay columns: the clearing overlay wins over the original; the
/// dynamic prefix is merged in `parse_row`.
const ROW_COLUMNS: &str = "id, seq, turn_id, kind, COALESCE(cleared_json, message_json), \
     COALESCE(cleared_est_tokens, est_tokens), dynamic_prefix";

fn raw_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
}

fn parse_row((id, seq, turn_id, kind, json, est, dynamic_prefix): RawRow) -> anyhow::Result<TranscriptRow> {
    let mut message: ChatMessage = serde_json::from_str(&json)?;
    let mut est_tokens = est.max(0) as u32;
    if let Some(prefix) = &dynamic_prefix {
        if merge_in_place(prefix, &mut message) {
            est_tokens += uclaw_message_types::estimate_tokens(prefix);
        }
    }
    Ok(TranscriptRow { id, seq, turn_id, kind: TranscriptKind::parse(&kind), message, est_tokens, dynamic_prefix })
}

/// Non-superseded rows ordered by seq.
pub fn load_active(conn: &Connection, session_id: &str) -> anyhow::Result<Vec<TranscriptRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ROW_COLUMNS} FROM agent_transcript
         WHERE session_id = ?1 AND superseded = 0 ORDER BY seq"
    ))?;
    let raw = stmt.query_map([session_id], raw_row)?.collect::<Result<Vec<_>, _>>()?;
    raw.into_iter().map(parse_row).collect()
}

/// The fold stored on the session's leading active row when that row is a
/// `session_block` (see `replace_prefix_with_session_block_folded`); `None`
/// when the head is not a block or the block carries no (readable) fold.
/// This is the only valid incremental-compaction base: it always describes
/// exactly the block the model sees.
pub fn leading_block_fold(
    conn: &Connection,
    session_id: &str,
) -> anyhow::Result<Option<crate::agent::compact::StructuredFold>> {
    let head: Option<(String, String)> = conn
        .query_row(
            "SELECT kind, message_json FROM agent_transcript
             WHERE session_id = ?1 AND superseded = 0 ORDER BY seq LIMIT 1",
            [session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((kind, json)) = head else { return Ok(None) };
    if TranscriptKind::parse(&kind) != TranscriptKind::SessionBlock {
        return Ok(None);
    }
    let mut value: serde_json::Value = serde_json::from_str(&json)?;
    let Some(fold) = value.as_object_mut().and_then(|o| o.remove(BLOCK_FOLD_KEY)) else {
        return Ok(None);
    };
    match serde_json::from_value(fold) {
        Ok(fold) => Ok(Some(fold)),
        Err(e) => {
            tracing::warn!(session_id, error = %e, "session_block fold unreadable; compacting from scratch");
            Ok(None)
        }
    }
}

/// The active row with the highest seq (only that row is read).
pub fn last_active(conn: &Connection, session_id: &str) -> anyhow::Result<Option<TranscriptRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ROW_COLUMNS} FROM agent_transcript
         WHERE session_id = ?1 AND superseded = 0 ORDER BY seq DESC LIMIT 1"
    ))?;
    let raw = stmt.query_map([session_id], raw_row)?.next().transpose()?;
    raw.map(parse_row).transpose()
}

/// Session deletion: drop every transcript row (it holds full tool outputs
/// and file contents), the V52 fold baseline, the V64 file events and the
/// V67 session frame of `session_id`. No own
/// transaction — callers run it inside the one that deletes the session.
pub fn delete_session(conn: &Connection, session_id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM agent_transcript WHERE session_id = ?1", [session_id])?;
    conn.execute("DELETE FROM agent_fold_baselines WHERE session_id = ?1", [session_id])?;
    crate::agent::file_working_set::delete_session(conn, session_id)?;
    conn.execute("DELETE FROM agent_session_frames WHERE session_id = ?1", [session_id])?;
    Ok(())
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
    let tx = conn.unchecked_transaction()?;
    let n = truncate_after_turn_within(&tx, session_id, keep_through_turn_id)?;
    tx.commit()?;
    Ok(n)
}

/// `truncate_after_turn` without its own transaction, for callers that must
/// commit it together with other writes (e.g. the `agent_messages` rewind).
pub fn truncate_after_turn_within(
    conn: &Connection,
    session_id: &str,
    keep_through_turn_id: &str,
) -> anyhow::Result<usize> {
    let Some(keep_max) = turn_max_seq(conn, session_id, keep_through_turn_id)? else {
        return Ok(0);
    };
    let block_ids: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT id FROM agent_transcript
             WHERE session_id = ?1 AND seq > ?2 AND kind = 'session_block'",
        )?;
        let ids = stmt
            .query_map(params![session_id, keep_max], |r| r.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        ids
    };
    for id in &block_ids {
        conn.execute(
            "UPDATE agent_transcript SET superseded = 0, superseded_by = NULL
             WHERE session_id = ?1 AND superseded_by = ?2",
            params![session_id, id],
        )?;
    }
    Ok(conn.execute(
        "DELETE FROM agent_transcript WHERE session_id = ?1 AND seq > ?2",
        params![session_id, keep_max],
    )?)
}

/// Highest seq of any row (active or superseded) tagged `turn_id`.
fn turn_max_seq(conn: &Connection, session_id: &str, turn_id: &str) -> anyhow::Result<Option<i64>> {
    Ok(conn.query_row(
        "SELECT MAX(seq) FROM agent_transcript WHERE session_id = ?1 AND turn_id = ?2",
        params![session_id, turn_id],
        |r| r.get(0),
    )?)
}

/// Fork support (no own transaction): clone `from_session`'s transcript as it
/// would be after `truncate_after_turn(from_session, through_turn_id)` into
/// the empty `to_session` — every row through that turn's last row, superseded
/// ones included, keeping seq and supersession (`superseded_by` re-pointed at
/// the cloned blocks; rows superseded by a block beyond the cut are restored).
/// So a turn that was fully compacted comes back as its raw rows, and a later
/// rewind inside the fork can still undo a cloned block. Turn ids are mapped
/// through `turn_id_map` (unmapped ids are kept). Returns rows cloned
/// (0 and no change if the turn is unknown).
pub fn clone_through_turn_within(
    conn: &Connection,
    from_session: &str,
    to_session: &str,
    through_turn_id: &str,
    turn_id_map: &std::collections::HashMap<String, String>,
) -> anyhow::Result<usize> {
    let Some(through_max) = turn_max_seq(conn, from_session, through_turn_id)? else {
        return Ok(0);
    };
    type CloneRow = (i64, i64, Option<String>, i64, Option<i64>);
    let rows: Vec<CloneRow> = {
        let mut stmt = conn.prepare(
            "SELECT id, seq, turn_id, superseded, superseded_by
             FROM agent_transcript WHERE session_id = ?1 AND seq <= ?2 ORDER BY seq",
        )?;
        let rows = stmt
            .query_map(params![from_session, through_max], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let mut new_ids: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for (id, seq, turn_id, _, _) in &rows {
        let turn_id = turn_id.as_ref().map(|t| turn_id_map.get(t).unwrap_or(t));
        // Same columns incl. both overlays, keeping created_at.
        conn.execute(
            "INSERT INTO agent_transcript
             (session_id, seq, turn_id, kind, message_json, est_tokens, created_at, cleared_json, cleared_est_tokens, dynamic_prefix)
             SELECT ?2, ?3, ?4, kind, message_json, est_tokens, created_at, cleared_json, cleared_est_tokens, dynamic_prefix
             FROM agent_transcript WHERE id = ?1",
            params![id, to_session, seq, turn_id],
        )?;
        new_ids.insert(*id, conn.last_insert_rowid());
    }
    for (id, _, _, superseded, superseded_by) in &rows {
        if *superseded == 0 {
            continue;
        }
        // Superseded by a cloned block → point at the clone; by a block beyond
        // the cut → restored (stays active); plain supersede → stays superseded.
        let by = match superseded_by {
            Some(b) => match new_ids.get(b) {
                Some(nb) => Some(*nb),
                None => continue,
            },
            None => None,
        };
        conn.execute(
            "UPDATE agent_transcript SET superseded = 1, superseded_by = ?2 WHERE id = ?1",
            params![new_ids[id], by],
        )?;
    }
    Ok(rows.len())
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
        copy_row(&tx, r.id, to_session, seq, r.turn_id.as_deref())?;
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
    fn last_active_returns_highest_seq_active_row() {
        let conn = db();
        assert!(last_active(&conn, "s").unwrap().is_none());
        let u = |t: &str| ChatMessage::user(t);
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[u("a"), u("b")]).unwrap();
        append(&conn, "other", Some("t9"), TranscriptKind::Message, &[u("zzz")]).unwrap();
        let last = last_active(&conn, "s").unwrap().unwrap();
        assert_eq!(last.seq, 2);
        assert_eq!(texts(&[last]), vec!["b"]);
        replace_prefix_with_session_block(&conn, "s", Some("t2"), &u("block"), 2).unwrap();
        let last = last_active(&conn, "s").unwrap().unwrap();
        assert_eq!(last.kind, TranscriptKind::SessionBlock);
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

    fn fold_of(q: &str) -> crate::agent::compact::StructuredFold {
        crate::agent::compact::StructuredFold::default().with_unresolved_questions(vec![q.to_string()])
    }

    /// The fold a block was rendered from rides on its row (not in the
    /// replayed message) and follows the block through nested compactions
    /// and rewinds.
    #[test]
    fn leading_block_fold_tracks_the_active_leading_block() {
        let conn = db();
        let u = |t: &str| ChatMessage::user(t);
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[u("t1")]).unwrap();
        append(&conn, "s", Some("t2"), TranscriptKind::Message, &[u("t2")]).unwrap();
        assert_eq!(leading_block_fold(&conn, "s").unwrap(), None, "no block");
        replace_prefix_with_session_block_folded(&conn, "s", Some("t3"), &u("b1"), Some(&fold_of("f1")), 1)
            .unwrap();
        append(&conn, "s", Some("t3"), TranscriptKind::Message, &[u("t3")]).unwrap();
        assert_eq!(leading_block_fold(&conn, "s").unwrap(), Some(fold_of("f1")));
        // Replay is unaffected by the fold riding on the row.
        assert_eq!(texts(&load_active(&conn, "s").unwrap()), vec!["b1", "t2", "t3"]);
        replace_prefix_with_session_block_folded(&conn, "s", Some("t4"), &u("b2"), Some(&fold_of("f2")), 2)
            .unwrap();
        assert_eq!(leading_block_fold(&conn, "s").unwrap(), Some(fold_of("f2")));

        truncate_after_turn(&conn, "s", "t2").unwrap(); // drops b2, restores b1
        assert_eq!(leading_block_fold(&conn, "s").unwrap(), Some(fold_of("f1")));
        truncate_after_turn(&conn, "s", "t1").unwrap(); // drops b1 too
        assert_eq!(leading_block_fold(&conn, "s").unwrap(), None);
    }

    #[test]
    fn leading_block_fold_none_for_block_written_without_fold() {
        let conn = db();
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[ChatMessage::user("t1")]).unwrap();
        replace_prefix_with_session_block(&conn, "s", Some("t2"), &ChatMessage::user("b"), 1).unwrap();
        assert_eq!(leading_block_fold(&conn, "s").unwrap(), None);
    }

    #[test]
    fn delete_session_removes_transcript_and_baseline_of_that_session_only() {
        let conn = db();
        for s in ["s", "other"] {
            append(&conn, s, Some("t1"), TranscriptKind::Message, &[ChatMessage::user("x")]).unwrap();
            crate::agent::compact::upsert_baseline(&conn, s, &fold_of("f")).unwrap();
        }
        delete_session(&conn, "s").unwrap();
        assert!(!has_transcript(&conn, "s").unwrap());
        assert!(crate::agent::compact::load_baseline(&conn, "s").is_none());
        assert!(has_transcript(&conn, "other").unwrap());
        assert!(crate::agent::compact::load_baseline(&conn, "other").is_some());
    }

    // ── Tool-result clearing overlay (P2 Task 8) ─────────────────────────

    fn cleared_result(id: &str) -> ChatMessage {
        ChatMessage::user_tool_result(id, "[已清理的工具结果：read_file a.rs 全文（约 1.0K token）——如需内容请重新读取]", false)
    }

    fn raw_message_json(conn: &Connection, session: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT message_json FROM agent_transcript WHERE session_id = ?1 ORDER BY seq")
            .unwrap();
        let rows = stmt.query_map([session], |r| r.get(0)).unwrap().collect::<Result<Vec<String>, _>>().unwrap();
        rows
    }

    /// user, call r1, result r1 (big), call r2, result r2.
    fn tool_rows(conn: &Connection, session: &str) -> Vec<ChatMessage> {
        let msgs = vec![
            ChatMessage::user("q"),
            ChatMessage::assistant_with_tool_use("r1", "read_file", json!({"path": "a.rs"})),
            ChatMessage::user_tool_result("r1", &"x".repeat(4_000), false),
            ChatMessage::assistant_with_tool_use("r2", "read_file", json!({"path": "b.rs"})),
            ChatMessage::user_tool_result("r2", "short", false),
        ];
        append(conn, session, Some("t1"), TranscriptKind::Message, &msgs).unwrap();
        msgs
    }

    #[test]
    fn set_cleared_overlays_without_touching_message_json() {
        let conn = db();
        tool_rows(&conn, "s");
        let before = raw_message_json(&conn, "s");

        set_cleared(&conn, "s", &[(2, cleared_result("r1"))]).unwrap();

        // Replay reads the overlay (and its token estimate) ...
        let rows = load_active(&conn, "s").unwrap();
        assert_eq!(
            serde_json::to_string(&rows[2].message).unwrap(),
            serde_json::to_string(&cleared_result("r1")).unwrap()
        );
        assert_eq!(rows[2].est_tokens, estimate_message_tokens(&cleared_result("r1")));
        assert_eq!(texts(&rows[..1]), vec!["q"]);
        // ... while message_json is never rewritten.
        assert_eq!(raw_message_json(&conn, "s"), before);
        assert_eq!(cleared_tool_call_ids(&conn, "s").unwrap(), vec!["r1"]);

        // A rewind (dropping a later turn) does not undo the overlay.
        append(&conn, "s", Some("t9"), TranscriptKind::Message, &[ChatMessage::user("later")]).unwrap();
        assert_eq!(truncate_after_turn(&conn, "s", "t1").unwrap(), 1);
        let rows = load_active(&conn, "s").unwrap();
        assert_eq!(rows.len(), 5);
        assert!(serde_json::to_string(&rows[2].message).unwrap().contains("已清理的工具结果"));

        // Compaction re-appends the kept tail with the overlay carried over
        // and the original message_json.
        replace_prefix_with_session_block(&conn, "s", Some("t2"), &ChatMessage::user("block"), 1).unwrap();
        let rows = load_active(&conn, "s").unwrap();
        assert_eq!(rows.len(), 5);
        assert!(serde_json::to_string(&rows[2].message).unwrap().contains("已清理的工具结果"));
        let raw = raw_message_json(&conn, "s");
        assert_eq!(raw[raw.len() - 3], before[2], "re-appended row keeps the original message_json");
        assert_eq!(cleared_tool_call_ids(&conn, "s").unwrap(), vec!["r1"], "deduplicated");

        // Out-of-range active index → error, nothing written.
        assert!(set_cleared(&conn, "s", &[(99, cleared_result("r2"))]).is_err());
    }

    #[test]
    fn fork_copies_cleared_overlay() {
        let conn = db();
        tool_rows(&conn, "src");
        set_cleared(&conn, "src", &[(2, cleared_result("r1"))]).unwrap();

        let n = clone_through_turn_within(&conn, "src", "dst", "t1", &std::collections::HashMap::new()).unwrap();
        assert_eq!(n, 5);
        let src: Vec<String> =
            load_active(&conn, "src").unwrap().iter().map(|r| serde_json::to_string(&r.message).unwrap()).collect();
        let dst: Vec<String> =
            load_active(&conn, "dst").unwrap().iter().map(|r| serde_json::to_string(&r.message).unwrap()).collect();
        assert_eq!(dst, src, "the fork replays the cleared version");
        assert_eq!(raw_message_json(&conn, "dst"), raw_message_json(&conn, "src"));
        assert_eq!(cleared_tool_call_ids(&conn, "dst").unwrap(), vec!["r1"]);
    }

    // ── Dynamic-prefix overlay (P3 Task 1, V67) ──────────────────────────

    const PREFIX: &str = "<turn_context>x</turn_context>";

    fn count(conn: &Connection, sql: &str, arg: &str) -> i64 {
        conn.query_row(sql, [arg], |r| r.get(0)).unwrap()
    }

    fn raw_prefixes(conn: &Connection, session: &str) -> Vec<Option<String>> {
        let mut stmt = conn
            .prepare("SELECT dynamic_prefix FROM agent_transcript WHERE session_id = ?1 ORDER BY seq")
            .unwrap();
        let rows = stmt.query_map([session], |r| r.get(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        rows
    }

    #[test]
    fn v67_adds_dynamic_prefix_and_frames_table_idempotently() {
        let conn = db();
        crate::db::migrations::run(&conn).expect("idempotent");
        crate::db::migrations::run(&conn).expect("idempotent twice");
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM pragma_table_info('agent_transcript') WHERE name = ?1", "dynamic_prefix"),
            1
        );
        for col in [
            "session_id", "frame_key_json", "system_prompt", "mode", "rules_hash", "tool_names_json",
            "manifest_json", "rebuild_reason", "rebuilt_turn_id", "created_at", "updated_at",
        ] {
            assert_eq!(
                count(&conn, "SELECT COUNT(*) FROM pragma_table_info('agent_session_frames') WHERE name = ?1", col),
                1,
                "missing column {col}"
            );
        }
    }

    #[test]
    fn set_dynamic_prefix_is_set_once() {
        let conn = db();
        append(
            &conn,
            "s",
            Some("t1"),
            TranscriptKind::Message,
            &[ChatMessage::user("hi"), ChatMessage::assistant_with_tool_use("c1", "read_file", json!({}))],
        )
        .unwrap();
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[ChatMessage::user_tool_result("c1", "r", false)])
            .unwrap();
        assert!(set_dynamic_prefix(&conn, "s", "t1", PREFIX).unwrap());
        assert!(!set_dynamic_prefix(&conn, "s", "t1", "<turn_context>y</turn_context>").unwrap());
        assert_eq!(raw_prefixes(&conn, "s"), vec![Some(PREFIX.to_string()), None, None]);
        assert!(!set_dynamic_prefix(&conn, "s", "unknown-turn", PREFIX).unwrap());
    }

    #[test]
    fn dynamic_prefix_is_merged_on_load_active_and_last_active() {
        let conn = db();
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[ChatMessage::user("hi")]).unwrap();
        let before = load_active(&conn, "s").unwrap()[0].est_tokens;
        set_dynamic_prefix(&conn, "s", "t1", PREFIX).unwrap();
        let rows = load_active(&conn, "s").unwrap();
        assert_eq!(texts(&rows), vec![format!("{PREFIX}\n\nhi")]);
        assert_eq!(rows[0].dynamic_prefix.as_deref(), Some(PREFIX));
        assert_eq!(rows[0].est_tokens, before + uclaw_message_types::estimate_tokens(PREFIX));
        let last = last_active(&conn, "s").unwrap().unwrap();
        assert_eq!(texts(&[last.clone()]), vec![format!("{PREFIX}\n\nhi")]);
        assert_eq!(last.est_tokens, rows[0].est_tokens);
    }

    #[test]
    fn merge_dynamic_prefix_without_text_block_leaves_message_untouched() {
        let original = ChatMessage::user_tool_result("c1", "r", false);
        let msg = merge_dynamic_prefix(PREFIX, original.clone());
        assert_eq!(serde_json::to_string(&msg).unwrap(), serde_json::to_string(&original).unwrap());
    }

    #[test]
    fn set_dynamic_prefix_refuses_when_prompt_row_superseded() {
        let conn = db();
        append(
            &conn,
            "s",
            Some("t1"),
            TranscriptKind::Message,
            &[
                ChatMessage::user("prompt"),
                ChatMessage::assistant_with_tool_use("c1", "read_file", json!({})),
                ChatMessage::user_tool_result("c1", "r", false),
            ],
        )
        .unwrap();
        supersede_up_to(&conn, "s", 1).unwrap();
        assert!(!set_dynamic_prefix(&conn, "s", "t1", PREFIX).unwrap());
        assert_eq!(raw_prefixes(&conn, "s"), vec![None, None, None]);
    }

    #[test]
    fn set_dynamic_prefix_refuses_prompt_row_with_tool_result() {
        let conn = db();
        let mixed = ChatMessage {
            role: MessageRole::User,
            content: vec![
                ContentBlock::ToolResult { tool_use_id: "c0".into(), content: "r".into(), is_error: Some(false) },
                ContentBlock::Text { text: "and also".into() },
            ],
            compacted: false,
        };
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[mixed]).unwrap();
        assert!(!set_dynamic_prefix(&conn, "s", "t1", PREFIX).unwrap());
        append(&conn, "s", Some("t2"), TranscriptKind::Message, &[ChatMessage::user_tool_result("c1", "r", false)])
            .unwrap();
        assert!(!set_dynamic_prefix(&conn, "s", "t2", PREFIX).unwrap(), "no Text block");
        assert_eq!(raw_prefixes(&conn, "s"), vec![None, None]);
    }

    #[test]
    fn steering_row_later_in_turn_never_gets_prefix() {
        let conn = db();
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[ChatMessage::user("prompt")]).unwrap();
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[ChatMessage::assistant("working")]).unwrap();
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[ChatMessage::user("steer")]).unwrap();
        assert!(set_dynamic_prefix(&conn, "s", "t1", PREFIX).unwrap());
        assert!(!set_dynamic_prefix(&conn, "s", "t1", PREFIX).unwrap());
        assert_eq!(raw_prefixes(&conn, "s"), vec![Some(PREFIX.to_string()), None, None]);
        assert_eq!(texts(&load_active(&conn, "s").unwrap())[2], "steer");
    }

    #[test]
    fn set_cleared_skips_rows_with_dynamic_prefix() {
        let conn = db();
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[ChatMessage::user("hi")]).unwrap();
        set_dynamic_prefix(&conn, "s", "t1", PREFIX).unwrap();
        let merged = load_active(&conn, "s").unwrap()[0].message.clone();
        set_cleared(&conn, "s", &[(0, merged)]).unwrap();
        let cleared: Option<String> =
            conn.query_row("SELECT cleared_json FROM agent_transcript WHERE session_id = 's'", [], |r| r.get(0)).unwrap();
        assert_eq!(cleared, None);
        assert_eq!(texts(&load_active(&conn, "s").unwrap()), vec![format!("{PREFIX}\n\nhi")]);
    }

    #[test]
    fn message_json_is_never_rewritten_by_dynamic_prefix() {
        let conn = db();
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[ChatMessage::user("hi")]).unwrap();
        let before = raw_message_json(&conn, "s");
        set_dynamic_prefix(&conn, "s", "t1", PREFIX).unwrap();
        assert_eq!(raw_message_json(&conn, "s"), before);
        assert_eq!(before[0], serde_json::to_string(&ChatMessage::user("hi")).unwrap());
    }

    #[test]
    fn dynamic_prefix_survives_compaction_reappend_and_fork_clone() {
        let conn = db();
        append(&conn, "s", Some("t1"), TranscriptKind::Message, &[ChatMessage::user("u1")]).unwrap();
        append(&conn, "s", Some("t2"), TranscriptKind::Message, &[ChatMessage::user("u2")]).unwrap();
        set_dynamic_prefix(&conn, "s", "t2", PREFIX).unwrap();

        // Fork clone carries the overlay.
        clone_through_turn_within(&conn, "s", "fork", "t2", &std::collections::HashMap::new()).unwrap();
        assert_eq!(texts(&load_active(&conn, "fork").unwrap()), vec!["u1".to_string(), format!("{PREFIX}\n\nu2")]);

        // Compaction re-appends the kept tail with the overlay.
        replace_prefix_with_session_block(&conn, "s", Some("t3"), &ChatMessage::user("block"), 1).unwrap();
        let rows = load_active(&conn, "s").unwrap();
        assert_eq!(texts(&rows), vec!["block".to_string(), format!("{PREFIX}\n\nu2")]);
        assert_eq!(rows[1].dynamic_prefix.as_deref(), Some(PREFIX));

        // copy_through_turn (copy_row) carries it too.
        copy_through_turn(&conn, "s", "copy", "t2").unwrap(); // tail row is the last t2 row
        assert_eq!(load_active(&conn, "copy").unwrap()[1].dynamic_prefix.as_deref(), Some(PREFIX));
    }

    #[test]
    fn rows_without_prefix_replay_unchanged() {
        let conn = db();
        let msgs = tool_rows(&conn, "s");
        let rows = load_active(&conn, "s").unwrap();
        for (r, m) in rows.iter().zip(&msgs) {
            assert_eq!(serde_json::to_string(&r.message).unwrap(), serde_json::to_string(m).unwrap());
            assert_eq!(r.est_tokens, estimate_message_tokens(m));
            assert_eq!(r.dynamic_prefix, None);
        }
    }

    #[test]
    fn delete_session_removes_frame_row() {
        let conn = db();
        for s in ["s", "other"] {
            conn.execute(
                "INSERT INTO agent_session_frames
                 (session_id, frame_key_json, system_prompt, mode, rules_hash, rebuild_reason, created_at, updated_at)
                 VALUES (?1, '{}', 'p', 'normal', 'h', 'new', 0, 0)",
                [s],
            )
            .unwrap();
        }
        delete_session(&conn, "s").unwrap();
        let q = "SELECT COUNT(*) FROM agent_session_frames WHERE session_id = ?1";
        assert_eq!(count(&conn, q, "s"), 0);
        assert_eq!(count(&conn, q, "other"), 1);
    }
}
