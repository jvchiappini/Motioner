#!/usr/bin/env python3
"""Pair a child process's memory counters with its ``STEP`` markers.

Runs a command, reads its standard output line by line, and for every line that
starts with ``STEP `` prints the child's private bytes, working set and commit
at that instant.  The child is expected to pause briefly after each marker
(``ferrous_renderer/examples/renderer_memory_steps.rs``), so the numbers belong
to that phase.

Read-only instrument for prompt 18 P3; not in the gate.

Usage::

    python scripts/mem_steps.py -- <exe> [args...]
"""

from __future__ import annotations

import json
import subprocess
import sys

import psutil


def main(argv: list[str]) -> int:
    if argv and argv[0] == "--":
        argv = argv[1:]
    if not argv:
        print("usage: mem_steps.py -- <exe> [args...]", file=sys.stderr)
        return 2
    process = subprocess.Popen(
        argv,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    handle = psutil.Process(process.pid)
    rows: list[dict] = []
    try:
        for line in process.stdout:
            text = line.rstrip("\n")
            if not text.startswith("STEP "):
                print(text)
                continue
            try:
                info = handle.memory_info()
                row = {
                    "step": text[len("STEP ") :],
                    "private_mb": round(info.private / (1024 * 1024), 1),
                    "working_set_mb": round(info.rss / (1024 * 1024), 1),
                }
            except psutil.Error as error:
                row = {"step": text[len("STEP ") :], "error": str(error)}
            rows.append(row)
            print(json.dumps(row))
    finally:
        process.wait(timeout=60)
    print(json.dumps({"steps": rows}, indent=2))
    return 0 if process.returncode == 0 else process.returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
