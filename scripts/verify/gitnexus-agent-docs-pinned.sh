#!/usr/bin/env bash
# Ensure GitNexus managed blocks are pinned so routine analyze runs do not
# rewrite AGENTS.md / CLAUDE.md with volatile graph counts.
set -euo pipefail

if [ "$#" -eq 0 ]; then
    set -- AGENTS.md CLAUDE.md
fi

for file in "$@"; do
    if [ ! -f "$file" ]; then
        echo "[gitnexus-agent-docs-pinned] missing file: $file" >&2
        exit 1
    fi

    awk -v file="$file" '
        /<!-- gitnexus:start -->/ {
            in_block = 1
            found_start = 1
            found_keep = 0
        }
        in_block && /<!-- gitnexus:keep -->/ {
            found_keep = 1
        }
        /<!-- gitnexus:end -->/ {
            if (in_block && !found_keep) {
                printf("[gitnexus-agent-docs-pinned] %s GitNexus block is missing <!-- gitnexus:keep -->\n", file) > "/dev/stderr"
                exit 2
            }
            in_block = 0
            found_end = 1
        }
        END {
            if (!found_start || !found_end) {
                printf("[gitnexus-agent-docs-pinned] %s is missing GitNexus start/end markers\n", file) > "/dev/stderr"
                exit 3
            }
        }
    ' "$file"
done
