use serde::{Deserialize, Serialize};

/// Easing kinds for animations (moved from `scene.rs`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Easing {
    Linear,
    /// Symmetric ease-in/out parameterized by `power` (1.0 == linear).
    /// DSL: `type = ease_in_out(power = 2.0)`
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
    /// Cubic Bezier curve defined by two control points P1 and P2.
    /// P0 is (0,0) and P3 is (1,1).
    /// P1 = (x1, y1), P2 = (x2, y2)
    Bezier {
        p1: (f32, f32),
        p2: (f32, f32),
    },
}
