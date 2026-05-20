#!/usr/bin/env bash
# PreToolUse hook — blocks writes that add new `dirs::home_dir().*.uclaw` constructions.
#
# Policy: `~/.uclaw` resolution must go through `uclaw_utils_home::uclaw_home()` so
# (a) the `UCLAW_HOME` env override works uniformly, and (b) tests can redirect to
# a tmpdir. Mirrors scripts/git-hooks/checks/check-dirs-home-dir-uclaw.sh.
#
# Hook contract: stdin is JSON, exit 2 to block, exit 0 to allow.

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

case "$FILE_PATH" in
    *.rs) ;;
    *) exit 0 ;;
esac

# The crate that legitimately implements uclaw_home() is exempt.
case "$FILE_PATH" in
    *crates/uclaw-utils-home/*) exit 0 ;;
esac

# Pattern: dirs::home_dir() followed (eventually) by a literal containing ".uclaw"
if printf '%s' "$BODY" | grep -qE 'dirs::home_dir\(\)' && \
   printf '%s' "$BODY" | grep -qE '"\.uclaw"|join\(\s*"\.uclaw"'; then
    cat >&2 <<MSG
BLOCKED: this change reaches for ~/.uclaw via dirs::home_dir().

  file:    $FILE_PATH
  policy:  uclaw_utils_home::uclaw_home() is the single canonical resolver
  why:     respects UCLAW_HOME env override + lets tests redirect to a tmpdir
  see:     BEHAVIOR.md §"uClaw-specific rules"

Use:
    use uclaw_utils_home::uclaw_home;
    let path = uclaw_home().join("config.json");
MSG
    exit 2
fi

exit 0
