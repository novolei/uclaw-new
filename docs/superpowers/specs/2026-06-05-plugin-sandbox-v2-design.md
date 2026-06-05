# Plugin Sandbox v2: macOS Read-Isolation Design (Slice 4 of the 4-feature batch)

**Date:** 2026-06-05
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b sandbox hardening. Adds read-isolation (crown-jewels denylist) to the #669 macOS seatbelt sandbox. Slice 4 of 4 (uninstall/upgrade #680 · env-config #681 · registry #682 · sandbox v2).

## Problem

The #669 sandbox enforces **write-isolation + network-gating** but reads are **broad** (`allow file-read*`) — required so runtimes (Node/Python) boot (a narrow read-set SIGABRTs them). So a sandboxed plugin (esp. an npx/registry one with `run_subprocess`) can READ anything the user can: `~/.ssh/id_rsa`, `~/.aws/credentials`, cloud tokens, **and uClaw's own DB (which now stores plugin_env secrets / API keys from Slice 2)**. That's the top remaining exfil vector.

## Scope decision (approved 2026-06-05; Linux adjusted for honesty)

The fork choice was "macOS read-isolation (validatable) + Linux landlock best-effort". On inspection:
- **macOS crown-jewels read-isolation** — fully implemented + **macOS-soak-validated**. This slice.
- **Linux landlock** — **deferred to a Linux-validated follow-up, NOT shipped here.** Reason: no Linux target is installed on the dev host, so the `#[cfg(target_os="linux")]` landlock code can't be compile-checked OR runtime-validated here; the repo has **no PR CI gate**, so a compile error would silently break Linux release builds. Shipping unvalidatable/uncompilable sandbox code that could break *all* Linux plugin spawns violates "never ship broken code". The Linux floor (env-scrub + cwd-jail + rlimits from #669) still applies; landlock read/write path-isolation is the documented next step (needs a Linux build+soak host). Recorded in "Out of scope".

## Decision

Keep the broad `(allow file-read*)` (runtime boot), then **append crown-jewels `(deny file-read* ...)` rules** that override it (macOS seatbelt = last-matching-rule-wins), then **re-allow the plugin's own dir** (so the plugin reads itself even though its parent data-dir is denied). Crown jewels = clear secret stores that no plugin legitimately needs, chosen to NOT break npx/uvx runtimes.

## Design

### §1 Crown-jewels denylist (plugins/sandbox.rs `build_seatbelt_profile`)
After `(allow file-read*)` (line 55) and before the write rules, append:
- A pure helper `crown_jewel_read_denials(home: &str, data_dir: &str, plugin_dir: &str) -> String`:
  - `(deny file-read* (subpath "<home>/.ssh") (subpath "<home>/.aws") (subpath "<home>/.gnupg") (subpath "<home>/.config/gcloud") (subpath "<home>/.kube") (subpath "<home>/.docker") (subpath "<home>/.netrc") (subpath "<home>/Library/Keychains") (subpath "<home>/Library/Cookies"))`
  - `(deny file-read* (subpath "<data_dir>"))` — uClaw's own data dir (DB w/ plugin_env secrets, llm config, etc.)
  - `(allow file-read* (subpath "<plugin_dir>"))` — re-allow the plugin's own dir (it lives under data_dir; this overrides the data_dir deny so the plugin can read its own files).
  - (NOT denied: `~/.npmrc`/`~/.pypirc` — npx/uvx read them for registry config; denying could break private-registry installs. Documented tradeoff — their auth-token risk is narrower than ssh/cloud creds.)
- `build_seatbelt_profile` reads `home = std::env::var("HOME").unwrap_or_default()` + derives `data_dir = policy.plugin_dir.parent()/* plugins/ */.and_then(parent)/* data_dir */` and calls the helper, inserting its output after the broad read allow. If HOME is empty or data_dir can't be derived, skip the corresponding denials (fail-open on those — never break the profile).

### §2 Tests
- `crown_jewel_read_denials` pure: contains deny for `<home>/.ssh`, deny for `<data_dir>`, and a re-allow for `<plugin_dir>`; the re-allow appears AFTER the data_dir deny (ordering matters).
- `build_seatbelt_profile` (extend existing): the profile still has `(deny default)`, `(allow file-read*)`, write-jail, network-gate AS BEFORE (v1 invariants preserved), PLUS a `(deny file-read*` line mentioning `.ssh`.
- macOS soak (manual, in the PR): spawn a trivial sandboxed command under the profile that tries to read `~/.ssh` (or a temp "crown jewel") → denied; reading the plugin_dir → allowed; a normal npx server still boots. (Document the soak result in the PR.)

## Data flow

```
plugin spawn (sandboxed) → build_seatbelt_profile(policy):
   (allow file-read*)                         # runtime boots
   (deny file-read* ~/.ssh ~/.aws ... )        # crown jewels (override broad)
   (deny file-read* <data_dir>)                # uClaw secrets/DB
   (allow file-read* <plugin_dir>)             # but the plugin reads itself
→ sandbox-exec runs the MCP subprocess: can read /usr, node libs, its own dir; CANNOT read ssh/cloud creds or uClaw's DB.
```

## Out of scope

- **Linux landlock / seccomp read-write path isolation** — deferred (needs a Linux build+soak host; no CI gate makes blind-shipping unsafe). The Linux env-scrub + cwd-jail + rlimits floor remains.
- Granular per-permission read allowlists (broad-read-minus-jewels is the v2 model, not an allowlist).
- `.npmrc`/`.pypirc` denial (runtime needs them).
- Denying `.env`-by-name (seatbelt is path-based, can't match by filename across the tree).
- Windows sandbox (no sandbox today; floor only).
- Network egress filtering (allow/deny by host — network is all-or-nothing).

## Error handling

If `HOME` is unset → skip the HOME-based denials (the data_dir + plugin_dir rules still apply). If `data_dir` can't be derived from `plugin_dir` (unexpected layout) → skip the data_dir deny (don't emit a malformed rule). The profile is always syntactically valid (deny-by-default base unchanged). A denied read inside the plugin manifests as an EPERM in the subprocess (the plugin handles/logs it; uClaw shows connect status in the detail drawer). No behavior change for plugins that don't touch crown jewels.

## Testing

`cargo test --lib plugins::sandbox` (the pure helpers) + `cargo build`/clippy. macOS soak documented in the PR. No frontend.

## Scope / files

| File | Change |
|---|---|
| `plugins/sandbox.rs` | `crown_jewel_read_denials` helper + wire into `build_seatbelt_profile` + tests |

## Risk

Med (security-sensitive, but single-file + macOS-validatable). Main risks: (1) **seatbelt rule ordering** — denials must come AFTER `(allow file-read*)` and the plugin-dir re-allow AFTER the data_dir deny (last-match-wins); tested + soaked. (2) **breaking runtimes** — crown jewels chosen to exclude paths npx/uvx need (node/npm caches, .npmrc left allowed); soak confirms a normal server still boots. (3) **HOME/data_dir derivation** — fail-open if underivable (never emit a malformed rule). (4) **the plugin's own dir** lives under the denied data_dir → MUST re-allow it last (else the plugin can't read its own server file → SIGABRT). Tested. (5) **Linux honesty** — not shipping unvalidatable Linux code is the deliberate, documented call; the Linux floor still provides env-scrub + cwd + rlimits. Single-file, bisectable. After this slice, macOS plugins cannot read SSH/cloud credentials or uClaw's own secret store — closing the last major exfil vector on the validated platform.
