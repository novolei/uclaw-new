use async_trait::async_trait;
use std::path::{Path, PathBuf};
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct GrepTool { workspace_root: PathBuf }

impl GrepTool {
    pub fn new(workspace_root: PathBuf) -> Self { Self { workspace_root } }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }
    fn description(&self) -> &str {
        "Search for a pattern in files within a directory. Returns matching lines with file paths and line numbers. \
         Prefer this over shell `grep`/`rg` — workspace-scoped and respects the working tree."
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

        // Validate regex eagerly — same InvalidParams error regardless of which backend runs.
        let re = regex::Regex::new(pattern).map_err(|e| ToolError::InvalidParams(format!("Invalid regex: {}", e)))?;

        let results = match self.try_ripgrep(&search_path, pattern, include_glob).await {
            Some(r) => r,
            None => {
                let mut r = Vec::new();
                self.search_dir(&search_path, &re, include_glob, &mut r).await?;
                r
            }
        };

        let output = if results.is_empty() {
            "No matches found.".to_string()
        } else {
            results.join("\n")
        };
        Ok(ToolOutput::success(&output, start.elapsed().as_millis() as u64))
    }
}

/// Parse `rg --line-number --no-heading` stdout (lines `<abs_path>:<line>:<text>`)
/// into the existing grep output format, workspace-relative, capped at 50.
/// Split one rg `--no-heading --line-number` line into `(path, line_num, text)`.
///
/// The format is `<path>:<line>:<text>`, but a naive `splitn(3, ':')` mis-parses
/// Windows paths that contain a drive-letter colon (`C:\ws\a.rs:12:hello`). So
/// locate the FIRST `:<ascii-digits>:` field — that boundary unambiguously marks
/// the line-number column on every platform. Byte-safe (splits on ASCII `:`).
fn split_rg_line(line: &str) -> Option<(&str, &str, &str)> {
    let mut search_from = 0;
    while let Some(off) = line[search_from..].find(':') {
        let colon = search_from + off;
        let rest = &line[colon + 1..];
        if let Some(next_off) = rest.find(':') {
            let num = &rest[..next_off];
            if !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()) {
                return Some((&line[..colon], num, &rest[next_off + 1..]));
            }
        }
        search_from = colon + 1;
    }
    None
}

fn parse_rg_output(stdout: &str, workspace_root: &Path) -> Vec<String> {
    let mut results = Vec::new();
    for raw_line in stdout.lines() {
        // `str::lines()` already strips trailing `\r\n`, so no carriage-return
        // creeps into `text` on Windows. Path/line split handled drive-aware.
        let (abs_path, line_num, text) = match split_rg_line(raw_line) {
            Some(t) => t,
            None => continue,
        };
        let relative = Path::new(abs_path)
            .strip_prefix(workspace_root)
            .unwrap_or(Path::new(abs_path));
        results.push(format!("{}:{}: {}", relative.display(), line_num, text));
        if results.len() >= 50 {
            break;
        }
    }
    results
}

/// Map an `include` glob to an rg `--glob` argument.
/// `*` and `*.*` are wildcard — no filtering needed; everything else is passed through.
fn include_to_rg_glob(include: Option<&str>) -> Option<String> {
    match include {
        None => None,
        Some("*") | Some("*.*") => None,
        Some(g) => Some(g.to_string()),
    }
}

impl GrepTool {
    /// Attempt to run ripgrep and return parsed results, or `None` to signal fallback.
    async fn try_ripgrep(&self, search_path: &Path, pattern: &str, include: Option<&str>) -> Option<Vec<String>> {
        use tokio::process::Command;
        use std::process::Stdio;

        let mut cmd = Command::new("rg");
        cmd.args(["--line-number", "--no-heading", "--color=never",
                  "--glob", "!target", "--glob", "!node_modules"]);
        if let Some(glob) = include_to_rg_glob(include) {
            cmd.args(["--glob", &glob]);
        }
        cmd.args(["-e", pattern]);
        cmd.arg(search_path);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let output = match cmd.output().await {
            Ok(o) => o,
            Err(_) => return None, // rg not installed / spawn fail → fall back
        };

        match output.status.code() {
            Some(0) => {
                // Matches found.
                let stdout = String::from_utf8_lossy(&output.stdout);
                Some(parse_rg_output(&stdout, &self.workspace_root))
            }
            Some(1) => {
                // rg exit code 1 = no matches (not an error).
                Some(vec![])
            }
            _ => {
                // Exit code 2+ or signal: error condition — fall back to walker.
                None
            }
        }
    }

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

#[cfg(test)]
mod rg_fast_path_tests {
    use super::*;
    use std::path::Path;

    // ── Test 1: parse_rg_output ──────────────────────────────────────────────

    #[test]
    fn parse_rg_output_normal_lines() {
        let ws = Path::new("/ws");
        let stdout = "/ws/src/a.rs:12:foo bar\n/ws/src/b.rs:3:hello\n/ws/lib.rs:99:end";
        let out = parse_rg_output(stdout, ws);
        assert_eq!(out.len(), 3);
        // Verify exact format: relative path, colon, line_num, colon-space, text
        assert_eq!(out[0], "src/a.rs:12: foo bar");
        assert_eq!(out[1], "src/b.rs:3: hello");
        assert_eq!(out[2], "lib.rs:99: end");
    }

    #[test]
    fn parse_rg_output_malformed_line_skipped() {
        let ws = Path::new("/ws");
        // Line with no colons → skipped; valid line still parsed.
        let stdout = "no_colons_at_all\n/ws/src/a.rs:5:valid line";
        let out = parse_rg_output(stdout, ws);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], "src/a.rs:5: valid line");
    }

    #[test]
    fn parse_rg_output_only_one_colon_skipped() {
        let ws = Path::new("/ws");
        // Only 2 parts → skipped.
        let stdout = "/ws/src/a.rs:5\n/ws/src/b.rs:7:good";
        let out = parse_rg_output(stdout, ws);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], "src/b.rs:7: good");
    }

    #[test]
    fn parse_rg_output_capped_at_50() {
        let ws = Path::new("/ws");
        // Build 60 valid lines.
        let lines: Vec<String> = (1..=60)
            .map(|i| format!("/ws/src/a.rs:{}:line content", i))
            .collect();
        let stdout = lines.join("\n");
        let out = parse_rg_output(&stdout, ws);
        assert_eq!(out.len(), 50, "should cap at 50 results");
    }

    #[test]
    fn parse_rg_output_text_with_colons_preserved() {
        // Text field may itself contain colons; splitn(3) keeps them intact.
        let ws = Path::new("/ws");
        let stdout = "/ws/file.rs:1:key: value: extra";
        let out = parse_rg_output(stdout, ws);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], "file.rs:1: key: value: extra");
    }

    #[test]
    fn parse_rg_output_path_outside_workspace_uses_full_path() {
        // If strip_prefix fails, the full path is used (graceful degradation).
        let ws = Path::new("/other");
        let stdout = "/ws/src/a.rs:3:text";
        let out = parse_rg_output(stdout, ws);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], "/ws/src/a.rs:3: text");
    }

    #[test]
    fn split_rg_line_handles_windows_drive_letter() {
        // A naive splitn(3, ':') would mis-parse the drive-letter colon. The
        // `:<digits>:` boundary scan must pick the line-number column correctly.
        let (path, num, text) = split_rg_line(r"C:\ws\a.rs:12:hello: world").unwrap();
        assert_eq!(path, r"C:\ws\a.rs");
        assert_eq!(num, "12");
        assert_eq!(text, "hello: world");
    }

    #[test]
    fn split_rg_line_no_numeric_field_is_none() {
        // `host:port` style with no `:<digits>:` line column → not a match line.
        assert!(split_rg_line("just-a-path-no-colon").is_none());
        assert!(split_rg_line("a:b:c").is_none()); // 'b' is not all-digits
    }

    // ── Test 2: include_to_rg_glob ───────────────────────────────────────────

    #[test]
    fn include_to_rg_glob_star_produces_none() {
        assert_eq!(include_to_rg_glob(Some("*")), None);
    }

    #[test]
    fn include_to_rg_glob_star_dot_star_produces_none() {
        assert_eq!(include_to_rg_glob(Some("*.*")), None);
    }

    #[test]
    fn include_to_rg_glob_none_produces_none() {
        assert_eq!(include_to_rg_glob(None), None);
    }

    #[test]
    fn include_to_rg_glob_rs_extension_passthrough() {
        assert_eq!(include_to_rg_glob(Some("*.rs")), Some("*.rs".to_string()));
    }

    #[test]
    fn include_to_rg_glob_ts_extension_passthrough() {
        assert_eq!(include_to_rg_glob(Some("*.ts")), Some("*.ts".to_string()));
    }

    #[test]
    fn include_to_rg_glob_explicit_filename_passthrough() {
        assert_eq!(include_to_rg_glob(Some("Cargo.toml")), Some("Cargo.toml".to_string()));
    }

    // ── Test 3: execute integration (works with rg present OR absent) ────────

    #[tokio::test]
    async fn execute_finds_hello_world_in_tempdir() {
        use crate::agent::tools::tool::Tool;

        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "hello world\nsecond line\n").expect("write");

        let tool = GrepTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({"pattern": "hello"}))
            .await
            .expect("execute ok");

        let text = result.result["content"].as_str().unwrap_or("");
        assert!(
            text.contains("a.txt:1: hello world"),
            "expected 'a.txt:1: hello world' in output, got: {text:?}"
        );
    }

    // ── Test 4: no-match path says "No matches found." ───────────────────────

    #[tokio::test]
    async fn execute_no_match_returns_no_matches_found() {
        use crate::agent::tools::tool::Tool;

        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("b.txt");
        std::fs::write(&file, "nothing relevant here\n").expect("write");

        let tool = GrepTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({"pattern": "zzz_does_not_exist_zzz"}))
            .await
            .expect("execute ok");

        let text = result.result["content"].as_str().unwrap_or("");
        assert_eq!(text, "No matches found.");
    }

    // ── Bonus: invalid regex still errors immediately ─────────────────────────

    #[tokio::test]
    async fn execute_invalid_regex_returns_error() {
        use crate::agent::tools::tool::Tool;

        let dir = tempfile::tempdir().expect("tempdir");
        let tool = GrepTool::new(dir.path().to_path_buf());
        let err = tool
            .execute(serde_json::json!({"pattern": "[invalid"}))
            .await
            .expect_err("should error on bad regex");

        let msg = format!("{err:?}");
        assert!(msg.contains("Invalid regex"), "expected InvalidParams error, got: {msg}");
    }
}
