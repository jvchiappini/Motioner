use anyhow::Result;
use image::{Rgba, RgbaImage};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// CPU rasteriser placeholder + ffmpeg exporter.
/// Returns the path to the generated MP4 on success.
#[allow(dead_code)]
pub fn render_and_encode(
    fps: u32,
    duration_secs: f32,
    progress: &Arc<AtomicUsize>,
) -> Result<PathBuf> {
    let total_frames = (fps as f32 * duration_secs).ceil() as usize;
    let width = 1280u32;
    let height = 720u32;

    let dir = tempfile::tempdir()?;
    let dir_path = dir.path().to_path_buf();

    for i in 0..total_frames {
        progress.store(i, Ordering::SeqCst);
        let t = i as f32 / (total_frames.saturating_sub(1) as f32).max(1.0);
        let mut img: RgbaImage = RgbaImage::from_pixel(width, height, Rgba([16, 18, 20, 255]));

        // simple animated circle (same logic used in preview)
        let cx = (0.1 + 0.8 * t) * (width as f32);
        let cy = (height as f32) * 0.5;
        let radius = 60.0 + 30.0 * (t * std::f32::consts::PI).sin();

        let radius_sq = radius * radius;
        for y in 0..(height as i32) {
            for x in 0..(width as i32) {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let dist2 = dx * dx + dy * dy;
                if dist2 <= radius_sq {
                    img.put_pixel(x as u32, y as u32, Rgba([120u8, 200u8, 255u8, 255u8]));
                }
            }
        }

        let filename = dir_path.join(format!("frame_{:05}.png", i));
        img.save(&filename)?;
    }

    let out_file = dir_path.join("out.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-framerate",
            &fps.to_string(),
            "-i",
            &format!("{}/frame_%05d.png", dir_path.display()),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            out_file.to_str().unwrap(),
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!("ffmpeg returned non-zero status");
    }
    progress.store(usize::MAX, Ordering::SeqCst);
    Ok(out_file)
}
