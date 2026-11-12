//! Geometry bake versus sample-time cost (D-0058).
//!
//! Run with `cargo run -p motioner_rig --release --example geometry_bench`.

use std::time::Instant;

use motioner_core::{Ease, GeometryChannel, LineCap, LineJoin, PathGeometry, PathPoint, Vec2};
use motioner_rig::{bake, Driver, Expr, GeometryExpr, GeometryWrite, PartBase, PropertyDef, Rig};

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn lid_expression() -> GeometryExpr {
    GeometryExpr::RoundedRect {
        width: Expr::Property(0),
        height: Expr::Number(20.0),
        radius: Expr::Number(5.0),
    }
}

fn base_geometry() -> PathGeometry {
    PathGeometry::new(
        (0..8)
            .map(|index| {
                let index = f64::from(index);
                PathPoint::corner(Vec2::new(index * 4.0, 0.0))
            })
            .collect(),
        true,
        LineCap::Round,
        LineJoin::Round,
    )
    .unwrap()
}

fn rig() -> Rig {
    let mut rig = Rig::new("lid");
    let mut part = PartBase::at("lid", Vec2::ZERO);
    part.geometry = Some(base_geometry());
    rig.add_part(part);
    rig.add_property(PropertyDef::number("width", 30.0, 20.0, 40.0));
    rig.channels[0].keys.push(motioner_rig::Key::new(
        0.0,
        motioner_rig::PropValue::Number(30.0),
        Ease::Linear,
    ));
    rig.channels[0].keys.push(motioner_rig::Key::new(
        120.0,
        motioner_rig::PropValue::Number(40.0),
        Ease::Linear,
    ));
    rig.drivers.push(Driver {
        name: "lid".to_owned(),
        params: vec![0],
        writes: Vec::new(),
        geometries: vec![GeometryWrite {
            part: 0,
            geometry: lid_expression(),
        }],
    });
    rig
}
#[allow(clippy::cast_precision_loss)]
fn channel() -> GeometryChannel {
    let mut channel = GeometryChannel::new(base_geometry());
    for frame in 0..=120 {
        let offset = frame as f64 / 120.0;
        channel
            .insert_key(
                frame,
                (0..8)
                    .map(|index| {
                        let index = f64::from(index);
                        PathPoint::corner(Vec2::new(index * 4.0, offset))
                    })
                    .collect(),
                Ease::Linear,
            )
            .unwrap();
    }
    channel
}

#[allow(clippy::cast_precision_loss)] // benchmark frames are small
fn main() {
    let rig = rig();
    let mut bake_times = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        let frames = bake(&rig, 0.0, 120.0).unwrap();
        assert_eq!(frames.len(), 121);
        bake_times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let bake_ms = median(&mut bake_times);

    let channel = channel();
    let mut sample_times = Vec::new();
    for _ in 0..100 {
        let start = Instant::now();
        for frame in 0..=120 {
            let geometry = channel
                .sample(f64::from(frame), motioner_eval::ease)
                .unwrap();
            assert_eq!(geometry.points().len(), 8);
        }
        sample_times.push(start.elapsed().as_secs_f64() * 1_000_000.0);
    }
    let sample_us = median(&mut sample_times);

    println!(
        "bake 121 frames: {bake_ms:.3} ms ({:.1} us/frame)",
        bake_ms * 1000.0 / 121.0
    );
    println!(
        "sample 121 keys: {sample_us:.1} us ({:.2} us/frame)",
        sample_us / 121.0
    );
}
