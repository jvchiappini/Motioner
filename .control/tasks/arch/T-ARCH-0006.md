---
id: T-ARCH-0006
titulo: Set up CI, format and lint gates
estado: in_progress
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

Set up automated checks so the tree stays healthy from the first commit:
formatting, clippy with warnings denied, unit tests and a workspace build
on Windows and Linux. GPU-dependent golden tests run separately so CI
does not require a real GPU.

Include dependency caching and a clear failure report. Enforce the
engineering conventions in REF-0002 where a machine can check them:
rustdoc warnings, unused dependencies, clippy warnings denied and the
workspace lints. Duplication is reviewed in code review, not assumed.

## Criterios de aceptacion
- [ ] CI runs fmt, clippy, rustdoc, tests and build on Windows and Linux.
- [ ] A dependency-boundary check runs in CI.
- [ ] Unused dependencies and rustdoc warnings fail the build.
- [ ] The pipeline is cached and finishes in a reasonable time.

## Notas del agente

CI added on 2026-10-31 in `.github/workflows/ci.yml`: Windows and Linux,
fmt, boundary check, clippy, tests, build and rustdoc with warnings
denied. The first run passed on both operating systems in about 40
seconds each (run 41199553252). The unused-dependency gate
(`cargo machete`) was added after that run and needs a final green run.

