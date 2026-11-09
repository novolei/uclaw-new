//! 每轮不可变配置快照 + 轮边界补丁 — Pi convergence Sprint 2 item 2。
//!
//! `TurnSnapshot` 冻结一轮的 model + 组装好的 system_prompt + tools。以 Arc 共享:
//! in-flight 的 call_llm 持有自己的 Arc clone,轮边界替换不影响它。

use std::sync::Arc;

use crate::agent::types::{ChatMessage, ToolDefinition};

/// 一轮(一次 loop 迭代)的不可变配置快照。
#[derive(Clone, Debug)]
pub struct TurnSnapshot {
    pub turn_index: u32,
    pub model: String,
    pub system_prompt: Arc<String>,
    /// Dynamic context block prepended to the last user message in each LLM
    /// call. Assembled alongside `system_prompt` in `create_turn_snapshot`
    /// via a single `assemble_prompt` call (P3-6 Task 4 — eliminates the
    /// second `assemble_system_prompt` invocation that `build_dynamic_context`
    /// would otherwise cause).
    pub dynamic_context: String,
    pub tools: Arc<Vec<ToolDefinition>>,
    pub force_text: bool,
}

impl TurnSnapshot {
    /// Tokens this turn's request carries beyond `ReasoningContext::
    /// estimate_token_count()` (whose basis is `base_system_prompt` + live
    /// messages): serialized tool definitions JSON + the assembled system
    /// prompt's excess over `base_system_prompt` (saturating) + the dynamic
    /// block prepended to the last user message. Same `estimate_tokens`
    /// heuristic as the base estimate.
    pub fn request_overhead_tokens(&self, base_system_prompt: &str) -> usize {
        use crate::agent::types::estimate_tokens;
        let system_excess = (estimate_tokens(&self.system_prompt) as usize)
            .saturating_sub(estimate_tokens(base_system_prompt) as usize);
        let tools = if self.tools.is_empty() {
            0
        } else {
            let json = serde_json::to_string(&*self.tools).unwrap_or_default();
            estimate_tokens(&json) as usize
        };
        // call_llm prepends nothing when the dynamic block is empty, and
        // estimate_tokens("") still yields its fixed +4 overhead — skip it.
        let dynamic = if self.dynamic_context.is_empty() {
            0
        } else {
            estimate_tokens(&self.dynamic_context) as usize
        };
        system_excess + tools + dynamic
    }
}

/// 轮边界对下一轮的补丁(显式配置变更/注入/停止)。
/// item ② 暂无生产者(prepare_next_turn 默认 None);为 Sprint 4 hot-swap + item ③ 注入预留。
#[derive(Default, Debug)]
pub struct NextTurnPatch {
    pub model: Option<String>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub inject_message: Option<ChatMessage>,
    pub should_stop: bool,
}

/// 把补丁应用到当前轮快照(只覆盖 model/tools)。turn_index 由 loop 经
/// create_turn_snapshot 决定,apply_patch 不动它。inject_message/should_stop 由 loop 处理。
pub fn apply_patch(mut snap: TurnSnapshot, patch: NextTurnPatch) -> TurnSnapshot {
    if let Some(m) = patch.model {
        snap.model = m;
    }
    if let Some(t) = patch.tools {
        snap.tools = Arc::new(t);
    }
    snap
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn snap() -> TurnSnapshot {
        TurnSnapshot {
            turn_index: 1,
            model: "m1".into(),
            system_prompt: Arc::new("sys".into()),
            dynamic_context: String::new(),
            tools: Arc::new(vec![]),
            force_text: false,
        }
    }

    #[test]
    fn apply_patch_overrides_model_keeps_turn_index() {
        let s = snap();
        let patched = apply_patch(s.clone(), NextTurnPatch { model: Some("m2".into()), ..Default::default() });
        assert_eq!(patched.model, "m2");
        assert_eq!(patched.turn_index, 1); // unchanged — loop owns turn_index via create_turn_snapshot
        assert_eq!(*patched.system_prompt, "sys"); // unchanged
    }

    #[test]
    fn snapshot_sets_request_overhead() {
        use crate::agent::types::estimate_tokens;
        let base = "base system prompt";
        let assembled = format!("{base}\n\n## Project rules\nAlways run tests before committing.\n规则：先读后写。");
        let tools = vec![ToolDefinition {
            name: "read_file".into(),
            description: "Read a file from the workspace and return its contents.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string", "description": "file path"}},
                "required": ["path"]
            }),
        }];
        let dynamic = "Current time: 2026-11-09 10:00\nWorkspace root: /tmp/ws".to_string();
        let s = TurnSnapshot {
            turn_index: 1,
            model: "m".into(),
            system_prompt: Arc::new(assembled.clone()),
            dynamic_context: dynamic.clone(),
            tools: Arc::new(tools.clone()),
            force_text: false,
        };
        let tools_json = serde_json::to_string(&tools).unwrap();
        let expected = (estimate_tokens(&assembled) as usize)
            .saturating_sub(estimate_tokens(base) as usize)
            + estimate_tokens(&tools_json) as usize
            + estimate_tokens(&dynamic) as usize;
        let got = s.request_overhead_tokens(base);
        assert!(got > 0);
        assert_eq!(got, expected);
        // Tools alone must contribute — under-counting is the failure mode.
        assert!(got >= estimate_tokens(&tools_json) as usize);
    }

    #[test]
    fn request_overhead_saturates_when_assembled_shorter_than_base() {
        let s = TurnSnapshot {
            turn_index: 0,
            model: String::new(),
            system_prompt: Arc::new("x".into()),
            dynamic_context: String::new(),
            tools: Arc::new(vec![]),
            force_text: true,
        };
        // No tools are sent (force_text), no dynamic block is prepended, and
        // the base excess saturates to 0.
        let got = s.request_overhead_tokens("a much longer base system prompt than the snapshot");
        assert_eq!(got, 0);
    }

    #[test]
    fn apply_patch_none_fields_keep_original() {
        let s = snap();
        let patched = apply_patch(s.clone(), NextTurnPatch::default());
        assert_eq!(patched.model, "m1");
        assert_eq!(patched.turn_index, 1); // unchanged
    }

    #[test]
    fn arc_snapshot_isolation() {
        let outer = Arc::new(snap());
        let held = Arc::clone(&outer);
        let _replaced = Arc::new(apply_patch((*outer).clone(), NextTurnPatch { model: Some("m2".into()), ..Default::default() }));
        assert_eq!(held.model, "m1"); // in-flight holder unaffected
    }
}
