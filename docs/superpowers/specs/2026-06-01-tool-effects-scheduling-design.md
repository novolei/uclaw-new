# Effect-Typed Tool Scheduling Design

**Date:** 2026-06-01
**Status:** Child spec, implementation pending
**Parent spec:** `docs/superpowers/specs/2026-05-31-pi-modernization-six-modules-design.md`
**Pi reference:** `/Users/ryanliu/Documents/pi_agent_rust/src/tools.rs`

## Problem

uClaw tool scheduling currently exposes `ToolConcurrency::{Sequential, Parallel}`.
That Interface is shallow: a caller only learns whether a tool can enter the
parallel lane, while the Implementation still needs separate knowledge for
approval, preview targets, path gates, mutation tracking, process tools, network
tools, and fallback behavior for unknown tools.

Pi Rust has a deeper Module shape: each tool declares `ToolEffects`, and the
scheduler derives compatibility from those effects. This gives one seam for
read/write/process/network scheduling facts while keeping tool-specific
Implementation details local to each adapter.

## Goal

Add a `ToolEffects` Interface to uClaw and make `ToolDispatcher` plan batches
from effects instead of asking every callsite to reason in terms of a raw
parallel/sequential bit.

## Current Code Truth

- `src-tauri/src/agent/tools/tool.rs` defines `ToolConcurrency` and
  `Tool::concurrency()`, defaulting undeclared tools to `Sequential`.
- `ReadFileTool` and `GetFileSkeletonTool` override `concurrency()` to
  `Parallel`; most tools inherit sequential behavior.
- `src-tauri/src/agent/tool_dispatch/mod.rs::dispatch_inner` resolves
  `tool.concurrency()` per call, spawns parallel calls in a `JoinSet`, and
  drains that set before every sequential call.
- Unknown tools default to sequential so they produce a normal `NotFound`
  outcome.

## Pi Reference Truth

Pi Rust `ToolEffects` declares `read`, `write`, `append`, `network`, and
`process` effects. It treats write, append, and process as scheduling barriers;
pure read and network declarations can share a compatible batch. Unknown or
undeclared tools fail closed by using write-like effects.

## Interface

`ToolEffects` becomes the deeper scheduling Interface:

- `Tool::effects()` declares coarse behavior.
- `Tool::concurrency()` remains during migration and is derived from
  `effects().parallel_safe()` by default.
- The dispatcher owns batch planning through a small `ToolBatchPlan` /
  `ToolBatch` Implementation detail.
- Batch-plan evidence is machine-readable in tests and trace logs.

## Acceptance Evidence

- Unit tests prove read/read calls share a concurrent batch.
- Unit tests prove write/process calls create barriers.
- Unit tests prove unknown tools fail closed and are not batched with reads.
- Existing dispatcher order and cancellation tests still pass.
- Existing tool trait tests prove read-only builtins remain parallel and bash
  remains sequential through derived effects.

## Non-Goals

- Do not remove `ToolConcurrency` in this slice.
- Do not rewrite approval, path gates, preview behavior, or mutation tracking.
- Do not reclassify every builtin tool in one pass; start with read tools,
  bash/process, and the fail-closed default.
- Do not change provider-visible tool definitions.

