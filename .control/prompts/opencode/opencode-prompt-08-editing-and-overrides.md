# opencode prompt 08 — Motioner: editing in the editor (overrides, inspector, gizmos, keyframes, undo) + engine UI fixes

You are the orchestrator of a long, multi-phase job in two repositories:

- **Motioner** at `C:\Users\jvchi\CARPETAS\Motioner` (branch
  `feat/m1-foundations`, head `1138527`): an AI-first motion-graphics editor
  in Rust. `motioner preview <file.motion>` opens a professional but
  **read-only** editor window (layers, canvas, inspector, timeline). This job
  makes it an **editor**: UI edits become structured *overrides* of the MDF
  program (spec §9), with undo/redo, editable inspector fields, canvas
  gizmos, keyframe editing in the timeline, visibility and lock, and a
  `bake` command that writes overrides back into the program.
- **FerrousEngine dev clone** at `C:\Users\jvchi\CARPETAS\FerrousEngine`
  (branch `feat/pure2d-text-and-readback`, head `0ef6fe6`, not pushed):
  four UI-layer fixes found in session 012 (Part A). Motioner does **not**
  see this clone; it uses the pinned submodule `Motioner\engine\ferrous`.
  Part A and Part B are independent; the reviewer bumps the pin later.

The quality bar is a top-tier studio: zero duplication, rustdoc on every
public item, typed errors, no panics on runtime paths, regression tests that
fail without the change, no warnings, and a UI that keeps the look of
session 012 (read `crates/motioner_ui/README.md` and ADR D-0021 first).
Work through every phase in order and run the phase gate before moving on.
Do not stop early unless a stop condition applies. The user wants to **use**
the editor at the end: the last phase is a manual run with screenshots.

---

## 0. Ground rules (every phase, every subagent)

- Work only inside the two folders above. Never edit
  `Motioner\engine\ferrous` (pinned submodule, D-0018).
- Do not commit, stage, stash, reset, checkout, rebase or push in either
  repository. The reviewer commits.
- Build with `CARGO_INCREMENTAL=0` **and `-j 2`** (8 GB RAM; more jobs fail
  with `os error 1455`, which is memory, not code: rerun with `-j 1`).
- Free space on C: is about **7 GB**. If it drops below **5 GB**, stop and
  report. No release builds except the single one in phase 12.
- Motioner: `cargo fmt` (never `cargo fmt --all`), `cargo machete crates`
  (never at the root). Clippy runs with the workspace's pedantic lints and
  `-D warnings`; split long functions instead of silencing
  `too_many_lines`, and justify every `#[allow]` in a comment.
- FerrousEngine is not fmt-clean: run `rustfmt --edition 2021 --config
  skip_children=true` only on files you edit. 25 files are stored with CRLF
  in its index (`git ls-files --eol | grep crlf`): keep their endings.
- No new dependencies in either repository unless a phase allows one. No
  `thiserror` in library crates; `anyhow` only in `motioner_app`. No new
  `unsafe`.
- **Determinism.** Rendered pixels depend only on the document (program +
  overrides) and the integer frame. The golden
  `crates/motioner_testkit/golden/hello_f120.png` must stay byte-identical:
  `hello.motion` has no overrides file, so nothing may change for it.
  Wall-clock time stays limited to the transport and chrome animation.
- **Boundaries** (`scripts/check_boundaries.py`, D-0020): engine types only in
  `motioner_render` and `motioner_ui`. `motioner_core`, `motioner_dsl` and
  `motioner_eval` stay engine-free and file-system-free (the DSL takes
  source text; reading and writing files belongs to `motioner_ui`/
  `motioner_app`).
- Shell gotchas on this machine: Bash heredocs with mixed quotes break (write
  files with the editor); PowerShell `Add-Type` needs `$env:LIB = $null`
  (already handled in the screenshot scripts).
- Subagents may run in parallel only where this prompt says so, each owning
  only the files listed for it.

## 1. Verified facts (read the cited code before changing it)

Motioner:
- Spec `spec/mdf-v0.md` §9 "Provenance and overrides": every created node
  gets a provenance path from its source location, call stack and loop
  iteration; `.id("name")` gives an explicit identity; UI edits create
  overrides keyed by provenance or ID, stored as `override(id, property,
  value)` statements executed after the program; orphans are reported;
  baking rewrites an element into the program and removes the override;
  overrides never silently change generated code. §17 lists override syntax,
  orphan reporting and bake as deferred to v0.1 — this job delivers them.
  D-0011 makes the MDF program the truth and the document a compiled
  artifact plus overrides. Task `T-DSL-0002` ("Define provenance, override
  and bake semantics", backlog) is this work; `T-UI-0002` (in progress) is
  the editor.
- `motioner_core`: `Document` (`src/document.rs`) with `add_node`,
  `reparent`, `set_property(id, Property, PropertyValue)` (:217),
  `add_keyframe` (:234), `set_blend`; `NodeId` is a deterministic index
  (`src/node.rs:15`); `Property` = `Position | Scale | Rotation | Anchor |
  Opacity` with MDF names (`pos`, `scale`, `rotation`, `anchor`, `opacity`,
  `src/node.rs:190-210`); `NodeContent` = `Group | Rect{width,height,radius,
  paint} | Circle{radius,paint} | Text(TextContent)` (`src/node.rs:128`);
  `Transform` holds `Animatable<T>` channels with sorted `Keyframe{frame,
  value, ease}`; `Ease` has names (`src/animation.rs`). There is no
  provenance, no explicit ID, no visibility flag yet.
- `motioner_dsl`: `run(source, &Limits) -> Result<Document, Diagnostic>`
  (`src/lib.rs:57`) on a large-stack thread. Nodes are created in
  `SceneHost::create` (`src/scene.rs:156`); node methods are `NodeMethod`
  (`src/value.rs:114`, `key`, `add`); the interpreter keeps call frames
  (`src/interpreter.rs:72`, pushed at :488, popped at :494) and builds
  diagnostics with spans and stacks (:82-100). `Span{start,end,line,column}`
  is in `src/diagnostic.rs:8`. `Diagnostic` has category, message, span,
  stack and **no severity** yet.
- `motioner_eval`: `Evaluator::evaluate(document, frame, &mut plan)`,
  `sample(&Animatable, frame)`, `Affine2`.
- `motioner_ui` (session 012, read the README): `session.rs`
  (`PreviewSession`: path, last good document, plan cache by generation and
  frame, diagnostics), `transport.rs`, `file_watch.rs` (250 ms poll, 150 ms
  debounce), `input.rs` (pure `action_for`/`apply`/`step_selection`),
  `outline.rs` (rows with `name`, `detail`, `depth`, `keyframes`),
  `inspect.rs` (pure `inspect_node`/`inspect_comp` → `Section`/`Prop`/
  `Value`), `bounds.rs` (`node_bounds`, `hit_node` with inverse transforms),
  `layout.rs` (pure `Layout::compute`, `Hit`, splitters, `scroll_into_view`,
  `resize_panels`), `theme.rs` (tokens; the only colour literals),
  `text.rs` (baseline text, kerning, ellipsis), `paint.rs` (`Painter`),
  `icons.rs`, `widgets.rs` (`icon_button`, `field`, `color_field`,
  `tooltip`, `Interaction`), `panels/{top_bar,layers,canvas,inspector,
  timeline,status}.rs` (pure drawing from a read-only `View`), `app.rs`
  (`PreviewApp`: `run_update` → `handle_pointer` → `on_press/on_drag/
  on_scroll`, `run_draw_ui` → `paint_canvas`, `inspector_content`).
  Eye and lock icons exist but do nothing (`Hit::LayerEye`,
  `Hit::LayerLock`).
- Engine input (`ferrous_core::KeyCode`, `engine/ferrous/crates/ferrous_core/
  src/key.rs:16`) has letters, digits, arrows, Home/End, Enter, Escape,
  Backspace, Delete, Tab, Period, Comma, Minus, Ctrl/Shift/Alt left/right,
  numpad digits — no apostrophe, no F-keys. Typed text arrives through
  `InputState::typed_chars()`. `MouseButton` has Left/Right/Middle.
- Screenshots: `scripts/ui_shot.ps1 -Out out/ui/x.png [-File f] [-Frame N]
  [-Size WxH] [-Scale F] [-Select ROW]` captures the window without focus
  (debug build). Hover cannot be synthesised. `out/` is not committed.
- Gates today: 215 tests green; all gates in §4A green.

FerrousEngine clone (same APIs as the pin for the UI layer):
- `ferrous_core::Color::hex` (`crates/ferrous_core/src/color.rs:91`) stores
  sRGB bytes as if linear and `to_linear_f32` (:144) is the identity;
  `hex_srgb` (:120) uses a 2.2 power, not the sRGB curve.
- `GuiBatch::border` (`crates/ferrous_ui_render/src/gui_batch.rs`, around
  :300) puts the width in `uv1.x`, but `assets/shaders/gui.wgsl` reads the
  border width from `uv0.x`; `GuiBatch::shadow` puts `[spread, blur]` in
  `uv0` and leaves `color_b` zero, but the shader reads the blur from
  `uv0.x` and the unexpanded size from `color_b.xy`. Both draw nothing
  useful today.
- `GuiBatch::draw_text` (`crates/ferrous_ui_render/src/text_utils.rs`)
  takes the top of the em box as `y` (baseline at `y + size`) and ignores
  the atlas `kerning` map.
- The window surface is configured with `usage: RENDER_ATTACHMENT` only
  (`crates/ferrous_engine/src/graphics.rs`, around :238), so a frame cannot
  be copied out; `runner/frame.rs` presents at :351.

---

## 2. Phases

### Part A — FerrousEngine UI-layer fixes (subagent E, parallel with phases 2–5)

Owner: the FerrousEngine clone only: `crates/ferrous_core/src/color.rs`,
`crates/ferrous_ui_render/src/{gui_batch.rs,text_utils.rs}`,
`crates/ferrous_engine/src/{graphics.rs,context.rs,runner/frame.rs,
runner/types.rs}` (screenshot path only), and new tests.

**Phase 1 (engine).**
1. Colour: add `Color::from_srgb_hex(&str)` and `Color::from_srgb8(r,g,b,a)`
   that apply the exact piecewise sRGB EOTF (not 2.2), and document on
   `hex`, `hex_srgb` and `to_linear_f32` exactly what they do (do not change
   their behaviour; existing callers depend on it). Tests with known values
   (`#808080` → 0.2159).
2. `GuiBatch::border` and `GuiBatch::shadow` pack their parameters the way
   `gui.wgsl` reads them (border width in `uv0.x`; shadow blur in `uv0.x`,
   unexpanded size in `color_b.xy`, quad expanded around the offset rect).
   Unit tests on the produced `GuiQuad` fields that fail before the fix.
3. Add `GuiBatch::draw_text_baseline(font, text, [x, baseline], size,
   color)` that places glyphs on a baseline using the atlas cell bounds and
   applies kerning; keep `draw_text` unchanged and document its em-box-top
   convention. Unit test with a fixture atlas (or the real Roboto bytes) that
   the second glyph of "AV" moves left by the kerning amount.
4. Screenshot hook: request `COPY_SRC` on the surface when the adapter
   supports it (fall back silently), and add
   `AppContext::request_screenshot()` plus a way to receive the next
   presented frame as tight RGBA8 (`Screenshot { width, height, rgba }`,
   swizzled from BGRA when needed), for example
   `FerrousApp::on_screenshot(&mut self, Screenshot)`. No panics; a
   failed copy logs and delivers nothing. Test the swizzle and row-padding
   removal as pure functions.
5. Engine gate (§4B). Write in the report what the reviewer must do to use
   (4) from Motioner after a pin bump (a `--screenshot <png>` flag that
   replaces `scripts/ui_shot.ps1`).

### Part B — Motioner editing

**Phase 2 — decision (orchestrator, no Rust yet).**
1. ADR D-0022 "Provenance, overrides and bake (MDF v0.1 subset)" with
   `python .control/scripts/pctl.py decision-new --estado aceptada "<title>"`.
   Decide and write down:
   - **Identity.** Explicit `.id("name")` wins (unique per document,
     `[A-Za-z_][A-Za-z0-9_.-]*`). Otherwise a provenance key built from the
     creating call's source span start offset, the call-site span starts of
     the user-function frames on the stack, and the iteration index of every
     enclosing `for` loop, rendered as a stable text key (for example
     `@412/c88/i3`). Document what keeps a key stable (editing other lines
     shifts byte offsets: say so honestly and recommend `.id()` for nodes
     users will edit; the UI offers "Assign ID" in phase 10).
   - **Overrides file.** `<name>.overrides.motion` next to `<name>.motion`
     (the project folder of §15 does not exist yet). It contains only
     override statements in a restricted MDF subset:
     `override("id", "property", value)`,
     `override_key("id", "property", time, value, ease="linear")`,
     `override_unkey("id", "property", time)` and
     `override_visible("id", false)` / `override_locked("id", true)`.
     Times are seconds like `key()`, converted with the composition rate.
     The file is written by the editor in a canonical, deterministic order
     (sorted by id, then property, then time) with a header comment.
   - **Properties** that can be overridden: the five transform properties
     and their keyframes; content fields `width`, `height`, `radius`,
     `fill`, `stroke`, `stroke_width` (rect/circle), `text`, `size`,
     `color`, `align` (text); `visible` (render) and `locked` (editor only,
     no render effect).
   - **Orphans.** An override whose id matches no node, or whose property
     does not apply to the node's kind, is kept in the file and reported as
     a **warning** diagnostic; never dropped.
   - **Bake.** `motioner bake <file.motion> [--id ID]...` rewrites overrides
     into the program for nodes whose identity is reachable from a
     top-level variable binding at the end of the program (the common
     `title = text(...)` case): it appends one generated block
     `# --- baked by motioner (do not edit above this line by hand) ---`
     with `var.prop = value` / `var.key(...)` / `var.<field> = value`
     statements, then removes those overrides. Nodes created in loops or
     functions without a binding are refused with a clear message. Bake must
     be exact: `run(baked program)` equals `run(program) + overrides`.
   - **Edit semantics in the UI** (After Effects-like): editing a property
     with no keys sets the static value; editing a keyed property at the
     current frame sets or replaces the key at that frame; a "stopwatch"
     toggle in the inspector adds the first key at the playhead or removes
     all keys.
2. Update `spec/mdf-v0.md` §9 and §17 to reference D-0022 (keep the spec
   honest: what is v0.1 now, what stays deferred). Move `T-DSL-0002` to
   `in_progress` with `pctl task-move`.

**Phase 3 — core model (subagent C, parallel with phase 4 after phase 2).**
Owner: `crates/motioner_core/**`.
1. `NodeIdentity` stored on each node: explicit id or provenance key, with
   `Document::find(&str) -> Option<NodeId>` (O(1) via an index map) and a
   uniqueness check for explicit ids (`CoreError::DuplicateId`).
2. `visible: bool` on nodes (default true). `motioner_eval` skips hidden
   nodes **and their descendants** (owner of that one-line change: C; add an
   evaluator test). `locked` is editor state and lives only in the override
   set, not in `Node`.
3. Content field setters with validation (same rules as
   `NodeContent::invalid_field`): `Document::set_content(id, ContentField,
   ContentValue)`; `ContentField` names match the override property names.
4. `overrides.rs`: pure `OverrideSet` (ordered entries: `Set`, `Key`,
   `Unkey`, `Visible`, `Locked`), `apply(&self, &mut Document) ->
   Vec<OverrideIssue>` (orphans, kind mismatches, invalid values), and
   `to_mdf(&self, rate) -> String` canonical text. Round-trip property:
   `parse(to_mdf(set)) == set` (parsing is phase 4; test it there).
   Exhaustive unit tests, including determinism of `to_mdf` and that an
   empty set is a no-op on the document.

**Phase 4 — DSL (subagent D, parallel with phase 3 after phase 2).**
Owner: `crates/motioner_dsl/**`.
1. Record provenance in `SceneHost::create` from the interpreter: the call
   span, the call-site spans of user frames and the active loop indices
   (track loop iteration in the interpreter's `for`). Same program → same
   keys; tests with nested functions and loops (`hello.motion` spinners and
   dots must get distinct, stable keys).
2. `.id("name")` node method (`NodeMethod::Id`), chainable, with duplicate
   and syntax errors as diagnostics pointing at the call.
3. `Diagnostic` gains `severity: Error | Warning` (default error; update
   every constructor and the UI/CLI display).
4. `run_with_overrides(source, overrides_source: Option<&str>, &Limits) ->
   Result<Compiled, Diagnostic>` where `Compiled { document, overrides:
   OverrideSet, warnings: Vec<Diagnostic> }`. The overrides source accepts
   only the statements of D-0022 (anything else is a diagnostic in the
   overrides file, with its own span and a file label). `run` keeps its
   signature and behaviour.
5. Tests: provenance stability, `.id`, override parsing errors, orphan
   warnings, and `parse(to_mdf(set)) == set` on generated sets.

**Phase 5 — bake (orchestrator).**
Owner: a new `crates/motioner_dsl/src/bake.rs` (pure: program text +
compiled info + override set → new program text + remaining overrides) and
`crates/motioner_app/src/main.rs` (`motioner bake` subcommand, writes both
files atomically via temp file + rename, prints what was baked and what was
refused; update `HELP` and parser tests). Tests: bake exactness
(`run(baked) == apply(run(program), overrides)` for `hello.motion` with
overrides on the title and on a spinner group bound by a variable), refusal
for loop-created dots, idempotence (baking twice changes nothing).

**Phase 6 — session and undo (orchestrator).**
Owner: `crates/motioner_ui/src/{session.rs,edit.rs (new),file_watch.rs}`.
1. `PreviewSession` loads `<name>.overrides.motion` when present, compiles
   with `run_with_overrides`, keeps the `OverrideSet` and the warnings
   (shown in the status bar like diagnostics, in the warning colour), and
   watches both files.
2. `edit.rs`: pure `EditCommand` (set value, set/replace/remove key, toggle
   stopwatch, move keys, set visible, set locked, assign id) applied to an
   `OverrideSet`; an `UndoStack` (bounded, 200 steps) with coalescing for
   continuous drags (one undo step per gesture). Unit tests for every
   command, undo/redo symmetry and coalescing.
3. Applying an edit: update the in-memory override set, recompile the
   document from the cached program AST/source (no file read), bump the
   generation, then write the overrides file **debounced** (300 ms after the
   last change) atomically. The file watcher must not trigger a reload for
   the editor's own write (compare content hash or a self-write stamp); an
   external change to either file still reloads. During a drag, preview
   from memory and write once on release.

**Phase 7 — editable inspector (orchestrator).**
Owner: `crates/motioner_ui/src/{inspect.rs,widgets.rs,panels/inspector.rs,
app.rs,input.rs,text_edit.rs (new)}`.
1. `text_edit.rs`: a pure single-line editor model (text, caret, selection,
   insert from `typed_chars`, Backspace/Delete, Left/Right, Home/End,
   Ctrl+A, Enter commit, Escape cancel, Tab/Shift+Tab next/previous field).
   Unit tests.
2. Numeric fields: click to edit; **drag on the label or the field's left
   label to scrub** (horizontal; Shift ×10, Alt ×0.1; cursor `EwResize`);
   arrow Up/Down while editing nudges by 1 (Shift 10). Expressions are not
   required; invalid input shows the field in the danger colour and does not
   commit. Colour fields accept `#RRGGBB[AA]`. Text content field edits the
   text layer. Align is a three-state segmented control.
3. A stopwatch icon per animatable property (new icon in `icons.rs`,
   accent when keyed) toggles keys as D-0022 says; the yellow diamond shows
   whether the current frame is on a key (filled) or between keys (outline).
4. Keyboard focus is visible (accent 1 px ring) and global shortcuts are
   suspended while a field is being edited.
5. Tests: inspector model → commands mapping (pure), field validation.

**Phase 8 — canvas gizmos (orchestrator).**
Owner: `crates/motioner_ui/src/{gizmo.rs (new),bounds.rs,panels/canvas.rs,
app.rs,layout.rs}`.
1. Drag the selected layer to move it (position override in its parent
   space: convert the screen delta through the inverse of the parent world
   transform; test with a rotated group). Shift locks to an axis.
2. Corner handles scale (Shift keeps aspect, Alt from the anchor), a
   rotation handle above the top edge rotates (Shift snaps to 15°).
3. Snapping (toggle with a top-bar button, on by default): composition
   edges and centre, other layers' bounds; draw accent guide lines while
   snapping. Pure snapping function with tests.
4. Locked layers cannot be picked on the canvas; hidden layers are not
   drawn and not picked.
5. Hover feedback: handle cursors (`NwseResize`, `NeswResize`, `Grab`/
   `Grabbing` or `Move`), handle highlight on hover.

**Phase 9 — timeline editing (orchestrator).**
Owner: `crates/motioner_ui/src/{outline.rs,panels/timeline.rs,
timeline.rs,app.rs,layout.rs}`.
0. **Fix what the collapsed timeline shows today** (the key frames are
   computed correctly — Circle 2 keys at 0.3 s/1.3 s, Group 1 at
   1.0/1.5/3.5 s — but the drawing misleads the user into thinking they are
   at the wrong times):
   - *The lifetime bar spans the whole composition for every layer*
     (`panels/timeline.rs` "every layer lives for the whole composition
     today"), so a dot that is invisible until 0.2 s or a spinner that is
     transparent until 1.0 s looks present from frame 0. Replace it with a
     **visibility strip**: sample the node's *effective* visibility per frame
     (own and inherited opacity > 0, own and inherited scale ≠ 0, not
     hidden) once per reload/override change, cache it as frame ranges on
     the outline row (pure function, tested on `hello.motion`: Circle 1
     invisible on 0..12, Group 1 invisible on 0..60), and draw invisible
     ranges as a dim/hatched bar and visible ranges as the normal bar.
   - *All channels are merged into one row of diamonds*, so Group 1 shows
     a key at 1.5 s that belongs only to opacity and the "animated span"
     mixes rotation and opacity. In the collapsed row keep one summary
     diamond per frame but tint/outline it when it is shared by several
     channels, and show a tooltip on hover listing the channels and values
     ("1.0 s · Rotation 0°, Opacity 0").
   - *Children of an animated group show nothing* (Rect 1 has an empty bar
     although it spins with Group 1). Draw the parent's keys on child rows
     as hollow "inherited" diamonds (not selectable) so motion that reaches
     a layer is always visible on its row.
   - Put the "1s", "2s"… labels centred on (or starting exactly at) their
     tick, and label the end of the composition (5 s) at the right edge;
     today the last tick is frame 299 and the 5 s mark is missing.
1. **Property timeline for the selected layer** (After Effects style
   twirl-down). Selecting a layer — click on its timeline row, its row in
   the layers panel, or on the canvas — expands it in place into one
   sub-row per property; selecting another layer collapses the previous one
   unless the user pinned it open with the chevron (chevron toggles
   expansion independently; Alt+click the chevron expands/collapses all).
   - Sub-rows, always present for every layer in this order: Position,
     Scale, Rotation, Anchor, Opacity — animated or not. For layers with
     content, add static rows below a thin divider: Size/Radius, Fill,
     Stroke, Stroke width (rect/circle) or Text, Size, Colour, Align (text).
     Content rows have no keys (content is not animatable in the core
     yet); they show the value only.
   - Each sub-row in the names column shows: a **stopwatch** toggle (on =
     animated; turning it on adds a key at the playhead with the current
     value; turning it off removes all keys of that channel after a
     confirmation and keeps the current value as a static override — both
     undoable), the property name, its **value at the playhead** as an
     editable scrub field (drag horizontally to change, click to type; same
     widget as the inspector, same edit commands; editing an animated
     channel adds or updates a key at the playhead), and ◀ ◆ ▶ buttons to
     jump to the previous/next key and add/remove a key at the playhead.
   - In the track area each animated sub-row shows only that channel's
     diamonds, plus a thin line between consecutive keys whose shape follows
     the segment's easing (sampled polyline) so the user sees where motion
     happens and how.
   - Sub-rows are indented and slightly darker than layer rows, with a
     hairline between groups; the layer row keeps its summary diamonds.
   - Layout and hit testing for layer rows + sub-rows (expanded set,
     scrolling, row index → (layer, property)) are pure and tested;
     keyboard: Up/Down walk all visible rows, Left/Right collapse/expand,
     `U` toggles "show only animated properties" (AE habit).
2. Click a diamond to select it (Shift adds, drag a marquee to box-select),
   drag selected keys horizontally to retime (snap to whole frames, show
   the frame delta in a pill), Delete removes, `K` adds keys at the
   playhead for the selected layer's animated channels (or Position if none
   is animated).
3. Easing: with keys selected, the inspector shows an "Ease" segmented
   control of the existing `Ease` names (linear, quad, cubic, back… in/out)
   applied to the outgoing segment; draw eased segments between keys on the
   expanded sub-rows as a thin line with the curve's shape (sampled
   polyline with the polygon/line primitives you have).
4. Work-area is out of scope.

**Phase 10 — layers panel actions (orchestrator).**
Owner: `crates/motioner_ui/src/{panels/layers.rs,app.rs}`.
1. Eye toggles visibility, lock toggles lock (override), both undoable; the
   icons stay visible (dimmed) on rows that are hidden or locked even when
   not hovered; hidden rows show their name dimmed.
2. Double-click a row name to rename it. The editor never edits the program
   outside bake, so a rename is a display-name override
   (`override("<key>", "name", "Title")`) shown as the row name; it does not
   change the node's identity. Show the node's identity key in the
   inspector header with a "Copy" action so users can add `.id("...")` to
   their program themselves. Add the `name` property to D-0022.
3. Orphaned overrides appear in a collapsible "Orphans" section at the
   bottom of the layers panel with a delete action (undoable).

**Phase 11 — tests, review and docs (orchestrator).**
1. Everything that can be pure is pure and unit-tested (edit commands,
   undo, text editing, snapping, gizmo maths, timeline sub-row layout and
   hit testing, override parsing/serialisation, bake).
2. Integration test (no GPU): load `hello.motion` + an overrides string,
   apply a sequence of edit commands, write the overrides text, recompile
   from disk text, and compare documents.
3. GPU test (skips without an adapter, pattern of `tests/live_painter.rs`):
   a hidden layer is absent from the painted pixels; the golden test for
   `hello.motion` is untouched.
4. Strict review of the whole diff: duplication (especially between
   inspector fields and gizmos writing the same commands), typed errors,
   no panics in the UI loop, allocations per frame in hot paths, rustdoc.
5. Docs: `crates/motioner_ui/README.md` (editing, shortcuts table with
   Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y, K, Delete, scrub modifiers, snapping),
   root `README.md` usage (`motioner bake`), `spec/mdf-v0.md`, D-0022.
   Create task `T-UI-0003` (editing) with `pctl task-new --prefix UI
   --dominio ui`, and log events with `pctl session-log` if a session is
   open.

**Phase 12 — run it (orchestrator).**
1. `cargo build -j 2 --release -p motioner_app` (the only release build).
2. Manual run on `examples/hello.motion` copied to `out/edit/hello.motion`
   (never edit the example itself): move the title with the gizmo, change
   its colour in the inspector, add keys with K, retime them in the
   timeline, hide a circle, lock a spinner, undo/redo all of it, close,
   reopen (overrides persist), then `motioner bake out/edit/hello.motion`
   and confirm the window shows the same frame with an empty overrides
   file. Take screenshots with `scripts/ui_shot.ps1` into
   `out/ui/p08-*.png` (selection with gizmo, the selected layer's property
   timeline with selected keys and eased segments, the collapsed timeline
   showing visibility strips and inherited keys on Rect 1, inspector
   editing a field, orphans section) and report
   their paths.
3. Report UI fps while dragging a gizmo and while scrubbing at 1600×900
   (release), and memory.

---

## 3. Acceptance criteria

- Editing any supported property from the inspector, the canvas or the
  timeline creates overrides, is undoable, persists in
  `<name>.overrides.motion`, survives a restart and never changes the
  `.motion` program except through `motioner bake`.
- Selecting a layer opens its property timeline (one sub-row per property
  with its own keys, value at the playhead and stopwatch); the collapsed
  timeline shows when each layer is actually visible and shows inherited
  parent motion on child rows.
- Provenance keys are stable for an unchanged program; `.id()` works;
  orphans are warnings and are never dropped.
- `bake` is exact and idempotent and refuses unbound nodes clearly.
- Visibility hides nodes and descendants in render and picking; lock blocks
  picking and editing.
- The UI keeps session 012's look: tokens only from `theme.rs`, icons from
  `icons.rs`, layouts pure and tested, no text overlap, 4 px grid.
- Golden byte-identical; all gates green; zero warnings; no new
  dependencies; no new `unsafe`.
- Part A: engine colour constructors, fixed `border`/`shadow`, baseline
  text with kerning and a screenshot hook, each with a failing-before test.

## 4. Gates

**4A — Motioner** (end of phases 3–11; `CARGO_INCREMENTAL=0`, `-j 2`):
- `cargo fmt -- --check`
- `cargo clippy -j 2 --workspace --all-targets -- -D warnings`
- `cargo test -j 2 --workspace`
- `cargo test -j 2 -p motioner_render` (alone, without `live`)
- `RUSTDOCFLAGS="-D warnings" cargo doc -j 2 --workspace --no-deps`
- `python scripts/check_boundaries.py`
- `cargo machete crates`
- `python .control/scripts/pctl.py validate`
- free space on C:

**4B — FerrousEngine** (end of phase 1; `-j 2`):
- `rustfmt --edition 2021 --config skip_children=true --check <files you edited>`
- `cargo clippy -j 2 -p ferrous_core -p ferrous_ui_render -p ferrous_engine --all-targets --no-deps -- -D warnings`
  (if `ferrous_engine` has pre-existing warnings, report them with
  file:line and do not fix unrelated code; your new code must be clean)
- `cargo test -j 2 -p ferrous_core -p ferrous_ui_render -p ferrous_engine`
- `cargo test -j 2 -p ferrous_font -p ferrous_2d -p ferrous_renderer`
- `cargo check -j 2 --workspace --all-targets`
- `cargo check -j 2 -p ferrous_web --target wasm32-unknown-unknown`
- `git ls-files --eol | grep -c crlf` is still 25

## 5. Stop conditions

Stop and report instead of improvising if: a gate fails twice after a fix
attempt; the golden changes; provenance cannot be made stable without
changing MDF v0 semantics (report the case); the engine runner cannot
deliver something the UI needs without editing the pinned submodule (report
file:line and the Part A-style change you would make); free space on C:
drops below 5 GB.

## 6. Final report

1. Phase summaries (which subagent did what).
2. Files created/changed per repository, one line each.
3. Public API additions and breaking changes (exact signatures) in
   `motioner_core`, `motioner_dsl`, `motioner_eval`, `motioner_ui`,
   `motioner_app` and the engine clone.
4. The override file grammar as implemented, with one full example.
5. Tests added per crate and why each fails without its change.
6. Gate tables (4A, 4B).
7. Manual run: what worked, what did not, screenshot paths, fps and memory.
8. Free space on C:, open questions (provenance stability limits, bake
   coverage) and the pin-bump steps for Part A.
