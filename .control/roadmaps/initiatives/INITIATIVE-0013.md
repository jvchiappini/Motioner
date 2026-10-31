---
id: INITIATIVE-0013
nombre: "Bidirectional motion language"
phase: PHASE-0005
estado: backlog
prioridad: critica
tamano_estimado: xlarge
depende_de: ["INITIATIVE-0012"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Bidirectional motion language

## Description

Deliver the code side of Motioner: a readable language that fully
describes a composition and can be edited by a human or an AI, while
remaining a lossless projection of the document. Round-trip fidelity and
actionable diagnostics are the hard requirements.

## Scope

### Deliverables
- Grammar and AST for compositions, layers, properties, keyframes,
  effects, assets and precomps.
- Parser, printer and formatter with stable output.
- Validator with precise spans and error messages.
- Document-to-code and code-to-document conversion.
- Sandboxed expression and generator runtime with fuel and time limits.
- Editor code panel with syntax highlighting, diagnostics and go-to
  definition.
- Round-trip and fuzz test suites.

### Non-goals
- General-purpose programming language.
- Arbitrary filesystem or network access from scripts.
- Full IDE features such as refactoring across files.

## Technical approach

The document remains the source of truth. The language is generated
deterministically from the document and parsed back into document
commands; semantic equality is asserted by tests, not by textual
equality. Formatting is idempotent. Scripts are compiled to a
deterministic bytecode or interpreted with limits and no I/O.

Key considerations:
- Stable ordering and IDs so diffs stay small.
- Preserve enough structure to map diagnostics to source spans.
- Decide early whether user comments survive round-trips; if not,
  document the limitation.

## Dependencies

- INITIATIVE-0012: complete document feature set.
- D-0003 and D-0009 approved.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0007 | AI creates a composition from a prompt | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] Every supported document round-trips without semantic loss.
- [ ] Formatting is idempotent and diffs are stable.
- [ ] Invalid code produces actionable diagnostics with spans.
- [ ] Scripts cannot perform I/O or exceed resource limits.

## Notes

This is the highest-risk initiative. Prototype the round-trip on the
full document model before investing in editor polish.
