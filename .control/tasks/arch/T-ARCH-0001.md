---
id: T-ARCH-0001
titulo: Review legacy Motioner for reusable ideas
estado: done
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

The legacy `jvchiappini/Motioner` repository (last pushed 2026-02) is a
Rust/egui prototype with a DSL (lexer, parser, evaluator, generator), a
code panel, a timeline and frame export. It predates the FerrousEngine
decision and uses egui 0.26 with wgpu 0.19. It is not the codebase we are
starting, but it contains prior art for the language and timeline.

Review it read-only, inventory the modules and decide, per module,
whether it informs the new design, can be ported, or should be discarded.
Record the result in this task and reference it from the blueprint.

## Criterios de aceptacion
- [x] Module inventory with file-level references and a one-line purpose.
- [x] Per-module recommendation: adopt, port with changes, or discard.
- [x] Risks of porting versus rewriting are recorded in the agent notes.

## Notas del agente

Inventory from the legacy repository tree and README (read-only; the
repository is being replaced and will not be ported):

- `src/dsl/` (ast, lexer, parser, validator, evaluator, generator,
  runtime): a complete animation DSL with a code generator.
  Recommendation: adopt the separation of grammar, AST, validation and
  evaluation as prior art; rewrite for MDF and the new document model.
- `src/code_panel/` (autocomplete, gutter, highlighter, minimap):
  reference for the code panel UX; rewrite on GUIMaker.
- `src/timeline/`, `src/scene.rs`, `src/scene_graph.rs`: reference for
  timeline and scene concepts; the document and runtime models replace
  them.
- `src/canvas/`, `src/ui.rs`, `src/modals/`: egui-specific; discard.
- `src/states/autosave.rs`, `src/states/dslstate.rs`: confirm the need
  for autosave and DSL state; rewrite.
- `src/logics/` (if/for): prior art for procedural logic; covered by
  D-0009.
- Dependencies: egui 0.26, wgpu 0.19, resvg/usvg; incompatible with the
  engine stack; discard.

Recommendation: adopt concepts, port no code. Risks of porting: a
different UI toolkit, renderer and document model make the old code a
liability; the only durable value is the DSL and code-panel ideas.

