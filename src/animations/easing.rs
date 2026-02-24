use serde::{Deserialize, Serialize};

/// Easing kinds for animations (moved from `scene.rs`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BezierPoint {
    pub pos: (f32, f32),
    pub handle_left: (f32, f32),  // Relative to pos
    pub handle_right: (f32, f32), // Relative to pos
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
    /// Discrete step: element teleports immediately to target as soon as the
    /// animation begins (no interpolation).  Behaviour is implemented both in
    /// the CPU fallback and in the GPU compute shader.
    Step,
}

impl Easing {
    pub fn to_dsl(&self) -> String {
        match self {
            Easing::Linear => "linear".to_string(),
            Easing::Step => "step".to_string(),
            Easing::Sine => "sine".to_string(),
            Easing::Expo => "expo".to_string(),
            Easing::Circ => "circ".to_string(),
            Easing::EaseIn { power } => format!("ease_in(power = {:.2})", power),
            Easing::EaseOut { power } => format!("ease_out(power = {:.2})", power),
            Easing::EaseInOut { power } => format!("ease_in_out(power = {:.2})", power),
            Easing::Bezier { p1, p2 } => format!("bezier(p1 = ({:.3}, {:.3}), p2 = ({:.3}, {:.3}))", p1.0, p1.1, p2.0, p2.1),
            Easing::Spring { damping, stiffness, mass } => format!("spring(damping = {:.2}, stiffness = {:.2}, mass = {:.2})", damping, stiffness, mass),
            Easing::Elastic { amplitude, period } => format!("elastic(amplitude = {:.2}, period = {:.2})", amplitude, period),
            Easing::Bounce { bounciness } => format!("bounce(bounciness = {:.2})", bounciness),
            Easing::Custom { points } => {
                let pts: Vec<String> = points.iter().map(|(t, v)| format!("({:.3}, {:.3})", t, v)).collect();
                format!("custom(points = [{}])", pts.join(", "))
            }
            Easing::CustomBezier { points } => {
                let pts: Vec<String> = points.iter().map(|p| format!("(({:.3}, {:.3}), ({:.3}, {:.3}), ({:.3}, {:.3}))", p.pos.0, p.pos.1, p.handle_left.0, p.handle_left.1, p.handle_right.0, p.handle_right.1)).collect();
                format!("custom_bezier(points = [{}])", pts.join(", "))
            }
        }
    }
}
