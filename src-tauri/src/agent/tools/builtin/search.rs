use async_trait::async_trait;
use std::path::PathBuf;
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct GrepTool { workspace_root: PathBuf }

impl GrepTool {
    pub fn new(workspace_root: PathBuf) -> Self { Self { workspace_root } }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }
    fn description(&self) -> &str {
        "Search for a pattern in files within a directory. Returns matching lines with file paths and line numbers."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "The regex pattern to search for"},
                "path": {"type": "string", "description": "Directory to search in (relative to workspace root)"},
                "include": {"type": "string", "description": "Optional glob to filter files (e.g. '*.rs')"}
            },
            "required": ["pattern"]
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
        let pattern = params["pattern"].as_str().ok_or_else(|| ToolError::InvalidParams("pattern is required".into()))?;
        let search_path = params["path"].as_str().map(|p| {
            if PathBuf::from(p).is_absolute() { PathBuf::from(p) } else { self.workspace_root.join(p) }
        }).unwrap_or_else(|| self.workspace_root.clone());
        let include_glob = params["include"].as_str();

        let re = regex::Regex::new(pattern).map_err(|e| ToolError::InvalidParams(format!("Invalid regex: {}", e)))?;
        let mut results = Vec::new();

        self.search_dir(&search_path, &re, include_glob, &mut results).await?;

        let output = if results.is_empty() {
            "No matches found.".to_string()
        } else {
            results.join("\n")
        };
        Ok(ToolOutput::success(&output, start.elapsed().as_millis() as u64))
    }
}

impl GrepTool {
    async fn search_dir(&self, dir: &PathBuf, re: &regex::Regex, include: Option<&str>, results: &mut Vec<String>) -> Result<(), ToolError> {
        let mut entries = tokio::fs::read_dir(dir).await.map_err(|e| {
            let kind = if e.kind() == std::io::ErrorKind::NotFound {
                ToolErrorKind::ResourceNotFound
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                ToolErrorKind::PermissionDenied
            } else {
                ToolErrorKind::Other
            };
            ToolError::kinded_with_source(kind, format!("Cannot read dir: {}", dir.display()), e.to_string())
        })?;
        while let Some(entry) = entries.next_entry().await.map_err(|e| ToolError::kinded_with_source(
            ToolErrorKind::Other,
            "Dir entry error",
            e.to_string(),
        ))? {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                // Descend into .uclaw (plans/config); skip .git and heavy build dirs.
                let skip = (name.starts_with('.') && name != ".uclaw")
                    || name == "node_modules"
                    || name == "target";
                if !skip {
                    Box::pin(self.search_dir(&path, re, include, results)).await?;
                }
            } else if path.is_file() {
                if let Some(glob) = include {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !self.match_glob(name, glob) { continue; }
                }
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    for (line_num, line) in content.lines().enumerate() {
                        if re.is_match(line) {
                            let relative = path.strip_prefix(&self.workspace_root).unwrap_or(&path);
                            results.push(format!("{}:{}: {}", relative.display(), line_num + 1, line));
                            if results.len() >= 50 { return Ok(()); }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn match_glob(&self, name: &str, glob: &str) -> bool {
        if glob == "*" || glob == "*.*" { return true; }
        if let Some(ext) = glob.strip_prefix("*.") {
            return name.ends_with(ext);
        }
        name == glob
    }
}

pub struct GlobTool { workspace_root: PathBuf }

impl GlobTool {
    pub fn new(workspace_root: PathBuf) -> Self { Self { workspace_root } }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str { "glob" }
    fn description(&self) -> &str {
        "Find files matching a glob pattern. Returns a list of matching file paths."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Glob pattern to match (e.g. 'src/**/*.rs')"},
                "path": {"type": "string", "description": "Directory to search in (relative to workspace root)"}
            },
            "required": ["pattern"]
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
        let pattern = params["pattern"].as_str().ok_or_else(|| ToolError::InvalidParams("pattern is required".into()))?;
        let search_path = params["path"].as_str().map(|p| {
            if PathBuf::from(p).is_absolute() { PathBuf::from(p) } else { self.workspace_root.join(p) }
        }).unwrap_or_else(|| self.workspace_root.clone());

        let mut results = Vec::new();
        self.glob_dir(&search_path, pattern, &search_path, &mut results).await?;

        let output = if results.is_empty() {
            "No files found.".to_string()
        } else {
            results.sort();
            results.join("\n")
        };
        Ok(ToolOutput::success(&output, start.elapsed().as_millis() as u64))
    }
}

impl GlobTool {
    async fn glob_dir(&self, dir: &PathBuf, pattern: &str, base: &PathBuf, results: &mut Vec<String>) -> Result<(), ToolError> {
        let mut entries = tokio::fs::read_dir(dir).await.map_err(|e| {
            let kind = if e.kind() == std::io::ErrorKind::NotFound {
                ToolErrorKind::ResourceNotFound
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                ToolErrorKind::PermissionDenied
            } else {
                ToolErrorKind::Other
            };
            ToolError::kinded_with_source(kind, format!("Cannot read dir: {}", dir.display()), e.to_string())
        })?;
        while let Some(entry) = entries.next_entry().await.map_err(|e| ToolError::kinded_with_source(
            ToolErrorKind::Other,
            "Dir entry error",
            e.to_string(),
        ))? {
            let path = entry.path();
            let relative = path.strip_prefix(base).unwrap_or(&path);
            let relative_str = relative.to_string_lossy();

            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                // Descend into .uclaw (plans/config); skip .git and heavy build dirs.
                let skip = (name.starts_with('.') && name != ".uclaw")
                    || name == "node_modules"
                    || name == "target";
                if !skip {
                    // Check if directory matches pattern for recursive glob
                    let dir_pattern = format!("{}/**", relative_str);
                    if self.simple_glob_match(&dir_pattern, pattern) || pattern.contains("**") {
                        Box::pin(self.glob_dir(&path, pattern, base, results)).await?;
                    }
                }
            } else if path.is_file() {
                if self.simple_glob_match(&relative_str, pattern) {
                    results.push(relative_str.to_string());
                    if results.len() >= 100 { return Ok(()); }
                }
            }
        }
        Ok(())
    }

    fn simple_glob_match(&self, path: &str, pattern: &str) -> bool {
        if pattern == "*" || pattern == "**/*" { return true; }
        if let Some(suffix) = pattern.strip_prefix("**/*.") {
            return path.ends_with(suffix);
        }
        if let Some(suffix) = pattern.strip_prefix("*.") {
            return path.ends_with(suffix);
        }
        if pattern.starts_with("**/") {
            let suffix = &pattern[3..];
            if suffix.contains('*') {
                if let Some((prefix, ext)) = suffix.rsplit_once("*.") {
                    return path.starts_with(prefix) && path.ends_with(ext);
                }
            }
            return path.ends_with(suffix);
        }
        path.contains(pattern) || path == pattern
    }
}

#[cfg(test)]
mod path_args_tests {
    use super::*;
    use crate::agent::tools::tool::Tool;

    #[test]
    fn grep_path_args_returns_path_when_present() {
        let tool = GrepTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"pattern": "TODO", "path": "src/"});
        assert_eq!(tool.path_args(&args), vec!["src/"]);
    }

    #[test]
    fn grep_path_args_empty_when_absent() {
        let tool = GrepTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"pattern": "TODO"});
        assert!(tool.path_args(&args).is_empty());
    }

    #[test]
    fn glob_path_args_returns_path_when_present() {
        let tool = GlobTool::new(std::path::PathBuf::from("/tmp"));
        let args = serde_json::json!({"pattern": "**/*.rs", "path": "src/"});
        assert_eq!(tool.path_args(&args), vec!["src/"]);
    }
}
