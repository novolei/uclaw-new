//! The agent loop body for ChatDelegate.
//!
//! `impl LoopDelegate` is the contract the outer `run_loop` calls into:
//! check_signals → before_llm_call → create_turn_snapshot → call_llm →
//! handle_text_response → execute_tool_calls → on_usage. Each is one
//! step of one iteration of one turn.
//!
//! Also holds the lazy `tool_dispatcher()` builder and
//! `spawn_post_turn_extraction()` which are tightly coupled to the loop
//! body but too large to sit next to `new()` in mod.rs.

use std::sync::atomic::Ordering;

use async_trait::async_trait;

use super::ChatDelegate;
use crate::agent::types::{
    ChatMessage, ContentBlock, LoopOutcome, LoopSignal, MessageRole, ReasoningContext,
    RespondOutput, ResponseMetadata, TextAction, TokenUsage, ToolCall,
};
use crate::error::Error;

// ── Lazy ToolDispatcher + post-turn extraction ────────────────────────────────

impl ChatDelegate {
    /// Sprint 3 ① — lazily build + cache the `ToolDispatcher`.
    ///
    /// First call constructs it from the (by-now fully-configured) delegate
    /// fields; subsequent calls return the cached `Arc`. Built lazily because
    /// `infra_service` / `trajectory_store` / `tool_budget` are set via setters
    /// AFTER `new` — see the `tool_dispatcher` field doc. Arg order matches
    /// `ToolDispatcher::new`: tools, app_handle, safety_manager,
    /// pending_approvals, infra_service, trajectory_store, tool_budget, hook_bus.
    pub(super) fn tool_dispatcher(
        &self,
    ) -> &std::sync::Arc<crate::agent::tool_dispatch::ToolDispatcher<tauri::Wry>> {
        self.tool_dispatcher.get_or_init(|| {
            std::sync::Arc::new(crate::agent::tool_dispatch::ToolDispatcher::new(
                self.tools.clone(),
                self.app_handle.clone(),
                self.app_state().safety_manager.clone(),
                self.app_state().pending_approvals.clone(),
                self.infra_service.clone(),
                self.trajectory_store.clone(),
                self.tool_budget.clone(),
                self.hook_bus.clone(),
                self.heartbeat.clone(),
            ))
        })
    }

    /// Spawns a background task to perform unified memory/gbrain extraction after a turn completes.
    pub(super) fn spawn_post_turn_extraction(&self, reason_ctx: &ReasoningContext) {
        // Find the latest User-role message text
        let user_text = reason_ctx
            .messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, MessageRole::User))
            .and_then(|m| m.content.iter().find_map(|c| match c {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            }));

        let text = match user_text {
            Some(t) if !t.trim().is_empty() => t,
            _ => return,
        };

        let session_id = self.conversation_id.clone();
        let turn_id = format!(
            "{}-{}",
            session_id,
            self.turn_index.load(Ordering::Relaxed)
        );

        // Learning extractor
        if self.learning_enabled {
            if let Some(buffer) = self.learning_buffer.as_ref().cloned() {
                let llm = self.learning_llm.clone();
                let db = self.try_app_state().map(|s| s.db.clone());
                let daily_budget = self.learning_llm_daily_budget;
                let session_id_clone = session_id.clone();
                let turn_id_clone = turn_id.clone();
                let text_clone = text.clone();
                tokio::spawn(async move {
                    let llm_allowed = match (&db, llm.as_ref()) {
                        (Some(db_arc), Some(_)) if daily_budget > 0 => {
                            let spent = crate::cost_store::today_learning_tokens(db_arc);
                            spent < daily_budget
                        }
                        _ => false,
                    };
                    let llm_for_call = if llm_allowed { llm.as_ref() } else { None };
                    let n = crate::learning::extractor::extract_from_chat_turn(
                        &text_clone,
                        &session_id_clone,
                        &turn_id_clone,
                        &buffer,
                        llm_allowed,
                        llm_for_call,
                    )
                    .await;
                    if n > 0 {
                        tracing::debug!(
                            candidates = n,
                            "[ChatDelegate] learning::extractor pushed candidates"
                        );
                    }
                });
            }
        }

        // gbrain extractor
        if self.gbrain_extractor_enabled {
            let llm = self.gbrain_extract_llm.clone();
            let db = self.try_app_state().map(|s| s.db.clone());
            let mcp_mgr = self.app_state().mcp_manager.clone();
            let daily_budget = self.gbrain_extract_daily_budget;
            let llm_present = llm.is_some();
            if llm_present && db.is_some() && daily_budget > 0 {
                let text_clone = text.clone();
                tokio::spawn(async move {
                    let llm = match llm {
                        Some(l) => l,
                        None => return,
                    };
                    let db = match db {
                        Some(d) => d,
                        None => return,
                    };
                    let spent = crate::cost_store::today_gbrain_extract_tokens(&db);
                    if spent >= daily_budget {
                        tracing::debug!(
                            spent,
                            budget = daily_budget,
                            "[ChatDelegate] gbrain extractor — budget exhausted, skipping"
                        );
                        return;
                    }
                    let proposals = crate::gbrain::chat_extractor::extract_from_chat_turn(
                        &text_clone,
                        "",
                        &llm,
                    )
                    .await;
                    if proposals.is_empty() {
                        return;
                    }
                    let actionable: Vec<_> = proposals
                        .into_iter()
                        .filter(|p| {
                            p.confidence >= crate::gbrain::chat_extractor::MIN_ACT_CONFIDENCE
                        })
                        .collect();
                    tracing::debug!(
                        count = actionable.len(),
                        "[ChatDelegate] gbrain extractor — firing put_page calls"
                    );
                    for proposal in actionable {
                        let args = serde_json::json!({
                            "slug": proposal.slug,
                            "content": proposal.content,
                        });
                        let result = {
                            let mgr = mcp_mgr.read().await;
                            mgr.call_tool("gbrain", "put_page", args).await
                        };
                        match result {
                            Ok(result) if result.is_error => {
                                let text = result
                                    .content
                                    .iter()
                                    .filter_map(|block| match block {
                                        crate::mcp::ContentBlock::Text { text } => Some(text.as_str()),
                                        _ => None,
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                let mut mgr = mcp_mgr.write().await;
                                mgr.set_error("gbrain", Some(text));
                            }
                            Ok(_) => {
                                let mut mgr = mcp_mgr.write().await;
                                mgr.set_error("gbrain", None);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    slug = %proposal.slug,
                                    error = %e,
                                    "[ChatDelegate] gbrain extractor — put_page failed"
                                );
                                let mut mgr = mcp_mgr.write().await;
                                mgr.set_error("gbrain", Some(e.to_string()));
                            }
                        }
                    }
                });
            }
        }
    }
}

// ── Free helpers (only used by LoopDelegate / tests below) ────────────────────

/// Compute per-model `max_tokens` for a single LLM call.
///
/// - 1M-context models (Sonnet 4+, Opus 4.6+) get 32768 — room for large file output.
/// - Reasoning models (DeepSeek R1) get 24576 — thinking tokens eat visible output budget.
/// - Default: 16384.
fn compute_max_tokens(model: &str, thinking_enabled: bool) -> u32 {
    let m = model.to_lowercase();
    let base = if m.contains("sonnet-4") || m.contains("sonnet4")
        || m.contains("opus-4-6") || m.contains("opus-4-7")
        || m.contains("opus-4-8") || m.contains("opus-4-9")
        || m.contains("opus-5")
    {
        32768
    } else {
        16384
    };
    // Reasoning models: thinking tokens consume output budget — give extra headroom
    if thinking_enabled && m.contains("deepseek") && m.contains("r1") {
        (base as f32 * 1.5) as u32
    } else {
        base
    }
}

/// Check whether the agent's text response signals it was engaged in
/// plan-related work (as opposed to giving a complete answer to an
/// unrelated user question).
///
/// Used as a relevance gate before the plan-aware termination guard
/// fires. Without this check, a pending plan from a prior task would
/// hijack every user message — even "头好疼" — and force the agent to
/// abandon its answer and start executing old plan steps.
///
/// Returns true when:
/// - The text contains tool-intent signals ("let me write", "我来编辑", etc.)
/// - The text explicitly mentions plan / step / task concepts
/// - The text is short and contains continuation markers
fn text_signals_plan_work(text: &str) -> bool {
    // Tool intent: the agent was trying to work but didn't call tools.
    // This is the strongest signal — the agent announced intent to act.
    if crate::agent::types::llm_signals_tool_intent(text) {
        return true;
    }

    let lower = text.to_lowercase();

    // Explicit plan references: the agent is aware it's working through a plan.
    // Check both English and Chinese keywords. Chinese action verbs ("实现/添加/
    // 编写/完成") are added because Mandarin verbs sit where English would have
    // "implement / add / write / finish" — they're the strongest signal that
    // the agent narrated intent to act on a plan step.
    let plan_keywords = [
        "plan", "step", "task", "todo",
        "计划", "步骤", "任务", "待办",
        "实现", "添加", "编写", "完成",
        "升级", "优化", "修改", "调整",
        "设计", "美化", "修复", "解决",
        "创建", "写入",
        "upgrade", "optimize", "modify", "adjust",
        "design", "beautify", "fix", "resolve",
        "create", "write", "implement",
    ];
    if plan_keywords.iter().any(|kw| lower.contains(kw) || text.contains(kw)) {
        return true;
    }

    // Short responses that mention continuing work are likely incomplete.
    // A complete answer to an unrelated question is typically longer and
    // self-contained. Short plan-related snippets like "Working on it..."
    // or "Let me check the next step" should still trigger the guard.
    //
    // Chinese "now-starting" markers ("现在/目前/马上/即将/开始/下面") cover what
    // "正在" alone doesn't — semantically the model uses them interchangeably
    // for "about to do X" but "正在" only matches "currently doing X".
    if text.len() < 200 {
        let continuation_markers = [
            "continue", "continuing", "next", "working on",
            "继续", "接下来", "下一步", "正在",
            "现在", "目前", "马上", "即将", "开始", "下面",
        ];
        if continuation_markers.iter().any(|m| lower.contains(m) || text.contains(m)) {
            return true;
        }
    }

    false
}

/// Scan session message history for the most recently referenced plan
/// filename. Used by the plan-guard to anchor on the session's actual
/// active plan rather than relying on the 5-minute mtime window — which
/// silently misses resume workflows (user comes back after an hour and
/// types "继续").
///
/// Recognises two shapes:
///   - `plan_update` tool_use: take `input.filename` directly.
///   - `plan_write` tool_use: pair with the matching ToolResult and
///     parse the canonical "Plan created at .../<filename>" text. Failed
///     (is_error=true) results are ignored so a botched plan_write
///     doesn't lock the guard onto a non-existent file.
///
/// Returns the filename from the LATEST plan-tool reference in the
/// history (forward walk, last-write-wins).
fn extract_active_plan_from_history(
    messages: &[crate::agent::types::ChatMessage],
) -> Option<String> {
    use crate::agent::types::ContentBlock;
    use std::collections::HashMap;
    // Pending plan_write ids -> nothing yet (we'll fill in filename from
    // result). We need this because the result message comes after the
    // ToolUse in the history.
    let mut pending_writes: HashMap<String, ()> = HashMap::new();
    let mut latest: Option<String> = None;

    for msg in messages {
        for block in &msg.content {
            match block {
                ContentBlock::ToolUse { id, name, input } => {
                    if name == "plan_update" {
                        if let Some(f) = input
                            .get("filename")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                        {
                            latest = Some(f.to_string());
                        }
                    } else if name == "plan_write" {
                        pending_writes.insert(id.clone(), ());
                    }
                }
                ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                    if !pending_writes.contains_key(tool_use_id) {
                        continue;
                    }
                    // Always consume the pending entry so a later unrelated
                    // ToolResult with the same id (shouldn't happen, but be safe)
                    // doesn't get reprocessed.
                    pending_writes.remove(tool_use_id);
                    if is_error.unwrap_or(false) {
                        continue;
                    }
                    if let Some(f) = parse_plan_write_result_filename(content) {
                        latest = Some(f);
                    }
                }
                _ => {}
            }
        }
    }
    latest
}

/// Parse `Plan created at /path/to/.uclaw/plans/<filename>.md` for the
/// trailing basename. Returns None if the canonical prefix is absent or
/// the path component looks unsafe (contains `..` or path separators
/// after the basename — defensive only, the producer never emits these).
fn parse_plan_write_result_filename(result_text: &str) -> Option<String> {
    let prefix = "Plan created at ";
    let path_str = result_text.find(prefix).map(|i| &result_text[i + prefix.len()..])?;
    // Trim at first newline / whitespace separator in case the message
    // continues after the path.
    let path_str = path_str.lines().next().unwrap_or(path_str).trim();
    let filename = std::path::Path::new(path_str)
        .file_name()
        .and_then(|n| n.to_str())?
        .to_string();
    if filename.is_empty() || filename.contains("..") {
        return None;
    }
    Some(filename)
}

/// Fallback heuristic for the plan guard when keyword matching misses.
///
/// Shape: a large output_tokens budget combined with a tiny text body means
/// the model spent its tokens on thinking / tool composition but emitted
/// only a transition stub — almost always plan continuation that forgot a
/// `tool_use` block. Observed in production (2026-05-18 gomoku session,
/// 14 chars / 1722 tokens).
///
/// The thresholds are deliberately conservative: 800 tokens excludes normal
/// short replies, 100 chars excludes complete answers that just happened to
/// be terse. Together they describe the specific failure mode without
/// hijacking healthy conversations.
pub(super) fn signals_truncated_plan_continuation(text_len: usize, output_tokens: u32) -> bool {
    output_tokens > 800 && text_len < 100
}

const BROWSER_TASK_TOOL_NAME: &str = "browser_task";
const BROWSER_TASK_RUNTIME_PATCH_SNAKE: &str = "browser_task_request_patch";
const BROWSER_TASK_RUNTIME_PATCH_CAMEL: &str = "browserTaskRequestPatch";
const BROWSER_TASK_RUNTIME_DECISION_SNAKE: &str = "runtime_preparation_decision";
const BROWSER_TASK_RUNTIME_DECISION_CAMEL: &str = "runtimePreparationDecision";

fn prepare_tool_call_for_dispatch(mut tool_call: ToolCall) -> ToolCall {
    if tool_call.name == BROWSER_TASK_TOOL_NAME {
        tool_call.arguments = apply_browser_task_runtime_dispatch_patch(tool_call.arguments);
    }
    tool_call
}

fn apply_browser_task_runtime_dispatch_patch(arguments: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(mut args) = arguments else {
        return arguments;
    };

    let patch = args
        .remove(BROWSER_TASK_RUNTIME_PATCH_SNAKE)
        .or_else(|| args.remove(BROWSER_TASK_RUNTIME_PATCH_CAMEL));

    if !args.contains_key(BROWSER_TASK_RUNTIME_DECISION_SNAKE) {
        if let Some(decision) = patch
            .as_ref()
            .and_then(runtime_decision_from_browser_task_patch)
        {
            args.insert(
                BROWSER_TASK_RUNTIME_DECISION_SNAKE.to_string(),
                serde_json::Value::String(decision.to_string()),
            );
        }
    }

    serde_json::Value::Object(args)
}

fn runtime_decision_from_browser_task_patch(patch: &serde_json::Value) -> Option<&str> {
    patch
        .get(BROWSER_TASK_RUNTIME_DECISION_SNAKE)
        .or_else(|| patch.get(BROWSER_TASK_RUNTIME_DECISION_CAMEL))
        .and_then(serde_json::Value::as_str)
}

// ── impl LoopDelegate for ChatDelegate ───────────────────────────────────────

#[async_trait]
impl crate::agent::types::LoopDelegate for ChatDelegate {
    async fn check_signals(&self) -> LoopSignal {
        if self.stop_flag.load(Ordering::Relaxed) {
            self.stop_flag.store(false, Ordering::Relaxed);
            return LoopSignal::Stop;
        }
        LoopSignal::Continue
    }

    async fn before_llm_call(&self, _reason_ctx: &mut ReasoningContext, _iteration: usize) -> Option<LoopOutcome> {
        None
    }

    /// Pi convergence Sprint 2 — assemble the per-turn immutable snapshot.
    /// Moves the system-prompt assembly (mode + effective_system_prompt + GEP
    /// + plan-suggest + project rules + ladder pad) and the tool assembly
    /// (list_definitions + L2 normalization) that `call_llm` used to perform
    /// per-call. Built once per loop iteration = the same frequency as before.
    async fn create_turn_snapshot(
        &self,
        reason_ctx: &ReasoningContext,
        turn_index: u32,
    ) -> crate::agent::turn::TurnSnapshot {
        // Resolve mode once (per-session override > global policy) so the
        // system prompt actually reflects the user's chosen mode. Without
        // this, dispatcher.safety_mode is None for normal sessions and the
        // composer falls through to Supervised default — meaning Plan/Ask/
        // Bypass/AcceptEdits prompt additions would never reach the LLM,
        // and the agent never learns it should call exit_plan_mode etc.
        let effective_mode = self.resolve_effective_mode().await;
        let effective_prompt = self.effective_system_prompt(&effective_mode);

        // System prompt is kept stable (no per-iteration time injection) so
        // Anthropic prompt cache can hit from iteration 2 onward. Time is now
        // injected into the last user message via build_dynamic_context().
        let mut full_system_prompt = effective_prompt;

        // Inject matched GEP Genes as control signals
        if let Some(ref retriever) = self.gene_retriever {
            let last_user_text = reason_ctx.messages.iter().rev()
                .find(|m| matches!(m.role, crate::agent::types::MessageRole::User))
                .and_then(|m| m.content.iter().find_map(|b| {
                    if let crate::agent::types::ContentBlock::Text { text } = b {
                        Some(text.as_str())
                    } else {
                        None
                    }
                }))
                .unwrap_or("");

            if !last_user_text.is_empty() {
                let tool_errors: Vec<String> = self.recent_tool_errors.lock()
                    .map(|e| e.clone())
                    .unwrap_or_default();
                let matches = retriever.match_genes(last_user_text, &tool_errors, 2).await;
                if !matches.is_empty() {
                    let gene_block = crate::agent::gep::retrieval::format_gene_injection(&matches, 2);
                    if !gene_block.is_empty() {
                        full_system_prompt.push_str(&gene_block);
                        tracing::debug!(
                            gene_count = matches.len(),
                            "[ChatDelegate] Injected Gene control signals into system prompt"
                        );
                    }
                    // Store matches for Capsule generation after tool execution
                    if let Ok(mut stored) = self.last_gene_matches.lock() {
                        *stored = matches;
                    }
                }
            }
        }

        // Aggregate plan-suggest accept-rate signal — when most suggestions
        // are being rejected, ask the model to be more conservative about
        // calling request_plan_mode_switch.
        {
            let plan_suggest_hint: Option<String> = self.try_app_state().and_then(|state| {
                let db = state.db.clone();
                drop(state);
                let conn = db.lock().ok()?;
                let window_start = chrono::Utc::now().timestamp_millis() - 7 * 24 * 60 * 60 * 1000;
                let stats = crate::agent::mode_suggest_store::query_per_pattern_stats(&conn, window_start).ok()?;
                let total_decided: u32 = stats.iter().map(|s| s.accepted + s.skipped + s.silenced).sum();
                let total_accepted: u32 = stats.iter().map(|s| s.accepted).sum();
                if total_decided >= 10 {
                    let agg_rate = total_accepted as f32 / total_decided as f32;
                    if agg_rate < 0.20 {
                        tracing::debug!(
                            agg_rate = agg_rate,
                            total_decided = total_decided,
                            "Plan-suggest aggregate hint injected into system prompt",
                        );
                        return Some(
                            "\n\n[Plan-suggest signal] Your recent request_plan_mode_switch \
                             calls have been declined frequently. Be more conservative — \
                             only suggest Plan mode for clearly multi-step build/refactor \
                             work, not casual questions.\n"
                                .to_string(),
                        );
                    }
                }
                None
            });
            if let Some(hint) = plan_suggest_hint {
                full_system_prompt.push_str(&hint);
            }
        }

        // Phase 3 Step 3.5: Dynamic Project-Rule Condensation
        let active_files = crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER.get_active_files();
        let project_rules = if let Some(ref root) = self.workspace_root {
            crate::agent::rule_context_builder::RuleContextBuilder::build_context(root, &active_files)
        } else {
            String::new()
        };
        if !project_rules.is_empty() {
            full_system_prompt.push_str(&project_rules);
        }

        // Phase 4 Step 4.2: 5-Tier Prompt Ladder alignment
        let full_system_prompt = crate::agent::compact::cache_align::pad_to_ladder(&full_system_prompt);

        let tools = if reason_ctx.force_text {
            Vec::new()
        } else {
            let mut defs = self.tools.list_definitions();
            // M2-H L2 — normalize tool schemas before announcing to LLM:
            //   * drop description.examples (saves ~hundreds of tokens per tool)
            //   * dedupe enum arrays
            //   * depth-pruning: ENABLED with type-preserving truncation.
            //     Bundle 5 hotfix had to disable this with `usize::MAX` because
            //     the original implementation replaced both Object and Array
            //     deep-nests with `{truncated, original_depth}` Object markers,
            //     breaking JSON-schema type invariants on tools that had deep
            //     arrays (DeepSeek/OpenAI strict validators 400'd with "is not
            //     of type 'array'"). The redesign (task #113, this change)
            //     replaces a deep-nested Object with `{}` and a deep-nested
            //     Array with `[]` — type preserved, content gone. Strict
            //     validators no longer reject because the keyword's value
            //     keeps the JSON type it had before truncation.
            // Idempotent + non-mutating; running on already-normalized schemas
            // is a no-op so re-invocation across loop iterations is cheap.
            let mut total_stats = crate::agent::tool_shaping::normalize::NormalizeStats::default();
            for def in defs.iter_mut() {
                let raw = std::mem::replace(&mut def.parameters, serde_json::Value::Null);
                let (rewritten, stats) = crate::agent::tool_shaping::normalize::normalize_tool_schema(
                    raw,
                    crate::agent::tool_shaping::normalize::DEFAULT_MAX_NESTING_DEPTH,
                );
                def.parameters = rewritten;
                total_stats.examples_dropped += stats.examples_dropped;
                total_stats.enums_deduped += stats.enums_deduped;
                total_stats.deep_nests_pruned += stats.deep_nests_pruned;
            }
            if !total_stats.is_noop() {
                tracing::info!(
                    examples_dropped = total_stats.examples_dropped,
                    enums_deduped = total_stats.enums_deduped,
                    deep_nests_pruned = total_stats.deep_nests_pruned,
                    tool_count = defs.len(),
                    "[L2] normalized tool schemas",
                );
            } else {
                // NOOP heartbeat — keeps the trace queryable per-turn so users
                // can confirm L2 actually ran even when schemas were already
                // clean. INFO would be too chatty (every turn); DEBUG is the
                // right level — surfaces under `RUST_LOG=uclaw_core::agent=debug`.
                tracing::debug!(
                    tool_count = defs.len(),
                    "[L2] normalize: noop (schemas already clean)",
                );
            }
            // Track tool list hash to detect unexpected changes mid-turn (e.g.
            // MCP reconnect during a multi-iteration turn). Anthropic's prompt
            // cache covers the tool list when it's byte-stable across iterations;
            // this logging surfaces cache-busting events for observability.
            // Hashed AFTER normalization so the cache key reflects the actual
            // payload the LLM sees, not the raw registry shape.
            let hash: u64 = {
                use std::hash::{Hash, Hasher};
                use std::collections::hash_map::DefaultHasher;
                let mut h = DefaultHasher::new();
                for d in &defs { d.name.hash(&mut h); }
                h.finish()
            };
            if let Ok(mut prev) = self.last_tool_defs_hash.lock() {
                if let Some(p) = *prev {
                    if p != hash {
                        tracing::warn!(
                            prev_hash = p, new_hash = hash,
                            "Tool list changed mid-turn — Anthropic cache miss likely"
                        );
                    }
                }
                *prev = Some(hash);
            }
            defs
        };

        crate::agent::turn::TurnSnapshot {
            turn_index,
            model: self.model.clone(),
            system_prompt: std::sync::Arc::new(full_system_prompt),
            tools: std::sync::Arc::new(tools),
            force_text: reason_ctx.force_text,
        }
    }

    async fn call_llm(
        &self,
        reason_ctx: &mut ReasoningContext,
        snapshot: &crate::agent::turn::TurnSnapshot,
        _iteration: usize,
    ) -> Result<RespondOutput, Error> {
        // Reset sequence counters on every new LLM call to ensure deduplication
        // and streaming state resets work correctly on the frontend for multi-iteration turns.
        self.chunk_seq.store(0, Ordering::Relaxed);
        self.thinking_seq.store(0, Ordering::Relaxed);

        // Bundle 27-A fix (2026-05-22) — beat LLM_CALL at the top so the UI
        // heartbeat indicator switches to "正在请求 LLM" immediately on the
        // iteration boundary. Before this, the indicator stayed at the
        // initial "starting" / "准备中" stage during prompt assembly +
        // memory recall + first-byte wait, which felt unresponsive to the
        // user even though the agent was working.
        self.beat(crate::agent::heartbeat::stages::LLM_CALL);

        // System-prompt + tool assembly now lives in create_turn_snapshot
        // (built once per loop iteration). Consume the frozen snapshot here.
        let full_system_prompt = (*snapshot.system_prompt).clone();

        let mut messages = vec![ChatMessage::system(&full_system_prompt)];
        // Skip compacted messages — they stay in memory for UI replay
        // but must not consume LLM context budget. (P1 logical-marking)
        messages.extend(reason_ctx.messages.iter().filter(|m| !m.compacted).cloned());

        // M2-H L6 — orphan tool-call defense.
        // If a previous turn was cancelled mid-call (M1-T2d), a tool handler
        // crashed, or context compaction (M2-G) dropped a tool_result while
        // keeping its tool_use, the history can contain `ToolUse` blocks
        // without a matching `ToolResult`. Anthropic/OpenAI both 400 on that
        // shape. Splice a synthetic ToolResult with "aborted" content so
        // the request is well-formed and the model sees the orphan.
        //
        // The system message stays at index 0 — we only audit the
        // conversation portion to keep the algorithm focused.
        let conversation: Vec<ChatMessage> = messages.drain(1..).collect();
        let (patched, audit_stats) =
            crate::agent::call_audit::audit_chat_history(conversation);
        messages.extend(patched);
        if !audit_stats.is_clean() {
            tracing::warn!(
                orphans = audit_stats.orphans_synthesized,
                first_call_id = %audit_stats
                    .orphan_calls
                    .first()
                    .map(|o| o.call_id.as_str())
                    .unwrap_or(""),
                "[L6] synthesized aborted ToolResults for orphan tool calls",
            );
        }

        // M2-H L5 — image stripping for image-blind providers.
        // browser_screenshot tool results are JSON-encoded blobs containing
        // base64 image data. The Anthropic adapter (anthropic.rs:152-168)
        // re-encodes those into vision content blocks when serializing —
        // but on image-blind models (DeepSeek, gpt-3.5-turbo, plain Llama,
        // etc.) this either 400s or wastes ~MB of tokens on opaque base64.
        //
        // Detect image-blind (provider, model) up front and rewrite each
        // browser_screenshot-shaped tool result to the placeholder text
        // BEFORE the provider sees it. Bytes saved per stripped screenshot
        // can run into millions; this also keeps the conversation's UI
        // replay clean (the placeholder is human-readable).
        if !crate::agent::image_policy::supports_images(&self.provider, &self.model) {
            let mut stripped = 0_usize;
            for m in messages.iter_mut().skip(1 /* system */) {
                for block in m.content.iter_mut() {
                    if let ContentBlock::ToolResult { content, .. } = block {
                        // Recognize the browser_screenshot wire shape:
                        // {"ok":true,"data":"<base64>","width":N,...}
                        let is_screenshot = serde_json::from_str::<serde_json::Value>(content)
                            .ok()
                            .as_ref()
                            .map(|v| {
                                v.get("ok").and_then(|x| x.as_bool()) == Some(true)
                                    && v.get("data").and_then(|x| x.as_str()).is_some()
                                    && v.get("width").is_some()
                            })
                            .unwrap_or(false);
                        if is_screenshot {
                            *content = crate::agent::image_policy::DEFAULT_PLACEHOLDER
                                .to_string();
                            stripped += 1;
                        }
                    }
                }
            }
            if stripped > 0 {
                tracing::info!(
                    stripped,
                    provider = %self.provider,
                    model = %self.model,
                    "[L5] stripped browser_screenshot images for image-blind model",
                );
            }
        }

        // Prepend dynamic context (workspace root) to the LAST user
        // message in this call. Mutates the clone only — the persisted
        // session messages stay clean so context isn't duplicated when the
        // session resumes or replays.
        //
        // We attach to the LAST user message (not a new message at the end)
        // because a fresh "user" message after assistant tool calls would
        // confuse the back-and-forth structure. Anthropic / OpenAI both
        // require alternating user/assistant turns.
        //
        // Time + workspace root are both injected here via build_dynamic_context().
        // Keeping time out of the system prompt preserves byte-stability for caching.
        if let Some(last_user_idx) = messages.iter().rposition(|m| {
            matches!(m.role, crate::agent::types::MessageRole::User)
                && m.content.iter().any(|b| matches!(b, crate::agent::types::ContentBlock::Text { .. }))
        }) {
            let dyn_ctx = self.build_dynamic_context();
            if !dyn_ctx.is_empty() {
                if let Some(crate::agent::types::ContentBlock::Text { text }) =
                    messages[last_user_idx].content.iter_mut().find(|b| {
                        matches!(b, crate::agent::types::ContentBlock::Text { .. })
                    })
                {
                    *text = format!("{}\n\n{}", dyn_ctx, text);
                }
            }
        }

        // Tools were assembled (list_definitions + L2 normalization + hash
        // tracking) in create_turn_snapshot; consume the frozen list here.
        let tools = (*snapshot.tools).clone();
        let max_tokens = compute_max_tokens(&snapshot.model, self.thinking_enabled);
        let config = crate::llm::CompletionConfig {
            model: snapshot.model.clone(),
            max_tokens,
            temperature: 0.7,
            thinking_enabled: self.thinking_enabled,
        };

        // Token cost breakdown — helps diagnose context bloat.
        let sys_tok = crate::agent::types::estimate_tokens(&full_system_prompt);
        let tool_tok: u32 = tools.iter().map(|t| {
            crate::agent::types::estimate_tokens(&t.name) + crate::agent::types::estimate_tokens(&t.description) + crate::agent::types::estimate_tokens(&t.parameters.to_string())
        }).sum();
        let msg_tok: u32 = messages.iter().skip(1 /* skip system */).map(|m| {
            m.content.iter().map(|b| match b {
                ContentBlock::Text { text } => crate::agent::types::estimate_tokens(text),
                ContentBlock::ToolResult { content, .. } => crate::agent::types::estimate_tokens(content) + 5,
                ContentBlock::ToolUse { name, input, .. } => crate::agent::types::estimate_tokens(name) + crate::agent::types::estimate_tokens(&input.to_string()) + 10,
                _ => 5,
            }).sum::<u32>()
        }).sum();
        tracing::info!(
            model = %self.model,
            message_count = messages.len(),
            tool_count = tools.len(),
            force_text = reason_ctx.force_text,
            max_tokens,
            system_prompt_tokens = sys_tok,
            tool_def_tokens = tool_tok,
            message_tokens = msg_tok,
            estimated_total = sys_tok + tool_tok + msg_tok,
            "Calling LLM"
        );

        // Sprint 3 ② Task 5 — PreLlmCall hook (observe-only).
        self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PreLlmCall {
            task_id: self.conversation_id.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            prompt_tokens_estimate: (sys_tok + tool_tok + msg_tok) as usize,
        }).await;

        // Bundle 27-B (settings exposure) — resolve the stream-idle
        // timeout from MemubotConfig on every call_llm so the user can
        // adjust the value in Settings → System and have it apply to
        // the very next message without restarting the session.
        let stream_idle_timeout = {
            use tauri::Manager;
            let app_state = self.app_handle.state::<crate::app::AppState>();
            let cfg = app_state.memubot_config.read().await;
            std::time::Duration::from_secs(cfg.stream_idle_timeout_secs)
        };

        crate::agent::llm_stream::stream_completion(
            self.llm.as_ref(),
            messages,
            tools,
            &config,
            self,
            stream_idle_timeout,
            reason_ctx.cancellation_token.as_ref(),
        )
        .await
    }

    async fn handle_text_response(
        &self,
        text: &str,
        metadata: ResponseMetadata,
        reason_ctx: &mut ReasoningContext,
    ) -> TextAction {
        let is_truncated = metadata.finish_reason.as_deref() == Some("length");

        // Build the effective rescue text. If we have a partial code block
        // accumulated from previous truncated responses, reconstruct the fence
        // so parse_code_blocks can see the full (possibly-now-complete) block.
        let effective_owned;
        let effective_text: &str = match &reason_ctx.partial_code_buffer {
            Some((lang, acc)) => {
                effective_owned = format!("```{}\n{}{}", lang, acc, text);
                &effective_owned
            }
            None => text,
        };

        if is_truncated {
            reason_ctx.consecutive_length_truncations += 1;
            let n = reason_ctx.consecutive_length_truncations;
            tracing::warn!(
                text_len = text.len(),
                consecutive = n,
                "Text response hit length limit (finish_reason=length)"
            );

            // Even with truncation, the accumulated+current text might now
            // form a complete code block — try rescue first.
            // `_safe` wrapper: code-rescue is best-effort fallback; panicking
            // here (we hit a CJK UTF-8 boundary bug previously) would kill the
            // whole agentic_loop task and freeze the UI. catch_unwind here means
            // "no rescue this turn", loop continues.
            let rescue_calls = crate::agent::code_rescue::extract_write_file_calls_safe(
                effective_text,
                self.workspace_root.as_deref(),
            );
            if !rescue_calls.is_empty() {
                tracing::info!(
                    count = rescue_calls.len(),
                    "Code-block rescue succeeded on accumulated partial text"
                );
                reason_ctx.partial_code_buffer = None;
                reason_ctx.consecutive_length_truncations = 0;
                return TextAction::RescueWithToolCalls(rescue_calls);
            }

            // No complete block yet — update accumulator so the next iteration
            // can try again with more content.
            if let Some((_, ref mut acc)) = reason_ctx.partial_code_buffer {
                acc.push_str(text);
            } else if let Some((lang, partial)) =
                crate::agent::code_rescue::extract_partial_code_block_safe(text)
            {
                reason_ctx.partial_code_buffer = Some((lang, partial));
            }

            // After 2+ truncations, escalate from generic nudge to explicit
            // chunked-writing strategy — the file is too large for one shot.
            let nudge = if n >= 2 {
                format!(
                    "Your response has been truncated {n} times in a row. \
                     The file you are writing is too large to output as text. \
                     STOP outputting text immediately. \
                     Strategy: call write_file with the FIRST 250-300 lines as \
                     the `content` argument. In the following turn, call \
                     write_file again (or edit) for the remaining sections. \
                     Do not output any code as text — call write_file NOW."
                )
            } else {
                "Your last reply was truncated by the token limit. \
                 If you were writing a file, call write_file NOW instead of \
                 outputting the content as text. \
                 If you were mid-sentence on something else, continue from \
                 where you left off."
                    .to_string()
            };
            return TextAction::ContinueWithNudge(nudge);
        }

        // ── Complete response path ────────────────────────────────────────
        // Save truncation state before resetting (any LLM call in this turn
        // was truncated → inform frontend via stream-complete and LoopOutcome).
        let was_truncated = reason_ctx.consecutive_length_truncations > 0;
        reason_ctx.consecutive_length_truncations = 0;

        // Code-block rescue: try with accumulated+current text (clears buffer
        // regardless of outcome — the response is done).
        let rescue_calls = crate::agent::code_rescue::extract_write_file_calls_safe(
            effective_text,
            self.workspace_root.as_deref(),
        );
        reason_ctx.partial_code_buffer = None;
        if !rescue_calls.is_empty() {
            tracing::warn!(
                count = rescue_calls.len(),
                "Code-block rescue triggered: model output code as text, converting to write_file calls"
            );
            return TextAction::RescueWithToolCalls(rescue_calls);
        }

        // Plan-aware termination guard: if a plan file still has undone steps
        // modified in the last 5 minutes, the model hasn't actually finished —
        // nudge it to continue rather than terminating.
        //
        // Safety gate: only fire when the agent's response indicates it was
        // engaged in plan-related work. If the user asked an unrelated question
        // (e.g. "头好疼") and the agent gave a complete answer, the plan guard
        // must NOT hijack the conversation. Without this gate, pending plan
        // steps from a prior task would cause the agent to abandon its answer
        // and start executing old plan steps — a severe UX regression.
        //
        // Cap: after MAX_PLAN_GUARD_NUDGES consecutive nudges without any tool
        // call response, give up and let the response through to avoid infinite
        // text-only loops (e.g. model did edits but forgot plan_update calls).
        // Discover the active plan two ways, in priority order:
        //   1. Scan message history for plan_write / plan_update calls THIS
        //      session made — the authoritative answer regardless of mtime.
        //      Fixes resume workflows where the plan file is hours old (the
        //      2026-05-18 04:46 五子棋 "继续" silent termination was this case).
        //   2. Fall back to mtime-based discovery (300s window) for truly
        //      fresh sessions or external plan creation that left no trace
        //      in this session's history.
        let undone_opt = match extract_active_plan_from_history(&reason_ctx.messages) {
            Some(filename) => crate::agent::plan_state::pending_plan_steps_in_file(
                self.workspace_root.as_deref(),
                &filename,
            ),
            None => crate::agent::plan_state::pending_plan_steps(
                self.workspace_root.as_deref(),
                300,
            ),
        };
        if let Some(undone) = undone_opt {
            // ── Relevance gate: only nudge if the response signals plan work ──
            // We check whether the agent's own text indicates it was in the
            // middle of plan execution, rather than giving a complete answer to
            // an unrelated user question. This prevents the guard from hijacking
            // conversations where the user asked about something completely
            // different (health, weather, general knowledge, etc.).
            //
            // Fallback (signals_truncated_plan_continuation): when the keyword
            // gate misses but the response shape screams "I composed a lot of
            // thinking and emitted only a transition stub" (large output_tokens
            // + tiny text), nudge anyway. This catches Chinese phrasings the
            // keyword list will never cover.
            let output_tokens = metadata.usage.as_ref().map(|u| u.output_tokens).unwrap_or(0);
            let signals_work = text_signals_plan_work(text)
                || signals_truncated_plan_continuation(text.len(), output_tokens);
            if !signals_work {
                // Upgraded from debug to warn: a skipped guard with undone
                // steps is the silent-termination smoking gun (2026-05-18
                // gomoku session). Logging at default-visible level lets
                // post-mortems happen without a debug build.
                tracing::warn!(
                    undone_steps = undone,
                    output_tokens,
                    text_len = text.len(),
                    text_preview = %&text[..text.len().min(120)],
                    "Plan guard skipped: response does not signal plan-related work"
                );
                // Fall through to emit_done / Return below — let the complete
                // answer reach the user without hijacking the conversation.
            } else if reason_ctx.consecutive_plan_guard_nudges >= crate::agent::types::MAX_PLAN_GUARD_NUDGES {
                tracing::warn!(
                    undone_steps = undone,
                    nudges = reason_ctx.consecutive_plan_guard_nudges,
                    "Plan guard giving up after {} nudges without tool response — returning response as-is",
                    crate::agent::types::MAX_PLAN_GUARD_NUDGES
                );
                // Fall through to emit_done / Return below
            } else {
                reason_ctx.consecutive_plan_guard_nudges += 1;
                tracing::info!(
                    undone_steps = undone,
                    nudge_count = reason_ctx.consecutive_plan_guard_nudges,
                    "Pending plan steps detected; injecting continuation instead of terminating"
                );
                return TextAction::ContinueWithNudge(format!(
                    "Your plan has {} undone step(s) marked `- [ ]`. \
                     Execute the next undone step RIGHT NOW by calling a tool — \
                     use write_file to write code, edit to modify an existing file, \
                     or bash to run a command. DO NOT output file content as text.",
                    undone
                ));
            }
        }

        self.spawn_post_turn_extraction(reason_ctx);
        self.emit_done(text, was_truncated);
        TextAction::Return(LoopOutcome::Response {
            text: text.to_string(),
            usage: metadata.usage,
            truncated: was_truncated,
            // M1-backlog #3 — real provider/model attribution into ModelTurn.
            model: Some(metadata.model.clone()),
        })
    }

    async fn execute_tool_calls(
        &self,
        tool_calls: Vec<ToolCall>,
        reason_ctx: &mut ReasoningContext,
    ) -> Result<Option<LoopOutcome>, Error> {
        // Sprint 3 ① cutover — prepare each call (browser_task runtime patch, etc.).
        let tool_calls = tool_calls
            .into_iter()
            .map(prepare_tool_call_for_dispatch)
            .collect::<Vec<_>>();

        // PR 2026-05-13 token-cost optim: once the agent reaches for
        // `skill_search` in this loop, the system-prompt manifest has
        // served its purpose (catalog discovery → tool delegation).
        // Mark the flag so `effective_system_prompt` skips the ~800-token
        // manifest block on subsequent calls. Sticky for the rest of the
        // loop; reset implicitly when the agent_loop spawns a fresh
        // ChatDelegate for the next user message.
        if tool_calls.iter().any(|tc| tc.name == "skill_search") {
            self.skill_search_used.store(true, Ordering::Relaxed);
        }

        // ── plan_update anti-fake-progress challenge pre-pass ─────────────
        // Intercept `plan_update done:true` calls that have neither a recent
        // mutating tool call NOR explicit evidence in `note`. Inject a
        // synthetic error tool_result + emit start/result and DROP the call
        // from the dispatch set. See agent/types.rs::FAKE_PROGRESS_CHALLENGE.
        // After MAX_MUTATION_CHALLENGES soft-blocks in this loop we let it
        // through (it stays in `dispatched_calls`) so a genuinely-completed
        // step doesn't loop forever. This is deeply reason_ctx-coupled, so it
        // runs as a pre-pass before dispatch — verbatim from the old code.
        // Challenged calls push their synthetic user_tool_result HERE, in
        // input order, before any dispatched-call result is pushed below;
        // this preserves the old per-call message order.
        let mut dispatched_calls: Vec<ToolCall> = Vec::with_capacity(tool_calls.len());
        for tc in tool_calls {
            if tc.name == "plan_update"
                && tc.arguments.get("done").and_then(|v| v.as_bool()).unwrap_or(false)
                && reason_ctx.mutations_since_last_plan_done == 0
                && reason_ctx.mutation_challenges_issued < crate::agent::types::MAX_MUTATION_CHALLENGES
            {
                // Treat a `note` of >= 20 chars as evidence: if the LLM
                // bothered to type a real explanation we let it through.
                let note_len = tc.arguments.get("note")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().len())
                    .unwrap_or(0);
                if note_len < 20 {
                    reason_ctx.mutation_challenges_issued += 1;
                    tracing::warn!(
                        tool = %tc.name,
                        challenges = reason_ctx.mutation_challenges_issued,
                        max = crate::agent::types::MAX_MUTATION_CHALLENGES,
                        step_index = ?tc.arguments.get("step_index"),
                        "Soft-blocking plan_update done:true with no mutation evidence"
                    );
                    // No preview_target for the soft-block path — the tool
                    // call is being short-circuited, no write will happen.
                    self.emit_tool_start(&tc.name, &tc.id, &tc.arguments, None);
                    self.emit_tool_result(
                        &tc.name,
                        &tc.id,
                        &crate::agent::tools::tool::ToolOutput::error(
                            crate::agent::types::FAKE_PROGRESS_CHALLENGE,
                            0,
                        ),
                    );
                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                        &tc.id,
                        crate::agent::types::FAKE_PROGRESS_CHALLENGE,
                        true,
                    ));
                    // Dropped from dispatch — do NOT push into dispatched_calls.
                    continue;
                }
                tracing::info!(
                    tool = %tc.name,
                    note_len,
                    "plan_update done:true accepted via `note` evidence"
                );
            }
            dispatched_calls.push(tc);
        }

        // ── Dispatch the surviving calls via ToolDispatcher ───────────────
        // The dispatcher owns: resolution, approval + path gating, execute +
        // bash streaming coalescer, serial/parallel split, budget truncation,
        // result/error emit, trajectory + infra recording, and observe-only
        // PreToolUse/PostToolUse hook fires. The per-outcome reason_ctx
        // bookkeeping below reproduces what the old inline code did AROUND
        // those dispatcher-owned side effects.
        //
        // `iteration`: the old serial/parallel paths used a per-call
        // `self.turn_index.fetch_add(1)` as the trajectory turn_index. The
        // dispatcher takes a single `iteration` per batch, so we advance the
        // same atomic once per execute_tool_calls and pass that value. The
        // monotonic counter stays monotonic; multi-call batches now share one
        // turn_index value (a non-asserted trajectory detail).
        let ctx = crate::agent::tool_dispatch::ToolDispatchContext {
            session_id: self.conversation_id.clone(),
            conversation_id: self.conversation_id.clone(),
            workspace_root: self.workspace_root.clone(),
            // The dispatcher re-derives attached dirs per call via
            // load_attached_dirs_for_session in gate_paths; the ctx field is
            // unused by the gating path, so an empty vec preserves behavior.
            attached_dirs: vec![],
            safety_mode: self.safety_mode.clone(),
            iteration: self.turn_index.fetch_add(1, Ordering::Relaxed) as usize,
            cancel: reason_ctx.cancellation_token.clone(),
            permissions: None,
            origin_kind: crate::agent::tool_dispatch::ApprovalOriginKind::Chat {
                conversation_id: self.conversation_id.clone(),
            },
        };
        let outcomes = self.tool_dispatcher().dispatch(dispatched_calls, &ctx).await;

        // ── Per-outcome reason_ctx bookkeeping ────────────────────────────
        // Order matches input order (the dispatcher preserves it). For each
        // outcome, reproduce the old per-call side effects that the dispatcher
        // does NOT do (it already emitted / recorded / hooked / gated):
        //   1. push the user_tool_result with the SAME budget-truncated
        //      content + is_error bit the old code pushed;
        //   2. on success (not soft-error) track file ops;
        //   3. on soft-error push recent_tool_errors "{name}: {text}" (cap 20);
        //      on hard error push recent_tool_errors "{name}: {e}" (cap 20);
        //   4. mutations / streak-reset / plan_update-reset bookkeeping;
        //   5. exit_plan_mode rejection → UserCorrection event.
        for o in &outcomes {
            // (1) user_tool_result — same content + is_error the dispatcher emitted.
            reason_ctx.messages.push(ChatMessage::user_tool_result(
                &o.tool_call_id,
                &o.message_content,
                o.is_error,
            ));

            // (2) file ops tracking on success (soft_error == false). Mirrors
            //     both the old serial path (gated on !soft_error) and the old
            //     parallel path (which tracked unconditionally for read-only
            //     tools — but those never soft-error, so !is_error is equivalent).
            if !o.is_error {
                reason_ctx.file_ops.track_tool_call(&o.tool_name, &o.arguments);
            }

            // (3) recent_tool_errors collection for GeneRetriever matching.
            //     Soft error: "{name}: {extracted-text}" (already truncate_utf8'd
            //     to 200 in the dispatcher). Hard error: "{name}: {e}" (raw
            //     ToolError display). Both capped at 20 like the old code.
            if let Some(text) = &o.soft_error {
                if let Ok(mut errors) = self.recent_tool_errors.lock() {
                    if errors.len() < 20 {
                        errors.push(format!("{}: {}", o.tool_name, text));
                    }
                }
            } else if let Err(e) = &o.result {
                if let Ok(mut errors) = self.recent_tool_errors.lock() {
                    if errors.len() < 20 {
                        errors.push(format!("{}: {}", o.tool_name, e));
                    }
                }
            }

            // (4) anti-fake-progress + streak bookkeeping.
            // The old success arm (`Ok(output)`) ran these for ANY non-Err
            // result (including soft errors): reset the truncation / plan-guard
            // streaks, increment mutations only when !soft_error && is_mutating,
            // and reset mutations/challenges on plan_update done:true.
            if o.result.is_ok() {
                // Any non-Err tool result ends the truncation + plan-guard streaks.
                reason_ctx.consecutive_length_truncations = 0;
                reason_ctx.partial_code_buffer = None;
                reason_ctx.consecutive_plan_guard_nudges = 0;

                // A failed mutation (soft_error) isn't mutation — gate on !is_error.
                if !o.is_error
                    && crate::agent::types::is_mutating_tool(&o.tool_name, &o.arguments)
                {
                    reason_ctx.mutations_since_last_plan_done = reason_ctx
                        .mutations_since_last_plan_done
                        .saturating_add(1);
                }

                // Reset on a successful plan_update done:true so the NEXT step
                // needs its own mutation evidence. Unconditional within the old
                // `Ok(output)` arm (not gated on soft_error).
                if o.tool_name == "plan_update"
                    && o.arguments.get("done").and_then(|v| v.as_bool()).unwrap_or(false)
                {
                    reason_ctx.mutations_since_last_plan_done = 0;
                    reason_ctx.mutation_challenges_issued = 0;
                }
            }

            // (5) UserCorrection on exit_plan_mode rejection. The old hard-error
            // arm published this off the ToolError text. A rejected
            // exit_plan_mode surfaces as Err(Execution("User rejected the plan.
            // Feedback: ...")). The dispatcher carries that text in o.result.
            if let Err(e) = &o.result {
                if let Some(ref infra) = self.infra_service {
                    let err_msg = e.to_string();
                    if o.tool_name == "exit_plan_mode" && err_msg.starts_with("User rejected the plan.") {
                        let feedback = err_msg
                            .strip_prefix("User rejected the plan. Feedback: ")
                            .unwrap_or(&err_msg)
                            .to_string();
                        infra.publish_user_correction(
                            "local",
                            &feedback,
                            serde_json::json!({
                                "session_id": self.conversation_id,
                                "source": "plan_rejection",
                                "feedback": feedback,
                                "trigger_context": "Agent submitted a plan via exit_plan_mode; user rejected it.",
                                "tool_name": o.tool_name,
                            }),
                        ).await;
                        tracing::info!(
                            session_id = %self.conversation_id,
                            feedback = %feedback,
                            "[ChatDelegate] UserCorrection event published (plan_rejection)"
                        );
                    }
                }
            }
        }

        Ok(None)
    }

    async fn on_usage(&self, usage: &TokenUsage, reason_ctx: &ReasoningContext) {
        tracing::info!(
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cumulative_input = reason_ctx.total_input_tokens,
            cumulative_output = reason_ctx.total_output_tokens,
            model = %self.model,
            "on_usage called"
        );
        self.emit_turn_cost(usage).await;

        // Sprint 3 ② Task 5 — PostLlmCall hook (observe-only).
        // on_usage is async so we can .await directly; no spawn needed.
        self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PostLlmCall {
            task_id: self.conversation_id.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            input_tokens: usage.input_tokens as u64,
            output_tokens: usage.output_tokens as u64,
        }).await;

        self.emit_context_stats(
            &reason_ctx.messages,
            reason_ctx.total_input_tokens,
            reason_ctx.total_output_tokens,
        );
        // Slice 1 — record TokenBudgetSnapshot for UI subscription.
        // No-op when the collector isn't wired (headless / harness).
        if let Some(ref collector) = self.token_budget_collector {
            let turn = self.turn_index.load(Ordering::Relaxed);
            let captured_at =
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let mut snap = crate::agent::token_budget::TokenBudgetSnapshot::new(
                self.conversation_id.clone(),
                turn,
                self.provider.clone(),
                self.model.clone(),
                captured_at,
            );
            snap.provider_input_tokens = usage.input_tokens as u64;
            snap.provider_output_tokens = usage.output_tokens as u64;
            // TokenUsage from M1-T6 fields. `cache_read_tokens` is
            // the cache-hit count; `reasoning_output_tokens` is the
            // Claude-extended-thinking / o1 reasoning count. Cost in
            // micro-USD is computed downstream (M1-T6 cost_records
            // pipeline) so we leave it None here for now.
            snap.provider_cached_tokens = usage.cache_read_tokens as u64;
            snap.provider_reasoning_tokens = usage.reasoning_output_tokens as u64;
            // snap.cost_usd_micros stays None (M2-J commit 2 wires it).
            collector.record(snap);
        }
    }

    async fn on_tool_intent_nudge(&self, text: &str, _ctx: &mut ReasoningContext) {
        self.emit_thinking(&format!("Detected tool intent in: {}", super::truncate_utf8(text, 100)));
    }

    /// Generate a semantic summary of compacted messages for context compression.
    /// Uses the same LLM provider as the main conversation to produce a concise
    /// summary of key decisions, file changes, tools used, and task progress.
    /// Falls back to `None` on any error, which triggers the template-based
    /// placeholder in `build_compression_summary_refs`.
    async fn summarize_for_compression(&self, messages: &[ChatMessage]) -> Option<String> {
        if messages.is_empty() {
            return None;
        }

        // Build a conversation transcript from the compacted messages.
        let mut transcript = String::new();
        for msg in messages {
            let role_label = match msg.role {
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
                MessageRole::System => "System",
            };
            for block in &msg.content {
                match block {
                    ContentBlock::Text { text } => {
                        transcript.push_str(&format!("[{}]: {}\n", role_label, text));
                    }
                    ContentBlock::ToolUse { name, input, .. } => {
                        let input_preview = super::truncate_utf8(&input.to_string(), 200);
                        transcript.push_str(&format!(
                            "[{} called tool '{}' with: {}]\n",
                            role_label, name, input_preview
                        ));
                    }
                    ContentBlock::ToolResult { content, .. } => {
                        let result_preview = super::truncate_utf8(content, 300);
                        transcript.push_str(&format!("[Tool result]: {}\n", result_preview));
                    }
                    ContentBlock::Thinking { .. } => {
                        // Skip thinking blocks — they add noise without signal for summarization.
                    }
                }
            }
        }

        if transcript.trim().is_empty() {
            return None;
        }

        // Summarization prompt: ask the LLM to produce a concise summary.
        // M2-T1b — render via uclaw_utils_template instead of format!() for
        // the same reasons as M2-T1a (#324): testable, fail-safe, ready for
        // the M2-A baseline.md rewrite.
        const SUMMARY_TEMPLATE: &str = "You are a conversation summarizer. Below is a transcript of earlier conversation turns that have been compacted from the active context window.\n\nProduce a concise summary (3-8 sentences) covering:\n- Key decisions made and their rationale\n- Files that were read, modified, or created (with paths)\n- Tools that were used and their outcomes\n- The current task state and what remains to be done\n- Any important constraints, preferences, or edge cases discovered\n\nWrite the summary in the same language as the conversation.\nBe specific — include file paths, tool names, and concrete details.\n\nConversation transcript:\n{{transcript}}";
        let summary_prompt = uclaw_utils_template::render(
            SUMMARY_TEMPLATE,
            [("transcript", transcript.as_str())],
        )
        .unwrap_or_else(|e| {
            // Fall back to the literal template — losing the {{transcript}}
            // substitution but keeping the instruction body. Better than
            // panicking the agent loop on a typo.
            tracing::warn!(
                "M2-T1b: summary template render failed: {e}; \
                 falling back to literal template (without transcript)"
            );
            SUMMARY_TEMPLATE.to_string()
        });

        let config = crate::llm::CompletionConfig {
            model: self.model.clone(),
            max_tokens: 1024,
            temperature: 0.3,
            thinking_enabled: false,
        };

        let messages = vec![ChatMessage::user(&summary_prompt)];

        match self.llm.complete(messages, vec![], &config).await {
            Ok(RespondOutput::Text { text, .. }) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    tracing::info!(
                        summary_len = trimmed.len(),
                        "LLM compression summary generated"
                    );
                    Some(trimmed.to_string())
                }
            }
            Ok(other) => {
                tracing::warn!(
                    ?other,
                    "LLM summarization returned unexpected output, falling back to placeholder"
                );
                None
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "LLM summarization failed, falling back to placeholder"
                );
                None
            }
        }
    }

    /// Generate a StructuredFold summary of compacted messages for context compression.
    async fn summarize_to_fold(&self, messages: &[ChatMessage]) -> Option<crate::agent::compact::StructuredFold> {
        if messages.is_empty() {
            return None;
        }
        match crate::agent::compact::summarize_to_fold(self.llm.clone(), &self.model, messages).await {
            Ok(fold) => Some(fold),
            Err(e) => {
                tracing::warn!("LLM fold summarization failed: {:?}", e);
                None
            }
        }
    }

    /// Incrementally update an existing StructuredFold with only the new messages.
    /// Falls back to a full `summarize_to_fold` if the incremental update fails.
    async fn update_fold_incremental(
        &self,
        prior_fold: &crate::agent::compact::StructuredFold,
        new_messages: &[ChatMessage],
    ) -> Option<crate::agent::compact::StructuredFold> {
        match crate::agent::compact::update_fold_incremental(
            self.llm.clone(), &self.model, prior_fold, new_messages,
        ).await {
            Ok(fold) => Some(fold),
            Err(e) => {
                tracing::warn!(error = %e, "incremental fold update failed; falling back to full summarize");
                self.summarize_to_fold(new_messages).await
            }
        }
    }

    /// After each iteration, generate Capsules for any Gene matches from this turn.
    async fn after_iteration(&self, _iteration: usize) {
        self.generate_capsule_for_turn().await;
    }

    /// Pi Sprint 2 item ③ — drain all pending steering messages from the queue.
    async fn get_steering_messages(&self) -> Vec<ChatMessage> {
        let items = self.steering_queue.drain();
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            if let Some(uuid) = item.uuid {
                self.emit_queued_consumed(&uuid);
            }
            out.push(item.message);
        }
        out
    }

    /// Pi Sprint 2 item ③ — pop one follow-up task from the queue.
    async fn get_follow_up_messages(&self) -> Vec<ChatMessage> {
        match self.follow_up_queue.next() {
            Some(task) => {
                if let Some(uuid) = task.uuid {
                    self.emit_queued_consumed(&uuid);
                }
                task.messages
            }
            None => Vec::new(),
        }
    }

    /// Pi Sprint 2 item ③ — persist an injected user message into agent_messages
    /// so reloads stay continuous. No-op when AppState is unavailable.
    async fn persist_user_message(&self, msg: &ChatMessage) {
        // Extract text from the message (user message -> single Text block)
        let text: String = msg.content.iter().filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        }).collect::<Vec<_>>().join("\n");
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let conv_id = self.conversation_id.clone();
        let did_persist: bool = self.try_app_state().map(|state| {
            let db = state.db.clone();
            drop(state);
            let result = if let Ok(conn) = db.lock() {
                let _ = conn.execute(
                    "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1,?2,'user',?3,?4)",
                    rusqlite::params![id, conv_id, text, now],
                );
                let _ = conn.execute(
                    "UPDATE agent_sessions SET message_count = message_count + 1, updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, conv_id],
                );
                true
            } else {
                false
            };
            result
        }).unwrap_or(false);
        let _ = did_persist; // suppress unused warning
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod browser_runtime_dispatch_patch_tests {
    use super::*;
    use crate::agent::types::ToolCall;

    fn tool_call(name: &str, arguments: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "tool-call-1".to_string(),
            name: name.to_string(),
            arguments,
        }
    }

    #[test]
    fn browser_task_request_patch_is_flattened_before_dispatch() {
        let prepared = prepare_tool_call_for_dispatch(tool_call(
            "browser_task",
            serde_json::json!({
                "task": "Open billing",
                "browser_task_request_patch": {
                    "runtime_preparation_decision": "defer"
                }
            }),
        ));

        assert_eq!(prepared.name, "browser_task");
        assert_eq!(prepared.arguments["task"], "Open billing");
        assert_eq!(prepared.arguments["runtime_preparation_decision"], "defer");
        assert!(prepared.arguments["browser_task_request_patch"].is_null());
    }

    #[test]
    fn browser_task_request_patch_accepts_camel_case_bridge_payload() {
        let prepared = prepare_tool_call_for_dispatch(tool_call(
            "browser_task",
            serde_json::json!({
                "task": "Open billing",
                "browserTaskRequestPatch": {
                    "runtimePreparationDecision": "defer"
                }
            }),
        ));

        assert_eq!(prepared.arguments["runtime_preparation_decision"], "defer");
        assert!(prepared.arguments["browserTaskRequestPatch"].is_null());
    }

    #[test]
    fn explicit_runtime_decision_is_not_overridden() {
        let prepared = prepare_tool_call_for_dispatch(tool_call(
            "browser_task",
            serde_json::json!({
                "task": "Open billing",
                "runtime_preparation_decision": "ready",
                "browser_task_request_patch": {
                    "runtime_preparation_decision": "defer"
                }
            }),
        ));

        assert_eq!(prepared.arguments["runtime_preparation_decision"], "ready");
        assert!(prepared.arguments["browser_task_request_patch"].is_null());
    }

    #[test]
    fn non_browser_tools_are_not_patched() {
        let original = serde_json::json!({
            "question": "Continue?",
            "browser_task_request_patch": {
                "runtime_preparation_decision": "defer"
            }
        });
        let prepared = prepare_tool_call_for_dispatch(tool_call("ask_user", original.clone()));

        assert_eq!(prepared.name, "ask_user");
        assert_eq!(prepared.arguments, original);
    }
}

#[cfg(test)]
mod plan_guard_relevance_tests {
    /// Verify that the plan guard relevance gate (`text_signals_plan_work`)
    /// correctly identifies when an agent response is about plan-related work
    /// vs. a complete answer to an unrelated question.
    use super::text_signals_plan_work;

    // ── Should return TRUE (plan work detected) ──────────────────────

    #[test]
    fn tool_intent_english() {
        // Agent said "let me write" — clearly trying to work but stopped.
        assert!(text_signals_plan_work("Let me write the config file now."));
    }

    #[test]
    fn tool_intent_chinese() {
        // Agent signals intent in Chinese.
        assert!(text_signals_plan_work("接下来我来编辑配置文件"));
    }

    #[test]
    fn mentions_plan_steps() {
        // Agent references plan/task concepts.
        assert!(text_signals_plan_work("I'll start on step 3 now."));
        assert!(text_signals_plan_work("Moving to the next task in the plan"));
        assert!(text_signals_plan_work("步骤 2 已完成，现在开始步骤 3"));
        assert!(text_signals_plan_work("计划还剩两个待办项"));
    }

    #[test]
    fn short_continuation_response() {
        // Short response with continuation markers — the agent was
        // working but gave a terse reply instead of calling tools.
        assert!(text_signals_plan_work("Continuing with the next step."));
        assert!(text_signals_plan_work("Working on it..."));
        assert!(text_signals_plan_work("接下来继续写代码"));
    }

    // ── Should return FALSE (unrelated answer) ────────────────────────

    #[test]
    fn health_question_answer() {
        // User asked "头好疼啊" — agent gave a complete health answer.
        // This is THE regression test for the bug described in the PR.
        assert!(!text_signals_plan_work(
            "头痛是很常见的症状，可能由多种原因引起。建议你休息一下，\
             多喝水，如果持续疼痛可以服用适量止痛药。如果症状严重或反复发作，\
             建议及时就医检查。"
        ));
    }

    #[test]
    fn general_knowledge_answer() {
        // Answer to an unrelated question — no plan keywords, no tool intent.
        assert!(!text_signals_plan_work(
            "Rust is a systems programming language focused on safety, \
             speed, and concurrency. It achieves memory safety without \
             garbage collection through its ownership system."
        ));
    }

    #[test]
    fn weather_question_answer() {
        assert!(!text_signals_plan_work(
            "今天北京的天气晴朗，气温在 15-25°C 之间，适合户外活动。"
        ));
    }

    #[test]
    fn code_explanation_answer() {
        // Agent explaining code that happens to contain "plan" in a
        // variable name — should NOT trigger because the response is a
        // self-contained explanation, not a work-in-progress update.
        // However, "plan" IS in the plan_keywords list, so this WILL
        // match. This is an acceptable false-positive: if the agent
        // mentions "plan" in any context, nudging it to continue is
        // harmless — it'll just say "I'm done" and the loop exits.
        //
        // We document this trade-off rather than trying to parse
        // whether "plan" refers to the agent's plan vs. a variable
        // name, which would be fragile.
        assert!(text_signals_plan_work(
            "The deployment plan involves three stages: build, test, deploy."
        ));
    }

    #[test]
    fn empty_response() {
        assert!(!text_signals_plan_work(""));
    }

    #[test]
    fn short_unrelated() {
        // Short reply to an unrelated question — no continuation markers.
        assert!(!text_signals_plan_work("Got it!"));
        assert!(!text_signals_plan_work("Sure."));
        assert!(!text_signals_plan_work("好的。"));
    }

    // ── Edge cases ───────────────────────────────────────────────────

    #[test]
    fn plan_keyword_in_unrelated_context() {
        // "plan" used as a verb in an unrelated answer. Acceptable
        // false-positive — see code_explanation_answer test comment.
        assert!(text_signals_plan_work(
            "I plan to visit the museum tomorrow."
        ));
    }

    #[test]
    fn task_keyword_in_unrelated_context() {
        // "task" used generically — false-positive but harmless.
        assert!(text_signals_plan_work(
            "My daily task list includes exercise and reading."
        ));
    }

    #[test]
    fn long_answer_without_plan_signals() {
        // Long, complete answer to user question — should NOT trigger.
        let long_answer = "Rust's type system is one of its most powerful features. \
            It uses affine types and ownership to ensure memory safety at compile time. \
            The borrow checker enforces rules about references, preventing data races \
            and use-after-free bugs. This system eliminates entire classes of bugs \
            that plague C and C++ programs without needing a garbage collector. \
            The trade-off is a steeper learning curve, but the compiler provides \
            excellent error messages to guide developers.";
        assert!(long_answer.len() >= 200, "sanity: test answer should be >= 200 chars");
        assert!(!text_signals_plan_work(long_answer));
    }

    // ── Regression coverage: 2026-05-18 五子棋 session (50596741) ─────
    // Real LLM transition stubs that the original gate missed. These
    // strings come from observed silent terminations — fixing the gate
    // here is fixing the exact production bug.

    #[test]
    fn recognises_now_starting_action_chinese() {
        // Final assistant message from gomoku session — agent emitted a
        // 14-char transition stub then loop exited cleanly. The bug.
        assert!(text_signals_plan_work("现在添加游戏逻辑和事件处理："));
        // Other observed "now-starting" variants from prior sessions.
        assert!(text_signals_plan_work("现在开始构建棋盘"));
        assert!(text_signals_plan_work("目前实现胜利检测"));
        assert!(text_signals_plan_work("马上编写事件处理"));
        assert!(text_signals_plan_work("即将开始下一步"));
        assert!(text_signals_plan_work("下面来实现金币系统"));
        assert!(text_signals_plan_work("升级按钮样式："));
    }

    #[test]
    fn recognises_action_verbs_chinese() {
        // Chinese action verbs the agent uses when announcing intent to
        // act on a plan step but failing to call the tool.
        assert!(text_signals_plan_work("添加事件监听器"));
        assert!(text_signals_plan_work("编写游戏循环"));
        assert!(text_signals_plan_work("实现胜负判定"));
        assert!(text_signals_plan_work("完成棋盘渲染"));
        assert!(text_signals_plan_work("优化棋盘渲染"));
    }
}

#[cfg(test)]
mod active_plan_history_tests {
    use super::extract_active_plan_from_history;
    use crate::agent::types::{ChatMessage, ContentBlock, MessageRole};
    use serde_json::json;

    fn plan_write_use(id: &str) -> ChatMessage {
        ChatMessage::assistant_with_tool_use(
            id,
            "plan_write",
            json!({"title": "demo", "steps": ["a", "b"]}),
        )
    }

    fn plan_write_result(id: &str, filename: &str, is_error: bool) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: format!(
                    "Plan created at /Users/u/Documents/workground/test/.uclaw/plans/{}",
                    filename
                ),
                is_error: Some(is_error),
            }],
            compacted: false,
        }
    }

    fn plan_update_use(id: &str, filename: &str) -> ChatMessage {
        ChatMessage::assistant_with_tool_use(
            id,
            "plan_update",
            json!({"filename": filename, "step_index": 0, "done": true}),
        )
    }

    fn bash_use(id: &str) -> ChatMessage {
        ChatMessage::assistant_with_tool_use(id, "bash", json!({"command": "ls"}))
    }

    #[test]
    fn extracts_filename_from_recent_plan_update() {
        let history = vec![
            ChatMessage::user("start"),
            plan_update_use("call_1", "2026-05-17_网页五子棋开发.md"),
        ];
        assert_eq!(
            extract_active_plan_from_history(&history),
            Some("2026-05-17_网页五子棋开发.md".to_string())
        );
    }

    #[test]
    fn extracts_filename_from_plan_write_result() {
        let history = vec![
            plan_write_use("call_w"),
            plan_write_result("call_w", "fresh.md", false),
        ];
        assert_eq!(
            extract_active_plan_from_history(&history),
            Some("fresh.md".to_string())
        );
    }

    #[test]
    fn returns_most_recent_when_multiple_plan_calls_exist() {
        // older plan_write, then a fresher plan_update for a DIFFERENT
        // filename — the latest wins regardless of which call shape it was.
        let history = vec![
            plan_write_use("call_w"),
            plan_write_result("call_w", "old.md", false),
            bash_use("call_b"),
            plan_update_use("call_u", "newest.md"),
        ];
        assert_eq!(
            extract_active_plan_from_history(&history),
            Some("newest.md".to_string())
        );
    }

    #[test]
    fn last_plan_write_overrides_earlier_plan_update() {
        // Symmetric to the above — plan_write wins if it's the last one.
        let history = vec![
            plan_update_use("call_u1", "first.md"),
            plan_write_use("call_w"),
            plan_write_result("call_w", "rewritten.md", false),
        ];
        assert_eq!(
            extract_active_plan_from_history(&history),
            Some("rewritten.md".to_string())
        );
    }

    #[test]
    fn ignores_failed_plan_write_result() {
        // is_error=true → don't trust the parsed filename.
        let history = vec![
            plan_write_use("call_w"),
            plan_write_result("call_w", "shouldnt-stick.md", /*is_error=*/ true),
        ];
        assert_eq!(extract_active_plan_from_history(&history), None);
    }

    #[test]
    fn returns_none_when_no_plan_history() {
        let history = vec![
            ChatMessage::user("hi"),
            bash_use("call_b"),
            ChatMessage::assistant("ran ls"),
        ];
        assert_eq!(extract_active_plan_from_history(&history), None);
    }

    #[test]
    fn returns_none_for_empty_history() {
        let history: Vec<ChatMessage> = Vec::new();
        assert_eq!(extract_active_plan_from_history(&history), None);
    }

    #[test]
    fn plan_update_without_filename_is_skipped() {
        // Malformed argument shouldn't crash or surface a bogus result.
        let bad = ChatMessage::assistant_with_tool_use(
            "call_x",
            "plan_update",
            json!({"step_index": 0}),
        );
        assert_eq!(extract_active_plan_from_history(&[bad]), None);
    }

    #[test]
    fn plan_write_result_without_match_is_skipped() {
        // ToolResult content that doesn't look like the canonical
        // "Plan created at /.../plans/<name>" — don't fabricate a filename.
        let history = vec![
            plan_write_use("call_w"),
            ChatMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "call_w".to_string(),
                    content: "something went wrong".to_string(),
                    is_error: Some(false),
                }],
                compacted: false,
            },
        ];
        assert_eq!(extract_active_plan_from_history(&history), None);
    }
}
