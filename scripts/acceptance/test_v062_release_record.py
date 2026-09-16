"""v0.6.2 verify_run contract.

The first six tests name the receipt schema and pin
``_release_candidate_matches``. The cleanliness tests require an empty
error list for a real CFG receipt shape and for the committed record —
``release.yml`` fails the tag if ``verify-run`` prints any error.
"""

from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Any

from scripts.acceptance.v050_common import active_receipt_paths, sha256_file
from scripts.acceptance.v050_receipt import verify_run

V062_RECEIPT_SCHEMA = "talaria-v0.6.2-receipt-v1"
V061_RECEIPT_SCHEMA = "talaria-v0.6.1-receipt-v1"
_WHEEL = "b" * 64
_CFG_ITEM = "cfg-cr4"
_GATE_ID = "v0-6-2-configuration"


def _git(repo: Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=repo, check=True, capture_output=True, text=True)


def _head(repo: Path) -> str:
    return subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def _init_repo(root: Path) -> str:
    _git(root, "init", "-q")
    _git(root, "config", "user.email", "acceptance@example.invalid")
    _git(root, "config", "user.name", "Acceptance")
    (root / "talaria").mkdir()
    (root / "talaria" / "module.py").write_text("value = 1\n", encoding="utf-8")
    (root / "pyproject.toml").write_text("[project]\nname = 'talaria'\n", encoding="utf-8")
    (root / "uv.lock").write_text("# lock\n", encoding="utf-8")
    (root / "src").mkdir()
    (root / "src" / "keep.txt").write_text("reference\n", encoding="utf-8")
    _git(root, "add", ".")
    _git(root, "commit", "-qm", "product")
    return _head(root)


def _write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")


def _v062_receipt(*, item: str = _CFG_ITEM, verdict: str = "pass") -> dict[str, Any]:
    return {
        "schema_version": V062_RECEIPT_SCHEMA,
        "release": "0.6.2",
        "checklist_item": item,
        "tester": "dedicated-tester",
        "verdict": verdict,
    }


def _v062_cfg_shape_receipt() -> dict[str, Any]:
    """The published CFG shape: applies_to_candidate plus one evidence digest."""
    return {
        "schema_version": V062_RECEIPT_SCHEMA,
        "release": "0.6.2",
        "checklist_item": _CFG_ITEM,
        "tester": "dedicated-tester",
        "verdict": "pass",
        "applies_to_candidate": "same",
        "evidence": {
            "review_artifact_sha256": "a" * 64,
        },
    }


def _write_v062_run(
    repo: Path,
    *,
    candidate_commit: str,
    gate_id: str = _GATE_ID,
    receipts: list[tuple[str, dict[str, Any]]] | None = None,
    expected_receipts: int | None = None,
) -> tuple[Path, Path]:
    evidence_root = repo / "docs" / "acceptance" / "v0.6.2" / "evidence"
    named: list[dict[str, Any]] = []
    for relative, receipt in receipts or [("cfg-cr4/receipts/receipt.json", _v062_receipt())]:
        path = evidence_root / relative
        _write_json(path, receipt)
        named.append(
            {
                "receipt_path": path.relative_to(repo).as_posix(),
                "receipt_sha256": sha256_file(path),
                "checklist_item": receipt.get("checklist_item"),
                "tester": receipt.get("tester"),
                "verdict": receipt.get("verdict"),
            }
        )
    counts: dict[str, Any] = {
        "item_receipts": len(named),
        "install_receipts": 0,
        "item_verdicts": {"pass": len(named), "fail": 0, "blocked": 0, "reserved": 0},
    }
    if expected_receipts is not None:
        counts["expected_receipts"] = expected_receipts
    manifest = {
        "schema_version": "talaria-v0.6.2-artifact-manifest-v1",
        "gate_id": gate_id,
        "candidate": {
            "commit": candidate_commit,
            "version": "0.6.2",
            "wheel_filename": "talaria-0.6.2-py3-none-any.whl",
            "wheel_sha256": _WHEEL,
        },
        "counts": counts,
        "receipts": named,
        "install_receipts": [],
    }
    manifest_path = repo / "docs" / "acceptance" / "v0.6.2" / "artifact-manifest.json"
    _write_json(manifest_path, manifest)
    return manifest_path, evidence_root


def test_verify_run_accepts_named_v062_receipt_schema(tmp_path: Path) -> None:
    """Teaching verify_run ``talaria-v0.6.2-receipt-v1`` flips this test.

    Today the same fixture is rejected as an unknown schema. The assertion
    names that gap; do not weaken it to expect the unknown-schema string.
    """
    product = _init_repo(tmp_path)
    manifest_path, evidence_root = _write_v062_run(tmp_path, candidate_commit=product)
    found = active_receipt_paths(evidence_root)
    assert found, (
        "fixture bug: the v0.6.2 receipt was not enumerated by active_receipt_paths"
    )

    errors = verify_run(
        manifest_path,
        evidence_root=evidence_root,
        repo_root=tmp_path,
    )

    assert not any(
        "unknown receipt schema_version" in error and V062_RECEIPT_SCHEMA in error
        for error in errors
    ), errors


def test_verify_run_accepts_docs_only_descendant_of_product_sha(tmp_path: Path) -> None:
    """Tag SHA may be a docs-only descendant; product SHA stays on the candidate."""
    product = _init_repo(tmp_path)
    manifest_path, evidence_root = _write_v062_run(tmp_path, candidate_commit=product)
    (tmp_path / "docs-note.txt").write_text("record only\n", encoding="utf-8")
    _git(tmp_path, "add", ".")
    _git(tmp_path, "commit", "-qm", "documentation only")
    descendant = _head(tmp_path)

    errors = verify_run(
        manifest_path,
        evidence_root=evidence_root,
        repo_root=tmp_path,
        expected_candidate_commit=descendant,
    )

    assert not any(
        "does not describe the released commit" in error for error in errors
    ), errors


def test_verify_run_rejects_talaria_drift_from_product_sha(tmp_path: Path) -> None:
    product = _init_repo(tmp_path)
    manifest_path, evidence_root = _write_v062_run(tmp_path, candidate_commit=product)
    (tmp_path / "talaria" / "module.py").write_text("value = 2\n", encoding="utf-8")
    _git(tmp_path, "add", ".")
    _git(tmp_path, "commit", "-qm", "product change")
    drifted = _head(tmp_path)

    errors = verify_run(
        manifest_path,
        evidence_root=evidence_root,
        repo_root=tmp_path,
        expected_candidate_commit=drifted,
    )

    assert any("release-relevant files differ" in error for error in errors), errors


def test_verify_run_rejects_v061_gate_id_on_a_v062_manifest(tmp_path: Path) -> None:
    product = _init_repo(tmp_path)
    manifest_path, evidence_root = _write_v062_run(
        tmp_path,
        candidate_commit=product,
        gate_id="v0-6-1-daily-driver",
    )

    errors = verify_run(
        manifest_path,
        evidence_root=evidence_root,
        repo_root=tmp_path,
    )

    assert any(
        "gate_id must be v0-6-2-configuration" in error
        or "must not reuse v0-6-1-daily-driver" in error
        for error in errors
    ), errors


def test_verify_run_rejects_v061_live_receipt_on_a_v062_manifest(tmp_path: Path) -> None:
    product = _init_repo(tmp_path)
    live = {
        "schema_version": V061_RECEIPT_SCHEMA,
        "release": "0.6.1",
        "checklist_item": "live-01",
        "tester": "dedicated-tester",
        "verdict": "pass",
    }
    manifest_path, evidence_root = _write_v062_run(
        tmp_path,
        candidate_commit=product,
        receipts=[("live-01/receipt.json", live)],
    )

    errors = verify_run(
        manifest_path,
        evidence_root=evidence_root,
        repo_root=tmp_path,
    )

    assert any(
        "must not reuse a v0.6.1 live receipt" in error
        or "v0.6.1 live-NN receipt is not a v0.6.2 CFG record" in error
        for error in errors
    ), errors


def test_v061_expected_receipts_ready_rule_does_not_apply_to_v062_cfg_set(
    tmp_path: Path,
) -> None:
    """Leftover expected_receipts / live-NN READY-waiver must not grade a CFG set."""
    product = _init_repo(tmp_path)
    live = {
        "schema_version": V061_RECEIPT_SCHEMA,
        "release": "0.6.1",
        "checklist_item": "live-01",
        "tester": "dedicated-tester",
        "verdict": "blocked",
    }
    manifest_path, evidence_root = _write_v062_run(
        tmp_path,
        candidate_commit=product,
        receipts=[("live-01/receipt.json", live)],
        expected_receipts=23,
    )

    errors = verify_run(
        manifest_path,
        evidence_root=evidence_root,
        repo_root=tmp_path,
    )

    assert not any("READY rule has no waiver" in error for error in errors), errors
    assert not any(
        "live receipts on disk" in error and "expected_receipts" in error
        for error in errors
    ), errors


def test_verify_run_is_clean_for_v062_cfg_receipt_shape(tmp_path: Path) -> None:
    """A real CFG receipt must produce no verify_run errors.

    Do not weaken this to ignore SchemaRegistry vocabulary or undeclared-key
    errors. Teaching the registry the v0.6.2 receipt is how this flips.
    """
    product = _init_repo(tmp_path)
    manifest_path, evidence_root = _write_v062_run(
        tmp_path,
        candidate_commit=product,
        receipts=[("cfg-cr4/receipts/receipt.json", _v062_cfg_shape_receipt())],
    )

    errors = verify_run(
        manifest_path,
        evidence_root=evidence_root,
        repo_root=tmp_path,
    )

    assert errors == [], errors


def test_verify_run_is_clean_for_committed_v062_record() -> None:
    """Publication gate: verify-run --expect-candidate HEAD must print no errors."""
    repo_root = Path(__file__).resolve().parents[2]
    errors = verify_run(
        repo_root / "docs" / "acceptance" / "v0.6.2" / "artifact-manifest.json",
        evidence_root=repo_root / "docs" / "acceptance" / "v0.6.2" / "evidence",
        repo_root=repo_root,
        expected_candidate_commit=_head(repo_root),
    )

    assert errors == [], errors
