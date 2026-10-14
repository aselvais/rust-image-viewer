# Rust Image Viewer

A simple and lightweight image viewer written in Rust, leveraging the `egui` framework for a smooth and responsive user interface.

## Features

- **File Explorer**: Easily navigate through your directories to find images.
- **Image Viewer**: View images with ease.
- **Fast and Lightweight**: Built with Rust for high performance.

## Technologies Used

- [Rust](https://www.rust-lang.org/): The core programming language.
- [egui](https://github.com/emilk/egui): An easy-to-use, immediate mode GUI library for Rust.
- [eframe](https://github.com/emilk/egui/tree/master/crates/eframe): The framework for running `egui` applications on desktop.
- [image](https://github.com/image-rs/image): A powerful image processing library in Rust.

## Prerequisites

Ensure you have the Rust toolchain installed on your system. If you don't have it, you can install it via [rustup](https://rustup.rs/).

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Getting Started

### 1. Clone the repository

```bash
git clone <repository-url>
cd rust-image-viewer
```

### 2. Build the project

To compile the project, run:

```bash
cargo build
```

### 3. Run the application

To start the application, run:

```bash
cargo run
```

You can also specify an initial directory to open:

```bash
cargo run -- /path/to/your/images
```

## Development

### Running Tests

To run the unit tests, use:

```bash
cargo test
```

### Project Structure

- `src/main.rs`: The entry point of the application.
- `src/app.rs`: Contains the main application state and logic.
- `src/explorer.rs`: Implements the file explorer functionality.
- `src/viewer.rs`: Implements the image viewer functionality.
- `src/utils.rs`: Contains utility functions used across the project.
- `src/explorer_new.rs`: An experimental version of the explorer.

## License

[Specify the license here, e.g., MIT or Apache 2.0]
