//! Symphony — Issue-Centric Pipeline Reconstruction (ZeptoBeam & SymphonyMac Fusion).
//!
//! Replaces legacy node graphs with a highly optimized, high-concurrency,
//! fault-tolerant 4-panel vertical issue execution pipeline.
//!
//! Submodules:
//! - `protocol` — Core types, status structures, and normalizers.
//! - `gateway` — SymphonyGithubGateway for GitHub CLI integrations.
//! - `workspace` — Git worktree sandboxing environment.
//! - `report` — Structured pipeline and stage report compiler.
//! - `agent` — LLM-driven cognitive pipeline orchestration.
//! - `orchestrator` — Task supervisor implementing ZeptoBeam's actor spine.
//! - `persistence` — Local disk serialization and resume mechanisms.

pub mod agent;
pub mod gateway;
pub mod orchestrator;
pub mod persistence;
pub mod paths;
pub mod logs;
pub mod protocol;
pub mod report;
pub mod workspace;
