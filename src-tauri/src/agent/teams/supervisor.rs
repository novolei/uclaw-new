use crate::agent::types::ToolDefinition;

/// System prompt for the Supervisor.
pub fn supervisor_system_prompt(original_task: &str) -> String {
    format!(
        "You are a Supervisor Agent coordinating a team to complete a complex task.\n\
         Original task: {}\n\n\
         You must:\n\
         1. Break the task into subtasks\n\
         2. Assign workers using assign_worker()\n\
         3. Monitor progress with read_channel()\n\
         4. When all workers are done, call request_review() with the combined results\n\
         5. If reviewer says revise, give workers feedback and re-assign\n\
         6. When review passes, call complete_task() with the final result\n\n\
         Never do the work yourself. Coordinate only.",
        original_task
    )
}

/// Tool definitions for the Supervisor.
pub fn supervisor_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "assign_worker".to_string(),
            description: "Spawn a worker agent with a specific role and task".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "worker_id": { "type": "string", "description": "Unique ID for this worker (e.g. 'researcher_1')" },
                    "role": { "type": "string", "description": "Worker role description" },
                    "task": { "type": "string", "description": "Specific task for this worker" }
                },
                "required": ["worker_id", "role", "task"]
            }),
        },
        ToolDefinition {
            name: "read_channel".to_string(),
            description: "Read all messages in the team channel to check worker progress"
                .to_string(),
            parameters: serde_json::json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "request_review".to_string(),
            description: "Submit combined worker results to the Reviewer for quality check"
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "combined_result": { "type": "string", "description": "Combined output from all workers" }
                },
                "required": ["combined_result"]
            }),
        },
        ToolDefinition {
            name: "complete_task".to_string(),
            description: "Finalize the team task and return the result to the user".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "result": { "type": "string", "description": "Final result to present to the user" }
                },
                "required": ["result"]
            }),
        },
    ]
}
