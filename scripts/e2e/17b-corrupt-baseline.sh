#!/usr/bin/env bash
# Bundle 17-B — corrupt the stored baseline_hash on a session to verify
# load_baseline's hash-mismatch graceful fallback (returns None → next /compact
# takes full-rewrite path, logs WARN, doesn't crash).
#
# Usage: ./17b-corrupt-baseline.sh <session_id>
# Lookup session_id via:
#   sqlite3 ~/.uclaw/uclaw.db "SELECT id, title FROM agent_sessions ORDER BY updated_at DESC LIMIT 5;"

set -euo pipefail
SID="${1:?usage: $0 <session_id>}"
DB="$HOME/.uclaw/uclaw.db"

ROW=$(sqlite3 "$DB" "SELECT COUNT(*) FROM agent_fold_baselines WHERE session_id='$SID'")
if [[ "$ROW" == "0" ]]; then
  echo "no baseline row for $SID — trigger /compact at least once first" >&2; exit 1
fi

echo "[corrupt] before:"
sqlite3 "$DB" "SELECT session_id, substr(baseline_hash,1,12) FROM agent_fold_baselines WHERE session_id='$SID'"

sqlite3 "$DB" "UPDATE agent_fold_baselines SET baseline_hash = 'wrong-hash-deadbeef' WHERE session_id = '$SID'"

echo "[corrupt] after:"
sqlite3 "$DB" "SELECT session_id, substr(baseline_hash,1,12) FROM agent_fold_baselines WHERE session_id='$SID'"
echo "[corrupt] now trigger /compact in the UI for session $SID"
echo "[corrupt] expect log line: '[fold_baseline] baseline_hash mismatch; ignoring stored row'"
echo "[corrupt] expect path: full-rewrite (not delta-rendered)"
echo "[corrupt] row will be repaired with the fresh upsert at the end of the /compact"
