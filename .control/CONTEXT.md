---
actualizado: 2026-11-02
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0032.

## Working model

- Code is usually written by opencode with DeepSeek V4.1 Flash; Claude is
  architect and auditor. Conversation with the user is in Spanish.
- Prompts live in `.control/prompts/{opencode,claude}/`. Work only inside
  Motioner, FerrousEngine and GUIMaker.
- The user wants LONG multi-phase prompts and a visible result (PNG/MP4).
- Prompt 10 is a 12-phase job. **Phases 1-5 are done.** Every later phase has
  its backlog task (T-RENDER-0011..0013, T-CORE-0002, T-UI-0006/0007,
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
  verify. Verification compares content, transform, **blend and effects**.
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
  non-normal blend **or effects**, wrapping its whole drawn subtree; the
  renderer opens/closes engine layers by item range. All seven render on the
  pin; the temporary `engine-composite` feature is deleted.
- **Layer effects (D-0032)**: ordered stack `drop_shadow` / `blur` / `glow`,
  one per kind, applied to the node and its subtree as one offscreen layer.
  Parameters are animatable channels (`drop_shadow.blur`, `glow.radius`,
  `drop_shadow.offset.x`); the stack itself is the `effects` override
  (comma-separated kinds) and `effect_add`/`effect_remove`/`effect_move`
  commands. DSL: `node.effect(kind, **params)` (add/update, idempotent) and
  `node.effect_stack("a,b")` (bulk rebuild, used by bake). Bake folds params
  as `var.effect("blur", radius=6)` / `var.key("blur.radius", t, v)` and the
  stack as `var.effect_stack("glow,blur")`.
  Rendered since pin 7a9b7fd through `push_layer_2d_with_effects`.
- Keyframes per axis; animated vectors are always keyed per axis (`pos.x`).
- In-app screenshots: `preview --screenshot <png>` and `ctl screenshot(...)`;
  `scripts/ui_shot.ps1 [-Commands ...] [-Select ROW]`.
- **GPU tests**: `motioner_testkit::gpu::lock()` serializes them; the normal
  `cargo test --workspace` gate is green without `--test-threads=1`.

## Review fixes from Phase 4 (done at Phase 5 start)

- `engine-text-gradient` and `engine-composite` are **deleted** from
  `motioner_render` (`default = []`; only `live` remains). Text gradients and
  all seven composite modes are unconditional.
- `golden_blend.rs` and `golden_gradients.rs` never skip: any render error,
  `Unsupported` included, fails the test.
- Engine clone mojibake repaired in `ferrous_2d/render/renderer.rs` and
  `ferrous_engine/runner/events.rs`; comments translated to English in
  `ShapeInstance` (no project names). All 13 rustc warnings in
  `ferrous_engine` are gone.
- UTF-8 discipline: read/write with Python `encoding='utf-8'` (or the editor
  tools). The console is cp850 and shows correct UTF-8 as mojibake; verify
  with `open(..., encoding='utf-8')`, not with PowerShell `Select-String`.

## Goldens

- `hello_f120.png` (byte-identical), `blend_f30.png`,
  `gradients_f30/f105.png`, plus three Phase 5 stills:
  `effects_shadow_f0.png`, `effects_blur_f30.png`, `effects_glow_f60.png`.
- `examples/effects.motion` shows one effect per beat (shadow at frame 0,
  blur at 30, glow at 60) by keying `drop_shadow.opacity`, `blur.radius` and
  `glow.intensity`; the effect goldens assert each isolation.
- `golden_effects.rs` has the pixel assertions on the stills and the render
  comparison of the three frames (runs on every test run).

## Gotchas

- **PowerShell**: double-quoted strings expand `` `n `` but keep `\n` literal;
  single-quoted keep both. Native args drop unescaped `"`, which breaks
  `ctl select("@288")`; use `--select ROW` instead (ROW is the outline index:
  roots in reverse order, row 0 is the bottom-most node).
- `scripts/ui_shot.ps1` escapes quotes but the outer PowerShell invocation may
  still strip them; prefer `-Select`.
- 8 GB RAM: cargo needs `-j 2`; `CARGO_INCREMENTAL=0`.
- `core.autocrlf=true`: working copies CRLF; engine files edited with Python
  `newline=''` keep their EOL; convert the flagged ones back to CRLF.
- Disk: keep C: above 5 GB; clean `target/debug/incremental` below 6 GB.
- Clippy pedantic: `option_option`, `float_cmp`, `similar_names`,
  `implicit_clone`, `items_after_statements`, `doc_markdown`,
  `too_many_arguments` are denied.
- The inspector starts with **Composite/Blend**, then **Effects** (one ops row
  per effect with Up/Dn/Del, parameter rows with stopwatches, and an Add
  effect row with Shadow/Blur/Glow tiles), then Transform and content.
- Engine clippy `-D warnings` still fails on pre-existing lints in
  `ferrous_engine` (27) — use `--no-deps` on the crates touched.

## FerrousEngine state

- Pin `e9fb90d` (gradients + text gradients + blend composites), pushed.
- **Uncommitted in the clone** (16 paths):
  - `crates/ferrous_2d/src/render/effects.rs`: `Effect2d`, `BlurKernel2d`,
    `gaussian_kernel`, `BlurUniform2d`/`EffectUniform2d`, `MAX_BLUR_TAPS=48`,
    `MAX_BLUR_RADIUS=48`, `MAX_EFFECTS_PER_LAYER=8`, kernel unit tests.
  - `pipeline.rs`: `BlurEffectPipeline`, `EffectCompositePipeline`,
    `uniform_entry`.
  - `assets/shaders/effect_blur_2d.wgsl`, `effect_composite_2d.wgsl`.
  - `renderer_core.rs`: `LayerRecord.effects`, `push_layer_2d_with_effects`,
    `LayerError::{InvalidEffect, TooManyEffects}`, effect pipelines and two
    scratch textures per layer, ping-pong chain, per-pass uniform pool, and a
    **bug fix**: the layered path is chosen by `frame_uses_layers`, not by the
    pooled `layers_2d` (otherwise the first frame after one that used layers
    renders only the clear).
  - `headless.rs`: `push_layer_with_effects`.
  - `types.rs`: `LayerError` variants, `ShapeInstance` docs in English.
  - `ferrous_engine/src/renderer.rs`, `runner/events.rs`: warnings cleaned.
  - Docs: `crates/ferrous_2d/docs/effects.md`, `README.md`, `mkdocs.yml`.
  - Tests/example: `tests/pure2d_effects.rs` (6 tests: identity, blur spread,
    shadow, glow, group opacity, typed errors),
    `examples/pure2d_effects.rs` (writes `brain/pure2d_effects.png`).
  - Clippy (`ferrous_2d`, `ferrous_renderer`, `--all-targets --no-deps -D
    warnings`) and all their tests are green.
- Golden `hello_f120.png` SHA `c2173bad...` (still byte-identical).

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.

## Editor UI

- Chrome is an immediate painter on the engine `GuiBatch`; tokens in
  `theme.rs` are the only colour literals.
- One key-marker language (`widgets::key_marker`).
- The inspector order is Composite → Effects → Transform → content → Layer.
- Gradient stops add one row pair each ("Stop N offset"/"Stop N colour").

## Last relevant session

Phase 5 (effects) complete up to the pin: review fixes, engine effect API and
passes in the clone, Motioner model/DSL/eval/commands/inspector/query/bake,
three effect goldens, typed `Unsupported`, captures `out/ui/p5-effects-*`.
Engine committed as 9aaa2b6 + e2b14a9 + 7a9b7fd (shadow behind the content, CSS-strength blur sigma = radius/2, outer glow) and pinned; the reviewer wired
`push_layer_2d_with_effects` in `motioner_render`, removed the precheck and the
ignored test, and the three effect stills now render and match. Next: Phase 6
(masks and track mattes).
