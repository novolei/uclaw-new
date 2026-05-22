# Bundle 17-B E2E scripts

Live-state verification helpers for `/compact` fold-delta path (PR #397, C1.1 PR-1).

## Quick run

```bash
chmod +x scripts/e2e/17b-*.sh

# Terminal 1: live-watch log + DB
./scripts/e2e/17b-live-watch.sh

# Terminal 2: in the uClaw UI, type `/compact` in a session with >10 messages
# Observe in Terminal 1:
#   - first /compact:  "full-rewrite path  threshold=5 had_prior=false"
#                       agent_fold_baselines rows: 0 -> 1
#   - second /compact: "delta-rendered path  drift=N threshold=5"
#                       rows stay at 1, updated_at advances

# Once you've seen at least one /compact, get the session id:
sqlite3 ~/.uclaw/uclaw.db "SELECT id, title FROM agent_sessions ORDER BY updated_at DESC LIMIT 5"

# Then run the report scripts (substitute <SID>):
./scripts/e2e/17b-cache-hit-report.sh <SID>
./scripts/e2e/17b-regression-check.sh
```

## Per-script

| Script | Purpose | When to run |
|---|---|---|
| `17b-live-watch.sh` | Tail log + poll baselines table | Always-on during UI testing |
| `17b-threshold-set.sh N` | Persist `fold_delta_threshold=N` to `memubot_config.json` (no restart) | Before testing edge cases (1 disables delta, 50 widens, 5 default) |
| `17b-corrupt-baseline.sh <SID>` | Manually wreck the stored `baseline_hash` to force the soft-fail path | After a working delta path is observed |
| `17b-cache-hit-report.sh <SID>` | Show recent `cost_records` cache-hit % for a session | After 5-10 turns + 2-3 /compact triggers |
| `17b-regression-check.sh` | Verify legacy `compaction_markers` + `agent_messages.compacted=1` invariants still hold | Once before merge, again after merge |

## L2 — threshold edge cases (manual UI script)

1. Fresh session, send 12-15 messages, `/compact` → expect `full-rewrite` (no prior).
2. Send 2-3 more messages, `/compact` → expect `delta-rendered drift=N`.
3. `./17b-threshold-set.sh 1` → next `/compact` must take `full-rewrite` again (delta disabled).
4. `./17b-threshold-set.sh 50` → next `/compact` should take `delta-rendered` for any drift up to 49.
5. `./17b-threshold-set.sh 5` to restore default.

## L3 — cache hit telemetry

`17b-cache-hit-report.sh <SID>` after the L2 sequence. Compare:
- Pre-baseline turns: `cached_input_tokens` ~0 (no prompt cache to hit yet)
- Post-first-/compact turns: still ~0 unless your LLM provider supports cache
- Post-second-/compact turns (delta path): if Anthropic with cache_control set, expect `cached_input_tokens` > 0 and `hit_pct` ≥ 30%

If `hit_pct` is 0 across all turns, the M2-I cache breakpoint isn't reaching prod yet — that's expected per PR #397's TODO(M2-I) marker, not a regression.

## L4 — regression

`17b-regression-check.sh` enforces:
- Every session with a baseline has at least one `compaction_markers` row (because the marker is written *before* the LLM summarize call — if baseline exists, the marker must have been written first)
- Most recent compacted messages match the `compaction_markers` count

If either WARN fires, the legacy /compact phase 1/3 plumbing has regressed and needs investigation.
