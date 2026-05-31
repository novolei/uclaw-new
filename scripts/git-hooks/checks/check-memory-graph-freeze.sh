#!/usr/bin/env bash
# ADR §11.2: memory_graph is FROZEN. Block any staged change that adds a
# `memory_graph::write*` (or insert/update/delete) call, or a direct
# `.create_node` / `.create_entity_page` / `.create_edge` / `.create_version` /
# `.create_keyword` method call that bypasses the path-prefix check.
#
# Allowlist: src-tauri/src/memory_graph/mod.rs (the freeze panic guard itself)
#            src-tauri/src/memory_graph/legacy_migration/  (one-time migration)
#            The following files are reviewed exemptions — they write
#            memory_graph by design and have no MemoryAdapter equivalent yet
#            (deferred to gbrain↔openhuman effort; see gbrain-primary-freeze ADR):
#              src-tauri/src/proactive/tool_memory.rs
#              src-tauri/src/proactive/skill_parser.rs
#              src-tauri/src/proactive/memory_migration.rs
#              src-tauri/src/agent/tools/builtin/skill_search.rs
#              src-tauri/src/skills_manifest.rs
#
# Bypass: only via `git commit --no-verify` with an ADR override commit message.

set -euo pipefail

# Collect staged Rust files
STAGED=()
    while IFS= read -r __line; do [ -n "$__line" ] && STAGED+=("$__line"); done < <(git diff --cached --name-only --diff-filter=AM | grep -E '\.rs$' || true)
[ "${#STAGED[@]}" -eq 0 ] && exit 0

VIOLATIONS=()
for f in "${STAGED[@]}"; do
    # Allowlist
    case "$f" in
        src-tauri/src/memory_graph/mod.rs) continue ;;
        src-tauri/src/memory_graph/legacy_migration/*) continue ;;
        src-tauri/src/proactive/tool_memory.rs) continue ;;
        src-tauri/src/proactive/skill_parser.rs) continue ;;
        src-tauri/src/proactive/memory_migration.rs) continue ;;
        src-tauri/src/agent/tools/builtin/skill_search.rs) continue ;;
        src-tauri/src/skills_manifest.rs) continue ;;
    esac

    # Only check newly-added lines in the staged diff (+ lines)
    added=$(git diff --cached -U0 -- "$f" | grep -E '^\+' | grep -vE '^\+\+\+' || true)
    [ -z "$added" ] && continue

    if echo "$added" | grep -qE '\bmemory_graph\s*::\s*(write|insert|update|delete)[A-Za-z_]*\s*\('; then
        VIOLATIONS+=("$f")
    fi

    if echo "$added" | grep -qE '\.(create_node|create_entity_page|create_edge|create_version|create_keyword)\s*\('; then
        VIOLATIONS+=("$f")
    fi
done

if [ "${#VIOLATIONS[@]}" -ne 0 ]; then
    echo "" >&2
    echo "[check-memory-graph-freeze] BLOCKED — memory_graph is FROZEN (ADR §11.2)" >&2
    echo "" >&2
    echo "  New memory_graph::write* calls detected in:" >&2
    for v in "${VIOLATIONS[@]}"; do echo "    - $v" >&2; done
    echo "" >&2
    echo "  Use gbrain instead. See:" >&2
    echo "    - docs/adr/2026-05-20-uclaw-agent-platform-north-star.md §11.2" >&2
    echo "    - docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md" >&2
    exit 1
fi
exit 0
