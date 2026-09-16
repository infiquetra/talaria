"""v0.6.2 release-record contract against the real tree.

These tests are the implementer's spec. They must not skip when the
v0.6.2 manifest is absent: a missing record is a failed release, not a
version that is "not yet recorded". ``tests/docs/test_acceptance_versions.py``
skips that case on purpose; this module exists so the skip cannot leak
into the tag that ``release.yml`` will bind.
"""

from __future__ import annotations

import hashlib
import json
import re
from collections import Counter
from datetime import date
from pathlib import Path
from typing import Any

import jsonschema

_REPO_ROOT = Path(__file__).resolve().parents[2]
_VERSION_DIR = _REPO_ROOT / "docs" / "acceptance" / "v0.6.2"
_MANIFEST_PATH = _VERSION_DIR / "artifact-manifest.json"
_SCHEMA_PATH = _VERSION_DIR / "artifact-manifest.schema.json"
_ANALYSIS_ROOT = _REPO_ROOT / "docs" / "analysis"
_V061_VERDICT = _ANALYSIS_ROOT / "2026-09-05-v0-6-1-daily-driver-verdict.md"

_PRODUCT_SHA = "fbaa534622c26fc3f1e370cc6d2d9b057bb2f5e0"
_MANIFEST_SCHEMA_VERSION = "talaria-v0.6.2-artifact-manifest-v1"
_GATE_ID = "v0-6-2-configuration"
_FORBIDDEN_GATE_IDS = frozenset(
    {
        "v0-6-1-daily-driver",
        "v0-6-daily-driver",
        "v0-1-daily-driver",
    }
)
_CFG_CHECKLIST_IDS = frozenset(
    {
        "cfg-cr4",
        "cfg-project-check",
        "cfg-t6",
        "cfg-cleanup",
    }
)
_LIVE_ITEM = re.compile(r"^live-\d{2}$")
_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_GATE_BLOCK = re.compile(r"^```gate\n(.*?)^```", re.MULTILINE | re.DOTALL)
_EMPHASIZED = re.compile(r"\*\*(.+?)\*\*")


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _load_manifest() -> dict[str, Any]:
    """Load the v0.6.2 manifest or fail. Never skip."""
    assert _MANIFEST_PATH.is_file(), (
        f"v0.6.2 release record is missing: {_MANIFEST_PATH.relative_to(_REPO_ROOT)}"
    )
    value = json.loads(_MANIFEST_PATH.read_text(encoding="utf-8"))
    assert isinstance(value, dict), f"{_MANIFEST_PATH}: manifest root is not an object"
    return value


def _candidate(manifest: dict[str, Any]) -> dict[str, Any]:
    candidate = manifest.get("candidate")
    assert isinstance(candidate, dict), "manifest names no candidate object"
    return candidate


def _receipt_entries(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    receipts = manifest.get("receipts")
    assert isinstance(receipts, list), "manifest.receipts is not an array"
    entries: list[dict[str, Any]] = []
    for entry in receipts:
        assert isinstance(entry, dict), "manifest.receipts holds a non-object entry"
        entries.append(entry)
    return entries


def _gate_fields(body: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    for raw_line in body.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        key, separator, value = line.partition(":")
        if not separator or key.strip() == "blocks-on":
            continue
        fields[key.strip()] = value.strip()
    return fields


def test_v062_manifest_exists_and_validates_against_sibling_schema() -> None:
    """The tagged tree must ship a schema-valid v0.6.2 manifest."""
    assert _MANIFEST_PATH.is_file(), (
        f"v0.6.2 release record is missing: {_MANIFEST_PATH.relative_to(_REPO_ROOT)}"
    )
    assert _SCHEMA_PATH.is_file(), (
        f"v0.6.2 manifest schema is missing: {_SCHEMA_PATH.relative_to(_REPO_ROOT)}"
    )
    manifest = _load_manifest()
    schema = json.loads(_SCHEMA_PATH.read_text(encoding="utf-8"))
    assert isinstance(schema, dict), f"{_SCHEMA_PATH}: schema root is not an object"
    jsonschema.validate(manifest, schema)


def test_v062_manifest_schema_version_is_named() -> None:
    manifest = _load_manifest()
    assert manifest.get("schema_version") == _MANIFEST_SCHEMA_VERSION, (
        f"manifest.schema_version must be {_MANIFEST_SCHEMA_VERSION!r}, "
        f"got {manifest.get('schema_version')!r}"
    )


def test_v062_manifest_gate_id_is_configuration_not_a_prior_daily_driver() -> None:
    manifest = _load_manifest()
    gate_id = manifest.get("gate_id")
    assert gate_id not in _FORBIDDEN_GATE_IDS, (
        f"manifest.gate_id {gate_id!r} reuses a prior release's gate"
    )
    assert gate_id == _GATE_ID, (
        f"manifest.gate_id must be {_GATE_ID!r}, got {gate_id!r}"
    )


def test_v062_candidate_commit_is_the_accepted_product_sha() -> None:
    candidate = _candidate(_load_manifest())
    assert candidate.get("commit") == _PRODUCT_SHA, (
        f"candidate.commit must be the accepted product SHA {_PRODUCT_SHA}, "
        f"got {candidate.get('commit')!r}"
    )


def test_v062_candidate_version_is_0_6_2() -> None:
    candidate = _candidate(_load_manifest())
    assert candidate.get("version") == "0.6.2", (
        f"candidate.version must be '0.6.2', got {candidate.get('version')!r}"
    )


def test_v062_candidate_names_a_wheel_without_a_hard_coded_digest() -> None:
    """The implementer builds the wheel from the product SHA; the digest is not a fixture."""
    candidate = _candidate(_load_manifest())
    filename = candidate.get("wheel_filename")
    digest = candidate.get("wheel_sha256")
    assert isinstance(filename, str) and filename.strip(), (
        "candidate.wheel_filename must be a non-empty string"
    )
    assert isinstance(digest, str) and _HEX64.fullmatch(digest), (
        "candidate.wheel_sha256 must be a 64-character lowercase hex digest"
    )


def test_v062_receipts_are_the_cfg_record_ids_not_live_nn() -> None:
    entries = _receipt_entries(_load_manifest())
    assert entries, "manifest.receipts must be non-empty"
    items: set[str] = set()
    for entry in entries:
        item = entry.get("checklist_item")
        assert isinstance(item, str) and item, "receipt entry missing checklist_item"
        assert not _LIVE_ITEM.fullmatch(item), (
            f"checklist_item {item!r} is a v0.6.1 live-NN id; "
            f"v0.6.2 names CFG record ids {_CFG_CHECKLIST_IDS}"
        )
        assert item in _CFG_CHECKLIST_IDS, (
            f"checklist_item {item!r} is not one of {_CFG_CHECKLIST_IDS}"
        )
        items.add(item)
    assert items == _CFG_CHECKLIST_IDS, (
        f"named checklist_item values {sorted(items)} != required CFG set "
        f"{sorted(_CFG_CHECKLIST_IDS)}"
    )


def test_v062_named_receipt_paths_exist_with_matching_digests() -> None:
    entries = _receipt_entries(_load_manifest())
    assert entries, "manifest.receipts must be non-empty"
    for entry in entries:
        rel = entry.get("receipt_path")
        digest = entry.get("receipt_sha256")
        assert isinstance(rel, str) and rel, "receipt entry missing receipt_path"
        assert isinstance(digest, str) and _HEX64.fullmatch(digest), (
            f"{rel}: receipt_sha256 is not a 64-hex digest"
        )
        path = _REPO_ROOT / rel
        assert path.is_file(), f"manifest names absent receipt {rel}"
        assert _sha256_file(path) == digest, (
            f"{rel}: on-disk bytes do not match the manifest digest"
        )


def test_v062_counts_tally_pass_verdicts_and_allow_empty_install_receipts() -> None:
    manifest = _load_manifest()
    entries = _receipt_entries(manifest)
    assert entries, "manifest.receipts must be non-empty"
    verdicts = [entry.get("verdict") for entry in entries]
    assert verdicts and all(verdict == "pass" for verdict in verdicts), (
        f"every item verdict must be pass, got {verdicts}"
    )
    counts = manifest.get("counts")
    assert isinstance(counts, dict), "manifest has no counts"
    expected = counts.get("item_verdicts")
    assert isinstance(expected, dict), "counts has no item_verdicts"
    tally = Counter(verdict for verdict in verdicts if isinstance(verdict, str))
    described = {key: tally.get(key, 0) for key in expected}
    assert described == dict(expected), (
        f"verdict tally {described} != counts {dict(expected)}"
    )
    install_entries = manifest.get("install_receipts", [])
    assert isinstance(install_entries, list), "manifest.install_receipts is not an array"
    declared_installs = counts.get("install_receipts", 0)
    assert declared_installs == len(install_entries), (
        f"counts.install_receipts {declared_installs} != "
        f"{len(install_entries)} named install receipts"
    )


def test_v062_notes_and_results_name_the_product_sha_and_configuration_gate() -> None:
    manifest = _load_manifest()
    for key in ("notes_document", "results_document"):
        rel = manifest.get(key)
        assert isinstance(rel, str) and rel, f"manifest.{key} is missing"
        path = _REPO_ROOT / rel
        assert path.is_file(), f"manifest.{key} points at {rel}, which is absent"
        body = path.read_text(encoding="utf-8")
        assert _PRODUCT_SHA in body, f"{rel}: does not name product SHA {_PRODUCT_SHA}"
        assert _GATE_ID in body, f"{rel}: does not name gate id {_GATE_ID}"


def test_v062_configuration_gate_is_ready_under_analysis_and_is_not_the_v061_verdict() -> None:
    """release.yml reads this id; a prior daily-driver verdict must not stand in."""
    matches: list[tuple[Path, dict[str, str]]] = []
    for path in sorted(_ANALYSIS_ROOT.glob("*.md")):
        text = path.read_text(encoding="utf-8")
        for match in _GATE_BLOCK.finditer(text):
            fields = _gate_fields(match.group(1))
            if fields.get("id") == _GATE_ID:
                matches.append((path, fields))
    assert matches, (
        f"no document under {_ANALYSIS_ROOT.relative_to(_REPO_ROOT)} declares "
        f"```gate id: {_GATE_ID} / verdict: READY```"
    )
    assert len(matches) == 1, (
        f"gate id {_GATE_ID!r} is declared more than once: "
        f"{[path.relative_to(_REPO_ROOT).as_posix() for path, _ in matches]}"
    )
    path, fields = matches[0]
    assert path.resolve() != _V061_VERDICT.resolve(), (
        f"{path.relative_to(_REPO_ROOT)} is the v0.6.1 daily-driver verdict; "
        "v0.6.2 must not reuse it"
    )
    assert fields.get("verdict") == "READY", (
        f"{path.relative_to(_REPO_ROOT)}: gate verdict is {fields.get('verdict')!r}, "
        "not READY"
    )
    raw_horizon = fields.get("review-by")
    assert isinstance(raw_horizon, str) and raw_horizon, (
        f"{path.relative_to(_REPO_ROOT)}: gate is missing review-by"
    )
    try:
        horizon = date.fromisoformat(raw_horizon)
    except ValueError as error:
        raise AssertionError(
            f"{path.relative_to(_REPO_ROOT)}: review-by {raw_horizon!r} is not an ISO date"
        ) from error
    assert horizon > date.today(), (
        f"{path.relative_to(_REPO_ROOT)}: review-by {horizon.isoformat()} is not a future date"
    )
    emphasized = [
        span
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.startswith("#")
        for span in _EMPHASIZED.findall(line)
    ]
    assert "READY" in emphasized, (
        f"{path.relative_to(_REPO_ROOT)}: no heading emphasizes **READY** "
        f"(found: {emphasized})"
    )
