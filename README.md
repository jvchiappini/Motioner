<div align="center">

# 🎬 Motioner

### **Modern Animation Editor & Prototyping Tool**

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/jvchiappini/Motioner/pulls)

**Create. Animate. Export.**

*A lightweight, high-performance animation editor built in Rust*

[Features](#-features) • [Quick Start](#-quick-start) • [Documentation](#-documentation) • [Roadmap](#-roadmap)

---

</div>

## 📸 Preview

> **Coming Soon**: Screenshots, demos, and GIF previews will be added here

```
┌─────────────────────────────────────────┐
│  🎥  Motioner - Animation Editor        │
├─────────────────────────────────────────┤
│                                         │
│   [Preview demos and screenshots]       │
│   [Interactive timeline showcase]       │
│   [Export workflow visualization]       │
│                                         │
└─────────────────────────────────────────┘
```

---

## ✨ Features

### 🚀 **Core Capabilities**
- **🎨 Intuitive UI** — Fast, responsive interface built with `egui` and `eframe`
- **⏱️ Timeline Editor** — Interactive timeline for precise animation control
- **👁️ Live Preview** — Real-time visualization of your animations
- **💾 Frame Export** — Export animations as PNG sequences
- **🎞️ Video Encoding** — Automatic MP4 generation via `ffmpeg` integration

### 🛠️ **Developer-Friendly**
- **🦀 Pure Rust** — Modern, safe, and performant codebase
- **🔌 Modular Architecture** — Easy to extend and customize
- **⚡ GPU-Ready** — Prepared for `wgpu` GPU rendering integration
- **📦 Zero-Config Build** — Just `cargo run` and you're ready

### 🎯 **Perfect For**
- Creating programmatic animations
- Rapid prototyping of motion graphics
- Frame-by-frame animation workflows
- Post-production pipelines requiring image sequences

---

## 🚀 Quick Start

### Prerequisites

| Tool | Purpose | Installation |
|------|---------|--------------|
| **Rust** (stable) | Build and run | [Install rustup](https://rustup.rs/) |
| **ffmpeg** | Video encoding | [Download ffmpeg](https://ffmpeg.org/download.html) |

### Installation

```powershell
# Clone the repository
git clone https://github.com/jvchiappini/Motioner.git
cd Motioner

# Run in development mode
cargo run

# Or build optimized release version
cargo run --release
```

### 🎬 Basic Workflow

1. **Launch** the application
2. **Configure** your animation (FPS, duration, scene settings)
3. **Preview** in real-time
4. **Export** to video or image sequence

```powershell
# The app handles everything, or manually encode with:
ffmpeg -framerate 30 -i out/frames/frame_%05d.png -c:v libx264 -pix_fmt yuv420p output.mp4
```

---

## 📦 Releases & downloads

Pre-built binaries are published automatically by GitHub Actions for Windows, macOS and Linux when a `v*` tag is pushed (e.g. `v1.2.3`). Each Release includes:

- Platform-specific archive (zip for Windows, tar.gz for macOS/Linux)
- SHA256 checksum files (one per asset)
- Release notes that include the list of commits contained in that tag

How to create a release (recommended):

```powershell
# create an annotated tag and push it (Actions will publish assets)
git tag -a v1.2.3 -m "release v1.2.3"
git push origin v1.2.3
```

Asset naming convention (examples):

- `motioner_v1.2.3_windows_x86_64.zip`
- `motioner_v1.2.3_macos_x86_64.tar.gz`
- `motioner_v1.2.3_linux_x86_64.tar.gz`

Tip: check the Release page on GitHub to view release notes and download platform assets.


---

## 📁 Project Structure

```
Motioner/
├── 📄 Cargo.toml              # Project dependencies and metadata
├── 📄 LICENSE                 # MIT License
├── 📄 README.md              # You are here!
├── 📄 rust-toolchain.toml    # Rust version specification
├── 📂 assets/                # Application assets and resources
├── 📂 src/                   # Source code
│   ├── 🦀 main.rs           # Application entry point
│   ├── 🦀 app_state.rs      # Application state management
│   ├── 🦀 canvas.rs         # Drawing canvas implementation
│   ├── 🦀 timeline.rs       # Timeline editor logic
│   ├── 🦀 scene.rs          # Scene management
│   ├── 🦀 renderer.rs       # Rendering engine
│   ├── 🦀 ui.rs             # UI components
│   └── 📂 animations/       # Animation presets and templates
└── 📂 target/               # Build artifacts (auto-generated)
```

---

## 🛠️ Development

### Essential Commands

```powershell
# Format code
cargo fmt

# Run linter
cargo clippy

# Build release version
cargo build --release

# Run tests (when available)
cargo test
```

### Code Quality

This project follows Rust best practices:
- ✅ Format code with `rustfmt`
- ✅ Lint with `clippy`
- ✅ Use semantic commit messages
- ✅ Write tests for new features

---

## 🤝 Contributing

We welcome contributions! Here's how you can help:

### 🐛 Report Bugs
Open an [issue](https://github.com/jvchiappini/Motioner/issues) with:
- Clear description
- Steps to reproduce
- Expected vs actual behavior

### 💡 Suggest Features
Share your ideas via [issues](https://github.com/jvchiappini/Motioner/issues) or [discussions](https://github.com/jvchiappini/Motioner/discussions)

### 🔧 Submit Pull Requests

```powershell
# 1. Fork and clone
git clone https://github.com/YOUR_USERNAME/Motioner.git

# 2. Create a feature branch
git checkout -b feat/amazing-feature

# 3. Make your changes and commit
git commit -m "feat: add amazing feature"

# 4. Push and open a PR
git push origin feat/amazing-feature
```

**Branch naming conventions:**
- `feat/` — New features
- `fix/` — Bug fixes
- `docs/` — Documentation updates
- `refactor/` — Code refactoring

---

## 🗺️ Roadmap

### 🎯 **Phase 1: Core Features** (Current)
- [x] Basic timeline editor
- [x] Frame-by-frame export
- [x] FFmpeg integration
- [x] Live preview

### 🚀 **Phase 2: Enhanced Editing**
- [ ] GPU-accelerated rendering with `wgpu`
- [ ] Advanced keyframe editor
- [ ] Easing functions and curves
- [ ] Layer system

### 🎨 **Phase 3: Professional Tools**
- [ ] Audio track support
- [ ] Effects and filters
- [ ] Export presets
- [ ] Project file format (.motioner)

### 🌟 **Phase 4: Advanced Features**
- [ ] Plugin system
- [ ] Scripting API
- [ ] Real-time collaboration
- [ ] Cloud export options

---

## 📚 Documentation

Comprehensive documentation is now available using mdBook!

### 📖 Read Online
Documentation will be automatically published to GitHub Pages (coming soon).

### 🏗️ Build Locally

```powershell
# Install mdBook
cargo install mdbook

# Build and serve documentation
cd docs
mdbook serve --open
```

Documentation includes:
- 📘 **User Guide** — Getting started, interface, animations, and export
- 🛠️ **Developer Guide** — Architecture, building, API reference, and contributing
- 🚀 **Advanced Topics** — GPU rendering, custom animations, performance
- 💡 **Examples** — Practical code examples and tutorials
- 📋 **Reference** — Shortcuts, configuration, troubleshooting, and FAQ

### Quick Links
- 📖 [Documentation Source](./docs/) — Browse documentation files
- 💻 [Code Documentation](https://github.com/jvchiappini/Motioner/tree/main/src) — Well-commented source code
- 💬 [Discussions](https://github.com/jvchiappini/Motioner/discussions) — Community Q&A
- 🐛 [Issues](https://github.com/jvchiappini/Motioner/issues) — Report bugs and request features

---

## 📄 License

This project is licensed under the **Apache License 2.0** - see the [LICENSE](./LICENSE) file for details.

---

## 👨‍💻 Author & Maintainer

**José Valentino Chiappini**
- GitHub: [@jvchiappini](https://github.com/jvchiappini)
- Project: [Motioner](https://github.com/jvchiappini/Motioner)

---

## 🙏 Acknowledgments

Built with amazing open-source technologies:
- [Rust](https://www.rust-lang.org/) — Systems programming language
- [egui](https://github.com/emilk/egui) — Immediate mode GUI framework
- [wgpu](https://wgpu.rs/) — GPU graphics API
- [FFmpeg](https://ffmpeg.org/) — Multimedia framework

---

<div align="center">

**⭐ Star this repo if you find it useful!**

Made with ❤️ and Rust 🦀

[Report Bug](https://github.com/jvchiappini/Motioner/issues) • [Request Feature](https://github.com/jvchiappini/Motioner/issues) • [View Roadmap](#-roadmap)

</div>


