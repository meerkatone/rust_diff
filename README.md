# Binary Diffing Plugin for Binary Ninja

A high-performance binary diffing plugin for Binary Ninja that compares functions between two binaries using advanced structural and semantic analysis. Built with Rust for performance and Python for Binary Ninja integration.

## Features

- **Multiple export formats**: JSON, CSV, SQLite, HTML reports
- **Optional Qt GUI**: Interactive results table with sorting and filtering
- **Cross-platform**: Supports Darwin, Linux, and Windows

## Installation

### Prerequisites

- [Binary Ninja](https://binary.ninja/) (Commercial or Personal license latest dev build)
- [Rust toolchain](https://rustup.rs/) (latest stable)
- Binary Ninja API development headers
- Python 3.x
- Optional: PySide6 or PySide2 (for GUI features)

### Build and Install

1. Clone this repository:
   ```bash
   git clone https://github.com/meerkatone/rust_diff.git
   cd rust_diff
   ```

2. Set up Binary Ninja environment (if needed):
   ```bash
   export BINJA_DIR="/path/to/your/binaryninja/installation"
   ```

3. Build the plugin:
  ```bash
  # Clone and build the Rust library
  cargo build --release

  # Install GUI dependencies (optional)
  pip install PySide6
  # or
  python install_pyside.py
  ```

4. Copy the compiled plugin to Binary Ninja's plugin directory:
   ```bash
   # macOS
   cp target/release/librust_diff.dylib ~/Library/Application\ Support/Binary\ Ninja/plugins/
   
   # Linux
   cp target/release/librust_diff.so ~/.binaryninja/plugins/
   
   # Windows
   copy target\release\librust_diff.dll %APPDATA%\Binary Ninja\plugins\
   ```

5. Restart Binary Ninja to load the plugin

## Usage

1. Open a binary in Binary Ninja
2. Go to Tools → Binary Diffing (Rust)
3. Select a target BNDB file to compare against
4. The plugin will analyze both binaries and display results
