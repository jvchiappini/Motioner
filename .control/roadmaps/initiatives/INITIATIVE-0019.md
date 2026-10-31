---
id: INITIATIVE-0019
nombre: "GPU particles and rigid-body physics"
phase: PHASE-0004
estado: backlog
prioridad: media
tamano_estimado: large
depende_de: ["INITIATIVE-0010"]
creado: 2026-10-31
responsable: agente
---

# Initiative: GPU particles and rigid-body physics

## Description

Add particle systems and physics-based motion. GPU particles are in 1.0;
rigid-body physics comes after 1.0; cloth and fluids come later. The
simulation must be reproducible for export.

## Scope

### Deliverables
- GPU particle emitters with deterministic seeds.
- Forces, lifetimes, sprites, trails and basic collision.
- Rigid-body physics after 1.0, with a proposed Rapier backend.
- Fixed-timestep simulation and bake-to-keyframes for export.
- Simulation controls in the inspector and the node graph.

### Non-goals
- Cloth and fluids in 1.0.
- Destructive fracture and large-scale destruction.
- Distributed or cloud simulation.

## Technical approach

Particles run on the GPU with seeded randomness and fixed step. Physics
uses a fixed timestep; simulations can be baked into keyframes so preview
and export are deterministic. Same-machine determinism is required;
cross-machine differences follow the documented tolerance.

Key considerations:
- Seeded RNG in shaders or upload from the CPU.
- Budget caps on particle count and physics bodies.
- Baking cost and cache invalidation.

## Dependencies

- INITIATIVE-0010: 3D scene integration.
- Physics library spike after 1.0.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0006 | 3D layer composited over 2D with effects | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] GPU particles render deterministically from a seed.
- [ ] Particle budgets hold on the reference machine.
- [ ] Rigid bodies simulate and bake reproducibly after 1.0.
- [ ] Cloth and fluids remain explicitly out of scope until later.

## Notes

Physics is staged: particles in 1.0, rigid bodies after, cloth/fluids
later. Do not let simulation work block the 1.0 release.
