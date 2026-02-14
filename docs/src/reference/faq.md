# Frequently Asked Questions

## General

### What is Motioner?

Motioner is a lightweight animation editor and prototyping tool built in Rust. It allows you to create programmatic animations, preview them in real-time, and export them as video or image sequences.

### Is Motioner free?

Yes! Motioner is open-source software licensed under Apache 2.0. You can use it freely for any purpose.

### What platforms are supported?

- Windows (fully supported)
- Linux (supported)
- macOS (supported)

## Features

### Can I import images/videos?

Not yet. This feature is planned for a future release. Currently, Motioner focuses on programmatic animations.

### Does it support audio?

Audio support is planned for Phase 3 of the roadmap but not currently implemented.

### Can I export transparent backgrounds?

Yes, when exporting PNG sequences. For video, you'll need to use appropriate codecs (VP9 with alpha, ProRes, etc.).

## Technical

### Why Rust?

Rust provides:
- High performance
- Memory safety
- Great tooling
- Excellent GUI libraries (egui)

### What are the system requirements?

**Minimum:**
- CPU: Dual-core 2GHz
- RAM: 4GB
- GPU: Any with OpenGL 3.3+

**Recommended:**
- CPU: Quad-core 3GHz+
- RAM: 8GB+
- GPU: Dedicated graphics card

### Is GPU acceleration supported?

Not yet, but it's a high priority feature. The architecture is already designed with GPU rendering in mind (wgpu integration planned).

## Usage

### How do I get started?

1. Install Rust and FFmpeg
2. Clone the repository
3. Run `cargo run --release`
4. Check the [Getting Started Guide](../user-guide/getting-started.md)

### Why is it so slow?

Make sure you're using the release build:
```powershell
cargo run --release  # Fast
# NOT: cargo run      # Very slow
```

### How do I export higher quality?

```powershell
# Use lower CRF value (higher quality)
ffmpeg -framerate 30 -i frames/frame_%05d.png `
  -c:v libx264 -crf 18 high_quality.mp4
```

## Development

### Can I contribute?

Absolutely! Read the [Contributing Guide](../developer-guide/contributing.md) to get started.

### What skills do I need to contribute?

- **Rust programming** — For core features
- **WGSL/shaders** — For GPU features
- **Documentation** — Always welcome!
- **Testing** — Help find bugs

### How do I report bugs?

Open an issue on GitHub with:
- Clear description
- Steps to reproduce
- System information
- Error messages

### How can I request features?

Use the feature request template on GitHub Issues. Describe:
- What you want to achieve
- Why it's valuable
- How it could work

## Roadmap

### When will GPU rendering be available?

GPU rendering is planned for Phase 2 (no specific date yet). Follow the repository for updates!

### Will there be a visual editor?

Yes! Visual scene editing is planned but in early design phase. Current focus is on core features and stability.

### Any plans for scripting?

Yes, a scripting API is planned for Phase 4 along with a plugin system.

## Troubleshooting

### FFmpeg not found?

See the [Troubleshooting Guide](./troubleshooting.md#ffmpeg-not-found) for solutions.

### Build fails?

Check:
1. Rust is up to date: `rustup update`
2. Visual Studio Build Tools (Windows)
3. Required libraries (Linux)

### High memory usage?

This is being optimized. For now:
- Process frames in batches
- Clear cache periodically
- Use release build

## Community

### Is there a Discord/forum?

Currently, use [GitHub Discussions](https://github.com/jvchiappini/Motioner/discussions) for community interaction.

### How can I stay updated?

- ⭐ Star the repository
- 👁️ Watch for releases
- 📬 Check Discussions regularly

### Can I use Motioner commercially?

Yes! The Apache 2.0 license allows commercial use.

## Comparison

### How does it compare to After Effects?

Motioner is not trying to replace professional tools like After Effects. It's focused on:
- Programmatic animations
- Rapid prototyping
- Developer-friendly workflows
- Open-source and free

### What about Blender?

Blender is a full 3D suite. Motioner is specifically for 2D motion graphics with a programming-first approach.

### Why not just use code?

Motioner provides:
- Visual preview
- Timeline editing
- One-click export
- Easier iteration

---

**Have more questions?**

- 💬 [Ask in Discussions](https://github.com/jvchiappini/Motioner/discussions)
- 📖 [Read the Documentation](../introduction.md)
- 🐛 [Report Issues](https://github.com/jvchiappini/Motioner/issues)
