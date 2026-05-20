//! AppRuntimeService — orchestrates spec activation, subscription wiring,
//! activity tracking, filter evaluation, run execution, and escalation
//! resolution.
//!
//! # execute_run pipeline (Phase 2a, design §D4)
//!
//! `execute_run` runs the full pipeline: TRIGGER → FILTER → per-spec
//! semaphore → per-day cost-cap check → run-session creation + ledger link →
//! provider resolution → `HeadlessDelegate` build → `run_agentic_loop` →
//! `CompletionGate` → activity-status mapping → transcript persist →
//! retention prune.
//!
//! The run-session is created and the ledger row linked EARLY (before
//! provider resolution) so a provider-resolution failure still leaves a
//! linked, observable run behind — it is marked `failed` and `execute_run`
//! returns `Ok(())` rather than panicking.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::{Mutex as TokioMutex, RwLock, Semaphore};

use crate::agent::types::{AgenticLoopConfig, ChatMessage, LoopOutcome, ReasoningContext};
use crate::automation::activity::{
    insert_activity, AutomationActivity, ActivityStatus, TriggerSource,
};
use crate::automation::filters;
use crate::automation::manager::HumaneSpecRow;
use crate::automation::memory::MemoryStore as AutomationMemoryStore;
use crate::automation::protocol::humane_v1::{HumaneAutomationSpec, Permission, Subscription};
use crate::automation::protocol::parse::parse_humane_v1;
use crate::automation::runtime::cost::{self, CostCapConfig, CostCapDecision, CostCapState};
use crate::automation::runtime::execute::HeadlessDelegate;
use crate::automation::runtime::{prompt, run_session, AutoContinueConfig, CompletionGate, PermissionSet};
use crate::automation::sources::{
    CustomSource, FileSource, RssSource, ScheduleSource, SubscriptionSource,
    TriggerCallback, WebhookSource, WebpageSource, WecomSource,
};
use crate::infra::InfraService;
use crate::memubot_config::AutomationConfig;
use crate::providers::service::ProviderService;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};

// ─── constants ────────────────────────────────────────────────────────────────

/// Maximum concurrent runs per spec (Phase 1 hard-code; Phase 2 makes this
/// configurable via `HumaneAutomationSpec.config_schema`).
const PER_SPEC_CONCURRENCY: usize = 2;

// ─── EscalationRow ───────────────────────────────────────────────────────────

/// A row from `automation_escalations` returned by `list_pending_escalations`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EscalationRow {
    pub id: String,
    pub spec_id: String,
    pub activity_id: String,
    pub question: String,
    pub choices_json: String,
    pub status: String,
    pub user_choice: Option<String>,
    pub user_note: Option<String>,
    pub created_at: i64,
    pub responded_at: Option<i64>,
}

// ─── AppRuntimeService ────────────────────────────────────────────────────────

/// Bundle of I/O handles attached to a chat-session run. Stashed in
/// `AppRuntimeService.pending_chat_handles` when `execute_run_in_chat_session`
/// is called, and consumed by `execute_run` when building the HeadlessDelegate.
struct ChatHandleBundle {
    streaming: Option<Arc<dyn crate::channels::types::StreamingHandle>>,
    reply: Option<Arc<crate::channels::types::ReplyHandle>>,
    app: Option<tauri::AppHandle>,
}

pub struct AppRuntimeService {
    pub db: Arc<StdMutex<rusqlite::Connection>>,
    pub schedule: Arc<ScheduleSource>,
    pub file: Arc<FileSource>,
    pub webhook: Arc<WebhookSource>,
    pub webpage: Arc<WebpageSource>,
    pub rss: Arc<RssSource>,
    pub wecom: Arc<WecomSource>,
    pub custom: Arc<CustomSource>,
    pub infra: Arc<InfraService>,
    /// Automation-scoped file-based memory store.
    pub memory: Arc<AutomationMemoryStore>,
    /// Provider service — resolves the LlmProvider + model for a run.
    pub provider_service: Arc<ProviderService>,

    /// Per-spec semaphore; inserted lazily on first `activate`.
    semaphores: Arc<RwLock<HashMap<String, Arc<Semaphore>>>>,

    /// Tracks (sub_id, source_type_tag) per spec for clean `deactivate`.
    attached: Arc<TokioMutex<HashMap<String, Vec<(String, String)>>>>,

    /// Per-chat-session mutex map. Serializes burst messages on the same
    /// (spec, identity) chat thread — the agent loop is not interruptible,
    /// so concurrent calls would race. Entries are created lazily; never
    /// cleaned up (bounded by #sessions, ~tens of KB).
    chat_session_locks: Arc<TokioMutex<HashMap<String, Arc<TokioMutex<()>>>>>,

    /// Stash of I/O handles awaiting consumption by the next `execute_run`
    /// for this chat session. Set by `execute_run_in_chat_session`; drained
    /// by `execute_run` when it builds the HeadlessDelegate.
    pending_chat_handles: Arc<TokioMutex<HashMap<String, ChatHandleBundle>>>,

    status: Arc<StdMutex<ServiceStatus>>,
    started_at: Arc<StdMutex<Option<Instant>>>,

    /// Weak self-reference, set once during `new()` so `weak_ref()` never
    /// needs to touch `Arc` internals via raw pointers.
    self_weak: OnceLock<Weak<AppRuntimeService>>,

    /// IPC handle for automation notifications. Passed to HeadlessDelegate.
    pub app_handle: Option<tauri::AppHandle>,
    /// Channel manager for extended notification types. Passed to HeadlessDelegate.
    pub channel_manager: Option<Arc<tokio::sync::RwLock<crate::channels::ChannelManager>>>,
}

impl AppRuntimeService {
    pub fn new(
        db: Arc<StdMutex<rusqlite::Connection>>,
        schedule: Arc<ScheduleSource>,
        file: Arc<FileSource>,
        webhook: Arc<WebhookSource>,
        webpage: Arc<WebpageSource>,
        rss: Arc<RssSource>,
        wecom: Arc<WecomSource>,
        custom: Arc<CustomSource>,
        infra: Arc<InfraService>,
        memory: Arc<AutomationMemoryStore>,
        provider_service: Arc<ProviderService>,
        app_handle: Option<tauri::AppHandle>,
        channel_manager: Option<Arc<tokio::sync::RwLock<crate::channels::ChannelManager>>>,
    ) -> Arc<Self> {
        let svc = Arc::new(Self {
            db,
            schedule,
            file,
            webhook,
            webpage,
            rss,
            wecom,
            custom,
            infra,
            memory,
            provider_service,
            semaphores: Arc::new(RwLock::new(HashMap::new())),
            attached: Arc::new(TokioMutex::new(HashMap::new())),
            chat_session_locks: Arc::new(TokioMutex::new(HashMap::new())),
            pending_chat_handles: Arc::new(TokioMutex::new(HashMap::new())),
            status: Arc::new(StdMutex::new(ServiceStatus::Stopped)),
            started_at: Arc::new(StdMutex::new(None)),
            self_weak: OnceLock::new(),
            app_handle,
            channel_manager,
        });
        let _ = svc.self_weak.set(Arc::downgrade(&svc));
        svc
    }

    // ── spec loading ────────────────────────────────────────────────────────

    fn load_spec_json(&self, spec_id: &str) -> anyhow::Result<(String, serde_json::Value)> {
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
        let (spec_json, enabled): (String, i64) = conn
            .query_row(
                "SELECT spec_json, enabled FROM automation_specs WHERE id = ?1",
                rusqlite::params![spec_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| anyhow::anyhow!("spec not found {}: {}", spec_id, e))?;

        if enabled == 0 {
            anyhow::bail!("spec {} is disabled", spec_id);
        }

        let value: serde_json::Value = serde_json::from_str(&spec_json)
            .map_err(|e| anyhow::anyhow!("spec_json parse error: {}", e))?;
        Ok((spec_json, value))
    }

    fn parse_humane_spec(spec_json: &str) -> anyhow::Result<HumaneAutomationSpec> {
        serde_json::from_str(spec_json)
            .map_err(|e| anyhow::anyhow!("humane spec parse: {}", e))
    }

    // ── semaphore access ────────────────────────────────────────────────────

    async fn semaphore_for(&self, spec_id: &str) -> Arc<Semaphore> {
        {
            let r = self.semaphores.read().await;
            if let Some(s) = r.get(spec_id) {
                return s.clone();
            }
        }
        let mut w = self.semaphores.write().await;
        w.entry(spec_id.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(PER_SPEC_CONCURRENCY)))
            .clone()
    }

    // ── source dispatch ─────────────────────────────────────────────────────

    fn source_tag(sub: &Subscription) -> &'static str {
        match sub {
            Subscription::Schedule(_) => "schedule",
            Subscription::File(_)     => "file",
            Subscription::Webhook(_)  => "webhook",
            Subscription::Webpage(_)  => "webpage",
            Subscription::Rss(_)      => "rss",
            Subscription::Wecom(_)    => "wecom",
            Subscription::Custom(_)   => "custom",
        }
    }

    fn trigger_source_from_tag(tag: &str) -> TriggerSource {
        match tag {
            "schedule" => TriggerSource::Schedule,
            "file"     => TriggerSource::File,
            "webhook"  => TriggerSource::Webhook,
            "webpage"  => TriggerSource::Webpage,
            "rss"      => TriggerSource::Rss,
            "wecom"    => TriggerSource::Wecom,
            "custom"   => TriggerSource::Custom,
            _          => TriggerSource::Custom,
        }
    }

    async fn attach_one(
        &self,
        spec_id: &str,
        sub_id: &str,
        sub: &Subscription,
        cb: TriggerCallback,
    ) -> anyhow::Result<()> {
        let src: &dyn SubscriptionSource = match sub {
            Subscription::Schedule(_) => self.schedule.as_ref(),
            Subscription::File(_)     => self.file.as_ref(),
            Subscription::Webhook(_)  => self.webhook.as_ref(),
            Subscription::Webpage(_)  => self.webpage.as_ref(),
            Subscription::Rss(_)      => self.rss.as_ref(),
            Subscription::Wecom(_)    => self.wecom.as_ref(),
            Subscription::Custom(_)   => self.custom.as_ref(),
        };
        src.attach(spec_id, sub_id, sub, cb).await
    }

    async fn detach_one(&self, spec_id: &str, sub_id: &str, tag: &str) -> anyhow::Result<()> {
        let src: &dyn SubscriptionSource = match tag {
            "schedule" => self.schedule.as_ref(),
            "file"     => self.file.as_ref(),
            "webhook"  => self.webhook.as_ref(),
            "webpage"  => self.webpage.as_ref(),
            "rss"      => self.rss.as_ref(),
            "wecom"    => self.wecom.as_ref(),
            _          => self.custom.as_ref(),
        };
        src.detach(spec_id, sub_id).await
    }

    // ── public API ──────────────────────────────────────────────────────────

    /// Register all subscriptions for `spec_id` and start listening.
    ///
    /// Idempotent: re-activating an already-active spec is a no-op (the
    /// attached map already has entries).
    pub async fn activate(&self, spec_id: &str) -> anyhow::Result<()> {
        // Already active?
        {
            let a = self.attached.lock().await;
            if a.contains_key(spec_id) {
                tracing::debug!("[AppRuntimeService] spec {} already active, skipping", spec_id);
                return Ok(());
            }
        }

        let (spec_json, _) = self.load_spec_json(spec_id)?;
        let spec = Self::parse_humane_spec(&spec_json)?;

        // Ensure semaphore entry exists.
        self.semaphore_for(spec_id).await;

        let mut attached_subs: Vec<(String, String)> = Vec::new();

        for (idx, sub) in spec.subscriptions.iter().enumerate() {
            let sub_id = format!("{}-sub-{}", spec_id, idx);
            let tag = Self::source_tag(sub).to_string();

            // Phase 2b cluster A: autonomous triggers (scheduled / file /
            // webhook / webpage / rss / wecom / custom) route into the spec
            // owner's "local" chat session instead of creating per-fire
            // automation:scheduled sessions.
            //
            // Operator-observability note: execute_run is now invoked with
            // sub_id = None (the chat session is the unit of work, not the
            // per-fire activity). As a result `automation_activities.subscription_id`
            // is NULL for every new autonomous fire. We capture sub_id in the
            // closure so it survives in tracing logs — that's the only place
            // where "which subscription fired" can still be observed.
            let svc = self.weak_ref();
            let sub_id_log = sub_id.clone();
            let cb: TriggerCallback = Arc::new(move |sid: String, _sub: String, payload: serde_json::Value| {
                let svc = svc.clone();
                let sub_id_log = sub_id_log.clone();
                tokio::spawn(async move {
                    if let Some(svc) = svc.upgrade() {
                        let app = svc.app_handle.clone();
                        if let Err(e) = svc
                            .execute_run_in_chat_session(
                                &sid,
                                "local",
                                payload,
                                None, // no UI streaming on autonomous fire
                                None, // no IM reply target for autonomous fire
                                app,
                            )
                            .await
                        {
                            tracing::warn!(
                                "[AppRuntimeService] execute_run_in_chat_session error for spec {} (sub {}): {}",
                                sid,
                                sub_id_log,
                                e
                            );
                        }
                    }
                });
            });

            match self.attach_one(spec_id, &sub_id, sub, cb).await {
                Ok(()) => {
                    tracing::info!(
                        "[AppRuntimeService] attached {} subscription {} for spec {}",
                        tag, sub_id, spec_id
                    );
                    attached_subs.push((sub_id, tag));
                }
                Err(e) => {
                    tracing::warn!(
                        "[AppRuntimeService] failed to attach {} sub {} for spec {}: {}",
                        tag, sub_id, spec_id, e
                    );
                }
            }
        }

        self.attached.lock().await.insert(spec_id.to_string(), attached_subs);
        tracing::info!("[AppRuntimeService] spec {} activated", spec_id);
        Ok(())
    }

    /// Detach all subscriptions for `spec_id`.
    pub async fn deactivate(&self, spec_id: &str) -> anyhow::Result<()> {
        let subs = self.attached.lock().await.remove(spec_id).unwrap_or_default();
        for (sub_id, tag) in &subs {
            if let Err(e) = self.detach_one(spec_id, sub_id, tag).await {
                tracing::warn!(
                    "[AppRuntimeService] detach error for spec {} sub {}: {}",
                    spec_id, sub_id, e
                );
            }
        }
        tracing::info!(
            "[AppRuntimeService] spec {} deactivated ({} subs removed)",
            spec_id,
            subs.len()
        );
        Ok(())
    }

    /// Core run pipeline: TRIGGER → FILTER → PERSIST → run-session →
    /// `run_agentic_loop` → `CompletionGate` → activity-status mapping.
    /// See the module-level doc for the full step sequence.
    pub async fn execute_run(
        &self,
        spec_id: &str,
        sub_id: Option<&str>,
        payload: serde_json::Value,
    ) -> anyhow::Result<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let activity_id = uuid::Uuid::new_v4().to_string();

        // ── 1. determine trigger source type ────────────────────────────────
        let source_tag = sub_id
            .and_then(|s| s.split('-').nth_back(0).and_then(|_| {
                // sub_id format: "{spec_id}-sub-{idx}"
                // derive tag from the attached map
                None::<&str>  // we'll look it up below
            }))
            .unwrap_or("custom");

        let trigger_source = {
            let attached = self.attached.lock().await;
            if let Some(subs) = attached.get(spec_id) {
                if let Some(sub_id_str) = sub_id {
                    subs.iter()
                        .find(|(sid, _)| sid == sub_id_str)
                        .map(|(_, tag)| Self::trigger_source_from_tag(tag))
                        .unwrap_or(TriggerSource::Custom)
                } else {
                    TriggerSource::Manual
                }
            } else {
                TriggerSource::Manual
            }
        };
        let _ = source_tag; // silence unused warning

        // ── 2. insert queued activity row ────────────────────────────────────
        let activity = AutomationActivity {
            id:                          activity_id.clone(),
            spec_id:                     spec_id.to_string(),
            subscription_id:             sub_id.map(|s| s.to_string()),
            trigger_source_type:         trigger_source,
            trigger_payload_json:        serde_json::to_string(&payload)
                                             .unwrap_or_else(|_| "{}".into()),
            status:                      ActivityStatus::Queued,
            error_text:                  None,
            queued_at:                   now_ms,
            started_at:                  None,
            completed_at:                None,
            duration_ms:                 0,
            llm_iterations:              0,
            llm_tokens_in:               0,
            llm_tokens_out:              0,
            session_id:                  None,
            report_artifacts_json:       "[]".into(),
            report_text:                 None,
            report_outcome:              None,
            escalation_id:               None,
            resumed_from_activity_id:    None,
            resumed_from_escalation_id:  None,
            working_dir:                 String::new(),
        };

        {
            let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
            insert_activity(&conn, &activity)
                .map_err(|e| anyhow::anyhow!("insert activity: {}", e))?;
        }

        tracing::debug!(
            "[AppRuntimeService] activity {} inserted for spec {} (source={:?})",
            activity_id, spec_id, trigger_source
        );

        // ── 3. load spec for filter evaluation ──────────────────────────────
        let spec = match self.load_spec_json(spec_id) {
            Ok((json, _)) => match Self::parse_humane_spec(&json) {
                Ok(s) => s,
                Err(e) => {
                    self.update_activity_status(&activity_id, "failed", Some(&e.to_string()))?;
                    return Err(e);
                }
            },
            Err(e) => {
                self.update_activity_status(&activity_id, "failed", Some(&e.to_string()))?;
                return Err(e);
            }
        };

        // ── 4. evaluate filters ──────────────────────────────────────────────
        let filter_ctx = serde_json::json!({ "event": payload });
        if !filters::evaluate(&spec.filters, &filter_ctx) {
            tracing::info!(
                "[AppRuntimeService] activity {} filtered out for spec {}",
                activity_id, spec_id
            );
            // New status value introduced in Phase 1: `filtered_out`
            self.update_activity_status(&activity_id, "filtered_out", None)?;
            return Ok(());
        }

        // ── 5. acquire per-spec semaphore ────────────────────────────────────
        let sem = self.semaphore_for(spec_id).await;
        let _permit = sem.acquire().await.map_err(|e| anyhow::anyhow!("semaphore: {}", e))?;

        // ── 6. per-day cost cap check ────────────────────────────────────────
        // TODO(2b): read caps from MemubotConfig.automation once threaded into
        // AppRuntimeService — for now use the AutomationConfig defaults.
        // Hoisted here so step 12 (retention_runs_per_spec) can reuse the same binding.
        let auto_cfg = AutomationConfig::default();
        let cost_cap = CostCapConfig {
            per_run_usd: auto_cfg.per_run_cost_cap_usd,
            per_day_usd: auto_cfg.per_day_cost_cap_usd,
        };
        {
            let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
            let day_total = cost::day_total_usd(&conn);
            if cost::check_per_day(day_total, cost_cap) == CostCapDecision::DenyPerDay {
                // drop before update_activity_status — StdMutex is not reentrant.
                drop(conn);
                tracing::warn!(
                    "[AppRuntimeService] per-day cost cap reached (${:.4} >= ${:.2}) — skipping run for spec {}",
                    day_total, cost_cap.per_day_usd, spec_id
                );
                self.update_activity_status(
                    &activity_id,
                    "failed",
                    Some("per-day cost cap reached"),
                )?;
                return Ok(());
            }
        }

        // ── 7. create the run-session + link the ledger row ──────────────────
        // Done EARLY (before provider resolution) so a provider failure still
        // leaves a linked run-session behind — the run is observable either way.
        let started_ms = chrono::Utc::now().timestamp_millis();

        // Phase 2b cluster A: if execute_run_in_chat_session routed us here,
        // it stashed the chat session id under `_chat_session_id` in the
        // payload so we reuse that session instead of creating a per-fire
        // automation:scheduled record. Legacy callers (no hint) keep the
        // pre-existing per-fire behavior unchanged.
        let chat_session_id_hint: Option<String> = payload
            .get("_chat_session_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        let (session_id, workspace_root) = {
            let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
            run_session::ensure_automations_space(&conn)
                .map_err(|e| anyhow::anyhow!("ensure automations space: {}", e))?;
            let space_id = run_session::resolve_home_space(&conn, spec_id)
                .map_err(|e| anyhow::anyhow!("resolve home space: {}", e))?;
            let session_id = match chat_session_id_hint.clone() {
                Some(id) => id,
                None => run_session::create_run_session(
                    &conn,
                    spec_id,
                    &space_id,
                    trigger_source.as_db_str(),
                    &activity_id,
                )
                .map_err(|e| anyhow::anyhow!("create run session: {}", e))?,
            };
            conn.execute(
                "UPDATE automation_activities
                 SET session_id = ?2, status = 'running', started_at = ?3
                 WHERE id = ?1",
                rusqlite::params![activity_id, session_id, started_ms],
            )
            .map_err(|e| anyhow::anyhow!("link session to activity: {}", e))?;

            // Resolve the run's working directory: the space's path if it has
            // one, else a per-spec dir under ~/Documents/workground/automations.
            let space_path: Option<String> = conn
                .query_row(
                    "SELECT path FROM spaces WHERE id = ?1",
                    rusqlite::params![space_id],
                    |r| r.get(0),
                )
                .ok()
                .flatten();
            let workspace_root = match space_path {
                Some(p) if !p.trim().is_empty() => std::path::PathBuf::from(p),
                _ => {
                    let dir = dirs::home_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join("Documents/workground/automations")
                        .join(spec_id);
                    let _ = std::fs::create_dir_all(&dir);
                    dir
                }
            };
            (session_id, workspace_root)
        };

        tracing::info!(
            "[AppRuntimeService] run-session {} created for spec {} (activity {})",
            session_id, spec_id, activity_id
        );

        // ── 8. resolve the LLM provider + model ──────────────────────────────
        // A resolution failure (no active model / no API key) is NOT fatal to
        // the pipeline: the run-session already exists + is linked, so mark
        // the activity `failed` and return Ok — never panic or propagate.
        let (llm, model) = match self.resolve_run_provider().await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(
                    "[AppRuntimeService] provider resolution failed for spec {}: {}",
                    spec_id, e
                );
                self.update_activity_status(
                    &activity_id,
                    "failed",
                    Some(&format!("resolve provider: {}", e)),
                )?;
                return Ok(());
            }
        };

        // ── 9. build the delegate + reasoning context ────────────────────────
        let permissions = match self.load_permission_set(spec_id) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    "[AppRuntimeService] permission load failed for spec {}: {}",
                    spec_id, e
                );
                self.update_activity_status(
                    &activity_id,
                    "failed",
                    Some(&format!("load permissions: {}", e)),
                )?;
                return Ok(());
            }
        };

        let memory_text = self.memory.read(spec_id).await.unwrap_or_default();
        let system_prompt = prompt::build_system_prompt(&spec);
        let initial_message = prompt::build_initial_message_with_memory(
            None,
            &payload,
            &serde_json::json!({}),
            None,
            &memory_text,
        );
        let mut reason_ctx = ReasoningContext::new(system_prompt);
        reason_ctx.messages.push(ChatMessage::user(&initial_message));

        let tools = self.build_automation_tool_registry(&workspace_root);

        // Phase 2b cluster A: if we're in a chat-session run, drain the
        // I/O handles stashed for us by execute_run_in_chat_session. None
        // for legacy per-fire runs.
        let chat_handles = if let Some(ref sid) = chat_session_id_hint {
            let mut slot = self.pending_chat_handles.lock().await;
            slot.remove(sid)
        } else {
            None
        };

        let delegate = HeadlessDelegate {
            spec_id: spec_id.to_string(),
            activity_id: activity_id.clone(),
            session_id: session_id.clone(),
            permissions,
            memory: self.memory.clone(),
            db: self.db.clone(),
            gate: Arc::new(TokioMutex::new(None)),
            auto_continue: AutoContinueConfig::default(),
            llm,
            model: model.clone(),
            tools,
            cost: Arc::new(CostCapState::new(cost_cap)),
            workspace_root,
            app_handle: chat_handles
                .as_ref()
                .and_then(|b| b.app.clone())
                .or_else(|| self.app_handle.clone()),
            channel_manager: self.channel_manager.clone(),
            reply_handle: chat_handles.as_ref().and_then(|b| b.reply.clone()),
            streaming_handle: chat_handles.as_ref().and_then(|b| b.streaming.clone()),
            system_prompt_override: None,
        };

        // ── 10. run the agentic loop ─────────────────────────────────────────
        let mut loop_config = AgenticLoopConfig::from_model(&model);
        loop_config.max_iterations = auto_cfg.max_iterations;
        let outcome = crate::agent::agentic_loop::run_agentic_loop(
            &delegate,
            &mut reason_ctx,
            &loop_config,
        )
        .await;

        // ── 11. map terminal state → activity row + persist transcript ───────
        let gate = delegate.gate.lock().await.clone().or_else(|| {
            matches!(outcome, LoopOutcome::MaxIterations)
                .then_some(CompletionGate::LoopExhausted)
        });
        let completed_ms = chrono::Utc::now().timestamp_millis();

        // Derive the failure text once — reused for the activity row's
        // error_text AND the transcript's terminal marker below. None for
        // the success (Reported) + escalation paths.
        let failure_text: Option<String> = match &gate {
            Some(CompletionGate::Reported { .. }) | Some(CompletionGate::Escalated { .. }) => None,
            Some(CompletionGate::ErrorTerminal(msg)) => Some(msg.clone()),
            Some(CompletionGate::LoopExhausted) | None => Some(match &outcome {
                LoopOutcome::Failure { error } => error.clone(),
                LoopOutcome::MaxIterations => {
                    "loop reached max iterations without report_to_user".to_string()
                }
                LoopOutcome::Stopped => {
                    "run stopped before completion".to_string()
                }
                LoopOutcome::Cancelled { .. } => {
                    "run stopped before completion".to_string()
                }
                _ => "loop ended without report".to_string(),
            }),
        };

        // Append a clear terminal marker to the transcript so the run-session
        // view shows how the run ended. report_to_user's tool_result already
        // marks the success case; this covers escalation + every failure.
        match &gate {
            Some(CompletionGate::Escalated { .. }) => {
                reason_ctx.messages.push(ChatMessage::user(
                    "⏸️ Run paused — escalated for a user decision.",
                ));
            }
            _ => {
                if let Some(err) = &failure_text {
                    reason_ctx
                        .messages
                        .push(ChatMessage::user(&format!("⚠️ Run failed: {}", err)));
                }
            }
        }

        {
            let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
            // Persist the transcript regardless of outcome.
            if let Err(e) = run_session::persist_transcript(&conn, &session_id, &reason_ctx.messages) {
                tracing::warn!(
                    "[AppRuntimeService] transcript persist failed for session {}: {}",
                    session_id, e
                );
            }
            match &gate {
                // report_to_user already set status='completed' in the delegate.
                Some(CompletionGate::Reported { .. }) => {}
                Some(CompletionGate::Escalated { escalation_id }) => {
                    conn.execute(
                        "UPDATE automation_activities
                         SET status = 'waiting_user', escalation_id = ?2, completed_at = ?3
                         WHERE id = ?1",
                        rusqlite::params![activity_id, escalation_id, completed_ms],
                    )
                    .map_err(|e| anyhow::anyhow!("activity escalation update: {}", e))?;
                }
                // ErrorTerminal / LoopExhausted / no gate → failed; failure_text
                // was derived above.
                _ => {
                    let err_text =
                        failure_text.as_deref().unwrap_or("loop ended without report");
                    conn.execute(
                        "UPDATE automation_activities
                         SET status = 'failed', error_text = ?2, completed_at = ?3
                         WHERE id = ?1",
                        rusqlite::params![activity_id, err_text, completed_ms],
                    )
                    .map_err(|e| anyhow::anyhow!("activity failure update: {}", e))?;
                }
            }

            // ── 12. retention prune (best-effort) ────────────────────────────
            let keep = auto_cfg.retention_runs_per_spec;
            match run_session::prune_old_run_sessions(&conn, spec_id, keep) {
                Ok(n) if n > 0 => tracing::info!(
                    "[AppRuntimeService] pruned {} old run-session(s) for spec {}",
                    n, spec_id
                ),
                Ok(_) => {}
                Err(e) => tracing::warn!(
                    "[AppRuntimeService] run-session prune failed for spec {}: {}",
                    spec_id, e
                ),
            }
        }

        tracing::info!(
            "[AppRuntimeService] run complete for spec {} (activity {}, session {})",
            spec_id, activity_id, session_id
        );
        // ── semaphore released (via _permit Drop) ────────────────────────────
        Ok(())
    }

    /// Acknowledge a human-in-the-loop escalation.
    ///
    /// Phase 1: marks the row resolved.  Auto-resumption of the original run
    /// is deferred to Phase 2 (which will call `execute_run` with an
    /// `EscalationResolution` payload referencing `resumed_from_escalation_id`).
    pub async fn resolve_escalation(
        &self,
        escalation_id: &str,
        user_choice: &str,
        user_note: Option<&str>,
    ) -> anyhow::Result<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
        let rows = conn
            .execute(
                "UPDATE automation_escalations
                 SET status = 'resolved',
                     user_choice  = ?2,
                     user_note    = ?3,
                     responded_at = ?4
                 WHERE id = ?1",
                rusqlite::params![escalation_id, user_choice, user_note, now_ms],
            )
            .map_err(|e| anyhow::anyhow!("escalation update: {}", e))?;

        if rows == 0 {
            anyhow::bail!("escalation {} not found", escalation_id);
        }

        tracing::info!(
            "[AppRuntimeService] escalation {} resolved with choice '{}'",
            escalation_id, user_choice
        );
        Ok(())
    }

    // ── § 7.3 management API ────────────────────────────────────────────────

    /// Parse YAML + insert a new row in `automation_specs` with source='local'.
    /// Delegates to [`install_humane_spec_from_source`] for the actual work.
    pub async fn install_humane_spec(
        &self,
        yaml: &str,
        source_ref: Option<String>,
    ) -> anyhow::Result<HumaneSpecRow> {
        self.install_humane_spec_from_source(yaml, "local", source_ref).await
    }

    /// Parse YAML + insert a new row in `automation_specs` with a caller-supplied `source`.
    /// Used by the marketplace path to stamp rows with `source='marketplace'`.
    pub async fn install_humane_spec_from_source(
        &self,
        yaml: &str,
        source: &str,
        source_ref: Option<String>,
    ) -> anyhow::Result<HumaneSpecRow> {
        // 1. Parse + validate
        let parsed = parse_humane_v1(yaml)
            .map_err(|e| anyhow::anyhow!("parse error: {}", e))?;
        let spec = &parsed.spec;

        // 2. IDs and timestamps
        let spec_id = uuid::Uuid::new_v4().to_string();
        let now_ms = chrono::Utc::now().timestamp_millis();

        // 3. Serialise to canonical JSON
        let spec_json = serde_json::to_string(spec)
            .map_err(|e| anyhow::anyhow!("spec_json serialise: {}", e))?;

        // 4. INSERT
        {
            let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
            conn.execute(
                "INSERT INTO automation_specs
                 (id, name, version, author, description, system_prompt,
                  spec_format, spec_yaml, spec_json,
                  user_config_values, permissions_granted, permissions_denied,
                  status, enabled, source, source_ref,
                  created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,'humane-yaml-v1',?7,?8,'{}','[]','[]',
                         'active',1,?9,?10,?11,?11)",
                rusqlite::params![
                    spec_id,
                    spec.name,
                    spec.version,
                    spec.author,
                    spec.description,
                    spec.system_prompt,
                    yaml,
                    spec_json,
                    source,
                    source_ref,
                    now_ms,
                ],
            )
            .map_err(|e| anyhow::anyhow!("insert spec: {}", e))?;
        }

        // 5. Re-read and return the persisted row
        self.get_spec(&spec_id)
    }

    /// Read a file from disk and install it as a Humane spec.
    pub async fn import_humane_spec_file(
        &self,
        path: &str,
    ) -> anyhow::Result<HumaneSpecRow> {
        let yaml = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| anyhow::anyhow!("read file '{}': {}", path, e))?;
        self.install_humane_spec(&yaml, Some(path.to_string())).await
    }

    /// List all specs ordered by creation time descending.
    pub fn list_specs(&self) -> anyhow::Result<Vec<HumaneSpecRow>> {
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, version, author, description, system_prompt,
                        spec_format, spec_yaml, spec_json,
                        user_config_values, permissions_granted, permissions_denied,
                        status, enabled, space_id, source, source_ref, source_version,
                        created_at, updated_at, last_run_at, last_run_outcome,
                        COALESCE(trigger_phrase, '') as trigger_phrase,
                        COALESCE(system_prompt_override, '') as system_prompt_override
                 FROM automation_specs
                 ORDER BY created_at DESC",
            )
            .map_err(|e| anyhow::anyhow!("prepare list: {}", e))?;
        let rows = stmt
            .query_map([], Self::row_to_spec_row)
            .map_err(|e| anyhow::anyhow!("query: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("row: {}", e))?;
        Ok(rows)
    }

    /// Fetch a single spec by ID.
    pub fn get_spec(&self, spec_id: &str) -> anyhow::Result<HumaneSpecRow> {
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
        conn.query_row(
            "SELECT id, name, version, author, description, system_prompt,
                    spec_format, spec_yaml, spec_json,
                    user_config_values, permissions_granted, permissions_denied,
                    status, enabled, space_id, source, source_ref, source_version,
                    created_at, updated_at, last_run_at, last_run_outcome,
                    COALESCE(trigger_phrase, '') as trigger_phrase,
                    COALESCE(system_prompt_override, '') as system_prompt_override
             FROM automation_specs WHERE id = ?1",
            rusqlite::params![spec_id],
            Self::row_to_spec_row,
        )
        .map_err(|e| anyhow::anyhow!("spec not found '{}': {}", spec_id, e))
    }

    /// Overwrite `user_config_values` for a spec.
    pub fn update_user_config(
        &self,
        spec_id: &str,
        values: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let json_str = serde_json::to_string(values)
            .map_err(|e| anyhow::anyhow!("serialise values: {}", e))?;
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
        let rows = conn
            .execute(
                "UPDATE automation_specs SET user_config_values = ?2, updated_at = ?3 WHERE id = ?1",
                rusqlite::params![spec_id, json_str, now_ms],
            )
            .map_err(|e| anyhow::anyhow!("update user_config: {}", e))?;
        if rows == 0 {
            anyhow::bail!("spec '{}' not found", spec_id);
        }
        Ok(())
    }

    /// Grant or deny a single permission; writes to `permission_audit_log`.
    pub async fn set_permission(
        &self,
        spec_id: &str,
        permission: &str,
        granted: bool,
    ) -> anyhow::Result<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;

        // Load current arrays
        let (mut pg, mut pd): (Vec<String>, Vec<String>) = conn
            .query_row(
                "SELECT permissions_granted, permissions_denied FROM automation_specs WHERE id = ?1",
                rusqlite::params![spec_id],
                |r| {
                    let pg: String = r.get(0)?;
                    let pd: String = r.get(1)?;
                    Ok((pg, pd))
                },
            )
            .map_err(|e| anyhow::anyhow!("spec not found '{}': {}", spec_id, e))
            .and_then(|(pg_str, pd_str)| {
                let pg: Vec<String> = serde_json::from_str(&pg_str)
                    .map_err(|e| anyhow::anyhow!("parse permissions_granted: {}", e))?;
                let pd: Vec<String> = serde_json::from_str(&pd_str)
                    .map_err(|e| anyhow::anyhow!("parse permissions_denied: {}", e))?;
                Ok((pg, pd))
            })?;

        // Mutate
        if granted {
            if !pg.contains(&permission.to_string()) {
                pg.push(permission.to_string());
            }
            pd.retain(|p| p != permission);
        } else {
            if !pd.contains(&permission.to_string()) {
                pd.push(permission.to_string());
            }
            pg.retain(|p| p != permission);
        }

        let pg_json = serde_json::to_string(&pg)
            .map_err(|e| anyhow::anyhow!("serialise granted: {}", e))?;
        let pd_json = serde_json::to_string(&pd)
            .map_err(|e| anyhow::anyhow!("serialise denied: {}", e))?;

        conn.execute(
            "UPDATE automation_specs
             SET permissions_granted = ?2, permissions_denied = ?3, updated_at = ?4
             WHERE id = ?1",
            rusqlite::params![spec_id, pg_json, pd_json, now_ms],
        )
        .map_err(|e| anyhow::anyhow!("update permissions: {}", e))?;

        // Audit log (V14 table — columns: id, session_id, tool_name, args_hash, decision, rule_id, created_at)
        let audit_id = uuid::Uuid::new_v4().to_string();
        let decision = if granted { "user_approve" } else { "user_deny" };
        conn.execute(
            "INSERT INTO permission_audit_log (id, session_id, tool_name, args_hash, decision, created_at)
             VALUES (?1, ?2, ?3, '', ?4, ?5)",
            rusqlite::params![audit_id, spec_id, permission, decision, now_ms],
        )
        .map_err(|e| anyhow::anyhow!("audit log insert: {}", e))?;

        tracing::info!(
            "[AppRuntimeService] permission '{}' {} for spec {}",
            permission,
            if granted { "granted" } else { "denied" },
            spec_id
        );
        Ok(())
    }

    /// Enable or disable a spec (activate/deactivate subscriptions + UPDATE column).
    pub async fn set_enabled(&self, spec_id: &str, enabled: bool) -> anyhow::Result<()> {
        if enabled {
            self.activate(spec_id).await?;
        } else {
            self.deactivate(spec_id).await?;
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
        conn.execute(
            "UPDATE automation_specs SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![spec_id, enabled as i64, now_ms],
        )
        .map_err(|e| anyhow::anyhow!("update enabled: {}", e))?;
        Ok(())
    }

    /// Deactivate subscriptions then hard-delete the spec (CASCADE handles child rows).
    pub async fn uninstall(&self, spec_id: &str) -> anyhow::Result<()> {
        let _ = self.deactivate(spec_id).await; // best-effort
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
        conn.execute(
            "DELETE FROM automation_specs WHERE id = ?1",
            rusqlite::params![spec_id],
        )
        .map_err(|e| anyhow::anyhow!("delete spec: {}", e))?;
        tracing::info!("[AppRuntimeService] spec {} uninstalled", spec_id);
        Ok(())
    }

    /// Trigger a manual run.
    pub async fn trigger_manual(&self, spec_id: &str) -> anyhow::Result<()> {
        self.execute_run(spec_id, None, serde_json::json!({"trigger": "manual"}))
            .await
    }

    /// List activity rows for a spec, newest first.
    pub fn get_activity(
        &self,
        spec_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<AutomationActivity>> {
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
        crate::automation::activity::list_activities_for_spec(&conn, spec_id, limit as u32)
            .map_err(|e| anyhow::anyhow!("list activity: {}", e))
    }

    /// List escalation rows with status='waiting', optionally filtered to one spec.
    pub fn list_pending_escalations(
        &self,
        spec_id: Option<&str>,
    ) -> anyhow::Result<Vec<EscalationRow>> {
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
        if let Some(sid) = spec_id {
            let mut stmt = conn.prepare(
                "SELECT id, spec_id, activity_id, question, choices_json,
                        status, user_choice, user_note, created_at, responded_at
                 FROM automation_escalations
                 WHERE status = 'waiting' AND spec_id = ?1
                 ORDER BY created_at DESC",
            ).map_err(|e| anyhow::anyhow!("prepare: {}", e))?;
            let rows = stmt
                .query_map(rusqlite::params![sid], Self::row_to_escalation)
                .map_err(|e| anyhow::anyhow!("query: {}", e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| anyhow::anyhow!("row: {}", e))?;
            Ok(rows)
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, spec_id, activity_id, question, choices_json,
                        status, user_choice, user_note, created_at, responded_at
                 FROM automation_escalations
                 WHERE status = 'waiting'
                 ORDER BY created_at DESC",
            ).map_err(|e| anyhow::anyhow!("prepare: {}", e))?;
            let rows = stmt
                .query_map([], Self::row_to_escalation)
                .map_err(|e| anyhow::anyhow!("query: {}", e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| anyhow::anyhow!("row: {}", e))?;
            Ok(rows)
        }
    }

    /// Read the current memory document for a spec.
    pub async fn read_memory(&self, spec_id: &str) -> anyhow::Result<String> {
        self.memory
            .read(spec_id)
            .await
            .map_err(Into::into)
    }

    /// Archive current memory and return the archive path.
    pub async fn compact_memory(&self, spec_id: &str) -> anyhow::Result<String> {
        let path = self.memory.compact(spec_id).await?;
        Ok(path.to_string_lossy().into_owned())
    }

    // ── row mappers ─────────────────────────────────────────────────────────

    fn row_to_spec_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<HumaneSpecRow> {
        Ok(HumaneSpecRow {
            id:                  r.get(0)?,
            name:                r.get(1)?,
            version:             r.get(2)?,
            author:              r.get(3)?,
            description:         r.get(4)?,
            system_prompt:       r.get(5)?,
            spec_format:         r.get(6)?,
            spec_yaml:           r.get(7)?,
            spec_json:           r.get(8)?,
            user_config_values:  r.get(9)?,
            permissions_granted: r.get(10)?,
            permissions_denied:  r.get(11)?,
            status:              r.get(12)?,
            enabled: {
                let v: i64 = r.get(13)?;
                v != 0
            },
            space_id:            r.get(14)?,
            source:              r.get(15)?,
            source_ref:          r.get(16)?,
            source_version:      r.get(17)?,
            created_at:          r.get(18)?,
            updated_at:          r.get(19)?,
            last_run_at:         r.get(20)?,
            last_run_outcome:    r.get(21)?,
            trigger_phrase:          r.get(22)?,
            system_prompt_override:  r.get(23)?,
        })
    }

    fn row_to_escalation(r: &rusqlite::Row<'_>) -> rusqlite::Result<EscalationRow> {
        Ok(EscalationRow {
            id:           r.get(0)?,
            spec_id:      r.get(1)?,
            activity_id:  r.get(2)?,
            question:     r.get(3)?,
            choices_json: r.get(4)?,
            status:       r.get(5)?,
            user_choice:  r.get(6)?,
            user_note:    r.get(7)?,
            created_at:   r.get(8)?,
            responded_at: r.get(9)?,
        })
    }

    // ── helpers ─────────────────────────────────────────────────────────────

    fn update_activity_status(
        &self,
        activity_id: &str,
        status: &str,
        error_text: Option<&str>,
    ) -> anyhow::Result<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
        conn.execute(
            "UPDATE automation_activities
             SET status = ?2, error_text = ?3, completed_at = ?4
             WHERE id = ?1",
            rusqlite::params![activity_id, status, error_text, now_ms],
        )
        .map_err(|e| anyhow::anyhow!("activity status update: {}", e))?;
        Ok(())
    }

    // ── run-pipeline helpers (Phase 2a, design §D4) ─────────────────────────

    /// Load the resolved [`PermissionSet`] for a spec: the spec's declared
    /// permissions (`HumaneAutomationSpec.permissions`) plus the user-level
    /// `permissions_granted` / `permissions_denied` overrides stored as JSON
    /// string arrays in `automation_specs`. Mirrors the granted/denied parse
    /// in [`Self::set_permission`].
    fn load_permission_set(&self, spec_id: &str) -> anyhow::Result<PermissionSet> {
        let (spec_json, pg_str, pd_str): (String, String, String) = {
            let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
            conn.query_row(
                "SELECT spec_json, permissions_granted, permissions_denied
                 FROM automation_specs WHERE id = ?1",
                rusqlite::params![spec_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| anyhow::anyhow!("spec not found '{}': {}", spec_id, e))?
        };

        let spec = Self::parse_humane_spec(&spec_json)?;
        let granted_strs: Vec<String> = serde_json::from_str(&pg_str)
            .map_err(|e| anyhow::anyhow!("parse permissions_granted: {}", e))?;
        let denied_strs: Vec<String> = serde_json::from_str(&pd_str)
            .map_err(|e| anyhow::anyhow!("parse permissions_denied: {}", e))?;

        // Permission derives Deserialize with #[serde(other)] => unknown
        // strings parse to Permission::Unknown rather than erroring.
        let to_perms = |strs: &[String]| -> Vec<Permission> {
            strs.iter()
                .map(|s| {
                    serde_json::from_value(serde_json::Value::String(s.clone()))
                        .unwrap_or(Permission::Unknown)
                })
                .collect()
        };

        Ok(PermissionSet {
            spec: spec.permissions.clone(),
            granted: to_perms(&granted_strs),
            denied: to_perms(&denied_strs),
        })
    }

    /// Like `execute_run`, but with IM close-loop handles injected into the HeadlessDelegate.
    ///
    /// For now: calls `execute_run` with the reply handles unused — the IM close-loop
    /// for automation specs will be wired in a follow-up PR. The agent-chat path
    /// (`run_agent_chat_via_im` in `channels/dispatcher.rs`) is fully wired.
    /// Run a spec inside its per-(spec, identity) chat session.
    ///
    /// Phase 2b cluster A entry point. Replaces per-fire `automation:scheduled`
    /// sessions for autonomous triggers and consolidates IM-triggered runs
    /// into per-user threads.
    ///
    /// Resolves the chat session id via `get_or_create_chat_session`, acquires
    /// a per-session mutex so burst messages serialize (the agent loop is not
    /// interruptible), then stashes the I/O handles for `execute_run` to drain
    /// when building the `HeadlessDelegate`. Returns when `execute_run` finishes.
    pub async fn execute_run_in_chat_session(
        &self,
        spec_id: &str,
        identity_key: &str,
        payload: serde_json::Value,
        streaming_handle: Option<Arc<dyn crate::channels::types::StreamingHandle>>,
        reply_handle: Option<Arc<crate::channels::types::ReplyHandle>>,
        app_handle: Option<tauri::AppHandle>,
    ) -> anyhow::Result<()> {
        // Resolve the chat session id (or create one). Look up the spec's
        // space so the agent_session is filed under it.
        let session_id = {
            let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
            let space_id = run_session::resolve_home_space(&conn, spec_id)
                .map_err(|e| anyhow::anyhow!("resolve home space: {e}"))?;
            crate::automation::runtime::chat_sessions::get_or_create_chat_session(
                &conn,
                spec_id,
                identity_key,
                &space_id,
            )?
        };

        // Acquire per-session mutex so burst messages queue rather than race
        // the agent loop.
        let lock = self.get_or_create_chat_lock(&session_id).await;
        let _guard = lock.lock().await;

        // Stash handles for execute_run to drain when building the delegate.
        {
            let mut slot = self.pending_chat_handles.lock().await;
            slot.insert(
                session_id.clone(),
                ChatHandleBundle {
                    streaming: streaming_handle,
                    reply: reply_handle,
                    app: app_handle,
                },
            );
        }

        // Pin the chat session via payload hint. execute_run reads
        // `_chat_session_id` to skip create_run_session and reuse this id.
        let mut payload_with_chat = payload;
        if let Some(obj) = payload_with_chat.as_object_mut() {
            obj.insert(
                "_chat_session_id".to_string(),
                serde_json::Value::String(session_id.clone()),
            );
        }

        let result = self.execute_run(spec_id, None, payload_with_chat).await;

        // Clean up any leftover stash in case execute_run failed before
        // draining (e.g., very early error).
        {
            let mut slot = self.pending_chat_handles.lock().await;
            slot.remove(&session_id);
        }

        result
    }

    /// Get or lazily create the per-chat-session mutex used by
    /// `execute_run_in_chat_session` to serialize burst messages.
    async fn get_or_create_chat_lock(&self, session_id: &str) -> Arc<TokioMutex<()>> {
        let mut map = self.chat_session_locks.lock().await;
        map.entry(session_id.to_string())
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }

    /// Build the headless tool set for an automation run: the AppHandle-free
    /// base built-in tools rooted at `workspace_root`, plus the automation
    /// schema tools (`report_to_user`, `notify_user`, `request_escalation`,
    /// `memory`).
    ///
    /// The interactive-chat tools that require a Tauri `AppHandle`
    /// (`ask_user`, `exit_plan_mode`, `plan`, `self_eval`, `skill_search`,
    /// `load_skill`, browser tools) are intentionally excluded — a headless
    /// automation run has no window to drive them.
    pub fn build_automation_tool_registry(
        &self,
        workspace_root: &std::path::Path,
    ) -> Arc<crate::agent::tools::tool::ToolRegistry> {
        crate::automation::runtime::tool_registry::build_base_registry(
            crate::automation::runtime::tool_registry::AutomationToolRegistryDeps {
                workspace_root: workspace_root.to_path_buf(),
                spec_permissions: Vec::new(),
                gbrain_declared: false,
            },
        )
    }

    /// Resolve the LLM provider + model id for an automation run from the
    /// app's [`ProviderService`]. Mirrors the chat send-message path's
    /// `get_active_llm_config` → `llm_config_from_provider` → `create_provider`
    /// chain. Errors (no active model / no API key) propagate so the caller
    /// can mark the run `failed` gracefully.
    async fn resolve_run_provider(
        &self,
    ) -> anyhow::Result<(Arc<dyn crate::llm::LlmProvider>, String)> {
        let (provider_id, model, api_key, base_url) = self
            .provider_service
            .get_active_llm_config()
            .await
            .ok_or_else(|| anyhow::anyhow!("no active LLM model configured"))?;
        // 8192 / 0.7 mirror the chat path's defaults; automation has no
        // per-message override surface.
        let llm_config =
            crate::llm::llm_config_from_provider(&provider_id, &model, &api_key, &base_url, 8192, 0.7);
        if llm_config.api_key.is_empty() && llm_config.provider != "ollama" {
            anyhow::bail!("no API key configured for provider '{}'", provider_id);
        }
        let llm = crate::llm::create_provider(&llm_config)
            .map_err(|e| anyhow::anyhow!("create provider: {}", e))?;
        Ok((llm, model))
    }

    /// Load all spec IDs marked `enabled = 1` from the DB.
    fn load_enabled_spec_ids(&self) -> anyhow::Result<Vec<String>> {
        let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT id FROM automation_specs WHERE enabled = 1")
            .map_err(|e| anyhow::anyhow!("prepare: {}", e))?;
        let ids: rusqlite::Result<Vec<String>> = stmt
            .query_map([], |r| r.get(0))
            .map_err(|e| anyhow::anyhow!("query: {}", e))?
            .collect();
        ids.map_err(|e| anyhow::anyhow!("row: {}", e))
    }

    /// Returns a `Weak<Self>` for use in async callbacks so they don't prevent
    /// shutdown.  The weak ref is seeded during `new()` via `OnceLock`; no
    /// raw-pointer trickery required.
    fn weak_ref(&self) -> Weak<AppRuntimeService> {
        self.self_weak
            .get()
            .expect("AppRuntimeService must be constructed via AppRuntimeService::new()")
            .clone()
    }
}

// ─── ManagedService impl ──────────────────────────────────────────────────────

#[async_trait]
impl ManagedService for AppRuntimeService {
    fn name(&self) -> &str {
        "AppRuntimeService"
    }

    async fn start(&self) -> anyhow::Result<()> {
        {
            let mut s = self.status.lock().unwrap();
            *s = ServiceStatus::Starting;
        }

        let spec_ids = match self.load_enabled_spec_ids() {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!("[AppRuntimeService] start: failed to load spec ids: {}", e);
                vec![]
            }
        };

        tracing::info!(
            "[AppRuntimeService] start: activating {} enabled spec(s)",
            spec_ids.len()
        );

        for spec_id in &spec_ids {
            if let Err(e) = self.activate(spec_id).await {
                tracing::warn!(
                    "[AppRuntimeService] start: activate failed for spec {}: {}",
                    spec_id, e
                );
            }
        }

        {
            let mut s = self.status.lock().unwrap();
            *s = ServiceStatus::Running;
        }
        *self.started_at.lock().unwrap() = Some(Instant::now());
        tracing::info!("[AppRuntimeService] started");
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        {
            let mut s = self.status.lock().unwrap();
            *s = ServiceStatus::Stopping;
        }

        let spec_ids: Vec<String> = self
            .attached
            .lock()
            .await
            .keys()
            .cloned()
            .collect();

        tracing::info!(
            "[AppRuntimeService] stop: deactivating {} spec(s)",
            spec_ids.len()
        );

        for spec_id in &spec_ids {
            if let Err(e) = self.deactivate(spec_id).await {
                tracing::warn!(
                    "[AppRuntimeService] stop: deactivate failed for spec {}: {}",
                    spec_id, e
                );
            }
        }

        {
            let mut s = self.status.lock().unwrap();
            *s = ServiceStatus::Stopped;
        }
        tracing::info!("[AppRuntimeService] stopped");
        Ok(())
    }

    fn status(&self) -> ServiceStatus {
        self.status.lock().unwrap().clone()
    }

    fn health(&self) -> ServiceHealth {
        let status = self.status();
        let uptime_secs = self
            .started_at
            .lock()
            .unwrap()
            .map(|t| t.elapsed().as_secs());

        ServiceHealth {
            name: "AppRuntimeService".into(),
            status,
            uptime_secs,
            last_error: None,
            metrics: serde_json::json!({
                "active_specs": self.attached.try_lock()
                    .map(|a| a.len())
                    .unwrap_or(0),
            }),
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ── DB bootstrap helpers ─────────────────────────────────────────────────

    fn open_test_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // Run the full migration stack so the schema is always up to date,
        // including V24 which adds session_id / report_artifacts_json and
        // drops tool_calls_json from automation_activities.
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn insert_test_spec(conn: &rusqlite::Connection, id: &str, spec_json: &str) {
        conn.execute(
            "INSERT INTO automation_specs
             (id, name, version, author, description, system_prompt,
              spec_yaml, spec_json, enabled, created_at, updated_at)
             VALUES (?1,'test','0.1.0','test','test','sys','type: automation',?2,1,1,1)",
            rusqlite::params![id, spec_json],
        )
        .unwrap();
    }

    fn minimal_spec_json() -> &'static str {
        r#"{
            "type": "automation",
            "name": "test",
            "version": "0.1.0",
            "author": "test",
            "description": "test",
            "system_prompt": "you are a test agent",
            "subscriptions": []
        }"#
    }

    fn make_service(conn: rusqlite::Connection) -> Arc<AppRuntimeService> {
        let db = Arc::new(StdMutex::new(conn));
        let tmp = std::env::temp_dir().join(format!("uclaw-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let memory_root = tmp.join("automation_memory");
        std::fs::create_dir_all(&memory_root).unwrap();
        AppRuntimeService::new(
            db,
            Arc::new(ScheduleSource::new()),
            Arc::new(FileSource::new()),
            Arc::new(WebhookSource::with_global_registry()),
            Arc::new(WebpageSource::new()),
            Arc::new(RssSource::new()),
            Arc::new(WecomSource::new()),
            Arc::new(CustomSource::new()),
            Arc::new(InfraService::new()),
            Arc::new(crate::automation::memory::MemoryStore::new(memory_root)),
            Arc::new(ProviderService::new(&tmp).expect("test provider service")),
            None, // app_handle not available in tests
            None, // channel_manager not available in tests
        )
    }

    // ── Test 1: activate with empty subscriptions succeeds ───────────────────

    #[tokio::test]
    async fn activate_with_no_subscriptions_succeeds() {
        let conn = open_test_db();
        insert_test_spec(&conn, "s1", minimal_spec_json());
        let svc = make_service(conn);

        svc.activate("s1").await.unwrap();

        let attached = svc.attached.lock().await;
        // Spec is tracked in the map…
        assert!(attached.contains_key("s1"));
        // …with no subscription entries (spec has subscriptions: []).
        assert!(attached["s1"].is_empty());
    }

    // ── Test 2: execute_run inserts activity row ─────────────────────────────

    #[tokio::test]
    async fn execute_run_inserts_activity_row() {
        let conn = open_test_db();
        insert_test_spec(&conn, "s2", minimal_spec_json());
        let svc = make_service(conn);

        svc.activate("s2").await.unwrap();
        svc.execute_run("s2", None, serde_json::json!({"key": "val"}))
            .await
            .unwrap();

        let row: (String, String) = {
            let db = svc.db.lock().unwrap();
            db.query_row(
                "SELECT id, status FROM automation_activities WHERE spec_id = 's2'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };

        // execute_run now invokes the real pipeline — the Phase 1
        // `deferred_phase_2` stub is gone. With a bare-temp-dir
        // ProviderService (no API key), the run ends `failed` after
        // provider resolution, but the ledger row is always inserted.
        assert!(!row.0.is_empty(), "activity row id must be set");
        assert_ne!(row.1, "deferred_phase_2", "stub status must be gone");
        assert_eq!(row.1, "failed", "no-API-key run must land failed");
    }

    // ── Test 2b: execute_run creates a run-session and links the activity ────

    #[tokio::test]
    async fn execute_run_creates_run_session_and_links_activity() {
        let conn = open_test_db();
        insert_test_spec(&conn, "rs", minimal_spec_json());
        let svc = make_service(conn);
        svc.activate("rs").await.unwrap();
        svc.execute_run("rs", None, serde_json::json!({"trigger": "manual"}))
            .await
            .unwrap();

        let (status, session_id): (String, Option<String>) = {
            let db = svc.db.lock().unwrap();
            db.query_row(
                "SELECT status, session_id FROM automation_activities WHERE spec_id = 'rs'",
                [], |r| Ok((r.get(0)?, r.get(1)?)),
            ).unwrap()
        };
        assert_ne!(status, "deferred_phase_2", "execute_run must invoke the loop");
        assert_eq!(status, "failed", "no-API-key run must land failed");
        assert!(session_id.is_some(), "run-session must be created and linked");
    }

    // ── Test 3: filter rejection marks activity filtered_out ─────────────────

    #[tokio::test]
    async fn execute_run_filters_out_when_filter_rejects() {
        // Build a spec with a filter that requires /event/branch == "main".
        let spec_with_filter = r#"{
            "type": "automation",
            "name": "filtered",
            "version": "0.1.0",
            "author": "test",
            "description": "test",
            "system_prompt": "sys",
            "subscriptions": [],
            "filters": [
                { "field": "/event/branch", "op": "eq", "value": "main" }
            ]
        }"#;

        let conn = open_test_db();
        insert_test_spec(&conn, "s3", spec_with_filter);
        let svc = make_service(conn);

        svc.activate("s3").await.unwrap();

        // Fire with branch = "feature" → filter should reject.
        svc.execute_run("s3", None, serde_json::json!({"branch": "feature"}))
            .await
            .unwrap();

        let status: String = {
            let db = svc.db.lock().unwrap();
            db.query_row(
                "SELECT status FROM automation_activities WHERE spec_id = 's3'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };

        // New Phase 1 status value.
        assert_eq!(status, "filtered_out");
    }

    // ── §7.3 Tests ───────────────────────────────────────────────────────────

    fn minimal_yaml() -> &'static str {
        "type: automation\nname: test-spec\nversion: 0.1.0\nauthor: tester\ndescription: a test\nsystem_prompt: you are a test agent\n"
    }

    // ── Test §7.3-1: install_humane_spec_persists_row ───────────────────────

    #[tokio::test]
    async fn install_humane_spec_persists_row() {
        let conn = open_test_db();
        let svc = make_service(conn);

        let row = svc.install_humane_spec(minimal_yaml(), None).await.unwrap();
        assert_eq!(row.name, "test-spec");
        assert_eq!(row.version, "0.1.0");
        assert_eq!(row.author, "tester");
        assert!(row.enabled);
        assert_eq!(row.source, "local");
        assert_eq!(row.permissions_granted, "[]");
        assert_eq!(row.permissions_denied, "[]");
    }

    // ── Test §7.3-1b: install_humane_spec_from_source_sets_marketplace_source ─

    #[tokio::test]
    async fn install_humane_spec_from_source_sets_marketplace_source() {
        let conn = open_test_db();
        let svc = make_service(conn);

        let row = svc
            .install_humane_spec_from_source(
                minimal_yaml(),
                "marketplace",
                Some("marketplace://halo/test-spec".into()),
            )
            .await
            .unwrap();
        assert_eq!(row.source, "marketplace");
        assert_eq!(row.source_ref.as_deref(), Some("marketplace://halo/test-spec"));
    }

    // ── Test §7.3-1c: install_humane_spec_legacy_path_still_returns_local ──

    #[tokio::test]
    async fn install_humane_spec_legacy_path_still_returns_local() {
        let conn = open_test_db();
        let svc = make_service(conn);

        let row = svc.install_humane_spec(minimal_yaml(), None).await.unwrap();
        assert_eq!(row.source, "local");
    }

    // ── Test §7.3-2: list_specs_returns_inserted_specs_in_order ────────────

    #[tokio::test]
    async fn list_specs_returns_inserted_specs_in_order() {
        let conn = open_test_db();
        let svc = make_service(conn);

        let yaml_a = "type: automation\nname: spec-a\nversion: 0.1.0\nauthor: t\ndescription: a\nsystem_prompt: s\n";
        let yaml_b = "type: automation\nname: spec-b\nversion: 0.2.0\nauthor: t\ndescription: b\nsystem_prompt: s\n";

        svc.install_humane_spec(yaml_a, None).await.unwrap();
        // Ensure b has a larger created_at
        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        svc.install_humane_spec(yaml_b, None).await.unwrap();

        let rows = svc.list_specs().unwrap();
        assert_eq!(rows.len(), 2);
        // DESC order: b first
        assert_eq!(rows[0].name, "spec-b");
        assert_eq!(rows[1].name, "spec-a");
    }

    // ── Test §7.3-3: update_user_config_writes_json ─────────────────────────

    #[tokio::test]
    async fn update_user_config_writes_json() {
        let conn = open_test_db();
        let svc = make_service(conn);

        let row = svc.install_humane_spec(minimal_yaml(), None).await.unwrap();
        let vals = serde_json::json!({"key": "val", "num": 42});
        svc.update_user_config(&row.id, &vals).unwrap();

        let loaded = svc.get_spec(&row.id).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&loaded.user_config_values).unwrap();
        assert_eq!(parsed["key"], "val");
        assert_eq!(parsed["num"], 42);
    }

    // ── Test §7.3-4: set_permission_grants_and_denies ───────────────────────

    #[tokio::test]
    async fn set_permission_grants_and_denies() {
        let conn = open_test_db();
        let svc = make_service(conn);

        let row = svc.install_humane_spec(minimal_yaml(), None).await.unwrap();

        // Grant "read_file"
        svc.set_permission(&row.id, "read_file", true).await.unwrap();
        let r1 = svc.get_spec(&row.id).unwrap();
        let pg: Vec<String> = serde_json::from_str(&r1.permissions_granted).unwrap();
        assert!(pg.contains(&"read_file".to_string()));

        // Deny "shell_exec" → moves to denied
        svc.set_permission(&row.id, "shell_exec", false).await.unwrap();
        let r2 = svc.get_spec(&row.id).unwrap();
        let pd: Vec<String> = serde_json::from_str(&r2.permissions_denied).unwrap();
        assert!(pd.contains(&"shell_exec".to_string()));

        // Flip "read_file" to denied → removed from granted
        svc.set_permission(&row.id, "read_file", false).await.unwrap();
        let r3 = svc.get_spec(&row.id).unwrap();
        let pg3: Vec<String> = serde_json::from_str(&r3.permissions_granted).unwrap();
        assert!(!pg3.contains(&"read_file".to_string()));
    }

    // ── Test §7.3-5: set_enabled_toggles_db_column ──────────────────────────

    #[tokio::test]
    async fn set_enabled_toggles_db_column() {
        let conn = open_test_db();
        insert_test_spec(&conn, "en-spec", minimal_spec_json());
        let svc = make_service(conn);

        // Activate so deactivate doesn't error
        svc.activate("en-spec").await.unwrap();
        svc.set_enabled("en-spec", false).await.unwrap();
        let row = svc.get_spec("en-spec").unwrap();
        assert!(!row.enabled);

        // Re-enable — activate skips because spec is disabled, set_enabled just updates the column
        {
            let c = svc.db.lock().unwrap();
            c.execute("UPDATE automation_specs SET enabled=1 WHERE id='en-spec'", []).unwrap();
        }
        svc.set_enabled("en-spec", true).await.unwrap();
        let row2 = svc.get_spec("en-spec").unwrap();
        assert!(row2.enabled);
    }

    // ── Test §7.3-6: uninstall_removes_spec_and_cascades ────────────────────

    #[tokio::test]
    async fn uninstall_removes_spec_and_cascades() {
        let conn = open_test_db();
        insert_test_spec(&conn, "del-spec", minimal_spec_json());
        let svc = make_service(conn);

        svc.activate("del-spec").await.unwrap();

        // Insert a child activity to verify CASCADE
        {
            let c = svc.db.lock().unwrap();
            c.execute(
                "INSERT INTO automation_activities
                 (id, spec_id, trigger_source_type, trigger_payload_json,
                  status, queued_at, duration_ms, llm_iterations,
                  llm_tokens_in, llm_tokens_out)
                 VALUES ('act-del','del-spec','manual','{}','queued',1,0,0,0,0)",
                [],
            ).unwrap();
        }

        svc.uninstall("del-spec").await.unwrap();

        let result = svc.get_spec("del-spec");
        assert!(result.is_err(), "spec should be gone after uninstall");

        // CASCADE: activity should also be gone
        let count: i64 = {
            let c = svc.db.lock().unwrap();
            c.query_row(
                "SELECT COUNT(*) FROM automation_activities WHERE spec_id='del-spec'",
                [], |r| r.get(0),
            ).unwrap()
        };
        assert_eq!(count, 0);
    }

    // ── Test §7.3-7: list_pending_escalations_filters_by_status ─────────────

    #[tokio::test]
    async fn list_pending_escalations_filters_by_status() {
        let conn = open_test_db();
        insert_test_spec(&conn, "esc-spec", minimal_spec_json());

        let act_id = "act-esc-pend";
        {
            conn.execute(
                "INSERT INTO automation_activities
                 (id, spec_id, trigger_source_type, trigger_payload_json,
                  status, queued_at, duration_ms, llm_iterations,
                  llm_tokens_in, llm_tokens_out)
                 VALUES (?1,'esc-spec','manual','{}','running',1,0,0,0,0)",
                rusqlite::params![act_id],
            ).unwrap();

            // Two escalations: one waiting, one resolved
            conn.execute(
                "INSERT INTO automation_escalations
                 (id, spec_id, activity_id, question, choices_json, status, created_at)
                 VALUES ('esc-wait','esc-spec',?1,'approve?','[\"yes\",\"no\"]','waiting',1)",
                rusqlite::params![act_id],
            ).unwrap();
            conn.execute(
                "INSERT INTO automation_escalations
                 (id, spec_id, activity_id, question, choices_json, status, created_at)
                 VALUES ('esc-done','esc-spec',?1,'done?','[\"ok\"]','resolved',2)",
                rusqlite::params![act_id],
            ).unwrap();
        }

        let svc = make_service(conn);

        let pending = svc.list_pending_escalations(None).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "esc-wait");

        // Filter by spec_id
        let pending2 = svc.list_pending_escalations(Some("esc-spec")).unwrap();
        assert_eq!(pending2.len(), 1);

        // Filter by different spec → empty
        let pending3 = svc.list_pending_escalations(Some("other-spec")).unwrap();
        assert!(pending3.is_empty());
    }

    // ── Test §7.3-8: compact_memory_returns_archive_path ────────────────────

    #[tokio::test]
    async fn compact_memory_returns_archive_path() {
        let conn = open_test_db();
        let svc = make_service(conn);

        // Write some memory first
        svc.memory.write("mem-spec", "important context").await.unwrap();
        assert_eq!(svc.memory.read("mem-spec").await.unwrap(), "important context");

        let path = svc.compact_memory("mem-spec").await.unwrap();
        assert!(!path.is_empty(), "archive path should be non-empty");

        // After compact, current memory is empty
        let after = svc.read_memory("mem-spec").await.unwrap();
        assert!(after.is_empty(), "memory should be empty after compact");
    }

    // ── Test 4: resolve_escalation updates the row ───────────────────────────

    #[tokio::test]
    async fn resolve_escalation_updates_row() {
        let conn = open_test_db();
        insert_test_spec(&conn, "s4", minimal_spec_json());

        // We need an activity row first (FK constraint).
        let activity_id = "act-esc-1";
        {
            conn.execute(
                "INSERT INTO automation_activities
                 (id, spec_id, trigger_source_type, trigger_payload_json,
                  status, queued_at, duration_ms, llm_iterations,
                  llm_tokens_in, llm_tokens_out)
                 VALUES (?1,'s4','manual','{}','running',1,0,0,0,0)",
                rusqlite::params![activity_id],
            )
            .unwrap();

            // Insert an escalation row with status='waiting'.
            conn.execute(
                "INSERT INTO automation_escalations
                 (id, spec_id, activity_id, question, choices_json, status, created_at)
                 VALUES ('esc-1','s4',?1,'approve?','[\"yes\",\"no\"]','waiting',1)",
                rusqlite::params![activity_id],
            )
            .unwrap();
        }

        let svc = make_service(conn);

        svc.resolve_escalation("esc-1", "yes", Some("looks good"))
            .await
            .unwrap();

        let (status, choice, note): (String, String, String) = {
            let db = svc.db.lock().unwrap();
            db.query_row(
                "SELECT status, user_choice, user_note FROM automation_escalations WHERE id='esc-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
        };

        assert_eq!(status, "resolved");
        assert_eq!(choice, "yes");
        assert_eq!(note, "looks good");
    }

    // ── Phase 2b cluster A · chat session lock map ──────────────────────────

    #[tokio::test]
    async fn chat_session_locks_returns_same_arc_for_same_session() {
        let svc = make_service(open_test_db());
        let a = svc.get_or_create_chat_lock("sess-x").await;
        let b = svc.get_or_create_chat_lock("sess-x").await;
        // Same session id → same Arc<Mutex<()>>.
        assert!(
            Arc::ptr_eq(&a, &b),
            "second call for same session id must return the same Arc"
        );
    }

    #[tokio::test]
    async fn chat_session_locks_returns_distinct_arcs_for_different_sessions() {
        let svc = make_service(open_test_db());
        let a = svc.get_or_create_chat_lock("sess-1").await;
        let b = svc.get_or_create_chat_lock("sess-2").await;
        assert!(
            !Arc::ptr_eq(&a, &b),
            "different session ids must get distinct Arcs"
        );
    }

    #[tokio::test]
    async fn chat_session_locks_serialize_concurrent_holders() {
        // Two tasks try to hold the same session's lock. Verify second
        // blocks until first releases. Loose lower bound on serialized
        // wall time vs. the parallel case.
        let svc = make_service(open_test_db());
        let lock = svc.get_or_create_chat_lock("sess-burst").await;
        let lock_clone = lock.clone();

        let start = std::time::Instant::now();
        let t1 = tokio::spawn(async move {
            let _g = lock.lock().await;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        });
        let t2 = tokio::spawn(async move {
            // Brief yield so t1 grabs the lock first.
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let _g = lock_clone.lock().await;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        });
        t1.await.unwrap();
        t2.await.unwrap();
        let elapsed = start.elapsed();

        // Serialized: ≥ 100 + 100 = 200ms. Parallel would be ~110ms.
        assert!(
            elapsed.as_millis() >= 190,
            "expected serialized execution (>=190ms), got {:?}",
            elapsed
        );
    }
}
