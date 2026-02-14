# Custom Animations

Learn how to create custom animation types in Motioner.

## Animation Trait

All animations implement the `Animation` trait:

```rust
pub trait Animation {
    fn update(&mut self, frame: usize);
    fn get_value(&self) -> f32;
    fn duration(&self) -> usize;
}
```

## Creating a Custom Animation

### Example: Bounce Animation

```rust
pub struct BounceAnimation {
    start_frame: usize,
    end_frame: usize,
    start_value: f32,
    end_value: f32,
    current_value: f32,
    bounces: u32,
}

impl BounceAnimation {
    pub fn new(
        start_frame: usize,
        end_frame: usize,
        start_value: f32,
        end_value: f32,
        bounces: u32,
    ) -> Self {
        Self {
            start_frame,
            end_frame,
            start_value,
            end_value,
            current_value: start_value,
            bounces,
        }
    }
    
    fn calculate_bounce(&self, progress: f32) -> f32 {
        let bounce_freq = self.bounces as f32 * std::f32::consts::PI * 2.0;
        let decay = (1.0 - progress).powi(2);
        let bounce = (progress * bounce_freq).sin() * decay;
        
        let range = self.end_value - self.start_value;
        self.start_value + range * progress + bounce * range * 0.1
    }
}

impl Animation for BounceAnimation {
    fn update(&mut self, frame: usize) {
        if frame < self.start_frame {
            self.current_value = self.start_value;
        } else if frame >= self.end_frame {
            self.current_value = self.end_value;
        } else {
            let progress = (frame - self.start_frame) as f32
                / (self.end_frame - self.start_frame) as f32;
            self.current_value = self.calculate_bounce(progress);
        }
    }
    
    fn get_value(&self) -> f32 {
        self.current_value
    }
    
    fn duration(&self) -> usize {
        self.end_frame - self.start_frame
    }
}
```

### Usage

```rust
let bounce = BounceAnimation::new(0, 60, 0.0, 100.0, 3);
scene.add_animation(object_id, Box::new(bounce));
```

## More Animation Examples

### Elastic Animation

```rust
pub struct ElasticAnimation {
    start_frame: usize,
    end_frame: usize,
    start_value: f32,
    end_value: f32,
    current_value: f32,
    elasticity: f32,
}

impl Animation for ElasticAnimation {
    fn update(&mut self, frame: usize) {
        let progress = ((frame - self.start_frame) as f32
            / (self.end_frame - self.start_frame) as f32)
            .clamp(0.0, 1.0);
        
        let elastic = (1.0 - progress).powi(3) 
            * (progress * 8.0 * std::f32::consts::PI).sin()
            * self.elasticity;
        
        let range = self.end_value - self.start_value;
        self.current_value = self.start_value + range * progress + elastic;
    }
    
    // ... other trait methods
}
```

### Spring Animation

```rust
pub struct SpringAnimation {
    start_frame: usize,
    end_frame: usize,
    start_value: f32,
    end_value: f32,
    current_value: f32,
    stiffness: f32,
    damping: f32,
}

impl Animation for SpringAnimation {
    fn update(&mut self, frame: usize) {
        let t = ((frame - self.start_frame) as f32
            / (self.end_frame - self.start_frame) as f32)
            .clamp(0.0, 1.0);
        
        let omega = self.stiffness;
        let zeta = self.damping;
        
        let spring = if zeta < 1.0 {
            let omega_d = omega * (1.0 - zeta * zeta).sqrt();
            let phase = (zeta * omega * t).exp();
            1.0 - phase * (omega_d * t).cos()
        } else {
            1.0 - ((-omega * t).exp())
        };
        
        let range = self.end_value - self.start_value;
        self.current_value = self.start_value + range * spring;
    }
    
    // ... other trait methods
}
```

## Easing Functions

### Common Easing Curves

```rust
pub mod easing {
    pub fn ease_in_quad(t: f32) -> f32 {
        t * t
    }
    
    pub fn ease_out_quad(t: f32) -> f32 {
        t * (2.0 - t)
    }
    
    pub fn ease_in_out_quad(t: f32) -> f32 {
        if t < 0.5 {
            2.0 * t * t
        } else {
            -1.0 + (4.0 - 2.0 * t) * t
        }
    }
    
    pub fn ease_in_cubic(t: f32) -> f32 {
        t * t * t
    }
    
    pub fn ease_out_cubic(t: f32) -> f32 {
        let t1 = t - 1.0;
        t1 * t1 * t1 + 1.0
    }
    
    pub fn ease_in_out_cubic(t: f32) -> f32 {
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            (t - 1.0) * (2.0 * t - 2.0) * (2.0 * t - 2.0) + 1.0
        }
    }
}
```

### Using Easing in Animations

```rust
pub struct EasedAnimation {
    start_frame: usize,
    end_frame: usize,
    start_value: f32,
    end_value: f32,
    current_value: f32,
    easing_fn: fn(f32) -> f32,
}

impl Animation for EasedAnimation {
    fn update(&mut self, frame: usize) {
        let t = ((frame - self.start_frame) as f32
            / (self.end_frame - self.start_frame) as f32)
            .clamp(0.0, 1.0);
        
        let eased = (self.easing_fn)(t);
        let range = self.end_value - self.start_value;
        self.current_value = self.start_value + range * eased;
    }
    
    // ... other methods
}

// Usage
let anim = EasedAnimation {
    easing_fn: easing::ease_in_out_cubic,
    // ... other fields
};
```

## Best Practices

1. **Keep animations pure** — No side effects in update()
2. **Clamp progress** — Always keep 0.0 ≤ progress ≤ 1.0
3. **Document parameters** — Explain what each parameter does
4. **Provide presets** — Common configurations for easy use
5. **Test edge cases** — Frame 0, last frame, out of range

## Animation Library

Consider creating reusable animation modules:

```
src/
└── animations/
    ├── mod.rs
    ├── basic.rs      // Linear, fade, etc.
    ├── easing.rs     // Easing functions
    ├── physics.rs    // Bounce, spring, elastic
    └── custom.rs     // Your custom animations
```

## Next Steps

- [API Reference](../developer-guide/api-reference.md)
- [Performance Optimization](./performance.md)
- [Basic Animation Example](../examples/basic-animation.md)
