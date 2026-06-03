# Local MiniCPM + Desktop Companion — Program Design

**Date:** 2026-06-03
**Status:** Design (recon done; pending spec review → plans)
**Worktree:** `.claude/worktrees/local-minicpm-deskpet` (branch `worktree-local-minicpm-deskpet`)
**Engine decision:** candle `quantized_llama` (pure Rust, zero external runtime)
**Shape:** one program, six dependency-ordered slices (A–F). Each slice gets its own plan → PR.

---

## Goal

Bring a local **MiniCPM5-1B-GGUF** model into uClaw-pi to (1) back the **light-tool** and **memory-summary** scenarios so they run locally and save tokens, with **zero manual config** (first-run guided env-check → download → warmup), and (2) ship a **desktop chat companion** — a floating, always-on-top pet that talks to MiniCPM, with importable/switchable persona adapters folded into the existing pet roster.

This honors the **Pi-lightweight + zero-external-runtimes** baseline (`docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`): the local model is a pure-Rust in-process engine in the same family as the existing `ort`/`fastembed` embedder — no bundled `llama-server`, no Bun, no Python.

## Problem (recon, file:line)

**P1 — Per-scenario model assignment is mostly dead config.** The Settings → 智能 → 模型分配 UI lets users assign a model to five roles (`chat`/`utility`/`utility_large`/`summarizer`/`compiler`), persists them, but the runtime only consumes **two**:

- `get_chat_llm_config()` and `get_ingestion_llm_config()` exist and check `role_models` first, then fall back to `active_model` (`src-tauri/src/providers/service.rs:165-236`).
- **No** `get_utility_llm_config()` / `get_summarizer_llm_config()` / `get_utility_large_llm_config()` / `get_compiler_llm_config()` exist.
- `/compact` summarization uses the **global** `get_active_llm_config()`, ignoring the `summarizer` role (`src-tauri/src/tauri_commands.rs:9425-9443`).
- Roles are declared in `MODEL_ROLES` (`src-tauri/src/providers/types.rs:272-285`); persistence to `providers.json` works (`providers/service.rs:297-315`, `providers/types.rs:306-321`); UI + IPC are fully wired (`ui/src/components/settings/ModelSettings.tsx`, `ui/src/lib/tauri-bridge.ts:691-700`, `tauri_commands.rs:4650-4670`).

The two scenarios this program targets (轻工具/utility, 记忆摘要/summarizer) are **exactly the unwired ones**. Wiring them is the load-bearing prerequisite (Slice A).

**P2 — No local LLM generation exists.** Today the only in-process ML is **ONNX embeddings** (bge-small-en-v1.5 via `ort 2.0.0-rc.10`) at `src-tauri/src/memory_bucket_seal/score/embed/onnx.rs:1-330`, with a HF + hf-mirror downloader (`.../embed/model_download.rs:1-151`, cache `~/.uclaw/models/bge-small-en-v1.5/`) and a `:7337` local OpenAI-compat endpoint pattern (`.../embed/factory.rs`, `memubot_config.rs`). There is **no GGUF/llama.cpp/candle**, no text generation (Slice B/C).

**P3 — Desk pet is an in-viewport `<div>`, not a desktop window.** `ui/src/components/agent/PetWidget.tsx:1-73` renders an anchored widget in AgentView; `ui/src/atoms/pet-atoms.ts:1-32` holds the state machine; `ui/src/components/settings/PetSettings.tsx:14-17` has two hardcoded characters (astro/clawby). There is **no** transparent/always-on-top window and **no** "chat with the pet". Persona Studio (`ui/src/components/settings/PersonaStudio.tsx`) is a separate voice/tone feature for the main agent, unrelated to the pet roster (Slice E/F).

## MiniCPM5-1B facts (drives the engine choice)

Standard `LlamaForCausalLM` — "no custom kernels or model-code forks" (HF model card). 24 layers, GQA (16 Q / 2 KV heads), 131K context. Quants: **Q4_K_M 688 MB** (default), Q8_0 1.15 GB, F16 2.17 GB. Because it is plain Llama, candle's existing `quantized_llama` module loads its GGUF directly — no custom arch module needed. (Source: huggingface.co/openbmb/MiniCPM5-1B-GGUF.)

## Approaches considered (key forks)

- **Engine:** candle `quantized_llama` (chosen) vs `llama-cpp-2` (C++ build, breaks zero-external-runtime) vs `mistral.rs` (heavier; reserved as the future real-LoRA upgrade path). candle wins on pure-Rust + same-family-as-`ort` + verified-because-standard-Llama.
- **Engine ↔ agent integration:** **B2 local OpenAI-compat HTTP** (chosen) — add `:7337/v1/chat/completions` mirroring the existing `:7337/v1/embeddings`, register MiniCPM as a provider at `http://localhost:7337/v1`; the agent's existing HTTP client + role assignment work unchanged. vs **B1 in-process trait** — rejected: invasive provider-layer branch, breaks "provider = HTTP endpoint" uniformity.
- **Pet roles:** **prompt-level personas** (chosen for v1) vs real LoRA weight hot-swap — deferred (candle LoRA weak; MiniCPM LoRA-in-GGUF immature). A reserved `lora_adapter?` field marks the future path.

## Program-wide default decisions

1. **Local model = a provider.** Registered as `local / minicpm5-1b` at `http://localhost:7337/v1`, so the existing 模型分配 dropdown targets it with zero new config surface.
2. **Graceful degradation.** When the local model is not ready (downloading/failed/disabled), `utility`/`summarizer` fall back to the cloud active model. Never block.
3. **Skippable first-run.** First launch pops a **non-blocking** wizard; tri-state `completed`/`deferred`/`skipped`. "稍后" is non-permanent (re-runnable from Settings → MiniCPM); "不再提示" is permanent.
4. **Default quant Q4_K_M (688 MB).** Q8_0/F16 are advanced options.

## Architecture (dependency graph)

```
         A 角色路由地基 (utility/summarizer dead-config → runtime)
              │
         B candle 本地引擎 (quantized_llama; :7337/v1/chat/completions; provider)
          ┌───┴───────────────┐
   下载/引导线                  桌宠线
   C 智能下载 (HF/ModelScope)   E 悬浮桌面伙伴 (transparent always-on-top window)
   D 首启动向导 (env→dl→warmup) F 宠物角色适配器 (persona registry + import)
```

---

## Slice A — Role routing foundation

**Goal:** wire `utility` + `summarizer` (and opportunistically `utility_large`/`compiler`) from dead config to runtime; unify the existing chat/ingestion lookups.

- **Unifying primitive:** `get_role_llm_config(role: &str) -> Option<ResolvedLlmConfig>` in `providers/service.rs` — `role_models[role]` hit resolves, else falls back to `active_model`. Refactor `get_chat_llm_config`/`get_ingestion_llm_config` into thin wrappers over it.
- **Type cleanup (on-path improvement):** replace the 5-tuple return `(String,String,String,String,Option<ApiType>)` with a named `ResolvedLlmConfig` struct (readability + avoids field misalignment when the local provider is added). No unrelated refactoring.
- **Call-site audit (first task of A):** enumerate every LLM call in `agent/` + `memory_*`, classify into roles. Known consumers: `summarizer` → `/compact` `summarize_to_fold` (`tauri_commands.rs:9425`) + memory fold summariser; `utility` → lightweight calls (title gen / translation / tag extraction / quick classification — located by the audit).
- **Honest boundary:** `utility_large`/`compiler` are wired **only if** the audit finds a real, distinct consumer; otherwise they remain "defined, unconsumed" and the spec says so — no new dead config. A's hard deliverable is `utility` + `summarizer` live.
- **Degradation:** role miss → `active_model`; local provider present-but-not-ready → resolve fails → fall back. Never block.
- **Surface:** `providers/service.rs` + call sites only. No new deps, no schema change, no frontend change.
- **Tests:** `#[cfg(test)]` for `get_role_llm_config` three states (role hit / role empty → active / active empty → None); chat/ingestion wrappers no-regression.

### Slice A — migration backlog (post-A, mechanical)

Slice A wires the two canonical consumers (summarizer → `/compact` fold; utility →
conversation title generation) and ships the generic `get_role_llm_config` primitive.
The remaining `get_active_llm_config()` call sites are now a mechanical migration —
each is a one-line swap to the appropriate role getter:

| Call site (file:line) | What it does | Target role |
|---|---|---|
| `tauri_commands.rs:13602` agent-session title summary | title/emoji gen | utility |
| `tauri_commands.rs:8228` `call_consolidation_llm` | skill metadata consolidation | utility |
| `tauri_commands.rs:9669`, `:13877` | other auxiliary completions | utility / chat (per audit) |
| `memory_graph/auto_classify.rs:40` | classify memory node | utility |
| `proactive/daily_summary.rs:143` | daily rollup | summarizer |
| `memory_bucket_seal/.../summariser/llm.rs:39` | bucket-seal tree fold (currently ingestion) | summarizer |
| `memorization/service.rs:469` | entity-page semantic merge | utility_large |
| `proactive/service.rs:2830`, `proactive/scenarios/entity_synthesizer.rs:203`, `memory_graph/wiki_synth.rs:269` | semantic synthesis | utility_large |

**`utility_large` and `compiler` remain defined-but-unconsumed in Slice A** — they have
candidate consumers above but no canonical wiring yet, and `compiler` has no distinct
consumer at all. They are intentionally left unrouted (no new dead config); wiring them
is follow-up work, not part of Slice A's deliverable.

**Test-hardening follow-up (from Task 3 code review):** the async test
`utility_getter_prefers_role_then_active` in `providers/service.rs` uses a fixed temp-dir
path (`std::env::temp_dir().join("uclaw_slice_a_utility_test")`), mirroring an existing
codebase pattern. It is collision-safe today (single such test) but should be hardened to a
`tempfile::tempdir()` or pid-scoped path before a second similar test lands, to avoid
parallel-test flakiness.

## Slice B — candle local inference engine

**Goal:** in-process load of MiniCPM5-1B Q4_K_M, text generation, exposed as a provider via local HTTP.

- **Integration: B2.** Extend the existing `:7337` LocalApiService with `/v1/chat/completions` (streaming SSE + non-stream) driving the candle engine, mirroring how `/v1/embeddings` → OnnxEmbedder is wired. Register a built-in provider `local / minicpm5-1b` (base_url `http://localhost:7337/v1`).
- **Module `src-tauri/src/local_llm/`** (mirrors `onnx.rs` Mutex-behind lazy-load):
  - `mod.rs` — lifecycle `load`/`unload`/`warmup`/`is_ready`; model + tokenizer behind `tokio::Mutex`; **lazy load** (no 688 MB at startup).
  - `engine.rs` — `quantized_llama::ModelWeights::from_gguf()`; forward + KV cache; sampling (temperature/top-p/top-k/repeat-penalty); stop tokens; token streaming.
  - `chat_template.rs` — MiniCPM chat template (pure fn, unit-tested).
  - `server.rs` (extend LocalApiService) — the chat-completions route.
- **Device/concurrency/warmup:** Metal on macOS (candle `metal` feature) with CPU fallback (Slice D's env-check informs); generation serialized behind the Mutex; warmup = 1-token forward to JIT Metal kernels + page in weights.
- **Degradation contract (to A):** model missing/not-ready → structured "model not ready" → A falls back to cloud. Load failure (corrupt GGUF / OOM) → surfaced + fall back.
- **Deps/cost (honest):** add `candle-core`/`candle-transformers`/`candle-nn` (macOS `metal` feature); `tokenizers 0.20` already present. Cost: compile time + binary size up; **no C++** (candle Metal via metal-rs). **Dependency to C:** candle's quantized loader uses an **external `tokenizer.json`** — C must fetch it from base repo `openbmb/MiniCPM5-1B` if the GGUF repo lacks it.
- **Tests:** `chat_template` pure unit tests; model-load + smoke gen (`"2+2="` contains `"4"`) **gated on model presence** (skip if absent, like the embedder tests) so CI isn't blocked by 688 MB.

## Slice C — Model management + smart download

**Goal:** auto-fetch MiniCPM5-1B GGUF + tokenizer.json, pick HF/ModelScope by network, cache/verify/manage, emit progress for D.

- **Generalize (not duplicate):** minimally refactor `model_download.rs` (bge-specific) into a manifest-driven core shared by the embedder and MiniCPM:
  ```
  ModelManifest { cache_dir, files: [{repo_path, dest_name, expected_size, sha256?}], sources: [SourceTemplate] }
  async fn download_manifest(m, progress) -> Result<()>
  ```
  bge becomes one manifest; MiniCPM another. No bge regression.
- **Sources:** HF `huggingface.co/openbmb/MiniCPM5-1B-GGUF/resolve/main/<file>` (+ tokenizer.json from base `openbmb/MiniCPM5-1B`); HF mirror `hf-mirror.com`; ModelScope `modelscope.cn/models/OpenBMB/MiniCPM5-1B-GGUF/resolve/master/<file>` (exact revision/path verified inside C).
- **Smart selection:** parallel lightweight **ranged GET (first 1–64 KB)** probe of candidate sources → measure reachability + first-byte latency → **first good responder wins**; mid-download source failure falls through the probe-ranked list. Probe behind a trait for injectable-latency unit tests.
- **ModelManager (`src-tauri/src/local_llm/model_manager.rs`):** resolve cache path, check installed, compute missing files, drive downloader, verify, list/delete. **Cache path contract (to B):** `~/.uclaw/models/minicpm5-1b/` with `MiniCPM5-1B-Q4_K_M.gguf` + `tokenizer.json`.
- **Quants:** default `Q4_K_M`; `Q8_0`/`F16` advanced; manifest maps quant → filename.
- **Tauri commands (two-edit registration):** `local_model_probe_sources()`, `local_model_download(model_id, quant, source?)`, `local_model_list()`, `local_model_cancel(model_id)`, `local_model_delete(model_id)`.
- **Progress:** downloader takes a progress callback; manager emits Tauri event `minicpm://download-progress { file, bytes, total, source, phase }`.
- **Verify/atomicity:** size match vs `expected_size`, optional sha256; existing tmp→final atomic rename; verify fail → retry once → surface. Guard disk space pre-download.
- **Tests:** source-URL construction unit tests (HF/mirror/ModelScope × quant × file, pure); cache-path resolution; probe ordering with injected latencies; real 688 MB download integration test gated/skipped in CI.

## Slice D — First-run onboarding wizard

**Goal:** non-blocking first-launch guide (env-check → source → download → warmup) that, on completion, **auto-wires** `utility` + `summarizer` to the local model. This is the "zero manual config" payoff (D → A closure).

- **Wizard state machine:** `intro → envcheck → source → download → warmup → smoketest → done`; per-step failure → error state (retry/back/switch-source/cancel).
  - intro: explains local benefits (save tokens / privacy / offline); choices 现在设置 / 稍后 / 不再提示.
  - envcheck: `local_model_env_check()` → `{ os, arch, total_ram, free_disk, metal_available, cpu_cores, recommended_quant, warnings[] }`; per-item pass/warn; recommends quant by hardware.
  - source: Slice C `local_model_probe_sources()` + override.
  - download: Slice C `local_model_download`, subscribe `minicpm://download-progress`, bar with source/speed/ETA, cancellable.
  - warmup: Slice B engine load + 1-token warmup.
  - smoketest: `local_model_smoke_test("你好")` — show first local output as proof.
  - done: auto `setRoleModel('utility', 'local/minicpm5-1b')` + `setRoleModel('summarizer', …)`; note it's changeable in 模型分配.
- **Trigger (default #3):** onboarding state `completed`/`deferred`/`skipped`. On start, if not completed and not skipped → non-blocking wizard overlay. 稍后 = deferred (cloud meanwhile via A fallback; re-runnable from Settings → MiniCPM); 不再提示 = skipped.
- **Backend commands (two-edit):** `local_model_env_check()` (`sysinfo` crate if absent; Metal via candle device probe), `local_model_warmup()`, `local_model_smoke_test(prompt)`, `get_onboarding_state()`/`set_onboarding_state(state)`. (`probe_sources`/`download`/`cancel` reuse C.)
- **Frontend:** new Settings tab **MiniCPM** (add to `SettingsNav.tsx`) hosting the re-runnable wizard + ongoing management; wizard component `ui/src/components/onboarding/MiniCPMWizard.tsx`; app-start hook renders the non-blocking overlay.
- **Errors:** envcheck shortfall (disk/RAM) → warn + smaller-quant / proceed-at-risk / cancel; download fail → C's switch-source retry; warmup fail → surface + cloud fallback + retry.
- **Tests:** `env_check` (mock sysinfo); wizard Vitest/jsdom (step transitions, progress render, **completion calls setRoleModel** via mocked bridge); onboarding-state persistence.

## Slice E — Floating desktop companion (form factor A)

**Goal:** transparent, always-on-top pet window; idle shows only the sprite; click expands a compact chat panel; proactive bubble at key moments. Pet chat is **local-only** (its save-tokens/offline identity).

- **Form factor A (chosen):** floating sprite + click-to-expand, **plus** absorbed proactive bubble from B-form.
- **Backend window (`main.rs` window creation):** new `WebviewWindow` — `transparent`, `decorations:false`, `always_on_top:true`, `skip_taskbar:true`, `resizable:false`, `shadow:false`; small footprint sized to sprite + expanded panel (not full-screen, so transparent areas don't block); bottom-right default, draggable (`data-tauri-drag-region`), position persisted. Loads a dedicated route (`#/pet`).
  - Commands (two-edit): `pet_window_toggle()`/`pet_window_show()`/`pet_window_hide()` bound to `petEnabledAtom`; `pet_window_set_position(x,y)`.
- **Frontend route (`ui/src/components/pet/PetWindow.tsx`):** reuse `PetWidget` sprites + `pet-atoms` state machine (astro/clawby; idle/hover/thinking/typing/success/error WebP crossfade). idle = sprite only → click → compact chat panel (message list + input) → generation drives thinking/typing → stream → success. Proactive bubble subscribes to cross-window events; **v1 trigger set is restrained** (model-ready + long-task-done only).
- **Cross-window:** Tauri events; main `emit('pet://nudge', { text })` → pet bubble. Pet chat is **self-contained**: pet webview calls local `:7337/v1/chat/completions` directly (Slice B), separate from the main agent thread, own system prompt = persona (Slice F).
- **Local-only decision:** pet chat explicitly never silently uses cloud. If local not ready → bubble "我还在热身,去把模型装好呀~" → jumps to Slice D wizard.
- **Context:** independent lightweight pet history store (v1: in-memory + optional light persistence).
- **Edges:** not-ready → guide to setup; multi-monitor + position persistence; always-on-top anti-annoyance (right-click → hide; re-summon from Settings/tray).
- **Tests:** window-control commands (show/hide/toggle/position); pet-chat component Vitest (expand/collapse, streaming render, state-machine, proactive bubble); cross-window event wiring (mock).

## Slice F — Pet persona adapters + roster integration

**Goal:** switch/import pet roles in Settings → MiniCPM; generalize the hardcoded astro/clawby roster into a persona registry; fold MiniCPM-Desk-Pet characters in. **v1 = prompt-level personas** (real LoRA reserved).

- **Model `PetPersona`:** `{ id, name, system_prompt, sprite_set, greeting, voice_params?, source: builtin|imported, lora_adapter? (reserved, v1 no-op) }`. Existing astro/clawby become two seed personas.
- **Storage:** registry JSON under `~/.uclaw/pet_personas/` (builtin seeds + imported); imported sprites copied to `~/.uclaw/pet_personas/<id>/sprites/` (builtin sprites stay in app bundle `/pet/*.webp`).
- **Desk-Pet integration (discovery sub-task):** inspect the MiniCPM-Desk-Pet repo's adapter format (prompt/character config/assets/whether it carries LoRA), write an importer mapping its persona + sprite into `PetPersona` (LoRA portion ignored in v1, recorded to the reserved field). Native uClaw persona JSON bundle is the preferred import format.
- **Backend commands (two-edit):** `pet_persona_list()`, `pet_persona_set_active(id)`, `pet_persona_import(path)`, `pet_persona_delete(id)`.
- **To E:** active persona's `system_prompt` injects into the pet's local-chat system prompt; `sprite_set` selects the WebP set; switching live-updates the pet window (`pet://` event / atom).
- **UI:** the Settings → MiniCPM page (built in D) gains a 宠物角色 section: list + switch + import (file picker → validate → register) + delete. Separate from the main-agent Persona Studio, but a pet persona may optionally borrow its slider values.
- **Edges:** bad bundle → validation error; missing sprites → default sprite fallback; name collision → dedupe.
- **Tests:** registry CRUD (Rust unit); importer parse/validate (native + Desk-Pet formats, with fixtures); persona list/switch/import (Vitest, mock bridge); E integration (switch persona updates system prompt + sprite, mock).

---

## Build order & PR shape

A → B → (C → D) and (B → E → F). A and B are the foundation; C/D are the onboarding track; E/F are the pet track. One plan + one PR per slice (`docs/superpowers/plans/<slice>.md`), each bisectable per repo convention. A is independently valuable on day one (makes 模型分配 actually work for any model).

## Cross-cutting conventions

- **Tauri commands:** every new command is registered in BOTH `tauri_commands.rs` and the `invoke_handler!` macro in `main.rs` (compiles without the macro entry, fails at runtime).
- **New window / background service:** registered in the `[Stage 3]` block in `main.rs`.
- **Zero external runtimes:** no bundled binaries; candle is a pure-Rust crate dependency.
- **Migration registry:** none of these slices add DB migrations as designed; if persona/onboarding state ends up in SQLite rather than JSON config, coordinate a V-number per `CONTEXT.md`.

## Out of scope (v1)

Real LoRA weight hot-swap (mistral.rs upgrade path); cloud fallback for pet chat; proactive-bubble triggers beyond model-ready/task-done; Windows/Linux transparent-window parity tuning (design targets macOS first, the primary platform).
