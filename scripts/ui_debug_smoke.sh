#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UI_DIR="$ROOT_DIR/ui"
TAURI_DIR="$ROOT_DIR/src-tauri"
TAURI_DEBUG_BIN="$TAURI_DIR/target/debug/uclaw"
STAMP="$(date +"%Y%m%d-%H%M%S")"
LOG_DIR="${UCLAW_UI_DEBUG_LOG_DIR:-/tmp/uclaw-ui-debug-$STAMP}"
VITE_HOST="${UCLAW_UI_DEBUG_HOST:-127.0.0.1}"
VITE_PORT="${UCLAW_UI_DEBUG_PORT:-5173}"
VITE_URL="http://${VITE_HOST}:${VITE_PORT}/"
TAURI_DEV_CONFIG="$LOG_DIR/tauri-dev-config.json"
DEBUG_PRODUCT_NAME="${UCLAW_UI_DEBUG_PRODUCT_NAME:-uClaw Debug $VITE_PORT}"
DEBUG_IDENTIFIER_SUFFIX="$(echo "${VITE_PORT}-${STAMP}" | tr -cd '[:alnum:]')"
DEBUG_IDENTIFIER="${UCLAW_UI_DEBUG_IDENTIFIER:-ai.uclaw.desktop.debug.$DEBUG_IDENTIFIER_SUFFIX}"
DEBUG_WINDOW_TITLE="${UCLAW_UI_DEBUG_WINDOW_TITLE:-uClaw Debug $VITE_PORT}"
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
    | rg "node $UI_DIR/node_modules/.bin/vite --host $VITE_HOST --port $VITE_PORT|target/debug/uclaw|$TAURI_DEBUG_BIN" \
    || true
}

assert_no_existing_uclaw_app() {
  if [[ "${UCLAW_UI_DEBUG_ALLOW_EXISTING:-0}" == "1" ]]; then
    echo "[ui-debug] existing uClaw app check bypassed by UCLAW_UI_DEBUG_ALLOW_EXISTING=1"
    return
  fi

  local matches
  matches="$(
    ps -axo pid,ppid,command \
      | rg 'target/debug/uclaw|uClaw\.app/Contents/MacOS' \
      | rg -v 'rg target/debug/uclaw|ps -axo' \
      || true
  )"

  if [[ -n "$matches" ]]; then
    echo "[ui-debug] refusing to launch: existing uClaw app/debug process detected"
    echo "$matches"
    echo "[ui-debug] Quit existing uClaw windows first, then rerun this script."
    echo "[ui-debug] Set UCLAW_UI_DEBUG_ALLOW_EXISTING=1 only if you intentionally want ambiguous Computer Use results."
    exit 1
  fi
}

mkdir -p "$LOG_DIR"
cat >"$TAURI_DEV_CONFIG" <<JSON
{
  "productName": "$DEBUG_PRODUCT_NAME",
  "identifier": "$DEBUG_IDENTIFIER",
  "build": {
    "beforeDevCommand": null,
    "devUrl": "$VITE_URL"
  },
  "app": {
    "windows": [
      {
        "title": "$DEBUG_WINDOW_TITLE",
        "width": 1280,
        "height": 820,
        "resizable": true,
        "fullscreen": false,
        "dragDropEnabled": true,
        "hiddenTitle": true,
        "titleBarStyle": "Overlay",
        "trafficLightPosition": {
          "x": 16,
          "y": 26
        }
      }
    ]
  }
}
JSON

print_header "preflight"
echo "[ui-debug] root=$ROOT_DIR"
echo "[ui-debug] ui=$UI_DIR"
echo "[ui-debug] tauri=$TAURI_DIR"
echo "[ui-debug] expected_debug_binary=$TAURI_DEBUG_BIN"
echo "[ui-debug] debug_product_name=$DEBUG_PRODUCT_NAME"
echo "[ui-debug] debug_identifier=$DEBUG_IDENTIFIER"
echo "[ui-debug] debug_window_title=$DEBUG_WINDOW_TITLE"
echo "[ui-debug] vite_url=$VITE_URL"
echo "[ui-debug] log_dir=$LOG_DIR"
git -C "$ROOT_DIR" status --short
assert_no_existing_uclaw_app

print_header "start vite"
(
  cd "$UI_DIR"
  npm run dev -- --host "$VITE_HOST" --port "$VITE_PORT" --strictPort
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
  cargo tauri dev --config "$TAURI_DEV_CONFIG"
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
echo "[ui-debug] Use Computer Use get_app_state('$DEBUG_PRODUCT_NAME') now."
echo "[ui-debug] Expected debug binary: $TAURI_DEBUG_BIN"
echo "[ui-debug] Expected app identity: $DEBUG_PRODUCT_NAME ($DEBUG_IDENTIFIER)"
echo "[ui-debug] Expected window title: $DEBUG_WINDOW_TITLE"
echo "[ui-debug] Expected Vite URL: $VITE_URL"
echo "[ui-debug] If using Playwright, inspect $VITE_URL."

if [[ "${UCLAW_UI_DEBUG_KEEP_ALIVE:-0}" == "1" ]]; then
  echo "[ui-debug] keep-alive enabled. Press Ctrl-C to stop."
  wait
else
  echo "[ui-debug] smoke launched and verified process truth; exiting will clean spawned processes."
fi
