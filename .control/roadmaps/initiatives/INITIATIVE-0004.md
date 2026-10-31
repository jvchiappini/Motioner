---
id: INITIATIVE-0004
nombre: "Document model and undo"
phase: PHASE-0002
estado: backlog
prioridad: alta
tamano_estimado: large
depende_de: ["INITIATIVE-0003"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Document model and undo

## Description

Design and implement the in-memory representation of a Motioner project
and the only mechanism allowed to change it: typed commands grouped into
transactions with undo/redo. This is the foundation for the editor, the
AI and the code layer; all three must mutate the document through the
same command surface.

## Scope

### Deliverables
- Typed document model: project, composition, node hierarchy, layer
  content, animatable properties, effects, assets, precomps.
- Stable persistent IDs and cheap runtime handles.
- Command set for structural and property edits, with inverses.
- Transaction grouping, undo/redo stack and coalescing.
- Autosave, recovery journal and backup policy.
- Schema version field and migration framework.

### Non-goals
- Timeline sampling and animation (INITIATIVE-0005).
- Renderer data structures.
- UI presentation of the document.

## Technical approach

Nodes live in generational arenas; persistent IDs are independent of
memory slots. Undo stores commands and inverse commands, not full
document copies; periodic lightweight snapshots bound recovery cost.
Immutable snapshots via reference-counted document versions allow
background jobs and autosave to read a consistent state.

Key considerations:
- Property storage should be cache-friendly and avoid per-property
  heap allocation.
- Commands must validate preconditions and return typed errors.
- Serialization order must be deterministic for diffable files.

## Dependencies

- INITIATIVE-0003: project format and determinism contract.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0001 | Decision gate: architecture approved | | in_progress |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] Structural and property edits undo and redo exactly.
- [ ] Save/load round-trip is lossless and deterministic.
- [ ] A crash during editing can be recovered from autosave.
- [ ] The command surface is documented for UI and AI use.

## Notes

If the AI or the code layer bypasses commands, undo, validation and
audit become impossible. Enforce a single mutation path from day one.
