---
id: INITIATIVE-0008
nombre: "Viewport, timeline and real-time preview"
phase: PHASE-0003
estado: backlog
prioridad: alta
tamano_estimado: large
depende_de: ["INITIATIVE-0007"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Viewport, timeline and real-time preview

## Description

Make the editor feel alive: a viewport texture fed by the renderer,
selection and manipulation in the canvas, a scrubbable timeline with
keyframe editing, and a playback loop that keeps interaction responsive.

## Scope

### Deliverables
- Viewport rendering to a texture, displayed inside the editor panel.
- Pan, zoom, fit, proxy resolution and quality toggle.
- Picking and selection via ID buffer; transform gizmos for 2D and 3D.
- Timeline: virtualized layer rows, playhead, scrub, in/out trim,
  reordering, snap and markers.
- Keyframe editor: selection, drag, copy, interpolation and handles.
- Playback engine with frame skipping under load.
- Thumbnails and frame cache for scrubbing.

### Non-goals
- Final quality rendering.
- Audio playback.
- Multi-user editing.

## Technical approach

The viewport renders the composition at proxy resolution into an
offscreen texture and composites it in the UI. Picking reads an ID
buffer, not scene iteration. Playback requests frames from the evaluator
and drops frames when the budget is exceeded instead of blocking input.
All edits remain commands.

Key considerations:
- Never block the UI thread on asset decode or AI.
- Keep the evaluated frame and displayed frame explicitly versioned.
- Separate preview settings from export settings.

## Dependencies

- INITIATIVE-0007: shell and panel hosting.
- INITIATIVE-0005: evaluator.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0004 | Interactive editor preview at target fps | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] Scrubbing and playback stay responsive at proxy quality.
- [ ] Selection and transforms work from the viewport.
- [ ] Keyframes can be created, moved and eased in the timeline.
- [ ] The preview meets the fps target on the reference machine.

## Notes

Real-time is a budget discipline: measure early, cap quality by default,
and let the user opt into higher fidelity.
