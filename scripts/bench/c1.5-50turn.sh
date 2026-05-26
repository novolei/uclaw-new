#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# C1.5 — 50-turn refactor benchmark orchestration.
#
# Stands up a pre-Dirac baseline build (git worktree @ f6447a71), forward-ports
# the bench artifacts into it, builds both binaries, replays both golden
# sequences on each, optionally runs live mode (when API keys are present), and
# renders an HONEST report (no fabricated deltas — see render_report.py).
#
# Run from the post-Dirac worktree (this repo).  Replay always works; live runs
# only when DEEPSEEK_API_KEY / ANTHROPIC_API_KEY are set.
set -euo pipefail

POST_DIR="$(git rev-parse --show-toplevel)"
BASE_SHA="f6447a71"                       # last commit before A1 (#494); pre-Dirac
BASE_DIR="$(dirname "$POST_DIR")/uclaw-bench-baseline"
REPORT="$POST_DIR/docs/superpowers/reports/2026-05-25-M2-benchmark.md"
mkdir -p "$(dirname "$REPORT")"

cleanup() {
  if git -C "$POST_DIR" worktree list --porcelain 2>/dev/null | grep -q "$BASE_DIR"; then
    git -C "$POST_DIR" worktree remove --force "$BASE_DIR" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# ── 1. baseline worktree @ pre-Dirac ───────────────────────────────────
git -C "$POST_DIR" worktree add --force "$BASE_DIR" "$BASE_SHA"

# ── 2. forward-port bench artifacts into the baseline ───────────────────
# (they postdate f6447a71, so they don't exist in the baseline tree).
mkdir -p "$BASE_DIR/src-tauri/src/bin" "$BASE_DIR/src-tauri/src/agent/bench"
cp "$POST_DIR/src-tauri/src/bin/bench_50turn.rs"     "$BASE_DIR/src-tauri/src/bin/"
cp "$POST_DIR/src-tauri/src/agent/bench/mod.rs"      "$BASE_DIR/src-tauri/src/agent/bench/"
cp "$POST_DIR/src-tauri/src/agent/bench/live.rs"     "$BASE_DIR/src-tauri/src/agent/bench/"
cp -R "$POST_DIR/src-tauri/tests/fixtures/c1.5-bench" "$BASE_DIR/src-tauri/tests/fixtures/"

# Copy gitignored build resources (pyembed/bunembed/gbrain-source) so the
# baseline Tauri build script passes — symlink to the canonical copies.
for res in pyembed bunembed gbrain-source; do
  if [ ! -e "$BASE_DIR/src-tauri/$res" ] && [ -e "$POST_DIR/src-tauri/$res" ]; then
    ln -s "$(readlink "$POST_DIR/src-tauri/$res" 2>/dev/null || echo "$POST_DIR/src-tauri/$res")" \
       "$BASE_DIR/src-tauri/$res" 2>/dev/null || true
  fi
done

# Apply the same Cargo.toml [[bin]]/feature + agent/mod.rs gate to the baseline.
python3 - "$BASE_DIR" <<'PY'
import sys, re, pathlib
base = pathlib.Path(sys.argv[1])
cargo = base / "src-tauri" / "Cargo.toml"
txt = cargo.read_text()
if "bench_50turn" not in txt:
    # add [[bin]] after the existing uclaw bin block
    txt = txt.replace(
        'name = "uclaw"\npath = "src/main.rs"\n',
        'name = "uclaw"\npath = "src/main.rs"\n\n[[bin]]\nname = "bench_50turn"\npath = "src/bin/bench_50turn.rs"\nrequired-features = ["bench"]\n',
        1,
    )
if "\nbench = []" not in txt:
    if "[features]" in txt:
        txt = txt.replace("[features]\n", "[features]\nbench = []\n", 1)
    else:
        txt += '\n[features]\nbench = []\n'
cargo.write_text(txt)

modrs = base / "src-tauri" / "src" / "agent" / "mod.rs"
m = modrs.read_text()
if "pub mod bench;" not in m:
    m += '\n#[cfg(feature = "bench")]\npub mod bench;\n'
    modrs.write_text(m)
print("baseline Cargo.toml + agent/mod.rs patched")
PY

# ── 3. build both — ABORT (no fabricated delta) if the baseline won't build ──
BASELINE_OK=1
if ! ( cd "$BASE_DIR/src-tauri" && cargo build --features bench --bin bench_50turn 2>build.log ); then
  echo "WARN: baseline build failed — pre-Dirac interfaces diverge from the bench." >&2
  echo "      See $BASE_DIR/src-tauri/build.log. Report will contain same-build replay" >&2
  echo "      only; NO fabricated cross-build delta (spec §3.5)." >&2
  BASELINE_OK=0
fi
( cd "$POST_DIR/src-tauri" && cargo build --features bench --bin bench_50turn )

# ── 4. replay both builds (both golden sequences) ───────────────────────
run() { ( cd "$1/src-tauri" && cargo run -q --features bench --bin bench_50turn -- \
            --fixture refactor-8-file --mode replay --golden "$2" --out "$3" ); }
run "$POST_DIR" post "/tmp/post-post.json"
run "$POST_DIR" pre  "/tmp/post-pre.json"
BASE_ARGS=()
if [ "$BASELINE_OK" = 1 ]; then
  run "$BASE_DIR" post "/tmp/base-post.json"
  run "$BASE_DIR" pre  "/tmp/base-pre.json"
  BASE_ARGS=(--base-post /tmp/base-post.json --base-pre /tmp/base-pre.json)
fi

# ── 5. optional live (only if keys present) — POST build ────────────────
LIVE=()
if [ -n "${DEEPSEEK_API_KEY:-}" ]; then
  ( cd "$POST_DIR/src-tauri" && cargo run -q --features bench --bin bench_50turn -- \
      --mode live --provider deepseek --runs 3 --out /tmp/live-ds.json ) && LIVE+=(/tmp/live-ds.json)
fi
if [ -n "${ANTHROPIC_API_KEY:-}" ]; then
  ( cd "$POST_DIR/src-tauri" && cargo run -q --features bench --bin bench_50turn -- \
      --mode live --provider anthropic --runs 3 --out /tmp/live-an.json ) && LIVE+=(/tmp/live-an.json)
fi

# ── 6. render honest report ─────────────────────────────────────────────
python3 "$POST_DIR/scripts/bench/render_report.py" \
  --report "$REPORT" --baseline-ok "$BASELINE_OK" \
  --post-post /tmp/post-post.json --post-pre /tmp/post-pre.json \
  "${BASE_ARGS[@]}" "${LIVE[@]}"

echo "report → $REPORT"
