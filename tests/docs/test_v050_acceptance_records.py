"""Keep the v0.5.0 acceptance summaries bound to their receipt evidence."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any, cast

import pytest

from scripts.acceptance.v050_common import HarnessError
from scripts.acceptance.v050_records import (
    check_records,
    choose_current_candidate_after_drive,
)

_REPO_ROOT = Path(__file__).resolve().parents[2]
_ACCEPTANCE_ROOT = _REPO_ROOT / "docs" / "acceptance" / "v0.5.0"
_MANIFEST_PATH = _ACCEPTANCE_ROOT / "artifact-manifest.json"
_RESULTS_PATH = (
    _REPO_ROOT
    / "docs"
    / "acceptance"
    / "2026-08-30-talaria-v0-5-0-live-acceptance-results.md"
)


def _manifest() -> dict[str, Any]:
    return cast(
        dict[str, Any], json.loads(_MANIFEST_PATH.read_text(encoding="utf-8"))
    )


def test_generated_acceptance_records_match_the_receipts() -> None:
    """This is the repository gate that refuses silently drifted summaries."""
    assert check_records() == []


def test_manifest_drift_is_detected(tmp_path: Path) -> None:
    manifest = _manifest()
    counts = manifest["counts"]
    assert isinstance(counts, dict)
    counts["item_receipts"] = -1
    drifted_manifest = tmp_path / "artifact-manifest.json"
    drifted_manifest.write_text(json.dumps(manifest), encoding="utf-8")

    errors = check_records(manifest_path=drifted_manifest)

    assert len(errors) == 1
    assert "disagrees with the receipts" in errors[0]


def test_results_verdict_drift_is_detected(tmp_path: Path) -> None:
    results = _RESULTS_PATH.read_text(encoding="utf-8")
    _, marker, generated = results.partition(
        "<!-- BEGIN GENERATED ACCEPTANCE VERDICTS -->"
    )
    assert marker
    status = re.search(r"`(?:[^`]+)`", generated)
    assert status is not None
    drifted_results = tmp_path / "results.md"
    drifted_results.write_text(
        results[: status.start() + len(results) - len(generated)]
        + "`MUTATED`"
        + results[status.end() + len(results) - len(generated) :],
        encoding="utf-8",
    )

    errors = check_records(results_path=drifted_results)

    assert len(errors) == 1
    assert "verdict cells that disagree" in errors[0]


def test_every_old_candidate_receipt_is_flagged_stale() -> None:
    manifest = _manifest()
    current = manifest["current_candidate"]
    assert isinstance(current, dict)
    current_commit = current["commit"]
    entries = [*manifest["install_receipts"], *manifest["receipts"]]
    current_wheel = current["wheel_sha256"]
    for entry in entries:
        expected_stale = entry["candidate_commit"] != current_commit or (
            current_wheel is not None and entry["wheel_sha256"] != current_wheel
        )
        assert entry["stale_candidate"] is expected_stale
        if expected_stale:
            assert entry["stale_reason"]
        else:
            assert entry["stale_reason"] is None


def test_drive_refresh_advances_only_to_a_descendant_candidate() -> None:
    older = "a" * 40
    newer = "b" * 40

    def is_ancestor(left: str, right: str) -> bool:
        return (left, right) == (older, newer)

    assert choose_current_candidate_after_drive(
        older, newer, is_ancestor=is_ancestor
    ) == newer
    assert choose_current_candidate_after_drive(
        newer, older, is_ancestor=is_ancestor
    ) == newer

    with pytest.raises(HarnessError, match="have diverged"):
        choose_current_candidate_after_drive(
            older, "c" * 40, is_ancestor=is_ancestor
        )
