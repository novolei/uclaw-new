use super::super::PlaywrightMcpAction;
use super::*;

#[test]
fn maps_browser_action_to_allowlisted_mcp_tool() {
    let call = PlaywrightMcpAdapterToolCall::navigate("https://example.test");

    assert_eq!(call.server_id, PLAYWRIGHT_MCP_SERVER_ID);
    assert_eq!(call.tool_name, "browser_navigate");
    assert_eq!(call.arguments["url"], "https://example.test");
}

#[test]
fn rejects_unknown_raw_tool() {
    let err = validate_playwright_mcp_tool("browser_press_key").unwrap_err();

    assert_eq!(err, PlaywrightMcpAdapterError::RawToolNotAllowed);
}

#[test]
fn maps_uclaw_actions_without_exposing_raw_tool_names_to_callers() {
    let call = PlaywrightMcpAdapterToolCall::from_action(&PlaywrightMcpAction::Type {
        locator: "Name input".to_string(),
        text: "Ada".to_string(),
    })
    .expect("type maps");

    assert_eq!(call.server_id, "playwright");
    assert_eq!(call.tool_name, "browser_type");
    assert_eq!(call.arguments["text"], "Ada");
}
