# Claude session start — PCS session 012: professional editor UI

Speak Spanish with me. Write code, docs and commits in English.

## What is different in this session

In earlier sessions you were only architect and reviewer, and opencode
implemented. **In this session you implement the Motioner editor UI
yourself.** The goal is a preview/editor window that looks like a
top-studio product (think Rive, Jitter, Cavalry, Figma, After Effects).
Today it looks amateurish. You work in a loop: change the code, build,
take a screenshot, look at it with the Read tool, critique it, and repeat.
I want to see before and after.

## Folders (work only here)

- Motioner: `C:\Users\jvchi\CARPETAS\Motioner` (branch `feat/m1-foundations`).
- FerrousEngine dev clone: `C:\Users\jvchi\CARPETAS\FerrousEngine` (branch
  `feat/pure2d-text-and-readback`, last pushed commit `df2d4bf`; the
  prompt-07 Part A fixes are uncommitted there).
- GUIMaker dev clone: `C:\Users\jvchi\CARPETAS\GUIMaker` (read-only reference).
- opencode prompts: `Motioner\.control\prompts\opencode\` (07 is the last one).

## Rules (unchanged)

- Never commit or push without my permission. Push only if I ask.
- Never edit `Motioner\engine\ferrous` (pinned submodule, D-0018). If the
  UI needs an engine feature (rounded rects, clipping, icons, a frame
  capture), implement it in the FerrousEngine clone on a branch, explain
  it to me, and propose a pin bump with an ADR (procedure in
  `engine/README.md`). Prefer solving things in Motioner first.
- Build with `CARGO_INCREMENTAL=0` and `-j 2` (8 GB RAM, `os error 1455`).
  **C: had 8.2 GB free at the end of session 011.** If it drops below 8 GB,
  stop and ask me before continuing. Clean caches outside the three repos
  first (I already allowed it: Temp, npm cache, browser caches, shader
  caches). Avoid release builds unless they are needed.
- Never `cargo fmt --all` in Motioner; use `cargo fmt`. Run
  `cargo machete crates`, not at the root.
- Engine types are allowed only in `motioner_render` and `motioner_ui`
  (D-0020). `motioner_core`, `motioner_dsl` and `motioner_eval` stay
  engine-free.
- Determinism of rendered frames is unchanged: the canvas shows exactly
  the `FrameRenderer` output for the transport frame (the golden
  `hello_f120.png` must stay byte-identical). UI chrome may animate with
  wall-clock time, but no clock may decide which frame is rendered.
- FerrousEngine keeps 25 CRLF files in the index; check with
  `git ls-files --eol` before any engine commit.

## Startup steps

1. Read `.control/CONTEXT.md`, run `python .control/scripts/pctl.py status`,
   close session 011 and open 012 with pctl.
2. `git status` in Motioner and in the engine clone; check free space on C:.
3. Summarise the state in a few lines, then start Phase 0.

## Phase 0 — review prompt 07 (not reviewed yet)

opencode finished prompt 07 (preview window plus engine Part A) and reported
all gates green, but I have not reviewed it. Before redesigning anything:

- Read the diff in both repos. Look for bugs, duplication, missing rustdoc,
  untyped errors, and boundary violations. Run gates 4A and 4B from
  `opencode-prompt-07-preview-window.md` with `-j 2`.
- Open points from its report:
  - Startup policy: an MDF diagnostic opens the window with the
    diagnostic, and an unreadable file exits 1. Keep that unless you find
    a reason not to.
  - The engine `AppContext` does not expose `UiSystem` or resolved widget
    rects, so the chrome is drawn by hand with `GuiBatch::rect` and
    `draw_text`.
  - `App::run()` returns `()` and unwraps window creation.
- Make small fixes yourself. Then ask me for permission to commit prompt
  07 (Motioner) and the engine Part A, as separate commits.

## Phase 1 — reliable screenshots (the iteration tool)

opencode captured the window with `CopyFromScreen` and got the terminal
instead (see `out/preview.png`), because the capture depends on focus. Build
a capture that does not need focus and that you can repeat:

- Preferred: a deterministic in-app capture. Add
  `motioner preview <file> --frame N --paused --window-size WxH
  --screenshot <png> --exit-after-frames K`, which writes the final
  presented frame (canvas and chrome) to a PNG. Check whether the engine
  can read back the surface or render the UI pass into an offscreen
  target. If it can only do that with an engine change, do the
  alternative below first and propose the engine change.
- Alternative: `scripts/capture_window.ps1 -ProcessId <pid> -Out <png>`
  using Win32 `PrintWindow` with `PW_RENDERFULLCONTENT` (flag 2) on the
  process's main HWND. It works without focus. Verify that it captures GPU
  content and not a black frame.
- Save the screenshots in `out/ui/iter-NN-<what>.png`. Keep `out/ui/before.png`
  as the baseline. These files are not committed.
- Every iteration: capture at 1600x900, look at it with Read, and write a
  short critique (hierarchy, alignment, spacing, contrast, text
  rendering). Every few iterations, also capture at 1280x720, and with a
  diagnostic shown (broken MDF).

## Phase 2 — diagnose the current look

Things visible in `out/preview_direct.png`, to check:

- The chrome greys look washed out. `linear_hex("#2f3d4d")` should be a
  dark blue-grey but renders as light grey. This suggests a double
  sRGB→linear conversion, or a GUI pass that does not expect linear
  colours. Find the real colour pipeline of `GuiBatch` and of the surface
  format. Fix it at the root, with one colour helper, and write a test or
  assert for the conversion.
- Top-bar text sits on the bottom edge (baseline vs top-left origin
  confusion). Find how `draw_text` positions text and centre it properly
  using font metrics.
- No visual hierarchy, no icons, flat grey panels, a thick vertical
  divider, text-only transport buttons, an outline without kind icons or
  indentation guides, and a timeline without layer tracks.

## Phase 3 — design system first, then panels

1. **Tokens** (`motioner_ui/src/theme.rs`): a dark neutral ramp (about 8
   steps, cool-neutral, with the background darker than the panels), one
   accent colour, semantic colours (selection, hover, danger, warning),
   a 4 px spacing grid, radii, border widths, a type scale (11/12/13/15
   px with weights if available) and DPI-scaled metrics. No hex literals
   outside the theme.
2. **Primitives** (`motioner_ui/src/widgets/`): panel, header, button
   (icon, label, toggle) with hover, pressed and disabled states, a
   separator of 1 px hairline, a tooltip, and text with ellipsis. Check
   what the engine already offers (rounded rects, clipping, scissor,
   SDF shapes in `GuiBatch` or `ferrous_ui_core`) before writing your own.
3. **Icons**: a small vector icon set drawn from shape primitives or paths
   (play, pause, step, jump to start/end, loop, eye, lock, and kind icons
   for text, circle, rect, group). One module, no bitmap assets unless the
   engine already supports them well.
4. **Layout** (pure, testable without GPU, in `layout.rs`), roughly:
   - Top bar (40 px): app mark, file name with a modified/error dot,
     composition size and fps as subtle pills, zoom % on the right.
   - Left panel (layers): header, rows with kind icon, name, indentation
     guides for groups, hover and selection states, and a visibility eye
     (display only for now).
   - Centre canvas: a darker pasteboard, the composition frame with a
     subtle shadow/border, and the letterboxed render. Optional safe-area
     toggle.
   - Right panel (inspector, new, read-only): properties of the selected
     item (position, size, fill, opacity, transform) from the evaluated
     frame, formatted in aligned columns.
   - Bottom (timeline): a ruler with major/minor ticks and labels, one
     track per layer aligned with the left rows, item lifetime bars,
     a playhead with a time label, and the transport centred above it
     with icon buttons and a timecode in a monospace-looking style.
   - Status bar (22 px): diagnostics count, render ms, UI fps.
   - Resizable splitters if feasible; otherwise fixed widths that scale with DPI.
5. **Interaction polish**: hover tracking, the cursor changes over
   splitters and the timeline, tooltips with shortcuts, keyboard focus,
   smooth playhead scrubbing, and selection synced between the outline,
   the canvas highlight and the inspector.

Split the current 948-line `window.rs` into modules (theme, layout,
widgets, icons, panels/{topbar,layers,canvas,inspector,timeline,status})
with no duplicated drawing code. Keep `PreviewSession`, `Transport` and the
rest of the pure state logic as they are, unless the review finds problems.

## Phase 4 — iterate to quality

- Do at least 8 screenshot iterations. In each one, fix the worst visible
  problem first.
- Before you call it done, check:
  - every edge aligns to the 4 px grid;
  - no text is clipped or overlaps, and long names get an ellipsis;
  - contrast is at least 4.5:1 for body text;
  - one accent is used consistently;
  - it looks right at 1280x720 and 1920x1080, and at 100 % and 125 %
    scale;
  - the empty state (no items), the error state (diagnostic) and the
    loading state all look designed;
  - scrubbing and playback stay at 60 fps or better on this machine.
- Show me 2 or 3 intermediate screenshots during the session (tell me
  the paths) so I can steer.

## Phase 5 — tests, gates, docs

- Unit tests for layout (rects at several sizes and scales, no overlaps,
  everything inside the window), hit testing, theme colour conversion and
  text ellipsis.
- The `live_painter` golden test stays green.
- Gates: `cargo fmt -- --check`,
  `cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
  `cargo test --workspace -j 2`,
  `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps`,
  `python scripts/check_boundaries.py`, `cargo machete crates`.
- Update `crates/motioner_ui/README.md` (structure, theme, how to take
  screenshots). Add an ADR only if you make an architecture decision
  (for example a new engine API or an icon strategy).
- Create the task `T-UI-0002` (professional editor chrome) in `.control/tasks/ui/`
  and log the session events with pctl.

## Final report (in Spanish)

- Paths to before/after screenshots.
- What changed, organised by area.
- Engine changes proposed or made (branch, files, pin-bump plan).
- Gate results and free space on C:.
- Known gaps and a proposed next step (for example editable inspector
  fields, keyframe display, or the GUIMaker integration from T-ARCH-0007).
- Do not commit anything without asking me.
