# Build a self-contained Linux x86_64 release of Rust Image Viewer + package it.
#
# Usage:
#   scripts/build-linux.sh              # native build on a Linux box (default)
#   scripts/build-linux.sh x86_64       # force x86_64-unknown-linux-gnu
#   scripts/build-linux.sh aarch64      # force aarch64-unknown-linux-gnu
#   scripts/build-linux.sh x86_64 7z     # package with 7z instead of zip
#
# Output (in ./dist/):
#   dist/rust-image-viewer-linux-x86_64/rust-image-viewer   (the stripped binary)
#   dist/rust-image-viewer-linux-x86_64/Rust Image Viewer.desktop
#   dist/rust-image-viewer-linux-x86_64/AppIcon.png
#   dist/rust-image-viewer-linux-x86_64/README.txt
#   dist/rust-image-viewer-linux-x86_64.tar.gz
#   dist/rust-image-viewer-linux-x86_64.zip                 (when zip/7z available)
#
# --- Why this is easy on Linux, unlike Windows -------------------------------
# The GUI stack (winit / wgpu / glutin) does NOT static-link the platform graphics
# libraries. X11, Wayland, GL/GLX/EGL and Vulkan are all loaded *at runtime* via
# libloading/dlib/pkg-config (see x11-dl, x11rb, wayland-sys, ash, khronos-egl in
# the dependency tree), so no MSVC/Windows-SDK linker is needed. The binary only
# hard-depends on the C library (glibc). That is why we ship a glibc
# `x86_64-unknown-linux-gnu` build rather than a musl one: a dynamic glibc binary
# runs on essentially every modern desktop Linux, whereas a fully static musl build
# fights winit's GL/wayland code paths.
#
# On a real Linux host we just use `cargo build`. On a non-Linux host (this Mac,
# or any arm/intel/macOS dev box) we cross-compile with `cargo-zigbuild` — zig
# supplies the glibc sysroot the linker needs, exactly as build-windows-cross.sh
# uses it for the Windows build. The resulting binary is a portable glibc ELF that
# links the target's glibc at run time, so an official, broadly-compatible release
# should still be built *on* Linux (the CI ubuntu job in .github/workflows/release.yml
# does exactly that); the cross build is a fast way to verify it compiles anywhere.
#
# Prerequisites (non-Linux cross-compile only):
#   - zig + cargo-zigbuild   (already used by build-windows-cross.sh)
# Native Linux build only needs a C linker (gcc/clang) in PATH.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="rust-image-viewer"
VER="$(grep -m1 '^version' Cargo.toml | sed -E 's/^version *= *\"([^\"]+)\".*/\1/')"
VER="${VER:-0.1.0}"
DIST="dist"
ARCH="x86_64"
PKG_TOOL="${2:-zip}"

# --- pick the target arch ----------------------------------------------------
case "${1:-native}" in
  native|"" )
    case "$(uname -m)" in
      x86_64|amd64)   ARCH="x86_64" ;;
      aarch64|arm64)  ARCH="aarch64" ;;
      *) echo "unknown host arch $(uname -m); pass x86_64 or aarch64 explicitly" >&2; exit 1 ;;
    esac
    ;;
  x86_64|amd64)    ARCH="x86_64" ;;
  aarch64|arm64)   ARCH="aarch64" ;;
  *) echo "unknown arch arg: $1 (use native|x86_64|aarch64)" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
  aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
esac

PKG="$DIST/${BIN}-linux-$ARCH"
TAR="$PKG.tar.gz"
ZIP="$PKG.zip"

HOSTOS="$(uname -s)"

# Decide how to build: native on Linux, cross via zig elsewhere.
if [ "$HOSTOS" = "Linux" ]; then
  BUILD_CMD=(cargo build --release --target "$TARGET")
else
  command -v zig >/dev/null 2>&1       || { echo "cross-compiling from $HOSTOS: need zig (https://ziglang.org)" >&2; exit 1; }
  command -v cargo-zigbuild            >/dev/null 2>&1 || { echo "need cargo-zigbuild: cargo install cargo-zigbuild" >&2; exit 1; }
  BUILD_CMD=(cargo zigbuild --release --target "$TARGET")
fi

echo ">> rustc $(rustc --version | awk '{print $2}')  target $TARGET  (host $HOSTOS/$(uname -m))"
rustup target add "$TARGET"

echo ">> ${BUILD_CMD[*]}"
# The cross (zig) link emits a harmless "deprecated linker optimization" note; it is
# not a failure, so we don't treat build-time warning notes as an error.
"${BUILD_CMD[@]}"

# --- assemble the self-contained package -------------------------------------
BIN_PATH="target/$TARGET/release/$BIN"
[ -f "$BIN_PATH" ] || { echo "expected binary not found: $BIN_PATH" >&2; exit 1; }

rm -rf "$PKG" "$TAR" "$ZIP"
mkdir -p "$PKG"
cp "$BIN_PATH" "$PKG/$BIN"
chmod 0755 "$PKG/$BIN"

# Ship the app icon alongside the binary.
if [ -f assets/AppIcon.png ]; then
  cp assets/AppIcon.png "$PKG/AppIcon.png"
fi

# A .desktop launcher. The canonical install target is /usr/local/bin (see README.txt
# for a no-root $HOME/.local install that rewrites the paths). Exec takes %f so the
# app can be started on a single image/folder from a file manager.
cat > "$PKG/Rust Image Viewer.desktop" <<DESKTOP
[Desktop Entry]
Version=$VER
Type=Application
Name=Rust Image Viewer
GenericName=Image Viewer
Comment=Fast, lightweight image viewer (Rust + egui/wgpu)
Exec=/usr/local/bin/$BIN %f
Icon=rust-image-viewer
Categories=Graphics;Viewer;Utility;
StartupNotify=false
Terminal=false
MimeType=image/jpeg;image/png;image/gif;image/bmp;image/webp;
DESKTOP

# A short install/run note so the tarball is self-explanatory.
cat > "$PKG/README.txt" <<EOF
Rust Image Viewer  v$VER  (Linux $ARCH, glibc)

Run it:
  chmod +x $BIN
  ./$BIN                       # opens the current directory in the file explorer
  ./$BIN /path/to/images       # open a folder (siblings are collectable for Prev/Next)
  ./$BIN /path/to/image.png    # open a single image

Install (no root — copy/paste this: it installs into \$HOME/.local and rewrites the
bundled .desktop's Exec/Icon to your own account, so the menu entry just works):

  mkdir -p "\$HOME/.local/bin" \
           "\$HOME/.local/share/applications" \
           "\$HOME/.local/share/icons/hicolor/256x256/apps"
  cp rust-image-viewer  "\$HOME/.local/bin/"
  cp AppIcon.png        "\$HOME/.local/share/icons/hicolor/256x256/apps/rust-image-viewer.png"
  # rewrite the shipped .desktop's Exec/Icon to absolute \$HOME paths, then install it
  sed -E -e "s#^Exec=.*#Exec=\$HOME/.local/bin/rust-image-viewer %f#" \\
          -e "s#^Icon=.*#Icon=\$HOME/.local/share/icons/hicolor/256x256/apps/rust-image-viewer.png#" \\
      "Rust Image Viewer.desktop" > "\$HOME/.local/share/applications/Rust Image Viewer.desktop"
  # one-time: put \$HOME/.local/bin on PATH and refresh the icon cache
  echo 'export PATH="\$HOME/.local/bin:\$PATH"' >> \$HOME/.zshrc        # or .bashrc
  gtk-update-icon-cache -f -t "\$HOME/.local/share/icons/hicolor" 2>/dev/null || true

Runtime libraries: only glibc. X11/Wayland/GL/Vulkan are loaded at runtime, so no
graphics dev packages are needed for end users to *run* the binary (they just need a
normal desktop environment).
EOF

# --- package -----------------------------------------------------------------
# (The subshells cd into $DIST, so reference the base names, not dist/-prefixed
# paths, when creating the archives.)
TAR_BASE="$(basename "$TAR")"    # e.g. rust-image-viewer-linux-x86_64.tar.gz
ZIP_BASE="$(basename "$ZIP")"    # e.g. rust-image-viewer-linux-x86_64.zip
PKG_BASE="$(basename "$PKG")"    # e.g. rust-image-viewer-linux-x86_64

# tar.gz is the canonical archive (universally extractable on Linux).
if ! ( cd "$DIST" && tar -czf "$TAR_BASE" "$PKG_BASE" ); then
  echo "error: creating tarball failed" >&2
  exit 1
fi

# A zip too, so the package is one-click on Windows / desktop file managers.
if command -v zip  >/dev/null 2>&1; then
  ( cd "$DIST" && zip -r -q "$ZIP_BASE" "$PKG_BASE" ) || echo "zip step failed (tar.gz still produced)" >&2
elif command -v 7z >/dev/null 2>&1; then
  ( cd "$DIST" && 7z a -tzip "$ZIP_BASE" "$PKG_BASE" >/dev/null ) || echo "7z step failed (tar.gz still produced)" >&2
else
  echo "note: neither zip nor 7z found; only $TAR was produced" >&2
fi

echo
echo "Built:   $PKG/$BIN   [v$VER, $ARCH-unknown-linux-gnu]"
echo "Tar:     $TAR  ($(du -h "$TAR" 2>/dev/null | cut -f1))"
[ -f "$ZIP" ] && echo "Zip:     $ZIP  ($(du -h "$ZIP" 2>/dev/null | cut -f1))"
if [ "$HOSTOS" != "Linux" ]; then
  echo
  echo "Cross-compiled from $HOSTOS — the binary should be produced *on* Linux for a"
  echo "broad, official release (see .github/workflows/release.yml build-linux)."
fi
echo "Run on Linux:  tar -xzf $TAR && ./$PKG/$BIN  (or see $PKG/README.txt to install)"
