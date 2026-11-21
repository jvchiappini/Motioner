# motioner_io

Project and media I/O for Motioner: the native Motioner Document Format
(MDF), the content-addressed asset store, project migrations, autosave and
the first-render export path.

This crate's export path works on plain, tightly packed RGBA8 bytes with the
top row first. It never depends on `motioner_render`; the renderer hands it
frames and it writes them out.

## Public API

```rust
pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), ExportError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoSettings {
    pub width: u32,
    pub height: u32,
    pub fps_numerator: u32,
    pub fps_denominator: u32,
    pub crf: u8,
}

impl VideoSettings {
    pub fn youtube_1080p() -> Self; // 1920x1080, 60/1, CRF 18
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FfmpegLocation {
    Path(PathBuf),
    FromEnvOrPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodeSummary {
    pub frames: u64,
    pub status: std::process::ExitStatus,
}

pub struct Mp4Encoder;

impl Mp4Encoder {
    pub fn start(
        path: &Path,
        settings: &VideoSettings,
        ffmpeg: &FfmpegLocation,
    ) -> Result<Self, ExportError>;
    pub fn push_frame(&mut self, rgba: &[u8]) -> Result<(), ExportError>;
    pub fn finish(self) -> Result<EncodeSummary, ExportError>;
}
```

`ExportError` is a typed error implementing `Display` and `std::error::Error`.
Its variants cover a wrong frame-buffer length, dimensions that cannot be
addressed as bytes, I/O failures, PNG encoder failures, a missing `ffmpeg`
binary, a failed `ffmpeg` run and a failed frame write. The missing-binary
case is the dedicated `FfmpegNotFound` variant.

## PNG stills

`write_png` validates that the buffer is exactly `width * height * 4` bytes,
then writes an 8-bit RGBA PNG (`png::ColorType::Rgba`,
`png::BitDepth::Eight`) with the `sRGB` chunk set to the perceptual rendering
intent through `Encoder::set_source_srgb`. Viewers and NLEs therefore treat
the pixels as sRGB-encoded RGBA, matching what the renderer produces.

## Finding ffmpeg

Resolution order:

1. For `FfmpegLocation::Path(path)`, that exact executable is used.
2. For `FfmpegLocation::FromEnvOrPath`, the `MOTIONER_FFMPEG` environment
   variable is used when it is set and non-empty.
3. Otherwise `ffmpeg` is resolved from `PATH`.

If the executable cannot be spawned because it does not exist, the call
fails with `ExportError::FfmpegNotFound`, whose message names both
`MOTIONER_FFMPEG` and `PATH` so the fix is obvious.

## Encoding settings

Every encode uses this fixed command template; only the dimensions, frame
rate, CRF and output path vary:

```
ffmpeg -y -loglevel error
  -f rawvideo -pix_fmt rgba -s {W}x{H} -framerate {N}/{D} -i -
  -vf scale=out_color_matrix=bt709:out_range=tv
  -c:v libx264 -preset medium -crf {crf}
  -pix_fmt yuv420p
  -colorspace bt709 -color_primaries bt709 -color_trc bt709
  -movflags +faststart
  {output}
```

Why these settings suit a YouTube upload:

- **H.264 through `libx264`** is the most widely accepted upload and
  playback codec; x264's High profile gives good quality per bit at 1080p.
- **`yuv420p`** is the chroma format every player and browser can decode
  without conversion; 4:4:4 or 10-bit output would be rejected or
  downsampled by many players.
- **`-crf 18`** is visually close to lossless for typical motion-graphics
  content while keeping files and upload times reasonable. (`youtube_1080p`
  uses CRF 18; other callers may choose their own value.)
- **`-preset medium`** balances encode speed against compression; it is the
  fixed preset for this pipeline.
- **BT.709 tagging** (`-colorspace`, `-color_primaries`, `-color_trc bt709`,
  plus the explicit `scale=out_color_matrix=bt709:out_range=tv` conversion)
  tells YouTube and playback software the correct matrix, primaries and
  transfer function. Without it, players guess and colors drift.
- **`-movflags +faststart`** moves the MP4 index to the front of the file so
  it can start playing while downloading, which is what streaming services
  expect.
- **`-loglevel error`** keeps stderr small in normal operation.

## Raw-frame pipeline

`Mp4Encoder::start` spawns `ffmpeg` with stdin piped, stdout discarded and
stderr piped. Each `push_frame` checks the byte length and writes one RGBA8
frame with `write_all`; frame order and the rational frame rate are the only
timing information `ffmpeg` receives. `finish` closes stdin (signalling end
of stream), waits for `ffmpeg`, and returns an `EncodeSummary` containing the
frames written and the process exit status. A non-zero exit becomes
`ExportError::Ffmpeg` with the stderr tail.

## stderr draining

`ffmpeg` writes diagnostics to stderr, but nobody reads the child's stderr
synchronously while frames are streaming. To keep the child from blocking on
a full pipe, a dedicated thread drains stderr continuously into a bounded
shared buffer (a `VecDeque` capped at 8 KiB, keeping the newest bytes). The
thread is joined by `finish` and by `Drop`, so it never outlives the
encoder. The buffered tail is included in encode and exit errors.

## Cancellation

Dropping an `Mp4Encoder` without calling `finish` closes stdin first, then
kills and waits for the child, then joins the stderr thread. That prevents
zombie processes and never panics, even if a thread panicked earlier.
