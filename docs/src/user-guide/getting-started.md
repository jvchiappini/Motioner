# Getting Started

This guide will help you install and run Motioner for the first time.

## Prerequisites

Before you begin, ensure you have the following installed:

### Required

| Tool | Version | Purpose | Installation |
|------|---------|---------|--------------|
| **Rust** | stable (1.70+) | Build and run Motioner | [Install rustup](https://rustup.rs/) |
| **FFmpeg** | Latest | Video encoding | [Download FFmpeg](https://ffmpeg.org/download.html) |

### Verify Installation

```powershell
# Check Rust installation
rustc --version
cargo --version

# Check FFmpeg installation
ffmpeg -version
```

## Installation

### 1. Clone the Repository

```powershell
git clone https://github.com/jvchiappini/Motioner.git
cd Motioner
```

### 2. Build the Project

```powershell
# Development build (faster compilation, slower runtime)
cargo build

# Release build (slower compilation, optimized runtime)
cargo build --release
```

### 3. Run Motioner

```powershell
# Run development version
cargo run

# Run optimized release version (recommended)
cargo run --release
```

## First Launch

When you first launch Motioner, you'll see:

1. **Welcome Modal** — Quick introduction and settings
2. **Main Canvas** — Animation preview area
3. **Timeline Panel** — Animation timeline editor
4. **Properties Panel** — Scene and animation settings

## Next Steps

- [Interface Overview](./interface-overview.md) — Learn about the UI
- [Creating Animations](./creating-animations.md) — Build your first animation
- [Exporting Projects](./export.md) — Export to video or images

## Troubleshooting

### FFmpeg Not Found

If you get an error about FFmpeg:

```powershell
# Windows: Add FFmpeg to PATH
# 1. Download FFmpeg from https://ffmpeg.org/download.html
# 2. Extract to C:\ffmpeg
# 3. Add C:\ffmpeg\bin to System PATH
```

### Build Errors

```powershell
# Clean and rebuild
cargo clean
cargo build --release
```

### Performance Issues

- Make sure you're running the release build: `cargo run --release`
- Close other GPU-intensive applications
- Check system requirements in the [FAQ](../reference/faq.md)
