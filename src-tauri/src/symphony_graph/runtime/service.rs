//! `SymphonyService: ManagedService` — the only thing wired into
//! `main.rs` Stage 3. Owns the run-actor map, the global concurrency
//! semaphore, the manual-trigger channel, and the reconcile-on-start sweep.
//!
//! ## Lifecycle
//!
//! - `register()` in main.rs adds the service to `ServiceManager`.
//! - `start()` runs `recovery::reconcile`, resumes in-flight runs as
//!   actors, then enters the trigger loop on a background task.
//! - `stop()` flips status to `Stopping`, cancels every actor's
//!   `CancellationToken`, awaits with a budget, returns.
//!
//! ## Authoritative state (Symphony SPEC §invariants)
//!
//! - `runs: RwLock<HashMap<RunId, Arc<RunActor>>>` — the in-memory map.
//! - SQLite mirrors every state transition (writes are synchronous from
//!   the actor, so the DB never lags by more than one tick).

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};
use std::time::Instant;

use async_trait::async_trait;
use rusqlite::{params, Connection};
use tokio::sync::{mpsc, Mutex as TokioMutex, RwLock, Semaphore};

use crate::automation::memory::MemoryStore;
use crate::infra::{ConversationMessage, InfraEvent, InfraEventType, InfraService};
use crate::memubot_config::SymphonyConfig;
use crate::providers::service::ProviderService;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};

use super::super::manager::SymphonyManager;
use super::node_run::NodeExecutionDeps;
use super::recovery;
use super::run_actor::RunActor;
use super::stall::Heartbeat;

/// Inbound command from the Tauri layer / scheduler.
pub enum TriggerCmd {
    /// `symphony_trigger_run` Tauri command lands here.
    Manual {
        run_id: String,
        workflow_id: String,
        workflow_version: i64,
        inputs_json: String,
        respond_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    /// `symphony_cancel_run` Tauri command lands here.
    Cancel { run_id: String },
}

pub struct SymphonyService {
    pub db: Arc<StdMutex<Connection>>,
    pub infra: Arc<InfraService>,
    #[allow(dead_code)]
    pub provider_service: Arc<ProviderService>,
    pub config: SymphonyConfig,
    pub app_handle: Option<tauri::AppHandle>,

    /// Authoritative in-memory map of in-flight runs.
    runs: Arc<RwLock<std::collections::HashMap<String, Arc<RunActor>>>>,

    /// Global concurrency limiter across all runs.
    global_run_sem: Arc<Semaphore>,

    /// Trigger channel sender (cloned into Tauri commands).
    trigger_tx: mpsc::UnboundedSender<TriggerCmd>,
    /// Receiver moved into the trigger loop on start.
    trigger_rx: TokioMutex<Option<mpsc::UnboundedReceiver<TriggerCmd>>>,

    /// Per-node heartbeat registry shared with `node_run`.
    heartbeat: Arc<Heartbeat>,

    /// Where per-node workspace dirs live (`~/.uclaw/symphony/`).
    workspace_root: PathBuf,

    /// In-process automation memory store, reused for per-workflow notes.
    memory: Arc<MemoryStore>,

    status: Arc<StdMutex<ServiceStatus>>,
    started_at: Arc<StdMutex<Option<Instant>>>,

    /// Weak self-ref so spawned tasks can call back into the service
    /// without an Arc cycle.
    self_weak: OnceLock<Weak<SymphonyService>>,
}

impl SymphonyService {
    pub fn new(
        db: Arc<StdMutex<Connection>>,
        infra: Arc<InfraService>,
        provider_service: Arc<ProviderService>,
        config: SymphonyConfig,
        app_handle: Option<tauri::AppHandle>,
        workspace_root: PathBuf,
        memory: Arc<MemoryStore>,
    ) -> Arc<Self> {
        let (trigger_tx, trigger_rx) = mpsc::unbounded_channel::<TriggerCmd>();
        let svc = Arc::new(Self {
            db,
            infra,
            provider_service,
            global_run_sem: Arc::new(Semaphore::new(config.max_concurrent_runs.max(1))),
            config,
            app_handle,
            runs: Arc::new(RwLock::new(std::collections::HashMap::new())),
            trigger_tx,
            trigger_rx: TokioMutex::new(Some(trigger_rx)),
            heartbeat: Arc::new(Heartbeat::new()),
            workspace_root,
            memory,
            status: Arc::new(StdMutex::new(ServiceStatus::Stopped)),
            started_at: Arc::new(StdMutex::new(None)),
            self_weak: OnceLock::new(),
        });
        let _ = svc.self_weak.set(Arc::downgrade(&svc));
        svc
    }

    /// Public handle for Tauri commands to enqueue manual triggers.
    pub fn trigger_sender(&self) -> mpsc::UnboundedSender<TriggerCmd> {
        self.trigger_tx.clone()
    }

    /// Resume a recovered run as a fresh `RunActor`. Differs from the manual
    /// trigger path in three ways:
    /// - the `symphony_runs` row already exists (we don't `create_run_row`),
    /// - in-memory `RunState` initializes every node to `Pending`, but the
    ///   actor's first tick will skip nodes whose `symphony_node_runs.status`
    ///   is `succeeded` because their depended-upon outputs are still in the
    ///   DB. Phase 2: load past node-run outputs into `RunState.nodes[*].output`
    ///   on resume so downstream nodes don't re-run their dep chain.
    /// - on resume we DO publish `SymphonyRunStarted` so subscribers see the
    ///   resume event with `metadata.resumed = true`.
    pub async fn resume_run(
        self: &Arc<Self>,
        bp: recovery::RunResumeBlueprint,
    ) -> anyhow::Result<()> {
        // Global concurrency check — recovery shouldn't overflow.
        let permit = self
            .global_run_sem
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("global run semaphore closed"))?;

        // Load workflow def at the version pinned to the run.
        let manager = SymphonyManager::new(self.db.clone());
        let detail = manager
            .get_workflow(&bp.workflow_id)
            .map_err(|e| anyhow::anyhow!("resume: workflow load: {}", e))?;
        if detail.row.current_version != bp.workflow_version {
            tracing::warn!(
                "SymphonyService::resume_run: run {} pinned to version {} but workflow current_version is {} (resuming with current — may diverge from original)",
                bp.run_id, bp.workflow_version, detail.row.current_version
            );
        }

        let deps = self
            .build_node_deps(detail.def.default_model.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("resume: build deps: {}", e))?;
        let per_wf_conc = detail
            .def
            .max_concurrent_nodes
            .unwrap_or(self.config.default_max_concurrent_nodes);

        let (reap_tx, reap_rx) = tokio::sync::oneshot::channel::<String>();
        let actor = RunActor::spawn(
            bp.run_id.clone(),
            detail.def.clone(),
            deps,
            per_wf_conc,
            Some(reap_tx),
        );
        self.runs.write().await.insert(bp.run_id.clone(), actor);

        let runs_for_reap = self.runs.clone();
        tokio::spawn(async move {
            if let Ok(reaped) = reap_rx.await {
                runs_for_reap.write().await.remove(&reaped);
            }
            drop(permit);
        });

        self.infra
            .publish(InfraEvent {
                id: 0,
                event_type: InfraEventType::SymphonyRunStarted,
                platform: "local".into(),
                timestamp: chrono::Utc::now().timestamp_millis(),
                message: ConversationMessage {
                    role: "system".into(),
                    content: format!("symphony run {} resumed", bp.run_id),
                },
                metadata: serde_json::json!({
                    "run_id": bp.run_id,
                    "workflow_id": bp.workflow_id,
                    "workflow_version": bp.workflow_version,
                    "resumed": true,
                }),
                trace_id: None,
            })
            .await;
        if let Some(app) = &self.app_handle {
            use tauri::Emitter;
            let _ = app.emit(
                "symphony:run_started",
                serde_json::json!({
                    "runId": bp.run_id,
                    "workflowId": bp.workflow_id,
                    "startedAt": chrono::Utc::now().timestamp_millis(),
                    "resumed": true,
                }),
            );
        }
        Ok(())
    }

    /// Public handle for the manager (used by `symphony_get_service_health`).
    pub fn run_count(&self) -> usize {
        // Best-effort sync read.
        if let Ok(g) = self.runs.try_read() {
            g.len()
        } else {
            0
        }
    }

    fn set_status(&self, s: ServiceStatus) {
        *self.status.lock().unwrap() = s;
    }

    /// Insert a `symphony_runs` row in `queued` state. Used by the manual
    /// trigger path before spawning the actor.
    pub fn create_run_row(
        conn: &Connection,
        run_id: &str,
        workflow_id: &str,
        workflow_version: i64,
        trigger_kind: &str,
        inputs_json: &str,
    ) -> rusqlite::Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO symphony_runs \
             (id, workflow_id, workflow_version, trigger_kind, trigger_payload_json, status, inputs_json, queued_at) \
             VALUES (?1, ?2, ?3, ?4, '{}', 'queued', ?5, ?6)",
            params![run_id, workflow_id, workflow_version, trigger_kind, inputs_json, now],
        )?;
        Ok(())
    }

    /// Build the per-node deps bundle for one run. Resolves the LLM provider
    /// + model via the app's `ProviderService` using the chain:
    /// 1. `workflow_default_model` (workflow YAML override), else
    /// 2. `provider_service.get_active_llm_config()` (the user's global default).
    ///
    /// Format of `workflow_default_model`: `"<provider_id>/<model_id>"` (mirrors
    /// the `role_models[*].model_ref` shape used elsewhere in the codebase).
    /// Returns Err if no model is configured / no API key set — caller marks
    /// the run as `quota_exceeded` semantically or fails it gracefully.
    ///
    /// Phase 1 limitation: one provider per run, shared across all nodes.
    /// Node-level `model` override changes the `model` string (cost calc)
    /// but still uses this provider. Per-node provider switching is a
    /// Phase 2 follow-up (`docs/superpowers/specs/...` §1.2).
    async fn build_node_deps(
        &self,
        workflow_default_model: Option<&str>,
    ) -> anyhow::Result<NodeExecutionDeps> {
        use crate::agent::tools::tool::ToolRegistry;

        let (provider_id, model, api_key, base_url, _) = if let Some(m) = workflow_default_model {
            // Parse `provider_id/model_id` per role_models convention.
            let parts: Vec<&str> = m.splitn(2, '/').collect();
            if parts.len() == 2 {
                self.provider_service
                    .get_provider_llm_config(parts[0], parts[1])
                    .await
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "symphony: workflow default_model `{}` references unconfigured provider",
                            m
                        )
                    })?
            } else {
                anyhow::bail!(
                    "symphony: workflow default_model `{}` must be in `provider/model` form",
                    m
                );
            }
        } else {
            self.provider_service
                .get_active_llm_config()
                .await
                .map(|(a, b, c, d, _api)| (a, b, c, d, None))
                .ok_or_else(|| anyhow::anyhow!("symphony: no active LLM model configured"))?
        };

        // Mirror automation's resolution: build LlmConfig from parts, validate
        // API key, then create the provider Arc.
        let llm_config = crate::llm::llm_config_from_provider(
            &provider_id,
            &model,
            &api_key,
            &base_url,
            8192,
            0.7,
            None, // TODO(Task 2): effective api
        );
        if llm_config.api_key.is_empty() && llm_config.provider != "ollama" {
            anyhow::bail!(
                "symphony: no API key configured for provider '{}'",
                provider_id
            );
        }
        let llm = crate::llm::create_provider(&llm_config)
            .map_err(|e| anyhow::anyhow!("symphony: create provider: {}", e))?;

        // Production wire-up of the Slice 1b safety chokepoint (follow-up to PR #564).
        // Source safety singletons from AppState via app_handle — SymphonyService
        // doesn't carry them directly. Falls back to None when no app_handle is
        // present (test scaffolds, early-boot paths).
        let (safety_manager, pending_approvals, hook_bus) =
            self.app_handle.as_ref().map(|ah| {
                use tauri::Manager;
                let state: tauri::State<'_, crate::app::AppState> = ah.state();
                let sm = state.safety_manager.clone();
                let pa = state.pending_approvals.clone();
                let hb = state.hook_bus.clone();
                drop(state);
                (Some(sm), Some(pa), Some(hb))
            }).unwrap_or((None, None, None));

        Ok(NodeExecutionDeps {
            db: self.db.clone(),
            llm,
            model,
            tools: Arc::new(ToolRegistry::new()),
            memory: self.memory.clone(),
            workspace_root: self.workspace_root.clone(),
            heartbeat: self.heartbeat.clone(),
            app_handle: self.app_handle.clone(),
            channel_manager: None,
            infra: self.infra.clone(),
            default_max_iterations: self.config.default_max_iterations,
            default_per_node_cost_cap_usd: self.config.default_per_node_cost_cap_usd,
            safety_manager,
            pending_approvals,
            hook_bus,
        })
    }

    /// Spawn the trigger loop. Returns immediately; the loop runs on a
    /// background tokio task until `stop()` cancels it.
    async fn spawn_trigger_loop(self: &Arc<Self>) {
        let mut rx = match self.trigger_rx.lock().await.take() {
            Some(rx) => rx,
            None => {
                tracing::warn!("symphony trigger loop already running");
                return;
            }
        };
        let svc = self.clone();
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    TriggerCmd::Manual {
                        run_id,
                        workflow_id,
                        workflow_version,
                        inputs_json: _,
                        respond_tx,
                    } => {
                        let permit = match svc.global_run_sem.clone().try_acquire_owned() {
                            Ok(p) => p,
                            Err(_) => {
                                let _ = respond_tx
                                    .send(Err("global concurrency cap reached".to_string()));
                                continue;
                            }
                        };
                        let manager = SymphonyManager::new(svc.db.clone());
                        let detail = match manager.get_workflow(&workflow_id) {
                            Ok(d) => d,
                            Err(e) => {
                                let _ =
                                    respond_tx.send(Err(format!("workflow load failed: {}", e)));
                                drop(permit);
                                continue;
                            }
                        };
                        // Resolve provider per-run. Failures (no model
                        // configured, missing API key, etc.) immediately
                        // fail the run rather than spending budget on
                        // doomed retries.
                        let deps = match svc
                            .build_node_deps(detail.def.default_model.as_deref())
                            .await
                        {
                            Ok(d) => d,
                            Err(e) => {
                                // Persist the failure on the run row so
                                // the canvas shows why the run never
                                // started, then release resources.
                                if let Ok(conn) = svc.db.lock() {
                                    let _ = conn.execute(
                                        "UPDATE symphony_runs SET status = 'failed', \
                                         outcome = 'failed', error_text = ?1, \
                                         completed_at = ?2 WHERE id = ?3",
                                        rusqlite::params![
                                            format!("provider resolve: {}", e),
                                            chrono::Utc::now().timestamp_millis(),
                                            &run_id,
                                        ],
                                    );
                                }
                                let _ = respond_tx
                                    .send(Err(format!("provider resolve: {}", e)));
                                drop(permit);
                                continue;
                            }
                        };
                        let per_wf_conc = detail
                            .def
                            .max_concurrent_nodes
                            .unwrap_or(svc.config.default_max_concurrent_nodes);

                        // Reaper oneshot — fires when the actor's run_loop
                        // exits, releases the permit + drops the actor from
                        // the in-flight map.
                        let (reap_tx, reap_rx) = tokio::sync::oneshot::channel::<String>();
                        let actor = RunActor::spawn(
                            run_id.clone(),
                            detail.def.clone(),
                            deps,
                            per_wf_conc,
                            Some(reap_tx),
                        );
                        svc.runs.write().await.insert(run_id.clone(), actor);

                        // Spawn a tiny task that awaits the reaper and tidies up.
                        let runs_for_reap = svc.runs.clone();
                        tokio::spawn(async move {
                            if let Ok(reaped) = reap_rx.await {
                                runs_for_reap.write().await.remove(&reaped);
                            }
                            // Dropping `permit` here releases one slot in the
                            // global semaphore — the next trigger can proceed.
                            drop(permit);
                        });

                        // InfraService event (run started). Uses the dedicated
                        // SymphonyRunStarted variant so proactive subscribers
                        // can distinguish dispatch from completion without
                        // sniffing `metadata.phase`.
                        svc.infra
                            .publish(InfraEvent {
                                id: 0,
                                event_type: InfraEventType::SymphonyRunStarted,
                                platform: "local".into(),
                                timestamp: chrono::Utc::now().timestamp_millis(),
                                message: ConversationMessage {
                                    role: "system".into(),
                                    content: format!("symphony run {} started", run_id),
                                },
                                metadata: serde_json::json!({
                                    "run_id": run_id,
                                    "workflow_id": workflow_id,
                                    "workflow_version": workflow_version,
                                }),
                                trace_id: None,
                            })
                            .await;
                        if let Some(app) = &svc.app_handle {
                            use tauri::Emitter;
                            let _ = app.emit(
                                "symphony:run_started",
                                serde_json::json!({
                                    "runId": run_id,
                                    "workflowId": workflow_id,
                                    "startedAt": chrono::Utc::now().timestamp_millis(),
                                }),
                            );
                        }
                        let _ = respond_tx.send(Ok(()));
                    }
                    TriggerCmd::Cancel { run_id } => {
                        if let Some(actor) = svc.runs.read().await.get(&run_id).cloned() {
                            actor.cancel.cancel();
                        }
                    }
                }
            }
        });
    }
}

#[async_trait]
impl ManagedService for SymphonyService {
    fn name(&self) -> &str {
        "SymphonyService"
    }

    async fn start(&self) -> anyhow::Result<()> {
        self.set_status(ServiceStatus::Starting);

        // 1. Reconcile in-flight rows. `reconcile()` flips stale running/
        //    ready node-runs to `stalled` and returns blueprints for every
        //    `queued`/`running` run.
        let blueprints = {
            let conn = self.db.lock().unwrap();
            let now_ms = chrono::Utc::now().timestamp_millis();
            recovery::reconcile(&conn, now_ms, self.config.stall_timeout_ms)?
        };
        tracing::info!(
            "SymphonyService: reconciled {} in-flight run(s) — resuming",
            blueprints.len()
        );

        // 2. Spawn the trigger loop. Resume actors AFTER so they enqueue
        //    through the same code path manual triggers use.
        let self_arc: Arc<SymphonyService> = self
            .self_weak
            .get()
            .and_then(|w| w.upgrade())
            .ok_or_else(|| anyhow::anyhow!("SymphonyService self-ref not initialized"))?;
        self_arc.spawn_trigger_loop().await;

        // 3. Resume each blueprint as a regular Manual trigger. We loop the
        //    sends in the background so a slow trigger loop doesn't block
        //    Stage 4. Each resumed run's row already exists in the DB
        //    (recovery only updates statuses), so the trigger loop's
        //    `create_run_row` path won't fire — instead it will go
        //    straight to actor spawn. To avoid recreating the row we
        //    short-circuit through a dedicated resume helper.
        for bp in blueprints {
            let svc_clone = self_arc.clone();
            tokio::spawn(async move {
                if let Err(e) = svc_clone.resume_run(bp).await {
                    tracing::warn!(
                        "SymphonyService: resume_run failed: {}",
                        e
                    );
                }
            });
        }

        *self.started_at.lock().unwrap() = Some(Instant::now());
        self.set_status(ServiceStatus::Running);
        Ok(())
    }

    /// Graceful shutdown per spec §5.1.
    ///
    /// 1. Flip status → Stopping. Trigger loop already accepts new Cancels but
    ///    Manual triggers received after this point race with shutdown; we
    ///    don't drain the channel — the outer `ServiceManager::stop_all`
    ///    completes before any new Tauri command can land.
    /// 2. Cancel every in-flight actor's token. The run loops respond by
    ///    cascading Cancelled across all non-terminal nodes, flushing the
    ///    transcript, then exiting.
    /// 3. Poll the `runs` map. Each actor's `reaper` removes its entry on
    ///    `run_loop` exit. When the map is empty (or budget elapses), return.
    ///
    /// Budget: `GRACEFUL_STOP_BUDGET_SECS` (4s). Spec §5.1 promises 10s, but
    /// `services::manager::STOP_TIMEOUT_SECS = 5` wraps each `ManagedService::
    /// stop()` in a 5s outer timeout — so any value we set above ~4.5s is
    /// effectively capped by the manager. We pick 4s to leave the manager
    /// ~1s of margin to finalize. If the spec budget needs to grow, bump
    /// the manager constant first.
    ///
    /// After budget, still-active actors are detached. The OS reaps them on
    /// process exit. We do NOT `abort()` because mid-transcript-persist
    /// aborts can corrupt agent_messages rows.
    async fn stop(&self) -> anyhow::Result<()> {
        const GRACEFUL_STOP_BUDGET_SECS: u64 = 4;
        self.set_status(ServiceStatus::Stopping);

        // Signal every actor to cancel.
        let actors: Vec<Arc<RunActor>> = self.runs.read().await.values().cloned().collect();
        let n_actors = actors.len();
        for a in &actors {
            a.cancel.cancel();
        }
        if n_actors == 0 {
            self.set_status(ServiceStatus::Stopped);
            return Ok(());
        }
        tracing::info!(
            "SymphonyService::stop: cancelling {} in-flight run(s), waiting up to {}s",
            n_actors,
            GRACEFUL_STOP_BUDGET_SECS
        );

        // Poll until the runs map drains or the budget expires.
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_secs(GRACEFUL_STOP_BUDGET_SECS);
        loop {
            if std::time::Instant::now() >= deadline {
                let still = self.runs.read().await.len();
                if still > 0 {
                    tracing::warn!(
                        "SymphonyService::stop: {}s budget elapsed with {} run(s) still active — detaching (transcripts may be partial)",
                        GRACEFUL_STOP_BUDGET_SECS,
                        still
                    );
                }
                break;
            }
            if self.runs.read().await.is_empty() {
                tracing::info!("SymphonyService::stop: all runs drained cleanly");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        self.set_status(ServiceStatus::Stopped);
        Ok(())
    }

    fn status(&self) -> ServiceStatus {
        self.status.lock().unwrap().clone()
    }

    fn health(&self) -> ServiceHealth {
        let uptime = self
            .started_at
            .lock()
            .unwrap()
            .map(|t| t.elapsed().as_secs());
        ServiceHealth {
            name: self.name().to_string(),
            status: self.status(),
            uptime_secs: uptime,
            last_error: None,
            metrics: serde_json::json!({
                "active_runs": self.run_count(),
                "stall_tracking_nodes": self.heartbeat.len(),
            }),
        }
    }
}
