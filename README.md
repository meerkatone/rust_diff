# Binary Diffing Plugin for Binary Ninja

A high-performance binary diffing plugin for Binary Ninja that compares functions between two binaries using advanced structural and semantic analysis. Built with Rust for performance and Python for Binary Ninja integration.

## Features

- **Multi-phase matching algorithm**: Exact hash, name-based, structural, and fuzzy matching
- **Advanced similarity metrics**: Jaccard, cosine similarity, edit distance
- **Comprehensive analysis**: CFG comparison, basic block analysis, instruction patterns
- **Multiple export formats**: JSON, CSV, SQLite, HTML reports
- **Optional Qt GUI**: Interactive results table with sorting and filtering
- **Cross-platform**: Supports Darwin, Linux, and Windows

## Installation

### Prerequisites
- Binary Ninja 3.0.0 or higher
- Rust toolchain (for building from source)
- Python 3.x
- Optional: PySide6 or PySide2 (for GUI features)

## Clone the repo
git clone https://github.com/meerkatone/rust_diff.git

## Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

## Install uv
curl -LsSf https://astral.sh/uv/install.sh | sh

Binary Diffing and Marimo Rust
Marimo notebook to compare and search the binary diffing results using the Rust Diff Plugin for Binary Ninja

git clone https://github.com/meerkatone/binary_diffing_and_marimo_rust.git

## Setup venv and Marimo
uv venv --python 3.13

source .venv/bin/activate

cd binary-diffing-and-marimo-rust

uv pip install marimo

## Launch the notebook
marimo edit ./binary_ninja_diffing_rust

The notebook will ask you to install the required dependencies via uv.

### Building
```bash
# Clone and build the Rust library
cargo build --release

# Install GUI dependencies (optional)
pip install PySide6
# or
python install_pyside.py
```

### Installing in Binary Ninja
1. Copy the plugin files to your Binary Ninja plugins directory
2. Restart Binary Ninja
3. The plugin will appear as "Binary Diffing (Rust)" in the Tools menu

## Usage

1. Open a binary in Binary Ninja
2. Go to Tools → Binary Diffing (Rust)
3. Select a target BNDB file to compare against
4. The plugin will analyze both binaries and display results

## Algorithm Overview

The plugin uses a sophisticated multi-phase matching approach:

1. **Exact Hash Matching**: Matches functions with identical CFG and call graph hashes
2. **Name Matching**: Matches functions with identical names
3. **MD-Index Matching**: Metadata-based matching similar to Diaphora
4. **Small Primes Product**: Instruction-based hashing using prime products
5. **Structural Matching**: CFG isomorphism checking
6. **Fuzzy Matching**: Similarity-based matching with configurable thresholds

## Output

Results include:
- Function matches with similarity and confidence scores
- Match type classification (exact, structural, fuzzy, etc.)
- Unmatched functions from both binaries
- Detailed analysis statistics
- Export options for further analysis

## Configuration

Default similarity thresholds:
- Similarity threshold: 0.6
- Confidence threshold: 0.5

These can be adjusted in the `BinaryDiffEngine` constructor for custom analysis requirements.

## Architecture

- **Rust Core**: High-performance matching algorithms and analysis
- **Python Interface**: Binary Ninja integration and GUI components
- **FFI Bridge**: C-compatible interface between Rust and Python
- **Modular Design**: Extensible algorithm and similarity metric system

## License

Licensed under the Apache License, Version 2.0. See the plugin metadata for full license text.

## Contributing

This plugin is designed for defensive security analysis and binary research. Extensions and improvements to matching algorithms, similarity metrics, and export formats are welcome.
