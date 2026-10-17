# Build a self-contained, universal macOS .app + zip of rust-image-viewer.
# Usage:
#   scripts/build-macos.sh            # optimized universal (arm64 + x86_64) .app
#   scripts/build-macos.sh x86_64     # Intel-only
#   scripts/build-macos.sh aarch64    # Apple-Silicon-only
#
# Output lands in ./dist/ :
#   dist/rust-image-viewer.app
#   dist/rust-image-viewer-macos-<arch>.app.zip
set -euo pipefail

VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/^version *= *"([^"]+)".*/\1/')"
BIN="rust-image-viewer"
APP_NAME="Rust Image Viewer"

# Architectures to build. Default is a universal binary.
ARCHS="${1:-universal}"
case "$ARCHS" in
  universal) BUILDS=("aarch64-apple-darwin" "x86_64-apple-darwin"); OUT_ARCH="universal" ;;
  aarch64)   BUILDS=("aarch64-apple-darwin"); OUT_ARCH="aarch64" ;;
  x86_64)    BUILDS=("x86_64-apple-darwin");  OUT_ARCH="x86_64" ;;
  *) echo "unknown arch: $ARCHS (use universal|aarch64|x86_64)" >&2; exit 1 ;;
esac

DIST="dist"
mkdir -p "$DIST"

# Make sure each target is installed.
for t in "${BUILDS[@]}"; do rustup target add "$t"; done

BUILD_PATHS=()
for t in "${BUILDS[@]}"; do
  echo ">> cargo build --release --target $t"
  cargo build --release --target "$t"
  BUILD_PATHS+=("target/$t/release/$BIN")
done

APP="$DIST/$BIN.app"
rm -rf "$APP" "$DIST/$BIN" "$DIST"/*.zip
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

# Assemble the Mach-O binary (single arch or universal via lipo).
if [ "${#BUILD_PATHS[@]}" -eq 1 ]; then
  cp "${BUILD_PATHS[0]}" "$APP/Contents/MacOS/$BIN"
else
  lipo -create "${BUILD_PATHS[@]}" -output "$APP/Contents/MacOS/$BIN"
fi
chmod +x "$APP/Contents/MacOS/$BIN"

# A minimal, self-describing Info.plist. Bundle a real Icon.icns into
# Contents/Resources to give the app a proper icon in Finder/Dock.
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>CFBundleName</key>              <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>       <string>$APP_NAME</string>
    <key>CFBundleExecutable</key>        <string>$BIN</string>
    <key>CFBundleIdentifier</key>        <string>com.aselvais.rust-image-viewer</string>
    <key>CFBundleVersion</key>           <string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundlePackageType</key>       <string>APPL</string>
    <key>CFBundleIconFile</key>          <string>AppIcon</string>
    <key>LSMinimumSystemVersion</key>    <string>12.0</string>
    <key>NSHighResolutionCapable</key>   <true/>
  </dict>
</plist>
PLIST

# If an icon was supplied as assets/AppIcon.icns, include it.
if [ -f assets/AppIcon.icns ]; then
  cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
fi

# Re-sign ad-hoc so Gatekeeper accepts the locally built app.
codesign --force --deep --sign - "$APP" || true

# Package into a zip that preserves the bundle structure.
ZIP="$DIST/$BIN-macos-$OUT_ARCH.app.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"

echo
echo "Built: $APP"
echo "Zip:   $ZIP"
echo "Run:   open '$APP'    (or  open '$ZIP' and launch the .app)"
