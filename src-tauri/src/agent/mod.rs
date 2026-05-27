pub mod agentic_loop;
pub mod anchor_state;
pub mod skeleton;
pub mod rule_context_builder;
// M1-T2c â RegularTask: SessionTask wrap of run_agentic_loop.
pub mod regular_task;
// M2-A pilot â BaselineBlock trait + 3-block registry.
pub mod baseline_blocks;
// M2-G pilot â StructuredFold 8-field compact representation.
pub mod compact;
// Pi Sprint 2 item 1 — CompactionState + structural split-turn cut-point detection.
pub mod compaction;
// Pi Sprint 2 item 2 — TurnSnapshot immutable per-turn config + NextTurnPatch + apply_patch.
pub mod turn;
// Pi Sprint 1 — SessionFileOps persistent file memory (StructuredFold axis 10).
pub mod file_ops;
// M2-H L1 pilot — TruncationPolicy + per-handler budgets.
// M2-H L1 pilot â TruncationPolicy + per-handler budgets.
pub mod truncation;
// M2-H L2 pilot â ToolExposure + normalize_tool_schema.
pub mod tool_shaping;
// M2-H L6 pilot â orphan tool-call audit + "aborted" synthesis.
pub mod call_audit;
// M2-H L5 pilot â image stripping for image-blind providers.
pub mod image_policy;
pub mod interrupts;
// M2-H L3 pilot â per-turn skill selection (top-K under token budget).
pub mod skill_selection;
// M2-H L7 pilot â Compaction state machine.
pub mod compression_state;
// M2-B pilot â ContextManager per-turn composition skeleton.
pub mod context_manager;
// M2-I pilot â Prompt caching policy (4 cache breakpoint placement).
pub mod cache_policy;
// M2-J pilot â TokenBudgetSnapshot UI backend contract.
pub mod token_budget;
// Slice 1 â Runtime telemetry collector (bridges agent loop to M2-J).
pub mod telemetry;
// M5 pilot â HookBus 13-event type skeleton.
pub mod hook_bus;
// M2-D pilot â Diff-based context re-injection.
pub mod context_diff;
// M1-T4b â opt-in rollout bridge for direct run_agentic_loop callsites.
pub mod rollout_integration;
pub mod code_rescue;
pub mod context;
pub mod dispatcher;
pub mod gbrain_prompt;
pub mod gep;
pub mod headless;
// Bundle 27-A â Heartbeat / stall detection / flight recorder.
pub mod heartbeat;
// Bundle 27-A â Reply recovery after unclean shutdown (uses flight record + 27-C's ProcessLock).
pub mod recovery;
pub mod history_window;
pub mod llm_stream;
pub mod mode_prompts;
pub mod mode_suggest;
pub mod mode_suggest_store;
pub mod persona;
pub mod plan_state;
pub mod retry;
pub mod session;
pub mod teams;
pub mod tools;
pub mod types;

/// C1.5 50-turn refactor benchmark support: a standalone deterministic replay
/// harness (reconstructs the request via ToolRegistry + compose_system_prompt +
/// the dispatcher tokenizer — no MockLlm/dispatcher, since ChatDelegate is
/// Wry-coupled) + a live runner + report types. Gated on the `bench` feature so
/// it is NOT compiled into the shipping app / release builds. See
/// `docs/superpowers/specs/2026-05-25-c1.5-50turn-bench-design.md`.
#[cfg(feature = "bench")]
pub mod bench;
