"""Real-screen startup check (prompt 17 S3b, outside the gate).

Launches the app, waits for its first *visible* top-level window, and captures
the desktop (PIL ImageGrab — the real screen, not a GPU readback) at the moment
it becomes visible and at +16/+50/+150 ms, plus a reference capture later.

Why it exists: an earlier S3b launch-snapshot implementation was declared good
from a capture taken by a *new PowerShell process*, whose own start-up delay
(several hundred ms) meant the frame was already up when it looked. This script
polls the window from the same process and grabs the screen immediately.

Modes:
    (default)            the first visible capture must already show the drawn
                         screen: it matches the reference capture (mean absolute
                         difference <= --tolerance) and is not a uniform
                         white/black client area.
    --expect-background  the first visible capture must be the app background
                         colour (the safety-timeout path) and the window must
                         appear around the timeout; the reference capture (take
                         it after the first frame, e.g. --reference-delay 12000
                         for DX12) must be the drawn screen.

Usage:
    python scripts/screen_check.py --args "--start --exit-after-frames 3"
    python scripts/screen_check.py --args "--start --exit-after-frames 3" \\
        --timeout-ms 50 --expect-background --reference-delay 1200
    python scripts/screen_check.py --args "--start --exit-after-frames 3" \\
        --backend dx12 --expect-background --reference-delay 12000

Exit code 0 when every run passes.
"""

from __future__ import annotations

import argparse
import ctypes
import ctypes.wintypes as wt
import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

try:
    from PIL import ImageGrab
except ImportError:  # pragma: no cover - environment guard
    print("screen_check needs Pillow (pip install pillow)", file=sys.stderr)
    raise SystemExit(2)

USER32 = ctypes.windll.user32
KERNEL32 = ctypes.windll.kernel32

SM_XVIRTUALSCREEN = 76
SM_YVIRTUALSCREEN = 77
SM_CXVIRTUALSCREEN = 78
SM_CYVIRTUALSCREEN = 79
WM_NULL = 0x0000
SMTO_ABORTIFHUNG = 0x0002
SW_RESTORE = 9
SWP_NOSIZE = 0x0001
SWP_NOMOVE = 0x0002
SWP_SHOWWINDOW = 0x0040
HWND_TOPMOST = ctypes.c_void_p(-1)
HWND_NOTOPMOST = ctypes.c_void_p(-2)

# Without argument types ctypes truncates HWND values to 32 bits and every call
# below silently fails.
USER32.GetSystemMetrics.argtypes = [ctypes.c_int]
USER32.GetSystemMetrics.restype = ctypes.c_int
USER32.GetWindowThreadProcessId.argtypes = [wt.HWND, ctypes.POINTER(wt.DWORD)]
USER32.GetWindowThreadProcessId.restype = wt.DWORD
USER32.IsWindowVisible.argtypes = [wt.HWND]
USER32.IsWindowVisible.restype = wt.BOOL
USER32.EnumWindows.argtypes = [ctypes.c_void_p, wt.LPARAM]
USER32.EnumWindows.restype = wt.BOOL
USER32.GetWindowRect.argtypes = [wt.HWND, ctypes.c_void_p]
USER32.GetClientRect.argtypes = [wt.HWND, ctypes.c_void_p]
USER32.ClientToScreen.argtypes = [wt.HWND, ctypes.c_void_p]
USER32.SetForegroundWindow.argtypes = [wt.HWND]
USER32.BringWindowToTop.argtypes = [wt.HWND]
USER32.ShowWindow.argtypes = [wt.HWND, ctypes.c_int]
USER32.SetWindowPos.argtypes = [
    wt.HWND,
    ctypes.c_void_p,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_int,
    wt.UINT,
]
USER32.IsHungAppWindow.argtypes = [wt.HWND]
USER32.IsHungAppWindow.restype = wt.BOOL
USER32.WindowFromPoint.argtypes = [wt.POINT]
USER32.WindowFromPoint.restype = wt.HWND
USER32.GetAncestor.argtypes = [wt.HWND, wt.UINT]
USER32.GetAncestor.restype = wt.HWND
GA_ROOT = 2
USER32.SendMessageTimeoutW.argtypes = [
    wt.HWND,
    wt.UINT,
    wt.WPARAM,
    wt.LPARAM,
    wt.UINT,
    wt.UINT,
    ctypes.POINTER(ctypes.c_size_t),
]
USER32.SendMessageTimeoutW.restype = wt.LPARAM

ROOT = Path(__file__).resolve().parent.parent


class RECT(ctypes.Structure):
    _fields_ = [
        ("left", wt.LONG),
        ("top", wt.LONG),
        ("right", wt.LONG),
        ("bottom", wt.LONG),
    ]


def dpi_aware() -> None:
    """Physical pixels everywhere.

    Without this, every rect and the desktop capture are virtualised to 96 DPI
    and the grabs show a scaled, wrong area (which is how an earlier check
    "passed" while looking at the wrong pixels).
    """
    try:
        ok = USER32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    except AttributeError:  # older Windows
        ok = None
        ctypes.windll.shcore.SetProcessDpiAwareness(2)
    if ok is False:
        raise SystemExit("SetProcessDpiAwarenessContext failed: captures would be scaled")


def windows_of(pid: int) -> list[int]:
    found: list[int] = []
    callback_type = ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)

    def callback(hwnd: int, _lparam: int) -> bool:
        owner = wt.DWORD()
        USER32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and USER32.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True

    USER32.EnumWindows(callback_type(callback), 0)
    return found


def largest(hwnds: list[int]) -> int | None:
    """The biggest window with a real client area.

    winit keeps a message-only window ("Winit Thread Event Target") that
    EnumWindows reports as visible but has no client area; the application
    window is the one with pixels.
    """
    best = None
    best_area = 0
    for hwnd in hwnds:
        rect = RECT()
        USER32.GetWindowRect(hwnd, ctypes.byref(rect))
        area = (rect.right - rect.left) * (rect.bottom - rect.top)
        client = RECT()
        if not USER32.GetClientRect(hwnd, ctypes.byref(client)):
            continue
        if client.right < 64 or client.bottom < 64:
            continue
        if area > best_area:
            best, best_area = hwnd, area
    return best


def screen_box() -> tuple[int, int, int, int]:
    """The virtual desktop rectangle in physical pixels."""
    x = USER32.GetSystemMetrics(SM_XVIRTUALSCREEN)
    y = USER32.GetSystemMetrics(SM_YVIRTUALSCREEN)
    return (x, y, x + USER32.GetSystemMetrics(SM_CXVIRTUALSCREEN), y + USER32.GetSystemMetrics(SM_CYVIRTUALSCREEN))


def client_box(hwnd: int) -> tuple[int, int, int, int] | None:
    """The visible part of the client area.

    A window bigger than the screen is cut by the desktop; those rows show as
    black in a desktop capture and say nothing about what the window paints, so
    they are excluded.
    """
    rect = RECT()
    if not USER32.GetClientRect(hwnd, ctypes.byref(rect)):
        return None
    width = rect.right - rect.left
    height = rect.bottom - rect.top
    if width <= 0 or height <= 0:
        return None
    origin = wt.POINT(0, 0)
    USER32.ClientToScreen(hwnd, ctypes.byref(origin))
    left = max(origin.x, screen_box()[0])
    top = max(origin.y, screen_box()[1])
    right = min(origin.x + width, screen_box()[2])
    bottom = min(origin.y + height, screen_box()[3])
    if right - left < 64 or bottom - top < 64:
        return None
    return (left, top, right, bottom)


def responsive(hwnd: int) -> bool:
    if USER32.IsHungAppWindow(hwnd):
        return False
    result = ctypes.c_size_t(0)
    status = USER32.SendMessageTimeoutW(
        hwnd, WM_NULL, 0, 0, SMTO_ABORTIFHUNG, 250, ctypes.byref(result)
    )
    return bool(status)


def raise_window(hwnd: int) -> None:
    """Bring the app above other windows before capturing the desktop.

    `SetForegroundWindow` alone is refused while another app owns the
    foreground (Windows focus stealing rules), and a window covered by another
    application would make the capture compare the wrong pixels.  A short
    topmost round-trip forces the app into view; the flag is removed after the
    reference capture.
    """
    USER32.ShowWindow(hwnd, SW_RESTORE)
    USER32.SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOSIZE | SWP_NOMOVE | SWP_SHOWWINDOW,
    )
    USER32.SetForegroundWindow(hwnd)
    USER32.BringWindowToTop(hwnd)


def lower_window(hwnd: int) -> None:
    """Removes the temporary topmost flag set by [`raise_window`]."""
    USER32.SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOSIZE | SWP_NOMOVE)


def covering_window(hwnd: int, box: tuple[int, int, int, int]) -> int | None:
    """A top-level window other than `hwnd` over the client area, if any.

    A desktop capture is only meaningful while the app is on top. Taskbar
    thumbnails and other applications can cover part of the client area; the
    capture would then compare those pixels instead of the app.
    """
    left, top, right, bottom = box
    fractions = ((0.1, 0.1), (0.9, 0.1), (0.5, 0.5), (0.1, 0.9), (0.9, 0.9))
    for fx, fy in fractions:
        point = wt.POINT(int(left + (right - left) * fx), int(top + (bottom - top) * fy))
        at = USER32.WindowFromPoint(point)
        if not at:
            continue
        root = USER32.GetAncestor(at, GA_ROOT)
        if root != hwnd:
            return int(root or at)
    return None


def capture_problem(process, hwnd: int, box: tuple[int, int, int, int]) -> str | None:
    """Why the window cannot be captured right now, or ``None``.

    The process may finish before a late capture (`--exit-after-frames` small)
    and the window may be hidden by closing or covered by another window; in
    both cases the desktop capture would show something else.
    """
    if process.poll() is not None:
        return (
            "the process exited before this capture; keep the window open longer "
            "with --exit-after-frames (e.g. 120) or use --paused"
        )
    if not USER32.IsWindowVisible(hwnd):
        return "the window was hidden before this capture"
    cover = covering_window(hwnd, box)
    if cover is not None:
        return f"another window (0x{cover:x}) covers the app; bring it to the front"
    return None


def recheck_capture(process, hwnd: int, box: tuple[int, int, int, int]) -> str | None:
    """Raises the app once and re-checks coverage before failing a run."""
    problem = capture_problem(process, hwnd, box)
    if problem is None:
        return None
    if "exited" in problem or "hidden" in problem:
        return problem
    raise_window(hwnd)
    time.sleep(0.05)
    return capture_problem(process, hwnd, box)


def distinct_colours(image, size=(200, 120)) -> int:
    small = image.convert("RGB").resize(size)
    return len(set(small.getdata()))


def mean_difference(a, b) -> float:
    a_small = a.convert("RGB").resize((160, 96))
    b_small = b.convert("RGB").resize((160, 96))
    total = 0
    count = 0
    for pa, pb in zip(a_small.getdata(), b_small.getdata()):
        total += abs(pa[0] - pb[0]) + abs(pa[1] - pb[1]) + abs(pa[2] - pb[2])
        count += 3
    return total / count


def uniform_fraction(image, colour: tuple[int, int, int], tolerance: int = 3) -> float:
    small = image.convert("RGB").resize((160, 96))
    pixels = list(small.getdata())
    close = sum(
        1
        for pixel in pixels
        if all(abs(pixel[i] - colour[i]) <= tolerance for i in range(3))
    )
    return close / len(pixels)


def run_once(args: argparse.Namespace, out_dir: Path, index: int) -> dict:
    env = os.environ.copy()
    env.pop("FERROUS_SHOW_TIMEOUT_MS", None)
    if args.timeout_ms is not None:
        env["FERROUS_SHOW_TIMEOUT_MS"] = str(args.timeout_ms)
    if args.backend:
        env["WGPU_BACKEND"] = args.backend
    for entry in args.env:
        key, _, value = entry.partition("=")
        env[key] = value

    command = [str(Path(args.binary)), *args.args.split()]
    started = time.perf_counter()
    process = subprocess.Popen(
        command,
        cwd=str(ROOT),
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        hwnd = None
        visible_ms = None
        while time.perf_counter() - started < args.timeout_seconds:
            hwnd = largest(windows_of(process.pid))
            if hwnd is not None:
                visible_ms = (time.perf_counter() - started) * 1000.0
                break
            time.sleep(0.005)
        if hwnd is None or visible_ms is None:
            return {
                "run": index + 1,
                "pass": False,
                "error": "no visible window within the timeout",
            }
        box = client_box(hwnd)
        if box is None:
            return {"run": index + 1, "pass": False, "error": "empty client area"}
        raise_window(hwnd)
        problem = recheck_capture(process, hwnd, box)
        if problem is not None:
            return {"run": index + 1, "pass": False, "error": problem}

        shots = []
        for delay in (0, 16, 50, 150):
            target = visible_ms / 1000.0 + delay / 1000.0
            remaining = target - (time.perf_counter() - started)
            if remaining > 0:
                time.sleep(remaining)
            problem = capture_problem(process, hwnd, box)
            if problem is not None:
                # A small --exit-after-frames closes the window before the
                # +150 ms shot and the reference; that capture would be the
                # desktop behind it (the terminal), not the app.
                return {"run": index + 1, "pass": False, "error": problem}
            image = ImageGrab.grab(bbox=box, all_screens=True)
            shots.append((delay, image))

        hung = not responsive(hwnd)

        reference_delay_ms = args.reference_delay
        remaining = (visible_ms + reference_delay_ms) / 1000.0 - (
            time.perf_counter() - started
        )
        if remaining > 0:
            time.sleep(remaining)
        problem = recheck_capture(process, hwnd, box)
        if problem is not None:
            return {"run": index + 1, "pass": False, "error": problem}
        reference = ImageGrab.grab(bbox=box, all_screens=True)
        lower_window(hwnd)

        prefix = f"run{index + 1}"
        first = shots[0][1]
        for delay, image in shots:
            image.save(out_dir / f"{prefix}-visible-{delay}ms.png")
        reference.save(out_dir / f"{prefix}-reference.png")
        timeline = [
            {
                "at_ms": delay,
                "colours": distinct_colours(image),
                "vs_first": round(mean_difference(first, image), 2),
                "vs_reference": round(mean_difference(image, reference), 2),
            }
            for delay, image in shots
        ]

        result = {
            "run": index + 1,
            "visible_ms": round(visible_ms, 1),
            "client": [box[2] - box[0], box[3] - box[1]],
            "capture": list(first.size),
            "first_colours": distinct_colours(first),
            "reference_colours": distinct_colours(reference),
            "first_vs_reference": round(mean_difference(first, reference), 2),
            "responsive_at_visibility": not hung,
            "alive": process.poll() is None,
            "timeline": timeline,
        }

        problems = []
        if first.size != (box[2] - box[0], box[3] - box[1]):
            problems.append(f"capture size {first.size} is not the client area")
        if max(first.size) < 1000:
            problems.append(f"capture is {first.size}: the process is DPI-virtualised")
        if result["reference_colours"] <= 3:
            problems.append("the reference capture is a uniform colour")

        # The capture taken the instant the window is reported visible predates
        # DWM's first composition of it (one vsync, ~16 ms at 60 Hz): the screen
        # still shows whatever was there. The first *composed* capture is the
        # +16 ms one, and that is what the user can see; it is the one that must
        # already be the drawn screen (or the background on the timeout path).
        composed = next(image for delay, image in shots if delay == 16)
        result["composed_vs_reference"] = round(mean_difference(composed, reference), 2)
        result["composed_colours"] = distinct_colours(composed)

        if args.expect_background:
            expected = args.background
            expected_rgb = tuple(int(expected[i : i + 2], 16) for i in (0, 2, 4))
            fraction = uniform_fraction(composed, expected_rgb)
            result["background_fraction"] = round(fraction, 3)
            result["composed_vs_first"] = round(mean_difference(composed, first), 2)
            # The timeout path is valid in two shapes: the background is shown
            # while the first frame is not ready, or the first frame already
            # beat the timeout and is on screen. Anything else (a stale window,
            # white/black) fails.
            drawn_already = result["composed_vs_reference"] <= args.tolerance
            if fraction < 0.9 and not drawn_already:
                problems.append(
                    f"the first composed capture is neither the app background "
                    f"{expected} ({fraction:.0%} of the client area) nor the drawn "
                    f"screen (difference {result['composed_vs_reference']})"
                )
            if result["composed_colours"] <= 3 and not drawn_already and fraction < 0.9:
                problems.append("the first composed capture is a uniform colour")
            if result["reference_colours"] <= 3:
                problems.append("the reference capture is a uniform colour")
            if (
                args.reference_delay >= 1000
                and result["composed_vs_reference"] < args.tolerance
                and fraction >= 0.9
            ):
                # Close to the reference *and* the background: the reference is
                # the plain background, so the first frame never arrived.
                problems.append(
                    "the reference capture still shows the background: the first "
                    "frame never arrived"
                )
            if not result["responsive_at_visibility"]:
                problems.append("the window is not responding at visibility")
            if args.timeout_ms is not None:
                # The timeout is enforced by the event loop, which cannot run
                # while the device/swapchain phase blocks it (~450 ms on the
                # reference machine). The window therefore appears at
                # max(timeout, that floor).
                low = max(0.0, args.timeout_ms - args.timeout_slack_ms)
                high = args.timeout_ms + args.timeout_floor_ms
                if not (low <= visible_ms <= high):
                    problems.append(
                        f"window visible at {visible_ms:.0f} ms, expected "
                        f"{args.timeout_ms} ms plus at most "
                        f"{args.timeout_floor_ms} ms of device init"
                    )
        else:
            if (
                not args.allow_late_content
                and result["composed_vs_reference"] > args.tolerance
            ):
                problems.append(
                    f"the first composed capture differs from the reference by "
                    f"{result['composed_vs_reference']} (tolerance {args.tolerance})"
                )
            if result["composed_colours"] <= 3:
                problems.append("the first composed capture is a uniform colour")
            white = uniform_fraction(composed, (255, 255, 255), 2)
            black = uniform_fraction(composed, (0, 0, 0), 2)
            if white > 0.9:
                problems.append("the first composed capture is a white client area")
            if black > 0.9:
                problems.append("the first composed capture is a black client area")
            if not result["responsive_at_visibility"]:
                problems.append("the window is not responding at visibility")

        result["pass"] = not problems
        if problems:
            result["problems"] = problems
        return result
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary", default=str(ROOT / "target" / "release" / "motioner.exe")
    )
    parser.add_argument("--args", default="--start")
    parser.add_argument("--runs", type=int, default=1)
    parser.add_argument("--backend", default=None, help="WGPU_BACKEND value")
    parser.add_argument("--env", action="append", default=[], metavar="KEY=VALUE")
    parser.add_argument("--timeout-ms", type=int, default=None)
    parser.add_argument("--timeout-slack-ms", type=int, default=300)
    parser.add_argument(
        "--timeout-floor-ms",
        type=int,
        default=700,
        help="how late the window may appear after the timeout because the "
        "device/swapchain phase blocks the event loop",
    )
    parser.add_argument("--reference-delay", type=int, default=1000)
    parser.add_argument("--tolerance", type=float, default=6.0)
    parser.add_argument(
        "--allow-late-content",
        action="store_true",
        help="accept a first frame that differs from the reference when media "
        "loads asynchronously (the chrome must still be drawn, never white/black)",
    )
    parser.add_argument(
        "--expect-background",
        action="store_true",
        help="the first visible capture must be the background colour",
    )
    parser.add_argument("--background", default="0A0B0D")
    parser.add_argument("--timeout-seconds", type=float, default=20.0)
    parser.add_argument("--out-dir", default="out/reports/phase17/screen-check")
    parser.add_argument("--json-out", default=None)
    args = parser.parse_args()

    dpi_aware()
    out_dir = Path(args.out_dir)
    if not out_dir.is_absolute():
        out_dir = ROOT / out_dir
    out_dir.mkdir(parents=True, exist_ok=True)

    results = []
    for index in range(args.runs):
        result = run_once(args, out_dir, index)
        results.append(result)
        line = f"run {result['run']}: {'PASS' if result['pass'] else 'FAIL'}"
        if "visible_ms" in result:
            line += (
                f"  visible {result['visible_ms']} ms"
                f"  colours {result['first_colours']}/{result['reference_colours']}"
                f"  first-vs-ref {result['first_vs_reference']}"
            )
        if result.get("problems"):
            line += "  " + "; ".join(result["problems"])
        if result.get("error"):
            line += f"  {result['error']}"
        print(line)

    if args.json_out:
        Path(args.json_out).write_text(
            json.dumps(results, indent=2), encoding="utf-8"
        )
    passed = sum(1 for result in results if result["pass"])
    if results and "visible_ms" in results[0]:
        values = [r["visible_ms"] for r in results if "visible_ms" in r]
        if len(values) >= 2:
            print(
                f"visible: median {statistics.median(values):.0f} ms, "
                f"max {max(values):.0f} ms"
            )
    print(f"{passed}/{len(results)} runs passed; captures in {out_dir}")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
