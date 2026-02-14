# Creating Animations

Learn how to create your first animation in Motioner.

## Quick Start Tutorial

### Step 1: Set Up Your Project

1. Launch Motioner
2. In Properties panel, set:
   - **FPS**: 30
   - **Duration**: 5 seconds (150 frames)
   - **Resolution**: 1920x1080

### Step 2: Add Elements

_(Current version uses programmatic scene definition)_

Future versions will include:
- Visual element creation
- Shape tools
- Text layers
- Image import

### Step 3: Create Animation

1. Position playhead at frame 0
2. Set initial properties
3. Move playhead forward
4. Adjust properties for new keyframe
5. Preview animation with `Space`

### Step 4: Refine

- Scrub timeline to review
- Adjust keyframe timing
- Fine-tune easing (future)
- Test at full speed

## Animation Techniques

### Position Animation

Animate element movement:
- X, Y coordinates
- Smooth motion paths
- Speed control via keyframe spacing

### Opacity Animation

Fade in/out effects:
- 0.0 = fully transparent
- 1.0 = fully opaque
- Great for transitions

### Scale Animation

Size changes:
- Uniform scaling (maintain aspect)
- Non-uniform (stretch/squash)
- Bounce and elastic effects

### Rotation Animation

Spin and rotate:
- Angle in degrees
- Multiple rotations
- Clockwise/counter-clockwise

## Scene Management

### Current Implementation

Animations are defined programmatically in `src/scene.rs`:

```rust
// Example scene structure
pub struct Scene {
    pub objects: Vec<SceneObject>,
    pub duration_frames: usize,
    // ... other properties
}
```

### Future Features

- Visual scene editor
- Drag-and-drop elements
- Layer composition
- Asset library

## Tips & Tricks

### Smooth Motion

- Use more keyframes for precise control
- Fewer keyframes = faster motion
- Plan timing before animating

### Reusable Animations

- Copy/paste keyframes
- Save animation presets (future)
- Template system (future)

### Performance

- Preview at lower resolution for complex scenes
- Use release build for final preview
- Limit simultaneous animations

## Common Workflows

### Logo Animation

1. Import logo (future)
2. Animate entrance (scale + opacity)
3. Hold for duration
4. Animate exit
5. Export

### Title Sequence

1. Create text layers (future)
2. Stagger animations
3. Add transitions
4. Time to music (future audio support)

### Motion Graphics

1. Define shape elements
2. Animate properties
3. Layer composition
4. Export as sequence

## Next Steps

- [Exporting Projects](./export.md) — Render your animation
- [Examples](../examples/basic-animation.md) — See working examples
