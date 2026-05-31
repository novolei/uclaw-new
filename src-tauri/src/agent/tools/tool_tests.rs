use super::*;

#[test]
fn kinded_error_displays_with_bracketed_kind() {
    let err = ToolError::kinded(ToolErrorKind::ResourceNotFound, "Page returned 404");
    assert_eq!(format!("{}", err), "[NotFound] Page returned 404");
}

#[test]
fn kinded_error_serializes_through_existing_serde_path() {
    let err = ToolError::kinded(ToolErrorKind::PermissionDenied, "URL blocked");
    let json = serde_json::to_string(&err).unwrap();
    assert!(json.contains("PermissionDenied"), "got json: {}", json);
    assert!(json.contains("URL blocked"), "got json: {}", json);
}

#[test]
fn kinded_with_source_keeps_source_field() {
    let err = ToolError::kinded_with_source(
        ToolErrorKind::ParseError,
        "Could not parse JSON",
        "expected ',' at line 5",
    );
    match err {
        ToolError::Kinded {
            kind,
            message,
            source_context,
        } => {
            assert_eq!(kind, ToolErrorKind::ParseError);
            assert_eq!(message, "Could not parse JSON");
            assert_eq!(source_context.as_deref(), Some("expected ',' at line 5"));
        }
        _ => panic!("expected Kinded variant"),
    }
}

#[test]
fn tool_context_derives_subcall_without_losing_parent_context() {
    let ctx = ToolExecutionContext::agent_turn(
        "session-1",
        "tool-1",
        Some(PathBuf::from("/tmp/workspace")),
        Some(SafetyMode::Supervised),
    );

    let sub = ctx.for_subcall("tool-2");

    assert_eq!(sub.session_id, "session-1");
    assert_eq!(sub.tool_call_id, "tool-2");
    assert_eq!(
        sub.workspace_root.as_deref(),
        Some(Path::new("/tmp/workspace"))
    );
    assert_eq!(sub.execution_mode, ToolExecutionMode::AgentTurn);
    assert_eq!(sub.safety_mode, Some(SafetyMode::Supervised));
}

#[test]
fn tool_context_resolves_relative_paths_against_workspace_root() {
    let ctx = ToolExecutionContext::agent_turn(
        "session-1",
        "tool-1",
        Some(PathBuf::from("/tmp/workspace")),
        None,
    );

    assert_eq!(
        ctx.resolve_candidate_path("src/lib.rs"),
        PathBuf::from("/tmp/workspace/src/lib.rs"),
    );
    assert_eq!(
        ctx.resolve_candidate_path("/var/tmp/file.txt"),
        PathBuf::from("/var/tmp/file.txt"),
    );
}

#[test]
fn tool_context_without_workspace_keeps_relative_path_relative() {
    let ctx = ToolExecutionContext::agent_turn("session-1", "tool-1", None, None);

    assert_eq!(
        ctx.resolve_candidate_path("src/lib.rs"),
        PathBuf::from("src/lib.rs"),
    );
}

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "echoes input"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::new(params, 7))
    }
}

#[tokio::test]
async fn execute_tool_with_context_preserves_old_execute_behavior() {
    let ctx = ToolExecutionContext::agent_turn(
        "session-1",
        "tool-1",
        Some(PathBuf::from("/tmp/workspace")),
        Some(SafetyMode::Plan),
    );

    let output = execute_tool_with_context(&EchoTool, serde_json::json!({"hello": "world"}), &ctx)
        .await
        .unwrap();

    assert_eq!(output.result, serde_json::json!({"hello": "world"}));
    assert_eq!(output.duration_ms, 7);
}

// ── sanitize_tool_name tests (tns.1) ─────────────────────────────────────────

#[test]
fn sanitize_passthrough_valid() {
    assert_eq!(super::sanitize_tool_name("read_file"), "read_file");
    assert_eq!(super::sanitize_tool_name("mcp__gitnexus__api_impact"), "mcp__gitnexus__api_impact");
    assert_eq!(super::sanitize_tool_name("a-b_c9"), "a-b_c9");
}

#[test]
fn sanitize_replaces_invalid_chars() {
    assert_eq!(super::sanitize_tool_name("mcp__srv__foo.bar"), "mcp__srv__foo_bar");
    assert_eq!(super::sanitize_tool_name("a b/c:d"), "a_b_c_d");
}

#[test]
fn sanitize_empty_falls_back() {
    assert_eq!(super::sanitize_tool_name(""), "unnamed_tool");
}

#[test]
fn sanitize_truncates_to_64() {
    assert_eq!(super::sanitize_tool_name(&"a".repeat(100)).len(), 64);
}

#[test]
fn sanitize_result_always_matches_pattern() {
    for raw in ["read_file", "mcp__s__a.b", "工具", "", "x/y z:1", &"q".repeat(80)] {
        let s = super::sanitize_tool_name(raw);
        assert!(!s.is_empty() && s.len() <= 64);
        assert!(
            s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "sanitized {raw:?} → {s:?} has invalid chars"
        );
    }
}
