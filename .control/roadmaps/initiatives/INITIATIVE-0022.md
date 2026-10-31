---
id: INITIATIVE-0022
nombre: "LaTeX and math typesetting"
phase: PHASE-0003
estado: backlog
prioridad: alta
tamano_estimado: large
depende_de: ["INITIATIVE-0009"]
creado: 2026-10-31
responsable: agente
---

# Initiative: LaTeX and math typesetting

## Description

Render LaTeX math in text layers so Motioner can produce Manim-class
educational and scientific animation. The renderer must be deterministic,
self-contained and integrated with the existing text and vector
pipeline.

## Scope

### Deliverables
- LaTeX math parser and layout for inline and display expressions.
- Vector output compatible with the MSDF/path rendering pipeline.
- Coverage for common symbols, fractions, roots, sums, integrals,
  matrices, accents, alignment and common fonts.
- Animatable math elements and per-part styling.
- Diagnostics for unsupported commands.

### Non-goals
- Full LaTeX documents, packages or arbitrary macros.
- A system LaTeX dependency at runtime.
- Chemistry, TikZ or advanced diagram packages in 1.0.

## Technical approach

Prefer a Rust-native LaTeX math renderer that produces outlines. If a
native renderer cannot cover the required command set, bundle a
deterministic JS/MathJax fallback. Math is laid out once and cached;
animation transforms the rendered vector elements.

Key considerations:
- Font coverage and metric compatibility.
- Cache keyed by expression, style and font revision.
- Error spans mapped back to the MDF source.
- No shelling out to a system TeX installation.

## Dependencies

- INITIATIVE-0009: typography and font pipeline.
- D-0016 approved.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0005 | MP4 export matches preview | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] Common LaTeX math renders correctly in preview and export.
- [ ] Math is animatable and styleable with the rest of the text layer.
- [ ] Unsupported commands produce clear diagnostics.
- [ ] Rendering is deterministic and self-contained.
- [ ] The supported command set is documented.

## Notes

This initiative is what makes the Manim comparison credible, but it is
not a full TeX distribution.
