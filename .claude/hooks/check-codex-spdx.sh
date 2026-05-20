#!/usr/bin/env bash
# PreToolUse hook — requires SPDX-License-Identifier header on every uclaw-utils-*
# Rust source file (these are derived from openai/codex under Apache-2.0).
#
# Mirrors scripts/git-hooks/checks/check-codex-derived-spdx.sh. Block reason: Apache-2.0
# §4(c) requires retaining attribution. See docs/THIRD_PARTY.md §3.2 for the header.
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
# For Edit/MultiEdit we only see the new_string fragment, not the whole file.
# That makes "missing SPDX header" hard to detect at edit time — instead we
# enforce it for Write (full-file content) only.
body = ti.get("content") or ""
print(fp)
print("---PAYLOAD_BOUNDARY---")
print(body)
PY
)"

FILE_PATH="$(printf '%s' "$PAYLOAD" | sed -n '1p')"
BODY="$(printf '%s' "$PAYLOAD" | awk 'f{print} /^---PAYLOAD_BOUNDARY---$/{f=1}')"

case "$FILE_PATH" in
    *crates/uclaw-utils-*/src/*.rs) ;;
    *) exit 0 ;;
esac

# Empty body = Edit/MultiEdit (partial change). Skip — git pre-commit will catch on commit.
[ -z "$BODY" ] && exit 0

if ! printf '%s' "$BODY" | head -10 | grep -q 'SPDX-License-Identifier: Apache-2.0'; then
    cat >&2 <<MSG
BLOCKED: file under crates/uclaw-utils-* is missing the SPDX header.

  file:    $FILE_PATH
  policy:  Apache-2.0 §4(c) requires attribution on derived files
  see:     docs/THIRD_PARTY.md §3.2 for the canonical template

Required header (first lines of the file):

    // SPDX-License-Identifier: Apache-2.0
    // Derived from codex-rs/<path> (https://github.com/openai/codex).
    // Copyright (c) OpenAI. Licensed under Apache License 2.0.
    // See NOTICE in the repository root.

If this file is NOT derived from codex, move it out of crates/uclaw-utils-*/.
MSG
    exit 2
fi

exit 0
