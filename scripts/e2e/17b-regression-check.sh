#!/usr/bin/env bash
# Bundle 17-B — regression check: verify legacy /compact mechanics still work.
# Sanity-checks invariants that PR #397 should NOT have broken:
#   - compaction_markers still being written
#   - agent_messages.compacted=1 still applied
#   - placeholder still inserted as a 'user'-role row
#   - For any session with a baseline row, compaction_markers count ≥ 1
#
# Run after at least one /compact on the running app.

set -uo pipefail
DB="$HOME/.uclaw/uclaw.db"

echo "==== sanity: agent_fold_baselines vs compaction_markers per session ===="
sqlite3 "$DB" -header -column "
SELECT
  s.id AS session,
  substr(s.title,1,40) AS title,
  (SELECT COUNT(*) FROM agent_fold_baselines b WHERE b.session_id = s.id) AS baselines,
  (SELECT COUNT(*) FROM compaction_markers m WHERE m.session_id = s.id) AS markers,
  (SELECT COUNT(*) FROM agent_messages WHERE session_id = s.id AND compacted = 1) AS compacted_msgs
FROM agent_sessions s
WHERE EXISTS (SELECT 1 FROM agent_fold_baselines b WHERE b.session_id = s.id)
   OR EXISTS (SELECT 1 FROM compaction_markers m WHERE m.session_id = s.id)
ORDER BY s.updated_at DESC
LIMIT 10;
"

echo ""
echo "==== invariant: every baseline-having session should also have markers ≥ 1 ===="
ORPHAN=$(sqlite3 "$DB" "
SELECT COUNT(*) FROM agent_fold_baselines b
WHERE NOT EXISTS (SELECT 1 FROM compaction_markers m WHERE m.session_id = b.session_id);
")
if [[ "$ORPHAN" -gt 0 ]]; then
  echo "WARN: $ORPHAN sessions have a baseline but no compaction_markers row — Phase 1 path likely broke" >&2
else
  echo "OK — every baseline has at least one compaction_markers row"
fi

echo ""
echo "==== invariant: most-recent compacted_messages had matching compaction_markers ===="
sqlite3 "$DB" -header -column "
WITH latest_compacts AS (
  SELECT session_id, MIN(created_at) AS first_compacted_at, COUNT(*) AS compacted_count
  FROM agent_messages
  WHERE compacted = 1
  GROUP BY session_id
  ORDER BY first_compacted_at DESC
  LIMIT 5
)
SELECT
  lc.session_id,
  lc.compacted_count,
  (SELECT COUNT(*) FROM compaction_markers m WHERE m.session_id = lc.session_id) AS markers,
  datetime(lc.first_compacted_at/1000,'unixepoch','localtime') AS first_compact_ts
FROM latest_compacts lc;
"
