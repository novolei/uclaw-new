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

/// Find the character index range for a range of 0-based lines (inclusive)
fn find_line_char_range(content: &str, start_line_idx: usize, end_line_idx: usize) -> (usize, usize) {
    let mut start_pos = None;
    let mut end_pos = None;
    let mut current_pos = 0;
    
    let lines: Vec<&str> = content.split('\n').collect();
    for (idx, line) in lines.iter().enumerate() {
        let line_len_with_nl = line.len() + 1;
        if idx == start_line_idx {
            start_pos = Some(current_pos);
        }
        if idx == end_line_idx {
            let line_end_with_nl = (current_pos + line_len_with_nl).min(content.len());
            end_pos = Some(line_end_with_nl);
            break;
        }
        current_pos += line_len_with_nl;
    }
    
    let start = start_pos.unwrap_or(content.len());
    let end = end_pos.unwrap_or(content.len());
    (start, end)
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Edit a file via search-replace or line insertion. Pass an array of edits; for insertion set old_text='' and supply insert_line (1-based)."
    }

    fn preview_target_path(&self, args: &serde_json::Value) -> Option<String> {
        args.get("path").and_then(|v| v.as_str()).map(String::from)
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
                            "insert_line": { "type": "integer", "description": "1-based line for insertion (only when old_text is empty)." },
                            "anchor": { "type": "string", "description": "Optional starting anchor for stateful Myers Diff alignment." },
                            "end_anchor": { "type": "string", "description": "Optional ending anchor for stateful Myers Diff alignment." }
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

        // Step 2.5: Active File External Change Watcher check
        if crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER.is_stale(&full_path) {
            return Err(ToolError::Execution(
                "File has been modified externally by the user. Run read_file tool to synchronize.".into()
            ));
        }

        info!(path = %full_path.display(), edits = edits.len(), "Applying edits");

        // Read the original content
        let original = fs::read_to_string(&full_path).await.map_err(|e| {
            ToolError::Execution(format!("Cannot read {}: {}", full_path.display(), e))
        })?;

        let mut content = original.clone();
        
        // Resolve each edit to character ranges
        struct ResolvedEdit {
            start_pos: usize,
            end_pos: usize,
            new_text: String,
        }

        let mut resolved_edits = Vec::new();
        let mut current_search_pos = 0;

        for (i, edit) in edits.iter().enumerate() {
            let old_text = edit.get("old_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams(format!("edits[{}].old_text is required and must be a string", i)))?;
            let new_text = edit.get("new_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidParams(format!("edits[{}].new_text is required and must be a string", i)))?;
            let insert_line = edit.get("insert_line").and_then(|v| v.as_u64());
            let anchor = edit.get("anchor").and_then(|v| v.as_str());
            let end_anchor = edit.get("end_anchor").and_then(|v| v.as_str());

            if let Some(anchor_str) = anchor {
                // Anchor-based edit!
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                let anchors = crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER.get_anchors(&full_path)
                    .unwrap_or_else(|| {
                        let a = crate::agent::anchor_state::initialize_anchors(&lines);
                        crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER.register_file_lines(&full_path, &lines);
                        a
                    });

                let start_idx = anchors.iter().position(|r| r == anchor_str)
                    .ok_or_else(|| ToolError::Execution(format!(
                        "Start anchor '{}' not found in file. Make sure you have the correct anchor.",
                        anchor_str
                    )))?;

                let end_idx = if let Some(end_anchor_str) = end_anchor {
                    anchors.iter().skip(start_idx).position(|r| r == end_anchor_str)
                        .map(|p| start_idx + p)
                        .ok_or_else(|| ToolError::Execution(format!(
                            "End anchor '{}' not found after start anchor in file.",
                            end_anchor_str
                        )))?
                } else {
                    start_idx
                };

                let (start_pos, end_pos) = find_line_char_range(&content, start_idx, end_idx);
                let mut formatted_new_text = new_text.to_string();
                if !formatted_new_text.ends_with('\n') && content[start_pos..end_pos].ends_with('\n') {
                    formatted_new_text.push('\n');
                }

                resolved_edits.push(ResolvedEdit {
                    start_pos,
                    end_pos,
                    new_text: formatted_new_text,
                });

            } else if old_text.is_empty() {
                // Insert mode!
                let (start_pos, end_pos) = match insert_line {
                    Some(line_num) => {
                        let lines_count = content.lines().count();
                        let line_idx = (line_num as usize).saturating_sub(1).min(lines_count);
                        if line_idx >= lines_count {
                            (content.len(), content.len())
                        } else {
                            let (start, _) = find_line_char_range(&content, line_idx, line_idx);
                            (start, start)
                        }
                    }
                    None => {
                        (content.len(), content.len())
                    }
                };

                let mut formatted_new_text = new_text.to_string();
                if !formatted_new_text.ends_with('\n') {
                    formatted_new_text.push('\n');
                }
                if insert_line.is_none() && start_pos == content.len() && !content.ends_with('\n') && !content.is_empty() {
                    formatted_new_text = format!("\n{}", formatted_new_text);
                }

                resolved_edits.push(ResolvedEdit {
                    start_pos,
                    end_pos,
                    new_text: formatted_new_text,
                });

            } else {
                // Search-replace mode!
                let mut pos = content[current_search_pos..].find(old_text).map(|p| p + current_search_pos);
                if pos.is_none() {
                    pos = content.find(old_text);
                }

                let start_pos = pos.ok_or_else(|| ToolError::Execution(format!(
                    "old_text '{}' not found in file. Make sure the text matches exactly including whitespace and indentation.",
                    old_text
                )))?;
                let end_pos = start_pos + old_text.len();

                current_search_pos = end_pos;

                resolved_edits.push(ResolvedEdit {
                    start_pos,
                    end_pos,
                    new_text: new_text.to_string(),
                });
            }
        }

        // Sort bottom-to-top (descending by start_pos, then end_pos)
        resolved_edits.sort_by(|a, b| {
            b.start_pos.cmp(&a.start_pos)
                .then_with(|| b.end_pos.cmp(&a.end_pos))
        });

        let mut applied = 0;
        for re in resolved_edits {
            let mut new_content = String::with_capacity(content.len() + re.new_text.len());
            new_content.push_str(&content[..re.start_pos]);
            new_content.push_str(&re.new_text);
            new_content.push_str(&content[re.end_pos..]);
            content = new_content;
            applied += 1;
        }

        // Register expected write before fs::write so watcher doesn't see it as external change
        crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER.register_expected_write(&full_path);

        // Write back
        fs::write(&full_path, &content).await.map_err(|e| {
            ToolError::Execution(format!("Cannot write {}: {}", full_path.display(), e))
        })?;

        // Align anchors
        let old_lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
        let new_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER.align_file_anchors(&full_path, &old_lines, &new_lines);

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

