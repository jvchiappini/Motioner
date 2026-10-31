# Implementation prompt: Motioner Milestone 1

You are the implementation agent for the Motioner repository at
`C:\Users\jvchi\CARPETAS\Motioner`. Your mission is to deliver the first
visible vertical slice: an MDF program that renders a short 1080p MP4
through FerrousEngine.

## Read first (mandatory, in this order)

1. `.control/SYSTEM.md`
2. `.control/CONTEXT.md`
3. `.control/PROJECT.md`, `.control/GOALS.md`, `.control/ROADMAP.md`
4. `.control/docs/reference/REF-0001.md` (system blueprint)
5. `.control/docs/reference/REF-0002.md` (engineering conventions)
6. `.control/docs/reference/REF-0003.md` (product scope)
7. `.control/docs/reference/REF-0004.md` and `spec/mdf-v0.md`
8. Decisions `D-0010` to `D-0017`
9. `README.md` and the current Cargo workspace

Repository: https://github.com/jvchiappini/Motioner
Engine: https://github.com/jvchiappini/FerrousEngine
UI toolkit (not needed for this milestone): https://github.com/jvchiappini/GUIMaker

The user speaks Spanish; all code, comments, tests, docs and commit
messages are in English.

## Non-negotiables

- Rust, edition 2021, toolchain 1.96.0.
- Apache-2.0.
- Program-first MDF (D-0011): the program is the source; executing it
  builds the composition.
- Deterministic: integer frames, rational fps, no wall-clock reads.
- Every mutation goes through typed commands or the document API; do not
  add a second truth.
- Engine types stay inside `motioner_render`. No engine types in core,
  eval, DSL or app.
- No duplicated logic; extract well-named helpers and document public
  APIs with rustdoc.
- CI has no GPU: CPU tests must pass; rendering tests are manual on the
  reference machine or marked ignored.
- Do not add GUI, NLE, audio, 3D, AI, LaTeX, node graph or overrides in
  this milestone.

## Environment and disk constraint

- Windows 11, Rust 1.96.0, about 18 GB free on C:.
- The engine is public and compiles on the user's machine.
- GUIMaker is not needed yet; do not clone it.
- Avoid downloading large assets. The engine contains a 69 MB 4K EXR
  skybox that is not needed for 2D.
- Monitor free space. If it drops below 8 GB, stop and report to the
  user before continuing.
- Use release builds for frame rendering and keep `target/` bounded.
  Document the exact build command in the final report.

## Goal

Produce a 5-second, 1920x1080, 60 fps MP4 from an MDF program using a
headless FerrousEngine render path and ffmpeg. The video must contain
animated 2D text and shapes and be deterministic.

## Deliverables

1. `motioner_dsl` crate: lexer, recursive-descent parser and AST for the
   frozen v0 subset.
2. Interpreter and 2D scene API producing a document.
3. Minimal document model and deterministic evaluator for 2D.
4. `motioner_render` adapter using FerrousEngine offscreen rendering.
5. CLI in `motioner_app`:
   `cargo run --release -p motioner_app -- render examples/hello.motion --out out/hello.mp4`
6. `examples/hello.motion` and `out/hello.mp4` (the output folder may be
   gitignored; include a generated sample or instructions).
7. Golden frame PNG and a short timing/determinism report.
8. Updated `.control` state: tasks, architecture docs for `dsl` and
   `render`, session log, CONTEXT and ADRs for engine integration.

## MDF v0 subset for this milestone

Implement exactly this subset; defer the rest to v0.1.

- Tokens: comments, identifiers, integers, floats, strings, booleans,
  `None`, lists, operators, indentation.
- Statements: assignment (`=`, `+=`, `-=`, `*=`, `/=`), expression
  statements, property assignment, `def`, `return`, `if`/`elif`/`else`,
  `for ... in`, `break`, `continue`, `pass`.
- Expressions: arithmetic, comparisons, `and`/`or`/`not`, calls,
  attribute access, indexing, lists.
- Built-ins: `range`, `len`, `min`, `max`, `abs`, `round`, `str`, `int`,
  `float`, `sin`, `cos`, `lerp`, `clamp`.
- Constructors: `comp`, `group`, `rect`, `circle`, `text`.
- Node properties: `pos`, `scale`, `rotation`, `anchor`, `opacity`,
  `blend`.
- Animation: `node.key(property, time_seconds, value, ease="linear")`.
  Support linear and at least `quad_out`, `cubic_out`, `back_out`.
- Colors: `#rrggbb` and `#rrggbbaa` strings.

Not in this milestone: `while`, `use`, dicts if not needed, overrides,
LaTeX, images, video, audio, 3D, effects, masks, precomps.

## Required example

Create `examples/hello.motion` close to this content:

```
# mdf: 0.1

comp(name="Hello", width=1920, height=1080, fps=60, duration=5.0,
     background="#0b0b12")

title = text("Motioner", size=160, color="#ffffff")
title.pos = (960, 480)
title.opacity = 0.0
title.key("opacity", 0.0, 0.0)
title.key("opacity", 0.8, 1.0, ease="quad_out")

for i in range(5):
    dot = circle(radius=24, fill="#ff3366")
    dot.pos = (480 + i * 240, 820)
    dot.key("scale", 0.2 + i * 0.1, 0.0)
    dot.key("scale", 1.2 + i * 0.1, 1.0, ease="back_out")
```

## Implementation order

1. **Space and engine setup.** Check free space. Add FerrousEngine as a
   pinned dependency. Prefer a git submodule at `engine/ferrous` with a
   shallow clone and, if practical, a sparse checkout that excludes
   `assets/skybox` and unused large assets. Record the exact commit in
   `engine/ferrous/COMMIT` or the repository README. If the engine does
   not expose a headless offscreen path, investigate before writing a
   custom renderer and report the finding.
2. **DSL crate.** Add `crates/motioner_dsl`, update the workspace and
   `scripts/check_boundaries.py`. Implement lexer, AST, parser and
   diagnostics with spans. Add unit and fuzz-style tests.
3. **Interpreter.** Values, scopes, functions, control flow, built-ins
   and host constructors. Enforce limits: statements, call depth, nodes,
   time and memory. No I/O or ambient time.
4. **Document and evaluator.** Minimal composition, node tree, transform,
   shape/text content, animatables and keyframes. Evaluate a frame into
   a flat list of 2D render items.
5. **Render adapter.** Convert render items to FerrousEngine draw calls,
   render offscreen at the requested size, read back RGBA8, and hand
   frames to ffmpeg. Keep the adapter as the only crate importing engine
   types. Reuse engine buffers and caches; avoid per-frame allocations
   beyond documented scratch buffers.
6. **CLI and export.** Parse arguments, load the MDF program, evaluate
   frames, pipe raw RGBA to ffmpeg with H.264, yuv420p, faststart and
   CRF 18. Support `--still <frame> --png <path>` for the golden frame.
   Fail with clear errors if ffmpeg is missing.
7. **Verification.** Render the example. Check 300 frames, 1920x1080,
   60 fps. Render frame 120 twice and compare hashes. Record wall time
   and peak memory. Produce the golden PNG.
8. **PCS closeout.** Update tasks, architecture docs, CONTEXT, decisions
   for engine integration, and session log. Run `pctl reindex`,
   `validate`, `doctor` and `cargo fmt/clippy/test/doc/machete` plus the
   boundary script.

## Engine integration rules

- Pin one commit; never float on `main`.
- Treat engine modifications as small, documented patches. If the engine
  needs a new public headless API, add it upstream-style and record the
  patch in a decision record.
- If the 2D path is insufficient, use the engine device and a minimal
  adapter-local renderer only after reporting the limitation and getting
  approval. Do not fork half the engine into Motioner.
- No engine types in serialized documents or public Motioner APIs.
- Select the high-performance NVIDIA adapter on hybrid laptops.

## Verification commands

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo machete
python scripts/check_boundaries.py
cargo run --release -p motioner_app -- render examples/hello.motion --out out/hello.mp4
```

Manual GPU verification is required on the reference machine; CI runs
only the CPU test subset.

## Acceptance criteria

- [ ] The example MP4 plays and has 300 frames at 1920x1080 and 60 fps.
- [ ] Animated text and shapes match the program.
- [ ] Frame 120 rendered twice produces the same hash or a documented
      tolerance.
- [ ] No engine type leaks outside `motioner_render`.
- [ ] No per-frame heap allocation beyond documented buffers.
- [ ] CPU tests, clippy, fmt, machete and boundary checks pass.
- [ ] A short report records the engine commit, commands, timings, peak
      memory, golden hash and known limitations.

## Git and repository rules

- Small conventional commits in English.
- Do not force-push.
- Do not touch `Motioner-legacy`.
- Do not commit `target/`, `out/`, caches or secrets.
- Do not push unless the user asks.
- Keep `origin` on `jvchiappini/Motioner`.

## Report format

Reply with:

1. What was built and the exact commands to reproduce it.
2. Engine commit and any patches.
3. Output path, frame count, resolution, fps and encode settings.
4. Determinism result and golden hash.
5. Timings and peak memory on the reference machine.
6. Deviations, limitations and the next recommended milestone.

If something blocks the milestone, stop and report the blocker with
evidence instead of expanding scope.
