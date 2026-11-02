---
actualizado: 2026-11-02
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0033.

## Working model

- Code is usually written by opencode with DeepSeek V4.1 Flash; Claude is
  architect and auditor. Conversation with the user is in Spanish.
- Prompts live in `.control/prompts/{opencode,claude}/`. Work only inside
  Motioner, FerrousEngine and GUIMaker.
- The user wants LONG multi-phase prompts and a visible result (PNG/MP4).
- Prompt 10 is a 12-phase job. **Phases 1-6 are done.** Every later phase has
  its backlog task (T-RENDER-0012/0013, T-CORE-0002, T-UI-0006/0007,
  T-APP-0003, T-PERF-0002) with its design in the body.
- **FerrousEngine is a general-purpose engine, not a Motioner backend.** A
  missing render capability is built in the clone as a public engine feature
  with rustdoc, `crates/<crate>/docs/`, an `mkdocs.yml` entry and engine
  tests; Motioner only consumes it. The user commits the clone and bumps the
  pin.

## Current state (branch feat/m1-foundations)

- `motioner render|still|info|bake|preview|query|edit|ctl`.
- The editor EDITS through overrides (D-0022); the program only changes with
  `bake`.
- **Agent control (D-0024)**: one `Command` enum, MDF call syntax,
  `motioner_ui::command`, executed only by the window-free `Editor`.
  `motioner query` (JSON scene), `motioner edit` (all-or-nothing,
  `--dry-run`), `preview --control` + `motioner ctl`. Docs: `docs/agents.md`.
- **Canvas view (D-0026)**: `ViewState.zoom/pan/fit_zoom`;
  `viewport::Viewport` is the one window → composition mapping.
- **Orphans (D-0025)**: `Editor::orphan_suggestions()` ranks live nodes.
- **Hybrid bake (D-0027)**: value origins + in-place literal edits + re-key +
  verify. Verification compares content, transform, **blend, effects and masks**.
- **Professional motion model (D-0028)**: content fields are animatable
  channels (`Animatable<f64|Color|Option<Color>>`), colour interpolation is
  linear-light sRGB. `NodeContent::channel(field)` is the one field table.
- **Vector paths (D-0029)**: `path`, `polygon`, `star`, `arc`, animatable
  trim/dash, lyon tessellation with a per-node cache.
- **Gradients (D-0030)**: `linear_gradient`/`radial_gradient` for fill, stroke
  and text; 2..16 stops with animatable offset/colour; linear-light
  interpolation; plan gradient arena; per-frame engine table. Gradients and
  text gradients are **unconditional** (features deleted after pin e9fb90d).
- **Blend modes (D-0031)**: seven modes on every node (discrete property,
  overridable, not keyable). Evaluate emits a `PlanLayer` for a node with a
  non-normal blend, effects **or a mask**, wrapping its whole drawn subtree.
- **Layer effects (D-0032)**: ordered stack `drop_shadow` / `blur` / `glow`,
  one per kind, applied to the node and its subtree as one offscreen layer.
  Params are animatable channels (`blur.radius`, `drop_shadow.offset.x`); the
  stack is the `effects` override. Rendered since pin 7a9b7fd through
  `push_layer_2d_with_effects` (sigma = radius/2, shadow behind the content,
  outer glow).
- **Masks (D-0033)**: `node.mask(source, mode="alpha"|"luma", invert=False)`,
  `node.mask(None)` clears. The source is hidden from the normal stream and
  drawn in world space as the coverage of the masked node's layer (a child of
  the masked node moves with it). Order: content → effects → mask → opacity/
  blend, so a shadow is clipped and a blur on the source feathers. The editor
  stores the `mask` override (`"<source>:<alpha|luma>:<true|false>"`), with
  `mask_set`/`mask_clear`, an inspector Mask section (source picker, mode,
  invert, clear) and layers badges. Masks render in stills and the preview
  since pin e8b9ba7.
- Keyframes per axis; animated vectors are always keyed per axis (`pos.x`).
- In-app screenshots: `preview --screenshot <png>` and `ctl screenshot(...)`;
  `scripts/ui_shot.ps1 [-Commands ...] [-Select ROW]`.
- **GPU tests**: `motioner_testkit::gpu::lock()` serializes them; the normal
  `cargo test --workspace` gate is green without `--test-threads=1`.

## Goldens

- `hello_f120.png` (byte-identical), `blend_f30.png`,
  `gradients_f30/f105.png`, three effect stills (`effects_shadow_f0`,
  `effects_blur_f30`, `effects_glow_f60`) and two mask stills:
  `masks_alpha_reveal_f36.png` (a white bar wipes a headline plate in) and
  `masks_luma_invert_f150.png` (a grey disc is a translucent window over a
  gradient card). `golden_masks.rs` validates the pixels and compares the
  render against both stills.
- `examples/masks.motion` has both beats and explicit ids (`headline`, `wipe`,
  `card`, `spot`).

## Gotchas

- **PowerShell**: double-quoted strings and here-strings expand backticks and
  `$`, so never pass Python code containing `` ` ``, `\"` or `${...}` through
  `python -c @"..."@`; write a `.py` script with the editor tool instead.
  Native args drop unescaped `"`, which breaks `ctl select("@288")`; use
  `--select ROW` instead (ROW is the outline index: roots in reverse order,
  row 0 is the bottom-most node).
- `scripts/ui_shot.ps1` escapes quotes but the outer PowerShell invocation may
  still strip them; prefer `-Select`.
- 8 GB RAM: cargo needs `-j 2`; `CARGO_INCREMENTAL=0`.
- `core.autocrlf=true`: working copies CRLF; engine files edited with Python
  `newline=''` keep their EOL.
- Disk: keep C: above 5 GB; clean `target/debug/incremental` below 6 GB.
- Clippy pedantic: `too_many_lines` is easy to hit (add an `#[allow]` with a
  reason when a function is one linear pass), plus `option_option`,
  `float_cmp`, `similar_names`, `implicit_clone`, `items_after_statements`,
  `doc_markdown`, `too_many_arguments`, `struct_excessive_bools`.
- The inspector order is Composite (Blend grid) → Effects → **Mask** (source
  picker, Alpha/Luma, Off/On/Clear) → Transform → content.
- Engine clippy `-D warnings` still fails on pre-existing lints in
  `ferrous_engine` — use `--no-deps` on the crates touched.

## FerrousEngine state

- Pin `e8b9ba7` (masks; before it `7a9b7fd`, effects: shadow behind the
  content, sigma = radius/2, outer glow). Committed and pushed (14 paths):
  - `crates/ferrous_2d/src/render/types.rs`: `MaskMode2d`, `Mask2d`,
    `MaskUniform2d`, four mask `LayerError` variants.
  - `crates/ferrous_2d/src/render/pipeline.rs`: `MaskPipeline`.
  - `assets/shaders/mask_2d.wgsl`.
  - `crates/ferrous_renderer/src/renderer_core.rs`: `LayerRecord.role/mask/
    mask_source`, `push_layer_2d_masked`, `begin_mask_2d`, `end_mask_2d`,
    `pop_layer_2d` misuse handling, `plan_layer_stream` (source planned and
    ordered before the masked layer, never composited), `run_layer_mask`
    (mask pass into the effect scratch, then copy back), `ensure_effect_targets`
    for masked layers, unbalanced-mask cleanup.
  - `crates/ferrous_renderer/src/headless.rs`: `push_masked_layer`,
    `begin_mask`, `end_mask`.
  - `crates/ferrous_2d/docs/masks.md`, `docs/README.md`, `mkdocs.yml`.
  - `crates/ferrous_renderer/tests/pure2d_masks.rs` (8 tests: alpha 0.5, luma
    grey, inverted alpha and luma, mask+opacity+multiply, mask clips a shadow,
    feathered source, nested masks, typed misuse).
  - Project-name cleanup: `cad_capture.rs`, `render_config.rs`,
    `graphics.rs`, `render_context.rs`, `docs/readback.md`; the unused
    `assert_send` test helper now asserts `RenderContext<'_>`.
  - Green: clippy `ferrous_2d`/`ferrous_renderer` `--all-targets --no-deps -D
    warnings`, their tests, `cargo check -p ferrous_engine --all-targets` (0
    warnings), diff free of mojibake.
- The pin's `engine/ferrous/.../ferrous_ui_core/src/text_field_state.rs` still
  has a pre-existing mojibake line (pin content, not touched).

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.

## Editor UI

- Chrome is an immediate painter on the engine `GuiBatch`; tokens in
  `theme.rs` are the only colour literals.
- One key-marker language (`widgets::key_marker`).
- The inspector order is Composite → Effects → Mask → Transform → content.
- Mask rows: Source cycles through `Document::can_mask` candidates, Mode is
  Alpha/Luma, Invert is Off/On/Clear (Clear removes the mask).
- Layers rows draw a mask badge for the masked node and its source
  (`OutlineRow.mask_role`).

## Last relevant session

Phase 6 (masks) landed: engine mask pass and 8 tests (FerrousEngine e8b9ba7,
the pin), Motioner core/eval/DSL/render/editor/bake/query and two goldens.
The reviewer removed the `engine-masks` gap with the bump. Next: Phase 7
(typography).
