//! Behaviour tests for the headless rig evaluator.

use std::collections::BTreeMap;

use motioner_core::{Ease, Vec2};
use motioner_rig::{
    bake, evaluate, Blink, Clip, ClipKey, Compare, Driver, Expr, Field, Key, Machine, PartBase,
    Play, Pose, PoseTrack, PropKind, PropValue, PropertyDef, Rig, State, Transition, Write,
};

fn needle_rig() -> Rig {
    let mut rig = Rig::new("needle");
    rig.add_part(PartBase::at("needle", Vec2::new(50.0, 50.0)));
    rig.add_property(PropertyDef::number("angle", 0.0, -180.0, 180.0));
    rig.channels[0]
        .keys
        .push(Key::new(0.0, PropValue::Number(0.0), Ease::Linear));
    rig.channels[0]
        .keys
        .push(Key::new(10.0, PropValue::Number(90.0), Ease::Linear));
    rig.drivers.push(Driver {
        name: "turn".to_owned(),
        params: vec![0],
        writes: vec![Write {
            part: 0,
            field: Field::Rotation,
            value: Expr::Property(0),
        }],
        geometries: Vec::new(),
    });
    rig
}

#[test]
fn channels_hold_before_and_after_and_blend_in_between() {
    let rig = needle_rig();
    assert_eq!(rig.base_value(0).number(), Some(0.0));
    let frames = bake(&rig, 0.0, 10.0).expect("bake");
    assert!(frames[0].parts[0].rotation.abs() < 1e-12);
    assert!((frames[5].parts[0].rotation - 45.0).abs() < 1e-9);
    assert!((frames[10].parts[0].rotation - 90.0).abs() < 1e-9);
}

#[test]
fn baking_is_repeatable_and_backwards_seeking_is_exact() {
    let rig = needle_rig();
    let first = bake(&rig, 0.0, 20.0).expect("bake");
    let second = bake(&rig, 0.0, 20.0).expect("bake");
    assert_eq!(first, second);
    let seek = evaluate(&rig, 7.0).expect("evaluate");
    assert_eq!(seek, first[7]);
}

#[test]
fn poses_blend_from_the_value_at_the_track_start() {
    let mut rig = Rig::new("pose");
    rig.add_part(PartBase::at("body", Vec2::ZERO));
    rig.add_property(PropertyDef::number("size", 1.0, 0.0, 10.0));
    rig.poses.push(Pose {
        name: "big".to_owned(),
        values: BTreeMap::from([(0, PropValue::Number(3.0))]),
        reset: false,
    });
    rig.tracks.push(PoseTrack {
        pose: 0,
        start: 10.0,
        duration: 10.0,
        ease: Ease::Linear,
    });
    let frames = bake(&rig, 0.0, 20.0).expect("bake");
    assert_eq!(frames[9].properties[&0].number(), Some(1.0));
    assert!((frames[15].properties[&0].number().unwrap() - 2.0).abs() < 1e-9);
    assert!((frames[20].properties[&0].number().unwrap() - 3.0).abs() < 1e-9);
}

#[test]
fn clips_sample_relative_keys_and_loop() {
    let mut rig = Rig::new("clip");
    rig.add_part(PartBase::at("body", Vec2::ZERO));
    rig.add_property(PropertyDef::number("x", 0.0, 0.0, 100.0));
    rig.clips.push(Clip {
        name: "slide".to_owned(),
        keys: vec![
            ClipKey {
                property: 0,
                frame: 0.0,
                value: PropValue::Number(0.0),
                ease: Ease::Linear,
            },
            ClipKey {
                property: 0,
                frame: 10.0,
                value: PropValue::Number(10.0),
                ease: Ease::Linear,
            },
        ],
        length: 10.0,
    });
    rig.plays.push(Play {
        clip: 0,
        start: 10.0,
        loops: 2,
        speed: 1.0,
    });
    let frames = bake(&rig, 0.0, 30.0).expect("bake");
    assert_eq!(frames[5].properties[&0].number(), Some(0.0));
    assert!((frames[15].properties[&0].number().unwrap() - 5.0).abs() < 1e-9);
    assert!((frames[25].properties[&0].number().unwrap() - 5.0).abs() < 1e-9);
    assert!((frames[30].properties[&0].number().unwrap() - 10.0).abs() < 1e-9);
}

#[test]
fn the_machine_transitions_when_an_input_is_keyed() {
    let mut rig = Rig::new("mascot");
    rig.add_part(PartBase::at("body", Vec2::ZERO));
    rig.add_property(PropertyDef::typed(
        "mood",
        PropKind::Enum,
        PropValue::Text("calm".to_owned()),
    ));
    rig.add_property(PropertyDef::number("mouth", 0.0, -20.0, 20.0));
    rig.channels[0].keys.push(Key::new(
        0.0,
        PropValue::Text("calm".to_owned()),
        Ease::Linear,
    ));
    rig.channels[0].keys.push(Key::new(
        20.0,
        PropValue::Text("sad".to_owned()),
        Ease::Hold,
    ));
    rig.poses.push(Pose {
        name: "calm".to_owned(),
        values: BTreeMap::from([(1, PropValue::Number(5.0))]),
        reset: false,
    });
    rig.poses.push(Pose {
        name: "sad".to_owned(),
        values: BTreeMap::from([(1, PropValue::Number(-10.0))]),
        reset: false,
    });
    rig.machine = Some(Machine {
        states: vec![
            State {
                name: "calm".to_owned(),
                pose: Some(0),
                clip: None,
                speed: 1.0,
                looping: true,
            },
            State {
                name: "sad".to_owned(),
                pose: Some(1),
                clip: None,
                speed: 1.0,
                looping: true,
            },
        ],
        transitions: vec![Transition {
            from: Some(0),
            to: 1,
            input: 0,
            compare: Compare::Equals,
            value: PropValue::Text("sad".to_owned()),
            duration: 10.0,
        }],
        default: 0,
    });
    let frames = bake(&rig, 0.0, 40.0).expect("bake");
    assert!((frames[5].properties[&1].number().unwrap() - 5.0).abs() < 1e-9);
    // Halfway through the 10-frame blend.
    assert!((frames[25].properties[&1].number().unwrap() + 2.5).abs() < 1.5);
    assert!((frames[40].properties[&1].number().unwrap() + 10.0).abs() < 1e-9);
    assert!(frames.iter().any(|frame| !frame.events.is_empty()));
}

#[test]
fn drivers_evaluate_the_expression_subset_and_are_bounded() {
    let mut rig = Rig::new("driver");
    rig.add_part(PartBase::at("body", Vec2::ZERO));
    rig.add_property(PropertyDef::number("amplitude", 2.0, 0.0, 10.0));
    rig.channels[0]
        .keys
        .push(Key::new(0.0, PropValue::Number(2.0), Ease::Linear));
    rig.drivers.push(Driver {
        name: "bulge".to_owned(),
        params: vec![0],
        writes: vec![Write {
            part: 0,
            field: Field::ScaleX,
            value: Expr::Mul(
                Box::new(Expr::Property(0)),
                Box::new(Expr::Add(
                    Box::new(Expr::Number(1.0)),
                    Box::new(Expr::Sin(Box::new(Expr::Number(0.0)))),
                )),
            ),
        }],
        geometries: Vec::new(),
    });
    let frame = evaluate(&rig, 0.0).expect("evaluate");
    assert!((frame.parts[0].scale.x - 2.0).abs() < 1e-12);
}

#[test]
fn blinks_follow_a_deterministic_schedule() {
    let mut rig = Rig::new("blink");
    rig.add_part(PartBase::at("eye", Vec2::ZERO));
    rig.add_property(PropertyDef::number("lid", 0.0, 0.0, 1.0));
    rig.blinks.push(Blink {
        start: 0.0,
        period: 20.0,
        jitter: 0.0,
        duration: 2.0,
        seed: 7,
        props: vec![(0, PropValue::Number(1.0))],
    });
    let first = bake(&rig, 0.0, 40.0).expect("bake");
    let second = bake(&rig, 0.0, 40.0).expect("bake");
    assert_eq!(first, second);
    assert_eq!(first[0].properties[&0].number(), Some(1.0));
    assert_eq!(first[1].properties[&0].number(), Some(1.0));
    assert_eq!(first[2].properties[&0].number(), Some(0.0));
    assert_eq!(first[20].properties[&0].number(), Some(1.0));
}

#[test]
fn a_driver_that_produces_a_non_finite_value_is_a_typed_error() {
    let mut rig = Rig::new("bad");
    rig.add_part(PartBase::at("body", Vec2::ZERO));
    rig.add_property(PropertyDef::number("zero", 0.0, 0.0, 1.0));
    rig.drivers.push(Driver {
        name: "explode".to_owned(),
        params: vec![0],
        writes: vec![Write {
            part: 0,
            field: Field::PositionX,
            value: Expr::Div(
                Box::new(Expr::Number(1.0)),
                Box::new(Expr::Sub(
                    Box::new(Expr::Number(0.0)),
                    Box::new(Expr::Number(0.0)),
                )),
            ),
        }],
        geometries: Vec::new(),
    });
    // Division by zero is defined as 0 by the IR, so this succeeds.
    let frame = evaluate(&rig, 0.0).expect("division by zero yields zero");
    assert!(frame.parts[0].position.x.abs() < 1e-12);
    rig.drivers[0].writes[0].value =
        Expr::Mul(Box::new(Expr::Number(1e308)), Box::new(Expr::Number(10.0)));
    assert!(evaluate(&rig, 0.0).is_err());
}
