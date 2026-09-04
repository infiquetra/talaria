"""Version resolution for acceptance evidence trees.

The release workflow derives everything per-version from the tag; this module
is the same mapping in Python so scripts and tests agree with the workflow
rather than each re-deriving it. ``0.6.0`` and ``0.6.0-rc1`` both resolve to
version ``0.6.0`` — the same strip-suffix idiom as the tag check in
``.github/workflows/release.yml``.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parents[2]


@dataclass(frozen=True)
class AcceptanceVersion:
    """Where one release version's acceptance evidence lives."""

    version: str
    directory: Path
    manifest_path: Path
    evidence_root: Path


def version_from_tag(tag: str) -> str:
    """Strip a leading ``v`` and any pre-release suffix from a release tag."""
    version = tag[1:] if tag.startswith("v") else tag
    return version.split("-", 1)[0]


def acceptance_paths(version: str, *, repo_root: Path = _REPO_ROOT) -> AcceptanceVersion:
    """Return the evidence-tree locations for a dotted release version."""
    directory = repo_root / "docs" / "acceptance" / f"v{version}"
    return AcceptanceVersion(
        version=version,
        directory=directory,
        manifest_path=directory / "artifact-manifest.json",
        evidence_root=directory / "evidence",
    )
