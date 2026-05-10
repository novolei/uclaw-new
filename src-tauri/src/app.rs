use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::Manager;

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
use crate::services::ServiceManager;
use crate::observability::MetricsService;
use crate::memubot_config::MemubotConfig;

// ─── Pending Approvals ──────────────────────────────────────────────────

/// Result of an approval decision from the user.
#[derive(Debug, Clone)]
pub struct ApprovalResult {
    pub approved: bool,
    pub always_allow: bool,
    pub tool_name: Option<String>,
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

    // ─── Phased Boot: 新增服务 ───────────────────────────────────────
    /// 中央消息总线
    pub infra_service: Arc<InfraService>,
    /// 服务管理器（统一管理所有后台服务的启停和健康监控）
    pub service_manager: Arc<ServiceManager>,
    /// 指标采集服务
    pub metrics_service: Arc<MetricsService>,
    /// memubot 功能配置
    pub memubot_config: MemubotConfig,

    /// Active agentic session cancellation tokens, keyed by conversation_id.
    /// Used by stop_agent_session to cancel a running loop.
    pub running_sessions: Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>>,

    /// Browser service for headless Chrome automation
    pub browser_service: Arc<crate::browser::BrowserService>,

    /// Automation scheduling service
    pub automation_service: Arc<crate::automation::AutomationService>,

    // Evaluation harness
    pub trajectory_store: Arc<crate::harness::TrajectoryStore>,
    pub tool_budget: Arc<crate::harness::ToolBudgetManager>,
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
            crate::db::migrations::run(&conn).ok();
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

        // B2: Skills registry
        let mut skills_reg = SkillsRegistry::new();
        // Add default scan directories
        let user_skills_dir = data_dir.join("skills");
        std::fs::create_dir_all(&user_skills_dir).ok();
        skills_reg.add_scan_dir(user_skills_dir);
        // Also scan project-level skills/ if it exists
        let project_skills = std::env::current_dir()
            .map(|d| d.join("skills"))
            .unwrap_or_default();
        if project_skills.exists() {
            skills_reg.add_scan_dir(project_skills);
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

        // memU integration (degraded mode if Python unavailable)
        // Get Tauri resource directory for embedded Python detection
        let resource_dir = app_handle.path().resource_dir().ok();
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
            Arc::new(store)
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

        // Evaluation harness
        let trajectory_store = Arc::new(crate::harness::TrajectoryStore::new(db.clone()));
        let tool_budget = Arc::new(crate::harness::ToolBudgetManager::new(&data_dir));
        let automation_service = Arc::new(crate::automation::AutomationService::new(db.clone()));

        // ─── Stage 2：核心服务 ─────────────────────────────────────────
        let infra_service = Arc::new(InfraService::new());
        tracing::info!("InfraService created");

        let metrics_service = Arc::new(MetricsService::new());
        tracing::info!("MetricsService created");

        let service_manager = Arc::new(ServiceManager::new());
        tracing::info!("ServiceManager created");

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
            provider_service,
            safety_manager,
            pending_approvals,
            pending_ask_users,
            pending_exit_plans,
            memu_client,
            memory_graph_store,
            infra_service,
            service_manager,
            metrics_service,
            memubot_config,
            running_sessions: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            browser_service: Arc::new(crate::browser::BrowserService::new()),
            automation_service,
            trajectory_store,
            tool_budget,
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
