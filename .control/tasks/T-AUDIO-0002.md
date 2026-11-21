---
id: T-AUDIO-0002
titulo: Audio: decode, mixer and MP4 mux (prompt 11 phase 5)
estado: done
prioridad: alta
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-04
actualizado: 2026-11-04
depende_de: []
bloqueado_por: null
version_schema: 1
---
## Contexto

Phase 5 adds real audio to the editor: ffmpeg decodes each asset once to
48 kHz stereo f32 PCM in `cache/`, MDF gains `audio(...)`, a pure mixer in
`motioner_io` feeds preview and export, the timeline shows waveform rows and
trim handles, and `render` muxes AAC into the MP4.

## Criterios de aceptacion
- [x] `motioner_io::audio` decodes once to `cache/<hash>.pcm` and mixes
      deterministically (`mix_document`), with a sample-exact unit test.
- [x] MDF `audio(source, in_point, out_point, gain, start, pan, fade_in,
      fade_out)` parses, is overridable/bakeable and appears in `query`;
      `gain` and `pan` are keyable.
- [x] Audio nodes never draw; they appear in Layers and the timeline with a
      waveform row, Gain/Pan rows and trim handles.
- [x] Playback uses the device clock to drive the playhead; scrub plays 60 ms
      grains; hidden layers and the solo state mute through the mixer.
- [x] `render` muxes AAC with `-shortest` and accepts `--no-audio`; a test
      proves the muxed MP4 keeps the video duration.
- [x] `pctl` tasks/docs/ADR D-0048 updated and the gate is green.

## Notas del agente

- `motioner_core::AudioContent` + `NodeContent::Audio`; `ContentField`
  gains `in_point`, `out_point`, `gain`, `pan`, `start`, `fade_in`,
  `fade_out`.
- `motioner_io::audio` (decode cache, `AudioStore`, `mix_document`,
  `write_wav_i16`), `Mp4Encoder::start_with_audio`.
- `motioner_app::audio_out` (cpal, D-0048) and `Host::audio_*`; the UI
  follows `audio_position` through `transport::audio_frame`.
- Screenshots: `out/ui/p11-06-timeline-waveform.png` and
  `out/ui/p11-06-audio-layer.png`; render `out/p11-audio/demo.mp4`.
