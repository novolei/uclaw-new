// wired in Task 15 (AutomationDelegate)
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct MemoryInput {
    pub op: String, // "read" | "write" | "append" | "compact"
    pub content: Option<String>,
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "memory",
        "description": "Read or write this automation's persistent memory file (~/.uclaw/automation/{spec_id}/memory.md). No permission gate — memory is private to this spec. Each op is logged in tool_calls_json.",
        "input_schema": {
            "type": "object",
            "required": ["op"],
            "properties": {
                "op":      { "enum": ["read", "write", "append", "compact"] },
                "content": { "type": "string" }
            }
        }
    })
}
