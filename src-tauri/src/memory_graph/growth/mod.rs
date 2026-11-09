//! Memory growth (T4 of the uclaw-pi backport): every N agent turns, distill
//! the new conversation into a durable *reflection*; keep one LLM-written
//! *user model* synthesized from the learned facts; inject both per turn.
//!
//! Sits above the fact pipeline (`memory_graph::reflection`, openhuman-G) and
//! the facet profile — it reads their output and never writes facts.
//! Plan: docs/superpowers/plans/2026-11-09-memory-growth-t4.md.

pub mod store;
