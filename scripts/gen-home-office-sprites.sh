#!/usr/bin/env bash
# gen-home-office-sprites.sh — generate Lofi-girl character sprites for HomeOffice.
#
# Pipeline per direction/pose:
#   1. Compose Veo prompt with green-screen background
#   2. Call Vertex AI Veo via REST (gcloud ADC for auth)
#   3. ffmpeg chromakey green → alpha
#   4. ffmpeg crop to square (720x720)
#   5. img2webp pack to animated WebP @ 24fps
#
# Output:  ui/public/home-office/sprites/lofi-girl/<name>.webp
#
# Requirements: gcloud CLI auth'd, ffmpeg, img2webp (brew install webp)
set -euo pipefail

PROJECT="${GCP_PROJECT:-project-ec32b5be-e193-4a7a-9c5}"
REGION="us-central1"
MODEL="veo-3.0-fast-generate-001"
OUT_DIR="ui/public/home-office/sprites/lofi-girl"
TMP_DIR=".cache/home-office-sprites"
mkdir -p "$OUT_DIR" "$TMP_DIR"

ASSETS=(
  "walk-N|lofi girl walking away from camera (back view), serene focused expression, holding a small leather notebook, miyazaki ghibli watercolor style"
  "walk-NW|lofi girl walking diagonally away-and-left (3/4 back-left view), miyazaki ghibli watercolor style"
  "walk-W|lofi girl walking to the left (full side view, left profile), miyazaki ghibli watercolor style"
  "walk-SW|lofi girl walking diagonally toward-and-left (3/4 front-left view), miyazaki ghibli watercolor style"
  "walk-S|lofi girl walking toward camera (front view), miyazaki ghibli watercolor style"
  "pose-idle|lofi girl lying relaxed on a hammock with arms behind head, eyes half-closed, miyazaki ghibli watercolor style"
  "pose-thinking|lofi girl sitting cross-legged with an open book in lap, head tilted thoughtfully, miyazaki ghibli watercolor style"
  "pose-typing|lofi girl seated at a wooden writing desk, leaning forward writing in a journal with a quill, miyazaki ghibli watercolor style"
  "pose-success|lofi girl standing with both arms raised, joyful grin, miyazaki ghibli watercolor style"
  "pose-error|lofi girl seated hugging knees, head bowed, sad expression, miyazaki ghibli watercolor style"
)

BG="solid uniform vivid pure green chromakey background (#00FF00), no other elements"
STYLE="character is fully visible, head to feet, centered, no horizon line, 8 second loop, 720p"

for entry in "${ASSETS[@]}"; do
  NAME="${entry%%|*}"
  PROMPT="${entry#*|}, $BG, $STYLE"
  echo "==> $NAME"

  MP4="$TMP_DIR/$NAME.mp4"
  ALPHA_DIR="$TMP_DIR/$NAME-frames"
  WEBP_OUT="$OUT_DIR/$NAME.webp"

  if [ -f "$WEBP_OUT" ]; then
    echo "    skip (exists)"
    continue
  fi

  if [ ! -f "$MP4" ]; then
    TOKEN=$(gcloud auth print-access-token)
    RESP=$(curl -sS -X POST \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/json" \
      "https://us-central1-aiplatform.googleapis.com/v1/projects/$PROJECT/locations/$REGION/publishers/google/models/$MODEL:predictLongRunning" \
      -d "{\"instances\":[{\"prompt\":\"$PROMPT\"}],\"parameters\":{\"aspectRatio\":\"16:9\",\"durationSeconds\":8}}")
    OP_NAME=$(echo "$RESP" | python3 -c "import json,sys; d=json.load(sys.stdin); n=d.get('name'); e=d.get('error',{}).get('message') if d.get('error') else None;
import sys as _s
if n: print(n)
else: _s.stderr.write(f'API error: {e or d}\n'); _s.exit(1)")
    if [ -z "$OP_NAME" ]; then echo "    failed to start op for $NAME"; exit 1; fi
    # Poll until done
    while true; do
      sleep 8
      OP=$(curl -sS -H "Authorization: Bearer $(gcloud auth print-access-token)" \
        "https://us-central1-aiplatform.googleapis.com/v1/$OP_NAME")
      DONE=$(echo "$OP" | python3 -c "import json,sys;d=json.load(sys.stdin);print(d.get('done',False))")
      if [ "$DONE" = "True" ]; then
        URI=$(echo "$OP" | python3 -c "import json,sys;d=json.load(sys.stdin);print(d['response']['videos'][0]['gcsUri'])")
        gsutil cp "$URI" "$MP4"
        break
      fi
    done
  fi

  mkdir -p "$ALPHA_DIR"
  # chromakey + crop to 720x720 (centered)
  ffmpeg -y -i "$MP4" -vf \
    "chromakey=0x00FF00:0.20:0.08,crop=720:720:(in_w-720)/2:(in_h-720)/2,format=yuva420p" \
    "$ALPHA_DIR/frame-%03d.png" 2>&1 | tail -2

  # Pack to animated WebP
  img2webp -loop 0 -lossy -q 75 -m 2 -d 42 -mixed \
    $(ls "$ALPHA_DIR"/frame-*.png | sort) -o "$WEBP_OUT"

  ls -la "$WEBP_OUT"
done

echo "==> Done. Generated $(ls "$OUT_DIR"/*.webp 2>/dev/null | wc -l)/10 sprites."
