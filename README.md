# Rust Image Viewer

A simple and lightweight image viewer written in Rust, built on the `egui`
immediate-mode GUI framework with an optional `EXIF` / metadata inspector.

This is a small project made with all local LLM as a test.
I've mainly used Hermes and qwen3.8:27b on a Mac M3 Max.

I wanted to make a very simple and fast image viewer for windows as I was not happy with the current bloated offering.

## What's new

- **Full 5-platform release matrix + aarch64 Linux** — all five targets now build
  clean: macOS **arm64 + x86_64** (native `cargo build`) and Linux **x86_64 +
  aarch64** and **Windows x64** (cross-compiled with `cargo-zigbuild`, Zig 0.17.0,
  which supplies the glibc / MSVC sysroot the linker needs). The **aarch64 Linux**
  package (`rust-image-viewer-linux-aarch64.zip` / `.tar.gz`) is new, mirroring the
  x86_64 one; per-platform sizes and SHA-256 checksums are in the
  **Download latest release** section below.
- **Linux build added** — there is now a native Linux release
  (`x86_64-unknown-linux-gnu`, glibc) via `scripts/build-linux.sh` plus a
  `build-linux` CI job. Because `winit`/`wgpu` load X11/Wayland/GL/Vulkan
  *dynamically at run time*, a Linux binary needs only glibc plus a desktop
  environment to run — no MSVC/Windows-SDK toolchain (unlike Windows) and no Apple
  re-signing (unlike macOS).
- **Windows image previews fixed** — thumbnails and the main viewer now load on
  Windows. Before this they showed broken-image placeholders (the red `!` triangles)
  because the `file://` image URI was built from a Windows path (`C:\…`), which
  `egui_extras`' file loader couldn't resolve. Local paths are now normalised to a
  valid `file:///C:/…` URI on Windows; MacOS and Linux were already fine.

## Download latest release

Self-contained packages are in [`dist/`](dist/). All builds are **v0.1.0**.
Unpacked sizes are the binary plus the bundled `AppIcon.png` / `README.txt` /
`.desktop` launcher.

| Platform | Download | Archive size | SHA-256 |
| --- | --- | --- | --- |
| **Linux** x86_64 (glibc) | [`.zip`](dist/rust-image-viewer-linux-x86_64.zip) · [`.tar.gz`](dist/rust-image-viewer-linux-x86_64.tar.gz) | 8.0 MiB | `2314ff88…` / `5193d9d8…` |
| **Linux** aarch64 (glibc) | [`.zip`](dist/rust-image-viewer-linux-aarch64.zip) · [`.tar.gz`](dist/rust-image-viewer-linux-aarch64.tar.gz) | 8.6 MiB | `649b3c5d…` / `c5ef537c…` |
| **macOS** universal (arm64 + x86_64) | [`rust-image-viewer-macos-universal.app.zip`](dist/rust-image-viewer-macos-universal.app.zip) | 10.5 MiB | `5703ebdd…` |
| **Windows** x64 | [`rust-image-viewer-win-x64.zip`](dist/rust-image-viewer-win-x64.zip) | 6.1 MiB | `1054f8e4…` |

- **Linux**: a stripped glibc ELF (~18 MiB unpacked) that needs only a desktop
  environment — X11/Wayland/GL/Vulkan are loaded at run time. Each `.zip`/`.tar.gz`
  bundles the binary, `AppIcon.png`, `README.txt`, and a `Rust Image Viewer.desktop`
  launcher with a no-root install snippet.
- **macOS**: a universal `.app` (~22 MiB unpacked), ad-hoc signed — runs on
  Apple Silicon *and* Intel.
- **Windows**: x64 `exe` (~14 MiB unpacked) **with the MSVC/UCRT runtime DLLs
  bundled** (via `deploy`), so it runs on a bare Windows install.

### Release build matrix (v0.1.0)

All five targets build clean and ship **stripped**. macOS targets are built
natively with `cargo build`; the Linux and Windows targets are cross-compiled with
[`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild) (Zig 0.17.0).

| Target triple | Size | Built with | SHA-256 |
| --- | --- | --- | --- |
| `aarch64-apple-darwin` | 10.2 MiB | `cargo build` (arm64 Mac) | `a610d542…` |
| `x86_64-apple-darwin` | 11.5 MiB | `cargo build` (x86_64 Mac) | `c79360e5…` |
| `x86_64-unknown-linux-gnu` | 17.9 MiB | `cargo-zigbuild` (Zig) | `d5297e3c…` |
| `aarch64-unknown-linux-gnu` | 17.9 MiB | `cargo-zigbuild` (Zig) | `519b7393…` |
| `x86_64-pc-windows-gnu` | 13.4 MiB | `cargo-zigbuild` (Zig) | `276c8bc8…` |

The raw release binaries live under `target/<triple>/release/` (git-ignored); the
packaged downloads in the table above are the shareable artifacts.

### Verify a download

Run this from the `dist/` directory (uses the full SHA-256 of each package):

```sh
shasum -a256 -c <<'EOF'
2314ff88528b26760ae1e0bbc5d64ffb52f6fd71aaea98372bf136e34e913a01  rust-image-viewer-linux-x86_64.zip
5193d9d8f4db22b83fd49563b5d8adbf0a831a2533071b65008bb1e108e177ee  rust-image-viewer-linux-x86_64.tar.gz
649b3c5d5df8854c5c7c91fa0b240a21bf9928b6b8f1cfd8a5d1f1a3225f0297  rust-image-viewer-linux-aarch64.zip
c5ef537c498f149588b2c30f22d5d8c975771939e5f2b568753712c69eefd545  rust-image-viewer-linux-aarch64.tar.gz
5703ebdda6644cce60cb641225d33efc79cdd18fd275846b9db96d3a23ffd142  rust-image-viewer-macos-universal.app.zip
1054f8e46975c54b2b139ff29e24f7fe4d76efe2adcfd3cb1ef7683bfb75c411  rust-image-viewer-win-x64.zip
EOF
```

## Screenshots

![thumbnails view](assets/screenshots/thubnailview.png)
![image view](assets/screenshots/imageview.png)


## Features

- **File Explorer**: navigate directories to find images.
- **Image Viewer**: flip through images with the arrow keys **or** on-screen
  **◀ Prev / Next ▶** buttons in the viewer's top toolbar.
- **Zoom**: a zoom slider, **Fit screen** and **100%** buttons, plus mouse-wheel
  zoom and trackpad pinch (scroll up = zoom in, scroll down = zoom out). When the
  image is larger than the fit window, scroll bars appear so you can pan it.
- **Details panel**: shows format, dimensions, megapixels, colour/channels,
  bit depth, file size, last-modified time, and any EXIF metadata — rendered
  **inside** the main window (no separate OS window).
- **Drag & drop / Open With**: drag a folder or image from the OS (e.g. Finder)
  straight onto the window, or right-click an image (or folder) and pick
  **Open With → Rust Image Viewer…**. A dropped or launched folder is *browsed*
  in the explorer; an image is *opened* for viewing (with its siblings collected
  for ◀ Prev / Next ▶ navigation).
- **App icon**: a custom image icon (`assets/AppIcon…`) appears in the Dock, the
  window title bar, and Launchpad.
- **Fast and lightweight**: Rust + `egui` (GPU-accelerated via wgpu) with
  optimized, stripped release binaries.

## Todo

- simple image resize (and bulk resize)
- Better design for the GUI
- Register the Linux `.desktop` icon / "Open With" associations *system-wide*
  (a no-root, per-user install snippet is already bundled in the Linux
  tarball/zip; a distro-packaged `.desktop` is a nice-to-have follow-up)
- ...

## Technologies Used

- [Rust](https://www.rust-lang.org/): the core language.
- [egui / eframe](https://github.com/emilk/egui): immediate-mode GUI and its desktop frontend.
- [image](https://github.com/image-rs/image): PNG, JPEG, BMP, GIF, WebP decoding.
- [kamadak-exif](https://github.com/retep0804/kamadak-exif): EXIF metadata parsing.
- [winit](https://github.com/rust-windowing/winit) + [wgpu](https://github.com/gfx-rs/wgpu):
  windowing + GPU rendering. On Linux, X11/Wayland/GL/Vulkan are loaded at run time,
  which is why the Linux build is so easy (no native graphics SDK).

## Prerequisites

- The **Rust** toolchain via [rustup](https://rustup.rs/) (stable):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **macOS 12+**, **Windows 10+**, or **Linux (glibc)** for the GUI — the GUI uses
  the Metal / D3D12 / GL/Vulkan backends and X11 or Wayland for windows.
- A native C/C++ linker is required (Xcode command-line tools on macOS; the
  "Desktop development with C++" workload on Windows; `gcc` or `clang` on Linux).

```bash
# macOS: enable the system linker
xcode-select --install
```

On Debian/Ubuntu you also need the (build-time-only) X11/Wayland/GL dev headers —
covered in the **Linux** build section below. At **run** time a normal desktop
environment is enough; the graphics libs themselves are loaded dynamically.

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
it via `ditto`. It also:

- bundles `assets/AppIcon.icns` into the bundle so the app has a proper icon in
  Finder/Dock/Launchpad;
- declares `CFBundleDocumentTypes` so right-click → **Open With → Rust Image
  Viewer…** works for images (and folders) and the app can be set as the default
  handler;
- registers the finished `.app` (and its document types) with Launch Services via
  `lsregister`, so the icon and associations take effect immediately on the
  machine that built it.

The custom icon lives in `assets/` — `AppIcon.svg` is the source of truth; run
`bash scripts/make-icon.sh` to regenerate `AppIcon.png` (embedded at compile time
as the window icon), `AppIcon.icns` (macOS), and `AppIcon.ico` (Windows). For
public distribution over the internet you'd re-sign with an Apple Developer
identity and notarize (ad-hoc signing is fine for local testing only).

> **Windows**: the release `exe` gets its window/taskbar icon at compile time from
> the embedded `assets/AppIcon.png`. Registering the `exe`'s file icon in
> Explorer and its "Open With" file association in the Windows registry is a
> follow-up (no change here); drag-and-drop and the in-app icon already work.

### Linux

Unlike Windows, there is **no hard blocker** to building for Linux: the graphics
backend (`wgpu`) and the windowing stack (`winit`) *load* X11, Wayland,
GL/GLX/EGL/Vulkan and the platform libraries at **run time** (via
`libloading`/`pkg-config`), so a build only needs a C linker plus those
libraries — no MSVC/Windows-SDK (as Windows needs) and no Apple re-signing (as
macOS needs). We therefore ship a **glibc** `x86_64-unknown-linux-gnu` binary: a
dynamic glibc build runs on essentially every modern desktop, whereas a fully
static `musl` build fights `winit`'s GL/Wayland code paths.

```bash
# Native build on Debian/Ubuntu (or any glibc box) — install the dev headers first:
sudo apt-get install -y build-essential pkg-config \
  libx11-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev
cargo build --release
# -> target/release/rust-image-viewer          (a stripped glibc x86_64 ELF)

# One command: build + bundle a .desktop launcher + icon + zip/tarball into ./dist/
bash scripts/build-linux.sh            # native x86_64 (default)
bash scripts/build-linux.sh aarch64    # aarch64-unknown-linux-gnu
# -> dist/rust-image-viewer-linux-x86_64/rust-image-viewer
# -> dist/rust-image-viewer-linux-x86_64/Rust Image Viewer.desktop
# -> dist/rust-image-viewer-linux-x86_64/{AppIcon.png,README.txt}
# -> dist/rust-image-viewer-linux-x86_64.zip  /  ...tar.gz
```

The script **detects the host**: on a Linux machine it runs a plain
`cargo build`; off Linux (an ARM/Intel macOS, another box, or a CI runner) it
cross-compiles with `cargo-zigbuild` — zig supplies the glibc sysroot the linker
needs, exactly as `scripts/build-windows-cross.sh` does for the Windows build.
(Run it off-Linux with `zig` + `cargo install cargo-zigbuild` available — the same
prerequisites as the cross Windows build.) The package ships a self-explanatory
`README.txt` with a copy-paste, **no-root** `$HOME/.local` install that also
rewrites the `.desktop`'s `Exec`/`Icon` to your user, plus the bundled
`AppIcon.png`, so a Linux user can extract, run, and register the app in the menu
without `sudo`.

> **For an official, broadly-compatible release, build *on* Linux.** The
> `build-linux` job in `.github/workflows/release.yml` runs
> `scripts/build-linux.sh` on an `ubuntu-latest` runner, where the graphics dev
> headers are guaranteed present and the binary links the runner's glibc — the most
> portable baseline. A cross-compiled binary also runs on most Linux systems, but
> it assumes a relatively new glibc, so prefer the CI / native Linux build for
> distribution.

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
# Build native, self-contained artifacts for Windows + macOS + Linux and attach
# them to a GitHub release. Push a version tag to trigger it automatically:
git tag v0.1.0 && git push origin v0.1.0
# (or trigger it manually: Actions → "Release" → "Run workflow".)
```

It produces, per tag:

- `rust-image-viewer-linux-x86_64.zip` / `…tar.gz` — the glibc `x86_64` Linux
  binary, bundled with its `.desktop` launcher and `AppIcon.png`. Runs with just a
  desktop environment (X11/Wayland/GL/Vulkan are loaded at run time, so no
  graphics dev packages are needed by end users).
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
- `src/viewer.rs` — the image viewer (toolbar, image rendering, zoom, mouse-wheel).
- `src/metadata.rs` — builds the image + EXIF metadata shown in the Details panel.
- `src/utils.rs` — shared helpers (directory listing, the cross-platform `file://` URI normaliser).
- `src/explorer_new.rs` — an experimental explorer; **not compiled** (not declared in `main.rs`).
- `scripts/build-macos.sh` — builds and packages the self-contained universal macOS `.app`.
- `scripts/build-linux.sh` — builds and packages the self-contained Linux glibc
  binary (`.desktop` + icon + zip/tarball); cross-compiles via `cargo-zigbuild`
  when run off a Linux host.
- `scripts/build-windows-cross.sh` — cross-compiles the Windows exe on macOS/Linux
  via `cargo-zigbuild`.
- `scripts/build-windows.ps1` — the native Windows build/packaging script (for a
  Windows host or the `windows-latest` CI runner).
- `scripts/make-icon.sh` — regenerates the app icons (`png`/`icns`/`ico`) from `assets/AppIcon.svg`.
- `assets/` — the custom app icon (`AppIcon.svg` artwork + generated `png`/`icns`/`ico`, plus `AppIcon.png` embedded at compile time as the window icon).
- `.github/workflows/release.yml` — CI that builds self-contained **Linux + Windows + macOS** release artifacts.

## Changelog

All notable changes are recorded here, newest first.

### 2026-10-18 — v0.1.0

#### Added
- **aarch64 Linux release** — `rust-image-viewer-linux-aarch64.zip` / `.tar.gz`
  now ship alongside the x86_64 Linux build, completing the five-platform matrix.
  The aarch64 target is cross-compiled with `cargo-zigbuild` (Zig 0.17.0).
- **Selection modal with bulk operations** — pick several thumbnails and run one
  action across all of them: **delete**, **rotate 90°** (clockwise /
  counter-clockwise), **resize** to a target width, or **convert to JPEG**. The
  logic lives in a new UI-free, unit-tested `src/image_ops.rs` module; a single
  corrupt or unwritable file is reported instead of aborting the whole batch, and
  the outcome is shown in a transient message bar.
- **Native Linux build + `build-linux` CI job** — `x86_64-unknown-linux-gnu`
  (glibc) via `scripts/build-linux.sh`, plus a `build-linux` job in the release
  workflow.
- **Comprehensive download documentation** — a "Download latest release" section
  with per-platform archive sizes and SHA-256 checksums, a "Verify a download"
  section, and a "Reproducing the artifacts" section.

#### Fixed
- **Windows image previews** — thumbnails and the main viewer now load on Windows.
  The `file://` image URI is normalised to `file:///C:/…` so `egui_extras` can
  resolve local paths; before this they showed broken-image placeholders.

## License

Not yet specified — add a `LICENSE` file (e.g. MIT or Apache-2.0) and replace this section.
