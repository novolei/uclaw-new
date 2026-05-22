#!/usr/bin/env bash
# Milestone Drift Detector
#
# Closes the loop on uclaw-upgrade-implementation-plan.md M1-M9 progress
# tracking. Reads merged PRs in a time window, classifies each by milestone,
# computes tactical-vs-strategic ratio, raises alarm if tactical > 40%.
#
# Triggered:
#   - Manually anytime: scripts/milestone-drift-check.sh
#   - Weekly cron (recommended): every Monday 9am
#   - After every batch merge >= 5 PRs
#
# Usage:
#   scripts/milestone-drift-check.sh                    # default 7 days
#   scripts/milestone-drift-check.sh --since "30 days ago"
#   scripts/milestone-drift-check.sh --since 2026-05-15 --until 2026-05-22
#   scripts/milestone-drift-check.sh --quiet            # only alarms
#   scripts/milestone-drift-check.sh --update-status    # append to MILESTONE_STATUS.md drift log
#
# Reference: docs/superpowers/plans/2026-05-22-pr-integration-strategy.md §5

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SINCE="1 week ago"
UNTIL=""
QUIET=false
UPDATE_STATUS=false

# ── Args ──────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --since) SINCE="$2"; shift 2 ;;
    --until) UNTIL="$2"; shift 2 ;;
    --quiet) QUIET=true; shift ;;
    --update-status) UPDATE_STATUS=true; shift ;;
    -h|--help)
      sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

# ── Output helpers ────────────────────────────────────────────────────
if [[ -t 1 ]]; then
  C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_YEL=$'\033[33m'
  C_BLUE=$'\033[34m'; C_BOLD=$'\033[1m'; C_RESET=$'\033[0m'
else
  C_RED=''; C_GREEN=''; C_YEL=''; C_BLUE=''; C_BOLD=''; C_RESET=''
fi

# ── Resolve time window ───────────────────────────────────────────────
SINCE_ISO=$(date -j -v"-7d" "+%Y-%m-%d" 2>/dev/null \
  || date -d "$SINCE" "+%Y-%m-%d" 2>/dev/null \
  || date -j -f "%Y-%m-%d" "$SINCE" "+%Y-%m-%d" 2>/dev/null \
  || echo "$SINCE")
if [[ "$SINCE" != "1 week ago" ]]; then
  # User passed a specific date or relative — try to parse
  SINCE_ISO=$(date -d "$SINCE" "+%Y-%m-%d" 2>/dev/null || \
              date -j -f "%Y-%m-%d" "$SINCE" "+%Y-%m-%d" 2>/dev/null || \
              echo "$SINCE")
fi
UNTIL_ISO=${UNTIL:-$(date "+%Y-%m-%d")}

# ── Pull merged PRs in window via gh ──────────────────────────────────
$QUIET || printf '%s[drift]%s pulling PRs from %s to %s ...\n' \
  "$C_BLUE" "$C_RESET" "$SINCE_ISO" "$UNTIL_ISO"

# gh pr list --search supports `merged:YYYY-MM-DD..YYYY-MM-DD` range
PRS_JSON=$(gh pr list \
  --state merged \
  --search "merged:${SINCE_ISO}..${UNTIL_ISO}" \
  --json number,title,mergedAt,headRefName \
  --limit 200 \
  2>/dev/null)

PR_COUNT=$(echo "$PRS_JSON" | jq 'length')
if [[ "$PR_COUNT" == "0" ]]; then
  $QUIET || printf '%s[drift]%s no merged PRs in window\n' "$C_BLUE" "$C_RESET"
  exit 0
fi

# ── Classify each PR ──────────────────────────────────────────────────
# Rules (priority order):
#   1. Branch matches prep/m[0-9]-t[0-9]   → M_Foundation
#   2. Branch matches prep/m[0-9][a-z]?    → M_Foundation
#   3. Title contains [M*-T* wire-up] or [Slice N-X] → M_Wireup
#   4. Title contains "pilot" or "skeleton" or "types" with M-tag → M_Pilot
#   5. Title contains "Bundle" with digit  → Tactical
#   6. Branch matches phase-0-5 / phase_0_5 / Phase 0.5 → Phase 0.5
#   7. Otherwise                            → Backlog

classify_pr() {
  local title="$1"
  local branch="$2"

  # 1+2: M_Foundation by branch name
  if [[ "$branch" =~ ^prep/m[0-9]+(-?t[0-9]+|[a-z]) ]]; then
    echo "M_Foundation"
    return
  fi

  # Slice / wire-up
  if [[ "$title" =~ \[Slice|wire-up|wireup|Wire-up ]]; then
    echo "M_Wireup"
    return
  fi
  # Branch convention
  if [[ "$branch" =~ ^prep/slice-|wireup ]]; then
    echo "M_Wireup"
    return
  fi

  # Pilot
  if [[ "$title" =~ pilot|skeleton|\[M[0-9]+-T[0-9] ]]; then
    echo "M_Pilot"
    return
  fi

  # Bundle = tactical
  if [[ "$title" =~ Bundle\ [0-9]+ ]]; then
    echo "Tactical"
    return
  fi

  # Phase 0.5
  if [[ "$title" =~ Phase\ 0\.5 ]] || [[ "$branch" =~ phase-?0[-_.]5 ]]; then
    echo "Phase_05"
    return
  fi

  # M_Foundation by title tag like "[M1-T2c]"
  if [[ "$title" =~ \[M[0-9]+-T[0-9] ]]; then
    echo "M_Foundation"
    return
  fi

  echo "Backlog"
}

# macOS ships bash 3.2 which lacks associative arrays — use flat vars.
N_M_Foundation=0; PRS_M_Foundation=""
N_M_Wireup=0;     PRS_M_Wireup=""
N_M_Pilot=0;      PRS_M_Pilot=""
N_Tactical=0;     PRS_Tactical=""
N_Phase_05=0;     PRS_Phase_05=""
N_Backlog=0;      PRS_Backlog=""

while IFS=$'\t' read -r num title branch; do
  cat=$(classify_pr "$title" "$branch")
  case "$cat" in
    M_Foundation) N_M_Foundation=$((N_M_Foundation + 1)); PRS_M_Foundation="$PRS_M_Foundation #$num" ;;
    M_Wireup)     N_M_Wireup=$((N_M_Wireup + 1));         PRS_M_Wireup="$PRS_M_Wireup #$num" ;;
    M_Pilot)      N_M_Pilot=$((N_M_Pilot + 1));           PRS_M_Pilot="$PRS_M_Pilot #$num" ;;
    Tactical)     N_Tactical=$((N_Tactical + 1));         PRS_Tactical="$PRS_Tactical #$num" ;;
    Phase_05)     N_Phase_05=$((N_Phase_05 + 1));         PRS_Phase_05="$PRS_Phase_05 #$num" ;;
    Backlog)      N_Backlog=$((N_Backlog + 1));           PRS_Backlog="$PRS_Backlog #$num" ;;
  esac
done < <(echo "$PRS_JSON" | jq -r '.[] | "\(.number)\t\(.title)\t\(.headRefName)"')

# ── Compute ratios ────────────────────────────────────────────────────
TACTICAL=$N_Tactical
STRATEGIC=$(( N_M_Foundation + N_M_Wireup + N_M_Pilot + N_Phase_05 ))
BACKLOG=$N_Backlog
TOTAL=$PR_COUNT
TACTICAL_PCT=0
if [[ $TOTAL -gt 0 ]]; then
  TACTICAL_PCT=$(( TACTICAL * 100 / TOTAL ))
fi

# ── Alarm thresholds ──────────────────────────────────────────────────
ALARM_LEVEL="green"
ALARM_MSG=""
if [[ $TACTICAL_PCT -gt 40 ]]; then
  ALARM_LEVEL="red"
  ALARM_MSG="tactical ratio ${TACTICAL_PCT}% > 40% threshold — milestone work is stalling"
elif [[ $TACTICAL_PCT -gt 30 ]]; then
  ALARM_LEVEL="yellow"
  ALARM_MSG="tactical ratio ${TACTICAL_PCT}% in warning band (30-40%) — note for next planning"
fi

# Consecutive Bundle check (separate alarm)
BUNDLE_RUN=0
LAST_WAS_BUNDLE=false
while IFS=$'\t' read -r num title branch; do
  if [[ "$title" =~ Bundle\ [0-9]+ ]]; then
    if $LAST_WAS_BUNDLE; then
      BUNDLE_RUN=$((BUNDLE_RUN + 1))
    else
      BUNDLE_RUN=1
      LAST_WAS_BUNDLE=true
    fi
  else
    LAST_WAS_BUNDLE=false
  fi
done < <(echo "$PRS_JSON" | jq -r 'sort_by(.mergedAt) | .[] | "\(.number)\t\(.title)\t\(.headRefName)"')
# Note: actually compute max consecutive run
BUNDLE_MAX_RUN=$(echo "$PRS_JSON" | jq -r 'sort_by(.mergedAt) | .[] | .title' \
  | awk '
    /Bundle [0-9]+/ { run++; if (run > max) max = run; next }
    { run = 0 }
    END { print max + 0 }
  ')
if [[ ${BUNDLE_MAX_RUN:-0} -gt 7 ]]; then
  ALARM_LEVEL="red"
  ALARM_MSG="${ALARM_MSG}; consecutive Bundle run = ${BUNDLE_MAX_RUN} > 7 threshold"
fi

# ── Output ────────────────────────────────────────────────────────────
print_header() {
  echo "${C_BOLD}==================== Drift Check ${UNTIL_ISO} ===================="
  printf "${C_RESET}"
}
print_table() {
  echo "Window:       ${SINCE_ISO} → ${UNTIL_ISO}"
  echo "PRs merged:   ${TOTAL}"
  echo ""
  echo "By bucket:"
  printf "  %-14s %3d  %s\n" "M-Foundation" "$N_M_Foundation" "$PRS_M_Foundation"
  printf "  %-14s %3d  %s\n" "M-Wireup"     "$N_M_Wireup"     "$PRS_M_Wireup"
  printf "  %-14s %3d  %s\n" "M-Pilot"      "$N_M_Pilot"      "$PRS_M_Pilot"
  printf "  %-14s %3d  %s\n" "Phase 0.5"    "$N_Phase_05"     "$PRS_Phase_05"
  printf "  %-14s %3d  %s\n" "Tactical"     "$N_Tactical"     "$PRS_Tactical"
  printf "  %-14s %3d  %s\n" "Backlog"      "$N_Backlog"      "$PRS_Backlog"
  echo ""
  printf "Tactical ratio:        %d/%d = %d%%\n" "$TACTICAL" "$TOTAL" "$TACTICAL_PCT"
  printf "Strategic + Phase0.5:  %d/%d = %d%%\n" "$STRATEGIC" "$TOTAL" "$(( STRATEGIC * 100 / TOTAL ))"
  printf "Max consecutive Bundle: %d\n" "${BUNDLE_MAX_RUN:-0}"
}
print_alarm() {
  case "$ALARM_LEVEL" in
    green)
      printf "%sStatus: GREEN%s — within healthy bounds\n" "$C_GREEN" "$C_RESET"
      ;;
    yellow)
      printf "%sStatus: YELLOW%s — %s\n" "$C_YEL" "$C_RESET" "$ALARM_MSG"
      ;;
    red)
      printf "%sStatus: RED ALARM%s — %s\n" "$C_RED" "$C_RESET" "$ALARM_MSG"
      printf "Recommendation: hold tactical work, finish in-flight milestone slice first.\n"
      printf "Refer: docs/superpowers/plans/2026-05-22-pr-integration-strategy.md §5.3\n"
      ;;
  esac
}

if ! $QUIET; then
  print_header
  print_table
  echo ""
fi
print_alarm

# ── Optional: append to MILESTONE_STATUS.md drift log ────────────────
if $UPDATE_STATUS && [[ "$ALARM_LEVEL" != "green" ]]; then
  status_file="$REPO_ROOT/docs/superpowers/MILESTONE_STATUS.md"
  if [[ -f "$status_file" ]]; then
    # Insert one line under "## Drift log" header
    tmp=$(mktemp)
    awk -v line="- ${UNTIL_ISO} [${ALARM_LEVEL^^}] tactical ${TACTICAL_PCT}% (${TACTICAL}/${TOTAL}); consecutive Bundle: ${BUNDLE_MAX_RUN:-0}" '
      /^## Drift log/ { print; getline; print; print line; next }
      { print }
    ' "$status_file" > "$tmp"
    mv "$tmp" "$status_file"
    $QUIET || printf '%s[drift]%s appended to MILESTONE_STATUS.md drift log\n' "$C_BLUE" "$C_RESET"
  fi
fi

# ── Exit code (for CI / scheduled task chaining) ─────────────────────
case "$ALARM_LEVEL" in
  green)  exit 0 ;;
  yellow) exit 1 ;;
  red)    exit 2 ;;
esac
