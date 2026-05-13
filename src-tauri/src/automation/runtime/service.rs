//! AppRuntimeService — orchestrates spec activation, subscription wiring,
//! activity tracking, filter evaluation, and escalation resolution.
//!
//! # Phase 1 scope
//!
//! `execute_run` implements the full TRIGGER → FILTER → PERSIST pipeline but
//! defers the agentic loop call to Phase 2.  The deliberate stopping point is
//! after filter evaluation: if filters pass we acquire a per-spec semaphore,
//! log the deferred intent, mark the activity row `deferred_phase_2`, and
//! release the semaphore.
//!
//! # Phase 2 deliverable
//!
//! Replace the `deferred_phase_2` block in `execute_run` with a call to
//! `run_agentic_loop(&delegate, &mut ctx, &config)`.  The key architectural
//! question deferred to Phase 2: should `AutomationDelegate::call_llm` be
//! composed with `ChatDelegate`, duplicate its logic, or introduce a new
//! abstraction?  That decision belongs to the Phase 2 design review, NOT here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::{Mutex as TokioMutex, RwLock, Semaphore};

use crate::automation::activity::{
    insert_activity, AutomationActivity, ActivityStatus, TriggerSource,
};
use crate::automation::filters;
use crate::automation::protocol::humane_v1::{HumaneAutomationSpec, Subscription};
use crate::automation::sources::{
    CustomSource, FileSource, RssSource, ScheduleSource, SubscriptionSource,
    TriggerCallback, WebhookSource, WebpageSource, WecomSource,
};
use crate::infra::InfraService;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};

// ─── constants ────────────────────────────────────────────────────────────────

/// Maximum concurrent runs per spec (Phase 1 hard-code; Phase 2 makes this
/// configurable via `HumaneAutomationSpec.config_schema`).
const PER_SPEC_CONCURRENCY: usize = 2;

// ─── AppRuntimeService ────────────────────────────────────────────────────────

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

    /// Per-spec semaphore; inserted lazily on first `activate`.
    semaphores: Arc<RwLock<HashMap<String, Arc<Semaphore>>>>,

    /// Tracks (sub_id, source_type_tag) per spec for clean `deactivate`.
    attached: Arc<TokioMutex<HashMap<String, Vec<(String, String)>>>>,

    status: Arc<StdMutex<ServiceStatus>>,
    started_at: Arc<StdMutex<Option<Instant>>>,

    /// Weak self-reference, set once during `new()` so `weak_ref()` never
    /// needs to touch `Arc` internals via raw pointers.
    self_weak: OnceLock<Weak<AppRuntimeService>>,
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
            semaphores: Arc::new(RwLock::new(HashMap::new())),
            attached: Arc::new(TokioMutex::new(HashMap::new())),
            status: Arc::new(StdMutex::new(ServiceStatus::Stopped)),
            started_at: Arc::new(StdMutex::new(None)),
            self_weak: OnceLock::new(),
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

            // Build the callback that funnels fire events into execute_run.
            let svc = Arc::new(self.weak_ref());
            let sub_id_cb = sub_id.clone();
            let cb: TriggerCallback = Arc::new(move |sid: String, _sub: String, payload: serde_json::Value| {
                let svc = svc.clone();
                let sub_id_inner = sub_id_cb.clone();
                tokio::spawn(async move {
                    if let Some(svc) = svc.upgrade() {
                        if let Err(e) = svc.execute_run(&sid, Some(&sub_id_inner), payload).await {
                            tracing::warn!("[AppRuntimeService] execute_run error for spec {}: {}", sid, e);
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

    /// Core run pipeline (Option A — Phase 1 stub).
    ///
    /// # Phase 2 TODO
    ///
    /// Replace steps 6-7 below with:
    /// ```text
    /// let delegate = AutomationDelegate::new(spec, config, db.clone(), infra.clone());
    /// let mut ctx  = build_initial_message(&spec, &payload_json);
    /// let outcome  = run_agentic_loop(&delegate, &mut ctx, &auto_continue_cfg).await;
    /// match outcome {
    ///     LoopOutcome::Success { report } => mark completed + store report,
    ///     LoopOutcome::Failure { error }  => mark failed + store error,
    ///     LoopOutcome::Escalated { esc }  => insert escalation row + mark waiting_user,
    /// }
    /// ```
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
            tool_calls_json:             "[]".into(),
            report_text:                 None,
            report_outcome:              None,
            escalation_id:               None,
            resumed_from_activity_id:    None,
            resumed_from_escalation_id:  None,
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

        // ── 6-7. Phase 1 stub: log deferred intent + mark activity ───────────
        // TODO(Phase 2): replace these two steps with run_agentic_loop call.
        // See module-level doc for the full Phase 2 interface sketch.
        tracing::info!(
            "[AppRuntimeService] phase 1 deferred: would invoke agentic loop for spec={}, activity={}, payload={}",
            spec_id, activity_id,
            serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into()),
        );

        // New status value introduced in Phase 1: `deferred_phase_2`
        self.update_activity_status(&activity_id, "deferred_phase_2", None)?;

        // ── 8. semaphore released (via _permit Drop) ─────────────────────────
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
        conn.execute_batch(crate::db::migrations::V1_INITIAL).unwrap();
        conn.execute_batch(crate::db::migrations::V7_AUTOMATIONS).unwrap();
        crate::db::migrations::run_v20(&conn).unwrap();
        crate::db::migrations::run_v21(&conn).unwrap();
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

        // New Phase 1 status value.
        assert_eq!(row.1, "deferred_phase_2");
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
                  llm_tokens_in, llm_tokens_out, tool_calls_json)
                 VALUES (?1,'s4','manual','{}','running',1,0,0,0,0,'[]')",
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
}
