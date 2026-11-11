#!/usr/bin/env python3
"""Check Motioner's workspace dependency boundaries.

The document core must stay free of graphics, UI and AI. The DSL must not
depend on evaluation, rendering, I/O, UI or application code. The
evaluator must not depend on rendering or I/O, and the renderer must not
depend on project I/O. These rules are architecture, not style: violating
them couples the editor to the renderer and breaks deterministic
evaluation.

New workspace crates must be added to FORBIDDEN_EDGES. External graphics
or UI dependencies are detected by prefix. Only the renderer and the editor
UI (`motioner_ui`) may use the engine's external graphics prefixes (wgpu,
ferrous), declared as data in ALLOWED_PREFIXES; `motioner_ui` may depend on
the document, DSL, evaluator and renderer crates but not on I/O, AI, the
application or the test kit. `motioner_app` may depend on `motioner_ui`.
`motioner_analysis` is pure over the document and evaluated frames: no DSL,
renderer, I/O, UI, tools or application. `motioner_tools` orchestrates the
document, DSL, evaluator, renderer, I/O and analysis, but never the UI,
application or test kit.
egui, eframe and guimaker stay forbidden for every crate.
"""

from __future__ import annotations

import json
import subprocess
import sys

FORBIDDEN_PREFIXES = ("wgpu", "ferrous_", "ferrous", "egui", "guimaker", "eframe")

ALLOWED_PREFIXES = {
    "motioner_render": ("wgpu", "ferrous"),
    "motioner_ui": ("wgpu", "ferrous"),
}

FORBIDDEN_EDGES = {
    "motioner_core": ("motioner_",),
    "motioner_dsl": (
        "motioner_eval",
        "motioner_render",
        "motioner_io",
        "motioner_ui",
        "motioner_ai",
        "motioner_app",
        "motioner_testkit",
    ),
    "motioner_eval": (
        "motioner_render",
        "motioner_io",
        "motioner_ui",
        "motioner_ai",
        "motioner_app",
        "motioner_testkit",
    ),
    "motioner_render": (
        "motioner_ui",
        "motioner_ai",
        "motioner_app",
        "motioner_testkit",
    ),
    "motioner_io": (
        "motioner_render",
        "motioner_ui",
        "motioner_ai",
        "motioner_app",
        "motioner_testkit",
    ),
    "motioner_ui": (
        "motioner_io",
        "motioner_ai",
        "motioner_app",
        "motioner_testkit",
    ),
    "motioner_analysis": (
        "motioner_render",
        "motioner_io",
        "motioner_ui",
        "motioner_ai",
        "motioner_app",
        "motioner_tools",
        "motioner_testkit",
    ),
    # The tool runtime orchestrates render and I/O but never the window, the
    # application wiring or the test kit.
    "motioner_tools": (
        "motioner_ui",
        "motioner_ai",
        "motioner_app",
        "motioner_testkit",
    ),
    "motioner_app": (),
    "motioner_testkit": (),
}


def _dependency_names(package: dict) -> set[str]:
    return {
        dependency["name"]
        for dependency in package["dependencies"]
        if dependency.get("kind") in (None, "normal")
    }


def _external_violation(name: str, dependency: str) -> bool:
    if not dependency.startswith(FORBIDDEN_PREFIXES):
        return False
    allowed = ALLOWED_PREFIXES.get(name, ())
    return not dependency.startswith(allowed)


def _violations(name: str, dependencies: set[str]) -> list[str]:
    forbidden = FORBIDDEN_EDGES.get(name, ())
    problems = []
    for dependency in sorted(dependencies):
        if dependency in forbidden or _external_violation(name, dependency):
            problems.append(f"{name} must not depend on {dependency}")
    return problems


def main() -> int:
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            text=True,
        )
    )
    workspace = [p for p in metadata["packages"] if p["name"].startswith("motioner_")]

    unknown = sorted({p["name"] for p in workspace} - set(FORBIDDEN_EDGES))
    problems = [f"{name} has no boundary rule; add it to FORBIDDEN_EDGES" for name in unknown]

    for package in workspace:
        name = package["name"]
        if name in FORBIDDEN_EDGES:
            problems.extend(_violations(name, _dependency_names(package)))

    if problems:
        print("Dependency boundary violations:")
        for problem in problems:
            print(f"  - {problem}")
        return 1

    print("Dependency boundaries OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
