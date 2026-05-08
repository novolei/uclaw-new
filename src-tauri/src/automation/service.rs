use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use rusqlite::{params, Connection};
use tokio::sync::RwLock;
use crate::agent::types::LoopDelegate;
use super::activity::ActivityStore;
use super::runtime::AutomationRuntime;
use super::spec::{AutomationSpec, AutomationSpecRow, TriggerConfig};

type DelegateFactory = Arc<dyn Fn(String) -> Box<dyn LoopDelegate + Send> + Send + Sync>;

pub struct AutomationService {
    db: Arc<Mutex<Connection>>,
    activity_store: Arc<ActivityStore>,
    /// Set after LLM provider is configured (see set_delegate_factory)
    delegate_factory: RwLock<Option<DelegateFactory>>,
    cron_handles: Arc<RwLock<HashMap<String, tokio::task::AbortHandle>>>,
}

impl AutomationService {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        let activity_store = Arc::new(ActivityStore::new(Arc::clone(&db)));
        Self {
            db,
            activity_store,
            delegate_factory: RwLock::new(None),
            cron_handles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Called after LLM provider initializes. Starts cron schedulers.
    pub async fn set_delegate_factory(&self, factory: DelegateFactory) {
        *self.delegate_factory.write().await = Some(factory);
        self.start_schedulers().await;
    }

    fn make_runtime(&self, factory: DelegateFactory) -> AutomationRuntime {
        AutomationRuntime {
            activity_store: Arc::clone(&self.activity_store),
            delegate_factory: factory,
        }
    }

    /// Install a new automation spec and start its scheduler if enabled.
    pub async fn install(&self, toml_content: &str) -> Result<AutomationSpecRow, String> {
        let spec = AutomationSpec::from_toml(toml_content)?;
        spec.validate()?;

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();

        {
            let conn = self.db.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO automation_specs (id, name, description, toml_content, enabled, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,1,?5,?5)",
                params![id, spec.name, spec.description, toml_content, now],
            ).map_err(|e| e.to_string())?;
        }

        let row = AutomationSpecRow {
            id: id.clone(),
            name: spec.name.clone(),
            description: spec.description.clone(),
            toml_content: toml_content.to_string(),
            enabled: true,
            created_at: now,
            updated_at: now,
        };

        self.schedule_spec(&id, &spec).await;
        Ok(row)
    }

    /// List all installed specs.
    pub fn list(&self) -> Result<Vec<AutomationSpecRow>, String> {
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT id, name, description, toml_content, enabled, created_at, updated_at
             FROM automation_specs ORDER BY created_at DESC"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map([], |row| {
            Ok(AutomationSpecRow {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                toml_content: row.get(3)?,
                enabled: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        }).map_err(|e| e.to_string())?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Trigger a spec manually by ID.
    pub async fn trigger_manual(&self, spec_id: &str) -> Result<(), String> {
        let factory = self.delegate_factory.read().await.clone()
            .ok_or_else(|| "Automation runtime not ready — no LLM provider configured".to_string())?;

        let row = self.get_spec_row(spec_id)?;
        let spec = row.parse_spec()?;
        let runtime = Arc::new(self.make_runtime(factory));
        let spec_id = spec_id.to_string();
        tokio::spawn(async move {
            runtime.run(&spec_id, &spec, "manual").await;
        });
        Ok(())
    }

    /// Get activity history for a spec.
    pub fn get_activity(&self, spec_id: &str, limit: usize) -> Result<Vec<super::activity::AutomationActivity>, String> {
        self.activity_store.list_for_spec(spec_id, limit)
    }

    /// Start cron schedulers for all enabled specs.
    async fn start_schedulers(&self) {
        let rows = match self.list() {
            Ok(r) => r,
            Err(e) => { tracing::error!("AutomationService: failed to list specs: {}", e); return; }
        };
        for row in rows {
            if !row.enabled {
                continue;
            }
            if let Ok(spec) = row.parse_spec() {
                self.schedule_spec(&row.id, &spec).await;
            }
        }
    }

    async fn schedule_spec(&self, spec_id: &str, spec: &AutomationSpec) {
        let cron_expr = match &spec.trigger {
            TriggerConfig::Cron { expression } => expression.clone(),
            _ => return,
        };

        let factory = match self.delegate_factory.read().await.clone() {
            Some(f) => f,
            None => {
                tracing::debug!("AutomationService: skipping cron schedule for '{}' — no factory yet", spec_id);
                return;
            }
        };

        let schedule = match cron_expr.parse::<cron::Schedule>() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Invalid cron '{}': {}", cron_expr, e);
                return;
            }
        };

        let runtime = Arc::new(self.make_runtime(factory));
        let spec_id_owned = spec_id.to_string();
        let spec_owned = spec.clone();

        let handle = tokio::spawn(async move {
            use chrono::Utc;
            for next in schedule.upcoming(Utc) {
                let now = Utc::now();
                let delay = next - now;
                if delay.num_milliseconds() > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay.num_milliseconds() as u64)).await;
                }
                let rt = Arc::clone(&runtime);
                let id = spec_id_owned.clone();
                let sp = spec_owned.clone();
                tokio::spawn(async move { rt.run(&id, &sp, "cron").await; });
            }
        });

        let mut handles = self.cron_handles.write().await;
        if let Some(old) = handles.insert(spec_id.to_string(), handle.abort_handle()) {
            old.abort();
        }
        // Detach the spawned task — we only retain the abort handle
        tokio::spawn(async move { let _ = handle.await; });
    }

    fn get_spec_row(&self, spec_id: &str) -> Result<AutomationSpecRow, String> {
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, name, description, toml_content, enabled, created_at, updated_at
             FROM automation_specs WHERE id=?1",
            params![spec_id],
            |row| Ok(AutomationSpecRow {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                toml_content: row.get(3)?,
                enabled: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            }),
        ).map_err(|e| e.to_string())
    }
}
