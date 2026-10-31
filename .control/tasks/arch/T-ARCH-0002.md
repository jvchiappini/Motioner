---
id: T-ARCH-0002
titulo: Approve ADR set and resolve open decisions
estado: done
prioridad: critica
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

Decisions D-0001 to D-0009 were recorded as proposals except D-0001.
Implementation must not start until the blocking decisions are resolved:
D-0003 (document as truth), D-0004 (UI toolkit) and D-0005 (format).
D-0002, D-0006, D-0007, D-0008 and D-0009 also shape the crate layout.

The user confirmed all three blockers on 2026-10-31 and asked to
continue. The remaining items are implementation details, not blocking
architecture.

## Criterios de aceptacion
- [x] Every ADR has a final state and no `propuesta` remains blocking.
- [x] The user explicitly confirms D-0003, D-0004 and D-0005.
- [x] REF-0001 and the roadmap reflect the final decisions.

## Notas del agente

Confirmed by the user:
- D-0003: the document is the truth and code is a lossless projection.
- D-0004: engine-native Ferrous UI plus GUIMaker; no egui.
- D-0005: a Motioner-owned native format is preferred.

Consequences recorded: MDF replaces RON/JSON; GUIMaker is a separate
project that will be pinned; the blueprint and roadmap were updated.

