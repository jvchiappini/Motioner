# Troubleshooting

Common issues and their solutions.

## Installation Issues

### Rust Installation Fails

**Problem:** `rustup` installation fails on Windows

**Solution:**
1. Download Visual Studio Build Tools
2. Install "Desktop development with C++"
3. Restart and try again

### FFmpeg Not Found

**Problem:** `Error: FFmpeg executable not found`

**Solution:**
```powershell
# 1. Download FFmpeg
# Visit: https://ffmpeg.org/download.html

# 2. Extract to C:\ffmpeg

# 3. Add to PATH
$env:Path += ";C:\ffmpeg\bin"

# 4. Verify
ffmpeg -version
```

## Build Issues

### Compilation Errors

**Problem:** `error: linking with 'link.exe' failed`

**Solution:**
```powershell
# Install Visual Studio Build Tools
# Restart terminal
cargo clean
cargo build --release
```

### Dependency Issues

**Problem:** `error: failed to fetch dependencies`

**Solution:**
```powershell
# Update cargo index
cargo update

# Clear cache and rebuild
cargo clean
rm -r ~/.cargo/registry
cargo build
```

## Runtime Issues

### Application Won't Start

**Problem:** Application crashes on startup

**Solution:**
1. Check console output for errors
2. Ensure graphics drivers are updated
3. Try with `RUST_BACKTRACE=1`:
   ```powershell
   $env:RUST_BACKTRACE=1
   cargo run --release
   ```

### Slow Performance

**Problem:** Application is very slow

**Solution:**
1. **Use release build:**
   ```powershell
   cargo run --release  # Not cargo run
   ```

2. **Close other applications**
3. **Check task manager** for CPU/memory usage
4. **Update graphics drivers**

### High Memory Usage

**Problem:** Memory usage keeps growing

**Solution:**
- Clear cache periodically
- Process frames in batches
- Check for memory leaks (open an issue if persistent)

## Export Issues

### Export Fails

**Problem:** Video export fails without error

**Solution:**
1. Check FFmpeg is installed: `ffmpeg -version`
2. Verify output directory exists
3. Check disk space
4. Try manual frame export first

### FFmpeg Encoding Errors

**Problem:** `Error: FFmpeg encoding failed`

**Solution:**
```powershell
# Check FFmpeg works
ffmpeg -version

# Try manual encoding
ffmpeg -framerate 30 -i out/frames/frame_%05d.png output.mp4

# Check console output for specific error
```

### Frames Missing

**Problem:** Some frames are missing from export

**Solution:**
1. Check frame numbering is sequential
2. Verify no errors during frame rendering
3. Check disk space
4. Try export with smaller range

## UI Issues

### UI Too Small/Large

**Problem:** UI elements are the wrong size

**Solution:**
```rust
// Future feature: UI scaling
// Will be configurable in settings
```

### Panel Layout Issues

**Problem:** Panels overlap or aren't visible

**Solution:**
- Resize window
- Reset layout (View → Reset Layout)
- Restart application

## Platform-Specific

### Windows

**DLL Not Found:**
```powershell
# Install Visual C++ Redistributable
# Download from Microsoft
```

### Linux

**Missing Libraries:**
```bash
# Ubuntu/Debian
sudo apt install libgtk-3-dev libx11-dev

# Fedora
sudo dnf install gtk3-devel libX11-devel
```

### macOS

**Code Signing Issues:**
```bash
# Allow unsigned application
xattr -cr /path/to/Motioner.app
```

## Getting Help

Still having issues?

1. **Check Documentation:** Read relevant guides
2. **Search Issues:** [GitHub Issues](https://github.com/jvchiappini/Motioner/issues)
3. **Open New Issue:** Provide:
   - OS and version
   - Rust version (`rustc --version`)
   - Complete error message
   - Steps to reproduce

## Debugging Tips

### Enable Logging

```powershell
$env:RUST_LOG="info"
cargo run --release
```

### Get Backtrace

```powershell
$env:RUST_BACKTRACE="full"
cargo run --release
```

### Build with Debug Info

```powershell
cargo build --release --config profile.release.debug=true
```

## Known Issues

- Issue #1: Description and workaround
- Issue #2: Description and workaround

_(Will be updated as issues are discovered)_

---

**Need more help?** Visit [GitHub Discussions](https://github.com/jvchiappini/Motioner/discussions)
