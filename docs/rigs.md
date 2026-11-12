# Rigs, symbols and state machines

Motioner treats a character or interactive graphic the same way it treats
every other composition element: a program builds it, the document holds
ordinary nodes and keys, and the renderer draws it frame by frame. This
page is the reference for the import model, SVG symbols, rig components,
custom properties, drivers, poses, clips, layers and state machines, and
it records how the feature set compares with Rive and Lottie.

Status line: prompt 16 delivered the import model (D-0051), SVG symbols
(D-0054), rig components with typed properties and mirror groups
(D-0052), deterministic per-frame drivers compiled to an expression IR,
poses, clips and layers (D-0053), state machines keyed on the timeline,
the headless `motioner_rig` runtime (D-0055) and the mascot acceptance
example. Rive's deformers (bones, skinning, mesh, blend shapes, IK) and
its interactive player are backlogged; each row below says so. Prompt 16b E2
shipped rig write-through and the `rig_*` agent tools; the editor rig panel
and the wasm player remain tracked in T-RIG-0009 and T-RIG-0015.

Measured on the reference machine (release): mascot.svg import 0.10 ms;
the whole 15 s demo builds and bakes in 31 ms (901 frames, 0.035 ms per
frame, 4 081 transform keys); a 121-frame 1080p60 render of the demo takes
1.7 s including process start, GPU init and encode.

## Quick example

```python
art = use("assets/mascot.svg")            # a symbol definition
mascot = use("rigs/mascot.motion")        # a MDF module (namespace)

m = mascot.build(art, pos=(960, 540), scale=3)

m.pose("Feliz", at=1.0, dur=0.35, ease="cubic_out")
m.mouth_open.key(2.0, 13)
m.look.key(3.0, (0.6, -0.2))
m.play("salto", at=4.0, loops=2)
m.mood.key(6.0, "sad")                    # a state-machine input on the timeline
```

`use("x.motion")` returns a namespace whose functions build into the
caller's document (D-0051). A symbol is callable too:
`s = art(pos=(960, 540), scale=3)`.

The acceptance example is `examples/rig/mascot.motion` (the rig) and
`examples/rig/mascot_demo.motion` (15 s, 1080p60, all 17 presets, both
clips, the look pad, a blink layer and a mood machine). Render it with
`motioner render examples/rig/mascot_demo.motion --out mascot.mp4`.

## 1. One import model

`use(path)` dispatches on the file extension and returns a definition:

| Kind | Extensions | Call creates | Metadata |
|---|---|---|---|
| module | `.motion` | its functions build nodes | `functions` |
| symbol | `.svg` | a group with addressable parts | `width`, `height`, `parts` |
| image | `.png .jpg .jpeg .webp` | the same node as `image(key)` | `width`, `height` |
| audio | `.wav .flac .mp3 .ogg` | the same node as `audio(...)` | `duration` |
| video | `.mp4 .mov .mkv` | the same node as `video(...)` | `width`, `height`, `duration` |
| font | `.ttf .otf` | reserves the family | `family` |

- `image("key")`, `audio(...)` and `video(...)` remain valid sugar; the
  documents they produce are byte-identical to calling the definition.
- Resolution follows the project asset registry (spec 7.7): by key, then
  by original file name, then as a project-relative path. Absolute paths
  and `..` are rejected before any I/O.
- A re-run after a changed file rebuilds the document; parts keep their
  identity because ids, not indices, address them (hot reload).

## 2. SVG symbols

Any SVG in the supported subset becomes a symbol in `motioner_io` (no
dependency; D-0054):

- groups, `path` (all commands, arcs converted to cubics), `rect`,
  `circle`, `ellipse`, `line`, `polyline`, `polygon`;
- transforms (`translate`, `scale`, `rotate`, `matrix`, `skew`), with the
  geometry flattened to document coordinates at import;
- fill/stroke colours, width, caps, joins, miter limit, dashes,
  opacity and fill/stroke opacity;
- `vector-effect="non-scaling-stroke"`, inherited so the root or a group can
  opt a whole symbol in: strokes keep their authored width when a rig driver
  squashes the part (the mascot's mouth no longer collapses to a hairline);
- linear and radial gradients, `defs`/`use`, `clipPath` as a D-0033
  mask, `viewBox`/`preserveAspectRatio`;
- every element with an `id` (or an Inkscape/Figma label) is a part:
  `m.part("eye-L")`, `m.parts`, `m.part_names()`.

Unsupported features are reported with the element id; nothing is
dropped silently. A multi-subpath `d` becomes numbered child parts under
the same id.

A part is a real node: it can be keyed, masked, effected and overridden
like any other node, and an SVG edit that keeps ids keeps its
animations.

## 3. Rigs and custom properties

`rig(symbol, pos=..., scale=..., name=...)` creates a group and an
addressable part per symbol id. Properties are declared on the rig:

```python
r = rig(art, name="mascot")
r.prop("mouth_open", 0.0, range=(0.0, 26.0), step=0.5)
r.prop("look", (0.0, 0.0), range=((-1.0, -1.0), (1.0, 1.0)))
r.prop("mood", "calm", kind="enum", values=["calm", "sad", "happy"])
r.mirror("lid", ("lid_top_L", "lid_top_R"), flip=False)
```

- Types: `float`, `int`, `bool`, `enum`, `color`, `vec2`. `kind` is
  inferred from the default unless given.
- Every property is animatable on the instance with the ordinary key
  API: `m.mouth_open.key(t, value, ease=...)`. Keys are the inputs the
  rig is evaluated with; they also show up as timeline rows of the rig
  manifest.
- A mirror group makes one name edit both sides. Per-side properties
  (`lid_top_L`) always win over the group value; `flip=True` negates the
  right side (the controller's `MIR` table).

## 4. Drivers

A driver is a pure MDF expression function registered with
`r.driver(fn)`. It receives the rig properties as parameters and returns
a list of writes:

```python
def lids(lt, lb, ew):
    return [
        ["lid-top-L", "position.y", -30.0 + 30.0 * lt],
        ["lid-top-R", "position.y", -30.0 + 30.0 * lt],
        ["lid-bottom-L", "position.y", 30.0 - 30.0 * lb]
    ]

r.driver(lids)
```

Supported writes: `position.x`, `position.y`, `rotation`, `scale.x`,
`scale.y`, `opacity`, `stroke_width` and `geometry`. The expression subset
is numbers, property names, `+ - * /`, comparisons, `and/or/not`, and the
pure builtins `min`, `max`, `abs`, `sqrt`, `sin`, `cos`, `clamp`, `lerp`.
Anything else is a typed compile error naming the driver.

A `geometry` write is a sandboxed path expression:
`path_builder(move_to(x, y), line_to(x, y), cubic_to(c1x, c1y, c2x, c2y,
x, y), arc_to(rx, ry, rotation, large_arc, sweep, x, y), close())`,
`ellipse_arc(cx, cy, rx, ry, start, end)`, `arc(radius, start, end)`,
`rounded_rect(width, height, radius)` or `mirror(geometry, axis="x")`.
Points interpolate with the part's fixed topology; a different point count
or closure is a typed error. The bake writes ordinary geometry keys, so the
runtime evaluator samples points exactly like transforms (D-0058).

Evaluation order per frame: property values (keys and state machine) →
poses and clips → drivers → constraints → writes to parts. Geometry drivers
run in the same driver pass and append to that part's output; the first
frame and every change become keys.

Drivers are evaluated deterministically, frame by frame, at load time,
and their output is written as keys (D-0052). The result is a document
that renders through the one evaluator, so scrubbing, motion blur,
masks and export need no special cases.

## 5. Poses, clips, layers

- Pose: a named partial property set. `m.pose("Feliz", at=1.0,
  dur=0.35, ease="cubic_out")` blends from the values at `at`.
- Clip: a named, reusable keyed sequence on properties. `cl =
  r.clip("salto")`, `cl.key("squash_y", 0.28, -60, ease="quad_out")`,
  then `m.play("salto", at=4.0, loops=2, speed=1.0)`.
- Layer: a named pass of poses/clips over the base state.
  `r.layer("blink", mode="override")`; override wins, additive sums
  numeric properties. `r.blink(every=3.5, jitter=0.6)` emits a
  deterministic blink clip from a fixed seed.

Blending per type: float/vec2 lerp, rotation shortest-arc slerp, colour
in linear light, bool/enum step at the end.

## 6. State machines

```python
r.input("mood", "calm", kind="enum", values=["calm", "sad", "happy"])
r.state("calm", pose="Neutral")
r.state("sad", pose="Triste")
r.transition("calm", "sad", input="mood", equals="sad", dur=0.3)
r.transition("*", "hit", trigger="jump", dur=0.1)

m.mood.key(6.0, "sad")     # inputs are properties, keyed on the timeline
m.jump.fire(4.0)           # a trigger fires at a frame
```

Transitions support conditions on number/bool/enum inputs, a blend
duration, an optional exit time and interruptibility. Any-state
transitions use `"*"`. States hold a pose or a clip. Events are emitted
when a transition fires or a clip marker is crossed, with the exact
frame.

Inputs are timeline-animatable as well as listener-driven in the preview.
The machine is evaluated frame-exactly at bake time; because the output
is keys, scrubbing backwards is exact by construction. If no input is
keyed, the machine stays in its default state.

## 7. Manifest and query

Every compile stores a typed, versioned description of the rigs in the
document (`RigManifest`, D-0056): root identity and name, addressable parts
with their node identities, properties with type/range/step/group/mirror and
their authored keys, poses, pose tracks, clips, plays, procedural layers,
aims and the machine (states, transitions, inputs and emitted events). It is
descriptive data: evaluation always uses the baked keys.

```text
# the manifest of one instance (by name or root identity)
motioner query examples/rig/mascot_demo.motion --rig mascot

# the positional alias does the same
motioner query rig examples/rig/mascot_demo.motion mascot

# every rig in the document
motioner query examples/rig/mascot_demo.motion --rig all
```

The JSON is `{"version":2,"rigs":[...]}` with fixed field order. The same
shape is the `rig_manifest` result schema in `motioner tools --schema`,
which agents use before editing a rig by property name.

The rig family in `motioner tools --schema` is:

| Tool | What it does |
|---|---|
| `rig_list` | every instance with part/property/pose/clip/state counts |
| `rig_manifest` | the typed manifest above, one instance or all |
| `rig_set` | set a property or mirror group (`side` = left/right/both) |
| `rig_key` | add or replace a property key |
| `rig_key_input` | key a state-machine input on the timeline |
| `rig_apply_pose` | apply a declared pose at a time |
| `rig_play_clip` | play a clip at a time, with loops and speed |
| `rig_save_pose` | declare or replace a pose |

Every editing tool goes through the same `Editor` as a drag: the DSL plans a
byte-range edit on the call, the editor compiles it, compares the rig
manifest with the exact expectation and records one undo step. When the
declaration lives inside a used `.motion` module the tool refuses with a
reason instead of editing the module behind the program's back (D-0057).

## 8. Rive and Lottie comparison

The "Status" column is what this repository ships after prompt 16 and
the backlog that remains.

| Capability | Rive | Lottie | Motioner equivalent | Status |
|---|---|---|---|---|
| Code-first authoring | .riv binary + editor, runtime APIs | JSON (Bodymovin export) | MDF program; rig is code, diffs in git | Delivered |
| Artboards, nesting | artboards, nested artboards | precomps | groups; nested rigs with forwarded properties | Not delivered (backlog) |
| SVG import | limited | via AE shapes | `use("x.svg")` symbol, parts by id | Delivered (subset) |
| Addressable parts | named shapes | layer names | `m.part("eye-L")`, `m.parts` | Delivered |
| Custom properties | view models / data binding | expressions | typed rig properties, keyable, agent-queryable | Delivered (no view-model bindings yet) |
| Property animation | timeline, keyed | keyframes, expressions | existing key API on instance properties | Delivered |
| Drivers / expressions | data binding + scripting | AE expressions | pure expression drivers compiled to IR, baked | Delivered (expression subset) |
| Bones + skinning | yes | no (plugins) | none | Backlogged (T-RIG) |
| Mesh deform, lattice | yes | no | none; SVG transform pivots only | Backlogged |
| Blend shapes | yes | no | none | Backlogged |
| IK, follow path, scroll | yes | no | aim/copy constraints | Aim delivered, IK/follow backlogged |
| Spring follow-through | yes (physics) | no | ease `elastic_out`/`bounce_out` only | Partial |
| State machines | inputs, layers, blend 1D/2D | markers only | inputs (number/bool/enum/trigger), states, transitions, any-state, layers | Delivered (blend 1D/2D backlogged) |
| Events | events, audio events | markers | transition/marker events on the bake; audio hook backlogged | Partial |
| Masks / clip paths | yes | mattes | SVG clipPath â†’ D-0033 mask | Delivered |
| Text runs | text | text layers | text nodes + text animators | Existing |
| Layout | layouts | no | `row/column/grid/stack` | Existing |
| Interactive runtime | C++/JS/wasm player | lottie players | `motioner_rig` headless evaluator; wasm player backlogged | Foundation delivered |
| Determinism | runtime clock | AE evaluation | bake-time keys; bit-identical goldens | Delivered, stronger |

### Better because

- The rig is readable MDF that the UI writes through (D-0036) and that
  an agent can create, inspect and drive. A Rive file is opaque binary.
- Timeline and state machine are one evaluation: "at 3 s set `mood =
  sad`" is a key, so a video and an interactive scene share the rig.
- Rendering goes through the same evaluator as every other node, so
  effects, masks, motion blur, 3D layering and export apply to rigs for
  free.
- Deterministic output: no ambient clock, no map iteration, and the
  baked document is the single source for preview and export.

### Deferred, and why

- Bones, skinning, meshes and blend shapes need new renderer primitives
  (textured triangle meshes, weighted point deformers) and are not
  justified by the mascot acceptance test; they are backlogged with
  concrete tasks.
- A real-time player that evaluates rigs without re-running the program
  is a delivery concern after the editor integration; `motioner_rig` is
  the seam and is tested headless.
- Lottie import/export needs the shape-layer model and is backlogged.

