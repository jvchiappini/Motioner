# Mascot rig module

`mascot.motion` rebuilds the reference controller as a Motioner rig:

- `art = use("assets/mascot.svg")` imports the symbol; every controller id
  (`body`, `eye-left`, `pupil-right`, `lid-top-left`, `mouth`, ...) is an
  addressable part.
- 21 typed properties (squash, translate, rotation, eye size, lids, pupil
  scale, brows, mouth, glow, sparks, `look`, `mood`) with ranges and steps.
- Six expression drivers replace the controller's `apply()`: body motion,
  eye scale, lids, brows, mouth and effects. Each returns
  `[part, field, value]` writes.
- 17 presets as `reset=True` poses, so each preset clears properties it
  does not set, exactly like the controller's `expand()`.
- Two clips built key by key from the controller's ANIM tables.
- A `mood` enum machine with calm/sad/happy states and a blink layer.

Documented deviations: path geometry is authored once, so lids slide and
the mouth scales instead of being re-derived; perspective/skew and the
four movable corners are approximated. See the phase report.

Use it from another program:

```python
rigs = use("examples/rig/mascot.motion")
m = rigs.build(pos=(960, 540), scale=4.2)
m.pose("Feliz", at=1.0, dur=0.35, ease="cubic_out")
m.mood.key(3.0, "sad")
m.play("salto", at=4.0)
```
