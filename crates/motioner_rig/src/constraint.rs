//! Constraints: aim, two-bone IK, follow path, copy/limit, distance and
//! spring follow-through (D-0060).
//!
//! Constraints run after deformers and mutate part transforms only. Each
//! constraint declares its part and property indices, so the bake writes the
//! same ordinary keys as drivers. Spring state is part of the constraint
//! evaluation state and is replayed from frame zero, keeping seeks exact.

use std::collections::BTreeMap;

use motioner_core::{PathGeometry, Vec2};

use crate::eval::RigError;
use crate::{PartBase, PartOutput, PropValue};

/// Maximum path length one follow-path constraint may flatten.
pub const MAX_FOLLOW_POINTS: usize = 65_536;

/// A constraint in the part pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum Constraint {
    /// Offsets a part by a Vec2 property scaled by `radius`.
    Aim {
        /// Part index.
        part: usize,
        /// Vec2 property.
        property: usize,
        /// Maximum offset in pixels.
        radius: f64,
    },
    /// Two-bone IK on a root and a middle part.
    TwoBoneIk {
        /// Upper/root part.
        upper: usize,
        /// Lower/middle part.
        lower: usize,
        /// Vec2 target property.
        target: usize,
        /// Optional Vec2 pole property.
        pole: Option<usize>,
        /// Length from the lower joint to the tip.
        tip_length: f64,
    },
    /// Places a part at a fraction of another part's path.
    FollowPath {
        /// Part to place.
        part: usize,
        /// Part whose geometry is the path.
        path: usize,
        /// Number property with progress in `0..=1`.
        progress: usize,
        /// Whether to rotate the part along the tangent.
        orient: bool,
    },
    /// Copies transform fields from another part.
    Copy {
        /// Part written.
        part: usize,
        /// Source part.
        source: usize,
        /// Copy the position offset.
        position: bool,
        /// Copy the rotation offset.
        rotation: bool,
        /// Copy the scale multiplier.
        scale: bool,
    },
    /// Clamps the position offset and rotation offset.
    Limit {
        /// Part written.
        part: usize,
        /// Minimum position offset.
        min: Vec2,
        /// Maximum position offset.
        max: Vec2,
        /// Minimum rotation offset in degrees.
        rotation_min: f64,
        /// Maximum rotation offset in degrees.
        rotation_max: f64,
    },
    /// Keeps a fixed distance between two parts.
    Distance {
        /// Part written.
        part: usize,
        /// Source part.
        source: usize,
        /// Distance in local pixels.
        distance: f64,
    },
    /// Spring follow-through towards a Vec2 property.
    Spring {
        /// Part written.
        part: usize,
        /// Vec2 target property.
        target: usize,
        /// Spring stiffness.
        stiffness: f64,
        /// Damping coefficient.
        damping: f64,
        /// Mass.
        mass: f64,
    },
}

/// Mutable spring state, one entry per spring constraint.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SpringState {
    /// Current offset.
    pub position: Vec2,
    /// Current velocity in pixels/frame.
    pub velocity: Vec2,
    /// Whether the first frame initialised the state.
    pub initialized: bool,
}

/// Evaluates every constraint in order.
///
/// # Errors
///
/// Returns [`RigError`] for a missing part, a non-finite target or invalid
/// spring/IK parameters.
#[allow(clippy::too_many_lines)] // one arm per constraint, kept together
pub fn apply(
    constraints: &[Constraint],
    bases: &[PartBase],
    parts: &mut [PartOutput],
    values: &BTreeMap<usize, PropValue>,
    states: &mut [SpringState],
) -> Result<(), RigError> {
    let mut state_index = 0;
    for constraint in constraints {
        match constraint {
            Constraint::Aim {
                part,
                property,
                radius,
            } => {
                let Some(part) = parts.get_mut(*part) else {
                    return Err(RigError::message(format!(
                        "aim: part {part} does not exist"
                    )));
                };
                if let Some(look) = values.get(property).and_then(PropValue::vector) {
                    part.position = part.position + look * *radius;
                    part.touched = true;
                }
            }
            Constraint::TwoBoneIk {
                upper,
                lower,
                target,
                pole,
                tip_length,
            } => {
                two_bone_ik(
                    bases,
                    parts,
                    *upper,
                    *lower,
                    *target,
                    *pole,
                    *tip_length,
                    values,
                )?;
            }
            Constraint::FollowPath {
                part,
                path,
                progress,
                orient,
            } => {
                follow_path(bases, parts, *part, *path, *progress, *orient, values)?;
            }
            Constraint::Copy {
                part,
                source,
                position,
                rotation,
                scale,
            } => {
                let Some(source_part) = parts.get(*source).cloned() else {
                    return Err(RigError::message(format!(
                        "copy: source {source} does not exist"
                    )));
                };
                let Some(part) = parts.get_mut(*part) else {
                    return Err(RigError::message(format!(
                        "copy: part {part} does not exist"
                    )));
                };
                if *position {
                    part.position = source_part.position;
                }
                if *rotation {
                    part.rotation = source_part.rotation;
                }
                if *scale {
                    part.scale = source_part.scale;
                }
                part.touched = true;
            }
            Constraint::Limit {
                part,
                min,
                max,
                rotation_min,
                rotation_max,
            } => {
                let Some(part) = parts.get_mut(*part) else {
                    return Err(RigError::message(format!(
                        "limit: part {part} does not exist"
                    )));
                };
                part.position.x = part.position.x.clamp(min.x, max.x);
                part.position.y = part.position.y.clamp(min.y, max.y);
                part.rotation = part.rotation.clamp(*rotation_min, *rotation_max);
                part.touched = true;
            }
            Constraint::Distance {
                part,
                source,
                distance,
            } => {
                let Some(source_part) = parts.get(*source).cloned() else {
                    return Err(RigError::message(format!(
                        "distance: source {source} does not exist"
                    )));
                };
                let Some(target_part) = parts.get_mut(*part) else {
                    return Err(RigError::message(format!(
                        "distance: part {part} does not exist"
                    )));
                };
                let Some(source_base) = bases.get(*source) else {
                    return Err(RigError::message(format!(
                        "distance: source {source} does not exist"
                    )));
                };
                let base = bases.get(*part).map_or(Vec2::ZERO, |base| base.position);
                let mut direction =
                    (base + target_part.position) - (source_part.position + source_base.position);
                if direction.length() <= 1e-9 {
                    direction = Vec2::new(1.0, 0.0);
                }
                let target = source_base.position
                    + source_part.position
                    + direction * (*distance / direction.length());
                target_part.position = target - base;
                target_part.touched = true;
            }
            Constraint::Spring {
                part,
                target,
                stiffness,
                damping,
                mass,
            } => {
                let Some(part) = parts.get_mut(*part) else {
                    return Err(RigError::message(format!(
                        "spring: part {part} does not exist"
                    )));
                };
                let Some(state) = states.get_mut(state_index) else {
                    return Err(RigError::message("spring state is missing"));
                };
                state_index += 1;
                let Some(goal) = values.get(target).and_then(PropValue::vector) else {
                    continue;
                };
                if !goal.is_finite()
                    || !stiffness.is_finite()
                    || !damping.is_finite()
                    || !mass.is_finite()
                    || *mass <= 0.0
                {
                    return Err(RigError::message("spring has invalid parameters"));
                }
                if !state.initialized {
                    state.position = part.position;
                    state.velocity = Vec2::ZERO;
                    state.initialized = true;
                }
                let force = (goal - state.position) * *stiffness - state.velocity * *damping;
                state.velocity = state.velocity + force * (1.0 / *mass);
                state.position = state.position + state.velocity;
                part.position = state.position;
                part.touched = true;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // the solver's flat inputs
fn two_bone_ik(
    bases: &[PartBase],
    parts: &mut [PartOutput],
    upper: usize,
    lower: usize,
    target: usize,
    pole: Option<usize>,
    tip_length: f64,
    values: &BTreeMap<usize, PropValue>,
) -> Result<(), RigError> {
    let Some(root) = bases.get(upper).map(|base| base.position) else {
        return Err(RigError::message(format!(
            "ik: upper {upper} does not exist"
        )));
    };
    let Some(elbow_base) = bases.get(lower).map(|base| base.position) else {
        return Err(RigError::message(format!(
            "ik: lower {lower} does not exist"
        )));
    };
    let Some(target) = values.get(&target).and_then(PropValue::vector) else {
        return Ok(());
    };
    let length1 = (elbow_base - root).length();
    let length2 = tip_length.max(0.0);
    if !target.is_finite() || length1 <= 1e-9 || length2 <= 1e-9 {
        return Err(RigError::message("ik: invalid target or bone length"));
    }
    let delta = target - root;
    let distance = delta
        .length()
        .clamp((length1 - length2).abs(), length1 + length2);
    let direction = if delta.length() <= 1e-9 {
        Vec2::new(1.0, 0.0)
    } else {
        delta * (1.0 / delta.length())
    };
    let along = (length1 * length1 - length2 * length2 + distance * distance) / (2.0 * distance);
    let height = (length1 * length1 - along * along).max(0.0).sqrt();
    let perpendicular = Vec2::new(-direction.y, direction.x);
    let sign = pole
        .and_then(|property| values.get(&property).and_then(PropValue::vector))
        .map_or(1.0, |pole| {
            if (pole - root).x * perpendicular.x + (pole - root).y * perpendicular.y >= 0.0 {
                1.0
            } else {
                -1.0
            }
        });
    let elbow = root + direction * along + perpendicular * (height * sign);
    let Some(upper_part) = parts.get_mut(upper) else {
        return Err(RigError::message(format!(
            "ik: upper {upper} does not exist"
        )));
    };
    upper_part.rotation += angle_between(elbow_base - root, elbow - root);
    upper_part.touched = true;
    let Some(lower_part) = parts.get_mut(lower) else {
        return Err(RigError::message(format!(
            "ik: lower {lower} does not exist"
        )));
    };
    lower_part.position = elbow - elbow_base;
    lower_part.rotation += angle_between(Vec2::new(1.0, 0.0), target - elbow);
    lower_part.touched = true;
    Ok(())
}

fn follow_path(
    bases: &[PartBase],
    parts: &mut [PartOutput],
    part: usize,
    path: usize,
    progress: usize,
    orient: bool,
    values: &BTreeMap<usize, PropValue>,
) -> Result<(), RigError> {
    let geometry: PathGeometry = parts
        .get(path)
        .and_then(|part| part.geometry.clone())
        .or_else(|| bases.get(path).and_then(|base| base.geometry.clone()))
        .ok_or_else(|| RigError::message(format!("follow path: {path} has no geometry")))?;
    let points = geometry.flatten();
    if points.len() > MAX_FOLLOW_POINTS {
        return Err(RigError::message("follow path: too many points"));
    }
    let progress = values
        .get(&progress)
        .and_then(PropValue::number)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let distance = progress * path_length(&points);
    let (point, tangent) = point_at(&points, distance);
    let Some(target) = parts.get_mut(part) else {
        return Err(RigError::message(format!(
            "follow path: part {part} does not exist"
        )));
    };
    target.position = point - bases[part].position;
    if orient {
        target.rotation = angle_between(Vec2::new(1.0, 0.0), tangent) - bases[part].rotation;
    }
    target.touched = true;
    Ok(())
}

fn path_length(points: &[Vec2]) -> f64 {
    points
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).length())
        .sum()
}

fn point_at(points: &[Vec2], distance: f64) -> (Vec2, Vec2) {
    let mut travelled = 0.0;
    for pair in points.windows(2) {
        let length = (pair[1] - pair[0]).length();
        if travelled + length >= distance {
            let t = if length <= 1e-12 {
                0.0
            } else {
                (distance - travelled) / length
            };
            return (pair[0].lerp(pair[1], t), pair[1] - pair[0]);
        }
        travelled += length;
    }
    (*points.last().unwrap_or(&Vec2::ZERO), Vec2::new(1.0, 0.0))
}

fn angle_between(from: Vec2, to: Vec2) -> f64 {
    from.y
        .atan2(from.x)
        .mul_add(-1.0, to.y.atan2(to.x))
        .to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_clamps_offsets() {
        let bases = vec![PartBase::at("p", Vec2::ZERO)];
        let mut parts = vec![PartOutput {
            part: 0,
            position: Vec2::new(20.0, -20.0),
            rotation: 40.0,
            scale: Vec2::ONE,
            opacity: 1.0,
            stroke_width: None,
            geometry: None,
            touched: false,
        }];
        let constraints = [Constraint::Limit {
            part: 0,
            min: Vec2::new(-5.0, -5.0),
            max: Vec2::new(5.0, 5.0),
            rotation_min: -10.0,
            rotation_max: 10.0,
        }];
        apply(&constraints, &bases, &mut parts, &BTreeMap::new(), &mut []).unwrap();
        assert!((parts[0].position.x - 5.0).abs() < 1e-9);
        assert!((parts[0].position.y + 5.0).abs() < 1e-9);
        assert!((parts[0].rotation - 10.0).abs() < 1e-9);
    }

    #[test]
    fn spring_moves_towards_the_target() {
        let bases = vec![PartBase::at("p", Vec2::ZERO)];
        let mut parts = vec![PartOutput {
            part: 0,
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            opacity: 1.0,
            stroke_width: None,
            geometry: None,
            touched: false,
        }];
        let mut values = BTreeMap::new();
        values.insert(0, PropValue::Vector(Vec2::new(10.0, 0.0)));
        let constraints = [Constraint::Spring {
            part: 0,
            target: 0,
            stiffness: 0.5,
            damping: 0.2,
            mass: 1.0,
        }];
        let mut state = [SpringState::default()];
        apply(&constraints, &bases, &mut parts, &values, &mut state).unwrap();
        assert!(parts[0].position.x > 0.0);
    }
}
