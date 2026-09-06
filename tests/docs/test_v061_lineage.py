"""The v0.6.1 acceptance lineage: verifier branch, routing, generator, conversion.

C12-T (#150's rulings) exists because the released workflow's `verify-run`
recognized two receipt shapes and judged every other schema by the v0.5.0
rules — so the run's own live receipts, which declare
`talaria-v0.6.1-receipt-v1`, failed on every v0.5.0 field. The second ruling
split identity by key (`candidate_commit_sha`, `install`, `harness`; the
retired `harness_commit` rejected), made the role labels a closed set, allowed
the `not recorded` literal only where it named, refused private identifiers
the way home paths are refused, and prescribed a `convert` subcommand so the
filed receipts convert reproducibly from explicit attestations, never by
back-filling. These tests pin each half.
"""

from __future__ import annotations

import copy
import hashlib
import json
import struct
import subprocess
import zlib
from pathlib import Path
from typing import Any

import pytest

from scripts.acceptance import v061_evidence
from scripts.acceptance.v050_receipt import (
    ALLOWED_PREIMAGE_CLASSES,
    ATTESTATION_MAP_SCHEMA,
    CAPTURE_METADATA_SCHEMA,
    DIRECTORY_EQUALITY_DERIVATION_SCHEMA,
    HOST_SENTINEL_SCHEMA,
    INSTALL_RECEIPT_SCHEMA,
    PIXEL_MEASUREMENTS_SCHEMA,
    READ_CONFIRMATION_RECORD_SCHEMA,
    RECEIPT_SCHEMA,
    REDACTION_CONFIRMATION_ITEM_SCHEMA,
    REDACTION_ITEM_SCHEMA,
    REFUSED_CLASSES,
    STEP_LOG_SCHEMA,
    V061_ITEM_SCHEMA,
    V061_ROLE_LABELS,
    RecordSchema,
    SchemaRegistry,
    ValueCategory,
    _find_capture_time_twin_digest,
    _png_chunk_errors,
    _public_evidence_roots,
    _validate_v061_install,
    _validate_v061_receipt,
    classify_evidence_file,
    evidence_file_privacy_errors,
    find_absolute_paths_in_text,
    is_absolute_filesystem_path,
    is_forbidden_key,
    mask_matched_sentinels,
    validate_directory_equality_derivation,
    validate_image_read_confirmations,
    validate_read_confirmation_record,
    validate_redactions_list,
    validate_twin_redactions,
    verify_run,
)

_REPO_ROOT = Path(__file__).resolve().parents[2]
_V061_RECEIPT_SCHEMA = (
    _REPO_ROOT / "docs" / "acceptance" / "v0.6.1" / "receipt.schema.json"
)

_COMMIT = "a" * 40
_OTHER_COMMIT = "b" * 40
_FRAME_ONE = (
    b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06"
    b"\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\x0bIDATx\x9cc`\x00\x02\x00\x00\x05"
    b"\x00\x01z^\xab?\x00\x00\x00\x00IEND\xaeB`\x82"
)



def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _conforming_receipt(
    receipt_dir: Path,
    *,
    checklist_item: str = "live-01",
    candidate_commit_sha: str = _COMMIT,
    verdict: str = "pass",
    tester: str = "dedicated-tester",
    harness_kind: str = "scratch-capture",
    harness_commit: str | None = None,
    harness_identity: str | None = None,
) -> tuple[Path, dict[str, Any]]:
    """Write one conforming v0.6.1 receipt with real evidence files beside it."""
    receipt_dir.mkdir(parents=True, exist_ok=True)
    (receipt_dir / "live-01-01-selected.png").write_bytes(_FRAME_ONE)
    receipt: dict[str, Any] = {
        "schema_version": V061_ITEM_SCHEMA,
        "release": "0.6.1",
        "checklist_item": checklist_item,
        "title": f"{checklist_item} title",
        "issue": "https://github.com/infiquetra/talaria/issues/140",
        "tester": tester,
        "verdict": verdict,
        "candidate_commit_sha": candidate_commit_sha,
        "recorded_at": "2026-09-05T05:21:00+00:00",
        "install": {
            "kind": "source-checkout",
            "commit": candidate_commit_sha,
            "basis": "attested at conversion by the capturing role, 2026-09-06",
        },
        "harness": {
            "kind": harness_kind,
            "commit": harness_commit,
            "identity": harness_identity,
        },
        "evidence": {
            "narrative": {
                "kind": "reported-live-dispatch",
                "method": "live PTY drive against the frozen head",
                "observation": "the observed behavior",
                "source": "https://github.com/infiquetra/talaria/issues/140",
            },
            "files": {
                "live-01-01-selected.png": _sha256(receipt_dir / "live-01-01-selected.png")
            },
            "files_listed_at": "2026-09-06",
            "screenshots_read_by": "controller",
            "screenshots_read_at": "2026-09-06",
        },
    }
    path = receipt_dir / "receipt.json"
    path.write_text(json.dumps(receipt, indent=1) + "\n", encoding="utf-8")
    return path, receipt


def _filed_thin_receipt(receipt_dir: Path, **overrides: Any) -> tuple[Path, dict[str, Any]]:
    """Write one receipt in the pre-conversion filed shape the thirteen carry.

    ``harness_commit`` holds the frozen target under test; ``evidence`` is the
    narrative object with no file listing — the exact shape `convert` takes as
    its input and the validator refuses.
    """
    receipt_dir.mkdir(parents=True, exist_ok=True)
    (receipt_dir / "live-01-01-selected.png").write_bytes(_FRAME_ONE)
    receipt: dict[str, Any] = {
        "schema_version": V061_ITEM_SCHEMA,
        "release": "0.6.1",
        "checklist_item": "live-01",
        "title": "Live 01: Custom theme workflow",
        "issue": "https://github.com/infiquetra/talaria/issues/140",
        "tester": "tester",
        "verdict": "pass",
        "harness_commit": _COMMIT,
        "recorded_at": "2026-09-05T05:21:00+00:00",
        "evidence": {
            "kind": "reported-live-dispatch",
            "method": "live PTY drive against the frozen head",
            "observation": "the observed behavior",
            "source": "https://github.com/infiquetra/talaria/issues/140",
        },
    }
    receipt.update(overrides)
    path = receipt_dir / "receipt.json"
    path.write_text(json.dumps(receipt, indent=1) + "\n", encoding="utf-8")
    return path, receipt


def _errors_of(receipt: dict[str, Any], receipt_path: Path) -> list[str]:
    return _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)


# ── the receipt contract ────────────────────────────────────────────────────


def test_a_conforming_receipt_validates_clean(tmp_path: Path) -> None:
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    assert _errors_of(receipt, path) == []


def test_the_receipt_contract_is_what_the_schema_copy_says(tmp_path: Path) -> None:
    """The validator and the shipped schema agree on the conforming shape."""
    import jsonschema

    _path, receipt = _conforming_receipt(tmp_path / "live-01")
    schema = json.loads(_V061_RECEIPT_SCHEMA.read_text(encoding="utf-8"))
    jsonschema.validate(receipt, schema)
    receipt["tester"] = "worker-2"
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate(receipt, schema)


def test_every_closed_set_role_label_is_accepted(tmp_path: Path) -> None:
    for label in V061_ROLE_LABELS:
        path, receipt = _conforming_receipt(tmp_path / f"live-{label}", tester=label)
        errors = _errors_of(receipt, path)
        assert not any("tester" in error for error in errors), (label, errors)


def test_the_retired_harness_commit_key_is_rejected(tmp_path: Path) -> None:
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    receipt["harness_commit"] = _COMMIT
    errors = _errors_of(receipt, path)
    assert any("harness_commit is retired" in error for error in errors), errors


@pytest.mark.parametrize(
    "overrides",
    [
        {"schema_version": "talaria-v0.7.0-receipt-v1"},
        {"release": "0.6.0"},
        {"checklist_item": "live-1"},
        {"checklist_item": "live-24"},
        {"checklist_item": 1},
        {"title": ""},
        {"issue": "not-an-issue"},
        {"tester": "worker-2"},
        {"tester": "tester-2"},
        {"verdict": "waived"},
        {"candidate_commit_sha": "a" * 39},
        {"candidate_commit_sha": None},
        {"recorded_at": "yesterday"},
        {"install": {"kind": "wheel"}},
        {"install": {"kind": "source-checkout", "commit": _OTHER_COMMIT}},
        {"install": {"kind": "source-checkout", "commit": _COMMIT}, "candidate_commit_sha": None},
        {"harness": {"kind": "repository-tooling", "commit": None, "identity": None}},
        {"harness": {"kind": "manual", "commit": "x", "identity": None}},
        {"harness": {"kind": "scratch-capture", "commit": "x", "identity": None}},
        {"harness": {"kind": "scratch-capture", "commit": None, "identity": "  "}},
    ],
)
def test_each_contract_violation_is_named(
    tmp_path: Path, overrides: dict[str, Any]
) -> None:
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    receipt.update(overrides)
    errors = _errors_of(receipt, path)
    assert errors, "the violation was accepted"


def test_a_wheel_install_is_valid_when_complete(tmp_path: Path) -> None:
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    receipt["install"] = {
        "kind": "wheel",
        "filename": "talaria-0.6.1-py3-none-any.whl",
        "sha256": "d" * 64,
    }
    assert _errors_of(receipt, path) == []


def test_a_repository_tooling_harness_names_its_own_commit(tmp_path: Path) -> None:
    path, receipt = _conforming_receipt(
        tmp_path / "live-01",
        harness_kind="repository-tooling",
        harness_commit=_COMMIT,
        harness_identity="v061_evidence",
    )
    assert _errors_of(receipt, path) == []


@pytest.mark.parametrize("field", ["gateway", "session", "terminal"])
def test_not_recorded_is_permitted_where_the_ruling_allows_it(
    tmp_path: Path, field: str
) -> None:
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    receipt[field] = "not recorded"
    errors = _errors_of(receipt, path)
    assert not any("'not recorded'" in error for error in errors), errors


@pytest.mark.parametrize(
    "field",
    [
        "candidate_commit_sha",
        "recorded_at",
        "verdict",
        "checklist_item",
        "issue",
        "tester",
    ],
)
def test_not_recorded_is_refused_on_the_never_fields(tmp_path: Path, field: str) -> None:
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    receipt[field] = "not recorded"
    errors = _errors_of(receipt, path)
    assert any(
        "the literal 'not recorded' is permitted only" in error and field in error
        for error in errors
    ), errors


def test_not_recorded_is_refused_on_every_prose_field_the_ruling_does_not_name(
    tmp_path: Path,
) -> None:
    """F-3: an allowlist, not a denylist — no prose field inherits acceptance."""
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    receipt["title"] = "not recorded"
    receipt["install"]["basis"] = "not recorded"
    receipt["evidence"]["files_listed_at"] = "not recorded"
    for prose_field in ("kind", "method", "observation", "source"):
        receipt["evidence"]["narrative"][prose_field] = "not recorded"
    errors = _errors_of(receipt, path)
    for field_path in (
        "title",
        "install.basis",
        "evidence.files_listed_at",
        "evidence.narrative.kind",
        "evidence.narrative.method",
        "evidence.narrative.observation",
        "evidence.narrative.source",
    ):
        assert any(
            error.startswith(field_path) and "'not recorded'" in error for error in errors
        ), (field_path, errors)


def test_not_recorded_is_permitted_on_nested_gateway_session_terminal(
    tmp_path: Path,
) -> None:
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    receipt["gateway"] = {"profile": "not recorded", "endpoint": "ws://x"}
    receipt["session"] = {"mode": "not recorded"}
    receipt["terminal"] = {"host": "not recorded"}
    errors = _errors_of(receipt, path)
    assert not any("'not recorded'" in error for error in errors), errors


def test_a_pass_receipt_with_an_unrecorded_observation_is_refused(
    tmp_path: Path,
) -> None:
    """The reviewer's stated harm, on the field where it costs most."""
    path, receipt = _conforming_receipt(tmp_path / "live-01", verdict="pass")
    receipt["evidence"]["narrative"]["observation"] = "not recorded"
    errors = _errors_of(receipt, path)
    assert any(
        "evidence.narrative.observation" in error and "'not recorded'" in error
        for error in errors
    ), errors


def test_a_pane_identifier_hiding_in_prose_is_refused(tmp_path: Path) -> None:
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    receipt["evidence"]["narrative"]["method"] = (
        "live PTY drive from herdr pane wFB:pT against the frozen head"
    )
    errors = _errors_of(receipt, path)
    assert any("terminal pane identifier" in error for error in errors), errors


def test_a_session_name_hiding_in_prose_is_refused(tmp_path: Path) -> None:
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    receipt["evidence"]["narrative"]["observation"] = (
        "the pane reported worker-2 as its occupant"
    )
    errors = _errors_of(receipt, path)
    assert any("session name" in error for error in errors), errors


def test_the_closed_role_labels_survive_the_identifier_scan(tmp_path: Path) -> None:
    """`worker-lane-a` is letters, not digits; it must never read as a session name."""
    path, receipt = _conforming_receipt(tmp_path / "live-01", tester="worker-lane-a")
    receipt["evidence"]["narrative"]["method"] = (
        "captured by worker-lane-a against the frozen head"
    )
    errors = _errors_of(receipt, path)
    assert not any("session name" in error for error in errors), errors


def test_a_missing_evidence_file_is_named(tmp_path: Path) -> None:
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    (tmp_path / "live-01" / "live-01-01-selected.png").unlink()
    errors = _errors_of(receipt, path)
    assert any("evidence file is missing" in error for error in errors), errors


def test_an_unlisted_evidence_file_is_named(tmp_path: Path) -> None:
    path, receipt = _conforming_receipt(tmp_path / "live-01")
    (tmp_path / "live-01" / "live-01-03-ghost.png").write_bytes(b"unlisted")
    errors = _errors_of(receipt, path)
    assert any("not listed in evidence.files" in error for error in errors), errors


def test_the_v061_install_shape_is_checked() -> None:
    good: dict[str, Any] = {
        "schema_version": "talaria-v0.6.1-install-v1",
        "tester": "operator",
        "candidate": {"commit": _COMMIT, "version": "0.6.1", "wheel_sha256": "d" * 64},
        "install": {"version_reported": "0.6.1", "help_ok": True},
    }
    assert _validate_v061_install(good) == []
    wrong_schema = dict(good)
    wrong_schema["schema_version"] = "talaria-v0.6.0-install-v1"
    assert _validate_v061_install(wrong_schema)
    wrong_version = dict(good)
    wrong_version["candidate"] = dict(good["candidate"], version="0.6.0")
    assert _validate_v061_install(wrong_version)


# ── verify-run: the routing fix and the run-level rules ─────────────────────


def _commit_at(repo: Path, message: str) -> str:
    subprocess.run(
        ["git", "commit", "--allow-empty", "-qm", message], cwd=repo, check=True
    )
    return subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=repo, capture_output=True, text=True, check=True
    ).stdout.strip()


def _git_repo(root: Path) -> tuple[Path, str, str]:
    """A repo with an early commit and a candidate head, for the F-1 floor.

    The early commit is an ancestor of the candidate, so a receipt riding it
    is the legitimate `applies_to_candidate` sentence case; a divergent side
    commit is created on demand for the non-ancestor case.
    """
    root.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    subprocess.run(["git", "config", "user.email", "t@example.com"], cwd=root, check=True)
    subprocess.run(["git", "config", "user.name", "t"], cwd=root, check=True)
    early = _commit_at(root, "early")
    candidate = _commit_at(root, "candidate")
    return root, early, candidate


def _side_commit(repo: Path, base: str) -> str:
    """A commit on a divergent branch: resolves, but is no candidate's ancestor."""
    branch = subprocess.run(
        ["git", "branch", "--show-current"], cwd=repo, capture_output=True, text=True, check=True
    ).stdout.strip()
    subprocess.run(["git", "checkout", "-q", base], cwd=repo, check=True)
    subprocess.run(["git", "checkout", "-q", "-b", "side"], cwd=repo, check=True)
    side = _commit_at(repo, "side")
    subprocess.run(["git", "checkout", "-q", branch], cwd=repo, check=True)
    return side


def _manifest(
    evidence_root: Path,
    receipt_entries: list[dict[str, Any]],
    *,
    expected: int,
    candidate_commit: str | None = None,
    harness_commit: str | None = None,
) -> Path:
    manifest = {
        "$schema": "./artifact-manifest.schema.json",
        "schema_version": "talaria-v0.6.1-artifact-manifest-v1",
        "gate_id": "v0-6-1-daily-driver",
        "generated_command": "recorded for test",
        "status": "complete",
        "recorded_at": "2026-09-05T00:00:00+00:00",
        "harness_commit": harness_commit or _COMMIT,
        "candidate": {
            "commit": candidate_commit or _COMMIT,
            "version": "0.6.1",
            "wheel_filename": "talaria-0.6.1-py3-none-any.whl",
            "wheel_sha256": "d" * 64,
        },
        "counts": {
            "expected_receipts": expected,
            "install_receipts": 0,
            "item_receipts": len(receipt_entries),
            "item_verdicts": {
                "blocked": 0,
                "fail": 0,
                "pass": len(receipt_entries),
                "reserved": 0,
            },
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
        "candidate_commit_sha": receipt["candidate_commit_sha"],
        "applies_to_candidate": "same",
    }


def test_verify_run_names_an_unknown_schema_instead_of_falling_through(
    tmp_path: Path,
) -> None:
    repo, _early, candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _conforming_receipt(
        evidence / "live-01", candidate_commit_sha=candidate
    )
    receipt["schema_version"] = "talaria-v0.7.0-receipt-v1"
    path.write_text(json.dumps(receipt, indent=1) + "\n", encoding="utf-8")
    manifest = _manifest(evidence, [], expected=1, candidate_commit=candidate)

    errors = verify_run(
        manifest, evidence_root=evidence, repo_root=repo, expected_candidate_commit=None
    )
    assert any("unknown receipt schema_version" in error for error in errors), errors
    assert not any(
        "schema_version is not talaria-v0.5.0-receipt-v1" in error for error in errors
    )


def test_verify_run_routes_v061_receipts_to_the_v061_contract(tmp_path: Path) -> None:
    repo, _early, candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _conforming_receipt(
        evidence / "live-01", candidate_commit_sha=candidate
    )
    manifest = _manifest(
        evidence, [_entry(path, repo, receipt)], expected=1, candidate_commit=candidate
    )

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert errors == [], errors


def test_a_duplicate_live_case_is_rejected(tmp_path: Path) -> None:
    repo, _early, candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    first, receipt_one = _conforming_receipt(
        evidence / "live-01", candidate_commit_sha=candidate
    )
    second, receipt_two = _conforming_receipt(
        evidence / "live-01-copy", checklist_item="live-01", candidate_commit_sha=candidate
    )
    manifest = _manifest(
        evidence,
        [_entry(first, repo, receipt_one), _entry(second, repo, receipt_two)],
        expected=2,
        candidate_commit=candidate,
    )

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert any("declared by more than one receipt" in error for error in errors), errors


def test_the_expected_receipt_count_is_read_from_the_manifest_not_a_literal(
    tmp_path: Path,
) -> None:
    repo, _early, candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _conforming_receipt(
        evidence / "live-01", candidate_commit_sha=candidate
    )
    entries = [_entry(path, repo, receipt)]
    manifest = _manifest(evidence, entries, expected=2, candidate_commit=candidate)

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert any(
        "live receipts on disk, but counts.expected_receipts" in error for error in errors
    )

    manifest = _manifest(evidence, entries, expected=1, candidate_commit=candidate)
    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert not any("expected_receipts declares" in error for error in errors), errors


def test_the_ready_rule_has_no_waiver_path(tmp_path: Path) -> None:
    repo, _early, candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _conforming_receipt(
        evidence / "live-01", candidate_commit_sha=candidate, verdict="blocked"
    )
    manifest = _manifest(
        evidence, [_entry(path, repo, receipt)], expected=1, candidate_commit=candidate
    )
    document = json.loads(manifest.read_text(encoding="utf-8"))
    document["counts"]["item_verdicts"] = {
        "blocked": 1,
        "fail": 0,
        "pass": 0,
        "reserved": 0,
    }
    manifest.write_text(json.dumps(document, indent=1), encoding="utf-8")

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert any("no waiver path" in error for error in errors), errors


@pytest.mark.parametrize(
    ("ride", "applies", "expected_fragment"),
    [
        ("candidate", "same", None),
        ("candidate", "the theme surfaces are unchanged", "applies_to_candidate must be 'same'"),
        ("early", "same", "non-empty sentence"),
        ("early", "  ", "non-empty sentence"),
        ("early", "the theme surfaces are unchanged since that commit", None),
    ],
)
def test_the_applies_attestation_is_enforced_against_the_candidate(
    tmp_path: Path,
    ride: str,
    applies: str,
    expected_fragment: str | None,
) -> None:
    repo, early, candidate = _git_repo(tmp_path)
    receipt_commit = candidate if ride == "candidate" else early
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _conforming_receipt(
        evidence / "live-01", candidate_commit_sha=receipt_commit
    )
    entry = _entry(path, repo, receipt)
    entry["applies_to_candidate"] = applies
    manifest = _manifest(
        evidence, [entry], expected=1, candidate_commit=candidate, harness_commit=candidate
    )

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    if expected_fragment is None:
        assert not any("applies_to_candidate" in error for error in errors), errors
    else:
        assert any(expected_fragment in error for error in errors), errors


def test_the_candidate_floor_refuses_a_commit_that_resolves_nowhere(
    tmp_path: Path,
) -> None:
    """F-1: a v0.6.1 receipt may not name a commit that exists in no repository."""
    repo, _early, candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _conforming_receipt(
        evidence / "live-01", candidate_commit_sha=_COMMIT
    )
    entry = _entry(path, repo, receipt)
    entry["applies_to_candidate"] = "the theme surfaces are unchanged since that commit"
    manifest = _manifest(
        evidence, [entry], expected=1, candidate_commit=candidate, harness_commit=candidate
    )

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert any("does not resolve in this repository" in error for error in errors), errors


def test_the_candidate_floor_refuses_a_commit_outside_the_candidates_lineage(
    tmp_path: Path,
) -> None:
    """F-1: the attestation sentence must explain a real lineage, not an invented one."""
    repo, early, candidate = _git_repo(tmp_path)
    side = _side_commit(repo, early)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    path, receipt = _conforming_receipt(
        evidence / "live-01", candidate_commit_sha=side
    )
    entry = _entry(path, repo, receipt)
    entry["applies_to_candidate"] = "the theme surfaces are unchanged since that commit"
    manifest = _manifest(
        evidence, [entry], expected=1, candidate_commit=candidate, harness_commit=candidate
    )

    errors = verify_run(manifest, evidence_root=evidence, repo_root=repo)
    assert any(
        "is not an ancestor of the manifest's candidate" in error for error in errors
    ), errors


# ── the generator's refusals ────────────────────────────────────────────────


def test_the_generator_refuses_a_tree_that_has_not_been_bumped(tmp_path: Path) -> None:
    repo = tmp_path / "repo"
    (repo / "talaria").mkdir(parents=True)
    (repo / "talaria" / "__init__.py").write_text('__version__ = "0.6.0"\n', encoding="utf-8")
    with pytest.raises(SystemExit) as caught:
        v061_evidence.record(
            candidate_commit=_COMMIT,
            wheel=Path("/nonexistent.whl"),
            expected_receipts=23,
            applies_map={},
            repo_root=repo,
        )
    assert "0.6.1" in str(caught.value)
    assert "version bump must land" in str(caught.value)


def test_the_generator_demands_a_sentence_for_every_earlier_head_receipt(
    tmp_path: Path,
) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _conforming_receipt(evidence / "live-01", candidate_commit_sha=_OTHER_COMMIT)
    receipts = v061_evidence._live_receipts(repo)
    assert set(receipts) == {"live-01"}

    missing = v061_evidence._applies_map_errors(receipts, {}, candidate_commit=_COMMIT)
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
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _filed_thin_receipt(evidence / "live-01", tester="worker-2")
    receipts = v061_evidence._live_receipts(repo)
    failures = v061_evidence._validate_tree(receipts, expected_receipts=1)
    assert failures, "a filed thin shape passed the tree validation"
    every_error = [error for _item, errors in failures for error in errors]
    assert any("harness_commit is retired" in error for error in every_error), every_error


def test_the_generator_refuses_private_identifiers_beside_the_evidence(
    tmp_path: Path,
) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    root = repo / "docs" / "acceptance" / "v0.6.1"
    (root / "evidence").mkdir(parents=True)
    stray = root / "CONTROLLER-HANDOFF.md"
    stray.write_text(
        "session worker-3-3 in pane wFB:pT holds goal t-4412\n", encoding="utf-8"
    )
    errors = v061_evidence._non_evidence_identifier_errors(repo)
    assert errors, "the stray note's identifiers passed the sweep"
    joined = "\n".join(errors)
    assert "terminal pane identifier" in joined
    assert "session name" in joined

    stray.unlink()
    assert v061_evidence._non_evidence_identifier_errors(repo) == []


def test_the_generator_records_the_binding_when_every_gate_opens(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _conforming_receipt(evidence / "live-01")
    _conforming_receipt(
        evidence / "live-02",
        checklist_item="live-02",
        candidate_commit_sha=_OTHER_COMMIT,
    )
    applies_map = {
        "docs/acceptance/v0.6.1/evidence/live-02/receipt.json": (
            "the transcript surfaces are unchanged since that commit"
        )
    }
    monkeypatch.setattr(v061_evidence, "_package_version", lambda *args, **kwargs: "0.6.1")

    def _fake_probe(
        wheel: Path, *, candidate: dict[str, str], recorded_at: str
    ) -> tuple[dict[str, Any], Path]:
        receipt = {
            "schema_version": "talaria-v0.6.0-install-v1",
            "tester": "operator",
            "candidate": dict(candidate),
            "install": {"version_reported": "0.6.1", "help_ok": True},
        }
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
    by_item = {entry["checklist_item"]: entry for entry in manifest["receipts"]}
    assert by_item["live-01"]["candidate_commit_sha"] == _COMMIT
    assert by_item["live-01"]["applies_to_candidate"] == "same"
    assert by_item["live-02"]["applies_to_candidate"] == applies_map[
        "docs/acceptance/v0.6.1/evidence/live-02/receipt.json"
    ]
    for probe_rel in (
        "docs/acceptance/v0.6.1/evidence/probe-1/install-receipt.json",
        "docs/acceptance/v0.6.1/evidence/probe-2/install-receipt.json",
    ):
        install = json.loads((repo / probe_rel).read_text(encoding="utf-8"))
        assert install["schema_version"] == "talaria-v0.6.1-install-v1"
    for pointer in ("results_document", "notes_document"):
        body = (repo / manifest[pointer]).read_text(encoding="utf-8")
        assert _COMMIT in body
    results_body = (repo / manifest["results_document"]).read_text(encoding="utf-8")
    assert "twenty-two source-checkout receipts" in results_body
    assert "one wheel receipt" in results_body


# ── convert: derivation from disk, attestation by argument, never back-fill ─


def _attestation(**overrides: Any) -> dict[str, Any]:
    attestation: dict[str, Any] = {
        "attested_by": "dedicated-tester",
        "attested_at": "2026-09-06",
        "tester": "dedicated-tester",
        "expected": "the derived theme applies live and persists",
        "install_kind": "source-checkout",
        "harness_kind": "scratch-capture",
        "screenshots_read_by": "controller",
        "screenshots_read_at": "2026-09-06",
    }
    attestation.update(overrides)
    return attestation


def test_convert_derives_attests_and_never_backfills(tmp_path: Path) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _filed_thin_receipt(evidence / "live-01")
    output = tmp_path / "converted"

    written = v061_evidence.convert(
        attestations={"live-01": _attestation()},
        output_root=output,
        listed_at="2026-09-06",
        repo_root=repo,
    )
    converted_path = output / "live-01" / "receipt.json"
    assert converted_path in written
    converted = json.loads(converted_path.read_text(encoding="utf-8"))

    assert converted["candidate_commit_sha"] == _COMMIT
    assert "harness_commit" not in converted
    assert converted["install"] == {
        "kind": "source-checkout",
        "commit": _COMMIT,
        "basis": "attested at conversion by the capturing role, 2026-09-06",
    }
    assert converted["harness"] == {
        "kind": "scratch-capture",
        "commit": None,
        "identity": "not recorded",
    }
    assert converted["tester"] == "dedicated-tester"
    assert converted["expected"] == {
        "source": "child pass condition",
        "text": "the derived theme applies live and persists",
    }
    assert converted["actions"] == "live PTY drive against the frozen head"
    assert converted["actual"] == "the observed behavior"
    assert converted["gateway"] == "not recorded"
    assert converted["session"] == "not recorded"
    assert converted["terminal"] == "not recorded"
    narrative = converted["evidence"]["narrative"]
    assert narrative["method"] == "live PTY drive against the frozen head"
    assert narrative["observation"] == "the observed behavior"
    assert converted["evidence"]["files_listed_at"] == "2026-09-06"
    assert converted["evidence"]["files"] == {
        "live-01-01-selected.png": _sha256(
            evidence / "live-01" / "live-01-01-selected.png"
        )
    }
    assert _sha256(output / "live-01" / "live-01-01-selected.png") == converted[
        "evidence"
    ]["files"]["live-01-01-selected.png"]
    # The conversion is reproducible: identical arguments, identical bytes.
    second = tmp_path / "converted-again"
    v061_evidence.convert(
        attestations={"live-01": _attestation()},
        output_root=second,
        listed_at="2026-09-06",
        repo_root=repo,
    )
    assert (
        second / "live-01" / "receipt.json"
    ).read_bytes() == converted_path.read_bytes()
    # And the converted receipt passes the validator against its own directory.
    errors = _validate_v061_receipt(
        converted, receipt_path=converted_path, verify_files=True
    )
    assert errors == [], errors


def test_convert_preserves_a_rich_receipt_minus_its_pane_keys(tmp_path: Path) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    case = repo / "docs" / "acceptance" / "v0.6.1" / "evidence" / "live-01"
    case.mkdir(parents=True)
    (case / "live-01-01-fresh.png").write_bytes(_FRAME_ONE)
    rich: dict[str, Any] = {
        "schema_version": V061_ITEM_SCHEMA,
        "release": "0.6.1",
        "checklist_item": "live-01",
        "title": "Live 09: seam freshness",
        "issue": "https://github.com/infiquetra/talaria/issues/144",
        "tester": "tester",
        "verdict": "pass",
        "candidate_commit_sha": _COMMIT,
        "candidate_branch": "work/139-w2",
        "recorded_at": "2026-09-06T02:44:03+00:00",
        "gateway": {"profile": "default", "tester_pane": "wFB:pT"},
        "session": {"mode": "live"},
        "terminal": {"rows": 40, "columns": 120, "tester_pane": "wFB:pQ"},
        "actions": ["focus composer", "focus transcript"],
        "expected": "four frames with a non-zero pixel diff",
        "actual": "frames 01 and 02 differ in the diagnostics section",
        "evidence": {"frames": ["01-fresh", "02-stale"], "wire": {"events": 4}},
    }
    (case / "receipt.json").write_text(json.dumps(rich, indent=1) + "\n", encoding="utf-8")
    output = tmp_path / "converted"

    v061_evidence.convert(
        attestations={"live-01": _attestation()},
        output_root=output,
        listed_at="2026-09-06",
        repo_root=repo,
    )
    converted = json.loads((output / "live-01" / "receipt.json").read_text(encoding="utf-8"))
    assert converted["candidate_commit_sha"] == _COMMIT
    assert converted["candidate_branch"] == "work/139-w2"
    assert converted["gateway"] == {"profile": "default"}
    assert converted["terminal"] == {"rows": 40, "columns": 120}
    assert converted["session"] == {"mode": "live"}
    assert converted["expected"] == "four frames with a non-zero pixel diff"
    assert converted["actual"] == "frames 01 and 02 differ in the diagnostics section"
    assert converted["evidence"]["frames"] == ["01-fresh", "02-stale"]
    assert converted["evidence"]["wire"] == {"events": 4}
    assert "tester_pane" not in json.dumps(converted)


@pytest.mark.parametrize(
    ("attestation", "fragment"),
    [
        (None, "no attestation for its commit — re-capture rather than convert"),
        (_attestation(attested_by="worker-2"), "closed-set role label"),
        (_attestation(tester="worker-2"), "closed-set role label"),
        (_attestation(attested_at=""), "attested_at must record its date"),
        (_attestation(expected=""), "the owning child's pass condition"),
    ],
)
def test_convert_refuses_without_every_attestation(
    tmp_path: Path, attestation: dict[str, Any] | None, fragment: str
) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _filed_thin_receipt(evidence / "live-01")
    output = tmp_path / "converted"

    with pytest.raises(SystemExit) as caught:
        v061_evidence.convert(
            attestations={} if attestation is None else {"live-01": attestation},
            output_root=output,
            listed_at="2026-09-06",
            repo_root=repo,
        )
    assert fragment in str(caught.value)
    assert not list(output.rglob("receipt.json")) if output.exists() else True


def test_convert_refuses_a_receipt_whose_commit_cannot_be_attested(
    tmp_path: Path,
) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _filed_thin_receipt(evidence / "live-01", harness_commit=None)
    output = tmp_path / "converted"

    with pytest.raises(SystemExit) as caught:
        v061_evidence.convert(
            attestations={"live-01": _attestation()},
            output_root=output,
            listed_at="2026-09-06",
            repo_root=repo,
        )
    assert "re-capture rather than convert" in str(caught.value)
    assert not list(output.rglob("receipt.json")) if output.exists() else True


def test_convert_refuses_hardcoded_kinds_from_the_attestation(
    tmp_path: Path,
) -> None:
    """F-4: the install and harness kinds are attested facts, not assumptions."""
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _filed_thin_receipt(evidence / "live-01")
    output = tmp_path / "converted"
    for attestation, fragment in (
        (_attestation(install_kind=None), "attestation.install_kind must be"),
        (_attestation(install_kind="wheel"), "wheel-install case is not defined"),
        (_attestation(harness_kind=None), "attestation.harness_kind must be"),
        (
            _attestation(harness_kind="repository-tooling"),
            "must attest its own commit in attestation.harness_commit",
        ),
    ):
        with pytest.raises(SystemExit) as caught:
            v061_evidence.convert(
                attestations={"live-01": attestation},
                output_root=output,
                listed_at="2026-09-06",
                repo_root=repo,
            )
        assert fragment in str(caught.value)
        assert not list(output.rglob("receipt.json")) if output.exists() else True

    # The other direction: a repository-tooling harness that attests its own
    # commit converts cleanly, and the commit rides in harness.commit.
    tooling = tmp_path / "tooling-converted"
    v061_evidence.convert(
        attestations={
            "live-01": _attestation(
                harness_kind="repository-tooling", harness_commit=_COMMIT
            )
        },
        output_root=tooling,
        listed_at="2026-09-06",
        repo_root=repo,
    )
    converted = json.loads(
        (tooling / "live-01" / "receipt.json").read_text(encoding="utf-8")
    )
    assert converted["harness"] == {
        "kind": "repository-tooling",
        "commit": _COMMIT,
        "identity": "not recorded",
    }


def test_convert_refuses_when_identifiers_survive_the_filed_text(tmp_path: Path) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _filed_thin_receipt(
        evidence / "live-01",
        candidate_commit_sha=_COMMIT,
        actual="captured in pane wFB:pT, frames differ",
    )
    output = tmp_path / "converted"

    with pytest.raises(SystemExit) as caught:
        v061_evidence.convert(
            attestations={"live-01": _attestation()},
            output_root=output,
            listed_at="2026-09-06",
            repo_root=repo,
        )
    assert "does not validate" in str(caught.value)
    assert "terminal pane identifier" in str(caught.value)
    assert not list(output.rglob("receipt.json")) if output.exists() else True


# ── derive-capture: declared derivation, withholdings, gapless seq ─────────


def test_derive_capture_produces_valid_derived_wire_capture(tmp_path: Path) -> None:
    raw_path = tmp_path / "raw_capture.jsonl"
    derived_path = tmp_path / "derived_capture.jsonl"
    lines = [
        json.dumps({
            "kind": "header",
            "version": 1,
            "startedAt": "2026-09-06T00:00:00Z",
            "endpoint": "ws://127.0.0.1:8765/api/ws",
        }),
        json.dumps({
            "kind": "frame",
            "seq": 1,
            "at": "2026-09-06T00:00:01Z",
            "dir": "in",
            "frame": {
                "type": "gateway.ready",
                "params": {"home": "/Users/operator/workspace", "pane": "wFB:pT"},
            },
        }),
        json.dumps({
            "kind": "frame",
            "seq": 2,
            "at": "2026-09-06T00:00:02Z",
            "dir": "in",
            "frame": {
                "type": "skills.roster",
                "params": {"skills": ["bash", "edit"], "session": "worker-2"},
            },
        }),
    ]
    raw_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    out = v061_evidence.derive_capture(
        input_path=raw_path,
        output_path=derived_path,
        tool="test-harness derive-capture",
        derived_at="2026-09-06T01:00:00Z",
    )
    assert out == derived_path
    assert derived_path.is_file()

    out_lines = derived_path.read_text(encoding="utf-8").splitlines()
    header = json.loads(out_lines[0])
    assert header["kind"] == "header"
    assert "derivation" in header
    deriv = header["derivation"]
    assert deriv["tool"] == "test-harness derive-capture"
    assert deriv["source_frames"] == 2
    assert deriv["derived_at"] == "2026-09-06T01:00:00Z"
    assert "source_sha256" in deriv
    assert deriv["source_bytes"] == len(raw_path.read_bytes())

    f1 = json.loads(out_lines[1])
    assert f1["seq"] == 1
    assert f1["sourceSeq"] == 1
    assert f1["frame"]["params"]["home"] == "[redacted]/workspace"
    assert f1["frame"]["params"]["pane"] == "[redacted]"
    assert len(f1["redactions"]) == 2

    f2 = json.loads(out_lines[2])
    assert f2["seq"] == 2
    assert f2["sourceSeq"] == 2
    assert f2["frame"]["params"]["skills"] == "[redacted]"
    assert f2["frame"]["params"]["session"] == "[redacted]"
    assert len(f2["redactions"]) == 2

    # Derived capture passes content-based privacy scan cleanly
    from scripts.acceptance.v050_receipt import evidence_file_privacy_errors
    assert evidence_file_privacy_errors(derived_path) == []


def test_derive_capture_filters_frames_by_seq_and_type(tmp_path: Path) -> None:
    raw_path = tmp_path / "raw_capture.jsonl"
    derived_path = tmp_path / "derived_filtered.jsonl"
    lines = [
        json.dumps({"kind": "header", "version": 1, "endpoint": "ws://127.0.0.1:8765"}),
        json.dumps({"kind": "frame", "seq": 10, "frame": {"type": "typeA"}}),
        json.dumps({"kind": "frame", "seq": 20, "frame": {"type": "typeB"}}),
        json.dumps({"kind": "frame", "seq": 30, "frame": {"type": "typeA"}}),
        json.dumps({"kind": "frame", "seq": 40, "frame": {"type": "typeC"}}),
    ]
    raw_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    v061_evidence.derive_capture(
        input_path=raw_path,
        output_path=derived_path,
        frame_types=("typeA",),
        start_seq=15,
    )
    out_lines = derived_path.read_text(encoding="utf-8").splitlines()
    assert len(out_lines) == 2  # header + 1 frame (seq 30)
    frame = json.loads(out_lines[1])
    assert frame["seq"] == 1  # gapless renumbered
    assert frame["sourceSeq"] == 30  # original sequence retained
    assert frame["frame"]["type"] == "typeA"


def test_derive_capture_refuses_to_write_when_private_pattern_remains(tmp_path: Path) -> None:
    raw_path = tmp_path / "raw_capture.jsonl"
    derived_path = tmp_path / "derived_fail.jsonl"
    lines = [
        json.dumps({"kind": "header", "version": 1, "endpoint": "ws://127.0.0.1:8765"}),
        json.dumps({
            "kind": "frame",
            "seq": 1,
            "frame": {"type": "event", "owner": "/Users/private-operator/code"},
        }),
    ]
    raw_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    # If withholding rules do NOT include operator-home-path, the self-scan must catch it
    with pytest.raises(SystemExit) as caught:
        v061_evidence.derive_capture(
            input_path=raw_path,
            output_path=derived_path,
            rules=("skills-roster",),  # intentionally omit operator-home-path
        )
    assert "refusing to write derived capture" in str(caught.value)
    assert not derived_path.exists()


# ── screenshot twin vs human read attestation ─────────────────────────────


def test_screenshot_twin_or_human_read_validation(tmp_path: Path) -> None:
    case_dir = tmp_path / "live-01"
    case_dir.mkdir(parents=True)
    png_file = case_dir / "capture.png"
    png_file.write_bytes(_FRAME_ONE)

    evidence: dict[str, Any] = {
        "files": {"capture.png": _sha256(png_file)},
        "files_listed_at": "2026-09-06",
    }
    receipt: dict[str, Any] = {
        "schema_version": V061_ITEM_SCHEMA,
        "release": "0.6.1",
        "checklist_item": "live-01",
        "title": "Live 01: Theme",
        "issue": "https://github.com/infiquetra/talaria/issues/140",
        "tester": "dedicated-tester",
        "verdict": "pass",
        "candidate_commit_sha": _COMMIT,
        "recorded_at": "2026-09-05T05:21:00+00:00",
        "install": {"kind": "source-checkout", "commit": _COMMIT, "basis": "attested"},
        "harness": {"kind": "scratch-capture", "commit": None, "identity": "harness"},
        "evidence": evidence,
    }
    receipt_path = case_dir / "receipt.json"

    # 1. Refused: no twin and no read_by / read_at
    errors = _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)
    assert any(
        "screenshots have neither a text twin nor a recorded human read" in e
        for e in errors
    )

    # 2. Refused: text twin present but not bound at capture time
    twin = case_dir / "capture.txt"
    twin.write_text("screen text", encoding="utf-8")
    twin_sha = _sha256(twin)
    files_map: dict[str, str] = evidence["files"]
    files_map["capture.txt"] = twin_sha
    errors = _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)
    assert any(
        "is not bound by capture-time twin_digest (twin was not produced at capture time)" in e
        for e in errors
    )

    # 3. Refused: capture-time binding mismatches text twin digest
    sidecar = case_dir / "capture.json"
    sidecar.write_text(json.dumps({"twin_digest": "0" * 64}), encoding="utf-8")
    files_map["capture.json"] = _sha256(sidecar)
    errors = _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)
    assert any(
        "does not match capture-time twin_digest" in e
        for e in errors
    )

    # 4. Accepted: text twin bound by capture-time twin_digest in sidecar
    sidecar.write_text(json.dumps({"twin_digest": twin_sha}), encoding="utf-8")
    files_map["capture.json"] = _sha256(sidecar)
    errors = _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)
    assert errors == []

    # 5. Accepted: text twin bound by capture-time twin_digest in PNG chunk
    del files_map["capture.json"]
    sidecar.unlink()
    meta_payload = {"twin_digest": twin_sha}
    text_chunk = (b"tEXt", b"talaria-evidence\x00" + json.dumps(meta_payload).encode("utf-8"))
    base_chunks = _base_png_chunks()
    png_with_meta = _make_png([base_chunks[0], text_chunk, base_chunks[1], base_chunks[2]])
    png_file.write_bytes(png_with_meta)
    files_map["capture.png"] = _sha256(png_file)
    errors = _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)
    assert errors == []

    # 6. Accepted with human read attestation even without text twin
    del files_map["capture.txt"]
    twin.unlink()
    evidence["screenshots_read_by"] = "dedicated-tester"
    evidence["screenshots_read_at"] = "2026-09-06"
    errors = _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)
    assert errors == []

    # 6a. Human read attestation missing timestamp is refused
    receipt_missing_ts = json.loads(json.dumps(receipt))
    del receipt_missing_ts["evidence"]["screenshots_read_at"]
    errs = _validate_v061_receipt(
        receipt_missing_ts, receipt_path=receipt_path, verify_files=True
    )
    assert any("screenshots have neither a text twin nor a recorded human read" in e for e in errs)

    # 6b. Human read attestation missing reader is refused
    receipt_missing_reader = json.loads(json.dumps(receipt))
    del receipt_missing_reader["evidence"]["screenshots_read_by"]
    errs = _validate_v061_receipt(
        receipt_missing_reader, receipt_path=receipt_path, verify_files=True
    )
    assert any("screenshots have neither a text twin nor a recorded human read" in e for e in errs)

    # 6c. Invalid role label for reader is refused
    receipt_invalid_role = json.loads(json.dumps(receipt))
    receipt_invalid_role["evidence"]["screenshots_read_by"] = "unapproved-reader"
    errs = _validate_v061_receipt(
        receipt_invalid_role, receipt_path=receipt_path, verify_files=True
    )
    assert any("screenshots_read_by must be a closed-set role label" in e for e in errs)


# ── Gate 4: convert file scanning and clean abort ─────────────────────────


def test_convert_refuses_when_copied_file_has_private_identifier(tmp_path: Path) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence" / "live-01"
    _filed_thin_receipt(evidence)
    # Put a private identifier into a sibling file
    (evidence / "captured_notes.txt").write_text("operator at /Users/secret-user/path\n")

    output = tmp_path / "converted"
    with pytest.raises(SystemExit) as caught:
        v061_evidence.convert(
            attestations={"live-01": _attestation()},
            output_root=output,
            listed_at="2026-09-06",
            repo_root=repo,
        )
    assert "carries privacy defects" in str(caught.value)
    assert "operator home path" in str(caught.value)
    # Atomic clean-up: output directory has no leftover files
    assert not list(output.rglob("*")) if output.exists() else True


def test_convert_refuses_when_screenshot_has_no_twin_and_no_read_attestation(
    tmp_path: Path,
) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence" / "live-01"
    _filed_thin_receipt(evidence)

    output = tmp_path / "converted"
    attestation = _attestation()
    # Remove screenshot read attestation
    del attestation["screenshots_read_by"]
    del attestation["screenshots_read_at"]

    with pytest.raises(SystemExit) as caught:
        v061_evidence.convert(
            attestations={"live-01": attestation},
            output_root=output,
            listed_at="2026-09-06",
            repo_root=repo,
        )
    assert "screenshots have neither a text twin nor a recorded human read" in str(caught.value)


def test_convert_refuses_when_screenshot_twin_has_no_capture_binding(
    tmp_path: Path,
) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence" / "live-01"
    _filed_thin_receipt(evidence)
    # Add a text twin without capture-time binding in sidecar or PNG
    (evidence / "live-01-01-selected.txt").write_text("screen text\n", encoding="utf-8")

    output = tmp_path / "converted"
    attestation = _attestation()
    del attestation["screenshots_read_by"]
    del attestation["screenshots_read_at"]

    with pytest.raises(SystemExit) as caught:
        v061_evidence.convert(
            attestations={"live-01": attestation},
            output_root=output,
            listed_at="2026-09-06",
            repo_root=repo,
        )
    assert "refusing to convert — nothing was written" in str(caught.value)
    expected_msg = (
        "is not bound by capture-time twin_digest (twin was not produced at capture time)"
    )
    assert expected_msg in str(caught.value)
    # Refusal happens before anything is copied or written to output
    assert not output.exists() or not list(output.iterdir())


def test_convert_refuses_invalid_screenshots_read_by_role(tmp_path: Path) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence" / "live-01"
    _filed_thin_receipt(evidence)

    output = tmp_path / "converted"
    attestation = _attestation()
    attestation["screenshots_read_by"] = "not-a-valid-role"
    attestation["screenshots_read_at"] = "2026-09-06"

    with pytest.raises(SystemExit) as caught:
        v061_evidence.convert(
            attestations={"live-01": attestation},
            output_root=output,
            listed_at="2026-09-06",
            repo_root=repo,
        )
    assert "refusing to convert — nothing was written" in str(caught.value)
    assert "screenshots_read_by must be a closed-set role label" in str(caught.value)
    assert not output.exists() or not list(output.iterdir())


def test_convert_accepts_when_screenshot_twin_is_bound_at_capture(
    tmp_path: Path,
) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence" / "live-01"
    _filed_thin_receipt(evidence)
    twin_path = evidence / "live-01-01-selected.txt"
    twin_path.write_text("screen text\n", encoding="utf-8")
    twin_sha = _sha256(twin_path)
    (evidence / "live-01-01-selected.json").write_text(
        json.dumps({"twin_digest": twin_sha}), encoding="utf-8"
    )

    output = tmp_path / "converted"
    attestation = _attestation()
    del attestation["screenshots_read_by"]
    del attestation["screenshots_read_at"]

    v061_evidence.convert(
        attestations={"live-01": attestation},
        output_root=output,
        listed_at="2026-09-06",
        repo_root=repo,
    )
    converted_receipt = json.loads(
        (output / "live-01" / "receipt.json").read_text(encoding="utf-8")
    )
    assert "live-01-01-selected.txt" in converted_receipt["evidence"]["files"]


# ── Dynamic Roots ─────────────────────────────────────────────────────────


def test_public_evidence_roots_enumerates_all_version_trees(tmp_path: Path) -> None:
    from scripts.acceptance.v050_receipt import _public_evidence_roots
    (tmp_path / "docs" / "acceptance" / "v0.5.0").mkdir(parents=True)
    (tmp_path / "docs" / "acceptance" / "v0.6.0").mkdir(parents=True)
    (tmp_path / "docs" / "acceptance" / "v0.6.1").mkdir(parents=True)
    (tmp_path / "docs" / "evidence").mkdir(parents=True)

    roots = _public_evidence_roots(tmp_path)
    root_strs = [r.as_posix() for r in roots]
    assert "docs/acceptance/v0.5.0" in root_strs
    assert "docs/acceptance/v0.6.0" in root_strs
    assert "docs/acceptance/v0.6.1" in root_strs
    assert "docs/evidence" in root_strs


# ── Gate 3: record privacy scan ──────────────────────────────────────────


def test_record_refuses_when_public_evidence_carries_privacy_defects(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, early, candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _conforming_receipt(evidence / "live-01", candidate_commit_sha=_COMMIT)

    # Put a private identifier into public evidence
    leaked = repo / "docs" / "evidence" / "notes.txt"
    leaked.parent.mkdir(parents=True, exist_ok=True)
    leaked.write_text("operator home: /Users/private-operator/code\n", encoding="utf-8")

    def _fake_probe(
        wheel: Path, *, candidate: dict[str, str], recorded_at: str
    ) -> tuple[dict[str, Any], Path]:
        return {"schema_version": "talaria-v0.6.0-install-v1"}, tmp_path / "scratch"

    monkeypatch.setattr(v061_evidence, "_package_version", lambda *args, **kwargs: "0.6.1")
    monkeypatch.setattr(v061_evidence, "_probe_install", _fake_probe)
    wheel = tmp_path / "talaria-0.6.1-py3-none-any.whl"
    wheel.write_bytes(b"wheel bytes")

    with pytest.raises(SystemExit) as caught:
        v061_evidence.record(
            candidate_commit=_COMMIT,
            wheel=wheel,
            expected_receipts=1,
            applies_map={},
            repo_root=repo,
        )
    assert "refusing to record: evidence files carry private identifiers" in str(caught.value)


# ── Amended privacy contract tests: PNG chunks, capture-metadata, markdown, keep-list ──


def _make_png(chunks: list[tuple[bytes, bytes]]) -> bytes:
    """Build a PNG byte stream from a list of (chunk_type_4bytes, chunk_data)."""
    header = b"\x89PNG\r\n\x1a\n"
    out = bytearray(header)
    for ctype, cdata in chunks:
        out.extend(struct.pack(">I", len(cdata)))
        out.extend(ctype)
        out.extend(cdata)
        crc = zlib.crc32(ctype + cdata)
        out.extend(struct.pack(">I", crc))
    return bytes(out)


def _base_png_chunks() -> list[tuple[bytes, bytes]]:
    ihdr_data = struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)
    idat_data = zlib.compress(b"\x00\x00\x00\x00\x00")
    return [
        (b"IHDR", ihdr_data),
        (b"IDAT", idat_data),
        (b"IEND", b""),
    ]


def test_png_chunk_allowlist_and_text_metadata() -> None:
    p = Path("screenshot.png")
    # Base PNG is clean
    base_png = _make_png(_base_png_chunks())
    assert _png_chunk_errors(p, base_png) == []

    # Allowed ancillary chunks (pHYs, gAMA, sRGB, tIME)
    phys_chunk = (b"pHYs", struct.pack(">IIB", 2835, 2835, 1))
    gama_chunk = (b"gAMA", struct.pack(">I", 45455))
    srgb_chunk = (b"sRGB", b"\x00")
    chunks_with_allowed = [
        _base_png_chunks()[0],
        phys_chunk,
        gama_chunk,
        srgb_chunk,
        _base_png_chunks()[1],
        _base_png_chunks()[2],
    ]
    assert _png_chunk_errors(p, _make_png(chunks_with_allowed)) == []

    # Disallowed chunk (e.g. bKGD)
    bkgd_chunk = (b"bKGD", b"\x00\x00\x00\x00\x00\x00")
    base = _base_png_chunks()
    disallowed_chunks = [base[0], bkgd_chunk, base[1], base[2]]
    errors = _png_chunk_errors(p, _make_png(disallowed_chunks))
    assert any("PNG contains disallowed chunk type 'bKGD'" in e for e in errors)

    # Disallowed text keyword in tEXt chunk
    bad_text = (b"tEXt", b"Software\x00Talaria")
    errors = _png_chunk_errors(p, _make_png([base[0], bad_text, base[1], base[2]]))
    assert any("unauthorized text chunk tEXt with keyword 'Software'" in e for e in errors)

    # Authorized talaria-evidence keyword with valid schema in tEXt
    valid_doc = {
        "frame": 1,
        "columns": 80,
        "rows": 24,
        "cell_width": 10,
        "cell_height": 20,
        "frame_digest": "0" * 64,
        "session_id": "ses-123",
        "captured_at": "2026-09-06T00:00:00Z",
    }
    good_text = (b"tEXt", b"talaria-evidence\x00" + json.dumps(valid_doc).encode("utf-8"))
    assert _png_chunk_errors(p, _make_png([base[0], good_text, base[1], base[2]])) == []

    # Authorized talaria-evidence keyword with forbidden key (tester_pane)
    forbidden_doc = dict(valid_doc, tester_pane="pane-coord")
    bad_meta_text = (b"tEXt", b"talaria-evidence\x00" + json.dumps(forbidden_doc).encode("utf-8"))
    errors = _png_chunk_errors(p, _make_png([base[0], bad_meta_text, base[1], base[2]]))
    assert any("tester_pane" in e for e in errors)

    # Authorized talaria-evidence keyword with undeclared key
    undeclared_doc = dict(valid_doc, undeclared_field="custom")
    undec_meta_text = (
        b"tEXt",
        b"talaria-evidence\x00" + json.dumps(undeclared_doc).encode("utf-8"),
    )
    errors = _png_chunk_errors(p, _make_png([base[0], undec_meta_text, base[1], base[2]]))
    assert any("undeclared key 'undeclared_field'" in e for e in errors)

    # Compressed zTXt chunk with valid talaria-evidence
    compressed_data = zlib.compress(json.dumps(valid_doc).encode("utf-8"))
    ztxt_chunk = (b"zTXt", b"talaria-evidence\x00\x00" + compressed_data)
    assert _png_chunk_errors(p, _make_png([base[0], ztxt_chunk, base[1], base[2]])) == []

    # Compressed iTXt chunk with valid talaria-evidence
    itxt_comp = (b"iTXt", b"talaria-evidence\x00\x01\x00\x00\x00" + compressed_data)
    assert _png_chunk_errors(p, _make_png([base[0], itxt_comp, base[1], base[2]])) == []

    # Uncompressed iTXt chunk with valid talaria-evidence
    itxt_uncomp = (
        b"iTXt",
        b"talaria-evidence\x00\x00\x00\x00\x00" + json.dumps(valid_doc).encode("utf-8"),
    )
    assert _png_chunk_errors(p, _make_png([base[0], itxt_uncomp, base[1], base[2]])) == []


def test_capture_metadata_file_validation(tmp_path: Path) -> None:
    meta_file = tmp_path / "metadata.json"
    valid_doc = {
        "frame": 2,
        "columns": 100,
        "rows": 30,
        "cell_width": 8,
        "cell_height": 16,
        "twin_digest": "1" * 64,
        "captured_at": "2026-09-06T00:00:00Z",
    }
    meta_file.write_text(json.dumps(valid_doc), encoding="utf-8")
    assert evidence_file_privacy_errors(meta_file) == []

    # String frame name is accepted (e.g. "instrument-01")
    string_frame_doc = dict(valid_doc, frame="instrument-01")
    meta_file.write_text(json.dumps(string_frame_doc), encoding="utf-8")
    assert evidence_file_privacy_errors(meta_file) == []

    # Undeclared key
    invalid_doc = dict(valid_doc, arbitrary_metric=42)
    meta_file.write_text(json.dumps(invalid_doc), encoding="utf-8")
    errors = evidence_file_privacy_errors(meta_file)
    assert any("undeclared key 'arbitrary_metric'" in e for e in errors)

    # Invalid type for declared category (columns must be a count)
    wrong_type = dict(valid_doc, columns="not-an-int")
    meta_file.write_text(json.dumps(wrong_type), encoding="utf-8")
    errors = evidence_file_privacy_errors(meta_file)
    assert any("must be a non-negative number" in e for e in errors)

    # Candidate and session provenance, text twin object, settling, and self check pass
    rich_doc = dict(
        valid_doc,
        case="live-01",
        record_type="v061-item",
        schema="talaria-v0.6.1-capture-v1",
        purpose="workflow-demonstration",
        scope="session-lifetime",
        tester="dedicated-tester",
        gateway="http://127.0.0.1:8000",
        event_log="wire.jsonl",
        first_ansi_offset=1024,
        final_ansi_offset=4096,
        frame_sha256="2" * 64,
        first_frame_sha256="3" * 64,
        png_sha256="4" * 64,
        candidate={
            "commit_sha": "a" * 40,
            "entry_point": "src/main.py",
            "source_module": "talaria/app.py",
            "binary_sha256": "5" * 64,
        },
        session={
            "durable_id": "ses-durable-123",
            "runtime_id": "ses-runtime-456",
            "request_id": "req-789",
            "reply_seq": 1,
            "profile": "default",
            "title": "Main Session",
            "mode": "normal",
        },
        settling={
            "quiet_seconds_per_window": 1,
            "timeout_seconds": 10,
            "windows": 2,
        },
        self_check={
            "algorithm": "sha256",
            "expected_rejection": "none",
            "stable_control": "ok",
            "status": "passed",
        },
        text_twin={
            "file": "capture.txt",
            "path": "capture.txt",
            "sha256": "1" * 64,
            "twin_digest": "1" * 64,
            "frame_sha256": "2" * 64,
        },
        diagnostics_cells=["cell1", "cell2"],
        diagnostics_crop_error="none",
    )
    meta_file.write_text(json.dumps(rich_doc), encoding="utf-8")
    assert evidence_file_privacy_errors(meta_file) == []


def test_markdown_files_forbidden_under_evidence(tmp_path: Path) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    ev_notes = repo / "docs" / "acceptance" / "v0.6.1" / "evidence" / "live-01" / "notes.md"
    ev_notes.parent.mkdir(parents=True, exist_ok=True)
    ev_notes.write_text("# Live 01 Notes\nSome clean observation\n", encoding="utf-8")
    errors = evidence_file_privacy_errors(ev_notes, repo_root=repo)
    assert any("markdown files are forbidden under evidence/" in e for e in errors)

    # Root document (conversion-notes.md) outside evidence/ is permitted if clean
    conv_notes = repo / "docs" / "acceptance" / "v0.6.1" / "conversion-notes.md"
    conv_notes.write_text("# Conversion Notes\nSummary of conversion\n", encoding="utf-8")
    assert evidence_file_privacy_errors(conv_notes, repo_root=repo) == []


def test_harness_identity_absolute_path_refused(tmp_path: Path) -> None:
    receipt_dir = tmp_path / "live-01"
    receipt_dir.mkdir(parents=True, exist_ok=True)
    receipt_path, receipt = _conforming_receipt(
        receipt_dir,
        harness_kind="repository-tooling",
        harness_commit=_COMMIT,
        harness_identity="/Users/tester/tools/run.py",
    )
    errors = _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)
    assert any("must not contain an absolute filesystem path" in e for e in errors)

    # Valid script-name-and-digest passes
    receipt["harness"]["identity"] = "scripts/run.py:a1b2c3d4"
    receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
    assert _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True) == []

    # Attestation refusal during convert
    repo, _early, _candidate = _git_repo(tmp_path / "repo")
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _filed_thin_receipt(evidence / "live-01")
    output = tmp_path / "converted"
    with pytest.raises(SystemExit) as caught:
        v061_evidence.convert(
            attestations={"live-01": _attestation(harness_identity="/tmp/run.sh")},
            output_root=output,
            listed_at="2026-09-06",
            repo_root=repo,
        )
    assert "must not contain an absolute filesystem path" in str(caught.value)


def test_derive_capture_with_keep_list(tmp_path: Path) -> None:
    raw = tmp_path / "raw.jsonl"
    lines = [
        json.dumps({
            "kind": "header",
            "version": 1,
            "startedAt": "2026-09-06T00:00:00Z",
            "endpoint": "ws://127.0.0.1:8000/events",
        }),
        json.dumps({
            "kind": "frame",
            "seq": 10,
            "redactions": [],
            "frame": {
                "type": "event",
                "session_id": "session-12345",
                "method": "inspect",
                "data": {"user_id": "usr-1", "note": "keep-me-not"},
            },
        }),
        json.dumps({
            "kind": "frame",
            "seq": 20,
            "redactions": [],
            "frame": {
                "type": "event",
                "session_id": "session-67890",
                "method": "inspect",
                "data": {"user_id": "usr-2", "note": "keep-me-not"},
            },
        }),
    ]
    raw.write_text("\n".join(lines) + "\n", encoding="utf-8")
    out = tmp_path / "derived.jsonl"

    v061_evidence.derive_capture(
        input_path=raw,
        output_path=out,
        keep_list=("/frame/type", "/frame/method"),
        rules=(),
    )

    out_lines = [json.loads(line) for line in out.read_text(encoding="utf-8").splitlines()]
    header = out_lines[0]
    assert header["derivation"]["keep_list"] == ["/frame/method", "/frame/type"]
    assert header["derivation"]["source_frames"] == 2

    frame1 = out_lines[1]
    assert frame1["seq"] == 1
    assert frame1["sourceSeq"] == 10
    assert frame1["frame"]["type"] == "event"
    assert frame1["frame"]["method"] == "inspect"
    assert frame1["frame"]["session_id"] == "[redacted]"
    assert frame1["frame"]["data"]["user_id"] == "[redacted]"
    assert frame1["frame"]["data"]["note"] == "[redacted]"

    frame2 = out_lines[2]
    assert frame2["seq"] == 2
    assert frame2["sourceSeq"] == 20


def test_convert_source_inventory_reconciliation_and_notes(tmp_path: Path) -> None:
    repo, _early, _candidate = _git_repo(tmp_path)
    evidence = repo / "docs" / "acceptance" / "v0.6.1" / "evidence"
    _filed_thin_receipt(evidence / "live-01", tester="worker-1")

    output = tmp_path / "converted"
    written = v061_evidence.convert(
        attestations={"live-01": _attestation(tester="dedicated-tester")},
        output_root=output,
        listed_at="2026-09-06",
        repo_root=repo,
    )
    notes_path = output / "conversion-notes.md"
    assert notes_path in written
    assert notes_path.is_file()
    notes_content = notes_path.read_text(encoding="utf-8")
    assert "# Talaria v0.6.1 acceptance evidence conversion notes" in notes_content
    assert "role-label-substitution" in notes_content
    assert "live-01" in notes_content
    assert "withheld" in notes_content

    # Uncorrected defect in source inventory causes clean refusal
    (evidence / "live-01" / "notes.md").write_text("# Markdown notes\n", encoding="utf-8")
    second_output = tmp_path / "converted-second"
    with pytest.raises(SystemExit) as caught:
        v061_evidence.convert(
            attestations={"live-01": _attestation(tester="dedicated-tester")},
            output_root=second_output,
            listed_at="2026-09-06",
            repo_root=repo,
        )
    assert "refusing to convert" in str(caught.value)
    assert "markdown files forbidden under evidence" in str(caught.value)


# ── F-1, F-2, F-4: Structural Path Checks & Schema Registry Allowlist ──────


def test_structural_path_check_all_probe_shapes() -> None:
    """F-1: All absolute filesystem path probe shapes are refused."""
    positive_probes = [
        "/tmp/talaria-v061-live/driver.py",
        "/private/tmp/talaria-v061-live/driver.py",
        "/var/folders/ky/n5fq/T/pytest-of-jefcox/x",
        "/opt/scratch/talaria-run",
        "/Users/someone/.codex/worktrees/v061-w2",
        "/Users/operator/workspace",
        "file:///tmp/talaria-v061-live/driver.py",
        "~/driver.py",
        "~alice/driver.py",
        "talaria at /tmp/talaria-v061-live/driver.py, commit 94b2aaa",
        "`/tmp/talaria-v061-live/driver.py`",
        "--workdir=/opt/scratch/talaria-run",
        "identity=/tmp/talaria-v061-live/driver.py",
    ]
    for probe in positive_probes:
        found = find_absolute_paths_in_text(probe)
        assert len(found) >= 1, f"Expected probe to be refused: {probe!r}"
    assert is_absolute_filesystem_path("/tmp/talaria-v061-live/driver.py")
    assert is_absolute_filesystem_path("~/driver.py")
    assert is_absolute_filesystem_path("~alice/driver.py")


def test_structural_path_check_exemptions() -> None:
    """F-1: Declared placeholders, URLs, device nodes, and slash commands are permitted."""
    negative_probes = [
        "<candidate-root>/dist/wheel.whl",
        "file://<candidate-root>/dist/wheel.whl",
        "<scratch-root>/evidence/live-01",
        "file://<scratch-root>/evidence/live-01",
        "<integration-tree>/test",
        "/dev/null",
        "/dev/ptmx",
        "/dev/tty",
        "/theme",
        "/diffs",
        "live-01.png",
        "https://github.com/infiquetra/talaria",
        "http://localhost:8000/v1/sessions",
        "/v1/sessions",
        "/api/v1/stream",
        "./artifact-manifest.schema.json",
        "../relative/path.txt",
    ]
    for probe in negative_probes:
        found = find_absolute_paths_in_text(probe)
        assert found == [], f"Expected exemption to pass: {probe!r}, got {found}"


def test_converted_receipt_with_tmp_path_in_identity_refused(tmp_path: Path) -> None:
    """F-1: harness.identity carrying /tmp paths is refused by receipt validation and scan."""
    receipt_dir = tmp_path / "evidence" / "live-01"
    receipt_dir.mkdir(parents=True)
    receipt_path, receipt = _conforming_receipt(
        receipt_dir,
        harness_identity="talaria at /tmp/talaria-v061-live/driver.py, commit 94b2aaa",
    )
    # Receipt validator rejects /tmp path in harness.identity
    errors = _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)
    assert any("must not contain an absolute filesystem path" in e for e in errors)

    # Privacy scanner on the receipt file rejects it structurally
    file_errors = evidence_file_privacy_errors(receipt_path)
    assert any("/tmp/talaria-v061-live/driver.py" in e for e in file_errors)


def test_schema_registry_covers_all_authored_record_types() -> None:
    """F-2: SchemaRegistry resolves every authored record type without key sniffing."""
    # 1. receipt
    r_schema = SchemaRegistry.lookup(
        Path("receipt.json"),
        {"checklist_item": "live-01", "verdict": "pass"},
    )
    assert r_schema is RECEIPT_SCHEMA

    # 2. install-receipt
    ir_schema = SchemaRegistry.lookup(
        Path("install-receipt.json"),
        {"candidate": {}, "install": {}, "tester": "dedicated-tester"},
    )
    assert ir_schema is INSTALL_RECEIPT_SCHEMA

    # 3. capture-metadata
    cm_schema = SchemaRegistry.lookup(
        Path("capture-metadata.json"),
        {"frame_digest": "0" * 64, "columns": 80, "rows": 24},
    )
    assert cm_schema is CAPTURE_METADATA_SCHEMA

    # 4. pixel-measurements
    pm_schema = SchemaRegistry.lookup(
        Path("measurements.json"),
        {"measurements": {}, "columns": 80, "rows": 24},
    )
    assert pm_schema is PIXEL_MEASUREMENTS_SCHEMA

    # 5. step-log
    sl_schema = SchemaRegistry.lookup(
        Path("step-log.json"),
        {"step": 1, "action": "focus"},
    )
    assert sl_schema is STEP_LOG_SCHEMA

    # 6. attestation-map
    am_schema = SchemaRegistry.lookup(
        Path("attestations.json"),
        {"live-01": {"tester": "dedicated-tester"}},
    )
    assert am_schema is ATTESTATION_MAP_SCHEMA


def test_schema_registry_enforces_allowlist_and_refuses_undeclared_or_unregistered(
    tmp_path: Path,
) -> None:
    """F-2: Unregistered record types or undeclared keys in evidence/ are refused."""
    ev_dir = tmp_path / "evidence" / "live-01"
    ev_dir.mkdir(parents=True)

    # Undeclared key in receipt.json
    receipt_file = ev_dir / "receipt.json"
    bad_receipt = {
        "schema_version": V061_ITEM_SCHEMA,
        "release": "0.6.1",
        "checklist_item": "live-01",
        "verdict": "pass",
        "tester": "dedicated-tester",
        "undeclared_private_field": "some-value",
    }
    receipt_file.write_text(json.dumps(bad_receipt), encoding="utf-8")
    errors = evidence_file_privacy_errors(receipt_file)
    assert any("undeclared key 'undeclared_private_field'" in e for e in errors)

    # Forbidden key in receipt.json
    forbidden_receipt = dict(bad_receipt)
    del forbidden_receipt["undeclared_private_field"]
    forbidden_receipt["tester_pane"] = "w1:p1"
    receipt_file.write_text(json.dumps(forbidden_receipt), encoding="utf-8")
    errors = evidence_file_privacy_errors(receipt_file)
    assert any("forbidden key 'tester_pane'" in e for e in errors)

    # Unregistered JSON record type under evidence/
    unknown_file = ev_dir / "arbitrary_custom_record.json"
    unknown_file.write_text(json.dumps({"arbitrary_payload": 123}), encoding="utf-8")
    errors = evidence_file_privacy_errors(unknown_file)
    assert any("unregistered record type or undeclared key" in e for e in errors)


def test_dead_constant_removed_and_dynamic_roots_functional(tmp_path: Path) -> None:
    """F-4: _PUBLIC_EVIDENCE_ROOTS dead constant is deleted and dynamic discovery works."""
    from scripts.acceptance import v050_receipt

    assert not hasattr(v050_receipt, "_PUBLIC_EVIDENCE_ROOTS")

    # Dynamic roots discover version directories correctly
    v_root = tmp_path / "docs" / "acceptance" / "v0.6.1"
    v_root.mkdir(parents=True)
    roots = _public_evidence_roots(tmp_path)
    assert "docs/acceptance/v0.6.1" in [r.as_posix() for r in roots]


# ── Second Amendment: Refused Value Derivations & Preimage Binding ────────


def test_reversible_transformations_and_abbreviations_refused() -> None:
    """Amendment 2, Sec 1: URL-encoded, escaped, encoded, and abbreviated paths are refused."""
    refused_probes = [
        # URL-encoded paths
        "%2FUsers%2Foperator%2Ftalaria",
        "%2ftmp%2flive-script.py",
        "%2fprivate%2ftmp%2fworker",
        "%2Fvar%2Ffolders%2Fky%2Ftest",
        "%2e%2e%2e%2ftalaria",
        "file://%2FUsers%2Foperator",
        # Escaped path separators
        r"\/Users\/operator\/talaria",
        r"\/tmp\/script.py",
        r"\\tmp\\driver.py",
        r"\\Users\\operator\\work",
        # Encoded path prefixes
        "/-Users-jefcox-workspace-infiquetra-talaria",
        "/-tmp-scratch-dir",
        "/-private-tmp-run",
        # Windows absolute paths
        r"C:\Users\operator\project",
        r"c:\tmp\run.py",
        # Abbreviated paths
        ".../talaria",
        "…/talaria",
        ".../scratch",
        "…/live-09",
    ]
    for probe in refused_probes:
        assert is_absolute_filesystem_path(probe), f"Expected probe to be refused: {probe!r}"
        found = find_absolute_paths_in_text(f"value at {probe}")
        assert len(found) >= 1, f"Expected text scan to refuse: {probe!r}"


def test_path_category_refuses_abbreviations_and_absolute_paths() -> None:
    """Amendment 2, Sec 1: ValueCategory.PATH requires relative or placeholder."""
    schema = RecordSchema(
        name="test-path",
        declared_keys={"target": ValueCategory.PATH},
    )

    # Valid relative paths and approved placeholders pass
    valid_paths = [
        "dist/wheel.whl",
        "evidence/live-01/file.txt",
        "<candidate-root>/dist/wheel.whl",
        "<scratch-root>/output.json",
        "<integration-tree>/evidence",
    ]
    for p in valid_paths:
        errors = schema.validate({"target": p}, path=Path("test.json"))
        assert errors == [], f"Expected valid path to pass: {p!r}, got {errors}"

    # Refused: abbreviated paths (... and …)
    for p in [".../talaria", "…/talaria", "path/.../file", "path/…/file"]:
        errors = schema.validate({"target": p}, path=Path("test.json"))
        assert any(
            "must not be an abbreviated path" in e for e in errors
        ), f"Expected abbreviation error: {p!r}"

    # Refused: absolute paths
    for p in ["/tmp/file.txt", "/Users/operator/file", "~/file.txt"]:
        errors = schema.validate({"target": p}, path=Path("test.json"))
        assert any(
            "must not be an absolute filesystem path" in e for e in errors
        ), f"Expected absolute path error: {p!r}"


def test_harness_label_category_enforces_placeholder_vocabulary() -> None:
    """Amendment 2, Sec 1: ValueCategory.HARNESS_LABEL requires bracketed placeholder format."""
    schema = RecordSchema(
        name="test-label",
        declared_keys={"label": ValueCategory.HARNESS_LABEL},
    )

    # Valid placeholder labels pass
    for valid in ["<project-a>", "<project-b>", "<run-1>"]:
        errors = schema.validate({"label": valid}, path=Path("test.json"))
        assert errors == [], f"Expected label to pass: {valid!r}, got {errors}"

    # Unbracketed or raw strings fail
    for invalid in ["project-a", "my-secret-task", "", "<>"]:
        errors = schema.validate({"label": invalid}, path=Path("test.json"))
        assert len(errors) >= 1, f"Expected invalid label to fail: {invalid!r}"


def test_digest_preimage_registration_enforcement() -> None:
    """Amendment 2, Sec 1: Schema registration fails without declared or valid preimage class."""
    assert "git-commit" in ALLOWED_PREIMAGE_CLASSES

    # 1. Registration fails if digest field has no declared preimage
    with pytest.raises(ValueError) as exc:
        RecordSchema(
            name="missing-preimage",
            declared_keys={"artifact_hash": ValueCategory.DIGEST},
        )
    assert "digest field 'artifact_hash' has no declared preimage class" in str(exc.value)

    # 2. Registration fails if nested digest field has no declared preimage
    with pytest.raises(ValueError) as exc:
        RecordSchema(
            name="missing-nested-preimage",
            declared_keys={"sub": ValueCategory.OBJECT},
            nested_schemas={"sub": {"item_digest": ValueCategory.DIGEST}},
        )
    assert "nested digest field 'sub.item_digest' has no declared preimage class" in str(exc.value)

    # 3. Registration fails if preimage class is not allowed (e.g. working-directory)
    with pytest.raises(ValueError) as exc:
        RecordSchema(
            name="disallowed-preimage",
            declared_keys={"workdir_sha": ValueCategory.DIGEST},
            digest_preimages={"workdir_sha": "working-directory"},
        )
    assert "is not an allowed preimage class" in str(exc.value)

    # 4. Valid registration with allowed preimage class succeeds
    schema = RecordSchema(
        name="valid-schema",
        declared_keys={"commit_sha": ValueCategory.DIGEST},
        digest_preimages={"commit_sha": "git-commit"},
    )
    assert schema.validate({"commit_sha": _COMMIT}, path=Path("test.json")) == []


def test_forbidden_workdir_hash_keys_refused() -> None:
    """Amendment 2, Sec 1: Workdir/path hash keys are strictly forbidden."""
    forbidden_keys = [
        "workdir_hash",
        "working_directory_hash",
        "cwd_hash",
        "workdir_sha256",
        "working_directory_sha256",
        "path_hash",
        "custom_workdir_hash",
        "run_cwd_hash",
    ]
    for key in forbidden_keys:
        assert is_forbidden_key(key), f"Expected key to be forbidden: {key!r}"


def test_receipt_supersedes_schema_support(tmp_path: Path) -> None:
    """Amendment 2, Sec 3: RECEIPT_SCHEMA supports supersedes object with valid digests."""
    receipt_dir = tmp_path / "evidence" / "live-01"
    receipt_dir.mkdir(parents=True)
    receipt_path, receipt = _conforming_receipt(receipt_dir)

    # Add valid supersedes block
    receipt["supersedes"] = {
        "receipt_sha256": "c" * 64,
        "candidate_commit_sha": _COMMIT,
    }
    receipt_path.write_text(json.dumps(receipt), encoding="utf-8")

    # Both _validate_v061_receipt and schema validation pass
    errors = _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)
    assert errors == [], f"Expected supersedes to pass validation: {errors}"
    scan_errors = evidence_file_privacy_errors(receipt_path)
    assert scan_errors == [], f"Expected clean scan: {scan_errors}"

    # Invalid supersedes digest fails
    receipt["supersedes"]["receipt_sha256"] = "not-a-sha256"
    receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
    errors = _validate_v061_receipt(receipt, receipt_path=receipt_path, verify_files=True)
    assert any(
        "supersedes.receipt_sha256 must be a 64-character SHA-256 digest" in e
        for e in errors
    )


def test_record_schema_requires_vocabulary_for_closed_vocabulary_fields() -> None:
    """F-2: RecordSchema must fail construction if CLOSED_VOCABULARY field lacks vocabulary."""
    with pytest.raises(ValueError) as exc:
        RecordSchema(
            name="test-unregistered",
            declared_keys={"status": ValueCategory.CLOSED_VOCABULARY},
        )
    assert "closed-vocabulary field 'status' has no declared vocabulary" in str(exc.value)

    with pytest.raises(ValueError) as exc:
        RecordSchema(
            name="test-nested-unregistered",
            declared_keys={"sub": ValueCategory.OBJECT},
            nested_schemas={"sub": {"role": ValueCategory.CLOSED_VOCABULARY}},
        )
    assert "nested closed-vocabulary field 'sub.role' has no declared vocabulary" in str(exc.value)


def test_url_category_allowlist_enforcement() -> None:
    """F-1: ValueCategory.URL enforces allowed schemes, approved loopback hosts, and no userinfo."""
    schema = RecordSchema(
        name="test-url",
        declared_keys={"endpoint": ValueCategory.URL},
    )
    p = Path("test.json")

    # Approved URL shapes
    assert schema.validate({"endpoint": "ws://127.0.0.1:8765/api/ws"}, path=p) == []
    assert schema.validate({"endpoint": "http://localhost:8000/v1"}, path=p) == []
    assert schema.validate({"endpoint": "http://127.0.0.1:8080/"}, path=p) == []
    assert schema.validate({"endpoint": "<gateway>"}, path=p) == []
    assert schema.validate({"endpoint": "file://<candidate-root>/bin"}, path=p) == []

    # Unapproved host
    errs = schema.validate({"endpoint": "http://jeffs-macbook.local:9119/v1"}, path=p)
    assert any("unapproved host" in e for e in errs)

    # Userinfo / credentials
    errs = schema.validate({"endpoint": "https://jeff:hunter2@10.0.1.44:9119/"}, path=p)
    assert any("must not contain userinfo or credentials" in e for e in errs)

    # Free text / non-URL
    errs = schema.validate({"endpoint": "hello world"}, path=p)
    assert any("unapproved url scheme" in e or "is not a valid url" in e for e in errs)

    # Absolute filesystem path
    errs = schema.validate({"endpoint": "/Users/jefcox/gw.sock"}, path=p)
    assert any("must not contain an absolute filesystem path" in e for e in errs)

    # Unapproved scheme
    errs = schema.validate({"endpoint": "ftp://127.0.0.1:8000/resource"}, path=p)
    assert any("unapproved url scheme" in e for e in errs)

    # Unapproved port
    errs = schema.validate({"endpoint": "http://127.0.0.1:9119/"}, path=p)
    assert any("unapproved port" in e for e in errs)


def test_frame_label_category_and_string_category() -> None:
    """F-10: ValueCategory.STRING strictly accepts str/list[str].

    FRAME_LABEL accepts non-negative int or string label.
    """
    schema = RecordSchema(
        name="test-frame-string",
        declared_keys={
            "text": ValueCategory.STRING,
            "frame": ValueCategory.FRAME_LABEL,
        },
    )
    p = Path("test.json")

    # STRING rejects int
    errs = schema.validate({"text": 42, "frame": 0}, path=p)
    assert any("field 'text' must be a string" in e for e in errs)

    # STRING accepts str
    assert schema.validate({"text": "hello", "frame": 0}, path=p) == []

    # FRAME_LABEL accepts non-negative int or clean string
    assert schema.validate({"text": "hello", "frame": 10}, path=p) == []
    assert schema.validate({"text": "hello", "frame": "frame-10"}, path=p) == []

    # FRAME_LABEL rejects negative int, empty string, or absolute path
    errs = schema.validate({"text": "hello", "frame": -1}, path=p)
    assert any("must be a non-negative number or string label" in e for e in errs)

    errs = schema.validate({"text": "hello", "frame": ""}, path=p)
    assert any("must be a non-empty string label" in e for e in errs)

    errs = schema.validate({"text": "hello", "frame": "/tmp/frame"}, path=p)
    assert any("must not contain an absolute filesystem path" in e for e in errs)


def test_shared_sidecar_does_not_falsely_bind_other_screenshots_in_multi_screenshot_case(
    tmp_path: Path,
) -> None:
    """F-4: Shared sidecar in multi-screenshot case does not falsely bind other PNGs."""
    repo, _early, _candidate = _git_repo(tmp_path)
    receipt_dir = repo / "docs" / "acceptance" / "v0.6.1" / "evidence" / "live-01"
    receipt_dir.mkdir(parents=True, exist_ok=True)
    # Write two distinct screenshots
    (receipt_dir / "a.png").write_bytes(_FRAME_ONE)
    (receipt_dir / "b.png").write_bytes(_FRAME_ONE + b"\x00")
    # Write twins for both
    (receipt_dir / "a.txt").write_text("text a\n", encoding="utf-8")
    (receipt_dir / "b.txt").write_text("text b\n", encoding="utf-8")
    twin_a_sha = _sha256(receipt_dir / "a.txt")
    twin_b_sha = _sha256(receipt_dir / "b.txt")

    # Shared directory-level capture-metadata.json is ignored (no directory-level fallback)
    meta_doc = {
        "frame": 1,
        "columns": 80,
        "rows": 24,
        "cell_width": 8,
        "cell_height": 16,
        "twin_digest": twin_a_sha,
        "png_sha256": _sha256(receipt_dir / "a.png"),
        "text_twin": {"file": "a.txt", "sha256": twin_a_sha},
    }
    (receipt_dir / "capture-metadata.json").write_text(json.dumps(meta_doc), encoding="utf-8")

    listed = {
        Path("a.png"): _sha256(receipt_dir / "a.png"),
        Path("b.png"): _sha256(receipt_dir / "b.png"),
        Path("a.txt"): twin_a_sha,
        Path("b.txt"): twin_b_sha,
        Path("capture-metadata.json"): _sha256(receipt_dir / "capture-metadata.json"),
    }
    # Directory-level capture-metadata.json binds neither screenshot
    assert (
        _find_capture_time_twin_digest(Path("a.png"), receipt_dir=receipt_dir, listed=listed)
        is None
    )
    assert (
        _find_capture_time_twin_digest(Path("b.png"), receipt_dir=receipt_dir, listed=listed)
        is None
    )

    # Per-screenshot sidecar for a.png binds only a.png
    a_meta = {
        "frame": 1,
        "columns": 80,
        "rows": 24,
        "cell_width": 8,
        "cell_height": 16,
        "twin_digest": twin_a_sha,
    }
    (receipt_dir / "a.json").write_text(json.dumps(a_meta), encoding="utf-8")
    listed[Path("a.json")] = _sha256(receipt_dir / "a.json")
    assert (
        _find_capture_time_twin_digest(Path("a.png"), receipt_dir=receipt_dir, listed=listed)
        == twin_a_sha
    )
    assert (
        _find_capture_time_twin_digest(Path("b.png"), receipt_dir=receipt_dir, listed=listed)
        is None
    )

    # Per-screenshot sidecar for b.png binds b.png
    b_meta = {
        "frame": 2,
        "columns": 80,
        "rows": 24,
        "cell_width": 8,
        "cell_height": 16,
        "twin_digest": twin_b_sha,
    }
    (receipt_dir / "b.json").write_text(json.dumps(b_meta), encoding="utf-8")
    listed[Path("b.json")] = _sha256(receipt_dir / "b.json")
    assert (
        _find_capture_time_twin_digest(Path("b.png"), receipt_dir=receipt_dir, listed=listed)
        == twin_b_sha
    )


def test_capture_metadata_schema_category_enforcement() -> None:
    """F-5: Verify each category in CAPTURE_METADATA_SCHEMA is strictly enforced."""
    base_doc = {
        "columns": 80,
        "rows": 24,
        "cell_width": 8,
        "cell_height": 16,
        "width": 640,
        "height": 384,
        "dpi": 72,
        "scale": 1,
        "frame": "frame-01",
        "frame_digest": "a" * 64,
        "frame_digests": ["a" * 64],
        "digests": ["a" * 64],
        "sha256": "b" * 64,
        "twin_digest": "c" * 64,
        "twin_sha256": "c" * 64,
        "twin_path": "twin.txt",
        "recorded_at": "2026-09-06T00:00:00Z",
        "captured_at": "2026-09-06T00:00:00Z",
        "timestamp": "2026-09-06T00:00:00Z",
        "session_id": "ses-12345",
        "format": "png",
        "schema_version": "talaria-v0.6.1-capture-v1",
        "title": "Clean capture title",
        "geometry": {"columns": 80, "rows": 24, "cell_width": 8, "cell_height": 16},
        "terminal": {"columns": 80, "rows": 24, "cell_width": 8, "cell_height": 16},
        "source_digest_sha256": "d" * 64,
        "source": "terminal",
        "view_id": "view-1",
        "capture_kind": "screenshot",
        "candidate": {
            "commit_sha": "e" * 40,
            "entry_point": "src/main.py",
            "source_module": "talaria/app.py",
            "binary_sha256": "f" * 64,
        },
        "case": "live-01",
        "record_type": "v061-item",
        "schema": "talaria-v0.6.1-capture-v1",
        "purpose": "workflow-demonstration",
        "session": {
            "durable_id": "ses-durable",
            "runtime_id": "ses-runtime",
            "request_id": "req-1",
            "reply_seq": 1,
            "profile": "default",
            "title": "Session title",
            "mode": "normal",
        },
        "first_ansi_offset": 0,
        "final_ansi_offset": 100,
        "frame_sha256": "1" * 64,
        "first_frame_sha256": "1" * 64,
        "png_sha256": "2" * 64,
        "gateway": "ws://127.0.0.1:8765/api/ws",
        "event_log": "events.jsonl",
        "tester": "dedicated-tester",
        "scope": "run",
        "settling": {"quiet_seconds_per_window": 1, "timeout_seconds": 10, "windows": 2},
        "self_check": {
            "algorithm": "sha256",
            "expected_rejection": "none",
            "stable_control": "ok",
            "status": "passed",
        },
        "text_twin": {
            "file": "twin.txt",
            "path": "twin.txt",
            "sha256": "3" * 64,
            "twin_digest": "3" * 64,
            "digest": "3" * 64,
            "frame_sha256": "4" * 64,
            "frame_digest": "4" * 64,
        },
        "diagnostics_cells": ["a", "b"],
        "diagnostics_crop_error": "none",
    }
    dummy_path = Path("docs/acceptance/v0.6.1/evidence/live-01/meta.json")
    assert CAPTURE_METADATA_SCHEMA.validate(base_doc, path=dummy_path) == []

    # Verify reviewer's 5 mutants specifically:
    # 1. candidate.entry_point PATH: rejects absolute path
    bad = json.loads(json.dumps(base_doc))
    bad["candidate"]["entry_point"] = "/tmp/entry.py"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must not be an absolute filesystem path" in e for e in errs)

    # 2. candidate.source_module PATH: rejects absolute path
    bad = json.loads(json.dumps(base_doc))
    bad["candidate"]["source_module"] = "/tmp/source.py"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must not be an absolute filesystem path" in e for e in errs)

    # 3. event_log PATH: rejects absolute path
    bad = json.loads(json.dumps(base_doc))
    bad["event_log"] = "/tmp/event.log"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must not be an absolute filesystem path" in e for e in errs)

    # 4. gateway URL: rejects external host, credentials, non-url
    bad = json.loads(json.dumps(base_doc))
    bad["gateway"] = "http://jeffs-macbook.local:9119/v1"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("unapproved host" in e for e in errs)

    bad["gateway"] = "https://jeff:hunter2@10.0.1.44:9119/"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must not contain userinfo or credentials" in e for e in errs)

    bad["gateway"] = "hello world"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("unapproved url scheme" in e or "is not a valid url" in e for e in errs)

    # 5. candidate.commit_sha DIGEST: rejects non-digest string
    bad = json.loads(json.dumps(base_doc))
    bad["candidate"]["commit_sha"] = "not-a-commit-digest"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must be a hex digest" in e for e in errs)

    # Additional category checks to bind all declared assignments:
    # 6. twin_path PATH: rejects absolute path
    bad = json.loads(json.dumps(base_doc))
    bad["twin_path"] = "/tmp/twin.txt"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must not be an absolute filesystem path" in e for e in errs)

    # 7. text_twin.file PATH: rejects absolute path
    bad = json.loads(json.dumps(base_doc))
    bad["text_twin"]["file"] = "/tmp/twin.txt"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must not be an absolute filesystem path" in e for e in errs)

    # 8. frame_digest DIGEST: rejects non-digest
    bad = json.loads(json.dumps(base_doc))
    bad["frame_digest"] = "invalid-hex"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must be a hex digest" in e for e in errs)

    # 9. twin_digest DIGEST: rejects non-digest
    bad = json.loads(json.dumps(base_doc))
    bad["twin_digest"] = "invalid-hex"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must be a hex digest" in e for e in errs)

    # 10. columns COUNT: rejects string
    bad = json.loads(json.dumps(base_doc))
    bad["columns"] = "eighty"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must be a non-negative number" in e for e in errs)

    # 11. rows COUNT: rejects negative number
    bad = json.loads(json.dumps(base_doc))
    bad["rows"] = -5
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must be a non-negative number" in e for e in errs)

    # 12. frame FRAME_LABEL: rejects absolute path or negative int
    bad = json.loads(json.dumps(base_doc))
    bad["frame"] = "/tmp/frame"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must not contain an absolute filesystem path" in e for e in errs)

    bad["frame"] = -1
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must be a non-negative number or string label" in e for e in errs)

    # Non-negative int frame accepted
    good = json.loads(json.dumps(base_doc))
    good["frame"] = 0
    assert CAPTURE_METADATA_SCHEMA.validate(good, path=dummy_path) == []

    # 13. tester CLOSED_VOCABULARY: rejects unapproved tester
    bad = json.loads(json.dumps(base_doc))
    bad["tester"] = "jefcox"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("not in registered closed vocabulary" in e for e in errs)

    # 14. case CLOSED_VOCABULARY: rejects unapproved case
    bad = json.loads(json.dumps(base_doc))
    bad["case"] = "unapproved-case"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("not in registered closed vocabulary" in e for e in errs)

    # 15. session.profile CLOSED_VOCABULARY: rejects unapproved profile
    bad = json.loads(json.dumps(base_doc))
    bad["session"]["profile"] = "unapproved-profile"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("not in registered closed vocabulary" in e for e in errs)

    # 16. self_check.status CLOSED_VOCABULARY: rejects unapproved status
    bad = json.loads(json.dumps(base_doc))
    bad["self_check"]["status"] = "unapproved-status"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("not in registered closed vocabulary" in e for e in errs)

    # 17. candidate.binary_sha256 DIGEST: rejects non-digest
    bad = json.loads(json.dumps(base_doc))
    bad["candidate"]["binary_sha256"] = "invalid"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must be a hex digest" in e for e in errs)

    # 18. text_twin.sha256 DIGEST: rejects non-digest
    bad = json.loads(json.dumps(base_doc))
    bad["text_twin"]["sha256"] = "invalid"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad, path=dummy_path)
    assert any("must be a hex digest" in e for e in errs)


def test_third_amendment_refused_classes_vocabulary() -> None:
    expected_classes = {
        "absolute-filesystem-path",
        "pane-tab-workspace-coordinate",
        "role-digit-session-name",
        "non-loopback-host",
        "operator-identity",
        "non-default-profile-name",
    }
    assert REFUSED_CLASSES == expected_classes

    dummy_path = Path("fake/metadata.json")
    for valid_class in expected_classes:
        item = {
            "index": 0,
            "covered_class": valid_class,
            "twin_span": f"[redacted:{valid_class}:0]",
            "region": {"x": 10, "y": 20, "width": 100, "height": 30},
        }
        assert REDACTION_ITEM_SCHEMA.validate(item, path=dummy_path) == []
        conf = {
            "index": 0,
            "covered_class": valid_class,
            "region_matches_twin_span": True,
        }
        assert REDACTION_CONFIRMATION_ITEM_SCHEMA.validate(conf, path=dummy_path) == []

    # Allowed classes and unknown classes must be refused
    disallowed = [
        "bearer-credential",
        "session-name",
        "temporary-directory",
        "email-address",
        "random-class",
    ]
    for bad_class in disallowed:
        item = {
            "index": 0,
            "covered_class": bad_class,
            "twin_span": f"[redacted:{bad_class}:0]",
            "region": {"x": 10, "y": 20, "width": 100, "height": 30},
        }
        errs = REDACTION_ITEM_SCHEMA.validate(item, path=dummy_path)
        assert any("not in registered closed vocabulary" in e for e in errs)

        conf = {
            "index": 0,
            "covered_class": bad_class,
            "region_matches_twin_span": True,
        }
        errs = REDACTION_CONFIRMATION_ITEM_SCHEMA.validate(conf, path=dummy_path)
        assert any("not in registered closed vocabulary" in e for e in errs)


def test_third_amendment_two_way_sentinel_match() -> None:
    twin_path = Path("frame.txt")
    twin_text = (
        "Hermes online: [redacted:role-digit-session-name:0] at "
        "[redacted:non-loopback-host:1]"
    )
    redactions = [
        {
            "index": 0,
            "covered_class": "role-digit-session-name",
            "twin_span": "[redacted:role-digit-session-name:0]",
            "region": {"x": 0, "y": 0, "width": 50, "height": 20},
        },
        {
            "index": 1,
            "covered_class": "non-loopback-host",
            "twin_span": "[redacted:non-loopback-host:1]",
            "region": {"x": 60, "y": 0, "width": 80, "height": 20},
        },
    ]
    assert validate_twin_redactions(twin_path, twin_text, redactions) == []

    # 1. Unmatched sentinel in twin
    extra_sentinel_twin = twin_text + " [redacted:operator-identity:2]"
    errs = validate_twin_redactions(twin_path, extra_sentinel_twin, redactions)
    assert any("unmatched redaction sentinel" in e for e in errs)

    # 2. Missing sentinel in twin for declared redaction
    missing_sentinel_twin = (
        "Hermes online: [redacted:role-digit-session-name:0] at normal-host"
    )
    errs = validate_twin_redactions(twin_path, missing_sentinel_twin, redactions)
    assert any("not found in text twin" in e for e in errs)

    # 3. Class mismatch between twin sentinel and metadata entry
    mismatched_twin = (
        "Hermes online: [redacted:absolute-filesystem-path:0] at "
        "[redacted:non-loopback-host:1]"
    )
    errs = validate_twin_redactions(twin_path, mismatched_twin, redactions)
    assert any("mismatched covered_class" in e for e in errs)

    # 4. Duplicate index in twin
    dup_twin = (
        "Hermes: [redacted:role-digit-session-name:0] and "
        "[redacted:role-digit-session-name:0]"
    )
    errs = validate_twin_redactions(twin_path, dup_twin, redactions[:1])
    assert any("duplicate redaction sentinel" in e for e in errs)

    # 5. Malformed sentinel in twin
    malformed_twin = "Hermes online: [redacted:broken] at [redacted:non-loopback-host:1]"
    errs = validate_twin_redactions(twin_path, malformed_twin, redactions)
    assert any("malformed redaction sentinel" in e for e in errs)

    # 6. Refused class in twin sentinel
    bad_sentinel_twin = "Hermes: [redacted:bearer-credential:0]"
    bad_redactions = [
        {
            "index": 0,
            "covered_class": "bearer-credential",
            "twin_span": "[redacted:bearer-credential:0]",
            "region": {"x": 0, "y": 0, "width": 10, "height": 10},
        }
    ]
    errs = validate_twin_redactions(twin_path, bad_sentinel_twin, bad_redactions)
    assert any("is not a refused class" in e for e in errs)


def test_third_amendment_span_aware_privacy_carve_out(tmp_path: Path) -> None:
    # 1. mask_matched_sentinels replaces only matched sentinels with spaces of equal length
    raw = "prefix [redacted:absolute-filesystem-path:0] suffix [redacted:unknown:1]"
    redactions = [
        {
            "index": 0,
            "covered_class": "absolute-filesystem-path",
            "twin_span": "[redacted:absolute-filesystem-path:0]",
        }
    ]
    masked = mask_matched_sentinels(raw, redactions)
    assert len(masked) == len(raw)
    assert "[redacted:absolute-filesystem-path:0]" not in masked
    assert "[redacted:unknown:1]" in masked
    assert masked.startswith("prefix ")
    assert masked.endswith(" suffix [redacted:unknown:1]")

    # 2. evidence_file_privacy_errors with valid sidecar and sentinel
    twin_file = tmp_path / "frame-01.txt"
    sidecar_file = tmp_path / "frame-01.json"
    sidecar_file.write_text(
        json.dumps({
            "schema_version": "talaria-v0.6.1-capture-v1",
            "columns": 80,
            "rows": 24,
            "cell_width": 10,
            "cell_height": 20,
            "width": 800,
            "height": 480,
            "dpi": 96,
            "scale": 1,
            "redactions": [
                {
                    "index": 0,
                    "covered_class": "absolute-filesystem-path",
                    "twin_span": "[redacted:absolute-filesystem-path:0]",
                    "region": {"x": 10, "y": 10, "width": 200, "height": 20},
                }
            ],
        }),
        encoding="utf-8",
    )
    twin_file.write_text(
        "Working directory: [redacted:absolute-filesystem-path:0]\n",
        encoding="utf-8",
    )
    assert evidence_file_privacy_errors(twin_file) == []

    # 3. Unmasked path outside sentinel is flagged
    leak_file = tmp_path / "frame-02.txt"
    leak_sidecar = tmp_path / "frame-02.json"
    leak_sidecar.write_text(
        json.dumps({
            "columns": 80,
            "rows": 24,
            "cell_width": 10,
            "redactions": [
                {
                    "index": 0,
                    "covered_class": "absolute-filesystem-path",
                    "twin_span": "[redacted:absolute-filesystem-path:0]",
                    "region": {"x": 10, "y": 10, "width": 200, "height": 20},
                }
            ],
        }),
        encoding="utf-8",
    )
    leak_file.write_text(
        "Masked: [redacted:absolute-filesystem-path:0] but leaked /Users/jefcox/secret\n",
        encoding="utf-8",
    )
    errs = evidence_file_privacy_errors(leak_file)
    assert any("contains absolute filesystem path" in e for e in errs)


def test_third_amendment_checkable_read_confirmations() -> None:
    png = "screenshot-01.png"
    receipt_p = Path("receipt.json")
    redactions = [
        {
            "index": 0,
            "covered_class": "role-digit-session-name",
            "twin_span": "[redacted:role-digit-session-name:0]",
            "region": {"x": 10, "y": 10, "width": 80, "height": 20},
        }
    ]
    twin_file = "screenshot-01.txt"
    twin_text = "Session [redacted:role-digit-session-name:0] is connected."

    valid_confirmation = {
        "image": png,
        "read_by": "dedicated-tester",
        "read_at": "2026-09-06T19:00:00+00:00",
        "redactions_confirmed": [
            {
                "index": 0,
                "covered_class": "role-digit-session-name",
                "region_matches_twin_span": True,
            }
        ],
        "witnessed_element": "is connected",
        "nothing_else_masked": True,
    }

    # Clean confirmation passes
    assert (
        SchemaRegistry.lookup(Path("read-confirmation.json"), valid_confirmation)
        is READ_CONFIRMATION_RECORD_SCHEMA
    )
    assert validate_read_confirmation_record(valid_confirmation, path=receipt_p) == []
    errs = validate_image_read_confirmations(
        png,
        twin_file=twin_file,
        twin_text=twin_text,
        redactions=redactions,
        confirmations=[valid_confirmation],
        receipt_or_path=receipt_p,
    )
    assert errs == []

    # 1. Missing confirmation record for redacted image
    errs = validate_image_read_confirmations(
        png,
        twin_file=twin_file,
        twin_text=twin_text,
        redactions=redactions,
        confirmations=[],
        receipt_or_path=receipt_p,
    )
    assert any("has no recorded read confirmation" in e for e in errs)

    # 2. Check 1 (completeness): unconfirmed redaction span
    bad_conf = json.loads(json.dumps(valid_confirmation))
    bad_conf["redactions_confirmed"] = []
    errs = validate_image_read_confirmations(
        png,
        twin_file=twin_file,
        twin_text=twin_text,
        redactions=redactions,
        confirmations=[bad_conf],
        receipt_or_path=receipt_p,
    )
    assert any("is unconfirmed" in e for e in errs)

    # 3. Check 1 (completeness): confirms phantom index
    bad_conf = json.loads(json.dumps(valid_confirmation))
    bad_conf["redactions_confirmed"].append({
        "index": 99,
        "covered_class": "role-digit-session-name",
        "region_matches_twin_span": True,
    })
    errs = validate_image_read_confirmations(
        png,
        twin_file=twin_file,
        twin_text=twin_text,
        redactions=redactions,
        confirmations=[bad_conf],
        receipt_or_path=receipt_p,
    )
    assert any("which does not exist in capture metadata" in e for e in errs)

    # 4. Check 2 (honest class): confirms allowed class
    bad_conf = json.loads(json.dumps(valid_confirmation))
    bad_conf["redactions_confirmed"][0]["covered_class"] = "bearer-credential"
    errs = validate_image_read_confirmations(
        png,
        twin_file=twin_file,
        twin_text=twin_text,
        redactions=redactions,
        confirmations=[bad_conf],
        receipt_or_path=receipt_p,
    )
    assert any("is not a refused class" in e for e in errs)

    # 5. Check 3 (witnessed element present outside mask)
    # Witnessed element is inside mask (matches sentinel)
    bad_conf = json.loads(json.dumps(valid_confirmation))
    bad_conf["witnessed_element"] = "[redacted:role-digit-session-name:0]"
    errs = validate_image_read_confirmations(
        png,
        twin_file=twin_file,
        twin_text=twin_text,
        redactions=redactions,
        confirmations=[bad_conf],
        receipt_or_path=receipt_p,
    )
    assert any("was found inside a mask" in e for e in errs)

    # Witnessed element does not appear in twin
    bad_conf = json.loads(json.dumps(valid_confirmation))
    bad_conf["witnessed_element"] = "phantom text not in twin"
    errs = validate_image_read_confirmations(
        png,
        twin_file=twin_file,
        twin_text=twin_text,
        redactions=redactions,
        confirmations=[bad_conf],
        receipt_or_path=receipt_p,
    )
    assert any("does not appear in text twin" in e for e in errs)

    # 6. nothing_else_masked must be True
    bad_conf = json.loads(json.dumps(valid_confirmation))
    bad_conf["nothing_else_masked"] = False
    errs = validate_image_read_confirmations(
        png,
        twin_file=twin_file,
        twin_text=twin_text,
        redactions=redactions,
        confirmations=[bad_conf],
        receipt_or_path=receipt_p,
    )
    assert any("nothing_else_masked must be true" in e for e in errs)

    # 7. read_by must be in V061_ROLE_LABELS
    bad_conf = json.loads(json.dumps(valid_confirmation))
    bad_conf["read_by"] = "unapproved-role"
    errs = validate_image_read_confirmations(
        png,
        twin_file=twin_file,
        twin_text=twin_text,
        redactions=redactions,
        confirmations=[bad_conf],
        receipt_or_path=receipt_p,
    )
    assert any("must be in V061_ROLE_LABELS" in e for e in errs)

    # 8. Pure screenshot (no twin) requires region_matches_twin_span == False
    pure_conf = json.loads(json.dumps(valid_confirmation))
    pure_conf["redactions_confirmed"][0]["region_matches_twin_span"] = False
    assert (
        validate_image_read_confirmations(
            png,
            twin_file=None,
            twin_text=None,
            redactions=redactions,
            confirmations=[pure_conf],
            receipt_or_path=receipt_p,
        )
        == []
    )

    bad_pure_conf = json.loads(json.dumps(valid_confirmation))
    bad_pure_conf["redactions_confirmed"][0]["region_matches_twin_span"] = True
    errs = validate_image_read_confirmations(
        png,
        twin_file=None,
        twin_text=None,
        redactions=redactions,
        confirmations=[bad_pure_conf],
        receipt_or_path=receipt_p,
    )
    assert any("region_matches_twin_span must be false" in e for e in errs)

    # 9. Image with twin requires region_matches_twin_span == True
    bad_twin_conf = json.loads(json.dumps(valid_confirmation))
    bad_twin_conf["redactions_confirmed"][0]["region_matches_twin_span"] = False
    errs = validate_image_read_confirmations(
        png,
        twin_file=twin_file,
        twin_text=twin_text,
        redactions=redactions,
        confirmations=[bad_twin_conf],
        receipt_or_path=receipt_p,
    )
    assert any("region_matches_twin_span must be true" in e for e in errs)

    # 10. Unredacted image cannot declare redactions_confirmed
    errs = validate_image_read_confirmations(
        png,
        twin_file=twin_file,
        twin_text=twin_text,
        redactions=[],
        confirmations=[valid_confirmation],
        receipt_or_path=receipt_p,
    )
    assert any("cannot declare redactions_confirmed" in e for e in errs)


def test_third_amendment_redaction_review_gate(tmp_path: Path) -> None:
    receipt_dir = tmp_path / "live-01"
    receipt_p, receipt = _conforming_receipt(receipt_dir)

    twin_path = receipt_dir / "live-01-01-selected.txt"
    sidecar_path = receipt_dir / "live-01-01-selected.json"

    twin_path.write_text("Result: [redacted:operator-identity:0]\n", encoding="utf-8")
    sidecar_path.write_text(
        json.dumps({
            "columns": 80,
            "rows": 24,
            "cell_width": 10,
            "cell_height": 20,
            "twin_digest": _sha256(twin_path),
            "redactions": [
                {
                    "index": 0,
                    "covered_class": "operator-identity",
                    "twin_span": "[redacted:operator-identity:0]",
                    "region": {"x": 5, "y": 5, "width": 50, "height": 15},
                }
            ],
        }),
        encoding="utf-8",
    )
    receipt["evidence"]["files"]["live-01-01-selected.txt"] = _sha256(twin_path)
    receipt["evidence"]["files"]["live-01-01-selected.json"] = _sha256(sidecar_path)

    # 1. Missing redaction_review field
    receipt["verdict"] = "pass"
    errs = _validate_v061_receipt(receipt, receipt_path=receipt_p)
    assert any("no redaction_review field" in e for e in errs)

    # 2. redaction_review is pending with verdict: pass
    receipt["evidence"]["redaction_review"] = "pending"
    receipt["evidence"]["read_confirmations"] = [
        {
            "image": "live-01-01-selected.png",
            "read_by": "dedicated-tester",
            "read_at": "2026-09-06T19:00:00+00:00",
            "redactions_confirmed": [
                {
                    "index": 0,
                    "covered_class": "operator-identity",
                    "region_matches_twin_span": True,
                }
            ],
            "witnessed_element": "Result:",
            "nothing_else_masked": True,
        }
    ]
    errs = _validate_v061_receipt(receipt, receipt_path=receipt_p)
    assert any(
        "verdict is pass but evidence.redaction_review is 'pending'" in e for e in errs
    )

    # 3. redaction_review is passed, verdict: pass, read confirmation clean -> PASSES
    receipt["evidence"]["redaction_review"] = "passed"
    assert _validate_v061_receipt(receipt, receipt_path=receipt_p) == []

    # 4. redaction_review is passed, but confirmation is defective (unconfirmed index)
    receipt["evidence"]["read_confirmations"][0]["redactions_confirmed"] = []
    errs = _validate_v061_receipt(receipt, receipt_path=receipt_p)
    assert any(
        "evidence.redaction_review cannot be 'passed' while redaction defects exist" in e
        for e in errs
    )


def test_third_amendment_blanket_redaction_refused() -> None:
    dummy_path = Path("fake/metadata.json")
    redactions = [
        {
            "index": 0,
            "covered_class": "pane-tab-workspace-coordinate",
            "twin_span": "[redacted:pane-tab-workspace-coordinate:0]",
            "region": {"x": 0, "y": 0, "width": 800, "height": 600},
        }
    ]
    errs = validate_redactions_list(
        redactions, path=dummy_path, surface_width=800, surface_height=600
    )
    assert any("blanket redactions are refused" in e for e in errs)


def test_third_amendment_v061_evidence_convert_with_redactions(tmp_path: Path) -> None:
    source_root = tmp_path / "source"
    case_dir = source_root / "docs" / "acceptance" / "v0.6.1" / "evidence" / "live-01"
    case_dir.mkdir(parents=True, exist_ok=True)

    png_path = case_dir / "live-01-01-selected.png"
    png_path.write_bytes(_FRAME_ONE)

    twin_path = case_dir / "live-01-01-selected.txt"
    twin_path.write_text(
        "Status: [redacted:non-default-profile-name:0] ready\n", encoding="utf-8"
    )

    sidecar_path = case_dir / "live-01-01-selected.json"
    sidecar_path.write_text(
        json.dumps({
            "columns": 80,
            "rows": 24,
            "cell_width": 10,
            "cell_height": 20,
            "twin_digest": _sha256(twin_path),
            "redactions": [
                {
                    "index": 0,
                    "covered_class": "non-default-profile-name",
                    "twin_span": "[redacted:non-default-profile-name:0]",
                    "region": {"x": 10, "y": 10, "width": 100, "height": 20},
                }
            ],
        }),
        encoding="utf-8",
    )

    receipt_doc = {
        "schema_version": V061_ITEM_SCHEMA,
        "release": "0.6.1",
        "checklist_item": "live-01",
        "title": "live-01 title",
        "issue": "https://github.com/infiquetra/talaria/issues/140",
        "tester": "dedicated-tester",
        "verdict": "pass",
        "candidate_commit_sha": _COMMIT,
        "recorded_at": "2026-09-06T19:00:00+00:00",
        "evidence": {
            "files": {
                "live-01-01-selected.png": _sha256(png_path),
                "live-01-01-selected.txt": _sha256(twin_path),
                "live-01-01-selected.json": _sha256(sidecar_path),
            },
        },
    }
    (case_dir / "receipt.json").write_text(json.dumps(receipt_doc), encoding="utf-8")

    # 1. Conversion with defective attestation (missing confirmation) fails and writes nothing
    bad_attestations = {
        "live-01": {
            "attested_by": "dedicated-tester",
            "tester": "dedicated-tester",
            "capturing_role": "dedicated-tester",
            "attested_at": "2026-09-06T19:00:00+00:00",
            "install_kind": "source-checkout",
            "harness_kind": "scratch-capture",
            "harness_identity": "<scratch-harness-id>",
            "expected": "expected behavior",
            "redaction_review": "passed",
            "read_confirmations": [],
        }
    }
    output_bad = tmp_path / "output_bad"
    with pytest.raises(SystemExit) as exc_info:
        v061_evidence.convert(
            attestations=bad_attestations,
            output_root=output_bad,
            listed_at="2026-09-06T19:00:00+00:00",
            repo_root=source_root,
        )
    assert "refusing to convert — nothing was written" in str(exc_info.value)
    assert not output_bad.exists() or not any(output_bad.iterdir())

    # 2. Conversion with valid attestation succeeds and produces valid converted receipt
    good_attestations = {
        "live-01": {
            "attested_by": "dedicated-tester",
            "tester": "dedicated-tester",
            "capturing_role": "dedicated-tester",
            "attested_at": "2026-09-06T19:00:00+00:00",
            "install_kind": "source-checkout",
            "harness_kind": "scratch-capture",
            "harness_identity": "<scratch-harness-id>",
            "expected": "expected behavior",
            "redaction_review": "passed",
            "read_confirmations": [
                {
                    "image": "live-01-01-selected.png",
                    "read_by": "dedicated-tester",
                    "read_at": "2026-09-06T19:00:00+00:00",
                    "redactions_confirmed": [
                        {
                            "index": 0,
                            "covered_class": "non-default-profile-name",
                            "region_matches_twin_span": True,
                        }
                    ],
                    "witnessed_element": "ready",
                    "nothing_else_masked": True,
                }
            ],
        }
    }
    output_good = tmp_path / "output_good"
    written = v061_evidence.convert(
        attestations=good_attestations,
        output_root=output_good,
        listed_at="2026-09-06T19:00:00+00:00",
        repo_root=source_root,
    )
    assert len(written) > 0
    converted_receipt_path = output_good / "live-01" / "receipt.json"
    assert converted_receipt_path.is_file()
    converted_doc = json.loads(converted_receipt_path.read_text(encoding="utf-8"))
    assert converted_doc["evidence"]["redaction_review"] == "passed"
    assert len(converted_doc["evidence"]["read_confirmations"]) == 1
    assert _validate_v061_receipt(converted_doc, receipt_path=converted_receipt_path) == []


def test_seven_value_collision_and_expected_rejection_privacy() -> None:
    meta_path = Path("capture.json")
    base_meta = {
        "columns": 80,
        "rows": 24,
        "cell_width": 10,
        "cell_height": 20,
        "twin_digest": "a" * 64,
        "schema": "talaria-v0.6.1-capture-v1",
        "record_type": "v061-item",
        "purpose": "acceptance",
        "case": "live-01",
        "tester": "dedicated-tester",
        "self_check": {
            "algorithm": "cell-frame-equality-v1",
            "stable_control": "pass",
            "status": "pass",
            "expected_rejection": "refusal on frame boundary",
        },
    }

    # 1. Widened values pass
    assert CAPTURE_METADATA_SCHEMA.validate(base_meta, path=meta_path) == []

    # 2. Refusals remain closed
    bad_case = json.loads(json.dumps(base_meta))
    bad_case["case"] = "live-09-recapture"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad_case, path=meta_path)
    assert any("not in registered closed vocabulary" in e for e in errs)

    bad_tester = json.loads(json.dumps(base_meta))
    bad_tester["tester"] = "external-tester"
    errs = CAPTURE_METADATA_SCHEMA.validate(bad_tester, path=meta_path)
    assert any("not in registered closed vocabulary" in e for e in errs)

    # 3. expected_rejection structural privacy enforcement
    # Leaked absolute path in error text is refused
    leaked_path_meta = json.loads(json.dumps(base_meta))
    leaked_path_meta["self_check"]["expected_rejection"] = (
        "error loading /Users/jefcox/secret/key.pem"
    )
    errs = CAPTURE_METADATA_SCHEMA.validate(leaked_path_meta, path=meta_path)
    assert any("contains absolute filesystem path" in e for e in errs)

    # Leaked private pattern in error text is refused
    leaked_session_meta = json.loads(json.dumps(base_meta))
    leaked_session_meta["self_check"]["expected_rejection"] = (
        "connection refused from session worker-1"
    )
    errs = CAPTURE_METADATA_SCHEMA.validate(leaked_session_meta, path=meta_path)
    assert any("contains a private" in e for e in errs)

    # Honestly redacted sentinel in expected_rejection passes
    redacted_meta = json.loads(json.dumps(base_meta))
    redacted_meta["self_check"]["expected_rejection"] = (
        "error loading [redacted:absolute-filesystem-path:0]"
    )
    assert CAPTURE_METADATA_SCHEMA.validate(redacted_meta, path=meta_path) == []


def test_live_22_and_live_23_case_vocabulary(tmp_path: Path) -> None:
    for case_name in ("live-22", "live-23"):
        # Capture metadata accepts live-22 and live-23
        meta = {
            "columns": 80,
            "rows": 24,
            "cell_width": 10,
            "cell_height": 20,
            "twin_digest": "a" * 64,
            "case": case_name,
        }
        assert CAPTURE_METADATA_SCHEMA.validate(meta, path=Path("capture.json")) == []

        # Receipt validation accepts live-22 and live-23
        case_dir = tmp_path / case_name
        receipt_p, receipt = _conforming_receipt(case_dir, checklist_item=case_name)
        assert _validate_v061_receipt(receipt, receipt_path=receipt_p) == []
        assert RECEIPT_SCHEMA.validate(receipt, path=receipt_p) == []

    # live-24 is refused
    bad_meta = {"case": "live-24"}
    errs = CAPTURE_METADATA_SCHEMA.validate(bad_meta, path=Path("capture.json"))
    assert any("not in registered closed vocabulary" in e for e in errs)

    bad_dir = tmp_path / "live-24"
    receipt_p, receipt = _conforming_receipt(bad_dir, checklist_item="live-24")
    errs = _validate_v061_receipt(receipt, receipt_path=receipt_p)
    assert any(
        "checklist_item must be a live-NN string with NN from 01 through 23" in e
        for e in errs
    )


def test_talaria_live_capture_v2_admitted_in_schema_and_format_version() -> None:
    # format_version, schema, and schema_version all admit talaria-live-capture-v2
    for version_key in ("format_version", "schema", "schema_version"):
        meta = {
            "columns": 80,
            "rows": 24,
            "cell_width": 10,
            "cell_height": 20,
            "twin_digest": "a" * 64,
            version_key: "talaria-live-capture-v2",
        }
        assert CAPTURE_METADATA_SCHEMA.validate(meta, path=Path("frame.json")) == []

        # SchemaRegistry.lookup resolves document with any of these fields
        doc = {version_key: "talaria-live-capture-v2"}
        assert SchemaRegistry.lookup(Path("frame.json"), doc) is CAPTURE_METADATA_SCHEMA


def test_host_sentinel_legacy_witnesses_format_refused(tmp_path: Path) -> None:
    sentinel_path = tmp_path / "live-22" / "evidence" / "sentinel.json"

    # 1. Document with legacy 'witnesses' list is strictly refused
    legacy_witnesses_doc = {
        "record_type": "host-sentinel",
        "schema_version": "talaria-v0.6.1-sentinel-v1",
        "checklist_item": "live-22",
        "case": "live-22",
        "read_by": "dedicated-tester",
        "read_at": "2026-09-06T18:00:00Z",
        "witnesses": [
            {
                "key": "ctrl+o",
                "before": "sentinel-idle",
                "after": "sentinel-idle",
                "positive_control": {"before": "sentinel-idle", "after": "sentinel-fired"},
            }
        ],
    }
    errs = HOST_SENTINEL_SCHEMA.validate(legacy_witnesses_doc, path=sentinel_path)
    assert any("undeclared key 'witnesses'" in e for e in errs)
    assert any("host-sentinel record requires 'keys' (quartet observations)" in e for e in errs)

    # 2. Document with legacy 'pairs' list is strictly refused
    legacy_pairs_doc = {
        "record_type": "host-sentinel",
        "schema_version": "talaria-v0.6.1-sentinel-v1",
        "checklist_item": "live-22",
        "case": "live-22",
        "read_by": "dedicated-tester",
        "read_at": "2026-09-06T18:00:00Z",
        "pairs": [],
    }
    errs = HOST_SENTINEL_SCHEMA.validate(legacy_pairs_doc, path=sentinel_path)
    assert any("undeclared key 'pairs'" in e for e in errs)
    assert any("host-sentinel record requires 'keys' (quartet observations)" in e for e in errs)

    # 3. Single-witness document with before/after/positive_control at root is strictly refused
    single_doc = {
        "record_type": "host-sentinel",
        "schema_version": "talaria-v0.6.1-sentinel-v1",
        "checklist_item": "live-22",
        "case": "live-22",
        "key": "ctrl+o",
        "before": "sentinel-idle",
        "after": "sentinel-idle",
        "positive_control": {
            "key": "ctrl+o",
            "before": "sentinel-idle",
            "after": "sentinel-fired",
        },
        "read_by": "dedicated-tester",
        "read_at": "2026-09-06T18:00:00Z",
    }
    errs = HOST_SENTINEL_SCHEMA.validate(single_doc, path=sentinel_path)
    assert any("undeclared key 'key'" in e for e in errs)
    assert any("undeclared key 'before'" in e for e in errs)
    assert any("undeclared key 'after'" in e for e in errs)
    assert any("undeclared key 'positive_control'" in e for e in errs)
    assert any("host-sentinel record requires 'keys' (quartet observations)" in e for e in errs)


def test_host_sentinel_content_free_constraints() -> None:
    path = Path("evidence/sentinel.json")

    # Constraint 1: pane or tab identifier is rejected as a forbidden or undeclared key
    for forbidden_field in ("pane_id", "tab_id", "multiplexer_pane", "window_id", "tokens"):
        doc_with_id = {
            "record_type": "host-sentinel-witness",
            "format_version": "talaria-host-sentinel-v1",
            "case": "live-22",
            "candidate_commit": "0" * 40,
            "read_by": "dedicated-tester",
            "read_at": "2026-09-06T18:00:00+00:00",
            "positive_controls_confirmed": True,
            "consumed_keys_stayed_idle": True,
            "keys": [],
            forbidden_field: "pane-01",
        }
        errs = HOST_SENTINEL_SCHEMA.validate(doc_with_id, path=path)
        assert any(
            f"forbidden key {forbidden_field!r}" in e or f"undeclared key {forbidden_field!r}" in e
            for e in errs
        )

    # Constraint 2: undeclared fields in item schema are refused
    doc_with_bad_item = {
        "record_type": "host-sentinel-witness",
        "format_version": "talaria-host-sentinel-v1",
        "case": "live-22",
        "candidate_commit": "0" * 40,
        "read_by": "dedicated-tester",
        "read_at": "2026-09-06T18:00:00+00:00",
        "positive_controls_confirmed": True,
        "consumed_keys_stayed_idle": True,
        "keys": [
            {
                "key": "ctrl+o",
                "consumed_before": "a.png",
                "consumed_after": "b.png",
                "control_before": "c.png",
                "control_after": "d.png",
                "arbitrary_extra": "leak",
            }
        ],
    }
    errs = HOST_SENTINEL_SCHEMA.validate(doc_with_bad_item, path=path)
    assert any("undeclared key 'arbitrary_extra'" in e for e in errs)


def _make_evidence_png(metadata_dict: dict[str, Any]) -> bytes:
    json_bytes = json.dumps(metadata_dict).encode("utf-8")
    text_data = b"talaria-evidence\x00" + json_bytes
    base = _base_png_chunks()
    return _make_png([base[0], (b"tEXt", text_data), base[1], base[2]])


def test_host_sentinel_quartet_proposal_validation(tmp_path: Path) -> None:
    evidence_dir = tmp_path / "docs" / "acceptance" / "v0.6.1" / "evidence" / "live-22"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    candidate_sha = "a" * 40
    read_at_str = "2026-09-06T18:00:00+00:00"

    key_slugs = [("ctrl+o", "ctrl-o"), ("ctrl+s", "ctrl-s"), ("f1", "f1"), ("f2", "f2")]
    phases = [
        ("consumed_before", "sentinel-idle"),
        ("consumed_after", "sentinel-idle"),
        ("control_before", "sentinel-idle"),
        ("control_after", "sentinel-fired"),
    ]

    keys_data: list[dict[str, Any]] = []
    minute_counter = 0

    for key, slug in key_slugs:
        key_entry: dict[str, Any] = {"key": key}
        for phase, expected_token in phases:
            stem = f"{slug}-{phase.replace('_', '-')}"
            png_name = f"{stem}.png"
            txt_name = f"{stem}.txt"
            json_name = f"{stem}.json"
            key_entry[phase] = png_name

            # Write text twin
            txt_path = evidence_dir / txt_name
            txt_path.write_text(expected_token + "\n", encoding="utf-8")
            twin_digest = hashlib.sha256(txt_path.read_bytes()).hexdigest()

            # Write sidecar JSON & embedded PNG
            minute_counter += 1
            captured_at = f"2026-09-06T17:{minute_counter:02d}:00+00:00"
            metadata = {
                "record_type": "capture-metadata",
                "format_version": "talaria-live-capture-v2",
                "case": "live-22",
                "candidate": {"commit_sha": candidate_sha},
                "frame": stem,
                "twin_digest": twin_digest,
                "captured_at": captured_at,
            }
            json_path = evidence_dir / json_name
            json_path.write_text(json.dumps(metadata), encoding="utf-8")

            png_path = evidence_dir / png_name
            png_path.write_bytes(_make_evidence_png(metadata))

        keys_data.append(key_entry)

    valid_quartet_doc: dict[str, Any] = {
        "record_type": "host-sentinel-witness",
        "format_version": "talaria-host-sentinel-v1",
        "case": "live-22",
        "checklist_item": "live-22",
        "candidate_commit": candidate_sha,
        "keys": keys_data,
        "read_by": "dedicated-tester",
        "read_at": read_at_str,
        "positive_controls_confirmed": True,
        "consumed_keys_stayed_idle": True,
    }

    witness_path = evidence_dir / "host-sentinel-witness.json"
    assert SchemaRegistry.lookup(witness_path, valid_quartet_doc) is HOST_SENTINEL_SCHEMA

    # 1. Complete tree passes validation cleanly
    assert HOST_SENTINEL_SCHEMA.validate(valid_quartet_doc, path=witness_path) == []
    witness_path.write_text(json.dumps(valid_quartet_doc), encoding="utf-8")
    assert evidence_file_privacy_errors(witness_path, repo_root=tmp_path) == []

    # 2. Deleting referenced trios is refused (mutation-held)
    missing_trio_doc = dict(valid_quartet_doc)
    fake_path = tmp_path / "empty_dir" / "host-sentinel-witness.json"
    fake_path.parent.mkdir(parents=True, exist_ok=True)
    fake_path.write_text(json.dumps(missing_trio_doc), encoding="utf-8")
    errs = HOST_SENTINEL_SCHEMA.validate(missing_trio_doc, path=fake_path)
    assert any(
        "referenced frame 'ctrl-o-consumed-before.png' does not exist" in e for e in errs
    )
    scan_errs = evidence_file_privacy_errors(fake_path, repo_root=tmp_path)
    assert any(
        "referenced frame 'ctrl-o-consumed-before.png' does not exist" in e for e in scan_errs
    )

    # 3. Missing confirming booleans is refused (mutation-held)
    no_bool_doc = copy.deepcopy(valid_quartet_doc)
    del no_bool_doc["positive_controls_confirmed"]
    del no_bool_doc["consumed_keys_stayed_idle"]
    no_bool_path = tmp_path / "no_bool" / "host-sentinel-witness.json"
    no_bool_path.parent.mkdir(parents=True, exist_ok=True)
    no_bool_path.write_text(json.dumps(no_bool_doc), encoding="utf-8")
    errs = HOST_SENTINEL_SCHEMA.validate(no_bool_doc, path=witness_path)
    assert any("'positive_controls_confirmed' must be explicitly true" in e for e in errs)
    assert any("'consumed_keys_stayed_idle' must be explicitly true" in e for e in errs)
    scan_errs = evidence_file_privacy_errors(no_bool_path, repo_root=tmp_path)
    assert any("'positive_controls_confirmed' must be explicitly true" in e for e in scan_errs)
    assert any("'consumed_keys_stayed_idle' must be explicitly true" in e for e in scan_errs)

    # 4. Arbitrary candidate commit is refused (mutation-held)
    bad_cand_doc = copy.deepcopy(valid_quartet_doc)
    bad_cand_doc["candidate_commit"] = "not-a-commit"
    bad_cand_path = tmp_path / "bad_cand" / "host-sentinel-witness.json"
    bad_cand_path.parent.mkdir(parents=True, exist_ok=True)
    bad_cand_path.write_text(json.dumps(bad_cand_doc), encoding="utf-8")
    errs = HOST_SENTINEL_SCHEMA.validate(bad_cand_doc, path=witness_path)
    assert any("must be a 40-character hexadecimal git commit SHA" in e for e in errs)
    scan_errs = evidence_file_privacy_errors(bad_cand_path, repo_root=tmp_path)
    assert any("must be a 40-character hexadecimal git commit SHA" in e for e in scan_errs)

    # 5. Reusing an observation reference across keys or phases is refused
    reused_keys: list[dict[str, Any]] = copy.deepcopy(keys_data)
    reused_keys[1]["consumed_before"] = "ctrl-o-consumed-before.png"
    reused_doc = dict(valid_quartet_doc, keys=reused_keys)
    errs = HOST_SENTINEL_SCHEMA.validate(reused_doc, path=witness_path)
    assert any("reused observation reference 'ctrl-o-consumed-before.png'" in e for e in errs)

    # 6. Missing one of the 4 mandatory keys for Live 22 is refused
    missing_keys: list[dict[str, Any]] = copy.deepcopy(keys_data)
    missing_key_doc = dict(valid_quartet_doc, keys=missing_keys[:3])  # drop f2
    errs = HOST_SENTINEL_SCHEMA.validate(missing_key_doc, path=witness_path)
    assert any("requires all 4 default keys" in e and "'f2'" in e for e in errs)

    # 7. Incomplete quartet missing an observation is refused
    incomplete_keys: list[dict[str, Any]] = copy.deepcopy(keys_data)
    del incomplete_keys[0]["control_after"]
    incomplete_quartet_doc = dict(valid_quartet_doc, keys=incomplete_keys)
    errs = HOST_SENTINEL_SCHEMA.validate(incomplete_quartet_doc, path=witness_path)
    assert any("incomplete quartet: missing observation 'control_after'" in e for e in errs)

    # 8. Empty root without keys or witnesses is refused
    empty_root_doc = {
        "record_type": "host-sentinel-witness",
        "format_version": "talaria-host-sentinel-v1",
        "case": "live-22",
        "positive_controls_confirmed": True,
        "consumed_keys_stayed_idle": True,
    }
    errs = HOST_SENTINEL_SCHEMA.validate(empty_root_doc, path=witness_path)
    assert any("host-sentinel record requires 'keys' (quartet observations)" in e for e in errs)


def test_live13_directory_equality_derivation_contract_and_mutations(tmp_path: Path) -> None:
    evidence_dir = tmp_path / "docs" / "acceptance" / "v0.6.1" / "evidence" / "live-13"
    evidence_dir.mkdir(parents=True, exist_ok=True)

    def _write_wire_file(name: str, frames: list[dict[str, Any]]) -> tuple[str, str]:
        p = evidence_dir / name
        header = {
            "kind": "header",
            "version": 1,
            "startedAt": "2026-09-06T12:00:00+00:00",
            "endpoint": "ws://127.0.0.1:8765/api/ws",
            "derivation": {
                "tool": "scripts/acceptance/v061_evidence.py derive-capture",
                "source_sha256": "a" * 64,
                "source_bytes": 500,
                "source_frames": len(frames),
                "selection": {"frame_types": None, "start_seq": 1, "end_seq": len(frames)},
                "rules": ["redact-cwd"],
                "derived_at": "2026-09-06T12:00:00+00:00",
            },
        }
        lines = [json.dumps(header)]
        for seq, f in enumerate(frames, start=1):
            lines.append(
                json.dumps(
                    {
                        "kind": "frame",
                        "seq": seq,
                        "sourceSeq": seq * 2,
                        "frame": f,
                        "redactions": [],
                    }
                )
            )
        text_content = "\n".join(lines) + "\n"
        p.write_text(text_content, encoding="utf-8")
        digest = hashlib.sha256(text_content.encode("utf-8")).hexdigest()
        return name, digest

    # Create 4 valid sibling derived wire capture files
    dummy_frame = {"type": "event", "data": "clean"}
    _, d_a_init = _write_wire_file("session-a-init.derived.jsonl", [dummy_frame, dummy_frame])
    _, d_b_init = _write_wire_file(
        "session-b-init.derived.jsonl",
        [dummy_frame, dummy_frame, dummy_frame, dummy_frame],
    )
    _, d_a_override = _write_wire_file(
        "session-a-override.derived.jsonl", [dummy_frame, dummy_frame]
    )
    _, d_a_reconnect = _write_wire_file(
        "session-a-reconnect.derived.jsonl", [dummy_frame, dummy_frame]
    )

    sources_data: dict[str, dict[str, Any]] = {
        "session-a-init": {
            "source_file": "session-a-init.wire.jsonl",
            "source_sha256": "1" * 64,
            "derived_file": "session-a-init.derived.jsonl",
            "derived_sha256": d_a_init,
        },
        "session-b-init": {
            "source_file": "session-b-init.wire.jsonl",
            "source_sha256": "2" * 64,
            "derived_file": "session-b-init.derived.jsonl",
            "derived_sha256": d_b_init,
        },
        "session-a-override": {
            "source_file": "session-a-override.wire.jsonl",
            "source_sha256": "3" * 64,
            "derived_file": "session-a-override.derived.jsonl",
            "derived_sha256": d_a_override,
        },
        "session-a-reconnect": {
            "source_file": "session-a-reconnect.wire.jsonl",
            "source_sha256": "4" * 64,
            "derived_file": "session-a-reconnect.derived.jsonl",
            "derived_sha256": d_a_reconnect,
        },
    }

    observations_data: dict[str, dict[str, Any]] = {
        "project-a-adoption": {
            "source_id": "session-a-init",
            "request_seq": 1,
            "reply_seq": 2,
            "request_source_seq": 2,
            "reply_source_seq": 4,
            "requested_equals_launch": True,
            "reported_equals_expected": True,
        },
        "project-a-tool": {
            "source_id": "session-a-init",
            "tool_start_seq": 1,
            "tool_complete_seq": 2,
            "pwd_equals_expected": True,
            "fixture_content_matches": True,
            "override_supplied": False,
        },
        "project-b-adoption": {
            "source_id": "session-b-init",
            "request_seq": 1,
            "reply_seq": 2,
            "request_source_seq": 2,
            "reply_source_seq": 4,
            "requested_equals_launch": True,
            "reported_equals_expected": True,
        },
        "project-b-tool": {
            "source_id": "session-b-init",
            "tool_start_seq": 1,
            "tool_complete_seq": 2,
            "pwd_equals_expected": True,
            "fixture_content_matches": True,
            "override_supplied": False,
        },
        "a-tool-override": {
            "source_id": "session-a-override",
            "tool_start_seq": 1,
            "tool_complete_seq": 2,
            "pwd_equals_expected": True,
            "fixture_content_matches": True,
            "override_supplied": True,
        },
        "b-after-a-override": {
            "source_id": "session-b-init",
            "tool_start_seq": 2,
            "tool_complete_seq": 4,
            "pwd_equals_expected": True,
            "fixture_content_matches": True,
            "override_supplied": False,
            "bounded_absence_session_cwd_set": True,
            "b_reported_cwd_unchanged": True,
        },
        "a-after-reconnect": {
            "source_id": "session-a-reconnect",
            "tool_start_seq": 1,
            "tool_complete_seq": 2,
            "pwd_equals_expected": True,
            "fixture_content_matches": True,
            "override_supplied": False,
        },
        "fresh-a": {
            "source_id": "session-a-init",
            "request_seq": 1,
            "reply_seq": 2,
            "request_source_seq": 2,
            "reply_source_seq": 4,
            "requested_equals_launch": True,
            "reported_equals_expected": True,
        },
        "resumed-a": {
            "source_id": "session-a-reconnect",
            "request_seq": 1,
            "reply_seq": 2,
            "request_source_seq": 2,
            "reply_source_seq": 4,
            "requested_equals_launch": True,
            "reported_equals_expected": True,
        },
        "resumed-a-tool": {
            "source_id": "session-a-reconnect",
            "tool_start_seq": 1,
            "tool_complete_seq": 2,
            "pwd_equals_expected": True,
            "fixture_content_matches": True,
            "override_supplied": False,
        },
    }

    valid_doc: dict[str, Any] = {
        "record_type": "directory-equality-derivation",
        "format_version": "talaria-directory-equality-v1",
        "case": "live-13",
        "checklist_item": "live-13",
        "candidate_commit": "e" * 40,
        "derived_by": "dedicated-tester",
        "derived_at": "2026-09-06T12:00:00+00:00",
        "status": "pass",
        "permission_semantics": "explicit-allow-all",
        "sources": sources_data,
        "observations": observations_data,
    }

    derivation_path = evidence_dir / "directory-equality-derivation.json"
    derivation_path.write_text(json.dumps(valid_doc, indent=2), encoding="utf-8")

    # 1. Conforming derivation passes all checks
    assert DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(valid_doc, path=derivation_path) == []
    assert validate_directory_equality_derivation(valid_doc, path=derivation_path) == []
    assert evidence_file_privacy_errors(derivation_path, repo_root=tmp_path) == []
    assert (
        SchemaRegistry.lookup(derivation_path, valid_doc)
        is DIRECTORY_EQUALITY_DERIVATION_SCHEMA
    )
    assert classify_evidence_file(derivation_path) == "directory-equality-derivation"

    # 2. Strict privacy check: rejects absolute filesystem paths
    bad_path_doc = copy.deepcopy(valid_doc)
    bad_path_doc["permission_semantics"] = "allow /private/var/folders/secret/path"
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(bad_path_doc, path=derivation_path)
    assert any("discloses absolute path" in e for e in errs)

    bad_path_obs = copy.deepcopy(valid_doc)
    bad_path_obs["observations"]["project-a-adoption"]["leaked_path"] = "/Users/jefcox/project"
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(bad_path_obs, path=derivation_path)
    assert any("discloses absolute path" in e for e in errs)

    # 3. Missing mandatory source refused (mutation-held)
    bad_src = copy.deepcopy(valid_doc)
    del bad_src["sources"]["session-a-reconnect"]
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(bad_src, path=derivation_path)
    assert any("Live 13 derivation requires all 4 mandatory sources" in e for e in errs)
    assert any("'session-a-reconnect'" in e for e in errs)

    # 4. Undeclared source refused
    extra_src = copy.deepcopy(valid_doc)
    extra_src["sources"]["session-c-init"] = copy.deepcopy(
        extra_src["sources"]["session-a-init"]
    )
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(extra_src, path=derivation_path)
    assert any("Live 13 derivation contains undeclared sources" in e for e in errs)

    # 5. Missing mandatory scenario stage refused (mutation-held)
    bad_stage = copy.deepcopy(valid_doc)
    del bad_stage["observations"]["b-after-a-override"]
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(bad_stage, path=derivation_path)
    assert any("Live 13 derivation requires all 10 mandatory scenario stages" in e for e in errs)
    assert any("'b-after-a-override'" in e for e in errs)

    # 6. Undeclared scenario stage refused
    extra_stage = copy.deepcopy(valid_doc)
    extra_stage["observations"]["rogue-stage"] = {"source_id": "session-a-init"}
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(extra_stage, path=derivation_path)
    assert any("Live 13 derivation contains undeclared scenario stages" in e for e in errs)

    # 7. One-sided adoption comparison refused (mutation-held)
    one_sided_adopt1 = copy.deepcopy(valid_doc)
    del one_sided_adopt1["observations"]["project-a-adoption"]["reported_equals_expected"]
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(one_sided_adopt1, path=derivation_path)
    assert any(
        "incomplete comparison: 'requested_equals_launch' present "
        "without 'reported_equals_expected'"
        in e
        for e in errs
    )

    one_sided_adopt2 = copy.deepcopy(valid_doc)
    del one_sided_adopt2["observations"]["project-a-adoption"]["requested_equals_launch"]
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(one_sided_adopt2, path=derivation_path)
    assert any(
        "incomplete comparison: 'reported_equals_expected' present "
        "without 'requested_equals_launch'"
        in e
        for e in errs
    )

    # 8. One-sided tool comparison refused (mutation-held)
    one_sided_tool1 = copy.deepcopy(valid_doc)
    del one_sided_tool1["observations"]["project-a-tool"]["pwd_equals_expected"]
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(one_sided_tool1, path=derivation_path)
    assert any(
        "incomplete comparison: 'fixture_content_matches' present without 'pwd_equals_expected'"
        in e
        for e in errs
    )

    one_sided_tool2 = copy.deepcopy(valid_doc)
    del one_sided_tool2["observations"]["project-a-tool"]["fixture_content_matches"]
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(one_sided_tool2, path=derivation_path)
    assert any(
        "incomplete comparison: 'pwd_equals_expected' present without 'fixture_content_matches'"
        in e
        for e in errs
    )

    # 9. Invariant omissions on b-after-a-override refused (mutation-held)
    no_bounded_absence = copy.deepcopy(valid_doc)
    del no_bounded_absence["observations"]["b-after-a-override"]["bounded_absence_session_cwd_set"]
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(no_bounded_absence, path=derivation_path)
    assert any("missing mandatory invariant 'bounded_absence_session_cwd_set'" in e for e in errs)

    no_b_unchanged = copy.deepcopy(valid_doc)
    del no_b_unchanged["observations"]["b-after-a-override"]["b_reported_cwd_unchanged"]
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(no_b_unchanged, path=derivation_path)
    assert any("missing mandatory invariant 'b_reported_cwd_unchanged'" in e for e in errs)

    # 10. Inverted sequence numbers refused (mutation-held)
    inverted_seq = copy.deepcopy(valid_doc)
    inverted_seq["observations"]["project-a-adoption"]["reply_seq"] = 1
    inverted_seq["observations"]["project-a-adoption"]["request_seq"] = 5
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(inverted_seq, path=derivation_path)
    assert any("reply_seq (1) cannot precede request_seq (5)" in e for e in errs)

    inverted_tool_seq = copy.deepcopy(valid_doc)
    inverted_tool_seq["observations"]["project-a-tool"]["tool_start_seq"] = 10
    inverted_tool_seq["observations"]["project-a-tool"]["tool_complete_seq"] = 2
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(inverted_tool_seq, path=derivation_path)
    assert any("tool_complete_seq (2) cannot precede tool_start_seq (10)" in e for e in errs)

    # 11. Sibling wire capture containing session.cwd.set in bounded window refused
    bad_b_frames: list[dict[str, Any]] = [
        dummy_frame,
        {"type": "event", "data": "clean"},
        {"type": "event", "method": "session.cwd.set", "params": {}},
        {"type": "event", "data": "clean"},
    ]
    _, bad_b_digest = _write_wire_file("session-b-bad.derived.jsonl", bad_b_frames)
    bad_b_doc = copy.deepcopy(valid_doc)
    bad_b_doc["sources"]["session-b-init"]["derived_file"] = "session-b-bad.derived.jsonl"
    bad_b_doc["sources"]["session-b-init"]["derived_sha256"] = bad_b_digest
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(bad_b_doc, path=derivation_path)
    assert any(
        "wire log 'session-b-bad.derived.jsonl' contains 'session.cwd.set' "
        "at seq 3 within bounded window [2, 4]"
        in e
        for e in errs
    )

    # 12. Missing referenced derived wire capture refused
    missing_wire_doc = copy.deepcopy(valid_doc)
    missing_wire_doc["sources"]["session-a-init"]["derived_file"] = "absent.derived.jsonl"
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(missing_wire_doc, path=derivation_path)
    assert any(
        "referenced derived wire capture 'absent.derived.jsonl' does not exist" in e
        for e in errs
    )

    # 13. SHA-256 digest mismatch on derived wire capture refused
    mismatched_sha_doc = copy.deepcopy(valid_doc)
    mismatched_sha_doc["sources"]["session-a-init"]["derived_sha256"] = "f" * 64
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(mismatched_sha_doc, path=derivation_path)
    assert any(
        "SHA-256 digest" in e and "does not match declared derived_sha256" in e
        for e in errs
    )

    # 14. Outcome coherence: pass status refused when equality predicate is false
    false_pred_doc = copy.deepcopy(valid_doc)
    false_pred_doc["observations"]["project-a-adoption"]["reported_equals_expected"] = False
    errs = DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(false_pred_doc, path=derivation_path)
    assert any(
        "status cannot be 'pass' when one or more equality predicates are false" in e
        for e in errs
    )

    # But failed status is permitted when an equality predicate is false (preserves failure reports)
    false_pred_doc["status"] = "failed"
    assert DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(false_pred_doc, path=derivation_path) == []

    # 15. generate_directory_equality_derivation helper creates conforming output
    gen_out = evidence_dir / "generated-derivation.json"
    written_path = v061_evidence.generate_directory_equality_derivation(
        output_path=gen_out,
        candidate_commit="e" * 40,
        sources=sources_data,
        observations=observations_data,
        status="pass",
        derived_by="dedicated-tester",
        derived_at="2026-09-06T12:00:00+00:00",
        permission_semantics="explicit-allow-all",
        case="live-13",
        checklist_item="live-13",
        repo_root=tmp_path,
    )
    assert written_path.is_file()
    assert evidence_file_privacy_errors(written_path, repo_root=tmp_path) == []

    # 16. Generator refuses invalid input and does not leak temporary files
    bad_gen_out = evidence_dir / "failed-generation.json"
    bad_sources_data = copy.deepcopy(sources_data)
    del bad_sources_data["session-a-init"]
    with pytest.raises(SystemExit) as excinfo:
        v061_evidence.generate_directory_equality_derivation(
            output_path=bad_gen_out,
            candidate_commit="e" * 40,
            sources=bad_sources_data,
            observations=observations_data,
            repo_root=tmp_path,
        )
    assert "Live 13 derivation requires all 4 mandatory sources" in str(excinfo.value)
    assert not bad_gen_out.exists()

    # 17. CLI derive-directory-equality execution
    sources_json_file = evidence_dir / "sources.json"
    sources_json_file.write_text(json.dumps(sources_data), encoding="utf-8")
    observations_json_file = evidence_dir / "observations.json"
    observations_json_file.write_text(json.dumps(observations_data), encoding="utf-8")
    cli_out = evidence_dir / "cli-derivation.json"
    exit_code = v061_evidence.main([
        "derive-directory-equality",
        "--candidate-commit", "e" * 40,
        "--sources-json", str(sources_json_file),
        "--observations-json", str(observations_json_file),
        "--output", str(cli_out),
        "--status", "pass",
        "--derived-by", "dedicated-tester",
        "--derived-at", "2026-09-06T12:00:00+00:00",
        "--repo-root", str(tmp_path),
    ])
    assert exit_code == 0
    assert cli_out.is_file()
    assert DIRECTORY_EQUALITY_DERIVATION_SCHEMA.validate(
        json.loads(cli_out.read_text(encoding="utf-8")), path=cli_out
    ) == []


def test_no_conflict_markers_in_repository() -> None:
    res = subprocess.run(
        ["git", "grep", "-nE", r"^(<{7}|={7}|>{7})( |$)", "--", "."],
        capture_output=True,
        text=True,
    )
    assert res.returncode != 0, f"Conflict markers found in repository:\n{res.stdout}"