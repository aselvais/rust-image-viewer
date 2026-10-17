# Cross-compile a Windows x64 .exe of Rust Image Viewer from macOS/Linux.
#
# wgpu's Windows backend links d3d12.dll, which needs an MSVC/Windows-SDK linker
# that is NOT a normal cross-toolchain target. `cargo-zigbuild` (zig used as the
# linker for the `x86_64-pc-windows-gnu` target) links it cleanly instead, so you
# can build the Windows exe on an ARM/intel macOS or Linux box.
#
# Output (in ./dist/):
#   rust-image-viewer-win-x64/rust-image-viewer.exe
#   rust-image-viewer-win-x64.zip
#
# The exe links statically against the Windows-10-era `api-ms-win-crt-*`
# (Universal C Runtime) contracts, so it runs on a bare Windows 10+ install with
# NO Visual C++ Redistributable. wgpu-hal dynamically loads d3d12.dll at runtime,
# so D3D12 is not a static import. The window icon is embedded from
# assets/AppIcon.png at runtime.
#
# Usage:
#   bash scripts/build-windows-cross.sh
#
# Prerequisites:
#   - Rust toolchain:   rustup            (rustc/cargo)
#   - zig:                 brew install zig   (macOS) / see https://ziglang.org (Linux)
#   - cargo-zigbuild:    cargo install cargo-zigbuild
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="rust-image-viewer"
VER="$(grep -m1 '^version' Cargo.toml | sed -E 's/^version *= *\"([^\"]+)\".*/\1/')"
PKG="dist/${BIN}-win-x64"
ZIP="dist/${BIN}-win-x64.zip"
TARGET="x86_64-pc-windows-gnu"

# --- prerequisites -----------------------------------------------------------
command -v cargo >/dev/null 2>&1 || { echo "need rustup/cargo" >&2; exit 1; }
command -v zig   >/dev/null 2>&1 || { echo "need zig: brew install zig" >&2; exit 1; }
command -v cargo-zigbuild >/dev/null 2>&1 || { echo "need cargo-zigbuild: cargo install cargo-zigbuild" >&2; exit 1; }

echo ">> rustc $(rustc --version | awk '{print $2}')  target $TARGET  zig $(zig version)"
rustup target add "$TARGET"

echo ">> cargo zigbuild --release --target $TARGET"
# The final binary relink on zig emits a harmless "deprecated linker optimization"
# stderr note; it is not a failure, so we don't treat warning notes as errors.
cargo zigbuild --release --target "$TARGET"

# --- assemble + package ------------------------------------------------------
mkdir -p "$PKG"
cp "target/$TARGET/release/$BIN.exe" "$PKG/$BIN.exe"
chmod +x "$PKG/$BIN.exe" 2>/dev/null || true

rm -f "$ZIP"
# Prefer zip (cross-platform); fall back to a self-extracting archive.
if command -v zip >/dev/null 2>&1; then
  ( cd dist && zip -r -q "$BIN-win-x64.zip" "$BIN-win-x64" )
elif command -v 7z >/dev/null 2>&1; then
  ( cd dist && 7z a -tzip "$BIN-win-x64.zip" "$BIN-win-x64" >/dev/null )
else
  echo "neither zip nor 7z found; package left unpacked at $PKG" >&2
  exit 0
fi

echo
echo "Built (cross-compiled): $PKG/$BIN.exe   [$VER]"
echo "Zip:                     $ZIP"
echo
echo "This exe targets Windows 10 x64+ and runs WITHOUT the VC++ Redistributable."
echo "To verify it on a real Windows host: expand the zip and run rust-image-viewer.exe."
