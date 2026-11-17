#!/usr/bin/env python3
"""Measure Motioner's startup with the engine's startup trace.

Runs the release binary N times for two scenarios and prints median/p90 per
startup mark:

    python scripts/startup_bench.py --runs 20

    python scripts/startup_bench.py --scenario preview --markdown

`--clear-cache DIR` deletes the font atlas cache before every run, which is how
a cold bake is measured (the default cache lives in
`%LOCALAPPDATA%\\Motioner\\cache\\fonts` on Windows):

    python scripts/startup_bench.py --cold --runs 3 --clear-cache "%LOCALAPPDATA%\\Motioner\\cache\\fonts"

The engine must be built with the startup trace (any release build is: the
trace is enabled by an environment variable this script sets). Every run is a
separate process, so the numbers include process creation, the GPU driver and
the first presented frame -- not the process exit, except for the rows that
measure the closing path on purpose.

The trace reports are emitted on stderr by the engine:

    startup-trace {"event": "startup-trace", "trace": {"marks": [...]}}
    startup-close {"event": "startup-close", "trace": {"marks": [...]}}

Marks are milliseconds since the first `startup::begin` of the process, which
`motioner` calls as the first statement of `main`. The wall time row is what
the shell sees (process creation plus loader plus exit).

Regression guard (`--check`)
----------------------------

`python scripts/startup_bench.py --check` is the guard the reviewer runs on
the reference machine. It fails (exit code 1) when:

- the window becomes visible before its first frame on the normal path
  (`engine: window visible` earlier than `engine: first frame presented`).
  The safety-timeout path is the only accepted exception and is identified by
  `engine: window visible (timeout)`;
- one of our own critical-path metrics (`scripts/startup_budgets.json`)
  exceeds its budget. The budgets are 2x the S6 measured medians (with a small
  floor for sub-millisecond spans) and the close-to-hidden target of 50 ms.
  The GPU driver floor (instance, adapter, device, swapchain) is excluded: the
  path budget is `first frame presented - surface configured`, so a slower
  driver does not fail the guard.

The guard is outside the CI gate on purpose (first-run Defender scans and GPU
state make wall-clock checks unreliable in CI); it is a reviewer tool.

This script is otherwise a measurement tool, not a gate: a measurement run
never fails on a slow run unless a run cannot produce a trace at all.

Only the Python standard library is used.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Scenario name -> extra arguments after the binary.
SCENARIOS = {
    "start": ["--start", "--exit-after-frames", "1"],
    "preview": ["preview", "examples/demo.motioner", "--exit-after-frames", "1"],
}

FIRST_PRESENT = "engine: first frame presented"
WINDOW_VISIBLE = "engine: window visible"
WINDOW_VISIBLE_TIMEOUT = "engine: window visible (timeout)"
SURFACE_CONFIGURED = "engine: surface configured"
CLOSE_REQUESTED = "engine: close requested"
WINDOW_HIDDEN = "engine: window hidden"
TEARDOWN_DONE = "engine: teardown done"

TRACE_PREFIX = "startup-trace "
CLOSE_PREFIX = "startup-close "

OUR_CRITICAL_PATH = "path: our critical path"
CLOSE_TO_HIDDEN = "close: requested to hidden"

# Metrics the guard verifies when `--check` (or `--write-budgets`) selects the
# defaults. A metric that should exist but is missing from the run also fails
# the guard: it means the trace changed and the budget is no longer checked.
BUDGETED_METRICS = {
    "start": [
        OUR_CRITICAL_PATH,
        "span: motioner: host",
        "span: motioner: recents",
        "span: motioner: examples",
        "span: motioner: app new",
        "span: engine: renderer build (worker)",
        CLOSE_TO_HIDDEN,
    ],
    "preview": [
        OUR_CRITICAL_PATH,
        "span: motioner: resolve input",
        "span: motioner: project fonts",
        "span: motioner: document load",
        "span: motioner: canvas",
        "span: motioner: paint composition",
        "span: motioner: refresh assets",
        "span: motioner: update",
        "span: engine: renderer build (worker)",
        CLOSE_TO_HIDDEN,
    ],
}

# A sub-millisecond span must not fail because the scheduler blinked.
BUDGET_FLOOR_MS = 5.0
# Closing the window is a user-visible target, not a measured budget.
CLOSE_BUDGET_MS = 50.0


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Measure Motioner startup from the engine's startup trace.",
    )
    parser.add_argument(
        "--runs",
        type=int,
        default=None,
        help="measured runs per scenario (default: 5 with --check, 20 otherwise)",
    )
    parser.add_argument(
        "--binary",
        type=Path,
        default=None,
        help="release binary (default: target/release/motioner[.exe])",
    )
    parser.add_argument(
        "--scenario",
        choices=sorted(SCENARIOS) + ["both"],
        default="both",
        help="what to launch (default: both)",
    )
    parser.add_argument(
        "--cold",
        action="store_true",
        help="skip the warm-up run (the first run pays the loader/Defender scan)",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=120.0,
        help="seconds per run before it is killed (default: 120)",
    )
    parser.add_argument(
        "--json-out",
        type=Path,
        default=None,
        help="write the aggregated numbers to this JSON file",
    )
    parser.add_argument(
        "--clear-cache",
        type=Path,
        action="append",
        default=[],
        metavar="DIR",
        help=(
            "delete this directory before every run so its cache is cold; repeat "
            "for the font atlas (%) and the pipelines (%): "
            "%%LOCALAPPDATA%%\\Motioner\\cache\\fonts, "
            "%%LOCALAPPDATA%%\\Motioner\\cache\\pipelines"
        ),
    )
    parser.add_argument(
        "--markdown",
        action="store_true",
        help="print Markdown tables instead of aligned text",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="run the regression guard (budgets + window-before-first-frame rule)",
    )
    parser.add_argument(
        "--budgets",
        type=Path,
        default=ROOT / "scripts" / "startup_budgets.json",
        help="budget file read by --check (default: scripts/startup_budgets.json)",
    )
    parser.add_argument(
        "--write-budgets",
        type=Path,
        default=None,
        metavar="FILE",
        help="write budgets.json from this run's measurements (2x, with floors)",
    )
    return parser.parse_args(argv)


def default_binary() -> Path:
    suffix = ".exe" if os.name == "nt" else ""
    return ROOT / "target" / "release" / f"motioner{suffix}"


def trace_environment() -> dict[str, str]:
    env = os.environ.copy()
    env["FERROUS_TRACE_STARTUP"] = "1"
    env["FERROUS_TRACE_STARTUP_JSON"] = "1"
    return env


def parse_streams(output: str) -> tuple[dict[str, float], dict[str, float]]:
    """Return (first-present marks, close marks) from one process's output."""
    first: dict[str, float] = {}
    close: dict[str, float] = {}
    for line in output.splitlines():
        line = line.strip()
        if line.startswith(TRACE_PREFIX):
            envelope = json.loads(line[len(TRACE_PREFIX) :])
            first = marks_of(envelope)
        elif line.startswith(CLOSE_PREFIX):
            envelope = json.loads(line[len(CLOSE_PREFIX) :])
            close = marks_of(envelope)
    return first, close


def marks_of(envelope: dict) -> dict[str, float]:
    trace = envelope.get("trace", {})
    return {mark["name"]: float(mark["at_ms"]) for mark in trace.get("marks", [])}


def span_durations(output: str) -> dict[str, float]:
    """Span name -> duration in ms from the first-present report."""
    durations: dict[str, float] = {}
    for line in output.splitlines():
        line = line.strip()
        if not line.startswith(TRACE_PREFIX):
            continue
        envelope = json.loads(line[len(TRACE_PREFIX) :])
        for span in envelope.get("trace", {}).get("spans", []):
            name = span["name"]
            duration = float(span["end_ms"]) - float(span["start_ms"])
            # Faces are unique; a repeated span name keeps the first one.
            durations.setdefault(name, duration)
    return durations


def run_once(
    binary: Path, args: list[str], timeout: float, clear_caches: list[Path]
) -> tuple[dict, float, str]:
    for cache in clear_caches:
        shutil.rmtree(cache, ignore_errors=True)
    started = time.perf_counter()
    completed = subprocess.run(
        [str(binary), *args],
        cwd=str(ROOT),
        env=trace_environment(),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
        check=False,
    )
    wall_ms = (time.perf_counter() - started) * 1000.0
    output = f"{completed.stdout}\n{completed.stderr}"
    return completed, wall_ms, output


def p90(values: list[float]) -> float:
    """Nearest-rank 90th percentile."""
    ordered = sorted(values)
    index = max(0, math.ceil(0.9 * len(ordered)) - 1)
    return ordered[index]


class Metrics:
    """Named samples from every run."""

    def __init__(self) -> None:
        self.samples: dict[str, list[float]] = {}

    def add(self, name: str, value: float) -> None:
        self.samples.setdefault(name, []).append(value)

    def rows(self) -> list[tuple[str, float, float, int]]:
        rows = []
        for name, values in self.samples.items():
            rows.append((name, statistics.median(values), p90(values), len(values)))
        return rows

    def to_json(self) -> dict[str, dict[str, float | int]]:
        return {
            name: {
                "median_ms": round(statistics.median(values), 3),
                "p90_ms": round(p90(values), 3),
                "runs": len(values),
            }
            for name, values in self.samples.items()
        }


def window_before_first_frame(marks: dict[str, float]) -> str | None:
    """Why `marks` violates the show-when-ready rule, or ``None``.

    The normal path records `engine: window visible` after the first frame was
    presented. The safety-timeout path is the only accepted exception and is
    marked `engine: window visible (timeout)`.
    """
    if WINDOW_VISIBLE not in marks or FIRST_PRESENT not in marks:
        return None
    if marks[WINDOW_VISIBLE] < marks[FIRST_PRESENT]:
        return (
            f"the window became visible at {marks[WINDOW_VISIBLE]:.2f} ms, "
            f"before the first frame at {marks[FIRST_PRESENT]:.2f} ms "
            "(not the timeout path)"
        )
    return None


def measure(
    scenario: str, binary: Path, args: list[str], opts, runs: int
) -> tuple[Metrics, list[str], list[str]]:
    metrics = Metrics()
    warnings: list[str] = []
    violations: list[str] = []

    if not opts.cold:
        run_once(binary, args, opts.timeout, opts.clear_cache)

    succeeded = 0
    for index in range(runs):
        try:
            completed, wall_ms, output = run_once(
                binary, args, opts.timeout, opts.clear_cache
            )
        except subprocess.TimeoutExpired:
            warnings.append(f"{scenario}: run {index + 1} timed out after {opts.timeout:g}s")
            continue
        if completed.returncode != 0:
            warnings.append(
                f"{scenario}: run {index + 1} exited with {completed.returncode}"
            )
            continue
        first, close = parse_streams(output)
        if not first:
            warnings.append(f"{scenario}: run {index + 1} produced no startup trace")
            continue
        succeeded += 1
        violation = window_before_first_frame(first)
        if violation is not None:
            violations.append(f"{scenario}: run {index + 1}: {violation}")
        metrics.add("process wall time", wall_ms)
        for name, at_ms in first.items():
            metrics.add(f"mark: {name}", at_ms)
        for name, duration in span_durations(output).items():
            metrics.add(f"span: {name}", duration)
        if FIRST_PRESENT in first and SURFACE_CONFIGURED in first:
            metrics.add(OUR_CRITICAL_PATH, first[FIRST_PRESENT] - first[SURFACE_CONFIGURED])
        if CLOSE_REQUESTED in close and WINDOW_HIDDEN in close:
            metrics.add(
                CLOSE_TO_HIDDEN,
                close[WINDOW_HIDDEN] - close[CLOSE_REQUESTED],
            )
        if WINDOW_HIDDEN in close and TEARDOWN_DONE in close:
            metrics.add(
                "teardown: hidden to teardown done",
                close[TEARDOWN_DONE] - close[WINDOW_HIDDEN],
            )
        if FIRST_PRESENT in first and TEARDOWN_DONE in close:
            metrics.add(
                "present to teardown done",
                close[TEARDOWN_DONE] - first[FIRST_PRESENT],
            )

    if succeeded == 0:
        raise SystemExit(
            f"no usable run for scenario {scenario!r}; is the trace enabled in "
            f"{binary}? ({'; '.join(warnings) or 'no output'})"
        )
    return metrics, warnings, violations


def load_budgets(path: Path) -> dict:
    """Reads a budget file, failing with a readable message."""
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise SystemExit(f"cannot read budgets {path}: {error}") from error
    except json.JSONDecodeError as error:
        raise SystemExit(f"invalid budgets JSON in {path}: {error}") from error


def evaluate_budgets(scenario: str, metrics: Metrics, budgets: dict) -> list[str]:
    """Compares the measured medians/p90 with the budgets for `scenario`."""
    scenarios = budgets.get("scenarios", {})
    entries = scenarios.get(scenario, {}).get("metrics", {})
    if not entries:
        return [f"{scenario}: the budget file has no metrics for this scenario"]
    measured = {
        name: (median, p90_value)
        for name, median, p90_value, _count in metrics.rows()
    }
    failures: list[str] = []
    for name, limits in entries.items():
        if name not in measured:
            failures.append(
                f"{scenario}: {name!r} was not measured; the trace changed and the "
                "budget cannot be checked"
            )
            continue
        median, p90_value = measured[name]
        budget_median = limits.get("median_ms")
        budget_p90 = limits.get("p90_ms")
        if budget_median is not None and median > budget_median:
            failures.append(
                f"{scenario}: {name} median {median:.2f} ms exceeds the "
                f"budget {budget_median:.2f} ms"
            )
        if budget_p90 is not None and p90_value > budget_p90:
            failures.append(
                f"{scenario}: {name} p90 {p90_value:.2f} ms exceeds the "
                f"budget {budget_p90:.2f} ms"
            )
    return failures


def budget_entries(metrics: Metrics, scenario: str, multiplier: float) -> dict:
    """Builds the budget entries for one scenario from its measurements."""
    measured = {
        name: (median, p90_value)
        for name, median, p90_value, _count in metrics.rows()
    }
    entries: dict[str, dict[str, float]] = {}
    for name in BUDGETED_METRICS[scenario]:
        if name not in measured:
            continue
        median, p90_value = measured[name]
        if name == CLOSE_TO_HIDDEN:
            entries[name] = {
                "median_ms": CLOSE_BUDGET_MS,
                "p90_ms": CLOSE_BUDGET_MS,
            }
            continue
        entries[name] = {
            "median_ms": round(max(median * multiplier, BUDGET_FLOOR_MS), 2),
            "p90_ms": round(max(p90_value * multiplier, BUDGET_FLOOR_MS), 2),
        }
    return entries


def print_table(title: str, metrics: Metrics, markdown: bool) -> None:
    rows = metrics.rows()
    print()
    print(title)
    if markdown:
        print("| metric | median (ms) | p90 (ms) | runs |")
        print("|---|---|---|---|")
        for name, median, p90_value, count in rows:
            print(f"| {name} | {median:.2f} | {p90_value:.2f} | {count} |")
        return
    width = max(len(name) for name, *_ in rows)
    header = f"  {'metric'.ljust(width)}  {'median':>10}  {'p90':>10}  {'runs':>5}"
    print(header)
    print(f"  {'-' * width}  {'-' * 10}  {'-' * 10}  {'-' * 5}")
    for name, median, p90_value, count in rows:
        print(f"  {name.ljust(width)}  {median:>10.2f}  {p90_value:>10.2f}  {count:>5}")


def main(argv: list[str]) -> int:
    opts = parse_args(argv)
    if opts.write_budgets is not None and opts.check:
        raise SystemExit("--check and --write-budgets are different modes")
    runs = opts.runs if opts.runs is not None else (5 if opts.check else 20)
    if runs < 1:
        raise SystemExit("--runs must be at least 1")
    binary = opts.binary or default_binary()
    if not binary.is_file():
        raise SystemExit(
            f"{binary} does not exist; build it with "
            "`cargo build --release -p motioner_app` first"
        )
    budgets = load_budgets(opts.budgets) if opts.check else None

    scenarios = sorted(SCENARIOS) if opts.scenario == "both" else [opts.scenario]
    if opts.write_budgets is not None and opts.scenario != "both":
        raise SystemExit("--write-budgets needs --scenario both (the default)")
    report: dict[str, dict] = {}
    failures: list[str] = []
    written_budgets: dict[str, dict] = {}
    for scenario in scenarios:
        metrics, warnings, violations = measure(
            scenario, binary, SCENARIOS[scenario], opts, runs
        )
        report[scenario] = metrics.to_json()
        mode = f"{runs} runs" if opts.cold else f"{runs} warm runs (1 warm-up)"
        if opts.clear_cache:
            mode += ", caches cleared before every run"
        print_table(f"scenario: {scenario} ({mode})", metrics, opts.markdown)
        for warning in warnings:
            print(f"  warning: {warning}")
        failures.extend(violations)
        for violation in violations:
            print(f"  FAIL: {violation}")
        if budgets is not None:
            budget_failures = evaluate_budgets(scenario, metrics, budgets)
            failures.extend(budget_failures)
            for failure in budget_failures:
                print(f"  FAIL: {failure}")
        if opts.write_budgets is not None:
            written_budgets[scenario] = {
                "metrics": budget_entries(metrics, scenario, 2.0)
            }

    print()
    print(f"binary: {binary}")
    if opts.write_budgets is not None:
        payload = {
            "version": 1,
            "notes": (
                "Budgets are 2x the S6 measured medians/p90 (the minimum is "
                f"{BUDGET_FLOOR_MS:g} ms) except close-to-hidden, which is the "
                f"{CLOSE_BUDGET_MS:g} ms user-visible target. The path metric "
                "excludes the GPU driver floor (first frame minus surface "
                "configured)."
            ),
            "scenarios": written_budgets,
        }
        opts.write_budgets.parent.mkdir(parents=True, exist_ok=True)
        opts.write_budgets.write_text(
            json.dumps(payload, indent=2) + "\n", encoding="utf-8"
        )
        print(f"wrote {opts.write_budgets}")
    if opts.json_out:
        opts.json_out.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "engine_trace_version": 1,
            "runs": runs,
            "cold": bool(opts.cold),
            "scenarios": report,
        }
        opts.json_out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {opts.json_out}")
    if opts.check:
        if failures:
            print()
            print(f"check: {len(failures)} failure(s)")
            for failure in failures:
                print(f"  - {failure}")
            return 1
        print()
        print("check: ok (window order and budgets)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
