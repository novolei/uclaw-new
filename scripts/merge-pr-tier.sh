#!/bin/bash
# Sequentially merge a tier of prep branches onto the current branch.
#
# Used by Phase 1 of the Agent OS v2 integration plan to land all 33
# foundation PRs onto integration/agent-os-v2-foundations.
#
# Usage:
#   scripts/merge-pr-tier.sh tier_name branch1 branch2 ...
#
# Relies on .gitattributes setting `merge=union` for the 3 module-
# registration files (agent/mod.rs, lib.rs, world/adapters/mod.rs).
# All other conflicts will fail loud — manual resolution required.

set -euo pipefail

TIER_NAME="${1:?missing tier name}"
shift
BRANCHES=("$@")

echo "═══════════════════════════════════════════════════════════════"
echo "  Merging Tier: ${TIER_NAME}"
echo "  Branches: ${#BRANCHES[@]}"
echo "═══════════════════════════════════════════════════════════════"

for branch in "${BRANCHES[@]}"; do
  echo ""
  echo "─── merging origin/${branch} ───"

  if ! git merge --no-edit --no-ff "origin/${branch}" 2>&1 | tail -10; then
    echo ""
    echo "✖ Merge of ${branch} failed."
    echo "Conflicted files:"
    git diff --name-only --diff-filter=U
    echo ""
    echo "Abort with: git merge --abort"
    exit 1
  fi

  # Sanity check: no leftover conflict markers in tree
  if git diff --check 2>&1 | grep -q "conflict marker"; then
    echo "✖ Conflict markers found after merge — manual fix required"
    exit 1
  fi

  echo "✓ merged ${branch}"
done

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  Tier ${TIER_NAME} merged successfully (${#BRANCHES[@]} branches)"
echo "═══════════════════════════════════════════════════════════════"
