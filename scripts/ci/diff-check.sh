#!/usr/bin/env bash
# Run git diff --check against the pull request base or push before-SHA.
set -euo pipefail

base="${BASE_SHA:-}"

if [ -z "$base" ] && [ "${GITHUB_EVENT_NAME:-}" = "pull_request" ]; then
    base="$(jq -r '.pull_request.base.sha' "$GITHUB_EVENT_PATH")"
fi

if [ -z "$base" ] && [ -n "${GITHUB_EVENT_PATH:-}" ] && [ -f "$GITHUB_EVENT_PATH" ]; then
    base="$(jq -r '.before // empty' "$GITHUB_EVENT_PATH")"
fi

if [ -z "$base" ] || ! git cat-file -e "$base^{commit}" 2>/dev/null; then
    base="HEAD~1"
fi

git diff --check "$base" "${GITHUB_SHA:-HEAD}"
