#!/usr/bin/env bash
# Bundle 17-B — token-savings smoke for a given session.
# Shows the most recent cost_records for the session, annotated by whether
# the turn happened before or after the most recent agent_fold_baselines
# upsert. After-baseline turns SHOULD show rising cached_input_tokens if the
# delta-rendered path is working.
#
# Usage: ./17b-cache-hit-report.sh <session_id>

set -euo pipefail
SID="${1:?usage: $0 <session_id>}"
DB="$HOME/.uclaw/uclaw.db"

echo "==== baseline events for $SID ===="
sqlite3 "$DB" -header -column \
  "SELECT datetime(updated_at/1000,'unixepoch','localtime') AS baseline_ts, length(fold_json) AS fjsize, substr(baseline_hash,1,8) AS hash FROM agent_fold_baselines WHERE session_id='$SID';"

echo ""
echo "==== last 15 cost_records turns (model, input, cached, cache hit %) ===="
sqlite3 "$DB" -header -column "
SELECT
  datetime(created_at/1000,'unixepoch','localtime') AS ts,
  substr(model,1,20) AS model,
  input_tokens AS input,
  cached_input_tokens AS cached,
  CASE WHEN input_tokens > 0
    THEN printf('%.1f%%', 100.0 * cached_input_tokens / input_tokens)
    ELSE '—' END AS hit_pct,
  output_tokens AS output
FROM cost_records
WHERE session_id = '$SID'
ORDER BY created_at DESC
LIMIT 15;"

echo ""
echo "==== summary ===="
sqlite3 "$DB" -header -column "
SELECT
  COUNT(*) AS turns,
  SUM(input_tokens) AS input_total,
  SUM(cached_input_tokens) AS cached_total,
  printf('%.1f%%', 100.0 * SUM(cached_input_tokens) / NULLIF(SUM(input_tokens),0)) AS overall_hit
FROM cost_records WHERE session_id = '$SID';"
