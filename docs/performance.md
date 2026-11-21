# Performance

Motioner follows the engine's performance policy (D-0063): a frame is drawn
only when something changed, a steady-state frame allocates nothing, every
cache has a byte budget and an owner, and every claim is a measured number.
The engine's own page is `ferrous_engine/docs/performance.md`.

## Instruments

- `FERROUS_TRACE_FRAMES=1` makes the engine print one `frame-trace` JSON line
  at exit: the nine frame stages (median/p90/p99), `frame_ms`, the per-frame
  GPU counters (draw calls, passes, pipeline/bind-group/buffer binds,
  scissors, GUI segments), `input_to_present_ms`, `ui_damage`, the memory
  report and the per-subsystem inventory.
- `MOTIONER_MEMORY_REPORT=1` (or `--memory-report`) adds one
  `memory-owners` line: document, plan, panel-cache and video-cache bytes.
  The video cache prints a `video-cache` line with the decode-ahead hit rate.
- `FERROUS_TRACE_STARTUP=1` keeps the prompt 17 startup marks and spans.
- `scripts/perf_bench.py` runs the eleven scenarios in release: idle start and
  editor, hover, static hover, scrub, 1080p and 4K30 playback, the heavy UI
  text fixture, the 3,000/1,000-layer lists, the heavy composition and the
  120-frame export.

## Commands

```powershell
cargo build --release -p motioner_app --bins
python scripts/perf_bench.py --binary target/release/motioner.exe `
  --runs 3 --warmup 1 --allow-pointer `
  --json-out out/reports/phase18/p7-after.json --force
python scripts/perf_bench.py --from-json out/reports/phase18/p7-after.json `
  --check scripts/perf_budgets.json
```

The budget file is the measured median plus the P0 15 % noise margin, with
absolute floors for near-zero metrics. `scripts/README-perf.md` documents the
full loop, `--write-budgets` and the recorded divan baselines.

## Rules for frame code

- Do not allocate. Reuse buffers with `clear()`, grow geometrically, and only
  shrink by an explicit trim policy. No `format!`, `to_string`, `Vec::new`,
  `collect` or heap `clone` on a frame path; intern ids and keep strings by
  value.
- Do not redraw when nothing changed. Request a frame from an event, a
  scheduled `redraw_at`, a worker wake or a resize/scale/theme change. Keep
  `RedrawPolicy::Continuous` only for games, demos and benchmark fixtures.
- Cache by the full key. A `PanelKey` revision covers every input the batch
  reads. An input that changes the value must change the key; a test changes
  each input once.
- Budget every cache. A new cache names its byte budget, its eviction policy
  and its inventory row.
- Use `rustc-hash` with integer or interned-id keys on hot maps. SipHash is for
  cold or security-sensitive work.
- Prefer a library; replace it only when a bench on our workloads shows a
  clear, repeatable win and the result stays readable. Record the margin.

## P7 reference numbers (release, 1600x900, median of three)

| Scenario | CPU % | frames / fps | private MB | VRAM MB | collection ms | damage |
|---|---|---|---|---|---|---|
| start idle | 2.33 (idle 0.43, tail 0.32) | 1 presented | 312.8 | 7.6 | - | 0 % |
| editor idle | 1.75 (idle 0.05, tail 0.0) | 1 presented | 338.6 | 19.3 | - | 100 % |
| hover | 5.78 [3.76-6.28] | 719 | 351.6 | 21.2 | 0.098 | 3.3 % |
| scrub | 6.96 [5.97-7.16] | 283 / 9.3 | 411.9 | 24.3 | 1.735 | 10.1 % |
| playback 1080p | 18.23 | 600 / 54.4, 0 dropped | 409.9 | 21.7 | 0.468 | 8.5 % |
| playback 4K30 (proxy) | 11.96 | 300 / 27.8, 0 dropped | 406.3 | 13.8 | 0.462 | 8.5 % |
| heavy UI | ~54.9 | 601 / 50.8 | 436.8 | 22.0 | 4.240 | 3.3 % |
| heavy UI list 3,000 | 9.24 | 4,471 / 59.1 | 333.0 | 7.6 | 0.154 | 14.2 % |
| heavy UI list 1,000 | 8.41 | 2,672 / 58.9 | 321.2 | 7.6 | 0.112 | 14.2 % |
| heavy composition | 9.96 | 601 / 55.5 | 383.7 | 50.8 | 0.103 | 3.3 % |
| export 120 | 29.6 | 120 / 17.02 fps | 380.1 | - | - | - |

Decode-ahead hit rate: 95.0 % at 1080p60, 89.6 % at 4K30. The offscreen
persistent present path is opt-in and **off by default**: it measured 31.8 %
worse on scrub and 66.8 % worse on hover in a clean back-to-back A/B, because
it still repaints and copies the whole frame. A true scissored partial update
is the change that would make it win.

The 3,000-layer document now compiles in **0.46 s / 23 MB peak** (it was
28.6 s / 488 MB): the MDF origin pass is linear instead of cubic.
