---
id: PHASE-0002
nombre: "Core document and animation engine"
orden: 2
estado: not_started
inicio: 2026-10-31
fin_estimado: ""
fin_real: null
depende_de: ["PHASE-0001"]
responsable: agente
prioridad: alta
---

# Phase: Core document and animation engine

## Summary

Build the headless heart of Motioner: the document model, the command and
undo system, the timeline and the deterministic evaluator that turns a
document plus a frame number into a render plan. This phase produces no
UI; it produces a library that can create, animate, save, load and
evaluate a composition from tests.

## Goals

- Make the document model expressive enough for real motion graphics:
  compositions, hierarchy, 2D/3D nodes, animatable properties, effects,
  assets and precomps.
- Guarantee lossless round-trips and safe evolution through schema
  migrations.
- Make evaluation pure, deterministic and fast enough to be called on
  every preview frame.

## Scope

### In scope
- Document model, stable IDs, animatable values and keyframes.
- Command transactions, undo/redo and autosave/recovery.
- Timeline semantics: in/out, time stretch, parenting, markers, local
  vs global time.
- Evaluator: resolve time, sample curves, propagate transforms, produce
  a flat render plan.
- Headless fixtures and golden render harness.

### Out of scope
- Interactive editing, panels and viewport.
- GPU effects and advanced compositing.
- AI generation and the text language.

## Dependencies

- PHASE-0001: workspace, project format and determinism contract.

## Risks and mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Data model too rigid for effects and 3D | Medium | High | Model node content as an open enum with versioned extensions |
| Keyframe evaluation drifts between preview/export | Medium | High | One evaluator, frame-quantized time, golden tests |
| Undo corrupts nested structures | Medium | High | All mutations through commands; snapshot tests for undo/redo |
| Document grows into a god module | Medium | Medium | Split core into store, schema, commands and eval crates when needed |

## Key decisions

- D-0002: deterministic frame evaluation.
- D-0003: document as source of truth.
- D-0006: document/runtime separation and caching model.

## Success criteria

- [ ] A 10-second composition with nested groups and keyframed
      transforms evaluates headlessly at every frame.
- [ ] Save/load round-trip is byte-stable for fixtures and survives
      migrations from v0.
- [ ] Undo/redo restores structural and property edits exactly.
- [ ] Evaluation allocates no per-frame heap memory beyond documented
      scratch buffers.

## Initiatives

- INITIATIVE-0004 - Document model and undo
- INITIATIVE-0005 - Timeline and animation evaluation
- INITIATIVE-0006 - Headless render harness

## Notes

The evaluator is the contract between the editor and the renderer. It
must never depend on the UI toolkit or on wall-clock time.
