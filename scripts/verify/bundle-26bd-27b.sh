#!/usr/bin/env bash
# Bundle 26-B / 26-D / 27-B E2E verification.
#
# Runs the full loop:
#   1. cuts a local verify branch from current HEAD
#   2. lowers the tick modulos in proactive/service.rs so prune fires
#      ~2min instead of ~2h and promote fires ~1min instead of ~30min
#   3. backs up ~/.uclaw/memubot_config.json + writes aggressive
#      thresholds (stream_idle=8s, prune_days=1, promote_count=1)
#   4. seeds 3 fixture skill directories covering all 3 judgment paths
#   5. builds + starts the app in background, tails log
#   6. asserts that 26-B / 26-D fired as expected
#   7. cleanup: stops app, restores config, removes fixtures + archive,
#      checks out service.rs, deletes verify branch
#
# Usage:
#   scripts/verify/bundle-26bd-27b.sh           # dry-run, prints plan
#   scripts/verify/bundle-26bd-27b.sh --apply   # actually runs
#   scripts/verify/bundle-26bd-27b.sh --apply --keep-on-fail
#                                               # leave fixtures intact
#                                               # for debugging on fail
#
# Notes:
#   - Bundle 27-B (LLM stream idle timeout) is hard to automate
#     deterministically — it needs network manipulation against a real
#     provider. Verify it manually via the settings page after this
#     script restores defaults: drop the timeout to 5s, send an Agent
#     task that pauses, watch for `[Bundle 27-B] LLM stream idle`.
#   - Script depends on python3 (preinstalled on macOS) for JSON edits.
#     No jq dependency.

set -euo pipefail

# ── Layout ────────────────────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SKILLS_DIR="${HOME}/.uclaw/skills/_auto_extracted"
ARCHIVE_DIR="${HOME}/.uclaw/skills/_archive"  # sibling of _auto_extracted/, NOT nested
CONFIG_PATH="${HOME}/.uclaw/memubot_config.json"
CONFIG_BACKUP="${HOME}/.uclaw/memubot_config.json.verify-26bd-27b-backup"
SERVICE_FILE="${REPO_ROOT}/src-tauri/src/proactive/service.rs"
LOG_FILE="/tmp/uclaw-verify-26bd-27b.log"
APP_PID_FILE="/tmp/uclaw-verify-26bd-27b.pid"
VERIFY_BRANCH="verify/bundle-26bd-27b-LOCAL"

APPLY=false
KEEP_ON_FAIL=false
ORIGINAL_BRANCH=""
DID_BRANCH_SWITCH=false
DID_CONFIG_PATCH=false
DID_SEED=false
APP_STARTED=false

# ── Output helpers ────────────────────────────────────────────────────
if [[ -t 1 ]]; then
  C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_YEL=$'\033[33m'
  C_BLUE=$'\033[34m'; C_RESET=$'\033[0m'
else
  C_RED=''; C_GREEN=''; C_YEL=''; C_BLUE=''; C_RESET=''
fi
log()  { printf '%s[verify]%s %s\n' "$C_BLUE" "$C_RESET" "$*"; }
ok()   { printf '  %s✓%s %s\n' "$C_GREEN" "$C_RESET" "$*"; }
fail() { printf '  %s✗%s %s\n' "$C_RED" "$C_RESET" "$*"; }
warn() { printf '  %s⚠%s %s\n' "$C_YEL" "$C_RESET" "$*"; }

# ── Arg parsing ───────────────────────────────────────────────────────
for arg in "$@"; do
  case "$arg" in
    --apply) APPLY=true ;;
    --dry-run) APPLY=false ;;
    --keep-on-fail) KEEP_ON_FAIL=true ;;
    -h|--help)
      sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      fail "unknown arg: $arg"
      echo "Usage: $0 [--apply] [--keep-on-fail]"
      exit 1
      ;;
  esac
done

# ── Cleanup trap ──────────────────────────────────────────────────────
cleanup() {
  local rc=$?
  echo
  log "cleanup (rc=$rc)..."

  if $KEEP_ON_FAIL && [[ $rc -ne 0 ]]; then
    warn "rc=$rc and --keep-on-fail set — leaving artifacts in place:"
    warn "  config:    $CONFIG_PATH (backup at $CONFIG_BACKUP)"
    warn "  fixtures:  $SKILLS_DIR/verify-*"
    warn "  branch:    $VERIFY_BRANCH (service.rs modified)"
    warn "  app log:   $LOG_FILE"
    warn "  app pid:   $(cat "$APP_PID_FILE" 2>/dev/null || echo none)"
    warn "When done, manually run:"
    warn "  kill \$(cat $APP_PID_FILE) ; mv $CONFIG_BACKUP $CONFIG_PATH ;"
    warn "  rm -rf $SKILLS_DIR/verify-* $ARCHIVE_DIR/*/verify-* ;"
    warn "  cd $REPO_ROOT && git checkout -- $SERVICE_FILE &&"
    warn "  git checkout $ORIGINAL_BRANCH && git branch -D $VERIFY_BRANCH"
    exit $rc
  fi

  if $APP_STARTED && [[ -f "$APP_PID_FILE" ]]; then
    local pid
    pid=$(cat "$APP_PID_FILE")
    if kill -0 "$pid" 2>/dev/null; then
      log "  stopping app pid=$pid"
      kill -INT "$pid" 2>/dev/null || true
      for _ in 1 2 3 4 5; do
        kill -0 "$pid" 2>/dev/null || break
        sleep 1
      done
      kill -KILL "$pid" 2>/dev/null || true
    fi
    rm -f "$APP_PID_FILE"
  fi

  if $DID_CONFIG_PATCH && [[ -f "$CONFIG_BACKUP" ]]; then
    mv "$CONFIG_BACKUP" "$CONFIG_PATH" && ok "restored $CONFIG_PATH"
  fi

  if $DID_SEED; then
    for slug in verify-stale-cold verify-hot verify-already-promoted; do
      rm -rf "${SKILLS_DIR:?}/$slug" 2>/dev/null || true
    done
    # Remove archived verify-* dirs first, then any TS-dir that
    # became empty as a result (don't touch TS dirs that still
    # contain non-verify entries — those belong to real prior runs).
    find "$ARCHIVE_DIR" -mindepth 2 -maxdepth 2 -type d \
        -name 'verify-*' 2>/dev/null \
      | xargs -I{} rm -rf "{}" 2>/dev/null || true
    find "$ARCHIVE_DIR" -mindepth 1 -maxdepth 1 -type d -empty \
        -name '????????-????' 2>/dev/null \
      | xargs -I{} rmdir "{}" 2>/dev/null || true
    ok "removed seeded fixtures + their archives"
  fi

  if $DID_BRANCH_SWITCH && [[ -n "$ORIGINAL_BRANCH" ]]; then
    cd "$REPO_ROOT"
    git checkout -- "$SERVICE_FILE" 2>/dev/null || true
    if git rev-parse --verify "$VERIFY_BRANCH" >/dev/null 2>&1; then
      git checkout "$ORIGINAL_BRANCH" 2>/dev/null || true
      git branch -D "$VERIFY_BRANCH" 2>/dev/null || true
      ok "restored $ORIGINAL_BRANCH, deleted $VERIFY_BRANCH"
    fi
  fi

  if [[ "${DRY_RUN_EXIT:-false}" == "true" && $rc -eq 0 ]]; then
    : # dry-run; nothing to declare
  elif [[ $rc -eq 0 ]]; then
    log "${C_GREEN}✓ verification PASS${C_RESET}"
  else
    log "${C_RED}✗ verification FAIL (rc=$rc)${C_RESET}"
    log "  app log preserved at: $LOG_FILE"
  fi
  exit $rc
}
trap cleanup EXIT INT TERM

# ── Pre-flight ────────────────────────────────────────────────────────
log "pre-flight checks..."

for tool in python3 cargo git; do
  command -v "$tool" >/dev/null || { fail "missing $tool"; exit 1; }
done
ok "deps present: python3 cargo git"

cd "$REPO_ROOT"
ORIGINAL_BRANCH=$(git rev-parse --abbrev-ref HEAD)
# Allow untracked files (?? lines) since the script itself may be
# untracked. Reject any actual modifications (M / A / D / R / C / U).
if git status --porcelain | grep -vE '^\?\?' | grep -q .; then
  fail "working tree has uncommitted modifications — commit or stash first:"
  git status --porcelain | grep -vE '^\?\?' >&2
  exit 1
fi
ok "working tree clean (modulo untracked), on $ORIGINAL_BRANCH"

if ! $APPLY; then
  log "${C_YEL}DRY RUN${C_RESET} — would perform 6 steps then cleanup:"
  echo
  cat <<PLAN
  [1/6] git checkout -b $VERIFY_BRANCH
        sed % 240 → % 4   (prune tick: ~2h → ~2min)
        sed % 60  → % 2   (promote tick: ~30min → ~1min)

  [2/6] cp $CONFIG_PATH $CONFIG_BACKUP
        patch JSON: stream_idle_timeout_secs=8,
                    memory_os.skill_prune_min_unused_days=1,
                    memory_os.skill_promote_min_returned_count=1

  [3/6] seed $SKILLS_DIR/{verify-stale-cold,verify-hot,verify-already-promoted}/
            each with SKILL.md + meta.json
            (stale-cold: returned_count=0, created_at=2d-ago — expect ARCHIVE)
            (hot:        returned_count=1, success_count=1 — expect PROMOTE)
            (promoted:   returned_count=10, promoted_at=now — expect SKIP)

  [4/6] cargo build && cargo run >$LOG_FILE 2>&1 &
        (Tauri window will open. Closing it manually = abort.)

  [5/6] tail $LOG_FILE for up to 180s waiting for both:
            "[Bundle 26-B] skill prune pass complete"
            "[Bundle 26-D] Promoted skill verify-hot"

  [6/6] filesystem assertions:
            _archive/<TS>/verify-stale-cold/ exists
            verify-stale-cold no longer in active dir
            verify-hot/meta.json has non-null promoted_at
            verify-already-promoted/meta.json unchanged

  cleanup: kill app, restore config + branch + service.rs, remove fixtures.
PLAN
  echo
  log "rerun with --apply to actually run."
  # Use a special exit code so the trap's "PASS" / "FAIL" footer is
  # suppressed — dry-run shouldn't claim verification succeeded.
  DRY_RUN_EXIT=true
  exit 0
fi
DRY_RUN_EXIT=${DRY_RUN_EXIT:-false}

# ── Step 1: verify branch + lower modulos ─────────────────────────────
log "[1/6] creating verify branch + lowering tick modulos..."
git checkout -b "$VERIFY_BRANCH" 2>&1 | grep -v '^Switched' || true
DID_BRANCH_SWITCH=true

python3 - "$SERVICE_FILE" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
s = p.read_text()
orig = s
s = s.replace(
    "tick_count.load(Ordering::SeqCst) % 240 == 0",
    "tick_count.load(Ordering::SeqCst) % 4 == 0   /* verify-lowered: was 240 */"
)
s = s.replace(
    "tick_count.load(Ordering::SeqCst) % 60 == 0",
    "tick_count.load(Ordering::SeqCst) % 2 == 0   /* verify-lowered: was 60 */"
)
if s == orig:
    print("ERROR: could not find tick modulo lines to patch — file shape changed?", file=sys.stderr)
    sys.exit(2)
p.write_text(s)
PY
ok "lowered tick modulos in service.rs"

# ── Step 2: backup config + write aggressive thresholds ───────────────
log "[2/6] patching memubot_config.json with aggressive thresholds..."
mkdir -p "$(dirname "$CONFIG_PATH")"

if [[ -f "$CONFIG_PATH" ]]; then
  cp "$CONFIG_PATH" "$CONFIG_BACKUP"
else
  # Mark with an empty backup so cleanup knows there was no original
  echo '{}' > "$CONFIG_BACKUP"
fi
DID_CONFIG_PATCH=true

python3 - "$CONFIG_PATH" "$CONFIG_BACKUP" <<'PY'
import sys, json, pathlib
out_path = pathlib.Path(sys.argv[1])
backup = pathlib.Path(sys.argv[2])
cfg = {}
if backup.exists():
    try:
        cfg = json.loads(backup.read_text() or '{}')
    except json.JSONDecodeError:
        cfg = {}
cfg["stream_idle_timeout_secs"] = 8
cfg.setdefault("memory_os", {})
cfg["memory_os"]["skill_prune_min_unused_days"] = 1
cfg["memory_os"]["skill_promote_min_returned_count"] = 1
out_path.write_text(json.dumps(cfg, indent=2) + "\n")
PY
ok "stream_idle=8s, prune_days=1, promote_count=1 written"

# ── Step 3: seed fixtures ─────────────────────────────────────────────
log "[3/6] seeding test skills..."
mkdir -p "$SKILLS_DIR"
DID_SEED=true

NOW_MS=$(($(date +%s) * 1000))
OLD_MS=$((NOW_MS - 2 * 86400 * 1000))   # 2 days ago

# A: stale + cold → should be ARCHIVED by 26-B
mkdir -p "$SKILLS_DIR/verify-stale-cold"
cat > "$SKILLS_DIR/verify-stale-cold/SKILL.md" <<'MD'
---
name: verify-stale-cold
description: Test fixture for Bundle 26-B prune verification.
---
This skill is seeded with returned_count=0 and created_at=2 days ago.
The 26-B prune pass should archive it on the next tick when
min_unused_days <= 2.
MD
python3 -c "
import json
json.dump({
    'slug': 'verify-stale-cold',
    'created_at': $OLD_MS,
    'updated_at': $OLD_MS,
    'returned_count': 0,
    'last_returned_at': None,
    'success_count': 0,
    'failure_count': 0,
    'last_used_at': $OLD_MS,
    'schema_version': 1,
    'promoted_at': None,
}, open('$SKILLS_DIR/verify-stale-cold/meta.json', 'w'), indent=2)
"

# B: hot + unpromoted → should be PROMOTED by 26-D
mkdir -p "$SKILLS_DIR/verify-hot"
cat > "$SKILLS_DIR/verify-hot/SKILL.md" <<'MD'
---
name: verify-hot
description: Test fixture for Bundle 26-D promotion verification.
---
returned_count=1 ≥ threshold of 1, success_count=1, promoted_at=null.
The 26-D promotion pass should push this into gene_candidate_pool
with source="skill_promotion" and stamp promoted_at.
MD
python3 -c "
import json
json.dump({
    'slug': 'verify-hot',
    'created_at': $NOW_MS,
    'updated_at': $NOW_MS,
    'returned_count': 1,
    'last_returned_at': $NOW_MS,
    'success_count': 1,
    'failure_count': 0,
    'last_used_at': $NOW_MS,
    'schema_version': 1,
    'promoted_at': None,
}, open('$SKILLS_DIR/verify-hot/meta.json', 'w'), indent=2)
"

# C: already promoted → control, should be SKIPPED
mkdir -p "$SKILLS_DIR/verify-already-promoted"
cat > "$SKILLS_DIR/verify-already-promoted/SKILL.md" <<'MD'
---
name: verify-already-promoted
description: Control fixture — promoted_at already set, should be skipped.
---
MD
python3 -c "
import json
json.dump({
    'slug': 'verify-already-promoted',
    'created_at': $NOW_MS,
    'updated_at': $NOW_MS,
    'returned_count': 10,
    'last_returned_at': $NOW_MS,
    'success_count': 5,
    'failure_count': 0,
    'last_used_at': $NOW_MS,
    'schema_version': 1,
    'promoted_at': $NOW_MS,
}, open('$SKILLS_DIR/verify-already-promoted/meta.json', 'w'), indent=2)
"
ok "3 fixtures seeded"

# ── Step 4: build + run app ───────────────────────────────────────────
log "[4/6] cargo build + cargo run (Tauri window will open)..."
cd "$REPO_ROOT/src-tauri"
if ! cargo build 2>&1 | grep -E "^error" >/dev/null; then
  ok "cargo build green"
else
  fail "cargo build failed"
  cargo build 2>&1 | grep -E "^error" | head
  exit 1
fi

cargo run >"$LOG_FILE" 2>&1 &
APP_PID=$!
echo "$APP_PID" > "$APP_PID_FILE"
APP_STARTED=true
ok "app started, pid=$APP_PID"
log "  tailing $LOG_FILE for [Bundle 26-B] + [Bundle 26-D] triggers..."

# ── Step 5: watch logs ────────────────────────────────────────────────
log "[5/6] waiting for triggers (up to 180s)..."

WAIT_SECS=180
START=$(date +%s)
SAW_PRUNE=false
SAW_PROMOTE=false

while true; do
  if ! kill -0 "$APP_PID" 2>/dev/null; then
    fail "app died unexpectedly — see $LOG_FILE"
    tail -30 "$LOG_FILE"
    exit 3
  fi
  if grep -qF "[Bundle 26-B] skill prune pass complete" "$LOG_FILE" 2>/dev/null; then
    SAW_PRUNE=true
  fi
  if grep -qF "[Bundle 26-D]" "$LOG_FILE" 2>/dev/null; then
    SAW_PROMOTE=true
  fi
  if $SAW_PRUNE && $SAW_PROMOTE; then
    break
  fi
  local_elapsed=$(( $(date +%s) - START ))
  if [[ $local_elapsed -ge $WAIT_SECS ]]; then
    fail "timeout — saw_prune=$SAW_PRUNE saw_promote=$SAW_PROMOTE after ${WAIT_SECS}s"
    echo
    log "tail $LOG_FILE:"
    tail -50 "$LOG_FILE"
    break
  fi
  sleep 5
done

$SAW_PRUNE   && ok "saw [Bundle 26-B] prune trigger"   || fail "no 26-B trigger"
$SAW_PROMOTE && ok "saw [Bundle 26-D] promote trigger" || fail "no 26-D trigger"

# ── Step 6: filesystem assertions ─────────────────────────────────────
log "[6/6] checking filesystem assertions..."

ASSERT_ARCHIVED=false
if find "$ARCHIVE_DIR" -mindepth 2 -maxdepth 2 -type d -name 'verify-stale-cold' 2>/dev/null | grep -q .; then
  ASSERT_ARCHIVED=true
  ok "verify-stale-cold found under _archive/"
else
  fail "verify-stale-cold NOT in _archive/"
fi

ASSERT_STALE_GONE=false
if [[ ! -d "$SKILLS_DIR/verify-stale-cold" ]]; then
  ASSERT_STALE_GONE=true
  ok "verify-stale-cold removed from active dir"
else
  fail "verify-stale-cold still in active dir (should have been moved)"
fi

ASSERT_PROMOTED=false
PROMOTED_AT=$(python3 -c "
import json
try:
    m = json.load(open('$SKILLS_DIR/verify-hot/meta.json'))
    print(m.get('promoted_at') or 'null')
except Exception as e:
    print(f'ERR: {e}')
")
case "$PROMOTED_AT" in
  null|ERR*|'')
    fail "verify-hot.meta.json promoted_at not set (got: $PROMOTED_AT)"
    ;;
  *)
    ASSERT_PROMOTED=true
    ok "verify-hot.meta.json promoted_at=$PROMOTED_AT"
    ;;
esac

ASSERT_CONTROL=true
CONTROL_PROMOTED=$(python3 -c "
import json
m = json.load(open('$SKILLS_DIR/verify-already-promoted/meta.json'))
print(m.get('promoted_at'))
")
if [[ "$CONTROL_PROMOTED" == "$NOW_MS" ]]; then
  ok "verify-already-promoted untouched (promoted_at stable at $NOW_MS)"
else
  warn "verify-already-promoted.promoted_at changed: was $NOW_MS, now $CONTROL_PROMOTED"
  warn "(not necessarily a bug — only a problem if it was re-promoted)"
fi

# ── Verdict ───────────────────────────────────────────────────────────
echo
PASS=true
$SAW_PRUNE       || PASS=false
$SAW_PROMOTE     || PASS=false
$ASSERT_ARCHIVED || PASS=false
$ASSERT_STALE_GONE || PASS=false
$ASSERT_PROMOTED || PASS=false

if $PASS; then
  log "${C_GREEN}all assertions green${C_RESET}"
  exit 0
else
  log "${C_RED}one or more assertions failed${C_RESET}"
  log "  app log: $LOG_FILE"
  exit 2
fi
