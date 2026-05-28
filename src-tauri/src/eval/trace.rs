use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum EvalEvent {
    RunStarted {
        ts: String,
        case_id: String,
    },
    ModelTurn {
        ts: String,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        token_usage: Option<Value>,
    },
    ToolCall {
        ts: String,
        tool_name: String,
        input_ref: String,
    },
    ToolResult {
        ts: String,
        tool_name: String,
        output_ref: String,
        ok: bool,
    },
    PermissionRequest {
        ts: String,
        request_id: String,
        reason: String,
    },
    BoundaryEvent {
        ts: String,
        boundary: Value,
    },
    MemoryWrite {
        ts: String,
        target: MemoryEvalTarget,
        artifact_ref: String,
    },
    MemoryRecall {
        ts: String,
        target: MemoryEvalTarget,
        artifact_ref: String,
    },
    Checkpoint {
        ts: String,
        checkpoint_ref: String,
    },
    RunFinished {
        ts: String,
        verdict: crate::eval::episode::HarnessVerdict,
    },
}

impl EvalEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            EvalEvent::RunStarted { .. } => "run_started",
            EvalEvent::ModelTurn { .. } => "model_turn",
            EvalEvent::ToolCall { .. } => "tool_call",
            EvalEvent::ToolResult { .. } => "tool_result",
            EvalEvent::PermissionRequest { .. } => "permission_request",
            EvalEvent::BoundaryEvent { .. } => "boundary_event",
            EvalEvent::MemoryWrite { .. } => "memory_write",
            EvalEvent::MemoryRecall { .. } => "memory_recall",
            EvalEvent::Checkpoint { .. } => "checkpoint",
            EvalEvent::RunFinished { .. } => "run_finished",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEvalTarget {
    MemorySystem,
    Gbrain,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_event_serializes_tagged_kind_and_camelcase_fields() {
        let event = EvalEvent::ToolResult {
            ts: "2026-05-19T00:00:00Z".into(),
            tool_name: "browser_task".into(),
            output_ref: "artifact-1".into(),
            ok: true,
        };

        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["kind"], "tool_result");
        assert_eq!(value["toolName"], "browser_task");
        assert_eq!(value["outputRef"], "artifact-1");
        assert_eq!(event.kind(), "tool_result");
    }

    #[test]
    fn memory_targets_cover_memory_system_and_gbrain() {
        let memory = serde_json::to_string(&MemoryEvalTarget::MemorySystem).unwrap();
        let gbrain = serde_json::to_string(&MemoryEvalTarget::Gbrain).unwrap();
        assert_eq!(memory, "\"memory_system\"");
        assert_eq!(gbrain, "\"gbrain\"");
    }
}
