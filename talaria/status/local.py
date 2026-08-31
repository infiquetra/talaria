"""Restart-scoped local status facts with no presentation-layer dependency."""

from __future__ import annotations

import shutil
import subprocess  # nosec B404
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class LocalStatus:
    """Working-directory facts captured once when the app is constructed."""

    cwd: str
    git_branch: str


def capture_local_status(cwd: Path | None = None) -> LocalStatus:
    """Capture cwd and Git branch from local process state, with no polling.

    The Git command uses a fixed argument vector and is run once by the app.
    Outside a repository, or when Git is unavailable, the branch is honestly
    unknown rather than guessed from a directory name.
    """
    launch_cwd = (cwd if cwd is not None else Path.cwd()).resolve()
    git = shutil.which("git")
    if git is None:
        return LocalStatus(str(launch_cwd), "?")
    try:
        # The executable path and argument vector are fixed; no shell is involved.
        result = subprocess.run(  # nosec B603
            [git, "branch", "--show-current"],
            cwd=launch_cwd,
            check=False,
            capture_output=True,
            text=True,
            timeout=2.0,
        )
    except (OSError, subprocess.TimeoutExpired):
        return LocalStatus(str(launch_cwd), "?")
    branch = result.stdout.strip() if result.returncode == 0 else ""
    return LocalStatus(str(launch_cwd), branch or "detached")


__all__ = ["LocalStatus", "capture_local_status"]
