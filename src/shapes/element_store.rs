//! Element keyframe store — frame-indexed snapshots for preview/render pipelines.
//!
//! This module provides a non-invasive, backward-compatible representation
//! that maps element properties to integer frames (keyframes). It includes
//! helpers to convert existing `Shape` instances into a per-frame snapshot
//! (useful when creating elements from DSL), sample properties at a given
//! frame, and reconstruct a `Shape` for rendering/preview at a specific
//! frame without changing the canonical `Scene`/`Shape` storage.
//!
//! The goal is to let the preview/render pipeline operate on compact,
//! deterministic per-frame data (frame -> properties) while keeping the
//! existing scene representation intact.

use crate::animations::easing::Easing;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type FrameIndex = usize;

/// Simple keyframe container (storage only — no interpolation here).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Keyframe<T> {
    pub frame: FrameIndex,
    pub value: T,
    pub easing: Easing,
}

/// Per-frame property snapshot (subset of properties common to visual shapes).
/// This type is still used as a convenience for sampling / converting to
/// legacy `Shape` instances — it is not the canonical storage format.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FrameProps {
    pub x: Option<f32>,
    pub y: Option<f32>,

    /* circle */
    pub radius: Option<f32>,

    /* rect */
    pub w: Option<f32>,
    pub h: Option<f32>,

    /* text */
    pub size: Option<f32>,
    pub value: Option<String>,

    /* common */
    pub color: Option<[u8; 4]>,
    pub visible: Option<bool>,
    pub z_index: Option<i32>,
}

impl FrameProps {
    pub fn merge(&self, other: &FrameProps) -> FrameProps {
        FrameProps {
            x: other.x.or(self.x),
            y: other.y.or(self.y),
            radius: other.radius.or(self.radius),
            w: other.w.or(self.w),
            h: other.h.or(self.h),
            size: other.size.or(self.size),
            value: other.value.clone().or_else(|| self.value.clone()),
            color: other.color.or(self.color),
            visible: other.visible.or(self.visible),
            z_index: other.z_index.or(self.z_index),
        }
    }

    pub fn with_visibility(mut self, v: bool) -> Self {
        self.visible = Some(v);
        self
    }
}

/// ElementKeyframes — canonical storage uses independent tracks per-property
/// (Vec<Keyframe<T>>). The old `frames: BTreeMap<FrameIndex, FrameProps>`-style
/// snapshot is no longer used internally; convenience APIs still return
/// `FrameProps` when sampling a given frame.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ElementKeyframes {
    /// Element name (from `Shape::name()`)
    pub name: String,
    /// Keyword / kind: "circle" | "rect" | "text"
    pub kind: String,
    /// Frames-per-second used when converting seconds -> frames
    pub fps: u32,

    /* per-property tracks (sorted by frame) */
    pub x: Vec<Keyframe<f32>>,
    pub y: Vec<Keyframe<f32>>,

    /* circle */
    pub radius: Vec<Keyframe<f32>>,

    /* rect */
    pub w: Vec<Keyframe<f32>>,
    pub h: Vec<Keyframe<f32>>,

    /* text */
    pub size: Vec<Keyframe<f32>>,
    pub value: Vec<Keyframe<String>>,

    /* common */
    pub color: Vec<Keyframe<[u8; 4]>>,
    pub visible: Vec<Keyframe<bool>>,
    pub z_index: Vec<Keyframe<i32>>,

    /// Ephemeral flag (shapes created at runtime / not serialized into DSL)
    pub ephemeral: bool,
    /// Animations attached to this element (kept for compatibility with existing UI/runtime)
    pub animations: Vec<crate::scene::Animation>,

    /// Spawn frame (computed from shape.spawn_time)
    pub spawn_frame: FrameIndex,
    /// Optional explicit kill frame (computed from kill_time)
    pub kill_frame: Option<FrameIndex>,
}

impl ElementKeyframes {
    pub fn new(name: String, kind: String, fps: u32) -> Self {
        ElementKeyframes {
            name,
            kind,
            fps,
            x: Vec::new(),
            y: Vec::new(),
            radius: Vec::new(),
            w: Vec::new(),
            h: Vec::new(),
            size: Vec::new(),
            value: Vec::new(),
            color: Vec::new(),
            visible: Vec::new(),
            z_index: Vec::new(),
            ephemeral: false,
            animations: Vec::new(),
            spawn_frame: 0,
            kill_frame: None,
        }
    }

    /// Insert a snapshot expressed as `FrameProps` — converts the snapshot
    /// into per-property keyframes (default easing: Linear).
    pub fn insert_frame(&mut self, frame: FrameIndex, props: FrameProps) {
        // generic helper to push a keyframe into a typed track
        fn push_kf<T: Clone>(vec: &mut Vec<Keyframe<T>>, frame: FrameIndex, value: T) {
            vec.push(Keyframe {
                frame,
                value,
                easing: Easing::Linear,
            });
            vec.sort_by_key(|k| k.frame);
        }

        if let Some(xv) = props.x {
            push_kf(&mut self.x, frame, xv);
        }
        if let Some(yv) = props.y {
            push_kf(&mut self.y, frame, yv);
        }
        if let Some(r) = props.radius {
            push_kf(&mut self.radius, frame, r);
        }
        if let Some(wv) = props.w {
            push_kf(&mut self.w, frame, wv);
        }
        if let Some(hv) = props.h {
            push_kf(&mut self.h, frame, hv);
        }
        if let Some(sz) = props.size {
            push_kf(&mut self.size, frame, sz);
        }
        if let Some(v) = props.value {
            push_kf(&mut self.value, frame, v);
        }
        if let Some(col) = props.color {
            push_kf(&mut self.color, frame, col);
        }
        if let Some(vis) = props.visible {
            push_kf(&mut self.visible, frame, vis);
        }
        if let Some(z) = props.z_index {
            push_kf(&mut self.z_index, frame, z);
        }
    }

    /// Sample the effective properties at `frame` by returning the latest
    /// keyframe <= `frame` (classic keyframe hold behaviour).
    fn latest_from_track<T: Clone>(track: &Vec<Keyframe<T>>, frame: FrameIndex) -> Option<T> {
        for kf in track.iter().rev() {
            if kf.frame <= frame {
                return Some(kf.value.clone());
            }
        }
        None
    }

    /// Sample the effective properties at `frame` by returning the latest
    /// keyframe <= `frame` for every property that has one.
    pub fn sample(&self, frame: FrameIndex) -> Option<FrameProps> {
        let any = !self.x.is_empty()
            || !self.y.is_empty()
            || !self.radius.is_empty()
            || !self.w.is_empty()
            || !self.h.is_empty()
            || !self.size.is_empty()
            || !self.value.is_empty()
            || !self.color.is_empty()
            || !self.visible.is_empty()
            || !self.z_index.is_empty();

        if !any {
            return None;
        }

        Some(FrameProps {
            x: Self::latest_from_track(&self.x, frame),
            y: Self::latest_from_track(&self.y, frame),
            radius: Self::latest_from_track(&self.radius, frame),
            w: Self::latest_from_track(&self.w, frame),
            h: Self::latest_from_track(&self.h, frame),
            size: Self::latest_from_track(&self.size, frame),
            value: Self::latest_from_track(&self.value, frame),
            color: Self::latest_from_track(&self.color, frame),
            visible: Self::latest_from_track(&self.visible, frame),
            z_index: Self::latest_from_track(&self.z_index, frame),
        })
    }

    /// Convert an existing `Shape` into a single-keyframe ElementKeyframes
    /// anchored at its `spawn_time`. Returns None for non-visual groups.
    pub fn from_shape_at_spawn(s: &crate::shapes::shapes_manager::Shape, fps: u32) -> Option<Self> {
        use crate::shapes::shapes_manager::Shape;

        match s {
            Shape::Circle(c) => {
                let mut ek = ElementKeyframes::new(c.name.clone(), "circle".into(), fps);
                let spawn = seconds_to_frame(c.spawn_time, fps);
                ek.spawn_frame = spawn;
                ek.kill_frame = c.kill_time.map(|k| seconds_to_frame(k, fps));
                // populate initial keyframes (single hold keyframe at spawn)
                ek.x.push(Keyframe {
                    frame: spawn,
                    value: c.x,
                    easing: Easing::Linear,
                });
                ek.y.push(Keyframe {
                    frame: spawn,
                    value: c.y,
                    easing: Easing::Linear,
                });
                ek.radius.push(Keyframe {
                    frame: spawn,
                    value: c.radius,
                    easing: Easing::Linear,
                });
                ek.color.push(Keyframe {
                    frame: spawn,
                    value: c.color,
                    easing: Easing::Linear,
                });
                ek.visible.push(Keyframe {
                    frame: spawn,
                    value: c.visible,
                    easing: Easing::Linear,
                });
                ek.z_index.push(Keyframe {
                    frame: spawn,
                    value: c.z_index,
                    easing: Easing::Linear,
                });
                ek.ephemeral = c.ephemeral;
                ek.animations = c.animations.clone();
                Some(ek)
            }
            Shape::Rect(r) => {
                let mut ek = ElementKeyframes::new(r.name.clone(), "rect".into(), fps);
                let spawn = seconds_to_frame(r.spawn_time, fps);
                ek.spawn_frame = spawn;
                ek.kill_frame = r.kill_time.map(|k| seconds_to_frame(k, fps));
                ek.x.push(Keyframe {
                    frame: spawn,
                    value: r.x,
                    easing: Easing::Linear,
                });
                ek.y.push(Keyframe {
                    frame: spawn,
                    value: r.y,
                    easing: Easing::Linear,
                });
                ek.w.push(Keyframe {
                    frame: spawn,
                    value: r.w,
                    easing: Easing::Linear,
                });
                ek.h.push(Keyframe {
                    frame: spawn,
                    value: r.h,
                    easing: Easing::Linear,
                });
                ek.color.push(Keyframe {
                    frame: spawn,
                    value: r.color,
                    easing: Easing::Linear,
                });
                ek.visible.push(Keyframe {
                    frame: spawn,
                    value: r.visible,
                    easing: Easing::Linear,
                });
                ek.z_index.push(Keyframe {
                    frame: spawn,
                    value: r.z_index,
                    easing: Easing::Linear,
                });
                ek.ephemeral = r.ephemeral;
                ek.animations = r.animations.clone();
                Some(ek)
            }
            Shape::Text(t) => {
                let mut ek = ElementKeyframes::new(t.name.clone(), "text".into(), fps);
                let spawn = seconds_to_frame(t.spawn_time, fps);
                ek.spawn_frame = spawn;
                ek.kill_frame = t.kill_time.map(|k| seconds_to_frame(k, fps));
                ek.x.push(Keyframe {
                    frame: spawn,
                    value: t.x,
                    easing: Easing::Linear,
                });
                ek.y.push(Keyframe {
                    frame: spawn,
                    value: t.y,
                    easing: Easing::Linear,
                });
                ek.size.push(Keyframe {
                    frame: spawn,
                    value: t.size,
                    easing: Easing::Linear,
                });
                ek.value.push(Keyframe {
                    frame: spawn,
                    value: t.value.clone(),
                    easing: Easing::Linear,
                });
                ek.color.push(Keyframe {
                    frame: spawn,
                    value: t.color,
                    easing: Easing::Linear,
                });
                ek.visible.push(Keyframe {
                    frame: spawn,
                    value: t.visible,
                    easing: Easing::Linear,
                });
                ek.z_index.push(Keyframe {
                    frame: spawn,
                    value: t.z_index,
                    easing: Easing::Linear,
                });
                ek.ephemeral = t.ephemeral;
                ek.animations = t.animations.clone();
                Some(ek)
            }
            Shape::Group { .. } => None,
        }
    }

    /// Reconstruct a `Shape` representing this element at the given `frame`.
    /// Useful for preview/render code that needs a temporary Shape instance
    /// without changing the canonical `state.scene` storage.
    pub fn to_shape_at_frame(
        &self,
        frame: FrameIndex,
    ) -> Option<crate::shapes::shapes_manager::Shape> {
        let props = self.sample(frame)?;
        match self.kind.as_str() {
            "circle" => {
                let mut c = crate::shapes::circle::Circle::default();
                c.name = self.name.clone();
                if let Some(x) = props.x {
                    c.x = x;
                }
                if let Some(y) = props.y {
                    c.y = y;
                }
                if let Some(radius) = props.radius {
                    c.radius = radius;
                }
                if let Some(col) = props.color {
                    c.color = col;
                }
                if let Some(v) = props.visible {
                    c.visible = v;
                }
                if let Some(z) = props.z_index {
                    c.z_index = z;
                }
                // set spawn_time to expressed frame
                c.spawn_time = frame as f32 / self.fps as f32;
                if let Some(kf) = self.kill_frame {
                    c.kill_time = Some(kf as f32 / self.fps as f32);
                }
                c.ephemeral = self.ephemeral;
                c.animations = self.animations.clone();
                Some(crate::shapes::shapes_manager::Shape::Circle(c))
            }
            "rect" => {
                let mut r = crate::shapes::rect::Rect::default();
                r.name = self.name.clone();
                if let Some(x) = props.x {
                    r.x = x;
                }
                if let Some(y) = props.y {
                    r.y = y;
                }
                if let Some(w) = props.w {
                    r.w = w;
                }
                if let Some(h) = props.h {
                    r.h = h;
                }
                if let Some(col) = props.color {
                    r.color = col;
                }
                if let Some(v) = props.visible {
                    r.visible = v;
                }
                if let Some(z) = props.z_index {
                    r.z_index = z;
                }
                r.spawn_time = frame as f32 / self.fps as f32;
                if let Some(kf) = self.kill_frame {
                    r.kill_time = Some(kf as f32 / self.fps as f32);
                }
                r.ephemeral = self.ephemeral;
                r.animations = self.animations.clone();
                Some(crate::shapes::shapes_manager::Shape::Rect(r))
            }
            "text" => {
                let mut t = crate::shapes::text::Text::default();
                t.name = self.name.clone();
                if let Some(x) = props.x {
                    t.x = x;
                }
                if let Some(y) = props.y {
                    t.y = y;
                }
                if let Some(sz) = props.size {
                    t.size = sz;
                }
                if let Some(val) = props.value.clone() {
                    t.value = val;
                }
                if let Some(col) = props.color {
                    t.color = col;
                }
                if let Some(v) = props.visible {
                    t.visible = v;
                }
                if let Some(z) = props.z_index {
                    t.z_index = z;
                }
                t.spawn_time = frame as f32 / self.fps as f32;
                if let Some(kf) = self.kill_frame {
                    t.kill_time = Some(kf as f32 / self.fps as f32);
                }
                t.ephemeral = self.ephemeral;
                t.animations = self.animations.clone();
                Some(crate::shapes::shapes_manager::Shape::Text(t))
            }
            _ => None,
        }
    }
}

/// Convert a slice of ElementKeyframes into legacy `Shape` instances by
/// materializing each element at its spawn frame. Used as a compatibility
/// shim for DSL generation and other code that still expects `Vec<Shape>`.
pub fn to_legacy_shapes(elements: &[ElementKeyframes]) -> Vec<crate::scene::Shape> {
    let mut out: Vec<crate::scene::Shape> = Vec::new();
    for e in elements {
        if let Some(s) = e.to_shape_at_frame(e.spawn_frame) {
            out.push(s);
        }
    }
    out
}

/// Convert seconds to a frame index using `fps`. Uses rounding to nearest
/// integer which makes `0.0s -> frame 0` and `1/fps -> frame 1` as expected.
pub fn seconds_to_frame(seconds: f32, fps: u32) -> FrameIndex {
    ((seconds * fps as f32).round() as isize).max(0) as FrameIndex
}
