# PR-3 Provider Readiness Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a provider readiness/core metadata layer inspired by jcode provider-core without changing uClaw's current provider runtime behavior.

**Architecture:** PR-3 creates a pure `uclaw-provider-core` crate for provider readiness, route, capability, cost, and failover DTOs, then adds a small uClaw adapter under `src-tauri/src/providers/readiness.rs`. Existing `ProviderService` methods that resolve `(provider_id, model, api_key, base_url)` remain unchanged because `get_active_llm_config` is HIGH risk and shared by chat, IM, memory, automation, and Symphony.

**Tech Stack:** Rust workspace crate, serde DTOs, existing `ProviderService`, existing provider registry, sibling `*_tests.rs` files.

---

## Scope

PR-3 implements the first provider-core slice only:

- typed provider readiness reports;
- credential/model/readiness status separation;
- route/capability/cost/failover metadata types;
- provider-service helper methods that derive reports from existing config.

PR-3 deliberately does not implement:

- `LlmProvider` trait changes;
- split prompt execution;
- OpenAI/Anthropic HTTP payload changes;
- runtime failover;
- Tauri IPC/UI changes;
- browser provider readiness;
- migrations or credential storage changes.

## ADR Section 18 Answers

| Question | PR-3 Answer |
|---|---|
| 1. User intent | Give uClaw a typed, inspectable provider readiness surface before routing more work through providers. |
| 2. Autonomy level | L1/L2 only; this PR reports readiness and does not autonomously switch providers. |
| 3. Canonical truth source | Existing `providers.json` remains provider config truth. New readiness reports are derived snapshots, not durable truth. |
| 4. TaskEvent entries | None emitted in PR-3. Later PRs can map provider readiness to `TaskEvent::ModelTurn` or capability events. |
| 5. Context reads | Reads built-in provider registry and saved provider configs through `ProviderService`; no external HTTP probe in the new helper. |
| 6. Capability cards | Formalizes provider capability metadata: API family, model listing, streaming assumption, prompt-cache hints, image/reasoning flags. |
| 7. Policy hooks | Provider credential policy and future capability selection can block runtime use. This PR does not bypass existing checks. |
| 8. World projection | Deferred. Reports are shaped so PR-12/PR-13 can render provider readiness later. |
| 9. Harness cases | Model-free unit tests for readiness derivation, serde round trips, and ProviderService helpers. |
| 10. Rollback | Revert the new crate, Cargo wiring, provider readiness adapter, tests, and status docs. No data migration rollback. |
| 11. Not owned | Does not own LLM client payloads, network probes, runtime failover, provider OAuth, browser providers, or frontend settings UI. |

## Evidence

- `docs/jcode_comparison/README.md` lists PR-3 as provider-core with split prompt, model capability, route, cost, and failover metadata.
- `docs/jcode_comparison/04_backend_reconstruction_blueprint.md` marks provider trait migration as high risk.
- jcode provider-core exposes provider/model/route/cost/failover concepts in `/Users/ryanliu/Documents/jcode/crates/jcode-provider-core/src/`.
- uClaw `ProviderService` owns config/control-plane behavior in `src-tauri/src/providers/service.rs`.
- uClaw `LlmProvider` is still a small hot runtime trait in `src-tauri/src/llm/provider.rs`; do not change it in PR-3.
- GitNexus impact reported `get_active_llm_config` as HIGH risk, so PR-3 avoids changing its signature or behavior.

## Allowed Files

- Create: `crates/uclaw-provider-core/Cargo.toml`
- Create: `crates/uclaw-provider-core/src/lib.rs`
- Create: `crates/uclaw-provider-core/src/provider_tests.rs`
- Create: `src-tauri/src/providers/readiness.rs`
- Create: `src-tauri/src/providers/readiness_tests.rs`
- Modify: `Cargo.toml`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/providers/mod.rs`
- Modify: `src-tauri/src/providers/service.rs`
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`

Avoid `src-tauri/src/llm/provider.rs`, `src-tauri/src/llm/providers/openai.rs`, `src-tauri/src/llm/providers/anthropic.rs`, `src-tauri/src/tauri_commands.rs`, migrations, frontend files, and `memory_graph/`.

## Task 1: Pure Provider Core Crate

**Files:**
- Create: `crates/uclaw-provider-core/Cargo.toml`
- Create: `crates/uclaw-provider-core/src/lib.rs`
- Create: `crates/uclaw-provider-core/src/provider_tests.rs`
- Modify: `Cargo.toml`

- [x] Add workspace member `crates/uclaw-provider-core`.
- [x] Define serde DTOs:
  - `ProviderApiFamily`
  - `ProviderCredentialStatus`
  - `ProviderReadinessState`
  - `ProviderProbeStatus`
  - `ProviderStreamingStatus`
  - `ProviderCapabilityFlags`
  - `ProviderRuntimeHints`
  - `ProviderReadinessIssue`
  - `ProviderReadinessReport`
  - `ProviderRoute`
  - `ProviderCostProfile`
  - `ProviderFallbackDecision`
- [x] Implement `ProviderReadinessReport::new`, `with_issue`, `is_usable`, and `redacted`.
- [x] Put all tests in sibling `provider_tests.rs`.
- [x] Test serde round trip and readiness state precedence.

Verification:

```bash
cargo test -p uclaw-provider-core
```

Expected: all provider-core tests pass.

## Task 2: uClaw Provider Readiness Adapter

**Files:**
- Create: `src-tauri/src/providers/readiness.rs`
- Create: `src-tauri/src/providers/readiness_tests.rs`
- Modify: `src-tauri/src/providers/mod.rs`
- Modify: `src-tauri/Cargo.toml`

- [x] Add `uclaw-provider-core` dependency to `src-tauri/Cargo.toml`.
- [x] Add `pub mod readiness;` to provider module.
- [x] Implement pure mapping helpers:
  - `api_family_from_api_type`
  - `credential_status_for`
  - `runtime_hints_for`
  - `assess_provider_readiness`
- [x] Keep readiness derivation model-free and HTTP-free.
- [x] Put all tests in sibling `readiness_tests.rs`.
- [x] Test missing API key, local provider no-key behavior, missing model, unknown provider, and streaming hints.

Verification:

```bash
cd src-tauri && cargo test providers::readiness --lib
```

Expected: readiness adapter tests pass.

## Task 3: ProviderService Helper Methods

**Files:**
- Modify: `src-tauri/src/providers/service.rs`

- [x] Add `provider_readiness(provider_id)` returning a derived `ProviderReadinessReport`.
- [x] Add `all_provider_readiness()` returning reports for all built-in providers.
- [x] Do not change `get_active_llm_config`, `get_chat_llm_config`, or `get_provider_llm_config`.
- [x] Reuse existing config lock and registry lookups only.

Verification:

```bash
cd src-tauri && cargo test providers::service --lib
```

Expected: existing provider service tests still pass.

## Task 4: Status Ledger

**Files:**
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`

- [x] Mark PR-1 and PR-2 as merged.
- [x] Mark PR-3 as in progress.
- [x] Add PR-3 impact notes, especially HIGH risk for `get_active_llm_config`.
- [x] Record that split prompt and runtime failover are deferred.

Verification:

```bash
git diff --check docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-23-pr3-provider-readiness-core.md
```

Expected: no whitespace errors.

## Full Verification

```bash
cargo test -p uclaw-provider-core
cd src-tauri && cargo test providers::readiness --lib
cd src-tauri && cargo test providers::service --lib
cd src-tauri && cargo test providers::types --lib
cd src-tauri && cargo test providers::store --lib
cargo check -p uclaw --lib
git diff --check
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr3-provider-core
```

Expected:

- provider-core tests pass;
- provider readiness/service/types/store tests pass;
- `cargo check -p uclaw --lib` passes with existing warnings only;
- GitNexus staged detect reports low/medium risk and only provider metadata/readiness/doc scope.

## Rollback

Revert the PR commit. Because PR-3 adds derived metadata and no migrations, no persistent config repair is required. Existing runtime provider resolution falls back to pre-PR behavior because no LLM trait or active config method is changed.
