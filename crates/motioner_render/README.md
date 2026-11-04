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

`render_frame` evaluates integer `frame` and honours
`comp(motion_blur=True, shutter=..., samples=...)`: it evaluates the centred
sub-frame times with `motioner_eval::Evaluator::evaluate_fractional` and
averages them in linear light through the engine accumulation API
(D-0038). With motion blur off the integer path is unchanged.

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
    pub fn render_frame(
        &mut self,
        document: &motioner_core::Document,
        frame: motioner_core::Frame,
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
    pub fn text_extent(
        &self,
        renderer: &ferrous_renderer::Renderer,
        text: &str,
        size: f64,
    ) -> Option<TextExtent>;
}

pub struct TextExtent { pub advance: f64, pub ascender: f64, pub descender: f64 }

pub fn srgb_to_linear(value: u8) -> f32;
pub fn linear_rgba(color: motioner_core::Color, opacity: f64) -> [f32; 4];
```

`srgb_to_linear` is the workspace's single sRGB → linear conversion; it is
defined in `motioner_core::Color` so the evaluator's colour interpolation and
this renderer share one transfer function (D-0028), and the editor theme uses
it for every UI colour. `TextExtent` (from
`LivePainter::text_extent`) gives the editor selection bounds for text layers.

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
coverage and is not transformed. Keyed colours are interpolated by the
evaluator in the same linear-light space before they reach the plan, so a
keyed fill and a stacked alpha gradient of the same stops agree (D-0028).

For every fill, stroke and text draw the instance colour is

```text
[r_linear, g_linear, b_linear, (colour.a / 255) * item.opacity]
```

The headless readback is straight-alpha sRGB RGBA8. Inside the engine every
Pure2D fragment is written **premultiplied** (the shader multiplies RGB by the
final coverage) and every pipeline uses a premultiplied blend state, including
the offscreen-layer composite. That is why additive and screen gradients add
only `alpha × colour` and leave the backdrop untouched where the ramp reaches
zero alpha.

## Depth and painter order

Pure2D has no depth buffer. Its painter order is the ascending-`z` sort of the
unified command stream (ties break shapes, then sprites, then text). The adapter
counts every draw layer in the plan first (one per fill, stroke or text batch),
then assigns

```text
z = -50 + (index + 0.5) * (100 / total)
```

with `index` increasing in plan order. The window `(-50, 50)` sits well inside
the camera's usable range `(-90, 110)`, and `z` is strictly increasing, so the
engine's sort reproduces the plan order exactly and interleaves shapes and text.
Within one item the fill is submitted before its stroke.

## Text path

D-0034 ships Inter 400/600/700 under `assets/fonts/` (OFL 1.1) and keeps Roboto
Regular embedded as the default/fallback, so rendering never depends on the
working directory. A `FontSet` holds project faces first and bundled faces
last; `FrameRenderer::with_fonts` registers one atlas per resolved
`(family, weight)` the plan uses. `font="Inter", weight=600` resolves to the
exact face, or to the nearest weight with a typed warning; a missing family is
`RenderError::FontFamily`, naming what was searched.

The string is read from the item's node (`NodeContent::Text`). Alignment maps
one-to-one (`Left`, `Center`, `Right`). `tracking`, `line_height` and `\n`
multi-line layout go through the engine's `draw_text_2d_styled`, and
character animators through `layout_text_2d` + `draw_glyphs_2d`. Single-line
Roboto with no tracking renders exactly as before.

`CpuTextMetrics` implements `motioner_core::TextMetrics` from the same
`ferrous_font` parser and is what `text_width`/`text_bounds` use. Missing
glyphs are measured once per text node with `measure_text` and cached on the
renderer; later frames reuse the cached count. The cache is keyed by `NodeId`
and assumes one document per renderer, which is how the CLI uses it.

## Images

An image item (`ItemShape::Image`) carries the content key, the destination
rectangle and the `uv_rect` crop. `FrameRenderer::set_images` takes the decoded
bytes from `motioner_io`; `paint_plan` uploads each key once through
`register_texture_2d` (mipmapped, trilinear, premultiplied) and reuses the
texture id by key across frames. `free_sprite_texture` (or a new image set)
releases it. `texture_bytes_2d()` is the resident budget and a frame above
`FrameRenderer::texture_budget` fails with `RenderError::TextureBudget`.

An unresolved image (missing/invalid path) evaluates to a placeholder item:
the preview draws a checker plus an outline; `still`/`render` check
`Document::first_unresolved_image` first and return
`RenderError::UnresolvedImage { source }` with the path. Image textures need
the engine's `register_texture_2d` (mipmapped, trilinear). `CpuTextMetrics`
measures through the engine's `ferrous_font::LayoutData`, so `text_width`
matches what is drawn, GPOS kerning included.

The engine contract (colour space, mip filter, sampler choice, `uv_rect`,
budget) is documented in `engine/ferrous/crates/ferrous_2d/docs/images.md`.
## Vector paths

A path item (`ItemShape::Path`) references a `ResolvedPath` in the plan's
arena: the authored `PathGeometry` plus the frame's trim, dash and paint.
`motioner_core::path` flattens cubics adaptively and cuts the flattened
polyline into trimmed and dashed pieces; `lyon_tessellation` turns those
pieces into triangles (fill and stroke with the authored cap/join). Each
triangle becomes a `ShapeInstance::polygon_triangle`.

The tessellation is cached per node. The key hashes the resolved geometry,
trim, dash, stroke width and cap/join, and the cache stores local-space
triangles, so only the model transform runs per frame. Edges shared by two
triangles are marked hard and only boundary edges get anti-aliasing: without
that, every internal edge would double-blend and show as a seam.

`path_bounds` returns the local box of a resolved path expanded by half the
stroke, which the editor uses for selection and hit testing. Hit testing
also asks the geometry whether the pointer is inside the fill (even-odd) or
within half the stroke of the outline; it uses the untrimmed geometry so a
draw-on line stays pickable.

`lyon_tessellation` is the only geometry dependency (D-0029); it is already
in the engine's build graph and adds no new crate to the lockfile.

## Gradients

`ItemShape::Rect`, `Circle`, `Path` and `Text` carry a `ResolvedPaint` whose
fill/stroke are `ResolvedFill::Solid(Color)` or `ResolvedFill::Gradient(index)`
into the plan's gradient arena. The evaluator samples each stop's offset and
colour, clamps the offsets to `0..1` and sorts them (D-0030).

`gradients::GradientTable` turns the arena into the engine's flat vec4 table
(headers, world-space geometry and one colour/offset pair per stop). The
transform of the item that owns a gradient maps its local geometry into
composition space, so a ramp follows its node. A radial gradient transforms
the centre and the local `+x` edge point separately and takes the length of
that world difference, so its radius is `radius * scale` and its alpha reaches
zero exactly at the node's silhouette. (The old code transformed the
already-transformed centre again, which folded the node translation into the
radius.) Under a non-uniform scale the ramp uses the x-axis scale because the
engine pass draws a circle. `tests/render.rs` pins the fade for all seven
blend modes with an off-centre circle.

The table is uploaded on every frame and both shape and text instances
reference an entry (`with_gradient`), so gradients are unconditional since the
pin that carries the Pure2D gradient pass (D-0030). Nothing silently falls
back to a solid colour except a gradient that does not fit the engine's
512-slot table.

Text gradients and the backdrop-aware blend modes are unconditional since the
pin that carries the Pure2D gradient and composite passes. There is no
`Unsupported` path left for them; the only fallback is a gradient that does
not fit the engine's 512-slot table, which uses its solid first stop.

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

## Known limitations

- `Multiply` omits the `Cs × (1 − Ad)` term over a transparent backdrop (the
  engine's fixed-function contract, D-0031), so a semi-transparent multiply is
  darker than a reference compositor. `Overlay`, `darken` and `lighten` need a
  readable destination and fall back to `normal` at a root target that cannot
  be snapshotted.
- A translucent **GUI** polyline (not a Motioner shape) composites the round
  caps at each shared endpoint twice; the engine documents the coverage and
  pins the geometry in `GuiBatch::polyline`.
- Readback is not pipelined yet: every `render` call renders, then copies and
  maps the target synchronously. The engine already exposes
  `submit_readback` / `take_readback` (a two-slot ring that overlaps the copy of
  frame N with rendering of frame N+1); wiring them in is a future optimisation.
- Roboto's `.notdef` glyph is empty, so characters without a glyph are skipped
  and counted in `FrameStats::missing_glyphs` rather than drawn as boxes.

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
(1920x1080, frame 120). Its SHA-256 is
`c2173badd7f23933461473c5d682e40fc9fc1dcbd4ebbe10a1c7c1c7de5cf15c`;
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

### Phase 12 release numbers

Measured once on the reference machine from the release binary
(`cargo build --release -p motioner_app`, 5m56s):

```text
examples/showcase.motion      Rendered 720 frames in 8.079 s
                              render+readback: 7.778 ms/frame
                              encode:          3.065 ms/frame

examples/showcase_pro.motion  Rendered 1440 frames in 26.621 s
                              render+readback: 16.499 ms/frame
                              encode:          1.763 ms/frame
```

`showcase_pro.motion` enables `motion_blur=True` with 8 samples, so its
16.5 ms/frame is the average of eight sub-frame renders per output frame. The
editor preview at 1600x900 plays the same composition at **59 UI fps**
(evaluation 0.03 ms, **0 chrome allocations/frame** with `alloc-count`), with
the canvas at a 39 % fit. The numbers are also in the Phase 12 report under
`out/reports/` and in `.control/CONTEXT.md`.

## Layers, blend modes and effects

A plan item with a non-normal blend mode, or a node with an effect chain,
renders inside an engine offscreen layer. `paint_plan` opens the layers in
`RenderPlan::layers()` before their first item and closes them after their
last one, so a group's blend and its effects apply to the whole subtree, and
nested layers nest correctly. The layer composites with the node's
`CompositeMode2d` (D-0031); the engine's backdrop-aware modes copy the
destination into a scratch texture first.

Effects (`Effect2d`: blur, drop shadow, glow; D-0032) are expressed on the
same layer: `paint_plan` maps each sampled `motioner_eval::EffectValue` to
the engine's `Effect2d` (straight linear colour; radii and offsets scaled from
composition pixels by the view) and opens the layer with
`push_layer_2d_with_effects`. A layer the engine rejects (invalid parameters)
is not opened, so its draws composite directly and it is never popped. See
the engine's `crates/ferrous_2d/docs/effects.md` for the passes.

Masks (D-0033) use the same layer: a masked node's `PlanLayer` also carries a
`PlanMask` with the item range of the mask source's subtree. `paint_plan`
opens the layer with `push_masked_layer`, draws the content, calls `begin_mask`
when the source range starts, draws the source items (including any layers and
effects they have, which is how a blurred mask feathers), calls `end_mask`,
and pops. The engine multiplies the finished, effected texture by the source's
coverage. See the engine's `crates/ferrous_2d/docs/masks.md`.
`golden_masks.rs` pins two stills and compares the render against them.

