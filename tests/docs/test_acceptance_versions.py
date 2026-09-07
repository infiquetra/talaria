"""Version-agnostic acceptance evidence validation.

Every ``docs/acceptance/v*/`` tree self-validates its own
manifest↔receipts↔results↔notes linkage:

- the manifest parses and validates against its ``$schema`` copy,
- every receipt path the manifest names exists with a matching digest,
- item verdicts tally to the manifest counts,
- declared results/notes documents exist and name the candidate.

Versions whose frozen manifest declares no results/notes pointers (v0.5.0)
validate on the manifest↔receipts half only; nothing here rewrites or
reinterprets frozen evidence, it only checks the linkage still holds.
"""

from __future__ import annotations

import hashlib
import json
from collections import Counter
from pathlib import Path
from typing import Any

import jsonschema
import pytest

from scripts.acceptance.versioning import acceptance_paths, version_from_tag

_REPO_ROOT = Path(__file__).resolve().parents[2]
_ACCEPTANCE_ROOT = _REPO_ROOT / "docs" / "acceptance"

_VERSION_DIRS = sorted(
    path for path in _ACCEPTANCE_ROOT.glob("v*") if path.is_dir()
)


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _manifest_or_skip(version_dir: Path) -> dict[str, object] | None:
    """The manifest, or None when the version is not yet recorded.

    A version directory without a manifest is "not yet recorded", not broken:
    schemas and tooling may land before the record flow runs. The release
    workflow fails loudly on a missing manifest at tag time, so the skip
    cannot leak into a release.
    """
    manifest_path = version_dir / "artifact-manifest.json"
    if not manifest_path.is_file():
        pytest.skip(f"{version_dir}: no artifact-manifest.json recorded yet")
    value = json.loads(manifest_path.read_text(encoding="utf-8"))
    assert isinstance(value, dict), f"{manifest_path}: manifest root is not an object"
    return value


def _manifest_entries(manifest: dict[str, object]) -> list[dict[str, object]]:
    entries: list[dict[str, object]] = []
    for key in ("receipts", "install_receipts", "gate0_receipts"):
        listed = manifest.get(key, [])
        assert isinstance(listed, list), f"manifest.{key} is not an array"
        for entry in listed:
            assert isinstance(entry, dict), f"manifest.{key} holds a non-object entry"
            entries.append(entry)
    return entries


@pytest.mark.parametrize("tag", ["v0.6.0", "v0.6.0-rc1", "v0.5.0", "0.6.0"])
def test_version_from_tag_strips_prefix_and_suffix(tag: str) -> None:
    """The Python mapping agrees with the release workflow's strip idiom."""
    assert version_from_tag(tag) == tag.lstrip("v").split("-", 1)[0]


def test_acceptance_paths_point_at_the_evidence_tree() -> None:
    """Manifest and evidence resolve under the version directory."""
    resolved = acceptance_paths("0.6.0")
    assert resolved.directory == _ACCEPTANCE_ROOT / "v0.6.0"
    assert resolved.manifest_path == resolved.directory / "artifact-manifest.json"
    assert resolved.evidence_root == resolved.directory / "evidence"


@pytest.mark.parametrize(
    "version_dir", _VERSION_DIRS, ids=[path.name for path in _VERSION_DIRS]
)
def test_manifest_validates_against_its_schema_copy(version_dir: Path) -> None:
    """Each manifest still satisfies the schema copy it ships with."""
    manifest = _manifest_or_skip(version_dir)
    assert manifest is not None
    schema_ref = manifest.get("$schema")
    assert isinstance(schema_ref, str), f"{version_dir}: manifest has no $schema"
    schema_path = version_dir / schema_ref
    assert schema_path.is_file(), f"{version_dir}: schema copy {schema_ref} missing"
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    jsonschema.validate(manifest, schema)


@pytest.mark.parametrize(
    "version_dir", _VERSION_DIRS, ids=[path.name for path in _VERSION_DIRS]
)
def test_every_manifest_receipt_exists_with_matching_digest(version_dir: Path) -> None:
    """Named receipts exist on disk with the recorded digest."""
    manifest = _manifest_or_skip(version_dir)
    assert manifest is not None
    entries = _manifest_entries(manifest)
    assert entries, f"{version_dir}: manifest names no receipts at all"
    for entry in entries:
        rel = entry.get("receipt_path")
        digest = entry.get("receipt_sha256")
        assert isinstance(rel, str) and rel, f"{version_dir}: entry without receipt_path"
        assert (
            isinstance(digest, str) and len(digest) == 64
        ), f"{rel}: entry without a 64-hex receipt_sha256"
        path = _REPO_ROOT / rel
        assert path.is_file(), f"{version_dir}: manifest names absent receipt {rel}"
        assert (
            _sha256_file(path) == digest
        ), f"{rel}: on-disk bytes do not match the manifest digest"


@pytest.mark.parametrize(
    "version_dir", _VERSION_DIRS, ids=[path.name for path in _VERSION_DIRS]
)
def test_item_verdicts_tally_to_manifest_counts(version_dir: Path) -> None:
    """The manifest counts describe exactly the named item receipts."""
    manifest = _manifest_or_skip(version_dir)
    assert manifest is not None
    counts = manifest.get("counts")
    assert isinstance(counts, dict), f"{version_dir}: manifest has no counts"
    expected = counts.get("item_verdicts")
    assert isinstance(expected, dict), f"{version_dir}: counts has no item_verdicts"
    receipts = manifest.get("receipts")
    assert isinstance(receipts, list), f"{version_dir}: manifest has no receipts array"
    tally = Counter(
        entry["verdict"] for entry in receipts if isinstance(entry, dict)
    )
    described = {key: tally.get(key, 0) for key in expected}
    assert described == dict(expected), (
        f"{version_dir}: verdict tally {described} != counts {dict(expected)}"
    )
    assert sum(tally.values()) == len(receipts), (
        f"{version_dir}: {len(receipts)} receipts but tally counts {sum(tally.values())}"
    )


@pytest.mark.parametrize(
    "version_dir", _VERSION_DIRS, ids=[path.name for path in _VERSION_DIRS]
)
def test_declared_results_and_notes_name_the_candidate(version_dir: Path) -> None:
    """Results/notes documents exist and bind the manifest's candidate."""
    manifest = _manifest_or_skip(version_dir)
    assert manifest is not None
    candidate = manifest.get("current_candidate", manifest.get("candidate"))
    assert isinstance(candidate, dict), f"{version_dir}: manifest names no candidate"
    commit = candidate.get("commit")
    assert isinstance(commit, str) and len(commit) == 40, (
        f"{version_dir}: candidate commit is not a full sha"
    )
    gate_id = manifest.get("gate_id")
    for key in ("results_document", "notes_document"):
        rel = manifest.get(key)
        if rel is None:
            continue
        assert isinstance(rel, str), f"{version_dir}: manifest.{key} is not a path"
        path = _REPO_ROOT / rel
        assert path.is_file(), f"{version_dir}: manifest.{key} points at {rel}, absent"
        body = path.read_text(encoding="utf-8")
        assert commit in body, f"{rel}: does not name candidate commit {commit[:12]}…"
        if isinstance(gate_id, str):
            assert gate_id in body or "gate" in body.lower(), (
                f"{rel}: names neither the gate id nor any gate"
            )


def test_talaria_live_capture_v2_vocabulary_registration() -> None:
    """talaria-live-capture-v2 is registered exclusively under format_version.

    A stale pre-split branch once silently widened CAPTURE_METADATA_SCHEMA to
    admit schema and schema_version aliases, while erasing the sole-path test in
    test_v061_lineage.py. This second assertion in a separate module ensures that
    re-adding the aliases fails outside the two files carried by that merge hunk.
    """
    from scripts.acceptance.v050_receipt import CAPTURE_METADATA_SCHEMA

    assert "talaria-live-capture-v2" in CAPTURE_METADATA_SCHEMA.vocabularies["format_version"]
    assert "talaria-live-capture-v2" not in CAPTURE_METADATA_SCHEMA.vocabularies.get(
        "schema_version", ()
    )
    assert "talaria-live-capture-v2" not in CAPTURE_METADATA_SCHEMA.vocabularies.get("schema", ())


def test_v061_manifest_schema_checklist_item_bounds() -> None:
    """The v0.6.1 manifest schema accepts items through live-23 and rejects live-24."""
    schema_path = _ACCEPTANCE_ROOT / "v0.6.1" / "artifact-manifest.schema.json"
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    item_subschema = schema["properties"]["receipts"]["items"]["properties"]["checklist_item"]

    validator = jsonschema.Draft202012Validator(item_subschema)

    for num in (1, 9, 10, 19, 20, 21, 22, 23):
        assert validator.is_valid(f"live-{num:02d}"), f"live-{num:02d} should be valid"

    for invalid in ("live-00", "live-24", "live-25", "live-1", "live-001"):
        assert not validator.is_valid(invalid), f"{invalid} should be rejected"


def test_v061_receipt_schema_checklist_item_bounds() -> None:
    """The v0.6.1 receipt schema accepts items through live-23 and rejects live-24."""
    schema_path = _ACCEPTANCE_ROOT / "v0.6.1" / "receipt.schema.json"
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    item_subschema = schema["properties"]["checklist_item"]

    validator = jsonschema.Draft202012Validator(item_subschema)

    for num in (1, 9, 10, 19, 20, 21, 22, 23):
        assert validator.is_valid(f"live-{num:02d}"), f"live-{num:02d} should be valid"

    for invalid in ("live-00", "live-24", "live-25", "live-1", "live-001"):
        assert not validator.is_valid(invalid), f"{invalid} should be rejected"


def test_v061_manifest_document_validates_against_shipped_schema() -> None:
    """A complete manifest containing live-22 and live-23 satisfies the schema copy."""
    schema_path = _ACCEPTANCE_ROOT / "v0.6.1" / "artifact-manifest.schema.json"
    schema = json.loads(schema_path.read_text(encoding="utf-8"))

    receipts: list[dict[str, Any]] = [
        {
            "receipt_path": f"docs/acceptance/v0.6.1/evidence/live-{i:02d}/receipt.json",
            "receipt_sha256": "c" * 64,
            "checklist_item": f"live-{i:02d}",
            "tester": "dedicated-tester",
            "verdict": "pass",
            "candidate_commit_sha": "a" * 40,
            "applies_to_candidate": "same",
        }
        for i in range(1, 24)
    ]
    manifest: dict[str, Any] = {
        "$schema": "./artifact-manifest.schema.json",
        "schema_version": "talaria-v0.6.1-artifact-manifest-v1",
        "gate_id": "v0-6-1-daily-driver",
        "generated_command": "recorded for test",
        "status": "complete",
        "recorded_at": "2026-09-07T00:00:00+00:00",
        "harness_commit": "a" * 40,
        "candidate": {
            "commit": "a" * 40,
            "version": "0.6.1",
            "wheel_filename": "talaria-0.6.1-py3-none-any.whl",
            "wheel_sha256": "d" * 64,
        },
        "counts": {
            "expected_receipts": 23,
            "install_receipts": 1,
            "item_receipts": 23,
            "item_verdicts": {"blocked": 0, "fail": 0, "pass": 23, "reserved": 0},
            "invalid_item_receipts": 0,
        },
        "receipts": receipts,
        "install_receipts": [
            {
                "receipt_path": "docs/acceptance/v0.6.1/evidence/probe-1/install-receipt.json",
                "receipt_sha256": "e" * 64,
                "tester": "operator",
            }
        ],
        "results_document": "docs/acceptance/v0.6.1/results.md",
        "notes_document": "docs/acceptance/v0.6.1/notes.md",
    }

    jsonschema.validate(manifest, schema)

    # Mutation: adding live-24 fails validation
    receipts.append(
        {
            "receipt_path": "docs/acceptance/v0.6.1/evidence/live-24/receipt.json",
            "receipt_sha256": "c" * 64,
            "checklist_item": "live-24",
            "tester": "dedicated-tester",
            "verdict": "pass",
            "candidate_commit_sha": "a" * 40,
            "applies_to_candidate": "same",
        }
    )
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate(manifest, schema)
