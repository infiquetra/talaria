"""Text shape of the release and package workflows."""

from __future__ import annotations

import re
import unittest

from .support import repo_root


_FORBIDDEN = "un" + "attended"


def _run_lines(text):
    lines = text.splitlines()
    chosen = []
    index = 0
    while index < len(lines):
        line = lines[index]
        if re.search(r"run:\s*[|>]", line):
            indent = len(line) - len(line.lstrip(" "))
            index += 1
            while index < len(lines):
                current = lines[index]
                if current.strip() == "":
                    chosen.append(current)
                    index += 1
                    continue
                width = len(current) - len(current.lstrip(" "))
                if width <= indent:
                    break
                chosen.append(current)
                index += 1
            continue
        if re.search(r"run:\s*\S", line):
            chosen.append(line)
        index += 1
    return chosen


class WorkflowTests(unittest.TestCase):
    def test_release_and_package_workflows_shape(self):
        root = repo_root() / ".github" / "workflows"
        release = (root / "release.yml").read_text(encoding="utf-8")
        build_path = root / "package-build.yml"
        check_path = root / "package-check.yml"
        self.assertTrue(build_path.is_file(), "jobs missing")
        self.assertTrue(check_path.is_file(), "jobs missing")
        build = build_path.read_text(encoding="utf-8")
        check = check_path.read_text(encoding="utf-8")
        for key in ("check:", "build:", "publish:", "tap:"):
            self.assertIn(f"\n  {key}\n", f"\n{release}", "jobs missing")

        self.assertIn("id: resolve", release, "jobs missing")
        self.assertIn(
            "=~ ^v[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.]+)?$",
            release,
            "jobs missing",
        )
        for step in (
            "Check out the tagged tree",
            "Install the pinned toolchain",
            "The tag agrees with the workspace version",
            "Project check",
        ):
            self.assertIn(step, release, "jobs missing")

        self.assertIn("--verify-tag", release, "jobs missing")
        for text in (release, build, check):
            self.assertNotIn("--clobber", text, "jobs missing")
            self.assertNotIn("cargo publish", text, "jobs missing")
            self.assertNotIn(_FORBIDDEN, text, "jobs missing")

        tap_at = release.index("\n  tap:\n")
        token = "secrets.HOMEBREW_TAP_TOKEN"
        self.assertEqual(release.find(token), release.rfind(token), "jobs missing")
        self.assertGreater(release.find(token), tap_at, "jobs missing")
        self.assertIn("if: needs.check.outputs.prerelease == 'false'", release, "jobs missing")
        self.assertIn('-z "$HOMEBREW_TAP_TOKEN"', release[tap_at:], "jobs missing")
        self.assertIn(
            "HOMEBREW_TAP_TOKEN is not set; the Homebrew tap was not updated",
            release[tap_at:],
            "jobs missing",
        )
        self.assertLess(
            release.find("gh pr list", tap_at),
            release.find("git commit", tap_at),
            "jobs missing",
        )

        for text in (release, build, check):
            for line in _run_lines(text):
                self.assertNotIn("${{ inputs.version }}", line, "jobs missing")
            for line in text.splitlines():
                stripped = line.strip()
                if not stripped.startswith("uses:") and " uses:" not in f" {stripped}":
                    if "uses:" not in stripped:
                        continue
                if "uses:" not in stripped:
                    continue
                if "./" in stripped:
                    continue
                self.assertRegex(stripped, r"@[0-9a-f]{40} #", "jobs missing")

        self.assertIn("package.py targets --json", build, "jobs missing")
        self.assertIn("fromJSON(needs.plan.outputs.matrix)", build, "jobs missing")
        check_at = release.index("\n  check:\n")
        build_at = release.index("\n  build:\n")
        self.assertIn("package.py version-gate", release[check_at:build_at], "jobs missing")
        self.assertIn("rustup toolchain install", check, "jobs missing")
        self.assertIn('PACKAGE_REQUIRE_RUBY: "1"', check, "jobs missing")
