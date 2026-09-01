#!/usr/bin/env python3
"""Generate and check the Talaria v0.5.0 acceptance records.

Receipts are the authority.  The artifact manifest, results document, and
evidence-index count summaries are projections of the committed install and
item receipts.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import Counter
from collections.abc import Callable
from pathlib import Path
from typing import Any

from scripts.acceptance.v050_common import (
    ORDERED_VERDICTS,
    RELEASE_VERSION,
    TESTERS,
    VERDICTS,
    HarnessError,
    active_receipt_paths,
    read_json_object,
    require_object,
    require_string,
    sha256_file,
    write_json_object,
)
from scripts.acceptance.v050_receipt import validate_receipt

_REPO_ROOT = Path(__file__).resolve().parents[2]
_ACCEPTANCE_ROOT = _REPO_ROOT / "docs" / "acceptance" / "v0.5.0"
_EVIDENCE_ROOT = _ACCEPTANCE_ROOT / "evidence"
_CHECKLIST_PATH = _ACCEPTANCE_ROOT / "checklist-items.json"
_MANIFEST_PATH = _ACCEPTANCE_ROOT / "artifact-manifest.json"
_RESULTS_PATH = (
    _REPO_ROOT
    / "docs"
    / "acceptance"
    / "2026-08-30-talaria-v0-5-0-live-acceptance-results.md"
)
_PROVENANCE_BEGIN = "<!-- BEGIN GENERATED ACCEPTANCE PROVENANCE -->"
_PROVENANCE_END = "<!-- END GENERATED ACCEPTANCE PROVENANCE -->"
_MATRIX_BEGIN = "<!-- BEGIN GENERATED ACCEPTANCE VERDICTS -->"
_MATRIX_END = "<!-- END GENERATED ACCEPTANCE VERDICTS -->"
_STATUS_BEGIN = "<!-- BEGIN GENERATED ACCEPTANCE STATUS -->"
_STATUS_END = "<!-- END GENERATED ACCEPTANCE STATUS -->"
_INDEX_COUNTS_BEGIN = "<!-- BEGIN GENERATED ACCEPTANCE MANIFEST COUNTS -->"
_INDEX_COUNTS_END = "<!-- END GENERATED ACCEPTANCE MANIFEST COUNTS -->"
_COMMIT = re.compile(r"^[0-9a-f]{40}$")
_SHA256 = re.compile(r"^[0-9a-f]{64}$")


def _commit(value: Any, *, field: str) -> str:
    commit = require_string(value, field=field)
    if not _COMMIT.fullmatch(commit):
        raise HarnessError(f"{field} must be a full lowercase 40-character Git commit")
    return commit


def _digest(value: Any, *, field: str) -> str:
    digest = require_string(value, field=field)
    if not _SHA256.fullmatch(digest):
        raise HarnessError(f"{field} must be a lowercase SHA-256 digest")
    return digest


def _repo_path(path: Path, repo_root: Path) -> str:
    try:
        return path.resolve().relative_to(repo_root.resolve()).as_posix()
    except ValueError as exc:
        raise HarnessError(f"evidence path escaped the repository: {path}") from exc


def _display_path(path: Path, repo_root: Path) -> str:
    """Render a repository-relative path when possible, including in tests."""
    try:
        return path.resolve().relative_to(repo_root.resolve()).as_posix()
    except ValueError:
        return str(path)


def _checklist(path: Path) -> list[dict[str, Any]]:
    document = read_json_object(path)
    items = document.get("items")
    if not isinstance(items, list):
        raise HarnessError(f"{path}: `items` must be an array")
    result: list[dict[str, Any]] = []
    for raw in items:
        if not isinstance(raw, dict):
            raise HarnessError(f"{path}: every item must be an object")
        number = raw.get("number")
        title = raw.get("title")
        owner = raw.get("owner")
        if (
            isinstance(number, bool)
            or not isinstance(number, int)
            or not isinstance(title, str)
            or not title
            or owner not in {"shared", *TESTERS}
        ):
            raise HarnessError(f"{path}: malformed checklist row {raw!r}")
        result.append({"number": number, "title": title, "owner": owner})
    if [item["number"] for item in result] != list(range(1, 37)):
        raise HarnessError(f"{path}: checklist must contain ordered items 1 through 36")
    return result


def _candidate_from_install(receipt: dict[str, Any], *, path: Path) -> dict[str, str]:
    if receipt.get("schema_version") != "talaria-v0.5.0-install-v1":
        raise HarnessError(f"{path}: not a Talaria v0.5.0 install receipt")
    candidate = require_object(receipt.get("candidate"), field=f"{path}: candidate")
    artifact = require_object(receipt.get("artifact"), field=f"{path}: artifact")
    version = require_string(artifact.get("version"), field=f"{path}: artifact.version")
    if version != RELEASE_VERSION:
        raise HarnessError(f"{path}: installed version is {version!r}, not {RELEASE_VERSION!r}")
    wheel_filename = require_string(
        candidate.get("wheel_filename"), field=f"{path}: candidate.wheel_filename"
    )
    if not wheel_filename.endswith(".whl"):
        raise HarnessError(f"{path}: candidate wheel filename does not end in .whl")
    return {
        "commit": _commit(candidate.get("commit"), field=f"{path}: candidate.commit"),
        "wheel_filename": wheel_filename,
        "wheel_sha256": _digest(
            candidate.get("wheel_sha256"), field=f"{path}: candidate.wheel_sha256"
        ),
        "version": version,
    }


def _stale_reason(
    *,
    candidate_commit: str,
    candidate_wheel_sha256: str,
    current_commit: str,
    current_wheel_sha256: str | None,
) -> str | None:
    if candidate_commit != current_commit:
        return (
            f"receipt candidate {candidate_commit} does not match current candidate "
            f"{current_commit}"
        )
    if current_wheel_sha256 is not None and candidate_wheel_sha256 != current_wheel_sha256:
        return (
            f"receipt wheel {candidate_wheel_sha256} does not match current candidate wheel "
            f"{current_wheel_sha256}"
        )
    return None


def _candidate_key(identity: dict[str, str]) -> tuple[str, str, str, str]:
    return (
        identity["commit"],
        identity["wheel_filename"],
        identity["wheel_sha256"],
        identity["version"],
    )


def _evidence_directory(tester: str) -> str:
    return tester.removeprefix("talaria-")


def build_manifest(
    *,
    repo_root: Path = _REPO_ROOT,
    evidence_root: Path = _EVIDENCE_ROOT,
    checklist_path: Path = _CHECKLIST_PATH,
    current_candidate_commit: str,
) -> dict[str, Any]:
    """Build one deterministic manifest from committed receipt files."""
    current_commit = _commit(
        current_candidate_commit, field="current_candidate_commit"
    )
    checklist = _checklist(checklist_path)
    install_entries: list[dict[str, Any]] = []
    installs_by_tester: dict[str, dict[str, str]] = {}
    candidate_install_counts: Counter[tuple[str, str, str, str]] = Counter()

    for path in sorted(evidence_root.glob("*/install-receipt.json")):
        receipt = read_json_object(path)
        tester = require_string(receipt.get("tester"), field=f"{path}: tester")
        if tester not in TESTERS:
            raise HarnessError(f"{path}: unknown tester {tester!r}")
        if path.parent.name != _evidence_directory(tester):
            raise HarnessError(f"{path}: tester does not match its evidence directory")
        if tester in installs_by_tester:
            raise HarnessError(f"duplicate install receipt for {tester}")
        identity = _candidate_from_install(receipt, path=path)
        installs_by_tester[tester] = identity
        candidate_install_counts[_candidate_key(identity)] += 1
        install_entries.append(
            {
                "tester": tester,
                "candidate_commit": identity["commit"],
                "wheel_filename": identity["wheel_filename"],
                "wheel_sha256": identity["wheel_sha256"],
                "version": identity["version"],
                "receipt_path": _repo_path(path, repo_root),
                "receipt_sha256": sha256_file(path),
            }
        )

    matching_identities = {
        key for key in candidate_install_counts if key[0] == current_commit
    }
    if len(matching_identities) > 1:
        raise HarnessError(
            f"current candidate commit {current_commit} has more than one installed wheel identity"
        )
    current_identity = next(iter(matching_identities), None)
    current_wheel = current_identity[2] if current_identity is not None else None

    for entry in install_entries:
        reason = _stale_reason(
            candidate_commit=str(entry["candidate_commit"]),
            candidate_wheel_sha256=str(entry["wheel_sha256"]),
            current_commit=current_commit,
            current_wheel_sha256=current_wheel,
        )
        entry["stale_candidate"] = reason is not None
        entry["stale_reason"] = reason

    item_entries: list[dict[str, Any]] = []
    seen_items: dict[tuple[int, str], Path] = {}
    candidate_item_counts: Counter[tuple[str, str, str, str]] = Counter()
    for path in active_receipt_paths(evidence_root):
        receipt = read_json_object(path)
        tester = require_string(receipt.get("tester"), field=f"{path}: tester")
        if tester not in TESTERS:
            raise HarnessError(f"{path}: unknown tester {tester!r}")
        if path.parents[1].name != _evidence_directory(tester):
            raise HarnessError(f"{path}: tester does not match its evidence directory")
        number = receipt.get("checklist_item")
        if isinstance(number, bool) or not isinstance(number, int) or number not in range(1, 37):
            raise HarnessError(f"{path}: checklist_item must be from 1 through 36")
        key = (number, tester)
        if key in seen_items:
            raise HarnessError(f"duplicate item receipt {key}: {seen_items[key]} and {path}")
        seen_items[key] = path

        artifact = require_object(receipt.get("artifact"), field=f"{path}: artifact")
        identity = {
            "commit": _commit(artifact.get("commit"), field=f"{path}: artifact.commit"),
            "wheel_filename": require_string(
                artifact.get("wheel_filename"), field=f"{path}: artifact.wheel_filename"
            ),
            "wheel_sha256": _digest(
                artifact.get("wheel_sha256"), field=f"{path}: artifact.wheel_sha256"
            ),
            "version": require_string(artifact.get("version"), field=f"{path}: artifact.version"),
        }
        install_identity = installs_by_tester.get(tester)
        if install_identity is None:
            raise HarnessError(f"{path}: no committed install receipt exists for {tester}")
        if identity != install_identity:
            raise HarnessError(f"{path}: item artifact does not match {tester}'s install receipt")
        candidate_item_counts[_candidate_key(identity)] += 1

        evidence = require_object(receipt.get("evidence"), field=f"{path}: evidence")
        capture_path = Path(
            require_string(evidence.get("capture_path"), field=f"{path}: evidence.capture_path")
        )
        screenshot_source = Path(
            require_string(
                evidence.get("screenshot_path"),
                field=f"{path}: evidence.screenshot_path",
            )
        )
        screenshot_path = (
            evidence_root
            / _evidence_directory(tester)
            / "screenshots"
            / screenshot_source.name
        )
        if not screenshot_path.is_file():
            raise HarnessError(f"{path}: committed screenshot is missing: {screenshot_path}")
        screenshot_sha256 = _digest(
            evidence.get("screenshot_sha256"), field=f"{path}: evidence.screenshot_sha256"
        )
        if sha256_file(screenshot_path) != screenshot_sha256:
            raise HarnessError(f"{path}: committed screenshot hash does not match the receipt")
        capture_sha256 = _digest(
            evidence.get("capture_sha256"), field=f"{path}: evidence.capture_sha256"
        )
        verdict = receipt.get("verdict")
        if verdict not in VERDICTS:
            raise HarnessError(f"{path}: unknown verdict {verdict!r}")
        reason = _stale_reason(
            candidate_commit=identity["commit"],
            candidate_wheel_sha256=identity["wheel_sha256"],
            current_commit=current_commit,
            current_wheel_sha256=current_wheel,
        )
        contract_errors = validate_receipt(receipt, verify_files=False)
        item_entries.append(
            {
                "checklist_item": number,
                "title": require_string(receipt.get("title"), field=f"{path}: title"),
                "tester": tester,
                "verdict": verdict,
                "candidate_commit": identity["commit"],
                "wheel_sha256": identity["wheel_sha256"],
                "stale_candidate": reason is not None,
                "stale_reason": reason,
                "contract_status": "valid" if not contract_errors else "invalid",
                "contract_errors": contract_errors,
                "receipt_path": _repo_path(path, repo_root),
                "receipt_sha256": sha256_file(path),
                "capture_path": f"{tester} scratch/raw/{capture_path.name}",
                "capture_sha256": capture_sha256,
                "screenshot_path": _repo_path(screenshot_path, repo_root),
                "screenshot_sha256": screenshot_sha256,
            }
        )

    item_entries.sort(key=lambda entry: (int(entry["checklist_item"]), str(entry["tester"])))
    item_verdicts = Counter(str(entry["verdict"]) for entry in item_entries)
    invalid_item_receipts = sum(
        entry["contract_status"] == "invalid" for entry in item_entries
    )
    stale_item_receipts = sum(bool(entry["stale_candidate"]) for entry in item_entries)
    stale_install_receipts = sum(bool(entry["stale_candidate"]) for entry in install_entries)

    candidates: list[dict[str, Any]] = []
    for candidate_key in sorted(candidate_install_counts):
        commit, wheel_filename, wheel_sha256, version = candidate_key
        candidates.append(
            {
                "commit": commit,
                "wheel_filename": wheel_filename,
                "wheel_sha256": wheel_sha256,
                "version": version,
                "install_receipt_count": candidate_install_counts[candidate_key],
                "item_receipt_count": candidate_item_counts[candidate_key],
                "matches_current_candidate": commit == current_commit
                and (current_wheel is None or wheel_sha256 == current_wheel),
            }
        )

    expected_keys = {
        (int(item["number"]), tester)
        for item in checklist
        for tester in (
            sorted(TESTERS) if item["owner"] == "shared" else [str(item["owner"])]
        )
    }
    present_keys = set(seen_items)
    present_keys.update((1, tester) for tester in installs_by_tester)
    current_keys = {
        (int(entry["checklist_item"]), str(entry["tester"]))
        for entry in item_entries
        if not entry["stale_candidate"]
    }
    current_keys.update(
        (1, str(entry["tester"]))
        for entry in install_entries
        if not entry["stale_candidate"]
    )
    missing_on_disk = sorted(expected_keys - present_keys)
    missing_current = sorted(expected_keys - current_keys)

    receipt_count = len(item_entries) + len(install_entries)
    stale_receipt_count = stale_item_receipts + stale_install_receipts
    if receipt_count == 0:
        status = "not-run"
    elif stale_receipt_count:
        status = "stale"
    elif invalid_item_receipts or item_verdicts["fail"] or item_verdicts["blocked"]:
        status = "blocked"
    elif missing_current:
        status = "in-progress"
    else:
        status = "complete"

    if current_identity is None:
        current_candidate = {
            "commit": current_commit,
            "wheel_filename": None,
            "wheel_sha256": None,
            "version": RELEASE_VERSION,
            "has_install_receipt": False,
        }
    else:
        current_candidate = {
            "commit": current_identity[0],
            "wheel_filename": current_identity[1],
            "wheel_sha256": current_identity[2],
            "version": current_identity[3],
            "has_install_receipt": True,
        }

    return {
        "$schema": "./artifact-manifest.schema.json",
        "schema_version": "talaria-v0.5.0-artifact-manifest-v2",
        "generated_command": (
            "uv run python -m scripts.acceptance.v050_records refresh "
            f"--current-candidate-commit {current_commit}"
        ),
        "status": status,
        "current_candidate": current_candidate,
        "counts": {
            "expected_receipts": len(expected_keys),
            "install_receipts": len(install_entries),
            "item_receipts": len(item_entries),
            "current_receipts": receipt_count - stale_receipt_count,
            "stale_receipts": stale_receipt_count,
            "invalid_item_receipts": invalid_item_receipts,
            "missing_receipts_on_disk": len(missing_on_disk),
            "missing_current_receipts": len(missing_current),
            "item_verdicts": {
                verdict: item_verdicts[verdict] for verdict in ORDERED_VERDICTS
            },
        },
        "candidates": candidates,
        "install_receipts": install_entries,
        "receipts": item_entries,
        "missing_receipts_on_disk": [
            {"checklist_item": number, "tester": tester}
            for number, tester in missing_on_disk
        ],
        "missing_current_receipts": [
            {"checklist_item": number, "tester": tester}
            for number, tester in missing_current
        ],
    }


def _generated_region(document: str, begin: str, end: str) -> tuple[int, int, str]:
    start = document.find(begin)
    finish = document.find(end)
    if start < 0 or finish < 0 or finish < start:
        raise HarnessError(f"document is missing generated markers {begin!r}/{end!r}")
    content_start = start + len(begin)
    return content_start, finish, document[content_start:finish]


def _status_cell(entry: dict[str, Any]) -> str:
    verdict = str(entry["verdict"]).upper()
    short_commit = str(entry["candidate_commit"])[:7]
    stale = bool(entry["stale_candidate"])
    invalid = entry.get("contract_status") == "invalid"
    if stale and invalid:
        return f"`STALE/INVALID — prior {verdict} @ {short_commit}`"
    if stale:
        return f"`STALE — prior {verdict} @ {short_commit}`"
    if invalid:
        return f"`INVALID — claimed {verdict}`"
    return f"`{verdict}`"


def _install_status_cell(entry: dict[str, Any]) -> str:
    if entry["stale_candidate"]:
        return f"`STALE — prior PASS @ {str(entry['candidate_commit'])[:7]}`"
    return "`PASS`"


def _provenance_table(manifest: dict[str, Any]) -> str:
    current = require_object(manifest["current_candidate"], field="manifest.current_candidate")
    candidates = manifest["candidates"]
    assert isinstance(candidates, list)
    candidate_lines = []
    for candidate in candidates:
        assert isinstance(candidate, dict)
        candidate_lines.append(
            f"`{str(candidate['commit'])[:7]}` / `{candidate['wheel_sha256']}` "
            f"({candidate['install_receipt_count']} install, "
            f"{candidate['item_receipt_count']} item receipts)"
        )
    candidate_summary = "<br>".join(candidate_lines) if candidate_lines else "none"
    wheel = current["wheel_sha256"] if current["wheel_sha256"] is not None else "not frozen"
    counts = require_object(manifest["counts"], field="manifest.counts")
    return "\n".join(
        [
            "| Field | Generated value |",
            "| --- | --- |",
            f"| Manifest status | `{str(manifest['status']).upper()}` |",
            f"| Current reviewed candidate commit | `{current['commit']}` |",
            f"| Current candidate wheel SHA-256 | `{wheel}` |",
            f"| Receipt candidate identities | {candidate_summary} |",
            "| Receipt counts | "
            f"{counts['install_receipts']} install; {counts['item_receipts']} item; "
            f"{counts['current_receipts']} current; {counts['stale_receipts']} stale; "
            f"{counts['invalid_item_receipts']} invalid |",
        ]
    )


def _receipt_observation(receipt_path: str, *, repo_root: Path) -> str:
    path = (repo_root / receipt_path).resolve()
    try:
        path.relative_to(repo_root.resolve())
    except ValueError as exc:
        raise HarnessError(f"receipt path escaped the repository: {path}") from exc
    receipt = read_json_object(path)
    raw = receipt.get("observations")
    if not isinstance(raw, list) or not raw or not all(
        isinstance(value, str) and value.strip() for value in raw
    ):
        raise HarnessError(f"{path}: observations must contain at least one non-empty string")
    return " ".join(value.strip() for value in raw)


def _observation_cell(
    number: int,
    *,
    entries: list[dict[str, Any]],
    install_entries: list[dict[str, Any]],
    repo_root: Path,
) -> str:
    observations = [
        _receipt_observation(str(entry["receipt_path"]), repo_root=repo_root)
        for entry in entries
        if int(entry["checklist_item"]) == number
    ]
    if observations:
        return " ".join(observations).replace("|", "\\|").replace("\n", " ")
    if number == 1 and install_entries:
        return (
            "The install receipts prove fresh, non-editable installations of the current "
            "reviewed wheel."
        )
    return "No current receipt records an observation for this checklist item."


def _blocking_rows(manifest: dict[str, Any]) -> list[tuple[int, str]]:
    blocked: dict[int, str] = {}
    for entry in manifest["receipts"]:
        if entry["stale_candidate"] or entry["contract_status"] != "valid":
            continue
        verdict = str(entry["verdict"])
        if verdict in {"fail", "blocked"}:
            number = int(entry["checklist_item"])
            blocked[number] = "fail" if verdict == "fail" else blocked.get(number, "blocked")
    for missing in manifest["missing_current_receipts"]:
        blocked.setdefault(int(missing["checklist_item"]), "missing")
    return sorted(blocked.items())


def acceptance_summary(manifest: dict[str, Any]) -> str:
    """Describe checklist coverage separately from physical receipt-file counts."""
    status = str(manifest["status"]).upper()
    counts = require_object(manifest["counts"], field="manifest.counts")
    verdicts = require_object(counts["item_verdicts"], field="counts.item_verdicts")
    covered = int(counts["expected_receipts"]) - int(counts["missing_current_receipts"])
    item_keys = {
        (int(entry["checklist_item"]), str(entry["tester"]))
        for entry in manifest["receipts"]
        if not entry["stale_candidate"]
    }
    install_keys = {
        (1, str(entry["tester"]))
        for entry in manifest["install_receipts"]
        if not entry["stale_candidate"]
    }
    overlap = sorted(item_keys & install_keys)
    if len(overlap) == 1:
        number, tester = overlap[0]
        overlap_summary = (
            f"The one-receipt overlap is checklist item {number} for {tester}, which has "
            "both an item receipt and an install receipt. "
        )
    elif overlap:
        labels = ", ".join(
            f"checklist item {number} for {tester}" for number, tester in overlap
        )
        overlap_summary = (
            f"The {len(overlap)} receipt overlaps are {labels}; each has both an item "
            "receipt and an install receipt. "
        )
    else:
        overlap_summary = "There is no overlap between item and install receipts. "
    return (
        f"**{status}**: {covered} of {counts['expected_receipts']} expected "
        "checklist/tester slots are covered. "
        f"The evidence set separately contains {counts['current_receipts']} current receipts "
        f"({counts['item_receipts']} item and {counts['install_receipts']} install). "
        f"{overlap_summary}"
        f"Item verdicts are {verdicts['pass']} pass, {verdicts['blocked']} blocked, and "
        f"{verdicts['fail']} fail."
    )


def _status_block(manifest: dict[str, Any]) -> str:
    status = str(manifest["status"]).upper()
    counts = require_object(manifest["counts"], field="manifest.counts")
    blocked = _blocking_rows(manifest)
    lines = [
        f"## Status: **{status}**",
        "",
        "This verdict is generated from `artifact-manifest.json`; it is not maintained by hand. "
        + acceptance_summary(manifest),
        f"The manifest also records {counts['stale_receipts']} stale receipts and "
        f"{counts['invalid_item_receipts']} invalid item receipts.",
        "Regenerate it with "
        f"`{manifest['generated_command']}`.",
        "",
        "```gate",
        "id: talaria-v0-5-0-live-acceptance",
        f"verdict: {status}",
        "review-by: 2026-09-30",
    ]
    lines.extend(f"blocks-on: row-{number} {grade}" for number, grade in blocked)
    lines.extend(["```", "", "## Evidence table", "", "| Item | Condition | Status |"])
    lines.append("| ---: | --- | --- |")
    if blocked:
        titles = {
            int(entry["checklist_item"]): str(entry["title"])
            for entry in manifest["receipts"]
        }
        lines.extend(
            f"| {number} | {titles.get(number, 'Missing checklist receipt')} | **{grade}** |"
            for number, grade in blocked
        )
    return "\n".join(lines)


def render_results_document(
    document: str,
    *,
    manifest: dict[str, Any],
    checklist: list[dict[str, Any]],
    repo_root: Path = _REPO_ROOT,
) -> str:
    """Replace generated status, provenance, verdict, and observation projections."""
    receipt_entries = {
        (int(entry["checklist_item"]), str(entry["tester"])): entry
        for entry in manifest["receipts"]
    }
    install_entries = {
        str(entry["tester"]): entry for entry in manifest["install_receipts"]
    }
    lines = [
        "| Item | Checklist item | T1 | T2 | Evidence and observation |",
        "| ---: | --- | :--- | :--- | --- |",
    ]
    for item in checklist:
        number = int(item["number"])
        cells: list[str] = []
        for tester in ("talaria-t1", "talaria-t2"):
            if item["owner"] != "shared" and item["owner"] != tester:
                cells.append("—")
                continue
            entry = receipt_entries.get((number, tester))
            if entry is not None:
                cells.append(_status_cell(entry))
            elif number == 1 and tester in install_entries:
                cells.append(_install_status_cell(install_entries[tester]))
            else:
                cells.append("`NO RECEIPT`")
        observation = _observation_cell(
            number,
            entries=manifest["receipts"],
            install_entries=manifest["install_receipts"],
            repo_root=repo_root,
        )
        lines.append(
            f"| {number} | {item['title']} | {cells[0]} | {cells[1]} | {observation} |"
        )
    generated_matrix = "\n".join(lines)
    status_start, status_finish, _ = _generated_region(document, _STATUS_BEGIN, _STATUS_END)
    document = (
        document[:status_start]
        + "\n"
        + _status_block(manifest)
        + "\n"
        + document[status_finish:]
    )
    provenance_start, provenance_finish, _ = _generated_region(
        document, _PROVENANCE_BEGIN, _PROVENANCE_END
    )
    document = (
        document[:provenance_start]
        + "\n"
        + _provenance_table(manifest)
        + "\n"
        + document[provenance_finish:]
    )
    matrix_start, matrix_finish, _ = _generated_region(document, _MATRIX_BEGIN, _MATRIX_END)
    return (
        document[:matrix_start]
        + "\n"
        + generated_matrix
        + "\n"
        + document[matrix_finish:]
    )


def _json_text(value: dict[str, Any]) -> str:
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def _evidence_index_count_sentence(manifest: dict[str, Any]) -> str:
    counts = require_object(manifest["counts"], field="manifest.counts")
    return (
        f"The generated manifest reports {counts['stale_receipts']} stale receipts, "
        f"{counts['missing_current_receipts']} missing current receipts, and "
        f"{counts['invalid_item_receipts']} invalid item receipts."
    )


def render_evidence_index(
    document: str,
    *,
    manifest: dict[str, Any],
    initialize: bool = False,
) -> str:
    """Replace only the generated manifest-count sentence in an evidence index."""
    sentence = _evidence_index_count_sentence(manifest)
    begin_count = document.count(_INDEX_COUNTS_BEGIN)
    end_count = document.count(_INDEX_COUNTS_END)
    if begin_count != 1 or end_count != 1:
        if not initialize or begin_count or end_count:
            raise HarnessError(
                "evidence index must contain exactly one generated manifest-count region"
            )
        table_start = document.find("\n| ")
        if table_start < 0:
            raise HarnessError(
                "evidence index has no generated count markers or Markdown table anchor"
            )
        block = "\n".join([_INDEX_COUNTS_BEGIN, sentence, _INDEX_COUNTS_END])
        return (
            document[:table_start].rstrip()
            + "\n\n"
            + block
            + "\n"
            + document[table_start:]
        )
    start, finish, _ = _generated_region(
        document, _INDEX_COUNTS_BEGIN, _INDEX_COUNTS_END
    )
    return document[:start] + "\n" + sentence + "\n" + document[finish:]


def _render_evidence_indexes(
    *,
    manifest: dict[str, Any],
    evidence_root: Path,
    initialize: bool,
) -> list[tuple[Path, str]]:
    rendered: list[tuple[Path, str]] = []
    for path in sorted(evidence_root.glob("*/README.md")):
        try:
            document = path.read_text(encoding="utf-8")
        except OSError as exc:
            raise HarnessError(f"cannot read evidence index {path}: {exc}") from exc
        rendered.append(
            (
                path,
                render_evidence_index(
                    document, manifest=manifest, initialize=initialize
                ),
            )
        )
    if not rendered:
        raise HarnessError(f"no tester evidence indexes found under {evidence_root}")
    return rendered


def evidence_index_errors(
    manifest: dict[str, Any],
    *,
    evidence_root: Path = _EVIDENCE_ROOT,
    repo_root: Path = _REPO_ROOT,
) -> list[str]:
    """Return drift errors for tester-index manifest-count projections."""
    errors: list[str] = []
    paths = sorted(evidence_root.glob("*/README.md"))
    for path in paths:
        try:
            document = path.read_text(encoding="utf-8")
            expected = render_evidence_index(document, manifest=manifest)
        except (HarnessError, OSError) as exc:
            errors.append(f"{_display_path(path, repo_root)}: {exc}")
            continue
        if document != expected:
            errors.append(
                f"{_display_path(path, repo_root)} has manifest counts that disagree "
                "with the generated manifest"
            )
    if not paths:
        errors.append(
            f"{_display_path(evidence_root, repo_root)} has no tester evidence indexes"
        )
    return errors


def refresh_records(
    *,
    current_candidate_commit: str,
    repo_root: Path = _REPO_ROOT,
    evidence_root: Path = _EVIDENCE_ROOT,
    checklist_path: Path = _CHECKLIST_PATH,
    manifest_path: Path = _MANIFEST_PATH,
    results_path: Path = _RESULTS_PATH,
) -> dict[str, Any]:
    """Regenerate repository records and tester-index counts from the receipt set."""
    if not _git_commit_exists(repo_root, current_candidate_commit):
        raise HarnessError(
            f"current candidate commit does not exist in this repository: "
            f"{current_candidate_commit}"
        )
    manifest = build_manifest(
        repo_root=repo_root,
        evidence_root=evidence_root,
        checklist_path=checklist_path,
        current_candidate_commit=current_candidate_commit,
    )
    checklist = _checklist(checklist_path)
    try:
        current_results = results_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise HarnessError(f"cannot read results document {results_path}: {exc}") from exc
    rendered_results = render_results_document(
        current_results, manifest=manifest, checklist=checklist, repo_root=repo_root
    )
    rendered_indexes = _render_evidence_indexes(
        manifest=manifest,
        evidence_root=evidence_root,
        initialize=True,
    )
    write_json_object(manifest_path, manifest, replace=True)
    results_path.write_text(rendered_results, encoding="utf-8")
    for path, document in rendered_indexes:
        path.write_text(document, encoding="utf-8")
    return manifest


def check_records(
    *,
    repo_root: Path = _REPO_ROOT,
    evidence_root: Path = _EVIDENCE_ROOT,
    checklist_path: Path = _CHECKLIST_PATH,
    manifest_path: Path = _MANIFEST_PATH,
    results_path: Path = _RESULTS_PATH,
) -> list[str]:
    """Return drift errors for every generated acceptance record."""
    current_manifest = read_json_object(manifest_path)
    current_candidate = require_object(
        current_manifest.get("current_candidate"), field="manifest.current_candidate"
    )
    current_commit = _commit(
        current_candidate.get("commit"), field="manifest.current_candidate.commit"
    )
    expected_manifest = build_manifest(
        repo_root=repo_root,
        evidence_root=evidence_root,
        checklist_path=checklist_path,
        current_candidate_commit=current_commit,
    )
    errors: list[str] = []
    try:
        manifest_text = manifest_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise HarnessError(f"cannot read manifest {manifest_path}: {exc}") from exc
    if manifest_text != _json_text(expected_manifest):
        errors.append(
            f"{_display_path(manifest_path, repo_root)} disagrees with the receipts; run "
            f"{expected_manifest['generated_command']}"
        )
    try:
        results_text = results_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise HarnessError(f"cannot read results document {results_path}: {exc}") from exc
    expected_results = render_results_document(
        results_text,
        manifest=expected_manifest,
        checklist=_checklist(checklist_path),
        repo_root=repo_root,
    )
    if results_text != expected_results:
        prefix = _display_path(results_path, repo_root)
        current_status = _generated_region(results_text, _STATUS_BEGIN, _STATUS_END)[2]
        expected_status = _generated_region(expected_results, _STATUS_BEGIN, _STATUS_END)[2]
        current_matrix = _generated_region(results_text, _MATRIX_BEGIN, _MATRIX_END)[2]
        expected_matrix = _generated_region(expected_results, _MATRIX_BEGIN, _MATRIX_END)[2]
        if current_status != expected_status:
            errors.append(f"{prefix} has status that disagrees with the manifest")
        if current_matrix != expected_matrix:
            current_rows = _matrix_rows(current_matrix)
            expected_rows = _matrix_rows(expected_matrix)
            if any(
                current_rows.get(number, [""] * 5)[4] != cells[4]
                for number, cells in expected_rows.items()
            ):
                errors.append(f"{prefix} has observations that disagree with the receipts")
            if any(
                current_rows.get(number, [""] * 5)[:4] != cells[:4]
                for number, cells in expected_rows.items()
            ):
                errors.append(f"{prefix} has verdict cells that disagree with the receipts")
    errors.extend(
        evidence_index_errors(
            expected_manifest,
            evidence_root=evidence_root,
            repo_root=repo_root,
        )
    )
    return errors


def _matrix_rows(matrix: str) -> dict[int, list[str]]:
    rows: dict[int, list[str]] = {}
    for line in matrix.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in re.split(r"(?<!\\)\|", line)[1:-1]]
        if len(cells) == 5 and cells[0].isdigit():
            rows[int(cells[0])] = cells
    return rows


def choose_current_candidate_after_drive(
    recorded_commit: str,
    driven_commit: str,
    *,
    is_ancestor: Callable[[str, str], bool],
) -> str:
    """Advance to a descendant drive, but never let an old drive hide staleness."""
    if recorded_commit == driven_commit:
        return recorded_commit
    if is_ancestor(recorded_commit, driven_commit):
        return driven_commit
    if is_ancestor(driven_commit, recorded_commit):
        return recorded_commit
    raise HarnessError(
        "driven candidate and recorded current candidate have diverged; run the records "
        "refresh command with the operator-selected candidate commit"
    )


def _git_is_ancestor(repo_root: Path, older: str, newer: str) -> bool:
    process = subprocess.run(
        ["git", "merge-base", "--is-ancestor", older, newer],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    if process.returncode == 0:
        return True
    if process.returncode == 1:
        return False
    detail = process.stderr.strip() or process.stdout.strip() or "unknown Git error"
    raise HarnessError(f"cannot compare candidate commits {older} and {newer}: {detail}")


def _git_commit_exists(repo_root: Path, commit: str) -> bool:
    process = subprocess.run(
        ["git", "cat-file", "-e", f"{commit}^{{commit}}"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    return process.returncode == 0


def refresh_records_after_drive(
    driven_candidate_commit: str, *, repo_root: Path = _REPO_ROOT
) -> dict[str, Any]:
    """Refresh generated records at the end of one pseudo-terminal drive."""
    manifest_path = repo_root / "docs" / "acceptance" / "v0.5.0" / "artifact-manifest.json"
    manifest = read_json_object(manifest_path)
    current = manifest.get("current_candidate")
    if isinstance(current, dict) and isinstance(current.get("commit"), str):
        recorded_commit = _commit(
            current["commit"], field="manifest.current_candidate.commit"
        )
        selected = choose_current_candidate_after_drive(
            recorded_commit,
            _commit(driven_candidate_commit, field="driven_candidate_commit"),
            is_ancestor=lambda older, newer: _git_is_ancestor(repo_root, older, newer),
        )
    else:
        selected = _commit(driven_candidate_commit, field="driven_candidate_commit")
    return refresh_records(
        current_candidate_commit=selected,
        repo_root=repo_root,
        evidence_root=repo_root / "docs" / "acceptance" / "v0.5.0" / "evidence",
        checklist_path=repo_root
        / "docs"
        / "acceptance"
        / "v0.5.0"
        / "checklist-items.json",
        manifest_path=manifest_path,
        results_path=repo_root
        / "docs"
        / "acceptance"
        / "2026-08-30-talaria-v0-5-0-live-acceptance-results.md",
    )


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    refresh = subparsers.add_parser("refresh", help="regenerate manifest and verdict rows")
    refresh.add_argument("--current-candidate-commit", required=True)
    subparsers.add_parser("check", help="fail if generated records disagree with receipts")
    return parser


def _main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.command == "refresh":
        manifest = refresh_records(current_candidate_commit=args.current_candidate_commit)
        print(_MANIFEST_PATH)
        print(_RESULTS_PATH)
        print(f"status: {manifest['status']}")
        return 0
    errors = check_records()
    if errors:
        for error in errors:
            print(f"v050_records: {error}", file=sys.stderr)
        return 1
    print("acceptance records match receipts")
    return 0


def main(argv: list[str] | None = None) -> int:
    try:
        return _main(argv)
    except HarnessError as exc:
        print(f"v050_records: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
