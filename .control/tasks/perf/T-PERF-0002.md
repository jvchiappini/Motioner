---
id: T-PERF-0002
titulo: Professional motion performance, docs and final report
estado: done
prioridad: baja
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-01
actualizado: 2026-11-03
depende_de: [T-APP-0003, T-PERF-0003]
bloqueado_por: null
version_schema: 1
---
Phase 12 closes the professional-motion job: release-build measurements,
documentation, task/backlog closure and the final report.

## Design

- Measure `render` ms/frame for both showcases and preview fps with the
  profiler overlay; record the numbers in `.control/CONTEXT.md` and the report.
- Docs: `spec/mdf-v0.md`, `docs/agents.md`, root README,
  `crates/motioner_ui/README.md`, `crates/motioner_render/README.md`.
- Backlog what is left (canvas point editing, audio, video, 3D, particles).
- Final report in Spanish: per phase, engine pins, ADRs, dependencies, goldens,
  screenshots, render paths, performance, gate results, free space, remaining
  work.

## Criterios de aceptacion
- [x] Release measurements recorded: showcase 7.778 ms/frame, Showcase Pro
      16.499 ms/frame (8-sample motion blur), preview 59 fps / 0 alloc/frame
- [x] Documentation updated (spec 7.5/7.6, agents, root and crate READMEs)
- [x] Tasks closed or backlogged (audio, NLE/video, 3D, particles, canvas
      point editing)
- [x] Spanish report delivered at `out/reports/PHASE-12-REPORT.md`

## Notas del agente

- Release build: `cargo build --release -p motioner_app -j 2` with
  `CARGO_INCREMENTAL=0`, 5m56s, 26.1 GB free on C: afterwards.
- `showcase.motion`: 720 frames in 8.079 s, render+readback 7.778 ms/frame,
  encode 3.065 ms/frame.
- `showcase_pro.motion`: 1440 frames in 26.621 s, render+readback
  16.499 ms/frame, encode 1.763 ms/frame.
- Preview: `showcase_pro.motion` at 1600x900, release with `alloc-count`,
  59 UI fps, eval 0.03 ms, 0 chrome allocations/frame.
- Gate: fmt, clippy pedantic `-D warnings`, `cargo test --workspace`,
  rustdoc `-D warnings`, boundaries, machete and `pctl validate` green.
- The engine clone commit `e482b3c` still needs the submodule pointer commit by
  the reviewer.
