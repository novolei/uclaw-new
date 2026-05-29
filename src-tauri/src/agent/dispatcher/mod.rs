use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use crate::agent::tools::tool::ToolRegistry;
use crate::agent::gep::repository::GeneRepository;
use crate::agent::gep::retrieval::{GeneRetriever, GeneMatch};
use crate::agent::gep::types::{Capsule, CapsuleOutcome, OutcomeStatus, BlastRadius, EnvFingerprint, EvolutionEvent};
use crate::infra::InfraService;
use crate::llm::LlmProvider;
use crate::safety::SafetyMode;


mod observability;
mod content_assembler;
mod safety_gate;
mod model_io;
mod turn_runner;

/// Sprint 2.0+ chat-turn extractor configuration. Set as a single
/// bundle by `set_learning_pipeline`; read together by
/// `turn_runner::before_llm_call` at iteration=0.
#[derive(Default)]
pub(super) struct LearningPipeline {
    pub(super) buffer: Option<Arc<crate::learning::candidate::Buffer>>,
    pub(super) llm: Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>,
    pub(super) enabled: bool,
    pub(super) llm_daily_budget: u32,
}

/// Sprint 2.4b gbrain chat-extractor configuration. Set as a single
/// bundle by `set_gbrain_extractor_pipeline`; read by
/// `turn_runner::before_llm_call` alongside the learning pipeline.
#[derive(Default)]
pub(super) struct GbrainExtractorPipeline {
    pub(super) enabled: bool,
    pub(super) llm: Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>,
    pub(super) daily_budget: u32,
}

/// Gene-Expression-Programming retrieval + capsule generation. The
/// retriever runs on every `call_llm`; matches accumulate in
/// `last_matches` and are consumed by `generate_capsule_for_turn`.
#[derive(Default)]
pub(super) struct GepPipeline {
    pub(super) retriever: Option<Arc<GeneRetriever>>,
    pub(super) last_matches: std::sync::Mutex<Vec<GeneMatch>>,
    pub(super) repo: Option<Arc<std::sync::Mutex<GeneRepository>>>,
}

/// Per-session telemetry collectors. Each is wired via its own setter;
/// all are observability-only and may be `None` in headless / test contexts.
#[derive(Default)]
pub(super) struct Telemetry {
    pub(super) heartbeat: Option<Arc<crate::agent::heartbeat::HeartbeatSupervisor>>,
    pub(super) token_budget: Option<crate::agent::telemetry::TokenBudgetCollector>,
    pub(super) compose_stats: Option<crate::agent::context_manager::ComposeStatsCollector>,
}

/// Pre-built system-prompt fragments. Each is computed once per
/// agent loop start (skill manifest scan, learning profile fold,
/// gbrain knowledge instruction); `effective_system_prompt` appends
/// the non-empty ones on every iteration. Empty string = no append.
#[derive(Default)]
pub(super) struct PromptBlocks {
    pub(super) skills_manifest: String,
    pub(super) learned_profile: String,
    pub(super) gbrain_knowledge: String,
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
    /// Safety mode for this session (overrides global if set)
    safety_mode: Option<SafetyMode>,
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
    trajectory_store: Option<Arc<crate::agent::trajectory::TrajectoryStore>>,
    /// Optional tool budget manager for truncating large results
    tool_budget: Option<Arc<crate::agent::tool_budget::ToolBudgetManager>>,
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
    /// GEP retrieval + capsule-generation pipeline (retriever, last_matches, repo).
    gep: GepPipeline,
    /// Recent tool error messages for passing to GeneRetriever.match_genes.
    recent_tool_errors: Mutex<Vec<String>>,
    /// Pre-built system-prompt fragments bundled: skills manifest,
    /// user profile, gbrain knowledge. Set via individual setters
    /// (`set_skills_manifest_block`, `set_learned_profile_block`,
    /// `set_gbrain_knowledge_block`) which map to fields inside this struct.
    prompt_blocks: PromptBlocks,
    /// Memory OS Sprint 2.0 — producer-side handles for the learning
    /// pipeline. When `learning.enabled = true` AND both handles are
    /// `Some`, `before_llm_call` at iteration=0 spawns the chat-turn
    /// extractor over the user's latest message text. Buffer is shared
    /// with ProactiveService's LearningScheduler so candidates pushed
    /// here surface in the next 30-min stability rebuild.
    /// Sprint 2.1b — daily token budget for the LLM extractor is
    /// `learning.llm_daily_budget`. When today's spend exceeds this,
    /// the LLM layer is skipped (regex layer still runs). Default 0
    /// = effectively disabled.
    learning: LearningPipeline,
    /// Sprint 2.4b — gbrain chat-turn auto-extractor handles. When
    /// `gbrain_extractor.enabled = true` AND all handles are
    /// `Some` AND daily budget remaining > 0, `before_llm_call` at
    /// iteration=0 spawns `gbrain::chat_extractor::extract_from_chat_turn`
    /// on the user's latest message. Accepted proposals (confidence
    /// >= `MIN_ACT_CONFIDENCE`) fire `mcp__gbrain__put_page` via the
    /// shared McpManager. Failures logged + swallowed so a producer
    /// bug never poisons the LLM call.
    gbrain_extractor: GbrainExtractorPipeline,
    /// FNV-style hash of the last tool definition list sent to the LLM.
    /// When the list is identical across iterations within a single agent
    /// turn, the Anthropic cache should cover it — this tracks whether the
    /// list actually changed (e.g. after an MCP reconnect) so we can log it.
    last_tool_defs_hash: Mutex<Option<u64>>,
    /// Slice 1 — provider id ('anthropic' / 'openai' / 'deepseek' / etc.)
    /// stamped on every TokenBudgetSnapshot. Default 'unknown' is
    /// replaced by the caller via `set_provider`.
    provider: String,
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
    /// PR5 of Tier 1+2+3: pragmatic per-message reset. The original comment
    /// admitted the flag would never reset on Plan→Auto toggle (waiting on
    /// M2-A finalization). Until M2-A lands, reset at every new user message
    /// — close enough to "user toggled mode" without a full mode-transition
    /// state machine. Call `reset_first_act_turn()` at chat-mode entry.
    is_first_act_turn: AtomicBool,
    /// Last tool error kind, if any — feeds A4's
    /// `InjectionContext.last_error_kind`. Set by the tool-execution
    /// path; `std::sync::Mutex`, never held across awaits.
    last_error_kind: std::sync::Mutex<Option<String>>,
    /// Per-session telemetry collectors (heartbeat, token budget, compose stats).
    telemetry: Telemetry,
    /// Pi Sprint 2 item ③ — steering queue drained at the start of each turn (mid-run).
    steering_queue: crate::agent::queues::SteeringQueue,
    /// Pi Sprint 2 item ③ — follow-up queue drained one task at a time at natural
    /// stop points; each entry is a Vec<ChatMessage> task.
    follow_up_queue: crate::agent::queues::FollowUpQueue,
    /// Sprint 3 ① — the cutover ToolDispatcher. Built LAZILY (first
    /// `execute_tool_calls`) rather than in `new`, because
    /// `infra_service` / `trajectory_store` / `tool_budget` are injected
    /// AFTER `new` via setters (`set_infra_service`, etc.). Building eagerly
    /// in `new` would capture `None` for all three and silently drop
    /// trajectory/infra/budget behavior. By first-dispatch time the agent
    /// loop has run every setter, so the lazy build sees the configured
    /// fields. Always present at dispatch time (no fallback path).
    tool_dispatcher:
        std::sync::OnceLock<std::sync::Arc<crate::agent::tool_dispatch::ToolDispatcher<tauri::Wry>>>,
}

impl ChatDelegate {
    pub fn new(
        llm: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistry>,
        app_handle: tauri::AppHandle,
        model: String,
        system_prompt: String,
        safety_mode: Option<SafetyMode>,
        conversation_id: String,
        workspace_root: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            llm, tools, app_handle, model, system_prompt,
            stop_flag: Arc::new(AtomicBool::new(false)),
            safety_mode,
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
            skill_search_used: AtomicBool::new(false),
            gep: Default::default(),
            recent_tool_errors: Mutex::new(Vec::new()),
            prompt_blocks: Default::default(),
            learning: Default::default(),
            gbrain_extractor: Default::default(),
            last_tool_defs_hash: Mutex::new(None),
            provider: "unknown".to_string(),
            context_manager: Arc::new(tokio::sync::RwLock::new(
                crate::agent::context_manager::ContextManager::new(),
            )),
            last_injected_fragments: std::sync::Mutex::new(Vec::new()),
            is_first_act_turn: AtomicBool::new(true),
            last_error_kind: std::sync::Mutex::new(None),
            telemetry: Default::default(),
            steering_queue: Default::default(),
            follow_up_queue: Default::default(),
            tool_dispatcher: std::sync::OnceLock::new(),
        }
    }

    /// Look up the process-scope AppState through the Tauri AppHandle.
    ///
    /// This is the canonical replacement for the 8 `ChatDelegate` fields
    /// dropped in P3-5b1 (safety_manager, pending_approvals, hook_bus via
    /// agent_api, 4 DB clones, mcp_manager). Reads forward as
    /// `self.app_state().subsystem.clone()`.
    ///
    /// PANICS if AppState is not registered on this Tauri AppHandle. In
    /// production this is wired by `AppState::new()` at boot, so the
    /// invariant holds for every code path the agent loop reaches.
    pub(super) fn app_state(&self) -> tauri::State<'_, crate::app::AppState> {
        use tauri::Manager;
        self.app_handle.state::<crate::app::AppState>()
    }

    /// None-tolerant variant of `app_state()`. For paths that previously
    /// tolerated Option semantics on the dropped fields.
    pub(super) fn try_app_state(&self) -> Option<tauri::State<'_, crate::app::AppState>> {
        use tauri::Manager;
        self.app_handle.try_state::<crate::app::AppState>()
    }

    /// Pi Sprint 2 item ③ — wire the dual interactive queues (agent path only).
    /// `::new` signature is unchanged so chat-mode call sites are unaffected.
    /// Pass `AppState::agent_queues_for(session_id)`.
    pub fn with_agent_queues(mut self, queues: crate::app::AgentQueues) -> Self {
        self.steering_queue = queues.steering;
        self.follow_up_queue = queues.follow_up;
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
        self.gep.retriever = Some(retriever);
    }

    /// Set the GEP GeneRepository for Capsule persistence.
    pub fn set_gene_repo(&mut self, repo: Arc<Mutex<GeneRepository>>) {
        self.gep.repo = Some(repo);
    }

    /// Generate and persist Capsules for the most recent Gene matches.
    ///
    /// Computes blast radius from workspace git diff, constructs full Capsule
    /// and EvolutionEvent records, and persists them to GeneRepository.
    /// Also publishes CapsuleCreated events via InfraService.
    /// Called after tool execution completes for the turn.
    pub async fn generate_capsule_for_turn(&self) {
        let gene_matches: Vec<GeneMatch> = {
            match self.gep.last_matches.lock() {
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
            if let Some(ref repo_arc) = self.gep.repo {
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
        if let Some(ref retriever) = self.gep.retriever {
            if let Some(ref repo_arc) = self.gep.repo {
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
    pub fn set_trajectory_store(&mut self, store: Arc<crate::agent::trajectory::TrajectoryStore>) {
        self.trajectory_store = Some(store);
    }

    /// Set the ToolBudgetManager for truncating large tool results.
    pub fn set_tool_budget(&mut self, budget: Arc<crate::agent::tool_budget::ToolBudgetManager>) {
        self.tool_budget = Some(budget);
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
        enabled: bool,
        llm_daily_budget: u32,
    ) {
        self.learning = LearningPipeline {
            buffer: Some(buffer),
            llm,
            enabled,
            llm_daily_budget,
        };
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
        enabled: bool,
        daily_budget: u32,
    ) {
        self.gbrain_extractor = GbrainExtractorPipeline {
            enabled,
            llm,
            daily_budget,
        };
    }

    /// Returns a cloneable handle that can be used to signal the loop to stop.
    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }

    /// Reset `is_first_act_turn` to true. Pragmatic per-message reset pending
    /// full M2-A mode-transition tracking. Call this at chat-mode `send_message`
    /// entry to reset the flag before the agent loop uses it.
    pub fn reset_first_act_turn(&self) {
        self.is_first_act_turn.store(true, std::sync::atomic::Ordering::Relaxed);
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
pub(crate) fn truncate_utf8(s: &str, max_chars: usize) -> String {
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
pub(crate) fn load_attached_dirs_for_session<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
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
