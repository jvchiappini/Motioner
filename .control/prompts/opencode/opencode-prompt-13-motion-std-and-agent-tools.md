# opencode prompt 13 — Motioner: motion standard library and the agent tool runtime (themes, layout, beats, presets, components, design analysis, render and media tools)

You are the orchestrator of a very long, multi-phase job in the Motioner
repository at `C:\Users\jvchi\CARPETAS\Motioner` (branch
`feat/m1-foundations`). Run this prompt **after**
`opencode-prompt-12-code-write-through.md`. Engine work, if any (overlay
drawing helpers, readback of regions), goes to the dev clone at
`C:\Users\jvchi\CARPETAS\FerrousEngine` as **generic** features documented in
the engine docs; the reviewer bumps the pin.

**Contracts for this prompt — read them first:**
- `D-0039` motion standard library (themes, relative layout, beats, scenes,
  presets, components).
- `D-0040` §5 quality rubric (the rules `check_design` implements) and §6
  (what vision tools must make possible).
- `D-0041` harness and `D-0043` tool catalogue (every tool here follows its
  signatures, result rules, cost classes and image budget).
- `D-0036` write-through (the `std` library is read-only for the planner) and
  `D-0024` command layer.

Why this matters: Motioner will train a small model that writes MDF, and an
in-app agent (prompt 14) will drive the editor with real LLM providers. Both
need (1) a language where the shortest program is a beautiful one, and (2)
exact tools to see and measure what a program produces. **No LLM is called in
this prompt**; everything here is deterministic and testable offline. At the
end, any agent (including Claude Code from a terminal) can already use the
tools through `motioner tool ...`.

Quality bar: top-tier studio. Zero duplication, rustdoc on public items,
typed errors, no panics on runtime paths, regression tests that fail without
the change, no warnings. Work phase by phase; run the gate after each phase.
If the job must be split across runs, stop cleanly at a phase boundary and
report.

---

## 0. Ground rules (every phase, every subagent)

- Work only inside the repository (and the engine dev clone for generic
  engine features). Never edit `engine\ferrous`. Do not commit, stage, stash,
  reset, checkout, rebase or push. The reviewer commits.
- `CARGO_INCREMENTAL=0`, `-j 2` (8 GB RAM). Release builds only to measure
  performance in the last phase. Stop if free space on C: drops below 5 GB.
- `cargo fmt` (never `--all`), `cargo machete crates`. Clippy pedantic
  `-D warnings`; every `#[allow]` has a reason. `unsafe_code` stays denied.
- **New dependencies or crates need an ADR first** (`pctl decision-new`).
  Expected: new crates `motioner_analysis` (design and media analysis,
  engine-free) and `motioner_tools` (catalogue + executors); prefer writing
  small algorithms (k-means, onset detection, autocorrelation) over pulling
  crates. `serde_json` for tool I/O needs a short ADR (it is already in the
  lock file through the engine).
- **Document format** stays MDF. `std` compiles to ordinary nodes and keys:
  `query`, overrides, bake and write-through work unchanged.
- **Determinism.** Same program → same document, same analysis, same images.
  No clocks, no unseeded randomness, stable ordering everywhere. Existing
  goldens stay byte-identical.
- **Boundaries** (`scripts/check_boundaries.py`, update it for the new
  crates): `motioner_analysis` is engine-free and file-system-free (takes
  documents, evaluated frames, decoded samples); pixel work (overlays,
  crops, background sampling for contrast) lives in `motioner_render`; files
  and ffmpeg in `motioner_io`; `motioner_tools` orchestrates.
- Shell gotchas: Bash heredocs containing `'''` break (write files with the
  editor tool); PowerShell 5.1 drops unescaped `"` in native arguments;
  `core.autocrlf=true`.
- **Look at what you make.** Every theme, preset and component is rendered
  and inspected (`motioner still`, filmstrips, GIFs); fix the worst visible
  problem first. Design quality is the deliverable, not just passing tests.

Phase gate: `cargo fmt -- --check`,
`cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
`cargo test --workspace -j 2 --no-fail-fast`,
`RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
`python scripts/check_boundaries.py`, `cargo machete crates`,
`python .control/scripts/pctl.py validate`.

Stop conditions: C: below 5 GB; a gate you cannot make green after two
honest attempts; an accepted ADR that a phase would contradict.

---

## Phase 1 — Prelude, measurement and write-through integration

- Ship `std/*.motion` embedded in the binary (`include_str!`), loaded as an
  implicit prelude before user code; user definitions shadow it; `lint`
  warns on shadowing. The prelude carries a version (`std_version()`), and
  `project_summary`/`query` report it.
- A text-measurement trait in `motioner_dsl` implemented by the same
  provider the renderer uses (D-0035 `LayoutData`), injected by
  `motioner_io`/`motioner_app`; tests use the real font files, never a fake
  metric.
- Origins (prompt 12): spans inside `std` are marked library spans; the
  write-through planner never edits them and resolves to the user's call
  arguments (`at=beat(2)` → `at=beat(3)`) or a local assignment. Planner
  corpus cases for every preset kind.
- Performance: measure interpreter time with the prelude on
  `showcase_pro.motion`; the prelude must cost < 5 ms per run on the
  reference machine (cache the parsed prelude).

## Phase 2 — Themes and tokens

- `theme(name, **overrides)`, `use_theme(t)`, token resolution in every
  constructor and setter that takes a colour, size, spacing, radius, font or
  ease (`fill="accent"`, `style="h1"`, `gap="m"`), with clear errors for
  unknown tokens (span + nearest token suggestion).
- Type scale relative to composition height; WCAG AA check of
  `text`/`bg` and `text`/`surface` at theme load (error with the measured
  ratio).
- 10 curated themes (dark, light, editorial, playful, corporate, neon,
  pastel, mono, warm, tech). For each, render a **theme board**
  (`out/std/themes/<name>.png`: palette swatches, type scale, a card, a
  button, a lower third) and review them side by side; iterate until they
  look like a professional design system. Commit the boards as docs images
  in `docs/std/`.

## Phase 3 — Relative layout

- `node.place(anchor, x=None, y=None, margin=None)`, anchors and corners,
  fractions, `safe_area` (action 5%, title 10%) default margin for text.
- `row`, `column`, `grid`, `stack` that measure children (text measured with
  the Phase 1 provider), with `gap` tokens and `align`.
- `node.bounds()`; layout is static at run time and exactly matches render
  (a test compares `bounds()` with the rendered alpha box for text, shapes,
  images and groups).

## Phase 4 — Beats and scenes

- `comp(..., tempo=120)` and theme tempo; `beat(n)`, `bar(n)`; every time
  argument still takes seconds.
- `scene(name, start, length)` → group visible only in its window,
  `scene.t(x)` for scene-local times; scenes appear in `outline`,
  `project_summary` and the timeline as labelled ranges.

## Phase 5 — Motion presets

- `enter`, `exit` (`fade`, `rise`, `drop`, `slide`, `pop`, `zoom`, `blur`,
  `wipe`, `draw`, `spin`), `emphasis` (`pulse`, `shake`, `wiggle`, `glow`,
  `bounce`), `move_to`, `text.reveal` (`fade`, `rise`, `type`, `mask_up`,
  `scramble`; `by=char|word|line`), `enter_all`/`exit_all`, `count_up`.
  Built on D-0037 eases, `stagger`, `wiggle` and D-0034 text animators.
- Defaults from theme motion tokens; durations in beats.
- **Visual review of every preset**: render a GIF per preset and theme pair
  (`out/std/presets/<preset>.gif`, via ffmpeg) and a filmstrip; tune
  defaults (distance, overshoot, duration, ease) until each one feels
  professional, fluid and consistent with the others. Record the final
  defaults and the reasoning in a D-0039 addendum.
- Each preset passes `check_design`'s fluidity rules on its own (Phase 7) —
  the library must never produce a rubric violation by default.

## Phase 6 — Components

`std/components.motion`: `card`, `title_card`, `lower_third`,
`logo_lockup`, `badge`, `button`, `bar_chart`, `progress`,
`background(kind)` (`gradient_drift`, `grain`, `glow_orbs`). Each returns a
group with named parts, takes theme tokens, has an enter/exit pair, and is
reviewed visually on 3 themes (`out/std/components/`).

## Phase 7 — Design analysis (`motioner_analysis`)

Implement, as pure functions over the document and evaluated frames:

- `describe_frame`, `hit_test`, `sample`, `code_for_node`/`node_for_code`
  (code mapping from origins).
- `motion_report`: per element onsets, durations, peak speed (px/frame,
  relative to element size), velocity discontinuities at keys (excluding
  `hold`), ease per segment; global stagger, dead time, overload (number of
  large movers per frame).
- `layout_report`: overlaps (text/text, text/busy shape), safe-area
  violations, alignment groups, type hierarchy.
- `legibility_report`: on-screen time vs `0.5 s + 0.3 s/word`; contrast of
  each text against **what is actually behind it** (sampled from a render
  without the text, provided by `motioner_render`).
- `check_design`: one rule table (id, description, severity, threshold,
  measured value, fix text) combining the above per D-0040 §5. Thresholds
  live in one place and are documented in `docs/design-rules.md` with a
  good/bad example image per rule.
- Defect fixtures: for each rule, a tiny program that violates it and the
  fixed version; tests assert the rule fires on one and not on the other.
  These fixtures are also seed data for the D-0040 vision defect set.

## Phase 8 — Render tools

In `motioner_render` (+ `motioner_tools` wiring):

- `render_still` with overlays drawn in a separate pass, never in the
  document: `marks` (numbered boxes, legend `#n → id, name`, stable numbering
  by z then id), `safe`, `grid`, `bounds`; size by long edge (default 768,
  max 1568).
- `render_filmstrip` (≤ 8 frames, one image, frame labels),
  `render_contact_sheet` (≤ 12), `render_zoom` (region in comp pixels),
  `compare` (side by side or diff heatmap + changed regions), `view_asset`,
  `render_variants` (≤ 4 variants applied to throwaway copies; the project
  is untouched — a test proves the files are byte-identical afterwards),
  `render_preview` (low-quality MP4 + sampled frames image).
- Image cache keyed by (program hash, frame, size, overlay).

## Phase 9 — Media analysis

- `analyze_image`: dominant palette (deterministic k-means in a perceptual
  space, fixed seeds), salient region (contrast/edge energy map), empty
  regions ranked for text placement, luminance per region.
- `analyze_video`: scene cuts (frame-difference histogram over ffmpeg-decoded
  low-res frames), motion intensity curve, calm segments.
- `analyze_audio`: onset envelope, tempo by autocorrelation, beat and downbeat
  times, sections (novelty curve), loudness (RMS/LUFS-like) curve.
- Tests with generated media of known truth: click tracks at 90/120/128 bpm
  (tempo within ±1 bpm, beats within ±20 ms), a video with cuts at known
  frames, images with a known empty half.

## Phase 10 — Tool runtime and CLI (`motioner_tools`)

- The D-0043 catalogue as Rust definitions: name, description (written for
  an LLM: when to use it, what it returns, cost), JSON Schema for parameters
  and results, example, cost class. `motioner tools --schema` prints it
  (replaces the prompt 12 version, which derived only commands).
- Executors for every non-user tool, operating on an open project through
  the same `Editor` (write-through, checkpoints, undo). User tools
  (`ask_user`, `ask_user_visual`, `show_plan`, `request_approval`,
  `remember_preference`) are declared here and executed by the harness in
  prompt 14; called from the CLI they return a typed `RequiresHarness` error.
- `motioner tool <project|file> <name> --args '<json>' [--out dir]`: runs one
  tool headless, prints the JSON result, writes images to `--out` and lists
  their paths. `motioner tool ... --batch file.jsonl` runs a sequence and is
  replayable.
- Contract tests: every tool has an example call that runs on
  `examples/demo.motioner` and whose result validates against its schema.

## Phase 11 — Examples, seed corpus, docs, report

1. Rewrite three examples with `std` (a kinetic title, a product teaser, a
   data story), each with a `<name>.prompt.md` brief and a plan in comments
   mapped to `scene` blocks (D-0040 §2); they must pass `check_design` with
   zero errors. Goldens for one frame of each.
2. Docs: `docs/std.md` (every theme, preset and component with images/GIFs),
   `docs/design-rules.md`, `docs/tools.md` (catalogue, examples, how an
   external agent uses `motioner tool`), `spec/mdf-v0.md`, `README.md`.
3. Release build once: prelude cost, `check_design` time on
   `showcase_pro.motion`, render-tool latencies; record in the report and
   `.control/CONTEXT.md`.
4. Tasks with `pctl task-new` per phase, `pctl session-log`, backlog what is
   left.
5. Final report (in Spanish): what changed per phase, theme boards and
   preset GIF paths, rule list with measured examples, tool catalogue
   status, ADRs, new dependencies, performance numbers, gate results, free
   space on C:, and what is left.
