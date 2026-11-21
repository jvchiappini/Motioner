//! The per-frame gradient table and the instance hooks (D-0030).
//!
//! The renderer uploads this table into the 2D camera uniform and every shape
//! instance references one entry; `shape_2d.wgsl` evaluates the ramp.
//!
//! Layout, shared with `shape_2d.wgsl`:
//!
//! ```text
//! [kind, stop_count, first_stop_slot, 0]      kind 0 = linear, 1 = radial
//! [start.xy, end.xy] or [center.xy, radius, 0]
//! per stop: [r, g, b, a] (linear) and [offset, 0, 0, 0]
//! ```

use ferrous_2d::render::ShapeInstance;
use motioner_core::{Color, GradientKind};
use motioner_eval::{Affine2, ItemShape, RenderPlan, ResolvedGradient};

/// Slots in the engine's gradient table (`GRADIENT_TABLE_VEC4`).
pub const TABLE_SLOTS: usize = 512;

/// Most stops one gradient can put in the table (the document allows 16).
pub const MAX_STOPS: usize = 16;

/// A gradient table built for one frame, plus the base slot of each plan
/// gradient.
#[derive(Debug, Default)]
pub struct GradientTable {
    slots: Vec<[f32; 4]>,
    bases: Vec<Option<u32>>,
}

impl GradientTable {
    /// Assigns a table region to every gradient in the plan.
    ///
    /// Gradients that do not fit (or degenerate to fewer than two stops) get
    /// no base and are drawn with their solid fallback.
    #[must_use]
    pub fn build(plan: &RenderPlan, flip: Affine2) -> Self {
        let transforms = owning_transforms(plan, flip);
        let mut slots = Vec::new();
        let mut bases = vec![None; plan.gradients().len()];
        for (index, gradient) in plan.gradients().iter().enumerate() {
            let count = gradient.stops.len().min(MAX_STOPS);
            let needed = 2 + count * 2;
            if count < 2 || slots.len() + needed > TABLE_SLOTS {
                continue;
            }
            #[allow(clippy::cast_possible_truncation)] // bounded by TABLE_SLOTS
            let base = slots.len() as u32;
            slots.push(header(gradient, count, base));
            slots.push(geometry(gradient, transforms[index].unwrap_or(flip)));
            for stop in &gradient.stops[..count] {
                let color = stop.color;
                slots.push([
                    Color::srgb_to_linear(color.r),
                    Color::srgb_to_linear(color.g),
                    Color::srgb_to_linear(color.b),
                    f32::from(color.a) / 255.0,
                ]);
                #[allow(clippy::cast_possible_truncation)]
                slots.push([stop.offset as f32, 0.0, 0.0, 0.0]);
            }
            bases[index] = Some(base);
        }
        Self { slots, bases }
    }

    /// The table entry base of one plan gradient, if it fits.
    #[must_use]
    pub fn base(&self, gradient: usize) -> Option<u32> {
        self.bases.get(gradient).copied().flatten()
    }

    /// The flat vec4 table uploaded to the engine.
    #[must_use]
    pub fn slots(&self) -> &[[f32; 4]] {
        &self.slots
    }
}

#[allow(clippy::cast_precision_loss)] // count <= 16 and base <= 512
fn header(gradient: &ResolvedGradient, count: usize, base: u32) -> [f32; 4] {
    let kind = match gradient.kind {
        GradientKind::Linear { .. } => 0.0,
        GradientKind::Radial { .. } => 1.0,
    };
    let count = count as f32;
    // The first stop pair starts right after the header and the geometry.
    let first = (base + 2) as f32;
    [kind, count, first, 0.0]
}

#[allow(clippy::cast_possible_truncation)] // geometry values are f64 pixels
fn geometry(gradient: &ResolvedGradient, transform: Affine2) -> [f32; 4] {
    match gradient.kind {
        GradientKind::Linear { start, end } => {
            let start = transform.apply(start);
            let end = transform.apply(end);
            [start.x as f32, start.y as f32, end.x as f32, end.y as f32]
        }
        GradientKind::Radial { center, radius } => {
            // `center` and `radius` are authored in the item's local space. The
            // curve only sees world positions, so both the centre and the +x
            // edge point are transformed first; taking the length of that
            // *world* difference gives `radius * scale`. Transforming the
            // already-transformed centre again (the old bug) folded the item's
            // translation into the radius, so an off-centre glow ended inside
            // or outside the disc and its alpha never reached zero at the edge.
            let center_world = transform.apply(center);
            // The engine pass draws a circle; under a non-uniform scale the
            // authored ellipse becomes the x-axis scale. Documented in D-0030.
            let edge_world = transform.apply(motioner_core::Vec2::new(center.x + radius, center.y));
            let radius = (edge_world - center_world).length();
            [
                center_world.x as f32,
                center_world.y as f32,
                radius as f32,
                0.0,
            ]
        }
    }
}

/// World transform of the item that owns each gradient arena entry.
fn owning_transforms(plan: &RenderPlan, flip: Affine2) -> Vec<Option<Affine2>> {
    let mut transforms = vec![None; plan.gradients().len()];
    for item in plan.items() {
        let world = flip.then(item.transform);
        match item.shape {
            ItemShape::Rect { paint, .. } | ItemShape::Circle { paint, .. } => {
                record_paint(&mut transforms, &paint, world);
            }
            ItemShape::Path { index } => {
                if let Some(path) = plan.path(index) {
                    record_paint(&mut transforms, &path.paint, world);
                }
            }
            ItemShape::Text {
                gradient: Some(index),
                ..
            } => {
                if let Some(slot) = transforms.get_mut(index) {
                    *slot = Some(world);
                }
            }
            ItemShape::Text { gradient: None, .. } | ItemShape::Image { .. } => {}
        }
    }
    transforms
}

fn record_paint(
    transforms: &mut [Option<Affine2>],
    paint: &motioner_eval::ResolvedPaint,
    world: Affine2,
) {
    if let Some(motioner_eval::ResolvedFill::Gradient(index)) = paint.fill {
        if let Some(slot) = transforms.get_mut(index) {
            *slot = Some(world);
        }
    }
    if let Some(motioner_eval::ResolvedFill::Gradient(index)) =
        paint.stroke.map(|stroke| stroke.paint)
    {
        if let Some(slot) = transforms.get_mut(index) {
            *slot = Some(world);
        }
    }
}

/// Points one shape instance at a gradient, or leaves the solid fallback.
#[must_use]
pub fn paint(instance: ShapeInstance, base: Option<u32>) -> ShapeInstance {
    match base {
        Some(base) => instance.with_gradient(base),
        None => instance,
    }
}

/// The base of one resolved fill of the plan.
#[must_use]
pub fn base_of(table: &GradientTable, paint: motioner_eval::ResolvedFill) -> Option<u32> {
    match paint {
        motioner_eval::ResolvedFill::Solid(_) => None,
        motioner_eval::ResolvedFill::Gradient(index) => table.base(index),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::Vec2;
    use motioner_eval::{ResolvedStop, ResolvedStroke};

    #[test]
    fn radial_radius_is_scaled_not_shifted_by_the_translation() {
        let gradient = ResolvedGradient {
            kind: GradientKind::Radial {
                center: Vec2::ZERO,
                radius: 150.0,
            },
            stops: vec![
                ResolvedStop {
                    offset: 0.0,
                    color: Color::WHITE,
                },
                ResolvedStop {
                    offset: 1.0,
                    color: Color::BLACK,
                },
            ],
        };
        let flip = Affine2 {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            tx: -480.0,
            ty: 270.0,
        };
        let transform = flip.then(Affine2::translation(Vec2::new(360.0, 210.0)));
        let encoded = geometry(&gradient, transform);
        assert!((encoded[0] - -120.0).abs() < 1e-3, "centre x");
        assert!((encoded[1] - 60.0).abs() < 1e-3, "centre y");
        assert!(
            (encoded[2] - 150.0).abs() < 1e-3,
            "radius must stay 150, got {}",
            encoded[2]
        );

        let scaled = geometry(&gradient, transform.then(Affine2::scale(Vec2::splat(2.0))));
        assert!((scaled[2] - 300.0).abs() < 1e-3, "scaled radius");
    }

    #[test]
    fn encodes_a_linear_gradient() {
        let gradient = ResolvedGradient {
            kind: GradientKind::Linear {
                start: Vec2::new(0.0, 0.0),
                end: Vec2::new(100.0, 0.0),
            },
            stops: vec![
                ResolvedStop {
                    offset: 0.0,
                    color: Color::BLACK,
                },
                ResolvedStop {
                    offset: 1.0,
                    color: Color::WHITE,
                },
            ],
        };
        let mut plan = RenderPlan::new();
        plan.push_gradient(gradient);
        let table = GradientTable::build(&plan, Affine2::IDENTITY);
        assert_eq!(table.base(0), Some(0));
        for (slot, expected) in table.slots()[0].iter().zip([0.0, 2.0, 2.0, 0.0]) {
            assert!((slot - expected).abs() < 1e-6);
        }
        for (slot, expected) in table.slots()[1].iter().zip([0.0, 0.0, 100.0, 0.0]) {
            assert!((slot - expected).abs() < 1e-6);
        }
        assert!((table.slots()[2][3] - 1.0).abs() < 1e-6);
        assert!((table.slots()[3][0] - 0.0).abs() < 1e-6);
        assert!((table.slots()[5][0] - 1.0).abs() < 1e-6);
        let _ = ResolvedStroke {
            paint: motioner_eval::ResolvedFill::Solid(Color::WHITE),
            width: 1.0,
            non_scaling: None,
        };
    }
}
