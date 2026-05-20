pub mod agentic_loop;
// M1-T2c — RegularTask: SessionTask wrap of run_agentic_loop.
pub mod regular_task;
// M2-A pilot — BaselineBlock trait + 3-block registry.
pub mod baseline_blocks;
// M2-G pilot — StructuredFold 8-field compact representation.
pub mod compact;
// M1-T4b — opt-in rollout bridge for direct run_agentic_loop callsites.
pub mod rollout_integration;
pub mod code_rescue;
pub mod context;
pub mod dispatcher;
pub mod gbrain_prompt;
pub mod gep;
pub mod headless;
pub mod history_window;
pub mod llm_stream;
pub mod mode_prompts;
pub mod mode_suggest;
pub mod mode_suggest_store;
pub mod plan_state;
pub mod retry;
pub mod session;
pub mod teams;
pub mod tools;
pub mod types;
