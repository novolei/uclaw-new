pub mod memory;
pub mod notify_user;
pub mod report_to_user;
pub mod request_escalation;

/// Returns the four built-in tool schemas for automation runs.
/// These are intentionally NOT exposed to interactive chat (ChatDelegate).
/// Task 15 (AutomationDelegate) will call this function to populate its
/// ToolRegistry.
pub fn humane_tool_schemas() -> Vec<serde_json::Value> {
    vec![
        report_to_user::schema(),
        notify_user::schema(),
        request_escalation::schema(),
        memory::schema(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humane_tool_schemas_are_valid_json() {
        let schemas = humane_tool_schemas();
        assert_eq!(schemas.len(), 4);
        let names: Vec<&str> = schemas
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"report_to_user"));
        assert!(names.contains(&"notify_user"));
        assert!(names.contains(&"request_escalation"));
        assert!(names.contains(&"memory"));
        for s in &schemas {
            assert!(s["input_schema"].is_object());
            assert!(s["description"].is_string());
        }
    }
}
