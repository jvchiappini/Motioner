# API Reference

Core APIs and interfaces in Motioner.

## Application State

### AppState

The central state manager for the entire application.

```rust
pub struct AppState {
    pub scene: Scene,
    pub current_frame: usize,
    pub is_playing: bool,
    pub fps: f32,
    // ... other fields
}

impl AppState {
    pub fn new() -> Self;
    pub fn update(&mut self);
    pub fn play(&mut self);
    pub fn pause(&mut self);
    pub fn export_video(&mut self);
}
```

## Scene Management

### Scene

Represents an animation scene with objects and timeline.

```rust
pub struct Scene {
    pub objects: Vec<SceneObject>,
    pub duration_frames: usize,
    pub width: u32,
    pub height: u32,
}

impl Scene {
    pub fn new() -> Self;
    pub fn add_object(&mut self, obj: SceneObject);
    pub fn remove_object(&mut self, id: usize);
    pub fn render_frame(&self, frame: usize) -> FrameBuffer;
}
```

### SceneObject

Individual animated object in the scene.

```rust
pub struct SceneObject {
    pub id: usize,
    pub name: String,
    pub position: (f32, f32),
    pub scale: (f32, f32),
    pub rotation: f32,
    pub opacity: f32,
}
```

## Timeline

### Timeline

Manages animation timeline and keyframes.

```rust
pub struct Timeline {
    pub current_frame: usize,
    pub total_frames: usize,
    pub keyframes: Vec<Keyframe>,
}

impl Timeline {
    pub fn new(total_frames: usize) -> Self;
    pub fn add_keyframe(&mut self, keyframe: Keyframe);
    pub fn remove_keyframe(&mut self, frame: usize);
    pub fn get_value_at_frame(&self, frame: usize) -> f32;
}
```

### Keyframe

Represents a keyframe in the timeline.

```rust
pub struct Keyframe {
    pub frame: usize,
    pub value: f32,
    pub easing: EasingType,
}

pub enum EasingType {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}
```

## Rendering

### Renderer

Handles frame rendering and export.

```rust
pub trait Renderer {
    fn render_frame(&mut self, scene: &Scene, frame: usize) -> FrameBuffer;
    fn export_frames(&mut self, scene: &Scene, output_dir: &Path);
}
```

### FrameBuffer

Represents a rendered frame.

```rust
pub struct FrameBuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl FrameBuffer {
    pub fn new(width: u32, height: u32) -> Self;
    pub fn save_png(&self, path: &Path) -> Result<()>;
    pub fn clear(&mut self, color: [u8; 4]);
}
```

## Animation System

### Animation Trait

Base trait for all animations.

```rust
pub trait Animation {
    fn update(&mut self, frame: usize);
    fn get_value(&self) -> f32;
    fn duration(&self) -> usize;
}
```

### Example Implementation

```rust
pub struct FadeAnimation {
    start_frame: usize,
    end_frame: usize,
    start_opacity: f32,
    end_opacity: f32,
}

impl Animation for FadeAnimation {
    fn update(&mut self, frame: usize) {
        // Calculate interpolated opacity
    }
    
    fn get_value(&self) -> f32 {
        // Return current opacity
    }
    
    fn duration(&self) -> usize {
        self.end_frame - self.start_frame
    }
}
```

## Export System

### VideoExporter

Exports animation to video format.

```rust
pub struct VideoExporter {
    pub fps: u32,
    pub output_path: PathBuf,
}

impl VideoExporter {
    pub fn new(output_path: PathBuf, fps: u32) -> Self;
    pub fn export(&self, frames_dir: &Path) -> Result<()>;
    fn run_ffmpeg(&self, args: &[String]) -> Result<()>;
}
```

## Events

### Event System

```rust
pub enum AppEvent {
    Play,
    Pause,
    Stop,
    FrameChanged(usize),
    Export,
    ProjectLoaded,
}

impl AppState {
    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Play => self.play(),
            AppEvent::Pause => self.pause(),
            // ... handle other events
        }
    }
}

### Time-changed DSL event

Motioner exposes a runtime `on_time` event that is emitted continuously while
the playhead advances. Handlers can be registered in DSL (block form) and
receive the current `seconds` and `frame` values for each tick.

Example (DSL):

```
on_time {
    move_element(name = "Circle", x = seconds * 0.1, y = 0.5)
}
```

This example moves the element named `Circle` every frame using the
`seconds` variable; the DSL currently supports a small set of built-in
actions (e.g. `move_element`) and arithmetic expressions using `seconds`
and `frame`.

Additional DSL features (variables & control flow):

```
on_time {
    // numeric and string variables
    let speed = seconds * 0.1
    let id = "Circle"

    // lists
    let steps = [0.0, 0.25, 0.5]

    // for-loop over a numeric range
    for i in 0..3 {
        move_element(name = "Circle", x = i * speed, y = 0.2)
    }

    // iterate a list and conditional
    for t in steps {
        if t > 0.2 {
            move_element(name = "Circle", x = t, y = 0.4)
        }
    }
}
```

```

## Error Handling

### Result Types

```rust
pub type Result<T> = std::result::Result<T, MotionerError>;

#[derive(Debug)]
pub enum MotionerError {
    IoError(std::io::Error),
    RenderError(String),
    ExportError(String),
    FFmpegError(String),
}
```

## Configuration

### ProjectSettings

```rust
pub struct ProjectSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration: f32,
    pub background_color: [u8; 4],
}

impl ProjectSettings {
    pub fn default() -> Self;
    pub fn load(path: &Path) -> Result<Self>;
    pub fn save(&self, path: &Path) -> Result<()>;
}
```

## Usage Examples

### Creating a Scene

```rust
let mut scene = Scene::new();
scene.add_object(SceneObject {
    id: 0,
    name: "Rectangle".to_string(),
    position: (100.0, 100.0),
    scale: (50.0, 50.0),
    rotation: 0.0,
    opacity: 1.0,
});
```

### Adding Animation

```rust
let mut timeline = Timeline::new(150);
timeline.add_keyframe(Keyframe {
    frame: 0,
    value: 0.0,
    easing: EasingType::Linear,
});
timeline.add_keyframe(Keyframe {
    frame: 150,
    value: 1.0,
    easing: EasingType::EaseOut,
});
```

### Exporting Video

```rust
let exporter = VideoExporter::new(
    PathBuf::from("output.mp4"),
    30
);
exporter.export(&PathBuf::from("frames/"))?;
```

## Next Steps

- [Contributing](./contributing.md)
- [Examples](../examples/basic-animation.md)
- [Architecture](./architecture.md)
