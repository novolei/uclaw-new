use super::*;

#[test]
fn protocol_envelope_wire_shape_is_camel_case() {
    let envelope = ProtocolEnvelope::new(ProtocolDomain::Agent, "payload");
    let value = serde_json::to_value(envelope).unwrap();
    assert_eq!(value["version"], UCLAW_PROTOCOL_VERSION);
    assert_eq!(value["domain"], "agent");
    assert_eq!(value["payload"], "payload");
}

#[test]
fn protocol_crate_reexports_runtime_message_and_tool_types() {
    let _message = ChatMessage::user("hello");
    let _event = TaskEvent::Warning {
        ts: "2026-05-23T00:00:00Z".into(),
        source: TaskEventSource::AgentLoop,
        task_id: "task-1".into(),
        code: "example".into(),
        message: "hello".into(),
    };
    let _tool = ToolCall {
        id: "call-1".into(),
        name: "shell".into(),
        arguments: serde_json::json!({"cmd": "pwd"}),
    };
}
