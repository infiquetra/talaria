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
