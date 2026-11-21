#!/usr/bin/env python3
"""Reject a UTF-8 BOM or CR in the repository's tracked text files.

PowerShell's ``Set-Content``/``Out-File`` add a UTF-8 BOM and CRLF line
endings, and every tool that reads MDF, Cargo manifests or the project
control files then sees stray bytes. This check reads the **index** blobs,
which are what the next commit contains, and the working tree of files that
differ from the index, so a new or edited file cannot slip a BOM in.

Binary files (``git`` reports ``-text``) are skipped, and ``engine/`` is
excluded: the pinned engine submodule has its own conventions and gate.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

BOM = b"\xef\xbb\xbf"
EXCLUDED_PREFIX = "engine/"


def is_checked(path: str) -> bool:
    """Whether ``path`` is a text file this repository owns."""
    return not path.startswith(EXCLUDED_PREFIX)


def looks_binary(data: bytes) -> bool:
    """A NUL byte in the first 8 KiB means git treats the file as binary."""
    return b"\x00" in data[:8192]


def problems_for(data: bytes) -> list[str]:
    """The encoding findings of one file's bytes."""
    problems: list[str] = []
    if data.startswith(BOM):
        problems.append("UTF-8 BOM")
    if b"\r" in data:
        problems.append("CR (CRLF line endings)")
    return problems


def run_git(root: Path, *args: str) -> str:
    """Runs git in ``root`` and returns its UTF-8 trimmed output."""
    return subprocess.check_output(
        ["git", *args], cwd=root, text=True, encoding="utf-8"
    ).strip()


def index_text_paths(root: Path) -> list[str]:
    """Tracked text paths outside ``engine/``, from ``git ls-files --eol``."""
    paths: list[str] = []
    for line in run_git(root, "ls-files", "--eol").splitlines():
        if "\t" not in line:
            continue
        head, path = line.split("\t", 1)
        if not is_checked(path):
            continue
        if "i/-text" in head.split():
            continue
        paths.append(path)
    return paths


def index_problems(root: Path, paths: list[str]) -> dict[str, list[str]]:
    """Reads the index blobs and reports BOM/CR findings per path."""
    findings: dict[str, list[str]] = {}
    if not paths:
        return findings
    request = "".join(f":{path}\n" for path in paths).encode("utf-8")
    output = subprocess.run(
        ["git", "cat-file", "--batch"],
        cwd=root,
        input=request,
        stdout=subprocess.PIPE,
        check=True,
    ).stdout
    position = 0
    for path in paths:
        end = output.index(b"\n", position)
        header = output[position:end].decode("utf-8", "replace")
        position = end + 1
        parts = header.split()
        if len(parts) != 3 or parts[1] != "blob":
            findings[path] = ["could not read the index blob"]
            continue
        size = int(parts[2])
        data = output[position : position + size]
        position += size + 1  # the newline after every blob
        problems = problems_for(data)
        if problems:
            findings[path] = problems
    return findings


def worktree_changes(root: Path) -> list[str]:
    """Paths that differ from the index or are untracked, outside engine/."""
    output = run_git(root, "diff", "--name-only", "--diff-filter=ACMRT", "-z")
    untracked = run_git(root, "ls-files", "--others", "--exclude-standard", "-z")
    names = [name for name in (output + untracked).split("\0") if name]
    return [name for name in names if is_checked(name)]


def worktree_problems(root: Path, paths: list[str]) -> dict[str, list[str]]:
    """Checks new or edited files for a BOM.

    CR is not reported here: ``.gitattributes`` sets ``eol=lf`` and Git
    normalises CRLF to LF on commit, so a plain text-mode write (the project
    control tooling does this) is not a defect. The index check above is what
    catches a committed CR. A BOM is never normalised, so it is reported.
    """
    findings: dict[str, list[str]] = {}
    for path in paths:
        try:
            data = (root / path).read_bytes()
        except OSError:
            continue
        if looks_binary(data):
            continue
        problems = [problem for problem in problems_for(data) if problem == "UTF-8 BOM"]
        if problems:
            findings[path] = problems
    return findings


def collect_problems(root: Path) -> dict[str, list[str]]:
    """Every encoding finding in the repository, index and changed files."""
    findings = index_problems(root, index_text_paths(root))
    for path, problems in worktree_problems(root, worktree_changes(root)).items():
        findings.setdefault(path, []).extend(
            problem for problem in problems if problem not in findings[path]
        )
    return findings


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    findings = collect_problems(root)
    if findings:
        print("Encoding violations (write files as UTF-8 without a BOM and LF only):")
        for path in sorted(findings):
            print(f"  - {path}: {', '.join(findings[path])}")
        return 1
    print("Encoding OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
