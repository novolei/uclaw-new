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
    pub tools: Arc<Vec<ToolDefinition>>,
    pub force_text: bool,
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

/// 把补丁应用到下一轮快照(turn_index +1)。inject_message/should_stop 由 loop 处理。
pub fn apply_patch(mut snap: TurnSnapshot, patch: NextTurnPatch) -> TurnSnapshot {
    if let Some(m) = patch.model {
        snap.model = m;
    }
    if let Some(t) = patch.tools {
        snap.tools = Arc::new(t);
    }
    snap.turn_index += 1;
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
            tools: Arc::new(vec![]),
            force_text: false,
        }
    }

    #[test]
    fn apply_patch_overrides_model_and_bumps_turn_index() {
        let s = snap();
        let patched = apply_patch(s.clone(), NextTurnPatch { model: Some("m2".into()), ..Default::default() });
        assert_eq!(patched.model, "m2");
        assert_eq!(patched.turn_index, 2);
        assert_eq!(*patched.system_prompt, "sys"); // unchanged
    }

    #[test]
    fn apply_patch_none_fields_keep_original() {
        let s = snap();
        let patched = apply_patch(s.clone(), NextTurnPatch::default());
        assert_eq!(patched.model, "m1");
        assert_eq!(patched.turn_index, 2);
    }

    #[test]
    fn arc_snapshot_isolation() {
        let outer = Arc::new(snap());
        let held = Arc::clone(&outer);
        let _replaced = Arc::new(apply_patch((*outer).clone(), NextTurnPatch { model: Some("m2".into()), ..Default::default() }));
        assert_eq!(held.model, "m1"); // in-flight holder unaffected
    }
}
