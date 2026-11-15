"""Tests for the compatibility table generator and copies."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

from . import support

START_MARKER = "<!-- hermes-compat:start -->"
END_MARKER = "<!-- hermes-compat:end -->"


def _hermes_line_from_toml(repo: Path) -> str:
    data = tomllib.loads(
        (repo / "crates" / "hermes-contract" / "compat.toml").read_text(encoding="utf-8")
    )
    row = data["release"][-1]
    return (
        f"hermes: pinned {row['pinned_tag']} ({row['pinned_commit'][:8]}), "
        f"supported from {row['floor']}, tested to {row['newest_tested']}"
    )


class CompatTests(unittest.TestCase):
    def test_check_passes_on_the_committed_copies(self) -> None:
        result = subprocess.run(
            [sys.executable, "-I", str(support.SCRIPT_PATH), "compat", "--check"],
            cwd=support.REPO,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            result.returncode,
            0,
            f"compat --check failed: stdout={result.stdout!r} stderr={result.stderr!r}",
        )

    def test_check_fails_when_a_copy_is_edited(self) -> None:
        with tempfile.TemporaryDirectory() as raw_dir:
            copy_root = Path(raw_dir) / "repo"
            shutil.copytree(
                support.REPO,
                copy_root,
                ignore=shutil.ignore_patterns(".git", "target"),
            )
            readme_path = copy_root / "README.md"
            lines = readme_path.read_text(encoding="utf-8").splitlines(keepends=True)

            install_idx = next(
                i for i, line in enumerate(lines) if line.rstrip("\r\n") == "## Install"
            )
            has_block = (
                install_idx >= 5
                and lines[install_idx - 1].rstrip("\r\n") == ""
                and lines[install_idx - 2].rstrip("\r\n") == END_MARKER
                and lines[install_idx - 5].rstrip("\r\n") == START_MARKER
            )
            if not has_block:
                hermes_line = _hermes_line_from_toml(copy_root)
                block = [
                    f"{START_MARKER}\n",
                    f"{hermes_line}\n",
                    "[docs/compatibility.md](docs/compatibility.md)\n",
                    f"{END_MARKER}\n",
                    "\n",
                ]
                lines[install_idx:install_idx] = block
                hermes_idx = install_idx + 1
            else:
                hermes_idx = install_idx - 4

            original = lines[hermes_idx].rstrip("\r\n")
            lines[hermes_idx] = f"{original}x\n"
            readme_path.write_text("".join(lines), encoding="utf-8")

            script = copy_root / "tools" / "contract-gen" / "contract_gen.py"
            result = subprocess.run(
                [sys.executable, "-I", str(script), "compat", "--check"],
                cwd=copy_root,
                capture_output=True,
                text=True,
                check=False,
            )
            combined = result.stdout + result.stderr
            self.assertEqual(result.returncode, 1, combined)
            self.assertIn("README.md", combined)


if __name__ == "__main__":
    unittest.main()
