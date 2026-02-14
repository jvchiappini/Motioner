# FFmpeg Integration

Learn how to use FFmpeg to encode your frame sequences into video files.

## Basic Video Encoding

### PNG Sequence to MP4

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libx264 -pix_fmt yuv420p output.mp4
```

**Parameters explained:**
- `-framerate 30` — Input frame rate (30 FPS)
- `-i out/frames/frame_%05d.png` — Input pattern (00001, 00002, etc.)
- `-c:v libx264` — Use H.264 codec
- `-pix_fmt yuv420p` — Pixel format (compatible with most players)
- `output.mp4` — Output filename

## Quality Settings

### High Quality

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libx264 -crf 18 -preset slow -pix_fmt yuv420p high_quality.mp4
```

- `crf 18` — Constant Rate Factor (0-51, lower = better quality)
- `preset slow` — Slower encoding, better compression

### Balanced Quality

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libx264 -crf 23 -preset medium -pix_fmt yuv420p balanced.mp4
```

### Web Optimized

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libx264 -crf 28 -preset fast -pix_fmt yuv420p `
  -movflags +faststart web.mp4
```

- `movflags +faststart` — Optimize for web streaming

## Different Output Formats

### WebM (VP9)

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libvpx-vp9 -crf 30 -b:v 0 output.webm
```

### GIF Animation

```powershell
ffmpeg -framerate 15 -i out/frames/frame_%05d.png `
  -vf "fps=15,scale=800:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse" `
  output.gif
```

### Apple ProRes (High Quality)

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v prores_ks -profile:v 3 output.mov
```

## Advanced Techniques

### Two-Pass Encoding

Better quality/size ratio:

```powershell
# First pass
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libx264 -b:v 2M -pass 1 -f null NUL

# Second pass
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libx264 -b:v 2M -pass 2 output.mp4
```

### With Alpha Channel (Transparent)

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v png output.mov
```

or with VP9:

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -c:v libvpx-vp9 -pix_fmt yuva420p output.webm
```

### Custom Resolution

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -vf "scale=1280:720" -c:v libx264 -crf 23 hd720.mp4
```

### Maintain Aspect Ratio

```powershell
# Scale to 1920 width, keep aspect ratio
ffmpeg -framerate 30 -i out/frames/frame_%05d.png `
  -vf "scale=1920:-1" -c:v libx264 -crf 23 scaled.mp4
```

## Rust Integration

### Call FFmpeg from Rust

```rust
use std::process::Command;
use std::path::Path;

pub struct VideoExporter {
    pub fps: u32,
    pub input_pattern: String,
    pub output_path: String,
}

impl VideoExporter {
    pub fn new(fps: u32, frames_dir: &Path, output: &Path) -> Self {
        let input_pattern = frames_dir
            .join("frame_%05d.png")
            .to_string_lossy()
            .to_string();
        
        let output_path = output.to_string_lossy().to_string();
        
        Self {
            fps,
            input_pattern,
            output_path,
        }
    }
    
    pub fn export(&self) -> Result<(), String> {
        let status = Command::new("ffmpeg")
            .args([
                "-y", // Overwrite output
                "-framerate", &self.fps.to_string(),
                "-i", &self.input_pattern,
                "-c:v", "libx264",
                "-crf", "23",
                "-pix_fmt", "yuv420p",
                &self.output_path,
            ])
            .status()
            .map_err(|e| format!("Failed to execute FFmpeg: {}", e))?;
        
        if status.success() {
            Ok(())
        } else {
            Err("FFmpeg encoding failed".to_string())
        }
    }
}

// Usage
let exporter = VideoExporter::new(
    30,
    Path::new("out/frames"),
    Path::new("output.mp4")
);

exporter.export()?;
```

### With Progress Feedback

```rust
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};

pub fn export_with_progress(
    fps: u32,
    input: &str,
    output: &str,
) -> Result<(), String> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-y",
            "-framerate", &fps.to_string(),
            "-i", input,
            "-c:v", "libx264",
            "-crf", "23",
            "-pix_fmt", "yuv420p",
            "-progress", "pipe:1",
            output,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg: {}", e))?;
    
    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);
    
    for line in reader.lines() {
        if let Ok(line) = line {
            if line.starts_with("frame=") {
                println!("FFmpeg: {}", line);
            }
        }
    }
    
    let status = child.wait()
        .map_err(|e| format!("Failed to wait for FFmpeg: {}", e))?;
    
    if status.success() {
        Ok(())
    } else {
        Err("FFmpeg encoding failed".to_string())
    }
}
```

## Troubleshooting

### FFmpeg Not Found

```powershell
# Check if FFmpeg is installed
ffmpeg -version

# Windows: Add to PATH
# 1. Download from https://ffmpeg.org/download.html
# 2. Extract to C:\ffmpeg
# 3. Add C:\ffmpeg\bin to System PATH
# 4. Restart terminal
```

### Encoding Fails

```powershell
# Check frame sequence
dir out\frames\

# Ensure continuous numbering
# frame_00001.png, frame_00002.png, etc.

# Test with verbose output
ffmpeg -v verbose -framerate 30 -i out/frames/frame_%05d.png output.mp4
```

### Quality Issues

```powershell
# Increase quality (lower CRF)
ffmpeg -framerate 30 -i frames/frame_%05d.png `
  -c:v libx264 -crf 15 -preset slow high_quality.mp4

# For archival quality
ffmpeg -framerate 30 -i frames/frame_%05d.png `
  -c:v ffv1 -level 3 lossless.mkv
```

## Next Steps

- [Export Guide](../user-guide/export.md)
- [Basic Animation Example](./basic-animation.md)
- [Performance Optimization](../advanced/performance.md)
