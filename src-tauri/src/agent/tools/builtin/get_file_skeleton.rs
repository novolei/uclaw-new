// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput};
use crate::agent::skeleton::generate_skeleton;
use crate::agent::anchor_state::initialize_anchors;

pub struct GetFileSkeletonTool {
    workspace_root: PathBuf,
}

impl GetFileSkeletonTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            self.workspace_root.join(path)
        }
    }
}

#[async_trait]
impl Tool for GetFileSkeletonTool {
    fn name(&self) -> &str {
        "get_file_skeleton"
    }

    fn description(&self) -> &str {
        "Generate a compressed symbol skeleton for a file (classes, functions, interfaces), collapsing bodies to '// ... §Anchor§ ...' to compress token context size by >90%."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative or absolute path to the code file."
                }
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
        let path = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;

        let full_path = self.resolve_path(path);

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

        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        
        // Register file and track it for external changes
        crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER.register_file_lines(&full_path, &lines);
        crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER.track_file(&full_path);

        let anchors = crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER.get_anchors(&full_path)
            .unwrap_or_else(|| initialize_anchors(&lines));

        let skeleton = generate_skeleton(&full_path, &content, &anchors);

        Ok(ToolOutput::success(&skeleton, start.elapsed().as_millis() as u64))
    }

    fn effects(&self) -> crate::agent::tools::tool::ToolEffects {
        crate::agent::tools::tool::ToolEffects::read()
    }
}
