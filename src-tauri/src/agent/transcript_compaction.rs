// SPDX-License-Identifier: Apache-2.0
//! Context continuity P1 — transcript-mode compaction: summarise the old
//! prefix into a `session_block`, rebuild the context around it (keeping the
//! 1:1 transcript invariant), and the pair-atomic hard-truncation fallback.
//! Called from `agentic_loop::apply_transcript_compaction_decision`.

use crate::agent::types::{ChatMessage, CompactionTrigger, ContentBlock, LoopDelegate, ReasoningContext};

/// Transcript-mode compaction (context continuity P1/P2).
/// ① flush so every message before the cut is in the transcript; ② pick the
/// cut (`choose_cut_index`); ③ summarise `messages[..cut]`: with a turn
/// `snapshot`, first the prefix-reusing summary (same request prefix as the
/// main call → prompt-cache hit); otherwise / on failure the standalone
/// summarizer (incremental when a prior fold exists), then the extractive
/// fallback — all aiming at `settings.summary_target_tokens`; ④ the rendered fold
/// becomes a `session_block` user message; ⑤ the delegate supersedes the
/// transcript rows `[..cut]` and appends the block; ⑥ the context is rebuilt
/// as `[block] + messages[cut..]` keeping the 1:1 transcript invariant.
/// `Ok(false)` = nothing was compacted (context untouched).
pub(crate) async fn transcript_compact(
    delegate: &dyn LoopDelegate,
    reason_ctx: &mut ReasoningContext,
    snapshot: Option<&crate::agent::turn::TurnSnapshot>,
    threshold: usize,
    trigger: CompactionTrigger,
    settings: &crate::memubot_config::AgentContextSettings,
) -> anyhow::Result<bool> {
    use crate::agent::compaction_policy::{choose_cut_index, is_session_block};

    let started = std::time::Instant::now();
    crate::agent::transcript_history::flush_transcript(delegate, reason_ctx).await;
    let Some(cut) = choose_cut_index(&reason_ctx.messages) else {
        return Ok(false);
    };
    let persisted = reason_ctx.transcript_persisted_len;
    if persisted < cut {
        tracing::warn!(
            cut,
            persisted,
            "transcript compaction skipped: history before the cut is not fully persisted"
        );
        return Ok(false);
    }

    // With a prior fold the incremental update already carries the old
    // block's content, so a leading session_block isn't re-summarised. A
    // prior fold only ever describes the leading block: without one (e.g. a
    // rewind dropped it) it is stale — it may cover rewound turns — and the
    // whole slice is summarised from scratch.
    let prior = reason_ctx
        .compaction_state
        .previous_fold
        .clone()
        .filter(|_| is_session_block(&reason_ctx.messages[0]));
    let start = usize::from(prior.is_some());
    let slice: Vec<ChatMessage> = reason_ctx.messages[start..cut].to_vec();
    let target = settings.summary_target_tokens as usize;
    let prefix_fold = match snapshot {
        Some(snap) => delegate.summarize_with_prefix(reason_ctx, snap, cut, target).await,
        None => None,
    };
    let used_prefix_summary = prefix_fold.is_some();
    let fold = match prefix_fold {
        Some(fold) => Some(fold),
        None => match &prior {
            Some(prior) => delegate.update_fold_incremental(prior, &slice, target).await,
            None => delegate.summarize_to_fold(&slice, target).await,
        },
    }
    .unwrap_or_else(|| {
        tracing::info!(removed = slice.len(), "fold summarisation unavailable; using extractive fallback fold");
        crate::agent::compact::summarize::extractive_fallback_fold(&slice)
    });
    // Session block (P2): summary + persisted file working set + current plan
    // + the most recently modified files' contents, within
    // min(25K, threshold / 4). The in-memory `reason_ctx.file_ops` (one turn
    // only) is no longer rendered here; it stays for the legacy path.
    let fold_md = fold.to_markdown();
    let context = delegate.session_block_context();
    let (block_text, reloaded_files) = crate::agent::session_block::assemble(
        &fold_md,
        context.as_ref(),
        settings.reload_recent_files as usize,
        crate::agent::session_block::session_block_budget(threshold),
    );
    let block = ChatMessage::user(&block_text);

    delegate.replace_with_session_block(&block, &fold, cut, threshold, trigger).await?;

    // Rebuild: [block] + messages[cut..]. The block and the already-persisted
    // part of the tail are exactly the active transcript rows (1:1); messages
    // not yet persisted stay unpersisted for the next flush.
    // Mid-turn compaction (in-turn nudges / steering are user text, so the
    // cut can land inside the current turn): stash the turn's messages being
    // folded away (its user message included; never a leading session_block)
    // for post-loop extraction.
    if let Some(i) = reason_ctx.current_turn_start.filter(|&i| i < cut) {
        let from = if i == 0 && is_session_block(&reason_ctx.messages[0]) { 1 } else { i };
        let stashed = reason_ctx.messages[from..cut].to_vec();
        reason_ctx.compacted_turn_messages.extend(stashed);
    }
    let mut rebuilt = Vec::with_capacity(reason_ctx.messages.len() - cut + 1);
    rebuilt.push(block);
    rebuilt.extend(reason_ctx.messages.drain(cut..));
    reason_ctx.messages = rebuilt;
    reason_ctx.transcript_persisted_len = 1 + (persisted - cut);
    // Current turn start: shift with the kept tail, or — when the cut fell
    // inside the turn (its messages were stashed above) — point at the block
    // so `current_turn_slice` yields the kept tail after it.
    if let Some(i) = reason_ctx.current_turn_start {
        reason_ctx.current_turn_start = Some(if i >= cut { i - cut + 1 } else { 0 });
    }
    reason_ctx.compaction_state.previous_fold = Some(fold);
    reason_ctx.compaction_state.compactions_done += 1;
    tracing::info!(
        compacted = cut,
        remaining = reason_ctx.messages.len(),
        estimated_request_tokens = reason_ctx.estimate_request_tokens(),
        "Transcript compacted into session_block"
    );
    tracing::info!(
        "transcript_compact trigger={} cut={} summary_tokens={} block_tokens={} reloaded_files={} ms={} prefix_summary={}",
        trigger.as_str(),
        cut,
        uclaw_message_types::estimate_tokens(&fold_md),
        uclaw_message_types::estimate_tokens(&block_text),
        reloaded_files,
        started.elapsed().as_millis(),
        used_prefix_summary,
    );
    Ok(true)
}

/// Transcript-mode last line of defence (only when the request can't fit the
/// window): logically drop (`compacted`) the oldest messages until
/// `estimate_token_count() <= threshold - overhead`. Only already-persisted
/// messages are dropped (they stay in the transcript, so the 1:1 index
/// invariant holds), whole tool call/result groups at a time, never the
/// leading `session_block`, the last user text message, or anything after it
/// (the current turn's tool progress). If that is not enough, warn and send.
pub(crate) async fn transcript_hard_truncate(
    delegate: &dyn LoopDelegate,
    reason_ctx: &mut ReasoningContext,
    threshold: usize,
) {
    use crate::agent::compaction_policy::{is_session_block, is_user_text};

    crate::agent::transcript_history::flush_transcript(delegate, reason_ctx).await;
    let target = threshold.saturating_sub(reason_ctx.request_overhead_tokens);
    // Candidates: persisted messages strictly before the last user text message.
    let last_user = reason_ctx.messages.iter().rposition(is_user_text).unwrap_or(0);
    let limit = reason_ctx.transcript_persisted_len.min(last_user);
    let block_idx = reason_ctx.messages.first().filter(|m| is_session_block(m)).map(|_| 0);
    let protected = |i: usize| Some(i) == block_idx;

    let mut removed = 0usize;
    let mut i = 0usize;
    while i < limit && reason_ctx.estimate_token_count() > target {
        if reason_ctx.messages[i].compacted || protected(i) {
            i += 1;
            continue;
        }
        let Some(end) = tool_group_end(&reason_ctx.messages[..limit], i) else {
            i += 1;
            continue;
        };
        if (i..end).any(protected) {
            i = end;
            continue;
        }
        for m in &mut reason_ctx.messages[i..end] {
            if !m.compacted {
                m.compacted = true;
                removed += 1;
            }
        }
        i = end;
    }
    let estimated_tokens = reason_ctx.estimate_token_count();
    tracing::warn!(
        removed,
        target,
        estimated_tokens,
        "Transcript-mode context hard-truncated (logical marking, persisted messages only)"
    );
    if estimated_tokens > target {
        tracing::warn!(
            estimated_tokens,
            target,
            "context still over target after hard truncation (protected session_block / current turn); sending anyway"
        );
    }
}

/// Outcome of a manual `/compact`, in active-transcript messages.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ManualCompactResult {
    /// A new `session_block` was written.
    pub compacted: bool,
    /// Active messages before.
    pub before: usize,
    /// Messages folded into the block (a previous leading block excluded).
    pub removed: usize,
    /// Active messages after.
    pub remaining: usize,
}

impl ManualCompactResult {
    /// The `/compact` completion text; a no-op never claims a compaction.
    pub fn report_text(&self) -> String {
        if self.compacted {
            format!("Compacted: folded {} earlier messages, {} remain.", self.removed, self.remaining)
        } else {
            format!("Nothing to compact: {} messages kept.", self.remaining)
        }
    }
}

/// Manual `/compact` body (context continuity P1 — single compaction path).
/// The caller holds the session's turn lock. Builds the agent history from
/// the transcript (`build_agent_history` imports a legacy session once),
/// seeds the prior fold from the active leading block
/// (`transcript::leading_block_fold`), runs `agentic_loop::compact_now`
/// through the delegate `make_delegate` builds around the session's
/// `TranscriptSink` (tagged with the latest turn); the new fold is stored on
/// the new block row. Never marks `agent_messages` rows `compacted`.
pub(crate) async fn run_manual_compact<D, F>(
    db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    session_id: &str,
    threshold: usize,
    settings: &crate::memubot_config::AgentContextSettings,
    make_delegate: F,
) -> anyhow::Result<ManualCompactResult>
where
    D: LoopDelegate,
    F: FnOnce(crate::agent::dispatcher::TranscriptSink) -> D,
{
    use crate::agent::compaction_policy::is_session_block;

    let (messages, persisted_len, turn_id, prior_fold) = {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("DB mutex poisoned"))?;
        let (messages, persisted_len) =
            crate::agent::transcript_history::build_agent_history(&conn, session_id)?;
        // The block belongs to the latest turn (rewinding past that turn
        // drops it and restores the superseded rows).
        let turn_id = crate::agent::transcript::last_active(&conn, session_id)?
            .and_then(|r| r.turn_id)
            .unwrap_or_else(|| format!("compact-{}", uuid::Uuid::new_v4()));
        // The incremental base is the fold of the active leading block — never
        // the session-wide baseline, which can describe rewound turns.
        let prior_fold = crate::agent::transcript::leading_block_fold(&conn, session_id)?;
        (messages, persisted_len, turn_id, prior_fold)
    };

    let before = messages.len();
    let had_block = messages.first().is_some_and(is_session_block);
    let mut ctx = ReasoningContext::new(String::new());
    ctx.messages = messages;
    ctx.transcript_persisted_len = persisted_len;
    ctx.compaction_state.previous_fold = prior_fold;

    let delegate = make_delegate(crate::agent::dispatcher::TranscriptSink {
        db: std::sync::Arc::clone(db),
        session_id: session_id.to_string(),
        turn_id,
    });
    let compacted = crate::agent::agentic_loop::compact_now(&delegate, &mut ctx, threshold, settings).await?;
    let remaining = ctx.messages.len();
    if !compacted {
        return Ok(ManualCompactResult { compacted, before, removed: 0, remaining });
    }

    // [..cut] became one block: remaining = before - cut + 1.
    let cut = before + 1 - remaining;
    let removed = cut - usize::from(had_block);
    Ok(ManualCompactResult { compacted, before, removed, remaining })
}

/// Exclusive end of the group starting at `msgs[start]`: the message itself,
/// plus — when it opens tool calls — every following message up to the one
/// that resolves the last of them. `None` if they don't all resolve in `msgs`.
pub(crate) fn tool_group_end(msgs: &[ChatMessage], start: usize) -> Option<usize> {
    let mut pending: Vec<&str> = Vec::new();
    for (j, m) in msgs.iter().enumerate().skip(start) {
        for b in &m.content {
            match b {
                ContentBlock::ToolUse { id, .. } => pending.push(id),
                ContentBlock::ToolResult { tool_use_id, .. } => pending.retain(|p| p != tool_use_id),
                _ => {}
            }
        }
        if pending.is_empty() {
            return Some(j + 1);
        }
    }
    None
}

#[cfg(test)]
mod transcript_compaction_tests {
    use super::*;
    use crate::agent::agentic_loop::run_agentic_loop;
    use crate::agent::types::{
        AgenticLoopConfig, CompactionTrigger, LoopOutcome, LoopSignal, RespondOutput, ResponseMetadata, TextAction, ToolCall,
    };
    use crate::agent::compaction_policy::{is_session_block, COMPACT_HINT, SESSION_BLOCK_HEADER};
    use crate::agent::dispatcher::TranscriptSink;
    use crate::agent::transcript::{self, TranscriptKind};
    use crate::agent::turn::TurnSnapshot;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// Fake delegate backed by a real in-memory transcript (via the same DB
    /// helpers `ChatDelegate` uses). The "LLM" returns a fixed fold; the
    /// single LLM turn returns a text response and records which messages
    /// were visible (non-compacted).
    struct SinkDelegate {
        sink: TranscriptSink,
        summarize_calls: AtomicUsize,
        incremental_calls: AtomicUsize,
        replace_calls: AtomicUsize,
        visible_at_llm_call: Mutex<Vec<Vec<ChatMessage>>>,
        triggers: Arc<Mutex<Vec<CompactionTrigger>>>,
        /// Echo mode: the "LLM" fold lists every `MK##` marker it was shown
        /// (prior fold's markers + the slice's), so a test can see exactly
        /// which turns reached a summary.
        echo: bool,
        /// `call_llm` fails with these errors (front first) before succeeding.
        llm_errors: Mutex<Vec<String>>,
        llm_calls: AtomicUsize,
        /// Prefix-reusing summary: `None` = the trait default (no prefix
        /// summary); `Some` = run the real `prefix_summary` against this LLM.
        prefix_llm: Option<Arc<dyn crate::llm::LlmProvider>>,
        prefix_calls: AtomicUsize,
        /// `target_tokens` seen by the standalone summarizer calls.
        summary_targets: Mutex<Vec<usize>>,
        /// Workspace root handed to the session-block context.
        workspace_root: Option<std::path::PathBuf>,
        /// `persist_cleared` calls: (tool call ids, freed tokens).
        cleared_calls: Mutex<Vec<(Vec<String>, usize)>>,
    }

    impl SinkDelegate {
        fn new() -> Self {
            let conn = rusqlite::Connection::open_in_memory().unwrap();
            crate::db::migrations::run(&conn).unwrap();
            Self::from_sink(TranscriptSink {
                db: Arc::new(Mutex::new(conn)),
                session_id: "s1".into(),
                turn_id: "t-current".into(),
            })
        }

        fn from_sink(sink: TranscriptSink) -> Self {
            Self {
                sink,
                summarize_calls: AtomicUsize::new(0),
                incremental_calls: AtomicUsize::new(0),
                replace_calls: AtomicUsize::new(0),
                visible_at_llm_call: Mutex::new(Vec::new()),
                triggers: Arc::new(Mutex::new(Vec::new())),
                echo: false,
                llm_errors: Mutex::new(Vec::new()),
                llm_calls: AtomicUsize::new(0),
                prefix_llm: None,
                prefix_calls: AtomicUsize::new(0),
                summary_targets: Mutex::new(Vec::new()),
                workspace_root: None,
                cleared_calls: Mutex::new(Vec::new()),
            }
        }

        fn echo(sink: TranscriptSink) -> Self {
            let mut d = Self::from_sink(sink);
            d.echo = true;
            d
        }

        fn active(&self) -> Vec<transcript::TranscriptRow> {
            transcript::load_active(&self.sink.db.lock().unwrap(), "s1").unwrap()
        }
    }

    /// Every `MK##` marker in the messages' text, in order.
    fn markers(msgs: &[ChatMessage]) -> Vec<String> {
        let mut out = Vec::new();
        for m in msgs {
            for b in &m.content {
                if let ContentBlock::Text { text } = b {
                    let mut rest = text.as_str();
                    while let Some(i) = rest.find("MK") {
                        let tail = &rest[i..];
                        if tail.len() >= 4 && tail[2..4].chars().all(|c| c.is_ascii_digit()) {
                            out.push(tail[..4].to_string());
                        }
                        rest = &rest[i + 2..];
                    }
                }
            }
        }
        out
    }

    fn echo_fold(mut qs: Vec<String>, msgs: &[ChatMessage]) -> crate::agent::compact::StructuredFold {
        qs.extend(markers(msgs));
        crate::agent::compact::StructuredFold::default().with_unresolved_questions(qs)
    }

    fn fixed_fold() -> crate::agent::compact::StructuredFold {
        crate::agent::compact::StructuredFold::default()
            .with_unresolved_questions(vec!["fixed fold".to_string()])
    }

    #[async_trait::async_trait]
    impl LoopDelegate for SinkDelegate {
        async fn check_signals(&self) -> LoopSignal {
            LoopSignal::Continue
        }
        async fn before_llm_call(&self, _: &mut ReasoningContext, _: usize) -> Option<LoopOutcome> {
            None
        }
        async fn call_llm(
            &self,
            reason_ctx: &mut ReasoningContext,
            _: &TurnSnapshot,
            _: usize,
        ) -> Result<RespondOutput, crate::error::Error> {
            self.llm_calls.fetch_add(1, Ordering::SeqCst);
            {
                let mut errs = self.llm_errors.lock().unwrap();
                if !errs.is_empty() {
                    let e = errs.remove(0);
                    return Err(crate::error::Error::Llm(crate::error::LlmError::ApiRequestFailed(e)));
                }
            }
            let visible: Vec<ChatMessage> =
                reason_ctx.messages.iter().filter(|m| !m.compacted).cloned().collect();
            self.visible_at_llm_call.lock().unwrap().push(visible);
            Ok(RespondOutput::Text {
                text: "done".into(),
                thinking: None,
                thinking_signature: None,
                metadata: ResponseMetadata { model: "m".into(), finish_reason: None, usage: None },
            })
        }
        async fn handle_text_response(
            &self,
            text: &str,
            _: ResponseMetadata,
            _: &mut ReasoningContext,
        ) -> TextAction {
            TextAction::Return(LoopOutcome::Response {
                text: text.to_string(),
                usage: None,
                truncated: false,
                model: None,
            })
        }
        async fn execute_tool_calls(
            &self,
            _: Vec<ToolCall>,
            _: &mut ReasoningContext,
        ) -> Result<Option<LoopOutcome>, crate::error::Error> {
            Ok(None)
        }
        async fn summarize_to_fold(
            &self,
            _messages: &[ChatMessage],
            target_tokens: usize,
        ) -> Option<crate::agent::compact::StructuredFold> {
            self.summarize_calls.fetch_add(1, Ordering::SeqCst);
            self.summary_targets.lock().unwrap().push(target_tokens);
            if self.echo {
                return Some(echo_fold(Vec::new(), _messages));
            }
            Some(fixed_fold())
        }
        async fn update_fold_incremental(
            &self,
            _prior: &crate::agent::compact::StructuredFold,
            _new_messages: &[ChatMessage],
            target_tokens: usize,
        ) -> Option<crate::agent::compact::StructuredFold> {
            self.incremental_calls.fetch_add(1, Ordering::SeqCst);
            self.summary_targets.lock().unwrap().push(target_tokens);
            if self.echo {
                return Some(echo_fold(_prior.unresolved_questions.clone(), _new_messages));
            }
            Some(fixed_fold())
        }
        async fn summarize_with_prefix(
            &self,
            reason_ctx: &ReasoningContext,
            snapshot: &TurnSnapshot,
            cut: usize,
            target_tokens: usize,
        ) -> Option<crate::agent::compact::StructuredFold> {
            self.prefix_calls.fetch_add(1, Ordering::SeqCst);
            let llm = self.prefix_llm.as_ref()?;
            let config = crate::llm::CompletionConfig { model: snapshot.model.clone(), ..Default::default() };
            crate::agent::compact::prefix_summary::summarize_with_prefix(
                llm.as_ref(), &config, reason_ctx, snapshot, cut, target_tokens, true,
            )
            .await
        }
        async fn persist_transcript(&self, new_msgs: &[ChatMessage]) -> anyhow::Result<()> {
            crate::agent::dispatcher::persist_to_sink(Some(&self.sink), new_msgs)
        }
        // Same DB-backed helper `ChatDelegate` uses.
        async fn persist_cleared(
            &self,
            cleared: &[(usize, ChatMessage)],
            tool_call_ids: &[String],
            freed_tokens: usize,
        ) -> anyhow::Result<()> {
            self.cleared_calls.lock().unwrap().push((tool_call_ids.to_vec(), freed_tokens));
            crate::agent::dispatcher::persist_cleared_in_sink(Some(&self.sink), cleared)
        }
        // Same DB-backed helper `ChatDelegate` uses.
        fn session_block_context(&self) -> Option<crate::agent::session_block::SessionBlockContext> {
            crate::agent::dispatcher::session_block_context_from_sink(Some(&self.sink), self.workspace_root.as_deref())
        }
        async fn replace_with_session_block(
            &self,
            block: &ChatMessage,
            fold: &crate::agent::compact::StructuredFold,
            superseded_through: usize,
            _threshold_tokens: usize,
            trigger: CompactionTrigger,
        ) -> anyhow::Result<()> {
            self.replace_calls.fetch_add(1, Ordering::SeqCst);
            self.triggers.lock().unwrap().push(trigger);
            crate::agent::dispatcher::replace_session_block_in_sink(&self.sink, block, Some(fold), superseded_through)
                .map(|_| ())
        }
    }

    fn big_user(tokens: usize) -> ChatMessage {
        ChatMessage::user(&"a".repeat(tokens * 4))
    }

    fn text_of(m: &ChatMessage) -> &str {
        match &m.content[0] {
            ContentBlock::Text { text } => text,
            _ => "",
        }
    }

    fn json(msgs: &[ChatMessage]) -> String {
        serde_json::to_string(msgs).unwrap()
    }

    /// 5 turns with ~15K-token user messages → cut at index 4 (last 3 turns kept).
    fn five_big_turns() -> ReasoningContext {
        let mut ctx = ReasoningContext::new("sys".into());
        for i in 0..5 {
            // Distinct openings so the prefix summary's cut anchor is unique.
            let mut user = big_user(15_000);
            if let Some(ContentBlock::Text { text }) = user.content.first_mut() {
                text.replace_range(0..8, &format!("turn {i}: "));
            }
            ctx.messages.push(user);
            ctx.messages.push(ChatMessage::assistant(&format!("reply {i}")));
        }
        ctx
    }

    #[tokio::test]
    async fn transcript_compact_rebuilds_context_with_session_block() {
        let delegate = SinkDelegate::new();
        let mut ctx = five_big_turns();
        let original = ctx.messages.clone();

        let compacted = transcript_compact(&delegate, &mut ctx, None, 128_000, CompactionTrigger::Auto, &settings()).await.unwrap();

        assert!(compacted);
        assert!(text_of(&ctx.messages[0]).starts_with(SESSION_BLOCK_HEADER));
        assert!(text_of(&ctx.messages[0]).contains("fixed fold"));
        assert_eq!(json(&ctx.messages[1..]), json(&original[4..]), "tail kept verbatim");
        assert_eq!(delegate.replace_calls.load(Ordering::SeqCst), 1);
        assert_eq!(delegate.summarize_calls.load(Ordering::SeqCst), 1);
        assert!(ctx.compaction_state.previous_fold.is_some());
        assert!(ctx.messages.iter().all(|m| !m.compacted), "transcript mode rebuilds, never flags");

        // 1:1 invariant: transcript active rows == messages[..persisted].
        assert_eq!(ctx.transcript_persisted_len, ctx.messages.len());
        let rows = delegate.active();
        assert_eq!(rows[0].kind, TranscriptKind::SessionBlock);
        assert_eq!(rows[0].turn_id.as_deref(), Some("t-current"));
        let row_msgs: Vec<ChatMessage> = rows.into_iter().map(|r| r.message).collect();
        assert_eq!(json(&row_msgs), json(&ctx.messages));
    }

    #[tokio::test]
    async fn transcript_compact_keeps_unpersisted_tail_unpersisted() {
        let delegate = SinkDelegate::new();
        let mut ctx = five_big_turns();
        // A dangling tool call can't be flushed (pairing guard).
        ctx.messages.push(ChatMessage::assistant_with_tool_use("t9", "read", serde_json::json!({})));

        assert!(transcript_compact(&delegate, &mut ctx, None, 128_000, CompactionTrigger::Auto, &settings()).await.unwrap());

        assert_eq!(ctx.transcript_persisted_len, ctx.messages.len() - 1);
        assert_eq!(delegate.active().len(), ctx.transcript_persisted_len);
    }

    #[tokio::test]
    async fn flush_after_compaction_writes_exactly_new_messages() {
        let delegate = SinkDelegate::new();
        let mut ctx = five_big_turns();
        assert!(transcript_compact(&delegate, &mut ctx, None, 128_000, CompactionTrigger::Auto, &settings()).await.unwrap());

        ctx.messages.push(ChatMessage::user("next question"));
        ctx.messages.push(ChatMessage::assistant_with_tool_use("t1", "read", serde_json::json!({})));
        ctx.messages.push(ChatMessage::user_tool_result("t1", "ok", false));
        ctx.messages.push(ChatMessage::assistant("answer"));
        crate::agent::transcript_history::flush_transcript(&delegate, &mut ctx).await;

        assert_eq!(ctx.transcript_persisted_len, ctx.messages.len());
        let row_msgs: Vec<ChatMessage> = delegate.active().into_iter().map(|r| r.message).collect();
        assert_eq!(row_msgs.len(), ctx.messages.len(), "no duplicates, no gaps");
        assert_eq!(json(&row_msgs), json(&ctx.messages));

        // A second compaction maps onto the same rows.
        ctx.messages.push(big_user(15_000));
        ctx.messages.push(ChatMessage::assistant("r"));
        ctx.messages.push(big_user(15_000));
        ctx.messages.push(ChatMessage::assistant("r"));
        assert!(transcript_compact(&delegate, &mut ctx, None, 128_000, CompactionTrigger::Auto, &settings()).await.unwrap());
        assert_eq!(delegate.summarize_calls.load(Ordering::SeqCst), 1);
        assert_eq!(delegate.incremental_calls.load(Ordering::SeqCst), 1, "second pass is incremental");
        let row_msgs: Vec<ChatMessage> = delegate.active().into_iter().map(|r| r.message).collect();
        assert_eq!(json(&row_msgs), json(&ctx.messages));
        assert_eq!(ctx.messages.iter().filter(|m| is_session_block(m)).count(), 1);
    }

    #[tokio::test]
    async fn transcript_compact_noop_when_nothing_to_compact() {
        let delegate = SinkDelegate::new();
        let mut ctx = ReasoningContext::new("sys".into());
        ctx.messages.push(ChatMessage::user("hi"));
        ctx.messages.push(ChatMessage::assistant("hello"));
        assert!(!transcript_compact(&delegate, &mut ctx, None, 128_000, CompactionTrigger::Auto, &settings()).await.unwrap());
        assert_eq!(delegate.replace_calls.load(Ordering::SeqCst), 0);
        assert_eq!(ctx.messages.len(), 2);
    }

    fn settings() -> crate::memubot_config::AgentContextSettings {
        crate::memubot_config::AgentContextSettings::default()
    }

    fn test_snapshot() -> TurnSnapshot {
        TurnSnapshot {
            turn_index: 1,
            model: "m".into(),
            system_prompt: Arc::new("sys".into()),
            dynamic_context: String::new(),
            tools: Arc::new(Vec::new()),
            force_text: false,
        }
    }

    #[tokio::test]
    async fn prefix_summary_used_when_snapshot_given() {
        use crate::agent::compact::prefix_summary::test_support::ScriptedLlm;
        let mut delegate = SinkDelegate::new();
        let llm = ScriptedLlm::text(r#"{"user_requests":["PREFIX FOLD"]}"#);
        delegate.prefix_llm = Some(llm.clone());
        let mut ctx = five_big_turns();
        let snap = test_snapshot();

        assert!(transcript_compact(&delegate, &mut ctx, Some(&snap), 128_000, CompactionTrigger::Auto, &settings())
            .await
            .unwrap());

        assert_eq!(delegate.prefix_calls.load(Ordering::SeqCst), 1);
        assert_eq!(delegate.summarize_calls.load(Ordering::SeqCst), 0, "no standalone summary");
        assert_eq!(llm.calls(), 1);
        assert!(text_of(&ctx.messages[0]).contains("PREFIX FOLD"));
        assert_eq!(ctx.compaction_state.previous_fold.as_ref().unwrap().user_requests, vec!["PREFIX FOLD"]);
    }

    #[tokio::test]
    async fn prefix_summary_falls_back_on_tool_call_or_bad_json() {
        use crate::agent::compact::prefix_summary::test_support::ScriptedLlm;
        for llm in [ScriptedLlm::tool_call(), ScriptedLlm::text(""), ScriptedLlm::text("not json at all")] {
            let mut delegate = SinkDelegate::new();
            delegate.prefix_llm = Some(llm.clone());
            let mut ctx = five_big_turns();
            let snap = test_snapshot();
            let mut s = settings();
            s.summary_target_tokens = 4_500;

            let compacted =
                transcript_compact(&delegate, &mut ctx, Some(&snap), 128_000, CompactionTrigger::Auto, &s).await.unwrap();

            assert!(compacted, "compaction still completes");
            assert_eq!(llm.calls(), 1, "prefix summary was attempted");
            assert_eq!(delegate.prefix_calls.load(Ordering::SeqCst), 1);
            assert_eq!(delegate.summarize_calls.load(Ordering::SeqCst), 1, "fell back to the standalone summarizer");
            assert_eq!(*delegate.summary_targets.lock().unwrap(), vec![4_500], "target length passed through");
            assert!(text_of(&ctx.messages[0]).contains("fixed fold"));
            assert_eq!(delegate.replace_calls.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn auto_compaction_in_loop_passes_snapshot_and_settings() {
        let delegate = SinkDelegate::new();
        let mut ctx = ReasoningContext::new("sys".into());
        for i in 0..3 {
            ctx.messages.push(ChatMessage::user(&format!("u{i}")));
            ctx.messages.push(ChatMessage::assistant(&format!("a{i}")));
        }
        ctx.messages.push(ChatMessage::user("last question"));
        ctx.messages.push(ChatMessage::assistant(&"a".repeat(40_000 * 4)));
        let mut config = loop_config(0);
        config.context_settings.summary_target_tokens = 5_000;

        let _ = run_agentic_loop(&delegate, &mut ctx, &config).await;

        assert_eq!(delegate.prefix_calls.load(Ordering::SeqCst), 1, "auto compaction tries the prefix summary");
        assert_eq!(*delegate.summary_targets.lock().unwrap(), vec![5_000]);
    }

    fn loop_config(window: u32) -> AgenticLoopConfig {
        AgenticLoopConfig {
            max_iterations: 1,
            enable_tool_intent_nudge: false,
            transcript_mode: true,
            compaction_threshold_tokens: 32_000,
            model_context_length: window,
            ..Default::default()
        }
    }

    fn unflagged(msgs: &[ChatMessage]) -> Vec<ChatMessage> {
        msgs.iter()
            .cloned()
            .map(|mut m| {
                m.compacted = false;
                m
            })
            .collect()
    }

    /// 64K window → threshold 32_000, hard limit 49_152. The request is over
    /// the hard limit; one compaction is not enough because the current turn's
    /// oversized reply sits after the last user message. No second compaction;
    /// hard truncation drops only what precedes the last user text message,
    /// then the request is sent anyway (warn).
    #[tokio::test]
    async fn compaction_does_not_thrash_on_oversized_tail() {
        let delegate = SinkDelegate::new();
        let mut ctx = ReasoningContext::new("sys".into());
        for i in 0..3 {
            ctx.messages.push(ChatMessage::user(&format!("u{i}")));
            ctx.messages.push(ChatMessage::assistant(&format!("a{i}")));
        }
        ctx.messages.push(ChatMessage::user("last question"));
        ctx.messages.push(ChatMessage::assistant(&"a".repeat(60_000 * 4)));

        let outcome = run_agentic_loop(&delegate, &mut ctx, &loop_config(65_536)).await;
        assert!(matches!(outcome, LoopOutcome::Response { .. }));

        assert_eq!(delegate.summarize_calls.load(Ordering::SeqCst), 1, "compacted exactly once");
        assert_eq!(delegate.incremental_calls.load(Ordering::SeqCst), 0);
        assert_eq!(delegate.replace_calls.load(Ordering::SeqCst), 1);
        let visible = delegate.visible_at_llm_call.lock().unwrap()[0].clone();
        assert!(is_session_block(&visible[0]), "session_block kept");
        assert_eq!(text_of(&visible[1]), "last question", "last user message kept");
        assert_eq!(visible.len(), 3, "the current turn's reply is never truncated");

        // Hard truncation is in-memory only: the transcript still maps 1:1.
        let row_msgs: Vec<ChatMessage> = delegate.active().into_iter().map(|r| r.message).collect();
        assert_eq!(json(&row_msgs), json(&unflagged(&ctx.messages)));
    }

    /// Ruling 8(b): over the soft threshold but under the hard limit (no
    /// window = no hard limit) → compact once, never truncate.
    #[tokio::test]
    async fn compact_over_threshold_under_hard_limit_does_not_truncate() {
        let delegate = SinkDelegate::new();
        let mut ctx = ReasoningContext::new("sys".into());
        for i in 0..3 {
            ctx.messages.push(ChatMessage::user(&format!("u{i}")));
            ctx.messages.push(ChatMessage::assistant(&format!("a{i}")));
        }
        ctx.messages.push(ChatMessage::user("last question"));
        ctx.messages.push(ChatMessage::assistant(&"a".repeat(40_000 * 4)));

        let _ = run_agentic_loop(&delegate, &mut ctx, &loop_config(0)).await;

        assert_eq!(delegate.summarize_calls.load(Ordering::SeqCst), 1);
        assert!(ctx.messages.iter().all(|m| !m.compacted), "no hard truncation under the hard limit");
        let visible = delegate.visible_at_llm_call.lock().unwrap()[0].clone();
        assert!(is_session_block(&visible[0]));
        assert_eq!(visible.len(), 7, "block + u1..a2 + last question + reply");
    }

    /// Ruling 8(b): a `Compact` decision with nothing compactable is a no-op.
    #[tokio::test]
    async fn compact_with_nothing_compactable_does_not_truncate() {
        let delegate = SinkDelegate::new();
        let mut ctx = ReasoningContext::new("sys".into());
        ctx.messages.push(ChatMessage::user("only question"));
        ctx.messages.push(ChatMessage::assistant(&"a".repeat(40_000 * 4)));

        let _ = run_agentic_loop(&delegate, &mut ctx, &loop_config(0)).await;

        assert_eq!(delegate.summarize_calls.load(Ordering::SeqCst), 0);
        assert!(ctx.messages.iter().all(|m| !m.compacted));
    }

    /// Ruling 8(a): a `HardTruncate` decision compacts first; when that brings
    /// the request under the hard limit nothing is truncated.
    #[tokio::test]
    async fn hard_truncate_decision_compacts_first() {
        let delegate = SinkDelegate::new();
        let mut ctx = five_big_turns(); // ~75K tokens ≥ 49_152 hard limit
        let original = ctx.messages.clone();

        let _ = run_agentic_loop(&delegate, &mut ctx, &loop_config(65_536)).await;

        assert_eq!(delegate.summarize_calls.load(Ordering::SeqCst), 1, "compaction ran");
        assert!(is_session_block(&ctx.messages[0]));
        assert_eq!(json(&ctx.messages[1..7]), json(&original[4..]), "tail kept verbatim");
        assert!(ctx.messages.iter().all(|m| !m.compacted), "no hard truncation needed");
    }

    /// Ruling 8(c): tool calls/results after the last user text message (the
    /// current turn's progress) survive hard truncation.
    #[tokio::test]
    async fn hard_truncate_keeps_current_turn_tool_progress() {
        let delegate = SinkDelegate::new();
        let mut ctx = ReasoningContext::new("sys".into());
        // 32K window → threshold 8_192, hard limit 16_384.
        ctx.messages.push(ChatMessage::user("u0"));
        ctx.messages.push(ChatMessage::assistant("a0"));
        ctx.messages.push(ChatMessage::user("current question"));
        ctx.messages.push(ChatMessage::assistant_with_tool_use("t1", "read", serde_json::json!({})));
        ctx.messages.push(ChatMessage::user_tool_result("t1", &"a".repeat(20_000 * 4), false));

        let _ = run_agentic_loop(&delegate, &mut ctx, &loop_config(32_768)).await;

        assert!(ctx.messages[0].compacted && ctx.messages[1].compacted, "older history dropped");
        assert!(ctx.messages[2..5].iter().all(|m| !m.compacted), "current turn kept");
        let visible = delegate.visible_at_llm_call.lock().unwrap()[0].clone();
        assert_eq!(text_of(&visible[0]), "current question");
        assert_eq!(visible.len(), 3);
    }

    #[tokio::test]
    async fn hard_truncate_with_small_window_keeps_tool_pairs_atomic() {
        let delegate = SinkDelegate::new();
        let mut ctx = ReasoningContext::new("sys".into());
        // 32K window → threshold 8_192, hard limit 16_384.
        ctx.messages.push(big_user(4_000));
        ctx.messages.push(ChatMessage::assistant(&"a".repeat(4_000 * 4)));
        ctx.messages.push(ChatMessage::user("u1"));
        ctx.messages.push(ChatMessage::assistant_with_tool_use("t1", "read", serde_json::json!({})));
        ctx.messages.push(ChatMessage::user_tool_result("t1", &"a".repeat(9_000 * 4), false));
        ctx.messages.push(ChatMessage::assistant("a1"));
        ctx.messages.push(ChatMessage::user("u2"));

        let outcome = run_agentic_loop(&delegate, &mut ctx, &loop_config(32_768)).await;
        assert!(matches!(outcome, LoopOutcome::Response { .. }));

        assert_eq!(delegate.summarize_calls.load(Ordering::SeqCst), 0, "HardTruncate does not summarise");
        assert_eq!(delegate.replace_calls.load(Ordering::SeqCst), 0);
        let visible = delegate.visible_at_llm_call.lock().unwrap()[0].clone();
        assert_eq!(text_of(visible.last().unwrap()), "u2", "last user message kept");
        // The tool call and its result go together (both dropped here).
        assert!(ctx.messages[..5].iter().all(|m| m.compacted), "oldest 5 dropped incl. the pair");
        assert!(!ctx.messages[5].compacted && !ctx.messages[6].compacted);
        let visible_tokens: usize = visible.iter().map(crate::agent::types::estimate_live_message_tokens).sum();
        assert!(visible_tokens <= 8_192, "visible history fits the target, got {visible_tokens}");
    }
    // ── Manual /compact (run_manual_compact) ─────────────────────────────

    fn test_db() -> Arc<Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute("INSERT INTO agent_sessions (id, created_at, updated_at) VALUES ('s1', 0, 0)", [])
            .unwrap();
        Arc::new(Mutex::new(conn))
    }

    /// `turns` user/assistant turns: agent_messages rows (user row id = turn
    /// id) and, when `with_transcript`, the matching transcript rows.
    fn seed_turns(db: &Arc<Mutex<rusqlite::Connection>>, turns: &[(String, String)], with_transcript: bool) {
        for (i, (user, reply)) in turns.iter().enumerate() {
            let turn = format!("t{i}");
            let ts = 1_000 * (i as i64 + 1);
            {
                let conn = db.lock().unwrap();
                conn.execute(
                    "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1,'s1','user',?2,?3)",
                    rusqlite::params![turn, user, ts],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1,'s1','assistant',?2,?3)",
                    rusqlite::params![format!("{turn}-a"), reply, ts + 1],
                )
                .unwrap();
            }
            if with_transcript {
                let sink = TranscriptSink { db: db.clone(), session_id: "s1".into(), turn_id: turn };
                crate::agent::dispatcher::persist_to_sink(
                    Some(&sink),
                    &[ChatMessage::user(user), ChatMessage::assistant(reply)],
                )
                .unwrap();
            }
        }
    }

    fn big_turns() -> Vec<(String, String)> {
        (0..5).map(|i| ("a".repeat(15_000 * 4), format!("reply {i}"))).collect()
    }

    fn compacted_agent_rows(db: &Arc<Mutex<rusqlite::Connection>>) -> i64 {
        db.lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM agent_messages WHERE session_id = 's1' AND compacted = 1", [], |r| r.get(0))
            .unwrap()
    }

    fn fold_display_rows(db: &Arc<Mutex<rusqlite::Connection>>) -> i64 {
        db.lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM agent_messages WHERE session_id = 's1' AND content LIKE ?1",
                [format!("{SESSION_BLOCK_HEADER}%")],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[tokio::test]
    async fn manual_compact_writes_session_block() {
        let db = test_db();
        seed_turns(&db, &big_turns(), true);

        let res = run_manual_compact(&db, "s1", 128_000, &settings(), SinkDelegate::from_sink).await.unwrap();

        assert_eq!(res, ManualCompactResult { compacted: true, before: 10, removed: 4, remaining: 7 });
        let rows = transcript::load_active(&db.lock().unwrap(), "s1").unwrap();
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[0].kind, TranscriptKind::SessionBlock);
        assert!(text_of(&rows[0].message).starts_with(SESSION_BLOCK_HEADER));
        assert_eq!(rows[0].turn_id.as_deref(), Some("t4"), "block is tagged with the latest turn");
        assert_eq!(fold_display_rows(&db), 1, "fold display row in agent_messages");
        assert_eq!(compacted_agent_rows(&db), 0, "no agent_messages row is marked compacted");
        assert!(
            transcript::leading_block_fold(&db.lock().unwrap(), "s1").unwrap().is_some(),
            "the new fold rides on the block row (next incremental base)"
        );
    }

    #[tokio::test]
    async fn manual_compact_uses_standalone_summarizer() {
        let db = test_db();
        seed_turns(&db, &big_turns(), true);
        let prefix_calls = Arc::new(AtomicUsize::new(0));
        let summarize_calls = Arc::new(AtomicUsize::new(0));
        let targets = Arc::new(Mutex::new(Vec::new()));
        let (p, sc, t) = (prefix_calls.clone(), summarize_calls.clone(), targets.clone());
        let mut s = settings();
        s.summary_target_tokens = 2_500;

        let res = run_manual_compact(&db, "s1", 128_000, &s, move |sink| CountingDelegate {
            inner: SinkDelegate::from_sink(sink),
            prefix_calls: p,
            summarize_calls: sc,
            targets: t,
        })
        .await
        .unwrap();

        assert!(res.compacted);
        assert_eq!(prefix_calls.load(Ordering::SeqCst), 0, "manual /compact has no real system/tools snapshot");
        assert_eq!(summarize_calls.load(Ordering::SeqCst), 1);
        assert_eq!(*targets.lock().unwrap(), vec![2_500]);
    }

    /// Wraps `SinkDelegate`, exposing its counters through shared handles
    /// (the delegate itself is moved into `run_manual_compact`).
    struct CountingDelegate {
        inner: SinkDelegate,
        prefix_calls: Arc<AtomicUsize>,
        summarize_calls: Arc<AtomicUsize>,
        targets: Arc<Mutex<Vec<usize>>>,
    }

    #[async_trait::async_trait]
    impl LoopDelegate for CountingDelegate {
        async fn check_signals(&self) -> LoopSignal {
            LoopSignal::Continue
        }
        async fn before_llm_call(&self, _: &mut ReasoningContext, _: usize) -> Option<LoopOutcome> {
            None
        }
        async fn call_llm(&self, c: &mut ReasoningContext, s: &TurnSnapshot, i: usize) -> Result<RespondOutput, crate::error::Error> {
            self.inner.call_llm(c, s, i).await
        }
        async fn handle_text_response(&self, t: &str, m: ResponseMetadata, c: &mut ReasoningContext) -> TextAction {
            self.inner.handle_text_response(t, m, c).await
        }
        async fn execute_tool_calls(&self, t: Vec<ToolCall>, c: &mut ReasoningContext) -> Result<Option<LoopOutcome>, crate::error::Error> {
            self.inner.execute_tool_calls(t, c).await
        }
        async fn summarize_to_fold(&self, m: &[ChatMessage], target: usize) -> Option<crate::agent::compact::StructuredFold> {
            self.summarize_calls.fetch_add(1, Ordering::SeqCst);
            self.targets.lock().unwrap().push(target);
            self.inner.summarize_to_fold(m, target).await
        }
        async fn summarize_with_prefix(
            &self,
            c: &ReasoningContext,
            s: &TurnSnapshot,
            cut: usize,
            target: usize,
        ) -> Option<crate::agent::compact::StructuredFold> {
            self.prefix_calls.fetch_add(1, Ordering::SeqCst);
            self.inner.summarize_with_prefix(c, s, cut, target).await
        }
        async fn persist_transcript(&self, m: &[ChatMessage]) -> anyhow::Result<()> {
            self.inner.persist_transcript(m).await
        }
        async fn replace_with_session_block(
            &self,
            block: &ChatMessage,
            fold: &crate::agent::compact::StructuredFold,
            through: usize,
            threshold: usize,
            trigger: CompactionTrigger,
        ) -> anyhow::Result<()> {
            self.inner.replace_with_session_block(block, fold, through, threshold, trigger).await
        }
    }

    // ── Session block: working set + plan + recent files (P2 Task 7) ─────

    fn record_events(db: &Arc<Mutex<rusqlite::Connection>>, turn: &str, events: &[(&str, crate::agent::file_working_set::FileOp)]) {
        let events: Vec<crate::agent::file_working_set::FileEvent> = events
            .iter()
            .map(|(p, op)| crate::agent::file_working_set::FileEvent { path: (*p).into(), op: *op, detail: "change".into() })
            .collect();
        crate::agent::file_working_set::record(&db.lock().unwrap(), "s1", Some(turn), &events).unwrap();
    }

    #[tokio::test]
    async fn manual_compact_block_includes_session_working_set() {
        use crate::agent::file_working_set::FileOp;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn a_body() {}\n").unwrap();
        std::fs::create_dir_all(dir.path().join(".uclaw/plans")).unwrap();
        std::fs::write(dir.path().join(".uclaw/plans/p.md"), "- [ ] PLAN STEP\n").unwrap();
        let db = test_db();
        seed_turns(&db, &big_turns(), true);
        record_events(&db, "t1", &[("a.rs", FileOp::Edit), (".uclaw/plans/p.md", FileOp::Plan)]);
        let root = dir.path().to_path_buf();

        let res = run_manual_compact(&db, "s1", 128_000, &settings(), move |sink| {
            let mut d = SinkDelegate::from_sink(sink);
            d.workspace_root = Some(root);
            d
        })
        .await
        .unwrap();

        assert!(res.compacted);
        let block = leading_block_text(&db);
        assert!(block.starts_with(SESSION_BLOCK_HEADER));
        assert!(block.contains("<file_working_set>") && block.contains("- a.rs（修改 1 次）"), "{block}");
        assert!(block.contains("<current_plan path=\".uclaw/plans/p.md\">\n- [ ] PLAN STEP\n</current_plan>"), "{block}");
        assert!(block.contains("<recent_files>\n<file path=\"a.rs\">\nfn a_body() {}\n</file>"), "{block}");
    }

    #[tokio::test]
    async fn compaction_block_lists_files_modified_in_earlier_turns() {
        use crate::agent::file_working_set::FileOp;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.rs"), "A CONTENT").unwrap();
        std::fs::write(dir.path().join("b.rs"), "B CONTENT").unwrap();
        let mut delegate = SinkDelegate::new();
        delegate.workspace_root = Some(dir.path().to_path_buf());
        record_events(&delegate.sink.db, "t1", &[("a.rs", FileOp::Edit)]);
        record_events(&delegate.sink.db, "t2", &[("b.rs", FileOp::Write)]);
        let mut ctx = five_big_turns(); // compaction happens in the current turn (t-current)

        assert!(transcript_compact(&delegate, &mut ctx, None, 128_000, CompactionTrigger::Auto, &settings()).await.unwrap());

        let block = text_of(&ctx.messages[0]);
        for (p, body) in [("a.rs", "A CONTENT"), ("b.rs", "B CONTENT")] {
            assert!(block.contains(&format!("- {p}（")), "{p} listed in the working set: {block}");
            assert!(block.contains(&format!("<file path=\"{p}\">\n{body}\n</file>")), "{p} reloaded: {block}");
        }
        // The persisted block row is the same text.
        assert_eq!(text_of(&delegate.active()[0].message), block);
    }

    #[tokio::test]
    async fn session_block_without_context_is_the_summary_only() {
        let delegate = SinkDelegate::new(); // no file events
        let mut ctx = five_big_turns();
        assert!(transcript_compact(&delegate, &mut ctx, None, 128_000, CompactionTrigger::Auto, &settings()).await.unwrap());
        let block = text_of(&ctx.messages[0]);
        assert!(!block.contains("<file_working_set>") && !block.contains("<recent_files>"), "{block}");
    }

    #[tokio::test]
    async fn manual_compact_imports_legacy_session_first() {
        let db = test_db();
        seed_turns(&db, &big_turns(), false);
        assert!(!transcript::has_transcript(&db.lock().unwrap(), "s1").unwrap());

        let res = run_manual_compact(&db, "s1", 128_000, &settings(), SinkDelegate::from_sink).await.unwrap();

        assert!(res.compacted);
        assert_eq!(res.before, 10, "every legacy text row was imported");
        let rows = transcript::load_active(&db.lock().unwrap(), "s1").unwrap();
        assert_eq!(rows[0].kind, TranscriptKind::SessionBlock);
        assert_eq!(rows.len(), 7);
        assert_eq!(fold_display_rows(&db), 1);
        assert_eq!(compacted_agent_rows(&db), 0);
    }

    #[tokio::test]
    async fn manual_compact_nothing_compactable_is_noop() {
        let db = test_db();
        let turns: Vec<(String, String)> =
            (0..2).map(|i| (format!("u{i}"), format!("a{i}"))).collect();
        seed_turns(&db, &turns, true);
        let ids = |db: &Arc<Mutex<rusqlite::Connection>>| -> Vec<i64> {
            transcript::load_active(&db.lock().unwrap(), "s1").unwrap().iter().map(|r| r.id).collect()
        };
        let before = ids(&db);

        let res = run_manual_compact(&db, "s1", 128_000, &settings(), SinkDelegate::from_sink).await.unwrap();

        assert_eq!(res, ManualCompactResult { compacted: false, before: 4, removed: 0, remaining: 4 });
        assert_eq!(ids(&db), before, "transcript unchanged");
        assert_eq!(fold_display_rows(&db), 0);
        assert_eq!(compacted_agent_rows(&db), 0);
    }

    #[test]
    fn manual_compact_report_text_does_not_claim_a_noop_compaction() {
        let done = ManualCompactResult { compacted: true, before: 10, removed: 4, remaining: 7 };
        assert_eq!(done.report_text(), "Compacted: folded 4 earlier messages, 7 remain.");
        let noop = ManualCompactResult { compacted: false, before: 4, removed: 0, remaining: 4 };
        assert_eq!(noop.report_text(), "Nothing to compact: 4 messages kept.");
    }

    #[tokio::test]
    async fn manual_compact_reports_manual_trigger() {
        let db = test_db();
        seed_turns(&db, &big_turns(), true);
        let triggers = Arc::new(Mutex::new(Vec::new()));
        let t = triggers.clone();

        let res = run_manual_compact(&db, "s1", 128_000, &settings(), move |sink| {
            let mut d = SinkDelegate::from_sink(sink);
            d.triggers = t;
            d
        })
        .await
        .unwrap();

        assert!(res.compacted);
        assert_eq!(*triggers.lock().unwrap(), vec![CompactionTrigger::Manual]);
    }

    #[tokio::test]
    async fn loop_compaction_reports_auto_trigger() {
        let delegate = SinkDelegate::new();
        let mut ctx = five_big_turns();

        let compacted = crate::agent::agentic_loop::apply_transcript_compaction_decision(
            &mut ctx,
            &delegate,
            crate::agent::compaction_policy::CompactionDecision::Compact,
            128_000,
            None,
            &settings(),
        )
        .await;

        assert!(compacted);
        assert_eq!(*delegate.triggers.lock().unwrap(), vec![CompactionTrigger::Auto]);
    }

    // ── Rewind must not resurrect rewound turns through the prior fold ───

    /// User/assistant turn `turn` (agent_messages rows + transcript rows);
    /// the user text carries the `MK##` marker and is big enough (~15K
    /// tokens) that the last 3 turns fill the kept tail.
    fn add_marked_turn(db: &Arc<Mutex<rusqlite::Connection>>, n: usize, ts: i64) {
        let turn = format!("t{n}");
        let user = format!("MK{n:02} {}", "a".repeat(15_000 * 4));
        {
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1,'s1','user',?2,?3)",
                rusqlite::params![turn, user, ts],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1,'s1','assistant',?2,?3)",
                rusqlite::params![format!("{turn}-a"), format!("reply {n}"), ts + 1],
            )
            .unwrap();
        }
        let sink = TranscriptSink { db: db.clone(), session_id: "s1".into(), turn_id: turn };
        crate::agent::dispatcher::persist_to_sink(
            Some(&sink),
            &[ChatMessage::user(&user), ChatMessage::assistant(&format!("reply {n}"))],
        )
        .unwrap();
    }

    async fn manual_compact_echo(db: &Arc<Mutex<rusqlite::Connection>>) -> ManualCompactResult {
        run_manual_compact(db, "s1", 128_000, &settings(), SinkDelegate::echo).await.unwrap()
    }

    fn leading_block_text(db: &Arc<Mutex<rusqlite::Connection>>) -> String {
        let rows = transcript::load_active(&db.lock().unwrap(), "s1").unwrap();
        assert_eq!(rows[0].kind, TranscriptKind::SessionBlock);
        text_of(&rows[0].message).to_string()
    }

    fn rewind(db: &Arc<Mutex<rusqlite::Connection>>, target: &str) {
        crate::agent::transcript_branch::rewind_with_transcript(&db.lock().unwrap(), "s1", target).unwrap();
    }

    fn assert_no_markers(text: &str, gone: std::ops::RangeInclusive<usize>) {
        for n in gone {
            let m = format!("MK{n:02}");
            assert!(!text.contains(&m), "rewound turn {m} resurfaced in the session_block:\n{text}");
        }
    }

    /// Compact → rewind to before the compaction → compact again: the new
    /// block summarises only what is still in the conversation.
    #[tokio::test]
    async fn compact_after_rewind_past_compaction_excludes_rewound_turns() {
        let db = test_db();
        for n in 0..7 {
            add_marked_turn(&db, n, 1_000 * (n as i64 + 1));
        }
        assert!(manual_compact_echo(&db).await.compacted); // folds t0..t3
        assert!(leading_block_text(&db).contains("MK03"));

        rewind(&db, "t1-a"); // drops t2..t6 and the block
        for n in 7..12 {
            add_marked_turn(&db, n, 100_000 + 1_000 * n as i64);
        }
        assert!(manual_compact_echo(&db).await.compacted);

        let block = leading_block_text(&db);
        assert!(block.contains("MK00") && block.contains("MK07"), "{block}");
        assert_no_markers(&block, 2..=6);
    }

    /// Two compactions, rewind to a turn kept verbatim by the first one but
    /// folded by the second: the next compaction builds on the first block's
    /// fold, never on the second (which covers rewound turns).
    #[tokio::test]
    async fn compact_after_rewind_between_two_compactions_excludes_rewound_turns() {
        let db = test_db();
        for n in 0..7 {
            add_marked_turn(&db, n, 1_000 * (n as i64 + 1));
        }
        assert!(manual_compact_echo(&db).await.compacted); // b1 = t0..t3; tail t4..t6
        for n in 7..9 {
            add_marked_turn(&db, n, 1_000 * (n as i64 + 1));
        }
        assert!(manual_compact_echo(&db).await.compacted); // b2 = b1 + t4, t5
        assert!(leading_block_text(&db).contains("MK05"));

        rewind(&db, "t4-a"); // drops t5..t8 and b2; b1 + t4 remain
        assert!(leading_block_text(&db).contains("MK03"));
        for n in 9..13 {
            add_marked_turn(&db, n, 100_000 + 1_000 * n as i64);
        }
        assert!(manual_compact_echo(&db).await.compacted);

        let block = leading_block_text(&db);
        for kept in ["MK00", "MK03", "MK04", "MK09"] {
            assert!(block.contains(kept), "missing {kept}:\n{block}");
        }
        assert_no_markers(&block, 5..=8);
    }

    /// A prior fold only describes a leading session_block: with no block at
    /// the head (e.g. after a rewind dropped it) compaction starts from scratch.
    #[tokio::test]
    async fn transcript_compact_ignores_prior_fold_without_leading_block() {
        let delegate = SinkDelegate::new();
        let mut ctx = five_big_turns();
        ctx.compaction_state.previous_fold = Some(fixed_fold());

        assert!(transcript_compact(&delegate, &mut ctx, None, 128_000, CompactionTrigger::Auto, &settings()).await.unwrap());

        assert_eq!(delegate.summarize_calls.load(Ordering::SeqCst), 1, "full summary");
        assert_eq!(delegate.incremental_calls.load(Ordering::SeqCst), 0, "stale prior not used");
    }

    // ── Provider context-overflow recovery ──────────────────────────────

    const OVERFLOW: &str = "HTTP 400: This model's maximum context length is 131072 tokens (context_length_exceeded)";

    fn overflow_config(transcript_mode: bool) -> AgenticLoopConfig {
        AgenticLoopConfig {
            max_iterations: 1,
            enable_tool_intent_nudge: false,
            transcript_mode,
            compaction_threshold_tokens: 128_000, // ~75K history: no proactive compaction
            model_context_length: 0,
            ..Default::default()
        }
    }

    fn failure_text(outcome: &LoopOutcome) -> String {
        match outcome {
            LoopOutcome::Failure { error } => error.clone(),
            other => panic!("expected Failure, got {other:?}"),
        }
    }

    /// Transcript mode: a context-length error compacts once and retries once.
    #[tokio::test]
    async fn context_overflow_compacts_and_retries_once() {
        let delegate = SinkDelegate::new();
        delegate.llm_errors.lock().unwrap().push(OVERFLOW.into());
        let mut ctx = five_big_turns();

        let outcome = run_agentic_loop(&delegate, &mut ctx, &overflow_config(true)).await;

        assert!(matches!(outcome, LoopOutcome::Response { .. }), "{outcome:?}");
        assert_eq!(delegate.llm_calls.load(Ordering::SeqCst), 2, "one retry");
        assert_eq!(delegate.replace_calls.load(Ordering::SeqCst), 1, "compacted once");
        // The rejected request + one instruction would overflow again: the
        // overflow path never tries the prefix-reusing summary.
        assert_eq!(delegate.prefix_calls.load(Ordering::SeqCst), 0, "no prefix summary on overflow");
        assert_eq!(*delegate.triggers.lock().unwrap(), vec![CompactionTrigger::Auto]);
        let visible = delegate.visible_at_llm_call.lock().unwrap()[0].clone();
        assert!(is_session_block(&visible[0]), "retry sees the compacted history");
    }

    /// A second overflow after the retry fails (no loop) with the /compact hint.
    #[tokio::test]
    async fn context_overflow_retries_at_most_once() {
        let delegate = SinkDelegate::new();
        delegate.llm_errors.lock().unwrap().extend([OVERFLOW.to_string(), OVERFLOW.to_string()]);
        let mut ctx = five_big_turns();

        let outcome = run_agentic_loop(&delegate, &mut ctx, &overflow_config(true)).await;

        assert_eq!(delegate.llm_calls.load(Ordering::SeqCst), 2);
        assert_eq!(delegate.replace_calls.load(Ordering::SeqCst), 1);
        let err = failure_text(&outcome);
        assert!(err.contains("context_length_exceeded") && err.contains(COMPACT_HINT), "{err}");
    }

    /// Nothing to compact → no retry; the error carries the /compact hint.
    #[tokio::test]
    async fn context_overflow_without_compactable_history_surfaces_hint() {
        let delegate = SinkDelegate::new();
        delegate.llm_errors.lock().unwrap().push(OVERFLOW.into());
        let mut ctx = ReasoningContext::new("sys".into());
        ctx.messages.push(ChatMessage::user("only question"));

        let outcome = run_agentic_loop(&delegate, &mut ctx, &overflow_config(true)).await;

        assert_eq!(delegate.llm_calls.load(Ordering::SeqCst), 1);
        assert!(failure_text(&outcome).ends_with(COMPACT_HINT));
    }

    /// Fallback (non-transcript) mode never compacts here; hint only.
    #[tokio::test]
    async fn context_overflow_outside_transcript_mode_surfaces_hint() {
        let delegate = SinkDelegate::new();
        delegate.llm_errors.lock().unwrap().push(OVERFLOW.into());
        let mut ctx = five_big_turns();

        let outcome = run_agentic_loop(&delegate, &mut ctx, &overflow_config(false)).await;

        assert_eq!(delegate.llm_calls.load(Ordering::SeqCst), 1);
        assert_eq!(delegate.replace_calls.load(Ordering::SeqCst), 0);
        assert!(failure_text(&outcome).ends_with(COMPACT_HINT));
    }

    /// Other errors are untouched (no retry, no hint).
    #[tokio::test]
    async fn non_overflow_llm_error_is_unchanged() {
        let delegate = SinkDelegate::new();
        delegate.llm_errors.lock().unwrap().push("HTTP 401: invalid api key".into());
        let mut ctx = five_big_turns();

        let outcome = run_agentic_loop(&delegate, &mut ctx, &overflow_config(true)).await;

        assert_eq!(delegate.llm_calls.load(Ordering::SeqCst), 1);
        assert!(!failure_text(&outcome).contains(COMPACT_HINT));
    }

    // ── Tool-result clearing before compaction (P2 Task 8) ───────────────

    /// One user turn "do it" with tool groups: two old `read_file` groups
    /// with `old_tokens`-token results, then three small recent groups.
    fn push_tool_turn(ctx: &mut ReasoningContext, old_tokens: usize) {
        ctx.messages.push(ChatMessage::user("do it"));
        for (n, tokens) in [(1, old_tokens), (2, old_tokens), (3, 50), (4, 50), (5, 50)] {
            let id = format!("g{n}");
            ctx.messages.push(ChatMessage::assistant_with_tool_use(
                &id,
                "read_file",
                serde_json::json!({"path": format!("src/{n}.rs")}),
            ));
            ctx.messages.push(ChatMessage::user_tool_result(&id, &"a".repeat(tokens * 4), false));
        }
    }

    fn result_of<'a>(msgs: &'a [ChatMessage], id: &str) -> &'a str {
        msgs.iter()
            .flat_map(|m| m.content.iter())
            .find_map(|b| match b {
                ContentBlock::ToolResult { tool_use_id, content, .. } if tool_use_id == id => Some(content.as_str()),
                _ => None,
            })
            .unwrap()
    }

    const CLEARED: &str = crate::agent::tool_result_clearing::CLEARED_PREFIX;

    /// ~36K request over the 32K threshold; clearing the two old groups frees
    /// ~36K → under the threshold: no session_block, one persist_cleared.
    #[tokio::test]
    async fn clearing_avoids_compaction_when_enough_is_freed() {
        let delegate = SinkDelegate::new();
        let mut ctx = ReasoningContext::new("sys".into());
        push_tool_turn(&mut ctx, 18_000);

        let outcome = run_agentic_loop(&delegate, &mut ctx, &loop_config(0)).await;
        assert!(matches!(outcome, LoopOutcome::Response { .. }), "{outcome:?}");

        let calls = delegate.cleared_calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1, "persist_cleared called once");
        assert_eq!(calls[0].0, vec!["g1", "g2"]);
        assert!(calls[0].1 > 35_000, "freed {}", calls[0].1);
        assert_eq!(delegate.replace_calls.load(Ordering::SeqCst), 0, "no compaction");
        assert_eq!(delegate.summarize_calls.load(Ordering::SeqCst) + delegate.prefix_calls.load(Ordering::SeqCst), 0);
        assert!(delegate.active().iter().all(|r| r.kind == TranscriptKind::Message), "no session_block row");

        let visible = delegate.visible_at_llm_call.lock().unwrap()[0].clone();
        assert!(result_of(&visible, "g1").starts_with(CLEARED));
        assert!(result_of(&visible, "g2").starts_with(CLEARED));
        assert!(!result_of(&visible, "g3").starts_with(CLEARED), "recent groups kept");
        assert_eq!(
            transcript::cleared_tool_call_ids(&delegate.sink.db.lock().unwrap(), "s1").unwrap(),
            vec!["g1", "g2"]
        );
    }

    /// Clearing frees ~24K but three ~15K-token turns keep the request over
    /// the threshold → clear first, then compact.
    #[tokio::test]
    async fn clearing_then_compaction_when_still_over() {
        let delegate = SinkDelegate::new();
        let mut ctx = ReasoningContext::new("sys".into());
        for i in 0..3 {
            ctx.messages.push(big_user(15_000));
            ctx.messages.push(ChatMessage::assistant(&format!("reply {i}")));
        }
        push_tool_turn(&mut ctx, 12_000);

        let _ = run_agentic_loop(&delegate, &mut ctx, &loop_config(0)).await;

        assert_eq!(delegate.cleared_calls.lock().unwrap().len(), 1, "cleared first");
        assert_eq!(delegate.replace_calls.load(Ordering::SeqCst), 1, "still over → compacted");
        let visible = delegate.visible_at_llm_call.lock().unwrap()[0].clone();
        assert!(is_session_block(&visible[0]));
        assert!(result_of(&visible, "g1").starts_with(CLEARED), "kept tail carries the placeholder");
        // The re-appended tail rows keep the overlay: transcript == context.
        let rows: Vec<ChatMessage> = delegate.active().into_iter().map(|r| r.message).collect();
        assert_eq!(json(&rows[..visible.len()]), json(&visible));
    }

    /// After clearing, the next turn's history (rebuilt from the transcript)
    /// starts with exactly what the clearing turn sent.
    #[tokio::test]
    async fn cleared_history_is_stable_prefix_next_turn() {
        let delegate = SinkDelegate::new();
        let mut ctx = ReasoningContext::new("sys".into());
        push_tool_turn(&mut ctx, 18_000);

        let _ = run_agentic_loop(&delegate, &mut ctx, &loop_config(0)).await;
        assert_eq!(delegate.cleared_calls.lock().unwrap().len(), 1);

        let sent = delegate.visible_at_llm_call.lock().unwrap()[0].clone();
        let (next, persisted) =
            crate::agent::transcript_history::build_agent_history(&delegate.sink.db.lock().unwrap(), "s1").unwrap();
        assert!(next.len() > sent.len() && persisted == next.len());
        assert_eq!(json(&next[..sent.len()]), json(&sent));
    }

    /// Provider overflow: clear first, then compact, then retry once.
    #[tokio::test]
    async fn context_overflow_clears_then_compacts() {
        let delegate = SinkDelegate::new();
        delegate.llm_errors.lock().unwrap().push(OVERFLOW.into());
        let mut ctx = ReasoningContext::new("sys".into());
        for i in 0..3 {
            ctx.messages.push(big_user(15_000));
            ctx.messages.push(ChatMessage::assistant(&format!("reply {i}")));
        }
        push_tool_turn(&mut ctx, 12_000);

        let outcome = run_agentic_loop(&delegate, &mut ctx, &overflow_config(true)).await;

        assert!(matches!(outcome, LoopOutcome::Response { .. }), "{outcome:?}");
        assert_eq!(delegate.cleared_calls.lock().unwrap().len(), 1);
        assert_eq!(delegate.replace_calls.load(Ordering::SeqCst), 1);
        assert_eq!(delegate.prefix_calls.load(Ordering::SeqCst), 0);
        let visible = delegate.visible_at_llm_call.lock().unwrap()[0].clone();
        assert!(result_of(&visible, "g1").starts_with(CLEARED));
    }
}
