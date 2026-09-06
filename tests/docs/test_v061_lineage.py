"""The v0.6.1 acceptance lineage: verifier branch, routing, and generator refusals.

C12-T (#150's architect ruling) exists because the released workflow's
`verify-run` recognized two receipt shapes and judged every other schema by
the v0.5.0 rules — so the run's own live receipts, which declare
`talaria-v0.6.1-receipt-v1`, failed on every v0.5.0 field. These tests pin the
three halves of the repair: the v0.6.1 receipt contract itself, the routing
that names an unknown schema instead of falling through, and the generator's
refusals — the version gate, the attestation gate, and the no-waiver gate.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
from pathlib import Path
from typing import Any

import pytest

from scripts.acceptance import v061_evidence
from scripts.acceptance.v050_receipt import (
    V061_ITEM_SCHEMA,
    _validate_v061_install,
    _validate_v061_receipt,
    verify_run,
)

_REPO_ROOT = Path(__file__).resolve().parents[2]
_V061_RECEIPT_SCHEMA = (
    _REPO_ROOT / "docs" / "acceptance" / "v0.6.1" / "receipt.schema.json"
)

_COMMIT = "a" * 40
_OTHER_COMMIT = "b" * 40


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _write_receipt(
    receipt_dir: Path,
    *,
    checklist_item: str = "live-01",
    harness_commit: str = _COMMIT,
    verdict: str = "pass",
    tester: str = "tester",
    evidence_files: dict[str, bytes] | None = None,
    **overrides: Any,
) -> tuple[Path, dict[str, Any]]:
    """Write one conforming receipt with real evidence files; return path and body."""
    receipt_dir.mkdir(parents=True, exist_ok=True)
    files: dict[str, bytes] = evidence_files if evidence_files is not None else {
        "live-01-01-selected.png": b"\x89PNG fake frame one",
        "live-01-02-reloaded.png": b"\x89PNG fake frame two",
    }
    receipt: dict[str, Any] = {
        "schema_version": V061_ITEM_SCHEMA,
        "release": "0.6.1",
        "checklist_item": checklist_item,
        "title": f"{checklist_item} title",
        "issue": "https://github.com/infiquetra/talaria/issues/140",
        "tester": tester,
        "verdict": verdict,
        "harness_commit": harness_commit,
        "recorded_at": "2026-09-05T05:21:00+00:00",
        "evidence": {
            "kind": "reported-live-dispatch",
            "method": "live PTY drive against the frozen head",
            "observation": "the observed behavior",
            "source": "https://github.com/infiquetra/talaria/issues/140",
            "files": {},
        },
    }
    receipt.update(overrides)
    if "evidence" not in overrides and isinstance(receipt.get("evidence"), dict):
        # Only a non-overridden evidence block gets its inventory filled; an
        # override that drops `files` is the violation under test.
        for name, payload in files.items():
            (receipt_dir / name).write_bytes(payload)
            receipt["evidence"]["files"][name] = _sha256(receipt_dir / name)
    path = receipt_dir / "receipt.json"
    path.write_text(json.dumps(receipt, indent=1) + "\n", encoding="utf-8")
    return path, receipt


def _errors_of(receipt: dict[str, Any], receipt_path: Path) -> list[str]:
    return _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)


# ── the receipt contract ───────────────────────────────────────────────────


def test_a_conforming_receipt_validates_clean(tmp_path: Path) -> None:
    path, receipt = _write_receipt(tmp_path / "live-01")
    assert _errors_of(receipt, path) == []


def test_the_receipt_contract_is_what_the_schema_copy_says(tmp_path: Path) -> None:
    """The validator and the shipped schema agree on the conforming shape."""
    import jsonschema

    _path, receipt = _write_receipt(tmp_path / "live-01")
    schema = json.loads(_V061_RECEIPT_SCHEMA.read_text(encoding="utf-8"))
    jsonschema.validate(receipt, schema)
    receipt["tester"] = "worker-2"
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate(receipt, schema)


@pytest.mark.parametrize(
    ("overrides", "expected_fragment"),
    [
        ({"schema_version": "talaria-v0.7.0-receipt-v1"}, "schema_version"),
        ({"release": "0.6.0"}, "release"),
        ({"checklist_item": "live-1"}, "checklist_item"),
        ({"checklist_item": "live-22"}, "checklist_item"),
        ({"checklist_item": 1}, "checklist_item"),
        ({"title": ""}, "title"),
        ({"issue": "not-an-issue"}, "issue"),
        ({"tester": "worker-2"}, "tester"),
        ({"tester": "controller-3"}, "tester"),
        ({"verdict": "waived"}, "verdict"),
        ({"harness_commit": "a" * 39}, "harness_commit"),
        ({"harness_commit": None}, "harness_commit"),
        ({"recorded_at": "yesterday"}, "recorded_at"),
        ({"evidence": {"kind": "x"}}, "evidence.files"),
    ],
)
def test_each_contract_violation_is_named(
    tmp_path: Path, overrides: dict[str, Any], expected_fragment: str
) -> None:
    path, receipt = _write_receipt(tmp_path / "live-01", **overrides)
    errors = _errors_of(receipt, path)
    assert errors, "the violation was accepted"
    assert any(expected_fragment in error for error in errors), errors


def test_a_missing_evidence_file_is_named(tmp_path: Path) -> None:
    path, receipt = _write_receipt(tmp_path / "live-01")
    victim = tmp_path / "live-01" / "live-01-01-selected.png"
    victim.unlink()
    errors = _errors_of(receipt, path)
    assert any("evidence file is missing" in error for error in errors), errors


def test_a_digest_drift_is_named(tmp_path: Path) -> None:
    path, receipt = _write_receipt(tmp_path / "live-01")
    (tmp_path / "live-01" / "live-01-01-selected.png").write_bytes(b"tampered")
    errors = _errors_of(receipt, path)
    assert any("does not match its digest" in error for error in errors), errors


def test_an_unlisted_evidence_file_is_named(tmp_path: Path) -> None:
    path, receipt = _write_receipt(tmp_path / "live-01")
    (tmp_path / "live-01" / "live-01-03-ghost.png").write_bytes(b"unlisted")
    errors = _errors_of(receipt, path)
    assert any("not listed in evidence.files" in error for error in errors), errors


def test_an_evidence_path_escaping_the_receipt_directory_is_refused(
    tmp_path: Path,
) -> None:
    path, receipt = _write_receipt(tmp_path / "live-01")
    receipt["evidence"]["files"]["../outside.png"] = "c" * 64
    errors = _errors_of(receipt, path)
    assert any("inside the receipt's directory" in error for error in errors), errors


def test_an_absolute_evidence_path_is_refused(tmp_path: Path) -> None:
    path, receipt = _write_receipt(tmp_path / "live-01")
    receipt["evidence"]["files"]["/etc/passwd"] = "c" * 64
    errors = _errors_of(receipt, path)
    assert any("inside the receipt's directory" in error for error in errors), errors


def test_the_v061_install_shape_is_checked() -> None:
    good: dict[str, Any] = {
        "schema_version": "talaria-v0.6.1-install-v1",
        "tester": "operator",
        "candidate": {"commit": _COMMIT, "version": "0.6.1", "wheel_sha256": "d" * 64},
        "install": {"version_reported": "0.6.1", "help_ok": True},
    }
    assert _validate_v061_install(good) == []
    wrong_schema: dict[str, Any] = dict(good)
    wrong_schema["schema_version"] = "talaria-v0.6.0-install-v1"
    assert _validate_v061_install(wrong_schema)
    wrong_version = dict(good)
    wrong_version["candidate"] = dict(good["candidate"], version="0.6.0")
    assert _validate_v061_install(wrong_version)


# ── verify-run: the routing fix and the run-level rules ────────────────────


def _git_repo(root: Path) -> Path:
    root.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    subprocess.run(["git", "config", "user.email", "t@example.com"], cwd=root, check=True)
    subprocess.run(["git", "config", "user.name", "t"], cwd=root, check=True)
    subprocess.run(["git", "commit", "-qm", "init", "--allow-empty"], cwd=root, check=True)
    return root


def _manifest(
    evidence_root: Path,
    receipt_entries: list[dict[str, Any]],
    *,
    expected: int,
) -> Path:
    manifest = {
        "$schema": "./artifact-manifest.schema.json",
        "schema_version": "talaria-v0.6.1-artifact-manifest-v1",
        "gate_id": "v0-6-1-daily-driver",
        "generated_command": "recorded for test",
        "status": "complete",
        "recorded_at": "2026-09-05T00:00:00+00:00",
        "harness_commit": _COMMIT,
        "candidate": {
            "commit": _COMMIT,
            "version": "0.6.1",
            "wheel_filename": "talaria-0.6.1-py3-none-any.whl",
            "wheel_sha256": "d" * 64,
        },
        "counts": {
            "expected_receipts": expected,
            "install_receipts": 0,
            "item_receipts": len(receipt_entries),
            "item_verdicts": {"blocked": 0, "fail": 0, "pass": len(receipt_entries), "reserved": 0},
            "invalid_item_receipts": 0,
        },
        "receipts": receipt_entries,
        "install_receipts": [],
        "results_document": "docs/acceptance/v0.6.1/results.md",
        "notes_document": "docs/acceptance/v0.6.1/notes.md",
    }
    path = evidence_root.parent / "artifact-manifest.json"
    path.write_text(json.dumps(manifest, indent=1) + "\n", encoding="utf-8")
    return path


def _entry(path: Path, repo: Path, receipt: dict[str, Any]) -> dict[str, Any]:
    return {
        "receipt_path": path.relative_to(repo).as_posix(),
        "receipt_sha256": _sha256(path),
        "checklist_item": receipt["checklist_item"],
        "tester": receipt["tester"],
        "verdict": receipt["verdict"],
        "harness_commit": receipt["harness_commit"],
        "applies_to_candidate": "same",
    }


def test_verify_run_names_an_unknown_schema_instead_of_falling_through(
    tmp_path: Path,
) -> None:
    """The defect this unit exists for: an unrecognized schema was judged by the
    v0.5.0 rules, failing on every v0.5.0 field instead of being named unknown."""
    repo = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _write_receipt(evidence / "live-01")
    receipt["schema_version"] = "talaria-v0.7.0-receipt-v1"
    path.write_text(json.dumps(receipt, indent=1) + "\n", encoding="utf-8")
    manifest = _manifest(evidence, [], expected=1)

    errors = verify_run(
        manifest, evidence_root=evidence, repo_root=repo, expected_candidate_commit=None
    )
    assert any("unknown receipt schema_version" in error for error in errors), errors
    assert not any("schema_version is not talaria-v0.5.0-receipt-v1" in error for error in errors)


def test_verify_run_routes_v061_receipts_to_the_v061_contract(tmp_path: Path) -> None:
    repo = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _write_receipt(evidence / "live-01")
    manifest = _manifest(evidence, [_entry(path, repo, receipt)], expected=1)

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert errors == [], errors


def test_a_session_named_tester_is_rejected_by_verify_run(tmp_path: Path) -> None:
    repo = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _write_receipt(evidence / "live-01", tester="worker-2")
    manifest = _manifest(evidence, [], expected=1)

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert any("tester must be a stable role label" in error for error in errors), errors


def test_a_duplicate_live_case_is_rejected(tmp_path: Path) -> None:
    repo = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    first, receipt_one = _write_receipt(evidence / "live-01")
    second, receipt_two = _write_receipt(evidence / "live-01-copy", checklist_item="live-01")
    manifest = _manifest(
        evidence,
        [_entry(first, repo, receipt_one), _entry(second, repo, receipt_two)],
        expected=2,
    )

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert any("declared by more than one receipt" in error for error in errors), errors


def test_the_expected_receipt_count_is_read_from_the_manifest_not_a_literal(
    tmp_path: Path,
) -> None:
    repo = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _write_receipt(evidence / "live-01")
    entries = [_entry(path, repo, receipt)]
    manifest = _manifest(evidence, entries, expected=2)

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert any("live receipts on disk, but counts.expected_receipts" in error for error in errors)

    manifest = _manifest(evidence, entries, expected=1)
    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert not any("expected_receipts declares" in error for error in errors), errors


def test_the_ready_rule_has_no_waiver_path(tmp_path: Path) -> None:
    repo = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _write_receipt(evidence / "live-01", verdict="blocked")
    manifest = _manifest(evidence, [_entry(path, repo, receipt)], expected=1)
    document = json.loads(manifest.read_text(encoding="utf-8"))
    document["counts"]["item_verdicts"] = {"blocked": 1, "fail": 0, "pass": 0, "reserved": 0}
    manifest.write_text(json.dumps(document, indent=1), encoding="utf-8")

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert any("no waiver path" in error for error in errors), errors


@pytest.mark.parametrize(
    ("receipt_commit", "applies", "expected_fragment"),
    [
        (_COMMIT, "same", None),
        (_COMMIT, "the theme surfaces are unchanged", "applies_to_candidate must be 'same'"),
        (_OTHER_COMMIT, "same", "non-empty sentence"),
        (_OTHER_COMMIT, "  ", "non-empty sentence"),
        (_OTHER_COMMIT, "the theme surfaces are unchanged since that commit", None),
    ],
)
def test_the_applies_attestation_is_enforced_against_the_candidate(
    tmp_path: Path,
    receipt_commit: str,
    applies: str,
    expected_fragment: str | None,
) -> None:
    repo = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _write_receipt(evidence / "live-01", harness_commit=receipt_commit)
    entry = _entry(path, repo, receipt)
    entry["applies_to_candidate"] = applies
    manifest = _manifest(evidence, [entry], expected=1)

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    if expected_fragment is None:
        assert not any("applies_to_candidate" in error for error in errors), errors
    else:
        assert any(expected_fragment in error for error in errors), errors


# ── the generator's refusals ────────────────────────────────────────────────


def test_the_generator_refuses_a_tree_that_has_not_been_bumped() -> None:
    """Fact 3 from the ruling: the manifest binds post-bump bytes, so recording
    against today's tree — which still reports 0.6.0 — must refuse."""
    with pytest.raises(SystemExit) as caught:
        v061_evidence.record(
            candidate_commit=_COMMIT,
            wheel=Path("/nonexistent.whl"),
            expected_receipts=21,
            applies_map={},
        )
    assert "0.6.1" in str(caught.value)
    assert "version bump must land" in str(caught.value)


def test_the_generator_demands_a_sentence_for_every_earlier_head_receipt(
    tmp_path: Path,
) -> None:
    repo = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _write_receipt(evidence / "live-01", harness_commit=_OTHER_COMMIT)
    receipts = v061_evidence._live_receipts(repo)
    assert set(receipts) == {"live-01"}

    missing = v061_evidence._applies_map_errors(
        receipts, {}, candidate_commit=_COMMIT
    )
    assert missing and "no sentence naming the unchanged surfaces" in missing[0]

    premature = v061_evidence._applies_map_errors(
        receipts,
        {"docs/acceptance/v0.6.1/evidence/live-01/receipt.json": "same"},
        candidate_commit=_COMMIT,
    )
    assert premature and "not `same`" in premature[0]

    refused_map_line = v061_evidence._applies_map_errors(
        receipts,
        {"docs/acceptance/v0.6.1/evidence/live-01/receipt.json": "same"},
        candidate_commit=_OTHER_COMMIT,
    )
    assert refused_map_line and "no applies map line is needed" in refused_map_line[0]


def test_the_generator_refuses_receipts_the_verifier_rejects(tmp_path: Path) -> None:
    repo = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _write_receipt(evidence / "live-01", tester="worker-2")
    receipts = v061_evidence._live_receipts(repo)
    failures = v061_evidence._validate_tree(receipts, expected_receipts=1)
    assert failures, "a session-named tester passed the tree validation"
    every_error = [error for _item, errors in failures for error in errors]
    assert any("tester must be a stable role label" in error for error in every_error)


def test_the_generator_records_the_binding_when_every_gate_opens(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _write_receipt(evidence / "live-01")
    _write_receipt(
        evidence / "live-02", checklist_item="live-02", harness_commit=_OTHER_COMMIT
    )
    applies_map = {
        "docs/acceptance/v0.6.1/evidence/live-02/receipt.json": (
            "the transcript surfaces are unchanged since that commit"
        )
    }
    monkeypatch.setattr(v061_evidence, "_package_version", lambda: "0.6.1")
    probed: list[dict[str, Any]] = []

    def _fake_probe(
        wheel: Path, *, candidate: dict[str, str], recorded_at: str
    ) -> tuple[dict[str, Any], Path]:
        receipt = {
            "schema_version": "talaria-v0.6.0-install-v1",
            "tester": "operator",
            "candidate": dict(candidate),
            "install": {"version_reported": "0.6.1", "help_ok": True},
        }
        probed.append(receipt)
        return receipt, tmp_path / "scratch"

    monkeypatch.setattr(v061_evidence, "_probe_install", _fake_probe)

    wheel = tmp_path / "talaria-0.6.1-py3-none-any.whl"
    wheel.write_bytes(b"wheel bytes")
    manifest_path = v061_evidence.record(
        candidate_commit=_COMMIT,
        wheel=wheel,
        expected_receipts=2,
        applies_map=applies_map,
        repo_root=repo,
    )
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    assert manifest["schema_version"] == "talaria-v0.6.1-artifact-manifest-v1"
    assert manifest["gate_id"] == "v0-6-1-daily-driver"
    assert manifest["counts"]["expected_receipts"] == 2
    assert manifest["counts"]["item_verdicts"]["pass"] == 2
    by_item = {entry["checklist_item"]: entry for entry in manifest["receipts"]}
    assert by_item["live-01"]["applies_to_candidate"] == "same"
    assert by_item["live-02"]["applies_to_candidate"] == applies_map[
        "docs/acceptance/v0.6.1/evidence/live-02/receipt.json"
    ]
    # The imported v0.6.0 probe stamps its own schema; the generator owns the
    # v0.6.1 install schema, and every write is exclusive.
    for probe_rel in (
        "docs/acceptance/v0.6.1/evidence/probe-1/install-receipt.json",
        "docs/acceptance/v0.6.1/evidence/probe-2/install-receipt.json",
    ):
        install = json.loads((repo / probe_rel).read_text(encoding="utf-8"))
        assert install["schema_version"] == "talaria-v0.6.1-install-v1"
    for pointer in ("results_document", "notes_document"):
        assert (repo / manifest[pointer]).is_file()
        body = (repo / manifest[pointer]).read_text(encoding="utf-8")
        assert _COMMIT in body