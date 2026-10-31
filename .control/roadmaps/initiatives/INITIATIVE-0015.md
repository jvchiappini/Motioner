---
id: INITIATIVE-0015
nombre: "Caching, render queue and performance"
phase: PHASE-0005
estado: backlog
prioridad: alta
tamano_estimado: large
depende_de: ["INITIATIVE-0014"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Caching, render queue and performance

## Description

Make long compositions and professional workflows practical: cache
evaluated frames and rendered layers, render in the background with
progress and recovery, and meet the documented performance budgets.

## Scope

### Deliverables
- In-memory and on-disk frame cache keyed by content hashes.
- Layer, effect and precomp result caching with conservative
  invalidation.
- Proxy and draft modes for preview.
- Render queue with jobs, priority, pause/resume and logs.
- Background export isolated from UI responsiveness.
- Performance budgets, benchmarks and profiling tools.
- Memory and VRAM accounting with eviction policies.

### Non-goals
- Distributed or cloud rendering.
- GPU cluster scheduling.
- Automatic quality tuning based on machine learning.

## Technical approach

Cache keys combine document revision, frame, settings and asset
versions. Exports run as jobs on worker threads or a separate process;
the UI observes progress and can cancel. Budgets are enforced in
benchmarks, with clear regression thresholds in CI.

Key considerations:
- Correct invalidation is more important than hit rate.
- Avoid copying frames between CPU and GPU unnecessarily.
- Ensure cancellation frees GPU resources promptly.

## Dependencies

- INITIATIVE-0014: full feature set and AI.
- Mature render graph.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0008 | Thirty-second 1080p render with caching | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] A 30-second 1080p composition renders with a progress report.
- [ ] Cached scrubbing is visibly faster than uncached.
- [ ] Editing while rendering does not corrupt output.
- [ ] Benchmarks fail CI on agreed regressions.

## Notes

Performance work is not a final polish phase; it starts with budgets in
Phase 1 and is enforced through every phase.
