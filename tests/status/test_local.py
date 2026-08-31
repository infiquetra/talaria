"""Framework-free capture of restart-scoped local status facts."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


def test_capture_reads_git_without_importing_the_terminal_framework(tmp_path: Path) -> None:
    repository = tmp_path / "repository"
    subprocess.run(
        ["git", "init", "--quiet", "--initial-branch", "repair/local-status", repository],
        check=True,
    )
    probe = """
import sys
from pathlib import Path

from talaria.status.local import capture_local_status

captured = capture_local_status(Path(sys.argv[1]))
assert captured.cwd == str(Path(sys.argv[1]).resolve())
assert captured.git_branch == "repair/local-status"
assert "textual" not in sys.modules
assert not any(name == "talaria.ui" or name.startswith("talaria.ui.") for name in sys.modules)
print(f"{captured.cwd}|{captured.git_branch}")
"""

    completed = subprocess.run(
        [sys.executable, "-c", probe, str(repository)],
        check=False,
        capture_output=True,
        text=True,
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout.rstrip().endswith("repository|repair/local-status")
