use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::Manager;
use sha2::{Digest, Sha256};

use crate::settings::UserSettings;
use crate::agent::session::SessionManager;
use crate::config::LlmConfig;
use crate::notifications::SharedNotificationManager;
use crate::background::SharedBackgroundManager;
use crate::memory::MemoryStore;
use crate::skills::SkillsRegistry;
use crate::mcp::SharedMcpManager;
use crate::channels::ChannelManager;
use crate::providers::service::ProviderService;
use crate::safety::SafetyManager;
use crate::memu::client::MemUClient;
use crate::memu::bridge::MemUBridge;
use crate::memory_graph::store::MemoryGraphStore;
use crate::infra::InfraService;
use crate::proactive::ProactiveService;
use crate::services::ServiceManager;
use crate::observability::MetricsService;
use crate::memubot_config::MemubotConfig;

// ─── Pending Approvals ──────────────────────────────────────────────────

/// Result of an approval decision from the user.
#[derive(Debug, Clone, Default)]
pub struct ApprovalResult {
    pub approved: bool,
    pub always_allow: bool,
    pub tool_name: Option<String>,
    /// Path approval: "once" | "session" | "deny" (only set when payload kind=="path")
    pub path_scope: Option<String>,
    /// Path approval: which absolute paths to grant for "session" scope
    pub paths: Option<Vec<String>>,
}

/// Manages pending tool approval requests using oneshot channels.
/// When a tool needs approval, we store a oneshot::Sender keyed by tool_id.
/// The agent loop awaits on the corresponding Receiver.
/// When the user responds, approve_tool_call sends the result through the Sender.
pub struct PendingApprovals {
    pending: std::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<ApprovalResult>>>,
}

impl PendingApprovals {
    pub fn new() -> Self {
        Self {
            pending: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Register a pending approval and return a receiver to await the result.
    pub fn register(&self, tool_id: String) -> tokio::sync::oneshot::Receiver<ApprovalResult> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().unwrap().insert(tool_id, tx);
        rx
    }

    /// Resolve a pending approval. Returns true if the approval was found and resolved.
    pub fn resolve(&self, tool_id: &str, result: ApprovalResult) -> bool {
        if let Some(tx) = self.pending.lock().unwrap().remove(tool_id) {
            tx.send(result).is_ok()
        } else {
            false
        }
    }
}

/// Result of an ask_user response.
#[derive(Debug, Clone)]
pub struct AskUserResult {
    /// Map of question_index → answer string
    pub answers: std::collections::HashMap<String, serde_json::Value>,
}

/// Manages pending ask_user requests from agent to user.
/// Mirrors PendingApprovals — oneshot per request_id.
pub struct PendingAskUsers {
    pending: std::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<AskUserResult>>>,
}

impl PendingAskUsers {
    pub fn new() -> Self {
        Self { pending: std::sync::Mutex::new(HashMap::new()) }
    }

    pub fn register(&self, request_id: String) -> tokio::sync::oneshot::Receiver<AskUserResult> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().unwrap().insert(request_id, tx);
        rx
    }

    pub fn resolve(&self, request_id: &str, result: AskUserResult) -> bool {
        if let Some(tx) = self.pending.lock().unwrap().remove(request_id) {
            tx.send(result).is_ok()
        } else {
            false
        }
    }
}

/// Decision from the user on an exit_plan_mode request.
#[derive(Debug, Clone)]
pub enum ExitPlanDecision {
    /// User accepted; switch session SafetyMode to Supervised and proceed.
    AcceptAndAuto,
    /// User accepted but wants to stay in Plan; agent may run only the
    /// pre-declared allowed_prompts.
    AcceptKeepPlan,
    /// User rejected; agent receives feedback as tool error.
    Reject { feedback: String },
}

#[derive(Debug, Clone)]
pub struct ExitPlanResult {
    pub decision: ExitPlanDecision,
}

pub struct PendingExitPlans {
    pending: std::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<ExitPlanResult>>>,
}

impl PendingExitPlans {
    pub fn new() -> Self {
        Self { pending: std::sync::Mutex::new(HashMap::new()) }
    }
    pub fn register(&self, request_id: String) -> tokio::sync::oneshot::Receiver<ExitPlanResult> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().unwrap().insert(request_id, tx);
        rx
    }
    pub fn resolve(&self, request_id: &str, result: ExitPlanResult) -> bool {
        if let Some(tx) = self.pending.lock().unwrap().remove(request_id) {
            tx.send(result).is_ok()
        } else {
            false
        }
    }
}

/// Global application state managed by Tauri
pub struct AppState {
    pub data_dir: PathBuf,
    pub config_path: PathBuf,
    pub llm_config_path: PathBuf,
    pub db_path: PathBuf,
    pub workspace_root: PathBuf,
    pub settings: Arc<RwLock<UserSettings>>,
    pub llm_config: Arc<RwLock<LlmConfig>>,
    pub db_ready: bool,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    pub session_manager: Arc<RwLock<SessionManager>>,

    // B0: Infrastructure
    pub notifications: SharedNotificationManager,
    pub background_tasks: SharedBackgroundManager,

    // B2: Infrastructure modules
    pub memory_store: Arc<MemoryStore>,
    pub skills_registry: Arc<RwLock<SkillsRegistry>>,
    pub mcp_manager: SharedMcpManager,
    pub channel_manager: Arc<RwLock<ChannelManager>>,
    pub im_channel_manager: Arc<crate::channels::manager::ImChannelManager>,
    pub im_session_registry: Arc<crate::channels::session_registry::ImSessionRegistry>,

    // Providers
    pub provider_service: Arc<ProviderService>,

    // Safety
    pub safety_manager: Arc<tokio::sync::RwLock<SafetyManager>>,

    // Tool approval
    pub pending_approvals: Arc<PendingApprovals>,

    // ask_user pending requests
    pub pending_ask_users: Arc<PendingAskUsers>,

    // exit_plan_mode pending requests
    pub pending_exit_plans: Arc<PendingExitPlans>,

    // memU memory service (None if Python is unavailable)
    pub memu_client: Option<Arc<MemUClient>>,

    // Memory graph store for Steward memory system
    pub memory_graph_store: Arc<MemoryGraphStore>,

    /// AI Wiki synthesizer — drives `wiki_artifacts(kind="overview")`
    /// regeneration. Memory OS Foundation Phase 3 ships
    /// `wiki_synth::StubSynthesizer` as the default; future PRs swap in
    /// a real Anthropic / OpenAI client without touching the IPC or
    /// scenario code paths.
    pub wiki_synthesizer: Arc<dyn crate::memory_graph::wiki_synth::WikiSynthesizer>,

    // ─── Phased Boot: 新增服务 ───────────────────────────────────────
    /// 中央消息总线
    pub infra_service: Arc<InfraService>,
    /// 服务管理器（统一管理所有后台服务的启停和健康监控）
    pub service_manager: Arc<ServiceManager>,
    /// 指标采集服务
    pub metrics_service: Arc<MetricsService>,
    /// memubot 功能配置（wrapped in RwLock so set_* Tauri commands can mutate + persist）
    pub memubot_config: Arc<tokio::sync::RwLock<MemubotConfig>>,

    /// Active agentic session cancellation tokens, keyed by conversation_id.
    /// Used by stop_agent_session to cancel a running loop.
    pub running_sessions: Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>>,

    /// Browser service for headless Chrome automation
    pub browser_service: Arc<crate::browser::BrowserService>,

    // Evaluation harness
    pub trajectory_store: Arc<crate::harness::TrajectoryStore>,
    pub tool_budget: Arc<crate::harness::ToolBudgetManager>,

    /// Files rail service — owns the filesystem watcher for the WorkspaceRail UI.
    pub files_rail_service: Arc<crate::files_rail::FilesRailService>,

    /// Humane Automation runtime — manages spec activation, subscriptions, and
    /// the § 7.3 command surface.  Registered into ServiceManager in main.rs
    /// Stage 3; constructed here so Tauri commands can borrow it via AppState.
    pub runtime_service: Arc<crate::automation::runtime::AppRuntimeService>,

    /// ProactiveService (None if proactive is disabled or init failed;
    /// populated asynchronously in main.rs Stage 3 via interior RwLock).
    pub proactive_service: Arc<tokio::sync::RwLock<Option<Arc<ProactiveService>>>>,

    /// SymphonyService — third parallel runtime (DAG-of-agent-runs).
    /// `None` until main.rs Stage 3 wires it (gated on
    /// `memubot_config.symphony.enabled`). Follows the same lazy-init shape
    /// as `proactive_service` so Tauri commands can borrow it via `RwLock`.
    pub symphony_service: Arc<tokio::sync::RwLock<Option<Arc<crate::symphony::runtime::service::SymphonyService>>>>,
}

impl AppState {
    pub fn new(app_handle: &tauri::AppHandle) -> Result<Self, crate::error::Error> {
        let data_dir = dirs::home_dir()
            .ok_or_else(|| crate::error::Error::Internal("Cannot find home directory".into()))?
            .join(".uclaw");

        std::fs::create_dir_all(&data_dir).ok();
        tracing::info!(data_dir = %data_dir.display(), "Initializing application state");

        let config_path = data_dir.join("config.json");
        let llm_config_path = data_dir.join("llm_config.json");
        let db_path = data_dir.join("uclaw.db");
        let workspace_root = dirs::document_dir()
            .ok_or_else(|| crate::error::Error::Internal("Cannot find Documents directory".into()))?
            .join("workground");
        std::fs::create_dir_all(&workspace_root).ok();

        let settings = UserSettings::load(&config_path)?;
        let llm_config = LlmConfig::load(&llm_config_path)?;
        let db_ready = db_path.exists();

        // Initialize database and session manager
        let db = Arc::new(std::sync::Mutex::new(
            crate::db::manager::Database::open(&db_path)?.into_inner(),
        ));
        tracing::info!(db_path = %db_path.display(), "Database opened");

        // Run migrations
        if let Ok(conn) = db.lock() {
            if let Err(e) = crate::db::migrations::run(&conn) {
                tracing::error!(error = %e, "DATABASE MIGRATION FAILED — app may be in inconsistent state");
            }
        }

        let session_manager = SessionManager::new(db.clone());

        // B0: Notification manager
        let notifications = Arc::new(tokio::sync::Mutex::new(
            crate::notifications::NotificationManager::new(app_handle.clone()),
        ));

        // B0: Background task manager
        let background_tasks = Arc::new(tokio::sync::Mutex::new(
            crate::background::BackgroundTaskManager::new(),
        ));

        // B2: Memory store
        let memory_store = Arc::new(MemoryStore::new(db.clone()));
        memory_store.ensure_table();

        // Resolve the Tauri resource dir up front — it's used to wire the
        // Bundled skills tier below, and (later in B-Stage) to find the
        // embedded Python runtime.
        let resource_dir = app_handle.path().resource_dir().ok();

        // B2: Skills registry
        //
        // Tier order matters: Bundled → User → Project. Later tiers
        // **shadow** earlier ones on name collision — that's the
        // intentional Fork affordance. A user-authored `tdd` in
        // `~/.uclaw/skills/tdd/SKILL.md` overrides the bundled `tdd` of
        // the same name without the user having to disable the bundled
        // copy first.
        let mut skills_reg = SkillsRegistry::new();

        // Bundled — comes from the Tauri resource dir, which `tauri.conf.json`
        // populates from the repo's top-level `skills/` directory. In dev
        // mode (`cargo tauri dev`) Tauri mirrors the dir into
        // `target/debug/skills/` so it works without a release bundle.
        if let Some(rd) = resource_dir.as_ref() {
            let bundled_skills = rd.join("skills");
            if bundled_skills.exists() {
                tracing::info!(
                    bundled_skills = %bundled_skills.display(),
                    "Registering bundled skills scan dir"
                );
                skills_reg.add_scan_dir(bundled_skills, crate::skills::SkillProvenance::Bundled);
            } else {
                tracing::debug!(
                    expected = %bundled_skills.display(),
                    "No bundled skills dir found in resource dir (running outside a bundle?)"
                );
            }
        }

        // User — survives uClaw upgrades, holds forks and user-authored skills.
        let user_skills_dir = data_dir.join("skills");
        std::fs::create_dir_all(&user_skills_dir).ok();
        skills_reg.add_scan_dir(user_skills_dir, crate::skills::SkillProvenance::User);

        // Marketplace — recovery path: any _marketplace/<slug>/ dirs that exist on disk
        // (e.g. after a backup restore or across cold starts) are registered here so
        // SkillsRegistry stays in sync with the FS even if DB rows were already present.
        let marketplace_root = data_dir.join("skills").join("_marketplace");
        if let Ok(entries) = std::fs::read_dir(&marketplace_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    skills_reg.add_scan_dir(path, crate::skills::SkillProvenance::Marketplace);
                }
            }
        }

        // Project — dev-mode-only fallback. In a bundled app `current_dir()`
        // is the launch dir which won't contain a skills tree, so this is
        // effectively a no-op in production. The Bundled tier covers the
        // production case.
        let project_skills = std::env::current_dir()
            .map(|d| d.join("skills"))
            .unwrap_or_default();
        if project_skills.exists() {
            skills_reg.add_scan_dir(project_skills, crate::skills::SkillProvenance::Project);
        }
        // Initial discovery
        let discovered = skills_reg.discover();
        if !discovered.is_empty() {
            tracing::info!("Discovered {} skill(s) at startup", discovered.len());
        }
        let skills_registry = Arc::new(RwLock::new(skills_reg));

        // B2: MCP manager
        let mcp_manager = Arc::new(RwLock::new(
            crate::mcp::McpManager::new(&data_dir),
        ));

        // B2: Channel manager
        let channel_manager = Arc::new(RwLock::new(ChannelManager::new()));

        // IM channel manager and session registry
        let im_session_registry = Arc::new(
            crate::channels::session_registry::ImSessionRegistry::new(db.clone())
        );
        let im_channel_manager = Arc::new(
            crate::channels::manager::ImChannelManager::new(
                db.clone(),
                im_session_registry.clone(),
                app_handle.clone(),
            )
        );

        // Providers
        let provider_service = Arc::new(ProviderService::new(&data_dir)?);

        // Safety
        let safety_manager = Arc::new(tokio::sync::RwLock::new(SafetyManager::new(&data_dir)));

        // Tool approval
        let pending_approvals = Arc::new(PendingApprovals::new());

        // ask_user pending requests
        let pending_ask_users = Arc::new(PendingAskUsers::new());

        // exit_plan_mode pending requests
        let pending_exit_plans = Arc::new(PendingExitPlans::new());

        // memU integration (degraded mode if Python unavailable). The Tauri
        // resource dir was already resolved above for the Bundled skills tier.
        let memu_client = Self::try_init_memu(&data_dir, resource_dir.as_deref());

        // Eagerly start the memU bridge in a background thread
        if let Some(ref client) = memu_client {
            let eager_client = Arc::clone(client);
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    match eager_client.health_check().await {
                        Ok(status) => tracing::info!("memU bridge health: {}", status),
                        Err(e) => tracing::warn!("memU bridge health check failed: {}; will retry later", e),
                    }
                });
            });
        }

        // Memory graph store
        let memory_graph_store = {
            let store = MemoryGraphStore::new(db.clone());
            store.ensure_tables();
            let mgs = Arc::new(store);
            // Environment memory — scan once at startup for the default space
            crate::memory_graph::environment::persist_environment(
                &mgs,
                "default",
                &workspace_root,
            );
            mgs
        };

        // ─── Stage 1 完成：基础初始化 ─────────────────────────────────

        // Stage 1 补充：加载 MemubotConfig
        let memubot_config = MemubotConfig::load(&data_dir);
        tracing::info!("MemubotConfig loaded (memorization={}, proactive={}, local_api={}, power={})",
            memubot_config.memorization.enabled,
            memubot_config.proactive.enabled,
            memubot_config.local_api.enabled,
            memubot_config.power.prevent_sleep,
        );

        // Memory OS Foundation Phase 2 — apply the auto_link feature flag
        // to the store now that we know what the user configured. The
        // store defaults this to `true` in `MemoryGraphStore::new`; we
        // override here so a user-set `false` in memubot_config.json takes
        // effect from the very first create_version call.
        memory_graph_store.set_auto_link_enabled(memubot_config.memory_os.auto_link_enabled);
        tracing::info!(
            "Memory OS flags applied: entity_page={}, auto_link={}",
            memubot_config.memory_os.entity_page_enabled,
            memubot_config.memory_os.auto_link_enabled,
        );

        // Evaluation harness
        let trajectory_store = Arc::new(crate::harness::TrajectoryStore::new(db.clone()));
        let tool_budget = Arc::new(crate::harness::ToolBudgetManager::new(&data_dir));

        // ─── Stage 2：核心服务 ─────────────────────────────────────────
        let infra_service = Arc::new(InfraService::new());
        tracing::info!("InfraService created");

        let metrics_service = Arc::new(MetricsService::new());
        tracing::info!("MetricsService created");

        let service_manager = Arc::new(ServiceManager::new());
        tracing::info!("ServiceManager created");

        // Files rail service — created here, registered into ServiceManager in main.rs Stage 3.
        let files_rail_service = Arc::new(crate::files_rail::FilesRailService::new(app_handle.clone()));

        // AppRuntimeService — constructed here so it is available to Tauri commands via
        // AppState.  main.rs Stage 3 calls `state.runtime_service.clone()` to register it
        // into ServiceManager (no double-construction needed).
        let automation_memory_root = data_dir.join("automation_memory");
        let _ = std::fs::create_dir_all(&automation_memory_root);
        let runtime_service = {
            use crate::automation::runtime::AppRuntimeService;
            use crate::automation::sources::{
                CustomSource, FileSource, RssSource, ScheduleSource,
                WebhookSource, WebpageSource, WecomSource,
            };
            use crate::automation::memory::MemoryStore as AutomationMemoryStore;
            AppRuntimeService::new(
                db.clone(),
                Arc::new(ScheduleSource::new()),
                Arc::new(FileSource::new()),
                Arc::new(WebhookSource::with_global_registry()),
                Arc::new(WebpageSource::new()),
                Arc::new(RssSource::new()),
                Arc::new(WecomSource::new()),
                Arc::new(CustomSource::new()),
                infra_service.clone(),
                Arc::new(AutomationMemoryStore::new(automation_memory_root)),
                provider_service.clone(),
                Some(app_handle.clone()),
                Some(channel_manager.clone()),
            )
        };

        // ─── Stage 3：注册后台服务到 ServiceManager（在后台异步完成启动）
        // 这些注册操作需要 async，因此在 setup 中通过 spawn 完成。
        // 此处仅构建 AppState，实际注册和启动在 main.rs setup 中执行。

        tracing::info!("Application state initialized successfully (phased boot)");

        Ok(Self {
            data_dir,
            config_path,
            llm_config_path,
            db_path,
            workspace_root,
            settings: Arc::new(RwLock::new(settings)),
            llm_config: Arc::new(RwLock::new(llm_config)),
            db_ready,
            db,
            session_manager: Arc::new(RwLock::new(session_manager)),
            notifications,
            background_tasks,
            memory_store,
            skills_registry,
            mcp_manager,
            channel_manager,
            im_channel_manager,
            im_session_registry,
            provider_service,
            safety_manager,
            pending_approvals,
            pending_ask_users,
            pending_exit_plans,
            memu_client,
            memory_graph_store,
            // Phase 3 ships the stub synthesizer — real LLM client lands
            // in a follow-up. The synthesizer is shared as Arc<dyn …> so
            // hot-swapping at runtime stays trivial.
            wiki_synthesizer: Arc::new(crate::memory_graph::wiki_synth::StubSynthesizer)
                as Arc<dyn crate::memory_graph::wiki_synth::WikiSynthesizer>,
            infra_service,
            service_manager,
            metrics_service,
            memubot_config: Arc::new(tokio::sync::RwLock::new(memubot_config)),
            running_sessions: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            browser_service: Arc::new(crate::browser::BrowserService::new()),
            trajectory_store,
            tool_budget,
            files_rail_service,
            runtime_service,
            proactive_service: Arc::new(tokio::sync::RwLock::new(None)),
            symphony_service: Arc::new(tokio::sync::RwLock::new(None)),
        })
    }

    /// Try to initialize the memU Python bridge.
    /// Returns None if Python is not available (degraded mode).
    fn try_init_memu(data_dir: &std::path::Path, resource_dir: Option<&std::path::Path>) -> Option<Arc<MemUClient>> {
        // 1. Locate memu_bridge.py
        let script_path = Self::find_bridge_script(resource_dir, data_dir)?;

        // 2. Locate Python
        let python_path = Self::find_python(resource_dir)?;

        tracing::info!(
            python = %python_path,
            script = %script_path.display(),
            "Initializing memU bridge"
        );

        // 3. Build LLM environment variables from providers.json (active provider)
        let mut llm_env = Vec::new();
        if let Some((api_key, base_url, model)) = Self::load_active_provider_config(data_dir) {
            if !api_key.is_empty() {
                llm_env.push(("MEMU_LLM_API_KEY".to_string(), api_key));
            }
            if !base_url.is_empty() {
                llm_env.push(("MEMU_LLM_BASE_URL".to_string(), base_url));
            }
            if !model.is_empty() {
                llm_env.push(("MEMU_LLM_CHAT_MODEL".to_string(), model));
            }
        }

        // Default to auto FastEmbed mode (use local embedding when fastembed is available)
        llm_env.push(("MEMU_EMBED_MODE".to_string(), "auto".to_string()));

        let bridge = Arc::new(MemUBridge::new(python_path, script_path, data_dir.to_path_buf(), llm_env));
        let client = Arc::new(MemUClient::new(bridge));
        Some(client)
    }

    /// Load active provider's LLM config from providers.json.
    /// Returns (api_key, base_url, model_id) if found.
    fn load_active_provider_config(data_dir: &std::path::Path) -> Option<(String, String, String)> {
        let providers_path = data_dir.join("providers.json");
        let content = std::fs::read_to_string(&providers_path).ok()?;
        let config: serde_json::Value = serde_json::from_str(&content).ok()?;

        // Get active_model's provider_id
        let active = config.get("active_model")?;
        let provider_id = active.get("provider_id")?.as_str()?;
        let model_id = active.get("model_id").and_then(|v| v.as_str()).unwrap_or("");

        // Find the matching provider in the providers array
        let providers = config.get("providers")?.as_array()?;
        for p in providers {
            if p.get("provider_id").and_then(|v| v.as_str()) == Some(provider_id) {
                let api_key = p.get("api_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let base_url = p.get("base_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                tracing::info!("Loaded memU LLM config from providers.json (provider: {})", provider_id);
                return Some((api_key, base_url, model_id.to_string()));
            }
        }
        None
    }

    /// Find the memU bridge script, checking bundled resource dir, dev path, and data dir.
    fn find_bridge_script(resource_dir: Option<&std::path::Path>, data_dir: &std::path::Path) -> Option<PathBuf> {
        // 1. Check Tauri resource_dir (Release bundle)
        if let Some(res_dir) = resource_dir {
            let bundled = res_dir.join("memu_bridge.py");
            if bundled.exists() {
                tracing::info!("Found bundled bridge script at {}", bundled.display());
                return Some(bundled);
            }
        }

        // 2. Check development path (cargo run / cargo tauri dev)
        let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("memu")
            .join("memu_bridge.py");
        if dev_path.exists() {
            tracing::debug!("Found dev bridge script at {}", dev_path.display());
            return Some(dev_path);
        }

        // 3. Check data_dir (~/.uclaw/memu_bridge.py)
        let data_script = data_dir.join("memu_bridge.py");
        if data_script.exists() {
            tracing::debug!("Found bridge script in data dir at {}", data_script.display());
            return Some(data_script);
        }

        tracing::warn!("memU bridge script not found in any location");
        None
    }

    /// Find a suitable Python interpreter, preferring embedded Python in resource_dir.
    fn find_python(resource_dir: Option<&std::path::Path>) -> Option<String> {
        // 1. Check embedded Python (Release mode)
        if let Some(res_dir) = resource_dir {
            let embedded = if cfg!(target_os = "windows") {
                res_dir.join("python").join("python.exe")
            } else {
                res_dir.join("python").join("bin").join("python3")
            };
            if embedded.exists() {
                let path_str = embedded.to_string_lossy().into_owned();
                tracing::info!("Found embedded Python at {}", path_str);
                return Some(path_str);
            }
        }

        // 2. Check dev pyembed Python (cargo tauri dev)
        let dev_pyembed = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("pyembed")
            .join("python")
            .join("bin")
            .join("python3");
        if dev_pyembed.exists() {
            let path_str = dev_pyembed.to_string_lossy().into_owned();
            tracing::info!("Found dev pyembed Python at {}", path_str);
            return Some(path_str);
        }

        // 3. Fall back to system Python (development mode)
        let candidates = ["python3.13", "python3", "python"];
        for candidate in &candidates {
            if let Ok(output) = std::process::Command::new(candidate)
                .arg("--version")
                .output()
            {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout);
                    tracing::debug!("Found system Python: {} -> {}", candidate, version.trim());
                    return Some(candidate.to_string());
                }
            }
        }
        None
    }
}

// ─── Files Rail Helpers ────────────────────────────────────────────────────

/// Stable per-path hash used inside `MountRoot.id` for attached directories.
///
/// We **must not** use the path's position in `attached_dirs` because the
/// frontend's `fileTreeAtomFamily` is keyed by `mount_id` and caches state
/// for the life of the renderer process. Index-based IDs ("...:0", "...:1")
/// silently shuffle whenever the user detaches anything, so the next mount
/// at that index inherits the previous mount's cached tree → the rail shows
/// stale filenames until a manual refresh.
///
/// SHA-256 truncated to 16 hex chars (64 bits) is plenty of uniqueness for
/// a single user's attached dirs and keeps IDs short enough to log.
fn mount_id_hash(path: &Path) -> String {
    let bytes = path.to_string_lossy();
    let digest = Sha256::digest(bytes.as_bytes());
    let mut out = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        use std::fmt::Write as _;
        let _ = write!(out, "{:02x}", byte);
    }
    out
}

impl AppState {
    pub async fn files_rail_list_mounts(
        &self,
        session_id: Option<String>,
    ) -> Result<Vec<crate::files_rail::MountRoot>, crate::error::Error> {
        use crate::files_rail::{MountKind, MountRoot};
        let mut out: Vec<MountRoot> = Vec::new();

        // Resolve the session's space_id + attached_dirs in a single short
        // lock so we don't hold it across awaits later.
        let (space_id_for_session, session_attached_dirs): (Option<String>, Vec<String>) =
            if let Some(sid) = session_id.as_deref() {
                match self.db.lock() {
                    Ok(conn) => {
                        let space: Option<String> = conn
                            .query_row(
                                "SELECT space_id FROM agent_sessions WHERE id = ?1",
                                rusqlite::params![sid],
                                |r| r.get::<_, String>(0),
                            )
                            .ok();
                        let attached_json: Option<String> = conn
                            .query_row(
                                "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
                                rusqlite::params![sid],
                                |r| r.get::<_, String>(0),
                            )
                            .ok();
                        let attached = attached_json
                            .as_deref()
                            .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
                            .unwrap_or_default();
                        (space, attached)
                    }
                    Err(_) => (None, Vec::new()),
                }
            } else {
                (None, Vec::new())
            };

        // Workspace mount — when a session_id resolves to a space with a custom
        // path, use that; otherwise fall back to the default ~/Documents/workground.
        let workspace_path: Option<std::path::PathBuf> = if let Some(space) = space_id_for_session
            .as_deref()
        {
            self.db.lock().ok().and_then(|conn| {
                conn.query_row(
                    "SELECT path FROM spaces WHERE id = ?1",
                    rusqlite::params![space],
                    |r| r.get::<_, Option<String>>(0),
                )
                .ok()
                .flatten()
                .filter(|s| !s.trim().is_empty())
                .map(std::path::PathBuf::from)
            })
        } else {
            None
        };
        let workspace_root = workspace_path.unwrap_or_else(|| {
            dirs::document_dir()
                .map(|d| d.join("workground"))
                .unwrap_or_default()
        });
        if workspace_root.exists() {
            let id = match space_id_for_session.as_deref() {
                Some(s) => format!("workspace:{}", s),
                None => "workspace:default".into(),
            };
            out.push(MountRoot {
                id,
                label: "工作区文件".into(),
                path: workspace_root,
                kind: MountKind::Workspace,
                editable: true,
            });
        }

        // W4d Issue 4 fix: workspace-level attached_dirs become MountRoots too.
        // Without this, dirs attached via attachWorkspaceDirectory (the standard
        // UI flow) never appear in the rail, which blocks testing of the
        // preview-write approval flow.
        let workspace_attached_dirs: Vec<String> = {
            let space_id_opt = space_id_for_session.clone().or_else(|| {
                // No session → fall back to the globally-active workspace.
                self.db.lock().ok().and_then(|conn| {
                    conn.query_row(
                        "SELECT value FROM settings WHERE key = 'active_workspace_id'",
                        [],
                        |r| r.get::<_, String>(0),
                    )
                    .ok()
                })
            });
            match space_id_opt {
                Some(space_id) => self
                    .db
                    .lock()
                    .ok()
                    .and_then(|conn| {
                        conn.query_row(
                            "SELECT attached_dirs FROM spaces WHERE id = ?1",
                            rusqlite::params![space_id],
                            |r| r.get::<_, Option<String>>(0),
                        )
                        .ok()
                        .flatten()
                    })
                    .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
                    .unwrap_or_default(),
                None => Vec::new(),
            }
        };

        for dir in workspace_attached_dirs.iter() {
            let pb = std::path::PathBuf::from(dir);
            if !pb.exists() {
                continue;
            }
            let space_id_label = space_id_for_session.as_deref().unwrap_or("default");
            let name = pb
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("attached")
                .to_string();
            out.push(MountRoot {
                id: format!("workspace-attached:{}:{}", space_id_label, mount_id_hash(&pb)),
                label: name,
                path: pb,
                kind: MountKind::AttachedDir,
                editable: false,
            });
        }

        // Session-scoped attached_dirs — one mount per path (filtered to existing).
        if let Some(sid) = session_id.as_deref() {
            for dir in session_attached_dirs.iter() {
                let pb = std::path::PathBuf::from(dir);
                if !pb.exists() {
                    continue;
                }
                let name = pb
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("attached")
                    .to_string();
                out.push(MountRoot {
                    id: format!("attached:{}:{}", sid, mount_id_hash(&pb)),
                    label: name,
                    path: pb,
                    kind: MountKind::AttachedDir,
                    editable: false,
                });
            }
        }

        Ok(out)
    }

    pub async fn files_rail_mount_path(
        &self,
        mount_id: &str,
        session_id_override: Option<String>,
    ) -> Result<std::path::PathBuf, crate::error::Error> {
        // Prefer the caller-supplied session_id (the frontend always knows which
        // session it is rendering). Fall back to mount_id parsing for
        // session:<id> / attached:<sid>:<idx> when the caller didn't pass it.
        let session = session_id_override.or_else(|| self.extract_session_from_mount(mount_id));
        let mounts = self.files_rail_list_mounts(session).await?;
        mounts
            .into_iter()
            .find(|m| m.id == mount_id)
            .map(|m| m.path)
            .ok_or_else(|| crate::error::Error::Internal(format!("mount not found: {}", mount_id)))
    }

    pub async fn files_rail_resolve_dir(
        &self,
        mount_id: &str,
        rel_path: &str,
        session_id_override: Option<String>,
    ) -> Result<(std::path::PathBuf, std::path::PathBuf), crate::error::Error> {
        let mount_root = self.files_rail_mount_path(mount_id, session_id_override).await?;
        let target = if rel_path.is_empty() || rel_path == "/" {
            mount_root.clone()
        } else {
            if rel_path.starts_with('/') || rel_path.split('/').any(|seg| seg == "..") {
                return Err(crate::error::Error::InvalidInput(
                    "invalid rel_path: absolute paths and .. segments are not allowed".into(),
                ));
            }
            mount_root.join(rel_path)
        };
        Ok((mount_root, target))
    }

    fn extract_session_from_mount(&self, mount_id: &str) -> Option<String> {
        if let Some(rest) = mount_id.strip_prefix("session:") {
            return Some(rest.to_string());
        }
        if let Some(rest) = mount_id.strip_prefix("attached:") {
            return rest.split(':').next().map(|s| s.to_string());
        }
        None
    }
}
