//! Release-mode cost of the textured mesh path compared with sprites.
//!
//! Run with:
//!
//! ```text
//! cargo test -p motioner_render --release --test mesh_bench -- --ignored --nocapture
//! ```
//!
//! Each scene renders the same number of items through the `Pure2D` headless
//! harness (1280x720) for 20 warm-up frames and 60 measured frames.  The table
//! reports the median and p90 frame time and the per-draw time
//! (`frame_ms / draws`).
#![allow(clippy::cast_precision_loss)] // item counts and pixel positions are small

use std::error::Error;
use std::time::Instant;

use ferrous_2d::render::{MeshVertex2d, SpriteInstance};
use ferrous_renderer::headless::Pure2dHarness;
use glam::{Mat4, Quat, Vec2, Vec3};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const WARMUP: usize = 20;
const SAMPLES: usize = 60;

fn grid(columns: u32, rows: u32, center: Vec2, size: f32) -> (Vec<MeshVertex2d>, Vec<u32>) {
    let mut vertices = Vec::with_capacity((columns * rows) as usize);
    for row in 0..rows {
        for column in 0..columns {
            let u = column as f32 / (columns - 1) as f32;
            let v = row as f32 / (rows - 1) as f32;
            vertices.push(MeshVertex2d::new(
                [center.x + (u - 0.5) * size, center.y + (v - 0.5) * size],
                [u, v],
            ));
        }
    }
    let mut indices = Vec::new();
    for row in 0..rows - 1 {
        for column in 0..columns - 1 {
            let top_left = row * columns + column;
            let top_right = top_left + 1;
            let bottom_left = top_left + columns;
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
    (vertices, indices)
}

fn sprite(center: Vec2, size: f32) -> SpriteInstance {
    let model = Mat4::from_scale_rotation_translation(
        Vec3::new(size, size, 1.0),
        Quat::IDENTITY,
        Vec3::new(center.x, center.y, 0.0),
    );
    SpriteInstance {
        transform_c0: model.x_axis.into(),
        transform_c1: model.y_axis.into(),
        transform_c2: model.z_axis.into(),
        transform_c3: model.w_axis.into(),
        color: [1.0, 1.0, 1.0, 1.0],
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        properties: [0.0, 0.0, 0.0, 0.0],
    }
}

fn texture(harness: &mut Pure2dHarness) -> u32 {
    let mut data = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64 {
        for x in 0..64 {
            let cell = ((x / 8) + (y / 8)) % 2 == 0;
            let color = if cell {
                [60, 140, 240, 255]
            } else {
                [240, 90, 70, 255]
            };
            data.extend_from_slice(&color);
        }
    }
    harness.register_texture_rgba8(64, 64, &data)
}

fn positions(count: usize) -> Vec<Vec2> {
    // A 10x5 grid of 20 px items spread over the frame.
    (0..count)
        .map(|index| {
            let column = (index % 10) as f32;
            let row = (index / 10) as f32;
            Vec2::new(40.0 + column * 130.0, 40.0 + row * 130.0)
        })
        .collect()
}

fn measure(
    label: &str,
    items: usize,
    mut draw: impl FnMut(&mut Pure2dHarness, u32),
) -> Result<(), Box<dyn Error>> {
    let mut harness =
        Pure2dHarness::new(WIDTH, HEIGHT, ferrous_renderer::headless::DEFAULT_FORMAT)?;
    let texture = texture(&mut harness);
    for _ in 0..WARMUP {
        harness.begin_frame();
        draw(&mut harness, texture);
        harness.render_frame_blocking();
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        harness.begin_frame();
        draw(&mut harness, texture);
        let start = Instant::now();
        harness.render_frame_blocking();
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    let median = samples[samples.len() / 2];
    let p90 = samples[(samples.len() * 9) / 10];
    println!(
        "{label:<22} items {items:>3}  median {median:>7.3} ms  p90 {p90:>7.3} ms  per item {:.4} ms",
        median / items as f64
    );
    Ok(())
}

#[test]
#[ignore = "release measurement; run explicitly with --release --ignored"]
fn mesh_versus_sprite_cost() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    for (columns, rows, label) in [(8, 8, "8x8"), (32, 32, "32x32")] {
        for count in [1usize, 50] {
            let places = positions(count);
            let items: Vec<_> = places
                .iter()
                .map(|&center| grid(columns, rows, center, 20.0))
                .collect();
            let label = format!("mesh {label} x{count}");
            measure(&label, count, |h, texture| {
                for (index, (vertices, indices)) in items.iter().enumerate() {
                    h.draw_textured_mesh(
                        texture,
                        vertices,
                        indices,
                        Mat4::from_translation(Vec3::new(0.0, 0.0, index as f32 * 0.001)),
                        ferrous_2d::render::BlendMode2d::Normal,
                    );
                }
            })?;
        }
    }
    for count in [1usize, 50] {
        let places = positions(count);
        let label = format!("sprite x{count}");
        measure(&label, count, |h, texture| {
            for (index, &center) in places.iter().enumerate() {
                let mut instance = sprite(center, 20.0);
                instance.transform_c3[2] = index as f32 * 0.001;
                h.draw_sprite(texture, instance);
            }
        })?;
    }
    Ok(())
}
