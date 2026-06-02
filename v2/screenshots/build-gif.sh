#!/bin/bash
# Stitch the captured frames in screenshots/frames/ into gallery.gif.
# Two-pass palette (palettegen → paletteuse) for clean color on a dark UI.
# Run after capture.mjs (or just `npm run screenshots`, which does both).

set -euo pipefail
cd "$(dirname "$0")"

FRAMES_DIR="frames"
OUT="gallery.gif"
WIDTH=1280        # downscale from the 2560px retina captures
SECONDS_PER=2.2   # hold each screen this long

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg not found — install it (brew install ffmpeg) to build the GIF." >&2
  exit 1
fi

frames=( "$FRAMES_DIR"/*.png )
if [[ ${#frames[@]} -eq 0 ]]; then
  echo "No frames in $FRAMES_DIR — run capture.mjs first." >&2
  exit 1
fi

# concat demuxer playlist: each frame held SECONDS_PER. The concat demuxer
# ignores the final entry's duration, so the last frame is listed twice.
list="$(mktemp)"
trap 'rm -f "$list" palette.png' EXIT
for f in "${frames[@]}"; do
  printf "file '%s'\nduration %s\n" "$PWD/$f" "$SECONDS_PER" >> "$list"
done
last="${frames[$((${#frames[@]} - 1))]}"  # bash 3.2 (macOS) has no negative indices
printf "file '%s'\n" "$PWD/$last" >> "$list"

echo "Building palette…"
ffmpeg -y -f concat -safe 0 -i "$list" \
  -vf "scale=${WIDTH}:-1:flags=lanczos,palettegen=stats_mode=full" \
  palette.png >/dev/null 2>&1

echo "Encoding ${OUT}…"
ffmpeg -y -f concat -safe 0 -i "$list" -i palette.png \
  -lavfi "scale=${WIDTH}:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3" \
  -loop 0 "${OUT}" >/dev/null 2>&1

echo "✓ $(pwd)/${OUT} ($(du -h "${OUT}" | cut -f1))"
