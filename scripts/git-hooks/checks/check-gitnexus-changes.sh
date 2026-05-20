#!/usr/bin/env bash
# Run `gitnexus detect-changes --scope staged` and warn if HIGH or CRITICAL risk.
# Per uclaw CLAUDE.md "GitNexus — Code Intelligence" discipline: MUST run before
# committing. We don't block (gitnexus output is advisory), but we make it loud.
#
# Soft-skip if gitnexus CLI is not installed.
# POSIX/bash3-compatible (no mapfile).

set -euo pipefail

if ! command -v gitnexus >/dev/null 2>&1; then
    exit 0
fi

STAGED=()
while IFS= read -r f; do
    [ -n "$f" ] && STAGED+=("$f")
done < <(git diff --cached --name-only --diff-filter=AM \
    | grep -E '\.(rs|ts|tsx|js|jsx|py)$' || true)
[ "${#STAGED[@]}" -eq 0 ] && exit 0

out=$(gitnexus detect-changes --scope staged 2>&1 || true)

risk=$(echo "$out" | grep -iE 'risk[_ ]?level' | head -1 | sed -E 's/.*[: ]+//' | tr -d '"' | tr '[:upper:]' '[:lower:]' || true)

case "$risk" in
    critical|high)
        echo "" >&2
        echo "[check-gitnexus-changes] WARNING — risk level: $risk" >&2
        echo "" >&2
        echo "$out" | sed 's/^/    /' >&2
        echo "" >&2
        echo "  Review the affected processes above. If intentional, proceed." >&2
        echo "  To force the commit anyway: this check is advisory, not blocking." >&2
        ;;
    "")
        echo "[check-gitnexus-changes] gitnexus detect-changes produced no risk_level (index may be stale; run 'gitnexus analyze')" >&2
        ;;
    *)
        ;;
esac

exit 0
