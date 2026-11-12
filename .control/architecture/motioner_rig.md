# Domain: motioner_rig

## Purpose

Headless rig semantics: typed custom properties, expression drivers, poses,
clips, procedural layers, aim constraints and state machines evaluated as a
pure function of (rig, frame, keyed inputs). The DSL uses it to bake the
document at load; a future player uses the same functions. No file system,
no clock, no GPU and no editor dependency.

## Key components

- `Rig` - parts, properties, channels, mirrors, drivers, poses, tracks,
  clips, plays, aims, blinks and the optional machine:
  `crates/motioner_rig/src/lib.rs:487-530`.
- Typed property values and blending per type (lerp, shortest-arc for
  angles, linear-light colours, step for bool/enum):
  `crates/motioner_rig/src/lib.rs:88-170`.
- `PropertyDef` with range, step, enum values and the angle flag:
  `crates/motioner_rig/src/lib.rs:174-240`.
- `Channel::sample` holds outside the key range and eases inside it with the
  one easing implementation from `motioner_eval`:
  `crates/motioner_rig/src/lib.rs:236-280`.
- `Rig::sample_properties` resolves the keyed base, applies pose tracks in
  start order (each blend starts from the previous tracks' value), then clip
  plays: `crates/motioner_rig/src/lib.rs:592-660`.
- `Expr` / `Driver` / `Field` - the sandboxed per-frame IR:
  `crates/motioner_rig/src/driver.rs:34-242`.
- `machine::Runner` - sequential state stepping, blend progress, trigger
  edges and events: `crates/motioner_rig/src/machine.rs:135-235`.
- `bake` - one pass per integer frame: stateless properties, machine,
  layers (blinks), drivers, aims; typed errors on non-finite values or
  budget overruns: `crates/motioner_rig/src/eval.rs:95-160`.
- `evaluate` - replay from the first frame, the documented seek rule:
  `crates/motioner_rig/src/eval.rs:162-175`.

## Diagram

Not needed: a linear pipeline per frame (properties, machine, layers,
drivers, constraints).

## Relevant decisions

- D-0052 - bake-on-build; the rig is evaluated at load and written as keys.
- D-0053 - poses, clips, layers and state machine semantics.
- D-0055 - this crate, dependencies and the headless contract.

## Boundaries

May depend on `motioner_core` and the pure `motioner_eval` (for easings).
Must not depend on the DSL, I/O, renderer, UI, tools or the application.

## Known gaps

Bones/skinning/mesh/blend shapes/IK/path drivers, blend states 1D/2D and the
wasm/binary player format are backlogged (T-RIG-0013, T-RIG-0015).


