//! 迭代式压缩状态 + 切点检测 — Pi convergence Sprint 2 item 1。
//!
//! `CompactionState` 跨轮次累积上一份 fold(增量基底)。
//! `find_compaction_cut_point` 用结构化(ToolUse/ToolResult id 配对)检测
//! 切点是否落在工具对中间(split turn),供 `soft_compress_context` 做部分摘要恢复。

use std::collections::HashSet;

use crate::agent::compact::fold::StructuredFold;
use crate::agent::types::{ChatMessage, ContentBlock};

/// 跨轮次累积的增量压缩状态(运行期内存,放 ReasoningContext)。
#[derive(Debug, Clone, Default)]
pub struct CompactionState {
    /// 上一份 fold(增量基底);None = 首次压缩(走全史路径)。
    pub previous_fold: Option<StructuredFold>,
    /// 已完成的压缩周期数(统计 / 调试)。
    pub compactions_done: u32,
}

/// 切点 + Split-Turn 信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionCutPoint {
    /// 后缀逐字保留起点(message index)。
    pub first_kept_index: usize,
    /// 切点是否落在 ToolUse/ToolResult 对中间。
    pub is_split_turn: bool,
    /// split 时:被切那一轮的起点 index。
    pub turn_start_index: Option<usize>,
}

fn tool_use_ids(msg: &ChatMessage) -> impl Iterator<Item = &str> {
    msg.content.iter().filter_map(|b| match b {
        ContentBlock::ToolUse { id, .. } => Some(id.as_str()),
        _ => None,
    })
}

fn tool_result_ids(msg: &ChatMessage) -> impl Iterator<Item = &str> {
    msg.content.iter().filter_map(|b| match b {
        ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
        _ => None,
    })
}

/// 计算切点并检测 split-turn。`desired_index` 是期望的 first_kept_index
/// (后缀保留起点),由调用方根据 token 预算算出。
///
/// split-turn 判定:保留侧(>= desired_index)存在某个 ToolResult,其配对
/// ToolUse 在压缩侧(< desired_index)。此时把 turn_start_index 定位到该
/// ToolUse 所在消息。
pub fn find_compaction_cut_point(messages: &[ChatMessage], desired_index: usize) -> CompactionCutPoint {
    let idx = desired_index.min(messages.len());

    let compacted_uses: HashSet<&str> =
        messages[..idx].iter().flat_map(tool_use_ids).collect();
    let split_id = messages[idx..]
        .iter()
        .flat_map(tool_result_ids)
        .find(|rid| compacted_uses.contains(rid));

    match split_id {
        None => CompactionCutPoint { first_kept_index: idx, is_split_turn: false, turn_start_index: None },
        Some(rid) => {
            let turn_start = messages[..idx]
                .iter()
                .rposition(|m| tool_use_ids(m).any(|id| id == rid));
            CompactionCutPoint { first_kept_index: idx, is_split_turn: true, turn_start_index: turn_start }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{ChatMessage, ContentBlock, MessageRole};

    fn user(text: &str) -> ChatMessage {
        ChatMessage { role: MessageRole::User, content: vec![ContentBlock::Text { text: text.into() }], compacted: false }
    }
    fn assistant_tool_use(id: &str) -> ChatMessage {
        ChatMessage { role: MessageRole::Assistant, content: vec![ContentBlock::ToolUse { id: id.into(), name: "bash".into(), input: serde_json::json!({}) }], compacted: false }
    }
    fn user_tool_result(id: &str) -> ChatMessage {
        ChatMessage { role: MessageRole::User, content: vec![ContentBlock::ToolResult { tool_use_id: id.into(), content: "ok".into(), is_error: None }], compacted: false }
    }

    #[test]
    fn cut_point_not_split_when_boundary_on_user() {
        let msgs = vec![user("a"), assistant_tool_use("t1"), user_tool_result("t1"), user("b")];
        let cp = find_compaction_cut_point(&msgs, 3);
        assert_eq!(cp.first_kept_index, 3);
        assert!(!cp.is_split_turn);
        assert_eq!(cp.turn_start_index, None);
    }

    #[test]
    fn cut_point_detects_split_pair() {
        let msgs = vec![user("a"), assistant_tool_use("t1"), user_tool_result("t1"), user("b")];
        let cp = find_compaction_cut_point(&msgs, 2);
        assert!(cp.is_split_turn, "keeping a ToolResult whose ToolUse is compacted must be a split turn");
        assert_eq!(cp.turn_start_index, Some(1), "turn_start should point at the ToolUse message");
    }

    #[test]
    fn default_state_is_first_compaction() {
        let s = CompactionState::default();
        assert!(s.previous_fold.is_none());
        assert_eq!(s.compactions_done, 0);
    }

    // ── Tier 1.2 — previous_fold persistence round-trip tests ─────────────────

    /// Round-trip: a `StructuredFold` survives serde_json serialization +
    /// deserialization with all fields equal.  This validates the persistence
    /// shape used by `upsert_baseline` / `load_baseline`.
    #[test]
    fn structured_fold_serde_roundtrip() {
        use crate::agent::compact::fold::{
            ArtifactRef, DecisionWithRationale, FactWithEvidence, StructuredFold,
        };
        let fold = StructuredFold::default()
            .with_facts(vec![FactWithEvidence {
                statement: "Tier 1.2 persist test".into(),
                evidence: vec![ArtifactRef::new("file:src/agent/compaction.rs@HEAD")],
                confidence: Some(0.8),
            }])
            .with_decisions(vec![DecisionWithRationale {
                decision: "Store fold in agent_fold_baselines".into(),
                rationale: "No FK — accepts any string ID including conversation_id".into(),
                alternatives_considered: vec!["Add a new column to conversations".into()],
                evidence: vec![],
            }])
            .with_next_actions(vec!["verify after reload".into()]);

        let json = serde_json::to_string(&fold).expect("StructuredFold must serialize");
        let restored: StructuredFold =
            serde_json::from_str(&json).expect("StructuredFold must deserialize");
        assert_eq!(fold, restored, "serde round-trip must preserve all fields");
    }

    /// Session-load with NULL baseline (no row in agent_fold_baselines) →
    /// `compaction_state.previous_fold == None`.
    #[test]
    fn session_load_null_baseline_leaves_previous_fold_none() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V52_AGENT_FOLD_BASELINES)
            .unwrap();

        // Simulate the restore logic from send_message:
        let mut state = CompactionState::default();
        if let Some(prior) = crate::agent::compact::load_baseline(&conn, "conv-no-fold") {
            state.previous_fold = Some(prior);
        }

        assert!(
            state.previous_fold.is_none(),
            "No row in agent_fold_baselines must leave previous_fold as None"
        );
    }

    /// Session-load with a persisted fold → `compaction_state.previous_fold == Some(fold)`.
    #[test]
    fn session_load_valid_baseline_restores_previous_fold() {
        use crate::agent::compact::fold::{FactWithEvidence, StructuredFold};

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V52_AGENT_FOLD_BASELINES)
            .unwrap();

        let fold = StructuredFold::default().with_facts(vec![FactWithEvidence {
            statement: "fact persisted across reload".into(),
            evidence: vec![],
            confidence: None,
        }]);
        crate::agent::compact::upsert_baseline(&conn, "conv-with-fold", &fold).unwrap();

        // Simulate the restore logic from send_message:
        let mut state = CompactionState::default();
        if let Some(prior) = crate::agent::compact::load_baseline(&conn, "conv-with-fold") {
            state.previous_fold = Some(prior);
        }

        assert_eq!(
            state.previous_fold,
            Some(fold),
            "Restored previous_fold must equal the originally persisted fold"
        );
    }
}
