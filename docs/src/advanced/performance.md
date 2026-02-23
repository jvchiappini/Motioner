# Performance Optimization

Tips and techniques for optimizing Motioner's performance.

## Build Configuration

### Release Build is Essential

```powershell
# Always use release for production
cargo build --release
cargo run --release

# Development is 10-100x slower
cargo run  # ⚠️ Only for development
```

### Cargo.toml Optimization

```toml
[profile.release]
opt-level = 3           # Maximum optimization
lto = true              # Link-time optimization
codegen-units = 1       # Better optimization, slower compile
strip = true            # Remove debug symbols
panic = 'abort'         # Smaller binary size
```

### Aggressive Optimization

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
panic = 'abort'

[profile.release.package."*"]
opt-level = 3
```

## Runtime Performance

### Rendering Optimization

```rust
// ❌ Don't: Allocate in hot loop
for frame in 0..total_frames {
    let buffer = vec![0u8; width * height * 4];  // Bad!
    render_to_buffer(&mut buffer);
}

// ✅ Do: Reuse buffers
let mut buffer = vec![0u8; width * height * 4];
for frame in 0..total_frames {
    buffer.fill(0);  // Clear instead of allocating
    render_to_buffer(&mut buffer);
}
```

### Avoid Clones

```rust
// ❌ Don't: Clone unnecessarily
fn render_scene(scene: Scene) {  // Takes ownership, forces clone
    // ...
}

// ✅ Do: Use references
fn render_scene(scene: &Scene) {  // Borrow, no clone
    // ...
}
```

### Pre-allocate Collections

```rust
// ❌ Don't: Grow dynamically
let mut objects = Vec::new();
for _ in 0..1000 {
    objects.push(create_object());  // Reallocates multiple times
}

// ✅ Do: Pre-allocate capacity
let mut objects = Vec::with_capacity(1000);
for _ in 0..1000 {
    objects.push(create_object());  // No reallocation
}
```

## Frame Export Optimization

### Parallel Export

```rust
use rayon::prelude::*;

// Export frames in parallel (use all CPU cores)
fn export_parallel(scene: &Scene) -> Result<()> {
    (0..scene.duration_frames)
        .into_par_iter()
        .try_for_each(|frame| {
            let frame_buffer = scene.render_frame(frame);
            save_frame(frame_buffer, frame)
        })
}
```

### Batched Processing

```rust
// Process in chunks to balance memory and speed
const BATCH_SIZE: usize = 100;

for batch_start in (0..total_frames).step_by(BATCH_SIZE) {
    let batch_end = (batch_start + BATCH_SIZE).min(total_frames);
    
    for frame in batch_start..batch_end {
        // Process frame
    }
    
    // Clear caches, write to disk, etc.
}
```

## Memory Management

### Monitor Memory Usage

```rust
use sysinfo::{System, SystemExt};

fn check_memory() {
    let mut sys = System::new_all();
    sys.refresh_memory();
    
    //println!("Used memory: {} MB", sys.used_memory() / 1024 / 1024);
    //println!("Total memory: {} MB", sys.total_memory() / 1024 / 1024);
}
```

### Clear Unused Data

```rust
impl Scene {
    pub fn clear_cache(&mut self) {
        self.cached_frames.clear();
        self.cached_textures.clear();
    }
}

// Call periodically during long exports
if frame % 100 == 0 {
    scene.clear_cache();
}
```

## Profiling

### CPU Profiling

```powershell
# Install cargo-flamegraph
cargo install flamegraph

# Generate flamegraph
cargo flamegraph --release

# Opens flamegraph.svg in browser
```

### Benchmarking

```rust
#[cfg(test)]
mod benches {
    use super::*;
    use std::time::Instant;
    
    #[test]
    fn bench_render_frame() {
        let scene = create_test_scene();
        
        let start = Instant::now();
        for frame in 0..100 {
            scene.render_frame(frame);
        }
        let duration = start.elapsed();
        
        //println!("100 frames in {:?}", duration);
        //println!("Average: {:?} per frame", duration / 100);
    }
}
```

### Using Criterion

```toml
[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "rendering"
harness = false
```

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn render_benchmark(c: &mut Criterion) {
    let scene = create_test_scene();
    
    c.bench_function("render frame", |b| {
        b.iter(|| {
            scene.render_frame(black_box(0))
        });
    });
}

criterion_group!(benches, render_benchmark);
criterion_main!(benches);
```

## egui Performance

### Minimize Repaints

```rust
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Only request repaint when needed
        if self.is_playing {
            ctx.request_repaint();
        }
    }
}
```

### Efficient UI Updates

```rust
// ❌ Don't: Rebuild entire UI every frame
egui::CentralPanel::default().show(ctx, |ui| {
    for item in &self.large_list {  // Expensive!
        ui.label(format!("{:?}", item));
    }
});

// ✅ Do: Use ScrollArea and only show visible items
egui::CentralPanel::default().show(ctx, |ui| {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            // egui automatically culls non-visible items
            for item in &self.large_list {
                ui.label(format!("{:?}", item));
            }
        });
    });
});
```

## System Optimization

### Windows Performance

```powershell
# Set high performance power plan
powercfg /setactive 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c

# Disable Windows Defender realtime scanning temporarily
# (for file-intensive operations)
```

### File I/O

```rust
use std::io::BufWriter;

// ❌ Don't: Write directly
let file = File::create("output.png")?;
image.write_to(&mut file, image::ImageFormat::Png)?;

// ✅ Do: Use buffered writer
let file = File::create("output.png")?;
let writer = BufWriter::new(file);
image.write_to(&mut writer, image::ImageFormat::Png)?;
```

## Tips Summary

✅ **Do:**
- Use release builds
- Pre-allocate collections
- Reuse buffers
- Use references over clones
- Profile before optimizing
- Parallelize independent work

❌ **Don't:**
- Allocate in hot loops
- Clone unnecessarily
- Ignore profiling data
- Optimize prematurely
- Use unwrap() in production
- Block the main thread

## Performance Checklist

Before release:
- [ ] Compiled with `--release`
- [ ] Profiled for bottlenecks
- [ ] Removed debug logging
- [ ] Optimized hot paths
- [ ] Tested with large scenes
- [ ] Memory usage is reasonable
- [ ] No unnecessary allocations

## Next Steps

- [GPU Rendering](./gpu-rendering.md) — Ultimate performance
- [Architecture Guide](../developer-guide/architecture.md)
- [Building from Source](../developer-guide/building.md)
