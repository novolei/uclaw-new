#!/usr/bin/env bash
# Check shell syntax for tracked repository scripts that CI and hooks execute.
set -euo pipefail

scripts=(
    scripts/ci/diff-check.sh
    scripts/ci/pr-review-digest.sh
    scripts/ci/shell-syntax-check.sh
    scripts/gitnexus-analyze-index-only.sh
    scripts/verify/gitnexus-agent-docs-pinned.sh
    scripts/git-hooks/pre-commit
    scripts/git-hooks/post-merge
)

while IFS= read -r check; do
    scripts+=("$check")
done < <(find scripts/git-hooks/checks -maxdepth 1 -type f -name 'check-*.sh' | sort)

for script in "${scripts[@]}"; do
    bash -n "$script"
done
