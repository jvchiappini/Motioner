# Project goals

## Objective
Build a deterministic, real-time motion-graphics editor where a human
and an AI agent can co-create professional 2D/3D videos from prompts and
code, with an After Effects-like timeline, and export frame-accurate
video from the same evaluation pipeline used for the preview.

## Success criteria
- [ ] A 30-second 1080p composition with animated 2D text/shapes and at
      least one 3D layer previews interactively (target 30+ fps at
      proxy quality on the reference machine) and exports an MP4 whose
      frames match the preview at the same times.
- [ ] An AI chat can create a new composition or modify an existing one
      from a natural-language prompt; every change is previewable,
      undoable and represented in readable, versionable code.
- [ ] Rendering is deterministic: the same project, time and settings
      produce identical frames across runs and machines, verified by
      golden-frame tests.
- [ ] The document model, evaluator, renderer, UI and AI layers stay
      decoupled: the renderer can be improved or swapped without
      rewriting the editor, and UI panels do not own business logic.
- [ ] A project survives save/load round-trips with schema migrations
      and never loses user work (autosave, recovery, backups).

## Out of scope
- Real-time multi-user collaboration.
- Web/mobile editor clients in v1 (the renderer may target web later).
- Full non-linear audio mixing/DSP suite; audio is limited to tracks,
  fades and muxing in v1.
- Plugin marketplace and third-party scripting sandbox in v1.
- Physics simulation, particles beyond simple emitters, and advanced
  VFX in v1.
