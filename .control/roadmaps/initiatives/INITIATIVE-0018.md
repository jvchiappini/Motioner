---
id: INITIATIVE-0018
nombre: "Node graph compositing"
phase: PHASE-0004
estado: backlog
prioridad: alta
tamano_estimado: xlarge
depende_de: ["INITIATIVE-0011"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Node graph compositing

## Description

Add a node graph as an alternate compositing view over the same
document. Layers, effects, masks, mattes and merges can be expressed as
a graph without creating a second data model.

## Scope

### Deliverables
- Graph view with nodes for layers, effects, masks and merges.
- Bidirectional mapping between graph and layer stack.
- Node selection, wiring, grouping and navigation.
- Shared evaluation with the layer timeline.
- Basic compositing nodes in 1.0; advanced nodes later.

### Non-goals
- Replacing the layer timeline as the primary model.
- A separate scene format for the graph.
- Plugin nodes in 1.0.

## Technical approach

The graph is a projection of the same document: a layer stack with
effects is already a DAG. The graph view edits the same nodes and
commands; ordering and merge semantics are explicit and shared with the
compositor. Cycles are rejected.

Key considerations:
- One source of truth for compositing.
- Stable node identities across views.
- Large graphs need virtualization and layout persistence.

## Dependencies

- INITIATIVE-0011: compositing effects and color.
- D-0019 node graph decision.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0006 | 3D layer composited over 2D with effects | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] A composition can be edited in the layer stack and the graph
      without divergence.
- [ ] Graph edits are undoable and visible in code.
- [ ] Cycles and invalid connections are rejected clearly.
- [ ] The graph stays responsive with hundreds of nodes.

## Notes

The graph is a view, not a second model. If it becomes a separate model,
the project gains two sources of truth.
