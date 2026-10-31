# Project goals

## Objective
Build a deterministic, real-time motion-graphics editor and NLE where a
human and an AI agent co-create professional 2D/3D videos from prompts,
code and timeline edits, with automatic AI iteration, audio mixing, a
node graph, Python-like executable code, and frame-accurate export.

The north star is After Effects-level control and parity, except for
importing After Effects projects, plus Manim-class programmatic
animation. The confirmed 1.0 scope is in
`docs/reference/REF-0003.md`.

## Success criteria for 1.0
- [ ] A 30-second 1080p composition with 2D/3D layers, text, shapes,
      images, video and audio previews at 60 fps on the reference
      machine (full quality, proxy only while scrubbing), and exports an
      MP4 and a PNG sequence from the same evaluation pipeline.
- [ ] NLE editing works: import, cut, trim and arrange video and audio on
      a timeline; stereo tracks have volume, pan, fades and a basic
      effects chain; playback and export stay in sync.
- [ ] 3D supports glTF models, cameras, lights, PBR, skeletal animation
      and GPU particles, composited with 2D layers.
- [ ] An AI prompt creates or edits a composition, renders previews,
      inspects them and iterates automatically within a budget, with
      every change undoable and visible as code; the final render needs
      human approval.
- [ ] Python-like MDF code with variables, functions, conditionals and
      loops executes deterministically and round-trips with the editable
      document.
- [ ] AE-style editing features are covered except AE project import:
      keyframes and curves, masks, mattes, precomps, adjustment layers,
      expressions, effects and a basic node graph.
- [ ] Rendering is deterministic on the reference machine, verified by
      golden-frame tests; cross-machine differences are documented with
      tolerance.
- [ ] A project survives save/load round-trips with migrations and never
      loses work (autosave, recovery, backups).
- [ ] The exported MP4 is ready for manual upload to YouTube.

## Out of scope for 1.0
- Importing Adobe After Effects project files.
- Direct YouTube upload; the user downloads and uploads manually.
- 4K export, HDR and wide-gamut output.
- Color grading and advanced audio mastering.
- Rigid-body physics, cloth and fluids.
- macOS and Linux releases.
- Plugin API, marketplace and cloud rendering.
- Real-time multi-user collaboration.

None of these are permanently excluded; they are post-1.0 milestones.
