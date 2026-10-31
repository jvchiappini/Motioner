---
id: T-ARCH-0004
titulo: Scaffold the Cargo workspace and crate skeleton
estado: backlog
prioridad: alta
tipo: chore
creado_por: agente
asignado_a: agente
creado: 2026-10-31
actualizado: 2026-10-31
depende_de: []
bloqueado_por: null
version_schema: 1
---

## Contexto

Create the Cargo workspace and only the crates Phase 1 needs:
`motioner_core`, `motioner_eval`, `motioner_render`, `motioner_io`,
`motioner_app` and `motioner_testkit`. Add UI, AI and DSL crates when
their phases start.

Enforce the dependency direction described in REF-0001 section 3: the
core crate must not depend on wgpu, FerrousEngine, UI or AI. Add a
lightweight check that fails the build if the core dependency graph is
violated.

## Criterios de aceptacion
- [ ] `cargo build` and `cargo test` pass on a clean checkout.
- [ ] The core crate has no graphics, UI or AI dependency.
- [ ] Each crate has a one-line purpose in its README or lib docs.

## Notas del agente

