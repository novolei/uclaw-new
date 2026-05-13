use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput};

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

    fn path_args<'a>(&self, args: &'a serde_json::Value) -> Vec<&'a str> {
        args["path"].as_str().map(|s| vec![s]).unwrap_or_default()
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let path = params["path"].as_str().ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;
        let full_path = if PathBuf::from(path).is_absolute() { PathBuf::from(path) } else { self.workspace_root.join(path) };

        let content = match fs::read_to_string(&full_path).await {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ToolError::kinded(
                    ToolErrorKind::ResourceNotFound,
                    format!("File not found: {}", full_path.display()),
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                return Err(ToolError::kinded(
                    ToolErrorKind::PermissionDenied,
                    format!("Permission denied: {}", full_path.display()),
                ));
            }
            Err(e) => return Err(e.into()),
        };

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

    fn preview_target_path(&self, args: &serde_json::Value) -> Option<String> {
        args.get("path").and_then(|v| v.as_str()).map(String::from)
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

    fn path_args<'a>(&self, args: &'a serde_json::Value) -> Vec<&'a str> {
        args["path"].as_str().map(|s| vec![s]).unwrap_or_default()
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let path = params["path"].as_str().ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;
        let content = params["content"].as_str().ok_or_else(|| ToolError::InvalidParams("content is required".into()))?;
        let full_path = if PathBuf::from(path).is_absolute() { PathBuf::from(path) } else { self.workspace_root.join(path) };

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                let kind = if e.kind() == std::io::ErrorKind::PermissionDenied {
                    ToolErrorKind::PermissionDenied
                } else {
                    ToolErrorKind::Other
                };
                ToolError::kinded_with_source(kind, format!("Cannot create dir: {}", parent.display()), e.to_string())
            })?;
        }
        fs::write(&full_path, content).await.map_err(|e| {
            let kind = match e.kind() {
                std::io::ErrorKind::PermissionDenied => ToolErrorKind::PermissionDenied,
                _ => ToolErrorKind::Other,
            };
            ToolError::kinded_with_source(kind, format!("Cannot write {}", full_path.display()), e.to_string())
        })?;

        Ok(ToolOutput::success(&format!("Successfully wrote {} bytes to {}", content.len(), full_path.display()), start.elapsed().as_millis() as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::tool::Tool;

    #[test]
    fn read_file_path_args_returns_path() {
        let tool = ReadFileTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"path": "src/main.rs"});
        assert_eq!(tool.path_args(&args), vec!["src/main.rs"]);
    }

    #[test]
    fn write_file_path_args_returns_path() {
        let tool = WriteFileTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"path": "out.txt", "content": "x"});
        assert_eq!(tool.path_args(&args), vec!["out.txt"]);
    }

    #[test]
    fn read_file_path_args_missing_path_returns_empty() {
        let tool = ReadFileTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({});
        assert!(tool.path_args(&args).is_empty());
    }

    #[tokio::test]
    async fn file_read_nonexistent_returns_resource_not_found_kind() {
        let tool = ReadFileTool::new(std::path::PathBuf::from("/tmp"));
        let result = tool.execute(serde_json::json!({
            "path": "/tmp/definitely-does-not-exist-xyz-12345.txt"
        })).await;
        match result.unwrap_err() {
            ToolError::Kinded { kind, .. } => assert_eq!(kind, ToolErrorKind::ResourceNotFound),
            other => panic!("expected Kinded(ResourceNotFound), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn file_read_nonexistent_error_message_contains_path() {
        let tool = ReadFileTool::new(std::path::PathBuf::from("/tmp"));
        let result = tool.execute(serde_json::json!({
            "path": "/tmp/definitely-does-not-exist-xyz-12345.txt"
        })).await;
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("NotFound"),
            "error display should contain NotFound kind tag: {}",
            err
        );
    }
}
