//! PNG still-image writing.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use crate::error::ExportError;
use crate::frame::validate_rgba;

/// Writes one 8-bit RGBA frame to `path` as a PNG marked as sRGB.
///
/// The image is stored with `png`'s `Rgba` color type at 8 bits per channel.
/// An `sRGB` chunk with the perceptual rendering intent is emitted so viewers
/// and editors interpret the bytes as sRGB-encoded RGBA.
///
/// # Errors
///
/// Returns [`ExportError::SizeMismatch`] when `rgba` is not exactly
/// `width * height * 4` bytes, [`ExportError::DimensionsTooLarge`] when the
/// dimensions overflow a `usize`, [`ExportError::Io`] when the file cannot be
/// created, and [`ExportError::Png`] when the PNG encoder fails.
pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), ExportError> {
    validate_rgba(width, height, rgba)?;
    let file = File::create(path).map_err(|err| ExportError::Io {
        message: format!("cannot create {}: {err}", path.display()),
    })?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    let mut writer = encoder.write_header().map_err(|err| ExportError::Png {
        message: format!("cannot write PNG header for {}: {err}", path.display()),
    })?;
    writer
        .write_image_data(rgba)
        .map_err(|err| ExportError::Png {
            message: format!("cannot write PNG data for {}: {err}", path.display()),
        })?;
    writer.finish().map_err(|err| ExportError::Png {
        message: format!("cannot finish PNG {}: {err}", path.display()),
    })
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::write_png;
    use crate::error::ExportError;

    const WIDTH: u32 = 7;
    const HEIGHT: u32 = 5;

    fn temp_png_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "motioner-io-png-{}-{unique}.png",
            std::process::id()
        ))
    }

    fn sample_pixels() -> Vec<u8> {
        let len = usize::try_from(WIDTH * HEIGHT * 4).unwrap_or(0);
        (0..len)
            .map(|index| u8::try_from(index % 256).unwrap_or(0))
            .collect()
    }

    #[test]
    fn png_round_trip_preserves_rgba() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_png_path();
        let pixels = sample_pixels();
        write_png(&path, WIDTH, HEIGHT, &pixels)?;

        let file = File::open(&path)?;
        let decoder = png::Decoder::new(file);
        let mut reader = decoder.read_info()?;
        let mut reread = vec![0_u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut reread)?;
        reread.truncate(info.buffer_size());

        assert_eq!((info.width, info.height), (WIDTH, HEIGHT));
        assert_eq!(reread, pixels);

        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn length_mismatch_is_reported() {
        let path = temp_png_path();
        let pixels = [0_u8; 8];
        let result = write_png(&path, WIDTH, HEIGHT, &pixels);
        match result {
            Err(ExportError::SizeMismatch { expected, actual }) => {
                assert_eq!(expected, usize::try_from(WIDTH * HEIGHT * 4).unwrap_or(0));
                assert_eq!(actual, pixels.len());
            }
            other => panic!("expected SizeMismatch, got {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }
}
