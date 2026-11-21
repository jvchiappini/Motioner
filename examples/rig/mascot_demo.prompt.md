# Mascot demo

A 15-second 1920x1080 60 fps character piece that exercises the rig system:
the reference mascot from `DailyAiRecap/mascot_controller.html`, rebuilt as
`examples/rig/mascot.motion` on top of `examples/rig/assets/mascot.svg`.

What it shows:

- all 17 controller presets as reset poses (0.7 s each), tweened with
  Motioner eases;
- the controller's two clips (`salto` squash jump, `giro` turn) played from
  the timeline at 10.9 s and 12.6 s;
- the look pad as a keyed `look` vec2 property driving both pupils through
  an aim constraint;
- a deterministic blink layer on top of everything;
- a `mood` state machine whose input is keyed at 13.4 s and 13.9 s.

Run it:

```
motioner render examples/rig/mascot_demo.motion --out mascot.mp4
motioner still  examples/rig/mascot_demo.motion --frame 90 --out mascot_90.png
```

From the repository root, so `use("examples/rig/mascot.motion")` resolves.
Frames used in the phase report: 30 (Neutral), 90 (Excitado), 160 (Enojado),
660 (jump anticipation), 800 (turn).
