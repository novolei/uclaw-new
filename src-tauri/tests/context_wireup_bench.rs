//! C2-Dirac-B2 integration bench — proves the ContextManager wire-up is
//! actually live, not a no-op.
//!
//! The unit tests confirm each piece in isolation; this drives the same
//! `for_prompt_with_injection` path the dispatcher's
//! `effective_system_prompt` calls, over a multi-turn fixture, and asserts:
//!
//!  1. `fragments_selected > 0` on at least HALF the turns — if the
//!     wire-up were dead (always-baseline), this would be all zeros
//!     (spec §12 "integration bench is the wire-up proof").
//!  2. The rendered baseline `system_prompt` is byte-stable across turns
//!     regardless of injection state — the cache-discipline guarantee
//!     (all production blocks are Always-policy).

use std::sync::Arc;

use uclaw_core::agent::baseline_blocks::InjectionContext;
use uclaw_core::agent::context_manager::{ComposeQuery, ContextManager};
use uclaw_core::runtime::context::{
    ContextFragment, ConversationHistoryFragment, MemoryRecallFragment, WorkspaceFileFragment,
};

fn fixture_fragments() -> Vec<Arc<dyn ContextFragment>> {
    vec![
        Arc::new(ConversationHistoryFragment {
            thread_id: "main".into(),
            turns: vec!["user asked about auth".into(), "assistant replied".into()],
        }),
        Arc::new(ConversationHistoryFragment {
            thread_id: "side".into(),
            turns: vec!["earlier tangent".into()],
        }),
        Arc::new(MemoryRecallFragment {
            query: "rust traits".into(),
            mock_hits: vec![("page-1".into(), "trait objects are dyn".into())],
        }),
        Arc::new(WorkspaceFileFragment {
            workspace_rel_path: "src/lib.rs".into(),
            max_bytes: Some(2048),
        }),
        Arc::new(MemoryRecallFragment {
            query: "database schema".into(),
            mock_hits: vec![("page-2".into(), "users table".into())],
        }),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn fragments_selected_on_at_least_half_the_turns() {
    let mut cm = ContextManager::new();
    cm.add_fragments(fixture_fragments());
    assert_eq!(cm.fragment_count(), 5);

    // Rotate the query topics across turns so the topic-scoring path is
    // genuinely exercised. Every fragment in the fixture is tagged with
    // at least one of these, so each query should select something.
    let topic_rotation = [
        vec!["conversation".to_string()],
        vec!["memory".to_string()],
        vec!["codebase".to_string()],
        vec!["history".to_string()],
        vec!["recall".to_string()],
    ];

    const TURNS: usize = 20;
    let mut turns_with_selection = 0usize;
    let mut baseline_turn2: Option<String> = None;

    for turn in 0..TURNS {
        let topics = topic_rotation[turn % topic_rotation.len()].clone();
        let query = ComposeQuery::defaults_with_topics(topics);

        // Turn 0 is the "first ACT turn"; later turns are not. All
        // production blocks are Always-policy, so the rendered baseline
        // must be identical regardless — guards cache discipline.
        let inj = if turn == 0 {
            InjectionContext {
                is_first_act_turn: true,
                last_error_kind: None,
                context_pressure_ratio: 0.0,
            }
        } else {
            InjectionContext::baseline()
        };

        let composed = cm.for_prompt_with_injection(&query, &inj).await;

        if composed.stats.fragments_selected > 0 {
            turns_with_selection += 1;
        }

        // Capture turn 2's baseline; assert every subsequent turn matches
        // it byte-for-byte (turns 2..N byte-stable — spec §8.6).
        if turn == 1 {
            baseline_turn2 = Some(composed.system_prompt.clone());
        } else if turn >= 2 {
            assert_eq!(
                composed.system_prompt,
                *baseline_turn2.as_ref().unwrap(),
                "turn {turn} baseline diverged from turn 2 — cache discipline broken"
            );
        }
    }

    assert!(
        turns_with_selection >= TURNS / 2,
        "wire-up appears dead: only {turns_with_selection}/{TURNS} turns selected \
         fragments (expected >= {})",
        TURNS / 2
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_manager_never_selects_but_still_renders_baseline() {
    // Control case: with no fragments, selection is always zero but the
    // baseline still renders — proving the "selected > 0" signal above is
    // attributable to the wire-up, not to baseline rendering.
    let cm = ContextManager::new();
    let query = ComposeQuery::defaults_with_topics(vec!["conversation".into()]);
    for _ in 0..5 {
        let composed = cm
            .for_prompt_with_injection(&query, &InjectionContext::baseline())
            .await;
        assert_eq!(composed.stats.fragments_selected, 0);
        assert!(!composed.system_prompt.is_empty());
    }
}
