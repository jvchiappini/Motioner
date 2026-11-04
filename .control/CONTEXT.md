---
actualizado: 2026-11-04
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## Prompt 12 status (2026-11-04, phase 0.5 and phase 1 done)

**Origins v2 + audio loop fix.** No commit; working tree for review. Gate green
(fmt, clippy pedantic, workspace tests, rustdoc, boundaries, machete,
`pctl validate`).

- **Phase 0.5 (T-APP-0006).** `AudioOutput::ended` reports a consumed range;
  `ProjectHost::audio_ended` exposes it. `Transport::follow_audio`
  (`crates/motioner_ui/src/transport.rs`) wraps a looping transport to frame 0
  and returns the loop start; `App::restart_audio`
  (`crates/motioner_ui/src/app/mod.rs`) re-mixes and restarts the cpal stream
  there. The first sample after the wrap is sample-exact (same integer sample
  offset arithmetic as playback). A non-looping stream stops at the end.
  Headless regressions:
  `a_finished_audio_stream_restarts_looping_playback_at_the_first_frame` and
  `audio_out::tests::the_cursor_reports_the_end_only_after_the_last_frame`.
- **Phase 1 (T-DSL-0007).** `motioner_dsl::origins` v2: every node creation and
  value/key origin records statement, block (span + indentation), binding and
  reassignment, loop stack (span, variable, iterable span, index, literal
  value, body span/indent), call stack (call site, definition, parameters and
  defaults, whether the invocation returned the node) and expression class
  (literal / parameter / loop variable / computed). Names are interned as
  `NameId`; loop/call context is shared through `Arc`. `for`/`if` parser spans
  now cover the whole construct. Tests:
  `crates/motioner_dsl/tests/origins_v2.rs`.
- **Interpreter cost** on `showcase_pro.motion` (release, 84 nodes, 647
  origins, same host): 0.80 ms/run before, ~1.50 ms/run after (interpreter
  only); the absolute cost is small next to a 16 ms render frame.
- Next: Phase 2 (code edit model + planner skeleton). Do not start it without
  the user's go-ahead.

## Prompt 11 status (2026-11-04, phases 0-7 done)

**All phases complete**: 0-4.5, 5 (audio), 5.5 (review fixes), 6 (video)
and **7 (video performance, demo/goldens/docs/release)**. Gate green (fmt,
clippy pedantic, tests, rustdoc, boundaries, machete, pctl validate). No
commit; the working tree is for review.

### Phase 7 (T-PERF-0004, T-RENDER-0016)

- **7a video performance.** The preview no longer decodes on the UI thread.
  `motioner_io::video` gained `decode_frames` (batch) and `FrameStream`
  (long-lived sequential decode, `-threads 1` for the worker).
  `motioner_app::video_cache` owns a worker thread, a mutex LRU and a
  lookahead of 24 frames; it keeps one stream open while the playhead moves
  forward and returns `Ready`/`Pending`/`Unavailable`. The canvas keeps the
  previous texture while a frame is pending. `AudioOutput::position` returns
  `None` for a silent mix, so video-only compositions use the wall clock.
  The per-frame `Project::open` is cached by manifest mtime.
  Release, 1600x900, `out/p11-video/proxy.motioner` (1080p60 source):
  baseline **112 dropped in ~5 s**; after **0 dropped in the measured 5 s
  window**, **59-61 UI fps**, **179-366 MB working set** (the SO trims it),
  **736-739 MB private**, **42 MB video cache**, eval 0.29-0.36 ms. Demo
  (1280x720, generated audio): **59-62 UI fps**, 42 MB cache; its playhead
  follows the audio device, which on this automation host runs below real
  time (the video-only test above uses the wall clock and is clean).
  Regression:
  `motioner_io/tests/video_decode_ahead.rs` (30-frame batch and persistent
  stream inside a budget).
- **7b demo.** `examples/demo.motioner/` is a real project with a
  `gradients` background PNG, an all-intra `testsrc2` 720p60 clip and an
  `aevalsrc` 48 kHz stereo music loop, a title/subtitle sequence, music and a
  fade-out. Goldens `demo_f090.png`/`demo_f330.png` are pinned by
  `motioner_render/tests/golden_demo.rs`, which also asserts the exact video
  source frame. `motioner_io/tests/demo_mixer.rs` pins the first 4096 mixed
  samples. `out/demo.mp4` is 6.000 s H.264 720p60 + 48 kHz stereo AAC
  (3,232,661 bytes); `out/demo_wave.png` shows the fade-out; `volumedetect`
  mean -21.5 dB, max -13.0 dB. Docs: `spec/mdf-v0.md` 7.7,
  `docs/projects.md`, `docs/agents.md`, `README.md`. Backlog: T-NLE-0002
  (transitions), T-AUDIO-0003 (audio effects), T-PUB-0001 (installer and
  file association). No engine pin bump was needed.

### Earlier prompt 11 phases

- Phase 5.5 (T-UI-0013): `motioner_ui::properties::PropertySet` is the one
  kind-to-property-set table; audio shows Source/Start/In/Out/Gain/Pan/Fade
  in/out/Mute/Solo and not transform/blend/effects/mask; expanded rows are
  Gain/Pan/fades; `channels::resolve`/`resolve_edit` return
  `UnknownProperty` for visual commands on audio; `Icon::ALL/has_glyph`
  covers mute/solo/filter/shadow/blur/glow; assets shows `A-N` with a
  tooltip and keeps the name at 180 px; effect rows use icons; the waveform
  uses a peak mip chain and a mirrored envelope with empty silence.
  Captures: `p11-06b-timeline-waveform`, `p11-06c-assets-narrow`,
  `p11-06d/e/f-waveform-zoom-*`, `p11-06h-effects-icons`.
- Phase 6 (T-RENDER-0015): `VideoContent`/`VideoAsset`/`VideoResolver`,
  `video()` MDF, `Document::resolve_videos`, exact frame selection,
  `ItemShape::Image` per node with `ImageKey::tagged`; `motioner_io::video`
  (ffprobe/decode/proxy); `ImageData.revision` and in-place updates with a
  size fallback; `motioner_app::video_cache` (LRU 192 MB/48 frames); all-intra
  640 proxy in `cache/<hash>.proxy.mp4` with `.proxy.done` and a background
  job; UI video inspector, thumbnails, trim/Alt-slip and proxy/memory
  readouts; video audio through the mixer and AAC mux. Flow F-0003.
  Captures: `p11-07-timeline-thumbs`, `p11-07-video-inspector`,
  `p11-07-trim-slip`, `p11-07-proxy-progress`, `p11-07-1080p60-playing`.
  Debug baseline: 198 MB working set, 769 MB private, 59 UI fps, 120 dropped
  in ~5 s, ~42 MB video cache; `out/p11-video/render.mp4` 3.000 s video +
  3.000 s AAC.
- D-0047: `time` in `motioner_io` for local relative ages; the age is pure
  with an injected `now` and the tooltip uses the local offset.
- D-0048: `cpal` only in `motioner_app`; `audio_out` owns the stream and the
  sample cursor; the transport follows `ProjectHost::audio_position`.
- Audio: `AudioContent`/`NodeContent::Audio`; MDF
  `audio(source, in_point, out_point, gain, start, pan, fade_in, fade_out)`
  with keyable `gain`/`pan`; `motioner_io::audio` (48 kHz stereo decode to
  `cache/<hash>.pcm`, pure sample-exact `mix_document`, WAV, AAC mux with
  `-shortest` and `--no-audio`); timeline waveform, Gain/Pan rows and
  trim/move through `Edit`; captures `p11-06-timeline-waveform.png`,
  `p11-06-audio-layer.png`; render `out/p11-audio/demo.mp4` (3.000 s video,
  3.000 s audio).
- Phase 4.5: `app::drop` routes OS drops (import + place on the canvas);
  window/panel highlight; assets toolbar with list/grid/clean icons and a
  compact geometry (no truncation at 180 px); recents relative with a local
  tooltip. Capture: `out/ui/p11-02-assets.png`.
- D-0045 "Project folder v1": `<name>.motioner/` with `project.motion` (MDF),
  `compositions/`, `assets/<sha256>.<ext>`, `cache/`, `backups/`, `traces/`,
  `editor.motion`; stable registry key; MDF resolves by key or original name;
  app state in `%APPDATA%\Motioner\app.motion`; atomic writes and explicit
  migrations.
- D-0046: `rfd` only in `motioner_app` (folder picker), behind `ProjectHost`.
- `motioner_io::project` (manifest, `EditorState`, `AppState`, validation,
  `clean_temp`) and `motioner_io::assets` (`ProjectImages`, import/dedupe/
  rename/remove, sources).
- CLI: `new`, `import <project> <file>...`, `backup <project>`, `migrate`,
  `info`; every command accepts a folder/manifest/composition.
- Start screen + File menu + `ProjectHost` (`motioner_app::host::Host`);
  `Editor::blank`/`replace_session`; commands `new_project`, `open_project`,
  `close_project`, `save`, `save_as`.
- **Layers/Assets** panel with a virtualised list/grid, import (menu, `+`,
  double click/drag), ffmpeg conversion to `cache/`, audio peaks, CLEAN and
  place; **0 alloc/frame** (`out/ui/p11-05-alloc.png`).
- Autosave: 300 ms overrides; `motioner_io::backups` (snapshot every 120 s,
  20/200 MB, prune) and `cache/session.lock` with a heartbeat; recovery banner
  with Restore/Dismiss.
- New asset commands: `import_asset`, `remove_asset`, `rename_asset`,
  `place_asset`, `remove_unused_assets`, `restore_backup`,
  `dismiss_recovery`; documented in `docs/agents.md`.
- Captures: `p11-01-start`, `p11-02-assets`, `p11-03-placed`, `p11-04-recovery`,
  `p11-05-alloc` in `out/ui/`.

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the custom
FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds the
composition. Scope and decisions: REF-0001..0004, D-0010..D-0048.

## Working model

- Code is usually written by opencode with DeepSeek V4.1 Flash; Claude is
  architect and auditor. Conversation with the user is in Spanish.
- Prompts live in `.control/prompts/{opencode,claude}/`. Work only inside
  Motioner, FerrousEngine and GUIMaker.
- The user wants LONG multi-phase prompts and a visible result (PNG/MP4).
- **FerrousEngine is a general-purpose engine, not a Motioner backend.** A
  missing render capability is built in the clone as a public engine feature
  with rustdoc, `crates/<crate>/docs/`, an `mkdocs.yml` entry and engine
  tests; Motioner only consumes it. The user commits the clone and bumps the
  pin.

## Current state (branch feat/m1-foundations)

- `motioner render|still|info|bake|preview|query|edit|ctl|new|import|backup|
  migrate`.
- The editor EDITS through overrides (D-0022); the program only changes with
  `bake`.
- **Agent control (D-0024)**: one `Command` enum, MDF call syntax,
  `motioner_ui::command`, executed only by the window-free `Editor`.
  `motioner query` (JSON scene), `motioner edit` (all-or-nothing,
  `--dry-run`), `preview --control` + `motioner ctl`. Docs: `docs/agents.md`.
- **Canvas view (D-0026)**: `ViewState.zoom/pan/fit_zoom`;
  `viewport::Viewport` is the one window to composition mapping.
- **Orphans (D-0025)**: `Editor::orphan_suggestions()` ranks live nodes.
- **Hybrid bake (D-0027)**: value origins + in-place literal edits + re-key +
  verify.
- **Professional motion model (D-0028)**: content fields are animatable
  channels; colour interpolation is linear-light sRGB. `NodeContent::channel`
  is the one field table.
- **Vector paths (D-0029)**, **gradients (D-0030)**, **blend modes (D-0031)**,
  **layer effects (D-0032)**, **masks (D-0033)**.
- **Typography (D-0034)**: Inter 400/600/700 in `assets/fonts/`; `FontSet`;
  `TextMetrics` in core + `CpuTextMetrics` in render; `text_width`/
  `text_bounds`; `node.animate_chars(...)`; the inspector Text section.
- **Images (D-0035)**: `image(source, width=None, height=None, fit=...)` with
  `contain|cover|fill`; PNG decoded by `motioner_io::ImageStore` (palette,
  grey and 16-bit expanded to RGBA8; gAMA/iCCP ignored); SHA-256 content keys;
  `Document::resolve_images` fills key/natural size; the plan carries key,
  destination rect and `uv_rect`; the renderer registers mipmapped textures on
  first use and reuses them by key; missing files fail `still`/`render` with
  `UnresolvedImage` and draw a placeholder in the preview.
  `examples/images.motion` and two goldens.
- **Timing (D-0037)**: `Ease` keeps `Copy + Eq + Hash`; `BezierEase` quantises
  the four control values to 1e-6 as `i32`; new presets `hold`,
  `elastic_out`, `bounce_out`, `expo_in_out`, `sine_in_out`; `bezier(...)` is
  parsed in MDF, overrides and commands; the solver is 8 Newton-Raphson steps
  then 64 bisection steps; `comp(..., motion_blur=True, shutter=180,
  samples=8)`; `Evaluator::evaluate_fractional`; `render_frame` averages
  sub-frames through the engine accumulation API; `stagger` and `wiggle` emit
  ordinary keys (SplitMix64 for `wiggle`).
- **Engine accumulation (D-0038)**: `Accumulation2d` in `ferrous_2d`, mirrored
  on `Renderer`, `Pure2dHarness` and `RenderContext`; `Rgba16Float`,
  premultiplied linear, weighted add, normalised resolve.
- **Editor quality of life (D-0044)**: `Node::name` + binding-derived defaults
  (`motioner_dsl::default_name`), `name` override, bake and `query`;
  `filter_layers`/collapse/`solo`/`shy`/`show_shy` (preview only);
  `graph_open`/`graph_view`/`move_key_value` and editable bezier fields;
  virtualised timeline rows + cached `OutlineRow::visible_runs`; optional
  `alloc-count` feature; live motion-blur preview (`app/blur.rs`) through
  `FrameRenderer::render_preview_frame`, 4 samples playing / full paused.
- Keyframes per axis; animated vectors are always keyed per axis (`pos.x`).
- In-app screenshots: `preview --screenshot <png>` and `ctl screenshot(...)`;
  `scripts/ui_shot.ps1 [-Commands ...] [-Select ROW]`.
- **GPU tests**: `motioner_testkit::gpu::lock()` serializes them; the normal
  `cargo test --workspace` gate is green without `--test-threads=1`.

## Phase 12 release numbers (reference machine)

- Release build once: `cargo build --release -p motioner_app -j 2` with
  `CARGO_INCREMENTAL=0`, 5m56s.
- `showcase.motion`: 720 frames in 8.079 s, **7.778 ms/frame**
  render+readback, 3.065 ms/frame encode.
- `showcase_pro.motion`: 1440 frames in 26.621 s, **16.499 ms/frame**
  render+readback (8-sample motion blur), 1.763 ms/frame encode.
- Preview (release + `alloc-count`, 1600x900, Pro reel playing): **59 UI fps**,
  eval 0.03 ms, **0 chrome allocations/frame**, canvas at a 39 % fit.
  Captures: `out/ui/p12-01-alloc.png`, `out/ui/p12-02-fps-1600.png`.
- Report (Spanish): `out/reports/PHASE-12-REPORT.md`.

## Goldens

- `hello_f120.png` (byte-identical), `blend_f30.png`,
  `gradients_f30/f105.png`, effect/mask/path/animatable stills.
- Typography (D-0034): `typography_tracking_f30.png`,
  `typography_lines_f150.png`, `typography_chars_f260.png`.
- Images (D-0035): `images_fit_f030.png`, `images_composite_f150.png`.
- Timing (D-0037): `timing_eases_f030.png`, `timing_motion_blur_f400.png`.
- Showcase Pro (Phase 11): `showcase_pro_f180/f330/f600/f810/f1086/f1320.png`
  (1920x1080) pinned by `motioner_render/tests/golden_showcase_pro.rs`.
- Demo (prompt 11 phase 7): `demo_f090.png`, `demo_f330.png` (1280x720)
  pinned by `motioner_render/tests/golden_demo.rs`; the same test asserts the
  exact video source frame. `motioner_io/tests/demo_mixer.rs` pins the first
  4096 mixed samples of the demo.
- `hello_f120.png` stays byte-identical after Phases 10-11:
  SHA-256 `C2173BADD7F23933461473C5D682E40FC9FC1DCBD4EBBE10A1C7C1C7DE5CF15C`.

## Gotchas

- **PowerShell**: double-quoted strings and here-strings expand backticks and
  `$`, so never pass Python code containing `` ` ``, `\"` or `${...}` through
  `python -c @"..."@`; write a `.py` script with the editor tool instead.
  Native args drop unescaped `"`, which breaks `ctl select("@288")` and
  `scroll("inspector", n)`; use `--select ROW` and a taller window instead.
  Escape inner quotes as `\"` when passing `screenshot("out.png")`.
- `scripts/ui_shot.ps1` escapes quotes but the outer PowerShell invocation may
  still strip them; prefer `-Select`.
- 8 GB RAM: cargo needs `-j 2`; clippy all-targets may need `-j 1` when the
  machine is busy. `CARGO_INCREMENTAL=0`.
- Disk: keep C: above 5 GB. `target/debug/deps/*.pdb` can be deleted to free
  ~11 GB; `cargo clean` after a phase is the last resort. The linker fails
  with LNK1318 at 0 bytes free.
- `core.autocrlf=true`: working copies CRLF; engine files edited with Python
  `newline=''` keep their EOL. rustfmt may turn a CRLF file into LF; restore
  it with a `\n` to `\r\n` pass. Motioner files are LF (`.gitattributes
  eol=lf`).
- Clippy pedantic: `too_many_lines`, `too_many_arguments`, `option_option`,
  `float_cmp`, `implicit_clone`, `doc_markdown`, `must_use`,
  `match_same_arms`, `single_match_else`, `cast_lossless` and
  `arc_with_non_send_sync` are easy hits.
- Inspector order: Composite -> Effects -> Mask -> Transform -> Text ->
  Animators -> Image -> Audio.
- `cpal` 0.18 uses `u32` sample rates (`min_sample_rate()` /
  `with_sample_rate` take `u32`, not `SampleRate`); tests never open a device,
  only `mix_document` and the WAV/mux path. A silent mix now returns
  `position() == None` so the transport uses the wall clock. The output stream
  asks for a 1024-frame fixed period and falls back to the device default.
- `time` may fail to read a local offset on unusual hosts; the tooltip falls
  back to UTC, which is still a full date.
- `motioner_ui` must NOT depend on `motioner_io` (boundary check). The image
  loader enters the UI as `Rc<dyn motioner_core::ImageResolver>` through
  `PreviewOptions.image_resolver`; the app creates `CachedImages`.
- The demo playhead follows the audio device. On this automation host the
  device clock runs slower than wall time, so a 6 s demo can take longer than
  6 s to reach the end; UI fps and steady-state dropped frames are unaffected.
  Looping with audio restarts the stream at the loop start (prompt 12 phase
  0.5); see `Transport::follow_audio` and `App::restart_audio`.

## FerrousEngine state

- Pin `7e95367` on `feat/pure2d-text-and-readback`: temporal accumulation
  (`Accumulation2d`) and level-zero texture upload.
- Pinned clone commit `6a30e7d` (Phase 10 review): `GuiBatch::line`/`polyline`
  with `LINE_BIT (1 << 9, shader 0x200)`, anti-aliased segments;
  `Renderer::update_sprite_texture` with in-place `queue.write_texture` for
  same-size frames.
- Clone commit `e482b3c` (Phase 12): `GuiBatch` recycle (`FramePacket::remove`,
  `FrameBuilder::take_ui_batch`, `Renderer::take_recycled_ui_batch`) so
  Motioner's chrome reads 0 allocations/frame.
- Pinned clone commit `cab7035` (in Motioner history at `fc41083`):
  `FileDropEvent`/`FileDropKind`, `FerrousApp::on_file_drop`; the runner
  queues `HoveredFile`/`HoveredFileCancelled`/`DroppedFile` and delivers them
  in the frame's `AppContext`; docs in `ferrous-app-trait.md` and the README,
  test `ferrous_engine/tests/file_drop.rs`.
- **No engine pin bump is needed for prompt 11 phase 7.** The features used by
  the preview worker (`update_sprite_texture`, file drops, accumulation,
  line/polyline, `GuiBatch` recycle) are already in the pinned clone and
  documented/tested there.
- **Flaky GPU tests**: the first parallel run after a build can fail with
  `wgpu error: Validation Error` / `create_buffer ... 'Particles Storage
  Buffer' ... Not enough memory left` or FXC out-of-memory. It is device
  contention; `tests/common/mod.rs` has a process-wide `gpu::lock`.

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.

## Editor UI

- Chrome is an immediate painter on the engine `GuiBatch`; tokens in
  `theme.rs` are the only colour literals.
- One key-marker language (`widgets::key_marker`).
- Mask rows: Source cycles through `Document::can_mask` candidates, Mode is
  Alpha/Luma, Invert is Off/On/Clear.
- Text rows: Family/Weight cycle tiles, Tracking/Line height numeric with
  stopwatches, and the content field shows `\n` for line breaks.
- Animators section: one row per animator with a Del tile, plus Fade/Rise/
  Colour add tiles (`text_anim_add`/`text_anim_remove`).
- Image section: Source (text), Natural size (read only), Width/Height with
  stopwatches, Fit cycle. Image layers use a picture icon.
- Video section: Source, Natural size, Width/Height, Fit, Speed, In/Out,
  Start, Gain; timeline thumbnails, trim and Alt-drag slip; proxy progress and
  video cache size in the status bar.
- Audio section: Source/Start/In/Out/Gain/Pan/Fade in/out/Mute/Solo; expanded
  rows are Gain/Pan/fades.

## Last relevant session

Prompt 11 phase 7 (T-PERF-0004/T-RENDER-0016, session
`S-2026-11-04-003`):

- **7a**: baseline release 112 dropped/5 s at 1080p60. `FrameStream` +
  worker + lookahead 24 + bounded LRU; steady-state 0 dropped/5 s, 59-60 UI
  fps, 179-366 MB working set, 736-739 MB private, 42 MB video cache. Silent mixes
  use the wall clock; `Project::open` cached by mtime. Headless decode-ahead
  regression added.
- **7b**: `examples/demo.motioner/` with lavfi image/video/music; goldens
  `demo_f090/f330`; video-frame-selection assertions; mixer sample-exact
  golden; `out/demo.mp4` (6 s H.264 + AAC) and `out/demo_wave.png`; docs
  (`spec/mdf-v0.md`, `docs/projects.md`, `docs/agents.md`, `README.md`);
  demo preview 59-60 fps; backlog T-NLE-0002, T-AUDIO-0003, T-PUB-0001.
- Gate green: fmt, clippy pedantic `-D warnings` (`-j 1`), workspace tests
  (`-j 1`), rustdoc `-D warnings`, boundaries, machete, `pctl validate`.
- C: free ~15.5 GB after deleting `target/debug/deps/*.pdb`.
- Reviewer: commit the working tree (no engine pin bump).
