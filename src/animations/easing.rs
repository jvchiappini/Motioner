use serde::{Deserialize, Serialize};

/// Easing kinds for animations (moved from `scene.rs`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BezierPoint {
    pub pos: (f32, f32),
    pub handle_left: (f32, f32),  // Relative to pos
    pub handle_right: (f32, f32), // Relative to pos
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Easing {
    Linear,
    /// Symmetric ease-in/out parameterized by `power` (1.0 == linear).
    /// DSL: `ease_in_out(power = 2.0)`
    EaseInOut {
        power: f32,
    },
    /// Power-based ease-in: progress = t^power
    EaseIn {
        power: f32,
    },
    /// Power-based ease-out: progress = 1 - (1-t)^power
    EaseOut {
        power: f32,
    },
    /// Custom easing defined by a list of control points (t, value).
    /// Points should be sorted by t.
    /// Implicit start at (0,0) and end at (1,1) are assumed if not present,
    /// but explicitly including them makes editing easier.
    Custom {
        points: Vec<(f32, f32)>,
    },
    /// Custom easing with Bezier handles for each point.
    CustomBezier {
        points: Vec<BezierPoint>,
    },
    /// Cubic Bezier curve defined by two control points P1 and P2.
    /// P0 is (0,0) and P3 is (1,1).
    /// P1 = (x1, y1), P2 = (x2, y2)
    Bezier {
        p1: (f32, f32),
        p2: (f32, f32),
    },
    /// Preset: smooth sinusoidal ease-in-out (progress = 0.5*(1 - cos(pi*t))).
    Sine,
    /// Preset: exponential ease-in-out (fast start/stop, slow middle).
    Expo,
    /// Preset: circular ease-in-out.
    Circ,
    /// Damped spring — parameters: damping, stiffness, mass.
    Spring {
        damping: f32,
        stiffness: f32,
        mass: f32,
    },
    /// Elastic easing with given amplitude and period.
    Elastic {
        amplitude: f32,
        period: f32,
    },
    /// Bounce-style easing with configurable bounciness (0.0 = linear, 1.0 = default bounce).
    Bounce {
        bounciness: f32,
    },
}
