use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;

use crate::agent::tools::{builtin, tool::ToolRegistry};
use crate::automation::protocol::humane_v1::Permission;

pub struct AutomationToolRegistryDeps {
    pub workspace_root: PathBuf,
    pub spec_permissions: Vec<Permission>,
    pub gbrain_declared: bool,
    pub browser_context_manager: Option<Arc<crate::browser::BrowserContextManager>>,
    pub browser_session_id: Option<String>,
    pub browser_builtin_root: Option<PathBuf>,
    pub browser_runtime_provider_config:
        crate::browser::runtime_control_center::BrowserRuntimeProviderConfig,
}

pub fn planned_tool_names(spec_permissions: &[Permission], gbrain_declared: bool) -> Vec<String> {
    let mut names = vec![
        "read_file".to_string(),
        "get_file_skeleton".to_string(),
        "write_file".to_string(),
        "grep".to_string(),
        "glob".to_string(),
        "web_fetch".to_string(),
        "http_request".to_string(),
        "edit".to_string(),
        "bash".to_string(),
        "report_to_user".to_string(),
        "notify_user".to_string(),
        "request_escalation".to_string(),
        "memory".to_string(),
    ];
    if spec_permissions.contains(&Permission::AiBrowser) {
        names.extend(
            [
                "browser_navigate",
                "browser_evaluate",
                "browser_wait",
                "browser_list_tabs",
                "browser_task",
                "browser_task_resume",
                "retry_with_browser_agent",
                "browser_run",
                "browser_run_script",
            ]
            .into_iter()
            .map(str::to_string),
        );
    }
    if gbrain_declared {
        names.extend(
            [
                "gbrain_room_search",
                "gbrain_room_get_page",
                "gbrain_room_put_page",
            ]
            .into_iter()
            .map(str::to_string),
        );
    }
    names
}

pub fn build_base_registry(deps: AutomationToolRegistryDeps) -> Arc<ToolRegistry> {
    let mut tools = ToolRegistry::new();
    register_base_tools(&mut tools, deps.workspace_root);
    register_automation_schema_tools(&mut tools);
    Arc::new(tools)
}

pub fn build_registry_with_capabilities(deps: AutomationToolRegistryDeps) -> Arc<ToolRegistry> {
    let mut tools = ToolRegistry::new();
    register_base_tools(&mut tools, deps.workspace_root.clone());
    register_automation_schema_tools(&mut tools);

    if deps.spec_permissions.contains(&Permission::AiBrowser) {
        if let (Some(ctx_mgr), Some(session_id)) = (
            deps.browser_context_manager.clone(),
            deps.browser_session_id.clone(),
        ) {
            let builtin_root = deps.browser_builtin_root.clone().unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/live-room")
            });
            let runtime_status_service = Some(Arc::new(
                crate::browser::BrowserRuntimeStatusService::new(ctx_mgr.clone()),
            ));
            let browser_run_script = crate::browser::tools::BrowserRunScriptTool {
                ctx_mgr: ctx_mgr.clone(),
                session_id: session_id.clone(),
                workspace_root: deps.workspace_root.clone(),
                builtin_root,
                runtime_status_service: runtime_status_service.clone(),
                runtime_provider_config: deps.browser_runtime_provider_config.clone(),
            };
            tools.register(crate::browser::tools::BrowserRunTool {
                inner: browser_run_script.clone(),
            });
            tools.register(browser_run_script);
            tools.register(crate::browser::tools::BrowserNavigateTool {
                ctx_mgr: ctx_mgr.clone(),
                session_id: session_id.clone(),
                runtime_status_service: runtime_status_service.clone(),
                runtime_provider_config: deps.browser_runtime_provider_config.clone(),
            });
            tools.register(crate::browser::tools::BrowserEvaluateTool {
                ctx_mgr: ctx_mgr.clone(),
                session_id: session_id.clone(),
                runtime_status_service: runtime_status_service.clone(),
                runtime_provider_config: deps.browser_runtime_provider_config.clone(),
            });
            tools.register(crate::browser::tools::BrowserWaitTool {
                ctx_mgr: ctx_mgr.clone(),
                session_id: session_id.clone(),
                runtime_status_service: runtime_status_service.clone(),
                runtime_provider_config: deps.browser_runtime_provider_config.clone(),
            });
            tools.register(crate::browser::tools::BrowserListTabsTool {
                ctx_mgr,
                session_id,
                runtime_status_service,
                runtime_provider_config: deps.browser_runtime_provider_config,
            });
        } else {
            tools.register(CapabilitySchemaTool::new(
                "browser_navigate",
                "Navigate to a browser URL. Execution is connected by AppRuntimeService when a browser session is available.",
            ));
            tools.register(CapabilitySchemaTool::new(
                "browser_evaluate",
                "Evaluate JavaScript in a browser tab. Execution is connected by AppRuntimeService when a browser session is available.",
            ));
            tools.register(CapabilitySchemaTool::new(
                "browser_wait",
                "Wait for a browser selector or duration. Execution is connected by AppRuntimeService when a browser session is available.",
            ));
            tools.register(CapabilitySchemaTool::new(
                "browser_list_tabs",
                "List browser tabs. Execution is connected by AppRuntimeService when a browser session is available.",
            ));
            tools.register(CapabilitySchemaTool::new(
                "browser_run",
                "Run a restricted browser adapter JavaScript file. Execution is connected by AppRuntimeService.",
            ));
            tools.register(CapabilitySchemaTool::new(
                "browser_run_script",
                "Run a restricted browser adapter JavaScript file. Execution is connected by AppRuntimeService.",
            ));
        }
        tools.register(CapabilitySchemaTool::new(
            "browser_task",
            "Run a constrained browser fallback task. Execution is connected by the live browser bridge.",
        ));
        tools.register(CapabilitySchemaTool::new(
            "browser_task_resume",
            "Resume a constrained browser fallback task. Execution is connected by the live browser bridge.",
        ));
        tools.register(CapabilitySchemaTool::new(
            "retry_with_browser_agent",
            "Retry a browser action through the browser agent. Execution is connected by the live browser bridge.",
        ));
    }
    if deps.gbrain_declared {
        tools.register(ScopedGbrainSchemaTool::new("gbrain_room_search"));
        tools.register(ScopedGbrainSchemaTool::new("gbrain_room_get_page"));
        tools.register(ScopedGbrainSchemaTool::new("gbrain_room_put_page"));
    }
    Arc::new(tools)
}

pub fn register_base_tools(tools: &mut ToolRegistry, workspace_root: PathBuf) {
    let ws = workspace_root;
    tools.register(builtin::file::ReadFileTool::new(ws.clone()));
    tools.register(builtin::file::WriteFileTool::new(ws.clone()));
    tools.register(builtin::get_file_skeleton::GetFileSkeletonTool::new(
        ws.clone(),
    ));
    tools.register(builtin::search::GrepTool::new(ws.clone()));
    tools.register(builtin::search::GlobTool::new(ws.clone()));
    tools.register(builtin::web::WebFetchTool::new());
    tools.register(builtin::web::HttpRequestTool::new());
    tools.register(builtin::edit::EditTool::new(ws.clone()));
    tools.register(builtin::shell::BashTool::new(ws));
}

fn register_automation_schema_tools(tools: &mut ToolRegistry) {
    for schema in crate::automation::tools::humane_tool_schemas() {
        tools.register(AutomationToolSchema::from_value(&schema));
    }
}

/// Schema-only wrapper for automation-native tools handled directly by
/// `HeadlessDelegate::execute_tool_calls`.
struct AutomationToolSchema {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

impl AutomationToolSchema {
    fn from_value(v: &serde_json::Value) -> Self {
        Self {
            name: v
                .get("name")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string(),
            description: v
                .get("description")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string(),
            input_schema: v
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({"type": "object"})),
        }
    }
}

#[async_trait]
impl crate::agent::tools::tool::Tool for AutomationToolSchema {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
    ) -> Result<crate::agent::tools::tool::ToolOutput, crate::agent::tools::tool::ToolError> {
        Err(crate::agent::tools::tool::ToolError::Execution(format!(
            "automation tool '{}' is delegate-dispatched, not registry-dispatched",
            self.name
        )))
    }
}

pub struct CapabilitySchemaTool {
    name: String,
    description: String,
}

impl CapabilitySchemaTool {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

#[async_trait]
impl crate::agent::tools::tool::Tool for CapabilitySchemaTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "additionalProperties": true
        })
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
    ) -> Result<crate::agent::tools::tool::ToolOutput, crate::agent::tools::tool::ToolError> {
        Err(crate::agent::tools::tool::ToolError::Execution(format!(
            "{} execution not connected",
            self.name
        )))
    }
}

pub struct ScopedGbrainSchemaTool {
    name: String,
}

impl ScopedGbrainSchemaTool {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait]
impl crate::agent::tools::tool::Tool for ScopedGbrainSchemaTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Room-scoped gbrain helper. Requires platform and room_id; unscoped access is rejected."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "platform": { "type": "string" },
                "room_id": { "type": "string" },
                "query": { "type": "string" },
                "slug": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["platform", "room_id"]
        })
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
    ) -> Result<crate::agent::tools::tool::ToolOutput, crate::agent::tools::tool::ToolError> {
        Err(crate::agent::tools::tool::ToolError::Execution(format!(
            "{} execution not connected",
            self.name
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::protocol::humane_v1::Permission;

    #[test]
    fn browser_tools_are_absent_without_ai_browser_permission() {
        let names = planned_tool_names(&[], false);
        assert!(!names.contains(&"browser_task".to_string()));
        assert!(!names.contains(&"browser_run_script".to_string()));
    }

    #[test]
    fn browser_tools_are_present_with_ai_browser_permission() {
        let names = planned_tool_names(&[Permission::AiBrowser], false);
        assert!(names.contains(&"browser_task".to_string()));
        assert!(names.contains(&"browser_task_resume".to_string()));
        assert!(names.contains(&"retry_with_browser_agent".to_string()));
        assert!(names.contains(&"browser_run".to_string()));
        assert!(names.contains(&"browser_run_script".to_string()));
    }

    #[test]
    fn scoped_gbrain_tools_are_present_when_gbrain_declared() {
        let names = planned_tool_names(&[Permission::AiBrowser], true);
        assert!(names.contains(&"gbrain_room_search".to_string()));
        assert!(names.contains(&"gbrain_room_get_page".to_string()));
        assert!(names.contains(&"gbrain_room_put_page".to_string()));
        assert!(!names.contains(&"gbrain_search".to_string()));
    }

    #[test]
    fn base_registry_preserves_automation_schema_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let registry = build_base_registry(AutomationToolRegistryDeps {
            workspace_root: tmp.path().to_path_buf(),
            spec_permissions: Vec::new(),
            gbrain_declared: false,
            browser_context_manager: None,
            browser_session_id: None,
            browser_builtin_root: None,
            browser_runtime_provider_config: Default::default(),
        });
        let names = registry
            .list_definitions()
            .into_iter()
            .map(|def| def.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"report_to_user".to_string()));
        assert!(names.contains(&"notify_user".to_string()));
        assert!(names.contains(&"request_escalation".to_string()));
        assert!(names.contains(&"memory".to_string()));
    }

    #[test]
    fn capable_registry_exposes_browser_and_room_gbrain_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let registry = build_registry_with_capabilities(AutomationToolRegistryDeps {
            workspace_root: tmp.path().to_path_buf(),
            spec_permissions: vec![Permission::AiBrowser],
            gbrain_declared: true,
            browser_context_manager: None,
            browser_session_id: None,
            browser_builtin_root: None,
            browser_runtime_provider_config: Default::default(),
        });
        let defs = registry.list_definitions();
        assert!(defs.iter().any(|tool| tool.name == "browser_task"));
        assert!(defs.iter().any(|tool| tool.name == "browser_navigate"));
        assert!(defs.iter().any(|tool| tool.name == "browser_evaluate"));
        assert!(defs.iter().any(|tool| tool.name == "browser_wait"));
        assert!(defs.iter().any(|tool| tool.name == "browser_list_tabs"));
        assert!(defs.iter().any(|tool| tool.name == "browser_run"));
        assert!(defs.iter().any(|tool| tool.name == "browser_run_script"));
        assert!(defs.iter().any(|tool| tool.name == "gbrain_room_search"));
    }

    #[test]
    fn capable_registry_registers_real_browser_run_when_session_is_available() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx_mgr = Arc::new(crate::browser::BrowserContextManager::new_for_test(
            tmp.path().join("profiles"),
        ));
        let registry = build_registry_with_capabilities(AutomationToolRegistryDeps {
            workspace_root: tmp.path().to_path_buf(),
            spec_permissions: vec![Permission::AiBrowser],
            gbrain_declared: false,
            browser_context_manager: Some(ctx_mgr),
            browser_session_id: Some("automation:spec:activity".to_string()),
            browser_builtin_root: Some(tmp.path().join("live-room")),
            browser_runtime_provider_config: Default::default(),
        });

        let browser_run = registry
            .get("browser_run")
            .expect("browser_run should be registered");
        assert!(
            browser_run
                .description()
                .contains("Validate and run a restricted browser adapter JavaScript file"),
            "expected real browser_run tool, got schema fallback description: {}",
            browser_run.description()
        );
        let browser_navigate = registry
            .get("browser_navigate")
            .expect("browser_navigate should be registered");
        assert!(
            browser_navigate
                .description()
                .contains("Navigate to a URL in the browser"),
            "expected real browser_navigate tool, got schema fallback description: {}",
            browser_navigate.description()
        );
    }
}
