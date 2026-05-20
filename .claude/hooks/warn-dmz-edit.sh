#!/usr/bin/env bash
# PreToolUse hook — emits a non-blocking advisory when an edit touches a DMZ file.
#
# DMZ files are high-blast-radius surfaces that need Writer/Reviewer two-session review
# per BEHAVIOR.md §"DMZ Files Need Two-Session Review". The hook does NOT block — it
# just reminds the agent (and the reviewing human) to slow down.
#
# Hook contract: stdin is JSON, exit 0 (always — advisory only), stderr is the warning.

set -u
INPUT="$(cat)"

FILE_PATH="$(python3 - "$INPUT" <<'PY'
import json, sys
try:
    data = json.loads(sys.argv[1])
except Exception:
    sys.exit(0)
ti = data.get("tool_input") or {}
print(ti.get("file_path") or "")
PY
)"

DMZ_MATCH=""
case "$FILE_PATH" in
    *src-tauri/src/agent/agentic_loop.rs)   DMZ_MATCH="agentic_loop.rs (the agent loop — every behavior change ripples through every session)" ;;
    *src-tauri/src/tauri_commands.rs)       DMZ_MATCH="tauri_commands.rs (must keep invoke_handler! macro in sync in main.rs)" ;;
    *src-tauri/src/db/migrations.rs)        DMZ_MATCH="db/migrations.rs (pick next free V-number; coordinate with open PRs)" ;;
    */CLAUDE.md)                            DMZ_MATCH="CLAUDE.md (read every session — keep ≤120 lines)" ;;
    */BEHAVIOR.md)                          DMZ_MATCH="BEHAVIOR.md (canonical multi-session contract — verify with DRI)" ;;
    */Cargo.toml)
        # Only flag the root workspace Cargo.toml, not crate-local ones.
        case "$FILE_PATH" in
            */uclaw-cowork/Cargo.toml|*/uclaw/Cargo.toml) DMZ_MATCH="workspace root Cargo.toml (workspace member edits affect every crate)" ;;
        esac
        ;;
esac

if [ -n "$DMZ_MATCH" ]; then
    cat >&2 <<MSG
DMZ EDIT WARNING (advisory — not blocking):

  file:    $FILE_PATH
  note:    $DMZ_MATCH
  policy:  BEHAVIOR.md §"DMZ Files Need Two-Session Review"
           — Writer drafts, Reviewer (separate session OR human) re-reads diff

This is just a reminder. The edit will proceed. Confirm you have a reviewer
lined up before merging.
MSG
fi

exit 0
