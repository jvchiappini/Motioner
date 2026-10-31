---
id: T-RENDER-0003
titulo: Upstream Pure2D text and pipelined headless readback in FerrousEngine
estado: done
prioridad: critica
tipo: feature
creado_por: usuario
asignado_a: agente
creado: 2026-10-31
actualizado: 2026-10-31
depende_de: []
bloqueado_por: null
version_schema: 1
---
## Contexto

User decision (2026-10-31): land engine improvements before the Motioner
render adapter so Motioner does not duplicate text or readback code. Work
happens in the dev clone `C:/Users/jvchi/CARPETAS/FerrousEngine` on branch
`feat/pure2d-text-and-readback`, driven by
`.control/prompts/opencode/opencode-prompt-05-engine-text-and-readback.md`.
After review the branch is committed and pushed, then the Motioner pin is
bumped with a new ADR (procedure in `engine/README.md`) and prompt 04 is
revised to use the new APIs.

## Criterios de aceptacion
- [x] Headless readback reuses staging buffers and returns typed errors (overlap of the two-slot ring moved to T-RENDER-0004).
- [x] Pure2D draws MSDF text with font metrics, kerning, alignment, transforms and z-interleaving.
- [x] GUI text shares the MSDF shader code and is unchanged.
- [x] Zero warnings in ferrous_font, ferrous_2d and ferrous_renderer; workspace and wasm checks pass; engine CI added.
- [x] Branch reviewed, committed and pushed; Motioner pin bumped with an ADR.

## Notas del agente

2026-10-31: reviewed and landed as FerrousEngine `32aea8a` on branch
`feat/pure2d-text-and-readback` (pushed, not merged to engine main).
Gates green: clippy -D warnings on the three crates, all tests (also with
`--no-default-features`), workspace check and wasm32 check. Motioner pin
bumped (D-0019); Motioner build, clippy and 91 tests green. Review
findings that do not block the first render are tracked in T-RENDER-0004.

