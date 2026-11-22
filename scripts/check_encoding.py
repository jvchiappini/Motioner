#!/usr/bin/env python3
"""Reject a UTF-8 BOM, a CR, or double-encoded UTF-8 in tracked text files.

PowerShell's ``Set-Content``/``Out-File`` add a UTF-8 BOM and CRLF line
endings, and every tool that reads MDF, Cargo manifests or the project
control files then sees stray bytes. This check reads the **index** blobs,
which are what the next commit contains, and the working tree of files that
differ from the index, so a new or edited file cannot slip a BOM in.

A file that was decoded as cp1252 and saved again as UTF-8 doubles every
non-ASCII character: an accented letter, an em dash or a multiplication sign
becomes two or three visible characters. Those sequences are never valid
English text, so they are rejected too: an A-tilde or an A-circumflex
followed by a non-letter, and the a-circumflex + euro pair anywhere. The
prompt-15 review section quotes the sequences on purpose and is allowlisted
by path and heading.

Binary files (``git`` reports ``-text``) are skipped, and ``engine/`` is
excluded: the pinned engine submodule has its own conventions and gate.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

BOM = b"\xef\xbb\xbf"
EXCLUDED_PREFIX = "engine/"
# The sequences are written as escapes so this file does not trip its own
# guard: A-tilde and A-circumflex followed by a non-letter, and a-circumflex +
# euro anywhere (the second byte of a UTF-8 sequence decoded as cp1252).
MOJIBAKE_PREFIXES = ("\u00c3", "\u00c2")
MOJIBAKE_EURO = "\u00e2\u20ac"
# Sections that quote the broken sequences on purpose. The value is the
# heading that opens the allowlisted section; the section ends at the next
# level-two heading.
MOJIBAKE_ALLOWLIST = {
    ".control/prompts/opencode/opencode-prompt-15-guimaker-design-system.md": (
        "## Review findings after the Phase 4B fix",
    ),
}


def is_checked(path: str) -> bool:
    """Whether ``path`` is a text file this repository owns."""
    return not path.startswith(EXCLUDED_PREFIX)


def looks_binary(data: bytes) -> bool:
    """A NUL byte in the first 8 KiB means git treats the file as binary."""
    return b"\x00" in data[:8192]


def allowlisted_lines(path: str, text: str) -> set[int]:
    """Zero-based line numbers whose mojibake is quoted on purpose."""
    markers = MOJIBAKE_ALLOWLIST.get(path)
    if not markers:
        return set()
    allowed: set[int] = set()
    inside = False
    for number, line in enumerate(text.splitlines()):
        if line.startswith("## "):
            inside = any(marker in line for marker in markers)
        if inside:
            allowed.add(number)
    return allowed


def mojibake_lines(text: str) -> set[int]:
    """Zero-based line numbers with a double-encoded UTF-8 marker."""
    flagged: set[int] = set()
    number = 0
    for index, char in enumerate(text):
        if char == "\n":
            number += 1
        elif char in MOJIBAKE_PREFIXES:
            following = text[index + 1] if index + 1 < len(text) else ""
            if not following.isalpha():
                flagged.add(number)
        if text.startswith(MOJIBAKE_EURO, index):
            flagged.add(number)
    return flagged


def problems_for(data: bytes, path: str = "") -> list[str]:
    """The encoding findings of one file's bytes."""
    problems: list[str] = []
    if data.startswith(BOM):
        problems.append("UTF-8 BOM")
    if b"\r" in data:
        problems.append("CR (CRLF line endings)")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        problems.append("not valid UTF-8")
        return problems
    flagged = sorted(mojibake_lines(text) - allowlisted_lines(path, text))
    if flagged:
        shown = ", ".join(str(number + 1) for number in flagged[:5])
        more = "" if len(flagged) <= 5 else f" (+{len(flagged) - 5} more)"
        problems.append(f"mojibake (double-encoded UTF-8) at line {shown}{more}")
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
    """Reads the index blobs and reports BOM/CR/mojibake findings per path."""
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
        problems = problems_for(data, path)
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
    """Checks new or edited files for a BOM and mojibake.

    CR is not reported here: ``.gitattributes`` sets ``eol=lf`` and Git
    normalises CRLF to LF on commit, so a plain text-mode write (the project
    control tooling does this) is not a defect. The index check above is what
    catches a committed CR. A BOM is never normalised, and a cp1252 round trip
    is damage, so both are reported.
    """
    findings: dict[str, list[str]] = {}
    for path in paths:
        try:
            data = (root / path).read_bytes()
        except OSError:
            continue
        if looks_binary(data):
            continue
        problems = [
            problem
            for problem in problems_for(data, path)
            if problem != "CR (CRLF line endings)"
        ]
        if problems:
            findings[path] = problems
    return findings


def collect_problems(root: Path) -> dict[str, list[str]]:
    """Every encoding finding in the repository, index and changed files.

    A path that differs from the index is checked in the working tree, which
    is what the next commit would contain; every other path is checked in the
    index blob.
    """
    changed = set(worktree_changes(root))
    findings = index_problems(
        root, [path for path in index_text_paths(root) if path not in changed]
    )
    for path, problems in worktree_problems(root, sorted(changed)).items():
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
