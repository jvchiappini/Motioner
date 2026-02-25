<div align="center">

# 🎬 Motioner

### **Next‑generation Animation Editor & Prototyping Suite**

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/jvchiappini/Motioner/pulls)

**Create. Animate. Export.**

*Lightning‑fast, modular animation editor written in Rust.*

[Highlights](#-highlights) • [Quick start](#-quick-start-development) • [Docs](#-documentation) • [Roadmap](#-roadmap)

---

</div>

## 📸 Preview & Demos

> **Coming soon:** live demos, animated GIFs and video previews.

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

## ✨ Highlights

### 🚀 Core Capabilities
* **🎨 Modern UI** – immediate‑mode graphics powered by `egui`/`eframe`.
* **⏱️ Robust timeline** with panning, zooming, playhead, tracks and keyframe support.
* **👁️ Live preview** – edit and see results instantly in the canvas.
* **💾 Frame exporter** (PNG sequence) and built‑in `ffmpeg` video helper.
* **🔌 Modular design** for easy extension and reuse.

### 🛠️ Developer-Friendly
* **🦀 Pure Rust** – no unsafe dependencies, cargo-based build.
* **🔧 Clean module layout** (ui, canvas, dsl, timeline, etc.).
* **⚡ GPU‑ready** – groundwork laid for `wgpu` rendering.
* **Zero‑config**: `cargo run` boots in seconds.

### 🎯 Perfect For
* Code‑driven animation workflows
* Motion‑graphics prototyping
* Frame‑by‑frame editing and rotoscoping
* Export pipelines for VFX / animation studios

---

## 🚀 Quick start (development)

### Prerequisites

| Tool | Purpose | Installation |
|------|---------|--------------|
| **Rust** (1.70+) | build & run | [rustup](https://rustup.rs/) |
| **ffmpeg** (optional) | encode video | https://ffmpeg.org/download.html |

### Getting started

```powershell
# clone repository
git clone https://github.com/jvchiappini/Motioner.git
cd Motioner

# quick dev run (hot rebuilds)
cargo run

# optimized release build
cargo run --release
```

### 🎬 Typical workflow
1. Launch the app.
2. Configure scene, duration, FPS, easing and shapes.
3. Use timeline & canvas to keyframe actions.
4. Preview live and export frames or video.

```
# manual encode (optional)
ffmpeg -framerate 30 -i out/frames/frame_%05d.png \
    -c:v libx264 -pix_fmt yuv420p output.mp4
```

---

## 📦 Releases & downloads

Pre-built binaries are published automatically by GitHub Actions for
Windows, macOS and Linux when a `v*` tag is pushed (e.g. `v1.2.3`). Each
release includes:

* Platform-specific archive (zip for Windows, tar.gz for macOS/Linux)
* SHA256 checksum files
* Release notes listing commits included

**Creating a release:**

```powershell
# annotate and push a version tag (Actions will publish assets)
git tag -a v1.2.3 -m "release v1.2.3"
git push origin v1.2.3
```

Asset naming convention examples:

* `motioner_v1.2.3_windows_x86_64.zip`
* `motioner_v1.2.3_macos_x86_64.tar.gz`
* `motioner_v1.2.3_linux_x86_64.tar.gz`

---

## 📁 Project structure

```
Motioner/
├── Cargo.toml             # dependencies & metadata
├── rust-toolchain.toml    # pinned toolchain
├── LICENSE
├── README.md
├── assets/                # icons, fonts, etc.
├── docs/                  # mdBook documentation
├── src/                   # source code
│   ├── main.rs            # entry point
│   ├── app_state.rs       # global state
│   ├── canvas.rs          # drawing helpers
│   ├── scene.rs           # scene model
│   ├── logo.rs            # icon loader
│   ├── timeline/          # timeline module
│   │   └── mod.rs
│   ├── ui.rs              # UI layout & panels
│   ├── code_panel/        # code editor widgets
│   ├── dsl/               # animation DSL (lexer/parser/runtime)
│   ├── events/            # event definitions
│   ├── logics/            # if/for logic blocks
│   ├── modals/            # popup dialogs
│   └── states/            # helpers (autosave, dslstate)
└── target/                # build artifacts
```

---

## 🛠️ Development

### Build & tooling

```powershell
# format & lint
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings

# build
cargo build
cargo build --release

# run tests (TODO: add tests)
cargo test
```

### Code Quality

This project follows Rust best practices:

* ✅ Format code with `rustfmt`
* ✅ Lint with `clippy`
* ✅ Use semantic commit messages
* ✅ Write tests for new features

---

## 🤝 Contributing

We welcome contributions! Here's how you can help:

### 🐛 Reporting bugs

Open an [issue](https://github.com/jvchiappini/Motioner/issues) with:

* Clear description
* Steps to reproduce
* Expected vs actual behavior

### 💡 Suggest features

Share ideas via [issues](https://github.com/jvchiappini/Motioner/issues)
or [discussions](https://github.com/jvchiappini/Motioner/discussions).

### 🔧 Pull requests

```powershell
# fork & clone
git clone https://github.com/YOUR_USERNAME/Motioner.git

# create a branch
git checkout -b feat/amazing-feature

# make your changes and commit
git commit -m "feat: add amazing feature"

# push & open a PR
git push origin feat/amazing-feature
```

**Branch guidelines:**

* `feat/` — new features
* `fix/` — bug fixes
* `docs/` — documentation updates
* `refactor/` — code restructuring

---

## 🗺️ Roadmap

### 🎯 Phase 1 – Core features (current)

* [x] Basic timeline editor
* [x] Frame-by-frame export
* [x] FFmpeg integration
* [x] Live preview

### 🚀 Phase 2 – Enhanced editing

* [ ] GPU-accelerated rendering with `wgpu`
* [ ] Advanced keyframe editor
* [ ] Easing functions and curves
* [ ] Layer system

### 🎨 Phase 3 – Professional tools

* [ ] Audio track support
* [ ] Effects and filters
* [ ] Export presets
* [ ] Project file format (.motioner)

### 🌟 Phase 4 – Advanced / long‑term

* [ ] Plugin system
* [ ] Scripting API
* [ ] Real-time collaboration
* [ ] Cloud export options

---

## 📚 Documentation

Comprehensive documentation is built using mdBook.

### 📖 Read online
Documentation will be published to GitHub Pages soon.

### 🏗️ Build locally

```powershell
# install mdBook
cargo install mdbook

# serve docs
cd docs
mdbook serve --open
```

**Sections include:** user guide, developer guide, examples,
advanced topics, and reference.

### Quick links

* 📖 [Docs source](./docs)
* 💻 [Source code on GitHub](https://github.com/jvchiappini/Motioner/tree/main/src)
* 💬 [Discussions](https://github.com/jvchiappini/Motioner/discussions)
* 🐛 [Issues](https://github.com/jvchiappini/Motioner/issues)

---

## 📄 License

This project is licensed under the **Apache License 2.0** – see
[LICENSE](./LICENSE) for details.

---

## 👨‍💻 Author & Maintainer

**José Valentino Chiappini**

* GitHub: [@jvchiappini](https://github.com/jvchiappini)
* Project: https://github.com/jvchiappini/Motioner

---

## 🙏 Acknowledgments

Built with amazing open-source technologies:

* [Rust](https://www.rust-lang.org/)
* [egui](https://github.com/emilk/egui)
* [wgpu](https://wgpu.rs/)
* [FFmpeg](https://ffmpeg.org/)

---

<div align="center">

**⭐ Star the repo if you like it!**

Made with ❤️ and Rust 🦀

[Report a bug](https://github.com/jvchiappini/Motioner/issues) •
[Request a feature](https://github.com/jvchiappini/Motioner/issues) •
[View roadmap](#-roadmap)

</div>


