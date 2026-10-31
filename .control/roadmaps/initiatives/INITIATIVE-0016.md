---
id: INITIATIVE-0016
nombre: "NLE timeline and media editing"
phase: PHASE-0003
estado: backlog
prioridad: alta
tamano_estimado: xlarge
depende_de: ["INITIATIVE-0009"]
creado: 2026-10-31
responsable: agente
---

# Initiative: NLE timeline and media editing

## Description

Add non-linear editing so Motioner is not only a motion-graphics tool
but also an editor for real footage. The NLE timeline shares the
composition model and the evaluator with motion graphics: a video or
audio clip is another time-based layer, edited with frame-accurate
commands.

## Scope

### Deliverables
- Media import for video and audio with metadata and proxies.
- Timeline operations: cut, trim, slip, slide, ripple, split and
  reorder.
- Transitions between clips with a small curated set in 1.0.
- Frame-accurate seeking, playback and export.
- Audio/video sync and drift detection.
- NLE-specific shortcuts and snapping.

### Non-goals
- Color grading (post-1.0).
- Multicam editing in 1.0.
- Advanced compositing; the node graph covers that separately.

## Technical approach

Media is decoded through ffmpeg into a bounded cache; proxies are
generated for scrubbing. Every edit is a command. The timeline resolves
clips to frame ranges and feeds the same evaluator as motion graphics.
Audio and video are decoded on worker threads with cancellation.

Key considerations:
- Frame accuracy at 23.976, 25, 29.97 and 60 fps.
- Memory budget on 8 GB RAM: bounded decode cache and proxies.
- Never block the UI thread.

## Dependencies

- INITIATIVE-0009: 2D compositor and export.
- INITIATIVE-0017: audio playback and mixing.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0009 | 1.0 release candidate | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] Video and audio clips can be imported, cut, trimmed and arranged.
- [ ] Playback and export are frame-accurate and in sync.
- [ ] Proxies keep scrubbing usable on the reference machine.
- [ ] Every NLE edit is undoable and visible in code.

## Notes

NLE and motion graphics share one timeline and one evaluator; they are
not separate applications.
