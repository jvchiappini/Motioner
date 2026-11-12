//! Frame evaluation: rig plus time to per-part writes.

use std::collections::BTreeMap;
use std::fmt;

use motioner_core::Vec2;

use crate::driver::{Driver, Field, MAX_WRITES};
use crate::machine::{MachineEvent, Runner};
use crate::{PropValue, Rig};

/// Maximum frames one bake may evaluate.
pub const MAX_BAKE_FRAMES: usize = 200_000;
/// Maximum writes all drivers may perform in one frame.
pub const MAX_FRAME_WRITES: usize = 4096;

/// Why a bake failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigError {
    /// What went wrong.
    pub message: String,
}

impl RigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for RigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RigError {}

/// The writes for one part at one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PartOutput {
    /// Part index into [`Rig::parts`].
    pub part: usize,
    /// Position relative to the part's base.
    pub position: Vec2,
    /// Rotation in degrees, added to the base.
    pub rotation: f64,
    /// Scale multiplier applied to the base.
    pub scale: Vec2,
    /// Absolute opacity.
    pub opacity: f64,
    /// Absolute stroke width, when a driver writes it.
    pub stroke_width: Option<f64>,
    /// Path points when a geometry driver writes this part.
    pub geometry: Option<motioner_core::PathGeometry>,
    /// Whether a driver or constraint touched this part.
    pub touched: bool,
}

/// Everything the bake produces for one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameOutput {
    /// Integer frame index.
    pub frame: i64,
    /// One entry per part.
    pub parts: Vec<PartOutput>,
    /// Property values after tracks, clips, the machine and layers.
    pub properties: BTreeMap<usize, PropValue>,
    /// Machine events emitted at this frame.
    pub events: Vec<MachineEvent>,
}

impl FrameOutput {
    /// The value of a property by index.
    #[must_use]
    pub fn property(&self, index: usize) -> Option<&PropValue> {
        self.properties.get(&index)
    }

    /// The output of a part by index.
    #[must_use]
    pub fn part(&self, index: usize) -> Option<&PartOutput> {
        self.parts.get(index)
    }
}

/// Bakes `rig` from `first` to `last`, inclusive, one output per integer frame.
///
/// The machine is stepped sequentially; calling this from the first frame is
/// the documented seek rule (there is no hidden state to rewind).
///
/// # Errors
///
/// Returns [`RigError`] for a driver that fails, too many writes, or a frame
/// range larger than [`MAX_BAKE_FRAMES`].
pub fn bake(rig: &Rig, first: f64, last: f64) -> Result<Vec<FrameOutput>, RigError> {
    #[allow(clippy::cast_possible_truncation)] // frames are bounded by MAX_BAKE_FRAMES
    let first = first.round().max(0.0) as i64;
    #[allow(clippy::cast_possible_truncation)]
    let last = last.round().max(0.0) as i64;
    if last < first {
        return Ok(Vec::new());
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // range checked below
    let count = (last - first + 1) as usize;
    if count > MAX_BAKE_FRAMES {
        return Err(RigError::new(format!(
            "rig '{}': bake range of {count} frames exceeds {MAX_BAKE_FRAMES}",
            rig.name
        )));
    }
    let mut runner = rig.machine.as_ref().map(Runner::new);
    let mut frames = Vec::with_capacity(count);
    for frame in first..=last {
        #[allow(clippy::cast_precision_loss)] // frames are bounded
        let time = frame as f64;
        let mut values = rig.sample_properties(time);
        let mut events = Vec::new();
        if let (Some(machine), Some(runner)) = (rig.machine.as_ref(), runner.as_mut()) {
            runner.step(rig, machine, time, &mut values, &mut events);
        }
        rig.apply_layers(time, &mut values);
        let mut parts: Vec<PartOutput> = rig
            .parts
            .iter()
            .enumerate()
            .map(|(index, part)| PartOutput {
                part: index,
                position: Vec2::ZERO,
                rotation: 0.0,
                scale: Vec2::ONE,
                opacity: part.opacity,
                stroke_width: None,
                geometry: None,
                touched: false,
            })
            .collect();
        let mut writes = 0usize;
        for driver in &rig.drivers {
            apply_driver(driver, rig, &values, &mut parts, &mut writes)?;
        }
        if writes > MAX_FRAME_WRITES {
            return Err(RigError::new(format!(
                "rig '{}' frame {frame}: {writes} writes exceed {MAX_FRAME_WRITES}",
                rig.name
            )));
        }
        apply_aims(rig, &values, &mut parts);
        frames.push(FrameOutput {
            frame,
            parts,
            properties: values,
            events,
        });
    }
    Ok(frames)
}

/// Evaluates one frame by replaying from the first frame.
///
/// # Errors
///
/// Same as [`bake`].
pub fn evaluate(rig: &Rig, frame: f64) -> Result<FrameOutput, RigError> {
    let frames = bake(rig, 0.0, frame)?;
    frames
        .into_iter()
        .last()
        .ok_or_else(|| RigError::new(format!("rig '{}': no frame produced for {frame}", rig.name)))
}

fn apply_driver(
    driver: &Driver,
    rig: &Rig,
    values: &BTreeMap<usize, PropValue>,
    parts: &mut [PartOutput],
    writes: &mut usize,
) -> Result<(), RigError> {
    if driver.writes.len() + driver.geometries.len() > MAX_WRITES {
        return Err(RigError::new(format!(
            "driver '{}': {} writes exceed {MAX_WRITES}",
            driver.name,
            driver.writes.len()
        )));
    }
    for write in &driver.writes {
        let Some(part) = parts.get_mut(write.part) else {
            return Err(RigError::new(format!(
                "driver '{}': part {} does not exist",
                driver.name, write.part
            )));
        };
        let value = write
            .value
            .eval(values, &driver.name)
            .map_err(|error| RigError::new(error.to_string()))?;
        match write.field {
            Field::PositionX => part.position.x += value,
            Field::PositionY => part.position.y += value,
            Field::Rotation => part.rotation += value,
            Field::ScaleX => part.scale.x *= value,
            Field::ScaleY => part.scale.y *= value,
            Field::Opacity => part.opacity = value.clamp(0.0, 1.0),
            Field::StrokeWidth => part.stroke_width = Some(value.max(0.0)),
        }
        part.touched = true;
        *writes += 1;
    }
    for write in &driver.geometries {
        let Some(part) = parts.get_mut(write.part) else {
            return Err(RigError::new(format!(
                "driver '{}': part {} does not exist",
                driver.name, write.part
            )));
        };
        let Some(base) = rig
            .parts
            .get(write.part)
            .and_then(|part| part.geometry.as_ref())
        else {
            return Err(RigError::new(format!(
                "driver '{}': part {} is not a path and cannot take geometry",
                driver.name, write.part
            )));
        };
        let geometry = write
            .geometry
            .eval(values, &driver.name)
            .map_err(|error| RigError::new(error.to_string()))?;
        if geometry.points().len() != base.points().len() || geometry.closed() != base.closed() {
            return Err(RigError::new(format!(
                "driver '{}': geometry topology mismatch: expected {} points ({}), got {} ({})",
                driver.name,
                base.points().len(),
                if base.closed() { "closed" } else { "open" },
                geometry.points().len(),
                if geometry.closed() { "closed" } else { "open" }
            )));
        }
        part.geometry = Some(
            motioner_core::PathGeometry::new(
                geometry.points().to_vec(),
                base.closed(),
                base.cap(),
                base.join(),
            )
            .map_err(|error| RigError::new(error.to_string()))?,
        );
        part.touched = true;
        *writes += 1;
    }
    Ok(())
}

fn apply_aims(rig: &Rig, values: &BTreeMap<usize, PropValue>, parts: &mut [PartOutput]) {
    for aim in &rig.aims {
        let Some(part) = parts.get_mut(aim.part) else {
            continue;
        };
        let Some(look) = values.get(&aim.property).and_then(PropValue::vector) else {
            continue;
        };
        let offset = look * aim.radius;
        part.position = part.position + offset;
        part.touched = true;
    }
}
