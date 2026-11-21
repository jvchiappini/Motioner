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
  - `cpu_percent`, `collection_ms`, `frame_ms_p99`, `input_to_present_p50`,
    `input_to_present_p99` and `damage_fraction` use
    `max(0.02, value * 1.15)`;
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
