---
id: INITIATIVE-0006
nombre: "Headless render harness"
phase: PHASE-0002
estado: backlog
prioridad: media
tamano_estimado: medium
depende_de: ["INITIATIVE-0005"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Headless render harness

## Description

Build the test infrastructure that lets Motioner render without an open
editor window: a small headless binary and a test kit that produce frames
from fixtures and compare them against golden images. This is how
rendering regressions are caught before they reach the UI.

## Scope

### Deliverables
- Headless render entry point: project file, frame range, output folder.
- Fixture projects covering 2D shapes, text, transforms and color.
- Golden-frame comparison with documented tolerance and diff images.
- CPU-side evaluation oracle for animation correctness.
- Criterion benchmarks for evaluation and frame rendering.

### Non-goals
- Video encoding.
- Visual UI testing.
- Production render queue.

## Technical approach

Use an offscreen surface at fixed resolution and settings. Compare
frames by perceptual or channel tolerance, not only exact bytes, and
store diff artifacts on failure. Keep GPU-dependent tests separate from
unit tests so CI can run the deterministic subset without a GPU.

Key considerations:
- Pin the engine and driver assumptions in the test documentation.
- Record the exact settings that produced each golden frame.
- Update goldens only through a reviewed command.

## Dependencies

- INITIATIVE-0005: evaluator and render plan.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0003 | Ten-second 2D composition animates headlessly | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] Golden tests run on demand and report diffs clearly.
- [ ] The deterministic subset runs in CI.
- [ ] Benchmarks track evaluation time per frame.
- [ ] Regressions in transform or color are detected.

## Notes

This harness becomes the safety net for every later rendering change.
Invest early and keep the fixtures small and readable.
