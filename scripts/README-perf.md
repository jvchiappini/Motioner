# Performance harness (prompt 18 P7)

`scripts/perf_bench.py` measures the release binary with `psutil` and parses
the engine's JSON summary lines. `scripts/perf_budgets.json` is the guard that
the normal gate runs; `scripts/bench_baselines.json` is the recorded divan
reference (not an automatic gate).

## Normal gate

The budget check joins the gate next to the tests that already enforce the
policy:

- `cargo test --workspace` runs the zero-allocation tests
  (`motioner_ui/tests/zero_alloc.rs`, the engine UI/Pure2D equivalents), the
  idle-runner tests and the engine flat-inventory test
  (`ferrous_renderer/tests/memory_flat.rs`).
- `python scripts/perf_bench.py --from-json out/reports/phase18/p7-after-clean.json --check scripts/perf_budgets.json`
  fails on a regression beyond the recorded median + 15 % margin. It needs a
  recorded release payload, so the reviewer builds once and checks the JSON
  (or runs the full measurement with `--check` in the same command).
- `scripts/bench_baselines.json` records the divan numbers; compare them with
  `cargo bench -p ferrous_renderer --bench subsystems`.

## Reviewer commands

Build once:

```powershell
cargo build --release -p motioner_app --bins
cargo build --release -p motioner_app --bin motioner-bench --features alloc-stats
```

Full measurement (three back-to-back medians, pointer scenarios included):

```powershell
python scripts/perf_bench.py `
  --binary target/release/motioner.exe `
  --alloc-binary target/release/motioner-bench.exe `
  --runs 3 --allow-pointer `
  --json-out out/reports/phase18/p7-after.json --force
```

Enforce the budgets against that recorded result (no app run):

```powershell
python scripts/perf_bench.py `
  --from-json out/reports/phase18/p7-after.json `
  --check scripts/perf_budgets.json
```

Or run and check in one command (same flags as above plus `--check`). The
check fails when a budgeted scenario or metric is missing, when a median is
above `max`, or when it is below `min`; it prints `check FAILED:` with every
violation and exits 1. A run without `--allow-pointer` skips the hover and
layers-scroll scenarios, so their budgets report `no measurement`.

Regenerate the budgets from a recorded result after the fixes:

```powershell
python scripts/perf_bench.py `
  --from-json out/reports/phase18/p7-after.json `
  --write-budgets scripts/perf_budgets.json --margin 0.15
```

`--margin` defaults to 0.15 (the P0 reference table reproduced within 15 %).
`--write-budgets` prints the derived file. `--from-json` never runs the app.

## Budget schema

```json
{
  "noise_margin": 0.15,
  "notes": ["..."],
  "scenarios": {
    "editor_hover": {
      "cpu_percent": {"max": 10.675},
      "cpu_ms_per_presented_frame": {"max": 0.2},
      "fps": {"min": 31.495},
      "damage_fraction": {"max": 1.0},
      "gpu_frame.draw_calls": {"max": 64.4}
    }
  }
}
```

- Every metric accepts `max`, `min`, or both.
- Dotted metric names read nested result objects, e.g.
  `gpu_frame.draw_calls`, `gpu_frame.pipeline_binds`,
  `gpu_frame.buffer_binds`, `stages_ms.render-command collection`.
- An entry may instead carry `baseline`, optional `noise_margin` (per metric)
  and `direction` (`"max"` by default, `"min"` for throughput). The bound is
  then `baseline * (1 +/- margin)`, raised to an optional `floor`. The
  top-level `noise_margin` is the fallback.
- `--write-budgets` writes explicit `max`/`min` values:
  - `cpu_percent`, `cpu_ms_per_presented_frame`, `collection_ms`,
    `frame_ms_p99`, `input_to_present_p50`, `input_to_present_p99` and
    `damage_fraction` use `max(0.02, value * 1.15)`;
  - `cpu_ms_per_presented_frame` is a budget only when the run presents at
    least 30 frames; `start_idle`, `editor_idle` and `editor_hover_static`
    report the value without a guard (a handful of frames is noise);
  - `damage_fraction` is clamped to 1.0;
  - `idle_cpu_percent` and `tail_cpu_percent` use a 1 % absolute floor (the
    NVIDIA driver wakes 0.1-0.85 % of a core on the reference machine);
  - `private_mb` uses a 1.0 MB floor and `vram_mb` a 0.1 MB floor;
  - `fps` becomes a `min` at `value * 0.85` when the scenario actually
    animates (`fps >= 1`);
  - `dropped_frames` is pinned at `max: 0`;
  - `allocations_per_frame` is included only when the alloc build ran;
  - `export_120` is excluded with a note because its throughput is noisy.
- `scripts/perf_budgets.json` currently contains the 10 guard scenarios
  derived from `out/reports/phase18/p7-baseline.json` (the 11 measured minus
  `export_120`) with `noise_margin: 0.15`; `playback_4k30` is added once
  `p7-after.json` exists.

## Scenarios and new fields

The twelve scenarios are start idle, editor idle, moving hover, static hover,
timeline scrub, 1080p playback, 4K30 playback, heavy UI text, the 3000/1000
layer list scroll, the heavy composition and the 120-frame export.

`playback_4k30` generates a 3840x2160, 30 fps `.motioner` project (reusing the
demo's test video and its proxy when available) and previews it at
`--window-size 1920x1080`, so the preview proxy path is exercised. It records
`dropped_frames` and the decode-ahead hit rate.

## Deterministic input (prompt 15 P4B fix)

The CPU scenarios used to post with `PostMessageW`, and Windows coalesced a
different fraction of the moves depending on how fast the build pumped its
message queue: the identical 30 s run presented 743 or 1 156 hover frames and
CPU followed the frame count. The harness fixes the delivery and keeps the
workload for the whole measured window:

- `post_mouse_move` uses `SendMessageW`: every move is processed once;
- `editor_hover` waits `HOVER_SETTLE_S = 2 s` for the project to open, parks
  the real cursor outside the window (a physical mouse over the app injects
  extra moves), restores it, and then sends a move every 16 ms
  (`HOVER_INTERVAL_S`) until the process exits. There is no event cap, so the
  tail is the same workload as the rest of the window;
- `timeline_scrub` sends a `seek()` every 60 ms (`SCRUB_INTERVAL_S`) through
  the control endpoint until the process exits. Each seek is one TCP
  request/reply on a single localhost connection (the endpoint protocol is one
  request per connection), not a `motioner ctl` process per seek: the
  process-per-seek loop paced at ~110 ms and kept the seek load in the tail;
- the input-to-present probe lives in its own `heavy_ui_probe` run; the
  `heavy_ui` CPU/frame numbers come from a run with no synthetic input, and
  the harness copies the probe's input fields into `heavy_ui` before `--check`;
- every run reports `input_events_sent` next to `frames`, so a bisect can see
  whether the input population changed.

Every result also carries `cpu_ms_per_presented_frame` (total CPU seconds over
the presented frames, or the drawn frames when a headless run presents none).
It does not move with how long the machine took to present, so it is the guard
that catches an engine regression under host load, together with the CPU and
tail percentages.

The heavy_ui fixture line had one pre-existing double-encoded em dash; it is
now a plain em dash. `scripts/check_encoding.py` rejects that class of mojibake
from now on, with the prompt-15 review section allowlisted because it quotes
the broken sequences on purpose.

The harness now parses two optional engine lines:

- `memory-owners {json}` -> `memory_owners` plus
  `memory_owners_document_bytes`, `..._plan_bytes`, `..._panel_bytes`,
  `..._video_bytes`, `..._total_bytes`;
- `video-cache {json}` -> `decode_ahead` plus `decode_ahead_requests`,
  `decode_ahead_hits`, `decode_ahead_misses`, `decode_ahead_hit_rate`,
  `decode_ahead_bytes`.

The per-scenario line prints `decode-ahead hit rate ... | memory owners:
document ... plan ... panel ... video ...`. A binary that does not print the
lines reports `not reported` and the run continues.

## Recorded bench baselines

Run the divan subsystem benches and compare by hand with
`scripts/bench_baselines.json`:

```powershell
cargo bench -p ferrous_renderer --bench subsystems
# Optional divan positional filter (matches bench names):
cargo bench -p ferrous_renderer --bench subsystems -- ui_layout
```

P0 numbers (Windows, release, RTX 4050): `ui_layout_large_tree` 8192 5.451 ms
(14.18 MB one-shot); `gui_batch_quads`/`gui_batch_glyphs` 16k ~165 us, 0
allocs; `text_layout_line` long 4.599 us, 0 allocs; MSDF ASCII 3.295 ms, 213
allocs, 1.80 MB; tessellation 0 allocs; `pure2d_heavy_mixed` 2048 328 us, 87
allocs; `svg_mesh_upload` 2.023 us, 6 allocs. There is no separate engine
bench target; `ferrous_renderer`'s `subsystems` is the only `[[bench]]`.

## Notes

- Three-run medians and the min/max ranges are unchanged; `--runs 1` is still
  useful for a quick probe, but budgets are meant for `--runs 3`.
- `--from-json` ignores `--json-out`/`--force`; it is read-only.
- Idle accounting still excludes the last `IDLE_EXCLUDE_S` seconds of
  teardown and still reports both idle labels.
