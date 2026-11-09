// SPDX-License-Identifier: Apache-2.0
//! Context continuity P3 §7.C — byte-level prefix consistency.
//!
//! In a transcript-mode session the request built for one LLM call must be a
//! byte prefix of the request built for the next one (same system prompt,
//! same tools, every earlier message unchanged, at least one message more),
//! except at the documented breaks: a tool-result clearing pass, a
//! compaction, a frame rebuild (system only) and a tool-set change (tools
//! only). These tests are the regression net for the whole phase.
//!
//! What is real: every turn starts like `send_agent_message`
//! (`build_agent_history` + `persist_current_user_turn` on the DB, then the
//! live context); the loop is the real `run_agentic_loop` (transcript mode,
//! real compaction decision, clearing, `transcript_compact`, flushes); the
//! frame comes from the real `frame_source` (key / persist guard) +
//! `load_or_rebuild_frame` + `frame_build_parts`; the per-turn context goes
//! through `claim_once` + `apply_turn_context`; the snapshot through
//! `transcript_turn_snapshot` (what `ChatDelegate::create_turn_snapshot`
//! returns in transcript mode);
//! the message list through `build_request_messages`; the bytes through the
//! providers' own `build_request_body` with `main_completion_config`.
//!
//! What is not (a `ChatDelegate` needs a Wry `AppHandle`): the gathering of
//! the per-turn inputs in `ChatDelegate::prepare_turn_context` (memory recall,
//! GEP, context-manager fragments) — the test feeds equivalent inputs; tool
//! execution (results are scripted); LLM summaries (the extractive fallback
//! fold is used, as when the summarizer is unavailable).

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::Connection;
use serde_json::Value;

use crate::agent::dispatcher::{self as turn_runner, FrameInputs, FrameSource, TranscriptSink};
use crate::agent::rule_context_builder::RuleContextBuilder;
use crate::agent::session_frame::SessionFrame;
use crate::agent::tools::builtin::file::{ReadFileTool, WriteFileTool};
use crate::agent::tools::builtin::search::GrepTool;
use crate::agent::tools::builtin::shell::BashTool;
use crate::agent::tools::tool::ToolRegistry;
use crate::agent::turn_context::TurnContextInputs;
use crate::agent::types::{
    AgenticLoopConfig, ChatMessage, ContentBlock, LoopDelegate, LoopOutcome, LoopSignal, MessageRole,
    ReasoningContext, RespondOutput, ResponseMetadata, TextAction, ToolCall, ToolDefinition,
};
use crate::error::Error;
use crate::safety::SafetyMode;

const S: &str = "s1";
const OPENAI_MODEL: &str = "deepseek-chat";
const ANTHROPIC_MODEL: &str = "claude-sonnet-4-5";
const BASE_PROMPT: &str = "你是 uClaw，一个本地智能体。";
const THRESHOLD: usize = 32_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Provider {
    OpenAi,
    Anthropic,
}

impl Provider {
    fn name(self) -> &'static str {
        match self {
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
        }
    }
}

// ── scripted turns ──────────────────────────────────────────────────────────

#[derive(Clone)]
enum Step {
    Tool { name: &'static str, args: Value, result: String },
    Reply(String),
}

fn tool(name: &'static str, args: Value, result: impl Into<String>) -> Step {
    Step::Tool { name, args, result: result.into() }
}

fn reply(text: impl Into<String>) -> Step {
    Step::Reply(text.into())
}

/// Text of roughly `tokens` estimated tokens ("abcd\n" = 2 tokens).
fn filler(tag: &str, tokens: usize) -> String {
    format!("{tag}\n{}", "abcd\n".repeat(tokens / 2))
}

struct TurnSpec {
    id: &'static str,
    text: String,
    steps: Vec<Step>,
    /// Per-turn inputs beyond the defaults (memory, profile, mode, …).
    edit: Box<dyn Fn(&mut TurnContextInputs) + Send + Sync>,
    /// Call `apply_turn_context` before every LLM call (no `claim_once`).
    apply_every_call: bool,
}

fn turn(id: &'static str, text: impl Into<String>, steps: Vec<Step>) -> TurnSpec {
    TurnSpec { id, text: text.into(), steps, edit: Box::new(|_| {}), apply_every_call: false }
}

impl TurnSpec {
    fn applied_every_call(mut self) -> Self {
        self.apply_every_call = true;
        self
    }

    fn with(mut self, edit: impl Fn(&mut TurnContextInputs) + Send + Sync + 'static) -> Self {
        self.edit = Box::new(edit);
        self
    }
}

fn memory(lines: &[&str]) -> Option<String> {
    let body: Vec<String> = lines.iter().map(|l| format!("- [Fact] {l}")).collect();
    Some(format!("<memory_context>\n## Facts\n{}\n</memory_context>", body.join("\n")))
}

/// One `call_llm` as the provider would receive it.
#[derive(Clone)]
struct Recorded {
    messages: Vec<ChatMessage>,
    tools: Vec<ToolDefinition>,
    model: String,
}

// ── the session: DB file + workspace + tools ────────────────────────────────

struct Session {
    _dir: tempfile::TempDir,
    db_path: PathBuf,
    db: Arc<Mutex<Connection>>,
    workspace: PathBuf,
    base_prompt: String,
    tools: Arc<ToolRegistry>,
    model: String,
    provider: Provider,
    clock: i64,
    /// Turn contexts `apply_turn_context` rendered in the last turn.
    last_rendered: u32,
}

fn registry(ws: &Path, extra: bool) -> Arc<ToolRegistry> {
    let mut r = ToolRegistry::new();
    r.register(ReadFileTool::new(ws.to_path_buf()));
    r.register(WriteFileTool::new(ws.to_path_buf()));
    r.register(BashTool::new(ws.to_path_buf()));
    if extra {
        r.register(GrepTool::new(ws.to_path_buf()));
    }
    Arc::new(r)
}

fn open_db(path: &Path) -> Arc<Mutex<Connection>> {
    let conn = Connection::open(path).unwrap();
    crate::db::migrations::run(&conn).unwrap();
    Arc::new(Mutex::new(conn))
}

impl Session {
    fn new(provider: Provider) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("ws");
        std::fs::create_dir_all(workspace.join("src")).unwrap();
        std::fs::write(workspace.join("uclaw.md"), "项目规则 A").unwrap();
        std::fs::write(workspace.join("AGENTS.md"), "# Agents\n写测试。").unwrap();
        std::fs::write(
            workspace.join(".uclawrules"),
            "rules:\n  - glob: \"src/**/*.rs\"\n    instructions: \"Rust 代码用 anyhow 处理错误\"\n",
        )
        .unwrap();
        let db_path = dir.path().join("uclaw.db");
        let db = open_db(&db_path);
        db.lock()
            .unwrap()
            .execute(
                "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at) VALUES (?1,'global','t',0,0)",
                [S],
            )
            .unwrap();
        let tools = registry(&workspace, false);
        let model = match provider {
            Provider::OpenAi => OPENAI_MODEL,
            Provider::Anthropic => ANTHROPIC_MODEL,
        };
        Self {
            _dir: dir,
            db_path,
            db,
            workspace,
            base_prompt: BASE_PROMPT.into(),
            tools,
            model: model.into(),
            provider,
            clock: 1_000_000,
            last_rendered: 0,
        }
    }

    /// Process restart: drop every in-memory handle (connection, tools) and
    /// reopen the DB file.
    fn restart(&mut self) {
        self.db = open_db(&self.db_path);
        self.tools = registry(&self.workspace, false);
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap()
    }

    /// One turn the way `send_agent_message` runs it; returns every request
    /// of the turn in order.
    async fn run(&mut self, spec: TurnSpec) -> Vec<Recorded> {
        self.clock += 1_000;
        let spec_apply_every_call = spec.apply_every_call;
        let (history, current) = {
            let conn = self.conn();
            let (history, _) = crate::agent::transcript_history::build_agent_history(&conn, S).unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            let current = crate::agent::transcript_history::persist_current_user_turn(
                &tx, S, spec.id, None, &spec.text, self.clock,
            )
            .unwrap();
            tx.commit().unwrap();
            (history, current)
        };
        let mut ctx = ReasoningContext::new(self.base_prompt.clone());
        ctx.messages = history;
        ctx.messages.push(current);
        ctx.transcript_persisted_len = ctx.messages.len();
        ctx.current_turn_start = Some(ctx.messages.len() - 1);

        let delegate = PrefixDelegate {
            sink: TranscriptSink { db: Arc::clone(&self.db), session_id: S.into(), turn_id: spec.id.into() },
            tools: Arc::clone(&self.tools),
            model: self.model.clone(),
            provider: self.provider,
            base_prompt: self.base_prompt.clone(),
            workspace: self.workspace.clone(),
            minute: (self.clock / 1_000) as u32,
            edit: spec.edit,
            frame: tokio::sync::OnceCell::new(),
            applied: AtomicBool::new(false),
            apply_every_call: spec.apply_every_call,
            applications: Mutex::new(0),
            rendered: Mutex::new(0),
            last_tool_defs_hash: Mutex::new(None),
            steps: Mutex::new(spec.steps.into()),
            calls: Mutex::new(0),
            pending_results: Mutex::new(VecDeque::new()),
            requests: Mutex::new(Vec::new()),
        };
        let config = AgenticLoopConfig {
            max_iterations: 10,
            enable_tool_intent_nudge: false,
            transcript_mode: true,
            model_context_length: 200_000,
            compaction_threshold_tokens: THRESHOLD,
            context_settings: crate::memubot_config::AgentContextSettings {
                keep_tool_result_groups: 1,
                tool_clear_floor_tokens: 5_000,
                summary_target_tokens: 1_000,
                reload_recent_files: 0,
            },
            ..Default::default()
        };
        let outcome = crate::agent::agentic_loop::run_agentic_loop(&delegate, &mut ctx, &config).await;
        assert!(matches!(outcome, LoopOutcome::Response { .. }), "{outcome:?}");
        assert!(delegate.steps.lock().unwrap().is_empty(), "script not consumed");
        if spec_apply_every_call {
            let applied = *delegate.applications.lock().unwrap() as usize;
            assert_eq!(applied, delegate.requests.lock().unwrap().len(), "applied before every call");
        } else {
            assert_eq!(*delegate.rendered.lock().unwrap(), 1, "one turn context per turn");
        }
        self.last_rendered = *delegate.rendered.lock().unwrap();
        delegate.requests.into_inner().unwrap()
    }
}

// ── the test delegate: production request-building functions ───────────────

struct PrefixDelegate {
    sink: TranscriptSink,
    tools: Arc<ToolRegistry>,
    model: String,
    provider: Provider,
    base_prompt: String,
    workspace: PathBuf,
    minute: u32,
    edit: Box<dyn Fn(&mut TurnContextInputs) + Send + Sync>,
    frame: tokio::sync::OnceCell<SessionFrame>,
    applied: AtomicBool,
    apply_every_call: bool,
    applications: Mutex<u32>,
    rendered: Mutex<u32>,
    last_tool_defs_hash: Mutex<Option<u64>>,
    steps: Mutex<VecDeque<Step>>,
    calls: Mutex<usize>,
    pending_results: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<Recorded>>,
}

impl PrefixDelegate {
    /// `ChatDelegate::session_frame` with fixed mode / persona / manifest
    /// (the agent turn path: frame inputs present).
    async fn session_frame(&self) -> &SessionFrame {
        self.frame
            .get_or_init(|| async {
                let FrameSource::Build { base_prompt, key, persist_db } = turn_runner::frame_source(
                    Some(&*self.sink.db),
                    S,
                    Some(&self.base_prompt),
                    "",
                    Some(&self.workspace),
                ) else {
                    unreachable!("the agent turn path always has frame inputs")
                };
                let (frame, _) = turn_runner::load_or_rebuild_frame(
                    persist_db,
                    S,
                    &self.sink.turn_id,
                    key,
                    0,
                    |_reason| async move {
                        let manifest = crate::skills_manifest::SessionManifest {
                            block: "## 可用技能\n- pdf: 处理 PDF".into(),
                            listed: vec!["pdf".into()],
                            total_available: 1,
                            learned_node_ids: Vec::new(),
                        };
                        let inputs = FrameInputs {
                            base_system_prompt: base_prompt.to_string(),
                            workspace_root: Some(self.workspace.clone()),
                            mode: SafetyMode::Supervised,
                            persona_block: None,
                            skills_manifest_block: manifest.block.clone(),
                            static_rules_block: RuleContextBuilder::build_static_context(&self.workspace),
                        };
                        turn_runner::frame_build_parts(&inputs, &self.tools, &manifest).0
                    },
                )
                .await;
                frame
            })
            .await
    }

    /// The per-turn inputs `prepare_turn_context` gathers, with the parts
    /// that need app state (recall, GEP, fragments) replaced by fixtures.
    /// `call` (1-based) = the n-th application in this turn: on a re-apply
    /// the clock and memory differ, as a recomputation would see them.
    fn turn_inputs(&self, frame: &SessionFrame, call: u32) -> TurnContextInputs {
        use chrono::TimeZone;
        let files = {
            let conn = self.sink.db.lock().unwrap();
            crate::agent::file_working_set::load(&conn, S).unwrap().paths()
        };
        let mut inputs = TurnContextInputs {
            now: chrono::Local.with_ymd_and_hms(2026, 11, 9, 9, 0, 0).unwrap()
                + chrono::Duration::minutes(i64::from(self.minute + call - 1)),
            memory_context: None,
            learned_profile: String::new(),
            growth: String::new(),
            relevant_skills: String::new(),
            gene_block: String::new(),
            plan_suggest_hint: String::new(),
            glob_rules: RuleContextBuilder::glob_matched_rules(&self.workspace, &files),
            rules_update: turn_runner::rules_update_for(&self.workspace, frame),
            context_fragments: Vec::new(),
            rewound_changes: turn_runner::rewound_changes_from_sink(Some(&self.sink)),
            current_mode: SafetyMode::Supervised,
            current_tool_names: crate::agent::tool_shaping::PerTurnToolSurface::from_registry(&self.tools)
                .definitions
                .into_iter()
                .map(|d| d.name)
                .collect(),
        };
        (self.edit)(&mut inputs);
        if call > 1 {
            let mem = inputs.memory_context.take().unwrap_or_default();
            inputs.memory_context = Some(format!("{mem}\n- [Fact] 第 {call} 次计算时新增的记忆"));
        }
        inputs
    }
}

#[async_trait]
impl LoopDelegate for PrefixDelegate {
    async fn check_signals(&self) -> LoopSignal {
        LoopSignal::Continue
    }

    async fn before_llm_call(&self, ctx: &mut ReasoningContext, _iteration: usize) -> Option<LoopOutcome> {
        // `apply_every_call` bypasses the once-per-turn gate to exercise the
        // persisted-once guard inside `apply_turn_context` itself.
        if self.apply_every_call || turn_runner::claim_once(&self.applied) {
            let call = {
                let mut n = self.applications.lock().unwrap();
                *n += 1;
                *n
            };
            let frame = self.session_frame().await;
            let inputs = self.turn_inputs(frame, call);
            let conn = self.sink.db.lock().unwrap();
            if turn_runner::apply_turn_context(&conn, S, &self.sink.turn_id, ctx, &inputs, frame).unwrap().is_some() {
                *self.rendered.lock().unwrap() += 1;
            }
        }
        None
    }

    async fn create_turn_snapshot(&self, ctx: &ReasoningContext, turn_index: u32) -> crate::agent::turn::TurnSnapshot {
        let frame = self.session_frame().await;
        turn_runner::transcript_turn_snapshot(
            frame,
            turn_index,
            &self.model,
            &self.tools,
            ctx.force_text,
            &self.last_tool_defs_hash,
        )
    }

    async fn call_llm(
        &self,
        ctx: &mut ReasoningContext,
        snapshot: &crate::agent::turn::TurnSnapshot,
        _iteration: usize,
    ) -> Result<RespondOutput, Error> {
        let supports_images = crate::agent::image_policy::supports_images(self.provider.name(), &snapshot.model);
        self.requests.lock().unwrap().push(Recorded {
            messages: crate::agent::dispatcher::build_request_messages(ctx, snapshot, supports_images),
            tools: (*snapshot.tools).clone(),
            model: snapshot.model.clone(),
        });
        let metadata = ResponseMetadata { model: self.model.clone(), finish_reason: Some("stop".into()), usage: None };
        let n = {
            let mut c = self.calls.lock().unwrap();
            *c += 1;
            *c
        };
        match self.steps.lock().unwrap().pop_front().expect("script exhausted") {
            Step::Tool { name, args, result } => {
                self.pending_results.lock().unwrap().push_back(result);
                Ok(RespondOutput::ToolCalls {
                tool_calls: vec![ToolCall { id: format!("{}-c{n}", self.sink.turn_id), name: name.into(), arguments: args }],
                text: None,
                thinking: None,
                thinking_signature: None,
                metadata,
            })
            }
            Step::Reply(text) => Ok(RespondOutput::Text { text, thinking: None, thinking_signature: None, metadata }),
        }
    }

    async fn handle_text_response(&self, text: &str, _m: ResponseMetadata, _c: &mut ReasoningContext) -> TextAction {
        TextAction::Return(LoopOutcome::Response { text: text.into(), usage: None, truncated: false, model: None })
    }

    /// One `user_tool_result` per call, as `ChatDelegate::execute_tool_calls`
    /// pushes them; a `write_file` records its file event like
    /// `record_file_events_in_sink`.
    async fn execute_tool_calls(&self, calls: Vec<ToolCall>, ctx: &mut ReasoningContext) -> Result<Option<LoopOutcome>, Error> {
        for c in calls {
            let content = self.pending_results.lock().unwrap().pop_front().expect("scripted result");
            if c.name == "write_file" {
                let path = c.arguments.get("path").and_then(Value::as_str).unwrap_or("").to_string();
                let conn = self.sink.db.lock().unwrap();
                crate::agent::file_working_set::record(
                    &conn,
                    S,
                    Some(&self.sink.turn_id),
                    &[crate::agent::file_working_set::FileEvent {
                        path,
                        op: crate::agent::file_working_set::FileOp::Write,
                        detail: "写入".into(),
                    }],
                )
                .unwrap();
            }
            ctx.messages.push(ChatMessage::user_tool_result(&c.id, &content, false));
        }
        Ok(None)
    }

    async fn persist_transcript(&self, new_msgs: &[ChatMessage]) -> anyhow::Result<()> {
        turn_runner::persist_to_sink(Some(&self.sink), new_msgs)
    }

    async fn persist_cleared(
        &self,
        cleared: &[(usize, ChatMessage)],
        _tool_call_ids: &[String],
        _freed_tokens: usize,
    ) -> anyhow::Result<()> {
        turn_runner::persist_cleared_in_sink(Some(&self.sink), cleared)
    }

    async fn replace_with_session_block(
        &self,
        block: &ChatMessage,
        fold: &crate::agent::compact::StructuredFold,
        superseded_through: usize,
        _threshold_tokens: usize,
        _trigger: crate::agent::types::CompactionTrigger,
    ) -> anyhow::Result<()> {
        turn_runner::replace_session_block_in_sink(&self.sink, block, Some(fold), superseded_through).map(|_| ())
    }

    fn session_block_context(&self) -> Option<crate::agent::session_block::SessionBlockContext> {
        turn_runner::session_block_context_from_sink(Some(&self.sink), Some(&self.workspace))
    }

    async fn persist_user_message(&self, msg: &ChatMessage) {
        turn_runner::persist_to_sink(Some(&self.sink), std::slice::from_ref(msg)).unwrap();
    }
}

// ── provider bytes and the prefix comparison ───────────────────────────────

/// The body the provider adapter would send for `req` (streaming, as
/// `call_llm` does), built by the adapter's own `build_request_body`.
fn request_body(provider: Provider, req: &Recorded) -> Value {
    let config = turn_runner::main_completion_config(&req.model, false);
    match provider {
        Provider::OpenAi => crate::llm::OpenAIProvider::new("test-key".into(), None)
            .build_request_body(&req.messages, &req.tools, &config, true),
        Provider::Anthropic => crate::llm::AnthropicProvider::new("test-key".into(), None)
            .build_request_body(&req.messages, &req.tools, &config, true),
    }
}

fn strip_cache_control(v: &mut Value) {
    match v {
        Value::Object(map) => {
            map.remove("cache_control");
            map.values_mut().for_each(strip_cache_control);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_cache_control),
        _ => {}
    }
}

/// A request split into what must stay byte-identical (system, tools, other
/// parameters) and the conversation (each message serialized on its own).
/// Anthropic: the `cache_control` breakpoints move every turn and are not
/// part of the cached content, so they are removed; on the messages that
/// carried one, the breakpoint pass had also rewritten a bare-string
/// `content` into its one-text-block form (same content for the API), which
/// is undone. No other message is touched.
struct Parts {
    system: String,
    tools: String,
    params: String,
    conversation: Vec<String>,
}

fn parts(provider: Provider, req: &Recorded) -> Parts {
    let mut body = request_body(provider, req);
    if provider == Provider::Anthropic {
        for m in body["messages"].as_array_mut().unwrap() {
            let had_breakpoint = m.to_string().contains("\"cache_control\"");
            strip_cache_control(m);
            // `convert_content` renders a lone text block as a bare string,
            // so a one-text-block array here is the breakpoint pass's wrap.
            let wrapped = match m.get("content") {
                Some(Value::Array(blocks)) if blocks.len() == 1 && blocks[0]["type"] == "text" => {
                    blocks[0]["text"].as_str().map(str::to_string)
                }
                _ => None,
            };
            if let (true, Some(text)) = (had_breakpoint, wrapped) {
                m["content"] = Value::String(text);
            }
        }
        strip_cache_control(&mut body);
    }
    let obj = body.as_object_mut().unwrap();
    let mut messages = match obj.remove("messages") {
        Some(Value::Array(m)) => m,
        other => panic!("no messages array: {other:?}"),
    };
    let tools = obj.remove("tools").unwrap_or(Value::Null);
    let system = match provider {
        Provider::Anthropic => obj.remove("system").unwrap_or(Value::Null),
        Provider::OpenAi => {
            assert_eq!(messages[0]["role"], "system", "OpenAI request starts with the system message");
            messages.remove(0)
        }
    };
    Parts {
        system: system.to_string(),
        tools: tools.to_string(),
        params: Value::Object(obj.clone()).to_string(),
        conversation: messages.iter().map(|m| serde_json::to_string(m).unwrap()).collect(),
    }
}

/// Where `next` departs from "byte prefix of `prev` plus more".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Diff {
    system_same: bool,
    tools_same: bool,
    params_same: bool,
    /// First conversation message of `prev` that `next` does not repeat
    /// byte for byte at the same position.
    first_divergence: Option<usize>,
    grew: bool,
}

const PREFIX: Diff = Diff { system_same: true, tools_same: true, params_same: true, first_divergence: None, grew: true };

fn diff(provider: Provider, prev: &Recorded, next: &Recorded) -> Diff {
    let (p, n) = (parts(provider, prev), parts(provider, next));
    Diff {
        system_same: p.system == n.system,
        tools_same: p.tools == n.tools,
        params_same: p.params == n.params,
        first_divergence: (0..p.conversation.len()).find(|&i| n.conversation.get(i) != Some(&p.conversation[i])),
        grew: n.conversation.len() > p.conversation.len(),
    }
}

#[track_caller]
fn assert_prefix(provider: Provider, prev: &Recorded, next: &Recorded) {
    let d = diff(provider, prev, next);
    if d != PREFIX {
        let (p, n) = (parts(provider, prev), parts(provider, next));
        let at = d.first_divergence.unwrap_or(0);
        panic!(
            "{provider:?}: request is not a byte prefix of the next one: {d:?}\nprev[{at}] = {}\nnext[{at}] = {}",
            p.conversation.get(at).map(String::as_str).unwrap_or("-"),
            n.conversation.get(at).map(String::as_str).unwrap_or("-"),
        );
    }
}

#[track_caller]
fn assert_chain(provider: Provider, reqs: &[&Recorded]) {
    for w in reqs.windows(2) {
        assert_prefix(provider, w[0], w[1]);
    }
}

fn all(turns: &[Vec<Recorded>]) -> Vec<&Recorded> {
    turns.iter().flatten().collect()
}

/// Text of the last user text message of a request (the turn's prompt row
/// on a turn's first call).
fn last_user_text(req: &Recorded) -> &str {
    req.messages
        .iter()
        .rev()
        .filter(|m| m.role == MessageRole::User)
        .find_map(|m| {
            m.content.iter().find_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
        })
        .unwrap_or("")
}

// ── scenarios ──────────────────────────────────────────────────────────────

/// Three turns with tool calls; memory changes every turn (new lines, a
/// dropped line); the clock moves.
async fn three_tool_turns(provider: Provider) -> Vec<Vec<Recorded>> {
    let mut s = Session::new(provider);
    let t1 = s
        .run(
            turn("t1", "看看 src/main.rs", vec![tool("read_file", serde_json::json!({"path": "src/main.rs"}), "fn main() {}"), reply("是个空 main。")])
                .with(|i| i.memory_context = memory(&["用户偏好: 简洁回答", "项目: uClaw"])),
        )
        .await;
    let t2 = s
        .run(
            turn("t2", "跑一下测试", vec![tool("bash", serde_json::json!({"command": "cargo test"}), "test result: ok. 3 passed"), reply("全部通过。")])
                .with(|i| i.memory_context = memory(&["用户偏好: 简洁回答", "项目: uClaw", "测试命令: cargo test"])),
        )
        .await;
    let t3 = s
        .run(
            turn(
                "t3",
                "再读一遍然后总结",
                vec![
                    tool("read_file", serde_json::json!({"path": "src/main.rs"}), "fn main() {}"),
                    tool("bash", serde_json::json!({"command": "wc -l src/main.rs"}), "1 src/main.rs"),
                    reply("一行代码，测试通过。"),
                ],
            )
            .with(|i| i.memory_context = memory(&["项目: uClaw", "仓库语言: Rust"])),
        )
        .await;
    vec![t1, t2, t3]
}

#[tokio::test]
async fn consecutive_turns_request_is_prefix_openai() {
    let turns = three_tool_turns(Provider::OpenAi).await;
    assert_eq!(turns.iter().map(Vec::len).collect::<Vec<_>>(), vec![2, 2, 3]);
    // Every call of the session — within and across turns — extends the previous one.
    assert_chain(Provider::OpenAi, &all(&turns));
    // The per-turn context really differs turn to turn (it is not trivially stable).
    let firsts: Vec<&str> = turns.iter().map(|t| last_user_text(&t[0])).collect();
    assert!(firsts[0].starts_with(crate::agent::turn_context::OPEN) && firsts[0].contains("<memory_context>"));
    assert!(firsts[1].contains("<memory_update>") && firsts[1].contains("测试命令"), "{}", firsts[1]);
    assert!(firsts[2].contains("仓库语言"), "{}", firsts[2]);
    assert_ne!(firsts[0], firsts[1]);
}

#[tokio::test]
async fn consecutive_turns_request_is_prefix_anthropic_messages() {
    let turns = three_tool_turns(Provider::Anthropic).await;
    assert_chain(Provider::Anthropic, &all(&turns));
    // The raw bodies do differ in their breakpoints — that is what the
    // comparison deliberately ignores.
    let a = request_body(Provider::Anthropic, &turns[0][0]);
    let b = request_body(Provider::Anthropic, &turns[1][0]);
    assert!(a.to_string().contains("cache_control") && b.to_string().contains("cache_control"));
}

#[tokio::test]
async fn tool_iterations_within_turn_are_prefix() {
    let mut s = Session::new(Provider::OpenAi);
    let reqs = s
        .run(
            turn(
                "t1",
                "修一下这个 bug",
                vec![
                    tool("read_file", serde_json::json!({"path": "src/lib.rs"}), "pub fn f() -> u8 { 1 }"),
                    tool("write_file", serde_json::json!({"path": "src/lib.rs", "content": "pub fn f() -> u8 { 2 }"}), "ok"),
                    reply("改好了。"),
                ],
            )
            .with(|i| i.memory_context = memory(&["项目: uClaw"]))
            .applied_every_call(),
        )
        .await;
    assert_eq!(reqs.len(), 3);
    assert_prefix(Provider::OpenAi, &reqs[0], &reqs[1]);
    assert_prefix(Provider::OpenAi, &reqs[1], &reqs[2]);
    // `apply_turn_context` ran before each of the 3 calls with a later clock
    // and more memory each time; the prompt row's persisted prefix wins:
    // rendered once, the later applications are no-ops.
    assert_eq!(s.last_rendered, 1, "turn context rendered more than once");
    // The turn context is applied once: the same prompt-row bytes on every iteration.
    assert_eq!(last_user_text(&reqs[0]), last_user_text(&reqs[2]));
}

#[tokio::test]
async fn restart_between_turns_preserves_prefix() {
    let mut s = Session::new(Provider::OpenAi);
    let t1 = s
        .run(turn("t1", "列出文件", vec![tool("bash", serde_json::json!({"command": "ls"}), "src\nuclaw.md"), reply("两个条目。")])
            .with(|i| i.memory_context = memory(&["项目: uClaw"])))
        .await;
    let t2 = s
        .run(turn("t2", "读 uclaw.md", vec![tool("read_file", serde_json::json!({"path": "uclaw.md"}), "项目规则 A"), reply("规则 A。")])
            .with(|i| i.memory_context = memory(&["项目: uClaw", "规则文件: uclaw.md"])))
        .await;
    let before_restart = t2.last().unwrap().clone();
    // Drop every in-memory handle: connection, tool registry, delegate state;
    // the frame and the turn contexts come back from the DB file.
    s.restart();
    let t3 = s
        .run(turn("t3", "继续", vec![tool("bash", serde_json::json!({"command": "pwd"}), "/ws"), reply("在工作区。")])
            .with(|i| i.memory_context = memory(&["项目: uClaw", "规则文件: uclaw.md"])))
        .await;
    assert_prefix(Provider::OpenAi, &before_restart, &t3[0]);
    assert_chain(Provider::OpenAi, &all(&[t1, t2, t3]));
}

#[tokio::test]
async fn rewound_changes_and_sticky_blocks_do_not_break_prefix() {
    let mut s = Session::new(Provider::OpenAi);
    let t1 = s.run(turn("t1", "开始", vec![reply("好的。")]).with(|i| {
        i.memory_context = memory(&["项目: uClaw"]);
        i.learned_profile = "喜欢中文".into();
    }))
    .await;
    // t2 writes src/main.rs (file working set → glob rule next turn) and the
    // learned profile changes.
    let t2 = s
        .run(turn(
            "t2",
            "写 main",
            vec![tool("write_file", serde_json::json!({"path": "src/main.rs", "content": "fn main() {}"}), "ok"), reply("写好了。")],
        )
        .with(|i| {
            i.memory_context = memory(&["项目: uClaw"]);
            i.learned_profile = "喜欢中文，回答要短".into();
            i.plan_suggest_hint = "可以先写计划。".into();
        }))
        .await;
    // t3: uclaw.md changed on disk (rules update), Plan mode, glob rule matches.
    std::fs::write(s.workspace.join("uclaw.md"), "项目规则 B").unwrap();
    let t3 = s
        .run(turn("t3", "检查一下", vec![reply("没问题。")]).with(|i| {
            i.memory_context = memory(&["项目: uClaw"]);
            i.learned_profile = "喜欢中文，回答要短".into();
            i.current_mode = SafetyMode::Plan;
        }))
        .await;
    let p3 = last_user_text(&t3[0]);
    for tag in ["<project_rule id=", "<project_rules_update>", "<mode_update mode=\"plan\">"] {
        assert!(p3.contains(tag), "{tag} missing:\n{p3}");
    }
    assert!(last_user_text(&t2[0]).contains("<learned_profile>"));
    assert_chain(Provider::OpenAi, &all(&[t1.clone(), t2, t3]));

    // Rewind to the end of t1: t2/t3 leave the transcript, t2's file change
    // becomes a rewound change.
    {
        let conn = s.conn();
        crate::agent::transcript::truncate_after_turn(&conn, S, "t1").unwrap();
        crate::agent::file_working_set::mark_rewound_after(&conn, S, &HashSet::from(["t1".to_string()])).unwrap();
    }
    let t4 = s.run(turn("t4", "换个思路", vec![reply("好。")]).with(|i| i.memory_context = memory(&["项目: uClaw"]))).await;
    let p4 = last_user_text(&t4[0]);
    assert!(p4.contains("<rewound_file_changes>") && p4.contains("src/main.rs"), "{p4}");
    // The rewound turns' notes are gone from the ledger: rules and mode are
    // announced again relative to what is still active (t1).
    assert!(p4.contains("<project_rules_update>"), "{p4}");
    // After a rewind the request continues the request of the turn rewound to.
    assert_prefix(Provider::OpenAi, t1.last().unwrap(), &t4[0]);
    let t5 = s.run(turn("t5", "继续", vec![reply("嗯。")]).with(|i| i.memory_context = memory(&["项目: uClaw"]))).await;
    assert_prefix(Provider::OpenAi, t4.last().unwrap(), &t5[0]);
    let p5 = last_user_text(&t5[0]);
    assert!(!p5.contains("<rewound_file_changes>") && !p5.contains("<project_rules_update>"), "sticky blocks repeat:\n{p5}");
}

#[tokio::test]
async fn compaction_turn_breaks_prefix_only_at_session_block() {
    let mut s = Session::new(Provider::OpenAi);
    let mem = |i: &mut TurnContextInputs| i.memory_context = memory(&["项目: uClaw"]);
    let mut turns = Vec::new();
    for (id, tag) in [("t1", "第一段"), ("t2", "第二段"), ("t3", "第三段")] {
        turns.push(s.run(turn(id, filler(tag, 9_000), vec![reply(format!("{tag}收到。"))]).with(mem)).await);
    }
    assert_chain(Provider::OpenAi, &all(&turns));
    // t4 pushes the request over the threshold: nothing to clear, so the
    // loop compacts t1 into a session_block before the first call.
    let t4 = s.run(turn("t4", filler("第四段", 9_000), vec![reply("第四段收到。")]).with(mem)).await;
    let prev = turns[2].last().unwrap();
    let d = diff(Provider::OpenAi, prev, &t4[0]);
    assert_eq!(d, Diff { first_divergence: Some(0), ..PREFIX }, "only the conversation head changes");
    let (p, n) = (parts(Provider::OpenAi, prev), parts(Provider::OpenAi, &t4[0]));
    assert!(n.conversation[0].contains(crate::agent::compaction_policy::SESSION_BLOCK_HEADER), "{}", n.conversation[0]);
    // Everything after the block is the old request from the cut on, byte for byte.
    let kept = &n.conversation[1..];
    let cut = p.conversation.iter().position(|m| m == &kept[0]).expect("kept tail starts with an old message");
    assert_eq!(cut, 2, "t1 (user + reply) folded");
    assert_eq!(&kept[..p.conversation.len() - cut], &p.conversation[cut..]);
    // Next turn: prefix again (the block replays from its transcript row).
    let t5 = s.run(turn("t5", "现在呢", vec![reply("继续。")]).with(mem)).await;
    assert_prefix(Provider::OpenAi, t4.last().unwrap(), &t5[0]);
    // TODO(ctx-p3 deferred, Task 6 minor "compaction in same iteration drops
    // most of this turn's memory until next turn"): t4's turn context is
    // rendered before the compaction folds t1's memory away, so t4 runs
    // without it. Not asserted here; t5 re-injects the memory in full.
    assert!(last_user_text(&t5[0]).contains("<memory_context>"), "{}", last_user_text(&t5[0]));
}

#[tokio::test]
async fn clearing_turn_breaks_prefix_only_at_cleared_results() {
    let mut s = Session::new(Provider::OpenAi);
    let mem = |i: &mut TurnContextInputs| i.memory_context = memory(&["项目: uClaw"]);
    let mut turns = Vec::new();
    for (id, path) in [("t1", "a.log"), ("t2", "b.log")] {
        let big = filler(path, 12_000);
        turns.push(
            s.run(turn(id, format!("读 {path}"), vec![tool("read_file", serde_json::json!({"path": path}), big), reply("读完了。")]).with(mem))
                .await,
        );
    }
    // t3's tool result pushes the second call over the threshold; clearing
    // the two older results is enough, so no compaction runs.
    let t3 = s
        .run(turn("t3", "读 c.log", vec![tool("read_file", serde_json::json!({"path": "c.log"}), filler("c.log", 12_000)), reply("读完了。")]).with(mem))
        .await;
    assert_chain(Provider::OpenAi, &[turns[0].last().unwrap(), turns[1].last().unwrap(), &t3[0]]);
    let d = diff(Provider::OpenAi, &t3[0], &t3[1]);
    let (p, n) = (parts(Provider::OpenAi, &t3[0]), parts(Provider::OpenAi, &t3[1]));
    let changed: Vec<usize> = (0..p.conversation.len()).filter(|&i| p.conversation[i] != n.conversation[i]).collect();
    assert_eq!(d, Diff { first_divergence: changed.first().copied(), ..PREFIX });
    assert_eq!(changed.len(), 2, "exactly t1's and t2's tool results: {changed:?}");
    for &i in &changed {
        assert!(n.conversation[i].contains(crate::agent::tool_result_clearing::CLEARED_PREFIX), "{}", n.conversation[i]);
    }
    // Next turn replays the cleared overlay: prefix again.
    let t4 = s.run(turn("t4", "好", vec![reply("嗯。")]).with(mem)).await;
    assert_prefix(Provider::OpenAi, t3.last().unwrap(), &t4[0]);
}

#[tokio::test]
async fn frame_rebuild_changes_only_system() {
    let mut s = Session::new(Provider::OpenAi);
    let mem = |i: &mut TurnContextInputs| i.memory_context = memory(&["项目: uClaw"]);
    let t1 = s.run(turn("t1", "你好", vec![tool("bash", serde_json::json!({"command": "ls"}), "src"), reply("你好。")]).with(mem)).await;
    // The prompt template changes → the frame key changes → rebuilt frame.
    s.base_prompt = "你是 uClaw，一个本地智能体。回答用中文。".into();
    let t2 = s.run(turn("t2", "再来", vec![reply("好。")]).with(mem)).await;
    assert_eq!(
        diff(Provider::OpenAi, t1.last().unwrap(), &t2[0]),
        Diff { system_same: false, ..PREFIX },
        "only the system prompt changes on a frame rebuild"
    );
    let reason = crate::agent::session_frame::load(&s.conn(), S).unwrap().unwrap().rebuild_reason;
    assert_ne!(reason, crate::agent::session_frame::FrameRebuildReason::New);
    let t3 = s.run(turn("t3", "继续", vec![reply("嗯。")]).with(mem)).await;
    assert_prefix(Provider::OpenAi, t2.last().unwrap(), &t3[0]);
}

#[tokio::test]
async fn tools_change_changes_only_tools() {
    let mut s = Session::new(Provider::OpenAi);
    let mem = |i: &mut TurnContextInputs| i.memory_context = memory(&["项目: uClaw"]);
    let t1 = s.run(turn("t1", "你好", vec![tool("bash", serde_json::json!({"command": "ls"}), "src"), reply("你好。")]).with(mem)).await;
    // A tool appears between turns (e.g. an MCP server connected).
    s.tools = registry(&s.workspace, true);
    let t2 = s.run(turn("t2", "搜一下", vec![tool("grep", serde_json::json!({"pattern": "main"}), "src/main.rs"), reply("找到了。")]).with(mem)).await;
    assert_eq!(
        diff(Provider::OpenAi, t1.last().unwrap(), &t2[0]),
        Diff { tools_same: false, ..PREFIX },
        "only the tool list changes"
    );
    let p2 = last_user_text(&t2[0]);
    assert!(p2.contains("<tools_update>") && p2.contains("grep"), "{p2}");
    assert_prefix(Provider::OpenAi, &t2[0], &t2[1]);
    let t3 = s.run(turn("t3", "继续", vec![reply("嗯。")]).with(mem)).await;
    assert_prefix(Provider::OpenAi, t2.last().unwrap(), &t3[0]);
    assert!(!last_user_text(&t3[0]).contains("<tools_update>"));
}
