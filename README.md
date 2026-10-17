# Rust Image Viewer

A simple and lightweight image viewer written in Rust, built on the `egui`
immediate-mode GUI framework with an optional `EXIF` / metadata inspector.

## Features

- **File Explorer**: navigate directories to find images.
- **Image Viewer**: flip through images with the arrow keys; `⌘/Ctrl + I` toggles
  an in-window details panel.
- **Details panel**: shows format, dimensions, megapixels, colour/channels,
  bit depth, file size, last-modified time, and any EXIF metadata — rendered
  **inside** the main window (no separate OS window).
- **Fast and lightweight**: Rust + `egui` (v2/Vello, GPU-accelerated) with
  optimized, stripped release binaries.

## Technologies Used

- [Rust](https://www.rust-lang.org/): the core language.
- [egui / eframe](https://github.com/emilk/egui): immediate-mode GUI and its desktop frontend.
- [image](https://github.com/image-rs/image): PNG, JPEG, BMP, GIF, WebP decoding.
- [kamadak-exif](https://github.com/retep0804/kamadak-exif): EXIF metadata parsing.

## Prerequisites

- The **Rust** toolchain via [rustup](https://rustup.rs/) (stable):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **macOS 12+** or **Windows 10+** for the GUI (uses the Metal / D3D backends).
- A native C/C++ linker is required (Xcode command-line tools on macOS; the
  "Desktop development with C++" workload on Windows).

```bash
# macOS: enable the system linker
xcode-select --install
```

## Getting Started

### 1. Clone the repository

```bash
git clone git@github.com:aselvais/rust-image-viewer.git
cd rust-image-viewer
```

### 2. Run in development

```bash
cargo run                       # opens in your current directory
cargo run -- /path/to/images    # start in a specific folder
```

## Build & Distribute (self-contained binaries)

The release profile is optimized and stripped (`Cargo.toml` → `[profile.release]`:
`opt-level = 2`, `lto`, `codegen-units = 1`, `strip`), so binaries are small and
need no bundled runtime beyond what ships with the OS.

### macOS (local — Apple Silicon or Intel)

```bash
# Optimized single-arch native binary
cargo build --release
open target/release/rust-image-viewer          # or just double-click it

# Self-contained UNIVERSAL .app (arm64 + x86_64) + zip, ad-hoc signed
bash scripts/build-macos.sh            # default: universal
bash scripts/build-macos.sh aarch64    # Apple-Silicon only
bash scripts/build-macos.sh x86_64     # Intel only
# -> dist/rust-image-viewer.app
# -> dist/rust-image-viewer-macos-universal.app.zip
```

The script `lipo`s both architecture slices into one universal Mach-O, wraps it in
`Rust Image Viewer.app` with a minimal `Info.plist`, ad-hoc code-signs it, and zips
it via `ditto`. Drop a real icon at `assets/AppIcon.icns` and it is bundled
automatically. For public distribution over the internet you'd re-sign with an Apple
Developer identity and notarize (ad-hoc signing is fine for local testing only).

### Windows (build natively on a Windows machine)

> The graphics backend (`wgpu`) links the Win32 Direct3D12 backend, which needs the
> Windows SDK/`link.exe`. There is **no** working cross-toolchain for that from
> macOS/Linux, so a Windows binary must be built on Windows (or in CI — see below).

```powershell
# 1. On Windows: install the VC++ "Desktop development" tools (provides link.exe + SDK)
# 2. Build an optimized release exe
cargo build --release
# -> target\release\rust-image-viewer.exe

# 3. Self-contained: the exe links the MSVC/UCRT runtime dynamically. Either install
#    the "Microsoft Visual C++ Redistributable" on the target machine, or bundle the
#    runtime DLLs next to the exe (recommended) using
#    https://github.com/deluan/deploy , then zip that folder.
```

### Continuous integration (recommended — builds every platform)

A ready-to-use workflow is at `.github/workflows/release.yml`:

```bash
# Build native, self-contained artifacts for Windows + macOS and attach them to a
# GitHub release. Push a version tag to trigger it automatically:
git tag v0.1.0 && git push origin v0.1.0
# (or trigger it manually: Actions → "Release" → "Run workflow".)
```

It produces, per tag:

- `rust-image-viewer-win-x64.zip` — the x64 Windows exe **with the MSVC/UCRT
  runtime DLLs bundled** (via `deploy`) so it runs on a bare Windows install.
- `rust-image-viewer-macos-universal.app.zip` — the universal `.app` (arm64 +
  x86_64), ad-hoc signed, matching `scripts/build-macos.sh`.

## Development

### Running tests

```bash
cargo test
```

### Project structure

- `src/main.rs` — entry point (native window loop, initial directory).
- `src/app.rs` — main application state; hosts the file explorer, the viewer, and the in-window Details panel.
- `src/explorer.rs` — the directory/file explorer.
- `src/viewer.rs` — the image viewer (navigation, image rendering).
- `src/metadata.rs` — builds the image + EXIF metadata shown in the Details panel.
- `src/utils.rs` — shared helpers (directory listing, etc.).
- `src/explorer_new.rs` — an experimental explorer; **not compiled** (not declared in `main.rs`).
- `scripts/build-macos.sh` — builds and packages the self-contained universal macOS `.app`.
- `.github/workflows/release.yml` — CI that builds self-contained Windows + macOS release artifacts.

## License

Not yet specified — add a `LICENSE` file (e.g. MIT or Apache-2.0) and replace this section.
