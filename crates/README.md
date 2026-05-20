# `crates/` — uClaw + codex-derived utility crates

This directory is populated by Phase 0.5-T3 (first 6 crates) and Phase 0.5-T5
(next 11 crates) per:

- `uclaw-codex-comparison-and-design.md` §17 — copy/clone tiered list
- `uclaw-upgrade-implementation-plan.md` §18 — crate-by-crate integration map

## Naming

All crates here use the prefix `uclaw-utils-<topic>` (e.g. `uclaw-utils-string`,
`uclaw-utils-stream`, `uclaw-utils-home`). The prefix signals "derived from
openai/codex codex-rs under Apache-2.0" and triggers `.claude/hooks/check-codex-
spdx.sh` to enforce the SPDX header.

## Required header on every `.rs` file

```rust
// SPDX-License-Identifier: Apache-2.0
// Derived from codex-rs/<path> (https://github.com/openai/codex).
// Copyright (c) OpenAI. Licensed under Apache License 2.0.
// See NOTICE in the repository root.
```

See `docs/THIRD_PARTY.md` §3.2 for the canonical template and `NOTICE` for the
upstream commit pin.

## Adding a crate

1. `mkdir crates/uclaw-utils-<topic>`
2. `cargo new --lib --vcs none crates/uclaw-utils-<topic>` (or copy upstream as-is)
3. Replace package metadata with workspace-inherited fields:
   ```toml
   [package]
   name = "uclaw-utils-<topic>"
   version = "0.1.0"
   description = "..."
   edition.workspace      = true
   license.workspace      = true
   authors.workspace      = true
   repository.workspace   = true
   rust-version.workspace = true
   ```
4. Add the SPDX header to every `.rs` file.
5. Register in the workspace by appending to `Cargo.toml`'s `[workspace] members`:
   ```toml
   members = [
       "src-tauri",
       "crates/uclaw-utils-<topic>",
   ]
   ```
6. Update `NOTICE` (root) with the new derived-crate entry.
7. Run `cargo build --workspace` and `cargo test --workspace`.

See the `uclaw-codex-derived` skill (`.claude/skills/uclaw-codex-derived/SKILL.md`)
for the full porting procedure.
