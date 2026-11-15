#!/usr/bin/env python3
"""Measure Motioner's startup with the engine's startup trace.

Runs the release binary N times for two scenarios and prints median/p90 per
startup mark:

    python scripts/startup_bench.py --runs 20

    python scripts/startup_bench.py --scenario preview --markdown

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

This script is a measurement tool, not a gate: it never fails on a slow run
unless a run cannot produce a trace at all.

Only the Python standard library is used.
"""

from __future__ import annotations

import argparse
import json
import math
import os
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
CLOSE_REQUESTED = "engine: close requested"
WINDOW_HIDDEN = "engine: window hidden"
TEARDOWN_DONE = "engine: teardown done"

TRACE_PREFIX = "startup-trace "
CLOSE_PREFIX = "startup-close "


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Measure Motioner startup from the engine's startup trace.",
    )
    parser.add_argument(
        "--runs",
        type=int,
        default=20,
        help="measured runs per scenario (default: 20)",
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
        "--markdown",
        action="store_true",
        help="print Markdown tables instead of aligned text",
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


def run_once(binary: Path, args: list[str], timeout: float) -> tuple[dict, float, str]:
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


def measure(scenario: str, binary: Path, args: list[str], opts) -> tuple[Metrics, list[str]]:
    metrics = Metrics()
    warnings: list[str] = []
    runs = opts.runs

    if not opts.cold:
        run_once(binary, args, opts.timeout)

    succeeded = 0
    for index in range(runs):
        try:
            completed, wall_ms, output = run_once(binary, args, opts.timeout)
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
        metrics.add("process wall time", wall_ms)
        for name, at_ms in first.items():
            metrics.add(f"mark: {name}", at_ms)
        for name, duration in span_durations(output).items():
            metrics.add(f"span: {name}", duration)
        if CLOSE_REQUESTED in close and WINDOW_HIDDEN in close:
            metrics.add(
                "close: requested to hidden",
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
    return metrics, warnings


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
    if opts.runs < 1:
        raise SystemExit("--runs must be at least 1")
    binary = opts.binary or default_binary()
    if not binary.is_file():
        raise SystemExit(
            f"{binary} does not exist; build it with "
            "`cargo build --release -p motioner_app` first"
        )

    scenarios = sorted(SCENARIOS) if opts.scenario == "both" else [opts.scenario]
    report: dict[str, dict] = {}
    for scenario in scenarios:
        metrics, warnings = measure(scenario, binary, SCENARIOS[scenario], opts)
        report[scenario] = metrics.to_json()
        mode = f"{opts.runs} runs" if opts.cold else f"{opts.runs} warm runs (1 warm-up)"
        print_table(f"scenario: {scenario} ({mode})", metrics, opts.markdown)
        for warning in warnings:
            print(f"  warning: {warning}")

    print()
    print(f"binary: {binary}")
    if opts.json_out:
        opts.json_out.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "engine_trace_version": 1,
            "runs": opts.runs,
            "cold": bool(opts.cold),
            "scenarios": report,
        }
        opts.json_out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {opts.json_out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
