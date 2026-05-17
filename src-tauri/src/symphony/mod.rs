//! Symphony — DAG-of-agent-runs runtime.
//!
//! Symphony is uClaw's third top-level execution runtime, parallel to Chat
//! and Agent. It orchestrates a directed acyclic graph of `HeadlessDelegate`
//! invocations, with edges describing handoffs, a visual canvas as the
//! authoring surface, and the same cost / safety / memory machinery the
//! Chat and Automation runtimes already use.
//!
//! Design spec: `docs/superpowers/specs/2026-05-17-symphony-runtime-design.md`.
//! Implementation plan: `docs/superpowers/plans/symphony-runtime.md`.
//!
//! ## Module map
//!
//! - `protocol/` — types (`SymphonyWorkflowDef`, `SymphonyNode`, `SymphonyEdge`,
//!   `NodeStatus`, `RunStatus`, …), the WORKFLOW.md parser (YAML front matter
//!   + Markdown prompt body), and the def-↔-DB-row normalizer.
//! - `manager.rs` (T5) — CRUD over `symphony_workflows` and
//!   `symphony_workflow_versions`.
//! - `runtime/` (T6–T12) — the live execution layer: cost caps, retry, per-
//!   node executor, DAG scheduler, stall detection, recovery, the
//!   `SymphonyService: ManagedService` impl.
//! - `tools/` (Phase 2) — Symphony-specific tools such as `record_handoff`.
//! - `sources/` (Phase 2) — workflow trigger sources (manual today; Linear /
//!   GitHub Issues / cron later).

pub mod manager;
pub mod protocol;
