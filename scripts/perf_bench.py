#!/usr/bin/env python3
"""Measure Motioner release scenarios (prompt 18 P0).

Runs the eight baseline scenarios, samples the process with ``psutil`` and
parses the engine's ``frame-trace``/``memory-report`` JSON lines.  For every
scenario it reports CPU % of one core, frames drawn, GPU ms per pass, the
per-stage medians, allocations per frame (with a bench build), working set,
private bytes, VRAM and threads.

Requirements: ``pip install psutil`` (the only non-stdlib dependency) and a
release build:

    cargo build --release -p motioner_app --bins
    # allocation runs only:
    cargo build --release -p motioner_app --bin motioner-bench --features alloc-stats

Usage:

    python scripts/perf_bench.py --binary target/release/motioner.exe
    python scripts/perf_bench.py --alloc-binary target/release/motioner-bench.exe
    python scripts/perf_bench.py --check scripts/perf_budgets.json

The scenarios:

1. start screen idle 10 s;
2. editor with ``examples/demo.motioner`` idle 10 s;
3. editor hover (the cursor is moved across the window);
4. timeline scrubbing through the control endpoint;
5. 1080p playback of the demo for 10 s;
6. heavy UI (a generated 3000-row text composition);
7. heavy composition (``examples/showcase_pro.motion``);
8. export of 120 frames.

All scenarios run in release.  ``--warmup`` runs (default 1) are discarded.
The frame trace is enabled with ``FERROUS_TRACE_FRAMES=1``; the engine prints
one summary at exit and one memory inventory line.
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
    """Move the cursor over the app's client area until `stop`.

    Only runs when the caller passed ``--allow-pointer``: it takes over the
    user's mouse (and raises the window), so it is never implicit. The
    original cursor position is restored when the loop ends.
    """
    origin = POINT()
    USER32.GetCursorPos(ctypes.byref(origin))
    hwnd = None
    try:
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
                x = left + int((right - left) * (0.1 + 0.8 * (step % 10) / 9.0))
                y = top + int((bottom - top) * (0.15 + 0.7 * ((step // 10) % 2)))
                USER32.SetCursorPos(x, y)
                if stop.wait(0.016):
                    return
    finally:
        USER32.SetCursorPos(origin.x, origin.y)


def hover_static_loop(pid: int, stop: threading.Event) -> None:
    """Move the cursor inside one static panel area until `stop`.

    Every position hits the same target, so the editor's pointer-move hook
    must not request a frame. Only runs with ``--allow-pointer``; the cursor
    is restored.
    """
    origin = POINT()
    USER32.GetCursorPos(ctypes.byref(origin))
    hwnd = None
    try:
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
        x0 = left + int((right - left) * 0.40)
        y0 = top + int((bottom - top) * 0.975)
        step = 0
        while not stop.is_set():
            USER32.SetCursorPos(x0 + (step % 20), y0 + ((step // 20) % 20))
            step += 1
            if stop.wait(0.016):
                return
    finally:
        USER32.SetCursorPos(origin.x, origin.y)


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


# ── fixtures ───────────────────────────────────────────────────────────────


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
            f'{name} = text("Row {index:04d} — a long list label with values 0123456789", '
            f'size=12, color="#d8d8e0", align="left")'
        )
        lines.append(f"{name}.pos = ({x}, {y})")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


# ── scenario definitions ───────────────────────────────────────────────────


def scenario_specs() -> dict[str, dict]:
    heavy = heavy_ui_fixture()
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
        "heavy_ui": {
            "description": "heavy UI: 3000-row text composition",
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


# ── process measurement ────────────────────────────────────────────────────


def parse_json_lines(output: str) -> tuple[dict | None, dict | None]:
    frame = None
    memory = None
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
    return frame, memory


def median_stage(stages: dict, name: str) -> float | None:
    entry = stages.get(name)
    if not isinstance(entry, dict):
        return None
    return entry.get("median_ms")


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
    idle_cpu_s = None
    idle_cpu_percent = None
    if (
        first_frame_at[0] is not None
        and cpu_at_first[0] is not None
        and last_cpu is not None
    ):
        idle_span = last_at - first_frame_at[0]
        if idle_span > 0.0:
            idle_cpu_s = max(0.0, last_cpu - cpu_at_first[0])
            idle_cpu_percent = idle_cpu_s / idle_span * 100.0
    tail_cpu_s = None
    tail_cpu_percent = None
    if tail_cpu is not None and tail_at is not None and last_cpu is not None:
        tail_span = last_at - tail_at
        if tail_span > 0.0:
            tail_cpu_s = max(0.0, last_cpu - tail_cpu)
            tail_cpu_percent = tail_cpu_s / tail_span * 100.0
    frame, memory = parse_json_lines("".join(output_lines))
    frames = 0
    presented = 0
    gpu_submits = 0
    gpu = {}
    stages = {}
    allocs = None
    alloc_bytes = None
    if frame is not None:
        frames = int(frame.get("frames", 0))
        presented = int(frame.get("presented", 0))
        gpu_submits = int(frame.get("gpu_submits", 0))
        stages = frame.get("stages", {})
        gpu = {
            pass_name: float(value.get("median_ms", 0.0))
            for pass_name, value in frame.get("gpu_passes", {}).items()
            if isinstance(value, dict)
        }
        alloc_entry = frame.get("allocations_per_frame", {})
        if frame.get("alloc_stats") and isinstance(alloc_entry, dict):
            allocs = float(alloc_entry.get("median", 0.0))
        bytes_entry = frame.get("bytes_per_frame", {})
        if frame.get("alloc_stats") and isinstance(bytes_entry, dict):
            alloc_bytes = float(bytes_entry.get("median", 0.0))
    expected = int(spec.get("expected_frames", 0))
    if spec.get("export"):
        frames = expected
    result = {
        "scenario": name,
        "wall_s": round(wall, 3),
        "cpu_s": round(cpu_seconds, 3),
        "cpu_percent": round(cpu_seconds / wall * 100.0, 3) if wall > 0 else 0.0,
        "frames": frames,
        "presented": presented,
        "gpu_submits": gpu_submits,
        "idle_cpu_s": round(idle_cpu_s, 4) if idle_cpu_s is not None else None,
        "idle_cpu_percent": (
            round(idle_cpu_percent, 3) if idle_cpu_percent is not None else None
        ),
        "tail_cpu_s": round(tail_cpu_s, 4) if tail_cpu_s is not None else None,
        "tail_cpu_percent": (
            round(tail_cpu_percent, 3) if tail_cpu_percent is not None else None
        ),
        "expected_frames": expected,
        "dropped_frames": max(0, expected - frames),
        "fps": round(frames / wall, 3) if wall > 0 else 0.0,
        "working_set_mb": round(max_rss / (1024 * 1024), 1),
        "private_mb": round(max_private / (1024 * 1024), 1),
        "threads": max_threads,
        "gpu_passes_ms": {key: round(value, 4) for key, value in gpu.items()},
        "stages_ms": {
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
        },
        "allocations_per_frame": allocs,
        "allocated_bytes_per_frame": alloc_bytes,
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

    gpu_keys = sorted({key for run in runs for key in run.get("gpu_passes_ms", {})})
    stage_keys = sorted({key for run in runs for key in run.get("stages_ms", {})})
    return {
        "runs": len(runs),
        "cpu_percent": med("cpu_percent"),
        "wall_s": med("wall_s"),
        "frames": med("frames"),
        "presented": med("presented"),
        "gpu_submits": med("gpu_submits"),
        "idle_cpu_s": med("idle_cpu_s"),
        "idle_cpu_percent": med("idle_cpu_percent"),
        "tail_cpu_s": med("tail_cpu_s"),
        "tail_cpu_percent": med("tail_cpu_percent"),
        "dropped_frames": med("dropped_frames"),
        "fps": med("fps"),
        "working_set_mb": maximum("working_set_mb"),
        "private_mb": maximum("private_mb"),
        "threads": maximum("threads"),
        "allocations_per_frame": med("allocations_per_frame"),
        "allocated_bytes_per_frame": med("allocated_bytes_per_frame"),
        "vram_mb": maximum("vram_mb"),
        "gpu_passes_ms": {
            key: round(
                statistics.median(
                    [run["gpu_passes_ms"][key] for run in runs if key in run["gpu_passes_ms"]]
                ),
                4,
            )
            for key in gpu_keys
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
    print(f"\n{name}: {description}")
    print(
        f"  cpu {result['cpu_percent']}%  wall {result['wall_s']} s  "
        f"frames {result['frames']}  presented {result['presented']}  "
        f"submits {result['gpu_submits']}  dropped {result['dropped_frames']}  "
        f"fps {result['fps']}"
    )
    if result["idle_cpu_percent"] is not None:
        print(
            f"  idle cpu {result['idle_cpu_percent']}% of a core  "
            f"({result['idle_cpu_s']} s after the first frame)"
        )
    if result["tail_cpu_percent"] is not None:
        print(
            f"  idle tail {result['tail_cpu_percent']}% of a core  "
            f"({result['tail_cpu_s']} s in the measured tail)"
        )
    print(
        f"  working set {result['working_set_mb']} MB  private "
        f"{result['private_mb']} MB  threads {result['threads']}  "
        f"VRAM {result['vram_mb']} MB"
    )
    if result["allocations_per_frame"] is not None:
        print(
            f"  allocations/frame {result['allocations_per_frame']}  "
            f"bytes/frame {result['allocated_bytes_per_frame']}"
        )
    if result["gpu_passes_ms"]:
        passes = ", ".join(f"{key} {value:.3f} ms" for key, value in result["gpu_passes_ms"].items())
        print(f"  GPU passes: {passes}")
    if result["stages_ms"]:
        stages = ", ".join(f"{key} {value:.3f}" for key, value in result["stages_ms"].items())
        print(f"  stages (ms): {stages}")
    if result.get("export_fps"):
        print(f"  export {result['export_fps']} frames/s")


def check_budgets(results: dict[str, dict], budgets: dict) -> list[str]:
    failures: list[str] = []
    for scenario, metrics in budgets.get("scenarios", {}).items():
        if scenario not in results:
            failures.append(f"{scenario}: no measurement")
            continue
        measured = results[scenario]
        for metric, limits in metrics.items():
            value = measured.get(metric)
            if value is None:
                failures.append(f"{scenario}.{metric}: not measured")
                continue
            if "max" in limits and value > limits["max"]:
                failures.append(
                    f"{scenario}.{metric}: {value} > max {limits['max']}"
                )
            if "min" in limits and value < limits["min"]:
                failures.append(
                    f"{scenario}.{metric}: {value} < min {limits['min']}"
                )
    return failures


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
    parser.add_argument("--alloc-frames", type=int, default=120)
    parser.add_argument(
        "--allow-pointer",
        action="store_true",
        help="allow the hover scenario to move the real cursor (and restore it)",
    )
    args = parser.parse_args(argv)
    if args.runs < 1:
        raise SystemExit("--runs must be at least 1")
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

    results: dict[str, dict] = {}
    for name in names:
        spec = specs[name]
        if (spec.get("hover") or spec.get("hover_static")) and not args.allow_pointer:
            print(f"\n{name}: skipped (needs --allow-pointer: it moves the real mouse)")
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
    json_out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
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
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
