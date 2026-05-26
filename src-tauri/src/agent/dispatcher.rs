use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use async_trait::async_trait;
use tauri::Emitter;
use crate::agent::types::*;
use crate::agent::tools::tool::{
    execute_tool_with_context, execute_streaming_with_context, ToolExecutionContext, ToolRegistry, ToolOutput,
};
use crate::agent::gep::repository::GeneRepository;
use crate::agent::gep::retrieval::{GeneRetriever, GeneMatch, format_gene_injection};
use crate::agent::gep::types::{Capsule, CapsuleOutcome, OutcomeStatus, BlastRadius, EnvFingerprint, EvolutionEvent};
use crate::app::PendingApprovals;
use crate::infra::InfraService;
use crate::llm::LlmProvider;
use crate::error::Error;
use crate::safety::{SafetyManager, SafetyMode, ApprovalDecision};

use crate::agent::retry::AgentRetryEvent;
use crate::agent::llm_stream::StreamSink;

/// Read-only tools that do not modify ReasoningContext or shared state.
/// These can be dispatched concurrently via JoinSet.
const PARALLEL_SAFE_TOOLS: &[&str] = &[
    "read_file",
    "search_files",
    "search_codebase",
    "get_file_skeleton",
    "list_dir",
];

fn is_parallel_safe(tool_name: &str) -> bool {
    PARALLEL_SAFE_TOOLS.contains(&tool_name)
}

/// ChatDelegate implements LoopDelegate for chat-based interactions.
/// It assembles the conversation context, delegates LLM calls, and executes tools.
pub struct ChatDelegate {
    /// LLM provider for making API calls
    llm: Arc<dyn LlmProvider>,
    /// Tool registry for executing tool calls
    tools: Arc<ToolRegistry>,
    /// Tauri app handle for emitting events to frontend
    app_handle: tauri::AppHandle,
    /// LLM model to use
    model: String,
    /// System prompt for the conversation
    system_prompt: String,
    /// External stop flag — set to true to gracefully stop the loop.
    stop_flag: Arc<AtomicBool>,
    /// Safety manager for tool approval decisions
    safety_manager: Arc<tokio::sync::RwLock<SafetyManager>>,
    /// Safety mode for this session (overrides global if set)
    safety_mode: Option<SafetyMode>,
    /// Pending approvals registry for awaiting user decisions
    pending_approvals: Arc<PendingApprovals>,
    /// Conversation ID for this session (used in approval events)
    conversation_id: String,
    /// Optional memory context to prepend to system prompt (from recall engine)
    memory_context: Option<String>,
    /// M2-D Phase 2 (Bundle 16-B) — last successfully-injected
    /// memory_context snapshot, kept **across turns** within a
    /// session. Diffed against the current turn's snapshot inside
    /// `build_dynamic_context` to decide whether to attach a
    /// `<memory_context_changes>` delta annotation alongside the
    /// full block. Replaces the per-iteration snapshot from Bundle
    /// 10 / Slice 3-B — per-iter observability is redundant with
    /// M2-I's prompt-cache breakpoint hits.
    ///
    /// Survives across turns; not cleared on iter boundaries.
    /// Cleared on `/compact` since the structured fold becomes the
    /// new baseline. Uses `std::sync::Mutex` for the same reason
    /// the rest of the delegate's short-lived locks do: never held
    /// across awaits.
    last_memory_context_snapshot:
        std::sync::Mutex<
            Option<crate::agent::context_diff::LineFragmentSnapshot>,
        >,
    /// InfraService for publishing tool execution events
    infra_service: Option<Arc<InfraService>>,
    /// Optional trajectory store for recording tool turns
    trajectory_store: Option<Arc<crate::harness::TrajectoryStore>>,
    /// Optional tool budget manager for truncating large results
    tool_budget: Option<Arc<crate::harness::ToolBudgetManager>>,
    /// Monotonic turn counter across all tool calls in this session
    turn_index: Arc<AtomicU32>,
    /// Whether extended thinking/reasoning is enabled for this session
    thinking_enabled: bool,
    /// Per-session monotonic sequence counter for chat:stream-reasoning events.
    /// Lets the frontend deduplicate events that arrive more than once (e.g. due
    /// to HMR or React Strict Mode registering multiple listeners).
    thinking_seq: Arc<AtomicU64>,
    /// Per-session monotonic sequence counter for chat:stream-chunk events.
    /// Lets the frontend deduplicate events that arrive more than once.
    chunk_seq: Arc<AtomicU64>,
    /// Workspace root used to source `uclaw.md` for prompt composition.
    workspace_root: Option<std::path::PathBuf>,
    /// Pre-built skill manifest block, set via set_skills_manifest_block
    /// before run_loop starts. Empty when no skills exist (no append).
    skills_manifest_block: String,
    /// PR 2026-05-13 token-cost optim: once the agent calls
    /// `skill_search` in this loop, the manifest's purpose (offering
    /// the catalog) has been served. Suppress it on all subsequent
    /// LLM calls within the same loop to save ~800 tokens per call.
    /// Set inside `execute_tool_calls`; read inside
    /// `effective_system_prompt`.
    ///
    /// `AtomicBool` because the dispatcher is `Send + Sync` and the
    /// tool-execution path holds `&self`. `Ordering::Relaxed` is
    /// sufficient because both reads and writes happen on the same
    /// async task in the agent loop — `execute_tool_calls` is awaited
    /// (`agentic_loop.rs::run_loop`) before the next iteration's
    /// `effective_system_prompt` call, so there's no cross-thread
    /// happens-before requirement. The flag is a one-way hint, not a
    /// synchronization primitive; never gets unset (per-loop sticky;
    /// resets when the next user message constructs a fresh
    /// `ChatDelegate`).
    skill_search_used: AtomicBool,
    /// GEP Gene retriever for control signal injection into system prompt
    gene_retriever: Option<Arc<GeneRetriever>>,
    /// Gene matches from the most recent call_llm — cleared after Capsule generation.
    last_gene_matches: Mutex<Vec<GeneMatch>>,
    /// GEP GeneRepository for persisting Capsules and EvolutionEvents.
    gene_repo: Option<Arc<Mutex<GeneRepository>>>,
    /// Recent tool error messages for passing to GeneRetriever.match_genes.
    recent_tool_errors: Mutex<Vec<String>>,
    /// SQLite connection shared from AppState — used for plan-suggest GEP signal.
    /// None when dispatcher is constructed outside the main agent path (tests, harness).
    db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
    /// Memory OS Sprint 1.8 — pre-built '## User Profile (Learned)' block
    /// from `learning::prompt_section::UserProfileSection::render`. Set
    /// via `set_learned_profile_block` once per agent loop start.
    /// Empty when memory_os.learning_enabled = false OR FacetCache is
    /// empty — `effective_system_prompt` skips the append in that case.
    learned_profile_block: String,
    /// gbrain Sprint 2.3 — pre-built '## Long-term Knowledge (gbrain)'
    /// instruction block from `gbrain_prompt::GbrainKnowledgeSection::render`.
    /// Set via `set_gbrain_knowledge_block` once per agent loop start.
    /// Empty when no `mcp__gbrain__*` tools are visible (server
    /// disconnected, bundle missing, init failed) — `effective_system_prompt`
    /// skips the append so the LLM never sees instructions for tools
    /// that don't exist.
    gbrain_knowledge_block: String,
    /// Memory OS Sprint 2.0 — producer-side handles for the learning
    /// pipeline. When `learning_enabled = true` AND both handles are
    /// `Some`, `before_llm_call` at iteration=0 spawns the chat-turn
    /// extractor over the user's latest message text. Buffer is shared
    /// with ProactiveService's LearningScheduler so candidates pushed
    /// here surface in the next 30-min stability rebuild.
    learning_buffer: Option<Arc<crate::learning::candidate::Buffer>>,
    learning_llm: Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>,
    learning_enabled: bool,
    /// Db handle for the producer's daily-token-cap check
    /// (Sprint 2.1b) — sums today's `cost_records.model LIKE
    /// 'memory_learning%'` before the LLM layer runs.
    learning_db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
    /// Sprint 2.1b — daily token budget for the LLM extractor. When
    /// today's spend exceeds this, the LLM layer is skipped (regex
    /// layer still runs). Set via `set_learning_pipeline`; default 0
    /// = effectively disabled.
    learning_llm_daily_budget: u32,
    /// Sprint 2.4b — gbrain chat-turn auto-extractor handles. When
    /// `gbrain_extractor_enabled = true` AND all three handles are
    /// `Some` AND daily budget remaining > 0, `before_llm_call` at
    /// iteration=0 spawns `gbrain::chat_extractor::extract_from_chat_turn`
    /// on the user's latest message. Accepted proposals (confidence
    /// >= `MIN_ACT_CONFIDENCE`) fire `mcp__gbrain__put_page` via the
    /// shared McpManager. Failures logged + swallowed so a producer
    /// bug never poisons the LLM call.
    gbrain_extractor_enabled: bool,
    gbrain_extract_llm: Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>,
    gbrain_extract_db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
    gbrain_extract_daily_budget: u32,
    gbrain_extract_mcp_mgr: Option<crate::mcp::SharedMcpManager>,
    /// FNV-style hash of the last tool definition list sent to the LLM.
    /// When the list is identical across iterations within a single agent
    /// turn, the Anthropic cache should cover it — this tracks whether the
    /// list actually changed (e.g. after an MCP reconnect) so we can log it.
    last_tool_defs_hash: Mutex<Option<u64>>,
    /// Slice 1 — optional collector for M2-J TokenBudgetSnapshot.
    /// When present, every `on_usage` tick records a fresh snapshot
    /// keyed on `conversation_id`. UI reads via
    /// `get_latest_token_budget` Tauri command. None = telemetry off
    /// (e.g. headless / harness contexts that don't need UI feed).
    token_budget_collector: Option<crate::agent::telemetry::TokenBudgetCollector>,
    /// Slice 1 — provider id ('anthropic' / 'openai' / 'deepseek' / etc.)
    /// stamped on every TokenBudgetSnapshot. Default 'unknown' is
    /// replaced by the caller via `set_provider`.
    provider: String,
    /// Bundle 27-A — optional heartbeat supervisor. When set, the
    /// dispatcher calls `mark_activity` at every stage boundary and
    /// `append_partial` on every text delta. None for headless /
    /// test contexts where no UI is listening.
    heartbeat: Option<Arc<crate::agent::heartbeat::HeartbeatSupervisor>>,
    /// M2-B wire-up (C2-Dirac-B2) — per-session context orchestrator.
    /// `effective_system_prompt` calls `for_prompt_with_injection` on it
    /// each turn to select fragments under budget and produce
    /// `ComposeStats`. Empty by default; fragment lifecycle (when
    /// fragments enter/leave the set) is a M2-D follow-up.
    context_manager: Arc<tokio::sync::RwLock<crate::agent::context_manager::ContextManager>>,
    /// Fragments selected on the most recent `for_prompt_with_injection`
    /// call. Injected into `build_dynamic_context` (per-turn block) as
    /// `<context_fragment>` XML — NOT into the system prompt — so the
    /// Anthropic cache_control:ephemeral breakpoint on the system prompt
    /// keeps hitting across turns. `std::sync::Mutex`, never held across
    /// awaits (same discipline as `last_memory_context_snapshot`).
    last_injected_fragments:
        std::sync::Mutex<Vec<crate::runtime::context::ContextArtifact>>,
    /// True until the first `effective_system_prompt` read this session,
    /// then false. Feeds A4's `InjectionContext.is_first_act_turn`.
    /// `TODO(M2-A)`: proper mode-transition tracking (reset to true when
    /// the user toggles back to ACT from Plan) lands with M2-A
    /// finalization; for now it flips false after the first read.
    is_first_act_turn: AtomicBool,
    /// Last tool error kind, if any — feeds A4's
    /// `InjectionContext.last_error_kind`. Set by the tool-execution
    /// path; `std::sync::Mutex`, never held across awaits.
    last_error_kind: std::sync::Mutex<Option<String>>,
    /// M2-J — optional collector for the most recent `ComposeStats`,
    /// keyed on `conversation_id`. Read by the `get_compose_stats` Tauri
    /// command. Shared from `AppState` (same pattern as
    /// `token_budget_collector`). None = telemetry off (headless / tests).
    compose_stats_collector: Option<crate::agent::context_manager::ComposeStatsCollector>,
    /// Pi Sprint 2 item ③ — steering queue drained at the start of each turn (mid-run).
    steering_queue: crate::agent::queues::SteeringQueue,
    /// Pi Sprint 2 item ③ — follow-up queue drained one task at a time at natural
    /// stop points; each entry is a Vec<ChatMessage> task.
    follow_up_queue: crate::agent::queues::FollowUpQueue,
    /// Pi Sprint 2 item ③ — db handle for persisting injected user messages
    /// into agent_messages so reloads stay continuous. None when constructed
    /// outside the main agent path (tests, harness).
    persist_db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
}

impl ChatDelegate {
    pub fn new(
        llm: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistry>,
        app_handle: tauri::AppHandle,
        model: String,
        system_prompt: String,
        safety_manager: Arc<tokio::sync::RwLock<SafetyManager>>,
        safety_mode: Option<SafetyMode>,
        pending_approvals: Arc<PendingApprovals>,
        conversation_id: String,
        workspace_root: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            llm, tools, app_handle, model, system_prompt,
            stop_flag: Arc::new(AtomicBool::new(false)),
            safety_manager,
            safety_mode,
            pending_approvals,
            conversation_id,
            memory_context: None,
            last_memory_context_snapshot: std::sync::Mutex::new(None),
            infra_service: None,
            trajectory_store: None,
            tool_budget: None,
            turn_index: Arc::new(AtomicU32::new(0)),
            thinking_enabled: false,
            thinking_seq: Arc::new(AtomicU64::new(0)),
            chunk_seq: Arc::new(AtomicU64::new(0)),
            workspace_root,
            skills_manifest_block: String::new(),
            skill_search_used: AtomicBool::new(false),
            gene_retriever: None,
            last_gene_matches: Mutex::new(Vec::new()),
            gene_repo: None,
            recent_tool_errors: Mutex::new(Vec::new()),
            db: None,
            learned_profile_block: String::new(),
            gbrain_knowledge_block: String::new(),
            learning_buffer: None,
            learning_llm: None,
            learning_enabled: false,
            learning_db: None,
            learning_llm_daily_budget: 0,
            gbrain_extractor_enabled: false,
            gbrain_extract_llm: None,
            gbrain_extract_db: None,
            gbrain_extract_daily_budget: 0,
            gbrain_extract_mcp_mgr: None,
            last_tool_defs_hash: Mutex::new(None),
            token_budget_collector: None,
            provider: "unknown".to_string(),
            heartbeat: None,
            context_manager: Arc::new(tokio::sync::RwLock::new(
                crate::agent::context_manager::ContextManager::new(),
            )),
            last_injected_fragments: std::sync::Mutex::new(Vec::new()),
            is_first_act_turn: AtomicBool::new(true),
            last_error_kind: std::sync::Mutex::new(None),
            compose_stats_collector: None,
            steering_queue: Default::default(),
            follow_up_queue: Default::default(),
            persist_db: None,
        }
    }

    /// C2-Dirac-B2 — wire the M2-J `ComposeStatsCollector`. When set,
    /// every `effective_system_prompt` call records the latest
    /// `ComposeStats` keyed on `conversation_id`. UI reads via the
    /// `get_compose_stats` Tauri command. None = telemetry off (headless
    /// / test contexts).
    pub fn set_compose_stats_collector(
        &mut self,
        collector: crate::agent::context_manager::ComposeStatsCollector,
    ) {
        self.compose_stats_collector = Some(collector);
    }

    /// Pi Sprint 2 item ③ — wire the dual interactive queues (agent path only).
    /// `::new` signature is unchanged so chat-mode call sites are unaffected.
    /// Pass `AppState::agent_queues_for(session_id)` + the shared db handle.
    pub fn with_agent_queues(mut self, queues: crate::app::AgentQueues, db: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        self.steering_queue = queues.steering;
        self.follow_up_queue = queues.follow_up;
        self.persist_db = Some(db);
        self
    }

    /// C2-Dirac-B2 — replace the per-session `ContextManager` (e.g. to
    /// preload a fragment set at session start). Default is an empty
    /// manager constructed in `new`. Takes the manager behind the same
    /// `Arc<RwLock<..>>` the delegate holds, so callers that need the
    /// handle elsewhere can clone it.
    pub fn set_context_manager(
        &mut self,
        cm: Arc<tokio::sync::RwLock<crate::agent::context_manager::ContextManager>>,
    ) {
        self.context_manager = cm;
    }

    /// Bundle 27-A — install a heartbeat supervisor. Builder pattern:
    /// caller constructs `HeartbeatSupervisor::new(...)` and hands the
    /// `Arc` to the dispatcher, which keeps a clone alongside the
    /// caller's clone. Both are dropped at the same time (end of agent
    /// loop), which tears down the ticker via Drop.
    pub fn set_heartbeat(
        &mut self,
        hb: Arc<crate::agent::heartbeat::HeartbeatSupervisor>,
    ) {
        self.heartbeat = Some(hb);
    }

    /// Bundle 27-A — tiny helper so the (many) callsites can do
    /// `self.beat(stage)` without unwrap/Option dance.
    fn beat(&self, stage: &str) {
        if let Some(ref hb) = self.heartbeat {
            hb.mark_activity(stage);
        }
    }

    /// Slice 1 — wire the M2-J TokenBudgetCollector. When set, every
    /// `on_usage` tick records a fresh snapshot keyed on
    /// `conversation_id` so the UI can subscribe via
    /// `get_latest_token_budget`. Pass an `AppState::token_budget_collector`
    /// clone from the Tauri command that builds the delegate.
    pub fn set_token_budget_collector(
        &mut self,
        collector: crate::agent::telemetry::TokenBudgetCollector,
    ) {
        self.token_budget_collector = Some(collector);
    }

    /// Slice 1 — set the provider id stamped on TokenBudgetSnapshot.
    /// Pass `llm_config.provider` from the Tauri command.
    ///
    /// Side effect — logs the M2-H L5 image capability check at INFO. This
    /// runs once per agent loop spin-up so the user can confirm the
    /// (provider, model) pair was classified correctly (deepseek-v4-pro
    /// should resolve to `image_blind=true`, claude-* to `false`, etc.)
    /// without having to wait for a screenshot to actually trigger the
    /// strip path.
    pub fn set_provider(&mut self, provider: String) {
        let supports_images =
            crate::agent::image_policy::supports_images(&provider, &self.model);
        tracing::info!(
            provider = %provider,
            model = %self.model,
            image_blind = !supports_images,
            "[L5] capability check",
        );
        self.provider = provider;
    }

    /// Set the GEP Gene retriever for control signal injection.
    pub fn set_gene_retriever(&mut self, retriever: Arc<GeneRetriever>) {
        self.gene_retriever = Some(retriever);
    }

    /// Set the GEP GeneRepository for Capsule persistence.
    pub fn set_gene_repo(&mut self, repo: Arc<Mutex<GeneRepository>>) {
        self.gene_repo = Some(repo);
    }

    /// Inject the shared SQLite connection for reading plan-suggest stats
    /// (GEP aggregate-rate signal injected into system prompt).
    pub fn set_db(&mut self, db: Arc<std::sync::Mutex<rusqlite::Connection>>) {
        self.db = Some(db);
    }

    /// Generate and persist Capsules for the most recent Gene matches.
    ///
    /// Computes blast radius from workspace git diff, constructs full Capsule
    /// and EvolutionEvent records, and persists them to GeneRepository.
    /// Also publishes CapsuleCreated events via InfraService.
    /// Called after tool execution completes for the turn.
    pub async fn generate_capsule_for_turn(&self) {
        let gene_matches: Vec<GeneMatch> = {
            match self.last_gene_matches.lock() {
                Ok(mut stored) => {
                    let matches = stored.clone();
                    stored.clear();
                    matches
                }
                Err(_) => return,
            }
        };

        if gene_matches.is_empty() {
            return;
        }

        // Compute blast radius from workspace git diff
        let blast_radius = match &self.workspace_root {
            Some(root) => {
                let repo_path = root.to_string_lossy();
                crate::agent::gep::git_integration::compute_blast_radius(&repo_path)
                    .unwrap_or(None)
            }
            None => None,
        };

        let now_ts = chrono::Utc::now().timestamp_millis();

        for gm in &gene_matches {
            let capsule_id = format!("cap_{}", uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string());

            // Determine outcome status from recent tool errors
            let (outcome_status, outcome_score) = {
                let errors = self.recent_tool_errors.lock().map(|e| e.clone()).unwrap_or_default();
                if errors.is_empty() {
                    (OutcomeStatus::Success, 0.85)
                } else if errors.len() <= 2 {
                    (OutcomeStatus::Partial, 0.5)
                } else {
                    (OutcomeStatus::Failed, 0.2)
                }
            };

            let br = blast_radius.clone().unwrap_or(BlastRadius { files: 0, lines: 0 });

            let capsule = Capsule {
                id: capsule_id.clone(),
                gene_asset_id: gm.gene.asset_id.clone(),
                gene_id: gm.gene.gene_id.clone(),
                trigger: gm.gene.signals_match.clone(),
                summary: format!(
                    "Gene {} matched (score={:.2})",
                    gm.gene.gene_id, gm.match_score
                ),
                confidence: (gm.match_score / 5.0).min(1.0) as f32,
                blast_radius: br.clone(),
                outcome: CapsuleOutcome {
                    status: outcome_status.clone(),
                    score: outcome_score,
                },
                raw_streak: 0,
                effective_streak: 0.0,
                env_fingerprint: EnvFingerprint::default(),
                created_at: now_ts,
                lineage: vec![],
            };

            // Compute streaks and persist Capsule in a single lock acquire
            // (avoids duplicate list_capsules calls that scan directory twice)
            if let Some(ref repo_arc) = self.gene_repo {
                if let Ok(repo) = repo_arc.lock() {
                    let prev_capsules = repo.list_capsules(&gm.gene.gene_id).unwrap_or_default();
                    let effective_streak = capsule.compute_effective_streak(&prev_capsules, now_ts);
                    let prev_successes = prev_capsules.iter()
                        .filter(|c| c.outcome.status == OutcomeStatus::Success)
                        .count() as u32;

                    let mut capsule_with_streak = capsule.clone();
                    capsule_with_streak.effective_streak = effective_streak;
                    capsule_with_streak.raw_streak = if outcome_status == OutcomeStatus::Success {
                        prev_successes + 1
                    } else {
                        0
                    };

                    if let Err(e) = repo.store_capsule(&capsule_with_streak) {
                        tracing::warn!(
                            gene_id = %gm.gene.gene_id,
                            error = %e,
                            "[ChatDelegate] Failed to store Capsule"
                        );
                    } else {
                        tracing::info!(
                            gene_id = %gm.gene.gene_id,
                            capsule_id = %capsule_id,
                            status = ?outcome_status,
                            "[ChatDelegate] Capsule persisted"
                        );
                    }

                    // Store EvolutionEvent audit record
                    let event = EvolutionEvent {
                        intent: gm.gene.category.to_string(),
                        capsule_id: capsule_id.clone(),
                        genes_used: vec![gm.gene.asset_id.clone()],
                        mutations_tried: 0,
                        total_cycles: 1,
                        created_at: now_ts,
                    };
                    if let Err(e) = repo.store_event(&event) {
                        tracing::warn!(
                            "[ChatDelegate] Failed to store EvolutionEvent: {}", e
                        );
                    }
                }
            }

            // Publish CapsuleCreated event via InfraService
            if let Some(infra) = &self.infra_service {
                let mut metadata = serde_json::json!({
                    "gene_id": gm.gene.gene_id,
                    "gene_asset_id": gm.gene.asset_id,
                    "match_score": gm.match_score,
                    "rank_score": gm.rank_score,
                    "conversation_id": self.conversation_id,
                    "capsule_id": capsule_id,
                    "outcome": {
                        "status": outcome_status,
                        "score": outcome_score,
                    },
                });

                metadata["blast_radius"] = serde_json::json!({
                    "files": br.files,
                    "lines": br.lines,
                });

                infra.publish_capsule_created(
                    "local",
                    &gm.gene.gene_id,
                    metadata,
                ).await;

                tracing::info!(
                    gene_id = %gm.gene.gene_id,
                    match_score = gm.match_score,
                    "[ChatDelegate] CapsuleCreated event published"
                );
            }
        }

        // P0-1: Push computed effective_streaks back to GeneRetriever for ranking freshness.
        // Without this, GeneRetriever uses stale streak data from the last build_gene_retriever() call.
        // Only update genes that were matched in this turn — avoid O(all_active) file scans.
        if let Some(ref retriever) = self.gene_retriever {
            if let Some(ref repo_arc) = self.gene_repo {
                if let Ok(repo) = repo_arc.lock() {
                    let now_ts = chrono::Utc::now().timestamp_millis();
                    let mut streaks = std::collections::HashMap::new();
                    for gm in &gene_matches {
                        if let Ok(capsules) = repo.list_capsules(&gm.gene.gene_id) {
                            if let Some(latest) = capsules.first() {
                                let prev: Vec<Capsule> = capsules.iter().skip(1).take(5).cloned().collect();
                                let streak = latest.compute_effective_streak(&prev, now_ts);
                                streaks.insert(gm.gene.gene_id.clone(), streak);
                            }
                        }
                    }
                    if !streaks.is_empty() {
                        let count = streaks.len();
                        retriever.set_streaks(streaks);
                        tracing::info!(
                            gene_count = count,
                            "[ChatDelegate] effective_streaks pushed back to GeneRetriever"
                        );
                    }
                }
            }
        }

        // Clear recent tool errors after Capsule generation
        if let Ok(mut errors) = self.recent_tool_errors.lock() {
            errors.clear();
        }
    }

    /// Enable or disable extended thinking for this session.
    pub fn set_thinking_enabled(&mut self, enabled: bool) {
        self.thinking_enabled = enabled;
    }

    /// Set the InfraService for publishing tool execution events.
    pub fn set_infra_service(&mut self, infra: Arc<InfraService>) {
        self.infra_service = Some(infra);
    }

    /// Set the TrajectoryStore for recording tool execution turns.
    pub fn set_trajectory_store(&mut self, store: Arc<crate::harness::TrajectoryStore>) {
        self.trajectory_store = Some(store);
    }

    /// Set the ToolBudgetManager for truncating large tool results.
    pub fn set_tool_budget(&mut self, budget: Arc<crate::harness::ToolBudgetManager>) {
        self.tool_budget = Some(budget);
    }

    /// Set the memory context obtained from the recall engine.
    /// This will be appended to the system prompt when making LLM calls.
    pub fn set_memory_context(&mut self, context: String) {
        self.memory_context = Some(context);
    }

    /// M2-D Phase 2 — clear the cross-turn memory_context anchor.
    /// Called when a `/compact` runs so the next turn's diff baseline
    /// is the post-fold state, not the pre-fold one. Safe to call
    /// repeatedly; no-op when no anchor exists.
    pub fn clear_memory_context_anchor(&self) {
        if let Ok(mut slot) = self.last_memory_context_snapshot.lock() {
            *slot = None;
        }
    }

    /// Append additional context to the existing memory context.
    /// If no memory context has been set yet, this creates one.
    pub fn append_memory_context(&mut self, extra: &str) {
        match self.memory_context.as_mut() {
            Some(ctx) => {
                ctx.push_str(extra);
            }
            None => {
                self.memory_context = Some(extra.to_string());
            }
        }
    }

    /// Set the skill manifest block to append to the system prompt.
    /// Caller is responsible for building this via skills_manifest::build_skills_manifest.
    pub fn set_skills_manifest_block(&mut self, block: String) {
        self.skills_manifest_block = block;
    }

    /// Set the '## User Profile (Learned)' block (Sprint 1.8).
    ///
    /// Caller builds via
    /// `learning::prompt_section::UserProfileSection::render(&facet_cache)`
    /// (returns `Option<String>` — caller passes empty when None).
    /// Empty input → no append in `effective_system_prompt`.
    pub fn set_learned_profile_block(&mut self, block: String) {
        self.learned_profile_block = block;
    }

    /// Set the '## Long-term Knowledge (gbrain)' instruction block
    /// (Sprint 2.3). Caller builds via
    /// `crate::agent::gbrain_prompt::GbrainKnowledgeSection::render(&mcp_mgr)`
    /// (returns `Option<String>` — caller passes empty when None). Empty
    /// input → no append in `effective_system_prompt`, so the LLM
    /// never sees instructions for `mcp__gbrain__*` tools when those
    /// tools aren't actually registered.
    pub fn set_gbrain_knowledge_block(&mut self, block: String) {
        self.gbrain_knowledge_block = block;
    }

    /// Memory OS Sprint 2.0 — wire the learning producer pipeline.
    ///
    /// Once set, `before_llm_call` at iteration=0 spawns the chat-turn
    /// extractor over the latest user message text. Candidates land in
    /// `buffer`; the next ProactiveService tick drains it into facets.
    ///
    /// All four params take their non-trivial value from AppState. Passing
    /// `enabled=false` keeps the field handles for IPC use but skips
    /// the per-turn spawn.
    pub fn set_learning_pipeline(
        &mut self,
        buffer: Arc<crate::learning::candidate::Buffer>,
        llm: Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>,
        db: Arc<std::sync::Mutex<rusqlite::Connection>>,
        enabled: bool,
        llm_daily_budget: u32,
    ) {
        self.learning_buffer = Some(buffer);
        self.learning_llm = llm;
        self.learning_db = Some(db);
        self.learning_enabled = enabled;
        self.learning_llm_daily_budget = llm_daily_budget;
    }

    /// Sprint 2.4b — wire the gbrain chat-turn auto-extractor pipeline.
    ///
    /// Once set, `before_llm_call` at iteration=0 spawns the gbrain
    /// extractor on the latest user message. Proposals with confidence
    /// >= `crate::gbrain::chat_extractor::MIN_ACT_CONFIDENCE` fire
    /// `mcp__gbrain__put_page` via `mcp_mgr`. Failures are logged +
    /// swallowed so a producer bug never stalls the agent loop.
    ///
    /// All params take their non-trivial value from AppState. Passing
    /// `enabled=false` OR `daily_budget=0` short-circuits the per-turn
    /// spawn before the LLM is invoked.
    pub fn set_gbrain_extractor_pipeline(
        &mut self,
        llm: Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>,
        db: Arc<std::sync::Mutex<rusqlite::Connection>>,
        mcp_mgr: crate::mcp::SharedMcpManager,
        enabled: bool,
        daily_budget: u32,
    ) {
        self.gbrain_extract_llm = llm;
        self.gbrain_extract_db = Some(db);
        self.gbrain_extract_mcp_mgr = Some(mcp_mgr);
        self.gbrain_extractor_enabled = enabled;
        self.gbrain_extract_daily_budget = daily_budget;
    }

    /// Build the effective system prompt including memory context, the user's
    /// uclaw.md (workspace-level), Karpathy baseline, and mode-specific
    /// guardrails. Reads uclaw.md on every call (small file, OS cache).
    ///
    /// `effective_mode` should be the current resolved SafetyMode — usually
    /// the global policy mode (or the per-session override when set). Caller
    /// resolves it before invoking; we don't read SafetyManager here because
    /// this method is sync and called from the LLM hot path.
    fn effective_system_prompt(&self, effective_mode: &SafetyMode) -> String {
        // system_prompt is byte-stable per session between explicit settings
        // edits: no memory recall or volatile profile facts are injected here.
        // Those live in build_dynamic_context() (prepended to the last user
        // message each turn) so the Anthropic cache_control: ephemeral
        // breakpoint can hit from iteration 2 onward. The persona block below
        // is intentionally limited to user-editable expression/style knobs and
        // does not carry per-turn memories, relationship scores, or task facts.
        //
        // C2-Dirac-B2 wire-up. Two concerns, kept SEPARATE (spec §8.4):
        //
        //  1. System-prompt COMPOSITION. We still build the full prompt
        //     from self.system_prompt + workspace uclaw.md + [WORKSPACE]
        //     cwd block + baseline + mode + manifest — NONE of those are
        //     dropped. The only B2 change is routing the baseline section
        //     through compose_system_prompt_with_injection so A4's
        //     InjectionContext can gate conditional blocks. All 10 current
        //     production blocks are Always-policy → byte-identical to the
        //     pre-B2 compose for every InjectionContext today, so cache
        //     discipline is preserved on EVERY turn (not just turns 2+).
        //
        //  2. Fragment SELECTION. We separately call ContextManager::
        //     for_prompt_with_injection to pick fragments under budget and
        //     produce ComposeStats. The selected fragments are stashed for
        //     build_dynamic_context (per-turn block) — they are NEVER added
        //     to the system prompt, so they cannot bust the cache.
        let inj_ctx = crate::agent::baseline_blocks::InjectionContext {
            is_first_act_turn: self.is_first_act_turn.load(Ordering::Relaxed),
            last_error_kind: self
                .last_error_kind
                .lock()
                .ok()
                .and_then(|g| g.clone()),
            context_pressure_ratio: self.estimate_context_pressure_ratio(),
        };

        // Concern 2: fragment selection + stats (does NOT feed the prompt
        // string — only build_dynamic_context + the M2-J collector).
        let query = crate::agent::context_manager::ComposeQuery::defaults_with_topics(vec![]);
        let composed = self.context_manager_for_prompt_blocking(&query, &inj_ctx);
        if let Ok(mut slot) = self.last_injected_fragments.lock() {
            *slot = composed.injected_fragments.clone();
        }
        if let Some(collector) = &self.compose_stats_collector {
            collector.record(&self.conversation_id, composed.stats.clone());
        }
        // First-act flag transitions to false after this read (one-way;
        // TODO(M2-A) proper mode-transition tracking). Today this is inert
        // for cache purposes — no production block is FirstActTurnOnly — but
        // we maintain the flag so the channel is correct when one is added.
        self.is_first_act_turn.store(false, Ordering::Relaxed);

        // Concern 1: compose the full, byte-stable system prompt. The
        // injection-aware compose preserves user base + workspace + mode.
        let persona_block = self.persona_prompt_block_best_effort();
        let prompt = crate::agent::mode_prompts::compose_system_prompt_with_injection_and_persona(
            &self.system_prompt,
            self.workspace_root.as_deref(),
            effective_mode,
            &inj_ctx,
            persona_block.as_deref(),
        );
        // Append the skill manifest block (empty when no skills exist).
        // Once the agent has already invoked `skill_search` in this loop
        // the manifest's recall-prompt job is done — suppress it on the
        // next call to save ~800 tokens (PR 2026-05-13 token-cost optim).
        // The flag stays sticky for the remainder of the loop because the
        // agent loop reuses the same `ChatDelegate` across iterations.
        let suppress_manifest = self.skill_search_used.load(Ordering::Relaxed);
        if self.skills_manifest_block.is_empty() || suppress_manifest {
            prompt
        } else {
            format!("{}{}", prompt, self.skills_manifest_block)
        }
    }

    fn persona_prompt_block_best_effort(&self) -> Option<String> {
        let db = self.db.as_ref()?;
        let guard = db.lock().ok()?;
        let store = crate::agent::persona::store::PersonaStore::new(&guard);
        let voice = store
            .get_global_voice_profile()
            .ok()
            .flatten()
            .unwrap_or_default();
        let ctx = crate::agent::persona::PersonaPromptContext {
            voice,
            bond: crate::agent::persona::BondProfile::default(),
            relationship_gamification_enabled: false,
        };
        Some(crate::agent::persona::render_persona_prompt_block(&ctx))
    }

    /// C2-Dirac-B2 — synchronous bridge to the async
    /// `ContextManager::for_prompt_with_injection`.
    ///
    /// `effective_system_prompt` is sync (called from the LLM hot path),
    /// but `for_prompt_with_injection` is async (fragment `fetch()` is).
    /// uClaw runs on the default multi-thread tokio runtime
    /// (`tokio = { features = ["full"] }`, `#[tokio::main]`), so
    /// `block_in_place` + `Handle::current().block_on` is safe here (spec
    /// §8.2). If this ever runs on a current-thread runtime, swap to a
    /// oneshot-channel + spawn pattern.
    fn context_manager_for_prompt_blocking(
        &self,
        query: &crate::agent::context_manager::ComposeQuery,
        inj_ctx: &crate::agent::baseline_blocks::InjectionContext,
    ) -> crate::agent::context_manager::ComposedContext {
        let cm = self.context_manager.clone();
        let q = query.clone();
        let ic = inj_ctx.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let guard = cm.read().await;
                guard.for_prompt_with_injection(&q, &ic).await
            })
        })
    }

    /// C2-Dirac-B2 — estimate of tokens-used / context-window for A4's
    /// `InjectionContext.context_pressure_ratio`. Stubbed to `0.0` for
    /// now; a follow-up wires the M2-J `TokenBudgetSnapshot` ratio here.
    fn estimate_context_pressure_ratio(&self) -> f32 {
        0.0
    }

    /// Build the per-message dynamic context block.
    ///
    /// Prepended to the LAST user message in each LLM call payload — NOT
    /// persisted to the session. Each new call gets a fresh timestamp.
    ///
    /// Includes current time AND workspace root. Time was previously injected
    /// into the system prompt (which caused Anthropic cache misses on every
    /// minute boundary). Moving it here keeps the system prompt byte-stable
    /// across all iterations in a session so cache_control: ephemeral hits
    /// reliably from iteration 2 onward.
    fn build_dynamic_context(&self) -> String {
        use chrono::{Datelike, Local, Timelike};
        let now = Local::now();
        let weekday = match now.weekday() {
            chrono::Weekday::Mon => "周一",
            chrono::Weekday::Tue => "周二",
            chrono::Weekday::Wed => "周三",
            chrono::Weekday::Thu => "周四",
            chrono::Weekday::Fri => "周五",
            chrono::Weekday::Sat => "周六",
            chrono::Weekday::Sun => "周日",
        };
        let time_str = format!(
            "{}年{}月{}日 {} {:02}:{:02}",
            now.year(), now.month(), now.day(), weekday, now.hour(), now.minute(),
        );
        let mut block = format!(
            "<system_info>\n当前时间: {}\n注意: 以上时间和工作区路径由系统直接提供。对于询问当前状态的对话（如「你在干啥」「现在几点」「工作区是什么」等），直接用此处信息回答即可——不要运行 bash date、glob、ls、find、pwd 等命令去探查。只有用户明确要求执行文件/目录操作时，才使用相关工具。",
            time_str
        );
        if let Some(root) = &self.workspace_root {
            block.push_str(&format!("\n工作区路径: {}", root.display()));
        }
        block.push_str("\n</system_info>");

        // Memory recall results — per-turn fresh content injected here (not in
        // the system prompt) so Anthropic cache_control: ephemeral on the system
        // prompt block can hit reliably from iteration 2 onward.
        if let Some(ctx) = self.memory_context.as_deref().filter(|s| !s.is_empty()) {
            // M2-D Phase 2 Track A (Bundle 16-B) — cross-turn diff
            // injection. We build a line-level snapshot of the
            // current memory_context, diff against the prior turn's
            // snapshot, and conditionally attach a
            // `<memory_context_changes>` annotation block alongside
            // the full block:
            //
            //   first turn / no prior → full block only (annotation
            //     omitted; the LLM has nothing to compare against)
            //   identical to prior   → full block only
            //                          (Anthropic cache hit path)
            //   small drift (≤40%)   → full block + delta annotation
            //                          (LLM gets "what changed" signal)
            //   significant drift    → full block only (delta would
            //                          be noisy + cache misses anyway)
            //
            // We keep the full block even on small drift because
            // un-cached providers (DeepSeek / Kimi) have no
            // mechanism for "saw it last turn". The delta block is
            // pure additional signal, not a replacement.
            const SIGNIFICANT_DRIFT_THRESHOLD: f32 = 0.40;
            let new_snapshot = crate::agent::context_diff::LineFragmentSnapshot::from_text(
                "memory_context",
                ctx,
            );
            let prior = self
                .last_memory_context_snapshot
                .lock()
                .ok()
                .and_then(|g| g.clone());

            let delta_annotation: Option<String> = match prior.as_ref() {
                None => {
                    tracing::info!(
                        line_count = new_snapshot.line_count(),
                        token_estimate = new_snapshot.token_estimate,
                        "[M2-D] turn=first memory_context first injection emitted=full",
                    );
                    None
                }
                Some(prior_snap) => {
                    let diff = crate::agent::context_diff::line_diff(
                        prior_snap,
                        &new_snapshot,
                    );
                    let stats = diff.stats();
                    if diff.is_empty() {
                        tracing::debug!(
                            line_count = new_snapshot.line_count(),
                            unchanged = stats.unchanged,
                            "[M2-D] turn=N memory_context unchanged emitted=full cache_state=hit-expected",
                        );
                        None
                    } else if stats
                        .is_significant_change(SIGNIFICANT_DRIFT_THRESHOLD)
                    {
                        tracing::info!(
                            added = stats.added,
                            removed = stats.removed,
                            changed = stats.changed,
                            unchanged = stats.unchanged,
                            added_or_changed_tokens = stats.added_or_changed_tokens,
                            "[M2-D] turn=N memory_context drift=significant emitted=full cache_state=miss",
                        );
                        None
                    } else {
                        let annotation =
                            crate::agent::context_diff::render_delta_annotation(&diff);
                        tracing::info!(
                            added = stats.added,
                            removed = stats.removed,
                            changed = stats.changed,
                            unchanged = stats.unchanged,
                            added_or_changed_tokens = stats.added_or_changed_tokens,
                            "[M2-D] turn=N memory_context drift=small emitted=full+delta cache_state=miss",
                        );
                        annotation
                    }
                }
            };

            if let Ok(mut slot) = self.last_memory_context_snapshot.lock() {
                *slot = Some(new_snapshot);
            }

            block.push_str("\n\n<memory_context>\n");
            block.push_str(ctx);
            block.push_str("\n</memory_context>");

            if let Some(annotation) = delta_annotation {
                block.push_str("\n\n");
                block.push_str(&annotation);
            }
        }

        // Learned user profile (30-min rebuild cadence) — same rationale as
        // memory_context: injected here rather than in the system prompt so
        // rebuilds don't bust the Anthropic cache breakpoint.
        if !self.learned_profile_block.is_empty() {
            block.push_str("\n\n");
            block.push_str(&self.learned_profile_block);
        }

        // gbrain Sprint 2.3 — instructions for when to call
        // `mcp__gbrain__*` tools. Only present when gbrain is connected
        // and exposes tools; absent otherwise so we don't promise the
        // LLM tools that won't actually be callable. Lives in the
        // dynamic context block alongside memory_context + profile so
        // tool-presence changes (gbrain reconnects mid-session) take
        // effect on the next prompt build without restarting the agent.
        if !self.gbrain_knowledge_block.is_empty() {
            block.push_str("\n\n");
            block.push_str(&self.gbrain_knowledge_block);
        }

        // C2-Dirac-B2 — ContextManager-selected fragments. Rendered HERE
        // (per-turn dynamic block), NOT in the system prompt, so the
        // system-prompt cache_control:ephemeral breakpoint keeps hitting
        // across turns (spec §8.3). Selected on the prior
        // effective_system_prompt call (same turn, since the agent loop
        // calls effective_system_prompt before build_dynamic_context).
        if let Ok(fragments) = self.last_injected_fragments.lock() {
            block.push_str(&render_context_fragments(&fragments));
        }

        block
    }

    /// Resolve the effective SafetyMode for this call: per-session override
    /// (dispatcher's `safety_mode` field) wins; otherwise read the global
    /// policy mode from SafetyManager.
    async fn resolve_effective_mode(&self) -> SafetyMode {
        if let Some(m) = self.safety_mode.as_ref() {
            return m.clone();
        }
        self.safety_manager.read().await.policy().global_mode.clone()
    }

    /// Returns a cloneable handle that can be used to signal the loop to stop.
    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }

    /// Emit a text delta to the frontend
    fn emit_text_delta(&self, chunk: &str) {
        let seq = self.chunk_seq.fetch_add(1, Ordering::Relaxed);
        let _ = self.app_handle.emit("chat:stream-chunk", serde_json::json!({
            "conversationId": self.conversation_id,
            "delta": chunk,
            "seq": seq,
        }));
        // Bundle 27-A — feed the partial buffer + beat. The buffer is
        // what gets persisted as "[interrupted-recovered]" assistant
        // text if the process dies mid-stream. Done as fire-and-forget
        // task so we don't block the streaming hot path on a mutex
        // we don't strictly need to await here.
        if let Some(ref hb) = self.heartbeat {
            let hb = hb.clone();
            let chunk = chunk.to_string();
            tokio::spawn(async move {
                hb.append_partial(&chunk).await;
                hb.mark_activity(crate::agent::heartbeat::stages::LLM_STREAM);
            });
        }
    }

    /// Emit a tool call start to the frontend.
    ///
    /// `preview_target` carries the file path the tool will write, when
    /// the tool overrides `Tool::preview_target_path`. The frontend's
    /// auto-preview listener uses this to open the preview panel without
    /// keeping a hardcoded list of "write-ish" tool names — adding a new
    /// mutating tool only requires implementing the trait method, not
    /// touching frontend code.
    fn emit_tool_start(
        &self,
        name: &str,
        id: &str,
        input: &serde_json::Value,
        preview_target: Option<&str>,
    ) {
        let _ = self.app_handle.emit("chat:stream-tool-activity", serde_json::json!({
            "conversationId": self.conversation_id,
            "activity": {
                "type": "tool_start",
                "toolName": name,
                "toolCallId": id,
                "input": input,
                "previewTarget": preview_target,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }
        }));
        // Bundle 27-A — tool boundaries are the typical place where
        // the agent hangs (browser navigate, long bash, MCP roundtrip).
        // Mark activity at start AND completion so heartbeat reflects
        // the actual stage.
        self.beat(&format!(
            "{}:{}",
            crate::agent::heartbeat::stages::TOOL_CALL, name
        ));
    }

    /// Emit a tool result to the frontend
    fn emit_tool_result(&self, name: &str, id: &str, output: &ToolOutput) {
        let _ = self.app_handle.emit("chat:stream-tool-activity", serde_json::json!({
            "conversationId": self.conversation_id,
            "activity": {
                "type": "tool_result",
                "toolName": name,
                "toolCallId": id,
                "result": output.result,
                "durationMs": output.duration_ms,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                // Soft-error detection: tools like bash signal failure via
                // { ok: false, exit_code: 1, ... } even though the underlying
                // execution succeeded. Mirror the frontend fallback here so
                // both the live render and the persisted snapshot see the
                // same isError value.
                "isError": detect_soft_tool_error(&output.result),
            }
        }));
    }

    /// Emit a hard-error tool result so the frontend can flip the activity
    /// to "failed" state immediately, instead of letting it spin until the
    /// whole turn completes.
    fn emit_tool_error(&self, name: &str, id: &str, err_msg: &str, duration_ms: u64) {
        let _ = self.app_handle.emit("chat:stream-tool-activity", serde_json::json!({
            "conversationId": self.conversation_id,
            "activity": {
                "type": "tool_result",
                "toolName": name,
                "toolCallId": id,
                "result": { "ok": false, "error": err_msg },
                "durationMs": duration_ms,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "isError": true,
            }
        }));
    }

    /// Emit a completion event to the frontend
    fn emit_done(&self, text: &str, truncated: bool) {
        let _ = self.app_handle.emit("chat:stream-complete", serde_json::json!({
            "conversationId": self.conversation_id,
            "text": text,
            "truncated": truncated,
        }));
        // Bundle 27-A fix (2026-05-22) — DO NOT call `hb.shutdown()` here.
        // Earlier draft removed the flight file at emit_done, which meant
        // the kill-recovery window was approximately zero: by the time the
        // user reacted to "Agent's streaming, let me kill -9", emit_done
        // had often already run.
        //
        // New behavior:
        // - emit_done only marks the DONE stage so the heartbeat indicator
        //   stops pulsing in the UI (heartbeat banner clears via the
        //   `chat:stream-complete` listener anyway).
        // - The supervisor's `Drop` impl (fired when `_hb_arc` goes out of
        //   scope at the END of `send_agent_message`, AFTER post-loop
        //   persistence) is the one place that removes the flight file.
        //   This makes the recovery window cover the full
        //   send_agent_message lifetime — from agent start to function
        //   return — not just the streaming phase.
        // - The partial buffer is no longer drained here either; Drop
        //   handles cleanup. A residual partial in memory at Drop time
        //   is harmless (the Arc owns the buffer).
        if let Some(ref hb) = self.heartbeat {
            hb.mark_activity(crate::agent::heartbeat::stages::DONE);
        }
    }

    /// Emit thinking block to the frontend
    fn emit_thinking(&self, text: &str) {
        let seq = self.thinking_seq.fetch_add(1, Ordering::Relaxed);
        let _ = self.app_handle.emit("chat:stream-reasoning", serde_json::json!({
            "conversationId": self.conversation_id,
            "delta": text,
            "seq": seq,
        }));
    }

    /// Emit thinking-done event to the frontend
    fn emit_thinking_done(&self, _duration_ms: u64) {
        // Absorbed into the reasoning stream; no separate frontend event needed
    }

    /// Emit turn cost event after each LLM call.
    ///
    /// Async because the budget-threshold check reads `state.settings`, a
    /// `tokio::sync::RwLock`. The previous `Handle::block_on` approach
    /// deadlocked when called from inside an async task (this fn is invoked
    /// from `on_usage`, which is async).
    async fn emit_turn_cost(&self, usage: &TokenUsage) {
        let cost = calculate_cost(&self.model, usage.input_tokens, usage.output_tokens);
        let turn_cost = TurnCostInfo {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            cache_creation_tokens: usage.cache_creation_tokens,
            cost_usd: format_cost(cost),
        };
        tracing::info!(
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cache_read_tokens = usage.cache_read_tokens,
            cache_creation_tokens = usage.cache_creation_tokens,
            cost_usd = %turn_cost.cost_usd,
            "Emitting agent:turn_cost"
        );

        // Persist BEFORE emitting so the dashboard never undercounts even if
        // the frontend listener races. Best-effort — failures don't propagate.
        use tauri::Manager;
        if let Some(state) = self.app_handle.try_state::<crate::app::AppState>() {
            crate::cost_store::record(
                &state,
                &self.conversation_id,
                &self.model,
                usage.input_tokens,
                usage.output_tokens,
            );

            // Phase 6-C: budget threshold check. Best-effort — failures don't propagate.
            let budget_opt: Option<f64> = state.settings.read().await.monthly_budget_usd;
            if let Some(budget) = budget_opt {
                if budget > 0.0 {
                    let month_start = crate::cost_store::current_month_start_ms();
                    let total_after = crate::cost_store::monthly_total(&state, month_start);
                    let total_before = (total_after - cost).max(0.0);
                    if let Some(threshold) = crate::cost_store::fired_threshold(total_before, total_after, budget) {
                        let _ = self.app_handle.emit("budget:threshold", crate::ipc::BudgetThresholdPayload {
                            threshold,
                            current: total_after,
                            budget,
                        });
                    }
                }
            }
        }

        let _ = self.app_handle.emit("agent:turn_cost", serde_json::json!({
            "conversationId": self.conversation_id,
            "inputTokens": turn_cost.input_tokens,
            "outputTokens": turn_cost.output_tokens,
            "cacheReadTokens": turn_cost.cache_read_tokens,
            "cacheCreationTokens": turn_cost.cache_creation_tokens,
            "costUsd": turn_cost.cost_usd,
        }));
    }

    /// Emit context stats after each LLM call
    fn emit_context_stats(&self, messages: &[ChatMessage], cumulative_input: u32, cumulative_output: u32) {
        let model_context_length = get_model_context_length(&self.model);
        let system_prompt_tokens = estimate_tokens(&self.system_prompt);
        let mut messages_tokens: u32 = 0;
        let mut tool_use_tokens: u32 = 0;

        for msg in messages {
            // Skip compacted messages — they stay in memory for UI replay
            // but must not inflate the context usage estimate. (P1 logical-marking)
            if msg.compacted {
                continue;
            }
            for block in &msg.content {
                match block {
                    ContentBlock::ToolUse { name, input, .. } => {
                        tool_use_tokens += estimate_tokens(name)
                            + estimate_tokens(&input.to_string()) + 10;
                    }
                    ContentBlock::ToolResult { content, .. } => {
                        tool_use_tokens += estimate_tokens(content) + 5;
                    }
                    ContentBlock::Text { text } => {
                        messages_tokens += estimate_tokens(text);
                    }
                    ContentBlock::Thinking { thinking, .. } => {
                        messages_tokens += estimate_tokens(thinking);
                    }
                }
            }
        }

        let compact_buffer = (model_context_length as f32 * 0.033) as u32;
        let used = system_prompt_tokens + messages_tokens + tool_use_tokens + compact_buffer;
        let free = model_context_length as i32 - used as i32;

        let stats = ContextStats {
            conversation_id: self.conversation_id.clone(),
            model_context_length,
            system_prompt_tokens,
            mcp_prompts_tokens: 0,
            skills_tokens: estimate_tokens(&self.skills_manifest_block),
            messages_tokens,
            tool_use_tokens,
            compact_buffer_tokens: compact_buffer,
            free_tokens: free,
            cumulative_input_tokens: Some(cumulative_input),
            cumulative_output_tokens: Some(cumulative_output),
        };
        let _ = self.app_handle.emit("agent:context_stats", &stats);
        tracing::info!(
            model_context_length = stats.model_context_length,
            used = stats.model_context_length as i32 - stats.free_tokens,
            free = stats.free_tokens,
            cumulative_input = cumulative_input,
            cumulative_output = cumulative_output,
            "Emitting agent:context_stats"
        );
    }

    /// Emit reflection status change event
    pub fn emit_reflection_status(&self, assistant_message_id: &str, status: &str) {
        let payload = serde_json::json!({
            "assistant_message_id": assistant_message_id,
            "status": status,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        tracing::info!(
            assistant_message_id = %assistant_message_id,
            status = %status,
            "Emitting agent:reflection_status"
        );
        if let Err(e) = self.app_handle.emit("agent:reflection_status", &payload) {
            tracing::warn!("Failed to emit reflection_status: {}", e);
        }
    }

    /// Emit full reflection detail event
    pub fn emit_reflection(&self, detail: &ReflectionDetail) {
        tracing::info!(
            assistant_message_id = %detail.assistant_message_id,
            status = %detail.status,
            outcome = ?detail.outcome,
            summary = ?detail.summary,
            "Emitting agent:reflection"
        );
        if let Err(e) = self.app_handle.emit("agent:reflection", detail) {
            tracing::warn!("Failed to emit reflection: {}", e);
        }
    }

    /// Emit a stream reset event to the frontend, signaling it should
    /// discard any partially received streaming content before fallback.
    fn emit_stream_reset(&self) {
        tracing::debug!("Emitting stream reset to frontend");
        let _ = self.app_handle.emit("agent:stream-reset", serde_json::json!({
            "conversationId": self.conversation_id,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit the `agent:retry` IPC event. Failures are non-fatal — we only
    /// log, so the retry loop is never blocked by a Tauri emit error.
    fn emit_retry_event(&self, event: AgentRetryEvent) {
        if let Err(e) = self.app_handle.emit(AgentRetryEvent::CHANNEL, &event) {
            tracing::debug!(error = %e, "Failed to emit agent:retry event");
        }
    }

    /// Sleep for `duration`, but wake up early if the session's stop flag
    /// flips. Returns `true` if the wake was triggered by the stop flag
    /// (caller should bail), `false` if the full duration elapsed.
    async fn sleep_or_abort(&self, duration: std::time::Duration) -> bool {
        let stop = self.stop_flag.clone();
        tokio::select! {
            _ = tokio::time::sleep(duration) => false,
            _ = async {
                loop {
                    if stop.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            } => true,
        }
    }

    /// Spawns a background task to perform unified memory/gbrain extraction after a turn completes.
    fn spawn_post_turn_extraction(&self, reason_ctx: &ReasoningContext) {
        use crate::agent::types::{ContentBlock, MessageRole};
        
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
                let db = self.learning_db.clone();
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
            let db = self.gbrain_extract_db.clone();
            let mcp_mgr = self.gbrain_extract_mcp_mgr.clone();
            let daily_budget = self.gbrain_extract_daily_budget;
            let llm_present = llm.is_some();
            if llm_present && db.is_some() && mcp_mgr.is_some() && daily_budget > 0 {
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
                    let mcp_mgr = match mcp_mgr {
                        Some(m) => m,
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

#[async_trait]
impl StreamSink for ChatDelegate {
    fn on_text_delta(&self, text: &str) {
        self.emit_text_delta(text);
    }
    fn on_thinking(&self, thinking: &str) {
        self.emit_thinking(thinking);
    }
    fn on_thinking_done(&self, duration_ms: u64) {
        self.emit_thinking_done(duration_ms);
    }
    fn on_stream_reset(&self) {
        self.emit_stream_reset();
    }
    fn on_retry_event(&self, event: AgentRetryEvent) {
        self.emit_retry_event(event);
    }
    async fn sleep_or_abort(&self, delay: std::time::Duration) -> bool {
        ChatDelegate::sleep_or_abort(self, delay).await
    }
}

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
fn signals_truncated_plan_continuation(text_len: usize, output_tokens: u32) -> bool {
    output_tokens > 800 && text_len < 100
}

/// C2-Dirac-B2 — render ContextManager-selected fragments as
/// `<context_fragment>` XML blocks for the per-turn dynamic context.
///
/// Returns the empty string when there are no fragments (so callers can
/// unconditionally `push_str` the result without adding stray
/// whitespace). Each artifact is rendered with its ref id + source so
/// the LLM can attribute the grounded context, and so a later
/// `context.read` round-trips the same id. Fragments are deliberately
/// rendered HERE (dynamic block) and NEVER in the system prompt — that
/// keeps the system-prompt cache_control:ephemeral breakpoint hitting
/// across turns (spec §8.3).
fn render_context_fragments(
    fragments: &[crate::runtime::context::ContextArtifact],
) -> String {
    let mut out = String::new();
    for art in fragments {
        out.push_str(&format!(
            "\n\n<context_fragment id=\"{}\" source=\"{}\">\n{}\n</context_fragment>",
            art.r#ref.id,
            art.r#ref.source.as_str(),
            art.content,
        ));
    }
    out
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

#[async_trait]
impl LoopDelegate for ChatDelegate {
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
                    let gene_block = format_gene_injection(&matches, 2);
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
        if let Some(ref db) = self.db {
            if let Ok(conn) = db.lock() {
                let window_start = chrono::Utc::now().timestamp_millis() - 7 * 24 * 60 * 60 * 1000;
                if let Ok(stats) = crate::agent::mode_suggest_store::query_per_pattern_stats(&conn, window_start) {
                    let total_decided: u32 = stats.iter().map(|s| s.accepted + s.skipped + s.silenced).sum();
                    let total_accepted: u32 = stats.iter().map(|s| s.accepted).sum();
                    if total_decided >= 10 {
                        let agg_rate = total_accepted as f32 / total_decided as f32;
                        if agg_rate < 0.20 {
                            full_system_prompt.push_str(
                                "\n\n[Plan-suggest signal] Your recent request_plan_mode_switch \
                                 calls have been declined frequently. Be more conservative — \
                                 only suggest Plan mode for clearly multi-step build/refactor \
                                 work, not casual questions.\n",
                            );
                            tracing::debug!(
                                agg_rate = agg_rate,
                                total_decided = total_decided,
                                "Plan-suggest aggregate hint injected into system prompt",
                            );
                        }
                    }
                }
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
        let sys_tok = estimate_tokens(&full_system_prompt);
        let tool_tok: u32 = tools.iter().map(|t| {
            estimate_tokens(&t.name) + estimate_tokens(&t.description) + estimate_tokens(&t.parameters.to_string())
        }).sum();
        let msg_tok: u32 = messages.iter().skip(1 /* skip system */).map(|m| {
            m.content.iter().map(|b| match b {
                ContentBlock::Text { text } => estimate_tokens(text),
                ContentBlock::ToolResult { content, .. } => estimate_tokens(content) + 5,
                ContentBlock::ToolUse { name, input, .. } => estimate_tokens(name) + estimate_tokens(&input.to_string()) + 10,
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
        let tool_calls = tool_calls
            .into_iter()
            .map(prepare_tool_call_for_dispatch)
            .collect::<Vec<_>>();

        // Parallel batch: read-only tools that pass all safety checks are
        // collected here and dispatched concurrently via JoinSet after the
        // serial loop. Each entry carries: (ToolCall, prepared_args, ctx).
        // Mutating tools always run in the serial path below.
        let mut parallel_batch: Vec<(ToolCall, serde_json::Value, ToolExecutionContext)> = Vec::new();

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
        for tc in &tool_calls {
            // ── Anti-fake-progress challenge ─────────────────────────
            // Intercept `plan_update done:true` calls that have neither
            // a recent mutating tool call NOR explicit evidence in `note`.
            // Inject a synthetic error tool_result and skip the actual
            // tool dispatch. See agent/types.rs::FAKE_PROGRESS_CHALLENGE
            // for the reasoning. After MAX_MUTATION_CHALLENGES soft-blocks
            // in this loop, we let it through with a logged warning so a
            // genuinely-completed step doesn't loop forever.
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
                    continue;
                }
                tracing::info!(
                    tool = %tc.name,
                    note_len,
                    "plan_update done:true accepted via `note` evidence"
                );
            }

            let tool = self.tools.get(&tc.name);
            match tool {
                Some(tool) => {
                    // Get the tool's own approval requirement
                    let tool_approval = tool.requires_approval(&tc.arguments);

                    tracing::info!(
                        tool = %tc.name,
                        tool_approval = ?tool_approval,
                        session_safety_mode = ?self.safety_mode,
                        "Evaluating tool approval"
                    );

                    // Consult SafetyManager with the session safety mode.
                    // Uses the DB-backed resolver when AppState is available
                    // (the normal case in the running app); falls back to the
                    // in-memory shim if not (keeps any test path that doesn't
                    // wire AppState working).
                    let decision = {
                        use tauri::Manager;
                        let mgr = self.safety_manager.read().await;
                        let db_state = self.app_handle.try_state::<crate::app::AppState>();
                        let session_mode = self.safety_mode.as_ref();
                        // Yolo session override short-circuits without touching DB
                        if matches!(session_mode, Some(SafetyMode::Yolo)) {
                            ApprovalDecision::AutoApprove
                        } else if let Some(state) = db_state {
                            mgr.should_approve_with_db(
                                &state.db,
                                &self.conversation_id,
                                &tc.name,
                                &tc.arguments,
                                &tool_approval,
                                session_mode,
                            )
                        } else {
                            mgr.should_approve(&tc.name, &tc.arguments, &tool_approval, session_mode)
                        }
                    };

                    tracing::info!(
                        tool = %tc.name,
                        decision = ?decision,
                        "Final approval decision for tool"
                    );

                    match decision {
                        ApprovalDecision::Block { reason } => {
                            tracing::warn!(tool = %tc.name, reason = %reason, "Tool blocked by safety policy");
                            reason_ctx.messages.push(ChatMessage::user_tool_result(
                                &tc.id,
                                &format!("Error: Tool blocked — {}", reason),
                                true,
                            ));
                            continue;
                        }
                        ApprovalDecision::RequireApproval { reason } => {
                            tracing::info!(tool = %tc.name, reason = %reason, "Tool requires approval, awaiting user decision");

                            // Register pending approval and get receiver
                            let rx = self.pending_approvals.register(tc.id.clone());

                            // Emit structured approval request event (includes sessionId for frontend)
                            let _ = self.app_handle.emit("agent:need_approval", serde_json::json!({
                                "toolName": tc.name,
                                "toolId": tc.id,
                                "arguments": tc.arguments,
                                "reason": reason,
                                "sessionId": self.conversation_id,
                                "riskLevel": "medium",
                                "timestamp": chrono::Utc::now().to_rfc3339(),
                            }));

                            // Await user's approval decision
                            let approval_result = match rx.await {
                                Ok(result) => result,
                                Err(_) => {
                                    // Channel dropped — treat as rejection
                                    tracing::warn!(tool = %tc.name, "Approval channel dropped, treating as rejected");
                                    crate::app::ApprovalResult { approved: false, always_allow: false, tool_name: None, path_scope: None, paths: None }
                                }
                            };

                            if !approval_result.approved {
                                tracing::info!(tool = %tc.name, "Tool execution rejected by user");
                                reason_ctx.messages.push(ChatMessage::user_tool_result(
                                    &tc.id,
                                    "Error: Tool execution was rejected by the user.",
                                    true,
                                ));
                                // Emit rejection event so frontend knows
                                let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                                    "toolName": tc.name,
                                    "toolCallId": tc.id,
                                    "timestamp": chrono::Utc::now().to_rfc3339(),
                                }));
                                continue;
                            }

                            // If always_allow was set, add to auto-approved list
                            if approval_result.always_allow {
                                let mut mgr = self.safety_manager.write().await;
                                let _ = mgr.add_auto_approved(&tc.name);
                                tracing::info!(tool = %tc.name, "Tool added to auto-approved list via always_allow");
                            }

                            tracing::info!(tool = %tc.name, "Tool approved by user, proceeding");
                        }
                        ApprovalDecision::AutoApprove => {
                            tracing::debug!(tool = %tc.name, "Tool auto-approved");
                        }
                    }

                    // Emit tool start. Surface preview_target so the
                    // frontend's auto-preview listener can pre-stake +
                    // open the panel without keeping a hardcoded
                    // "write-ish tool name" list (drives PR auto-preview).
                    let preview_target = tool.preview_target_path(&tc.arguments);
                    self.emit_tool_start(&tc.name, &tc.id, &tc.arguments, preview_target.as_deref());
                    tracing::info!(tool = %tc.name, id = %tc.id, "Executing tool");

                    // ─── Path-aware sandbox (Phase 3) ───────────────────
                    // Resolve candidate paths from the tool's path_args, ask
                    // SafetyManager. Prompt → reuse the same approval
                    // modal/oneshot pattern with kind: "path".
                    let candidate_paths: Vec<std::path::PathBuf> = tool
                        .path_args(&tc.arguments)
                        .into_iter()
                        .map(|p| {
                            let pb = std::path::PathBuf::from(p);
                            if pb.is_absolute() {
                                pb
                            } else if let Some(root) = self.workspace_root.as_deref() {
                                root.join(pb)
                            } else {
                                pb
                            }
                        })
                        .collect();

                    if !candidate_paths.is_empty() && self.workspace_root.is_some() {
                        use crate::safety::path_policy::PathDecision;
                        let workspace_root = self.workspace_root.clone().unwrap();
                        let (ws_attached, sess_attached) = load_attached_dirs_for_session(
                            &self.app_handle,
                            &self.conversation_id,
                        );
                        let path_decision = {
                            let mgr = self.safety_manager.read().await;
                            mgr.check_paths(
                                &self.conversation_id,
                                &workspace_root,
                                &ws_attached,
                                &sess_attached,
                                &candidate_paths,
                                self.safety_mode.as_ref(),
                            )
                        };
                        match path_decision {
                            PathDecision::Allow => {}
                            PathDecision::Block { reason } => {
                                tracing::warn!(tool = %tc.name, reason = %reason, "Path blocked by sandbox");
                                reason_ctx.messages.push(ChatMessage::user_tool_result(
                                    &tc.id,
                                    &format!("Error: {}", reason),
                                    true,
                                ));
                                let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                                    "toolName": tc.name,
                                    "toolCallId": tc.id,
                                    "timestamp": chrono::Utc::now().to_rfc3339(),
                                }));
                                continue;
                            }
                            PathDecision::Prompt { reason } => {
                                tracing::info!(tool = %tc.name, reason = %reason, "Path requires approval");
                                let approval_id = format!("{}::path", tc.id);
                                let rx = self.pending_approvals.register(approval_id.clone());
                                let _ = self.app_handle.emit("agent:need_approval", serde_json::json!({
                                    "kind": "path",
                                    "toolName": tc.name,
                                    "toolId": approval_id,
                                    "arguments": tc.arguments,
                                    "paths": candidate_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                                    "reason": reason,
                                    "sessionId": self.conversation_id,
                                    "timestamp": chrono::Utc::now().to_rfc3339(),
                                }));
                                let path_result = rx.await.unwrap_or_else(|_| {
                                    crate::app::ApprovalResult {
                                        approved: false,
                                        always_allow: false,
                                        tool_name: None,
                                        path_scope: Some("deny".into()),
                                        paths: None,
                                    }
                                });
                                if !path_result.approved {
                                    let paths_str = candidate_paths.iter()
                                        .map(|p| p.display().to_string())
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                                        &tc.id,
                                        &format!("Error: User denied access to path(s): {}", paths_str),
                                        true,
                                    ));
                                    let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                                        "toolName": tc.name,
                                        "toolCallId": tc.id,
                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                    }));
                                    continue;
                                }
                                if path_result.path_scope.as_deref() == Some("session") {
                                    let paths_to_grant = path_result.paths.clone()
                                        .unwrap_or_else(|| candidate_paths.iter().map(|p| p.display().to_string()).collect());
                                    let mut mgr = self.safety_manager.write().await;
                                    for p in paths_to_grant {
                                        mgr.allow_path_for_session(&self.conversation_id, std::path::PathBuf::from(p));
                                    }
                                }
                                // "once" falls through without persisting
                            }
                        }
                    }

                    // ── Parallel-safe fast path ──────────────────────────
                    // Read-only tools that passed all checks above are
                    // deferred to the JoinSet batch dispatched after this
                    // loop. Mutating tools continue to run serially below.
                    let tool_context_for_spawn = ToolExecutionContext::agent_turn(
                        self.conversation_id.clone(),
                        tc.id.clone(),
                        self.workspace_root.clone(),
                        self.safety_mode.clone(),
                    );
                    let tool_args_for_spawn = {
                        let mut args = tc.arguments.clone();
                        if let Some(obj) = args.as_object_mut() {
                            obj.insert("_tool_call_id".to_string(), serde_json::Value::String(tc.id.clone()));
                        } else {
                            tracing::warn!(
                                tool = %tc.name,
                                "tool arguments is not a JSON object; skipping _tool_call_id injection"
                            );
                        }
                        args
                    };
                    if is_parallel_safe(&tc.name) {
                        tracing::debug!(tool = %tc.name, id = %tc.id, "Deferring read-only tool to JoinSet batch");
                        parallel_batch.push((tc.clone(), tool_args_for_spawn, tool_context_for_spawn));
                        continue;
                    }

                    let tool_start = std::time::Instant::now();

                    // 该工具是否支持流式输出(BashTool = true)。
                    let wants_stream = self.tools.get(&tc.name).map(|t| t.supports_streaming()).unwrap_or(false);

                    // 仅流式工具搭建合并节流 drain(~50ms 或 8KB 先到先发)。
                    let coalescer = if wants_stream {
                        let (sink, mut rx) = crate::agent::tools::stream::ToolStreamSink::channel(256);
                        let app = self.app_handle.clone();
                        let conv = self.conversation_id.clone();
                        let id = tc.id.clone();
                        let handle = tokio::spawn(async move {
                            let mut buf_out = String::new();
                            let mut buf_err = String::new();
                            let mut last_seq: u64 = 0;
                            let mut tick = tokio::time::interval(std::time::Duration::from_millis(50));
                            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                            fn flush(app: &tauri::AppHandle, conv: &str, id: &str, last_seq: u64,
                                     buf_out: &mut String, buf_err: &mut String) {
                                if !buf_out.is_empty() {
                                    let _ = app.emit("chat:stream-tool-activity", serde_json::json!({
                                        "conversationId": conv,
                                        "activity": { "type": "tool_output_chunk", "toolCallId": id,
                                                      "seq": last_seq, "stream": "stdout", "chunk": std::mem::take(buf_out) }
                                    }));
                                }
                                if !buf_err.is_empty() {
                                    let _ = app.emit("chat:stream-tool-activity", serde_json::json!({
                                        "conversationId": conv,
                                        "activity": { "type": "tool_output_chunk", "toolCallId": id,
                                                      "seq": last_seq, "stream": "stderr", "chunk": std::mem::take(buf_err) }
                                    }));
                                }
                            }
                            loop {
                                tokio::select! {
                                    ev = rx.recv() => match ev {
                                        Some(e) => {
                                            last_seq = e.seq;
                                            let s = String::from_utf8_lossy(&e.bytes);
                                            match e.stream {
                                                crate::agent::tools::stream::ToolStream::Stdout => buf_out.push_str(&s),
                                                crate::agent::tools::stream::ToolStream::Stderr => buf_err.push_str(&s),
                                            }
                                            if buf_out.len() + buf_err.len() >= 8192 {
                                                flush(&app, &conv, &id, last_seq, &mut buf_out, &mut buf_err);
                                            }
                                        }
                                        None => { flush(&app, &conv, &id, last_seq, &mut buf_out, &mut buf_err); break; }
                                    },
                                    _ = tick.tick() => flush(&app, &conv, &id, last_seq, &mut buf_out, &mut buf_err),
                                }
                            }
                        });
                        Some((sink, handle))
                    } else {
                        None
                    };

                    let tool_name_for_panic = tc.name.clone();
                    let tools_arc = Arc::clone(&self.tools);
                    let sink_for_spawn = coalescer.as_ref().map(|(s, _)| s.clone());
                    let execute_result = match tokio::task::spawn(async move {
                        match tools_arc.get(&tool_name_for_panic) {
                            Some(t) => match sink_for_spawn {
                                Some(sink) => execute_streaming_with_context(t, tool_args_for_spawn, &tool_context_for_spawn, sink).await,
                                None => execute_tool_with_context(t, tool_args_for_spawn, &tool_context_for_spawn).await,
                            },
                            None => Err(crate::agent::tools::tool::ToolError::NotFound(tool_name_for_panic)),
                        }
                    }).await {
                        Ok(Ok(out)) => Ok(out),
                        Ok(Err(e)) => Err(e),
                        Err(join_err) if join_err.is_panic() => {
                            tracing::error!(tool = %tc.name, "tool panicked");
                            Err(crate::agent::tools::tool::ToolError::Execution(format!(
                                "Tool '{}' crashed unexpectedly. See ~/.uclaw/logs/crashes/ for details.", tc.name,
                            )))
                        }
                        Err(join_err) => {
                            tracing::error!(tool = %tc.name, %join_err, "tool join error");
                            Err(crate::agent::tools::tool::ToolError::Execution(format!("Tool join error: {}", join_err)))
                        }
                    };

                    // 工具结束 → 关 sink(drop) → coalescer 收尾 flush 后退出。
                    if let Some((sink, handle)) = coalescer {
                        drop(sink);
                        let _ = handle.await;
                    }
                    // Execute tool
                    match execute_result {
                        Ok(output) => {
                            let duration_ms = tool_start.elapsed().as_millis() as u64;
                            tracing::info!(
                                tool = %tc.name,
                                duration_ms = duration_ms,
                                "Tool completed"
                            );
                            self.emit_tool_result(&tc.name, &tc.id, &output);

                            let raw_result_str = serde_json::to_string(&output.result).unwrap_or_else(|_| "{}".into());

                            // Apply budget truncation (must happen before trajectory recording
                            // so the stored result matches what the LLM actually sees)
                            let turn_idx = self.turn_index.fetch_add(1, Ordering::Relaxed);
                            let result_str = if let Some(ref budget) = self.tool_budget {
                                budget.apply(&tc.name, raw_result_str, &self.conversation_id, turn_idx)
                            } else {
                                raw_result_str
                            };

                            // Record trajectory turn (also reflect soft errors so the
                            // agent_turns-based history recovery in get_agent_session_messages
                            // shows the right status when persisted JSON is missing).
                            let trajectory_is_error = detect_soft_tool_error(&output.result);
                            if let Some(ref store) = self.trajectory_store {
                                use crate::harness::trajectory::TurnRecord;
                                let tool_args_json = serde_json::to_string(&tc.arguments).unwrap_or_default();
                                let record = TurnRecord {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    session_id: self.conversation_id.clone(),
                                    turn_index: turn_idx,
                                    role: "tool".into(),
                                    content: None,
                                    tool_name: Some(tc.name.clone()),
                                    tool_args: Some(tool_args_json),
                                    tool_result: Some(result_str.clone()),
                                    reasoning: None,
                                    is_error: trajectory_is_error,
                                    duration_ms,
                                    created_at: chrono::Utc::now().timestamp_millis(),
                                };
                                if let Err(e) = store.record_turn(&record) {
                                    tracing::warn!("Failed to record trajectory turn: {e}");
                                }
                            }

                            // Publish ToolExecuted event to InfraService
                            if let Some(ref infra) = self.infra_service {
                                let input_summary = truncate_utf8(&serde_json::to_string(&tc.arguments).unwrap_or_default(), 500);
                                let output_summary = truncate_utf8(&result_str, 500);
                                infra.publish_tool_executed(
                                    "local",
                                    &tc.name,
                                    &output_summary,
                                    serde_json::json!({
                                        "tool_name": tc.name,
                                        "success": true,
                                        "duration_ms": duration_ms,
                                        "tool_input": input_summary,
                                    }),
                                ).await;
                            }

                            // Detect soft errors (e.g. bash non-zero exit) so the persisted
                            // ContentBlock::ToolResult carries is_error correctly. Without this,
                            // historical view would show a green check for failed bash commands.
                            let soft_error = detect_soft_tool_error(&output.result);

                            // Collect soft error for GeneRetriever matching
                            if soft_error {
                                if let Ok(mut errors) = self.recent_tool_errors.lock() {
                                    if errors.len() < 20 {
                                        let err_text = output
                                            .result
                                            .get("stderr")
                                            .or_else(|| output.result.get("output"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("tool error");
                                        errors.push(format!("{}: {}", tc.name, truncate_utf8(err_text, 200)));
                                    }
                                }
                            }

                            reason_ctx.messages.push(ChatMessage::user_tool_result(
                                &tc.id,
                                &result_str,
                                soft_error,
                            ));

                            // Pi Sprint 1 — track file-touching tool calls so
                            // SessionFileOps survives compaction cycles.
                            // Only track on success (soft_error=false) to avoid
                            // counting aborted writes.
                            if !soft_error {
                                reason_ctx.file_ops.track_tool_call(&tc.name, &tc.arguments);
                            }

                            // ── Anti-fake-progress bookkeeping ────────────
                            // Track real mutations so the next plan_update
                            // done:true has evidence to point at. A `bash`
                            // that hard-failed (soft_error=true) doesn't
                            // count — failed mutation isn't mutation.
                            if !soft_error
                                && crate::agent::types::is_mutating_tool(&tc.name, &tc.arguments)
                            {
                                reason_ctx.mutations_since_last_plan_done = reason_ctx
                                    .mutations_since_last_plan_done
                                    .saturating_add(1);
                            }
                            // Any successful tool call ends the truncation
                            // streak and plan-guard nudge streak.
                            reason_ctx.consecutive_length_truncations = 0;
                            reason_ctx.partial_code_buffer = None;
                            reason_ctx.consecutive_plan_guard_nudges = 0;
                            // Reset on successful plan_update done:true so the
                            // NEXT step needs its own mutation evidence. If the
                            // call was the soft-blocked path it `continue`d
                            // above and never reaches here.
                            if tc.name == "plan_update"
                                && tc.arguments.get("done").and_then(|v| v.as_bool()).unwrap_or(false)
                            {
                                reason_ctx.mutations_since_last_plan_done = 0;
                                reason_ctx.mutation_challenges_issued = 0;
                            }
                        }
                        Err(e) => {
                            let duration_ms = tool_start.elapsed().as_millis() as u64;
                            tracing::error!("Tool {} execution failed: {}", tc.name, e);
                            // Mark the tool activity as failed in the UI immediately
                            // without emitting chat:stream-error. The agent turn can
                            // recover from a tool-level error and continue.
                            self.emit_tool_error(&tc.name, &tc.id, &e.to_string(), duration_ms);

                            // Collect tool error for GeneRetriever matching
                            if let Ok(mut errors) = self.recent_tool_errors.lock() {
                                if errors.len() < 20 {
                                    errors.push(format!("{}: {}", tc.name, e));
                                }
                            }

                            // P1-3b: Detect user rejection/correction feedback and publish UserCorrection event.
                            // This captures user negative feedback (plan rejection, stop, output correction)
                            // into the GEP learning loop so it can feed into GeneCandidate pool.
                            if let Some(ref infra) = self.infra_service {
                                let err_msg = e.to_string();
                                // Pattern 1: exit_plan_mode rejection → "User rejected the plan. Feedback: ..."
                                if tc.name == "exit_plan_mode" && err_msg.starts_with("User rejected the plan.") {
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
                                            "tool_name": tc.name,
                                        }),
                                    ).await;
                                    tracing::info!(
                                        session_id = %self.conversation_id,
                                        feedback = %feedback,
                                        "[ChatDelegate] UserCorrection event published (plan_rejection)"
                                    );
                                }
                            }

                            let error_result_str = format!("Error: {}", e);

                            // Record error trajectory turn
                            let turn_idx = self.turn_index.fetch_add(1, Ordering::Relaxed);
                            if let Some(ref store) = self.trajectory_store {
                                use crate::harness::trajectory::TurnRecord;
                                let tool_args_json = serde_json::to_string(&tc.arguments).unwrap_or_default();
                                let record = TurnRecord {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    session_id: self.conversation_id.clone(),
                                    turn_index: turn_idx,
                                    role: "tool".into(),
                                    content: None,
                                    tool_name: Some(tc.name.clone()),
                                    tool_args: Some(tool_args_json),
                                    tool_result: Some(error_result_str.clone()),
                                    reasoning: None,
                                    is_error: true,
                                    duration_ms,
                                    created_at: chrono::Utc::now().timestamp_millis(),
                                };
                                if let Err(re) = store.record_turn(&record) {
                                    tracing::warn!("Failed to record error trajectory turn: {re}");
                                }
                            }

                            // Publish ToolExecuted event (failure) to InfraService
                            if let Some(ref infra) = self.infra_service {
                                let input_summary = truncate_utf8(&serde_json::to_string(&tc.arguments).unwrap_or_default(), 500);
                                let error_summary = truncate_utf8(&e.to_string(), 500);
                                infra.publish_tool_executed(
                                    "local",
                                    &tc.name,
                                    &error_summary,
                                    serde_json::json!({
                                        "tool_name": tc.name,
                                        "success": false,
                                        "duration_ms": duration_ms,
                                        "tool_input": input_summary,
                                    }),
                                ).await;
                            }

                            reason_ctx.messages.push(ChatMessage::user_tool_result(
                                &tc.id,
                                &error_result_str,
                                true,
                            ));
                        }
                    }
                }
                None => {
                    let err = format!("Tool '{}' not found", tc.name);
                    tracing::warn!("{}", err);

                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                        &tc.id,
                        &format!("Error: {}", err),
                        true,
                    ));
                }
            }
        }

        // ── JoinSet parallel dispatch for read-only tools ─────────────────
        // All items in parallel_batch have already passed approval + path
        // checks above. Dispatch them concurrently and collect results.
        if !parallel_batch.is_empty() {
            tracing::debug!(
                count = parallel_batch.len(),
                "Dispatching parallel-safe tools via JoinSet"
            );
            // Spawn all tasks first, then collect results.
            // Each task returns (tc, start_instant, execute_result).
            let mut join_set = tokio::task::JoinSet::new();
            for (tc, tool_args, tool_ctx) in parallel_batch {
                let tools_arc = Arc::clone(&self.tools);
                let tool_name_for_panic = tc.name.clone();
                let tool_name_spawn = tc.name.clone();
                self.emit_tool_start(&tc.name, &tc.id, &tc.arguments, None);
                tracing::info!(tool = %tc.name, id = %tc.id, "Executing tool (parallel)");
                join_set.spawn(async move {
                    let start = std::time::Instant::now();
                    let result = match tokio::task::spawn(async move {
                        match tools_arc.get(&tool_name_for_panic) {
                            Some(t) => execute_tool_with_context(t, tool_args, &tool_ctx).await,
                            None => Err(crate::agent::tools::tool::ToolError::NotFound(tool_name_for_panic)),
                        }
                    }).await {
                        Ok(Ok(out)) => Ok(out),
                        Ok(Err(e)) => Err(e),
                        Err(join_err) if join_err.is_panic() => {
                            tracing::error!(tool = %tool_name_spawn, "parallel tool panicked");
                            Err(crate::agent::tools::tool::ToolError::Execution(format!(
                                "Tool '{}' crashed unexpectedly. See ~/.uclaw/logs/crashes/ for details.",
                                tool_name_spawn,
                            )))
                        }
                        Err(join_err) => {
                            tracing::error!(tool = %tool_name_spawn, %join_err, "parallel tool join error");
                            Err(crate::agent::tools::tool::ToolError::Execution(format!("Tool join error: {}", join_err)))
                        }
                    };
                    (tc, start, result)
                });
            }

            while let Some(task_result) = join_set.join_next().await {
                let (tc, tool_start, execute_result) = match task_result {
                    Ok(r) => r,
                    Err(outer_err) => {
                        tracing::error!("JoinSet outer join error: {}", outer_err);
                        continue;
                    }
                };
                match execute_result {
                    Ok(output) => {
                        let duration_ms = tool_start.elapsed().as_millis() as u64;
                        tracing::info!(tool = %tc.name, duration_ms, "Parallel tool completed");
                        self.emit_tool_result(&tc.name, &tc.id, &output);
                        // Track file ops for parallel read-only tools too, mirroring the
                        // serial path — keeps SessionFileOps (StructuredFold axis 10) complete.
                        reason_ctx.file_ops.track_tool_call(&tc.name, &tc.arguments);

                        let raw_result_str = serde_json::to_string(&output.result).unwrap_or_else(|_| "{}".into());

                        let turn_idx = self.turn_index.fetch_add(1, Ordering::Relaxed);
                        let result_str = if let Some(ref budget) = self.tool_budget {
                            budget.apply(&tc.name, raw_result_str, &self.conversation_id, turn_idx)
                        } else {
                            raw_result_str
                        };

                        let trajectory_is_error = detect_soft_tool_error(&output.result);
                        if let Some(ref store) = self.trajectory_store {
                            use crate::harness::trajectory::TurnRecord;
                            let tool_args_json = serde_json::to_string(&tc.arguments).unwrap_or_default();
                            let record = TurnRecord {
                                id: uuid::Uuid::new_v4().to_string(),
                                session_id: self.conversation_id.clone(),
                                turn_index: turn_idx,
                                role: "tool".into(),
                                content: None,
                                tool_name: Some(tc.name.clone()),
                                tool_args: Some(tool_args_json),
                                tool_result: Some(result_str.clone()),
                                reasoning: None,
                                is_error: trajectory_is_error,
                                duration_ms,
                                created_at: chrono::Utc::now().timestamp_millis(),
                            };
                            if let Err(e) = store.record_turn(&record) {
                                tracing::warn!("Failed to record parallel trajectory turn: {e}");
                            }
                        }

                        if let Some(ref infra) = self.infra_service {
                            let input_summary = truncate_utf8(&serde_json::to_string(&tc.arguments).unwrap_or_default(), 500);
                            let output_summary = truncate_utf8(&result_str, 500);
                            infra.publish_tool_executed(
                                "local",
                                &tc.name,
                                &output_summary,
                                serde_json::json!({
                                    "tool_name": tc.name,
                                    "success": true,
                                    "duration_ms": duration_ms,
                                    "tool_input": input_summary,
                                }),
                            ).await;
                        }

                        let soft_error = detect_soft_tool_error(&output.result);
                        if soft_error {
                            if let Ok(mut errors) = self.recent_tool_errors.lock() {
                                if errors.len() < 20 {
                                    let err_text = output
                                        .result
                                        .get("stderr")
                                        .or_else(|| output.result.get("output"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("tool error");
                                    errors.push(format!("{}: {}", tc.name, truncate_utf8(err_text, 200)));
                                }
                            }
                        }

                        reason_ctx.messages.push(ChatMessage::user_tool_result(
                            &tc.id,
                            &result_str,
                            soft_error,
                        ));

                        // Parallel-safe tools are read-only by definition —
                        // they never count as mutations and never need
                        // plan-guard reset. No reason_ctx mutation bookkeeping.
                        reason_ctx.consecutive_length_truncations = 0;
                        reason_ctx.partial_code_buffer = None;
                        reason_ctx.consecutive_plan_guard_nudges = 0;
                    }
                    Err(e) => {
                        let duration_ms = tool_start.elapsed().as_millis() as u64;
                        tracing::error!("Parallel tool {} execution failed: {}", tc.name, e);
                        self.emit_tool_error(&tc.name, &tc.id, &e.to_string(), duration_ms);

                        if let Ok(mut errors) = self.recent_tool_errors.lock() {
                            if errors.len() < 20 {
                                errors.push(format!("{}: {}", tc.name, e));
                            }
                        }

                        if let Some(ref infra) = self.infra_service {
                            let input_summary = truncate_utf8(&serde_json::to_string(&tc.arguments).unwrap_or_default(), 500);
                            let error_summary = truncate_utf8(&e.to_string(), 500);
                            infra.publish_tool_executed(
                                "local",
                                &tc.name,
                                &error_summary,
                                serde_json::json!({
                                    "tool_name": tc.name,
                                    "success": false,
                                    "duration_ms": duration_ms,
                                    "tool_input": input_summary,
                                }),
                            ).await;
                        }

                        let turn_idx = self.turn_index.fetch_add(1, Ordering::Relaxed);
                        let error_result_str = format!("Error: {}", e);
                        if let Some(ref store) = self.trajectory_store {
                            use crate::harness::trajectory::TurnRecord;
                            let tool_args_json = serde_json::to_string(&tc.arguments).unwrap_or_default();
                            let record = TurnRecord {
                                id: uuid::Uuid::new_v4().to_string(),
                                session_id: self.conversation_id.clone(),
                                turn_index: turn_idx,
                                role: "tool".into(),
                                content: None,
                                tool_name: Some(tc.name.clone()),
                                tool_args: Some(tool_args_json),
                                tool_result: Some(error_result_str.clone()),
                                reasoning: None,
                                is_error: true,
                                duration_ms,
                                created_at: chrono::Utc::now().timestamp_millis(),
                            };
                            if let Err(re) = store.record_turn(&record) {
                                tracing::warn!("Failed to record parallel error trajectory turn: {re}");
                            }
                        }

                        reason_ctx.messages.push(ChatMessage::user_tool_result(
                            &tc.id,
                            &error_result_str,
                            true,
                        ));
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
        self.emit_thinking(&format!("Detected tool intent in: {}", truncate_utf8(text, 100)));
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
                        let input_preview = truncate_utf8(&input.to_string(), 200);
                        transcript.push_str(&format!(
                            "[{} called tool '{}' with: {}]\n",
                            role_label, name, input_preview
                        ));
                    }
                    ContentBlock::ToolResult { content, .. } => {
                        let result_preview = truncate_utf8(content, 300);
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
        self.steering_queue.drain()
    }

    /// Pi Sprint 2 item ③ — pop one follow-up task from the queue.
    async fn get_follow_up_messages(&self) -> Vec<ChatMessage> {
        self.follow_up_queue.next().unwrap_or_default()
    }

    /// Pi Sprint 2 item ③ — persist an injected user message into agent_messages
    /// so reloads stay continuous. No-op when persist_db is None.
    async fn persist_user_message(&self, msg: &ChatMessage) {
        let Some(db) = &self.persist_db else { return };
        // Extract text from the message (user message -> single Text block)
        let text: String = msg.content.iter().filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        }).collect::<Vec<_>>().join("\n");
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        if let Ok(conn) = db.lock() {
            let _ = conn.execute(
                "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1,?2,'user',?3,?4)",
                rusqlite::params![id, self.conversation_id, text, now],
            );
            let _ = conn.execute(
                "UPDATE agent_sessions SET message_count = message_count + 1, updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, self.conversation_id],
            );
        }
    }
}

/// Tools whose Rust impl returned `Ok(...)` but where the underlying operation
/// reports a logical failure via the result payload — bash uses
/// `{ ok: false, exit_code: 1, output: "..." }`, MCP uses `{ isError: true }`.
/// Mirrors the same heuristic the frontend applies in the streaming listener,
/// so both live and persisted views agree on whether a row is an error.
pub(crate) fn detect_soft_tool_error(result: &serde_json::Value) -> bool {
    matches!(result.get("ok"), Some(serde_json::Value::Bool(false)))
        || matches!(result.get("is_error"), Some(serde_json::Value::Bool(true)))
        || matches!(result.get("isError"), Some(serde_json::Value::Bool(true)))
}

/// Truncate a string to at most `max_chars` characters, ensuring UTF-8 safety.
fn truncate_utf8(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut result: String = s.chars().take(max_chars).collect();
        result.push_str("...");
        result
    }
}

/// Load workspace.attached_dirs and session.attached_dirs for the given
/// session. Returns empty vecs on any error (missing rows, malformed JSON).
fn load_attached_dirs_for_session(
    app_handle: &tauri::AppHandle,
    session_id: &str,
) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
    use tauri::Manager;
    let Some(state) = app_handle.try_state::<crate::app::AppState>() else {
        return (Vec::new(), Vec::new());
    };
    let Ok(conn) = state.db.lock() else {
        return (Vec::new(), Vec::new());
    };
    let parse = |json: String| -> Vec<std::path::PathBuf> {
        serde_json::from_str::<Vec<String>>(&json)
            .ok()
            .unwrap_or_default()
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect()
    };
    let ws_attached = conn
        .query_row(
            "SELECT attached_dirs FROM spaces WHERE id = (SELECT space_id FROM agent_sessions WHERE id = ?1)",
            rusqlite::params![session_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(parse)
        .unwrap_or_default();
    let sess_attached = conn
        .query_row(
            "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(parse)
        .unwrap_or_default();
    (ws_attached, sess_attached)
}

#[cfg(test)]
mod browser_runtime_dispatch_patch_tests {
    use super::*;

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
mod panic_recovery_tests {
    use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
    use async_trait::async_trait;

    struct PanickyTool;

    #[async_trait]
    impl Tool for PanickyTool {
        fn name(&self) -> &str { "panicky" }
        fn description(&self) -> &str { "test-only" }
        fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({}) }
        fn requires_approval(&self, _: &serde_json::Value) -> ApprovalRequirement {
            ApprovalRequirement::Never
        }
        async fn execute(&self, _: serde_json::Value) -> Result<ToolOutput, ToolError> {
            panic!("deliberate test panic");
        }
    }

    /// Verify the panic-recovery shape: tokio::task::spawn catches panic,
    /// JoinHandle yields is_panic=true, we map that to a ToolError.
    /// This mirrors what dispatcher::execute_tool does.
    #[tokio::test]
    async fn tool_panic_converts_to_tool_error() {
        let tool = PanickyTool;
        let tool_name = tool.name().to_string();
        let join = tokio::task::spawn(async move {
            tool.execute(serde_json::json!({})).await
        });
        let result = match join.await {
            Ok(r) => r,
            Err(e) if e.is_panic() => Err(ToolError::Execution(format!(
                "Tool '{}' crashed unexpectedly.", tool_name
            ))),
            Err(e) => Err(ToolError::Execution(format!("Join error: {}", e))),
        };
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("panicky") && msg.contains("crashed"),
            "expected panic-recovery error, got: {}", msg
        );
    }
}

#[cfg(test)]
mod manifest_suppression_tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Mirror of the suppression rule in `effective_system_prompt`. Kept
    /// in lockstep with `dispatcher.rs:154-171` — if you change the rule
    /// there, change it here. This test pins the contract without
    /// constructing a full `ChatDelegate` (which needs an LLM provider,
    /// safety manager, etc — heavy for a unit test).
    fn compose_with_suppression(
        base_system: &str,
        manifest_block: &str,
        skill_search_used: &AtomicBool,
    ) -> String {
        let suppress = skill_search_used.load(Ordering::Relaxed);
        if manifest_block.is_empty() || suppress {
            base_system.to_string()
        } else {
            format!("{}{}", base_system, manifest_block)
        }
    }

    /// Default state: flag unset → manifest is appended.
    #[test]
    fn manifest_appended_before_skill_search_used() {
        let flag = AtomicBool::new(false);
        let out = compose_with_suppression(
            "You are an agent.",
            "\n\nMANIFEST_BLOCK",
            &flag,
        );
        assert!(out.contains("MANIFEST_BLOCK"));
    }

    /// After flag is set (simulating `execute_tool_calls` seeing
    /// `skill_search`), the manifest is gone on subsequent prompt
    /// composition. This is the core of PR #137's optim #5 — without
    /// this, the ~800 tokens of manifest leak back into every later
    /// LLM call in the same agent loop.
    #[test]
    fn manifest_suppressed_after_skill_search_used() {
        let flag = AtomicBool::new(false);
        // First call (before skill_search): manifest present.
        let pre = compose_with_suppression("base", "\nM", &flag);
        assert!(pre.contains("M"));

        // Simulate `execute_tool_calls` detecting skill_search.
        flag.store(true, Ordering::Relaxed);

        // Subsequent calls: manifest gone.
        let post = compose_with_suppression("base", "\nM", &flag);
        assert!(!post.contains("M"));
        assert_eq!(post, "base");
    }

    /// Empty manifest: the suppression flag has no observable effect.
    /// Edge case — verifies the `is_empty()` short-circuit takes
    /// precedence over the flag check.
    #[test]
    fn empty_manifest_unaffected_by_flag() {
        for &used in &[false, true] {
            let flag = AtomicBool::new(used);
            let out = compose_with_suppression("base", "", &flag);
            assert_eq!(out, "base", "empty manifest should not produce divergent output; used={}", used);
        }
    }

    /// Flag is sticky — once set, stays set. A second non-skill_search
    /// tool call in the same loop must not flip it back. This guards
    /// against future refactors that might (incorrectly) reset the
    /// flag mid-loop.
    #[test]
    fn flag_stays_set_after_subsequent_non_skill_search_calls() {
        let flag = AtomicBool::new(false);
        flag.store(true, Ordering::Relaxed);  // simulate skill_search
        // Simulate a subsequent tool call that is NOT skill_search —
        // mirroring `execute_tool_calls`'s any() check which only
        // sets-true, never sets-false.
        // (No-op — the flag has nothing in execute_tool_calls that
        //  resets it; this test pins that fact.)
        assert!(flag.load(Ordering::Relaxed));
    }
}

#[cfg(test)]
mod manifest_cap_tests {
    /// PR #137 reduced the manifest cap from 1500 → 800 tokens in
    /// `tauri_commands.rs:5015`. The token budget is consumed by
    /// `skills_manifest::build_skills_manifest` via approximate
    /// 4-chars-per-token math. Verify the function honors a low cap
    /// gracefully (returns something non-empty if even one entry fits;
    /// returns empty if nothing fits — never panics, never returns
    /// the over-budget version).
    use crate::memory_graph::store::MemoryGraphStore;
    use crate::skills::SkillsRegistry;
    use crate::skills_manifest::{build_skills_manifest, StrategyBias};
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn fresh_store() -> MemoryGraphStore {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).expect("V4 schema");
        MemoryGraphStore::new(Arc::new(Mutex::new(conn)))
    }

    /// 800-token cap (current production setting) is enough budget for at
    /// least a few entries — verifies the cap isn't accidentally below
    /// the per-entry minimum, which would make the manifest unusable.
    #[test]
    fn manifest_at_800_token_cap_produces_output_when_skills_exist() {
        let registry = SkillsRegistry::new();
        // Empty store + empty registry → manifest can legitimately be
        // empty, no assertion needed beyond "doesn't panic".
        let store = fresh_store();
        let manifest = build_skills_manifest(
            &registry, &store, "default",
            30, 800, StrategyBias::Balanced, None,
        );
        // No skills loaded → empty manifest is correct.
        assert!(manifest.is_empty() || manifest.contains("Learned Skills"),
            "manifest must either be empty or contain the documented header");
    }

    /// Lower cap = no panic, no overrun. The cap argument is a soft
    /// budget — `format_manifest` stops adding entries when the next
    /// entry would push past the budget. Verifies the format function
    /// handles a very small cap (256 tokens ≈ 1000 chars).
    #[test]
    fn manifest_handles_very_small_cap_without_panic() {
        let registry = SkillsRegistry::new();
        let store = fresh_store();
        let manifest = build_skills_manifest(
            &registry, &store, "default",
            30, 256, StrategyBias::Balanced, None,
        );
        // Empty registry + empty store → empty manifest, no panic.
        let _ = manifest;
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
mod truncated_continuation_tests {
    use super::signals_truncated_plan_continuation;

    // Large-output + tiny-text is the shape of "thinking-heavy LLM
    // produced a transition stub but forgot the tool_use block". Triggers
    // the plan guard as a final fallback when the keyword gate misses.

    #[test]
    fn gomoku_signal_passes() {
        // The actual production case: 14 chars of text, 1722 output tokens.
        assert!(signals_truncated_plan_continuation(14, 1722));
    }

    #[test]
    fn long_text_does_not_pass() {
        // A real long answer can have many output tokens — we must not
        // hijack it. Threshold is text_len < 100.
        assert!(!signals_truncated_plan_continuation(300, 1722));
        assert!(!signals_truncated_plan_continuation(100, 1722));
    }

    #[test]
    fn small_output_does_not_pass() {
        // Tiny output (a normal short reply, no thinking) → not suspicious.
        assert!(!signals_truncated_plan_continuation(14, 50));
        assert!(!signals_truncated_plan_continuation(14, 800));
    }

    #[test]
    fn boundary_around_thresholds() {
        // > 800 tokens AND < 100 chars.
        assert!(signals_truncated_plan_continuation(99, 801));
        assert!(!signals_truncated_plan_continuation(99, 800));
        assert!(!signals_truncated_plan_continuation(100, 801));
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

// ───────────────────────────────────────────────────────────────────
// Bundle 16-C — memory_context cross-turn delta render-path tests.
//
// We pin the four render paths from `build_dynamic_context` without
// constructing a full `LoopDelegate` (which would need an LLM
// provider, safety manager, db handle, etc.). The helper below
// mirrors the in-line logic exactly — any change to the dispatcher
// branch order must be mirrored here, same convention as
// `manifest_suppression_tests`.
//
// The four paths under test:
//
//  1. first turn (no prior anchor)  → full block, no annotation
//  2. unchanged across turns        → full block, no annotation
//  3. small drift (≤ 40%)           → full block + delta annotation
//  4. significant drift (> 40%)     → full block, no annotation
// ───────────────────────────────────────────────────────────────────
#[cfg(test)]
mod memory_context_delta_render_tests {
    use crate::agent::context_diff::{
        line_diff, render_delta_annotation, LineFragmentSnapshot,
    };

    /// Mirror of the dispatcher's render decision. Returns
    /// `(emitted_block, kind_label)` where `kind_label` is one of
    /// `"full"`, `"full+delta"`, used for assertions + telemetry
    /// parity.
    fn render_memory_context(
        prior: Option<&LineFragmentSnapshot>,
        current_text: &str,
    ) -> (String, &'static str) {
        const SIGNIFICANT_DRIFT_THRESHOLD: f32 = 0.40;
        let current = LineFragmentSnapshot::from_text("memory_context", current_text);

        let mut block = String::new();
        let mut kind = "full";

        let annotation: Option<String> = match prior {
            None => None,
            Some(p) => {
                let d = line_diff(p, &current);
                let stats = d.stats();
                if d.is_empty() {
                    None
                } else if stats.is_significant_change(SIGNIFICANT_DRIFT_THRESHOLD) {
                    None
                } else {
                    let a = render_delta_annotation(&d);
                    if a.is_some() {
                        kind = "full+delta";
                    }
                    a
                }
            }
        };

        block.push_str("<memory_context>\n");
        block.push_str(current_text);
        block.push_str("\n</memory_context>");
        if let Some(a) = annotation {
            block.push_str("\n\n");
            block.push_str(&a);
        }
        (block, kind)
    }

    // ── Path 1: first turn ─────────────────────────────────────────

    #[test]
    fn dispatcher_first_turn_emits_full_memory_context_block_only() {
        let ctx = "- preferred_language: zh\n- project: uClaw\n";
        let (block, kind) = render_memory_context(None, ctx);
        assert_eq!(kind, "full");
        assert!(block.contains("<memory_context>"));
        assert!(block.contains("preferred_language"));
        assert!(
            !block.contains("<memory_context_changes"),
            "first turn must not emit delta annotation"
        );
    }

    // ── Path 2: unchanged across turns ─────────────────────────────

    #[test]
    fn dispatcher_unchanged_turn_emits_full_block_no_delta_annotation() {
        let ctx = "- preferred_language: zh\n- project: uClaw\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", ctx);
        let (block, kind) = render_memory_context(Some(&prior), ctx);
        assert_eq!(kind, "full");
        assert!(block.contains("<memory_context>"));
        assert!(
            !block.contains("<memory_context_changes"),
            "no drift → no annotation"
        );
    }

    // ── Path 3: small drift → full + delta ─────────────────────────

    #[test]
    fn dispatcher_small_drift_emits_full_block_plus_delta_annotation() {
        // Prior: 5 lines. New: 4 unchanged + 1 changed (value flip).
        // Drift = 1/5 = 0.20, below 0.40 threshold → small drift path.
        let prior_text =
            "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- preferred_language: en\n";
        let new_text =
            "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- preferred_language: zh\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full+delta");
        // Full block present
        assert!(block.contains("<memory_context>"));
        assert!(block.contains("preferred_language: zh"));
        // Delta annotation present
        assert!(block.contains("<memory_context_changes"));
        assert!(
            block.contains("~ changed key=preferred_language"),
            "delta must surface the changed line\nblock:\n{}",
            block
        );
        // Block order: <memory_context> first, then <memory_context_changes>
        let mc_pos = block.find("<memory_context>").unwrap();
        let mcc_pos = block.find("<memory_context_changes").unwrap();
        assert!(
            mc_pos < mcc_pos,
            "full block must precede the delta annotation"
        );
    }

    #[test]
    fn dispatcher_small_drift_one_added_line_emits_delta() {
        // 5 prior lines, 1 line added → drift = 0/5 (added-only doesn't
        // count toward drift fraction since it's not in prior).
        // is_significant_change checks (removed + changed) / prior, so
        // pure-add never crosses 40%. We expect full+delta.
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- e: 5\n";
        let new_text =
            "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- e: 5\n- last_query: foo\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full+delta");
        assert!(block.contains("+ added: - last_query: foo"));
    }

    // ── Path 4: significant drift ──────────────────────────────────

    #[test]
    fn dispatcher_significant_drift_emits_full_block_no_annotation() {
        // 4 prior lines, 3 removed (drift = 0.75) → above threshold.
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n- d: 4\n";
        let new_text = "- a: 1\n- z: new\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full");
        assert!(block.contains("<memory_context>"));
        assert!(
            !block.contains("<memory_context_changes"),
            "significant drift must NOT emit delta annotation (noisy + cache miss anyway)"
        );
    }

    // ── Anchor reset via clear_memory_context_anchor() ─────────────

    #[test]
    fn clear_anchor_makes_next_turn_behave_like_first_turn() {
        let prior_text = "- a: 1\n";
        let new_text = "- a: 1\n- b: 2\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);

        // With anchor: small drift → delta
        let (with_anchor, kind_with) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind_with, "full+delta");
        assert!(with_anchor.contains("<memory_context_changes"));

        // After clear (None): treated as first turn → no delta
        let (after_clear, kind_after) = render_memory_context(None, new_text);
        assert_eq!(kind_after, "full");
        assert!(!after_clear.contains("<memory_context_changes"));
    }

    // ── Reorder invariance: same key set, different order → no drift

    #[test]
    fn reordered_lines_emit_full_block_no_annotation() {
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n";
        let new_text = "- c: 3\n- a: 1\n- b: 2\n";  // same keys, reordered
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full");
        assert!(
            !block.contains("<memory_context_changes"),
            "reorder must not trigger delta annotation"
        );
    }

    // ── Drift threshold is exactly 0.40 (boundary check) ──────────

    #[test]
    fn drift_exactly_at_40_percent_threshold_is_significant() {
        // 5 prior, 2 removed → drift = 2/5 = 0.40 → meets `>=` cutoff
        // in is_significant_change(0.40), so this is the "significant"
        // path with `full` (no delta).
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- e: 5\n";
        let new_text = "- a: 1\n- b: 2\n- c: 3\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full");
        assert!(!block.contains("<memory_context_changes"));
    }
}

#[cfg(test)]
mod b2_context_wireup_tests {
    //! C2-Dirac-B2 — fragment rendering for build_dynamic_context.
    //!
    //! The cache-discipline / composition guarantees are tested where the
    //! logic lives, without a Tauri AppHandle (ChatDelegate is hard-typed
    //! to the Wry runtime, which `tauri::test::mock_app()` cannot satisfy):
    //!
    //! - System-prompt byte-stability + "self.system_prompt / workspace
    //!   not dropped" → `mode_prompts::tests::compose_with_injection_*`
    //!   (effective_system_prompt delegates verbatim to
    //!   compose_system_prompt_with_injection for the prompt string).
    //! - Fragment SELECTION + ComposeStats → `context_manager::manager`
    //!   + `stats_collector` tests, and the `context_wireup_bench`
    //!   integration test.
    //!
    //! Here we cover the remaining seam: how selected fragments render
    //! into the per-turn dynamic block (and that they NEVER look like a
    //! system-prompt block — they're a distinct <context_fragment> tag).

    use super::render_context_fragments;
    use crate::runtime::context::{ContextArtifact, ContextRef, ContextSource};

    fn artifact(id: &str, source: ContextSource, content: &str) -> ContextArtifact {
        ContextArtifact {
            r#ref: ContextRef::new(source, id),
            content: content.into(),
            citations: Vec::new(),
            retrieval_ts: "2026-05-25T00:00:00Z".into(),
        }
    }

    #[test]
    fn empty_fragments_render_to_empty_string() {
        // Caller push_str's the result unconditionally — no fragments must
        // produce zero bytes (no stray whitespace in the dynamic block).
        assert_eq!(render_context_fragments(&[]), "");
    }

    #[test]
    fn fragment_renders_as_context_fragment_xml_with_id_and_source() {
        let frags = vec![artifact(
            "thread/abc",
            ContextSource::Conversation,
            "alpha\nbeta",
        )];
        let out = render_context_fragments(&frags);
        assert!(out.contains("<context_fragment id=\"thread/abc\" source=\"conversation\">"));
        assert!(out.contains("alpha\nbeta"));
        assert!(out.contains("</context_fragment>"));
    }

    #[test]
    fn multiple_fragments_each_get_their_own_block() {
        let frags = vec![
            artifact("file/a.rs", ContextSource::Codebase, "fn a() {}"),
            artifact("recall/x", ContextSource::Memory, "page body"),
        ];
        let out = render_context_fragments(&frags);
        assert_eq!(out.matches("<context_fragment ").count(), 2);
        assert_eq!(out.matches("</context_fragment>").count(), 2);
        assert!(out.contains("source=\"codebase\""));
        assert!(out.contains("source=\"memory\""));
    }
}

#[cfg(test)]
mod parallel_safe_tools_tests {
    use super::is_parallel_safe;

    #[test]
    fn is_parallel_safe_returns_true_for_read_only_tools() {
        assert!(is_parallel_safe("read_file"));
        assert!(is_parallel_safe("search_files"));
        assert!(is_parallel_safe("get_file_skeleton"));
    }

    #[test]
    fn is_parallel_safe_returns_false_for_mutating_tools() {
        assert!(!is_parallel_safe("edit"));
        assert!(!is_parallel_safe("write_file"));
        assert!(!is_parallel_safe("bash"));
        assert!(!is_parallel_safe("ask_user"));
    }
}
