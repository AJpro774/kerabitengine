#!/usr/bin/env bash
# Record the Kerabit showcase trailer (real engine frames) and encode site media.
#
# Prefers GPU headless capture when a wgpu adapter is available:
#   KERABIT_SHOWCASE_RECORD=1 cargo run -p showcase --release
# Falls back to scripts/soft_showcase_trailer.py if Metal/Vulkan is unavailable.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FRAMES="${KERABIT_CAPTURE_DIR:-$ROOT/target/showcase-frames}"
MEDIA="$ROOT/site/media"
FFMPEG="${FFMPEG:-$ROOT/tools/bin/ffmpeg}"
if [[ ! -x "$FFMPEG" ]]; then
  FFMPEG="$(command -v ffmpeg || true)"
fi
if [[ -z "$FFMPEG" ]]; then
  echo "ffmpeg not found; install it or place a binary at tools/bin/ffmpeg" >&2
  exit 1
fi

rm -rf "$FRAMES"
mkdir -p "$FRAMES" "$MEDIA"

echo "==> capturing frames → $FRAMES"
if KERABIT_SHOWCASE_RECORD=1 KERABIT_CAPTURE_DIR="$FRAMES" \
  cargo run -p showcase --release --manifest-path "$ROOT/Cargo.toml"; then
  :
else
  echo "GPU capture failed; using CPU soft renderer of the same scene layout" >&2
  python3 "$ROOT/scripts/soft_showcase_trailer.py"
fi

COUNT="$(find "$FRAMES" -name 'frame_*.png' | wc -l | tr -d ' ')"
if [[ "$COUNT" -lt 10 ]]; then
  echo "expected frame_*.png under $FRAMES, found $COUNT" >&2
  exit 1
fi

# Infer fps from count (~20s loop).
FPS=$(( COUNT / 20 ))
if [[ "$FPS" -lt 15 ]]; then FPS=20; fi

echo "==> encoding WebM + MP4 ($COUNT frames @ ${FPS}fps)"
"$FFMPEG" -y -framerate "$FPS" -i "$FRAMES/frame_%05d.png" \
  -vf "scale=1280:720:flags=lanczos,format=yuv420p" \
  -c:v libvpx-vp9 -b:v 0 -crf 36 -row-mt 1 -an \
  "$MEDIA/showcase-loop.webm"
"$FFMPEG" -y -framerate "$FPS" -i "$FRAMES/frame_%05d.png" \
  -vf "scale=1280:720:flags=lanczos,format=yuv420p" \
  -c:v libx264 -preset slow -crf 28 -movflags +faststart -an \
  "$MEDIA/showcase-loop.mp4"

ls -lh "$MEDIA/showcase-loop.webm" "$MEDIA/showcase-loop.mp4"
echo "done — rebuild/deploy site/ to publish"
