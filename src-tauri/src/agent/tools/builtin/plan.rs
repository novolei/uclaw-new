use std::path::PathBuf;
use std::time::Instant;
use async_trait::async_trait;
use tauri::Emitter;
use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};

pub struct PlanWriteTool {
    workspace_root: PathBuf,
    app_handle: tauri::AppHandle,
}

impl PlanWriteTool {
    pub fn new(workspace_root: PathBuf, app_handle: tauri::AppHandle) -> Self {
        Self { workspace_root, app_handle }
    }
}

#[async_trait]
impl Tool for PlanWriteTool {
    fn name(&self) -> &str { "plan_write" }
    fn description(&self) -> &str {
        "Create a structured plan file before starting a complex task. Saves to .uclaw/plans/ in the workspace."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "Plan title" },
                "steps": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Ordered list of steps to complete"
                },
                "notes": { "type": "string", "description": "Optional additional notes" }
            },
            "required": ["title", "steps"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let title = params["title"].as_str().unwrap_or("Plan");
        let steps: Vec<&str> = params["steps"].as_array()
            .map(|a| a.iter().filter_map(|s| s.as_str()).collect())
            .unwrap_or_default();
        let notes = params["notes"].as_str().unwrap_or("");

        let slug: String = title.to_lowercase()
            .split_whitespace()
            .take(6)
            .collect::<Vec<_>>()
            .join("-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect();

        let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H%M%S");
        let filename = format!("{}-{}.md", timestamp, slug);

        let plans_dir = self.workspace_root.join(".uclaw").join("plans");
        std::fs::create_dir_all(&plans_dir)
            .map_err(|e| ToolError::Execution(format!("Failed to create plans dir: {}", e)))?;
        let path = plans_dir.join(&filename);

        let now = chrono::Utc::now().to_rfc3339();
        let steps_md = steps.iter()
            .enumerate()
            .map(|(i, s)| format!("- [ ] {}. {}", i + 1, s))
            .collect::<Vec<_>>()
            .join("\n");

        let content = format!(
            "---\ntask: \"{}\"\nstatus: in_progress\ncreated_at: {}\n---\n\n## Goal\n{}\n\n## Steps\n{}\n\n## Notes\n{}\n",
            title, now, title, steps_md, notes
        );

        std::fs::write(&path, &content)
            .map_err(|e| ToolError::Execution(format!("Failed to write plan: {}", e)))?;

        let _ = self.app_handle.emit("plan:updated", serde_json::json!({
            "filename": filename,
            "content": content,
        }));

        let duration = start.elapsed().as_millis() as u64;
        Ok(ToolOutput::success(
            &format!("Plan created at {}", path.display()),
            duration,
        ))
    }
}

pub struct PlanUpdateTool {
    workspace_root: PathBuf,
    app_handle: tauri::AppHandle,
}

impl PlanUpdateTool {
    pub fn new(workspace_root: PathBuf, app_handle: tauri::AppHandle) -> Self {
        Self { workspace_root, app_handle }
    }
}

#[async_trait]
impl Tool for PlanUpdateTool {
    fn name(&self) -> &str { "plan_update" }
    fn description(&self) -> &str {
        "Update a step in an existing plan file. Mark a step done or add a note."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "filename": { "type": "string", "description": "Plan filename (e.g. '2025-01-01_120000-my-plan.md')" },
                "step_index": { "type": "integer", "description": "Zero-based index of the step to update" },
                "done": { "type": "boolean", "description": "Mark step as done (true) or undone (false)" },
                "note": { "type": "string", "description": "Optional note to append to the step" }
            },
            "required": ["filename", "step_index"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let filename = params["filename"].as_str()
            .ok_or_else(|| ToolError::Execution("filename is required".to_string()))?;
        let step_index = params["step_index"].as_u64().unwrap_or(0) as usize;
        let done = params["done"].as_bool().unwrap_or(true);
        let note = params["note"].as_str().unwrap_or("");

        // Prevent path traversal
        let safe_filename = std::path::Path::new(filename)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ToolError::Execution("Invalid filename".to_string()))?;

        let path = self.workspace_root.join(".uclaw").join("plans").join(safe_filename);

        let content = std::fs::read_to_string(&path)
            .map_err(|e| ToolError::Execution(format!("Cannot read plan file: {}", e)))?;

        let marker = if done { "- [x]" } else { "- [ ]" };
        let mut step_count = 0usize;
        let mut found = false;
        let updated: Vec<String> = content.lines().map(|line| {
            if line.trim_start().starts_with("- [ ]") || line.trim_start().starts_with("- [x]") {
                if step_count == step_index {
                    found = true;
                    step_count += 1;
                    let rest_start = line.find(']').map(|i| i + 1).unwrap_or(line.len());
                    let rest = &line[rest_start..];
                    let note_suffix = if note.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", note)
                    };
                    return format!("{}{}{}", marker, rest, note_suffix);
                }
                step_count += 1;
            }
            line.to_string()
        }).collect();

        if !found {
            return Err(ToolError::Execution(format!(
                "step_index {} is out of range (plan has {} steps)",
                step_index, step_count
            )));
        }

        let updated_content = updated.join("\n") + "\n";
        std::fs::write(&path, &updated_content)
            .map_err(|e| ToolError::Execution(format!("Failed to update plan: {}", e)))?;

        let _ = self.app_handle.emit("plan:updated", serde_json::json!({
            "filename": safe_filename,
            "content": updated_content,
        }));

        let duration = start.elapsed().as_millis() as u64;
        // When marking done, append an honesty reminder to the tool result so
        // the LLM doesn't use plan_update as a shortcut to "complete" steps it
        // didn't actually execute. Observed in the wild: agent calls
        // mkdir + ls + plan_update(done:true) for a "build the game engine"
        // step without ever calling write_file. The reminder makes the LLM
        // re-check itself and prevents the plan-aware termination guard from
        // being bypassed by fake completions.
        let result_text = if done {
            format!(
                "Step {} marked DONE in {}.\n\n\
                 IMPORTANT: plan_update is a bookkeeping tool. It does NOT execute work. \
                 If this step required code changes (writing files, editing, running commands), \
                 you must have already called edit / write_file / bash to actually do that work. \
                 If you haven't, undo this update (call plan_update again with done:false) and \
                 perform the actual work first. Users see code on disk, not plan checkmarks.",
                step_index, safe_filename
            )
        } else {
            format!("Step {} marked NOT DONE in {}", step_index, safe_filename)
        };
        Ok(ToolOutput::success(&result_text, duration))
    }
}
