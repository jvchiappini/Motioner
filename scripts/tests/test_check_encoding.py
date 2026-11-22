"""Tests for ``scripts/check_encoding.py``.

Run with ``python -m unittest discover -s scripts/tests``. The gate runs the
check itself; these tests pin the classification that makes it trustworthy.
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import check_encoding

PROMPT = ".control/prompts/opencode/opencode-prompt-15-guimaker-design-system.md"


class ClassificationTest(unittest.TestCase):
    def test_clean_utf8_is_accepted(self) -> None:
        self.assertEqual(check_encoding.problems_for("caf\u00e9\n".encode()), [])

    def test_a_bom_is_rejected(self) -> None:
        self.assertEqual(
            check_encoding.problems_for(b"\xef\xbb\xbfhello\n"), ["UTF-8 BOM"]
        )

    def test_crlf_is_rejected(self) -> None:
        self.assertEqual(
            check_encoding.problems_for(b"hello\r\n"),
            ["CR (CRLF line endings)"],
        )

    def test_both_findings_are_reported(self) -> None:
        self.assertEqual(
            check_encoding.problems_for(b"\xef\xbb\xbfhello\r\n"),
            ["UTF-8 BOM", "CR (CRLF line endings)"],
        )

    def test_mojibake_is_rejected(self) -> None:
        cases = {
            "A-tilde": "\u00c3\u00a9\n",
            "a-circumflex + euro": "\u00e2\u20ac\u201d\n",
            "A-circumflex": "\u00c2\u00b1\n",
        }
        for name, text in cases.items():
            with self.subTest(name):
                problems = check_encoding.problems_for(text.encode(), "notes.md")
                self.assertTrue(
                    any(problem.startswith("mojibake") for problem in problems),
                    problems,
                )

    def test_a_tilde_before_a_letter_is_not_mojibake(self) -> None:
        self.assertEqual(
            check_encoding.problems_for("\u00c3a\n".encode(), "notes.md"), []
        )

    def test_the_prompt_review_section_is_allowlisted(self) -> None:
        text = (
            "## Review findings after the Phase 4B fix\n\n"
            "the sequences `\u00c3`, `\u00c2` and `\u00e2\u20ac` on purpose\n\n"
            "## A later section\n\n"
            "\u00c3\u00a9\n"
        )
        problems = check_encoding.problems_for(text.encode(), PROMPT)
        self.assertTrue(any("line 7" in problem for problem in problems), problems)

    def test_engine_and_binaries_are_skipped(self) -> None:
        self.assertFalse(check_encoding.is_checked("engine/ferrous/README.md"))
        self.assertTrue(check_encoding.is_checked("crates/motioner_dsl/src/lib.rs"))
        self.assertTrue(check_encoding.looks_binary(b"\x00\x01\r\n"))

    def test_worktree_findings_name_the_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "clean.md").write_bytes(b"text\n")
            (root / "crlf.md").write_bytes(b"text\r\n")
            (root / "bom.md").write_bytes(b"\xef\xbb\xbftext\n")
            (root / "mojibake.md").write_bytes("\u00c3\u00a9\n".encode())
            (root / "blob.bin").write_bytes(b"\x00\r\n")
            findings = check_encoding.worktree_problems(
                root, ["clean.md", "crlf.md", "bom.md", "mojibake.md", "blob.bin"]
            )
        # CRLF becomes LF when Git commits it; a BOM and mojibake do not.
        self.assertEqual(
            findings,
            {
                "bom.md": ["UTF-8 BOM"],
                "mojibake.md": ["mojibake (double-encoded UTF-8) at line 1"],
            },
        )


if __name__ == "__main__":
    unittest.main()
