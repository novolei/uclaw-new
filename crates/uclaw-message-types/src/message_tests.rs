use super::*;

#[test]
fn chat_message_wire_shape_preserves_role_and_content_type() {
    let msg = ChatMessage::user("hello");
    let value = serde_json::to_value(&msg).unwrap();
    assert_eq!(value["role"], "user");
    assert_eq!(value["content"][0]["type"], "text");
    assert_eq!(value["compacted"], false);
}

#[test]
fn tool_result_helper_preserves_error_flag() {
    let msg = ChatMessage::user_tool_result("call-1", "failed", true);
    let value = serde_json::to_value(&msg).unwrap();
    assert_eq!(value["content"][0]["type"], "tool_result");
    assert_eq!(value["content"][0]["tool_use_id"], "call-1");
    assert_eq!(value["content"][0]["is_error"], true);
}

#[test]
fn cjk_estimator_counts_chinese_more_heavily_than_ascii() {
    assert!(estimate_tokens("你好世界") > estimate_tokens("hello"));
}

#[test]
fn assistant_from_response_with_thinking_text_and_tool_uses() {
    let msg = ChatMessage::assistant_from_response(
        Some("reasoning..."),
        Some("sig-abc".to_string()),
        "I'll call two tools.",
        vec![
            ("call_1".to_string(), "bash".to_string(), serde_json::json!({"cmd": "ls"})),
            ("call_2".to_string(), "read_file".to_string(), serde_json::json!({"path": "/tmp/x"})),
        ],
    );

    assert_eq!(msg.role, MessageRole::Assistant);
    assert!(!msg.compacted);
    assert_eq!(msg.content.len(), 4);  // 1 Thinking + 1 Text + 2 ToolUse

    match &msg.content[0] {
        ContentBlock::Thinking { thinking, signature } => {
            assert_eq!(thinking, "reasoning...");
            assert_eq!(signature.as_deref(), Some("sig-abc"));
        }
        other => panic!("expected Thinking, got {:?}", other),
    }
    assert!(matches!(&msg.content[1], ContentBlock::Text { text } if text == "I'll call two tools."));
    assert!(matches!(&msg.content[2], ContentBlock::ToolUse { id, .. } if id == "call_1"));
    assert!(matches!(&msg.content[3], ContentBlock::ToolUse { id, .. } if id == "call_2"));
}

#[test]
fn assistant_from_response_empty_thinking_is_omitted() {
    let msg = ChatMessage::assistant_from_response(
        Some(""),  // empty thinking → no Thinking block
        None,
        "just text",
        std::iter::empty(),
    );
    assert_eq!(msg.content.len(), 1);  // only Text
    assert!(matches!(&msg.content[0], ContentBlock::Text { .. }));
}

#[test]
fn assistant_from_response_no_thinking_no_tools() {
    let msg = ChatMessage::assistant_from_response(None, None, "hi", std::iter::empty());
    assert_eq!(msg.content.len(), 1);
    assert!(matches!(&msg.content[0], ContentBlock::Text { .. }));
}
