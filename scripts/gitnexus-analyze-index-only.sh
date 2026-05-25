#!/usr/bin/env bash
# Refresh the local GitNexus index without rewriting AGENTS.md, CLAUDE.md, or
# repo-local GitNexus skill files.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

exec npx gitnexus analyze --index-only "$@"
