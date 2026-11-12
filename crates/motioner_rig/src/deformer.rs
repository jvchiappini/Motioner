//! Native deformers: lattice, skinning and blend shapes (D-0059).
//!
//! Deformers run after drivers and before constraints. They only touch a
//! part's path points; transforms stay with the driver/constraint stages.
//! Every operation is pure and allocation-bounded, so bake and a future
//! runtime share the same code.

use std::collections::BTreeMap;

use motioner_core::{PathGeometry, PathPoint, Vec2};

use crate::eval::RigError;
use crate::{PartBase, PartOutput, PropValue};

/// Maximum bones in one skin.
pub const MAX_BONES: usize = 64;
/// Maximum blend shapes on one part.
pub const MAX_BLEND_SHAPES: usize = 32;
/// Maximum point count a skin or blend shape may touch.
pub const MAX_DEFORM_POINTS: usize = 16_384;

/// A 2x2 free-form lattice with quadratic edge bends.
#[derive(Debug, Clone, PartialEq)]
pub struct Lattice {
    /// Part index.
    pub part: usize,
    /// Vec2 properties for the top-left, top-right, bottom-right and
    /// bottom-left corners, as offsets from the part's local bounds.
    pub corners: [usize; 4],
    /// Optional Vec2 midpoint offsets for the top, right, bottom and left
    /// edges.
    pub edges: [Option<usize>; 4],
}

/// One bone in a skin.
#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    /// Stable name used by weights and the DSL.
    pub name: String,
    /// Parent bone index.
    pub parent: Option<usize>,
    /// Bind position in the parent's space.
    pub bind_position: Vec2,
    /// Bind rotation in degrees, relative to the parent.
    pub bind_rotation: f64,
    /// Length along the bone's local +x axis, for auto weights.
    pub length: f64,
    /// Property that adds to the bone's rotation.
    pub rotation_property: Option<usize>,
    /// Vec2 property that adds to the bone's position.
    pub position_property: Option<usize>,
}

/// A skin: bones plus optional per-point weights.
#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    /// Part index.
    pub part: usize,
    /// Bone hierarchy.
    pub bones: Vec<Bone>,
    /// Per-point `(bone, weight)` pairs. `None` uses distance weights.
    pub weights: Option<Vec<Vec<(usize, f64)>>>,
}

/// One blend shape (morph target).
#[derive(Debug, Clone, PartialEq)]
pub struct BlendShape {
    /// Stable name.
    pub name: String,
    /// Target points, already compatible with the part topology.
    pub points: Vec<PathPoint>,
    /// Number property holding the blend weight in `0..=1`.
    pub weight: usize,
}

/// A set of blend shapes on one part.
#[derive(Debug, Clone, PartialEq)]
pub struct BlendShapes {
    /// Part index.
    pub part: usize,
    /// Shapes in application order.
    pub shapes: Vec<BlendShape>,
}

/// A deformer in the part pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum Deformer {
    /// 2x2 free-form lattice with edge bends.
    Lattice(Lattice),
    /// Hierarchical bone skinning.
    Skin(Skin),
    /// Blend shapes.
    BlendShapes(BlendShapes),
}

impl Deformer {
    /// The part this deformer writes.
    #[must_use]
    pub fn part(&self) -> usize {
        match self {
            Self::Lattice(lattice) => lattice.part,
            Self::Skin(skin) => skin.part,
            Self::BlendShapes(shapes) => shapes.part,
        }
    }

    /// Applies the deformer to one part.
    ///
    /// # Errors
    ///
    /// Returns [`RigError`] for a missing path part, an out-of-range bone,
    /// invalid weights, a topology mismatch or a non-finite result.
    pub fn apply(
        &self,
        part: &mut PartOutput,
        base: &PartBase,
        values: &BTreeMap<usize, PropValue>,
    ) -> Result<(), RigError> {
        let source = part
            .geometry
            .as_ref()
            .or(base.geometry.as_ref())
            .cloned()
            .ok_or_else(|| {
                RigError::message(format!("part '{}' has no path to deform", base.name))
            })?;
        let deformed = match self {
            Self::Lattice(lattice) => lattice.apply(&source, values)?,
            Self::Skin(skin) => skin.apply(&source, values)?,
            Self::BlendShapes(shapes) => shapes.apply(&source, values)?,
        };
        part.geometry = Some(deformed);
        part.touched = true;
        Ok(())
    }
}

impl Lattice {
    fn apply(
        &self,
        geometry: &PathGeometry,
        values: &BTreeMap<usize, PropValue>,
    ) -> Result<PathGeometry, RigError> {
        let bounds = geometry
            .local_bounds()
            .ok_or_else(|| RigError::message("lattice source has no finite bounds"))?;
        let [left, top, right, bottom] = bounds;
        let width = right - left;
        let height = bottom - top;
        if width.abs() < 1e-9 || height.abs() < 1e-9 {
            return Ok(geometry.clone());
        }
        let source = [
            Vec2::new(left, top),
            Vec2::new(right, top),
            Vec2::new(right, bottom),
            Vec2::new(left, bottom),
        ];
        let mut corners = source;
        for (index, corner) in corners.iter_mut().enumerate() {
            *corner = source[index] + vector(values, self.corners[index]).unwrap_or(Vec2::ZERO);
        }
        let mut midpoints = [
            (corners[0] + corners[1]) * 0.5,
            (corners[1] + corners[2]) * 0.5,
            (corners[2] + corners[3]) * 0.5,
            (corners[3] + corners[0]) * 0.5,
        ];
        for (index, edge) in self.edges.iter().enumerate() {
            if let Some(property) = edge {
                midpoints[index] =
                    midpoints[index] + vector(values, *property).unwrap_or(Vec2::ZERO);
            }
        }
        let mut points = Vec::with_capacity(geometry.points().len());
        for point in geometry.points() {
            let u = ((point.position.x - left) / width).clamp(0.0, 1.0);
            let v = ((point.position.y - top) / height).clamp(0.0, 1.0);
            let position = map_lattice(corners, midpoints, u, v);
            let end = map_lattice(
                corners,
                midpoints,
                ((point.position.x + point.handle_out.x - left) / width).clamp(0.0, 1.0),
                ((point.position.y + point.handle_out.y - top) / height).clamp(0.0, 1.0),
            );
            let incoming = map_lattice(
                corners,
                midpoints,
                ((point.position.x + point.handle_in.x - left) / width).clamp(0.0, 1.0),
                ((point.position.y + point.handle_in.y - top) / height).clamp(0.0, 1.0),
            );
            points.push(PathPoint::curved(
                position,
                incoming - position,
                end - position,
            ));
        }
        PathGeometry::new(points, geometry.closed(), geometry.cap(), geometry.join())
            .map_err(|error| RigError::message(error.to_string()))
    }
}

fn map_lattice(corners: [Vec2; 4], midpoints: [Vec2; 4], u: f64, v: f64) -> Vec2 {
    let top = quadratic(corners[0], midpoints[0], corners[1], u);
    let bottom = quadratic(corners[3], midpoints[2], corners[2], u);
    let left = quadratic(corners[0], midpoints[3], corners[3], v);
    let right = quadratic(corners[1], midpoints[1], corners[2], v);
    let bilinear = (corners[0] * (1.0 - u) + corners[1] * u) * (1.0 - v)
        + (corners[3] * (1.0 - u) + corners[2] * u) * v;
    top * (1.0 - v) + bottom * v + left * (1.0 - u) + right * u - bilinear
}

fn quadratic(a: Vec2, mid: Vec2, b: Vec2, t: f64) -> Vec2 {
    let one = 1.0 - t;
    a * (one * one) + mid * (2.0 * one * t) + b * (t * t)
}

impl Skin {
    fn apply(
        &self,
        geometry: &PathGeometry,
        values: &BTreeMap<usize, PropValue>,
    ) -> Result<PathGeometry, RigError> {
        if self.bones.len() > MAX_BONES {
            return Err(RigError::message(format!(
                "skin has more than {MAX_BONES} bones"
            )));
        }
        if geometry.points().len() > MAX_DEFORM_POINTS {
            return Err(RigError::message(format!(
                "skin touches more than {MAX_DEFORM_POINTS} points"
            )));
        }
        for (index, bone) in self.bones.iter().enumerate() {
            if bone.parent.is_some_and(|parent| parent >= index) {
                return Err(RigError::message(format!(
                    "bone '{}' has a parent after it",
                    bone.name
                )));
            }
        }
        let mut bind = Vec::with_capacity(self.bones.len());
        let mut posed = Vec::with_capacity(self.bones.len());
        for (index, _) in self.bones.iter().enumerate() {
            bind.push(bone_world(&self.bones, index, None, true)?);
            posed.push(bone_world(&self.bones, index, Some(values), false)?);
        }
        let weights = self
            .weights
            .clone()
            .unwrap_or_else(|| auto_weights(self, &bind, geometry));
        if weights.len() != geometry.points().len() {
            return Err(RigError::message(format!(
                "skin weights cover {} points but the path has {}",
                weights.len(),
                geometry.points().len()
            )));
        }
        let mut points = Vec::with_capacity(geometry.points().len());
        for (point, weights) in geometry.points().iter().zip(&weights) {
            let mut position = Vec2::ZERO;
            let mut incoming = Vec2::ZERO;
            let mut outgoing = Vec2::ZERO;
            let mut total = 0.0;
            for &(bone, weight) in weights {
                if bone >= self.bones.len() || !weight.is_finite() || weight < 0.0 {
                    return Err(RigError::message("skin has an invalid weight"));
                }
                if weight == 0.0 {
                    continue;
                }
                let local = inverse(bind[bone], point.position);
                let mapped = forward(posed[bone], local);
                let delta = posed[bone].rotation - bind[bone].rotation;
                position = position + mapped * weight;
                incoming = incoming + rotate(point.handle_in, delta) * weight;
                outgoing = outgoing + rotate(point.handle_out, delta) * weight;
                total += weight;
            }
            if total > 1e-9 {
                position = position * (1.0 / total);
                incoming = incoming * (1.0 / total);
                outgoing = outgoing * (1.0 / total);
            } else {
                position = point.position;
                incoming = point.handle_in;
                outgoing = point.handle_out;
            }
            points.push(PathPoint::curved(position, incoming, outgoing));
        }
        PathGeometry::new(points, geometry.closed(), geometry.cap(), geometry.join())
            .map_err(|error| RigError::message(error.to_string()))
    }
}

#[derive(Debug, Clone, Copy)]
struct BoneWorld {
    position: Vec2,
    rotation: f64,
}

fn bone_world(
    bones: &[Bone],
    index: usize,
    values: Option<&BTreeMap<usize, PropValue>>,
    bind: bool,
) -> Result<BoneWorld, RigError> {
    let bone = &bones[index];
    let (position, rotation) = if bind {
        (bone.bind_position, bone.bind_rotation)
    } else {
        let position = bone.bind_position
            + bone
                .position_property
                .and_then(|property| values.and_then(|values| values.get(&property)))
                .and_then(PropValue::vector)
                .unwrap_or(Vec2::ZERO);
        let rotation = bone.bind_rotation
            + bone
                .rotation_property
                .and_then(|property| values.and_then(|values| values.get(&property)))
                .and_then(PropValue::number)
                .unwrap_or(0.0);
        (position, rotation)
    };
    if let Some(parent) = bone.parent {
        let parent = bone_world(bones, parent, values, bind)?;
        Ok(BoneWorld {
            position: parent.position + rotate(position, parent.rotation),
            rotation: parent.rotation + rotation,
        })
    } else {
        Ok(BoneWorld { position, rotation })
    }
}

fn inverse(world: BoneWorld, point: Vec2) -> Vec2 {
    rotate(point - world.position, -world.rotation)
}

fn forward(world: BoneWorld, point: Vec2) -> Vec2 {
    rotate(point, world.rotation) + world.position
}

fn rotate(value: Vec2, degrees: f64) -> Vec2 {
    let (sin, cos) = degrees.to_radians().sin_cos();
    Vec2::new(value.x * cos - value.y * sin, value.x * sin + value.y * cos)
}

fn auto_weights(
    skin: &Skin,
    bind: &[BoneWorld],
    geometry: &PathGeometry,
) -> Vec<Vec<(usize, f64)>> {
    geometry
        .points()
        .iter()
        .map(|point| {
            let mut weights: Vec<(usize, f64)> = bind
                .iter()
                .enumerate()
                .map(|(index, bone)| {
                    let start = bone.position;
                    let end =
                        start + rotate(Vec2::new(skin.bones[index].length, 0.0), bone.rotation);
                    let distance = distance_to_segment(point.position, start, end);
                    (index, 1.0 / (distance + 1.0).powi(2))
                })
                .collect();
            weights.sort_by(|a, b| b.1.total_cmp(&a.1));
            weights.truncate(2);
            let total: f64 = weights.iter().map(|(_, weight)| weight).sum();
            if total > 0.0 {
                for (_, weight) in &mut weights {
                    *weight /= total;
                }
            }
            weights
        })
        .collect()
}

fn distance_to_segment(point: Vec2, start: Vec2, end: Vec2) -> f64 {
    let line = end - start;
    let length = line.length_squared();
    if length <= 1e-12 {
        return (point - start).length();
    }
    let t = ((point - start).x * line.x + (point - start).y * line.y) / length;
    let t = t.clamp(0.0, 1.0);
    (point - (start + line * t)).length()
}

impl BlendShapes {
    fn apply(
        &self,
        geometry: &PathGeometry,
        values: &BTreeMap<usize, PropValue>,
    ) -> Result<PathGeometry, RigError> {
        if self.shapes.len() > MAX_BLEND_SHAPES {
            return Err(RigError::message(format!(
                "part has more than {MAX_BLEND_SHAPES} blend shapes"
            )));
        }
        let mut points = geometry.points().to_vec();
        for shape in &self.shapes {
            if shape.points.len() != points.len() {
                return Err(RigError::message(format!(
                    "blend shape '{}' has {} points, expected {}",
                    shape.name,
                    shape.points.len(),
                    points.len()
                )));
            }
            let weight = values
                .get(&shape.weight)
                .and_then(PropValue::number)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            if weight == 0.0 {
                continue;
            }
            for (point, target) in points.iter_mut().zip(&shape.points) {
                *point = PathPoint::curved(
                    point.position.lerp(target.position, weight),
                    point.handle_in.lerp(target.handle_in, weight),
                    point.handle_out.lerp(target.handle_out, weight),
                );
            }
        }
        PathGeometry::new(points, geometry.closed(), geometry.cap(), geometry.join())
            .map_err(|error| RigError::message(error.to_string()))
    }
}

/// Resamples `target` to `count` points along its polyline so a blend shape
/// can use any authored topology.
#[must_use]
pub fn compatible_points(target: &PathGeometry, count: usize, closed: bool) -> Vec<PathPoint> {
    let source = target.flatten();
    if source.len() < 2 || count < 2 {
        return target.points().to_vec();
    }
    let mut points = source.clone();
    if closed {
        points.push(source[0]);
    }
    let mut lengths = Vec::with_capacity(points.len());
    let mut total = 0.0;
    lengths.push(0.0);
    for pair in points.windows(2) {
        total += (pair[1] - pair[0]).length();
        lengths.push(total);
    }
    if total <= 1e-12 {
        return target.points().to_vec();
    }
    (0..count)
        .map(|index| {
            #[allow(clippy::cast_precision_loss)]
            let distance = total * index as f64 / (count - 1) as f64;
            let segment = lengths
                .partition_point(|length| *length <= distance)
                .saturating_sub(1)
                .min(points.len() - 2);
            let span = lengths[segment + 1] - lengths[segment];
            let t = if span <= 1e-12 {
                0.0
            } else {
                (distance - lengths[segment]) / span
            };
            PathPoint::corner(points[segment].lerp(points[segment + 1], t))
        })
        .collect()
}

fn vector(values: &BTreeMap<usize, PropValue>, property: usize) -> Option<Vec2> {
    values.get(&property).and_then(PropValue::vector)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> PathGeometry {
        PathGeometry::new(
            vec![
                PathPoint::corner(Vec2::new(0.0, 0.0)),
                PathPoint::corner(Vec2::new(10.0, 0.0)),
                PathPoint::corner(Vec2::new(10.0, 10.0)),
                PathPoint::corner(Vec2::new(0.0, 10.0)),
            ],
            true,
            motioner_core::LineCap::Round,
            motioner_core::LineJoin::Round,
        )
        .unwrap()
    }

    #[test]
    fn lattice_moves_a_corner_and_keeps_topology() {
        let mut values = BTreeMap::new();
        values.insert(0, PropValue::Vector(Vec2::new(2.0, 0.0)));
        let lattice = Lattice {
            part: 0,
            corners: [0, 1, 2, 3],
            edges: [None; 4],
        };
        let deformed = lattice.apply(&square(), &values).unwrap();
        assert_eq!(deformed.points().len(), 4);
        assert!((deformed.points()[0].position.x - 2.0).abs() < 1e-9);
    }

    #[test]
    fn compatible_points_matches_the_requested_count() {
        let points = compatible_points(&square(), 8, true);
        assert_eq!(points.len(), 8);
    }
}
