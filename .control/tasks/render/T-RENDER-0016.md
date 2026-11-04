---
id: T-RENDER-0016
titulo: Demo project, goldens, docs and release numbers (prompt 11 phase 7b)
estado: done
prioridad: alta
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-04
actualizado: 2026-11-04
depende_de: [T-PERF-0004]
bloqueado_por: null
version_schema: 1
---
## Contexto

Prompt 11 phase 7 closes the job with a real example project, goldens for the
demo and the video/mixer paths, the technical documentation and one release
measurement of the demo and 1080p60 playback.

## Criterios de aceptacion
- [x] `examples/demo.motioner/` is a real project folder whose image, video
      and music are generated only from ffmpeg `lavfi` test sources.
- [x] The demo has a title sequence over the video, music and a fade-out.
- [x] Two demo stills are pinned by `motioner_render/tests/golden_demo.rs`.
- [x] The same test asserts the exact video source frame at both beats.
- [x] `motioner_io/tests/demo_mixer.rs` pins the first 4096 mixed samples.
- [x] `out/demo.mp4` has a 6.000 s H.264 video and a 6.000 s AAC stereo track;
      the waveform still `out/demo_wave.png` shows the fade-out and
      `volumedetect` reports a non-silent track.
- [x] `spec/mdf-v0.md`, `docs/agents.md`, `docs/projects.md` and `README.md`
      are updated for audio, video, assets, projects and the app.
- [x] Release preview fps recorded for the demo and 1080p60 playback in the
      report and `.control/CONTEXT.md`.
- [x] Tasks, session log and backlog entries created.
- [x] Gate green.

## Notas del agente

- Media generation (ffmpeg 7.0.2):
  `gradients=size=1280x720:c0=0x0B0B14:c1=0x2A3A5A` -> `bg.png`;
  `testsrc2=size=1280x720:rate=60:duration=6` -> `clip.mp4` (all-intra,
  `-g 1`, CRF 28); `aevalsrc=...:s=48000:d=6` -> `music.wav` (PCM s16).
- The project was created with `motioner new`, the three files imported with
  `motioner import`, and the composition authored in
  `examples/demo.motioner/compositions/main.motion`.
- Goldens: `demo_f090.png` (title + subtitle over the video) and
  `demo_f330.png` (the fade), pinned with SHA-256 and the usual GPU
  tolerance. `hello_f120.png` is untouched.
- `out/demo.mp4` (3,232,661 bytes) was rendered with audio; `ffprobe` reports
  1280x720 60 fps H.264 + 48 kHz stereo AAC, 6.000 s. `out/demo_wave.png`
  is the waveform still; `volumedetect` mean -21.5 dB, max -13.0 dB.
- The demo render decoded the original video per frame and took 30.2 s
  (82.7 ms/frame render+readback, 0.62 ms/frame encode).
- Docs: `spec/mdf-v0.md` 7.7 (assets/project resolution, `asset()` reserved),
  `docs/projects.md` (layout, authoritative files, safe deletion, backups,
  recovery, CLI), `docs/agents.md` (video timeline/proxy, recovery commands),
  `README.md` (start screen and demo).
- No engine pin bump: the engine features already in use (file drops,
  in-place sprite texture update, accumulation, line/polyline, `GuiBatch`
  recycle) are documented in the pinned clone and covered by engine tests.
- Backlog: NLE transitions, audio effects, installer and file association.
