---
id: INITIATIVE-0001
nombre: "Architecture decisions and contracts"
phase: PHASE-0001
estado: in_progress
prioridad: critica
tamano_estimado: medium
depende_de: []
creado: 2026-10-31
responsable: agente
---

# Initiative: Architecture decisions and contracts

## Description

Record and approve the decisions that shape every later line of code:
what owns the truth of a project, how time and rendering stay
deterministic, which UI toolkit builds the editor, how the engine is
pinned and modified, how projects and assets are stored, and how the AI
is allowed to mutate a document.

The output is a small set of ADRs plus the living system blueprint in
`docs/reference/REF-0001`. The initiative is complete when the user has
approved the decisions or replaced them with alternatives.

## Scope

### Deliverables
- ADR set D-0001..D-0009 with context, decision and consequences.
- System blueprint covering model, memory, rendering, AI and UI.
- Glossary of domain terms used across the project.
- Explicit non-goals and risk register.

### Non-goals
- Writing production code.
- Choosing libraries that are trivial to replace later.
- Detailed API documentation of code that does not exist yet.

## Technical approach

Decisions are recorded as ADRs and linked from phases and initiatives.
The blueprint is the single narrative reference; ADRs are the atomic
decisions. Any later change that contradicts an ADR requires a new ADR
that supersedes it.

Key considerations:
- Irreversible decisions first: source of truth, UI toolkit, format.
- Reversible decisions are noted but not over-engineered.
- Every decision names the trade-off it accepts.

## Dependencies

- None.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0001 | Decision gate: architecture approved | | in_progress |

## Tasks

- T-ARCH-0001 - Review legacy Motioner for reusable ideas
- T-ARCH-0002 - Approve ADR set
- T-ARCH-0003 - Maintain system blueprint

## Acceptance criteria

- [ ] D-0001..D-0009 are reviewed and marked accepted, rejected or
      superseded.
- [ ] The blueprint reflects the accepted decisions.
- [ ] The user confirms the source-of-truth and UI-toolkit choices.

## Notes

Two decisions block most of the work: document-as-truth vs code-as-truth
(D-0011), and Ferrous UI vs egui (D-0004).
