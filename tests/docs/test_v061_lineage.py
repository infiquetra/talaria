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

import hashlib
import json
import subprocess
from pathlib import Path
from typing import Any

import pytest

from scripts.acceptance import v061_evidence
from scripts.acceptance.v050_receipt import (
    V061_ITEM_SCHEMA,
    V061_ROLE_LABELS,
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
_FRAME_ONE = b"\x89PNG fake frame one"


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
        {"checklist_item": "live-22"},
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


def test_the_generator_refuses_a_tree_that_has_not_been_bumped() -> None:
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
    monkeypatch.setattr(v061_evidence, "_package_version", lambda: "0.6.1")

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
    assert "twenty source-checkout receipts" in results_body
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