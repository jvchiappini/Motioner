---
id: T-PERF-0004
titulo: Video playback: decode-ahead worker and release performance (prompt 11 phase 7a)
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

Phase 6 measured 120 dropped frames in 5 s at 1080p60 (debug, 1600x900).
The preview decoded every video frame synchronously on the UI thread through
a new `ffmpeg` process per frame. Phase 7a re-measures in release, moves the
decode to a worker with decode-ahead into the bounded cache, and adds a
headless regression for the batch/stream path.

## Criterios de aceptacion
- [x] Release baseline and after numbers recorded with the same clip and
      window size (1600x900): UI fps, dropped frames, working set, private
      bytes and video cache size.
- [x] Decode runs on a background worker; the UI thread only looks up the
      bounded cache and queues a miss.
- [x] The worker keeps one sequential `ffmpeg` stream open while the playhead
      moves forward and decodes a lookahead batch into the LRU.
- [x] `render`/`still` still decode the original at full resolution and the
      existing goldens stay byte-identical.
- [x] A headless regression (`motioner_io/tests/video_decode_ahead.rs`)
      asserts a 30-frame batch and a persistent stream inside a time budget.
- [x] Gate green.

## Notas del agente

- `motioner_io::video`: new `decode_frames` (batch) and `FrameStream`
  (long-lived sequential decode, `-threads 1` for the preview worker).
- `motioner_app::video_cache`: `VideoCache` now owns a worker thread and a
  mutex-protected LRU; `decode` returns `Ready`/`Pending`/`Unavailable`
  instead of blocking. `LOOKAHEAD_FRAMES = 24`; the worker keeps a stream
  open across batches and restarts only on a seek, a mode change or EOF.
- `motioner_ui::ProjectHost::video_frame` returns `VideoFrameState`; the
  canvas keeps the previous texture while a frame is pending and retries.
- The transport uses the wall clock when the mix is silent
  (`AudioOutput::position` returns `None`); a device clock that is not running
  no longer slows playback for video-only compositions. The output stream asks
  for a 1024-frame fixed period and falls back to the device default, so the
  cursor does not arrive in large bursts.
- The per-frame `Project::open` in the video path is cached by manifest mtime.
- Release, 1600x900, `out/p11-video/proxy.motioner` (1080p60 source, 3 s
  composition): baseline 112 dropped in ~5 s, 346.7 MB working set, 732.5 MB
  private, 42 MB video cache. After: cold start 4-5 dropped, then **0 in the
  steady-state 5 s window**, 59-60 UI fps, ~101 MB working set, ~718 MB
  private, 42 MB video cache, eval 0.33-0.58 ms.
- Demo preview (1280x720, 6 s, generated audio): 59-60 UI fps, 0 dropped in
  the steady-state 5 s window, ~379 MB working set, ~747 MB private, 42 MB
  video cache. The playhead follows the audio device; on the reference
  automation host the device clock runs slower than wall time, so the demo
  reaches the end later than 6 s. That is a device-clock property, not a UI
  stall.
- Known residual: the first 1-2 s after play include a one-time paused
  full-resolution texture to proxy texture resize and the worker startup
  (4-5 skipped transport frames in the cold-start window).
