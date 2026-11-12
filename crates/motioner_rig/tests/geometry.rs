//! Geometry driver and channel tests (D-0058).

use motioner_core::{Ease, LineCap, LineJoin, PathGeometry, PathPoint, Vec2};
use motioner_rig::{
    bake, Driver, Expr, GeometryCommand, GeometryExpr, GeometryWrite, Key, PartBase, PropValue,
    PropertyDef, Rig,
};

fn lid() -> GeometryExpr {
    GeometryExpr::Builder(vec![
        GeometryCommand::MoveTo {
            x: Expr::Number(-20.0),
            y: Expr::Number(0.0),
        },
        GeometryCommand::CubicTo {
            c1x: Expr::Number(-20.0),
            c1y: Expr::Property(0),
            c2x: Expr::Number(20.0),
            c2y: Expr::Property(0),
            x: Expr::Number(20.0),
            y: Expr::Number(0.0),
        },
    ])
}

fn base() -> PathGeometry {
    PathGeometry::new(
        vec![
            PathPoint::curved(Vec2::new(-20.0, 0.0), Vec2::ZERO, Vec2::new(0.0, 0.0)),
            PathPoint::curved(Vec2::new(20.0, 0.0), Vec2::new(0.0, 0.0), Vec2::ZERO),
        ],
        false,
        LineCap::Round,
        LineJoin::Round,
    )
    .unwrap()
}

fn rig_with_geometry(expression: GeometryExpr) -> Rig {
    let mut rig = Rig::new("lid");
    let mut part = PartBase::at("lid", Vec2::ZERO);
    part.geometry = Some(base());
    rig.add_part(part);
    rig.add_property(PropertyDef::number("lift", 0.0, 0.0, 20.0));
    rig.channels[0]
        .keys
        .push(Key::new(0.0, PropValue::Number(0.0), Ease::Linear));
    rig.channels[0]
        .keys
        .push(Key::new(10.0, PropValue::Number(10.0), Ease::Linear));
    rig.drivers.push(Driver {
        name: "lid".to_owned(),
        params: vec![0],
        writes: Vec::new(),
        geometries: vec![GeometryWrite {
            part: 0,
            geometry: expression,
        }],
    });
    rig
}

#[test]
fn geometry_driver_updates_the_part_each_frame() {
    let rig = rig_with_geometry(lid());
    let frames = bake(&rig, 0.0, 1.0).unwrap();
    let first = frames[0].part(0).unwrap().geometry.as_ref().unwrap();
    let second = frames[1].part(0).unwrap().geometry.as_ref().unwrap();
    assert!((first.points()[0].handle_out.y - 0.0).abs() < 1e-9);
    assert!((second.points()[0].handle_out.y - 1.0).abs() < 1e-9);
    assert!((second.points()[1].handle_in.y - 1.0).abs() < 1e-9);
}

#[test]
fn geometry_driver_rejects_a_topology_change() {
    let mut rig = rig_with_geometry(lid());
    rig.parts[0].geometry = Some(
        PathGeometry::new(
            vec![
                PathPoint::corner(Vec2::new(-20.0, 0.0)),
                PathPoint::corner(Vec2::new(0.0, 0.0)),
                PathPoint::corner(Vec2::new(20.0, 0.0)),
            ],
            false,
            LineCap::Round,
            LineJoin::Round,
        )
        .unwrap(),
    );
    let error = bake(&rig, 0.0, 0.0).unwrap_err();
    assert!(error.message.contains("topology"));
}

#[test]
fn lid_arc_matches_the_html_formula_point_for_point() {
    let lt = Expr::Property(0);
    let d = Expr::Add(
        Box::new(Expr::Number(-15.0)),
        Box::new(Expr::Mul(Box::new(Expr::Number(30.0)), Box::new(lt))),
    );
    let h = Expr::Sqrt(Box::new(Expr::Sub(
        Box::new(Expr::Number(225.0)),
        Box::new(Expr::Mul(Box::new(d.clone()), Box::new(d.clone()))),
    )));
    let a_x = Expr::Sub(Box::new(Expr::Number(76.0)), Box::new(h.clone()));
    let b_x = Expr::Add(Box::new(Expr::Number(76.0)), Box::new(h));
    let a_y = Expr::Add(Box::new(Expr::Number(88.0)), Box::new(d.clone()));
    let b_y = Expr::Add(Box::new(Expr::Number(88.0)), Box::new(d.clone()));
    let geometry = GeometryExpr::Builder(vec![
        GeometryCommand::MoveTo { x: a_x, y: a_y },
        GeometryCommand::ArcTo {
            rx: Expr::Number(15.0),
            ry: Expr::Number(15.0),
            rotation: Expr::Number(0.0),
            large_arc: Expr::Greater(Box::new(d), Box::new(Expr::Number(0.0))),
            sweep: Expr::Number(1.0),
            x: b_x,
            y: b_y,
        },
        GeometryCommand::Close,
    ]);
    let mut rig = Rig::new("lid");
    let mut part = PartBase::at("lid", Vec2::ZERO);
    part.geometry = Some(
        PathGeometry::new(
            vec![
                PathPoint::corner(Vec2::new(61.0, 88.0)),
                PathPoint::corner(Vec2::new(76.0, 103.0)),
                PathPoint::corner(Vec2::new(91.0, 88.0)),
            ],
            true,
            LineCap::Round,
            LineJoin::Round,
        )
        .unwrap(),
    );
    rig.add_part(part);
    rig.add_property(PropertyDef::number("lt", 0.5, 0.0, 1.0));
    rig.drivers.push(Driver {
        name: "lid".to_owned(),
        params: vec![0],
        writes: Vec::new(),
        geometries: vec![GeometryWrite { part: 0, geometry }],
    });
    let frames = bake(&rig, 0.0, 0.0).unwrap();
    let points = &frames[0]
        .part(0)
        .unwrap()
        .geometry
        .as_ref()
        .unwrap()
        .points()
        .iter()
        .map(|point| point.position)
        .collect::<Vec<_>>();
    assert_eq!(points.len(), 3);
    assert!((points[0].x - 61.0).abs() < 1e-9);
    assert!((points[0].y - 88.0).abs() < 1e-9);
    assert!((points[1].x - 76.0).abs() < 1e-9);
    assert!((points[1].y - 73.0).abs() < 1e-9, "{points:?}");
    assert!((points[2].x - 91.0).abs() < 1e-9);
    assert!((points[2].y - 88.0).abs() < 1e-9);
}

#[test]
fn geometry_expression_mirror_flips_x() {
    let geometry = GeometryExpr::Mirror {
        geometry: Box::new(GeometryExpr::Builder(vec![
            GeometryCommand::MoveTo {
                x: motioner_rig::Expr::Number(-10.0),
                y: motioner_rig::Expr::Number(0.0),
            },
            GeometryCommand::LineTo {
                x: motioner_rig::Expr::Number(10.0),
                y: motioner_rig::Expr::Number(0.0),
            },
        ])),
        axis: motioner_rig::MirrorAxis::X,
    }
    .eval(&std::collections::BTreeMap::new(), "mirror")
    .unwrap();
    assert!((geometry.points()[0].position.x - 10.0).abs() < 1e-9);
    assert!((geometry.points()[1].position.x + 10.0).abs() < 1e-9);
}
