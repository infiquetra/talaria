"""Tag shape and the release version gate."""

from __future__ import annotations

import re
import unittest

import package

from .support import repo_root


_PATTERN = "v[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.]+)?"


class VersionTests(unittest.TestCase):
    def test_tag_version_rules(self):
        release = (repo_root() / ".github" / "workflows" / "release.yml").read_text(
            encoding="utf-8"
        )
        found = re.search(r"=~ \^(.+)\$", release)
        self.assertIsNotNone(found, "no version functions")
        self.assertEqual(found.group(1), _PATTERN, "no version functions")
        self.assertEqual(package.TAG_PATTERN, found.group(1), "no version functions")

        self.assertEqual(package.parse_release_tag("v0.7.1"), "0.7.1", "no version functions")
        self.assertEqual(
            package.parse_release_tag("v0.7.1-rc.1"),
            "0.7.1-rc.1",
            "no version functions",
        )
        for refused in ("0.7.1", "v0.7", "v1.2.3; x", "v0.7.1\n"):
            self.assertIsNone(package.parse_release_tag(refused), "no version functions")
            gate = package.version_gate(refused, "0.7.1")
            self.assertEqual(gate.code, 2, "no version functions")
            self.assertTrue(gate.stderr.startswith("package.py:"), "no version functions")

        newline_workspace = package.version_gate("v0.7.1", "0.7.1\n")
        self.assertEqual(newline_workspace.code, 2, "no version functions")
        self.assertTrue(
            newline_workspace.stderr.startswith("package.py:"),
            "no version functions",
        )

        exact = package.version_gate("v0.7.1", "0.7.1")
        self.assertEqual(exact.code, 0, "no version functions")
        self.assertEqual(exact.stdout, "0.7.1\n", "no version functions")

        final_mismatch = package.version_gate("v0.7.1", "0.7.1-dev")
        self.assertEqual(final_mismatch.code, 1, "no version functions")
        self.assertEqual(
            final_mismatch.stderr.strip(),
            "package.py: final tag v0.7.1 needs workspace version 0.7.1, found 0.7.1-dev",
            "no version functions",
        )

        pre = package.version_gate("v0.7.1-rc.1", "0.7.1-dev")
        self.assertEqual(pre.code, 0, "no version functions")
        self.assertEqual(pre.stdout, "0.7.1-rc.1\n", "no version functions")

        pre_mismatch = package.version_gate("v0.7.1-rc.1", "0.8.0-dev")
        self.assertEqual(pre_mismatch.code, 1, "no version functions")
        self.assertEqual(
            pre_mismatch.stderr.strip(),
            "package.py: tag v0.7.1-rc.1 and workspace version 0.8.0-dev "
            "have different base versions",
            "no version functions",
        )
