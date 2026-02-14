# Basic Animation Example

This example demonstrates creating a simple animation in Motioner.

## Overview

We'll create a basic fade-in animation that:
- Starts with 0% opacity
- Fades to 100% opacity over 3 seconds
- Uses 30 FPS

## Code Example

```rust
use motioner_ui::*;

fn main() {
    // Initialize scene
    let mut scene = Scene::new();
    scene.width = 1920;
    scene.height = 1080;
    scene.duration_frames = 90; // 3 seconds at 30 FPS
    
    // Create animated object
    let object = SceneObject {
        id: 0,
        name: "FadeInRect".to_string(),
        position: (960.0, 540.0), // Center
        scale: (200.0, 200.0),
        rotation: 0.0,
        opacity: 0.0, // Start transparent
    };
    
    scene.add_object(object);
    
    // Create fade animation
    let fade = FadeAnimation {
        start_frame: 0,
        end_frame: 90,
        start_opacity: 0.0,
        end_opacity: 1.0,
    };
    
    // Apply animation to object
    scene.add_animation(0, Box::new(fade));
}
```

## Step-by-Step

### 1. Create the Scene

```rust
let mut scene = Scene::new();
scene.width = 1920;
scene.height = 1080;
scene.duration_frames = 90;
```

### 2. Add an Object

```rust
let object = SceneObject {
    id: 0,
    name: "FadeInRect".to_string(),
    position: (960.0, 540.0),
    scale: (200.0, 200.0),
    rotation: 0.0,
    opacity: 0.0,
};

scene.add_object(object);
```

### 3. Create Animation

```rust
let fade = FadeAnimation {
    start_frame: 0,
    end_frame: 90,
    start_opacity: 0.0,
    end_opacity: 1.0,
};
```

### 4. Apply to Object

```rust
scene.add_animation(0, Box::new(fade));
```

## Running the Example

```powershell
# Save as examples/basic_animation.rs
cargo run --example basic_animation
```

## Expected Result

- Frame 0: Object is invisible (0% opacity)
- Frame 45: Object is half-visible (50% opacity)
- Frame 90: Object is fully visible (100% opacity)

## Variations

### Faster Animation (1 second)

```rust
scene.duration_frames = 30; // 1 second at 30 FPS
fade.end_frame = 30;
```

### Different Position

```rust
object.position = (100.0, 100.0); // Top-left
```

### Multiple Objects

```rust
let obj1 = SceneObject { id: 0, /* ... */ };
let obj2 = SceneObject { id: 1, /* ... */ };

scene.add_object(obj1);
scene.add_object(obj2);
```

## Next Steps

- [Frame-by-Frame Export](./frame-export.md)
- [FFmpeg Integration](./ffmpeg-integration.md)
- [User Guide](../user-guide/creating-animations.md)
