use super::*;
use serde_json::json;

#[test]
fn tool_call_wire_shape_is_stable() {
    let call = ToolCall {
        id: "call-1".into(),
        name: "shell".into(),
        arguments: json!({"cmd": "pwd"}),
    };
    let value = serde_json::to_value(call).unwrap();
    assert_eq!(value["id"], "call-1");
    assert_eq!(value["name"], "shell");
    assert_eq!(value["arguments"]["cmd"], "pwd");
}

#[test]
fn tool_definition_wire_shape_is_stable() {
    let definition = ToolDefinition {
        name: "read_file".into(),
        description: "Read a file".into(),
        parameters: json!({"type": "object"}),
    };
    let value = serde_json::to_value(definition).unwrap();
    assert_eq!(value["name"], "read_file");
    assert_eq!(value["description"], "Read a file");
    assert_eq!(value["parameters"]["type"], "object");
}
