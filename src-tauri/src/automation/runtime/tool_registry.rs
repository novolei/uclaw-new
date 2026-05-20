use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;

use crate::agent::tools::{builtin, tool::ToolRegistry};
use crate::automation::protocol::humane_v1::Permission;

pub struct AutomationToolRegistryDeps {
    pub workspace_root: PathBuf,
    pub spec_permissions: Vec<Permission>,
    pub gbrain_declared: bool,
}

pub fn planned_tool_names(spec_permissions: &[Permission], gbrain_declared: bool) -> Vec<String> {
    let mut names = vec![
        "read_file".to_string(),
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
                "browser_task",
                "browser_task_resume",
                "retry_with_browser_agent",
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

pub fn register_base_tools(tools: &mut ToolRegistry, workspace_root: PathBuf) {
    let ws = workspace_root;
    tools.register(builtin::file::ReadFileTool::new(ws.clone()));
    tools.register(builtin::file::WriteFileTool::new(ws.clone()));
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
}
