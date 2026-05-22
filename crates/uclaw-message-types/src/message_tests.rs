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
