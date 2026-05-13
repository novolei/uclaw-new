use async_trait::async_trait;
use similar::TextDiff;
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info, warn};

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};

/// Edit tool — supports search-and-replace edits on files.
///
/// Each edit specifies `old_text` (the text to find) and `new_text` (the replacement).
/// If `old_text` is empty, `new_text` is inserted at the given `insert_line` (1-based),
/// or appended to the end of the file if `insert_line` is omitted.
pub struct EditTool {
    workspace_root: PathBuf,
}

impl EditTool {
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

    /// Apply a single search-replace edit to `content`.
    fn apply_edit(content: &str, old_text: &str, new_text: &str) -> Result<String, ToolError> {
        if old_text.is_empty() {
            return Err(ToolError::InvalidParams(
                "old_text must not be empty for search_replace edits".into(),
            ));
        }

        // Try exact match first
        if let Some(pos) = content.find(old_text) {
            // Ensure unique match
            if content[pos + old_text.len()..].contains(old_text) {
                warn!("old_text matches multiple locations; replacing first occurrence");
            }
            let mut result = String::with_capacity(content.len());
            result.push_str(&content[..pos]);
            result.push_str(new_text);
            result.push_str(&content[pos + old_text.len()..]);
            return Ok(result);
        }

        Err(ToolError::Execution(
            "old_text not found in file. Make sure the text matches exactly including whitespace and indentation.".into(),
        ))
    }

    /// Insert `new_text` at the given 1-based line number, or append if line is None.
    fn apply_insert(content: &str, new_text: &str, line: Option<u64>) -> String {
        let lines: Vec<&str> = content.lines().collect();
        match line {
            Some(line_num) => {
                let idx = (line_num as usize).saturating_sub(1).min(lines.len());
                let mut result_lines: Vec<&str> = Vec::with_capacity(lines.len() + 1);
                result_lines.extend_from_slice(&lines[..idx]);
                // new_text may be multi-line
                let new_lines: Vec<&str> = new_text.lines().collect();
                result_lines.extend(new_lines);
                result_lines.extend_from_slice(&lines[idx..]);
                let mut out = result_lines.join("\n");
                if content.ends_with('\n') {
                    out.push('\n');
                }
                out
            }
            None => {
                // Append
                let mut out = content.to_string();
                if !out.ends_with('\n') && !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(new_text);
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out
            }
        }
    }

    /// Generate a unified diff between two strings.
    fn generate_diff(original: &str, modified: &str, path: &str) -> String {
        let diff = TextDiff::from_lines(original, modified);
        let mut output = String::new();

        let unified = diff.unified_diff();
        for hunk in unified.iter_hunks() {
            output.push_str(&hunk.to_string());
        }

        if output.is_empty() {
            format!("No changes to {}", path)
        } else {
            format!("--- {path}\n+++ {path}\n{output}")
        }
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Edit a file via search-replace or line insertion. Pass an array of edits; for insertion set old_text='' and supply insert_line (1-based)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path." },
                "edits": {
                    "type": "array",
                    "description": "Edits applied in order.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_text": { "type": "string", "description": "Exact text to find; empty = insert mode." },
                            "new_text": { "type": "string", "description": "Replacement or text to insert." },
                            "insert_line": { "type": "integer", "description": "1-based line for insertion (only when old_text is empty)." }
                        },
                        "required": ["old_text", "new_text"]
                    }
                }
            },
            "required": ["path", "edits"]
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

        let path = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;
        let edits = params["edits"]
            .as_array()
            .ok_or_else(|| ToolError::InvalidParams("edits must be an array".into()))?;

        if edits.is_empty() {
            return Err(ToolError::InvalidParams("edits array is empty".into()));
        }

        let full_path = self.resolve_path(path);
        info!(path = %full_path.display(), edits = edits.len(), "Applying edits");

        // Read the original content
        let original = fs::read_to_string(&full_path).await.map_err(|e| {
            ToolError::Execution(format!("Cannot read {}: {}", full_path.display(), e))
        })?;

        let mut content = original.clone();
        let mut applied = 0;

        for (i, edit) in edits.iter().enumerate() {
            let old_text = edit["old_text"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams(format!("edits[{}].old_text must be a string", i)))?;
            let new_text = edit["new_text"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams(format!("edits[{}].new_text must be a string", i)))?;

            if old_text.is_empty() {
                // Insert mode
                let insert_line = edit["insert_line"].as_u64();
                debug!(edit_index = i, insert_line = ?insert_line, "Inserting text");
                content = Self::apply_insert(&content, new_text, insert_line);
            } else {
                // Search-replace mode
                debug!(edit_index = i, old_len = old_text.len(), new_len = new_text.len(), "Replacing text");
                content = Self::apply_edit(&content, old_text, new_text)?;
            }
            applied += 1;
        }

        // Write back
        fs::write(&full_path, &content).await.map_err(|e| {
            ToolError::Execution(format!("Cannot write {}: {}", full_path.display(), e))
        })?;

        // Generate diff
        let diff = Self::generate_diff(&original, &content, path);
        let summary = format!(
            "Applied {} edit(s) to {}\n\n{}",
            applied,
            full_path.display(),
            diff
        );

        info!(path = %full_path.display(), applied, "Edits applied successfully");
        Ok(ToolOutput::success(&summary, start.elapsed().as_millis() as u64))
    }
}

#[cfg(test)]
mod path_args_tests {
    use super::*;
    use crate::agent::tools::tool::Tool;

    #[test]
    fn edit_path_args_returns_path() {
        let tool = EditTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"path": "lib.rs", "edits": []});
        assert_eq!(tool.path_args(&args), vec!["lib.rs"]);
    }
}
