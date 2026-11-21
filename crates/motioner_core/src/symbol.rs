//! Imported symbols: the editable, addressable art a rig is built on.
//!
//! A [`Symbol`] is pure data produced by an importer (today the SVG reader
//! in `motioner_io`) and consumed by the DSL scene, which instantiates it as
//! ordinary nodes. Geometry is flattened to document coordinates at import,
//! so a symbol carries no transform stack and a golden document never
//! depends on how a file nested its groups (D-0054).
//!
//! Parts are addressed by their source `id` (or editor label). A part is a
//! group or a path; each one records the *pivot* (the centre of its bounds
//! in local coordinates) so an instance can scale and rotate around the
//! shape's centre with the ordinary node anchor.

use crate::math::Vec2;
use crate::node::PaintSpec;
use crate::path::PathGeometry;

/// A parsed piece of vector art.
#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    /// Display name (usually the file stem).
    pub name: String,
    /// Natural width from the source `viewBox` or `width` attribute.
    pub width: f64,
    /// Natural height from the source `viewBox` or `height` attribute.
    pub height: f64,
    /// Top-level parts in paint order.
    pub parts: Vec<SymbolPart>,
    /// Non-fatal import findings, each naming the element id.
    pub warnings: Vec<String>,
}

/// One addressable piece of a symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolPart {
    /// Stable id from the source, when the element had one.
    pub id: Option<String>,
    /// Pivot of the part in symbol coordinates (centre of its bounds).
    pub anchor: Vec2,
    /// Group or element opacity (`0.0..=1.0`).
    pub opacity: f64,
    /// What the part draws.
    pub shape: SymbolShape,
    /// Clip-path geometry, applied as an alpha mask on the part's nodes.
    pub clip: Vec<SymbolPart>,
}

/// The two kinds of symbol part.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // one parsed tree; boxing would hurt pattern use
pub enum SymbolShape {
    /// A container; draws nothing itself.
    Group(Vec<SymbolPart>),
    /// One filled and/or stroked path.
    Path(SymbolPath),
}

/// A path with its paint.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolPath {
    /// Flattened geometry in symbol coordinates.
    pub geometry: PathGeometry,
    /// Fill and stroke (static values here; the instance may key them).
    pub paint: PaintSpec,
    /// Dash on-length in local pixels, when the source had a dash pattern.
    pub dash: Option<f64>,
}

impl Symbol {
    /// The number of addressable ids (parts with an `id`), recursively.
    #[must_use]
    pub fn part_count(&self) -> usize {
        fn count(parts: &[SymbolPart]) -> usize {
            parts
                .iter()
                .map(|part| {
                    usize::from(part.id.is_some())
                        + match &part.shape {
                            SymbolShape::Group(children) => count(children),
                            SymbolShape::Path(_) => 0,
                        }
                })
                .sum()
        }
        count(&self.parts)
    }

    /// The ids of every addressable part, in depth-first paint order.
    #[must_use]
    pub fn part_ids(&self) -> Vec<&str> {
        fn collect<'a>(parts: &'a [SymbolPart], out: &mut Vec<&'a str>) {
            for part in parts {
                if let Some(id) = part.id.as_deref() {
                    out.push(id);
                }
                if let SymbolShape::Group(children) = &part.shape {
                    collect(children, out);
                }
            }
        }
        let mut out = Vec::new();
        collect(&self.parts, &mut out);
        out
    }

    /// Finds an addressable part by id.
    #[must_use]
    pub fn part(&self, id: &str) -> Option<&SymbolPart> {
        fn find<'a>(parts: &'a [SymbolPart], id: &str) -> Option<&'a SymbolPart> {
            for part in parts {
                if part.id.as_deref() == Some(id) {
                    return Some(part);
                }
                if let SymbolShape::Group(children) = &part.shape {
                    if let Some(found) = find(children, id) {
                        return Some(found);
                    }
                }
            }
            None
        }
        find(&self.parts, id)
    }

    /// The union of every drawn path's bounds, as `[left, top, right, bottom]`.
    #[must_use]
    pub fn bounds(&self) -> Option<[f64; 4]> {
        fn accumulate(parts: &[SymbolPart], bounds: &mut Option<[f64; 4]>) {
            for part in parts {
                match &part.shape {
                    SymbolShape::Group(children) => accumulate(children, bounds),
                    SymbolShape::Path(path) => {
                        if let Some(local) = path.geometry.local_bounds() {
                            *bounds = Some(match *bounds {
                                None => local,
                                Some(current) => [
                                    current[0].min(local[0]),
                                    current[1].min(local[1]),
                                    current[2].max(local[2]),
                                    current[3].max(local[3]),
                                ],
                            });
                        }
                    }
                }
                accumulate(&part.clip, bounds);
            }
        }
        let mut bounds = None;
        accumulate(&self.parts, &mut bounds);
        bounds
    }
}

impl SymbolPart {
    /// The centre of the part's own drawn bounds, or [`Vec2::ZERO`] for an
    /// empty group.
    #[must_use]
    pub fn centre(&self) -> Vec2 {
        self.anchor
    }
}
