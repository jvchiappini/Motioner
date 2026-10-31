---
id: INITIATIVE-0013
nombre: "Program-first MDF language"
phase: PHASE-0005
estado: backlog
prioridad: critica
tamano_estimado: xlarge
depende_de: ["INITIATIVE-0012"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Program-first MDF language

## Description

Deliver the code side of Motioner: a Python-like executable language
with variables, functions, conditionals, loops and LaTeX that builds a
composition when it runs. The program is the source of truth; the
document is a compiled result with provenance, plus structured
overrides for editor edits.

## Scope

### Deliverables
- Python-like grammar and AST, including `def`, `if`, `for`, `while`,
  expressions, collections and calls.
- Scene API for compositions, layers, properties, keyframes, effects,
  assets, precomps and LaTeX math.
- Deterministic sandboxed interpreter with fuel, time and cost limits.
- Provenance from compiled elements back to source spans.
- Override and bake model for UI edits, with orphan detection.
- Formatter, diagnostics with spans and a code panel with highlighting.
- Grammar, round-trip, property and fuzz test suites.

### Non-goals
- Embedded CPython or access to Python packages in 1.0.
- Arbitrary filesystem, network or process access.
- Full LaTeX documents; math typesetting only.
- A general-purpose programming language outside the scene domain.

## Technical approach

MDF is program-first (D-0011). Running a program produces a document
snapshot with provenance. The editor displays that snapshot; edits
become overrides keyed by stable identity or bake an element, and are
written back to the program. Execution is deterministic: no ambient
time, no unseeded randomness, no I/O, bounded resources.

Key considerations:
- The language and the on-disk format are the same.
- Stable identity and source spans survive formatting.
- LaTeX rendering is integrated through the text pipeline (D-0016).
- Comments and formatting are preserved for human-authored code.

## Dependencies

- INITIATIVE-0012: complete document feature set.
- D-0011, D-0016 and D-0009 approved.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0007 | AI creates a composition from a prompt | | backlog |

## Tasks

- T-DSL-0001 - Review and freeze the MDF v0 specification
- T-DSL-0002 - Define provenance, override and bake semantics

## Acceptance criteria

- [ ] Python-like programs with loops and functions build compositions.
- [ ] Execution is deterministic, sandboxed and resource-limited.
- [ ] Provenance and overrides survive edit, save and reload.
- [ ] LaTeX math renders through the text pipeline.
- [ ] Invalid code produces actionable diagnostics with spans.

## Notes

This is the highest-risk initiative. Prototype the language, provenance
and override model on a small scene before editor polish.
