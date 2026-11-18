---
id: T-PERF-0005
titulo: FerrousEngine to the byte: measured, zero-waste, idle-free engine (prompt 18)
estado: in_progress
prioridad: alta
tipo: feature
creado_por: usuario
asignado_a: agente
creado: 2026-11-17
actualizado: 2026-11-17
depende_de: []
bloqueado_por: null
version_schema: 1
---
## Contexto

User rule (2026-11-17): FerrousEngine must be as optimized as possible —
own algorithms where they win, every byte owned and justified, nothing
drawn when nothing changed. Motioner follows; the engine is held higher.

Outside startup (prompt 17) nothing is measured: one bench, no allocation
accounting, no per-stage frame timing, no GPU timing, no memory inventory.
Reviewer measurement, start screen idle 10 s (release, RTX 4050 laptop):
797 ms CPU per 10 s (~8 % of a core), working set 336 MB, private 457 MB,
45 threads. Motioner redraws at 60 fps when idle (no idle policy set).
Known per-frame waste: `GuiBatch::new()` per frame, `draw_text_baseline`
Vec per call, SVG vertices re-baked every frame in `GuiRenderer::prepare`,
Motioner `draw_image` mesh Vecs and `MeshSample::indices()` per frame,
`Mesh::sample` cloning UVs.

Plan: `.control/prompts/opencode/opencode-prompt-18-engine-performance.md`
(phases P0–P7). Supersedes T-PERF-0001.

## Criterios de aceptacion
- [ ] P0: alloc-stats allocator, frame stage trace, GPU timestamps, memory
      inventory, per-subsystem benches, `scripts/perf_bench.py` scenarios,
      baseline report with top-20 costs.
- [ ] P1: on-demand redraw policy; idle start screen and editor ≤ 0.5 % of a
      core, 0 frames and 0 GPU submits in 10 s idle; threads audited.
- [ ] P2: 0 heap allocations per steady-state frame (engine UI, Pure2D,
      Motioner editor and playback), enforced by tests.
- [ ] P3: compact vertex/instance formats with identical goldens, staging
      ring, smallest texture formats, CPU copies dropped, every cache with a
      byte budget; idle private bytes at least halved.
- [ ] P4: incremental layout, retained commands, text/tessellation caches,
      measured own algorithms (kept only when they win).
- [ ] P5: draw calls/state changes/overdraw/partial redraw/present mode
      measured and optimised.
- [ ] P6: Motioner eval cache, render hot paths, virtualised lists,
      playback and export numbers.
- [ ] P7: budgets as tests, D-0063 performance policy, engine
      `performance.md`, final report in Spanish with before/after ledger.

## Notas del agente

- P0 (2026-11-17): instruments and baseline complete. Engine `23278a2`
  (not pushed; submodule local at `23278a2`), Motioner no commit. Report
  `out/reports/PHASE-18-P0-REPORT.md`. Baseline: start idle 11.85 % of a
  core / 601 frames / 114 allocs per frame; heavy UI 99.86 % / 15 459
  allocs per frame; playback 18.33 % / 2.83 MB per frame; showcase_pro
  748.7 MB private. Next: P1 (idle = zero).
