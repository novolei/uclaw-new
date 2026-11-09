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

use super::content_assembler::gene_signals;
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
    /// pending_approvals, infra_service, trajectory_store, tool_budget, hook_bus
    /// (sourced via self.app_state().agent_api.hook_bus()).
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
                self.app_state().agent_api.hook_bus().cloned()
                    .expect("hook_bus wired at boot"),
                self.telemetry.heartbeat.clone(),
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
                // P3: learn from the user's own words, never from the
                // generated <turn_context> prefix.
                ContentBlock::Text { text } => Some(crate::agent::turn_context::strip(text).to_string()),
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
        if self.learning.enabled {
            if let Some(buffer) = self.learning.buffer.as_ref().cloned() {
                let llm = self.learning.llm.clone();
                let db = self.try_app_state().map(|s| s.db.clone());
                let daily_budget = self.learning.llm_daily_budget;
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

        // gbrain extractor — Step 2b: writes via write_page (EntityPage + bucket_seal)
        // instead of mcp__gbrain__put_page. The gbrain MCP server is no longer called
        // from this path; gbrain-specific error-state surfacing removed accordingly.
        if self.gbrain_extractor.enabled {
            let llm = self.gbrain_extractor.llm.clone();
            let db = self.try_app_state().map(|s| s.db.clone());
            let daily_budget = self.gbrain_extractor.daily_budget;
            let page_store = self.gbrain_extractor.entity_page_store.clone();
            let page_adapter = self.gbrain_extractor.page_adapter.clone();
            let space_id = self.gbrain_extractor.space_id.clone().unwrap_or_else(|| "default".to_string());
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
                    let proposals = crate::memory_graph::chat_extractor::extract_from_chat_turn(
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
                            p.confidence >= crate::memory_graph::chat_extractor::MIN_ACT_CONFIDENCE
                        })
                        .collect();
                    if let (Some(store), Some(adapter)) = (&page_store, &page_adapter) {
                        tracing::debug!(
                            count = actionable.len(),
                            "[ChatDelegate] extractor — firing write_page calls"
                        );
                        for proposal in actionable {
                            if let Err(e) = crate::memory_adapter::page_dual_write::write_page(
                                store, adapter, &space_id, &proposal.slug, &proposal.content,
                            ).await {
                                tracing::warn!(slug = %proposal.slug, error = %e, "[ChatDelegate] extractor write_page failed");
                            }
                        }
                    } else {
                        tracing::debug!(
                            "[ChatDelegate] extractor: page writers not wired, skipping {} proposals",
                            actionable.len()
                        );
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

/// The completion config of the main conversation request; shared with the
/// prefix-reusing summary (same model + thinking → same cache key).
pub(crate) fn main_completion_config(model: &str, thinking_enabled: bool) -> crate::llm::CompletionConfig {
    crate::llm::CompletionConfig {
        model: model.to_string(),
        max_tokens: compute_max_tokens(model, thinking_enabled),
        temperature: 0.7,
        thinking_enabled,
    }
}

/// The message list `call_llm` actually sends for this turn, built from the
/// live context + the frozen snapshot: system prompt first, `compacted`
/// messages skipped, orphan tool calls patched (L6), screenshots stripped for
/// image-blind models (L5), and the snapshot's dynamic block prepended to the
/// last user text message (request copy only). Shared with the prefix-reusing
/// compaction summary (`compact::prefix_summary`) so both requests carry a
/// byte-identical prefix.
pub(crate) fn build_request_messages(
    reason_ctx: &ReasoningContext,
    snapshot: &crate::agent::turn::TurnSnapshot,
    model_supports_images: bool,
) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage::system(&snapshot.system_prompt)];
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
    if !model_supports_images {
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
                model = %snapshot.model,
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
    // Time + workspace root injected here via snapshot.dynamic_context
    // (assembled alongside the system prompt in create_turn_snapshot —
    // P3-6 Task 4 single call site). Keeping time out of the system
    // prompt preserves byte-stability for Anthropic prompt caching.
    if let Some(last_user_idx) = messages.iter().rposition(|m| {
        matches!(m.role, crate::agent::types::MessageRole::User)
            && m.content.iter().any(|b| matches!(b, crate::agent::types::ContentBlock::Text { .. }))
    }) {
        let dyn_ctx = snapshot.dynamic_context.clone();
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
    messages
}

/// The turn snapshot `ChatDelegate::create_turn_snapshot` returns, from the
/// already-chosen system prompt / dynamic block: the tool surface
/// (`PerTurnToolSurface`: list_definitions + L2 normalization, with the
/// mid-turn tool-list hash tracking in `last_tool_defs_hash`), or no tools
/// when `force_text`. Free function so the §7.C prefix-consistency test runs
/// the same code (a `ChatDelegate` needs a Wry `AppHandle`).
pub(crate) fn turn_snapshot(
    turn_index: u32,
    model: &str,
    system_prompt: String,
    dynamic_context: String,
    registry: &crate::agent::tools::tool::ToolRegistry,
    force_text: bool,
    last_tool_defs_hash: &std::sync::Mutex<Option<u64>>,
) -> crate::agent::turn::TurnSnapshot {
    let tools = if force_text {
        Vec::new()
    } else {
        let surface = crate::agent::tool_shaping::PerTurnToolSurface::from_registry(registry);
        let total_stats = surface.normalize_stats;
        if !total_stats.is_noop() {
            tracing::info!(
                examples_dropped = total_stats.examples_dropped,
                enums_deduped = total_stats.enums_deduped,
                deep_nests_pruned = total_stats.deep_nests_pruned,
                tool_count = surface.definitions.len(),
                "[L2] normalized tool schemas",
            );
        } else {
            // NOOP heartbeat — keeps the trace queryable per-turn so users
            // can confirm L2 actually ran even when schemas were already
            // clean. INFO would be too chatty (every turn); DEBUG is the
            // right level — surfaces under `RUST_LOG=uclaw_core::agent=debug`.
            tracing::debug!(
                tool_count = surface.definitions.len(),
                "[L2] normalize: noop (schemas already clean)",
            );
        }
        // Track tool list hash to detect unexpected changes mid-turn (e.g.
        // MCP reconnect during a multi-iteration turn). Anthropic's prompt
        // cache covers the tool list when it's byte-stable across iterations;
        // this logging surfaces cache-busting events for observability.
        // Hashed AFTER normalization so the cache key reflects the actual
        // payload the LLM sees, not the raw registry shape.
        if let Ok(mut prev) = last_tool_defs_hash.lock() {
            if let Some(p) = *prev {
                if p != surface.definition_hash {
                    tracing::warn!(
                        prev_hash = p, new_hash = surface.definition_hash,
                        "Tool list changed mid-turn — Anthropic cache miss likely"
                    );
                }
            }
            *prev = Some(surface.definition_hash);
        }
        surface.definitions
    };

    crate::agent::turn::TurnSnapshot {
        turn_index,
        model: model.to_string(),
        system_prompt: std::sync::Arc::new(system_prompt),
        dynamic_context,
        tools: std::sync::Arc::new(tools),
        force_text,
    }
}

impl ChatDelegate {
    /// Pre-P3 per-iteration prompt assembly (non-transcript callers): the
    /// full system prompt (mode + gene + plan hint + project rules + cache
    /// align) and the ephemeral dynamic block spliced into the request copy.
    async fn assemble_legacy_prompt(
        &self,
        reason_ctx: &ReasoningContext,
        effective_mode: &crate::safety::SafetyMode,
    ) -> (String, String) {
        let effective_mode = effective_mode.clone();

        // PR4: extract GEP inputs from reason_ctx so build_prompt_context can
        // run the async match_genes call inside the unified seam.
        let last_user_text: String = reason_ctx.messages.iter().rev()
            .find(|m| matches!(m.role, crate::agent::types::MessageRole::User))
            .and_then(|m| m.content.iter().find_map(|b| {
                if let crate::agent::types::ContentBlock::Text { text } = b {
                    Some(crate::agent::turn_context::strip(text).to_string())
                } else {
                    None
                }
            }))
            .unwrap_or_default();
        let tool_errors: Vec<String> = self.recent_tool_errors.lock()
            .map(|e| e.clone())
            .unwrap_or_default();

        // P3-6 Task 4 + PR4: single async call — assemble_prompt now builds
        // all 4 post-append layers (gene_block, plan_suggest_hint,
        // project_rules_block, ladder_pad) inside the unified seam.
        // full_system_prompt is the complete, fully-assembled string.
        let assembled = self.assemble_prompt(&effective_mode, &last_user_text, tool_errors).await;
        let full_system_prompt = assembled.system;
        // Store the dynamic half so call_llm can consume it from the snapshot
        // without calling build_dynamic_context (which would re-invoke
        // assemble_system_prompt a second time).
        // P2: after a rewind/fork, modified files from the rewound turns are
        // surfaced in the dynamic block (their tool calls are gone from the
        // transcript). Loaded once per turn — rewound flags only change on a
        // rewind/fork, never mid-turn.
        let rewound_changes =
            self.rewound_changes.get_or_init(|| rewound_changes_from_sink(self.transcript_sink.as_ref()));
        let assembled_dynamic = with_rewound_changes(assembled.dynamic_for_last_user, rewound_changes);
        (full_system_prompt, assembled_dynamic)
    }
}

impl ChatDelegate {
    /// Context continuity P3 — gather this turn's per-turn inputs (time,
    /// memory, learned profile, growth, relevant skills, GEP signals with the
    /// `gep.last_matches` side effect, plan hint, `.uclawrules` glob rules
    /// against the session's file working set, a rules update when the rules
    /// source hash differs from the frame's, context-manager fragments +
    /// compose stats, rewound file changes, mode and tool names), then
    /// `apply_turn_context`, keep the report and emit `agent:prompt_frame`.
    /// GEP error signals come from the previous turn's failed tool results
    /// (`previous_turn_tool_errors`). Never fails the turn.
    async fn prepare_turn_context(&self, ctx: &mut ReasoningContext) {
        use crate::agent::rule_context_builder::RuleContextBuilder;
        use tauri::Emitter;
        let Some(sink) = self.transcript_sink.as_ref() else { return };
        let frame = self.session_frame().await;

        // GEP: once per turn on the user's own words + the previous turn's
        // tool errors (from the transcript; this per-turn delegate's own
        // `recent_tool_errors` is still empty here).
        let user_text = turn_user_text(ctx);
        let mut tool_errors: Vec<String> = match sink.db.lock() {
            Ok(conn) => previous_turn_tool_errors(&conn, &sink.session_id, &sink.turn_id).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "[TURN_CTX] previous turn's tool errors unavailable");
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        if let Ok(errors) = self.recent_tool_errors.lock() {
            tool_errors.extend(errors.iter().cloned());
        }
        tool_errors.truncate(PREVIOUS_TURN_TOOL_ERRORS_MAX);
        let gene_block =
            gene_signals(self.gep.retriever.as_deref(), &self.gep.last_matches, &user_text, &tool_errors).await;
        let plan_suggest_hint = self.plan_suggest_hint();

        // Context-manager fragments (stash + compose stats), then the
        // first-act flip — the side effects `assemble_prompt` had.
        let injection = crate::agent::baseline_blocks::InjectionContext {
            is_first_act_turn: self.is_first_act_turn.load(Ordering::Relaxed),
            last_error_kind: self.last_error_kind.lock().ok().and_then(|g| g.clone()),
            context_pressure_ratio: self.estimate_context_pressure_ratio(),
        };
        let query = crate::agent::context_manager::ComposeQuery::defaults_with_topics(vec![]);
        let composed = self.context_manager.read().await.for_prompt_with_injection(&query, &injection).await;
        if let Ok(mut slot) = self.last_injected_fragments.lock() {
            *slot = composed.injected_fragments.clone();
        }
        if let Some(collector) = &self.telemetry.compose_stats {
            collector.record(&self.conversation_id, composed.stats.clone());
        }
        self.is_first_act_turn.store(false, Ordering::Relaxed);

        let current_mode = self.resolve_effective_mode().await;
        let current_tool_names: Vec<String> = crate::agent::tool_shaping::PerTurnToolSurface::from_registry(&self.tools)
            .definitions
            .into_iter()
            .map(|d| d.name)
            .collect();
        let rewound_changes =
            self.rewound_changes.get_or_init(|| rewound_changes_from_sink(Some(sink))).clone();

        // Project rules: glob rules for this session's files; the full rules
        // text whenever the rules source hash differs from the frame's frozen
        // baseline (emitted only when the active ledger lacks it).
        let (glob_rules, rules_update) = match self.workspace_root.as_deref() {
            Some(root) => {
                let files = match sink.db.lock() {
                    Ok(conn) => crate::agent::file_working_set::load(&conn, &sink.session_id)
                        .map(|ws| ws.paths())
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, "[TURN_CTX] file working set unavailable; no glob rules");
                            Vec::new()
                        }),
                    Err(_) => Vec::new(),
                };
                (RuleContextBuilder::glob_matched_rules(root, &files), rules_update_for(root, frame))
            }
            None => (Vec::new(), None),
        };

        let inputs = crate::agent::turn_context::TurnContextInputs {
            now: chrono::Local::now(),
            memory_context: self.memory_context.clone(),
            learned_profile: self.prompt_blocks.learned_profile.clone(),
            growth: self.prompt_blocks.growth.clone(),
            relevant_skills: self.prompt_blocks.relevant_skills.clone(),
            gene_block,
            plan_suggest_hint,
            glob_rules,
            rules_update,
            context_fragments: composed.injected_fragments,
            rewound_changes,
            current_mode,
            current_tool_names,
        };
        let tool_count = inputs.current_tool_names.len();
        let rules_changed = inputs.rules_update.is_some();
        let result = match sink.db.lock() {
            Ok(conn) => apply_turn_context(&conn, &sink.session_id, &sink.turn_id, ctx, &inputs, frame),
            Err(_) => Err(anyhow::anyhow!("transcript sink DB mutex poisoned")),
        };
        match result {
            Ok(Some(report)) => {
                tracing::info!(
                    session_id = %sink.session_id,
                    memory = ?report.memory,
                    memory_tokens = report.memory_tokens,
                    turn_context_tokens = report.total_tokens,
                    mode_changed = ?report.mode_changed,
                    tools_added = report.tools_added.len(),
                    tools_removed = report.tools_removed.len(),
                    rules_changed,
                    "[TURN_CTX] rendered and persisted the turn context",
                );
                let _ = self.app_handle.emit(
                    "agent:prompt_frame",
                    prompt_frame_payload(&sink.session_id, frame, &sink.turn_id, &report, tool_count),
                );
                if let Ok(mut slot) = self.last_turn_context_report.lock() {
                    *slot = Some(report);
                }
            }
            Ok(None) => tracing::debug!(session_id = %sink.session_id, "[TURN_CTX] prompt row already carries a turn context"),
            Err(e) => tracing::warn!(error = %e, session_id = %sink.session_id, "[TURN_CTX] turn context not applied; continuing"),
        }
    }
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

    /// Context continuity P3 — transcript mode: the first call of the turn
    /// renders, persists and splices this turn's `<turn_context>` block
    /// (`prepare_turn_context`); every later iteration replays it as-is.
    async fn before_llm_call(&self, reason_ctx: &mut ReasoningContext, _iteration: usize) -> Option<LoopOutcome> {
        if self.transcript_sink.is_some() && claim_once(&self.turn_context_applied) {
            self.prepare_turn_context(reason_ctx).await;
        }
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

        // Context continuity P3 — transcript mode: the system prompt is the
        // session's frozen frame (byte-identical across turns until its key
        // changes); per-turn content goes into the persisted `<turn_context>`
        // block instead (Task 6 wires it in `before_llm_call`), so no
        // ephemeral dynamic splice here. Other callers keep the pre-P3
        // per-iteration assembly (`assemble_legacy_prompt`).
        let (full_system_prompt, assembled_dynamic) = if self.transcript_sink.is_some() {
            (self.session_frame().await.system_prompt.clone(), String::new())
        } else {
            self.assemble_legacy_prompt(reason_ctx, &effective_mode).await
        };

        turn_snapshot(
            turn_index,
            &self.model,
            full_system_prompt,
            assembled_dynamic,
            &self.tools,
            reason_ctx.force_text,
            &self.last_tool_defs_hash,
        )
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

        let supports_images = crate::agent::image_policy::supports_images(&self.provider, &snapshot.model);
        let messages = build_request_messages(reason_ctx, snapshot, supports_images);

        // Tools were assembled (list_definitions + L2 normalization + hash
        // tracking) in create_turn_snapshot; consume the frozen list here.
        let tools = (*snapshot.tools).clone();
        let config = main_completion_config(&snapshot.model, self.thinking_enabled);
        let max_tokens = config.max_tokens;

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
            model = %snapshot.model,
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
        // Clone the Arc out before .await to avoid holding a borrow into AppState across the await point.
        let hook_bus = self.app_state().agent_api.hook_bus().cloned()
            .expect("hook_bus wired at boot");
        hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PreLlmCall {
            task_id: self.conversation_id.clone(),
            provider: self.provider.clone(),
            model: snapshot.model.clone(),
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

        // (6) Context continuity P2 — persist this batch's file events
        //     (transcript mode only: no sink = no-op). Best-effort: a failed
        //     write must not fail the turn.
        match record_file_events_in_sink(
            self.transcript_sink.as_ref(),
            &outcomes,
            self.workspace_root.as_deref(),
        ) {
            Ok(0) => {}
            Ok(_) => {
                use tauri::Emitter;
                if let Some(sink) = self.transcript_sink.as_ref() {
                    let _ = self.app_handle.emit(
                        "agent:file_working_set_changed",
                        serde_json::json!({ "sessionId": sink.session_id }),
                    );
                }
            }
            Err(e) => tracing::warn!(
                error = %e,
                session_id = %self.conversation_id,
                "failed to record file events; continuing"
            ),
        }

        Ok(None)
    }

    async fn on_usage(&self, usage: &TokenUsage, reason_ctx: &ReasoningContext, snapshot: &crate::agent::turn::TurnSnapshot) {
        tracing::info!(
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cumulative_input = reason_ctx.total_input_tokens,
            cumulative_output = reason_ctx.total_output_tokens,
            model = %snapshot.model,
            "on_usage called"
        );
        self.emit_turn_cost(usage, &snapshot.model).await;

        // Sprint 3 ② Task 5 — PostLlmCall hook (observe-only).
        // Clone the Arc out before .await to avoid holding a borrow into AppState across the await point.
        let hook_bus = self.app_state().agent_api.hook_bus().cloned()
            .expect("hook_bus wired at boot");
        hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PostLlmCall {
            task_id: self.conversation_id.clone(),
            provider: self.provider.clone(),
            model: snapshot.model.clone(),
            input_tokens: usage.input_tokens as u64,
            output_tokens: usage.output_tokens as u64,
        }).await;

        self.emit_context_stats(
            &reason_ctx.messages,
            reason_ctx.total_input_tokens,
            reason_ctx.total_output_tokens,
            &snapshot.model,
        );
        // Slice 1 — record TokenBudgetSnapshot for UI subscription.
        // No-op when the collector isn't wired (headless / harness).
        if let Some(ref collector) = self.telemetry.token_budget {
            let turn = self.turn_index.load(Ordering::Relaxed);
            let captured_at =
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let mut snap = crate::agent::token_budget::TokenBudgetSnapshot::new(
                self.conversation_id.clone(),
                turn,
                self.provider.clone(),
                snapshot.model.clone(),
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
    async fn summarize_to_fold(
        &self,
        messages: &[ChatMessage],
        target_tokens: usize,
    ) -> Option<crate::agent::compact::StructuredFold> {
        if messages.is_empty() {
            return None;
        }
        match crate::agent::compact::summarize_to_fold(self.llm.clone(), &self.model, messages, target_tokens).await {
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
        target_tokens: usize,
    ) -> Option<crate::agent::compact::StructuredFold> {
        match crate::agent::compact::update_fold_incremental(
            self.llm.clone(), &self.model, prior_fold, new_messages, target_tokens,
        ).await {
            Ok(fold) => Some(fold),
            Err(e) => {
                tracing::warn!(error = %e, "incremental fold update failed; falling back to full summarize");
                self.summarize_to_fold(new_messages, target_tokens).await
            }
        }
    }

    /// Context continuity P2 — prefix-reusing compaction summary. Same LLM,
    /// model, config, tools and message prefix as `call_llm` (request logic
    /// in `compact::prefix_summary`); non-streaming, nothing persisted.
    async fn summarize_with_prefix(
        &self,
        reason_ctx: &ReasoningContext,
        snapshot: &crate::agent::turn::TurnSnapshot,
        cut: usize,
        target_tokens: usize,
    ) -> Option<crate::agent::compact::StructuredFold> {
        let config = main_completion_config(&snapshot.model, self.thinking_enabled);
        let supports_images = crate::agent::image_policy::supports_images(&self.provider, &snapshot.model);
        crate::agent::compact::prefix_summary::summarize_with_prefix(
            self.llm.as_ref(),
            &config,
            reason_ctx,
            snapshot,
            cut,
            target_tokens,
            supports_images,
        )
        .await
    }

    /// Context continuity P2 — the session's persisted file working set,
    /// read through the transcript sink's DB (also present for manual `/compact`).
    fn session_block_context(&self) -> Option<crate::agent::session_block::SessionBlockContext> {
        session_block_context_from_sink(self.transcript_sink.as_ref(), self.workspace_root.as_deref())
    }

    /// After each iteration, generate Capsules for any Gene matches from this turn.
    async fn after_iteration(&self, _iteration: usize) {
        self.generate_capsule_for_turn().await;
    }

    /// Context continuity P1 — append to the session transcript via the
    /// installed `TranscriptSink`; no-op when none is installed.
    async fn persist_transcript(&self, new_msgs: &[ChatMessage]) -> anyhow::Result<()> {
        persist_to_sink(self.transcript_sink.as_ref(), new_msgs)
    }

    /// Context continuity P2 — store cleared tool results in the transcript
    /// overlay (`persist_cleared_in_sink`), then notify the UI so it marks
    /// the cards (`agent:tool_results_cleared`). No sink = no-op.
    async fn persist_cleared(
        &self,
        cleared: &[(usize, ChatMessage)],
        tool_call_ids: &[String],
        freed_tokens: usize,
    ) -> anyhow::Result<()> {
        use tauri::Emitter;
        let Some(sink) = self.transcript_sink.as_ref() else { return Ok(()) };
        persist_cleared_in_sink(Some(sink), cleared)?;
        let _ = self.app_handle.emit(
            "agent:tool_results_cleared",
            tool_results_cleared_payload(&sink.session_id, tool_call_ids, freed_tokens),
        );
        Ok(())
    }

    /// Context continuity P1 — transcript-mode compaction: supersede +
    /// session_block + fold display row (see `replace_session_block_in_sink`),
    /// then notify the UI. No sink = no-op (nothing persisted, nothing to refresh).
    async fn replace_with_session_block(
        &self,
        block: &ChatMessage,
        fold: &crate::agent::compact::StructuredFold,
        superseded_through: usize,
        threshold_tokens: usize,
        trigger: crate::agent::types::CompactionTrigger,
    ) -> anyhow::Result<()> {
        use tauri::Emitter;
        let Some(sink) = self.transcript_sink.as_ref() else { return Ok(()) };
        let compacted_messages = replace_session_block_in_sink(sink, block, Some(fold), superseded_through)?;
        let summary_tokens = crate::agent::types::estimate_live_message_tokens(block);
        let _ = self.app_handle.emit(
            "agent:context_compacted",
            context_compacted_payload(
                &sink.session_id,
                compacted_messages,
                summary_tokens,
                threshold_tokens,
                trigger,
            ),
        );
        Ok(())
    }

    /// Transcript compaction is about to summarise: heartbeat → `compacting`
    /// stage (no 30s stall alarm) and notify the UI
    /// (`agent:context_compacting`, same session id as `agent:context_compacted`).
    async fn on_context_compacting(&self, trigger: crate::agent::types::CompactionTrigger) {
        use tauri::Emitter;
        self.beat(crate::agent::heartbeat::stages::COMPACTING);
        let session_id = self
            .transcript_sink
            .as_ref()
            .map(|s| s.session_id.as_str())
            .unwrap_or(self.conversation_id.as_str());
        let _ = self
            .app_handle
            .emit("agent:context_compacting", context_compacting_payload(session_id, trigger));
    }

    /// Compaction done: back to normal stage tracking. On failure no
    /// `agent:context_compacted` follows, so tell the UI to drop its status
    /// (`agent:context_compaction_failed`).
    async fn on_context_compaction_finished(&self, succeeded: bool) {
        use tauri::Emitter;
        self.beat(crate::agent::heartbeat::stages::THINKING);
        if !succeeded {
            let session_id = self
                .transcript_sink
                .as_ref()
                .map(|s| s.session_id.as_str())
                .unwrap_or(self.conversation_id.as_str());
            let _ = self
                .app_handle
                .emit("agent:context_compaction_failed", context_compaction_failed_payload(session_id));
        }
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

/// Write `msgs` as `message` rows tagged with the sink's turn id, in one
/// transaction under the sink's DB lock. `None` sink = Ok no-op.
pub(crate) fn persist_to_sink(
    sink: Option<&super::TranscriptSink>,
    msgs: &[ChatMessage],
) -> anyhow::Result<()> {
    let Some(sink) = sink else { return Ok(()) };
    let conn = sink
        .db
        .lock()
        .map_err(|_| anyhow::anyhow!("transcript sink DB mutex poisoned"))?;
    crate::agent::transcript::append(
        &conn,
        &sink.session_id,
        Some(&sink.turn_id),
        crate::agent::transcript::TranscriptKind::Message,
        msgs,
    )?;
    Ok(())
}

/// Context continuity P2 — write cleared tool results (active transcript
/// index, cleared message) to the overlay column under the sink's DB lock
/// (`transcript::set_cleared`). `None` sink = Ok no-op.
pub(crate) fn persist_cleared_in_sink(
    sink: Option<&super::TranscriptSink>,
    cleared: &[(usize, ChatMessage)],
) -> anyhow::Result<()> {
    let Some(sink) = sink else { return Ok(()) };
    let conn = sink
        .db
        .lock()
        .map_err(|_| anyhow::anyhow!("transcript sink DB mutex poisoned"))?;
    crate::agent::transcript::set_cleared(&conn, &sink.session_id, cleared)
}

/// `agent:tool_results_cleared` payload (camelCase keys).
pub(crate) fn tool_results_cleared_payload(
    session_id: &str,
    tool_call_ids: &[String],
    freed_tokens: usize,
) -> serde_json::Value {
    serde_json::json!({
        "sessionId": session_id,
        "toolCallIds": tool_call_ids,
        "freedTokens": freed_tokens,
    })
}

/// Context continuity P2 — derive file events from one tool batch and append
/// them to the session's file-event log, tagged with the sink's turn id, under
/// the sink's DB lock. Returns the number of events recorded. `None` sink =
/// Ok(0) (non-transcript callers record nothing).
pub(crate) fn record_file_events_in_sink(
    sink: Option<&super::TranscriptSink>,
    outcomes: &[crate::agent::tool_dispatch::ToolDispatchOutcome],
    workspace_root: Option<&std::path::Path>,
) -> anyhow::Result<usize> {
    let Some(sink) = sink else { return Ok(0) };
    let events: Vec<crate::agent::file_working_set::FileEvent> = outcomes
        .iter()
        .flat_map(|o| crate::agent::file_events::file_events_from_outcome(o, workspace_root))
        .collect();
    if events.is_empty() {
        return Ok(0);
    }
    let conn = sink
        .db
        .lock()
        .map_err(|_| anyhow::anyhow!("transcript sink DB mutex poisoned"))?;
    crate::agent::file_working_set::record(&conn, &sink.session_id, Some(&sink.turn_id), &events)?;
    Ok(events.len())
}

/// Context continuity P2 — load the session's file working set via the
/// sink's DB for the compaction session block. `None` without a sink or when
/// the DB is unavailable (logged; the block then carries the summary only).
pub(crate) fn session_block_context_from_sink(
    sink: Option<&super::TranscriptSink>,
    workspace_root: Option<&std::path::Path>,
) -> Option<crate::agent::session_block::SessionBlockContext> {
    let sink = sink?;
    let loaded = match sink.db.lock() {
        Ok(conn) => crate::agent::file_working_set::load(&conn, &sink.session_id),
        Err(_) => Err(anyhow::anyhow!("transcript sink DB mutex poisoned")),
    };
    match loaded {
        Ok(working_set) => Some(crate::agent::session_block::SessionBlockContext {
            working_set,
            workspace_root: workspace_root.map(std::path::Path::to_path_buf),
        }),
        Err(e) => {
            tracing::warn!(error = %e, "file working set unavailable; session block carries the summary only");
            None
        }
    }
}

/// Token cap of the `<rewound_file_changes>` block in the dynamic context.
const REWOUND_CHANGES_MAX_TOKENS: usize = 1_500;

/// Context continuity P3 — everything a newly built frame records besides
/// its key (see [`load_or_rebuild_frame`]).
pub(crate) struct FrameBuildParts {
    pub system_prompt: String,
    pub mode: String,
    pub rules_hash: String,
    pub tool_names: Vec<String>,
    pub manifest: crate::agent::session_frame::ManifestStats,
}

/// Context continuity P3 — the parts of a (re)built session frame from its
/// inputs: the frozen system prompt (`assemble_frozen_system_prompt`), the
/// frame's mode name, the rules source hash of the workspace, the announced
/// tool names (`PerTurnToolSurface`) and the manifest stats; plus the token
/// breakdown for the `[FRAME]` log. Shared by `ChatDelegate::session_frame`
/// and the §7.C prefix-consistency test.
pub(crate) fn frame_build_parts(
    inputs: &super::content_assembler::FrameInputs,
    registry: &crate::agent::tools::tool::ToolRegistry,
    manifest: &crate::skills_manifest::SessionManifest,
) -> (FrameBuildParts, super::content_assembler::FrameTokenBreakdown) {
    let system_prompt = super::content_assembler::assemble_frozen_system_prompt(inputs);
    let tokens = super::content_assembler::frame_token_breakdown(inputs, &system_prompt);
    let parts = FrameBuildParts {
        system_prompt,
        mode: crate::agent::session_frame::mode_name(&inputs.mode).to_string(),
        rules_hash: inputs
            .workspace_root
            .as_deref()
            .map(crate::agent::rule_context_builder::RuleContextBuilder::rules_source_hash)
            .unwrap_or_default(),
        tool_names: crate::agent::tool_shaping::PerTurnToolSurface::from_registry(registry)
            .definitions
            .into_iter()
            .map(|d| d.name)
            .collect(),
        manifest: crate::agent::session_frame::ManifestStats {
            listed: manifest.listed.len() as u32,
            total: manifest.total_available as u32,
            tokens: tokens.manifest,
        },
    };
    (parts, tokens)
}

/// Context continuity P3 — load the session's frozen prompt frame and reuse
/// it when its key still matches `key`; otherwise await `build` (outside the
/// DB lock — it may take other locks; it is the only place the session
/// manifest is built) and persist the new frame tagged with `turn_id`. A load error (e.g. a corrupt row) is treated as a missing
/// frame; a save error is logged and the in-memory frame is still used —
/// the turn never fails here. Returns the frame and, when it was (re)built,
/// the reason.
pub(crate) async fn load_or_rebuild_frame<F, Fut>(
    db: Option<&std::sync::Mutex<rusqlite::Connection>>,
    session_id: &str,
    turn_id: &str,
    key: crate::agent::session_frame::FrameKey,
    now_ms: i64,
    build: F,
) -> (crate::agent::session_frame::SessionFrame, Option<crate::agent::session_frame::FrameRebuildReason>)
where
    F: FnOnce(crate::agent::session_frame::FrameRebuildReason) -> Fut,
    Fut: std::future::Future<Output = FrameBuildParts>,
{
    use crate::agent::session_frame as sf;
    let existing = match db.map(|d| d.lock()) {
        Some(Ok(conn)) => sf::load(&conn, session_id).unwrap_or_else(|e| {
            tracing::warn!(session_id, error = %e, "[FRAME] stored frame unreadable; rebuilding");
            None
        }),
        _ => None,
    };
    let Some(reason) = sf::rebuild_reason(existing.as_ref(), &key) else {
        // `rebuild_reason` returns None only when `existing` is Some.
        return (existing.expect("reused frame exists"), None);
    };
    let parts = build(reason).await;
    let frame = sf::SessionFrame {
        session_id: session_id.to_string(),
        key,
        system_prompt: parts.system_prompt,
        mode: parts.mode,
        rules_hash: parts.rules_hash,
        tool_names: parts.tool_names,
        manifest: parts.manifest,
        rebuild_reason: reason,
        rebuilt_turn_id: Some(turn_id.to_string()),
        created_at: existing.as_ref().map(|f| f.created_at).unwrap_or(now_ms),
        updated_at: now_ms,
    };
    match db.map(|d| d.lock()) {
        Some(Ok(conn)) => {
            if let Err(e) = sf::upsert(&conn, &frame) {
                tracing::warn!(session_id, error = %e, "[FRAME] failed to persist frame; using it in memory");
            }
        }
        Some(Err(_)) => tracing::warn!(session_id, "[FRAME] db lock poisoned; frame not persisted"),
        None => {}
    }
    (frame, Some(reason))
}

/// Context continuity P3 — claim the once-per-turn `<turn_context>` slot:
/// true exactly once per flag (the first `before_llm_call` of the turn).
pub(crate) fn claim_once(flag: &std::sync::atomic::AtomicBool) -> bool {
    flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok()
}

/// The user's own text of the turn's prompt message
/// (`messages[current_turn_start]`, first Text block) with a generated
/// `<turn_context>` prefix stripped. `""` when the turn start is not tracked.
pub(crate) fn turn_user_text(ctx: &ReasoningContext) -> String {
    ctx.current_turn_start
        .and_then(|i| ctx.messages.get(i))
        .and_then(|m| {
            m.content.iter().find_map(|b| match b {
                ContentBlock::Text { text } => Some(crate::agent::turn_context::strip(text).to_string()),
                _ => None,
            })
        })
        .unwrap_or_default()
}

/// Context continuity P3 — compute this turn's `<turn_context>` prefix once,
/// persist it on the turn's prompt row and splice it into the live prompt
/// message (`ctx.messages[current_turn_start]`), so the request and every
/// later replay carry the same bytes.
///
/// The ledger ("what the model has already seen") is built from the
/// `dynamic_prefix` of the active rows of OTHER turns, in order. When the
/// prompt row already has a prefix (retry / resume) nothing is recomputed:
/// the live message is aligned with the stored prefix and `None` returned.
/// `None` too when the turn start is untracked or the row can't carry a
/// prefix (`set_dynamic_prefix` refused) — the live message is then left
/// untouched so request == replay still holds.
///
/// Tool-set changes are written to the frame (`update_tool_names`) in the
/// same transaction: the frame's `tool_names` thus tracks the last announced
/// set (Task 10's sticky browser tools read it), which is why the ledger's
/// `tools_update` note is ignored here (it would otherwise produce a
/// spurious "back to the frame's list" block the turn after every change).
/// Consequently `TurnContextReport::tools_restored` / `TOOLS_BACK_TO_FRAME`
/// are never produced in production. Project rules work the other way: the
/// frame's `rules_hash` stays frozen and the ledger decides (see
/// [`rules_update_for`]).
pub(crate) fn apply_turn_context(
    conn: &rusqlite::Connection,
    session_id: &str,
    turn_id: &str,
    ctx: &mut ReasoningContext,
    inputs: &crate::agent::turn_context::TurnContextInputs,
    frame: &crate::agent::session_frame::SessionFrame,
) -> anyhow::Result<Option<crate::agent::turn_context::TurnContextReport>> {
    use crate::agent::transcript::{self, TranscriptKind};
    use crate::agent::turn_context::{self, TurnContextLedger};
    let Some(idx) = ctx
        .current_turn_start
        .filter(|&i| ctx.messages.get(i).is_some_and(|m| m.role == MessageRole::User))
    else {
        return Ok(None);
    };
    let rows = transcript::load_active(conn, session_id)?;
    let this_turn = |r: &transcript::TranscriptRow| r.turn_id.as_deref() == Some(turn_id);
    let prompt_row = rows
        .iter()
        .find(|r| this_turn(r) && r.kind == TranscriptKind::Message && r.message.role == MessageRole::User);
    if let Some(existing) = prompt_row.and_then(|r| r.dynamic_prefix.as_deref()) {
        let live_has_prefix = ctx.messages[idx].content.iter().find_map(|b| match b {
            ContentBlock::Text { text } => Some(text.starts_with(turn_context::OPEN)),
            _ => None,
        });
        if live_has_prefix == Some(false) {
            ctx.messages[idx] = transcript::merge_dynamic_prefix(existing, ctx.messages[idx].clone());
        }
        return Ok(None);
    }
    let mut ledger = TurnContextLedger::from_prefixes(
        rows.iter().filter(|r| !this_turn(r)).filter_map(|r| r.dynamic_prefix.as_deref()),
    );
    ledger.sticky.remove("tools_update");
    // Rules back at the frame's baseline while the active ledger still holds
    // an announced update (A → B → A): announce the restore (once — the
    // sticky then matches).
    let restore;
    let inputs = if inputs.rules_update.is_none() && ledger.sticky.contains_key("project_rules_update") {
        restore = turn_context::TurnContextInputs { rules_update: Some(RULES_RESTORED.to_string()), ..inputs.clone() };
        &restore
    } else {
        inputs
    };
    let (prefix, report) = turn_context::render(inputs, &ledger, frame);

    let tx = conn.unchecked_transaction()?;
    if !transcript::set_dynamic_prefix(&tx, session_id, turn_id, &prefix)? {
        tracing::warn!(session_id, turn_id, "[TURN_CTX] prompt row cannot carry a prefix; turn context skipped");
        return Ok(None);
    }
    if !report.tools_added.is_empty() || !report.tools_removed.is_empty() {
        crate::agent::session_frame::update_tool_names(
            &tx,
            session_id,
            &inputs.current_tool_names,
            inputs.now.timestamp_millis(),
        )?;
    }
    tx.commit()?;
    ctx.messages[idx] = transcript::merge_dynamic_prefix(&prefix, ctx.messages[idx].clone());
    Ok(Some(report))
}

/// Cap of [`previous_turn_tool_errors`] (same as `recent_tool_errors`).
const PREVIOUS_TURN_TOOL_ERRORS_MAX: usize = 20;

/// GEP error signals for a new turn (Decision 19): the failed tool results
/// (`is_error = true`, synthesized cancellations excluded) of the last
/// active turn before `current_turn_id`, as `"{tool_name}: {text}"` (text
/// capped at 200 chars, at most 20 entries) — the shape `execute_tool_calls`
/// pushes into `recent_tool_errors`, which a per-turn delegate starts empty.
pub(crate) fn previous_turn_tool_errors(
    conn: &rusqlite::Connection,
    session_id: &str,
    current_turn_id: &str,
) -> anyhow::Result<Vec<String>> {
    let rows = crate::agent::transcript::load_active(conn, session_id)?;
    let Some(prev) = rows
        .iter()
        .rev()
        .filter_map(|r| r.turn_id.as_deref())
        .find(|t| *t != current_turn_id)
    else {
        return Ok(Vec::new());
    };
    let mut names: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    let mut errors = Vec::new();
    for r in rows.iter().filter(|r| r.turn_id.as_deref() == Some(prev)) {
        for b in &r.message.content {
            match b {
                ContentBlock::ToolUse { id, name, .. } => {
                    names.insert(id.as_str(), name.as_str());
                }
                ContentBlock::ToolResult { tool_use_id, content, is_error: Some(true) }
                    if content != crate::agent::transcript_history::CANCELLED_TOOL_RESULT
                        && errors.len() < PREVIOUS_TURN_TOOL_ERRORS_MAX =>
                {
                    let name = names.get(tool_use_id.as_str()).copied().unwrap_or("tool");
                    errors.push(format!("{name}: {}", super::truncate_utf8(content, 200)));
                }
                _ => {}
            }
        }
    }
    Ok(errors)
}

/// `Some(rules_update_text(..))` when the workspace's rules source hash
/// differs from the frame's — the frame's hash is the frozen baseline and is
/// never moved by an announcement: whether the model has already seen this
/// text is decided by the ledger (the `project_rules_update` sticky), so a
/// rewind / compaction that drops the announcing turn re-announces it. A
/// revert to the baseline (`None` here) is announced by `apply_turn_context`
/// with [`RULES_RESTORED`].
pub(crate) fn rules_update_for(
    root: &std::path::Path,
    frame: &crate::agent::session_frame::SessionFrame,
) -> Option<String> {
    use crate::agent::rule_context_builder::RuleContextBuilder;
    if RuleContextBuilder::rules_source_hash(root) == frame.rules_hash {
        return None;
    }
    let uclaw_md = std::fs::read_to_string(root.join("uclaw.md")).unwrap_or_default();
    Some(rules_update_text(&uclaw_md, &RuleContextBuilder::build_static_context(root)))
}

/// `<project_rules_update>` body when the rules files are back at the
/// frame's frozen baseline after an announced change (A → B → A): it
/// supersedes the earlier update so the model stops following B.
pub(crate) const RULES_RESTORED: &str =
    "项目规则文件已恢复：以系统提示词中的 uclaw.md 内容与 ACTIVE PROJECT RULES 为准，此前的项目规则更新作废。";

/// `<project_rules_update>` body after the rules source hash changed: the
/// new `uclaw.md` text and static project rules, declared to supersede the
/// frozen system prompt's copies (Decision 9).
pub(crate) fn rules_update_text(uclaw_md: &str, static_rules: &str) -> String {
    let mut parts = vec![
        "项目规则文件已更新：以下内容取代系统提示词中对应的部分（uclaw.md 内容与 ACTIVE PROJECT RULES）。".to_string(),
    ];
    if !uclaw_md.trim().is_empty() {
        parts.push(format!("## uclaw.md\n{}", uclaw_md.trim()));
    }
    if !static_rules.trim().is_empty() {
        parts.push(static_rules.trim().to_string());
    }
    parts.join("\n\n")
}

/// `agent:prompt_frame` payload (camelCase). `rebuilt` = the frame was
/// (re)built by this turn.
pub(crate) fn prompt_frame_payload(
    conversation_id: &str,
    frame: &crate::agent::session_frame::SessionFrame,
    turn_id: &str,
    report: &crate::agent::turn_context::TurnContextReport,
    tool_count: usize,
) -> serde_json::Value {
    let rebuilt = frame.rebuilt_turn_id.as_deref() == Some(turn_id);
    serde_json::json!({
        "conversationId": conversation_id,
        "frozen": true,
        "rebuilt": rebuilt,
        "rebuildReason": if rebuilt { serde_json::to_value(frame.rebuild_reason).unwrap_or_default() } else { serde_json::Value::Null },
        "systemPromptTokens": crate::agent::types::estimate_tokens(&frame.system_prompt),
        "toolCount": tool_count,
        "modeChanged": report.mode_changed.as_ref().map(|(from, to)| serde_json::json!({ "from": from, "to": to })),
        "toolsAdded": report.tools_added,
        "toolsRemoved": report.tools_removed,
        "memory": report.memory,
        "memoryTokens": report.memory_tokens,
        "turnContextTokens": report.total_tokens,
    })
}

/// Context continuity P2 — the `<rewound_file_changes>` block for this turn:
/// the session's modified files from rewound/forked-away turns, whose tool
/// calls are no longer in the transcript. `""` without a sink (non-transcript
/// mode), without rewound entries, or when the DB is unavailable (logged).
pub(crate) fn rewound_changes_from_sink(sink: Option<&super::TranscriptSink>) -> String {
    let Some(sink) = sink else { return String::new() };
    let loaded = match sink.db.lock() {
        Ok(conn) => crate::agent::file_working_set::load(&conn, &sink.session_id),
        Err(_) => Err(anyhow::anyhow!("transcript sink DB mutex poisoned")),
    };
    match loaded {
        Ok(ws) => crate::agent::file_working_set::render_rewound_changes(&ws, REWOUND_CHANGES_MAX_TOKENS),
        Err(e) => {
            tracing::warn!(error = %e, "file working set unavailable; rewound file changes not surfaced");
            String::new()
        }
    }
}

/// Append the rewound-changes block to the per-turn dynamic context
/// (prepended to the last user message by `build_request_messages`).
pub(crate) fn with_rewound_changes(dynamic: String, block: &str) -> String {
    match (dynamic.is_empty(), block.is_empty()) {
        (_, true) => dynamic,
        (true, false) => block.to_string(),
        (false, false) => format!("{dynamic}\n\n{block}"),
    }
}

/// `agent:context_compacting` payload (camelCase keys; `trigger` is
/// "auto" | "manual" | "overflow").
pub(crate) fn context_compacting_payload(
    session_id: &str,
    trigger: crate::agent::types::CompactionTrigger,
) -> serde_json::Value {
    serde_json::json!({ "sessionId": session_id, "trigger": trigger.as_str() })
}

/// `agent:context_compaction_failed` payload (camelCase keys).
pub(crate) fn context_compaction_failed_payload(session_id: &str) -> serde_json::Value {
    serde_json::json!({ "sessionId": session_id })
}

/// `agent:context_compacted` payload (camelCase keys; `trigger` is
/// "auto" | "manual" | "overflow").
pub(crate) fn context_compacted_payload(
    session_id: &str,
    compacted_messages: usize,
    summary_tokens: usize,
    threshold_tokens: usize,
    trigger: crate::agent::types::CompactionTrigger,
) -> serde_json::Value {
    serde_json::json!({
        "sessionId": session_id,
        "compactedMessages": compacted_messages,
        "summaryTokens": summary_tokens,
        "thresholdTokens": threshold_tokens,
        "trigger": trigger.as_str(),
    })
}

/// DB half of `ChatDelegate::replace_with_session_block` (testable without an
/// AppHandle). By the 1:1 invariant the first `superseded_through` active
/// transcript rows are `messages[..cut]`: they are replaced by `block` (a
/// `session_block` row tagged with the sink's current turn id) followed by the
/// kept tail, atomically (`transcript::replace_prefix_with_session_block`). Then a display row (role `user`, content = block text) is added
/// to `agent_messages` just before the first kept row's turn so the UI's
/// `CompactionFoldCard` renders in place; that part is best-effort. Returns
/// the number of compacted messages (a leading old session_block excluded).
pub(crate) fn replace_session_block_in_sink(
    sink: &super::TranscriptSink,
    block: &ChatMessage,
    fold: Option<&crate::agent::compact::StructuredFold>,
    superseded_through: usize,
) -> anyhow::Result<usize> {
    let conn = sink
        .db
        .lock()
        .map_err(|_| anyhow::anyhow!("transcript sink DB mutex poisoned"))?;
    let rows = crate::agent::transcript::load_active(&conn, &sink.session_id)?;
    // A previous session_block at the head is re-folded, not a compacted message.
    let compacted_messages = match rows.first() {
        Some(r) if r.kind == crate::agent::transcript::TranscriptKind::SessionBlock => {
            superseded_through.saturating_sub(1)
        }
        _ => superseded_through,
    };
    // The first kept row's turn (its agent_messages user row) anchors the
    // display row; fall back to the current turn.
    let anchor_turn = rows
        .get(superseded_through)
        .and_then(|r| r.turn_id.clone())
        .unwrap_or_else(|| sink.turn_id.clone());
    // Validates `superseded_through`; atomic supersede + block + kept tail.
    crate::agent::transcript::replace_prefix_with_session_block_folded(
        &conn,
        &sink.session_id,
        Some(&sink.turn_id),
        block,
        fold,
        superseded_through,
    )?;

    // Best-effort from here: the transcript is already consistent.
    let text: String = block
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let now = chrono::Utc::now().timestamp_millis();
    let created_at = match conn.query_row(
        "SELECT created_at FROM agent_messages WHERE id = ?1 AND session_id = ?2",
        rusqlite::params![anchor_turn, sink.session_id],
        |r| r.get::<_, i64>(0),
    ) {
        Ok(ts) => ts - 1,
        Err(e) => {
            tracing::warn!(error = %e, anchor_turn = %anchor_turn, "compaction display row: anchor lookup failed; using now");
            now
        }
    };
    let inserted = conn.execute(
        "INSERT INTO agent_messages (id, session_id, role, content, created_at, compacted)
         VALUES (?1, ?2, 'user', ?3, ?4, 0)",
        rusqlite::params![uuid::Uuid::new_v4().to_string(), sink.session_id, text, created_at],
    );
    match inserted {
        Ok(_) => {
            if let Err(e) = conn.execute(
                "UPDATE agent_sessions SET message_count = message_count + 1, updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, sink.session_id],
            ) {
                tracing::warn!(error = %e, "compaction display row: message_count update failed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "failed to insert compaction display row"),
    }
    Ok(compacted_messages)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod session_frame_tests {
    use super::*;
    use crate::agent::dispatcher::content_assembler::{assemble_frozen_system_prompt, FrameInputs};
    use crate::agent::session_frame::{self as sf, FrameKey, FrameRebuildReason, ManifestStats};
    use crate::safety::SafetyMode;
    use std::sync::Mutex;

    fn db() -> Mutex<rusqlite::Connection> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        Mutex::new(conn)
    }

    fn inputs(base: &str, mode: SafetyMode, persona: &str) -> FrameInputs {
        FrameInputs {
            base_system_prompt: base.to_string(),
            workspace_root: None,
            mode,
            persona_block: Some(persona.to_string()),
            skills_manifest_block: "## 可用技能\n\n- **a** — x\n\n共 1 个技能，此处列出 1 个。".into(),
            static_rules_block: String::new(),
        }
    }

    fn parts(i: &FrameInputs) -> FrameBuildParts {
        FrameBuildParts {
            system_prompt: assemble_frozen_system_prompt(i),
            mode: sf::mode_name(&i.mode).to_string(),
            rules_hash: "rh".into(),
            tool_names: vec!["read_file".into()],
            manifest: ManifestStats { listed: 1, total: 1, tokens: 10 },
        }
    }

    /// One turn: resolve the frame the way `ChatDelegate::session_frame` does.
    fn turn(
        db: &Mutex<rusqlite::Connection>,
        turn_id: &str,
        base: &str,
        mode: SafetyMode,
        persona: &str,
    ) -> (sf::SessionFrame, Option<FrameRebuildReason>, bool) {
        let built = std::cell::Cell::new(false);
        let key = FrameKey::compute(base, None);
        let fut = load_or_rebuild_frame(Some(db), "s1", turn_id, key, 1_000, |_| {
            built.set(true);
            std::future::ready(parts(&inputs(base, mode, persona)))
        });
        let (f, r) = futures::executor::block_on(fut);
        (f, r, built.get())
    }

    #[test]
    fn frame_reused_across_turns_byte_identical() {
        let db = db();
        let (f1, r1, b1) = turn(&db, "t1", "BASE", SafetyMode::Supervised, "P");
        assert_eq!(r1, Some(FrameRebuildReason::New));
        assert!(b1);
        let (f2, r2, b2) = turn(&db, "t2", "BASE", SafetyMode::Supervised, "P");
        assert_eq!(r2, None);
        assert!(!b2, "a matching key must not rebuild");
        assert_eq!(f1.system_prompt.as_bytes(), f2.system_prompt.as_bytes());
        assert_eq!(f2.rebuilt_turn_id.as_deref(), Some("t1"));
        assert_eq!(sf::load(&db.lock().unwrap(), "s1").unwrap().unwrap(), f2);
    }

    #[test]
    fn frame_rebuilt_on_prompt_change_with_reason() {
        let db = db();
        let (f1, ..) = turn(&db, "t1", "BASE", SafetyMode::Supervised, "P");
        let (f2, r2, b2) = turn(&db, "t2", "BASE v2", SafetyMode::Supervised, "P");
        assert!(b2);
        assert_eq!(r2, Some(FrameRebuildReason::PromptChanged));
        assert_ne!(f1.system_prompt, f2.system_prompt);
        assert!(f2.system_prompt.starts_with("BASE v2"));
        assert_eq!(f2.rebuilt_turn_id.as_deref(), Some("t2"));
        assert_eq!(f2.created_at, f1.created_at, "rebuild keeps created_at");
        let stored = sf::load(&db.lock().unwrap(), "s1").unwrap().unwrap();
        assert_eq!(stored.rebuild_reason, FrameRebuildReason::PromptChanged);
        assert_eq!(stored.system_prompt, f2.system_prompt);
    }

    #[test]
    fn frame_not_rebuilt_on_mode_or_persona_change() {
        let db = db();
        let (f1, ..) = turn(&db, "t1", "BASE", SafetyMode::Supervised, "PERSONA_A");
        let (f2, r2, b2) = turn(&db, "t2", "BASE", SafetyMode::Plan, "PERSONA_B");
        assert_eq!(r2, None);
        assert!(!b2);
        assert_eq!(f1.system_prompt, f2.system_prompt);
        assert!(f2.system_prompt.contains("PERSONA_A"));
        assert!(!f2.system_prompt.contains("PERSONA_B"));
        assert_eq!(f2.mode, "supervised", "frame keeps the mode it was built with");
    }

    #[test]
    fn existing_session_without_frame_gets_new_frame() {
        use crate::agent::transcript::{self, TranscriptKind};
        use uclaw_message_types::ChatMessage;
        let db = db();
        {
            let conn = db.lock().unwrap();
            transcript::append(
                &conn,
                "s1",
                Some("u0"),
                TranscriptKind::Message,
                &[ChatMessage::user("old q"), ChatMessage::assistant("old a")],
            )
            .unwrap();
        }
        let before = serde_json::to_string(
            &transcript::load_active(&db.lock().unwrap(), "s1").unwrap().iter().map(|r| &r.message).collect::<Vec<_>>(),
        )
        .unwrap();
        let (f, r, b) = turn(&db, "u1", "BASE", SafetyMode::Supervised, "P");
        assert!(b);
        assert_eq!(r, Some(FrameRebuildReason::New));
        assert_eq!(f.rebuilt_turn_id.as_deref(), Some("u1"));
        let after = serde_json::to_string(
            &transcript::load_active(&db.lock().unwrap(), "s1").unwrap().iter().map(|r| &r.message).collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(before, after, "old rows replay unchanged");
    }

    #[test]
    fn corrupt_frame_row_is_rebuilt_not_fatal() {
        let db = db();
        turn(&db, "t1", "BASE", SafetyMode::Supervised, "P");
        db.lock()
            .unwrap()
            .execute("UPDATE agent_session_frames SET frame_key_json = 'garbage' WHERE session_id = 's1'", [])
            .unwrap();
        let (f, r, b) = turn(&db, "t2", "BASE", SafetyMode::Supervised, "P");
        assert!(b);
        assert_eq!(r, Some(FrameRebuildReason::New));
        assert_eq!(sf::load(&db.lock().unwrap(), "s1").unwrap().unwrap(), f);
    }

    /// Decision 12: the session manifest is built (and its learned skills
    /// bumped) only inside the (re)build — a reuse turn never builds it.
    #[test]
    fn manifest_builder_runs_only_on_rebuild() {
        let db = db();
        let manifest_builds = std::cell::Cell::new(0u32);
        let run = |turn_id: &str, base: &str| {
            let key = FrameKey::compute(base, None);
            futures::executor::block_on(load_or_rebuild_frame(Some(&db), "s1", turn_id, key, 1, |_| async {
                // stands in for FrameManifestSource::build + bump
                manifest_builds.set(manifest_builds.get() + 1);
                parts(&inputs(base, SafetyMode::Supervised, "P"))
            }))
        };
        run("t1", "BASE");
        assert_eq!(manifest_builds.get(), 1);
        run("t2", "BASE");
        run("t3", "BASE");
        assert_eq!(manifest_builds.get(), 1, "reuse turns must not build the manifest");
        run("t4", "BASE v2");
        assert_eq!(manifest_builds.get(), 2);
    }

    #[test]
    fn frame_without_db_is_built_in_memory() {
        let (f, r) = futures::executor::block_on(load_or_rebuild_frame(None, "s1", "", FrameKey::compute("B", None), 5, |_| {
            std::future::ready(parts(&inputs("B", SafetyMode::Supervised, "P")))
        }));
        assert_eq!(r, Some(FrameRebuildReason::New));
        assert!(f.system_prompt.starts_with('B'));
    }
}

#[cfg(test)]
mod transcript_sink_tests {
    use super::*;
    use crate::agent::dispatcher::TranscriptSink;
    use crate::agent::transcript::{self, TranscriptKind};
    use std::sync::{Arc, Mutex};

    #[test]
    fn tool_results_cleared_payload_and_sink_overlay() {
        assert_eq!(
            tool_results_cleared_payload("s1", &["a".to_string(), "b".to_string()], 21_000),
            serde_json::json!({"sessionId": "s1", "toolCallIds": ["a", "b"], "freedTokens": 21_000})
        );
        assert!(persist_cleared_in_sink(None, &[(0, ChatMessage::user("x"))]).is_ok());
        let s = sink();
        persist_to_sink(Some(&s), &[ChatMessage::user("orig")]).unwrap();
        persist_cleared_in_sink(Some(&s), &[(0, ChatMessage::user("cleared"))]).unwrap();
        let rows = transcript::load_active(&s.db.lock().unwrap(), &s.session_id).unwrap();
        assert!(serde_json::to_string(&rows[0].message).unwrap().contains("cleared"));
    }

    fn sink() -> TranscriptSink {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        TranscriptSink {
            db: Arc::new(Mutex::new(conn)),
            session_id: "s1".into(),
            turn_id: "t1".into(),
        }
    }

    #[test]
    fn persist_with_sink_appends_rows_in_order() {
        let sink = sink();
        persist_to_sink(Some(&sink), &[ChatMessage::user("one"), ChatMessage::assistant("two")])
            .unwrap();
        persist_to_sink(Some(&sink), &[ChatMessage::user("three")]).unwrap();

        let rows = transcript::load_active(&sink.db.lock().unwrap(), "s1").unwrap();
        let texts: Vec<String> = rows
            .iter()
            .map(|r| match &r.message.content[0] {
                ContentBlock::Text { text } => text.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(texts, vec!["one", "two", "three"]);
        assert!(rows.iter().all(|r| r.kind == TranscriptKind::Message));
        assert!(rows.iter().all(|r| r.turn_id.as_deref() == Some("t1")));
    }

    #[test]
    fn persist_without_sink_is_noop() {
        assert!(persist_to_sink(None, &[ChatMessage::user("x")]).is_ok());
    }

    fn text_of(m: &ChatMessage) -> String {
        match &m.content[0] {
            ContentBlock::Text { text } => text.clone(),
            _ => String::new(),
        }
    }

    #[test]
    fn context_compacting_payload_is_camel_case_with_trigger() {
        use crate::agent::types::CompactionTrigger;
        assert_eq!(
            context_compacting_payload("s1", CompactionTrigger::Auto),
            serde_json::json!({ "sessionId": "s1", "trigger": "auto" })
        );
        assert_eq!(context_compacting_payload("s1", CompactionTrigger::Overflow)["trigger"], "overflow");
    }

    #[test]
    fn context_compaction_failed_payload_is_camel_case() {
        assert_eq!(context_compaction_failed_payload("s1"), serde_json::json!({ "sessionId": "s1" }));
    }

    #[test]
    fn context_compacted_payload_carries_trigger() {
        use crate::agent::types::CompactionTrigger;
        let auto = context_compacted_payload("s1", 12, 800, 128_000, CompactionTrigger::Auto);
        assert_eq!(
            auto,
            serde_json::json!({
                "sessionId": "s1",
                "compactedMessages": 12,
                "summaryTokens": 800,
                "thresholdTokens": 128_000,
                "trigger": "auto",
            })
        );
        let manual = context_compacted_payload("s1", 3, 50, 64_000, CompactionTrigger::Manual);
        assert_eq!(manual["trigger"], "manual");
        let overflow = context_compacted_payload("s1", 3, 50, 64_000, CompactionTrigger::Overflow);
        assert_eq!(overflow["trigger"], "overflow");
    }

    #[test]
    fn replace_with_session_block_supersedes_and_appends() {
        let t1 = sink();
        let t2 = TranscriptSink { db: t1.db.clone(), session_id: "s1".into(), turn_id: "t2".into() };
        {
            let conn = t1.db.lock().unwrap();
            conn.execute("INSERT INTO agent_sessions (id, created_at, updated_at) VALUES ('s1', 0, 0)", [])
                .unwrap();
            for (id, ts) in [("t1", 1_000i64), ("t2", 2_000)] {
                conn.execute(
                    "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1,'s1','user',?1,?2)",
                    rusqlite::params![id, ts],
                )
                .unwrap();
            }
        }
        persist_to_sink(Some(&t1), &[ChatMessage::user("u1"), ChatMessage::assistant("a1")]).unwrap();
        persist_to_sink(Some(&t2), &[ChatMessage::user("u2"), ChatMessage::assistant("a2")]).unwrap();

        let block = ChatMessage::user("## Earlier conversation (compacted)\n- fold");
        replace_session_block_in_sink(&t2, &block, None, 2).unwrap();

        let conn = t1.db.lock().unwrap();
        let rows = transcript::load_active(&conn, "s1").unwrap();
        let texts: Vec<String> = rows.iter().map(|r| text_of(&r.message)).collect();
        assert_eq!(texts, vec!["## Earlier conversation (compacted)\n- fold", "u2", "a2"]);
        assert_eq!(rows[0].kind, TranscriptKind::SessionBlock);
        assert_eq!(rows[0].turn_id.as_deref(), Some("t2"), "block is tagged with the current turn");

        let (content, created_at): (String, i64) = conn
            .query_row(
                "SELECT content, created_at FROM agent_messages
                 WHERE session_id = 's1' AND content LIKE '## Earlier conversation (compacted)%'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(content.starts_with("## Earlier conversation (compacted)"));
        assert!(created_at > 1_000 && created_at < 2_000, "display row sits before the first kept turn, got {created_at}");
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_messages WHERE session_id = 's1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 3);
    }

    #[test]
    fn replace_with_session_block_reports_compacted_messages_excluding_old_block() {
        let t1 = sink();
        persist_to_sink(Some(&t1), &[ChatMessage::user("u1"), ChatMessage::assistant("a1")]).unwrap();
        let first = replace_session_block_in_sink(&t1, &ChatMessage::user("## Earlier conversation (compacted)\nb1"), None, 1)
            .unwrap();
        assert_eq!(first, 1);
        persist_to_sink(Some(&t1), &[ChatMessage::user("u2")]).unwrap();
        // Active: [b1, a1, u2]; compacting [b1, a1] summarises one new message.
        let second = replace_session_block_in_sink(&t1, &ChatMessage::user("## Earlier conversation (compacted)\nb2"), None, 2)
            .unwrap();
        assert_eq!(second, 1, "the previous session_block is not a compacted message");
    }

    fn write_outcome(path: &str) -> crate::agent::tool_dispatch::ToolDispatchOutcome {
        crate::agent::tool_dispatch::ToolDispatchOutcome {
            tool_call_id: "c1".into(),
            tool_name: "write_file".into(),
            arguments: serde_json::json!({"path": path, "content": "a\nb\n"}),
            result: Ok(crate::agent::tools::tool::ToolOutput::success("ok", 1)),
            paths_touched: vec![],
            was_mutation: true,
            soft_error: None,
            rejected: false,
            message_content: "ok".into(),
            is_error: false,
        }
    }

    #[test]
    fn transcript_mode_records_file_events_after_tool_batch() {
        let t1 = sink();
        let root = std::path::Path::new("/ws");
        let n = record_file_events_in_sink(Some(&t1), &[write_outcome("/ws/src/new.rs")], Some(root))
            .unwrap();
        assert_eq!(n, 1);
        let conn = t1.db.lock().unwrap();
        let set = crate::agent::file_working_set::load(&conn, "s1").unwrap();
        assert_eq!(set.modified.len(), 1);
        assert_eq!(set.modified[0].path, "src/new.rs");
        assert!(set.modified[0].created);
        let turn: Option<String> = conn
            .query_row("SELECT turn_id FROM agent_file_events WHERE session_id = 's1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(turn.as_deref(), Some("t1"), "events are tagged with the sink's turn id");
        drop(conn);

        // No sink (headless / non-transcript) → nothing recorded.
        assert_eq!(record_file_events_in_sink(None, &[write_outcome("x.rs")], Some(root)).unwrap(), 0);
    }

    #[test]
    fn replace_with_session_block_rejects_cut_beyond_transcript() {
        let t1 = sink();
        persist_to_sink(Some(&t1), &[ChatMessage::user("u1")]).unwrap();
        let block = ChatMessage::user("## Earlier conversation (compacted)");
        assert!(replace_session_block_in_sink(&t1, &block, None, 2).is_err());
        assert!(replace_session_block_in_sink(&t1, &block, None, 0).is_err());
        let rows = transcript::load_active(&t1.db.lock().unwrap(), "s1").unwrap();
        assert_eq!(rows.len(), 1, "transcript untouched on rejection");
    }
}

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

#[cfg(test)]
mod build_request_messages_tests {
    use super::*;
    use crate::agent::turn::TurnSnapshot;
    use crate::agent::types::MessageRole;

    fn fixture() -> (ReasoningContext, TurnSnapshot) {
        let mut ctx = ReasoningContext::new("base".into());
        let mut gone = ChatMessage::user("compacted away");
        gone.compacted = true;
        let shot = r#"{"ok":true,"data":"QUJD","width":10,"height":10}"#;
        ctx.messages = vec![
            gone,
            ChatMessage::user("first request"),
            // orphan tool call (no result) → synthesized aborted result
            ChatMessage::assistant_with_tool_use("t_orphan", "read", serde_json::json!({"path": "a.rs"})),
            ChatMessage::user("second request"),
            ChatMessage::assistant_with_tool_use("t_shot", "browser_screenshot", serde_json::json!({})),
            ChatMessage::user_tool_result("t_shot", shot, false),
            ChatMessage::assistant("looked at it"),
        ];
        let snap = TurnSnapshot {
            turn_index: 1,
            model: "m".into(),
            system_prompt: std::sync::Arc::new("SYSTEM PROMPT".into()),
            dynamic_context: "<dyn>now</dyn>".into(),
            tools: std::sync::Arc::new(Vec::new()),
            force_text: false,
        };
        (ctx, snap)
    }

    /// Recorded (baseline capture) from a verbatim copy of the pre-extraction
    /// message construction inside `ChatDelegate::call_llm`, before
    /// `build_request_messages` existed. `ChatDelegate` itself can't be built
    /// in unit tests (it holds a Wry `AppHandle`), so the golden is the proof
    /// the extraction is behaviour-preserving.
    const GOLDEN_IMAGE_BLIND: &str = r#"[{"role":"system","content":[{"type":"text","text":"SYSTEM PROMPT"}],"compacted":false},{"role":"user","content":[{"type":"text","text":"first request"}],"compacted":false},{"role":"assistant","content":[{"type":"tool_use","id":"t_orphan","name":"read","input":{"path":"a.rs"}}],"compacted":false},{"role":"user","content":[{"type":"tool_result","tool_use_id":"t_orphan","content":"aborted","is_error":true}],"compacted":false},{"role":"user","content":[{"type":"text","text":"<dyn>now</dyn>\n\nsecond request"}],"compacted":false},{"role":"assistant","content":[{"type":"tool_use","id":"t_shot","name":"browser_screenshot","input":{}}],"compacted":false},{"role":"user","content":[{"type":"tool_result","tool_use_id":"t_shot","content":"image content omitted because the current model does not support image input","is_error":false}],"compacted":false},{"role":"assistant","content":[{"type":"text","text":"looked at it"}],"compacted":false}]"#;
    const GOLDEN_IMAGE_CAPABLE: &str = r#"[{"role":"system","content":[{"type":"text","text":"SYSTEM PROMPT"}],"compacted":false},{"role":"user","content":[{"type":"text","text":"first request"}],"compacted":false},{"role":"assistant","content":[{"type":"tool_use","id":"t_orphan","name":"read","input":{"path":"a.rs"}}],"compacted":false},{"role":"user","content":[{"type":"tool_result","tool_use_id":"t_orphan","content":"aborted","is_error":true}],"compacted":false},{"role":"user","content":[{"type":"text","text":"<dyn>now</dyn>\n\nsecond request"}],"compacted":false},{"role":"assistant","content":[{"type":"tool_use","id":"t_shot","name":"browser_screenshot","input":{}}],"compacted":false},{"role":"user","content":[{"type":"tool_result","tool_use_id":"t_shot","content":"{\"ok\":true,\"data\":\"QUJD\",\"width\":10,\"height\":10}","is_error":false}],"compacted":false},{"role":"assistant","content":[{"type":"text","text":"looked at it"}],"compacted":false}]"#;

    #[test]
    fn build_request_messages_matches_call_llm_input() {
        let (ctx, snap) = fixture();
        for (supports, golden) in [(false, GOLDEN_IMAGE_BLIND), (true, GOLDEN_IMAGE_CAPABLE)] {
            let out = build_request_messages(&ctx, &snap, supports);
            assert_eq!(serde_json::to_string(&out).unwrap(), golden, "supports_images={supports}");
            assert!(matches!(out[0].role, MessageRole::System));
        }
    }
}

#[cfg(test)]
mod turn_context_wiring_tests {
    use super::*;
    use crate::agent::gep::retrieval::GeneRetriever;
    use crate::agent::gep::types::*;
    use crate::agent::session_frame::{FrameKey, FrameRebuildReason, ManifestStats, SessionFrame};
    use crate::agent::transcript::{self, TranscriptKind};
    use crate::agent::turn_context::{self, MemoryInjection, TurnContextInputs, TurnContextLedger};
    use crate::safety::SafetyMode;
    use chrono::TimeZone;
    use rusqlite::Connection;
    use std::sync::atomic::AtomicBool;

    const S: &str = "s1";
    const MEM: &str = "<memory_context>\n## Facts\n- [Fact] 用户偏好: 简洁回答\n- [Fact] 项目: uClaw\n</memory_context>";

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn frame(tools: &[&str]) -> SessionFrame {
        SessionFrame {
            session_id: S.into(),
            key: FrameKey { version: 1, prompt_sha: "p".into(), workspace: String::new(), static_sha: "s".into() },
            system_prompt: "FROZEN SYSTEM".into(),
            mode: "supervised".into(),
            rules_hash: "r".into(),
            tool_names: tools.iter().map(|s| s.to_string()).collect(),
            manifest: ManifestStats::default(),
            rebuild_reason: FrameRebuildReason::New,
            rebuilt_turn_id: Some("t1".into()),
            created_at: 0,
            updated_at: 0,
        }
    }

    fn inputs(memory: Option<&str>) -> TurnContextInputs {
        TurnContextInputs {
            now: chrono::Local.with_ymd_and_hms(2026, 11, 9, 14, 30, 0).unwrap(),
            memory_context: memory.map(str::to_string),
            learned_profile: "## User Profile (Learned)\n- 喜欢中文".into(),
            growth: String::new(),
            relevant_skills: String::new(),
            gene_block: String::new(),
            plan_suggest_hint: String::new(),
            glob_rules: Vec::new(),
            rules_update: None,
            context_fragments: Vec::new(),
            rewound_changes: String::new(),
            current_mode: SafetyMode::Supervised,
            current_tool_names: vec!["read".into(), "bash".into()],
        }
    }

    /// Start a turn the way `send_agent_message` does: the prompt row is
    /// appended to the transcript first, then pushed into the live context.
    fn start_turn(conn: &Connection, ctx: &mut ReasoningContext, turn: &str, text: &str) {
        let msg = crate::agent::transcript_history::append_current_user_turn(conn, S, turn, None, text).unwrap();
        ctx.messages.push(msg);
        ctx.transcript_persisted_len = ctx.messages.len();
        ctx.current_turn_start = Some(ctx.messages.len() - 1);
    }

    fn finish_turn(conn: &Connection, ctx: &mut ReasoningContext, turn: &str, reply: &str) {
        let m = ChatMessage::assistant(reply);
        transcript::append(conn, S, Some(turn), TranscriptKind::Message, std::slice::from_ref(&m)).unwrap();
        ctx.messages.push(m);
        ctx.transcript_persisted_len = ctx.messages.len();
    }

    fn json(m: &[ChatMessage]) -> String {
        serde_json::to_string(m).unwrap()
    }

    fn text(m: &ChatMessage) -> &str {
        m.content.iter().find_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        }).unwrap_or("")
    }

    fn active_messages(conn: &Connection, session: &str) -> Vec<ChatMessage> {
        transcript::load_active(conn, session).unwrap().into_iter().map(|r| r.message).collect()
    }

    fn prefixes(conn: &Connection, session: &str) -> Vec<String> {
        transcript::load_active(conn, session).unwrap().into_iter().filter_map(|r| r.dynamic_prefix).collect()
    }

    #[test]
    fn transcript_turn_context_applied_once_per_turn_and_persisted() {
        let conn = db();
        let mut ctx = ReasoningContext::new(String::new());
        start_turn(&conn, &mut ctx, "t1", "帮我看看");
        let applied = AtomicBool::new(false);
        let mut reports = Vec::new();
        for _iteration in 1..=2 {
            if claim_once(&applied) {
                reports.push(apply_turn_context(&conn, S, "t1", &mut ctx, &inputs(Some(MEM)), &frame(&["read", "bash"])).unwrap());
            }
        }
        assert_eq!(reports.len(), 1);
        let report = reports[0].clone().expect("first application renders");
        assert_eq!(report.memory, MemoryInjection::Full);
        // A second application (retry / resume) writes nothing and does not double-prefix.
        assert!(apply_turn_context(&conn, S, "t1", &mut ctx, &inputs(Some(MEM)), &frame(&["read", "bash"])).unwrap().is_none());
        assert_eq!(prefixes(&conn, S).len(), 1);
        let live = text(&ctx.messages[0]);
        assert!(live.starts_with(turn_context::OPEN));
        assert_eq!(live.matches(turn_context::OPEN).count(), 1);
        assert_eq!(turn_context::strip(live), "帮我看看");
        assert_eq!(json(&active_messages(&conn, S)), json(&ctx.messages), "replay == request bytes");
    }

    #[test]
    fn turn_context_attaches_only_to_turn_user_row() {
        let conn = db();
        let mut ctx = ReasoningContext::new(String::new());
        start_turn(&conn, &mut ctx, "t1", "主请求");
        apply_turn_context(&conn, S, "t1", &mut ctx, &inputs(Some(MEM)), &frame(&["read", "bash"])).unwrap().unwrap();
        // Mid-turn steering and a follow-up message, persisted with the same turn id.
        for m in [ChatMessage::assistant("ok"), ChatMessage::user("顺便改一下"), ChatMessage::assistant("done"), ChatMessage::user("下一个任务")] {
            transcript::append(&conn, S, Some("t1"), TranscriptKind::Message, std::slice::from_ref(&m)).unwrap();
            ctx.messages.push(m);
        }
        assert!(apply_turn_context(&conn, S, "t1", &mut ctx, &inputs(Some(MEM)), &frame(&["read", "bash"])).unwrap().is_none());
        let rows = transcript::load_active(&conn, S).unwrap();
        let with_prefix: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r.dynamic_prefix.is_some()).map(|(i, _)| i).collect();
        assert_eq!(with_prefix, vec![0]);
        assert_eq!(text(&rows[2].message), "顺便改一下");
        assert_eq!(text(&rows[4].message), "下一个任务");
        assert_eq!(json(&active_messages(&conn, S)), json(&ctx.messages));
    }

    #[test]
    fn second_turn_replays_first_turn_context_verbatim() {
        let conn = db();
        let mut ctx = ReasoningContext::new(String::new());
        start_turn(&conn, &mut ctx, "t1", "第一轮");
        apply_turn_context(&conn, S, "t1", &mut ctx, &inputs(Some(MEM)), &frame(&["read", "bash"])).unwrap().unwrap();
        finish_turn(&conn, &mut ctx, "t1", "好的");
        let turn1 = json(&ctx.messages);

        start_turn(&conn, &mut ctx, "t2", "第二轮");
        let report = apply_turn_context(&conn, S, "t2", &mut ctx, &inputs(Some(MEM)), &frame(&["read", "bash"])).unwrap().unwrap();
        assert_eq!(json(&ctx.messages[..2]), turn1, "earlier turn bytes unchanged");
        assert_eq!(report.memory, MemoryInjection::None, "same recall → nothing re-injected");
        let t2 = text(&ctx.messages[2]);
        assert!(!t2.contains("<memory_context>"));
        assert!(!t2.contains("<learned_profile>"), "sticky block unchanged → omitted");
        assert!(t2.contains("<system_info>"));
        assert_eq!(json(&active_messages(&conn, S)), json(&ctx.messages));
    }

    #[test]
    fn restart_replay_includes_persisted_turn_context() {
        let conn = db();
        let mut ctx = ReasoningContext::new(String::new());
        start_turn(&conn, &mut ctx, "t1", "第一轮");
        apply_turn_context(&conn, S, "t1", &mut ctx, &inputs(Some(MEM)), &frame(&["read", "bash"])).unwrap().unwrap();
        finish_turn(&conn, &mut ctx, "t1", "好的");
        // Restart: drop the in-memory context, rebuild from the transcript.
        let (replayed, persisted) = crate::agent::transcript_history::build_agent_history(&conn, S).unwrap();
        assert_eq!(persisted, ctx.messages.len());
        assert_eq!(json(&replayed), json(&ctx.messages));
    }

    #[test]
    fn fork_and_rewind_keep_prefix_and_ledger_consistent() {
        let conn = db();
        let mut ctx = ReasoningContext::new(String::new());
        let mem2 = "<memory_context>\n## Facts\n- [Fact] 用户偏好: 简洁回答\n- [Fact] 项目: uClaw\n- [Fact] 截止日期: 周五\n</memory_context>";
        start_turn(&conn, &mut ctx, "t1", "一");
        apply_turn_context(&conn, S, "t1", &mut ctx, &inputs(Some(MEM)), &frame(&["read", "bash"])).unwrap().unwrap();
        finish_turn(&conn, &mut ctx, "t1", "a");
        start_turn(&conn, &mut ctx, "t2", "二");
        let r2 = apply_turn_context(&conn, S, "t2", &mut ctx, &inputs(Some(mem2)), &frame(&["read", "bash"])).unwrap().unwrap();
        assert_eq!(r2.memory, MemoryInjection::Delta);
        finish_turn(&conn, &mut ctx, "t2", "b");

        // Fork through t1 into s2: rows keep their prefix.
        let n = transcript::clone_through_turn_within(&conn, S, "s2", "t1", &Default::default()).unwrap();
        assert_eq!(n, 2);
        assert_eq!(prefixes(&conn, "s2"), prefixes(&conn, S)[..1].to_vec());
        // Rewind s1 to t1: the ledger matches the fork's.
        transcript::truncate_after_turn(&conn, S, "t1").unwrap();
        let ledger = |s: &str| {
            let p = prefixes(&conn, s);
            let l = TurnContextLedger::from_prefixes(p.iter().map(String::as_str));
            (l.seen_memory_lines, l.sticky, l.last_mode)
        };
        assert_eq!(ledger(S), ledger("s2"));
        // The rewound delta line is new again for the next turn on either side.
        for session in [S, "s2"] {
            let mut c = ReasoningContext::new(String::new());
            c.messages = active_messages(&conn, session);
            let msg = crate::agent::transcript_history::append_current_user_turn(&conn, session, "t3", None, "三").unwrap();
            c.messages.push(msg);
            c.current_turn_start = Some(c.messages.len() - 1);
            let r = apply_turn_context(&conn, session, "t3", &mut c, &inputs(Some(mem2)), &frame(&["read", "bash"])).unwrap().unwrap();
            assert_eq!(r.memory, MemoryInjection::Delta, "{session}");
            assert!(text(c.messages.last().unwrap()).contains("截止日期"), "{session}");
        }
        // Compaction that moves every memory line out of the active context → full again.
        let max_seq: i64 = conn.query_row("SELECT MAX(seq) FROM agent_transcript WHERE session_id = 's2'", [], |r| r.get(0)).unwrap();
        transcript::supersede_up_to(&conn, "s2", max_seq).unwrap();
        let mut c = ReasoningContext::new(String::new());
        let msg = crate::agent::transcript_history::append_current_user_turn(&conn, "s2", "t4", None, "四").unwrap();
        c.messages.push(msg);
        c.current_turn_start = Some(0);
        let r = apply_turn_context(&conn, "s2", "t4", &mut c, &inputs(Some(mem2)), &frame(&["read", "bash"])).unwrap().unwrap();
        assert_eq!(r.memory, MemoryInjection::Full);
    }

    #[test]
    fn non_transcript_mode_keeps_ephemeral_splice() {
        let mut ctx = ReasoningContext::new(String::new());
        ctx.messages = vec![ChatMessage::user("hi")];
        let mut snap = crate::agent::turn::TurnSnapshot {
            turn_index: 1,
            model: "m".into(),
            system_prompt: std::sync::Arc::new("SYS".into()),
            dynamic_context: "<system_info>x</system_info>".into(),
            tools: std::sync::Arc::new(Vec::new()),
            force_text: false,
        };
        let out = build_request_messages(&ctx, &snap, true);
        assert_eq!(text(&out[1]), "<system_info>x</system_info>\n\nhi");
        assert_eq!(text(&ctx.messages[0]), "hi", "live context untouched");
        // Transcript mode: empty dynamic context → the persisted prefix is sent as-is.
        snap.dynamic_context = String::new();
        ctx.messages = vec![transcript::merge_dynamic_prefix("<turn_context>\nP\n</turn_context>", ChatMessage::user("hi"))];
        let out = build_request_messages(&ctx, &snap, true);
        assert_eq!(json(&out[1..]), json(&ctx.messages));
    }

    fn gene(id: &str, category: GeneCategory, signal: &str) -> Gene {
        Gene {
            gene_id: id.into(),
            version: "1.0".into(),
            category,
            signals_match: vec![signal.into()],
            summary: id.into(),
            strategy: vec!["step".into()],
            avoid: vec![],
            constraints: GeneConstraints { max_files: 3, forbidden_paths: vec![] },
            validation: "test".into(),
            asset_id: String::new(),
            status: GeneStatus::Active,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[tokio::test]
    async fn gep_matches_once_per_turn_on_stripped_user_text() {
        let conn = db();
        let mut ctx = ReasoningContext::new(String::new());
        start_turn(&conn, &mut ctx, "t1", "接口返回403");
        apply_turn_context(&conn, S, "t1", &mut ctx, &inputs(Some(MEM)), &frame(&["read", "bash"])).unwrap().unwrap();
        // "当前时间" only occurs in the generated prefix; it must not match.
        let retriever = GeneRetriever::new(
            vec![gene("on-403", GeneCategory::Repair, "403"), gene("prefix-only", GeneCategory::Optimize, "当前时间")],
            false,
            None,
        );
        let last_matches = std::sync::Mutex::new(Vec::new());
        let applied = AtomicBool::new(false);
        let mut blocks = Vec::new();
        for _iteration in 1..=3 {
            if claim_once(&applied) {
                let user_text = turn_user_text(&ctx);
                assert_eq!(user_text, "接口返回403");
                blocks.push(gene_signals(Some(&retriever), &last_matches, &user_text, &[]).await);
            }
        }
        assert_eq!(blocks.len(), 1, "matched once per turn");
        assert!(blocks[0].contains("on-403"));
        let ids: Vec<String> = last_matches.lock().unwrap().iter().map(|m| m.gene.gene_id.clone()).collect();
        assert_eq!(ids, vec!["on-403".to_string()], "gep.last_matches side effect kept");
    }

    #[test]
    fn summary_prompt_mentions_turn_context() {
        use crate::agent::compact::summarize::TURN_CONTEXT_NOTE;
        assert_eq!(
            TURN_CONTEXT_NOTE,
            "用户消息开头的 <turn_context> 块是系统注入的上下文（时间、记忆、提示），不是用户原话，不要当作用户要求记录。"
        );
        assert!(crate::agent::compact::summarize::build_system_prompt(1_000).contains(TURN_CONTEXT_NOTE));
        assert!(crate::agent::compact::summarize::build_update_system_prompt(1_000).contains(TURN_CONTEXT_NOTE));
        let anchor = crate::agent::compact::prefix_summary::CutAnchor::Everything;
        assert!(crate::agent::compact::prefix_summary::instruction(&anchor, 1_000).contains(TURN_CONTEXT_NOTE));
    }

    #[test]
    fn prefix_summary_anchor_uses_the_users_own_text() {
        let a = transcript::merge_dynamic_prefix("<turn_context>\n<system_info>\nT\n</system_info>\n</turn_context>", ChatMessage::user("第一件事"));
        let b = transcript::merge_dynamic_prefix("<turn_context>\n<system_info>\nT\n</system_info>\n</turn_context>", ChatMessage::user("第二件事"));
        let msgs = vec![a, ChatMessage::assistant("x"), b, ChatMessage::assistant("y")];
        let anchor = crate::agent::compact::prefix_summary::cut_anchor(&msgs, 2);
        assert!(
            matches!(anchor, Some(crate::agent::compact::prefix_summary::CutAnchor::Before { ref preview, .. }) if preview == "第二件事"),
            "{anchor:?}"
        );
    }

    #[test]
    fn tool_change_updates_frame_and_is_announced_once() {
        let conn = db();
        let mut f = frame(&["read", "bash", "mcp__x__y"]);
        sf_upsert(&conn, &f);
        let mut ctx = ReasoningContext::new(String::new());
        start_turn(&conn, &mut ctx, "t1", "一");
        let r1 = apply_turn_context(&conn, S, "t1", &mut ctx, &inputs(None), &f).unwrap().unwrap();
        assert_eq!(r1.tools_removed, vec!["mcp__x__y".to_string()]);
        assert!(text(&ctx.messages[0]).contains("移除: mcp__x__y"));
        let stored = crate::agent::session_frame::load(&conn, S).unwrap().unwrap();
        assert_eq!(stored.tool_names, vec!["read".to_string(), "bash".to_string()]);
        finish_turn(&conn, &mut ctx, "t1", "a");
        // Next turn sees the updated frame: no tools block at all.
        f = stored;
        start_turn(&conn, &mut ctx, "t2", "二");
        let r2 = apply_turn_context(&conn, S, "t2", &mut ctx, &inputs(None), &f).unwrap().unwrap();
        assert!(r2.tools_added.is_empty() && r2.tools_removed.is_empty() && !r2.tools_restored);
        assert!(!text(&ctx.messages[2]).contains("<tools_update>"));
    }

    fn sf_upsert(conn: &Connection, f: &SessionFrame) {
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at) VALUES (?1,'global','t',0,0)",
            [S],
        )
        .ok();
        crate::agent::session_frame::upsert(conn, f).unwrap();
    }

    #[test]
    fn rules_update_text_declares_replacement() {
        let t = rules_update_text("用 tab 缩进", "### ACTIVE PROJECT RULES\n- r");
        assert!(t.starts_with("项目规则文件已更新"));
        assert!(t.contains("## uclaw.md\n用 tab 缩进"));
        assert!(t.ends_with("### ACTIVE PROJECT RULES\n- r"));
        assert!(!rules_update_text("", "").contains("## uclaw.md"));
    }

    #[test]
    fn prompt_frame_payload_is_camel_case() {
        let report = crate::agent::turn_context::TurnContextReport {
            memory: MemoryInjection::Delta,
            memory_tokens: 12,
            total_tokens: 40,
            mode_changed: Some(("supervised".into(), "plan".into())),
            tools_added: vec!["a".into()],
            tools_removed: vec![],
            tools_restored: false,
        };
        let f = frame(&["read"]);
        let v = prompt_frame_payload("s1", &f, "t1", &report, 7);
        assert_eq!(v["conversationId"], "s1");
        assert_eq!(v["frozen"], true);
        assert_eq!(v["rebuilt"], true);
        assert_eq!(v["rebuildReason"], "new");
        assert_eq!(v["systemPromptTokens"], crate::agent::types::estimate_tokens("FROZEN SYSTEM"));
        assert_eq!(v["toolCount"], 7);
        assert_eq!(v["modeChanged"], serde_json::json!({"from": "supervised", "to": "plan"}));
        assert_eq!(v["toolsAdded"], serde_json::json!(["a"]));
        assert_eq!(v["toolsRemoved"], serde_json::json!([]));
        assert_eq!(v["memory"], "delta");
        assert_eq!(v["memoryTokens"], 12);
        assert_eq!(v["turnContextTokens"], 40);
        let v2 = prompt_frame_payload("s1", &f, "t2", &report, 7);
        assert_eq!(v2["rebuilt"], false);
        assert!(v2["rebuildReason"].is_null());
    }

    #[tokio::test]
    async fn gep_sees_previous_turns_tool_errors() {
        let conn = db();
        let mut ctx = ReasoningContext::new(String::new());
        start_turn(&conn, &mut ctx, "t1", "查一下行情");
        let call = ChatMessage::assistant_with_tool_use("c1", "web_fetch", serde_json::json!({"url": "x"}));
        let err = ChatMessage::user_tool_result("c1", "HTTP 403 Forbidden", true);
        let ok_call = ChatMessage::assistant_with_tool_use("c2", "read", serde_json::json!({}));
        let ok = ChatMessage::user_tool_result("c2", "403 lines read", false);
        for m in [call, err, ok_call, ok] {
            transcript::append(&conn, S, Some("t1"), TranscriptKind::Message, std::slice::from_ref(&m)).unwrap();
            ctx.messages.push(m);
        }
        finish_turn(&conn, &mut ctx, "t1", "失败了");
        start_turn(&conn, &mut ctx, "t2", "再试一次");
        let errors = previous_turn_tool_errors(&conn, S, "t2").unwrap();
        assert_eq!(errors, vec!["web_fetch: HTTP 403 Forbidden".to_string()]);
        let retriever = GeneRetriever::new(vec![gene("on-403", GeneCategory::Repair, "403")], false, None);
        let last_matches = std::sync::Mutex::new(Vec::new());
        let block = gene_signals(Some(&retriever), &last_matches, &turn_user_text(&ctx), &errors).await;
        assert!(block.contains("on-403"), "user text alone has no 403; the error signal must match");
        assert_eq!(last_matches.lock().unwrap()[0].gene.gene_id, "on-403");
        // Only the last turn counts: the current turn has none yet, and an earlier turn's errors are not repeated.
        finish_turn(&conn, &mut ctx, "t2", "好了");
        assert!(previous_turn_tool_errors(&conn, S, "t3").unwrap().is_empty());
    }

    #[test]
    fn rules_update_announced_from_ledger_and_reannounced_after_rewind() {
        let conn = db();
        let f = frame(&["read", "bash"]);
        let with_rules = || {
            let mut i = inputs(None);
            i.rules_update = Some(rules_update_text("新规则", ""));
            i
        };
        let mut ctx = ReasoningContext::new(String::new());
        start_turn(&conn, &mut ctx, "t0", "零");
        apply_turn_context(&conn, S, "t0", &mut ctx, &inputs(None), &f).unwrap().unwrap();
        finish_turn(&conn, &mut ctx, "t0", "a");
        start_turn(&conn, &mut ctx, "t1", "一");
        apply_turn_context(&conn, S, "t1", &mut ctx, &with_rules(), &f).unwrap().unwrap();
        assert!(text(ctx.messages.last().unwrap()).contains("<project_rules_update>"));
        finish_turn(&conn, &mut ctx, "t1", "b");
        start_turn(&conn, &mut ctx, "t2", "二");
        apply_turn_context(&conn, S, "t2", &mut ctx, &with_rules(), &f).unwrap().unwrap();
        assert!(!text(ctx.messages.last().unwrap()).contains("<project_rules_update>"), "already in the ledger");
        // Rewind past the announcing turn → re-announced.
        transcript::truncate_after_turn(&conn, S, "t0").unwrap();
        let mut c = ReasoningContext::new(String::new());
        c.messages = active_messages(&conn, S);
        let msg = crate::agent::transcript_history::append_current_user_turn(&conn, S, "t3", None, "三").unwrap();
        c.messages.push(msg);
        c.current_turn_start = Some(c.messages.len() - 1);
        apply_turn_context(&conn, S, "t3", &mut c, &with_rules(), &f).unwrap().unwrap();
        assert!(text(c.messages.last().unwrap()).contains("<project_rules_update>"));
    }

    #[test]
    fn rules_update_for_compares_against_the_frozen_frame_hash() {
        use crate::agent::rule_context_builder::RuleContextBuilder;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("uclaw.md"), "原规则").unwrap();
        let mut f = frame(&["read"]);
        f.rules_hash = RuleContextBuilder::rules_source_hash(dir.path());
        assert!(rules_update_for(dir.path(), &f).is_none());
        std::fs::write(dir.path().join("uclaw.md"), "新规则").unwrap();
        let t = rules_update_for(dir.path(), &f).expect("hash moved off the frame's");
        assert!(t.contains("## uclaw.md\n新规则"));
        // The frame's hash is a frozen baseline: still differs next turn.
        assert!(rules_update_for(dir.path(), &f).is_some());
    }

    // ── loop-level: the gate + apply through run_agentic_loop ─────────────
    struct ApplyingDelegate {
        conn: std::sync::Mutex<Connection>,
        applied: AtomicBool,
        reports: std::sync::atomic::AtomicUsize,
        steering: std::sync::Mutex<Vec<ChatMessage>>,
        follow_up: std::sync::Mutex<Vec<ChatMessage>>,
    }

    #[async_trait::async_trait]
    impl crate::agent::types::LoopDelegate for ApplyingDelegate {
        async fn check_signals(&self) -> LoopSignal {
            LoopSignal::Continue
        }
        async fn before_llm_call(&self, ctx: &mut ReasoningContext, _iteration: usize) -> Option<LoopOutcome> {
            if claim_once(&self.applied) {
                let conn = self.conn.lock().unwrap();
                if apply_turn_context(&conn, S, "t1", ctx, &inputs(Some(MEM)), &frame(&["read", "bash"])).unwrap().is_some() {
                    self.reports.fetch_add(1, Ordering::SeqCst);
                }
            }
            None
        }
        async fn call_llm(
            &self,
            _ctx: &mut ReasoningContext,
            _snapshot: &crate::agent::turn::TurnSnapshot,
            _iteration: usize,
        ) -> Result<RespondOutput, Error> {
            Ok(RespondOutput::Text {
                text: "done".into(),
                thinking: None,
                thinking_signature: None,
                metadata: ResponseMetadata { model: "m".into(), finish_reason: Some("end_turn".into()), usage: None },
            })
        }
        async fn handle_text_response(&self, _t: &str, _m: ResponseMetadata, _c: &mut ReasoningContext) -> TextAction {
            TextAction::Return(LoopOutcome::Response { text: "done".into(), usage: None, truncated: false, model: None })
        }
        async fn execute_tool_calls(&self, _c: Vec<ToolCall>, _x: &mut ReasoningContext) -> Result<Option<LoopOutcome>, Error> {
            Ok(None)
        }
        async fn get_steering_messages(&self) -> Vec<ChatMessage> {
            std::mem::take(&mut *self.steering.lock().unwrap())
        }
        async fn get_follow_up_messages(&self) -> Vec<ChatMessage> {
            std::mem::take(&mut *self.follow_up.lock().unwrap())
        }
        async fn persist_user_message(&self, msg: &ChatMessage) {
            let conn = self.conn.lock().unwrap();
            transcript::append(&conn, S, Some("t1"), TranscriptKind::Message, std::slice::from_ref(msg)).unwrap();
        }
    }

    #[tokio::test]
    async fn loop_applies_turn_context_once_to_the_prompt_row_only() {
        let conn = db();
        let mut ctx = ReasoningContext::new(String::new());
        start_turn(&conn, &mut ctx, "t1", "主请求");
        let delegate = ApplyingDelegate {
            conn: std::sync::Mutex::new(conn),
            applied: AtomicBool::new(false),
            reports: std::sync::atomic::AtomicUsize::new(0),
            steering: std::sync::Mutex::new(vec![ChatMessage::user("STEER")]),
            follow_up: std::sync::Mutex::new(vec![ChatMessage::user("FOLLOW")]),
        };
        let config = crate::agent::types::AgenticLoopConfig {
            max_iterations: 5,
            enable_tool_intent_nudge: false,
            ..Default::default()
        };
        let outcome = crate::agent::agentic_loop::run_agentic_loop(&delegate, &mut ctx, &config).await;
        assert!(matches!(outcome, LoopOutcome::Response { .. }), "{outcome:?}");
        assert_eq!(delegate.reports.load(Ordering::SeqCst), 1, "applied exactly once across both runs");
        let conn = delegate.conn.lock().unwrap();
        let rows = transcript::load_active(&conn, S).unwrap();
        let prefixed: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r.dynamic_prefix.is_some()).map(|(i, _)| i).collect();
        assert_eq!(prefixed, vec![0]);
        let texts: Vec<&str> = rows.iter().map(|r| text(&r.message)).collect();
        assert!(texts.contains(&"STEER") && texts.contains(&"FOLLOW"), "{texts:?}");
        assert_eq!(json(&rows[..1].iter().map(|r| r.message.clone()).collect::<Vec<_>>()), json(&ctx.messages[..1]));
        let live: Vec<&str> = ctx.messages.iter().map(text).filter(|t| t.starts_with(turn_context::OPEN)).collect();
        assert_eq!(live.len(), 1);
        assert_eq!(turn_context::strip(live[0]), "主请求");
    }

    #[test]
    fn rules_revert_to_baseline_is_announced_once() {
        let conn = db();
        let f = frame(&["read", "bash"]);
        let rules = |i: Option<String>| {
            let mut x = inputs(None);
            x.rules_update = i;
            x
        };
        let mut ctx = ReasoningContext::new(String::new());
        let mut turn = |id: &str, update: Option<String>, ctx: &mut ReasoningContext| -> String {
            start_turn(&conn, ctx, id, id);
            apply_turn_context(&conn, S, id, ctx, &rules(update), &f).unwrap().unwrap();
            let t = text(ctx.messages.last().unwrap()).to_string();
            finish_turn(&conn, ctx, id, "ok");
            t
        };
        // Baseline A, no announcement yet → no restore notice.
        assert!(!turn("t0", None, &mut ctx).contains("<project_rules_update>"));
        // A → B: B announced.
        let b = turn("t1", Some(rules_update_text("B", "")), &mut ctx);
        assert!(b.contains("<project_rules_update>") && b.contains("## uclaw.md\nB"));
        // B → A (hash back at the frame's baseline): restore announced once.
        let r = turn("t2", None, &mut ctx);
        assert!(r.contains("<project_rules_update>") && r.contains(RULES_RESTORED), "{r}");
        assert!(!turn("t3", None, &mut ctx).contains("<project_rules_update>"));
        // A → B again: re-announced (the latest note is the restore).
        assert!(turn("t4", Some(rules_update_text("B", "")), &mut ctx).contains("## uclaw.md\nB"));
    }
}
