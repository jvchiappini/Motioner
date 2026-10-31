# Project goals

## Objective
Build a deterministic, real-time motion-graphics editor and NLE where a
human and an AI agent co-create professional 2D/3D videos from prompts,
code and timeline edits, with automatic AI iteration, full audio mixing,
a node graph, and frame-accurate export including direct YouTube upload.

The north star is After Effects-level control and parity, except for
importing After Effects projects. The 1.0 staging is proposed in
`docs/reference/REF-0003.md` and awaits confirmation.

## Success criteria
- [ ] A 30-second 1080p composition with 2D/3D layers, text, shapes,
      images, video and audio previews at 60 fps on the reference
      machine (full quality, proxy only while scrubbing), and exports an
      MP4 and a PNG sequence from the same evaluation pipeline.
- [ ] NLE editing works: import, cut, trim and arrange video and audio on
      a timeline; audio tracks have volume, pan, fades and an effects
      chain; playback and export stay in sync.
- [ ] 3D supports glTF models, cameras, lights, PBR, skeletal animation,
      GPU particles and physics, composited with 2D layers.
- [ ] An AI prompt creates or edits a composition, renders previews,
      critiques them and iterates automatically until the result meets
      the request, within a cost and time budget, with every change
      undoable and visible as code.
- [ ] The full composition is readable and editable as MDF code, with
      lossless round-trips between code, document and disk.
- [ ] AE-style editing features are covered except AE project import:
      keyframes and curves, masks, mattes, precomps, adjustment layers,
      expressions, effects, node graph.
- [ ] Rendering is deterministic on the reference machine, verified by
      golden-frame tests; cross-machine differences are documented with
      tolerance.
- [ ] A project survives save/load round-trips with migrations and never
      loses work (autosave, recovery, backups).
- [ ] Videos can be published to YouTube from the application.

## Out of scope (confirmed)
- Importing Adobe After Effects project files.
- Real-time multi-user collaboration (not requested; revisit later).

## Deferred by the proposed 1.0 (pending confirmation)
- Advanced physics such as cloth and fluids.
- HDR output and wide-gamut workflows.
- macOS and Linux releases.
- Plugin API and marketplace.
- Cloud rendering.

Nothing in this section is permanently excluded; the north star remains
full parity and maximum capability.
