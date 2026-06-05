//! Pi-3b — `install_plugin` agent tool: install a plugin from a local directory
//! the agent has scaffolded (plugin-author skill). Approval-gated (installs
//! runnable code). The plugin activates on next app restart.

use std::path::Path;
use std::sync::Arc;
use async_trait::async_trait;

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};

pub struct InstallPluginTool {
    data_dir: std::path::PathBuf,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl InstallPluginTool {
    pub fn new(data_dir: std::path::PathBuf, db: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        Self { data_dir, db }
    }
}

#[async_trait]
impl Tool for InstallPluginTool {
    fn name(&self) -> &str { "install_plugin" }

    fn description(&self) -> &str {
        "Install a uClaw plugin from a local directory you have scaffolded (the directory must \
         contain a valid plugin.toml + its stdio MCP server executable). The plugin activates on \
         the next app restart. Use this after authoring a complete, self-contained plugin (see the \
         plugin-author skill). Requires user approval."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "dir": {
                    "type": "string",
                    "description": "Absolute path to the scaffolded plugin directory containing plugin.toml."
                }
            },
            "required": ["dir"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // Installs runnable third-party code — always confirm with the user.
        ApprovalRequirement::Always
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let dir = params
            .get("dir")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("missing 'dir'".into()))?;
        let plugins_root = self.data_dir.join("plugins");
        match crate::plugins::install::install_from_local_dir(Path::new(dir), &plugins_root) {
            Ok(p) => {
                let now_ms = chrono::Utc::now().timestamp_millis();
                if let Ok(conn) = self.db.lock() {
                    let _ = crate::plugins::state::ensure_plugin_row(&conn, &p.id, now_ms);
                }
                Ok(ToolOutput::success(
                    &format!(
                        "Installed plugin '{}' v{} (id: {}). Restart uClaw to activate its tools/commands.",
                        p.display_name, p.version, p.id
                    ),
                    start.elapsed().as_millis() as u64,
                ))
            }
            Err(e) => Ok(ToolOutput::error(
                &format!("Install failed: {e}"),
                start.elapsed().as_millis() as u64,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mem_db() -> Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE plugins (id TEXT PRIMARY KEY, enabled INTEGER NOT NULL DEFAULT 1, updated_at INTEGER NOT NULL)", []).unwrap();
        Arc::new(std::sync::Mutex::new(conn))
    }
    fn write_plugin(dir: &Path, id: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("plugin.toml"), format!(
            "id = \"{id}\"\nversion = \"0.1.0\"\ndisplay_name = \"Demo\"\n\n[author]\nname = \"t\"\n\n[runtime]\nmin_uclaw_version = \"0.1.0\"\n"
        )).unwrap();
        std::fs::write(dir.join("server.mjs"), "// demo").unwrap();
    }

    #[tokio::test]
    async fn install_plugin_tool_installs_and_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let src = tmp.path().join("src");
        write_plugin(&src, "demo");
        let tool = InstallPluginTool::new(data_dir.clone(), mem_db());
        assert_eq!(tool.requires_approval(&serde_json::json!({})), ApprovalRequirement::Always);
        let out = tool.execute(serde_json::json!({ "dir": src.to_str().unwrap() })).await.unwrap();
        assert_eq!(out.result["ok"], true, "got {:?}", out.result);
        assert!(data_dir.join("plugins/demo/plugin.toml").exists());
    }

    #[tokio::test]
    async fn install_plugin_tool_reports_error_on_bad_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let tool = InstallPluginTool::new(tmp.path().join("data"), mem_db());
        let out = tool.execute(serde_json::json!({ "dir": tmp.path().join("nope").to_str().unwrap() })).await.unwrap();
        assert_eq!(out.result["ok"], false);
    }

    #[tokio::test]
    async fn install_plugin_tool_missing_dir_param() {
        let tmp = tempfile::tempdir().unwrap();
        let tool = InstallPluginTool::new(tmp.path().join("data"), mem_db());
        let err = tool.execute(serde_json::json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidParams(_)));
    }
}
