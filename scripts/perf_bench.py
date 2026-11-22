#!/usr/bin/env python3
"""Measure Motioner release scenarios (prompt 18 P7).

Runs the twelve baseline scenarios, samples the process with ``psutil`` and
parses the engine's ``frame-trace``/``memory-report`` JSON lines.  For every
scenario it reports CPU % of one core, frames drawn, GPU ms per pass, the
per-stage medians, allocations per frame (with a bench build), working set,
private bytes, VRAM, threads, the decode-ahead hit rate and the memory-owner
bytes when the binary prints them.

Requirements: ``pip install psutil`` (the only non-stdlib dependency) and a
release build:

    cargo build --release -p motioner_app --bins
    # allocation runs only:
    cargo build --release -p motioner_app --bin motioner-bench --features alloc-stats

Usage:

    python scripts/perf_bench.py --binary target/release/motioner.exe
    python scripts/perf_bench.py --alloc-binary target/release/motioner-bench.exe
    python scripts/perf_bench.py --check scripts/perf_budgets.json
    python scripts/perf_bench.py --write-budgets scripts/perf_budgets.json
    # Re-derive budgets from a recorded result without running the app:
    python scripts/perf_bench.py --from-json out/reports/phase18/p7-after.json \
        --write-budgets scripts/perf_budgets.json --margin 0.15

The scenarios:

1. start screen idle 30 s;
2. editor with ``examples/demo.motioner`` idle 30 s;
3. editor hover (the cursor is moved across the window);
4. editor hover inside one static panel area;
5. timeline scrubbing through the control endpoint;
6. 1080p playback of the demo for 10 s;
7. 4K30 playback: a generated 3840x2160 ``.motioner`` project with a video,
   previewed at ``--window-size 1920x1080`` so the preview proxy is used
   (dropped frames and the decode-ahead hit rate are recorded);
8. heavy UI (a generated 3000-row text composition);
9. heavy UI layers list: a generated 3000-layer ``.motioner`` project whose
   layers panel is scrolled at a steady rate for 30 s (target collection
   <= 2 ms; flat 1k vs 3k);
10. heavy UI layers list, 1000 layers: the same project at a tenth of the row
    count, to check that collection stays flat;
11. heavy composition (``examples/showcase_pro.motion``);
12. export of 120 frames.

All scenarios run in release.  ``--warmup`` runs (default 1) are discarded.
The frame trace is enabled with ``FERROUS_TRACE_FRAMES=1``; the engine prints
one summary at exit and one memory inventory line.

Three-run rule: the P6 numbers are medians of three back-to-back measured
runs (``--runs 3``).  ``aggregate`` keeps the median in the existing scalar
fields and adds a ``ranges`` object with the ``min``/``max`` of every scalar
metric across those runs.  ``print_result`` always prints CPU %, fps and
collection as ``median [min-max]``; a bare CPU % is never printed.

Idle accounting excludes the last ``IDLE_EXCLUDE_S`` seconds before the
process exits: the close path tears down the GPU, audio and worker threads,
and that CPU would otherwise land in the final sample and masquerade as idle
work.  Both idle numbers are reported and labelled explicitly:

- ``idle_after_first_cpu_percent`` (alias ``idle_cpu_percent``): CPU after the
  first presented frame, up to the end of the measured window;
- ``idle_tail_cpu_percent`` (alias ``tail_cpu_percent``): CPU over the last
  measured tail (the final 15 s of a 30 s run).

The same result carries ``idle_excluded_s`` (teardown seconds dropped, alias
``idle_excluded_teardown_s``) and ``idle_frames_after_first`` (``frames - 1``,
alias ``idle_after_first_frames``, because the trace's frame count is
cumulative and includes the first presented frame).  A tail above 0.1 % of a
core is printed as an explicit FINDING line.

P6 trace fields: the engine adds ``input_to_present_ms``
(``{median, p50, p90, p99, samples}``) and ``ui_damage``
(``{median, p90, p99}``) to the frame-trace JSON.  The harness parses them
into ``input_to_present`` / ``damage_fraction`` (plus flat scalar fields for
aggregation and ranges) and prints the damaged fraction as a percentage of
the window and the input-to-present p50/p99 in ms.  When an older binary does
not emit the fields the harness prints ``not reported`` instead of failing.

Input-to-present population (prompt 15 phase 4).  A p99 over one event is not
a measurement, so every measured run must feed at least
``MIN_INPUT_SAMPLES`` (30) input instants to presented frames.  ``heavy_ui``
therefore runs a pointer probe (``--allow-pointer``): one ``WM_MOUSEMOVE``
every ~16 ms between two points in the title bar, while the scene stays
still.  A run with fewer samples prints an explicit FINDING and
``--check`` fails every ``input_to_present*`` metric for that scenario
("fewer than 30 samples").  The probe is part of the same run, so
``heavy_ui.frame_ms_p99`` is re-checked with it enabled.

The heavy UI layers fixtures are generated under ``FIXTURES`` as real
``.motioner`` project directories (``project.motion`` plus
``compositions/main.motion``): one hidden rect node per layer row, no
grouping.  They are driven with ``preview --paused --continuous
--exit-after-seconds 30 --window-size 1600x900``; while the window is raised
the harness posts ``WM_MOUSEWHEEL`` messages over the layers panel every
~16 ms (gated on ``--allow-pointer``, like the hover scenarios).  The run is
steady, one wheel event per tick, not a burst.

P7 trace lines: when the binary prints ``memory-owners {json}`` (document,
plan, panel and video bytes) and ``video-cache {json}`` (``requests``,
``hits``, ``misses``, ``hit_rate``, ``bytes``) the harness parses them into
``memory_owners`` / ``decode_ahead`` plus flat scalar fields for aggregation
and ranges (``memory_owners_document_bytes``, ``decode_ahead_hit_rate``, ...).
An older binary that does not print them yields ``not reported`` and does not
fail the run.

``--check BUDGETS`` enforces a budget JSON.  The schema is deliberately small:

    {
      "noise_margin": 0.15,
      "scenarios": {
        "editor_hover": {
          "cpu_percent": {"max": 10.675},
          "fps": {"min": 31.495},
          "damage_fraction": {"max": 1.0},
          "gpu_frame.draw_calls": {"max": 64.4}
        }
      }
    }

Every metric may carry ``max``, ``min`` or both.  Metric names may use dotted
paths into nested result objects (``gpu_frame.draw_calls``,
``stages_ms.render-command collection``).  A metric entry may instead carry a
recorded ``baseline`` plus an optional ``noise_margin`` (fraction) and
``direction`` (``"max"`` by default, ``"min"`` for throughput); the limit is
then derived as ``baseline * (1 +/- margin)`` with an optional ``floor``.  The
top-level ``noise_margin`` is the fallback for those entries.  A missing
scenario or metric, a value above ``max`` or below ``min`` all fail the check;
the failure list is printed and the process returns 1.

``--write-budgets PATH`` derives that file from the measured (or
``--from-json`` recorded) results.  ``--margin`` defaults to 0.15, the P0
spread ceiling.  Near-zero metrics get an absolute floor instead of a useless
percentage: collection, frame-time p99, input-to-present and damage use at
least 0.02 (a damage fraction is clamped to 1.0), idle/tail CPU uses 1 % of a
core (the NVIDIA driver wakes 0.1-0.85 % on the reference machine),
``private_mb`` at least 1.0 and ``vram_mb`` at least 0.1.  Throughput
(``fps``) becomes a ``min`` at ``value * (1 - margin)``; ``dropped_frames`` is
pinned at ``max: 0``.  The noisy ``export_120`` scenario is skipped with a
note rather than a loose budget.  ``--from-json`` reads a recorded result
payload and never runs the app, so budgets can be regenerated after a
measurement without a rebuild.

Bench baselines: the divan numbers recorded in P0 live in
``scripts/bench_baselines.json`` (ui_layout 8192 5.451 ms, gui_batch 16k
~165 us, text layout 4.599 us, MSDF ASCII 3.295 ms / 213 allocs / 1.80 MB,
tessellation 0 allocs, pure2d heavy 2048 328 us / 87 allocs, SVG upload
2.023 us / 6 allocs).  They are a recorded reference, not an automatic gate;
re-run ``cargo bench -p ferrous_renderer --bench subsystems`` and compare in
``scripts/README-perf.md``.

A baseline output is never overwritten implicitly: an existing ``--json-out``
needs ``--force``.

CPU that remains after the teardown exclusion is not Motioner waking up.  A
per-thread start-address attribution of the residual ticks maps them to the
NVIDIA user-mode driver threads (``nvoglv64.dll`` / ``nvwgf2umx.dll``): one
to three of them per 30 s plus the teardown, with no thread of ours waking
while the window is idle.  It is a property of the driver, not a regression
in the editor loop, so it is reported and not "fixed".
"""

from __future__ import annotations

import argparse
import ctypes
import ctypes.wintypes as wt
import json
import math
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

try:
    import psutil
except ImportError:  # pragma: no cover - environment guard
    print("perf_bench needs psutil (pip install psutil)", file=sys.stderr)
    raise SystemExit(2)

ROOT = Path(__file__).resolve().parent.parent
WORK = ROOT / "out" / "reports" / "phase18"
FIXTURES = WORK / "fixtures"
# Seconds before process exit excluded from idle/tail CPU: the close path
# tears down the GPU, audio and workers and would otherwise be counted as
# idle work in the final sample.
IDLE_EXCLUDE_S = 0.5
# A single input-to-present sample is not a measurement: every run that reports
# the metric must feed at least this many input events to the presented frames.
MIN_INPUT_SAMPLES = 30

USER32 = ctypes.windll.user32
KERNEL32 = ctypes.windll.kernel32
SW_RESTORE = 9
SWP_NOSIZE = 0x0001
SWP_NOMOVE = 0x0002
SWP_SHOWWINDOW = 0x0040
HWND_TOPMOST = ctypes.c_void_p(-1)
HWND_NOTOPMOST = ctypes.c_void_p(-2)
USER32.GetWindowThreadProcessId.argtypes = [wt.HWND, ctypes.POINTER(wt.DWORD)]
USER32.IsWindowVisible.argtypes = [wt.HWND]
USER32.IsWindowVisible.restype = wt.BOOL
USER32.EnumWindows.argtypes = [ctypes.c_void_p, wt.LPARAM]
USER32.EnumWindows.restype = wt.BOOL
USER32.GetClientRect.argtypes = [wt.HWND, ctypes.c_void_p]
USER32.ClientToScreen.argtypes = [wt.HWND, ctypes.c_void_p]
USER32.GetWindowRect.argtypes = [wt.HWND, ctypes.c_void_p]
USER32.ShowWindow.argtypes = [wt.HWND, ctypes.c_int]
USER32.SetForegroundWindow.argtypes = [wt.HWND]
USER32.SetWindowPos.argtypes = [wt.HWND, ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wt.UINT]
USER32.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
USER32.GetCursorPos.argtypes = [ctypes.c_void_p]
USER32.PostMessageW.argtypes = [wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM]
USER32.PostMessageW.restype = wt.BOOL

WM_MOUSEMOVE = 0x0200
WM_MOUSEWHEEL = 0x020A
WHEEL_DELTA = 120
# Wheel lines per event. One row per event at one event per 16 ms is a steady
# ~62 rows/s: the 3000-row list stays inside its scroll range for the whole
# 30 s run and the 1000-row list for about half of it.
WHEEL_LINES = 1


def post_mouse_move(hwnd: int, x: int, y: int) -> None:
    """Deliver one `WM_MOUSEMOVE` with client coordinates to `hwnd`.

    The static-hover numbers must measure the app's own pointer path. Moving
    the real cursor does not guarantee that: a window that is not foreground
    (or is covered) never receives `WM_MOUSEMOVE`, and Windows coalesces moves
    when the queue is not pumped. Posting the message makes the delivery
    deterministic and independent of the desktop while the app still sees a
    normal `CursorMoved` with the same coordinates.
    """
    lparam = ((y & 0xFFFF) << 16) | (x & 0xFFFF)
    USER32.PostMessageW(hwnd, WM_MOUSEMOVE, 0, lparam)


def enable_dpi_awareness() -> None:
    """Make window and cursor coordinates physical pixels.

    The hover scenarios compute the client box with ``GetClientRect`` and
    ``ClientToScreen`` and post client-relative coordinates back with
    ``PostMessage``.  A DPI-unaware process gets virtualised coordinates, so
    on a scaled display they would land elsewhere.  Per-monitor-v2 awareness
    keeps both sides in physical pixels.
    """
    try:
        USER32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
        return
    except (AttributeError, OSError):
        pass
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(2)
    except (AttributeError, OSError):
        pass


class POINT(ctypes.Structure):
    _fields_ = [("x", wt.LONG), ("y", wt.LONG)]




class RECT(ctypes.Structure):
    _fields_ = [
        ("left", wt.LONG),
        ("top", wt.LONG),
        ("right", wt.LONG),
        ("bottom", wt.LONG),
    ]


def find_window(pid: int) -> int | None:
    found: list[int] = []
    callback_type = ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)

    def callback(hwnd: int, _lparam: int) -> bool:
        owner = wt.DWORD()
        USER32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and USER32.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True

    USER32.EnumWindows(callback_type(callback), 0)
    best = None
    best_area = 0
    for hwnd in found:
        rect = RECT()
        USER32.GetWindowRect(hwnd, ctypes.byref(rect))
        client = RECT()
        if not USER32.GetClientRect(hwnd, ctypes.byref(client)):
            continue
        if client.right < 320 or client.bottom < 240:
            continue
        area = (rect.right - rect.left) * (rect.bottom - rect.top)
        if area > best_area:
            best, best_area = hwnd, area
    return best


def client_box(hwnd: int) -> tuple[int, int, int, int] | None:
    rect = RECT()
    if not USER32.GetClientRect(hwnd, ctypes.byref(rect)):
        return None
    width = rect.right - rect.left
    height = rect.bottom - rect.top
    origin = wt.POINT(0, 0)
    USER32.ClientToScreen(hwnd, ctypes.byref(origin))
    if width < 320 or height < 240:
        return None
    return (origin.x, origin.y, origin.x + width, origin.y + height)


def raise_window(hwnd: int) -> None:
    USER32.ShowWindow(hwnd, SW_RESTORE)
    USER32.SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOSIZE | SWP_NOMOVE | SWP_SHOWWINDOW)
    USER32.SetForegroundWindow(hwnd)
    USER32.SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOSIZE | SWP_NOMOVE)


def hover_loop(pid: int, stop: threading.Event) -> None:
    """Move a synthetic pointer over the app's client area until `stop`.

    Only runs when the caller passed ``--allow-pointer``. It raises the window
    and posts `WM_MOUSEMOVE` messages with client coordinates; the real mouse
    is never moved, so the run cannot disturb the desktop.
    """
    hwnd = None
    while not stop.is_set() and hwnd is None:
        hwnd = find_window(pid)
        if hwnd is None:
            time.sleep(0.05)
    if hwnd is None:
        return
    raise_window(hwnd)
    box = client_box(hwnd)
    if box is None:
        return
    left, top, right, bottom = box
    while not stop.is_set():
        for step in range(20):
            x = int((right - left) * (0.1 + 0.8 * (step % 10) / 9.0))
            y = int((bottom - top) * (0.15 + 0.7 * ((step // 10) % 2)))
            post_mouse_move(hwnd, x, y)
            if stop.wait(0.016):
                return


def hover_static_loop(pid: int, stop: threading.Event) -> None:
    """Move the cursor inside one static panel area until `stop`.

    Every position hits the same target, so the editor's pointer-move hook
    must not request a frame. Only runs with ``--allow-pointer``; the cursor
    is restored.
    """
    hwnd = None
    while not stop.is_set() and hwnd is None:
        hwnd = find_window(pid)
        if hwnd is None:
            time.sleep(0.05)
    if hwnd is None:
        return
    raise_window(hwnd)
    box = client_box(hwnd)
    if box is None:
        return
    left, top, right, bottom = box
    # The status bar: a static strip with no hover target.
    x0 = int((right - left) * 0.40)
    y0 = int((bottom - top) * 0.975)
    step = 0
    while not stop.is_set():
        post_mouse_move(hwnd, x0 + (step % 20), y0 + ((step // 20) % 20))
        step += 1
        if stop.wait(0.016):
            return


def input_probe_loop(pid: int, stop: threading.Event) -> None:
    """Deliver one pointer move per ~16 ms so frames answer real input.

    The input-to-present metric needs a population, not one sample: a scenario
    with no input (heavy_ui) used to report ``samples: 1`` and the p99 was that
    single event. This probe raises the window and posts ``WM_MOUSEMOVE``
    between two points two logical pixels apart in the title bar, so every
    presented frame answers an input instant while the scene stays still. Only
    runs with ``--allow-pointer``.
    """
    hwnd = None
    while not stop.is_set() and hwnd is None:
        hwnd = find_window(pid)
        if hwnd is None:
            time.sleep(0.05)
    if hwnd is None:
        return
    raise_window(hwnd)
    box = client_box(hwnd)
    if box is None:
        return
    left, top, right, bottom = box
    x0 = max(8, min(64, right - left - 8))
    y0 = max(4, min(16, bottom - top - 4))
    screen_x = left + x0
    screen_y = top + y0
    # A zero-delta wheel is an input event that changes nothing on screen. A
    # pointer move is opt-out (redraw_on_pointer_move) and would not mark the
    # input instant, which is why the first version produced three samples.
    while not stop.is_set():
        post_mouse_wheel(hwnd, screen_x, screen_y, 0)
        if stop.wait(0.016):
            return


def post_mouse_wheel(hwnd: int, screen_x: int, screen_y: int, delta: int) -> None:
    """Deliver one `WM_MOUSEWHEEL` with screen coordinates to `hwnd`.

    The wheel delta sits in the high word of ``wparam`` and the cursor
    position in screen coordinates in ``lparam``; this is what winit reads
    when it turns the message into a ``MouseWheel`` event. Only runs with
    ``--allow-pointer`` because the real cursor is parked over the panel.
    """
    wparam = (delta & 0xFFFF) << 16
    lparam = ((screen_y & 0xFFFF) << 16) | (screen_x & 0xFFFF)
    USER32.PostMessageW(hwnd, WM_MOUSEWHEEL, wparam, lparam)


def scroll_layers_loop(pid: int, stop: threading.Event) -> None:
    """Scroll the layers panel at a steady rate until `stop`.

    Raises the window, parks the real cursor over the layers body (left
    panel, above the timeline) and posts one wheel event per ~16 ms.  The
    original cursor position is restored on the way out. Only runs with
    ``--allow-pointer``.
    """
    hwnd = None
    while not stop.is_set() and hwnd is None:
        hwnd = find_window(pid)
        if hwnd is None:
            time.sleep(0.05)
    if hwnd is None:
        return
    raise_window(hwnd)
    box = client_box(hwnd)
    if box is None:
        return
    left, top, right, bottom = box
    width = right - left
    height = bottom - top
    # Left panel (264 logical px) over its body; any layer row hit works.
    client_x = max(24, int(width * 0.08))
    client_y = max(96, int(height * 0.30))
    screen_x = left + client_x
    screen_y = top + client_y
    original = POINT()
    have_original = bool(USER32.GetCursorPos(ctypes.byref(original)))
    try:
        USER32.SetCursorPos(screen_x, screen_y)
        post_mouse_move(hwnd, client_x, client_y)
        # Negative delta scrolls the list down (a positive wheel step would
        # clamp at the top and never move the rows).
        delta = -WHEEL_DELTA * WHEEL_LINES
        while not stop.is_set():
            post_mouse_wheel(hwnd, screen_x, screen_y, delta)
            if stop.wait(0.016):
                return
    finally:
        if have_original:
            USER32.SetCursorPos(original.x, original.y)


def scrub_loop(binary: Path, endpoint: Path, stop: threading.Event) -> None:
    """Send seek() commands through the control endpoint until `stop`."""
    deadline = time.perf_counter() + 8.0
    while time.perf_counter() < deadline and not endpoint.exists() and not stop.is_set():
        time.sleep(0.05)
    frame = 0
    while not stop.is_set():
        subprocess.run(
            [str(binary), "ctl", "--endpoint", str(endpoint), f"seek({frame})"],
            cwd=str(ROOT),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        frame = (frame + 30) % 300
        if stop.wait(0.03):
            return


# â”€â”€ fixtures â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€


def heavy_ui_fixture() -> Path:
    FIXTURES.mkdir(parents=True, exist_ok=True)
    path = FIXTURES / "heavy_ui.motion"
    if path.exists():
        return path
    lines = [
        '# generated by scripts/perf_bench.py (prompt 18 P0)',
        'comp(name="Heavy UI", width=1600, height=900, fps=60, duration=600, background="#101014")',
    ]
    for index in range(3000):
        name = f"row_{index:04d}"
        x = 20 + (index % 8) * 190
        y = 20 + (index % 120) * 7
        lines.append(
            f'{name} = text("Row {index:04d} â€” a long list label with values 0123456789", '
            f'size=12, color="#d8d8e0", align="left")'
        )
        lines.append(f"{name}.pos = ({x}, {y})")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def heavy_ui_list_fixture(count: int) -> Path:
    """Generate a real ``.motioner`` project with `count` layer rows.

    Every row is its own root rect node (no grouping), so the layers panel
    lists exactly `count` rows. The nodes are hidden with the MDF override
    ``<name>.visible = False`` (the same syntax the DSL writes and the bake
    path emits): a hidden node still has an outline row and is never painted,
    which keeps the fixture cheap while the panel does the real work.
    """
    FIXTURES.mkdir(parents=True, exist_ok=True)
    label = "heavy_ui_list" if count == 3000 else f"heavy_ui_list_{count}"
    root = FIXTURES / f"{label}.motioner"
    compositions = root / "compositions"
    compositions.mkdir(parents=True, exist_ok=True)
    title = f"Heavy UI List {count}"
    project = (
        f'project(version=1, name="{title}", width=1600, height=900, fps=60, '
        'duration=36000, background="#101014", main="main")\n'
        'composition(slug="main", title="Main")\n'
    )
    lines = [
        f'comp(name="{title}", width=1600, height=900, fps=60, duration=600, '
        'background="#101014")',
    ]
    for index in range(count):
        name = f"row_{index:04d}"
        lines.append(f'{name} = rect(6, 6, fill="#141820")')
        lines.append(f"{name}.visible = False")
    (root / "project.motion").write_text(project, encoding="utf-8", newline="\n")
    (compositions / "main.motion").write_text(
        "\n".join(lines) + "\n", encoding="utf-8", newline="\n"
    )
    return root


def playback_4k_fixture() -> Path:
    """Generate a 3840x2160 30 fps project for the 4K playback scenario.

    Reuses the demo's generated test video (and its ready proxy when present)
    so the run exercises the preview proxy and decode-ahead path; if the demo
    asset is missing it falls back to shapes so the scenario still runs.  The
    preview window is 1920x1080, so the 4K composition is drawn through the
    proxy; the video frames themselves come from the 640 px preview proxy.
    """
    FIXTURES.mkdir(parents=True, exist_ok=True)
    root = FIXTURES / "playback_4k30.motioner"
    compositions = root / "compositions"
    assets = root / "assets"
    cache = root / "cache"
    for directory in (compositions, assets, cache):
        directory.mkdir(parents=True, exist_ok=True)
    main = compositions / "main.motion"
    if main.is_file():
        return root
    demo = ROOT / "examples" / "demo.motioner"
    clip = next(iter(sorted((demo / "assets").glob("*.mp4"))), None)
    registry = ""
    intro: list[str] = []
    if clip is not None:
        shutil.copyfile(clip, assets / clip.name)
        digest = clip.stem
        registry = (
            f'asset(key="clip", hash="{digest}", original="clip.mp4", '
            'kind="video", imported="2026-11-21T00:00:00Z")\n'
        )
        proxy = demo / "cache" / f"{digest}.proxy.mp4"
        done = demo / "cache" / f"{digest}.proxy.done"
        if proxy.is_file():
            shutil.copyfile(proxy, cache / proxy.name)
            if done.is_file():
                shutil.copyfile(done, cache / done.name)
        intro = [
            'clip = video("clip.mp4", in_point=0, out_point=6, width=3840, '
            'height=2160, fit="cover", gain=1)',
            'clip.id("clip")',
            'clip.pos = (1920, 1080)',
        ]
    else:
        intro = [
            'backdrop = rect(3840, 2160, fill="#101830")',
            'backdrop.id("backdrop")',
            'backdrop.pos = (1920, 1080)',
        ]
    lines = [
        '# generated by scripts/perf_bench.py (prompt 18 P7)',
        'comp(name="Playback 4K30", width=3840, height=2160, fps=30, '
        'duration=10, background="#07070C")',
        *intro,
        'bar = rect(1840, 10, radius=5, fill="#5B8CFF")',
        'bar.id("bar")',
        'bar.pos = (1920, 980)',
        'title = text("4K30", font="Inter", weight=700, size=280, '
        'color="#F4F7FF", align="center", tracking=0.06)',
        'title.id("title")',
        'title.pos = (1920, 1080)',
        'title.key("scale", 0, 1.0)',
        'title.key("scale", 10, 1.08)',
    ]
    project = (
        'project(version=1, name="Playback 4K30", width=3840, height=2160, '
        'fps=30, duration=300, background="#07070C", main="main")\n'
        f'{registry}composition(slug="main", title="Main")\n'
    )
    (root / "project.motion").write_text(project, encoding="utf-8", newline="\n")
    main.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    return root


# â”€â”€ scenario definitions â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€


def scenario_specs() -> dict[str, dict]:
    heavy = heavy_ui_fixture()
    heavy_list = heavy_ui_list_fixture(3000)
    heavy_list_1000 = heavy_ui_list_fixture(1000)
    playback_4k = playback_4k_fixture()
    tmp = Path(tempfile.gettempdir()) / "motioner-p0"
    tmp.mkdir(parents=True, exist_ok=True)
    return {
        "start_idle": {
            "description": "start screen idle 30 s (15 s measured tail)",
            "args": lambda _p: ["--start", "--exit-after-seconds", "30", "--hidden", "--window-size", "1600x900"],
            "expected_frames": 0,
            "idle": True,
            "startup_trace": True,
            "timeout": 40,
        },
        "editor_idle": {
            "description": "editor with the demo project idle 30 s (15 s measured tail)",
            "args": lambda _p: [
                "preview",
                "examples/demo.motioner",
                "--paused",
                "--exit-after-seconds",
                "30",
                "--hidden",
                "--window-size",
                "1600x900",
            ],
            "expected_frames": 0,
            "idle": True,
            "startup_trace": True,
            "timeout": 40,
        },
        "editor_hover": {
            "description": "editor hover (cursor moved across the window)",
            "args": lambda _p: [
                "preview",
                "examples/demo.motioner",
                "--paused",
                "--exit-after-seconds",
                "30",
                "--window-size",
                "1600x900",
            ],
            "expected_frames": 0,
            "idle": True,
            "timeout": 40,
            "hover": True,
        },
        "editor_hover_static": {
            "description": "editor hover inside one static panel area (0 frames once settled)",
            "args": lambda _p: [
                "preview",
                "examples/demo.motioner",
                "--paused",
                "--exit-after-seconds",
                "30",
                "--window-size",
                "1600x900",
            ],
            "expected_frames": 0,
            "idle": True,
            "hover_static": True,
            "timeout": 40,
        },
        "timeline_scrub": {
            "description": "timeline scrubbing through the control endpoint",
            "args": lambda endpoint: [
                "preview",
                "examples/demo.motioner",
                "--paused",
                "--exit-after-seconds",
                "30",
                "--window-size",
                "1600x900",
                "--control-file",
                str(endpoint),
            ],
            "expected_frames": 0,
            "idle": True,
            "timeout": 40,
            "control": True,
        },
        "playback_1080p": {
            "description": "1080p playback of the demo for 10 s",
            "args": lambda _p: [
                "preview",
                "examples/demo.motioner",
                "--exit-after-frames",
                "600",
                "--window-size",
                "1920x1080",
            ],
            "expected_frames": 600,
            "timeout": 45,
        },
        "playback_4k30": {
            "description": (
                "4K30 playback: generated 3840x2160 project with a video, "
                "previewed at 1920x1080 through the proxy for 10 s"
            ),
            "args": lambda _p: [
                "preview",
                str(playback_4k),
                "--exit-after-frames",
                "300",
                "--window-size",
                "1920x1080",
            ],
            "expected_frames": 300,
            "timeout": 60,
            "4k": True,
        },
        "heavy_ui": {
            "description": "heavy UI: 3000-row text composition with a pointer probe",
            "args": lambda _p: [
                "preview",
                str(heavy),
                "--paused",
                "--continuous",
                "--exit-after-frames",
                "600",
                "--window-size",
                "1600x900",
            ],
            "expected_frames": 600,
            "timeout": 90,
            "input_probe": True,
        },
        "heavy_ui_list": {
            "description": (
                "heavy UI layers list: 3000 layers scrolled for 75 s "
                "(the program takes ~28 s to compile, so ~45 s are measured; "
                "collection target <= 2 ms; flat 1k vs 3k)"
            ),
            "args": lambda _p: [
                "preview",
                str(heavy_list),
                "--paused",
                "--continuous",
                "--exit-after-seconds",
                "75",
                "--window-size",
                "1600x900",
            ],
            "expected_frames": 0,
            "wheel": True,
            "timeout": 110,
        },
        "heavy_ui_list_1000": {
            "description": (
                "heavy UI layers list: 1000 layers scrolled for 45 s "
                "(flatness check vs 3000; collection target <= 2 ms)"
            ),
            "args": lambda _p: [
                "preview",
                str(heavy_list_1000),
                "--paused",
                "--continuous",
                "--exit-after-seconds",
                "45",
                "--window-size",
                "1600x900",
            ],
            "expected_frames": 0,
            "wheel": True,
            "timeout": 90,
        },
        "heavy_composition": {
            "description": "heavy composition: showcase_pro",
            "args": lambda _p: [
                "preview",
                "examples/showcase_pro.motion",
                "--paused",
                "--continuous",
                "--exit-after-frames",
                "600",
                "--window-size",
                "1600x900",
            ],
            "expected_frames": 600,
            "timeout": 60,
        },
        "export_120": {
            "description": "export of 120 frames",
            "args": lambda _p: [
                "render",
                "examples/demo.motioner",
                "--out",
                str(tmp / "p0-export.mp4"),
                "--to",
                "120",
            ],
            "expected_frames": 120,
            "timeout": 120,
            "export": True,
        },
    }


# â”€â”€ process measurement â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€


def parse_json_lines(
    output: str,
) -> tuple[dict | None, dict | None, dict | None, dict | None]:
    """Parse the engine's summary lines from captured stdout.

    Returns ``(frame_trace, memory_report, memory_owners, video_cache)``.  The
    last two are ``None`` on a binary that does not print the P7 lines yet.
    """
    frame = None
    memory = None
    memory_owners = None
    video_cache = None
    for line in output.splitlines():
        line = line.strip()
        if line.startswith("frame-trace "):
            try:
                frame = json.loads(line[len("frame-trace ") :])
            except json.JSONDecodeError:
                pass
        elif line.startswith("memory-report "):
            try:
                memory = json.loads(line[len("memory-report ") :])
            except json.JSONDecodeError:
                pass
        elif line.startswith("memory-owners "):
            try:
                memory_owners = json.loads(line[len("memory-owners ") :])
            except json.JSONDecodeError:
                pass
        elif line.startswith("memory_owners "):
            try:
                memory_owners = json.loads(line[len("memory_owners ") :])
            except json.JSONDecodeError:
                pass
        elif line.startswith("video-cache "):
            try:
                video_cache = json.loads(line[len("video-cache ") :])
            except json.JSONDecodeError:
                pass
        elif line.startswith("video_cache "):
            try:
                video_cache = json.loads(line[len("video_cache ") :])
            except json.JSONDecodeError:
                pass
    return frame, memory, memory_owners, video_cache


def median_stage(stages: dict, name: str) -> float | None:
    entry = stages.get(name)
    if not isinstance(entry, dict):
        return None
    return entry.get("median_ms")


def _round_or_none(value: float | int | None) -> float | None:
    return None if value is None else round(float(value), 3)


def trace_stat(entry: object, *keys: str) -> float | None:
    """Read the first numeric field from a trace stats object."""
    if not isinstance(entry, dict):
        return None
    for key in keys:
        value = entry.get(key)
        if isinstance(value, (int, float)):
            return float(value)
    return None


def trace_present_mode(frame: dict) -> str | None:
    """Return the trace's present mode, when the engine reports one."""
    value = frame.get("present_mode")
    if value is None:
        value = frame.get("present-mode")
    if isinstance(value, dict):
        value = value.get("mode", value.get("name"))
    return value if isinstance(value, str) and value else None


def trace_input_to_present(frame: dict) -> tuple[dict | None, dict[str, float | None]]:
    """Parse ``input_to_present_ms`` into a result dict and flat scalars."""
    entry = frame.get("input_to_present_ms")
    if entry is None:
        entry = frame.get("input_to_present")
    scalars = {
        "median": trace_stat(entry, "median", "median_ms", "p50", "p50_ms"),
        "p50": trace_stat(entry, "p50", "p50_ms", "median", "median_ms"),
        "p90": trace_stat(entry, "p90", "p90_ms"),
        "p99": trace_stat(entry, "p99", "p99_ms"),
        "samples": trace_stat(entry, "samples", "sample_count"),
    }
    if all(value is None for value in scalars.values()):
        return None, scalars
    parsed = {
        "median": _round_or_none(scalars["median"]),
        "p50": _round_or_none(scalars["p50"]),
        "p90": _round_or_none(scalars["p90"]),
        "p99": _round_or_none(scalars["p99"]),
        "samples": None if scalars["samples"] is None else int(scalars["samples"]),
    }
    return parsed, scalars


def trace_damage(frame: dict) -> tuple[float | None, float | None, float | None]:
    """Parse ``ui_damage`` into (median, p90, p99) fractions of the window."""
    entry = frame.get("ui_damage")
    if entry is None and isinstance(frame.get("damage_fraction"), (int, float)):
        median = float(frame["damage_fraction"])
        return (median, None, None)
    return (
        trace_stat(entry, "median", "median_fraction", "p50"),
        trace_stat(entry, "p90", "p90_fraction"),
        trace_stat(entry, "p99", "p99_fraction"),
    )


def trace_memory_owners(
    entry: object,
) -> tuple[dict | None, dict[str, float | None]]:
    """Parse ``memory-owners`` (document/plan/panel/video bytes)."""
    keys = (
        "memory_owners_document_bytes",
        "memory_owners_plan_bytes",
        "memory_owners_panel_bytes",
        "memory_owners_video_bytes",
        "memory_owners_total_bytes",
    )
    if not isinstance(entry, dict):
        return None, {key: None for key in keys}
    owners = entry.get("owners")
    scope = owners if isinstance(owners, dict) else entry

    def owner_bytes(name: str) -> float | None:
        return trace_stat(scope, name, f"{name}_bytes", f"{name}_owner_bytes")

    document = owner_bytes("document")
    plan = owner_bytes("plan")
    panel = owner_bytes("panel")
    video = owner_bytes("video")
    total = trace_stat(entry, "total", "total_bytes")
    if total is None:
        total = trace_stat(scope, "total", "total_bytes")
    if total is None:
        present = [value for value in (document, plan, panel, video) if value is not None]
        total = sum(present) if present else None
    return entry, {
        "memory_owners_document_bytes": document,
        "memory_owners_plan_bytes": plan,
        "memory_owners_panel_bytes": panel,
        "memory_owners_video_bytes": video,
        "memory_owners_total_bytes": total,
    }


def trace_decode_ahead(
    entry: object,
) -> tuple[dict | None, dict[str, float | int | None]]:
    """Parse ``video-cache`` into a normalized dict and flat scalars."""
    keys = (
        "decode_ahead_requests",
        "decode_ahead_hits",
        "decode_ahead_misses",
        "decode_ahead_hit_rate",
        "decode_ahead_bytes",
    )
    if not isinstance(entry, dict):
        return None, {key: None for key in keys}
    stats = entry.get("stats")
    scope = stats if isinstance(stats, dict) else entry
    requests = trace_stat(scope, "requests", "request_count", "requested")
    hits = trace_stat(scope, "hits", "hit_count")
    misses = trace_stat(scope, "misses", "miss_count")
    hit_rate = trace_stat(scope, "hit_rate", "rate")
    if hit_rate is None and requests and hits is not None:
        hit_rate = hits / requests
    cache_bytes = trace_stat(scope, "bytes", "resident_bytes", "cache_bytes")

    def as_int(value: float | None) -> int | None:
        return None if value is None else int(round(value))

    parsed = {
        "requests": as_int(requests),
        "hits": as_int(hits),
        "misses": as_int(misses),
        "hit_rate": _round_or_none(hit_rate),
        "bytes": as_int(cache_bytes),
    }
    if all(value is None for value in parsed.values()):
        return None, {key: None for key in keys}
    return parsed, {
        "decode_ahead_requests": parsed["requests"],
        "decode_ahead_hits": parsed["hits"],
        "decode_ahead_misses": parsed["misses"],
        "decode_ahead_hit_rate": parsed["hit_rate"],
        "decode_ahead_bytes": parsed["bytes"],
    }


def run_once(
    binary: Path,
    name: str,
    spec: dict,
    endpoint: Path,
    timeout: float,
    hang_timeout: float | None = None,
    allow_pointer: bool = False,
) -> dict:
    args = spec["args"](endpoint)
    env = os.environ.copy()
    env["FERROUS_TRACE_FRAMES"] = "1"
    # The startup report carries the first-frame mark; the harness uses it to
    # separate the one-shot start-up CPU from the idle CPU after the window is
    # live (prompt 18 P1).
    if spec.get("startup_trace"):
        env["FERROUS_TRACE_STARTUP"] = "1"
    command = [str(binary), *args]
    started = time.perf_counter()
    process = subprocess.Popen(
        command,
        cwd=str(ROOT),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )

    try:
        ps = psutil.Process(process.pid)
    except psutil.NoSuchProcess:
        ps = None

    output_lines: list[str] = []
    first_frame_at: list[float | None] = [None]
    cpu_at_first: list[float | None] = [None]

    def read_output(pipe) -> None:
        for line in pipe:
            output_lines.append(line)
            if first_frame_at[0] is None and "first frame presented" in line:
                first_frame_at[0] = time.perf_counter()
                if ps is not None:
                    try:
                        times = ps.cpu_times()
                        cpu_at_first[0] = times.user + times.system
                    except psutil.Error:
                        pass

    reader = threading.Thread(target=read_output, args=(process.stdout,), daemon=True)
    reader.start()

    stop = threading.Event()
    workers: list[threading.Thread] = []
    if spec.get("hover") and allow_pointer:
        workers.append(threading.Thread(target=hover_loop, args=(process.pid, stop), daemon=True))
    if spec.get("hover_static") and allow_pointer:
        workers.append(
            threading.Thread(target=hover_static_loop, args=(process.pid, stop), daemon=True)
        )
    if spec.get("wheel") and allow_pointer:
        workers.append(
            threading.Thread(target=scroll_layers_loop, args=(process.pid, stop), daemon=True)
        )
    if spec.get("input_probe") and allow_pointer:
        workers.append(
            threading.Thread(target=input_probe_loop, args=(process.pid, stop), daemon=True)
        )
    if spec.get("control"):
        workers.append(
            threading.Thread(
                target=scrub_loop, args=(binary, endpoint, stop), daemon=True
            )
        )
    for worker in workers:
        worker.start()

    start_cpu = None
    if ps is not None:
        try:
            times = ps.cpu_times()
            start_cpu = times.user + times.system
        except psutil.Error:
            start_cpu = None
    last_cpu = start_cpu
    last_at = started
    # (time, cpu seconds) snapshots, so the idle number can stop before the
    # process teardown window once the real exit time is known.
    samples: list[tuple[float, float]] = []
    max_rss = 0
    max_private = 0
    max_threads = 0
    # Idle scenarios keep a 15 s tail after a 15 s settle window: start-up
    # work (GPU init, asset probes, proxies) is excluded from the idle number.
    tail_start = started + 15.0 if spec.get("idle") else None
    tail_cpu = None
    tail_at = None
    limit = hang_timeout or timeout
    try:
        while process.poll() is None and time.perf_counter() - started < limit:
            if ps is not None:
                try:
                    times = ps.cpu_times()
                    last_cpu = times.user + times.system
                    last_at = time.perf_counter()
                    samples.append((last_at, last_cpu))
                    info = ps.memory_info()
                    max_rss = max(max_rss, int(info.rss))
                    max_private = max(max_private, int(getattr(info, "private", 0)))
                    max_threads = max(max_threads, ps.num_threads())
                    if (
                        tail_start is not None
                        and tail_cpu is None
                        and time.perf_counter() >= tail_start
                        and last_cpu is not None
                    ):
                        tail_cpu = last_cpu
                        tail_at = time.perf_counter()
                except psutil.Error:
                    pass
            time.sleep(0.25)
    finally:
        stop.set()
        timed_out = process.poll() is None
        if timed_out:
            process.kill()
        process.wait(timeout=30)
        end_at = time.perf_counter()
        reader.join(timeout=10)
    wall = end_at - started
    if last_cpu is None:
        last_cpu = start_cpu
    cpu_seconds = 0.0
    if start_cpu is not None and last_cpu is not None:
        cpu_seconds = max(0.0, last_cpu - start_cpu)
    # Drop the close/teardown window: the process exits at the deadline and
    # the last sample can include GPU, audio and worker shutdown. Use the last
    # snapshot at least IDLE_EXCLUDE_S before the real exit time.
    measured: tuple[float, float] | None = None
    cutoff = end_at - IDLE_EXCLUDE_S
    for sample_at, sample_cpu in reversed(samples):
        if sample_at <= cutoff:
            measured = (sample_at, sample_cpu)
            break
    if measured is None and samples:
        measured = samples[-1]
    idle_excluded_s = None
    if spec.get("idle"):
        if measured is not None:
            idle_excluded_s = max(0.0, end_at - measured[0])
        else:
            idle_excluded_s = 0.0
    idle_cpu_s = None
    idle_cpu_percent = None
    if (
        first_frame_at[0] is not None
        and cpu_at_first[0] is not None
        and measured is not None
    ):
        measured_at, measured_cpu = measured
        idle_span = measured_at - first_frame_at[0]
        if idle_span > 0.0:
            idle_cpu_s = max(0.0, measured_cpu - cpu_at_first[0])
            idle_cpu_percent = idle_cpu_s / idle_span * 100.0
    tail_cpu_s = None
    tail_cpu_percent = None
    if tail_cpu is not None and tail_at is not None and measured is not None:
        measured_at, measured_cpu = measured
        tail_span = measured_at - tail_at
        if tail_span > 0.0:
            tail_cpu_s = max(0.0, measured_cpu - tail_cpu)
            tail_cpu_percent = tail_cpu_s / tail_span * 100.0
    frame, memory, memory_owners_line, video_cache_line = parse_json_lines(
        "".join(output_lines)
    )
    frames = 0
    presented = 0
    gpu_submits = 0
    gpu = {}
    gpu_frame = {}
    stages = {}
    allocs = None
    alloc_bytes = None
    frame_ms = {}
    input_to_present = None
    input_to_present_median = None
    input_to_present_p50 = None
    input_to_present_p90 = None
    input_to_present_p99 = None
    input_to_present_samples = None
    damage_fraction = None
    damage_fraction_p90 = None
    damage_fraction_p99 = None
    present_mode = None
    memory_owners, memory_owner_scalars = trace_memory_owners(memory_owners_line)
    decode_ahead, decode_ahead_scalars = trace_decode_ahead(video_cache_line)
    if frame is not None:
        frames = int(frame.get("frames", 0))
        presented = int(frame.get("presented", 0))
        gpu_submits = int(frame.get("gpu_submits", 0))
        stages = frame.get("stages", {})
        frame_ms = frame.get("frame_ms", {}) or {}
        gpu = {
            pass_name: float(value.get("median_ms", 0.0))
            for pass_name, value in frame.get("gpu_passes", {}).items()
            if isinstance(value, dict)
        }
        gpu_frame = {
            key: int(value)
            for key, value in (frame.get("gpu_frame", {}) or {}).items()
            if isinstance(value, (int, float))
        }
        alloc_entry = frame.get("allocations_per_frame", {})
        if frame.get("alloc_stats") and isinstance(alloc_entry, dict):
            allocs = float(alloc_entry.get("median", 0.0))
        bytes_entry = frame.get("bytes_per_frame", {})
        if frame.get("alloc_stats") and isinstance(bytes_entry, dict):
            alloc_bytes = float(bytes_entry.get("median", 0.0))
        input_to_present, ip_scalars = trace_input_to_present(frame)
        input_to_present_median = ip_scalars["median"]
        input_to_present_p50 = ip_scalars["p50"]
        input_to_present_p90 = ip_scalars["p90"]
        input_to_present_p99 = ip_scalars["p99"]
        input_to_present_samples = ip_scalars["samples"]
        damage_fraction, damage_fraction_p90, damage_fraction_p99 = trace_damage(frame)
        present_mode = trace_present_mode(frame)
    expected = int(spec.get("expected_frames", 0))
    if spec.get("export"):
        frames = expected
    # The trace reports frames cumulatively (the first presented frame counts
    # as one), so on idle-style runs this is the interesting "after the first"
    # number: 0 when the window presented nothing since it settled.
    idle_like = spec.get("idle") or spec.get("hover_static")
    idle_frames_after_first = max(0, frames - 1) if idle_like else None
    idle_percent_value = (
        round(idle_cpu_percent, 3) if idle_cpu_percent is not None else None
    )
    tail_percent_value = (
        round(tail_cpu_percent, 3) if tail_cpu_percent is not None else None
    )
    excluded_value = round(idle_excluded_s, 3) if idle_excluded_s is not None else None
    stages_ms = {
        key: round(value, 4)
        for key, value in {
            stage: median_stage(stages, stage)
            for stage in (
                "event handling",
                "app update",
                "ui layout",
                "render-command collection",
                "batch build",
                "prepare/upload",
                "encode",
                "submit",
                "present/wait",
            )
        }.items()
        if value is not None
    }
    collection_ms = stages_ms.get("render-command collection")
    result = {
        "scenario": name,
        "wall_s": round(wall, 3),
        "cpu_s": round(cpu_seconds, 3),
        "cpu_percent": round(cpu_seconds / wall * 100.0, 3) if wall > 0 else 0.0,
        "frames": frames,
        "presented": presented,
        "gpu_submits": gpu_submits,
        "idle_cpu_s": round(idle_cpu_s, 4) if idle_cpu_s is not None else None,
        "idle_cpu_percent": idle_percent_value,
        "idle_after_first_cpu_percent": idle_percent_value,
        "tail_cpu_s": round(tail_cpu_s, 4) if tail_cpu_s is not None else None,
        "tail_cpu_percent": tail_percent_value,
        "idle_tail_cpu_percent": tail_percent_value,
        "idle_excluded_s": excluded_value,
        "idle_excluded_teardown_s": excluded_value,
        "idle_frames_after_first": idle_frames_after_first,
        "idle_after_first_frames": idle_frames_after_first,
        "expected_frames": expected,
        "dropped_frames": max(0, expected - frames),
        "fps": round(frames / wall, 3) if wall > 0 else 0.0,
        "working_set_mb": round(max_rss / (1024 * 1024), 1),
        "private_mb": round(max_private / (1024 * 1024), 1),
        "threads": max_threads,
        "gpu_passes_ms": {key: round(value, 4) for key, value in gpu.items()},
        "stages_ms": stages_ms,
        "collection_ms": _round_or_none(collection_ms),
        "gpu_frame": gpu_frame,
        "allocations_per_frame": allocs,
        "allocated_bytes_per_frame": alloc_bytes,
        "frame_ms_median": _round_or_none(frame_ms.get("median_ms")),
        "frame_ms_p90": _round_or_none(frame_ms.get("p90_ms")),
        "frame_ms_p99": _round_or_none(frame_ms.get("p99_ms")),
        "input_to_present": input_to_present,
        "input_to_present_median": _round_or_none(input_to_present_median),
        "input_to_present_p50": _round_or_none(input_to_present_p50),
        "input_to_present_p90": _round_or_none(input_to_present_p90),
        "input_to_present_p99": _round_or_none(input_to_present_p99),
        "input_to_present_samples": (
            None if input_to_present_samples is None else int(input_to_present_samples)
        ),
        "input_to_present_insufficient": (
            input_to_present_samples is not None
            and int(input_to_present_samples) < MIN_INPUT_SAMPLES
        ),
        "damage_fraction": _round_or_none(damage_fraction),
        "damage_fraction_p90": _round_or_none(damage_fraction_p90),
        "damage_fraction_p99": _round_or_none(damage_fraction_p99),
        "present_mode": present_mode,
        "decode_ahead": decode_ahead,
        **decode_ahead_scalars,
        "memory_owners": memory_owners,
        **memory_owner_scalars,
        "vram_mb": None,
        "gpu_bytes": None,
        "cpu_bytes": None,
        "memory_by_subsystem": {},
        "memory_largest": [],
        "timed_out": timed_out,
    }
    if memory is not None:
        result["vram_mb"] = round(memory.get("gpu_bytes", 0) / (1024 * 1024), 1)
        result["gpu_bytes"] = int(memory.get("gpu_bytes", 0))
        result["cpu_bytes"] = int(memory.get("cpu_bytes", 0))
        result["memory_by_subsystem"] = memory.get("by_subsystem", [])
        result["memory_largest"] = memory.get("largest", [])
    if spec.get("export"):
        out_path = Path(args[args.index("--out") + 1])
        result["export_fps"] = round(expected / wall, 3) if wall > 0 else 0.0
        if out_path.exists():
            result["export_bytes"] = out_path.stat().st_size
            out_path.unlink()
    return result


RANGE_METRICS = (
    "cpu_percent",
    "idle_cpu_s",
    "idle_cpu_percent",
    "idle_after_first_cpu_percent",
    "tail_cpu_s",
    "tail_cpu_percent",
    "idle_tail_cpu_percent",
    "idle_excluded_s",
    "idle_excluded_teardown_s",
    "idle_frames_after_first",
    "idle_after_first_frames",
    "dropped_frames",
    "fps",
    "working_set_mb",
    "private_mb",
    "threads",
    "vram_mb",
    "collection_ms",
    "frame_ms_median",
    "frame_ms_p90",
    "frame_ms_p99",
    "allocations_per_frame",
    "allocated_bytes_per_frame",
    "damage_fraction",
    "damage_fraction_p90",
    "damage_fraction_p99",
    "input_to_present_median",
    "input_to_present_p50",
    "input_to_present_p90",
    "input_to_present_p99",
    "input_to_present_samples",
    "decode_ahead_requests",
    "decode_ahead_hits",
    "decode_ahead_misses",
    "decode_ahead_hit_rate",
    "decode_ahead_bytes",
    "memory_owners_document_bytes",
    "memory_owners_plan_bytes",
    "memory_owners_panel_bytes",
    "memory_owners_video_bytes",
    "memory_owners_total_bytes",
    "wall_s",
    "frames",
    "presented",
    "gpu_submits",
)


def aggregate(runs: list[dict]) -> dict:
    def med(key: str) -> float | int | None:
        values = [run[key] for run in runs if run.get(key) is not None]
        if not values:
            return None
        return round(statistics.median(values), 3)

    def maximum(key: str) -> float | int | None:
        values = [run[key] for run in runs if run.get(key) is not None]
        if not values:
            return None
        return max(values)

    def spread(key: str) -> dict | None:
        values = [run[key] for run in runs if run.get(key) is not None]
        if not values:
            return None
        return {"min": round(min(values), 3), "max": round(max(values), 3)}

    gpu_keys = sorted({key for run in runs for key in run.get("gpu_passes_ms", {})})
    stage_keys = sorted({key for run in runs for key in run.get("stages_ms", {})})
    gpu_frame_keys = sorted({key for run in runs for key in run.get("gpu_frame", {})})
    input_to_present = {
        "median": med("input_to_present_median"),
        "p50": med("input_to_present_p50"),
        "p90": med("input_to_present_p90"),
        "p99": med("input_to_present_p99"),
        "samples": med("input_to_present_samples"),
    }
    if all(value is None for value in input_to_present.values()):
        input_to_present = None
    elif input_to_present["samples"] is not None:
        input_to_present["samples"] = int(input_to_present["samples"])
    decode_ahead = {
        "requests": med("decode_ahead_requests"),
        "hits": med("decode_ahead_hits"),
        "misses": med("decode_ahead_misses"),
        "hit_rate": med("decode_ahead_hit_rate"),
        "bytes": med("decode_ahead_bytes"),
    }
    if all(value is None for value in decode_ahead.values()):
        decode_ahead = None
    else:
        for key in ("requests", "hits", "misses", "bytes"):
            if decode_ahead[key] is not None:
                decode_ahead[key] = int(decode_ahead[key])
    return {
        "runs": len(runs),
        "cpu_percent": med("cpu_percent"),
        "wall_s": med("wall_s"),
        "frames": med("frames"),
        "presented": med("presented"),
        "gpu_submits": med("gpu_submits"),
        "idle_cpu_s": med("idle_cpu_s"),
        "idle_cpu_percent": med("idle_cpu_percent"),
        "idle_after_first_cpu_percent": med("idle_after_first_cpu_percent"),
        "tail_cpu_s": med("tail_cpu_s"),
        "tail_cpu_percent": med("tail_cpu_percent"),
        "idle_tail_cpu_percent": med("idle_tail_cpu_percent"),
        "idle_excluded_s": med("idle_excluded_s"),
        "idle_excluded_teardown_s": med("idle_excluded_teardown_s"),
        "idle_frames_after_first": med("idle_frames_after_first"),
        "idle_after_first_frames": med("idle_after_first_frames"),
        "dropped_frames": med("dropped_frames"),
        "fps": med("fps"),
        "working_set_mb": maximum("working_set_mb"),
        "private_mb": maximum("private_mb"),
        "threads": maximum("threads"),
        "allocations_per_frame": med("allocations_per_frame"),
        "allocated_bytes_per_frame": med("allocated_bytes_per_frame"),
        "collection_ms": med("collection_ms"),
        "frame_ms_median": med("frame_ms_median"),
        "frame_ms_p90": med("frame_ms_p90"),
        "frame_ms_p99": med("frame_ms_p99"),
        "vram_mb": maximum("vram_mb"),
        "input_to_present": input_to_present,
        "input_to_present_median": med("input_to_present_median"),
        "input_to_present_p50": med("input_to_present_p50"),
        "input_to_present_p90": med("input_to_present_p90"),
        "input_to_present_p99": med("input_to_present_p99"),
        "input_to_present_samples": med("input_to_present_samples"),
        "input_to_present_insufficient": bool(
            med("input_to_present_samples") is not None
            and med("input_to_present_samples") < MIN_INPUT_SAMPLES
        ),
        "damage_fraction": med("damage_fraction"),
        "damage_fraction_p90": med("damage_fraction_p90"),
        "damage_fraction_p99": med("damage_fraction_p99"),
        "decode_ahead": decode_ahead,
        "decode_ahead_requests": med("decode_ahead_requests"),
        "decode_ahead_hits": med("decode_ahead_hits"),
        "decode_ahead_misses": med("decode_ahead_misses"),
        "decode_ahead_hit_rate": med("decode_ahead_hit_rate"),
        "decode_ahead_bytes": med("decode_ahead_bytes"),
        "memory_owners": runs[-1].get("memory_owners"),
        "memory_owners_document_bytes": med("memory_owners_document_bytes"),
        "memory_owners_plan_bytes": med("memory_owners_plan_bytes"),
        "memory_owners_panel_bytes": med("memory_owners_panel_bytes"),
        "memory_owners_video_bytes": med("memory_owners_video_bytes"),
        "memory_owners_total_bytes": med("memory_owners_total_bytes"),
        "present_mode": runs[-1].get("present_mode"),
        "ranges": {
            key: value
            for key in RANGE_METRICS
            if (value := spread(key)) is not None
        },
        "gpu_passes_ms": {
            key: round(
                statistics.median(
                    [run["gpu_passes_ms"][key] for run in runs if key in run["gpu_passes_ms"]]
                ),
                4,
            )
            for key in gpu_keys
        },
        "gpu_frame": {
            key: int(
                statistics.median(
                    [run["gpu_frame"][key] for run in runs if key in run.get("gpu_frame", {})]
                )
            )
            for key in gpu_frame_keys
        },
        "stages_ms": {
            key: round(
                statistics.median(
                    [run["stages_ms"][key] for run in runs if key in run["stages_ms"]]
                ),
                4,
            )
            for key in stage_keys
        },
        "memory_by_subsystem": runs[-1].get("memory_by_subsystem", []),
        "memory_largest": runs[-1].get("memory_largest", []),
        "timed_out": any(run.get("timed_out") for run in runs),
    }


def print_result(name: str, description: str, result: dict) -> None:
    ranges = result.get("ranges", {})

    def ranged(key: str, value: float | int | None, unit: str = "") -> str:
        if value is None:
            return "n/a"
        spread = ranges.get(key)
        if not spread:
            return f"{value}{unit}"
        return f"{value}{unit} [{spread['min']}-{spread['max']}{unit}]"

    def fraction_percent(value: float | None) -> float | None:
        if value is None:
            return None
        return value * 100.0 if abs(value) <= 1.0 else value

    print(f"\n{name}: {description}")
    print(
        f"  cpu {ranged('cpu_percent', result['cpu_percent'], '%')}  "
        f"wall {result['wall_s']} s  frames {result['frames']}  "
        f"presented {result['presented']}  submits {result['gpu_submits']}  "
        f"dropped {result['dropped_frames']}  fps {ranged('fps', result['fps'])}"
    )
    frames_after_first = result.get(
        "idle_after_first_frames", result.get("idle_frames_after_first")
    )
    if frames_after_first is not None:
        print(
            "  frames after the first "
            f"(idle_after_first_frames): {frames_after_first}"
        )
    excluded = result.get("idle_excluded_teardown_s", result.get("idle_excluded_s"))
    if excluded is not None:
        print(
            "  idle window excludes the last "
            f"{excluded} s before exit (idle_excluded_teardown_s, teardown)"
        )
    idle_after_first = result.get(
        "idle_after_first_cpu_percent", result.get("idle_cpu_percent")
    )
    if idle_after_first is not None:
        print(
            "  idle after first frame (idle_after_first_cpu_percent) "
            f"{ranged('idle_after_first_cpu_percent', idle_after_first, '%')} "
            f"of a core  ({result['idle_cpu_s']} s after the first frame)"
        )
    idle_tail = result.get("idle_tail_cpu_percent", result.get("tail_cpu_percent"))
    if idle_tail is not None:
        print(
            "  idle tail (idle_tail_cpu_percent) "
            f"{ranged('idle_tail_cpu_percent', idle_tail, '%')} of a core  "
            f"({result['tail_cpu_s']} s in the measured tail)"
        )
        if idle_tail > 0.1:
            print(
                "  FINDING: idle tail "
                f"{ranged('idle_tail_cpu_percent', idle_tail, '%')} of a core "
                "is above 0.1 % (the window is not sleeping)"
            )
    print(
        f"  working set {result['working_set_mb']} MB  private "
        f"{result['private_mb']} MB  threads {result['threads']}  "
        f"VRAM {result['vram_mb']} MB"
    )
    if result.get("frame_ms_p99") is not None:
        print(
            f"  frame time median {result['frame_ms_median']} ms  p90 "
            f"{result['frame_ms_p90']} ms  p99 {result['frame_ms_p99']} ms"
        )
    if result.get("collection_ms") is not None:
        print(
            "  collection (render-command) "
            f"{ranged('collection_ms', result['collection_ms'], ' ms')}"
        )
    if result["allocations_per_frame"] is not None:
        print(
            f"  allocations/frame {result['allocations_per_frame']}  "
            f"bytes/frame {result['allocated_bytes_per_frame']}"
        )
    present_mode = result.get("present_mode")
    present_label = f" (present mode {present_mode})" if present_mode else ""
    input_to_present = result.get("input_to_present")
    if input_to_present and (
        input_to_present.get("p50") is not None
        or input_to_present.get("p99") is not None
    ):
        print(
            f"  input-to-present{present_label}: "
            f"p50 {input_to_present.get('p50')} ms  "
            f"p99 {input_to_present.get('p99')} ms  "
            f"(samples {input_to_present.get('samples')})"
        )
        samples = input_to_present.get("samples")
        if samples is not None and int(samples) < MIN_INPUT_SAMPLES:
            print(
                "  FINDING: only "
                f"{samples} input-to-present samples (< {MIN_INPUT_SAMPLES}); "
                "p50/p99 are not a measurement"
            )
    else:
        print(f"  input-to-present{present_label}: not reported")
    damage = result.get("damage_fraction")
    if damage is not None:
        p90 = fraction_percent(result.get("damage_fraction_p90"))
        p99 = fraction_percent(result.get("damage_fraction_p99"))
        detail = ""
        if p90 is not None:
            detail += f"  p90 {p90:.2f}%"
        if p99 is not None:
            detail += f"  p99 {p99:.2f}%"
        print(
            f"  damaged fraction {fraction_percent(damage):.2f}% of the window"
            f"{detail}"
        )
    else:
        print("  damaged fraction: not reported")
    decode_ahead = result.get("decode_ahead")
    memory_owners = result.get("memory_owners")
    if decode_ahead and any(value is not None for value in decode_ahead.values()):
        rate = fraction_percent(decode_ahead.get("hit_rate"))
        rate_text = "n/a" if rate is None else f"{rate:.1f}%"
        cache_bytes = decode_ahead.get("bytes")
        cache_mb = (
            "n/a"
            if cache_bytes is None
            else f"{float(cache_bytes) / (1024 * 1024):.1f} MB"
        )
        decode_text = (
            f"decode-ahead hit rate {rate_text} "
            f"(requests {decode_ahead.get('requests')}, "
            f"hits {decode_ahead.get('hits')}, "
            f"misses {decode_ahead.get('misses')}, cache {cache_mb})"
        )
    else:
        decode_text = "decode-ahead: not reported"

    def owner_mb(name: str) -> str:
        value = result.get(f"memory_owners_{name}_bytes")
        if value is None and isinstance(memory_owners, dict):
            key = {
                "document": "document_bytes_estimate",
                "plan": "plan_bytes_estimate",
                "panel": "panel_bytes_estimate",
                "video": "video_cache_bytes",
            }.get(name)
            if key is not None:
                value = memory_owners.get(key)
        if value is None:
            return "n/a"
        return f"{float(value) / (1024 * 1024):.1f} MB"

    if isinstance(memory_owners, dict):
        owners_text = (
            "memory owners: "
            f"document {owner_mb('document')}  plan {owner_mb('plan')}  "
            f"panel {owner_mb('panel')}  video {owner_mb('video')}"
        )
    else:
        owners_text = "memory owners: not reported"
    print(f"  {decode_text}  |  {owners_text}")
    if result["gpu_passes_ms"]:
        passes = ", ".join(f"{key} {value:.3f} ms" for key, value in result["gpu_passes_ms"].items())
        print(f"  GPU passes: {passes}")
    if result.get("gpu_frame"):
        gf = result["gpu_frame"]
        print(
            f"  GPU frame: {gf.get('draw_calls')} draw calls, "
            f"{gf.get('gpu_passes')} passes, {gf.get('pipeline_binds')} pipeline binds, "
            f"{gf.get('bind_group_binds')} bind-group binds, {gf.get('buffer_binds')} buffer binds, "
            f"{gf.get('ui_segments')} GUI segments, {gf.get('scissor_changes')} scissors"
        )
    if result["stages_ms"]:
        stages = ", ".join(f"{key} {value:.3f}" for key, value in result["stages_ms"].items())
        print(f"  stages (ms): {stages}")
    if result.get("export_fps"):
        print(f"  export {result['export_fps']} frames/s")


MISSING = object()


def resolve_metric(result: dict, metric: str):
    """Read a metric, following dotted paths into nested result objects."""
    value: object = result
    for part in metric.split("."):
        if not isinstance(value, dict) or part not in value:
            return MISSING
        value = value[part]
    return value


def _entry_limit(
    entry: dict, name: str, baseline: float | None, margin: float
) -> float | None:
    """Return an explicit ``max``/``min`` or derive it from a baseline."""
    if name in entry:
        return float(entry[name])
    if baseline is None:
        return None
    direction = entry.get("direction", "max")
    if direction != name:
        return None
    floor = float(entry.get("floor", 0.0))
    if direction == "min":
        return max(floor, baseline * (1.0 - margin))
    return max(floor, baseline * (1.0 + margin))


def check_budgets(results: dict[str, dict], budgets: dict) -> list[str]:
    """Return every budget violation; an empty list means the check passed."""
    global_margin = float(budgets.get("noise_margin", 0.0))
    failures: list[str] = []
    for scenario, metrics in budgets.get("scenarios", {}).items():
        if scenario not in results:
            failures.append(f"{scenario}: no measurement")
            continue
        measured = results[scenario]
        if not isinstance(metrics, dict):
            failures.append(f"{scenario}: budget is not an object")
            continue
        insufficient = bool(measured.get("input_to_present_insufficient"))
        if insufficient and any(
            metric.startswith("input_to_present") for metric in metrics
        ):
            failures.append(
                f"{scenario}.input_to_present: fewer than {MIN_INPUT_SAMPLES} "
                "samples (one sample is not a measurement)"
            )
        for metric, entry in metrics.items():
            if insufficient and metric.startswith("input_to_present"):
                # Reported once above; a p99 with five samples is noise.
                continue
            if not isinstance(entry, dict):
                failures.append(f"{scenario}.{metric}: budget entry is not an object")
                continue
            value = resolve_metric(measured, metric)
            if value is MISSING or value is None:
                failures.append(f"{scenario}.{metric}: not measured")
                continue
            if not isinstance(value, (int, float)):
                failures.append(f"{scenario}.{metric}: not numeric ({value!r})")
                continue
            baseline = entry.get("baseline")
            baseline_value = float(baseline) if baseline is not None else None
            margin = float(entry.get("noise_margin", global_margin))
            max_value = _entry_limit(entry, "max", baseline_value, margin)
            min_value = _entry_limit(entry, "min", baseline_value, margin)
            if max_value is not None and value > max_value:
                suffix = (
                    f" (baseline {baseline_value} + margin {margin})"
                    if baseline_value is not None
                    else ""
                )
                failures.append(
                    f"{scenario}.{metric}: {value} > max {round(max_value, 3)}{suffix}"
                )
            if min_value is not None and value < min_value:
                suffix = (
                    f" (baseline {baseline_value} + margin {margin})"
                    if baseline_value is not None
                    else ""
                )
                failures.append(
                    f"{scenario}.{metric}: {value} < min {round(min_value, 3)}{suffix}"
                )
    return failures


# Metrics written by --write-budgets: ``max`` with an absolute floor so that
# a near-zero median (idle collection, 0 damage, 0 input latency) does not
# turn into a meaningless 0 * 1.15 budget.
MAX_FLOOR_METRICS = {
    "cpu_percent": 0.02,
    # Idle/tail CPU is near zero and the NVIDIA driver wakes 0.1-0.85 % of a
    # core on the reference machine; a 0.02 % floor would flake on the
    # baseline, so the absolute floor is 1 % of a core.
    "idle_cpu_percent": 1.0,
    "tail_cpu_percent": 1.0,
    "collection_ms": 0.02,
    "frame_ms_p99": 0.02,
    "damage_fraction": 0.02,
    "input_to_present_p50": 0.02,
    "input_to_present_p99": 0.02,
}
MAX_MB_METRICS = {
    "private_mb": 1.0,
    "vram_mb": 0.1,
}
GPU_FRAME_METRICS = (
    "gpu_frame.draw_calls",
    "gpu_frame.pipeline_binds",
    "gpu_frame.bind_group_binds",
    "gpu_frame.buffer_binds",
    "gpu_frame.ui_segments",
)


def derive_budgets(results: dict[str, dict], margin: float) -> dict:
    """Build a budget payload from measured medians, with noise margins."""
    notes: list[str] = [
        (
            "export_120 is excluded: its throughput is noisy without a much "
            "wider margin; measure it separately."
        ),
        (
            "Budgets use the three-run median and a 15 % margin; the recorded "
            "min/max span can be wider on noisy metrics (collection, damage, "
            "hover CPU)."
        ),
    ]
    scenarios: dict[str, dict] = {}
    for name, measured in results.items():
        if name == "export_120":
            continue
        if measured.get("timed_out"):
            notes.append(f"{name}: excluded because the run timed out.")
            continue
        limits: dict[str, dict[str, float]] = {}
        for metric, floor in MAX_FLOOR_METRICS.items():
            value = measured.get(metric)
            if not isinstance(value, (int, float)):
                continue
            bound = max(floor, float(value) * (1.0 + margin))
            if metric == "damage_fraction":
                bound = min(1.0, bound)
            limits[metric] = {"max": round(bound, 3)}
        for metric, floor in MAX_MB_METRICS.items():
            value = measured.get(metric)
            if not isinstance(value, (int, float)):
                continue
            limits[metric] = {
                "max": round(max(floor, float(value) * (1.0 + margin)), 3)
            }
        fps = measured.get("fps")
        if isinstance(fps, (int, float)) and float(fps) >= 1.0:
            limits["fps"] = {"min": round(float(fps) * (1.0 - margin), 3)}
        dropped = measured.get("dropped_frames")
        if isinstance(dropped, (int, float)):
            limits["dropped_frames"] = {"max": 0}
        allocations = measured.get("allocations_per_frame")
        if isinstance(allocations, (int, float)):
            limits["allocations_per_frame"] = {
                "max": round(float(allocations) * (1.0 + margin), 3)
            }
        gpu_frame = measured.get("gpu_frame")
        if isinstance(gpu_frame, dict):
            for dotted in GPU_FRAME_METRICS:
                leaf = dotted.split(".", 1)[1]
                value = gpu_frame.get(leaf)
                if isinstance(value, (int, float)):
                    limits[dotted] = {
                        "max": round(float(value) * (1.0 + margin), 3)
                    }
        if limits:
            scenarios[name] = limits
        else:
            notes.append(f"{name}: no scalar guard metrics were measured.")
    if not any(
        "allocations_per_frame" in metrics for metrics in scenarios.values()
    ):
        notes.append(
            "allocations_per_frame is omitted when the run has no "
            "motioner-bench (alloc-stats) binary."
        )
    if "playback_4k30" not in scenarios:
        notes.append(
            "playback_4k30 is absent from this result; regenerate from "
            "p7-after.json to add its budget."
        )
    return {
        "noise_margin": margin,
        "notes": notes,
        "scenarios": scenarios,
    }


def write_budgets_file(path: Path, results: dict[str, dict], margin: float) -> None:
    """Derive, write and print a budget file."""
    budgets = derive_budgets(results, margin)
    text = json.dumps(budgets, indent=2) + "\n"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")
    print(f"\nwrote budgets {path}")
    print(text)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Measure Motioner release scenarios.")
    parser.add_argument(
        "--binary",
        type=Path,
        default=ROOT / "target" / "release" / "motioner.exe",
    )
    parser.add_argument(
        "--alloc-binary",
        type=Path,
        default=None,
        help="bench build with alloc-stats for allocation-per-frame runs",
    )
    parser.add_argument("--runs", type=int, default=1)
    parser.add_argument("--warmup", type=int, default=1)
    parser.add_argument("--scenario", action="append", default=[], help="only these scenarios")
    parser.add_argument(
        "--from-json",
        type=Path,
        default=None,
        help=(
            "read a recorded result payload instead of running the app; "
            "--json-out/--force are ignored in this mode"
        ),
    )
    parser.add_argument(
        "--json-out",
        type=Path,
        default=None,
        help="output JSON (default: out/reports/phase18/perf-<timestamp>.json)",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="overwrite an existing output file (a baseline is never overwritten implicitly)",
    )
    parser.add_argument("--check", type=Path, default=None, help="budget JSON to enforce")
    parser.add_argument(
        "--write-budgets",
        type=Path,
        default=None,
        help="derive a budget JSON from the measured (or --from-json) results",
    )
    parser.add_argument(
        "--margin",
        type=float,
        default=0.15,
        help="noise margin used by --write-budgets (default: 0.15)",
    )
    parser.add_argument("--alloc-frames", type=int, default=120)
    parser.add_argument(
        "--allow-pointer",
        action="store_true",
        help=(
            "run the pointer scenarios (hover and layers scroll post synthetic "
            "pointer/wheel messages to the app window)"
        ),
    )
    args = parser.parse_args(argv)
    enable_dpi_awareness()
    if args.runs < 1:
        raise SystemExit("--runs must be at least 1")
    if args.margin < 0.0:
        raise SystemExit("--margin must not be negative")

    if args.from_json is not None:
        source = args.from_json
        if not source.is_file():
            raise SystemExit(f"{source} does not exist")
        payload = json.loads(source.read_text(encoding="utf-8"))
        results = payload.get("scenarios", {})
        if not isinstance(results, dict) or not results:
            raise SystemExit(f"{source} has no scenarios")
        for name, result in results.items():
            print_result(name, f"recorded in {source.name}", result)
    else:
        if not args.binary.is_file():
            raise SystemExit(f"{args.binary} does not exist; build the release binary first")
        WORK.mkdir(parents=True, exist_ok=True)
        json_out = args.json_out or (
            WORK / f"perf-{time.strftime('%Y%m%d-%H%M%S')}.json"
        )
        if json_out.exists() and not args.force:
            raise SystemExit(
                f"{json_out} already exists; pass --force to overwrite it "
                "(baselines are never overwritten implicitly)"
            )
        specs = scenario_specs()
        names = args.scenario or list(specs)
        for name in names:
            if name not in specs:
                raise SystemExit(f"unknown scenario {name!r}")

        results = {}
        for name in names:
            spec = specs[name]
            if (
                spec.get("hover")
                or spec.get("hover_static")
                or spec.get("wheel")
                or spec.get("input_probe")
            ) and not args.allow_pointer:
                print(
                    f"\n{name}: skipped "
                    "(needs --allow-pointer: it moves the real mouse and wheel)"
                )
                continue
            endpoint = Path(tempfile.gettempdir()) / f"motioner-p0-{name}.ctl"
            if endpoint.exists():
                endpoint.unlink()
            for _ in range(args.warmup):
                run_once(args.binary, name, spec, endpoint, spec["timeout"], allow_pointer=args.allow_pointer)
            runs = [
                run_once(args.binary, name, spec, endpoint, spec["timeout"], allow_pointer=args.allow_pointer)
                for _ in range(args.runs)
            ]
            result = aggregate(runs)
            if args.alloc_binary is not None and not spec.get("export"):
                alloc_args = spec["args"](endpoint)
                if "--exit-after-frames" in alloc_args:
                    index = alloc_args.index("--exit-after-frames") + 1
                    alloc_args[index] = str(args.alloc_frames)
                alloc_spec = dict(spec)
                alloc_spec["args"] = lambda _p, a=alloc_args: a
                alloc_spec["expected_frames"] = args.alloc_frames
                alloc = run_once(
                    args.alloc_binary,
                    name,
                    alloc_spec,
                    endpoint,
                    spec["timeout"],
                    allow_pointer=False,
                )
                result["allocations_per_frame"] = alloc.get("allocations_per_frame")
                result["allocated_bytes_per_frame"] = alloc.get("allocated_bytes_per_frame")
            results[name] = result
            print_result(name, spec["description"], result)

        payload = {
            "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "binary": str(args.binary),
            "alloc_binary": str(args.alloc_binary) if args.alloc_binary else None,
            "runs": args.runs,
            "warmup": args.warmup,
            "scenarios": results,
        }
        json_out.parent.mkdir(parents=True, exist_ok=True)
        json_out.write_text(
            json.dumps(payload, indent=2) + "\n", encoding="utf-8", newline="\n"
        )
        print(f"\nwrote {json_out}")

    if args.check is not None:
        budgets = json.loads(args.check.read_text(encoding="utf-8"))
        failures = check_budgets(results, budgets)
        if failures:
            print("\ncheck FAILED:")
            for failure in failures:
                print(f"  - {failure}")
            return 1
        print("\ncheck: ok")
    if args.write_budgets is not None:
        write_budgets_file(args.write_budgets, results, args.margin)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
