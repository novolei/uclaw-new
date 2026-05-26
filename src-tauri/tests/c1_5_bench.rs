//! C1.5 50-turn refactor benchmark — deterministic unit tests.
//! Bench-only: the whole file is gated on `feature = "bench"`, so it only
//! compiles/runs under `cargo test --features bench --test c1_5_bench`.
#![cfg(feature = "bench")]

use std::path::PathBuf;
use uclaw_core::agent::bench::{load_golden, replay};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/c1.5-bench/refactor-8-file")
}

/// #1 — replaying the post golden twice yields identical per-turn token
/// breakdowns (the bench is deterministic; it can't silently drift).
#[test]
fn golden_sequence_replays_deterministically() {
    let a = replay(&fixture(), "post");
    let b = replay(&fixture(), "post");
    assert_eq!(a.total_input_tokens, b.total_input_tokens);
    assert_eq!(
        a.per_turn.iter().map(|t| t.total).collect::<Vec<_>>(),
        b.per_turn.iter().map(|t| t.total).collect::<Vec<_>>(),
    );
}

/// #2 — the structural premise: the legacy sequence does far more round-trips
/// than the borrow sequence (16 read+edit pairs vs one batched read + one
/// batch edit).
#[test]
fn pre_golden_has_more_roundtrips_than_post() {
    assert!(replay(&fixture(), "pre").round_trips > replay(&fixture(), "post").round_trips);
}

/// #3 — the golden sequence loads in turn order and the post sequence uses the
/// A2 multi-file batch edit (`edit{files:[...]}`).
#[test]
fn mock_llm_returns_golden_calls_in_order() {
    let recs = load_golden(&fixture().join("post_dirac.jsonl"));
    assert!(recs.windows(2).all(|w| w[1].turn >= w[0].turn), "turns must be non-decreasing");
    assert!(
        recs.iter().any(|r| r.tool_name == "edit" && r.tool_args.get("files").is_some()),
        "post sequence must contain a multi-file batch edit (A2)",
    );
    assert!(
        recs.last().map(|r| r.tool_name == "__final__").unwrap_or(false),
        "post sequence must terminate with a __final__ record",
    );
}

/// #4 — the replay output carries a per-turn token breakdown with non-zero
/// system-prompt + tool-def components on every turn (mirrors the dispatcher's
/// `Calling LLM` log fields).
#[test]
fn replay_emits_per_turn_token_breakdown() {
    let r = replay(&fixture(), "post");
    assert!(!r.per_turn.is_empty(), "per_turn must not be empty");
    assert!(
        r.per_turn.iter().all(|t| t.system_prompt_tokens > 0 && t.tool_def_tokens > 0),
        "every turn must have non-zero system_prompt + tool_def tokens",
    );
    assert_eq!(
        r.total_input_tokens,
        r.per_turn.iter().map(|t| t.total).sum::<usize>(),
        "total_input_tokens must equal the sum of per-turn totals",
    );
}

/// #5 — the headline: the borrow sequence's total input tokens are strictly
/// lower than the legacy sequence's (the M2 DoD reduction premise).
#[test]
fn post_total_input_is_lower_than_pre() {
    let pre = replay(&fixture(), "pre");
    let post = replay(&fixture(), "post");
    assert!(
        post.total_input_tokens < pre.total_input_tokens,
        "post ({}) must be < pre ({})",
        post.total_input_tokens,
        pre.total_input_tokens,
    );
}
