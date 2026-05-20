#!/usr/bin/env bash
# Files inside derived crates (src-tauri/uclaw-{utils,async,file}-*) must carry
# both an SPDX-License-Identifier: Apache-2.0 header AND a "Derived from
# codex-rs/" attribution comment in their first 10 lines.
#
# See docs/THIRD_PARTY.md §3.2 for the canonical template.
# POSIX/bash3-compatible (no mapfile).

set -euo pipefail

STAGED=()
while IFS= read -r f; do
    [ -n "$f" ] && STAGED+=("$f")
done < <(git diff --cached --name-only --diff-filter=AM \
    | grep -E '\.rs$' \
    | grep -E '^src-tauri/(uclaw-utils-[a-z-]+|uclaw-async-utils|uclaw-file-watcher|uclaw-file-search)/' \
    || true)
[ "${#STAGED[@]}" -eq 0 ] && exit 0

MISSING_SPDX=()
MISSING_ATTR=()
for f in "${STAGED[@]}"; do
    head_content=$(git show ":$f" | head -10)
    if ! echo "$head_content" | grep -q 'SPDX-License-Identifier:\s*Apache-2\.0'; then
        MISSING_SPDX+=("$f")
    fi
    if ! echo "$head_content" | grep -qE 'Derived from codex-rs/'; then
        MISSING_ATTR+=("$f")
    fi
done

FAIL=0
if [ "${#MISSING_SPDX[@]}" -ne 0 ]; then
    echo "" >&2
    echo "[check-codex-derived-spdx] BLOCKED — missing SPDX-License-Identifier header" >&2
    for v in "${MISSING_SPDX[@]}"; do echo "    - $v" >&2; done
    FAIL=1
fi
if [ "${#MISSING_ATTR[@]}" -ne 0 ]; then
    echo "" >&2
    echo "[check-codex-derived-spdx] BLOCKED — missing 'Derived from codex-rs/...' attribution" >&2
    for v in "${MISSING_ATTR[@]}"; do echo "    - $v" >&2; done
    FAIL=1
fi
if [ "$FAIL" -ne 0 ]; then
    echo "" >&2
    echo "  Required header template (top of every derived Rust file):" >&2
    echo "    // SPDX-License-Identifier: Apache-2.0" >&2
    echo "    // Derived from codex-rs/<path> (https://github.com/openai/codex)." >&2
    echo "    // Copyright (c) OpenAI. Licensed under Apache License 2.0." >&2
    echo "    // See NOTICE in the repository root." >&2
    echo "" >&2
    echo "  Reference: docs/THIRD_PARTY.md §3.2" >&2
    exit 1
fi
exit 0
