---
id: T-AUDIO-0001
titulo: Audio engine and stereo mixer (decode, tracks, gain/pan/fades, export)
estado: backlog
prioridad: alta
tipo: feature
creado_por: usuario
asignado_a: agente
creado: 2026-11-03
actualizado: 2026-11-03
depende_de: []
bloqueado_por: null
version_schema: 1
---

## Contexto

The 1.0 success criterion is a stereo mixer: import/record audio, per-track
volume, pan, fades and a basic effects chain, with playback and export in sync
with video. ffmpeg decodes/encodes; the mixer must run off the same rational
frame clock as the evaluator (D-0007/D-0010). No advanced mastering.

## Criterios de aceptacion
- [ ] Decode audio through ffmpeg into a deterministic sample buffer
- [ ] Track model with volume, pan and fades, keyable and overridable
- [ ] Basic effects chain (at least EQ and compressor) with an ordered stack
- [ ] Preview playback stays in sync with the composition frame clock
- [ ] Export muxes the mixed stereo track into the MP4

## Notas del agente

Backlogged at the end of prompt 10 (Phase 12). No implementation yet.
