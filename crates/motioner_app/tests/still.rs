//! End-to-end check of `motioner still`.
//!
//! Skips cleanly when the machine has no usable GPU adapter, so CPU-only CI
//! stays green.

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use motioner_render::{FrameRenderer, RenderError};

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/hello.motion");

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("motioner-app-still-{}", std::process::id()))
}

#[test]
fn still_writes_a_1920x1080_png() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    match FrameRenderer::new(1920, 1080) {
        Ok(_) => {}
        Err(RenderError::NoAdapter { message }) => {
            eprintln!("skipping still test: no usable adapter ({message})");
            return Ok(());
        }
        Err(other) => panic!("renderer creation failed: {other}"),
    }

    let directory = temp_dir();
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory)?;
    let output_path = directory.join("hello_f0.png");

    let output = Command::new(env!("CARGO_BIN_EXE_motioner"))
        .arg("still")
        .arg(INPUT)
        .arg("--frame")
        .arg("0")
        .arg("--out")
        .arg(&output_path)
        .output()?;
    assert!(
        output.status.success(),
        "still failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let file = fs::File::open(&output_path)?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0_u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer)?;
    assert_eq!((info.width, info.height), (1920, 1080));
    assert_eq!(info.color_type, png::ColorType::Rgba);

    fs::remove_dir_all(&directory)?;
    Ok(())
}
