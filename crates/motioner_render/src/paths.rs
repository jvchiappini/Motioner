//! Vector path tessellation and its per-node cache (D-0029).
//!
//! A path item is tessellated in local space into triangles for the fill and
//! the stroke, then each triangle is transformed by the item matrix and
//! submitted as a `Pure2D` triangle instance. Edges shared by two triangles
//! are marked hard and only boundary edges are anti-aliased, so an opaque
//! fill has no seams.
//!
//! Tessellation is cached per node. The key hashes the resolved geometry,
//! trim, dash, stroke width and cap/join, so an animated trim only
//! re-tessellates the frames whose value changed.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use ferrous_2d::render::ShapeInstance;
use lyon_tessellation::path::Path;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use motioner_core::{DashSpec, NodeId, PathGeometry, PathPiece, PathPoint, TrimSpec, Vec2};
use motioner_eval::{Affine2, ResolvedPath};

/// One triangle of a tessellated path, with its anti-aliasing edge mask.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LocalTriangle {
    /// Vertices in the path's local space.
    pub vertices: [Vec2; 3],
    /// Bit 0 = edge `a→b`, bit 1 = `b→c`, bit 2 = `c→a` get anti-aliasing.
    pub edges: u8,
}

/// The tessellated fill and stroke of one path at one set of values.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PathMeshes {
    /// Triangles of the filled area.
    pub fill: Vec<LocalTriangle>,
    /// Triangles of the stroked outline.
    pub stroke: Vec<LocalTriangle>,
}

#[derive(Debug)]
struct Entry {
    key: u64,
    meshes: PathMeshes,
}

/// Per-renderer cache of path tessellations.
#[derive(Debug, Default)]
pub(crate) struct PathCache {
    entries: HashMap<NodeId, Entry>,
}

impl PathCache {
    /// The meshes of `path`, tessellating them again only when the key
    /// changed since the last frame.
    pub fn meshes(&mut self, node: NodeId, path: &ResolvedPath) -> &PathMeshes {
        let key = cache_key(path);
        let entry = self.entries.entry(node).or_insert_with(|| Entry {
            key,
            meshes: tessellate(path),
        });
        if entry.key != key {
            entry.key = key;
            entry.meshes = tessellate(path);
        }
        &entry.meshes
    }
}

fn cache_key(path: &ResolvedPath) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for point in path.geometry.points() {
        hash_f64(&mut hasher, point.position.x);
        hash_f64(&mut hasher, point.position.y);
        hash_f64(&mut hasher, point.handle_in.x);
        hash_f64(&mut hasher, point.handle_in.y);
        hash_f64(&mut hasher, point.handle_out.x);
        hash_f64(&mut hasher, point.handle_out.y);
    }
    path.geometry.closed().hash(&mut hasher);
    path.geometry.cap().hash(&mut hasher);
    path.geometry.join().hash(&mut hasher);
    hash_trim(&mut hasher, path.trim);
    hash_dash(&mut hasher, path.dash);
    path.paint.fill.is_some().hash(&mut hasher);
    match path.paint.stroke {
        Some(stroke) => {
            true.hash(&mut hasher);
            hash_f64(&mut hasher, stroke.width);
            match stroke.non_scaling {
                Some(reference) => {
                    true.hash(&mut hasher);
                    hash_f64(&mut hasher, reference);
                }
                None => false.hash(&mut hasher),
            }
        }
        None => false.hash(&mut hasher),
    }
    hasher.finish()
}

fn hash_f64(hasher: &mut impl Hasher, value: f64) {
    value.to_bits().hash(hasher);
}

fn hash_trim(hasher: &mut impl Hasher, trim: TrimSpec) {
    hash_f64(hasher, trim.start);
    hash_f64(hasher, trim.end);
    hash_f64(hasher, trim.offset);
}

fn hash_dash(hasher: &mut impl Hasher, dash: DashSpec) {
    hash_f64(hasher, dash.length);
    hash_f64(hasher, dash.offset);
}

fn tessellate(path: &ResolvedPath) -> PathMeshes {
    let outline = path.geometry.outline(path.trim, path.dash);
    let mut meshes = PathMeshes::default();
    if path.paint.fill.is_some() {
        let mut triangles = Vec::new();
        for piece in &outline.fill {
            fill_piece(piece, &mut triangles);
        }
        meshes.fill = classify_edges(triangles);
    }
    if let Some(stroke) = path
        .paint
        .stroke
        .filter(|stroke| stroke.width > 0.0 && stroke.non_scaling.is_none())
    {
        let mut triangles = Vec::new();
        for piece in &outline.strokes {
            stroke_piece(
                piece,
                stroke.width,
                path.geometry.cap(),
                path.geometry.join(),
                &mut triangles,
            );
        }
        meshes.stroke = classify_edges(triangles);
    }
    meshes
}

/// Triangles of a non-scaling stroke, tessellated in composition space.
///
/// `vector-effect="non-scaling-stroke"` keeps the authored width when the node
/// or an ancestor scales the path, including a non-uniform scale. Stroking in
/// local space and transforming the triangles cannot do that: the scale would
/// squash one side of the outline to a hairline. This transforms the path
/// first, so the width stays in composition pixels; the caller submits the
/// triangles with the identity transform. It is not cached because the result
/// depends on the item's transform, which changes whenever the node animates.
pub(crate) fn non_scaling_stroke(path: &ResolvedPath, model: Affine2) -> Vec<LocalTriangle> {
    let Some(stroke) = path.paint.stroke.filter(|stroke| stroke.width > 0.0) else {
        return Vec::new();
    };
    let points: Vec<PathPoint> = path
        .geometry
        .points()
        .iter()
        .map(|point| {
            PathPoint::curved(
                model.apply(point.position),
                model.apply_vector(point.handle_in),
                model.apply_vector(point.handle_out),
            )
        })
        .collect();
    let Ok(geometry) = PathGeometry::new(
        points,
        path.geometry.closed(),
        path.geometry.cap(),
        path.geometry.join(),
    ) else {
        return Vec::new();
    };
    let outline = geometry.outline(path.trim, path.dash);
    let width = stroke.width * stroke.non_scaling.unwrap_or(1.0);
    let mut triangles = Vec::new();
    for piece in &outline.strokes {
        stroke_piece(
            piece,
            width,
            geometry.cap(),
            geometry.join(),
            &mut triangles,
        );
    }
    classify_edges(triangles)
}

fn fill_piece(piece: &PathPiece, out: &mut Vec<[Vec2; 3]>) {
    if piece.points.len() < 3 {
        return;
    }
    let path = build_path(piece, true);
    let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();
    let options = FillOptions::tolerance(0.05).with_fill_rule(FillRule::NonZero);
    let result = tessellator.tessellate_path(
        &path,
        &options,
        &mut BuffersBuilder::new(&mut buffers, |vertex: FillVertex<'_>| {
            vertex.position().to_array()
        }),
    );
    if result.is_err() {
        return;
    }
    push_triangles(&buffers, out);
}

fn stroke_piece(
    piece: &PathPiece,
    width: f64,
    cap: motioner_core::LineCap,
    join: motioner_core::LineJoin,
    out: &mut Vec<[Vec2; 3]>,
) {
    if piece.points.len() < 2 {
        return;
    }
    let path = build_path(piece, piece.closed);
    let options = StrokeOptions::tolerance(0.05)
        .with_line_width(super::space::to_f32(width))
        .with_line_cap(match cap {
            motioner_core::LineCap::Butt => LineCap::Butt,
            motioner_core::LineCap::Round => LineCap::Round,
            motioner_core::LineCap::Square => LineCap::Square,
        })
        .with_line_join(match join {
            motioner_core::LineJoin::Miter => LineJoin::Miter,
            motioner_core::LineJoin::Bevel => LineJoin::Bevel,
            motioner_core::LineJoin::Round => LineJoin::Round,
        });
    let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut tessellator = StrokeTessellator::new();
    let result = tessellator.tessellate_path(
        &path,
        &options,
        &mut BuffersBuilder::new(&mut buffers, |vertex: StrokeVertex<'_, '_>| {
            vertex.position().to_array()
        }),
    );
    if result.is_err() {
        return;
    }
    push_triangles(&buffers, out);
}

fn build_path(piece: &PathPiece, close: bool) -> Path {
    let mut builder = Path::builder();
    let mut points = piece.points.iter();
    if let Some(first) = points.next() {
        builder.begin(lyon_point(*first));
        for point in points {
            builder.line_to(lyon_point(*point));
        }
        if close {
            builder.close();
        } else {
            builder.end(false);
        }
    }
    builder.build()
}

fn lyon_point(point: Vec2) -> lyon_tessellation::math::Point {
    lyon_tessellation::math::point(super::space::to_f32(point.x), super::space::to_f32(point.y))
}

fn push_triangles(buffers: &VertexBuffers<[f32; 2], u32>, out: &mut Vec<[Vec2; 3]>) {
    for triangle in buffers.indices.chunks_exact(3) {
        let vertex = |index: u32| {
            let position = buffers.vertices[index as usize];
            Vec2::new(f64::from(position[0]), f64::from(position[1]))
        };
        let tri = [
            vertex(triangle[0]),
            vertex(triangle[1]),
            vertex(triangle[2]),
        ];
        if triangle_area(tri) > 1e-9 {
            out.push(tri);
        }
    }
}

fn triangle_area([a, b, c]: [Vec2; 3]) -> f64 {
    ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() * 0.5
}

/// Marks an edge anti-aliased only when it belongs to one triangle.
fn classify_edges(triangles: Vec<[Vec2; 3]>) -> Vec<LocalTriangle> {
    let mut counts: HashMap<(u64, u64), u8> = HashMap::new();
    for triangle in &triangles {
        for edge in edges(triangle) {
            *counts.entry(edge).or_insert(0) += 1;
        }
    }
    triangles
        .into_iter()
        .map(|triangle| {
            let mut mask = 0u8;
            for (bit, edge) in edges(&triangle).into_iter().enumerate() {
                if counts.get(&edge) == Some(&1) {
                    mask |= 1 << bit;
                }
            }
            LocalTriangle {
                vertices: triangle,
                edges: mask,
            }
        })
        .collect()
}

fn edges(triangle: &[Vec2; 3]) -> [(u64, u64); 3] {
    [
        edge_key(triangle[0], triangle[1]),
        edge_key(triangle[1], triangle[2]),
        edge_key(triangle[2], triangle[0]),
    ]
}

fn edge_key(a: Vec2, b: Vec2) -> (u64, u64) {
    let key = |point: Vec2| {
        let x = point.x.to_bits();
        let y = point.y.to_bits();
        x.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ y
    };
    let (a, b) = (key(a), key(b));
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Builds the engine triangle instance of one local triangle under `model`,
/// at depth `z`.
pub(crate) fn instance(
    model: motioner_eval::Affine2,
    triangle: &LocalTriangle,
    color: [f32; 4],
    z: f32,
) -> ShapeInstance {
    let vertex = |point: Vec2| {
        let world = model.apply(point);
        glam::Vec2::new(super::space::to_f32(world.x), super::space::to_f32(world.y))
    };
    let [a, b, c] = triangle.vertices;
    let mut instance =
        ShapeInstance::polygon_triangle(vertex(a), vertex(b), vertex(c), color, triangle.edges);
    instance.transform_c3[2] = z;
    instance
}

/// Local-space bounds of a path, expanded by half the stroke width.
#[must_use]
pub fn path_bounds(path: &ResolvedPath) -> Option<[f64; 4]> {
    let mut bounds = path.geometry.local_bounds()?;
    if let Some(stroke) = path.paint.stroke {
        let half = stroke.width * 0.5;
        bounds[0] -= half;
        bounds[1] -= half;
        bounds[2] += half;
        bounds[3] += half;
    }
    Some(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::{Color, LineCap as CoreCap, LineJoin as CoreJoin, PathGeometry, PathPoint};
    use motioner_eval::{ResolvedFill, ResolvedPaint, ResolvedStroke};

    fn path(geometry: PathGeometry, fill: bool, stroke: Option<f64>) -> ResolvedPath {
        ResolvedPath {
            geometry,
            trim: TrimSpec::FULL,
            dash: DashSpec::default(),
            paint: ResolvedPaint {
                fill: fill.then_some(ResolvedFill::Solid(Color::WHITE)),
                stroke: stroke.map(|width| ResolvedStroke {
                    paint: ResolvedFill::Solid(Color::WHITE),
                    width,
                    non_scaling: None,
                }),
            },
        }
    }

    fn square() -> PathGeometry {
        PathGeometry::new(
            vec![
                PathPoint::corner(Vec2::new(0.0, 0.0)),
                PathPoint::corner(Vec2::new(100.0, 0.0)),
                PathPoint::corner(Vec2::new(100.0, 100.0)),
                PathPoint::corner(Vec2::new(0.0, 100.0)),
            ],
            true,
            CoreCap::Butt,
            CoreJoin::Miter,
        )
        .unwrap()
    }

    fn cache_key_of(path: &ResolvedPath) -> u64 {
        cache_key(path)
    }

    #[test]
    fn an_open_path_fills_and_strokes() {
        let geometry = PathGeometry::new(
            vec![
                PathPoint::corner(Vec2::new(0.0, 0.0)),
                PathPoint::corner(Vec2::new(100.0, 0.0)),
                PathPoint::corner(Vec2::new(100.0, 100.0)),
            ],
            false,
            CoreCap::Round,
            CoreJoin::Round,
        )
        .unwrap();
        let meshes = tessellate(&path(geometry, true, Some(4.0)));
        assert!(!meshes.fill.is_empty());
        assert!(!meshes.stroke.is_empty());
        let bounds = path_bounds(&path(square(), false, Some(10.0))).unwrap();
        for (value, expected) in bounds.iter().zip([-5.0, -5.0, 105.0, 105.0]) {
            assert!((value - expected).abs() < 1e-9, "{bounds:?}");
        }
    }

    #[test]
    fn internal_edges_are_hard_and_the_boundary_is_soft() {
        let meshes = tessellate(&path(square(), true, None));
        assert!(meshes.fill.len() >= 2);
        assert!(
            meshes.fill.iter().all(|triangle| triangle.edges != 0),
            "a fill has a soft boundary"
        );
        assert!(
            meshes.fill.iter().any(|triangle| triangle.edges != 0b111),
            "at least one internal edge stays hard"
        );
    }

    #[test]
    fn the_cache_reuses_meshes_until_values_change() {
        let mut cache = PathCache::default();
        let node = NodeId::from_raw(0);
        let resolved = path(square(), true, Some(2.0));
        let first = cache.meshes(node, &resolved).fill.clone();
        let second = cache.meshes(node, &resolved).fill.clone();
        assert_eq!(first, second);
        let mut changed = resolved.clone();
        changed.trim.end = 0.5;
        assert_ne!(cache_key_of(&resolved), cache_key_of(&changed));
        let third = cache.meshes(node, &changed).stroke.clone();
        assert!(!third.is_empty());
    }

    #[test]
    fn an_empty_trim_tessellates_nothing() {
        let mut resolved = path(square(), true, Some(2.0));
        resolved.trim = TrimSpec::new(0.3, 0.3);
        let meshes = tessellate(&resolved);
        assert!(meshes.fill.is_empty());
        assert!(meshes.stroke.is_empty());
    }

    #[test]
    fn non_scaling_strokes_ignore_a_non_uniform_scale() {
        let geometry = PathGeometry::new(
            vec![
                PathPoint::corner(Vec2::new(0.0, 0.0)),
                PathPoint::corner(Vec2::new(100.0, 0.0)),
            ],
            false,
            CoreCap::Butt,
            CoreJoin::Miter,
        )
        .unwrap();
        let mut resolved = path(geometry, false, Some(10.0));
        resolved.paint.stroke.as_mut().unwrap().non_scaling = Some(1.0);
        // A local stroke would come out 1 px thick under this scale.
        let model = Affine2::scale(Vec2::new(2.0, 0.1));
        let triangles = non_scaling_stroke(&resolved, model);
        assert!(!triangles.is_empty());
        let ys: Vec<f64> = triangles
            .iter()
            .flat_map(|triangle| triangle.vertices.iter().map(|point| point.y))
            .collect();
        let min = ys.iter().copied().fold(f64::INFINITY, f64::min);
        let max = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(
            (max - min - 10.0).abs() < 0.5,
            "the stroke keeps its width, got {}",
            max - min
        );
        assert!(
            tessellate(&resolved).stroke.is_empty(),
            "the local stroke is not tessellated when it is drawn in world space"
        );
    }
}
