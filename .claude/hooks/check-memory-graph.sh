#!/usr/bin/env bash
# PreToolUse hook — blocks writes that add new `memory_graph::write|insert|update|delete*` calls.
#
# Policy: ADR `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md` §11.2 freezes
# memory_graph writes. New code goes through gbrain (Path C-2). Reads are still allowed.
#
# Mirrors scripts/git-hooks/checks/check-memory-graph-freeze.sh but runs at edit time
# so Claude catches it before the file is even written.
#
# Hook contract: stdin is JSON, exit 2 to block (stderr shown to model), exit 0 to allow.

set -u
INPUT="$(cat)"

PAYLOAD="$(python3 - "$INPUT" <<'PY'
import json, sys
try:
    data = json.loads(sys.argv[1])
except Exception:
    sys.exit(0)
ti = data.get("tool_input") or {}
fp = ti.get("file_path") or ""
parts = []
for k in ("content", "new_string"):
    v = ti.get(k)
    if isinstance(v, str):
        parts.append(v)
for ed in ti.get("edits") or []:
    v = ed.get("new_string")
    if isinstance(v, str):
        parts.append(v)
print(fp)
print("---PAYLOAD_BOUNDARY---")
print("\n".join(parts))
PY
)"

FILE_PATH="$(printf '%s' "$PAYLOAD" | sed -n '1p')"
BODY="$(printf '%s' "$PAYLOAD" | awk 'f{print} /^---PAYLOAD_BOUNDARY---$/{f=1}')"

# Only enforce on Rust source under src-tauri/src/. Anything else (docs, migrations,
# ADRs that legitimately reference the symbol name) is fine.
case "$FILE_PATH" in
    *src-tauri/src/*.rs) ;;
    *) exit 0 ;;
esac

# Exempt the memory_graph module itself (the actual implementation needs to write).
case "$FILE_PATH" in
    *src-tauri/src/memory_graph/*) exit 0 ;;
esac

if printf '%s' "$BODY" | grep -qE 'memory_graph::(write|insert|update|delete)[a-z_]*'; then
    cat >&2 <<MSG
BLOCKED: this change adds a memory_graph write/insert/update/delete call.

  file:    $FILE_PATH
  policy:  ADR §11.2 freezes memory_graph writes (gbrain is primary, Path C-2)
  see:     BEHAVIOR.md §"uClaw-specific rules" + docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md
  use:     gbrain MCP (Sprint 2.1) for new knowledge writes

If this is genuinely needed (e.g. a migration helper inside memory_graph/), the
hook exempts that path. Otherwise the DRI must approve before this can land.
MSG
    exit 2
fi

exit 0
