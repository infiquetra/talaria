"""Keep v0.5.0 reader-facing release claims bound to current evidence."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any, cast

import pytest

from talaria import __version__
from talaria.themes.builtins import BUILTIN_THEMES

_REPO_ROOT = Path(__file__).resolve().parents[2]
_ACCEPTANCE_ROOT = _REPO_ROOT / "docs" / "acceptance" / "v0.5.0"
_MANIFEST_PATH = _ACCEPTANCE_ROOT / "artifact-manifest.json"
_GENERATED_RESULTS_PATH = (
    _REPO_ROOT
    / "docs"
    / "acceptance"
    / "2026-08-30-talaria-v0-5-0-live-acceptance-results.md"
)
_RELEASE_DOCUMENT_PATHS = (_REPO_ROOT / "README.md", _REPO_ROOT / "CHANGELOG.md")
_DOCUMENTATION_INDEX_PATH = _REPO_ROOT / "docs" / "00-index.md"
_INSTALL_GUIDE_PATH = _REPO_ROOT / "docs" / "install.md"
_V050_RELEASE_NOTES_PATH = _REPO_ROOT / "docs" / "releases" / "v0.5.0.md"


def _manifest() -> dict[str, Any]:
    return cast(
        dict[str, Any], json.loads(_MANIFEST_PATH.read_text(encoding="utf-8"))
    )


def _generated_acceptance_summary(status: str) -> str:
    """Read the reader-facing summary emitted by the acceptance generator."""
    document = _GENERATED_RESULTS_PATH.read_text(encoding="utf-8")
    marker = f"**{status}**:"
    matches = [line.partition(marker)[2] for line in document.splitlines() if marker in line]

    assert len(matches) == 1, _GENERATED_RESULTS_PATH
    return re.sub(r"\s+", " ", f"{marker}{matches[0]}").strip()


def test_release_documents_quote_the_current_manifest_outcome() -> None:
    """Reader-facing release claims cannot lag a regenerated manifest again."""
    manifest = _manifest()
    status = str(manifest["status"]).upper()
    summary = _generated_acceptance_summary(status)

    for path in _RELEASE_DOCUMENT_PATHS:
        document = path.read_text(encoding="utf-8")
        normalized = re.sub(r"\s+", " ", re.sub(r"(?m)^> ?", "", document))
        assert summary in normalized, path
        assert document.count(f"**{status}**") >= 2, path
        assert document.lower().count("item 24") >= 2, path
        assert "gateway always assigns a request identifier" in document, path
        assert "receipts and screenshots do not exist" not in document, path
        assert "no current item receipts" not in document, path
        assert "no approved live primary model receipt" not in document, path


def test_documentation_index_reaches_every_tester_evidence_index() -> None:
    """Adding an acceptance tester must also make its evidence discoverable."""
    index = _DOCUMENTATION_INDEX_PATH.read_text(encoding="utf-8")
    linked_targets = set(re.findall(r"\[[^]]+\]\(([^)]+)\)", index))
    evidence_indexes = sorted((_ACCEPTANCE_ROOT / "evidence").glob("*/README.md"))

    assert evidence_indexes
    for evidence_index in evidence_indexes:
        target = evidence_index.relative_to(_REPO_ROOT / "docs").as_posix()
        assert target in linked_targets, evidence_index


def _assert_evidence_indexes_agree_with_manifest(manifest_path: Path) -> None:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    counts = manifest["counts"]
    expected = (
        f"The generated manifest reports {counts['stale_receipts']} stale receipts, "
        f"{counts['missing_current_receipts']} missing current receipts, and "
        f"{counts['invalid_item_receipts']} invalid item receipts."
    )
    evidence_indexes = sorted((_ACCEPTANCE_ROOT / "evidence").glob("*/README.md"))
    combined = re.sub(
        r"\s+",
        " ",
        "\n".join(path.read_text(encoding="utf-8") for path in evidence_indexes),
    )

    assert expected in combined
    if counts["stale_receipts"] == 0:
        assert "still flags the T2 receipts currently on this branch as stale" not in combined
    if counts["missing_current_receipts"] == 0:
        assert "only after the parallel T2 half is merged" not in combined


def test_evidence_indexes_agree_with_the_manifest() -> None:
    _assert_evidence_indexes_agree_with_manifest(_MANIFEST_PATH)


def test_evidence_index_guard_detects_manifest_count_drift(tmp_path: Path) -> None:
    _assert_evidence_indexes_agree_with_manifest(_MANIFEST_PATH)
    manifest = _manifest()
    manifest["counts"]["stale_receipts"] += 1
    drifted = tmp_path / "artifact-manifest.json"
    drifted.write_text(json.dumps(manifest), encoding="utf-8")

    with pytest.raises(AssertionError):
        _assert_evidence_indexes_agree_with_manifest(drifted)


def test_documentation_index_reaches_the_install_and_release_guides() -> None:
    """The two reader entry points must be present and discoverable."""
    index = _DOCUMENTATION_INDEX_PATH.read_text(encoding="utf-8")
    linked_targets = set(re.findall(r"\[[^]]+\]\(([^)]+)\)", index))

    assert _INSTALL_GUIDE_PATH.is_file()
    assert _V050_RELEASE_NOTES_PATH.is_file()
    assert "install.md" in linked_targets
    assert "releases/v0.5.0.md" in linked_targets


def test_install_guide_states_its_audience_prerequisites_and_supported_platform() -> None:
    """A new user sees the support boundary before the first command."""
    document = _INSTALL_GUIDE_PATH.read_text(encoding="utf-8")
    introduction = document.split("```", maxsplit=1)[0]

    for required in ("This guide is for", "macOS", "Python 3.12 or 3.13", "uv", "Hermes"):
        assert required in introduction

    assert "Linux is not supported" in document
    assert "Windows is not supported" in document
    assert "no person has driven Talaria on Linux" in document


def test_install_guide_pins_the_real_install_verification_and_recovery_paths() -> None:
    """Installation claims stay attached to commands and observed failure remedies."""
    document = _INSTALL_GUIDE_PATH.read_text(encoding="utf-8")

    assert "uv tool install git+https://github.com/infiquetra/talaria@v0.5.0" in document
    assert "uv tool install talaria" in document
    assert "uv sync --all-groups" in document
    assert "uv run talaria --version" in document
    assert f"talaria {__version__}" in document
    assert "uv tool dir --bin" in document
    assert "command -v talaria" in document
    assert "stale" in document.lower()
    assert "exits without output" in document
    assert "~/.talaria/credentials" in document
    assert "Hermes restart" in document
    assert "authentication failed" in document
    assert "talaria refresh-credential" in document


def test_v050_release_notes_describe_the_shipped_upgrade_surfaces() -> None:
    """Release notes cover each reader-visible v0.5.0 addition against code-owned names."""
    document = _V050_RELEASE_NOTES_PATH.read_text(encoding="utf-8")

    assert f"# Talaria v{__version__}" in document
    assert "v0.4.0" in document
    assert len(BUILTIN_THEMES) == 4
    for theme in BUILTIN_THEMES:
        assert theme.name in document

    for required in (
        "talaria theme import FILE [--name NAME]",
        "bottom status bar",
        "/bar",
        "right inspector",
        "/inspector",
        "read-only diff viewer",
        "/diffs",
        "ui.reduced_motion",
        "restart-to-apply",
        "no external-file live reload",
    ):
        assert required in document


def test_new_reader_guides_do_not_restate_acceptance_ledger_metadata() -> None:
    """Reader guides cannot create another stale acceptance-status narrative."""
    for path in (_INSTALL_GUIDE_PATH, _V050_RELEASE_NOTES_PATH):
        document = path.read_text(encoding="utf-8").lower()
        for ledger_term in ("candidate commit", "receipt", "manifest", "superseded"):
            assert ledger_term not in document, (path, ledger_term)


def test_v050_candidate_remains_under_unreleased_in_the_changelog() -> None:
    """An untagged candidate must not be presented as a released changelog section."""
    changelog = (_REPO_ROOT / "CHANGELOG.md").read_text(encoding="utf-8")

    assert "## [Unreleased]" in changelog
    assert re.search(r"(?m)^## \[0\.5\.0\]", changelog) is None
