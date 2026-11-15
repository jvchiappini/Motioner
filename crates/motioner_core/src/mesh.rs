//! Fixed-topology meshes for image and video nodes (D-0061).
//!
//! A mesh is a rectangular grid of `columns x rows` vertices with fixed UVs.
//! Positions live in normalised local space: the 2x2 default spans
//! `[-0.5, 0.5]` on both axes, matching the sprite quad. UVs span `[0, 1]`
//! and never animate. The position channel mirrors [`crate::GeometryChannel`]:
//! every key must keep the same vertex count.
//!
//! [`crate::GeometryChannel`]: crate::GeometryChannel

use crate::animation::Ease;
use crate::error::CoreError;
use crate::math::Vec2;
use crate::time::{frame_within_bounds, Frame};

/// Largest supported grid edge. Keeps one mesh's point count bounded.
pub const MAX_MESH_EDGE: u32 = 128;

/// One animated mesh key: positions with the ease arriving at the key.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshKey {
    /// Composition frame.
    pub frame: Frame,
    /// Row-major vertex positions in normalised local space.
    pub points: Vec<Vec2>,
    /// Ease from the previous key to this one.
    pub ease: Ease,
}

/// A rectangular mesh sampled at one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshSample {
    /// Grid columns.
    pub columns: u32,
    /// Grid rows.
    pub rows: u32,
    /// Row-major UVs, `columns * rows` entries.
    pub uvs: Vec<[f64; 2]>,
    /// Row-major positions in normalised local space.
    pub points: Vec<Vec2>,
}

impl MeshSample {
    /// Triangle indices for the grid, two triangles per cell.
    #[must_use]
    pub fn indices(&self) -> Vec<u32> {
        let mut indices = Vec::with_capacity(
            ((self.columns.saturating_sub(1)) * (self.rows.saturating_sub(1)) * 6) as usize,
        );
        for row in 0..self.rows.saturating_sub(1) {
            for column in 0..self.columns.saturating_sub(1) {
                let top_left = row * self.columns + column;
                let top_right = top_left + 1;
                let bottom_left = top_left + self.columns;
                let bottom_right = bottom_left + 1;
                indices.extend_from_slice(&[
                    top_left,
                    bottom_left,
                    top_right,
                    top_right,
                    bottom_left,
                    bottom_right,
                ]);
            }
        }
        indices
    }
}

/// An image or video mesh with fixed topology and an animatable position
/// channel (D-0061).
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    columns: u32,
    rows: u32,
    uvs: Vec<[f64; 2]>,
    points: Vec<Vec2>,
    keys: Vec<MeshKey>,
}

impl Mesh {
    /// The default 2x2 quad. It matches the sprite path exactly.
    ///
    /// # Panics
    ///
    /// Never in practice: 2x2 is a valid grid. Kept as an infallible
    /// convenience for Default.
    #[must_use]
    pub fn quad() -> Self {
        Self::grid(2, 2).expect("2x2 is a valid mesh")
    }

    /// A rectangular grid with uniform UVs and unit-square positions.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidContentValue`] when an edge is below 2 or
    /// above [`MAX_MESH_EDGE`].
    pub fn grid(columns: u32, rows: u32) -> Result<Self, CoreError> {
        check_grid(columns, rows)?;
        let mut points = Vec::with_capacity((columns * rows) as usize);
        let mut uvs = Vec::with_capacity((columns * rows) as usize);
        for row in 0..rows {
            for column in 0..columns {
                let u = f64::from(column) / f64::from(columns - 1);
                let v = f64::from(row) / f64::from(rows - 1);
                points.push(Vec2::new(u - 0.5, v - 0.5));
                uvs.push([u, v]);
            }
        }
        Ok(Self {
            columns,
            rows,
            uvs,
            points,
            keys: Vec::new(),
        })
    }

    /// A grid with authored positions; UVs stay uniform.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidContentValue`] for a bad grid or a point
    /// count that does not equal `columns * rows`, and
    /// [`CoreError::InvalidGeometry`] for a non-finite coordinate.
    pub fn with_points(columns: u32, rows: u32, points: Vec<Vec2>) -> Result<Self, CoreError> {
        let mut mesh = Self::grid(columns, rows)?;
        mesh.set_points(points)?;
        Ok(mesh)
    }

    /// Grid columns.
    #[must_use]
    pub const fn columns(&self) -> u32 {
        self.columns
    }

    /// Grid rows.
    #[must_use]
    pub const fn rows(&self) -> u32 {
        self.rows
    }

    /// Row-major UVs.
    #[must_use]
    pub fn uvs(&self) -> &[[f64; 2]] {
        &self.uvs
    }

    /// Static positions (the value when there are no keys).
    #[must_use]
    pub fn points(&self) -> &[Vec2] {
        &self.points
    }

    /// Keys, sorted by strictly increasing frame.
    #[must_use]
    pub fn keys(&self) -> &[MeshKey] {
        &self.keys
    }

    /// Whether the channel has keys.
    #[must_use]
    pub fn is_animated(&self) -> bool {
        !self.keys.is_empty()
    }

    /// Whether this is still the undeformed 2x2 quad.
    #[must_use]
    pub fn is_default_quad(&self) -> bool {
        self.columns == 2
            && self.rows == 2
            && !self.is_animated()
            && self.points
                == [
                    Vec2::new(-0.5, -0.5),
                    Vec2::new(0.5, -0.5),
                    Vec2::new(-0.5, 0.5),
                    Vec2::new(0.5, 0.5),
                ]
    }

    /// Replaces the static positions, keeping topology and UVs.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::GeometryTopologyMismatch`] when the count differs
    /// and [`CoreError::InvalidGeometry`] for a non-finite coordinate.
    pub fn set_points(&mut self, points: Vec<Vec2>) -> Result<(), CoreError> {
        check_points(self.points.len(), &points)?;
        self.points = points;
        Ok(())
    }

    /// Inserts a position key, replacing one at the same frame.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::FrameOutOfRange`] outside the frame bounds and
    /// [`CoreError::GeometryTopologyMismatch`] when the count differs.
    pub fn insert_key(
        &mut self,
        frame: Frame,
        points: Vec<Vec2>,
        ease: Ease,
    ) -> Result<(), CoreError> {
        if !frame_within_bounds(frame) {
            return Err(CoreError::FrameOutOfRange { frame });
        }
        check_points(self.points.len(), &points)?;
        let key = MeshKey {
            frame,
            points,
            ease,
        };
        match self
            .keys
            .binary_search_by_key(&frame, |existing| existing.frame)
        {
            Ok(index) => self.keys[index] = key,
            Err(index) => self.keys.insert(index, key),
        }
        Ok(())
    }

    /// Removes the key at `frame`, if any.
    pub fn remove_key(&mut self, frame: Frame) -> bool {
        match self.keys.binary_search_by_key(&frame, |key| key.frame) {
            Ok(index) => {
                self.keys.remove(index);
                true
            }
            Err(_) => false,
        }
    }

    /// Samples the position channel at `time` (composition frames).
    ///
    /// `ease` applies the arriving key's curve. Returns `None` only if the
    /// stored topology was corrupted, which the insertion path prevents.
    #[must_use]
    pub fn sample(&self, time: f64, ease: impl FnOnce(Ease, f64) -> f64) -> Option<MeshSample> {
        let points = if let (Some(first), Some(last)) = (self.keys.first(), self.keys.last()) {
            let first_time = frame_seconds(first.frame);
            let last_time = frame_seconds(last.frame);
            if time <= first_time {
                first.points.clone()
            } else if time >= last_time {
                last.points.clone()
            } else {
                let next = self
                    .keys
                    .partition_point(|key| frame_seconds(key.frame) <= time);
                let (a, b) = (&self.keys[next - 1], &self.keys[next]);
                let span = frame_seconds(b.frame - a.frame);
                let t = ease(b.ease, (time - frame_seconds(a.frame)) / span);
                a.points
                    .iter()
                    .zip(&b.points)
                    .map(|(a, b)| a.lerp(*b, t))
                    .collect()
            }
        } else {
            self.points.clone()
        };
        Some(MeshSample {
            columns: self.columns,
            rows: self.rows,
            uvs: self.uvs.clone(),
            points,
        })
    }

    /// The sampled mesh converted to local pixels for a `width x height` box.
    #[must_use]
    pub fn sample_pixels(
        &self,
        time: f64,
        width: f64,
        height: f64,
        ease: impl FnOnce(Ease, f64) -> f64,
    ) -> Option<MeshSample> {
        let mut mesh = self.sample(time, ease)?;
        for point in &mut mesh.points {
            point.x *= width;
            point.y *= height;
        }
        Some(mesh)
    }
}

impl Default for Mesh {
    fn default() -> Self {
        Self::quad()
    }
}

fn check_grid(columns: u32, rows: u32) -> Result<(), CoreError> {
    if !(2..=MAX_MESH_EDGE).contains(&columns) || !(2..=MAX_MESH_EDGE).contains(&rows) {
        return Err(CoreError::InvalidContentValue {
            field: "mesh".to_owned(),
            reason: "columns and rows must be between 2 and 128",
        });
    }
    Ok(())
}

fn check_points(expected: usize, points: &[Vec2]) -> Result<(), CoreError> {
    if points.len() != expected {
        return Err(CoreError::GeometryTopologyMismatch {
            expected_points: expected,
            got_points: points.len(),
            expected_closed: false,
            got_closed: false,
        });
    }
    if points.iter().any(|point| !point.is_finite()) {
        return Err(CoreError::InvalidGeometry { field: "mesh" });
    }
    Ok(())
}

fn frame_seconds(frame: Frame) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    {
        frame as f64
    }
}

/// Samples a mesh channel exactly like [`crate::GeometryChannel`] does for paths.
#[must_use]
pub fn sample_mesh_at(
    mesh: &Mesh,
    time: f64,
    ease: impl FnOnce(Ease, f64) -> f64,
) -> Option<MeshSample> {
    mesh.sample(time, ease)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(_: Ease, t: f64) -> f64 {
        t
    }

    #[test]
    fn default_quad_has_four_corner_vertices() {
        let mesh = Mesh::quad();
        assert!(mesh.is_default_quad());
        assert_eq!(mesh.points().len(), 4);
        assert_eq!(
            mesh.uvs(),
            &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]]
        );
    }

    #[test]
    fn mesh_keys_interpolate_per_vertex() {
        let mut mesh = Mesh::grid(2, 2).unwrap();
        mesh.insert_key(0, mesh.points().to_vec(), Ease::Linear)
            .unwrap();
        let moved = mesh
            .points()
            .iter()
            .map(|point| *point + Vec2::new(10.0, 0.0))
            .collect();
        mesh.insert_key(10, moved, Ease::Linear).unwrap();
        let sample = mesh.sample(5.0, linear).unwrap();
        assert_eq!(sample.points[0], Vec2::new(4.5, -0.5));
    }

    #[test]
    fn mesh_topology_is_fixed() {
        let mut mesh = Mesh::grid(3, 2).unwrap();
        let error = mesh.insert_key(0, vec![Vec2::ZERO; 5], Ease::Linear);
        assert!(matches!(
            error,
            Err(CoreError::GeometryTopologyMismatch { .. })
        ));
    }

    #[test]
    fn mesh_scales_to_pixels() {
        let mesh = Mesh::quad();
        let sample = mesh.sample_pixels(0.0, 100.0, 50.0, linear).unwrap();
        assert_eq!(sample.points[0], Vec2::new(-50.0, -25.0));
        assert_eq!(sample.points[3], Vec2::new(50.0, 25.0));
    }
}
