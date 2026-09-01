"""Keep the v0.5.0 acceptance summaries bound to their receipt evidence."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any, cast

import pytest
from jsonschema import Draft202012Validator

from scripts.acceptance import v050_records
from scripts.acceptance.v050_common import (
    ORDERED_VERDICTS,
    HarnessError,
    active_receipt_paths,
)
from scripts.acceptance.v050_records import (
    acceptance_summary,
    check_records,
    choose_current_candidate_after_drive,
    evidence_index_errors,
    refresh_records,
    render_evidence_index,
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


def test_results_observation_drift_is_detected(tmp_path: Path) -> None:
    results = _RESULTS_PATH.read_text(encoding="utf-8")
    receipt = json.loads(
        (
            _ACCEPTANCE_ROOT
            / "evidence"
            / "t1"
            / "receipts"
            / "item-24-talaria-t1.json"
        ).read_text(encoding="utf-8")
    )
    observation = receipt["observations"][0]
    _, marker, generated = results.partition(
        "<!-- BEGIN GENERATED ACCEPTANCE VERDICTS -->"
    )
    assert marker
    assert observation in generated
    drifted_results = tmp_path / "results.md"
    drifted_results.write_text(
        results.replace(
            observation,
            "This observation was not recorded by either tester.",
            1,
        ),
        encoding="utf-8",
    )

    errors = check_records(results_path=drifted_results)

    assert len(errors) == 1
    assert "observations that disagree" in errors[0]


def test_acceptance_summary_distinguishes_coverage_from_receipt_files() -> None:
    manifest = _manifest()

    assert acceptance_summary(manifest) == (
        "**BLOCKED**: 43 of 43 expected checklist/tester slots are covered. "
        "The evidence set separately contains 44 current receipts (42 item and 2 install). "
        "The one-receipt overlap is checklist item 1 for talaria-t2, which has both an "
        "item receipt and an install receipt. "
        "Item verdicts are 41 pass, 1 blocked, and 0 fail."
    )


def test_results_status_drift_is_detected(tmp_path: Path) -> None:
    results = _RESULTS_PATH.read_text(encoding="utf-8")
    drifted_results = tmp_path / "results.md"
    drifted_results.write_text(
        results.replace("## Status: **BLOCKED**", "## Status: **MUTATED**", 1),
        encoding="utf-8",
    )

    errors = check_records(results_path=drifted_results)

    assert len(errors) == 1
    assert "status that disagrees" in errors[0]


def test_evidence_index_renderer_changes_only_its_managed_count_region() -> None:
    manifest = _manifest()
    document = "# Evidence\n\nHand-written introduction.\n\n| Item | Verdict |\n| --- | --- |\n"

    initialized = render_evidence_index(document, manifest=manifest, initialize=True)

    assert initialized.startswith("# Evidence\n\nHand-written introduction.\n\n")
    assert initialized.endswith("\n| Item | Verdict |\n| --- | --- |\n")
    assert initialized.count("<!-- BEGIN GENERATED ACCEPTANCE MANIFEST COUNTS -->") == 1
    assert initialized.count("<!-- END GENERATED ACCEPTANCE MANIFEST COUNTS -->") == 1
    assert (
        "The generated manifest reports 0 stale receipts, 0 missing current receipts, "
        "and 0 invalid item receipts."
    ) in initialized

    mutated_manifest = json.loads(json.dumps(manifest))
    mutated_manifest["counts"]["stale_receipts"] = 7
    updated = render_evidence_index(initialized, manifest=mutated_manifest)
    begin = "<!-- BEGIN GENERATED ACCEPTANCE MANIFEST COUNTS -->"
    end = "<!-- END GENERATED ACCEPTANCE MANIFEST COUNTS -->"

    assert updated.partition(begin)[0] == initialized.partition(begin)[0]
    assert updated.partition(end)[2] == initialized.partition(end)[2]
    assert "The generated manifest reports 7 stale receipts" in updated


def test_refresh_records_writes_counts_to_each_tester_index(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    manifest = _manifest()
    evidence_root = tmp_path / "evidence"
    for tester in ("t1", "t2"):
        path = evidence_root / tester / "README.md"
        path.parent.mkdir(parents=True)
        path.write_text(
            f"# {tester}\n\nTester narrative.\n\n| Item | Verdict |\n| --- | --- |\n",
            encoding="utf-8",
        )
    results_path = tmp_path / "results.md"
    results_path.write_text("hand-written results\n", encoding="utf-8")
    monkeypatch.setattr(v050_records, "_git_commit_exists", lambda *_: True)
    monkeypatch.setattr(v050_records, "build_manifest", lambda **_: manifest)
    monkeypatch.setattr(v050_records, "_checklist", lambda _: [])
    monkeypatch.setattr(
        v050_records,
        "render_results_document",
        lambda document, **_: document,
    )

    refresh_records(
        current_candidate_commit="4c2d8dbf0ddfb7f38ba1f228369ae2d929319758",
        repo_root=tmp_path,
        evidence_root=evidence_root,
        checklist_path=tmp_path / "checklist-items.json",
        manifest_path=tmp_path / "artifact-manifest.json",
        results_path=results_path,
    )

    for tester in ("t1", "t2"):
        index = (evidence_root / tester / "README.md").read_text(encoding="utf-8")
        assert "Tester narrative." in index
        assert (
            "The generated manifest reports 0 stale receipts, 0 missing current receipts, "
            "and 0 invalid item receipts."
        ) in index


def test_evidence_index_drift_is_checked_for_each_tester(tmp_path: Path) -> None:
    manifest = _manifest()
    evidence_root = tmp_path / "evidence"
    for tester in ("t1", "t2"):
        path = evidence_root / tester / "README.md"
        path.parent.mkdir(parents=True)
        path.write_text(
            render_evidence_index(
                f"# {tester}\n\nTester narrative.\n\n| Item | Verdict |\n| --- | --- |\n",
                manifest=manifest,
                initialize=True,
            ),
            encoding="utf-8",
        )

    assert evidence_index_errors(
        manifest, evidence_root=evidence_root, repo_root=tmp_path
    ) == []
    t2_index = evidence_root / "t2" / "README.md"
    t2_index.write_text(
        t2_index.read_text(encoding="utf-8").replace(
            "The generated manifest reports 0 stale receipts",
            "The generated manifest reports 9 stale receipts",
            1,
        ),
        encoding="utf-8",
    )

    errors = evidence_index_errors(
        manifest, evidence_root=evidence_root, repo_root=tmp_path
    )

    assert errors == [
        "evidence/t2/README.md has manifest counts that disagree with the generated "
        "manifest"
    ]


def test_receipt_schema_requires_observations() -> None:
    receipt_path = active_receipt_paths(_ACCEPTANCE_ROOT / "evidence")[0]
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    del receipt["observations"]
    schema = json.loads(
        (_ACCEPTANCE_ROOT / "receipt.schema.json").read_text(encoding="utf-8")
    )

    errors = list(Draft202012Validator(schema).iter_errors(receipt))

    assert any(error.validator == "required" for error in errors)


def test_receipt_schema_requires_the_drive_time_harness_commit() -> None:
    schema = json.loads(
        (_ACCEPTANCE_ROOT / "receipt.schema.json").read_text(encoding="utf-8")
    )
    receipt_path = active_receipt_paths(_ACCEPTANCE_ROOT / "evidence")[0]
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    receipt.pop("harness_commit", None)

    assert "harness_commit" in schema["required"]
    errors = list(Draft202012Validator(schema).iter_errors(receipt))
    assert any(
        error.validator == "required" and "harness_commit" in error.message
        for error in errors
    )


def test_every_manifest_count_has_machine_readable_meaning() -> None:
    schema = json.loads(
        (_ACCEPTANCE_ROOT / "artifact-manifest.schema.json").read_text(encoding="utf-8")
    )
    counts = schema["properties"]["counts"]

    assert "expected_receipts - missing_current_receipts" in counts["description"]
    for name, definition in counts["properties"].items():
        assert definition.get("description"), name


def test_verdict_schemas_match_the_ordered_runtime_vocabulary() -> None:
    receipt_schema = json.loads(
        (_ACCEPTANCE_ROOT / "receipt.schema.json").read_text(encoding="utf-8")
    )
    manifest_schema = json.loads(
        (_ACCEPTANCE_ROOT / "artifact-manifest.schema.json").read_text(encoding="utf-8")
    )
    manifest_verdicts = manifest_schema["properties"]["counts"]["properties"][
        "item_verdicts"
    ]

    assert receipt_schema["properties"]["verdict"]["enum"] == list(ORDERED_VERDICTS)
    assert manifest_verdicts["required"] == list(ORDERED_VERDICTS)
    assert list(manifest_verdicts["properties"]) == list(ORDERED_VERDICTS)


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


def test_refresh_rejects_a_nonexistent_candidate_before_writing(tmp_path: Path) -> None:
    manifest = tmp_path / "artifact-manifest.json"

    with pytest.raises(HarnessError, match="does not exist in this repository"):
        refresh_records(
            current_candidate_commit="0" * 40,
            repo_root=_REPO_ROOT,
            evidence_root=tmp_path / "evidence",
            checklist_path=_ACCEPTANCE_ROOT / "checklist-items.json",
            manifest_path=manifest,
            results_path=_RESULTS_PATH,
        )

    assert not manifest.exists()
