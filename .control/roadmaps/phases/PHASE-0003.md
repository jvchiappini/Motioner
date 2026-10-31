---
id: PHASE-0003
nombre: "Editor UI and 2D motion graphics"
orden: 3
estado: not_started
inicio: 2026-10-31
fin_estimado: ""
fin_real: null
depende_de: ["PHASE-0002"]
responsable: agente
prioridad: alta
---

# Phase: Editor UI and 2D motion graphics

## Summary

Turn the headless core into an editor a person can use: an After
Effects-style shell with a project panel, composition viewport, timeline,
inspector and transport controls. On the rendering side, deliver the 2D
motion-graphics feature set (shapes, text, images, solids, basic effects)
and the first MP4 export.

The phase ends when a 10-second 1080p 2D composition can be authored in
the UI, previewed interactively and exported to a video that matches the
preview.

## Goals

- Provide an editor layout that scales to many panels and documents.
- Make timeline interaction (scrub, keyframe, trim, reorder) feel
  immediate.
- Render 2D layers correctly with typography, alpha and blend modes.

## Scope

### In scope
- Editor shell, docking, panels, command palette, keyboard shortcuts.
- Viewport with pan/zoom, selection, snapping and picking.
- Timeline with layers, keyframes, easing editor and playhead.
- 2D layers: solid, shape, path, text, image, precomp placeholder.
- Basic effects: transform, opacity, blur, glow, color correction.
- MP4 export via ffmpeg from the same evaluated frames.

### Out of scope
- 3D layers and PBR lighting.
- AI chat and the text language.
- Advanced masks, track mattes and motion blur.

## Dependencies

- PHASE-0002: document, evaluator and headless harness.

## Risks and mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Custom UI toolkit lacks needed widgets | High | High | Decide D-0004 early; build a widget spike before panels |
| Timeline becomes a performance bottleneck | Medium | High | Virtualized rows, cached layout, redraw only dirty regions |
| GPU/UI devices conflict | Medium | High | One renderer/device; composite the viewport as a texture |
| Typography quality varies | Medium | Medium | MSDF for preview, path/outline option for final render |

## Key decisions

- D-0004: editor UI toolkit.
- D-0006: document/runtime separation.
- D-0007: commands as the only mutation path.

## Success criteria

- [ ] A user can create a comp, add and animate 2D layers, scrub and
      play the preview at proxy quality.
- [ ] Selection and edits from the UI go through commands and are
      undoable.
- [ ] Exported MP4 frames match the preview frames within the agreed
      tolerance.
- [ ] The editor opens a project and restores its workspace layout.

## Initiatives

- INITIATIVE-0007 - Editor shell and panels
- INITIATIVE-0008 - Viewport, timeline and real-time preview
- INITIATIVE-0009 - 2D compositor, typography and MP4 export

## Notes

This is the first phase where the product becomes demoable. Avoid
expanding into 3D or AI until the 2D vertical slice is solid.
