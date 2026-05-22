#!/usr/bin/env bash
# Bundle 17-B — set fold_delta_threshold in memubot_config.json
# Usage: ./17b-threshold-set.sh <integer 1..50>
# Restart effect: the dispatcher reads cfg.context.fold_delta_threshold afresh on
# each /compact, so changes take effect on the NEXT /compact without restart.
# (Tauri command set_fold_delta_threshold writes the same field; this script is
#  an alternative entry point useful when no UI exposes the knob yet.)

set -euo pipefail
N="${1:?usage: $0 <integer 1..50>}"
if ! [[ "$N" =~ ^[0-9]+$ ]] || (( N < 1 )) || (( N > 50 )); then
  echo "fold_delta_threshold must be integer in [1, 50]" >&2; exit 1
fi

CFG="$HOME/.uclaw/memubot_config.json"
[[ -f "$CFG" ]] || echo "{}" > "$CFG"

python3 - "$CFG" "$N" <<'PY'
import json, sys, pathlib
path = pathlib.Path(sys.argv[1])
n = int(sys.argv[2])
cfg = json.loads(path.read_text() or "{}")
cfg.setdefault("context", {})["fold_delta_threshold"] = n
path.write_text(json.dumps(cfg, indent=2))
print(f"fold_delta_threshold -> {n} (persisted to {path})")
PY
