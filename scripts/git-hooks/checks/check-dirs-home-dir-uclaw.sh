#!/usr/bin/env bash
# Block new `dirs::home_dir().unwrap().join(".uclaw")` (and variants).
# Use `uclaw_utils_home::uclaw_home()` instead (added in Phase 0.5-T3).
#
# Allowlist: existing call sites that the Phase 0.5-T6 sweep will clean up.

set -euo pipefail

STAGED=()
    while IFS= read -r __line; do [ -n "$__line" ] && STAGED+=("$__line"); done < <(git diff --cached --name-only --diff-filter=AM | grep -E '\.rs$' || true)
[ "${#STAGED[@]}" -eq 0 ] && exit 0

VIOLATIONS=()
for f in "${STAGED[@]}"; do
    # Allowlist (Phase 0.5-T6 sweep targets — pre-existing call sites)
    case "$f" in
        src-tauri/src/tauri_commands.rs) continue ;;
        src-tauri/src/memubot_config.rs) continue ;;
        src-tauri/uclaw-utils-home/*) continue ;;       # the crate that defines uclaw_home()
        src-tauri/uclaw-utils-abs-path/*) continue ;;
    esac

    added=$(git diff --cached -U0 -- "$f" | grep -E '^\+' | grep -vE '^\+\+\+' || true)
    [ -z "$added" ] && continue

    # Detect `dirs::home_dir()` followed by `.join("...uclaw...")` (with optional .unwrap())
    if echo "$added" | grep -qE 'dirs\s*::\s*home_dir\s*\(\s*\)' \
       && echo "$added" | grep -qE '\.join\s*\(\s*"[^"]*\.uclaw[^"]*"'; then
        VIOLATIONS+=("$f")
    fi
done

if [ "${#VIOLATIONS[@]}" -ne 0 ]; then
    echo "" >&2
    echo "[check-dirs-home-dir-uclaw] BLOCKED — use uclaw_utils_home instead" >&2
    echo "" >&2
    echo "  New 'dirs::home_dir().*\".uclaw\"' patterns in:" >&2
    for v in "${VIOLATIONS[@]}"; do echo "    - $v" >&2; done
    echo "" >&2
    echo "  Use uclaw_utils_home::uclaw_home() (or uclaw_*_dir() helpers)." >&2
    echo "  See Phase 0.5-T6 sweep plan in uclaw-upgrade-implementation-plan.md." >&2
    exit 1
fi
exit 0
