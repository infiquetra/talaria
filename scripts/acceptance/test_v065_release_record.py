"""v0.6.5 verify_run contract.

Fixture-only except the committed-record placeholder. Teaching verify_run
``talaria-v0.6.5-receipt-v1`` and ``v0-6-5-configuration-ui-residuals``
flips the schema/gate tests. Do not invent the real record here.
"""

from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Any

from scripts.acceptance.v050_common import active_receipt_paths, sha256_file
from scripts.acceptance.v050_receipt import verify_run

V065_RECEIPT_SCHEMA = "talaria-v0.6.5-receipt-v1"
V065_MANIFEST_SCHEMA = "talaria-v0.6.5-artifact-manifest-v1"
_WHEEL = "b" * 64
_CFG_ITEM = "cfg-p4"
_GATE_ID = "v0-6-5-configuration-ui-residuals"
_FORBIDDEN_GATE_IDS = frozenset(
    {
        "v0-6-4-configuration-ui-residuals",
        "v0-6-3-configuration-residuals",
        "v0-6-2-configuration",
        "v0-6-1-daily-driver",
    }
)
_PRODUCT_SHA = "a83238541bb0228f18e4ef3a911bf238f7b7d0bd"


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


def _v065_receipt(*, item: str = _CFG_ITEM, verdict: str = "pass") -> dict[str, Any]:
    return {
        "schema_version": V065_RECEIPT_SCHEMA,
        "release": "0.6.5",
        "checklist_item": item,
        "tester": "dedicated-tester",
        "verdict": verdict,
    }


def _write_v065_run(
    repo: Path,
    *,
    candidate_commit: str,
    gate_id: str = _GATE_ID,
    receipts: list[tuple[str, dict[str, Any]]] | None = None,
) -> tuple[Path, Path]:
    evidence_root = repo / "docs" / "acceptance" / "v0.6.5" / "evidence"
    named: list[dict[str, Any]] = []
    for relative, receipt in receipts or [("cfg-p4/receipts/receipt.json", _v065_receipt())]:
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
    manifest = {
        "schema_version": V065_MANIFEST_SCHEMA,
        "gate_id": gate_id,
        "candidate": {
            "commit": candidate_commit,
            "version": "0.6.5",
            "wheel_filename": "talaria-0.6.5-py3-none-any.whl",
            "wheel_sha256": _WHEEL,
        },
        "counts": {
            "item_receipts": len(named),
            "install_receipts": 0,
            "item_verdicts": {"pass": len(named), "fail": 0, "blocked": 0, "reserved": 0},
        },
        "receipts": named,
        "install_receipts": [],
    }
    manifest_path = repo / "docs" / "acceptance" / "v0.6.5" / "artifact-manifest.json"
    _write_json(manifest_path, manifest)
    return manifest_path, evidence_root


def test_verify_run_accepts_named_v065_receipt_schema(tmp_path: Path) -> None:
    """Teaching verify_run ``talaria-v0.6.5-receipt-v1`` flips this test.

    Today the same fixture is rejected as an unknown schema. The assertion
    names that gap; do not weaken it to expect the unknown-schema string.
    """
    product = _init_repo(tmp_path)
    manifest_path, evidence_root = _write_v065_run(tmp_path, candidate_commit=product)
    found = active_receipt_paths(evidence_root)
    assert found, (
        "fixture bug: the v0.6.5 receipt was not enumerated by active_receipt_paths"
    )

    errors = verify_run(
        manifest_path,
        evidence_root=evidence_root,
        repo_root=tmp_path,
    )

    assert not any(
        "unknown receipt schema_version" in error and V065_RECEIPT_SCHEMA in error
        for error in errors
    ), errors


def test_verify_run_accepts_docs_only_descendant_of_product_sha(tmp_path: Path) -> None:
    """Tag SHA may be a docs-only descendant of product SHA a832385."""
    product = _init_repo(tmp_path)
    manifest_path, evidence_root = _write_v065_run(tmp_path, candidate_commit=product)
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
    assert not any(
        "unknown receipt schema_version" in error and V065_RECEIPT_SCHEMA in error
        for error in errors
    ), errors


def test_verify_run_rejects_talaria_drift_from_product_sha(tmp_path: Path) -> None:
    product = _init_repo(tmp_path)
    manifest_path, evidence_root = _write_v065_run(tmp_path, candidate_commit=product)
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


def test_verify_run_rejects_prior_gate_id_on_a_v065_manifest(tmp_path: Path) -> None:
    product = _init_repo(tmp_path)
    for prior in sorted(_FORBIDDEN_GATE_IDS):
        manifest_path, evidence_root = _write_v065_run(
            tmp_path,
            candidate_commit=product,
            gate_id=prior,
        )
        errors = verify_run(
            manifest_path,
            evidence_root=evidence_root,
            repo_root=tmp_path,
        )
        assert not any(
            "unknown receipt schema_version" in error and V065_RECEIPT_SCHEMA in error
            for error in errors
        ), errors
        assert any(
            "must not reuse" in error or prior in error
            for error in errors
        ), errors


def test_verify_run_rejects_non_cfg_p4_checklist_item(tmp_path: Path) -> None:
    """Closed v0.6.5 vocab is cfg-p4 only."""
    product = _init_repo(tmp_path)
    manifest_path, evidence_root = _write_v065_run(
        tmp_path,
        candidate_commit=product,
        receipts=[("cfg-p3/receipts/receipt.json", _v065_receipt(item="cfg-p3"))],
    )
    errors = verify_run(
        manifest_path,
        evidence_root=evidence_root,
        repo_root=tmp_path,
    )
    assert not any(
        "unknown receipt schema_version" in error and V065_RECEIPT_SCHEMA in error
        for error in errors
    ), errors
    assert any(
        "cfg-p4" in error or "checklist_item" in error
        for error in errors
    ), errors


def test_verify_run_is_clean_for_committed_v065_record() -> None:
    """Publication gate once a later owner writes the v0.6.5 record.

    Encode the missing files. Do not invent the record.
    """
    repo_root = Path(__file__).resolve().parents[2]
    version_dir = repo_root / "docs" / "acceptance" / "v0.6.5"
    required = (
        version_dir / "artifact-manifest.json",
        version_dir / "artifact-manifest.schema.json",
        version_dir / "evidence",
    )
    missing = [
        path.relative_to(repo_root).as_posix()
        for path in required
        if not path.exists()
    ]
    assert not missing, (
        "v0.6.5 release record is missing: "
        + ", ".join(missing)
        + "; later owner writes the record for verify-run --expect-candidate HEAD"
    )

    errors = verify_run(
        version_dir / "artifact-manifest.json",
        evidence_root=version_dir / "evidence",
        repo_root=repo_root,
        expected_candidate_commit=_head(repo_root),
    )
    assert errors == [], errors
    assert _PRODUCT_SHA == "a83238541bb0228f18e4ef3a911bf238f7b7d0bd"
