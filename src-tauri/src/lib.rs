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

// B2: Infrastructure modules
pub mod memory;
pub mod memory_graph;
pub mod skills;
pub mod skills_manifest;
pub mod mcp;
pub mod channels;
pub mod providers;
pub mod workspace;
pub mod safety;
pub mod memu;
pub mod proactive;

// Re-export key types
pub use error::Error;
pub use ipc::*;
pub mod services;
pub mod memubot_config;
pub mod memorization;
pub mod local_api;
pub mod observability;

// Phase 3: AI Browser
pub mod browser;

// Phase 3: Automation
pub mod automation;

// Evaluation harness
pub mod harness;
