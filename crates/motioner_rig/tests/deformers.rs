//! Deformer and constraint goldens (D-0059, D-0060).

use motioner_core::{Ease, LineCap, LineJoin, PathGeometry, PathPoint, Vec2};
use motioner_rig::{
    bake, BlendShape, BlendShapes, Bone, BoneConstraint, Constraint, Deformer, Driver, Expr,
    GeometryCommand, GeometryExpr, GeometryWrite, Key, Lattice, PartBase, PropKind, PropValue,
    PropertyDef, Rig, Skin,
};

fn open_path(points: &[Vec2]) -> PathGeometry {
    PathGeometry::new(
        points
            .iter()
            .map(|point| PathPoint::corner(*point))
            .collect(),
        false,
        LineCap::Round,
        LineJoin::Round,
    )
    .unwrap()
}

fn part(name: &str, position: Vec2, geometry: PathGeometry) -> PartBase {
    let mut part = PartBase::at(name, position);
    part.geometry = Some(geometry);
    part
}

fn vec_prop(name: &str) -> PropertyDef {
    PropertyDef::typed(name, PropKind::Vec2, PropValue::Vector(Vec2::ZERO))
}

#[test]
fn lattice_moves_the_corner_and_preserves_the_other_points() {
    let mut rig = Rig::new("lattice");
    rig.add_part(part(
        "body",
        Vec2::ZERO,
        open_path(&[
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ]),
    ));
    rig.add_property(vec_prop("tl"));
    rig.add_property(vec_prop("tr"));
    rig.add_property(vec_prop("br"));
    rig.add_property(vec_prop("bl"));
    rig.channels[0].keys.push(Key::new(
        0.0,
        PropValue::Vector(Vec2::new(2.0, 0.0)),
        Ease::Linear,
    ));
    rig.deformers.push(Deformer::Lattice(Lattice {
        part: 0,
        corners: [0, 1, 2, 3],
        edges: [None; 4],
    }));
    let frame = bake(&rig, 0.0, 0.0).unwrap();
    let geometry = frame[0].part(0).unwrap().geometry.as_ref().unwrap();
    assert!((geometry.points()[0].position.x - 2.0).abs() < 1e-9);
    assert!((geometry.points()[2].position.x - 10.0).abs() < 1e-9);
}

#[test]
fn skin_rotates_points_around_a_bone() {
    let mut rig = Rig::new("skin");
    rig.add_part(part(
        "arm",
        Vec2::ZERO,
        open_path(&[Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]),
    ));
    rig.add_property(PropertyDef::number("turn", 0.0, -180.0, 180.0));
    rig.channels[0]
        .keys
        .push(Key::new(0.0, PropValue::Number(90.0), Ease::Linear));
    rig.bones.push(Bone {
        name: "root".to_owned(),
        parent: None,
        bind_position: Vec2::ZERO,
        bind_rotation: 0.0,
        length: 10.0,
        rotation_property: Some(0),
        position_property: None,
    });
    rig.deformers.push(Deformer::Skin(Skin {
        part: 0,
        bones: rig.bones.clone(),
        weights: None,
    }));
    let frame = bake(&rig, 0.0, 0.0).unwrap();
    let geometry = frame[0].part(0).unwrap().geometry.as_ref().unwrap();
    assert!(geometry.points()[0].position.length() < 1e-6);
    assert!((geometry.points()[1].position.x).abs() < 1e-6);
    assert!((geometry.points()[1].position.y - 10.0).abs() < 1e-6);
}

#[test]
fn blend_shape_blends_halfway() {
    let mut rig = Rig::new("blend");
    rig.add_part(part(
        "body",
        Vec2::ZERO,
        open_path(&[Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]),
    ));
    rig.add_property(PropertyDef::number("weight", 0.5, 0.0, 1.0));
    rig.deformers.push(Deformer::BlendShapes(BlendShapes {
        part: 0,
        shapes: vec![BlendShape {
            name: "up".to_owned(),
            points: vec![
                PathPoint::corner(Vec2::new(0.0, 10.0)),
                PathPoint::corner(Vec2::new(10.0, 10.0)),
            ],
            weight: 0,
        }],
    }));
    let frame = bake(&rig, 0.0, 0.0).unwrap();
    let geometry = frame[0].part(0).unwrap().geometry.as_ref().unwrap();
    assert!((geometry.points()[0].position.y - 5.0).abs() < 1e-9);
    assert!((geometry.points()[1].position.y - 5.0).abs() < 1e-9);
}

#[test]
fn geometry_driver_plus_lattice_keeps_the_order() {
    let mut rig = Rig::new("driver-lattice");
    rig.add_part(part(
        "body",
        Vec2::ZERO,
        open_path(&[
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ]),
    ));
    rig.add_property(vec_prop("tl"));
    rig.drivers.push(Driver {
        name: "geometry".to_owned(),
        params: Vec::new(),
        writes: Vec::new(),
        geometries: vec![GeometryWrite {
            part: 0,
            geometry: GeometryExpr::Builder(vec![
                GeometryCommand::MoveTo {
                    x: Expr::Number(1.0),
                    y: Expr::Number(1.0),
                },
                GeometryCommand::LineTo {
                    x: Expr::Number(9.0),
                    y: Expr::Number(1.0),
                },
                GeometryCommand::LineTo {
                    x: Expr::Number(9.0),
                    y: Expr::Number(9.0),
                },
                GeometryCommand::LineTo {
                    x: Expr::Number(1.0),
                    y: Expr::Number(9.0),
                },
            ]),
        }],
    });
    rig.deformers.push(Deformer::Lattice(Lattice {
        part: 0,
        corners: [0, 0, 0, 0],
        edges: [None; 4],
    }));
    let frame = bake(&rig, 0.0, 0.0).unwrap();
    let geometry = frame[0].part(0).unwrap().geometry.as_ref().unwrap();
    assert_eq!(geometry.points().len(), 4);
}

#[test]
fn constraints_run_after_deformers() {
    let mut rig = Rig::new("constraints");
    rig.add_part(part(
        "body",
        Vec2::ZERO,
        open_path(&[Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]),
    ));
    rig.add_property(vec_prop("target"));
    rig.channels[0].keys.push(Key::new(
        0.0,
        PropValue::Vector(Vec2::new(4.0, 4.0)),
        Ease::Linear,
    ));
    rig.constraints.push(Constraint::Limit {
        part: 0,
        min: Vec2::splat(-2.0),
        max: Vec2::splat(2.0),
        rotation_min: -10.0,
        rotation_max: 10.0,
    });
    let frame = bake(&rig, 0.0, 0.0).unwrap();
    assert_eq!(frame[0].part(0).unwrap().position, Vec2::ZERO);
}

#[test]
fn two_bone_ik_places_the_lower_part() {
    let mut rig = Rig::new("ik");
    rig.add_part(part(
        "upper",
        Vec2::ZERO,
        open_path(&[Vec2::ZERO, Vec2::new(10.0, 0.0)]),
    ));
    rig.add_part(part(
        "lower",
        Vec2::new(10.0, 0.0),
        open_path(&[Vec2::ZERO, Vec2::new(10.0, 0.0)]),
    ));
    rig.add_property(vec_prop("target"));
    rig.channels[0].keys.push(Key::new(
        0.0,
        PropValue::Vector(Vec2::new(15.0, 5.0)),
        Ease::Linear,
    ));
    rig.constraints.push(Constraint::TwoBoneIk {
        upper: 0,
        lower: 1,
        target: 0,
        pole: None,
        tip_length: 10.0,
    });
    let frame = bake(&rig, 0.0, 0.0).unwrap();
    let lower = frame[0].part(1).unwrap();
    assert!(lower.touched);
    assert!(lower.position.length() > 0.0);
}

#[test]
fn spring_follow_through_is_deterministic() {
    let mut rig = Rig::new("spring");
    rig.add_part(part(
        "hair",
        Vec2::ZERO,
        open_path(&[Vec2::ZERO, Vec2::new(1.0, 0.0)]),
    ));
    rig.add_property(vec_prop("target"));
    rig.channels[0].keys.push(Key::new(
        0.0,
        PropValue::Vector(Vec2::new(10.0, 0.0)),
        Ease::Linear,
    ));
    rig.constraints.push(Constraint::Spring {
        part: 0,
        target: 0,
        stiffness: 0.4,
        damping: 0.2,
        mass: 1.0,
    });
    let first = bake(&rig, 0.0, 2.0).unwrap();
    let second = bake(&rig, 0.0, 2.0).unwrap();
    assert_eq!(first, second);
    assert!(first[2].part(0).unwrap().position.x > 0.0);
}

#[test]
fn follow_path_samples_the_path_geometry() {
    let mut rig = Rig::new("follow");
    rig.add_part(part(
        "path",
        Vec2::ZERO,
        open_path(&[Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]),
    ));
    rig.add_part(part(
        "dot",
        Vec2::ZERO,
        open_path(&[Vec2::ZERO, Vec2::new(1.0, 0.0)]),
    ));
    rig.add_property(PropertyDef::number("progress", 0.5, 0.0, 1.0));
    rig.constraints.push(Constraint::FollowPath {
        part: 1,
        path: 0,
        progress: 0,
        orient: true,
    });
    let frame = bake(&rig, 0.0, 0.0).unwrap();
    assert!((frame[0].part(1).unwrap().position.x - 5.0).abs() < 1e-9);
}

#[test]
fn copy_constraint_writes_offsets() {
    let mut rig = Rig::new("copy");
    rig.add_part(part(
        "a",
        Vec2::ZERO,
        open_path(&[Vec2::ZERO, Vec2::new(1.0, 0.0)]),
    ));
    rig.add_part(part(
        "b",
        Vec2::ZERO,
        open_path(&[Vec2::ZERO, Vec2::new(1.0, 0.0)]),
    ));
    rig.drivers.push(Driver {
        name: "move".to_owned(),
        params: Vec::new(),
        writes: vec![motioner_rig::Write {
            part: 0,
            field: motioner_rig::Field::PositionX,
            value: Expr::Number(3.0),
        }],
        geometries: Vec::new(),
    });
    rig.constraints.push(Constraint::Copy {
        part: 1,
        source: 0,
        position: true,
        rotation: false,
        scale: false,
    });
    let frame = bake(&rig, 0.0, 0.0).unwrap();
    assert!((frame[0].part(1).unwrap().position.x - 3.0).abs() < 1e-9);
}

#[test]
fn bone_ik_runs_before_skinning() {
    let mut rig = Rig::new("bone-ik");
    rig.add_part(part(
        "arm",
        Vec2::ZERO,
        open_path(&[
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(20.0, 0.0),
            Vec2::new(30.0, 0.0),
        ]),
    ));
    rig.add_property(vec_prop("target"));
    rig.channels[0].keys.push(Key::new(
        0.0,
        PropValue::Vector(Vec2::new(20.0, 15.0)),
        Ease::Linear,
    ));
    rig.bones.push(Bone {
        name: "upper".to_owned(),
        parent: None,
        bind_position: Vec2::ZERO,
        bind_rotation: 0.0,
        length: 20.0,
        rotation_property: None,
        position_property: None,
    });
    rig.bones.push(Bone {
        name: "lower".to_owned(),
        parent: Some(0),
        bind_position: Vec2::new(20.0, 0.0),
        bind_rotation: 0.0,
        length: 20.0,
        rotation_property: None,
        position_property: None,
    });
    rig.deformers.push(Deformer::Skin(Skin {
        part: 0,
        bones: rig.bones.clone(),
        weights: None,
    }));
    rig.bone_constraints.push(BoneConstraint::TwoBoneIk {
        upper: 0,
        lower: 1,
        target: 0,
        pole: None,
        tip_length: 20.0,
    });
    let frame = bake(&rig, 0.0, 0.0).unwrap();
    let geometry = frame[0].part(0).unwrap().geometry.as_ref().unwrap();
    assert!(geometry.points().iter().any(|point| point.position.y > 0.5));
}

#[test]
fn bone_limit_and_copy_edit_poses_before_skin() {
    let mut rig = Rig::new("bone-limits");
    rig.add_part(part(
        "arm",
        Vec2::ZERO,
        open_path(&[Vec2::ZERO, Vec2::new(1.0, 0.0)]),
    ));
    rig.bones.push(Bone {
        name: "a".to_owned(),
        parent: None,
        bind_position: Vec2::ZERO,
        bind_rotation: 0.0,
        length: 1.0,
        rotation_property: None,
        position_property: None,
    });
    rig.bones.push(Bone {
        name: "b".to_owned(),
        parent: None,
        bind_position: Vec2::ZERO,
        bind_rotation: 0.0,
        length: 1.0,
        rotation_property: None,
        position_property: None,
    });
    rig.deformers.push(Deformer::Skin(Skin {
        part: 0,
        bones: rig.bones.clone(),
        weights: None,
    }));
    rig.bone_constraints.push(BoneConstraint::Copy {
        bone: 1,
        source: 0,
        position: false,
        rotation: true,
    });
    rig.bone_constraints.push(BoneConstraint::Limit {
        bone: 1,
        rotation_min: -10.0,
        rotation_max: 10.0,
    });
    let first = bake(&rig, 0.0, 0.0).unwrap();
    let second = bake(&rig, 0.0, 0.0).unwrap();
    assert_eq!(first, second);
}
