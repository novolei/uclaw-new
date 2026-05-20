#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UI_DIR="$ROOT_DIR/ui"
TAURI_DIR="$ROOT_DIR/src-tauri"
STAMP="$(date +"%Y%m%d-%H%M%S")"
LOG_DIR="${UCLAW_UI_DEBUG_LOG_DIR:-/tmp/uclaw-ui-debug-$STAMP}"
VITE_HOST="${UCLAW_UI_DEBUG_HOST:-127.0.0.1}"
VITE_PORT="${UCLAW_UI_DEBUG_PORT:-5173}"
VITE_URL="http://${VITE_HOST}:${VITE_PORT}/"
PIDS=()

cleanup() {
  local code=$?
  if [[ "${UCLAW_UI_DEBUG_KEEP_ALIVE:-0}" != "1" ]]; then
    for pid in "${PIDS[@]:-}"; do
      if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
      fi
    done
  fi
  echo "[ui-debug] log_dir=$LOG_DIR"
  exit "$code"
}
trap cleanup EXIT INT TERM

print_header() {
  echo
  echo "== $1 =="
}

print_process_truth() {
  print_header "process truth"
  ps -axo pid,ppid,command \
    | rg "$ROOT_DIR|vite --host $VITE_HOST|target/debug/uclaw" \
    || true
}

mkdir -p "$LOG_DIR"

print_header "preflight"
echo "[ui-debug] root=$ROOT_DIR"
echo "[ui-debug] ui=$UI_DIR"
echo "[ui-debug] tauri=$TAURI_DIR"
echo "[ui-debug] vite_url=$VITE_URL"
echo "[ui-debug] log_dir=$LOG_DIR"
git -C "$ROOT_DIR" status --short

print_header "start vite"
(
  cd "$UI_DIR"
  npm run dev -- --host "$VITE_HOST" --port "$VITE_PORT"
) >"$LOG_DIR/vite.log" 2>&1 &
PIDS+=("$!")

for _ in {1..80}; do
  if curl -sS --max-time 1 "$VITE_URL" >/dev/null 2>&1; then
    echo "[ui-debug] vite ready: $VITE_URL"
    break
  fi
  sleep 0.25
done

if ! curl -sS --max-time 2 "$VITE_URL" >/dev/null 2>&1; then
  echo "[ui-debug] vite did not become ready"
  tail -80 "$LOG_DIR/vite.log" || true
  exit 1
fi

print_header "start tauri"
(
  cd "$TAURI_DIR"
  cargo tauri dev
) >"$LOG_DIR/tauri.log" 2>&1 &
PIDS+=("$!")

for _ in {1..240}; do
  if rg -q "uClaw started successfully|Running .*target/debug/uclaw" "$LOG_DIR/tauri.log" 2>/dev/null; then
    echo "[ui-debug] tauri debug app launched"
    break
  fi
  sleep 0.5
done

if ! rg -q "target/debug/uclaw|uClaw started successfully" "$LOG_DIR/tauri.log" 2>/dev/null; then
  echo "[ui-debug] tauri did not report a debug launch"
  tail -120 "$LOG_DIR/tauri.log" || true
  exit 1
fi

print_process_truth

print_header "manual verification"
echo "[ui-debug] Use Computer Use get_app_state('uClaw') now."
echo "[ui-debug] Expected debug binary: $ROOT_DIR/target/debug/uclaw"
echo "[ui-debug] Expected Vite URL: $VITE_URL"
echo "[ui-debug] If using Playwright, inspect $VITE_URL."

if [[ "${UCLAW_UI_DEBUG_KEEP_ALIVE:-0}" == "1" ]]; then
  echo "[ui-debug] keep-alive enabled. Press Ctrl-C to stop."
  wait
else
  echo "[ui-debug] smoke launched and verified process truth; exiting will clean spawned processes."
fi
