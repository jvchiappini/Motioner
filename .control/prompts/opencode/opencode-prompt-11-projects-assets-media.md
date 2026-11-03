# opencode prompt 11 — Motioner: a real app (projects on disk, start screen, assets panel, autosave, audio and video)

You are the orchestrator of a very long, multi-phase job in two repositories:

- **Motioner** at `C:\Users\jvchi\CARPETAS\Motioner` (branch
  `feat/m1-foundations`). It renders MDF programs (Python-like DSL) to MP4
  and has a full editor with a typed command layer for agents (D-0024).
  Run this prompt **after** `opencode-prompt-10-professional-motion.md`.
- **FerrousEngine dev clone** at `C:\Users\jvchi\CARPETAS\FerrousEngine`:
  engine work Motioner cannot do on its own (file-drop events, streamed
  texture updates, anything windowing). Motioner uses the pinned submodule
  `Motioner\engine\ferrous`, never the clone; at the end of each engine part
  the reviewer pushes and bumps the pin with an ADR (procedure in
  `engine/README.md`). Until the pin moves, Motioner phases that need engine
  features finish behind a typed `Unsupported` error, never a panic.
  **Engine features must be generic** (useful to any FerrousEngine app, no
  Motioner names or concepts) and documented in the engine docs.

Today Motioner is a tool you launch from a terminal with one `.motion` file;
fonts and images resolve from `assets/` under the process working directory
(D-0034, D-0035); there is no audio or video. The goal of this prompt is that
a person double-clicks Motioner, sees a start screen, creates or opens a
**project folder**, imports images, sounds and videos into it, and everything
the editor creates lives inside that folder, saved automatically. The folder
layout is the one D-0005 already decided.

Quality bar: top-tier studio. Zero duplication, rustdoc on public items,
typed errors, no panics on runtime paths, regression tests that fail without
the change, no warnings. Work phase by phase; run the gate after each phase.
The phases are ordered by value; if the job must be split across runs, stop
cleanly at a phase boundary and report.

---

## 0. Ground rules (every phase, every subagent)

- Work only inside the two folders above. Never edit
  `Motioner\engine\ferrous`. Do not commit, stage, stash, reset, checkout,
  rebase or push. The reviewer commits.
- `CARGO_INCREMENTAL=0`, `-j 2` (8 GB RAM). Release builds only to measure
  performance in the last phase. Stop if free space on C: drops below 5 GB
  (you may clean caches outside the repositories).
- Motioner: `cargo fmt` (never `--all`), `cargo machete crates`. Clippy
  pedantic `-D warnings`; every `#[allow]` has a reason.
- FerrousEngine: run `rustfmt --edition 2021 --config skip_children=true`
  only on files you edit; keep CRLF files' endings (`git ls-files --eol`);
  run the engine's tests for the crates you touch.
- **New dependencies need an ADR first** (`pctl decision-new`): what it is,
  why writing it is worse, licence, size, transitive deps. Expected
  candidates: a native folder/file dialog (`rfd`, or the `windows` crate if
  the engine already depends on it), an audio output (`cpal`), a file
  watcher (`notify`). ffmpeg stays an external binary as for export
  (`motioner_io::encode`); decoding audio and video through it needs no
  crate.
- **Document format** stays MDF (D-0005, D-0011). Every new property is
  readable from MDF, keyable where it says so, overridable (D-0022),
  bakeable, and visible in `query`. Prompt 12 will make UI edits write code
  directly (D-0036); keep every new edit path behind the existing
  `Command` → `EditCommand` resolution so prompt 12 only has to swap the
  sink.
- **Command layer is law (D-0024).** New editor actions (new/open project,
  import asset, add audio/video layer, trim, volume...) are `Command`
  variants with parity coverage and `docs/agents.md` entries. A file dialog
  is UI only: the command takes a path, the dialog just fills it.
- **Determinism.** Pixels and audio samples depend only on the document and
  the integer frame (sample index for audio). Video frames are selected by
  exact timestamp math, never by wall clock. `hello_f120.png` and every
  existing golden stay byte-identical.
- **Boundaries** (`scripts/check_boundaries.py`): engine types only in
  `motioner_render` and `motioner_ui`; core/DSL/eval engine-free and
  file-system-free. Project files, assets, ffmpeg and audio devices live in
  `motioner_io` / `motioner_app` (add a crate such as `motioner_media` only
  if an ADR justifies it).
- Shell gotchas: Bash heredocs containing `'''` break in this environment
  (write files with the editor tool); PowerShell 5.1 drops unescaped `"` in
  native arguments; `core.autocrlf=true`.
- Screenshots: `scripts/ui_shot.ps1 -Out out/ui/p11-NN-<what>.png
  [-Commands ...]`; stills: `motioner still <file> --frame N --out ...`.
  Look at what you render and fix the worst visible problem first.

Phase gate (Motioner): `cargo fmt -- --check`,
`cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
`cargo test --workspace -j 2 --no-fail-fast`,
`RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
`python scripts/check_boundaries.py`, `cargo machete crates`,
`python .control/scripts/pctl.py validate`.

Stop conditions: C: below 5 GB; a gate you cannot make green after two
honest attempts; an accepted ADR that a phase would contradict; a needed
engine feature whose pin is not bumped yet (finish the Motioner side behind
`Unsupported` and continue with the next phase).

Before Phase 1, write one ADR **"Project folder v1"** that turns D-0005 into
an implementable contract, so later phases do not reinvent it:
- folder `<name>.motioner/` with `project.motion` (format version, settings,
  asset registry, composition list, main composition), `compositions/
  <slug>.motion` (+ `.overrides.motion` while overrides exist), `assets/
  <sha256>.<ext>`, `cache/`, `backups/`, `traces/` (reserved for D-0036),
  `editor.motion` (layout, locked, expanded rows, last frame: editor state,
  never rendered);
- asset registry entries: content hash, original file name, kind (image,
  font, audio, video), import date, and a stable human key used from MDF.
  Decide the MDF surface (proposal: `image("logo.png")` resolves through the
  registry by original name, `asset("logo.png")` is the explicit form; a
  clash of names gets a suffix at import);
- how the project root replaces "the process working directory" of D-0034
  and D-0035, and how a loose `.motion` file still opens (single-file mode
  for `examples/` and tests);
- where app-level state lives (`%APPDATA%\Motioner\`: recent projects,
  window placement, preferences) and its format (MDF);
- atomic writes, validation on load, unknown fields rejected, migrations
  explicit (D-0005 rules).

---

## Phase 1 — Project model and I/O

- `motioner_io::project`: load, validate and save the folder; typed errors
  with paths; atomic writes (temp + rename); deterministic serialization
  order; round-trip tests (load → save → byte-identical).
- The asset resolver used by fonts and images reads from the project root;
  single-file mode keeps today's behaviour.
- CLI: `motioner new <dir> [--name N] [--size 1920x1080] [--fps 60]`,
  `motioner info <project>`, and every existing command (`still`, `render`,
  `preview`, `query`, `edit`, `bake`, `ctl`) accepts a project folder as
  well as a `.motion` file.
- `motioner migrate <file.motion> <dir>` turns a loose file plus the assets
  it references into a project.

## Phase 2 — Motioner opens like a normal app

- Running `motioner` with no arguments (or double-clicking the exe) opens
  the editor on a **start screen**: New project, Open project, recent
  projects (name, path, last opened, a thumbnail rendered from the last
  frame and stored in `cache/`), and the examples.
- New project dialog: name, location (native folder picker), size, fps,
  duration, background; creates the folder through Phase 1 and opens it.
- Open: native folder picker; also accepts a `project.motion` or a loose
  `.motion` file. A missing recent entry is shown greyed with "remove".
- A **File menu** in the editor chrome: New, Open, Open recent, Save (forces
  a write; normally autosave), Save as (copies the folder), Import asset,
  Reveal in Explorer, Close project → start screen.
- Window title `<project> — Motioner`, dirty marker while a write is pending.
- `motioner <path>` still opens directly. Register nothing in the OS yet;
  file association waits for the installer (INITIATIVE-0021).
- Commands: `new_project`, `open_project`, `close_project`, `save`.

## Phase 3 — Assets panel and import

- An **Assets panel** (docked next to Layers): grid/list toggle, thumbnails
  for images and video (first frame), waveform strip for audio, font name
  sample for fonts, size and duration, "used by N layers".
- Import by menu, by the panel's + button and by **dropping files** on the
  window (engine: a generic file-drop event; Motioner: routes it to
  `import_asset`). Import hashes the bytes, copies to `assets/<sha256>.<ext>`
  (dedupe by hash), adds the registry entry and writes `project.motion`.
- Dragging an asset from the panel to the canvas or timeline creates the
  right layer (`image`, `audio`, `video`) at the drop point / playhead.
- Formats: PNG native (D-0035). JPEG, WebP, GIF still frames, etc. are
  converted once at import through ffmpeg into a PNG stored in `cache/`
  (the original stays in `assets/`), unless the phase ADR prefers a crate.
- Fonts dropped into the project become assets and appear in the Family
  cycle (D-0034 resolution order: project assets first).
- "Remove unused assets" lists and deletes unreferenced blobs after a
  confirmation.
- Commands: `import_asset(path)`, `remove_asset(key)`, `rename_asset(key,
  name)`, `place_asset(key, frame=None, pos=None)`.

## Phase 4 — Autosave, backups and recovery

- Every committed change writes the affected files (debounced, atomic).
- `backups/`: rolling snapshots (every N minutes and before migrations),
  bounded count and size, oldest pruned.
- Crash recovery: a lock file with the PID; on open after a crash, offer to
  restore the newest backup newer than the files on disk.
- Tests simulate interrupted writes (temp file left behind) and prove no
  file is ever half written.

## Phase 5 — Audio

- ffmpeg decodes an audio asset once to 48 kHz stereo f32 PCM in `cache/`
  (keyed by asset hash). Peaks for waveforms cached alongside.
- MDF: extend the signature the spec already reserves:
  `audio(source, in_point=0.0, out_point=None, gain=1.0)` plus `start=0`,
  `pan=0.0`, `fade_in=0`, `fade_out=0`; `gain` and `pan` keyable. Keep the
  spec names (`in_point`, `out_point`, `gain`); do not invent synonyms. Audio nodes
  are not drawn; they appear in the layers panel and timeline with a
  waveform row.
- A deterministic mixer in a non-engine crate (pure function: document +
  sample range → samples), unit-tested sample-exactly.
- Playback through the audio device synchronised to the transport: the
  audio clock drives the playhead while playing; scrub plays short grains;
  mute/solo per audio layer.
- Export: `render` muxes the mixed track into the MP4 (AAC through ffmpeg);
  `--no-audio` flag. A test checks the muxed duration and a known sample
  pattern.
- Timeline: drag to move, trim handles on both ends, volume/pan rows.

## Phase 6 — Video

- `video(source, in_point=0.0, out_point=None)` as reserved in the spec,
  extended with `start=0`, `speed=1.0`, `width=None`, `height=None`,
  `fit="contain"`, `gain=1.0`; behaves like `image` for
  geometry, effects, masks and blend modes; its audio goes through the
  Phase 5 mixer.
- Frame-exact decode through ffmpeg: source frame = f(composition frame,
  start, in_point, speed, source fps), tested for 24/25/30/60 fps sources
  into 60 fps compositions.
- Preview uses proxies generated in `cache/` (lower resolution, all-intra)
  with a background job and a progress indicator; `render`/`still` always
  decode the original at full resolution.
- Streamed texture updates (engine: generic "update texture region"
  without re-registering) and a bounded frame cache; memory readout in the
  profiler overlay.
- Timeline: thumbnail strip, trim, slip (alt-drag), speed field.

## Phase 7 — Example project, goldens and docs

- `examples/demo.motioner/`: a real project folder with an image, a short
  generated audio loop and a short generated video (generate them with
  ffmpeg test sources; no third-party media), a title sequence over the
  video with music and a fade-out.
- Goldens: a still of the demo at two frames, a video-frame-selection golden,
  a mixer sample-exact test. Render `out/demo.mp4` and listen to it (check the
  waveform in a still).
- Docs: `spec/mdf-v0.md` (`audio`, `video`, `asset`, project resolution),
  `docs/agents.md` (new commands), `docs/projects.md` (folder layout, what is
  safe to delete, backups), root `README.md` (how to open the app), engine
  docs for every engine feature.
- Release build once: preview fps with the demo and with a 1080p60 video
  playing; record in the report and `.control/CONTEXT.md`.
- Tasks with `pctl task-new` per phase, `pctl session-log`, backlog what is
  left (NLE transitions, audio effects, installer and file association).
- Final report (in Spanish): what changed per phase, engine parts and the
  clone head(s) for the pin bump, ADRs, new dependencies, goldens,
  screenshots and render paths, performance numbers, gate results, free
  space on C:, and what is left.
