//! C1.5 50-turn refactor benchmark support — **bench-only** (gated by
//! `feature = "bench"`). NOT compiled into release / default builds.
//!
//! Provides a deterministic *replay* harness that reconstructs the exact LLM
//! request the dispatcher would send each turn (real tool definitions + real
//! composed system prompt + a message history grown by replaying a fixed
//! golden tool-call sequence) and sums the per-turn token breakdown using the
//! **same tokenizer the dispatcher uses** (`uclaw_message_types::estimate_tokens`,
//! re-exported as `crate::agent::types::estimate_tokens`).
//!
//! Run on the pre-Dirac build (`git worktree` @ f6447a71) and the post build,
//! then diff → the measured M2-DoD token / round-trip deltas.
//!
//! See `docs/superpowers/specs/2026-05-25-c1.5-50turn-bench-design.md`.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::agent::types::{estimate_tokens, ChatMessage, ContentBlock, MessageRole, ToolDefinition};

// ─── Golden-record + report types ──────────────────────────────────────

/// One line of a `*_dirac.jsonl` golden sequence. The mock tool result is the
/// deterministic stand-in the dispatcher would have produced for this call, so
/// history grows realistically without touching the real filesystem.
#[derive(Debug, Clone, Deserialize)]
pub struct GoldenRecord {
    pub turn: u32,
    pub tool_name: String,
    pub tool_args: serde_json::Value,
    /// Mock tool result text. `__final__` records leave this empty.
    #[serde(default)]
    pub mock_tool_result: String,
}

/// Per-turn request token breakdown — mirrors the dispatcher's `Calling LLM`
/// log fields (`system_prompt_tokens` / `tool_def_tokens` / `message_tokens`).
#[derive(Debug, Clone, Serialize)]
pub struct TurnBreakdown {
    pub turn: u32,
    pub system_prompt_tokens: usize,
    pub tool_def_tokens: usize,
    pub message_tokens: usize,
    pub total: usize,
}

/// Replay result for one golden sequence on one build.
#[derive(Debug, Clone, Serialize)]
pub struct ReplayReport {
    pub fixture: String,
    /// "pre" | "post"
    pub golden: String,
    pub turns: usize,
    /// Non-`__final__` tool calls (= LLM round-trips that hit a tool).
    pub round_trips: usize,
    /// Number of builtin tool definitions sent each turn (pre/post compare the
    /// same deterministic set).
    pub tool_count: usize,
    /// Σ per-turn total (system + tool_def + message) — the headline metric.
    pub total_input_tokens: usize,
    pub per_turn: Vec<TurnBreakdown>,
}

/// Load + parse a golden sequence file (one JSON object per non-blank line).
pub fn load_golden(path: &Path) -> Vec<GoldenRecord> {
    let body = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()));
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l)
                .unwrap_or_else(|e| panic!("parse golden record in {}: {e}\n  line: {l}", path.display()))
        })
        .collect()
}

// ─── Tool registry reconstruction ───────────────────────────────────────

/// Build the deterministic builtin tool set the live agent registers (mirrors
/// the builtin block in `tauri_commands.rs:1878-1902`).
///
/// We register ONLY the builtins whose constructors take just a workspace path
/// (or nothing) — `read_file`, `write_file`, `get_file_skeleton`, `grep`,
/// `glob`, `web_fetch`, `http_request`, `edit`, `bash`. The runtime-conditional
/// tools (ask_user / exit_plan_mode / plan / self_eval / context / memu /
/// browser) require an `AppHandle`, DB handle, or live service and are
/// deliberately excluded: they would make the bench non-deterministic AND
/// can't be constructed without the full Tauri app. Both the pre- and post-
/// build replays register the identical set, so the comparison stays
/// apples-to-apples (the Dirac borrows live in `edit` / `read_file` schemas,
/// which ARE included). `tool_count` is recorded so any drift is visible.
pub fn bench_tool_registry(workspace: &Path) -> crate::agent::tools::tool::ToolRegistry {
    use crate::agent::tools::builtin;
    use crate::agent::tools::tool::ToolRegistry;

    let mut tools = ToolRegistry::new();
    let ws = workspace.to_path_buf();
    tools.register(builtin::file::ReadFileTool::new(ws.clone()));
    tools.register(builtin::file::WriteFileTool::new(ws.clone()));
    tools.register(builtin::get_file_skeleton::GetFileSkeletonTool::new(ws.clone()));
    tools.register(builtin::search::GrepTool::new(ws.clone()));
    tools.register(builtin::search::GlobTool::new(ws.clone()));
    tools.register(builtin::web::WebFetchTool::new());
    tools.register(builtin::web::HttpRequestTool::new());
    tools.register(builtin::edit::EditTool::new(ws.clone()));
    tools.register(builtin::shell::BashTool::new(ws));
    tools
}

/// The tool definitions the LLM would see, in the dispatcher's cache-stable
/// (name-sorted) order. Wraps [`bench_tool_registry`] + `list_definitions()`
/// (the dispatcher's own method — see `agent::tools::tool::ToolRegistry`).
pub fn bench_tool_definitions(workspace: &Path) -> Vec<ToolDefinition> {
    bench_tool_registry(workspace).list_definitions()
}

/// Tool-definition token cost, computed EXACTLY as the dispatcher does
/// (`dispatcher.rs:2068-2071`): per-tool `name + description + parameters`,
/// summed. Using the same per-field decomposition (rather than serializing the
/// whole `Vec` once) keeps the bench faithful to the real request measurement.
fn tool_def_tokens(tools: &[ToolDefinition]) -> usize {
    tools
        .iter()
        .map(|t| {
            estimate_tokens(&t.name) as usize
                + estimate_tokens(&t.description) as usize
                + estimate_tokens(&t.parameters.to_string()) as usize
        })
        .sum()
}

/// Message-history token cost, computed EXACTLY as the dispatcher does
/// (`dispatcher.rs:2072-2079`). The dispatcher skips the leading system
/// message (`skip(1)`); our `history` contains no synthetic system message
/// (the system prompt is measured separately), so we sum all of it.
fn message_tokens(history: &[ChatMessage]) -> usize {
    history
        .iter()
        .map(|m| {
            m.content
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => estimate_tokens(text) as usize,
                    ContentBlock::ToolResult { content, .. } => estimate_tokens(content) as usize + 5,
                    ContentBlock::ToolUse { name, input, .. } => {
                        estimate_tokens(name) as usize
                            + estimate_tokens(&input.to_string()) as usize
                            + 10
                    }
                    _ => 5,
                })
                .sum::<usize>()
        })
        .sum()
}

// ─── Replay harness ─────────────────────────────────────────────────────

/// Replay a golden sequence (`{golden}_dirac.jsonl`), growing the message
/// history turn-by-turn and summing the per-turn request token breakdown. No
/// network, no dispatcher, no Wry — a standalone reconstruction of the request
/// the real loop would build.
pub fn replay(fixture_dir: &Path, golden: &str) -> ReplayReport {
    let workspace = fixture_dir.join("workspace");
    let records = load_golden(&fixture_dir.join(format!("{golden}_dirac.jsonl")));

    let tools = bench_tool_definitions(&workspace);
    let tool_count = tools.len();
    let tool_def_tok = tool_def_tokens(&tools);

    // LCD system prompt — `compose_system_prompt` exists in both pre- and
    // post-Dirac builds (verified). Default SafetyMode = Supervised (Auto),
    // which adds no mode block, matching the most common live config.
    let system_prompt = crate::agent::mode_prompts::compose_system_prompt(
        "",
        Some(workspace.as_path()),
        &crate::safety::SafetyMode::default(),
    );
    let system_prompt_tokens = estimate_tokens(&system_prompt) as usize;

    // Seed history with the user's refactor request (the same prompt used by
    // live mode, read from task.json so pre/post seed identically).
    let task_prompt = std::fs::read_to_string(fixture_dir.join("task.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("prompt").and_then(|p| p.as_str()).map(String::from))
        .unwrap_or_else(|| "Rename old_name to new_name across the workspace.".to_string());

    let mut history: Vec<ChatMessage> = vec![ChatMessage {
        role: MessageRole::User,
        content: vec![ContentBlock::Text { text: task_prompt }],
        compacted: false,
    }];

    let mut per_turn = Vec::with_capacity(records.len());
    let mut round_trips = 0usize;

    for rec in &records {
        // Measure the request that WOULD be sent THIS turn (before appending
        // this turn's response to history — matches the dispatcher, which logs
        // the breakdown right before calling the LLM).
        let msg_tok = message_tokens(&history);
        per_turn.push(TurnBreakdown {
            turn: rec.turn,
            system_prompt_tokens,
            tool_def_tokens: tool_def_tok,
            message_tokens: msg_tok,
            total: system_prompt_tokens + tool_def_tok + msg_tok,
        });

        if rec.tool_name == "__final__" {
            // Terminal assistant text message — stop.
            history.push(ChatMessage {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::Text {
                    text: rec.tool_args.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                }],
                compacted: false,
            });
            break;
        }

        round_trips += 1;
        let call_id = format!("call_{}", rec.turn);
        // Assistant tool_use turn + user tool_result turn — grows history the
        // way the real loop does.
        history.push(ChatMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: call_id.clone(),
                name: rec.tool_name.clone(),
                input: rec.tool_args.clone(),
            }],
            compacted: false,
        });
        history.push(ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: call_id,
                content: rec.mock_tool_result.clone(),
                is_error: Some(false),
            }],
            compacted: false,
        });
    }

    ReplayReport {
        fixture: fixture_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        golden: golden.to_string(),
        turns: per_turn.len(),
        round_trips,
        tool_count,
        total_input_tokens: per_turn.iter().map(|t| t.total).sum(),
        per_turn,
    }
}

// ─── Live mode (real provider) ──────────────────────────────────────────

mod live;
pub use live::{live_run, LiveReport};
