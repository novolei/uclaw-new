use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Per-tool and per-turn character budget to prevent context explosion.
pub struct ToolBudgetManager {
    per_tool: HashMap<String, usize>,
    default_limit: usize,
    preview_chars: usize,
    overflow_dir: PathBuf,
}

impl ToolBudgetManager {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let overflow_dir = data_dir.join("tool-results");
        fs::create_dir_all(&overflow_dir).ok();
        let mut per_tool = HashMap::new();
        per_tool.insert("shell".into(), 20_000);
        per_tool.insert("bash".into(), 20_000);
        per_tool.insert("read_file".into(), usize::MAX);
        Self {
            per_tool,
            default_limit: 8_000,
            preview_chars: 500,
            overflow_dir,
        }
    }

    /// Apply budget to a tool result. Returns the (possibly truncated) result.
    pub fn apply(
        &self,
        tool_name: &str,
        result: String,
        session_id: &str,
        turn_index: u32,
    ) -> String {
        let limit = self.per_tool.get(tool_name).copied().unwrap_or(self.default_limit);
        if result.len() <= limit {
            return result;
        }

        // Write full result to overflow file
        let safe_session = session_id.replace(['/', '\\', '.'], "_");
        let safe_tool = tool_name.replace(['/', '\\', '.'], "_");
        let session_dir = self.overflow_dir.join(&safe_session);
        fs::create_dir_all(&session_dir).ok();
        let file_path = session_dir.join(format!("{}-{}.txt", turn_index, safe_tool));
        if let Err(e) = fs::write(&file_path, result.as_bytes()) {
            tracing::warn!("ToolBudgetManager: failed to write overflow file: {e}");
        }

        // Safe UTF-8 preview truncation
        let preview_end = result.floor_char_boundary(self.preview_chars.min(result.len()));
        let preview = &result[..preview_end];
        format!(
            "[Result truncated ({} chars). Full output saved to {} — use read_file to access it.]\n\n{}...",
            result.len(),
            file_path.display(),
            preview,
        )
    }
}
