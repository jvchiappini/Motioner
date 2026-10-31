---
id: T-ARCH-0007
titulo: Run GUIMaker widget gap analysis for the editor
estado: backlog
prioridad: alta
tipo: investigacion
creado_por: agente
asignado_a: agente
creado: 2026-10-31
actualizado: 2026-10-31
depende_de: []
bloqueado_por: null
version_schema: 1
---

## Contexto

D-0004 is accepted: the editor uses engine-native Ferrous UI plus
GUIMaker, with no egui. GUIMaker is a separate project whose updated
repository will be provided.

Inventory what the editor needs and what GUIMaker already provides:
docking with split resize, virtualized timeline rows with drag
reordering, a bezier keyframe graph, a syntax-highlighted code editor,
color pickers and context menus. Prototype the riskiest gaps, then
record which widgets belong upstream in GUIMaker and which stay in
`motioner_ui`.

## Criterios de aceptacion
- [ ] Widget inventory with a gap list and a prototype for each risk.
- [ ] Contribution boundary between GUIMaker and `motioner_ui` is defined.
- [ ] GUIMaker revision is pinned and D-0004 records the integration
      details.

## Notas del agente

2026-10-31 (S-2026-10-31-009): GUIMaker inspected via GitHub API (head
9dcbc28). It is a designer binary with code generation, not a library;
it imports the removed `ferrous_app` crate, so it does not build against
engine 02e6d48. First upstream fix: port it to the current engine API.
D-0004 and REF-0001 section 3 must be corrected (motioner_ui depends on
the engine UI crates, GUIMaker generates code for them). See CONTEXT.
