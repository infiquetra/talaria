"""Keep v0.5.0 reader-facing release claims bound to current evidence."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any, cast

_REPO_ROOT = Path(__file__).resolve().parents[2]
_ACCEPTANCE_ROOT = _REPO_ROOT / "docs" / "acceptance" / "v0.5.0"
_MANIFEST_PATH = _ACCEPTANCE_ROOT / "artifact-manifest.json"
_RELEASE_DOCUMENT_PATHS = (_REPO_ROOT / "README.md", _REPO_ROOT / "CHANGELOG.md")
_DOCUMENTATION_INDEX_PATH = _REPO_ROOT / "docs" / "00-index.md"


def _manifest() -> dict[str, Any]:
    return cast(
        dict[str, Any], json.loads(_MANIFEST_PATH.read_text(encoding="utf-8"))
    )


def test_release_documents_quote_the_current_manifest_outcome() -> None:
    """Reader-facing release claims cannot lag a regenerated manifest again."""
    manifest = _manifest()
    counts = manifest["counts"]
    verdicts = counts["item_verdicts"]
    status = str(manifest["status"]).upper()
    summary = (
        f"**{status}**: {counts['current_receipts']} of "
        f"{counts['expected_receipts']} receipts are current; item verdicts are "
        f"{verdicts['pass']} pass, {verdicts['blocked']} blocked, and "
        f"{verdicts['fail']} fail"
    )

    for path in _RELEASE_DOCUMENT_PATHS:
        document = path.read_text(encoding="utf-8")
        normalized = re.sub(r"\s+", " ", re.sub(r"(?m)^> ?", "", document))
        assert summary in normalized, path
        assert document.count(f"**{status}**") >= 2, path
        assert document.lower().count("item 24") >= 2, path
        assert "gateway always assigns a request identifier" in document, path
        assert "receipts and screenshots do not exist" not in document, path
        assert "no current item receipts" not in document, path
        assert "no approved live primary model receipt" not in document, path


def test_documentation_index_reaches_every_tester_evidence_index() -> None:
    """Adding an acceptance tester must also make its evidence discoverable."""
    index = _DOCUMENTATION_INDEX_PATH.read_text(encoding="utf-8")
    linked_targets = set(re.findall(r"\[[^]]+\]\(([^)]+)\)", index))
    evidence_indexes = sorted((_ACCEPTANCE_ROOT / "evidence").glob("*/README.md"))

    assert evidence_indexes
    for evidence_index in evidence_indexes:
        target = evidence_index.relative_to(_REPO_ROOT / "docs").as_posix()
        assert target in linked_targets, evidence_index
