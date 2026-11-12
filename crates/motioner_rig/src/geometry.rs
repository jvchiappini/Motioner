//! Geometry driver IR: sandboxed path builders evaluated per frame (D-0058).
//!
//! Drivers return ordinary scalar writes and, optionally, one geometry write
//! per part. A geometry expression is built from numeric expressions and a
//! small set of path constructors: a command builder (`move_to`, `line_to`,
//! `cubic_to`, `close`), `ellipse_arc`, `arc`, `rounded_rect` and `mirror`.
//! Evaluation is allocation-bounded and returns typed errors naming the
//! driver.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use motioner_core::{LineCap, LineJoin, PathGeometry, PathPoint, Vec2};

use crate::driver::{DriverError, Expr};
use crate::PropValue;

/// Maximum points one geometry expression may produce.
pub const MAX_GEOMETRY_POINTS: usize = 4096;
/// Maximum commands in one `path_builder`.
pub const MAX_GEOMETRY_COMMANDS: usize = 2048;
/// Maximum nesting of `mirror` expressions.
pub const MAX_GEOMETRY_DEPTH: usize = 16;

/// Which coordinate a mirror negates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorAxis {
    /// Negate X: left and right sides swap.
    X,
    /// Negate Y: top and bottom swap.
    Y,
}

impl MirrorAxis {
    /// Parses `"x"` or `"y"`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            _ => None,
        }
    }
}

/// One command in a path builder.
#[derive(Debug, Clone, PartialEq)]
pub enum GeometryCommand {
    /// Starts the contour at a point.
    MoveTo {
        /// X coordinate.
        x: Expr,
        /// Y coordinate.
        y: Expr,
    },
    /// Adds a straight corner.
    LineTo {
        /// X coordinate.
        x: Expr,
        /// Y coordinate.
        y: Expr,
    },
    /// Adds a cubic segment ending at `(x, y)`.
    CubicTo {
        /// First control point X.
        c1x: Expr,
        /// First control point Y.
        c1y: Expr,
        /// Second control point X.
        c2x: Expr,
        /// Second control point Y.
        c2y: Expr,
        /// Endpoint X.
        x: Expr,
        /// Endpoint Y.
        y: Expr,
    },
    /// Adds an SVG elliptical arc to (x, y).
    ArcTo {
        /// Horizontal radius.
        rx: Expr,
        /// Vertical radius.
        ry: Expr,
        /// X-axis rotation in degrees.
        rotation: Expr,
        /// SVG large-arc flag (non-zero is true).
        large_arc: Expr,
        /// SVG sweep flag (non-zero is clockwise).
        sweep: Expr,
        /// Endpoint X.
        x: Expr,
        /// Endpoint Y.
        y: Expr,
    },
    /// Closes the contour.
    Close,
}

/// A geometry-producing expression.
#[derive(Debug, Clone, PartialEq)]
pub enum GeometryExpr {
    /// A command builder.
    Builder(Vec<GeometryCommand>),
    /// An elliptical arc centred on `(cx, cy)`.
    EllipseArc {
        /// Centre X.
        cx: Expr,
        /// Centre Y.
        cy: Expr,
        /// Horizontal radius.
        rx: Expr,
        /// Vertical radius.
        ry: Expr,
        /// Start angle in degrees.
        start: Expr,
        /// End angle in degrees.
        end: Expr,
    },
    /// A circular arc centred on the origin.
    Arc {
        /// Radius.
        radius: Expr,
        /// Start angle in degrees.
        start: Expr,
        /// End angle in degrees.
        end: Expr,
    },
    /// A rounded rectangle centred on the origin.
    RoundedRect {
        /// Width.
        width: Expr,
        /// Height.
        height: Expr,
        /// Corner radius.
        radius: Expr,
    },
    /// A mirrored copy of another geometry expression.
    Mirror {
        /// Geometry to mirror.
        geometry: Box<Self>,
        /// Axis to negate.
        axis: MirrorAxis,
    },
}

impl GeometryExpr {
    /// Evaluates the expression against the property values at one frame.
    ///
    /// # Errors
    ///
    /// Returns [`DriverError`] for a missing move, a point count or command
    /// count over the budget, a non-finite coordinate, or an invalid
    /// constructor argument.
    pub fn eval(
        &self,
        properties: &BTreeMap<usize, PropValue>,
        driver: &str,
    ) -> Result<PathGeometry, DriverError> {
        self.eval_depth(properties, driver, 0)
    }

    fn eval_depth(
        &self,
        properties: &BTreeMap<usize, PropValue>,
        driver: &str,
        depth: usize,
    ) -> Result<PathGeometry, DriverError> {
        if depth > MAX_GEOMETRY_DEPTH {
            return Err(error(
                driver,
                format!("geometry nested deeper than {MAX_GEOMETRY_DEPTH}"),
            ));
        }
        match self {
            Self::Builder(commands) => build(commands, properties, driver),
            Self::EllipseArc {
                cx,
                cy,
                rx,
                ry,
                start,
                end,
            } => {
                let center = Vec2::new(
                    number(cx, properties, driver)?,
                    number(cy, properties, driver)?,
                );
                let geometry = PathGeometry::ellipse_arc(
                    center,
                    number(rx, properties, driver)?,
                    number(ry, properties, driver)?,
                    number(start, properties, driver)?,
                    number(end, properties, driver)?,
                );
                geometry.map_err(|_| error(driver, "invalid ellipse_arc arguments"))
            }
            Self::Arc { radius, start, end } => PathGeometry::arc(
                number(radius, properties, driver)?,
                number(start, properties, driver)?,
                number(end, properties, driver)?,
            )
            .map_err(|_| error(driver, "invalid arc arguments")),
            Self::RoundedRect {
                width,
                height,
                radius,
            } => rounded_rect(
                number(width, properties, driver)?,
                number(height, properties, driver)?,
                number(radius, properties, driver)?,
            )
            .map_err(|message| error(driver, message)),
            Self::Mirror { geometry, axis } => {
                let geometry = geometry.eval_depth(properties, driver, depth + 1)?;
                Ok(mirror(&geometry, *axis))
            }
        }
    }
}

#[allow(clippy::too_many_lines)] // one arm per command, kept linear
fn build(
    commands: &[GeometryCommand],
    properties: &BTreeMap<usize, PropValue>,
    driver: &str,
) -> Result<PathGeometry, DriverError> {
    if commands.len() > MAX_GEOMETRY_COMMANDS {
        return Err(error(
            driver,
            format!("path_builder has more than {MAX_GEOMETRY_COMMANDS} commands"),
        ));
    }
    let mut points: Vec<PathPoint> = Vec::new();
    let mut closed = false;
    for command in commands {
        if closed {
            return Err(error(driver, "path_builder has commands after close()"));
        }
        match command {
            GeometryCommand::MoveTo { x, y } => {
                if !points.is_empty() {
                    return Err(error(
                        driver,
                        "path_builder supports one contour; move_to must come first",
                    ));
                }
                points.push(PathPoint::corner(Vec2::new(
                    number(x, properties, driver)?,
                    number(y, properties, driver)?,
                )));
            }
            GeometryCommand::LineTo { x, y } => {
                require_start(&points, driver)?;
                points.push(PathPoint::corner(Vec2::new(
                    number(x, properties, driver)?,
                    number(y, properties, driver)?,
                )));
            }
            GeometryCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                require_start(&points, driver)?;
                let c1 = Vec2::new(
                    number(c1x, properties, driver)?,
                    number(c1y, properties, driver)?,
                );
                let c2 = Vec2::new(
                    number(c2x, properties, driver)?,
                    number(c2y, properties, driver)?,
                );
                let end = Vec2::new(
                    number(x, properties, driver)?,
                    number(y, properties, driver)?,
                );
                if let Some(last) = points.last_mut() {
                    last.handle_out = c1 - last.position;
                }
                points.push(PathPoint::curved(end, c2 - end, Vec2::ZERO));
            }
            GeometryCommand::ArcTo {
                rx,
                ry,
                rotation,
                large_arc,
                sweep,
                x,
                y,
            } => {
                require_start(&points, driver)?;
                let end = Vec2::new(
                    number(x, properties, driver)?,
                    number(y, properties, driver)?,
                );
                push_arc(
                    &mut points,
                    number(rx, properties, driver)?,
                    number(ry, properties, driver)?,
                    number(rotation, properties, driver)?,
                    number(large_arc, properties, driver)? != 0.0,
                    number(sweep, properties, driver)? != 0.0,
                    end,
                    driver,
                )?;
            }
            GeometryCommand::Close => {
                require_start(&points, driver)?;
                closed = true;
            }
        }
        if points.len() > MAX_GEOMETRY_POINTS {
            return Err(error(
                driver,
                format!("path_builder produces more than {MAX_GEOMETRY_POINTS} points"),
            ));
        }
    }
    if points.len() < 2 {
        return Err(error(
            driver,
            "path_builder needs a move_to and at least one more point",
        ));
    }
    PathGeometry::new(points, closed, LineCap::Round, LineJoin::Round)
        .map_err(|_| error(driver, "path_builder produced a non-finite point"))
}

fn require_start(points: &[PathPoint], driver: &str) -> Result<(), DriverError> {
    if points.is_empty() {
        Err(error(driver, "path_builder must start with move_to"))
    } else {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn push_arc(
    points: &mut Vec<PathPoint>,
    radius_x: f64,
    radius_y: f64,
    rotation: f64,
    large_arc: bool,
    sweep: bool,
    end: Vec2,
    driver: &str,
) -> Result<(), DriverError> {
    let start = points
        .last()
        .map(|point| point.position)
        .ok_or_else(|| error(driver, "arc_to needs a preceding move_to"))?;
    if !radius_x.is_finite()
        || !radius_y.is_finite()
        || radius_x == 0.0
        || radius_y == 0.0
        || !rotation.is_finite()
        || !end.is_finite()
    {
        return Err(error(driver, "arc_to needs finite radii and endpoint"));
    }
    if start == end {
        return Ok(());
    }
    let radius_x = radius_x.abs();
    let radius_y = radius_y.abs();
    let phi = rotation.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let half = (start - end) * 0.5;
    let x1 = cos_phi * half.x + sin_phi * half.y;
    let y1 = -sin_phi * half.x + cos_phi * half.y;
    let lambda = (x1 * x1) / (radius_x * radius_x) + (y1 * y1) / (radius_y * radius_y);
    let (radius_x, radius_y) = if lambda > 1.0 {
        let scale = lambda.sqrt();
        (radius_x * scale, radius_y * scale)
    } else {
        (radius_x, radius_y)
    };
    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let numerator = (radius_x * radius_x * radius_y * radius_y)
        - (radius_x * radius_x * y1 * y1)
        - (radius_y * radius_y * x1 * x1);
    let denominator = (radius_x * radius_x * y1 * y1) + (radius_y * radius_y * x1 * x1);
    let coefficient = if denominator <= 0.0 {
        0.0
    } else {
        sign * (numerator.max(0.0) / denominator).sqrt()
    };
    let cx1 = coefficient * (radius_x * y1 / radius_y);
    let cy1 = coefficient * (-radius_y * x1 / radius_x);
    let center = Vec2::new(
        cos_phi * cx1 - sin_phi * cy1 + (start.x + end.x) * 0.5,
        sin_phi * cx1 + cos_phi * cy1 + (start.y + end.y) * 0.5,
    );
    let u = Vec2::new((x1 - cx1) / radius_x, (y1 - cy1) / radius_y);
    let v = Vec2::new((-x1 - cx1) / radius_x, (-y1 - cy1) / radius_y);
    let start_angle = vector_angle(1.0, 0.0, u.x, u.y);
    let mut delta = vector_angle(u.x, u.y, v.x, v.y);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let segments = (((delta.abs() / (PI / 2.0)) - 1e-6).ceil() as u32).max(1);
    let step = delta / f64::from(segments);
    for index in 0..segments {
        let t0 = start_angle + step * f64::from(index);
        let t1 = t0 + step;
        let p0 = ellipse_point(center, radius_x, radius_y, phi, t0);
        let p3 = ellipse_point(center, radius_x, radius_y, phi, t1);
        let derivative0 = ellipse_derivative(radius_x, radius_y, phi, t0);
        let derivative1 = ellipse_derivative(radius_x, radius_y, phi, t1);
        let alpha = 4.0 / 3.0 * (step / 4.0).tan();
        let c1 = p0 + derivative0 * alpha;
        let c2 = p3 - derivative1 * alpha;
        if let Some(last) = points.last_mut() {
            last.handle_out = c1 - last.position;
        }
        points.push(PathPoint::curved(p3, c2 - p3, Vec2::ZERO));
    }
    Ok(())
}

fn ellipse_point(center: Vec2, rx: f64, ry: f64, phi: f64, angle: f64) -> Vec2 {
    let (sin_phi, cos_phi) = phi.sin_cos();
    let (sin_a, cos_a) = angle.sin_cos();
    Vec2::new(
        center.x + rx * cos_a * cos_phi - ry * sin_a * sin_phi,
        center.y + rx * cos_a * sin_phi + ry * sin_a * cos_phi,
    )
}

fn ellipse_derivative(rx: f64, ry: f64, phi: f64, angle: f64) -> Vec2 {
    let (sin_phi, cos_phi) = phi.sin_cos();
    let (sin_a, cos_a) = angle.sin_cos();
    Vec2::new(
        -rx * sin_a * cos_phi - ry * cos_a * sin_phi,
        -rx * sin_a * sin_phi + ry * cos_a * cos_phi,
    )
}

fn vector_angle(ux: f64, uy: f64, vx: f64, vy: f64) -> f64 {
    (ux * vy - uy * vx).atan2(ux * vx + uy * vy)
}

fn rounded_rect(width: f64, height: f64, radius: f64) -> Result<PathGeometry, &'static str> {
    if !width.is_finite() || width <= 0.0 || !height.is_finite() || height <= 0.0 {
        return Err("rounded_rect width and height must be positive");
    }
    if !radius.is_finite() || radius < 0.0 {
        return Err("rounded_rect radius must be zero or positive");
    }
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let radius = radius.min(half_width).min(half_height);
    let handle = radius * 0.552_284_749_830_793_6;
    let points = vec![
        PathPoint::curved(
            Vec2::new(half_width - radius, -half_height),
            Vec2::new(-handle, 0.0),
            Vec2::new(handle, 0.0),
        ),
        PathPoint::curved(
            Vec2::new(half_width, -half_height + radius),
            Vec2::new(0.0, -handle),
            Vec2::new(0.0, 0.0),
        ),
        PathPoint::curved(
            Vec2::new(half_width, half_height - radius),
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, handle),
        ),
        PathPoint::curved(
            Vec2::new(half_width - radius, half_height),
            Vec2::new(handle, 0.0),
            Vec2::new(-handle, 0.0),
        ),
        PathPoint::curved(
            Vec2::new(-half_width + radius, half_height),
            Vec2::new(0.0, 0.0),
            Vec2::new(-handle, 0.0),
        ),
        PathPoint::curved(
            Vec2::new(-half_width, half_height - radius),
            Vec2::new(0.0, handle),
            Vec2::new(0.0, 0.0),
        ),
        PathPoint::curved(
            Vec2::new(-half_width, -half_height + radius),
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, -handle),
        ),
        PathPoint::curved(
            Vec2::new(-half_width + radius, -half_height),
            Vec2::new(-handle, 0.0),
            Vec2::new(handle, 0.0),
        ),
    ];
    PathGeometry::new(points, true, LineCap::Round, LineJoin::Round)
        .map_err(|_| "rounded_rect produced a non-finite point")
}

fn mirror(geometry: &PathGeometry, axis: MirrorAxis) -> PathGeometry {
    let points = geometry
        .points()
        .iter()
        .map(|point| {
            let position = mirrored(point.position, axis);
            let handle_in = mirrored(point.handle_in, axis);
            let handle_out = mirrored(point.handle_out, axis);
            PathPoint::curved(position, handle_in, handle_out)
        })
        .collect();
    PathGeometry::new(points, geometry.closed(), geometry.cap(), geometry.join())
        .unwrap_or_else(|_| geometry.clone())
}

fn mirrored(value: Vec2, axis: MirrorAxis) -> Vec2 {
    match axis {
        MirrorAxis::X => Vec2::new(-value.x, value.y),
        MirrorAxis::Y => Vec2::new(value.x, -value.y),
    }
}

fn number(
    expression: &Expr,
    properties: &BTreeMap<usize, PropValue>,
    driver: &str,
) -> Result<f64, DriverError> {
    expression.eval(properties, driver)
}

fn error(driver: &str, message: impl Into<String>) -> DriverError {
    DriverError {
        driver: driver.to_owned(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expr(value: f64) -> Expr {
        Expr::Number(value)
    }

    #[test]
    fn builder_makes_a_closed_triangle() {
        let geometry = GeometryExpr::Builder(vec![
            GeometryCommand::MoveTo {
                x: expr(0.0),
                y: expr(0.0),
            },
            GeometryCommand::LineTo {
                x: expr(10.0),
                y: expr(0.0),
            },
            GeometryCommand::LineTo {
                x: expr(0.0),
                y: expr(10.0),
            },
            GeometryCommand::Close,
        ])
        .eval(&BTreeMap::new(), "triangle")
        .unwrap();
        assert_eq!(geometry.points().len(), 3);
        assert!(geometry.closed());
    }

    #[test]
    fn builder_rejects_a_command_before_move() {
        let error = GeometryExpr::Builder(vec![GeometryCommand::LineTo {
            x: expr(0.0),
            y: expr(0.0),
        }])
        .eval(&BTreeMap::new(), "bad")
        .unwrap_err();
        assert!(error.message.contains("move_to"));
    }

    #[test]
    fn mirror_negates_x_and_keeps_handles_relative() {
        let geometry = GeometryExpr::Builder(vec![
            GeometryCommand::MoveTo {
                x: expr(-4.0),
                y: expr(0.0),
            },
            GeometryCommand::CubicTo {
                c1x: expr(-2.0),
                c1y: expr(1.0),
                c2x: expr(2.0),
                c2y: expr(1.0),
                x: expr(4.0),
                y: expr(0.0),
            },
        ])
        .eval(&BTreeMap::new(), "mirror")
        .unwrap();
        let flipped = mirror(&geometry, MirrorAxis::X);
        assert!((flipped.points()[0].position.x - 4.0).abs() < 1e-9);
        assert!((flipped.points()[1].position.x + 4.0).abs() < 1e-9);
    }

    #[test]
    fn ellipse_arc_matches_the_circular_constructor() {
        let ellipse = GeometryExpr::EllipseArc {
            cx: expr(0.0),
            cy: expr(0.0),
            rx: expr(10.0),
            ry: expr(10.0),
            start: expr(0.0),
            end: expr(90.0),
        }
        .eval(&BTreeMap::new(), "arc")
        .unwrap();
        let arc = PathGeometry::arc(10.0, 0.0, 90.0).unwrap();
        assert_eq!(ellipse, arc);
    }
}
