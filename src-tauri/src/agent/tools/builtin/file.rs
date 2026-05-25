use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput};

/// FNV-1a 32-bit hash of file content. Delegates to anchor_state::fnv1a_32
/// (same algorithm Dirac uses for contentHash in src/utils/line-hashing.ts)
/// so hash values are algorithm-compatible across the codebase.
///
/// Reuses `anchor_state::fnv1a_32` — no duplication of the algorithm.
/// [C1-Dirac-A3]
pub fn compute_file_hash(content: &str) -> u32 {
    crate::agent::anchor_state::fnv1a_32(content.as_bytes())
}

/// Parse an `assume_hash` parameter string. Accepts `0xABCD1234` or
/// `0XABCD1234` (case-insensitive prefix + 8 hex digits). Returns `None`
/// for any other input. Per spec §8.4 the caller converts `None` to
/// `ToolError::InvalidParams` (malformed input is rejected, not silently
/// ignored) so a corrupt hash surfaces a clear error instead of a silent re-read.
fn parse_assume_hash(s: &str) -> Option<u32> {
    let stripped = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if stripped.len() != 8 || !stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(stripped, 16).ok()
}

pub struct ReadFileTool { workspace_root: PathBuf }

impl ReadFileTool {
    pub fn new(workspace_root: PathBuf) -> Self { Self { workspace_root } }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str {
        "Read the contents of a file. Returns a [File Hash: 0x...] header line, then each \
         file line prefixed with a stable anchor token and the § delimiter, e.g. \
         `Apple§    def process(data):`. The anchor tokens stay stable across reads for \
         unchanged lines (Myers-diff carry-forward) — pass one as the `anchor`/`end_anchor` \
         parameter of `edit` to target an edit precisely. \
         For repeated reads of the same file, pass the prior hash as `assume_hash` — \
         if the file is unchanged the tool short-circuits with a one-line confirmation \
         instead of re-emitting the full content."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Relative or absolute path to the file to read"},
                "assume_hash": {
                    "type": "string",
                    "description": "Optional. If you saw `[File Hash: 0x...]` on a prior read of this file, \
                                    pass that value here. If the file hasn't changed since, the tool returns \
                                    a short 'no changes' confirmation instead of the full content — saves \
                                    significant tokens on repeated reads of large files. \
                                    Format: 0x-prefixed 8-char hex, e.g. \"0xab12cd34\".",
                    "pattern": "^0[xX][0-9a-fA-F]{8}$"
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

        let current_hash = compute_file_hash(&content);
        let header = format!("[File Hash: 0x{:08x}]", current_hash);

        // Resolve optional assume_hash — malformed value is an error (spec §8.4).
        let provided_hash = match params.get("assume_hash").and_then(|v| v.as_str()) {
            Some(s) => match parse_assume_hash(s) {
                Some(h) => Some(h),
                None => return Err(ToolError::InvalidParams(format!(
                    "Invalid assume_hash: {:?}. Expected 0x-prefixed 8-char hex (e.g., \"0xab12cd34\").",
                    s
                ))),
            },
            None => None,
        };

        let output = if Some(current_hash) == provided_hash {
            // Short-circuit: file unchanged since last read — no content re-emit.
            // Matches Dirac ReadFileToolHandler.ts:293-294.
            format!(
                "{}\nno changes have been made to the file since your last read (Hash: 0x{:08x})",
                header, current_hash,
            )
        } else {
            // B1: emit one `<token>§<literal line>` per line after the header.
            // record_read aligns tokens against the prior read via Myers diff so
            // unchanged lines keep their tokens across reads (spec §3.3 / §4.3).
            let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
            let anchors = crate::agent::anchor_state::GLOBAL_ANCHOR_STATE_MANAGER
                .record_read(&full_path, &lines);

            // Track the file for external-modification detection so the
            // EditTool stale-file gate (spec §3.6) has something to check.
            crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER.track_file(&full_path);

            let mut out = String::with_capacity(content.len() + lines.len() * 12 + header.len() + 1);
            out.push_str(&header);
            out.push('\n');
            for (token, line) in anchors.iter().zip(lines.iter()) {
                out.push_str(&crate::agent::anchor_state::render_anchor_line(token, line));
                out.push('\n');
            }
            // Preserve the original's trailing-newline shape: `str::lines`
            // already dropped a final newline, so only trim our own trailing
            // '\n' when the source did NOT end with one.
            if !content.ends_with('\n') {
                out.pop();
            }
            out
        };

        Ok(ToolOutput::success(&output, start.elapsed().as_millis() as u64))
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

    // ── Dirac-A3: [File Hash] header + assume_hash short-circuit tests ──

    /// Verifies FNV-1a 32-bit known test vectors. These are the published
    /// FNV-1a 32 reference values and lock the algorithm identity.
    /// If `anchor_state::fnv1a_32` ever changes, this test will catch it.
    /// [C1-Dirac-A3]
    #[test]
    fn test_compute_file_hash_fnv1a_known_values() {
        assert_eq!(compute_file_hash(""), 0x811c9dc5u32,
            "FNV-1a 32 of empty string must equal offset basis 0x811c9dc5");
        assert_eq!(compute_file_hash("a"), 0xe40c292cu32,
            "FNV-1a 32 of \"a\" must equal 0xe40c292c");
        assert_eq!(compute_file_hash("foobar"), 0xbf9cf968u32,
            "FNV-1a 32 of \"foobar\" must equal 0xbf9cf968");
    }

    /// Every read emits a `[File Hash: 0x...]` header as the first line,
    /// followed by the actual file content. [C1-Dirac-A3]
    #[tokio::test]
    async fn test_read_emits_file_hash_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("foo.txt");
        tokio::fs::write(&path, "hello world").await.unwrap();
        let tool = ReadFileTool::new(dir.path().to_path_buf());
        let result = tool.execute(serde_json::json!({"path": "foo.txt"})).await.unwrap();
        let out = result.result["content"].as_str().unwrap();
        assert!(out.starts_with("[File Hash: 0x"), "header missing; got: {}", out);
        // B1: each line is now anchor-prefixed (`<token>§<line>`), so the raw
        // content appears after the § delimiter rather than after a bare \n.
        assert!(out.contains("§hello world"), "anchored file content missing; got: {}", out);
    }

    /// Matching assume_hash → short-circuit message returned instead of full content.
    /// [C1-Dirac-A3]
    #[tokio::test]
    async fn test_read_short_circuits_on_matching_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stable.txt");
        tokio::fs::write(&path, "stable content").await.unwrap();
        let tool = ReadFileTool::new(dir.path().to_path_buf());

        // First read — capture hash from header.
        let first = tool.execute(serde_json::json!({"path": "stable.txt"})).await.unwrap();
        let first_out = first.result["content"].as_str().unwrap();
        // Header is `[File Hash: 0xXXXXXXXX]`, extract the hex value.
        let header_line = first_out.split('\n').next().unwrap();
        let hash_hex = header_line
            .trim_start_matches("[File Hash: ")
            .trim_end_matches(']');

        // Second read with assume_hash — should short-circuit.
        let second = tool.execute(serde_json::json!({
            "path": "stable.txt",
            "assume_hash": hash_hex,
        })).await.unwrap();
        let second_out = second.result["content"].as_str().unwrap();
        assert!(second_out.contains("no changes have been made"),
            "expected short-circuit message; got: {}", second_out);
        assert!(!second_out.contains("stable content"),
            "full content must not be re-emitted on short-circuit; got: {}", second_out);
    }

    /// Stale (mismatched) assume_hash → full content returned with updated header.
    /// [C1-Dirac-A3]
    #[tokio::test]
    async fn test_read_returns_content_on_mismatched_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("foo.txt");
        tokio::fs::write(&path, "new content").await.unwrap();
        let tool = ReadFileTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({
            "path": "foo.txt",
            "assume_hash": "0x00000000",
        })).await.unwrap();
        let out = result.result["content"].as_str().unwrap();
        assert!(out.contains("new content"),
            "full content must be returned on hash mismatch; got: {}", out);
        assert!(!out.contains("no changes have been made"),
            "short-circuit message must not appear on mismatch; got: {}", out);
    }

    /// No assume_hash param → full content returned normally. [C1-Dirac-A3]
    #[tokio::test]
    async fn test_read_returns_content_when_no_assume_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("foo.txt");
        tokio::fs::write(&path, "first read content").await.unwrap();
        let tool = ReadFileTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({"path": "foo.txt"})).await.unwrap();
        let out = result.result["content"].as_str().unwrap();
        assert!(out.contains("[File Hash:"),
            "hash header must be present; got: {}", out);
        assert!(out.contains("first read content"),
            "full content must be present; got: {}", out);
    }

    /// Malformed assume_hash (not valid 0x-prefixed 8-char hex) →
    /// `ToolError::InvalidParams` with message containing "Invalid assume_hash".
    /// Matches spec §8.4: reject, do not silently ignore. [C1-Dirac-A3]
    #[tokio::test]
    async fn test_read_rejects_malformed_assume_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("foo.txt");
        tokio::fs::write(&path, "x").await.unwrap();
        let tool = ReadFileTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({
            "path": "foo.txt",
            "assume_hash": "not-hex",
        })).await;
        match result {
            Err(ToolError::InvalidParams(msg)) => {
                assert!(msg.contains("Invalid assume_hash"),
                    "error message must contain 'Invalid assume_hash'; got: {}", msg);
            }
            other => panic!("expected InvalidParams, got: {:?}", other),
        }
    }

    // ── Dirac-B1: anchored read output tests ──

    /// read_file emits `[File Hash: 0x...]` then one `<token>§<line>` per
    /// file line. (spec §5 test #11)
    #[tokio::test]
    async fn read_file_emits_anchored_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("anchored.rs");
        tokio::fs::write(&path, "fn foo() {\n    bar();\n}\n").await.unwrap();
        let tool = ReadFileTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({"path": "anchored.rs"})).await.unwrap();
        let out = result.result["content"].as_str().unwrap();

        let mut iter = out.split('\n');
        let header = iter.next().unwrap();
        assert!(header.starts_with("[File Hash: 0x"), "first line is header; got: {}", header);

        let expected = ["fn foo() {", "    bar();", "}"];
        for exp in expected {
            let line = iter.next().expect("an anchored line per source line");
            let (token, content) = line
                .split_once('§')
                .unwrap_or_else(|| panic!("line must be `<token>§<content>`; got: {:?}", line));
            assert!(!token.is_empty(), "token must be non-empty; got: {:?}", line);
            assert!(
                token.chars().next().unwrap().is_ascii_uppercase(),
                "token must start uppercase; got: {:?}",
                token
            );
            assert_eq!(content, exp, "content after § must be the literal line");
        }
    }

    /// Re-reading an unmodified file yields a byte-identical anchor section
    /// (tokens carry forward via Myers diff). (spec §5 test #12)
    #[tokio::test]
    async fn read_file_anchor_stability_across_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stable_anchors.rs");
        tokio::fs::write(&path, "alpha\nbeta\ngamma\n").await.unwrap();
        let tool = ReadFileTool::new(dir.path().to_path_buf());

        let first = tool.execute(serde_json::json!({"path": "stable_anchors.rs"})).await.unwrap();
        let second = tool.execute(serde_json::json!({"path": "stable_anchors.rs"})).await.unwrap();

        // Strip the [File Hash:] header line; compare the anchor section.
        let anchor_section = |s: &str| s.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap();
        let a = anchor_section(first.result["content"].as_str().unwrap());
        let b = anchor_section(second.result["content"].as_str().unwrap());
        assert_eq!(a, b, "anchor section must be byte-stable across identical re-reads");
    }
}
