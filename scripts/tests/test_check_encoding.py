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
            (root / "blob.bin").write_bytes(b"\x00\r\n")
            findings = check_encoding.worktree_problems(
                root, ["clean.md", "crlf.md", "bom.md", "blob.bin"]
            )
        # CRLF becomes LF when Git commits it; a BOM does not.
        self.assertEqual(findings, {"bom.md": ["UTF-8 BOM"]})


if __name__ == "__main__":
    unittest.main()
