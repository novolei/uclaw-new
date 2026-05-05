use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};

pub struct ReadFileTool { workspace_root: PathBuf }

impl ReadFileTool {
    pub fn new(workspace_root: PathBuf) -> Self { Self { workspace_root } }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str {
        "Read the contents of a file. Returns the full text content of the specified file."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Relative or absolute path to the file to read"}
            },
            "required": ["path"]
        })
    }
    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let path = params["path"].as_str().ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;
        let full_path = if PathBuf::from(path).is_absolute() { PathBuf::from(path) } else { self.workspace_root.join(path) };

        let content = fs::read_to_string(&full_path).await.map_err(|e| {
            ToolError::Execution(format!("Cannot read {}: {}", full_path.display(), e))
        })?;

        Ok(ToolOutput::success(&content, start.elapsed().as_millis() as u64))
    }
}

pub struct WriteFileTool { workspace_root: PathBuf }

impl WriteFileTool {
    pub fn new(workspace_root: PathBuf) -> Self { Self { workspace_root } }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, overwrites if it does."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Relative or absolute path to the file to write"},
                "content": {"type": "string", "description": "The content to write to the file"}
            },
            "required": ["path", "content"]
        })
    }
    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::UnlessAutoApproved
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let path = params["path"].as_str().ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;
        let content = params["content"].as_str().ok_or_else(|| ToolError::InvalidParams("content is required".into()))?;
        let full_path = if PathBuf::from(path).is_absolute() { PathBuf::from(path) } else { self.workspace_root.join(path) };

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| ToolError::Execution(format!("Cannot create dir: {}", e)))?;
        }
        fs::write(&full_path, content).await.map_err(|e| ToolError::Execution(format!("Cannot write {}: {}", full_path.display(), e)))?;

        Ok(ToolOutput::success(&format!("Successfully wrote {} bytes to {}", content.len(), full_path.display()), start.elapsed().as_millis() as u64))
    }
}
