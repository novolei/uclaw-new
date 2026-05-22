---
name: uclaw-tick-feature-verify
description: Use when E2E-verifying a uClaw feature whose production behavior is driven by a slow tick / cron / scheduler — typical signs are `tick_count % N == 0` branches in `proactive/service.rs`, periodic batch passes (prune, promote, decay, drift, lint, learning), or "fires every ~hours" comments in `docs/superpowers/specs/*.md`. Trigger phrases include "verify the bundle", "E2E test the tick", "test the prune pass", "run the promotion pipeline end-to-end", "exercise the scheduler", "test the proactive scenario", "dogfood the bundle", "verify before merging", "bundle verification script". Loads the temp-verify-branch + lowered-modulos + seeded-fixtures + trap-based-cleanup pattern that produced PR #396's E2E verification.
---

# uClaw — Tick-driven Feature E2E Verification

When you ship a feature that fires inside `ProactiveService::tick_inner` (or
any `tokio::time::interval`-driven loop), the production cadence is almost
always too slow for fast feedback — the `% 240` branch fires every ~2h, the
`% 60` branch every ~30min. Waiting for natural cadence kills the
edit-test-iterate loop. This skill encodes the verify pattern that worked
for Bundle 26-B / 26-D / 27-B and that should generalize to any other
tick-driven uClaw feature.

## When this skill applies

- New feature lives behind a tick-modulo gate (`tick_count % N == 0`).
- The feature has **thresholds** that decide behavior (`min_unused_days`,
  `min_returned_count`, `daily_token_budget`, etc.).
- The feature has **observable side effects** on disk, in SQLite, or
  in a shared Arc pool (`gene_candidate_pool`, `memory_graph_store`).
- You want green/red answer **within minutes**, not hours.

If the feature only logs and doesn't change disk/DB state, you may not
need fixtures — `tail -f` + a deliberate trigger may be enough.

## The five-axis design

Every verify script worth writing covers these five axes:

1. **Time** — lower tick modulos so the loop fires inside a coffee break,
   not overnight. Edit `proactive/service.rs` via sed on a throwaway
   branch; `git checkout -- file` on cleanup. Never commit these.

2. **Thresholds** — when the feature has settings-page knobs, write
   aggressive values to `~/.uclaw/memubot_config.json` (after backing it
   up). When the feature has no settings exposure yet, the script also
   has to sed the threshold constants — and that's a flag that you
   should expose them to settings as a separate PR (see PR #396).

3. **Fixtures** — seed **all three judgment paths**:
   - **Eligible** (expect: trigger fires, side effect happens)
   - **Eligible-but-different-axis** (expect: same trigger doesn't fire
     because a sibling gate fails — e.g. `returned_count > 0` blocks
     stale-pruning even for an old skill)
   - **Control** (expect: nothing happens — already-promoted /
     already-archived / disabled-flag — proves you're not just blindly
     processing everything)
   Always make sure your fixture round-trips through serde — read the
   `Deserialize` impl for required fields (e.g. SkillMeta needs
   `slug`, `created_at`, `updated_at` without serde defaults).

4. **Observability** — `tail -f` + `grep -F "[Bundle XX-Y]"` is enough.
   Make sure your feature's INFO-level log line has a unique, greppable
   tag. If it only logs at DEBUG when "nothing to do," that's not a bug
   — it's a verify-time signal that your fixtures aren't being picked
   up at all.

5. **Cleanup** — `trap cleanup EXIT INT TERM`. Restore the config
   backup, remove fixtures, remove archived/promoted side effects,
   `git checkout -- <patched_files>`, switch back to original branch,
   delete the verify branch. Use `$DID_X = true/false` guards so
   half-done runs cleanup correctly. Provide `--keep-on-fail` so debug
   runs preserve evidence.

## The canonical reference implementation

[`scripts/verify/bundle-26bd-27b.sh`](../../scripts/verify/bundle-26bd-27b.sh)
is the worked example. Pattern its shape when writing a new verify
script:

```
#!/usr/bin/env bash
# Header: usage block extractable via `sed -n '2,30p' "$0"`
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# Constants block: every path one variable.
CONFIG_PATH="${HOME}/.uclaw/memubot_config.json"
CONFIG_BACKUP="...verify-NAME-backup"
SERVICE_FILE="${REPO_ROOT}/src-tauri/src/proactive/service.rs"
LOG_FILE="/tmp/uclaw-verify-NAME.log"
APP_PID_FILE="/tmp/uclaw-verify-NAME.pid"
VERIFY_BRANCH="verify/NAME-LOCAL"

# Guards for cleanup
APPLY=false
KEEP_ON_FAIL=false
DID_BRANCH_SWITCH=false
DID_CONFIG_PATCH=false
DID_SEED=false
APP_STARTED=false

cleanup() {
  local rc=$?
  # 1. Kill app (SIGINT → 5s → SIGKILL)
  # 2. Restore config from backup if $DID_CONFIG_PATCH
  # 3. Remove fixtures + side effects if $DID_SEED
  # 4. git checkout -- $SERVICE_FILE + branch switch + branch delete
  #    if $DID_BRANCH_SWITCH
  # ... and print PASS/FAIL based on $rc
  exit $rc
}
trap cleanup EXIT INT TERM

# Step 1: pre-flight (deps, working tree clean modulo untracked, branch)
# Step 2: --apply gate; if dry-run, print plan and exit 0
# Step 3: branch + sed modulos
# Step 4: backup config + write aggressive thresholds
# Step 5: seed fixtures (one per judgment path)
# Step 6: cargo build + cargo run >$LOG_FILE 2>&1 &
# Step 7: poll-and-grep loop with --quick / --natural timeout
# Step 8: filesystem assertions
# Step 9: verdict
```

## Common pitfalls (caught during PR #396 verification)

1. **`#[serde(default)]` is per-field, not per-struct.** Even when
   the parent struct has `#[serde(default)]`, individual fields can
   still be required if they lack a per-field default. Read the
   target struct's `Deserialize` impl carefully before constructing a
   fixture JSON. Symptom: scanner silently treats your fixture as
   "meta unreadable" and your eligible-path test fails for no
   apparent reason.

2. **Sibling vs nested directory.** Don't assume archive/staging
   dirs are children of the feature dir. Read the actual code:
   `archive_skill()` writes to `data_dir/skills/_archive/<TS>/<slug>/`,
   NOT `data_dir/skills/_auto_extracted/_archive/`. The PR body for
   the original 26-B PR even had this wrong — only the code is the
   source of truth.

3. **Cleanup empties parent dirs.** After removing your seeded
   archive entries, the parent `_archive/<TS>/` dir is now empty —
   `rmdir` it to keep `_archive/` clean. Otherwise you accumulate
   one empty TS dir per verify run.

4. **`tokio::time::timeout(Duration::ZERO, ...)` fires immediately.**
   If your feature uses an idle timeout configured via settings and
   the settings value happens to be 0, you'll tear down healthy
   streams. Normalize zero to a sane fallback (see `llm_stream.rs`'s
   `STREAM_IDLE_TIMEOUT_FALLBACK` pattern).

5. **`cargo run` is blocking — background it.** Always
   `cargo run >$LOG_FILE 2>&1 &` and capture the PID to a file the
   cleanup trap reads. SIGINT then SIGKILL after a few seconds — the
   Tauri main loop responds to SIGINT.

## When you DON'T need this skill

- The feature fires on user action (button click, IPC), not a timer
  — call the Tauri command directly via `invoke()` from the settings
  UI or from `cargo test`.
- The feature is pure logic with no side effects — write a unit
  test in `#[cfg(test)] mod tests` instead of an E2E script.
- You're verifying a single fix, not a feature suite — `cargo test`
  + manual UI dogfood is faster than a script.

## After verify passes

Don't commit `verify/<name>-LOCAL` branch. The trap deletes it.
The whole point is the working code stays on the merged branch and
the verify is reproducible from `scripts/verify/<name>.sh` against
that code, not against a frozen branch.

If verify finds a real bug in the merged code, fix it on a fresh
prep branch + PR — the verify script is the regression catcher,
not the bug-fix vehicle.
