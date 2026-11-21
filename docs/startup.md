# Startup

Motioner opens its window only when the first real frame is ready: no white
or black flash, no half-drawn start screen. The GPU driver floor (instance,
device and swapchain) is still there; everything that is ours is overlapped,
cached or kept off the critical path.

The engine side of this policy (backends, caches, show-when-ready, the trace)
is documented in
[`engine/ferrous/crates/ferrous_engine/docs/startup.md`](../engine/ferrous/crates/ferrous_engine/docs/startup.md).
The project decision is D-0062.

## Measuring

```text
cargo build --release -p motioner_app --bins
python scripts/startup_bench.py --runs 20
```

The script runs `--start --exit-after-frames 1` and
`preview examples/demo.motioner --exit-after-frames 1`, parses the engine's
JSON startup trace and prints median/p90 per mark, per span and for the
closing path. It needs no special build: the trace is enabled by the
environment variables the script sets.

Cold runs (empty font and pipeline caches):

```text
python scripts/startup_bench.py --cold --runs 5 \
  --clear-cache "%LOCALAPPDATA%\Motioner\cache\fonts" \
  --clear-cache "%LOCALAPPDATA%\Motioner\cache\pipelines"
```

The integrated GPU is measured with `WGPU_POWER_PREF=low` in the environment.

Reference numbers (release, 1600x900, RTX 4050, S6, 20 warm runs) are in
`out/reports/PHASE-17-REPORT.md`: first frame 471 ms for the start screen and
624 ms for `examples/demo.motioner`; the driver floor is ~438-484 ms
(`engine: surface configured`).

## Regression guard

```text
python scripts/startup_bench.py --check
```

The reviewer runs this on the reference machine after a release build. It
runs five processes per scenario and exits non-zero when:

- the window becomes visible before its first frame on the normal path
  (`engine: window visible` earlier than `engine: first frame presented`);
  the safety-timeout path is the only accepted exception and has its own
  mark;
- one of the budgets in `scripts/startup_budgets.json` is exceeded or the
  metric disappears from the trace.

The budgets are 2x the S6 medians/p90 (minimum 5 ms for sub-millisecond
spans) and the 50 ms close-to-hidden target. The critical-path budget is
`first frame presented - surface configured`, so it excludes the GPU driver
floor: a slower driver does not fail the guard. It is deliberately outside
the CI gate (Defender scans and GPU state make a wall-clock threshold
unreliable in CI).

## What runs before the first frame

- `Host::new`, recents and the `examples/` listing: listings and cheap
  lookups only, budgeted at 10 ms (0.94 ms measured in S5).
- In parallel with the GPU: font bakes, document load, image decode and the
  audio device open, on workers.
- After the first frame: ffprobe and video thumbnails, the audio mix/play,
  and any 3D resources (Flat2D builds only 2D).
- Asynchronous media shows a placeholder without layout shift and swaps in
  when ready.

## Console vs desktop binary

`motioner.exe` is a Windows GUI-subsystem binary: double-clicking it opens the
editor with no console window. A GUI process is asynchronous to `cmd`, which
does not wait for it and does not hand it `>` redirection handles.

For terminal use, build/run the console companion:

```text
motioner-cli render examples/demo.motioner --out demo.mp4
motioner-cli info examples/hello.motion > info.txt
echo %ERRORLEVEL%        # cmd
$LASTEXITCODE            # PowerShell
```

`motioner-cli.exe` is built from the same crate and library as
`motioner.exe`; it runs the same CLI in-process with the console subsystem.
Both binaries are produced by `cargo build --release -p motioner_app --bins`.
To keep the short name `motioner` in a packaged build, copy
`motioner-cli.exe` to `motioner.com` next to `motioner.exe`: Windows resolves
`.com` before `.exe`, so `motioner info ... > out.txt` becomes the synchronous
console path while the desktop shortcut still targets `motioner.exe`.

The engineering convention is recorded in REF-0002 §11.
