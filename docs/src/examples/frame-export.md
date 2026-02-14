# Frame-by-Frame Export

Learn how to export animations as individual frames.

## Overview

Exporting frames gives you maximum flexibility for post-processing, custom encoding, or frame-by-frame analysis.

## Basic Export

```rust
use motioner_ui::*;
use std::path::PathBuf;

fn export_frames(scene: &Scene) -> Result<()> {
    let output_dir = PathBuf::from("output/frames");
    std::fs::create_dir_all(&output_dir)?;
    
    for frame in 0..scene.duration_frames {
        let frame_buffer = scene.render_frame(frame);
        let filename = format!("frame_{:05}.png", frame);
        let path = output_dir.join(filename);
        frame_buffer.save_png(&path)?;
        
        println!("Exported frame {}/{}", frame + 1, scene.duration_frames);
    }
    
    Ok(())
}
```

## Advanced Export with Progress

```rust
use indicatif::{ProgressBar, ProgressStyle};

fn export_with_progress(scene: &Scene) -> Result<()> {
    let output_dir = PathBuf::from("output/frames");
    std::fs::create_dir_all(&output_dir)?;
    
    // Create progress bar
    let pb = ProgressBar::new(scene.duration_frames as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
    );
    
    for frame in 0..scene.duration_frames {
        let frame_buffer = scene.render_frame(frame);
        let filename = format!("frame_{:05}.png", frame);
        let path = output_dir.join(filename);
        frame_buffer.save_png(&path)?;
        
        pb.inc(1);
        pb.set_message(format!("Frame {}", frame + 1));
    }
    
    pb.finish_with_message("Export complete!");
    Ok(())
}
```

## Parallel Export (Multi-threaded)

```rust
use rayon::prelude::*;

fn export_parallel(scene: &Scene) -> Result<()> {
    let output_dir = PathBuf::from("output/frames");
    std::fs::create_dir_all(&output_dir)?;
    
    // Export frames in parallel
    (0..scene.duration_frames)
        .into_par_iter()
        .try_for_each(|frame| {
            let frame_buffer = scene.render_frame(frame);
            let filename = format!("frame_{:05}.png", frame);
            let path = output_dir.join(filename);
            frame_buffer.save_png(&path)
        })?;
    
    Ok(())
}
```

## Custom Frame Range

```rust
fn export_range(scene: &Scene, start: usize, end: usize) -> Result<()> {
    let output_dir = PathBuf::from("output/frames");
    std::fs::create_dir_all(&output_dir)?;
    
    for frame in start..=end {
        let frame_buffer = scene.render_frame(frame);
        let filename = format!("frame_{:05}.png", frame);
        let path = output_dir.join(filename);
        frame_buffer.save_png(&path)?;
    }
    
    Ok(())
}

// Usage: Export frames 30-60
export_range(&scene, 30, 60)?;
```

## Different Image Formats

### JPEG Export

```rust
use image::{ImageFormat, RgbaImage};

fn export_as_jpeg(frame_buffer: &FrameBuffer, path: &Path) -> Result<()> {
    let img = RgbaImage::from_raw(
        frame_buffer.width,
        frame_buffer.height,
        frame_buffer.pixels.clone()
    ).unwrap();
    
    img.save_with_format(path, ImageFormat::Jpeg)?;
    Ok(())
}
```

### WebP Export

```rust
fn export_as_webp(frame_buffer: &FrameBuffer, path: &Path) -> Result<()> {
    let img = RgbaImage::from_raw(
        frame_buffer.width,
        frame_buffer.height,
        frame_buffer.pixels.clone()
    ).unwrap();
    
    img.save_with_format(path, ImageFormat::WebP)?;
    Ok(())
}
```

## Error Handling

```rust
fn export_with_error_handling(scene: &Scene) -> Result<()> {
    let output_dir = PathBuf::from("output/frames");
    
    // Ensure directory exists
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        eprintln!("Failed to create output directory: {}", e);
        return Err(e.into());
    }
    
    for frame in 0..scene.duration_frames {
        match scene.render_frame(frame) {
            Ok(frame_buffer) => {
                let filename = format!("frame_{:05}.png", frame);
                let path = output_dir.join(filename);
                
                if let Err(e) = frame_buffer.save_png(&path) {
                    eprintln!("Failed to save frame {}: {}", frame, e);
                    return Err(e.into());
                }
            }
            Err(e) => {
                eprintln!("Failed to render frame {}: {}", frame, e);
                return Err(e);
            }
        }
    }
    
    Ok(())
}
```

## Performance Tips

1. **Use Release Build**
   ```powershell
   cargo run --release
   ```

2. **Parallel Processing**
   - Use Rayon for CPU-intensive rendering
   - Balance thread count with available cores

3. **Disk I/O**
   - Use SSD for faster write speeds
   - Consider writing to RAM disk for maximum speed

4. **Memory Management**
   - Clear frame buffers after writing
   - Process in batches if memory limited

## Next Steps

- [FFmpeg Integration](./ffmpeg-integration.md) — Convert frames to video
- [Exporting Guide](../user-guide/export.md) — Complete export workflow
