"""Formula text, refused inputs, and Ruby syntax."""

from __future__ import annotations

import os
import shutil
import subprocess
import unittest
from pathlib import Path

import package

from .support import scratch


_GOLDEN = Path(__file__).resolve().parent / "data" / "talaria.rb.golden"
_TARGETS = (
    ("aarch64-apple-darwin", "1"),
    ("x86_64-apple-darwin", "2"),
    ("x86_64-unknown-linux-musl", "3"),
    ("aarch64-unknown-linux-musl", "4"),
)


def _sums(version, targets=_TARGETS):
    lines = []
    for target, digit in targets:
        name = f"talaria-{version}-{target}.tar.gz"
        lines.append(f"{digit * 64}  {name}")
    return "\n".join(lines) + "\n"


class FormulaTests(unittest.TestCase):
    def test_formula_matches_golden(self):
        with scratch():
            Path("SHA256SUMS").write_text(_sums("0.7.1"), encoding="utf-8")
            result = package.formula("0.7.1", "SHA256SUMS")
            golden = _GOLDEN.read_text(encoding="utf-8")
            self.assertEqual(result.stdout, golden, "no formula command")

    def test_formula_refuses_bad_inputs(self):
        with scratch():
            kept = [item for item in _TARGETS if item[0] != "aarch64-unknown-linux-musl"]
            Path("three").write_text(_sums("0.7.1", kept), encoding="utf-8")
            missing = package.formula("0.7.1", "three")
            self.assertEqual(missing.code, 1, "no validation")
            self.assertIn(
                "SHA256SUMS has no archive for aarch64-unknown-linux-musl",
                missing.stderr,
                "no validation",
            )

            Path("pre").write_text(_sums("0.7.1-rc.1"), encoding="utf-8")
            mismatch = package.formula("0.7.1", "pre")
            self.assertEqual(mismatch.code, 2, "no validation")
            self.assertEqual(
                mismatch.stderr.strip(),
                "package.py: SHA256SUMS names version 0.7.1-rc.1, not 0.7.1",
                "no validation",
            )

            Path("good").write_text(_sums("0.7.1"), encoding="utf-8")
            bad_versions = (
                '0.7.1"',
                "0.7.1#{x}",
                "0.7.1 ",
                "0.7.1\\x",
                "0.7.1\n",
            )
            for version in bad_versions:
                result = package.formula(version, "good")
                self.assertEqual(result.code, 2, "no validation")
                self.assertIn("package.py: invalid version", result.stderr, "no validation")
            newline = package.formula("0.7.1\n", "good")
            self.assertIn(
                "package.py: invalid version '0.7.1?'",
                newline.stderr,
                "no validation",
            )

            bad_urls = (
                'https://example.com/a"b',
                "https://example.com/a#{b",
                "https://example.com/a b",
                "https://example.com/a\\b",
                "https://example.com/a\n",
            )
            for url in bad_urls:
                result = package.formula("0.7.1", "good", url)
                self.assertEqual(result.code, 2, "no validation")
                self.assertIn("package.py: unsafe base URL", result.stderr, "no validation")

    def test_formula_is_valid_ruby(self):
        if shutil.which("ruby") is None:
            if os.environ.get("PACKAGE_REQUIRE_RUBY") == "1":
                self.fail("ruby not found")
            self.skipTest("ruby not found")
        with scratch():
            Path("SHA256SUMS").write_text(_sums("0.7.1"), encoding="utf-8")
            result = package.formula("0.7.1", "SHA256SUMS")
            self.assertIn("class Talaria", result.stdout, "no formula")
            Path("formula.rb").write_text(result.stdout, encoding="utf-8")
            checked = subprocess.run(
                ["ruby", "-c", "formula.rb"],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertIn("Syntax OK", checked.stdout, "no formula")
