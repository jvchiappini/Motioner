---
id: INITIATIVE-0017
nombre: "Audio engine and mixer"
phase: PHASE-0003
estado: backlog
prioridad: alta
tamano_estimado: large
depende_de: ["INITIATIVE-0009"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Audio engine and mixer

## Description

Provide a real audio pipeline for NLE and motion graphics: stereo tracks,
a mixer, basic effects and sample-accurate sync with video. Advanced
mastering is post-1.0.

## Scope

### Deliverables
- Audio decode, resampling and a sample-accurate playback clock.
- Stereo mixer with volume, pan, mute, solo and fades.
- Basic effects chain: EQ, compressor and limiter.
- Waveform display and scrubbing.
- Audio export muxed into MP4 (AAC).
- Loudness-safe export defaults.

### Non-goals
- Surround and immersive audio.
- Advanced mastering, stem delivery and broadcast loudness compliance.
- Audio plug-ins in 1.0.

## Technical approach

A graph of tracks and effects processes blocks on an audio thread with a
lock-free command queue. Video sync follows the audio clock during
playback. The offline mixdown uses the same graph and deterministic
parameters.

Key considerations:
- 48 kHz stereo in 1.0.
- No allocations on the audio callback.
- Underruns are reported, never silent.
- The effect graph is animatable and serializable.

## Dependencies

- INITIATIVE-0016: NLE timeline.
- Audio library choice by spike.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0009 | 1.0 release candidate | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] Audio plays in sync with video during scrubbing and playback.
- [ ] Volume, pan, fades, EQ, compressor and limiter work.
- [ ] Export muxes AAC audio with correct duration and sync.
- [ ] The audio thread never allocates or locks.

## Notes

No advanced mastering in 1.0; the goal is clean, in-sync audio.
