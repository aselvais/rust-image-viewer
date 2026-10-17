Rust Image Viewer  v0.1.0  (Linux x86_64, glibc)

Run it:
  chmod +x rust-image-viewer
  ./rust-image-viewer                       # opens the current directory in the file explorer
  ./rust-image-viewer /path/to/images       # open a folder (siblings are collectable for Prev/Next)
  ./rust-image-viewer /path/to/image.png    # open a single image

Install (no root — copy/paste this: it installs into $HOME/.local and rewrites the
bundled .desktop's Exec/Icon to your own account, so the menu entry just works):

  mkdir -p "$HOME/.local/bin"            "$HOME/.local/share/applications"            "$HOME/.local/share/icons/hicolor/256x256/apps"
  cp rust-image-viewer  "$HOME/.local/bin/"
  cp AppIcon.png        "$HOME/.local/share/icons/hicolor/256x256/apps/rust-image-viewer.png"
  # rewrite the shipped .desktop's Exec/Icon to absolute $HOME paths, then install it
  sed -E -e "s#^Exec=.*#Exec=$HOME/.local/bin/rust-image-viewer %f#" \
          -e "s#^Icon=.*#Icon=$HOME/.local/share/icons/hicolor/256x256/apps/rust-image-viewer.png#" \
      "Rust Image Viewer.desktop" > "$HOME/.local/share/applications/Rust Image Viewer.desktop"
  # one-time: put $HOME/.local/bin on PATH and refresh the icon cache
  echo 'export PATH="$HOME/.local/bin:$PATH"' >> $HOME/.zshrc        # or .bashrc
  gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

Runtime libraries: only glibc. X11/Wayland/GL/Vulkan are loaded at runtime, so no
graphics dev packages are needed for end users to *run* the binary (they just need a
normal desktop environment).
