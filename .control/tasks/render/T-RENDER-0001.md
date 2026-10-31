---
id: T-RENDER-0001
titulo: Spike offscreen deterministic still frame
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

Prove the determinism contract end to end with a throwaway spike: load a
fixture project, evaluate one fixed frame, render it offscreen through
FerrousEngine and hash the output. Repeat the run and compare.

This also validates the engine host: device creation without a visible
window, a render target texture and a readback path. Record the settings
that produced the frame so the golden can be reproduced.

## Criterios de aceptacion
- [ ] A fixed frame renders offscreen without an open window.
- [ ] Two runs produce the same hash, or a diff within documented
      tolerance with the cause explained.
- [ ] The golden frame and its settings are committed to the test kit.
- [ ] Blockers in the engine host are reported with file-level detail.

## Notas del agente

