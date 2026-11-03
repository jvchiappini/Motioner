# Driving Motioner from an agent

Everything a person can do in the Motioner editor is a **command**. The
window turns clicks, drags and shortcuts into the same commands, so an agent
(or a shell script) has exactly the editor's power, with no mouse coordinates
and no window focus. Decision record: `.control/decisions/D-0024.md`.

There are two ways in:

| You want to | Use | Window? |
|---|---|---|
| Read the scene | `motioner query <file>` | no |
| Change it (writes `<name>.overrides.motion`) | `motioner edit <file> <command>...` | no |
| Fold edits into the program | `motioner bake <file>` | no |
| Drive a running editor and see it | `motioner preview <file> --control` + `motioner ctl <request>...` | yes |

Every one of them prints **one JSON object per line** on stdout.

## Files and identities

- The program is `<name>.motion` (MDF, `spec/mdf-v0.md`). Edits never touch
  it: they go to `<name>.overrides.motion` next to it (D-0022). Only `bake`
  rewrites the program.
- Nodes are named by **identity**: an explicit `.id("title")` from the
  program, or a provenance key such as `@288` or `@586/i2` (third iteration of
  a loop). Get them from `query`: `nodes[].id`. Provenance keys change when
  lines above the node are edited; prefer `.id()` for nodes you edit often,
  and move orphaned overrides with `assign_id(old, new)`.
- **Channels** are `pos.x`, `pos.y`, `scale.x`, `scale.y`, `rotation`,
  `anchor.x`, `anchor.y`, `opacity`. `pos`, `scale` and `anchor` without an
  axis mean both axes (`(x, y)` values). Scale and opacity are factors (`1` =
  100 %); rotation is degrees; positions are composition pixels.
- **Content fields**: `width`, `height`, `radius`, `fill`, `stroke`,
  `stroke_width` (rect/circle/path); `trim_start`, `trim_end`,
  `trim_offset`, `dash`, `dash_offset`, `cap`, `join` (path); `text`,
  `size`, `weight`, `tracking`, `line_height`, `color`, `align` (text). Colours are `"#RRGGBB"` or
  `"#RRGGBBAA"`; `fill`/`stroke` also take `"none"`. `align` is `"left"`,
  `"center"` or `"right"`; `cap` is `"butt"`, `"round"` or `"square"`;
  `join` is `"miter"`, `"bevel"` or `"round"`.
- **Blend mode** (D-0031): `blend` is `"normal"`, `"add"`, `"multiply"`,
  `"screen"`, `"overlay"`, `"darken"` or `"lighten"`. It is a whole-node
  property, not a content channel: `set(id, "blend", "multiply")` or
  `edit(id, "blend", "overlay")` write a static override; it cannot be keyed.
  `query` reports `blend` per node and the inspector offers the seven modes as
  a segmented grid. All seven modes composite through an engine offscreen
  layer.
- **Effects** (D-0032): a node carries an ordered stack with at most one of
  each kind: `drop_shadow`, `blur`, `glow`. Add and reorder them with
  `effect_add(id, kind)`, `effect_remove(id, kind)` and
  `effect_move(id, kind, index)`; the stack is stored as one `effects`
  override (`set(id, "effects", "drop_shadow,glow")`) that rebuilds the stack
  and preserves parameters. Parameters are animatable channels named
  `<kind>.<param>`: `drop_shadow.color` (colour), `drop_shadow.offset.x` /
  `.y`, `drop_shadow.blur`, `drop_shadow.opacity`, `blur.radius`,
  `glow.color` (colour), `glow.radius`, `glow.intensity`. They work with
  `set`, `edit`, `key`, `unkey`, `stopwatch` and `toggle_key` like any other
  channel, and `query` reports `effects` as an array of
  `{kind, params: {<name>: {value, keys}}}`. Effects apply to the node **and
  its subtree** as one offscreen layer, in `render`, `still` and `preview`.
- **Masks** (D-0033): `node.mask(source, mode="alpha"|"luma", invert=False)`
  makes another node's drawn subtree the coverage of this node. The source is
  never drawn on its own; it is evaluated in world space with its own animated
  transform. `node.mask(None)` clears it. In the editor the relationship is the
  `mask` override (`set(id, "mask", "<source>:<luma>:true")`, or `"none"`), set
  and cleared with `mask_set`/`mask_clear`; `query` reports it as
  `mask: {source, mode, invert}` (or `null`). A mask applies after the effect
  chain and before group opacity/blend, so a drop shadow is clipped by it; a
  `blur` effect on the source feathers the edge. `render`, `still` and
  `preview` all draw masks; the source is never shown as content.
- **Gradients** (D-0030): `fill`/`stroke`/text `color` may be a gradient
  instead of a colour. A gradient adds one pair of channels per stop:
  `fill_stop<n>_offset` (0..1), `fill_stop<n>_color`, and
  `stroke_stop<n>_offset` / `stroke_stop<n>_color` for the stroke. `query`
  reports the gradient as an object (`kind`, `start`/`end` or
  `center`/`radius`, `stops`) and the stop channels like any other channel.
  The gradient structure itself is authored in MDF; the editor and `edit`
  change stop colours and offsets.
- **Animatable content fields** (D-0028): every field above except `text`,
  `align`, `weight`, `cap` and `join` is a channel with no axes.
  `tracking` and `line_height` are ordinary text channels (D-0034).
  `key(id, "fill", t, "#FF0000")`,
  `stopwatch(id, "size")` and `edit(id, "width", 420)` work exactly like
  transform channels; the timeline and inspector show the same stopwatch and
  diamonds. Colours interpolate in linear-light sRGB, so a keyed red-green
  midpoint is `#BCBC00`, not `#808000`; keying a colour to `"none"` holds
  the previous value until that key. A gradient cannot be keyed as one
  value: key its stops (`key(id, "fill_stop1_color", t, "#00FF00")`).

## Command syntax

Commands are MDF function calls, the same syntax as the overrides file:

```text
seek(90)
select("@288")
edit("@288", "pos.x", 900)
key("@288", "opacity", 1.5, 0.5, ease="quad_out")
move_key("@288", "pos.y", 0.8, frame(60))
visible("@586/i0", false)
```

- Strings in double quotes; numbers; pairs `(x, y)`; `true`/`false`
  (`True`/`False` also work); lists `[...]`.
- **Times are seconds** (as in the overrides file), rounded to the nearest
  frame. `frame(N)` names an exact frame. `seek`, `step` and `retime_keys`
  take frames.
- Keyword arguments work everywhere: `key(id="@288", property="opacity",
  time=1, value=0.5)`.
- Several commands can go in one argument separated by newlines.

## Commands

The playhead (`seek`) matters for `edit`, `stopwatch`, `toggle_key` and
`add_keys`. Selection matters for `add_keys`, `set_ease`, `delete_keys`,
`retime_keys` and `copy_id`.

### Transport

| Command | What it does (the person's equivalent) |
|---|---|
| `play()`, `pause()`, `toggle_play()` | playback (Space) |
| `step(frames)` | step by frames (arrows; Shift = 10) |
| `seek(frame)` | move the playhead (click/drag the ruler) |
| `go_to_start()`, `go_to_end()` | Home, End |
| `loop(on)` | looping on/off (L) |
| `prev_key(id, property)`, `next_key(id, property)` | ◀ ▶ on a property row |

### Selection and view

| Command | What it does |
|---|---|
| `select(id)` | select a layer (layers panel, timeline, canvas click) |
| `deselect()` | click on empty canvas |
| `select_relative(rows)` | Up/Down |
| `expand(id, open=true)` | show/hide a layer's property rows (chevron) |
| `expand_all(open)` | Alt+chevron |
| `select_keys([(id, property, time), ...], add=false)` | click/Shift+click/marquee on diamonds |
| `clear_keys()` | deselect keys |
| `safe_area(on)`, `snap(on)`, `only_animated(on)` | G, the magnet, U |
| `motion_blur_preview(on)` | preview the real accumulation blur (D-0037/D-0044); paused uses every sample, playing caps at 4 |
| `filter_layers(text)` | search the layers panel and timeline by name, kind or identity |
| `solo(id, on)`, `shy(id, on)`, `show_shy(on)` | preview-only solo/shy layer state (never rendered) |
| `graph_open(id, property)`, `graph_close()` | open/close the value graph of one numeric channel |
| `graph_view(zoom, pan_x=0, pan_y=0)` | pan/zoom the graph (view state only) |
| `zoom(factor)` | absolute canvas zoom, `1` = 100 % (clamped to 0.05..32); `1` key |
| `zoom_fit()` | fit the composition and reset the pan; `F` or a click on the zoom pill |
| `zoom_by(steps, x=None, y=None)` | Ctrl+wheel over the canvas: ×1.25 per step, keeping the composition point under the pointer fixed |
| `pan(dx, dy)` | move the view by composition pixels; middle drag or Space+drag |
| `panel_size("layers"\|"inspector"\|"timeline", size)` | drag a splitter (logical px) |
| `scroll("layers"\|"inspector"\|"timeline", offset)` | mouse wheel (logical px) |
| `copy_id()` | the Copy button next to the identity |
| `reload()` | R |
| `quit()` | Escape |

### Canvas view

`state()` reports `zoom` (`null` = fit) and `pan` (`[x, y]` composition
pixels). Zoom and pan are view state: they never change the document, the
overrides or a rendered frame. `zoom_by` keeps the composition point under
the pointer fixed when `x`/`y` are given; a wheel step while fitted multiplies
the fit scale. Space toggles playback on release only if it did not drag:
Space+drag over the canvas pans.

### Editing (each is one undo step)

| Command | What it does |
|---|---|
| `edit(id, property, value)` | type into an inspector field or drag a gizmo: static value if the channel has no keys, otherwise a key at the playhead (keeping that key's ease) |
| `set(id, property, value)` | always a static override (`override(...)`) |
| `key(id, property, time, value, ease="linear")` | add or replace a key (`override_key(...)`); without `ease` a replaced key keeps its ease |
| `unkey(id, property, time)` | remove a key |
| `move_key(id, property, from_time, to_time)` | retime one key, keeping value and ease |
| `ease(id, property, time, name)` | change one key's ease |
| `set_ease(name)` | change the ease of every selected key (inspector ease grid) |
| `retime_keys(frames)` | move the selected keys (drag a selected diamond) |
| `delete_keys()` | delete the selected keys (Delete) |
| `stopwatch(id, property)` | stopwatch: first key at the playhead, or remove all keys keeping the current value |
| `toggle_key(id, property)` | ◆ button: add/remove the key at the playhead |
| `add_keys()` | K: key the selected layer's animated channels at the playhead (Position if none) |
| `visible(id, on)`, `lock(id, on)` | eye and padlock |
| `rename(id, name)` | set the display name (empty removes it); identity is unchanged |
| `move_key_value(id, property, from_time, to_time, value)` | move a key in time and change its value (graph key drag) |
| `assign_id(from, to)` | move every override of one identity to another (the Rebind action does this with the suggested node) |
| `delete_orphan(id, property, time=None)` | × in the orphans list |
| `effect_add(id, kind)` | the Shadow/Blur/Glow tiles in the inspector Effects section |
| `effect_remove(id, kind)` | the remove tile of an effect row |
| `effect_move(id, kind, index)` | the up/down tiles of an effect row |
| `mask_set(id, source, mode="alpha", invert=False)` | the Source/Mode/Invert rows of the inspector Mask section |
| `mask_clear(id)` | the Clear tile of the Mask section |
| `text_anim_add(id, property, from, to, start, stagger, duration, ease="linear", order="forward")` | the Fade/Rise/Colour tiles of the inspector Animators section |
| `text_anim_remove(id, index)` | the Del tile of an animator row |
| `undo()`, `redo()` | Ctrl+Z, Ctrl+Y |
| `bake(id, ...)` | fold overrides into the program (all ids when none) |

Eases: `linear`, `quad_in`, `quad_out`, `quad_in_out`, `cubic_in`,
`cubic_out`, `cubic_in_out`, `back_in`, `back_out`, `hold`, `elastic_out`,
`bounce_out`, `expo_in_out`, `sine_in_out`, and the CSS curve
`bezier(x1, y1, x2, y2)` (x in `[0, 1]`, y may overshoot). The overrides
file, `bake` and `query` round-trip every form; `query` prints a bezier as
`"bezier(0.25, 0.1, 0.25, 1)"`.

### Timing (D-0037)

- `comp(..., motion_blur=False, shutter=180, samples=8)` sets motion blur.
  `shutter` is in `(0, 360]` degrees and `samples` in `1..=64`; anything else
  is a span-carrying DSL error. `still` and `render` honour it; the preview
  is off by default.
- `motion_blur_preview(on)` toggles the real accumulation preview
  (`state()` reports `motion_blur_preview`). When the composition enables
  motion blur, the canvas is painted from the same accumulation pass as
  `still`/`render`; while paused every sample is used, while playing the
  editor caps it at four so playback stays usable.
- Display names: `node.name = "Dot grid"` in MDF, `rename(id, name)` in the
  editor and the `name` override. A node without one is named after its first
  binding (`title`, `dot 3` for a loop). The identity and every override key
  stay unchanged.
- Outline: the chevron collapses a group in the layers panel and timeline;
  `filter_layers(text)` matches name, kind and identity and keeps ancestors;
  `solo`/`shy` are preview-only and never affect `render`/`still`.
- The graph editor (double click a numeric timeline row) opens
  `graph_open(id, property)`. Keys are points; bezier segments have draggable
  handles. A handle drag is one `ease` command with `bezier(...)` (x clamped
  to `[0, 1]`), a key drag is `move_key_value`, and double clicking a named
  key converts it to an editable bezier. `graph_view` pans and zooms.
- `stagger(nodes, property, values, start, step, duration, ease)`: `values`
  is a list of `(from, to)` pairs (one pair is shared). Node `i` gets a
  linear key at `start + i*step` and an eased key at
  `start + i*step + duration`.
- `node.wiggle(property, seed, frequency, amplitude, start, end)`: one
  `sine_in_out` key per `1/frequency` seconds, value `base ± amplitude` from a
  SplitMix64 stream. The same seed gives the same keys; `base` is the
  property's static value.

`motioner --help` prints the same list from the code, so it cannot drift.

## Typography (D-0034)

- `text(..., font="Inter", weight=600, tracking=0.02, line_height=1.2)`; `\n`
  starts a new line and each line is aligned on its own.
- `text_width(content, size, font="Roboto", weight=400, tracking=0)` and
  `text_bounds(...)` measure with the same layout the renderer draws.
  `text_bounds` returns `(left, top, right, bottom)` in local pixels (y down).
- `node.animate_chars("opacity", 0, 1, start, stagger, duration,
  ease="cubic_out", order="forward")` animates every character. Properties:
  `opacity`, `offset`, `scale`, `rotation`, `color`; orders: `forward`,
  `reverse`, `center`. `start`, `stagger` and `duration` are seconds; the
  document stores frames, so `stagger` and `duration` are rounded to the
  nearest frame. `node.clear_chars()` removes every animator.
- The editor stores the whole list as one `text_animators` override, whose
  value is the canonical encoded list (`property:from:to:start:stagger:
  duration:ease:order`, `;`-joined, frames). Agents normally use
  `text_anim_add` / `text_anim_remove`, which rewrite that list; `bake` folds
  it into `clear_chars()` plus one `animate_chars(...)` per animator.
- `text_width` / `text_bounds` use the engine's `LayoutData`, the same tables
  the atlas is built from, so GPOS kerning matches the drawn text.

## Images (D-0035)

- `image(source, width=None, height=None, fit="contain")`; `source` is a
  project-relative PNG path (absolute paths and `..` are errors). `width` and
  `height` are the destination box in pixels; `None`/`0` means natural, one
  dimension keeps the aspect. `fit` is `contain`, `cover` or `fill`.
- The image key is the SHA-256 of the file bytes. `query` reports
  `source`, `natural_width`/`natural_height` (when resolved), `width`,
  `height`, `fit` and `key`; the plan carries the key, the destination rect
  and `uv_rect`.
- `set`/`edit`/`key` work by name: `set(id, "source", "assets/logo.png")`,
  `set(id, "fit", "cover")`, `key(id, "width", 1.0, 240)`. A source change
  clears the old key and the next compile re-resolves the file.
- `still`/`render` fail with a typed error naming the path when the file is
  missing or invalid; `preview` draws a checker/outline placeholder and shows
  a warning instead.
- Images are sRGB PNGs; palette, greyscale and 16-bit files are expanded to
  RGBA8. `gAMA`/`iCCP` are ignored. JPEG/WebP are not supported yet.

## `motioner query`

```text
motioner query <file> [--frame N] [--node ID]
```

Prints the scene at a frame (0 by default):

```json
{"file":"hello.motion","frame":48,"time":0.8,
 "composition":{"name":"Hello","width":1920,"height":1080,"fps":"60/1","duration":300,"background":"#0B0B12"},
 "nodes":[{"id":"@288","explicit_id":false,"kind":"text","name":"Motioner","parent":null,
           "visible":true,"drawn":true,"locked":false,
           "channels":{"pos.x":{"value":960,"keys":[{"frame":0,"time":0,"value":960,"ease":"linear"}, ...]}, ...},
           "content":{"text":"Motioner","size":160,"color":"#FFFFFF","align":"center"}}, ...],
 "overrides":["override_key(\"@288\", \"pos.x\", 0.8, 900, ease=\"cubic_out\")"],
 "orphans":[{"id":"gone","property":"opacity","frame":null,"suggestions":["@288","@596/i1"]}],
 "diagnostics":[{"severity":"warning","line":3,"column":1,"message":"..."}]}
```

Nodes are in layer order; `visible` is the eye, `drawn` whether the node is
actually drawn at that frame (opacity and scale included); a key's `ease` is
the ease of the segment arriving at it. Animatable content fields appear in
the same `channels` object (`"fill"`, `"width"`, `"size"`, ...), so keys and
eases are read the same way for every property. Each orphan lists the live nodes it
most likely belongs to, best first (property acceptance, inferred program
shift, same loop iteration, nearest offset); feed the first one to
`assign_id`.

## `motioner edit`

```text
motioner edit <file> [--frame N] [--dry-run] <command>...
```

Runs the commands in order through the editor's command layer (the code the
window runs). It is **all or nothing**: if one fails, nothing is written.

```json
{"ok":true,
 "results":[{"command":"edit(\"@288\", \"pos.x\", 900)","result":"edit(\"@288\", \"pos.x\", 900)"}],
 "removed":[],
 "added":["override_key(\"@288\", \"pos.x\", 0.8, 900, ease=\"cubic_out\")"],
 "written":"out/hello.overrides.motion"}
```

`--dry-run` writes nothing, sets `"written":null` and adds the full
`"overrides"` text. Undo history only lives for one invocation (`undo()`
inside the same call works).

## Live control: `preview --control` and `ctl`

```text
motioner preview <file> --control [--control-file FILE]
motioner ctl [--endpoint FILE] <request>...
```

`--control` opens an endpoint on `127.0.0.1` only (OS-chosen port, random
token) and writes `port`, `token` and `pid` to
`<temp>/motioner/control.txt` (or `FILE`); the file is removed when the
window closes. `ctl` reads that file, sends each request on its own
connection and prints the reply:

```json
{"ok":true,"request":"seek(30)","result":"frame 30"}
```

Requests are any command above plus three queries:

| Query | Result |
|---|---|
| `state()` | `{"frame","duration","playing","looping","selected","expanded","selected_keys","only_animated","motion_blur_preview","safe_area","snap","zoom","pan","panels":{...},"undo_depth","redo_depth","dirty","errors","warnings"}` |
| `query(node=None)` | the same JSON as `motioner query`, live |
| `screenshot(path)` | writes the window to a PNG: `{"path","width","height"}` |

Edits through `ctl` are the person's edits: the same undo entries, the same
overrides file (saved after a short debounce, `"dirty":true` until then).

## Errors

A failure is `{"ok":false,"error":{"kind":...,"message":...}}`, with
`"command": <index>` for `edit`. The exit code is 1 (2 for an MDF program
error in `still`/`render`/`info`). Kinds:

| Kind | Meaning |
|---|---|
| `parse` | not a call such as `seek(10)` |
| `unknown_command` | no command with that name |
| `bad_argument` | wrong arguments; the message shows the usage |
| `no_document` | the program has an error (see `query` diagnostics) |
| `unknown_node` | no node with that identity |
| `unknown_property` | not a channel or field of that node |
| `no_selection` | the command needs a selected layer or keys |
| `no_key` | no key at that time |
| `bad_value` | the value does not fit the property |
| `io` | a file could not be read or written; or no editor is running (`ctl`) |
| `refused` | understood but not possible here |

## Bake

`motioner bake` folds an override into the program in place when its value
came from a private plain literal (a constructor argument, a
`node.property = literal` assignment or a `.key(...)` value/ease). A literal
is private when the run evaluated it once; values created in a loop or in a
function called twice are shared, so they are never edited. Everything else
keeps the older behaviour: bound nodes get `var.property = value` statements
after the generated-block marker, and loop-created nodes without a binding
keep their overrides with a refusal reason.

In-place edits shift the byte-offset keys of later nodes; bake re-keys every
override that stays in the same atomic write (D-0025, D-0027). The result is
verified by re-running the program; if it is not exact, bake falls back to
the generated block.

## End-to-end example

```sh
# 1. Look at the scene.
motioner query hello.motion --frame 48 --node @288

# 2. Move the title 40 px down at 2 s, recolour it and soften its entrance.
motioner edit hello.motion \
  'seek(120)' 'select("@288")' 'add_keys()' \
  'edit("@288", "pos.y", 520)' \
  'edit("@288", "color", "#FF3366")' \
  'ease("@288", "opacity", 0.8, "cubic_out")'

# 3. Check it in a live editor and take a screenshot.
motioner preview hello.motion --paused --control &
motioner ctl 'seek(120)' 'select("@288")' 'screenshot("out/title.png")' 'quit()'

# 4. Fold the edits into the program.
motioner bake hello.motion

# 5. Render.
motioner render hello.motion --out out/hello.mp4
```

From PowerShell 5.1, escape the double quotes inside arguments to native
programs (`'select(\"@288\")'`); `scripts/ui_shot.ps1 -Commands` does it for
you.
