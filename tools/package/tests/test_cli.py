"""Command errors and a build that must stop before a compiler runs."""

from __future__ import annotations

import unittest
from pathlib import Path

import package

from .support import prepend_path, scratch, write_script


class CliTests(unittest.TestCase):
    def test_usage_errors_and_exit_codes(self):
        code, _err = _run(["--help"])
        self.assertEqual(code, 0, "no parser")
        for args in ([], ["no-such"], ["build"], ["archive"]):
            captured = _run(args)
            self.assertEqual(captured[0], 2, "no parser")
            self.assertTrue(captured[1].startswith("package.py:"), "no parser")
            self.assertEqual(captured[1].splitlines(), [captured[1].strip()], "no parser")

    def test_build_failure_paths(self):
        with scratch():
            bindir = Path("bin")
            bindir.mkdir()
            write_script(
                bindir / "rustup",
                "printf '%s\\n' \"$*\" >> rustup.log\nexit 0\n",
            )
            write_script(bindir / "cargo", "exit 3\n")
            with prepend_path(bindir):
                stopped = package.build("x86_64-unknown-linux-musl")
                self.assertEqual(stopped.code, 2, "no build command")
                self.assertIn(
                    "musl-gcc not found; install musl-tools",
                    stopped.stderr,
                    "no build command",
                )
                self.assertFalse(Path("rustup.log").exists(), "no build command")
                write_script(bindir / "musl-gcc", "exit 0\n")
                failed = package.build("x86_64-unknown-linux-musl")
                self.assertEqual(failed.code, 1, "no build command")
                self.assertIn(
                    "cargo build failed for x86_64-unknown-linux-musl (exit 3)",
                    failed.stderr,
                    "no build command",
                )


def _run(args):
    import io
    from contextlib import redirect_stderr

    buffer = io.StringIO()
    with redirect_stderr(buffer):
        code = package.main(args)
    return code, buffer.getvalue()
