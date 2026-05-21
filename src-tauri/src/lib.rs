pub mod app;
pub mod config;
pub mod db;
pub mod error;
pub mod ipc;
pub mod settings;
pub mod tauri_commands;
pub mod cost_store;

pub mod agent;
pub mod llm;
pub mod api;

// B0: Infrastructure
pub mod background;
pub mod notifications;
pub mod infra;
// Phase 0.5 M1-T1 — IntentSpec/TaskSpec/TaskEvent runtime contracts.
pub mod runtime;

// M3-T1 — Five-registry skeleton (skills/connectors/tools/models/themes).
pub mod registries;

// B2: Infrastructure modules
pub mod memory;
pub mod memory_graph;
pub mod skills;
pub mod skills_manifest;
pub mod mcp;
pub mod gbrain;
pub mod channels;
pub mod providers;
pub mod workspace;
pub mod safety;
pub mod stt;
pub mod memu;
pub mod proactive;
pub mod learning;

// Re-export key types
pub use error::Error;
pub use ipc::*;
pub mod services;
pub mod memubot_config;
pub mod memorization;
pub mod local_api;
pub mod observability;

// Phase 3: Preview Engine
pub mod preview;

// Phase 3: AI Browser
pub mod browser;

// Phase 3: Automation
pub mod automation;

// Phase 4: Symphony — DAG-of-agent-runs runtime (parallel to Chat/Agent/Automation).
pub mod symphony_graph;

// Phase 3: Files Rail
pub mod files_rail;

// W6: Git integration (workspace + branch picker backbone)
pub mod git;
pub mod tauri_commands_git;

// Evaluation harness
pub mod harness;

// Sub-project B: knowledge ingestion pipeline
pub mod ingestion;
