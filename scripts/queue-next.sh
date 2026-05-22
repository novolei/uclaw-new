#!/usr/bin/env bash
# queue-next.sh — show next item from a queue file + status summary
#
# Usage:
#   scripts/queue-next.sh                  # default: docs/superpowers/queue/C1-execution-queue.md
#   scripts/queue-next.sh C2               # → docs/superpowers/queue/C2-execution-queue.md
#   scripts/queue-next.sh --list           # list all known queues
#   scripts/queue-next.sh --done C1.1-PR-1 # mark a specific item as done (matches title prefix)
#   scripts/queue-next.sh --status         # short status of all queues
#
# Closed-loop reference:
#   docs/superpowers/queue/README.md  (queue conventions)
#   docs/superpowers/plans/2026-05-22-pr-integration-strategy.md  (closed-loop §5)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
QUEUE_DIR="$REPO_ROOT/docs/superpowers/queue"

# ── Colors ────────────────────────────────────────────────────────────
if [[ -t 1 ]]; then
  C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_YEL=$'\033[33m'
  C_BLUE=$'\033[34m'; C_BOLD=$'\033[1m'; C_RESET=$'\033[0m'
else
  C_RED=''; C_GREEN=''; C_YEL=''; C_BLUE=''; C_BOLD=''; C_RESET=''
fi

# ── Helpers ───────────────────────────────────────────────────────────
default_queue="C1"

count_total() {
  awk '/^### \[[ x]\]/' "$1" 2>/dev/null | wc -l | tr -d ' '
}
count_checked() {
  awk '/^### \[x\]/' "$1" 2>/dev/null | wc -l | tr -d ' '
}

list_queues() {
  echo "Known queues under $QUEUE_DIR:"
  find "$QUEUE_DIR" -maxdepth 1 -name '*-execution-queue.md' -type f \
    | sort \
    | while read -r f; do
        local stem
        stem=$(basename "$f" -execution-queue.md)
        local total checked
        total=$(count_total "$f")
        checked=$(count_checked "$f")
        printf "  %s: %d/%d done  →  %s\n" "$stem" "$checked" "$total" "$f"
      done
}

short_status() {
  for f in "$QUEUE_DIR"/*-execution-queue.md; do
    [[ -e "$f" ]] || continue
    local stem total checked
    stem=$(basename "$f" -execution-queue.md)
    total=$(count_total "$f")
    checked=$(count_checked "$f")
    if [[ "$total" -eq 0 ]]; then continue; fi
    if [[ "$checked" -eq "$total" ]]; then
      printf "  %s%s%s %s — %d/%d done\n" "$C_GREEN" "✓" "$C_RESET" "$stem" "$checked" "$total"
    elif [[ "$checked" -eq 0 ]]; then
      printf "  %s○%s %s — 0/%d done\n" "$C_YEL" "$C_RESET" "$stem" "$total"
    else
      printf "  %s◐%s %s — %d/%d done\n" "$C_BLUE" "$C_RESET" "$stem" "$checked" "$total"
    fi
  done
}

show_next() {
  local q="$1"
  local f="$QUEUE_DIR/$q-execution-queue.md"
  if [[ ! -f "$f" ]]; then
    printf "%sNo such queue:%s %s\n" "$C_RED" "$C_RESET" "$f"
    echo
    list_queues
    exit 1
  fi

  # Find first unchecked item heading + body, stop at next ### header
  local next_block
  next_block=$(awk '
    /^### \[/ && in_block { exit }
    /^### \[ \]/ { in_block = 1; print; next }
    in_block { print }
  ' "$f")

  local total checked
  total=$(count_total "$f")
  checked=$(count_checked "$f")

  printf "%s═══ Queue: %s ═══%s\n" "$C_BOLD" "$q" "$C_RESET"
  printf "Progress: %s%d/%d%s done\n\n" "$C_GREEN" "$checked" "$total" "$C_RESET"

  if [[ -z "$next_block" ]]; then
    printf "%s✓ Queue complete — all %d items done.%s\n" "$C_GREEN" "$total" "$C_RESET"
    printf "Next: archive queue + start next phase (see README §Post-queue).\n"
    return 0
  fi

  printf "%sNext item:%s\n\n" "$C_BOLD" "$C_RESET"
  printf "%s\n" "$next_block"
  echo
  printf "%s──── Pick up via:%s\n" "$C_BLUE" "$C_RESET"
  printf '   git checkout -b $(grep "^- \\*\\*Branch\\*\\*:" <<< "%s" | head -1 | awk -F\\` "{print \$2}")\n' "$next_block" \
    | sed 's/  *$//' \
    | head -1
  printf '   (or just tell the agent: "继续 %s 队列下一项")\n' "$q"
}

mark_done() {
  local target="$1"
  # Search all queue files for an item whose title starts with $target
  for f in "$QUEUE_DIR"/*-execution-queue.md; do
    [[ -e "$f" ]] || continue
    if grep -qE "^### \[ \] $target" "$f"; then
      # Use sed in-place (macOS variant)
      sed -i '' -E "s|^### \\[ \\] ($target.*)|### [x] \\1|" "$f"
      printf "%s✓ Marked done:%s %s in %s\n" "$C_GREEN" "$C_RESET" "$target" "$(basename "$f")"
      return 0
    fi
  done
  printf "%s✗ Couldn't find item starting with '%s' in any queue%s\n" "$C_RED" "$target" "$C_RESET"
  return 1
}

# ── Main ──────────────────────────────────────────────────────────────
case "${1:-}" in
  --list|-l)
    list_queues
    ;;
  --status|-s)
    short_status
    ;;
  --done)
    if [[ -z "${2:-}" ]]; then
      echo "Usage: $0 --done <item-prefix>" >&2
      exit 1
    fi
    mark_done "$2"
    ;;
  -h|--help)
    sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
    ;;
  '')
    show_next "$default_queue"
    ;;
  *)
    show_next "$1"
    ;;
esac
