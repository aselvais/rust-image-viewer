# Regenerate the app icon artwork in every platform format from the master SVG.
#
#   assets/AppIcon.svg      master artwork (the source of truth)
#   assets/AppIcon.png      1024x1024, embedded at compile time as the OS
#                           window/taskbar icon (src/main.rs -> include_bytes!)
#   assets/AppIcon.icns     macOS .app bundle icon (scripts/build-macos.sh)
#   assets/AppIcon.ico      Windows .exe resource icon
#
# Run:  bash scripts/make-icon.sh
set -euo pipefail
cd "$(dirname "$0")/.."

SRC=assets/AppIcon.svg
OUT=assets
ICONSET="$(mktemp -d)/AppIcon.iconset"

[ -f "$SRC" ] || { echo "missing $SRC" >&2; exit 1; }
mkdir -p "$OUT"
rm -rf "$ICONSET"; mkdir -p "$ICONSET"

# Master render (1024x1024, transparent background).
if which rsvg-convert >/dev/null 2>&1; then
  rsvg-convert -w 1024 -h 1024 "$SRC" -o "$OUT/AppIcon.png"
else
  magick -background none "$SRC" -resize 1024x1024 "$OUT/AppIcon.png"
fi

# Build the iconset PNGs from the master (sips resamples, square, keeps AR).
gen() { sips -z "$1" "$1" "$OUT/AppIcon.png" --out "$2" >/dev/null 2>&1; }
gen 16   "$ICONSET/icon_16x16.png"
gen 32   "$ICONSET/icon_16x16@2x.png"
gen 32   "$ICONSET/icon_32x32.png"
gen 64   "$ICONSET/icon_32x32@2x.png"
gen 128  "$ICONSET/icon_128x128.png"
gen 256  "$ICONSET/icon_128x128@2x.png"
gen 256  "$ICONSET/icon_256x256.png"
gen 512  "$ICONSET/icon_256x256@2x.png"
gen 512  "$ICONSET/icon_512x512.png"
gen 1024 "$ICONSET/icon_512x512@2x.png"

# macOS: .icns from the iconset.
if which iconutil >/dev/null 2>&1; then
  iconutil -c icns "$ICONSET" -o "$OUT/AppIcon.icns"
fi

# Windows: multi-size .ico (best supported converter).
if which magick >/dev/null 2>&1; then
  magick "$OUT/AppIcon.png" -define icon:auto-resize=16,24,32,48,64,128,256 "$OUT/AppIcon.ico" 2>/dev/null || true
elif which convert >/dev/null 2>&1; then
  convert "$OUT/AppIcon.png" -define icon:auto-resize=16,24,32,48,64,128,256 "$OUT/AppIcon.ico" 2>/dev/null || true
fi

rm -rf "$(dirname "$ICONSET")"
echo "Wrote: $OUT/AppIcon.png $OUT/AppIcon.icns $OUT/AppIcon.ico"
