"""v0.6.3 release-record contract against the real tree.

These tests are the implementer's spec. They must not skip when the
v0.6.3 manifest is absent: a missing record is a failed release, not a
version that is "not yet recorded". ``tests/docs/test_acceptance_versions.py``
skips that case on purpose; this module exists so the skip cannot leak
into the tag that ``release.yml`` will bind.

Do not invent the real manifest, schema, receipts, or notes here.
"""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

import jsonschema

_REPO_ROOT = Path(__file__).resolve().parents[2]
_VERSION_DIR = _REPO_ROOT / "docs" / "acceptance" / "v0.6.3"
_MANIFEST_PATH = _VERSION_DIR / "artifact-manifest.json"
_SCHEMA_PATH = _VERSION_DIR / "artifact-manifest.schema.json"

_PRODUCT_SHA = "7c34cc2a7187dd8a89173fe5658821e5e5da21e1"
_MANIFEST_SCHEMA_VERSION = "talaria-v0.6.3-artifact-manifest-v1"
_GATE_ID = "v0-6-3-configuration-residuals"
_FORBIDDEN_GATE_IDS = frozenset(
    {
        "v0-6-2-configuration",
        "v0-6-1-daily-driver",
    }
)
_HEX64 = re.compile(r"^[0-9a-f]{64}$")


def _load_manifest() -> dict[str, Any]:
    """Load the v0.6.3 manifest or fail. Never skip."""
    assert _MANIFEST_PATH.is_file(), (
        f"v0.6.3 release record is missing: {_MANIFEST_PATH.relative_to(_REPO_ROOT)}"
    )
    value = json.loads(_MANIFEST_PATH.read_text(encoding="utf-8"))
    assert isinstance(value, dict), f"{_MANIFEST_PATH}: manifest root is not an object"
    return value


def _candidate(manifest: dict[str, Any]) -> dict[str, Any]:
    candidate = manifest.get("candidate")
    assert isinstance(candidate, dict), "manifest names no candidate object"
    return candidate


def test_v063_manifest_exists_and_validates_against_sibling_schema() -> None:
    """The tagged tree must ship a schema-valid v0.6.3 manifest."""
    assert _MANIFEST_PATH.is_file(), (
        f"v0.6.3 release record is missing: {_MANIFEST_PATH.relative_to(_REPO_ROOT)}"
    )
    assert _SCHEMA_PATH.is_file(), (
        f"v0.6.3 manifest schema is missing: {_SCHEMA_PATH.relative_to(_REPO_ROOT)}"
    )
    manifest = _load_manifest()
    schema = json.loads(_SCHEMA_PATH.read_text(encoding="utf-8"))
    assert isinstance(schema, dict), f"{_SCHEMA_PATH}: schema root is not an object"
    jsonschema.validate(manifest, schema)


def test_v063_manifest_schema_version_is_named() -> None:
    manifest = _load_manifest()
    assert manifest.get("schema_version") == _MANIFEST_SCHEMA_VERSION, (
        f"manifest.schema_version must be {_MANIFEST_SCHEMA_VERSION!r}, "
        f"got {manifest.get('schema_version')!r}"
    )


def test_v063_manifest_gate_id_is_residuals_not_a_prior_release_gate() -> None:
    manifest = _load_manifest()
    gate_id = manifest.get("gate_id")
    assert gate_id not in _FORBIDDEN_GATE_IDS, (
        f"manifest.gate_id {gate_id!r} reuses a prior release's gate"
    )
    assert gate_id == _GATE_ID, (
        f"manifest.gate_id must be {_GATE_ID!r}, got {gate_id!r}"
    )


def test_v063_candidate_commit_is_the_accepted_product_sha() -> None:
    candidate = _candidate(_load_manifest())
    assert candidate.get("commit") == _PRODUCT_SHA, (
        f"candidate.commit must be the accepted product SHA {_PRODUCT_SHA}, "
        f"got {candidate.get('commit')!r}"
    )


def test_v063_candidate_version_is_0_6_3() -> None:
    candidate = _candidate(_load_manifest())
    assert candidate.get("version") == "0.6.3", (
        f"candidate.version must be '0.6.3', got {candidate.get('version')!r}"
    )


def test_v063_candidate_names_a_wheel_without_a_hard_coded_digest() -> None:
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
