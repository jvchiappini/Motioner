---
actualizado: 2026-11-07
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## Prompt 13 status (phases 1-6 done, 7-11 backlog)

The standard library is in place and the shortest MDF program can now be a
designed one. **No commit in this session** (the reviewer commits).
Remaining phases are tracked as T-DSL-0020..0024 and T-AI-0003.

### Phase 1 — Prelude (T-DSL-0014, done)

- `crates/motioner_dsl/std/{std,motion,components}.motion` embedded with
  `include_str!` and parsed before user code (`src/prelude.rs`). User
  definitions shadow; `lint` warns; `std_version()`/`query` report `1`.
- Prelude spans are shifted by `LIBRARY_OFFSET` (100M bytes, 1M lines).
  `prelude::is_library_span` marks them; `plan.rs` never edits library
  source and falls back to the user call site, `call_site.rs` refuses to
  promote parameters into `std`.
- `parse_with_prelude` is public for tools/benchmarks.
- Measured (release, RTX 4050): `showcase_pro` execution 1.224 ms baseline
  vs 1.414 ms with prelude; parse 0.513 vs 1.406 ms. Total prelude cost
  ~1.08 ms, under the 5 ms budget. The prelude is re-parsed each run (a
  cache is still possible).

### Phase 2 — Themes and tokens (T-DSL-0015, done)

- `src/theme.rs`: ten themes (dark, light, editorial, playful, corporate,
  neon, pastel, mono, warm, tech), WCAG AA on `text/bg` and `text/surface`
  when active, nearest-token suggestions.
- `theme(name)`, `use_theme(t)`, `token(name)` are scene builtins;
  `comp(..., theme=..., tempo=...)` applies both. Constructors/setters/keys
  resolve colour, size, spacing, radius, font and ease tokens. `text` gained
  `style="h1"`.
- Boards rendered at 1600x1000 in `out/std/themes/<name>.png` and copied to
  `docs/std/`.

### Phase 3 — Relative layout (T-DSL-0016, done)

- Node methods `place(anchor, x, y, margin)` and `bounds()` in
  `src/scene.rs`; builtins `row`, `column`, `grid`, `stack` measure with the
  injected `TextMetrics`. Text gets a 10 % title-safe default margin.
- Model tests assert arithmetic. Alpha-box parity against the renderer is
  backlog (T-DSL-0024).

### Phase 4 — Beats and scenes (T-DSL-0017, done)

- `comp(tempo=...)`/theme tempo; `beat(n)`, `bar(n)` (four beats).
- `scene(name, start, length)` returns a named group with opacity hold keys
  over its window; `scene.t(x)` via `NodeMethod::SceneTime`. The document has
  no keyed visibility channel, so visibility is compiled to opacity.
- Named groups show in the outline/timeline; labelled scene ranges are
  backlog.

### Phase 5 — Presets (T-DSL-0018, done)

- `std/motion.motion`: `enter`/`exit` (fade, rise, drop, slide, pop, zoom,
  blur, wipe, draw, spin), `emphasis` (pulse, shake, wiggle, glow, bounce),
  `move_to`, text `reveal`, `enter_all`/`exit_all`. Durations in beats,
  defaults from theme motion tokens.
- 15-frame GIFs per preset in `out/std/presets/<preset>.gif`, reviewed.
  `count_up` needs an animatable text channel (T-DSL-0024).

### Phase 6 — Components (T-DSL-0019, done)

- `std/components.motion`: `card`, `title_card`, `lower_third`,
  `logo_lockup`, `badge`, `button`, `bar_chart`, `progress`,
  `background(kind)` with gradient_drift, grain, glow_orbs. Named parts,
  theme tokens only. Dark board reviewed; three-theme review is backlog.

### Method dispatch

`node.enter(...)` works because `Interpreter::call` intercepts an attribute
call on a node and calls a user function of that name with the node prepended
as the first argument (`src/interpreter.rs`). Built-in node methods still go
through `NodeMethod`.

### What is next (prompt 13 phases 7-11)

- T-DSL-0020 design analysis crate (`check_design`, motion/layout/legibility
  reports, defect fixtures).
- T-DSL-0021 render tools (overlays, filmstrip, contact sheet, zoom, compare,
  variants, preview, cache).
- T-DSL-0022 media analysis (palette, salient/empty regions, cuts, tempo).
- T-AI-0003 tool runtime (D-0043 catalogue, executors, `motioner tool`).
- T-DSL-0023 std examples, design-rules/tools docs, release measurements.
- T-DSL-0024 std hardening: planner corpus cases, count_up, theme × preset
  matrix, alpha-box parity, labelled scene ranges.

## Write-through essentials (unchanged)

Strategy ladder: private literal → loop guard → call site (argument, returned
node, parameter promotion) → local assignment → fallback override. Every edit
is verified by re-running; undo stores source + overrides. `motioner_dsl`
owns planner/printer/lint (strings); writes are in `motioner_ui`/`motioner_app`.

## Project essentials

Motioner is an AI-first motion-graphics editor and NLE in Rust on the custom
FerrousEngine (wgpu), branch `feat/m1-foundations`. Program-first MDF builds
the composition; scope/decisions are REF-0001..0004 and D-0010..D-0048. Code
is written by opencode with DeepSeek V4.1 Flash; Claude is architect/auditor;
the user speaks Spanish.

- CLI: `render`, `still`, `info`, `bake`, `preview --control`, `ctl`, `new`,
  `import`, `backup`, `migrate`, `query`, `edit`, `fmt`, `lint`,
  `trace export|replay`, `corpus check`, `tools --schema`.
- Editor: projects (`<name>.motioner/`), assets, timeline, graph editor,
  effects/masks/gradients/blend modes, typography (Inter), images, video
  proxy/cache, audio mixer, autosave/backup/recovery, write-through gestures,
  overrides list, code panel, traces.
- Std CLI surface: `theme`, `use_theme`, `token`, `beat`, `bar`, `scene`,
  `comp_size`, `row`, `column`, `grid`, `stack`; node methods `place`,
  `bounds`, `t`.

## Gotchas

- **Host memory**: another job builds GUIMaker/FerrousEngine in parallel.
  Pagefile commit can be exhausted; rustc fails with `E0786` / 1455 /
  `STATUS_STACK_BUFFER_OVERRUN`. Running the gate with
  `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` avoids the largest
  link allocations. Check C: before phases; stop below 5 GB.
- PowerShell 5.1 drops unescaped `"` in native args. `Set-Content` has no
  `-NoNewline`; use `[IO.File]::WriteAllText` and normalise CRLF to LF.
- MDF has no `is` operator, no dicts, no `**kwargs`, no `while`. Use
  `== None` / `!= None`. `tracking` must be >= 0.
- `add([list])` and `add(a, b)` both work; `group()` needs `add`.
- `cargo fmt` normalises LF; run it before the gate.
- Clippy pedantic hits: `too_many_lines`, `similar_names`,
  `cast_precision_loss`, `assigning_clones`, `collapsible_match`,
  `double_must_use`, `explicit_auto_deref`, `assertions_on_constants`.
- Windows ffmpeg lacks `-pattern_type glob`; render frames with sequential
  `seq%03d.png` names or use the concat demuxer.
- GPU tests serialize through `motioner_testkit::gpu::lock()`.

## Engine

Pin `7e95367` on `feat/pure2d-text-and-readback`; dev clone `6a30e7d`
(line/polyline, in-place texture update), `e482b3c` (GuiBatch recycle),
`cab7035` (file drops). **No pin bump in this session.** Never edit
`engine/ferrous`.

## Goldens

`hello_f120.png` byte-identical (SHA-256
`C2173BADD7F23933461473C5D682E40FC9FC1DCBD4EBBE10A1C7C1C7DE5CF15C`);
`showcase_pro_f180/f330/f600/f810/f1086/f1320.png`; `demo_f090/f330.png`;
plus blend/gradients/effect/mask/path/typography/images/timing stills. The
prelude adds no nodes, so all goldens are unchanged (workspace tests green).

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.
Cargo needs `-j 1..2`; `CARGO_INCREMENTAL=0`.

## Last session

`S-2026-11-07-001` (prompt 13 phases 1-6): std prelude and library spans,
ten themes/tokens with boards, relative layout, beats/scenes, motion presets
with GIFs, components. Gate green (fmt, clippy workspace, 59 test binaries,
rustdoc -D warnings, boundaries, machete, pctl validate); no commit.
