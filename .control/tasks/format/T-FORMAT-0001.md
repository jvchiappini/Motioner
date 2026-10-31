---
id: T-FORMAT-0001
titulo: Specify project format v0
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

Specify the on-disk project as described in D-0005 and REF-0001 section
8: folder layout, metadata, composition files, content-addressed asset
store, manifest, cache and backup folders, packed archive, format
version and migration rules.

Define the Motioner Document Format (MDF) v0 grammar and its mapping to
the document AST. The format is native, text-based and shared with the
code layer; it is not RON or JSON. Write down deterministic ordering
rules. No implementation starts before the spec is reviewed.

## Criterios de aceptacion
- [ ] The folder layout and every file's schema are documented.
- [ ] MDF v0 grammar and its document AST mapping are specified.
- [ ] Asset hashing, deduplication and relinking rules are defined.
- [ ] Migration and forward-compatibility rules are documented.

## Notas del agente

