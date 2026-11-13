# MDF v0 specification

Status: frozen v0 subset (2026-10-31). The first implementation covers
numbers, strings, booleans, lists, variables, functions, conditionals,
`for` loops, arithmetic, comparisons, calls, attribute access, the 2D
scene API, keyframes and linear/eased interpolation. `while`, `use`,
overrides and LaTeX are deferred to v0.1 and must not block the first
render milestone.

Language: Motioner Document Format (MDF).
Runtime: deterministic, sandboxed, program-first.

## 1. Design goals

- Program-first: the MDF program is the canonical source of a
  composition. Running it produces the document.
- Python-like: indentation blocks, functions, conditionals, loops,
  expressions and collections.
- Deterministic: the same program and inputs produce the same document
  and frames.
- Sandboxed: no filesystem, network, process or ambient time.
- Expressive enough to surpass Manim for procedural motion graphics.
- Readable by humans, editable by the AI and renderable by the editor.

## 2. Execution model

A program is executed once when the project is loaded or refreshed. It
builds a scene by calling constructors and setting properties. Execution
is depth-first and statements run in source order.

- Top-level statements run in the program scope.
- Functions run in an isolated call scope.
- Constructors create nodes and attach them to the current composition.
- Property assignment and keyframe calls mutate the created node.
- Running the program again rebuilds the document from scratch.
- UI overrides are applied after execution.

No I/O, no threads, no async and no external processes.

## 3. Lexical structure

- Source is UTF-8 text.
- A program may start with a version comment: `# mdf: 0.1`.
- Indentation uses spaces; four spaces per level. Tabs are rejected.
- Comments start with `#` and run to end of line.
- Blank lines and comment-only lines are ignored for indentation.
- Identifiers use letters, digits and underscores, and cannot start with
  a digit.
- Keywords: `def`, `return`, `if`, `elif`, `else`, `for`, `in`, `while`,
  `break`, `continue`, `pass`, `True`, `False`, `None`, `and`, `or`,
  `not`, `use`.
- Literals: integers, floats, strings (single or double quoted), raw
  strings prefixed with `r`, booleans, `None`, lists, tuples and dicts.
- Strings support `\n`, `\t`, `\\`, `\"`, `\'` and `\uXXXX`.

## 4. Types

Dynamic types: number, string, boolean, `None`, list, tuple, dict,
handle (scene node), function and module.

Numbers are IEEE-754 doubles in the script layer; rendering quantizes
them through the document model. Integer and float syntax are accepted;
there is no separate integer division rule: `/` always produces a float,
`//` produces an integer.

## 5. Statements

- Assignment: `name = expression`, `name += expression`, and the same for
  `-=`, `*=`, `/=`.
- Expression statement: a call or property assignment.
- Property assignment: `handle.property = expression`.
- Indexed assignment: `list[index] = expression`, `dict[key] = expression`.
  A list may never contain itself, directly or indirectly; such an
  assignment is rejected with a value error.
- `def name(params):` followed by an indented block. A parameter may have a
  default (`def badge(x, fill="#ffffff"):`); parameters after the first
  default must have defaults too. A missing argument uses its default, and
  the default expression is evaluated at each call in the caller's scope.
- `return expression` or `return`.
- `if condition:` / `elif` / `else`.
- `for name in iterable:` over lists, ranges, strings and dicts (keys in
  stable insertion order). A comma-separated target unpacks a pair:
  `for i, name in enumerate(names):`.
- `while condition:`.
- `break`, `continue`, `pass`.
- `use("path")` returns a module handle (see section 10).

## 6. Expressions

Supported operators, from lowest to highest precedence:

- `or`
- `and`
- `not`
- comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `not in`
- `+`, `-`
- `*`, `/`, `//`, `%`
- unary `-`, `+`
- calls, attribute access, indexing
- literals, names, parenthesized expressions

Built-ins implemented in v0: `len`, `range`, `str`, `int`, `float`, `min`,
`max`, `abs`, `round`, `enumerate`, `sin`, `cos`, `lerp`, `clamp`,
`text_width`, `text_bounds` and `bezier`. `enumerate(value, start=0)`
returns `(index, item)` pairs and feeds the guarded loop edits:
`for i, name in enumerate(names):`. `bool`, `sorted`, `reversed`, `zip`,
`seed`, `rand`, `tan`, `radians` and `degrees` are deferred (section 17).

`seed(value)` initializes the deterministic generator; `rand()` returns
a deterministic float in `[0, 1)`. Using `rand()` without `seed` is an
error.

## 7. Constructors and scene API

Constructors create nodes. Every node exposes properties and methods.
The v0 surface:

| Call | Purpose |
|---|---|
| `comp(name, width=1920, height=1080, fps=60, duration=5.0, background="#000000", motion_blur=False, shutter=180, samples=8)` | Create the composition; `shutter` is in `(0, 360]` degrees and `samples` in `1..=64` |
| `group()` | Empty container |
| `rect(width, height, radius=0, fill="#ffffff", stroke=None, stroke_width=0)` | Rectangle |
| `circle(radius, fill="#ffffff", stroke=None, stroke_width=0)` | Circle |
| `path(points, closed=False, fill=None, stroke="#ffffff", stroke_width=2, cap="round", join="round", trim_start=0, trim_end=1, trim_offset=0, dash=0, dash_offset=0)` | Vector path |
| `polygon(sides, radius, fill=None, stroke="#ffffff", stroke_width=2, cap="round", join="round")` | Regular polygon |
| `star(points, inner, outer, fill=None, stroke="#ffffff", stroke_width=2, cap="round", join="round")` | Star |
| `arc(radius, start=0, end=360, fill=None, stroke="#ffffff", stroke_width=2, cap="round", join="round")` | Circular arc |
| `linear_gradient(start=(0, 0), end=(100, 0), stops=[(0, "#000000"), (1, "#ffffff")])` | Linear paint |
| `radial_gradient(center=(0, 0), radius=100, stops=[(0, "#ffffff"), (1, "#000000")])` | Radial paint |
| `text(content, font="Roboto", size=48, color="#ffffff", align="left", weight=400, tracking=0.0, line_height=1.0)` | Text |
| `latex(expression, size=48, color="#ffffff", display=False)` | LaTeX math |
| `image(source, width=None, height=None, fit="contain")` | Image |
| `video(source, in_point=0.0, out_point=None, start=0, speed=1.0, width=None, height=None, fit="contain", gain=1.0)` | Video clip |
| `audio(source, in_point=0.0, out_point=None, gain=1.0, start=0, pan=0.0, fade_in=0, fade_out=0)` | Audio clip (D-0048) |
| `camera(pos=(0, 0, 8), target=(0, 0, 0), fov=45)` | 3D camera |
| `light(kind="directional", intensity=1.0, color="#ffffff")` | 3D light |
| `mesh(source)` | glTF model |
| `particles(count=1000, seed=0, lifetime=2.0)` | Particle emitter |
| `use(path)` | Load a project-relative MDF module |

Common node members:

- `parent`, `children`, `name`, `id`, `enabled`.
- `pos`, `scale`, `rotation`, `anchor`, `opacity`, `blend` (`"normal"`,
  `"add"`, `"multiply"`, `"screen"`, `"overlay"`, `"darken"`, `"lighten"`;
  D-0031).
- Content fields: `width`, `height`, `radius`, `fill`, `stroke`,
  `stroke_width` (rect/circle/path); `trim_start`, `trim_end`,
  `trim_offset`, `dash`, `dash_offset`, `cap`, `join` (path); `text`,
  `size`, `color`, `align` (text). A paint gradient adds
  `fill_stop<n>_offset`, `fill_stop<n>_color`, `stroke_stop<n>_offset` and
  `stroke_stop<n>_color` per stop; text gradients use the fill stop fields
  with `color` as the gradient.
- `clip_in`, `clip_out`, `time_remap`.
- Methods: `add(*nodes)`, `remove()`, `key(property, time, value, ease="linear")`,
  `ease(name)`, `effect(kind, **params)`, `mask(source, mode="alpha", invert=False)`.

`audio()` plays a project asset; it never draws pixels. `start` is the first
composition frame, `in_point`/`out_point` are seconds inside the source
(`out_point=None` means the file end), `gain` and `pan` (`-1..=1`) are
keyable channels, and `fade_in`/`fade_out` are seconds. The mixer combines
every visible audio layer to 48 kHz stereo; the editor's mute is the layer
visibility and its solo mode silences the other audio layers.

`video()` draws a project asset like `image()` (width, height, fit, blend,
effects and masks) and selects its source frame from the composition frame
with exact rational time: `source_frame = floor((in_point + (frame - start) /
fps) * speed * source_fps)`. `out_point=None` means the file end. Its audio
uses the same `gain`, `in_point` and `out_point` through the Phase 5 mixer.
The preview decodes low-resolution all-intra proxies from `cache/` on a
background worker that fills a bounded frame cache ahead of the playhead;
`render` and `still` always decode the original at full resolution.

Every content field except `text`, `align`, `weight`, `cap`, `join`,
`source`, `fit`, `in_point`, `out_point`, `start`, `speed`, `fade_in` and
`fade_out` is an **animatable channel**: `key("fill", t, "#RRGGBB")`, `key("size", t, 72)`,
`key("tracking", t, 0.02)`, `key("line_height", t, 1.4)`,
`key("stroke_width", t, 8)`, `key("trim_end", t, 1)`,
`key("gain", t, 0.5)`, `key("pan", t, -0.5)`. Colours interpolate
in linear-light sRGB; a `fill`/`stroke` keyed to `None` holds the previous
colour until that key (a step). See D-0028.

### 7.1 Paths

`path()` takes a list of points. A point is `(x, y)` for a corner, or
`(x, y, in_dx, in_dy, out_dx, out_dy)` when it has Bézier handles; the
handles are offsets relative to the point and a zero handle makes that side
a straight line. The first and last point connect straight when `closed`.
`polygon` starts at -90 degrees (a triangle points up), `star` takes the
number of spikes, and `arc` takes degrees clockwise on screen (`0` is +x,
a zero or full sweep draws a circle).

`trim_start` and `trim_end` are fractions of the path length and
`trim_offset` shifts that window along the path (wrapping past 1); this is
the After Effects trim-path model, so `key("trim_end", 0.0, 0.0)` +
`key("trim_end", 1.0, 1.0)` draws a line on. `dash` is the on-length in
local pixels (the gap is equal) and `dash_offset` shifts the pattern. Both
are animatable. See D-0029.

A path's points are also a keyable channel: `path.key("geometry", t, points)`
or, on a rig part, `m.geometry("lid-L", t, points, ease=...)`. Every key must
have the same point count and the path's closure, cap and join stay fixed;
the point positions and handles interpolate point-wise with the arriving
key's ease. A different topology is a typed error, never a silent reshape.
Geometry drivers (section 7.9) are baked into these keys. See D-0058.

### 7.2 Gradients

`linear_gradient(start, end, stops)` and `radial_gradient(center, radius,
stops)` are paint values accepted by `fill`, `stroke` and text `color`
(D-0030). `stops` is a list of `(offset, "#rrggbb[aa]")` pairs, 2 to 16 of
them; offsets are `0..=1` and are sorted when the ramp is sampled, so a
keyed offset may cross a neighbour. Two stops at one offset make a hard
edge. Gradient geometry is authored in the node's local pixels: moving,
rotating or scaling the node transforms the ramp with it. For a radial
gradient the centre and the local `+x` edge point are transformed separately
and the world radius is the length of that difference, so the last stop lands
exactly on `radius × scale` even when the node is off-centre. Under a
non-uniform scale the engine pass draws a circle and follows the x-axis scale.

The stop offset and colour are ordinary content channels:

```
card = rect(400, 240, fill=linear_gradient(
    start=(-200, 0), end=(200, 0),
    stops=[(0, "#141026"), (0.6, "#7C5CFF"), (1, "#FF5A6E")]))
card.key("fill_stop1_color", 0.0, "#7C5CFF")
card.key("fill_stop1_color", 1.0, "#5CDBC5", ease="cubic_out")
card.key("fill_stop1_offset", 0.0, 0.3)
```

Interpolation is linear-light sRGB per channel, alpha included, exactly
like `Color::lerp`. A pixel exactly at a stop offset shows the authored
sRGB byte. Setting a solid colour replaces the gradient; setting
`fill="none"` clears it. `cap` and `join` stay discrete, and a gradient
itself is not keyed as one value — key its stops instead.

### 7.3 Effects

`node.effect(kind, **params)` adds an effect to a node's ordered stack, or
updates the parameters of an effect already present (the call is idempotent, so
a bake can append it). A node holds at most one effect of each kind, and the
order of the calls is the order of application:

| kind | parameters |
|---|---|
| `drop_shadow` | `color`, `offset=(x, y)`, `blur`, `opacity` |
| `blur` | `radius` |
| `glow` | `color`, `radius`, `intensity` |

```
card = rect(300, 180, radius=16, fill="#E0E7F0")
card.effect("drop_shadow", color="#000000B3", offset=(10, 12), blur=10, opacity=0.7)
title.effect("glow", color="#33F2E6", radius=14, intensity=0.9)
group.effect("blur", radius=9)
```

An effect applies to the node **and its whole subtree**: the subtree renders
into one offscreen layer, the chain runs over that image, and the layer
composites with the node's blend mode. Every parameter is an animatable
channel named `<kind>.<param>` (`offset` adds `.x`/`.y`), so
`key("drop_shadow.blur", 1.0, 20)`, overrides and the editor's stopwatches work
on it without special cases. Colours are `#RRGGBB[AA]`; radii are non-negative
pixels; `opacity` is `0..1`, `intensity` is added light. See D-0032.

### 7.4 Masks

`node.mask(source, mode="alpha", invert=False)` uses the drawn subtree of
`source` as the coverage of this node's layer; `node.mask(None)` clears it. The
source is not drawn by itself, is evaluated in world space with its own animated
transform (a child of the masked node moves with it), and may carry effects: a
`blur` on the source feathers the mask edge.

```
card = rect(440, 210, fill=linear_gradient(...))
spot = circle(110, fill="#808080")
card.mask(spot, mode="luma", invert=True)
```

Coverage is `alpha` (the source's alpha) or `luma` (its linear-light luminance),
optionally inverted, multiplied into the finished, effected layer before the
node's opacity and blend mode. Masks compose with nested masks, effects and
blend modes. The editor stores the relationship as the `mask` override
(`"<source>:<alpha|luma>:<true|false>"`), set and cleared with `mask_set` and
`mask_clear`. See D-0033.

3D nodes add `target`, `fov`, `intensity`, `material`, `cast_shadow` and
similar properties. NLE nodes add clip points and track assignment.

Assignments use plain values:

```
title = text("Motioner", size=140)
title.pos = (960, 540)
title.opacity = 0
title.key("opacity", 0.0, 0.0)
title.key("opacity", 0.6, 1.0, ease="quad_out")
```

Keyframe times are in seconds and are converted to frames with the
composition fps. Values are lists or tuples for vectors.

Ease names: `linear`, `quad_in`, `quad_out`, `quad_in_out`, `cubic_in`,
`cubic_out`, `cubic_in_out`, `back_in`, `back_out`, `hold`, `elastic_out`,
`bounce_out`, `expo_in_out`, `sine_in_out`. `ease=bezier(x1, y1, x2, y2)` is
the CSS cubic-bezier form; `x1` and `x2` must be in `[0, 1]` and `y` may
overshoot. `hold` keeps the previous value until the arriving key.

`stagger(nodes, property, values, start, step, duration, ease="linear")`
writes two keys per node: `values` is a list of `(from, to)` pairs (one pair is
shared), node `i` gets a linear key at `start + i*step` and an eased key at
`start + i*step + duration`.

`node.wiggle(property, seed, frequency, amplitude, start, end)` writes one
`sine_in_out` key per `1/frequency` seconds with value `base ± amplitude`
from a deterministic SplitMix64 stream.

### 7.5 Text layout, measurement and per-character animation (D-0034)

`text()` accepts `font`, `weight`, `tracking` (extra advance between
characters, em units) and `line_height` (multiplier of the font's natural line
step). A `\n` in the content starts a new line; each line is aligned on its
own and line 0's baseline is the node origin. `tracking` and `line_height` are
animatable channels; `weight` is discrete.

`text_width(content, size, font="Roboto", weight=400, tracking=0,
line_height=1, align="left")` returns the widest line's advance, and
`text_bounds(...)` returns `(left, top, right, bottom)` in Motioner local pixels
(y down). Both use the CPU measurer installed by the host; a host without one
reports a runtime error.

`node.animate_chars(property, from, to, start, stagger, duration,
ease="linear", order="forward")` appends a per-character animator. Properties
are `opacity`, `offset`, `scale`, `rotation` and `color`; order is `forward`,
`reverse` or `center`. Whitespace counts for the stagger but draws no quad.
Animators are deterministic and bakeable.

### 7.6 Images (D-0035)

`image(source, width=None, height=None, fit="contain")` loads a
project-relative PNG. `source` is resolved against the program's project root;
absolute paths and `..` are rejected. `width`/`height` are the destination box
in composition pixels; `None` or `0` means the natural size and one dimension
keeps the aspect. `fit` is `contain`, `cover` or `fill`.

The image key is the SHA-256 of the file bytes. `width`, `height`, `source`
and `fit` are ordinary content fields (`key`/`edit`/`set` work by name);
the destination rectangle, the crop and the decoded key reach the renderer
through the plan. A missing or invalid file is a typed `still`/`render` error
naming the path; the live preview draws a checker placeholder instead.
PNG only for v0; palette, greyscale and 16-bit files are expanded to RGBA8,
and `gAMA`/`iCCP` are ignored.

### 7.7 Assets and project resolution

`image`, `audio` and `video` take a source string. In a project folder the
loader resolves it through the asset registry **by key or original file
name**, case-insensitively, project assets first; the resolved hash and blob
are recorded on the node so the renderer never re-resolves a path per frame.
A name that matches no registry entry is still tried as a project-relative
path (`assets/fonts/...`, loose files). Absolute paths and `..` are rejected.

The registry key is the explicit form: `image("logo")` selects the entry
whose key is `logo` even when its original file is `logo.png`. The v0
implementation has no separate `asset()` call: the key is passed directly to
the constructor, which avoids a second resolution surface and keeps one
registry lookup. (D-0045 proposed `asset("logo")`; it remains reserved.)
An unknown source is a typed `still`/`render` error naming the path. The
preview draws a checker placeholder for images and keeps the last uploaded
frame for video. Audio sources that do not resolve are skipped by the mixer.

The project root replaces the process working directory for every relative
path. A loose `.motion` file keeps single-file mode: root = the process
working directory, no registry, no autosave. `docs/projects.md` describes
the folder, the authoritative files and what is safe to delete.

### 7.8 Standard library (D-0039)

`std` is an implicit prelude: its definitions are in scope before the user
program, a user definition shadows one, and `lint` warns about the shadow.
`std_version()` and `query` report the library version. The library is
read-only for write-through: edits resolve to the user call site or a local
assignment.

- `theme(name)` returns a validated token set; `use_theme(t)` makes it the
  default. `comp(..., theme=name, tempo=bpm)` does both. Constructors and
  setters accept colour, type-scale, spacing, radius, font and ease tokens in
  place of literals; `token(name)` returns the active value.
- `beat(n)` and `bar(n)` return seconds at the composition tempo. `comp`
  takes `tempo`; presets take durations in beats.
- `scene(name, start, length)` returns a named group visible only inside its
  window; `scene.t(x)` converts scene-local time to absolute seconds.
- `node.place(anchor, x=None, y=None, margin=None)` positions by anchor and
  composition fractions; text gets a title-safe default margin.
  `node.bounds()` returns `(left, top, right, bottom)` in composition space.
  `row`, `column`, `grid` and `stack` lay out measured children.
- Motion presets: `enter`, `exit`, `emphasis`, `move_to`, text `reveal`,
  `enter_all`, `exit_all`. They emit ordinary keys.
- Components: `card`, `title_card`, `lower_third`, `logo_lockup`, `badge`,
  `button`, `bar_chart`, `progress`, `background(kind)`; each returns a group
  with named parts.

`docs/std.md` documents every token, theme, preset and component with images.

### 7.9 Rigs, drivers and state machines

`rig(symbol_or_node, pos=None, scale=None, name=None)` creates a component
instance: a group whose symbol parts are real nodes (`m.part("eye-L")`,
`m.parts()`). Properties are declared with `m.prop(name, default,
range=None, step=None, kind=None, values=None, angle=False)`; their types
are float, int, bool, enum, color, vec2 and angle. A property is keyed
like any channel (`m.name.key(time, value, ease=...)`) and set with
`m.name.set(value)`. `m.mirror(group, (left, right), flip=False)` makes
one name write both sides.

`m.driver(fn)` registers a pure expression function: its body must be
exactly `return [ [part, field, value], ... ]`, and it may read property
names and use numbers, `+ - * /`, comparisons, `and/or/not` and
`min/max/abs/sqrt/sin/cos/clamp/lerp`. Fields are `position.x`,
`position.y`, `rotation`, `scale.x`, `scale.y`, `opacity`,
`stroke_width` and `geometry`. Anything else is a typed error naming the
driver.

A `geometry` value is a path builder or a whole path:
`path_builder(move_to(x, y), line_to(x, y), cubic_to(c1x, c1y, c2x, c2y,
x, y), arc_to(rx, ry, rotation, large_arc, sweep, x, y, segments=...),
close())`, `ellipse_arc(cx, cy, rx, ry, start, end, segments=...)`,
`arc(radius, start, end, segments=...)`, `rounded_rect(width, height,
radius)` or `mirror(geometry, axis="x"|"y")`. `segments` is a whole number
`1..=64` and defaults to 4; a fixed count keeps the point topology stable
while an angle animates. Its point count and closure must match the part's
authored path; a mismatch is a typed error. The bake writes geometry keys
like transform keys (D-0058).

Deformers run after drivers and before constraints:

- `m.lattice(part, corners=(tl, tr, br, bl), edges=(top, right, bottom,
  left))` maps the part's local bounds through a 2x2 Coons patch; corner and
  edge values are Vec2 properties. Edges may be `None`.
- `m.bone(name, parent=None, at=(x, y), rotation=0, length=0,
  rotation_prop=None, position_prop=None)` declares a bone;
  `m.skin(part, weights=None)` skins the part with all declared bones. Omit
  weights for deterministic distance weights, or pass one `(bone, weight)`
  list per point.
- `m.blend(part, points=..., weight="property", fit=True)` adds a blend
  shape. `fit=True` resamples a differently sized target to the part's
  topology; otherwise the mismatch is a typed error.

Constraints run in declaration order after deformers:

- `m.aim(part, property, radius=...)` (existing pupils route through it);
- `m.ik(upper, lower, target="property", pole=None, tip=30)`;
- `m.follow(part, path, progress="property", orient=True)`;
- `m.copy(part, source, position=True, rotation=True, scale=True)`;
- `m.limit(part, min=(x, y), max=(x, y), rotation=(min, max))`;
- `m.distance(part, source, distance=...)`;
- `m.spring(part, target="property", stiffness=0.4, damping=0.2, mass=1)`.

The per-frame order is properties/machine → drivers → deformers →
constraints → draw. Deformers and constraint targets are deterministic
bakes; spring state is replayed from frame zero (D-0059, D-0060).

Constraints also accept bones. Prefix a part name with `bone:` in `aim`,
`ik`, `copy` or `limit` to run on the bone pose before skinning:
`m.ik("bone:upper", "bone:lower", target="hand", tip=30)`. Bone
constraints run before the deformers; part constraints run after them.
Deformers and constraint targets are deterministic bakes; spring state is
replayed from frame zero (D-0059, D-0060 and its E5 addendum).

Poses (`m.pose(name, prop=value, reset=False)` to declare;
`m.pose(name, at=..., dur=..., ease=...)` to apply), clips
(`cl = m.clip(name, length=...)`, `cl.key(prop, time, value, ease=...)`,
`m.play(name, at=..., loops=1, speed=1.0)`) and layers
(`m.blink(prop, every=..., jitter=..., duration=..., closed=...)`) are
property tracks. State machines use `m.input`, `m.state`,
`m.transition(from, to, input=..., equals=/gt=/lt=..., trigger=...,
dur=...)` and fire triggers with `m.fire(name, at=...)`; inputs are
properties, so they are keyed on the timeline.

The document is derived at load (section 2): rig evaluation runs once per
frame while the program loads and writes ordinary keys. Preview, export
and scrubbing therefore use the one evaluator, and seeking backwards is
exact by construction (D-0052/D-0053).

## 8. Control flow example

```
def grid(count, spacing, radius):
    for i in range(count):
        dot = circle(radius=radius, fill="#ff3366")
        dot.pos = (360 + i * spacing, 800)
        dot.key("scale", 0.0 + i * 0.08, 0.0)
        dot.key("scale", 0.8 + i * 0.08, 1.0, ease="back_out")

grid(6, 220, 24)
```

## 9. Provenance and overrides

Every created node gets a provenance path derived from its source
location, function call stack and loop iteration. Explicit identities
are allowed with `.id("name")` and must be unique.

- The editor shows the compiled document.
- A UI edit creates an override keyed by provenance or explicit ID.
- Overrides are stored in `<name>.overrides.motion` next to the program as
  `override(id, property, value)`, `override_key(id, property, time, value,
  ease)`, `override_unkey(id, property, time)`, `override_visible(id, bool)`
  and `override_locked(id, bool)` statements executed after the program.
- An override that no longer matches a node is reported as an orphan
  **warning** and kept: overrides are never dropped silently.
- Baking an element rewrites it into the program as explicit code and
  removes the override.

Overrides never silently change generated code.

### 9.1 Identities

An explicit `.id("name")` wins; names match `[A-Za-z_][A-Za-z0-9_.-]*` and
must be unique per document. Otherwise the identity is the provenance key
`@<call start>/c<call-site start>.../i<loop index>...` built from the byte
offset of the creating call, the call sites of the user function frames and
the iteration index of every enclosing `for` loop. The same program always
produces the same keys, but editing a line above a call shifts byte offsets,
so nodes users edit should carry `.id()`. D-0022 has the full rules.

### 9.2 Overridable properties

Transform properties (`pos`, `scale`, `rotation`, `anchor`, `opacity`) and
their keyframes. Vector properties have independently keyable axes:
`.key("pos.x", t, v)`, `override_key("id", "pos.x", t, v)` and
`override_unkey("id", "pos.x", t)` key only that axis, while
`key("pos", t, (x, y))` keys both. The editor shows one timeline row per axis.

Content fields are animatable channels with no axes (D-0028): `width`,
`height`, `radius`, `fill`, `stroke`, `stroke_width` (rect/circle),
`trim_start`, `trim_end`, `trim_offset`, `dash`, `dash_offset` (path),
`geometry` (path points, source keys only), `size`, `color`, `tracking`,
`line_height` (text), `width`, `height`
(image) and every gradient stop offset/colour plus every effect parameter.
`key("fill", t, value)`, `override_key("id", "fill", t, value)` and
`override_unkey("id", "fill", t)` work exactly like transform channels.
Colours are interpolated in linear-light sRGB; an optional colour keyed to
`None` holds the previous value as a step. `text` and `align` are discrete and
reject `key`/`override_key`.

`visible` (render) and `locked` (editor only) are not channels; `name`
(display name, no effect on identity or rendering) is a string override.

### 9.3 Bake

`motioner bake <file.motion> [--id ID]...` folds overrides into the program in
four ways (D-0027, D-0036):

1. **In place** when the overridden value came from a private plain literal:
   a constructor argument, a `node.property = literal` assignment, or the
   value/ease arguments of a `.key(...)` call. A literal is private when its
   source span was evaluated exactly once in the run; a loop body or a
   function called twice shares it and is not edited. Static values replace
   the literal text (numbers keep `format_number`, strings are quoted, pairs
   are `(x, y)`, booleans `True`/`False`, optional colours `None`); a key
   edits its value and, when the call has one, its `ease` literal.
2. **Loop guard** when the node was created in a `for` body: the statement is
   inserted at the loop body's indentation, after the node's last write, as
   `if i == 3:` (nested loops join with `and`). Repeated iterations merge into
   `if i in (1, 3):`; an existing guard for the same node is extended. A `_`
   loop variable is renamed, a repeated or non-literal iteration value is
   rewritten to `for i, x in enumerate(xs):`, and an inline node gets a
   readable binding. A guard that becomes equal to the shared value again is
   removed.
3. **Call site** when the value lives in a user function: a value that is
   already a parameter is passed (or edited) as a keyword argument; a function
   that returns the node gets the call result bound when needed and an
   assignment after the call; otherwise the literal becomes a keyword
   parameter with its old text as default and only this call passes the new
   value. A function called from a loop binds the result and adds the loop
   guard after the call. Other call sites keep rendering identically.
4. **Generated block** for everything else reachable from a top-level
   variable binding: statements appended after
   `# --- baked by motioner (do not edit above this line by hand) ---`, then
   the overrides are removed. Unbound nested calls and calls in the middle of
   an expression are refused with a message and keep their overrides.

In-place edits change the program length, so every provenance key after them
is re-keyed by the exact edit offsets in the same write (D-0025). Bake is
exact (`run(baked)` plus the remaining overrides equals `run(program)` plus
all overrides, node identities compared by position) and idempotent. If the
in-place pass is not exact it is discarded and the generated-block behaviour
is used.

### 9.4 Editor commands and re-binding

Editor actions are MDF calls too (`edit("title", "pos.x", 900)`,
`key("title", "opacity", 1.5, 0.5, ease="quad_out")`), parsed with this
grammar and run by `motioner edit` and `motioner ctl` (D-0024,
`docs/agents.md`). They are not program statements: they only produce the
override statements above. When a provenance key changes after an edit above
the call, its overrides become orphans; `motioner query` lists them with a
ranked `suggestions` array, and `assign_id("@old", "@new")` moves them to the
node's new identity. The orphans list in the layers panel offers the same
re-binding with one click (D-0025 addendum).

Outline and graph state is editor-only: collapse, `filter_layers`, `solo`,
`shy` and `show_shy` never reach the document or `render`/`still`, and
`graph_open`/`graph_view` only change the view. Display names are the `name`
override; graph drags are ordinary `ease` (with a `bezier(...)` value) and
`move_key_value` commands, so they are undoable and visible to agents.

## 10. Modules and imports

`use(path)` returns a definition (D-0051). The extension decides the kind:

- `.motion` executes the module once and returns a namespace; its
  top-level names are attributes on the namespace and the module's
  functions build into the caller's document.
- `.svg` returns a symbol definition; calling it instantiates a group
  whose parts are addressable by id (`m.part("eye-L")`).
- image, audio, video and font extensions return asset definitions;
  calling one builds the same node as the matching constructor.

Definitions are callable values with metadata (`width`, `height`, `name`,
`parts`, `kind`, `path`). Paths are sandboxed to the project root:
absolute paths and parent escapes outside the root are rejected before
any I/O. Modules cannot perform I/O; the host injects a resolver at the
`run_*` entry point. A module cycle is a typed diagnostic. Relative
`use()` inside a module resolves against that module's directory.

v0 module semantics import the module's top-level names into one shared
global scope; namespaced globals are backlogged.

## 11. Determinism

- No ambient time, no wall-clock reads and no external state.
- `rand()` requires `seed()`.
- Iteration over dicts uses insertion order.
- Sorting is stable.
- Floating-point math uses the platform IEEE-754 implementation; rendered
  output is quantized by the document model.
- Same program and settings must produce the same document and the same
  frames on the same machine.
- Cross-machine differences are documented with tolerance.

## 12. Limits and sandbox

Configurable defaults for v0:

- Maximum execution steps: 10,000,000. One step is charged for each
  executed statement, each collection item visited, copied or materialized,
  and each 64 bytes of string produced.
- Maximum function call depth: 256.
- Maximum nodes created: 1,000,000.
- Maximum string length: 16 MiB.
- Maximum list length: 1,000,000 items.
- Maximum value nesting depth: 100 levels.
- Maximum expression depth: 4096 levels (`MAX_EXPR_DEPTH`).
- Maximum script wall time: 5 seconds.
- Maximum memory: 256 MiB.

Exceeding a limit raises a runtime error with source span and stops
execution without corrupting the previous document.

## 13. Errors

Errors carry a category, message, source span and call stack.

Categories: lexical, parse, name, type, value, limit, unsupported.

The editor highlights the span, and the AI receives the same diagnostic
so it can repair the program.

## 14. Grammar (EBNF, v0 draft)

```
program        = { statement } ;
statement      = simple_stmt NEWLINE | compound_stmt ;
simple_stmt    = assignment | expression | return_stmt | break_stmt
               | continue_stmt | pass_stmt ;
assignment     = target ( "=" | "+=" | "-=" | "*=" | "/=" ) expression ;
target         = NAME { "." NAME | "[" expression "]" } ;
expression     = or_expr ;
or_expr        = and_expr { "or" and_expr } ;
and_expr       = not_expr { "and" not_expr } ;
not_expr       = [ "not" ] comparison ;
comparison     = additive [ ( "==" | "!=" | "<" | "<=" | ">" | ">="
                           | "in" | "not" "in" ) additive ] ;
additive       = multiplicative { ( "+" | "-" ) multiplicative } ;
multiplicative = unary { ( "*" | "/" | "//" | "%" ) unary } ;
unary          = [ "-" | "+" ] postfix ;
postfix        = primary { "." NAME | "[" expression "]" | "(" arguments ")" } ;
primary        = NAME | NUMBER | STRING | "True" | "False" | "None"
               | "(" expression ")" | list | tuple | dict ;
compound_stmt  = if_stmt | for_stmt | while_stmt | def_stmt ;
if_stmt        = "if" expression ":" block { "elif" expression ":" block }
               [ "else" ":" block ] ;
for_stmt       = "for" for_target "in" expression ":" block ;
for_target     = NAME { "," NAME }
               | "(" NAME { "," NAME } ")" ;
while_stmt     = "while" expression ":" block ;
def_stmt       = "def" NAME "(" [ params ] ")" ":" block ;
block          = NEWLINE INDENT { statement } DEDENT ;
return_stmt    = "return" [ expression ] ;
break_stmt     = "break" ;
continue_stmt  = "continue" ;
pass_stmt      = "pass" ;
arguments      = [ expression { "," expression } ] ;
params         = param { "," param } ;
param          = NAME [ "=" expression ] ;
list           = "[" [ expression { "," expression } ] "]" ;
tuple          = "(" expression "," [ expression { "," expression } ] ")" ;
dict           = "{" [ expression ":" expression { "," expression ":" expression } ] "}" ;
```

## 15. Project layout

MDF programs live in a project folder:

```
<name>.motioner/
  project.motion          manifest and entry program
  compositions/*.motion   one program per composition
  overrides.motion        UI overrides applied after execution
  assets/                 content-addressed media
  cache/                  decoded media and frame caches
  backups/                autosave and recovery
```

The exact manifest schema is specified with the project format.

## 16. Example program

```
# mdf: 0.1

comp(name="Intro", width=1920, height=1080, fps=60, duration=6.0,
     background="#0b0b12")

title = text("Motioner", font="Inter", size=160, color="#ffffff")
title.pos = (960, 470)
title.opacity = 0.0
title.key("opacity", 0.0, 0.0)
title.key("opacity", 0.8, 1.0, ease="quad_out")

formula = latex(r"E = mc^2", size=72, color="#9ad1ff")
formula.pos = (960, 700)
formula.key("scale", 0.8, 0.6)
formula.key("scale", 1.6, 1.0, ease="back_out")

for i in range(5):
    dot = circle(radius=18, fill="#ff3366")
    dot.pos = (480 + i * 240, 900)
    dot.key("scale", 0.2 + i * 0.1, 0.0)
    dot.key("scale", 1.2 + i * 0.1, 1.0, ease="back_out")
```

## 17. Deferred to v0.1

1. Namespaced module globals and explicit export lists.
2. Override *statements inside a program* (overrides live in the separate
   `<name>.overrides.motion` file described in §9 and D-0022; that subset is
   implemented) and override diagnostics for animated content
   properties, and the cross-file project folder.
3. `while` loops.
4. F-strings and advanced formatting helpers.
5. LaTeX math commands and error policy.
6. Final resource-limit defaults.
7. Formatter and comment-preservation rules.

The v0 subset is frozen; these items have specifications in this
document but are not required for the first MP4 milestone.


