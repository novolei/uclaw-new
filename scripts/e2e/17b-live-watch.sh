#!/usr/bin/env bash
# Bundle 17-B — live-watch: tail the uClaw log + poll agent_fold_baselines.
# Run this in one terminal, then type `/compact` in the uClaw UI.
#
# Expected on FIRST /compact of a fresh session:
#   [/compact] M2-G StructuredFold produced
#   [/compact] no prior baseline (first compact) — full-rewrite
#   [/compact] full-rewrite path  threshold=5 had_prior=false
#   [db] agent_fold_baselines: 0 -> 1
#
# Expected on SECOND /compact of the same session (small drift):
#   [/compact] M2-G StructuredFold produced
#   [/compact] delta-rendered path  drift=N threshold=5
#   [db] (count stays at 1, updated_at advances)

set -uo pipefail

LOG="$HOME/.uclaw/logs/uclaw.log.$(date +%Y-%m-%d)"
DB="$HOME/.uclaw/uclaw.db"

[[ -f "$LOG" ]] || { echo "no log at $LOG — uClaw not running?" >&2; exit 1; }

echo "[live-watch] log: $LOG"
echo "[live-watch] db:  $DB"
echo "----"

(
  prev=$(sqlite3 "$DB" "SELECT COUNT(*) FROM agent_fold_baselines")
  echo "[db] starting agent_fold_baselines rows = $prev"
  while true; do
    sleep 2
    cur=$(sqlite3 "$DB" "SELECT COUNT(*) FROM agent_fold_baselines")
    if [[ "$cur" != "$prev" ]]; then
      echo "[db] rows: $prev -> $cur"
      sqlite3 "$DB" "SELECT session_id, length(fold_json) AS fjsize, substr(baseline_hash,1,8) AS hash, datetime(updated_at/1000,'unixepoch','localtime') AS ts FROM agent_fold_baselines ORDER BY updated_at DESC LIMIT 3" \
        | sed 's/^/[db]   /'
      prev=$cur
    fi
  done
) &
DB_PID=$!
trap "kill $DB_PID 2>/dev/null; exit 0" INT TERM

tail -F "$LOG" 2>/dev/null \
  | grep --line-buffered -E "/compact|fold_delta|delta-rendered|full-rewrite|fold_baseline|baseline read|baseline upsert|Bundle 17|StructuredFold produced"
