#!/usr/bin/env bash
# Install uclaw's tracked git hooks by pointing core.hooksPath at scripts/git-hooks/.
# One-shot operation; safe to re-run; reversible.
#
# Run from the repo root after a fresh clone:
#     ./scripts/install-git-hooks.sh
#
# To uninstall:
#     git config --unset core.hooksPath
# (Falls back to .git/hooks/ default again.)

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
HOOKS_DIR="scripts/git-hooks"

cd "$REPO_ROOT"

if [ ! -d "$HOOKS_DIR" ]; then
    echo "[install-git-hooks] Expected $HOOKS_DIR/ to exist; aborting." >&2
    exit 1
fi

# Make sure every hook is executable (file mode can be lost in some checkouts).
chmod +x "$HOOKS_DIR"/* 2>/dev/null || true
chmod +x "$HOOKS_DIR"/checks/*.sh 2>/dev/null || true

# Detect any non-tracked custom hook the user may have placed under .git/hooks/
# (e.g. a pre-existing post-merge before this change landed). Warn but don't
# delete — let the user decide.
HOOKS_GIT_DIR="$(git rev-parse --git-path hooks)"
CUSTOM_FOUND=()
if [ -d "$HOOKS_GIT_DIR" ]; then
    while IFS= read -r f; do
        name="$(basename "$f")"
        case "$name" in *.sample) continue ;; esac
        CUSTOM_FOUND+=("$f")
    done < <(find "$HOOKS_GIT_DIR" -maxdepth 1 -type f 2>/dev/null)
fi
if [ "${#CUSTOM_FOUND[@]}" -ne 0 ]; then
    echo "[install-git-hooks] Detected custom hooks already in $HOOKS_GIT_DIR/:"
    for f in "${CUSTOM_FOUND[@]}"; do echo "    - $f"; done
    echo "  These will be IGNORED once core.hooksPath is set to $HOOKS_DIR/."
    echo "  If you want their logic preserved, copy them into $HOOKS_DIR/ first."
    echo "  Continuing in 3 seconds (Ctrl-C to abort)..."
    sleep 3
fi

git config core.hooksPath "$HOOKS_DIR"
echo "[install-git-hooks] core.hooksPath set to: $HOOKS_DIR"
echo ""
echo "Installed hooks:"
ls -1 "$HOOKS_DIR" | grep -vE '^(checks|README\.md)$' | sed 's/^/    /'
echo ""
echo "Pre-commit checks:"
ls -1 "$HOOKS_DIR/checks" | sed 's/^/    /'
echo ""
echo "To bypass in an emergency: git commit --no-verify"
echo "To uninstall:              git config --unset core.hooksPath"
