# Third-Party Code & License Compliance

> This document is the canonical procedure for adding, modifying, or removing
> third-party derived code in uClaw. **Read this before touching `NOTICE`,
> `LICENSE`, `licenses/`, or any file imported from an external source.**

---

## 1. License decision

uClaw is licensed under **Apache License, Version 2.0** (see `LICENSE`).

This was a deliberate choice for the following reasons (see
`uclaw-codex-comparison-and-design.md` §3 for the full rationale):

- **Direct compatibility with codex (Apache-2.0)** — most derived utility
  crates come from `openai/codex`.
- **Explicit patent grant (§3)** — material protection for an AI agent
  product where the patent landscape is unsettled.
- **Rust/Tauri ecosystem alignment** — Tauri itself is dual MIT/Apache-2.0
  and the majority of Rust crates publish under MIT OR Apache-2.0.
- **Commercial flexibility** — Apache-2.0 is not copyleft; closed-source
  plugins, hosted SaaS, and proprietary integrations are permitted as long
  as `LICENSE` and `NOTICE` are preserved in distributions.

## 2. Compliance checklist (Apache-2.0 §4)

When distributing uClaw (binary or source), the four §4 obligations must be
satisfied:

| § | Obligation | uClaw practice |
|---|---|---|
| 4(a) | Recipient gets a copy of the License | `LICENSE` at repo root + `licenses/apache-2.0.txt` |
| 4(b) | Modified files carry prominent notices | SPDX header in every file + "modified from …" comment when applicable |
| 4(c) | NOTICE file (if any) is preserved in derivatives | `NOTICE` at repo root, included in installer/bundle |
| 4(d) | New derived works may add their own attribution but cannot alter the License of the parent | New crates copied in carry their own SPDX header and append to `NOTICE`, never modify `LICENSE` |

## 3. Procedure for adding derived code

Follow these steps **every time** code is copied (with or without modification)
from an external repository:

### 3.1 Pre-flight

1. Verify the upstream license is **compatible with Apache-2.0**.
   - ✅ Compatible: Apache-2.0, MIT, BSD-2/3, ISC, Unlicense, MPL-2.0 (file-level), zlib.
   - ❌ Incompatible without ADR override: GPL/LGPL/AGPL (any version), CC-BY-SA, BSL, proprietary EULA.
2. Record the upstream **commit hash** of what is being copied (not just "main").
3. If an upstream `NOTICE` file exists, it must be merged into our `NOTICE` (Apache-2.0 §4(c) is non-optional).

### 3.2 Copy / modify

1. Copy the source into the appropriate uClaw location (e.g. `src-tauri/uclaw-utils-*/` for utility crates).
2. **Rename the crate** if it is a Rust crate: `codex-utils-template` → `uclaw-utils-template` (avoid collision and confusion).
3. Add an SPDX header to every Rust source file at the top:

```rust
// SPDX-License-Identifier: Apache-2.0
// Derived from codex-rs/utils/<name> (https://github.com/openai/codex).
// Copyright (c) OpenAI. Licensed under Apache License 2.0.
// See NOTICE in the repository root.
```

   If you modify the file relative to upstream, append:

```rust
// Modifications by uClaw contributors:
//   - <summary of what changed and why>
```

4. Append an entry to `NOTICE` under the "Derived crates" list with the upstream path and a one-line summary of any modification.

### 3.3 Verify

1. `grep -rL 'SPDX-License-Identifier' src-tauri/uclaw-*/src/` should return empty (every file has a header).
2. `cargo build` passes.
3. CI lint (`.claude/hooks/block_codex_derived_without_notice.sh`, to be added in Phase 0.5-T9) does not block.

## 4. Procedure for modifying already-derived code

If you change a file that was derived from upstream:

1. Update the file header: append a `// Modifications by uClaw contributors:` block summarizing the change.
2. Update `NOTICE` so the entry for that crate reflects "with modifications" and lists the modification summary.
3. Do **not** strip the original SPDX header or "Derived from …" line.

## 5. Procedure for removing derived code

1. Delete the source.
2. Remove the entry from `NOTICE`.
3. If the removal is meaningful (drops an entire crate), note it in the next ADR or design doc.

## 6. Upstream sync (codex)

`NOTICE` pins a specific upstream codex commit hash. To re-sync against a newer
codex commit:

1. Read upstream `CHANGELOG.md` (or commit history) between pinned hash and new hash.
2. If any breaking change touches a derived crate: update our copy and document
   the diff in the relevant file header.
3. Bump the pinned hash in `NOTICE`.
4. Run full test suite + harness.
5. The bump goes in its own PR titled `chore(deps): bump codex pin to <hash>`.

Recommended sync cadence: **quarterly**, or when a specific issue motivates it.

## 7. Trademark posture

uClaw is **not** endorsed by, sponsored by, or affiliated with OpenAI. "Codex"
and any associated trademarks remain the property of their respective owners.
Public-facing copy must:

- Refer to upstream as `openai/codex` or "OpenAI Codex CLI" (factual).
- Not imply partnership, certification, or endorsement.
- Not use the OpenAI logo without separate permission.

## 8. Questions / disputes

Open a `governance/` issue in the repo or contact the DRI listed in `CLAUDE.md`
under "License & compliance".

---

**Last reviewed**: 2026-05-20
**Next scheduled review**: 2026-08-20 (quarterly cadence)
