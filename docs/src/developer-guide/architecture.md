# Architecture

Understanding Motioner's architecture will help you contribute effectively and extend its capabilities.

## High-Level Overview

```
┌─────────────────────────────────────────────────┐
│                   main.rs                       │
│              (Application Entry)                │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│              app_state.rs                       │
│         (Central State Manager)                 │
└─────┬─────────┬─────────┬─────────┬────────────┘
      │         │         │         │
┌─────▼──┐ ┌───▼────┐ ┌──▼─────┐ ┌▼────────────┐
│ ui.rs  │ │ scene  │ │timeline│ │  renderer   │
│        │ │        │ │        │ │             │
└────────┘ └────────┘ └────────┘ └─────────────┘
```

## Core Components

### main.rs
**Entry Point**
- Initializes eframe application
- Sets up window configuration
- Starts event loop

### app_state.rs
**State Management**
- Central application state
- Handles user interactions
- Manages scene data
- Coordinates between components

### ui.rs
**User Interface**
- Built with egui immediate-mode GUI
- Renders all UI panels
- Handles user input
- Updates app state based on interactions

### scene.rs
**Scene Management**
- Defines scene structure
- Manages scene objects
- Handles scene hierarchy
- Animation data

### timeline.rs
**Timeline Editor**
- Frame management
- Playback controls
- Keyframe tracking
- Time-based operations

### canvas.rs
**Drawing Canvas**
- Renders scene preview
- Handles canvas interactions
- Coordinate transformations
- Visual feedback

### renderer.rs
**Rendering Engine**
- Frame rendering
- Export pipeline
- GPU integration point (future)
- Image generation

## Data Flow

### User Interaction Flow

```
User Input → UI Layer → App State → Scene/Timeline → Renderer → Display
                ↑                                                   │
                └───────────────────────────────────────────────────┘
```

### Export Flow

```
App State → Scene Data → Renderer → Frame Buffer → PNG Files → FFmpeg → MP4
```

## Module Dependencies

```rust
// Dependency hierarchy
main
├── app_state
│   ├── ui
│   ├── scene
│   ├── timeline
│   ├── canvas
│   └── renderer
├── project_settings
└── welcome_modal
```

## Key Design Patterns

### Immediate Mode GUI (egui)

Benefits:
- Simple state management
- No retained UI tree
- Declarative syntax
- Fast iteration

Example:
```rust
ui.horizontal(|ui| {
    if ui.button("Play").clicked() {
        app_state.play();
    }
    if ui.button("Stop").clicked() {
        app_state.stop();
    }
});
```

### State-Driven Rendering

All rendering derives from app state:
```rust
pub struct AppState {
    pub scene: Scene,
    pub current_frame: usize,
    pub is_playing: bool,
    // ...
}
```

### Modular Architecture

Each component is self-contained:
- Clear responsibilities
- Minimal coupling
- Easy to test
- Simple to extend

## GPU Rendering Integration (Future)

### Current: CPU Rendering
```
Scene Data → CPU Compute → Frame Buffer → PNG
```

### Planned: GPU Acceleration
```
Scene Data → wgpu Pipeline → GPU Shaders → Frame Buffer → PNG
```

Integration points:
- `renderer.rs` — Add wgpu backend
- `composition.wgsl` — WGSL shaders already present
- `canvas.rs` — GPU texture rendering

## Performance Considerations

### Optimization Strategies

**Current:**
- Efficient egui rendering
- Frame-by-frame export
- Release builds

**Future:**
- GPU-accelerated rendering
- Multi-threaded export
- Render caching
- Incremental updates

### Memory Management

- Rust's ownership system prevents memory leaks
- No garbage collector overhead
- Explicit resource management
- Zero-cost abstractions

## Extension Points

### Adding New Animation Types

1. Define in `src/animations/`
2. Register in scene system
3. Add UI controls
4. Implement rendering

### Custom Exporters

1. Implement export trait
2. Add to renderer pipeline
3. Register in UI
4. Handle format specifics

### Plugin System (Future)

Planned architecture:
```rust
trait MotionerPlugin {
    fn init(&mut self, app: &mut AppState);
    fn update(&mut self, app: &mut AppState);
    fn render(&self, frame: &mut FrameBuffer);
}
```

## Next Steps

- [Building from Source](./building.md)
- [Project Structure](./project-structure.md)
- [Contributing](./contributing.md)
