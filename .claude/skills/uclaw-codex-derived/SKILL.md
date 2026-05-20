---
name: uclaw-codex-derived
description: Use whenever you copy, port, or adapt code from openai/codex (the Codex CLI Rust workspace at /Users/ryanliu/Documents/Hero/codex). Trigger phrases include "from codex", "port codex", "copy crate", "codex-rs", "uclaw-utils-*", "SPDX header", "Apache-2.0", "derived crate", "absorb codex", "NOTICE file". Loads the SPDX header template, the NOTICE update procedure, the renaming convention, and the legal-cleanliness checklist required by Apache-2.0 §4(c).
---

# uClaw — Codex-Derived Code

uClaw is **Apache-2.0**. Codex is **Apache-2.0**. License-compatible. But
§4(c) of Apache-2.0 requires attribution on derived files. uClaw enforces
this in three places:

1. Per-file SPDX header (this skill, + `.claude/hooks/check-codex-spdx.sh`)
2. `NOTICE` file at repo root (pins the upstream commit + lists derived crates)
3. `docs/THIRD_PARTY.md` §3 (the procedure of record)

## When this skill applies

Any time you copy code from `~/Documents/Hero/codex/codex-rs/<path>/...` —
whether a whole crate, a module, or even a single function — into uClaw.

Naming convention: **derived crates go under `crates/uclaw-utils-<topic>/`**.
This signals "derived" without anyone reading the SPDX header.

## Required SPDX header (top of every derived `.rs` file)

```rust
// SPDX-License-Identifier: Apache-2.0
// Derived from codex-rs/<path> (https://github.com/openai/codex).
// Copyright (c) OpenAI. Licensed under Apache License 2.0.
// See NOTICE in the repository root.
```

Fill `<path>` with the **upstream path**, e.g. `template/src/lib.rs`.

`.claude/hooks/check-codex-spdx.sh` blocks `Write` of any file under
`crates/uclaw-utils-*/src/` that's missing this header. The matching git
pre-commit hook (`scripts/git-hooks/checks/check-codex-derived-spdx.sh`)
provides the safety net.

## Procedure — porting a codex crate to uclaw

1. **Read the codex source first.** Use shell:
   ```bash
   ls ~/Documents/Hero/codex/codex-rs/<crate>/
   cat ~/Documents/Hero/codex/codex-rs/<crate>/Cargo.toml
   cat ~/Documents/Hero/codex/codex-rs/<crate>/src/lib.rs
   ```
   You can't use Read on this path (it's outside the mounted directories) —
   use the bash tool.

2. **Choose the uclaw crate name.** Map `codex-rs/<topic>/` →
   `crates/uclaw-utils-<topic>/`. Keep the topic name unless it clashes
   with an existing uclaw concept.

3. **Copy + adapt** — usually a minimal adapt:
   - Add the SPDX header to every `.rs` file
   - Update `Cargo.toml` `[package]`:
     - `name = "uclaw-utils-<topic>"`
     - `license = "Apache-2.0"`
     - Strip `repository`, `homepage` (those reference upstream)
   - Replace any `codex-*` workspace deps with `uclaw-utils-*` equivalents
     (or external crates if a uclaw equivalent doesn't exist yet)

4. **Update `NOTICE`** — add an entry under "Derived crates":
   ```
   - crates/uclaw-utils-<topic>/  ← codex-rs/<topic>/ (commit <pinned-sha>)
   ```
   The commit SHA at the top of `NOTICE` pins the upstream revision. If
   you bump the pin, do it in a dedicated PR and update every derived
   crate's "as of" comment.

5. **Wire it in** — usually as a workspace dep. Add to the root
   `Cargo.toml`'s `[workspace.dependencies]` block; consume from any crate
   that needs it.

6. **Verify the SPDX hook** by attempting an Edit on the new file — the
   in-session hook should already be happy (you added the header). The
   git pre-commit hook will check on commit.

## Procedure — porting a single file (not a whole crate)

Same as above, but you can place the file directly in an existing
`uclaw-utils-*` crate OR in `src-tauri/src/<area>/` with the SPDX header.

If the file is going somewhere OTHER than `crates/uclaw-utils-*`, the
in-session SPDX hook won't fire — be extra careful to add the header
manually, and document the file in `NOTICE` under "Derived files
(outside uclaw-utils-* crates)".

## Frequency-and-batch advice

The `uclaw-codex-comparison-and-design.md` §17 + §24 lists **17 codex
crates** scheduled for clean copy + 1 microchange. Do them in batches of
3–5, not all at once. Each batch is a PR. Each crate within a batch is its
own commit, so the PR is bisectable.

## What NOT to do

- ❌ Don't strip the SPDX header to "clean up" the file. It's required.
- ❌ Don't rename a derived crate without updating `NOTICE` and the SPDX
  `Derived from codex-rs/<path>` line in every file.
- ❌ Don't copy code from any branch of codex other than the pinned commit.
  The pin in `NOTICE` is the contract.
- ❌ Don't introduce GPL or AGPL deps inside a derived crate. Apache-2.0
  is incompatible with GPL-2.0; the tree stays clean.

## See also

- `NOTICE` — root file, pins upstream commit + lists derived crates
- `docs/THIRD_PARTY.md` — full procedure
- `uclaw-codex-comparison-and-design.md` §17 + §24 — the 17-crate batch plan
- `.claude/hooks/check-codex-spdx.sh` + `scripts/git-hooks/checks/check-codex-derived-spdx.sh`
