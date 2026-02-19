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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type FrameIndex = usize;

/// Per-frame property snapshot (subset of properties common to visual shapes).
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

/// Frame-keyed element container.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ElementKeyframes {
    /// Element name (from `Shape::name()`)
    pub name: String,
    /// Keyword / kind: "circle" | "rect" | "text"
    pub kind: String,
    /// Frames-per-second used when converting seconds -> frames
    pub fps: u32,
    /// Frame index -> properties (BTreeMap keeps deterministic ordering)
    pub frames: BTreeMap<FrameIndex, FrameProps>,
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
            frames: BTreeMap::new(),
            spawn_frame: 0,
            kill_frame: None,
        }
    }

    pub fn insert_frame(&mut self, frame: FrameIndex, props: FrameProps) {
        self.frames.insert(frame, props);
    }

    /// Sample the effective properties at `frame` by returning the latest
    /// keyframe <= `frame` (classic keyframe hold behaviour).
    pub fn sample(&self, frame: FrameIndex) -> Option<FrameProps> {
        let mut last: Option<FrameProps> = None;
        for (&k, v) in &self.frames {
            if k <= frame {
                last = Some(v.clone());
            } else {
                break;
            }
        }
        last
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
                ek.insert_frame(
                    spawn,
                    FrameProps {
                        x: Some(c.x),
                        y: Some(c.y),
                        radius: Some(c.radius),
                        w: None,
                        h: None,
                        size: None,
                        value: None,
                        color: Some(c.color),
                        visible: Some(c.visible),
                        z_index: Some(c.z_index),
                    },
                );
                Some(ek)
            }
            Shape::Rect(r) => {
                let mut ek = ElementKeyframes::new(r.name.clone(), "rect".into(), fps);
                let spawn = seconds_to_frame(r.spawn_time, fps);
                ek.spawn_frame = spawn;
                ek.kill_frame = r.kill_time.map(|k| seconds_to_frame(k, fps));
                ek.insert_frame(
                    spawn,
                    FrameProps {
                        x: Some(r.x),
                        y: Some(r.y),
                        radius: None,
                        w: Some(r.w),
                        h: Some(r.h),
                        size: None,
                        value: None,
                        color: Some(r.color),
                        visible: Some(r.visible),
                        z_index: Some(r.z_index),
                    },
                );
                Some(ek)
            }
            Shape::Text(t) => {
                let mut ek = ElementKeyframes::new(t.name.clone(), "text".into(), fps);
                let spawn = seconds_to_frame(t.spawn_time, fps);
                ek.spawn_frame = spawn;
                ek.kill_frame = t.kill_time.map(|k| seconds_to_frame(k, fps));
                ek.insert_frame(
                    spawn,
                    FrameProps {
                        x: Some(t.x),
                        y: Some(t.y),
                        radius: None,
                        w: None,
                        h: None,
                        size: Some(t.size),
                        value: Some(t.value.clone()),
                        color: Some(t.color),
                        visible: Some(t.visible),
                        z_index: Some(t.z_index),
                    },
                );
                Some(ek)
            }
            Shape::Group { .. } => None,
        }
    }

    /// Reconstruct a `Shape` representing this element at the given `frame`.
    /// Useful for preview/render code that needs a temporary Shape instance
    /// without changing the canonical `state.scene` storage.
    pub fn to_shape_at_frame(&self, frame: FrameIndex) -> Option<crate::shapes::shapes_manager::Shape> {
        let props = self.sample(frame)?;
        match self.kind.as_str() {
            "circle" => {
                let mut c = crate::shapes::circle::Circle::default();
                c.name = self.name.clone();
                if let Some(x) = props.x { c.x = x; }
                if let Some(y) = props.y { c.y = y; }
                if let Some(radius) = props.radius { c.radius = radius; }
                if let Some(col) = props.color { c.color = col; }
                if let Some(v) = props.visible { c.visible = v; }
                if let Some(z) = props.z_index { c.z_index = z; }
                // set spawn_time to expressed frame
                c.spawn_time = frame as f32 / self.fps as f32;
                if let Some(kf) = self.kill_frame { c.kill_time = Some(kf as f32 / self.fps as f32); }
                Some(crate::shapes::shapes_manager::Shape::Circle(c))
            }
            "rect" => {
                let mut r = crate::shapes::rect::Rect::default();
                r.name = self.name.clone();
                if let Some(x) = props.x { r.x = x; }
                if let Some(y) = props.y { r.y = y; }
                if let Some(w) = props.w { r.w = w; }
                if let Some(h) = props.h { r.h = h; }
                if let Some(col) = props.color { r.color = col; }
                if let Some(v) = props.visible { r.visible = v; }
                if let Some(z) = props.z_index { r.z_index = z; }
                r.spawn_time = frame as f32 / self.fps as f32;
                if let Some(kf) = self.kill_frame { r.kill_time = Some(kf as f32 / self.fps as f32); }
                Some(crate::shapes::shapes_manager::Shape::Rect(r))
            }
            "text" => {
                let mut t = crate::shapes::text::Text::default();
                t.name = self.name.clone();
                if let Some(x) = props.x { t.x = x; }
                if let Some(y) = props.y { t.y = y; }
                if let Some(sz) = props.size { t.size = sz; }
                if let Some(val) = props.value.clone() { t.value = val; }
                if let Some(col) = props.color { t.color = col; }
                if let Some(v) = props.visible { t.visible = v; }
                if let Some(z) = props.z_index { t.z_index = z; }
                t.spawn_time = frame as f32 / self.fps as f32;
                if let Some(kf) = self.kill_frame { t.kill_time = Some(kf as f32 / self.fps as f32); }
                Some(crate::shapes::shapes_manager::Shape::Text(t))
            }
            _ => None,
        }
    }
}

/// Convert seconds to a frame index using `fps`. Uses rounding to nearest
/// integer which makes `0.0s -> frame 0` and `1/fps -> frame 1` as expected.
pub fn seconds_to_frame(seconds: f32, fps: u32) -> FrameIndex {
    ((seconds * fps as f32).round() as isize).max(0) as FrameIndex
}

// -------------------- tests -------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::circle::Circle;
    use crate::shapes::shapes_manager::Shape;

    #[test]
    fn seconds_to_frame_rounds_correctly() {
        assert_eq!(seconds_to_frame(0.0, 60), 0);
        assert_eq!(seconds_to_frame(1.0 / 60.0, 60), 1);
        assert_eq!(seconds_to_frame(0.5, 2), 1); // 0.5s @2fps -> frame 1
    }

    #[test]
    fn circle_to_keyframes_spawn_frame_zero() {
        let mut c = Circle::default();
        c.spawn_time = 0.0;
        c.name = "C".into();
        c.x = 0.1;
        c.y = 0.25;
        c.radius = 0.08;
        let s = Shape::Circle(c);
        let kf = ElementKeyframes::from_shape_at_spawn(&s, 60).unwrap();
        assert_eq!(kf.spawn_frame, 0);
        let frame0 = kf.sample(0).unwrap();
        assert!((frame0.x.unwrap() - 0.1).abs() < 1e-6);
        assert!((frame0.y.unwrap() - 0.25).abs() < 1e-6);
        assert!((frame0.radius.unwrap() - 0.08).abs() < 1e-6);
    }

    #[test]
    fn to_shape_at_frame_restores_values() {
        let mut c = Circle::default();
        c.spawn_time = 0.0;
        c.name = "C2".into();
        c.x = 0.45;
        c.y = 0.55;
        c.radius = 0.12;
        c.color = [10, 20, 30, 255];
        let s = Shape::Circle(c);
        let ek = ElementKeyframes::from_shape_at_spawn(&s, 30).unwrap();
        let restored = ek.to_shape_at_frame(ek.spawn_frame).unwrap();
        match restored {
            Shape::Circle(rc) => {
                assert_eq!(rc.name, "C2");
                assert!((rc.x - 0.45).abs() < 1e-6);
                assert!((rc.y - 0.55).abs() < 1e-6);
                assert!((rc.radius - 0.12).abs() < 1e-6);
                assert_eq!(rc.color, [10, 20, 30, 255]);
            }
            _ => panic!("expected circle"),
        }
    }
}
