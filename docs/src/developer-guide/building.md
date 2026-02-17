# Building from Source

Complete guide to building Motioner from source code.

## Prerequisites

### Required Tools

| Tool | Minimum Version | Download |
|------|----------------|----------|
| Rust | pinned nightly (see `rust-toolchain.toml`) | The repository uses a pinned nightly toolchain (e.g. `nightly-2026-01-25`) — `rustup` will respect `rust-toolchain.toml` |
| Git | Any recent | [git-scm.com](https://git-scm.com/) |
| FFmpeg | 4.0+ | [ffmpeg.org](https://ffmpeg.org/) |

### Platform-Specific Requirements

#### Windows
- Visual Studio Build Tools or Visual Studio with C++ support
- Windows SDK

#### Linux
- GCC or Clang
- Development libraries: `libgtk-3-dev`, `libx11-dev`

#### macOS
- Xcode Command Line Tools

## Clone Repository

```bash
git clone https://github.com/jvchiappini/Motioner.git
cd Motioner
```

## Build Configurations

### Development Build

Fast compilation, includes debug symbols:

```powershell
# Use the toolchain pinned in `rust-toolchain.toml` (rustup does this automatically).
# If needed, install/override explicitly:
rustup toolchain install nightly-2026-01-25
rustup override set nightly-2026-01-25

cargo build
```

**Output:** `target/debug/motioner_ui.exe`

**Use for:**
- Development
- Debugging
- Testing changes

### Release Build

Optimized binary, slower compilation:

```powershell
# Ensure the pinned toolchain is active, then:
cargo build --release
```

**Output:** `target/release/motioner_ui.exe`

**Use for:**
- Production use
- Performance testing
- Distribution

### Build with Specific Features

```powershell
# Build with wgpu feature enabled
cargo build --release --features wgpu

# Build without default features
cargo build --release --no-default-features
```

## Running

### Development Mode

```powershell
cargo run
```

### Release Mode

```powershell
cargo run --release
```

### With Logging

```powershell
$env:RUST_LOG="info"
cargo run --release
```

## Testing

### Run All Tests

```powershell
cargo test
```

### Run Specific Test

```powershell
cargo test test_name
```

### Run with Output

```powershell
cargo test -- --nocapture
```

## Code Quality

### Format Code

```powershell
cargo fmt
```

### Check Formatting

```powershell
cargo fmt -- --check
```

### Run Linter

```powershell
cargo clippy
```

### Strict Linting

```powershell
cargo clippy -- -D warnings
```

## Documentation

### Generate Rust Docs

```powershell
cargo doc --open
```

### Build mdBook Documentation

```powershell
# Install mdbook if needed
cargo install mdbook

# Build documentation
cd docs
mdbook build

# Serve locally
mdbook serve --open
```

## Troubleshooting

### Compilation Errors

**Problem:** Linking errors on Windows

**Solution:**
```powershell
# Install Visual Studio Build Tools
# Restart terminal
cargo clean
cargo build --release
```

**Problem:** Missing dependencies on Linux

**Solution:**
```bash
# Ubuntu/Debian
sudo apt install libgtk-3-dev libx11-dev

# Fedora
sudo dnf install gtk3-devel libX11-devel
```

### Performance Issues

**Problem:** Slow compilation

**Solutions:**
- Use `cargo build` instead of `--release` during development
- Install `cargo-watch` for incremental builds
- Use `sccache` for compilation caching

```powershell
# Install cargo-watch
cargo install cargo-watch

# Auto-rebuild on changes
cargo watch -x run
```

### Clean Build

```powershell
# Remove all build artifacts
cargo clean

# Full rebuild
cargo build --release
```

## Advanced Build Options

### Cross-Compilation

```powershell
# Install target
rustup target add x86_64-pc-windows-msvc

# Build for target
cargo build --release --target x86_64-pc-windows-msvc
```

### Custom Build Script

Create `build.rs` in project root for custom build logic.

### Profile Optimization

Edit `Cargo.toml`:

```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

## Benchmarking

```powershell
# Run benchmarks (if available)
cargo bench

# Profile release build
cargo build --release
# Use profiling tools on target/release/motioner_ui
```

## Distribution

### Create Distributable

```powershell
# Build optimized release
cargo build --release

# Binary location
# Windows: target\release\motioner_ui.exe
# Linux: target/release/motioner_ui
# macOS: target/release/motioner_ui
```

### Package with Assets

```powershell
# Create distribution folder
mkdir dist
cp target\release\motioner_ui.exe dist\
cp -r assets dist\
cp LICENSE dist\
cp README.md dist\
```

## Next Steps

- [Project Structure](./project-structure.md)
- [API Reference](./api-reference.md)
- [Contributing](./contributing.md)
