# motioner_render

Motioner's rendering adapter over the pinned FerrousEngine.

## Purpose

This crate is the only place in Motioner that talks to the engine. It converts
an evaluated [`motioner_eval::RenderPlan`] into a headless frame using the
engine's Pure2D path, then reads the frame back as straight-alpha sRGB RGBA8
bytes with row 0 at the top. Engine types (`ferrous_*`, `wgpu`, `glam`) are
private: with the default features they never appear in a public signature or a
public error payload.

The opt-in `live` feature (only `motioner_ui` enables it) adds `LivePainter`.
It is the one documented exception: `LivePainter::new` and `paint` name
`ferrous_renderer::Renderer` so the editor can paint the same plan into the
window's Pure2D pass. `LivePainter` and `FrameRenderer` share the single
"render plan item → engine draw" path, so the live preview and the export
cannot drift apart.

Rendering is deterministic. The only clock-dependent value is
`FrameStats::cpu_ms`, which the engine measures and this crate merely reports.

## Public API

```rust
pub struct FrameRenderer;

impl FrameRenderer {
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError>;
    pub fn render(
        &mut self,
        document: &motioner_core::Document,
        plan: &motioner_eval::RenderPlan,
        out: &mut Vec<u8>,
    ) -> Result<FrameStats, RenderError>;
    pub fn adapter_name(&self) -> &str;
}

pub struct FrameStats {
    pub shape_draws: u32,
    pub text_draws: u32,
    pub draw_calls: u32,
    pub missing_glyphs: u32,
    pub non_finite_items: u32,
    pub cpu_ms: f64,
}

pub enum RenderError {
    NoAdapter { message: String },
    SizeMismatch { expected: (u32, u32), actual: (u32, u32) },
    Font { message: String },
    Readback { message: String },
}

pub struct ViewFit { /* scale, world offset */ }

pub struct PaintStats {
    pub shape_draws: u32,
    pub text_draws: u32,
    pub missing_glyphs: u32,
    pub non_finite_items: u32,
}

#[cfg(feature = "live")]
pub struct LivePainter;

#[cfg(feature = "live")]
impl LivePainter {
    pub fn new(
        renderer: &mut ferrous_renderer::Renderer,
    ) -> Result<Self, RenderError>;
    pub fn paint(
        &mut self,
        renderer: &mut ferrous_renderer::Renderer,
        document: &motioner_core::Document,
        plan: &motioner_eval::RenderPlan,
        fit: ViewFit,
    ) -> PaintStats;
}
```

`render` writes exactly `width * height * 4` bytes and reuses the allocation of
`out`. A plan whose size differs from the renderer's is rejected with
`SizeMismatch` before any engine work happens.

`ViewFit::contain(comp_w, comp_h, rect, window_w, window_h)` letterboxes a
composition into a window rectangle (aspect preserved, centred) and returns a
fit that draws nothing for a zero-size or non-finite input. `ViewFit::IDENTITY`
is the mapping `FrameRenderer` uses, so its pixels are unchanged.

## Coordinate contract

Composition space is pixels, origin at the top-left, `y` pointing down. Pure2D
world space has the origin at the target centre, `y` pointing up and one world
unit per pixel. The single mapping between them is the flip

```text
F(x, y) = (x - W/2, H/2 - y)
```

A shape's model is `M = F.then(item.transform)`, so the item transform is
applied first. The canonical shape quad `[-0.5, 0.5]²` is scaled by the item's
local width and height. The resulting determinant is negative (the composition
is mirrored); the engine's SDF is symmetric, so nothing needs culling.

Text local space is also `y`-down, while the engine lays text out `y`-up. Text
therefore uses `M_text = F.then(item.transform).then(S)` with `S = diag(1, -1)`.
`S` makes the text determinant positive, so glyphs stay upright.

## Colour contract

Document colours are 8-bit sRGB with straight (non-premultiplied) alpha. The
engine renders in linear light into an sRGB target, so every RGB channel goes
through the exact piecewise sRGB EOTF before submission. Alpha is already linear
coverage and is not transformed.

For every fill, stroke and text draw the instance colour is

```text
[r_linear, g_linear, b_linear, (colour.a / 255) * item.opacity]
```

The output is straight-alpha sRGB RGBA8; the engine's blending is straight
alpha, not premultiplied.

## Depth and painter order

Pure2D has no depth buffer. Its painter order is the ascending-`z` sort of the
unified command stream (ties break shapes, then sprites, then text). The adapter
counts every draw in the plan first, then assigns

```text
z = -50 + (index + 0.5) * (100 / total)
```

with `index` increasing in plan order. The window `(-50, 50)` sits well inside
the camera's usable range `(-90, 110)`, and `z` is strictly increasing, so the
engine's sort reproduces the plan order exactly and interleaves shapes and text.
Within one item the fill is submitted before its stroke.

## Text path

Roboto Regular is embedded with `include_bytes!` (relative to the source file),
so rendering never depends on the working directory. The atlas is baked once
over `ferrous_font::charset::latin_western()`. Any requested font family other
than Roboto falls back to Roboto; the family name is currently not used to
select another face.

The string is read from the item's node (`NodeContent::Text`). Alignment maps
one-to-one (`Left`, `Center`, `Right`). Missing glyphs are measured once per
text node with `measure_text` and cached on the renderer; later frames reuse the
cached count instead of re-measuring. The cache is keyed by `NodeId` and assumes
one document per renderer, which is how the CLI uses it.

## Radius and stroke approximations

Corner radii and stroke widths are authored in local pixels and converted to
world units with `world_unit_scale = sqrt(|det(M.linear)|)`. Under a uniform
scale this is exact. Under a non-uniform scale no single factor is exact, so the
geometric mean is a documented approximation.

A stroke is centred on the shape edge. The adapter grows the shape by half the
stroke width per side and draws a ring of the full stroke width inside the grown
boundary, which is exactly a centred stroke. A zero-width stroke is treated as
absent (the document asked for no visible outline). If `radius + stroke_width /
2` exceeds half the smallest grown side, the radius is clamped so the engine SDF
stays well-behaved; fills receive the same clamp to half the smallest side.

## Known engine limitations

- Blending is straight alpha only; Motioner's `Normal` blend mode is the only
  one rendered.
- Readback is not pipelined yet: every `render` call renders, then copies and
  maps the target synchronously. The engine already exposes
  `submit_readback` / `take_readback` (a two-slot ring that overlaps the copy of
  frame N with rendering of frame N+1); wiring them in is a future optimisation.
- Roboto's `.notdef` glyph is empty, so characters without a glyph are skipped
  and counted in `FrameStats::missing_glyphs` rather than drawn as boxes.
- Text is a single line; there is no multi-line layout in this path.

## First render report

Measured on the reference machine (Ryzen 5 8645HS, 8 GB RAM, Windows 11) with
the RTX 4050 Laptop GPU through Vulkan. Timings are wall clock on that machine
and are only meaningful as relative figures.

Command:

```text
cargo run --release -p motioner_app -- render examples/hello.motion --out out/hello.mp4
```

```text
Rendered 300 frames in 2.750 s (300 encoded)
  evaluate: 0.011 ms/frame
  render+readback: 5.510 ms/frame
  encode: 3.042 ms/frame
Adapter: NVIDIA GeForce RTX 4050 Laptop GPU
Missing glyphs: 0
Output: out/hello.mp4
```

The video is 1920x1080 at 60/1 fps, 300 frames (`out/hello.mp4`, 178,860 bytes).
`ffprobe -v error -select_streams v:0 -count_frames -show_entries
stream=width,height,r_frame_rate,nb_read_frames,pix_fmt -of default=nw=1
out/hello.mp4` reports:

```text
width=1920
height=1080
pix_fmt=yuv420p
r_frame_rate=60/1
nb_read_frames=300
```

The golden still is `crates/motioner_testkit/golden/hello_f120.png`
(1920x1080, 92,536 bytes, frame 120). Its SHA-256 is
`ebebab4eab0c7b3da97e1b9139aef83fa9fb88e4c8cc950f0f75edf71875fe44`;
`crates/motioner_testkit/tests/golden_hello.rs` stores it, re-renders frame 120
and compares with a maximum per-channel difference of 2 and a limit of 0.1 % of
differing pixels, skipping cleanly without a GPU.

What the checked frames show:

- **0**: empty composition background (`#0b0b12`). The title opacity is 0 and
  every circle's scale starts at 0, so nothing is visible yet.
- **60** (1 s): the title "Motioner" is upright, legible and centred at
  (960, 480), already faded in; the five pink circles are growing in on their
  row at y = 820. The two spinners are still at opacity 0.
- **120** (2 s): title, five fully scaled pink circles, and both rounded squares
  visible with their light-blue strokes; the left one is rotated about 35–45°
  and the right one about 20°, matching their staggered keyframes.
- **240** (4 s): identical layout with both spinners at their final 180°
  orientation (visually the same square pose), title and circles unchanged.
- **299** (5 s, last frame): same as frame 240; the animation has reached its
  final pose.
