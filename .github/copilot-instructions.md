# GitHub Copilot instructions for uClaw

Repository-wide custom instructions for GitHub Copilot working in this repo
(VS Code, JetBrains, web).

## Canonical sources

uClaw has a single behavior contract that applies to every agent and IDE.
Before suggesting non-trivial changes, read:

1. `BEHAVIOR.md` — the canonical 10-practice multi-session behavior contract.
2. `CONTEXT.md` — project reference: architecture, build, migration registry.
3. `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md` — strategic baseline.

The summary below is inlined for fast loading. **`BEHAVIOR.md` wins on any
conflict.**

## Critical rules

- **License**: Apache-2.0. Files derived from `openai/codex` carry an SPDX
  header (`SPDX-License-Identifier: Apache-2.0`) and `Derived from codex-rs/<path>`
  attribution in the first 10 lines, plus an entry in `NOTICE`. See
  `docs/THIRD_PARTY.md` §3.2.
- **`memory_graph` is FROZEN** (ADR §11.2). Do not generate code that calls
  `memory_graph::{write,insert,update,delete}*`. Suggest `gbrain` instead
  for new durable facts. Pre-commit hook will block any violation.
- **`dirs::home_dir().*".uclaw"` is banned**. Suggest
  `uclaw_utils_home::uclaw_home()` (and its sibling helpers) for paths
  under `~/.uclaw/`. The crate lands in Phase 0.5-T5; existing call sites
  in `tauri_commands.rs` and `memubot_config.rs` are allowlisted until the
  Phase 0.5-T6 sweep.
- **Plan first for non-trivial work**: explore → plan → implement → commit.
  Plans live in `docs/superpowers/plans/`. One plan = one PR with bisectable
  commits.
- **Verification mandatory**: every PR's commit body should include the
  verification command and its expected output.
- **Migration registry**: see `CONTEXT.md` § Active migration registry.
  Never reuse a V-number.
- **DMZ files** (`agentic_loop.rs`, `tauri_commands.rs`, `CLAUDE.md`,
  `db/migrations.rs`, `Cargo.toml` workspace root, `BEHAVIOR.md`): touching
  these requires a writer/reviewer two-session review.
- **ADR §18 11 questions**: every strategic spec must answer all 11. See the
  ADR before drafting one.

## Style — Rust

- Edition 2021. Rust toolchain pinned in `rust-toolchain.toml`.
- Prefer flat enumeration over generic dispatchers — uClaw's existing
  `search_conversations` UNION pattern is the example.
- Inline format args: `format!("foo: {x}")` not `format!("foo: {}", x)`.

## Style — TypeScript / React

- React 18 + TypeScript strict. Tailwind + Radix UI + Jotai.
- Always use theme tokens (`bg-popover`, `text-muted-foreground`), never
  hardcoded colors like `bg-zinc-900`. There are 11 themes; hardcoded
  colors break under warm-paper / qingye / forest-* themes.
- `@/*` path alias maps to `ui/src/*`.

## Install pre-commit hooks

After cloning: `./scripts/install-git-hooks.sh`. Hooks deterministically
enforce the rules above; bypass with `git commit --no-verify` only for real
emergencies.
