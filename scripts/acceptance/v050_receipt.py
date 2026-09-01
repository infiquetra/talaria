#!/usr/bin/env python3
"""Create and validate one evidence receipt per v0.5.0 checklist item and tester."""

from __future__ import annotations

import argparse
import datetime as dt
import re
import shutil
import struct
import subprocess
import sys
from pathlib import Path
from typing import Any

from scripts.acceptance.v050_common import (
    FALLBACK_MODEL_ROUTE,
    FALLBACK_REASON_CODES,
    PRIMARY_MODEL_ROUTE,
    RELEASE_VERSION,
    TERMINAL_VERDICTS,
    TESTERS,
    VERDICTS,
    HarnessError,
    is_within,
    read_json_object,
    sha256_file,
    validate_tester,
    write_json_object,
)
from scripts.acceptance.v050_common import (
    require_object as _object,
)
from scripts.acceptance.v050_common import (
    require_string as _string,
)

_REPO_ROOT = Path(__file__).resolve().parents[2]
_ACCEPTANCE_ROOT = _REPO_ROOT / "docs" / "acceptance" / "v0.5.0"
_CHECKLIST_PATH = _ACCEPTANCE_ROOT / "checklist-items.json"
_MANIFEST_PATH = _ACCEPTANCE_ROOT / "artifact-manifest.json"
_EVIDENCE_ROOT = _ACCEPTANCE_ROOT / "evidence"
_ROUTE_ALIASES: dict[str, str | None] = {
    "primary": PRIMARY_MODEL_ROUTE,
    "fallback": FALLBACK_MODEL_ROUTE,
    "none": None,
}
_RELEASE_RELEVANT_PATHS = ("talaria", "pyproject.toml", "uv.lock", "src")
_PRIVATE_PATTERNS = (
    (re.compile(rb"/(?:Users|home)/[A-Za-z0-9._-]+"), "operator home path"),
    (
        re.compile(rb"/-(?:Users|home)-[A-Za-z0-9._-]+-"),
        "encoded operator home path",
    ),
    (re.compile(rb"/private/var/folders/"), "operator temporary-directory identifier"),
    (re.compile(rb"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"), "email address"),
    (re.compile(rb"Authorization:\s*Bearer\s+\S+", re.IGNORECASE), "bearer credential"),
    (re.compile(rb"(?:token|credential)=[^&\s]+", re.IGNORECASE), "credential query value"),
)
_MACOS_ACCEPTANCE_SCRATCH_PATH = re.compile(
    rb"/private/var/folders/[A-Za-z0-9._/-]+/T/"
    rb"talaria-v050-[A-Za-z0-9._-]+(?:\xe2\x80\xa6)?"
)


def _utc_now() -> str:
    return dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat()


def _contains_home_path(value: Any, *, home: str | None = None) -> bool:
    """Return whether any nested string exposes the current operator home path."""
    home_path = home or str(Path.home().resolve())
    if isinstance(value, str):
        return home_path in value
    if isinstance(value, dict):
        return any(_contains_home_path(item, home=home_path) for item in value.values())
    if isinstance(value, list):
        return any(_contains_home_path(item, home=home_path) for item in value)
    return False


def _evidence_path(value: Any, *, field: str, repo_root: Path) -> Path:
    path = Path(_string(value, field=field)).expanduser()
    if not path.is_absolute():
        path = repo_root / path
    return path.resolve()


def _checklist() -> dict[int, dict[str, Any]]:
    document = read_json_object(_CHECKLIST_PATH)
    raw_items = document.get("items")
    if not isinstance(raw_items, list):
        raise HarnessError(f"{_CHECKLIST_PATH}: `items` must be an array")
    items: dict[int, dict[str, Any]] = {}
    for raw in raw_items:
        if not isinstance(raw, dict):
            raise HarnessError(f"{_CHECKLIST_PATH}: every item must be an object")
        number = raw.get("number")
        owner = raw.get("owner")
        title = raw.get("title")
        if (
            isinstance(number, bool)
            or not isinstance(number, int)
            or number < 1
            or owner not in {"shared", *TESTERS}
            or not isinstance(title, str)
            or not title
        ):
            raise HarnessError(f"{_CHECKLIST_PATH}: malformed checklist item {raw!r}")
        if number in items:
            raise HarnessError(f"{_CHECKLIST_PATH}: duplicate item {number}")
        items[number] = raw
    if set(items) != set(range(1, 37)):
        raise HarnessError(f"{_CHECKLIST_PATH}: checklist must contain exactly items 1 through 36")
    return items


def _tester_owns(owner: str, tester: str) -> bool:
    return owner == "shared" or owner == tester


def validate_receipt(
    receipt: dict[str, Any],
    *,
    verify_files: bool = True,
    expected_commit: str | None = None,
    repo_root: Path = _REPO_ROOT,
) -> list[str]:
    """Return every receipt defect; an empty list means the receipt is coherent."""
    errors: list[str] = []
    if _contains_home_path(receipt):
        errors.append("receipt contains the current user's home path")
    items = _checklist()
    if receipt.get("schema_version") != "talaria-v0.5.0-receipt-v1":
        errors.append("schema_version is not talaria-v0.5.0-receipt-v1")
    if receipt.get("release") != RELEASE_VERSION:
        errors.append(f"release is not {RELEASE_VERSION}")
    number = receipt.get("checklist_item")
    tester = receipt.get("tester")
    owner = receipt.get("owner")
    verdict = receipt.get("verdict")
    item = (
        items.get(number) if isinstance(number, int) and not isinstance(number, bool) else None
    )
    if item is None:
        errors.append("checklist_item is not an integer from 1 through 36")
    if tester not in TESTERS:
        errors.append("tester is not talaria-t1 or talaria-t2")
    if item is not None and owner != item["owner"]:
        errors.append(f"owner does not match checklist item {number}: expected {item['owner']!r}")
    if (
        item is not None
        and isinstance(tester, str)
        and not _tester_owns(str(item["owner"]), tester)
    ):
        errors.append(f"tester {tester} does not own checklist item {number}")
    # Receipt validation accumulates defects for the operator; record generation
    # fails immediately at its trust boundary, but both use this same vocabulary.
    if verdict not in VERDICTS:
        errors.append("verdict must be pass, fail, blocked, or reserved")

    try:
        artifact = _object(receipt.get("artifact"), field="artifact")
        for field in (
            "commit",
            "wheel_filename",
            "wheel_sha256",
            "version",
            "executable",
            "executable_sha256",
            "installed_files_sha256",
            "distribution_root",
            "install_receipt_path",
            "install_receipt_sha256",
        ):
            _string(artifact.get(field), field=f"artifact.{field}")
        if artifact.get("version") != RELEASE_VERSION:
            errors.append(f"artifact.version is not {RELEASE_VERSION}")
        if expected_commit is not None and artifact.get("commit") != expected_commit:
            errors.append("artifact.commit does not match the release candidate")
    except HarnessError as exc:
        errors.append(str(exc))

    try:
        terminal = _object(receipt.get("terminal"), field="terminal")
        _string(terminal.get("program"), field="terminal.program")
        _string(terminal.get("term"), field="terminal.term")
        for field in ("rows", "columns"):
            value = terminal.get(field)
            if isinstance(value, bool) or not isinstance(value, int) or value < 1:
                errors.append(f"terminal.{field} must be a positive integer")
    except HarnessError as exc:
        errors.append(str(exc))

    try:
        session = _object(receipt.get("session"), field="session")
        mode = session.get("mode")
        if mode not in {"live", "replay", "install-probe", "failure"}:
            errors.append("session.mode must be live, replay, install-probe, or failure")
        _string(session.get("profile"), field="session.profile")
        route = _object(session.get("model_route"), field="session.model_route")
        requested = route.get("requested")
        observed = route.get("observed")
        route_status = route.get("status")
        availability = route.get("fallback_availability")
        allowed_routes = {None, PRIMARY_MODEL_ROUTE, FALLBACK_MODEL_ROUTE}
        if requested not in allowed_routes:
            errors.append(f"model_route.requested is an unapproved route: {requested!r}")
        if observed not in allowed_routes:
            errors.append(f"model_route.observed is an unapproved route: {observed!r}")
        if route_status not in {"used", "not-reached", "not-applicable"}:
            errors.append("model_route.status must be used, not-reached, or not-applicable")
        if availability not in {"available", "unavailable", "not-checked", "not-applicable"}:
            errors.append("model_route.fallback_availability has an unknown value")

        reason = route.get("fallback_reason")
        fallback_involved = FALLBACK_MODEL_ROUTE in {requested, observed}
        if fallback_involved:
            if not isinstance(reason, dict):
                errors.append("a fallback request or use requires fallback_reason")
            else:
                if reason.get("code") not in FALLBACK_REASON_CODES:
                    errors.append(
                        "fallback_reason.code is not one of the four permitted reasons"
                    )
                try:
                    _string(
                        reason.get("detail"),
                        field="session.model_route.fallback_reason.detail",
                    )
                except HarnessError as exc:
                    errors.append(str(exc))
            if availability == "not-applicable":
                errors.append("fallback involvement requires a checked fallback availability")
        elif reason is not None:
            errors.append(
                "fallback_reason must be null when fallback was neither requested nor used"
            )

        if route_status == "used" and observed not in {PRIMARY_MODEL_ROUTE, FALLBACK_MODEL_ROUTE}:
            errors.append("model_route.status used requires an observed approved route")
        if route_status == "not-reached" and observed is not None:
            errors.append("model_route.status not-reached requires observed to be null")
        if route_status == "not-applicable" and (requested is not None or observed is not None):
            errors.append("model_route.status not-applicable requires both route fields to be null")
        if verdict == "pass" and mode == "live" and route_status != "used":
            errors.append("a passing live leg must observe the model route it used")
        if verdict == "pass" and observed == FALLBACK_MODEL_ROUTE and availability != "available":
            errors.append("a fallback leg cannot pass unless the named fallback was available")
        if verdict == "pass" and fallback_involved and availability == "unavailable":
            errors.append("a leg cannot pass when the named fallback was required but unavailable")
    except HarnessError as exc:
        errors.append(str(exc))

    try:
        evidence = _object(receipt.get("evidence"), field="evidence")
        redaction = evidence.get("redaction_review")
        if redaction not in {"passed", "withheld", "pending"}:
            errors.append("evidence.redaction_review must be passed, withheld, or pending")
        capture_path = _evidence_path(
            evidence.get("capture_path"), field="evidence.capture_path", repo_root=repo_root
        )
        capture_hash = _string(evidence.get("capture_sha256"), field="evidence.capture_sha256")
        screenshot_path = _evidence_path(
            evidence.get("screenshot_path"),
            field="evidence.screenshot_path",
            repo_root=repo_root,
        )
        screenshot_hash = _string(
            evidence.get("screenshot_sha256"), field="evidence.screenshot_sha256"
        )
        if verify_files:
            for path, expected_hash, label in (
                (capture_path, capture_hash, "capture"),
                (screenshot_path, screenshot_hash, "screenshot"),
            ):
                errors.extend(_verify_evidence_file(path, expected_hash, label=label))
        if verdict == "pass" and redaction != "passed":
            errors.append("a receipt cannot pass before capture and screenshot redaction review")
    except HarnessError as exc:
        errors.append(str(exc))
    return errors


def _verify_evidence_file(path: Path, expected_hash: str, *, label: str) -> list[str]:
    """Return missing or digest errors for any receipt-bound evidence file."""
    if not path.is_file():
        return [f"{label} file is missing: {path}"]
    if sha256_file(path) != expected_hash:
        return [f"{label} hash does not match its file: {path}"]
    return []


def _install_receipt(path: Path, tester: str) -> dict[str, Any]:
    receipt = read_json_object(path)
    if receipt.get("schema_version") != "talaria-v0.5.0-install-v1":
        raise HarnessError(f"{path}: not a Talaria v0.5.0 install receipt")
    if receipt.get("tester") != tester:
        raise HarnessError(f"{path}: install receipt belongs to a different tester")
    return receipt


def _path(value: Any, *, field: str) -> Path:
    return Path(_string(value, field=field)).expanduser().resolve()


def validate_scratch_evidence_paths(
    *, capture: Path, screenshot: Path, scratch_root: Path
) -> None:
    """Refuse evidence selected from outside the tester-owned scratch tree."""
    if not is_within(capture, scratch_root / "raw"):
        raise HarnessError("raw capture escaped the tester scratch raw directory")
    if not is_within(screenshot, scratch_root / "screenshots"):
        raise HarnessError(
            "screenshot must remain in the tester scratch screenshots directory"
        )


def record_receipt(args: argparse.Namespace) -> Path:
    tester = validate_tester(args.tester)
    item = _checklist().get(args.item)
    if item is None:
        raise HarnessError("checklist item must be from 1 through 36")
    owner = str(item["owner"])
    if not _tester_owns(owner, tester):
        raise HarnessError(f"{tester} does not own checklist item {args.item}")

    install_path = args.install_receipt.expanduser().resolve()
    install = _install_receipt(install_path, tester)
    scratch_root = _path(install.get("scratch_root"), field="install.scratch_root")
    pty_path = args.pty_result.expanduser().resolve()
    pty_result = read_json_object(pty_path)
    if pty_result.get("schema_version") != "talaria-v0.5.0-pty-v1":
        raise HarnessError(f"{pty_path}: not a v0.5.0 pseudo-terminal result")
    if pty_result.get("tester") != tester:
        raise HarnessError(f"{pty_path}: pseudo-terminal result belongs to a different tester")
    capture = _object(pty_result.get("capture"), field="pty.capture")
    capture_path = _path(capture.get("path"), field="pty.capture.path")
    screenshot = args.screenshot.expanduser().resolve()
    validate_scratch_evidence_paths(
        capture=capture_path,
        screenshot=screenshot,
        scratch_root=scratch_root,
    )
    if not screenshot.is_file():
        raise HarnessError(f"screenshot does not exist: {screenshot}")

    candidate = _object(install.get("candidate"), field="install.candidate")
    artifact = _object(install.get("artifact"), field="install.artifact")
    requested = _ROUTE_ALIASES[args.route_requested]
    observed = _ROUTE_ALIASES[args.route_observed]
    fallback_reason: dict[str, str] | None = None
    if args.fallback_reason_code is not None or args.fallback_reason_detail is not None:
        if args.fallback_reason_code is None or args.fallback_reason_detail is None:
            raise HarnessError("fallback reason code and exact detail must be supplied together")
        fallback_reason = {
            "code": args.fallback_reason_code,
            "detail": args.fallback_reason_detail,
        }

    terminal_program = _string(
        pty_result.get("terminal_program"), field="pty.terminal_program"
    )
    term = _string(pty_result.get("term"), field="pty.term")
    rows = pty_result.get("rows")
    columns = pty_result.get("columns")
    receipt = {
        "schema_version": "talaria-v0.5.0-receipt-v1",
        "release": RELEASE_VERSION,
        "recorded_at": _utc_now(),
        "checklist_item": args.item,
        "title": item["title"],
        "owner": owner,
        "tester": tester,
        "verdict": args.verdict,
        "artifact": {
            "commit": candidate["commit"],
            "wheel_filename": candidate["wheel_filename"],
            "wheel_sha256": candidate["wheel_sha256"],
            "version": artifact["version"],
            "executable": artifact["executable"],
            "executable_sha256": artifact["executable_sha256"],
            "installed_files_sha256": artifact["installed_files_sha256"],
            "distribution_root": artifact["distribution_root"],
            "install_receipt_path": str(install_path),
            "install_receipt_sha256": sha256_file(install_path),
        },
        "terminal": {
            "program": terminal_program,
            "term": term,
            "rows": rows,
            "columns": columns,
        },
        "session": {
            "mode": args.session_mode,
            "profile": args.session_profile,
            "model_route": {
                "requested": requested,
                "observed": observed,
                "status": args.route_status,
                "fallback_reason": fallback_reason,
                "fallback_availability": args.fallback_availability,
            },
        },
        "evidence": {
            "capture_path": str(capture_path),
            "capture_sha256": capture["sha256"],
            "screenshot_path": str(screenshot),
            "screenshot_sha256": sha256_file(screenshot),
            "redaction_review": args.redaction_review,
            "pty_result_path": str(pty_path),
            "pty_result_sha256": sha256_file(pty_path),
        },
        "observations": args.observation,
    }
    errors = validate_receipt(receipt)
    if errors:
        raise HarnessError("receipt is invalid:\n- " + "\n- ".join(errors))
    output = Path(args.output).expanduser().resolve()
    if not is_within(output, scratch_root / "receipts"):
        raise HarnessError("receipt output must stay in the tester scratch receipts directory")
    write_json_object(output, receipt)
    return output


def _repo_relative(path: Path, *, repo_root: Path) -> str:
    try:
        return path.resolve().relative_to(repo_root.resolve()).as_posix()
    except ValueError as exc:
        raise HarnessError(f"published evidence escaped the repository: {path}") from exc


def _copy_new(source: Path, destination: Path) -> None:
    if not source.is_file():
        raise HarnessError(f"evidence source does not exist: {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        raise HarnessError(f"refusing to replace published evidence: {destination}")
    shutil.copyfile(source, destination)


def _copy_public_capture(
    source: Path, destination: Path, *, scratch_root: Path
) -> None:
    """Copy terminal bytes while replacing only private scratch-root identifiers."""
    if not source.is_file():
        raise HarnessError(f"evidence source does not exist: {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        raise HarnessError(f"refusing to replace published evidence: {destination}")
    data = source.read_bytes().replace(
        str(scratch_root.resolve()).encode(), b"<scratch-root>"
    )
    destination.write_bytes(
        _MACOS_ACCEPTANCE_SCRATCH_PATH.sub(b"<scratch-root>", data)
    )


def _portable_json(
    value: Any, *, repo_root: Path, scratch_root: Path | None = None
) -> Any:
    """Return public JSON with checkout paths made relative and home paths redacted."""
    if isinstance(value, dict):
        return {
            key: _portable_json(item, repo_root=repo_root, scratch_root=scratch_root)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [
            _portable_json(item, repo_root=repo_root, scratch_root=scratch_root)
            for item in value
        ]
    if not isinstance(value, str):
        return value
    repository = str(repo_root.resolve())
    home = str(Path.home().resolve())
    scratch = str(scratch_root.resolve()) if scratch_root is not None else None
    if scratch is not None and value == scratch:
        return "<scratch-root>"
    if scratch is not None and value.startswith(f"{scratch}/"):
        return f"<scratch-root>/{value.removeprefix(f'{scratch}/')}"
    if value == repository:
        return "."
    if value.startswith(f"{repository}/"):
        return value.removeprefix(f"{repository}/")
    return value.replace(home, "<home>")


def _has_superseded_path(receipt: dict[str, Any]) -> bool:
    artifact = receipt.get("artifact")
    evidence = receipt.get("evidence")
    path_values: list[Any] = []
    if isinstance(artifact, dict):
        path_values.append(artifact.get("install_receipt_path"))
    if isinstance(evidence, dict):
        path_values.extend(
            evidence.get(field)
            for field in ("capture_path", "screenshot_path", "pty_result_path")
        )
    return any(
        isinstance(value, str) and "superseded" in Path(value).parts
        for value in path_values
    )


def _png_ancillary_payloads(data: bytes) -> list[bytes]:
    """Return Portable Network Graphics ancillary chunks without image pixels."""
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        return [data]
    payloads: list[bytes] = []
    offset = 8
    while offset + 12 <= len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        chunk_type = data[offset + 4 : offset + 8]
        end = offset + 12 + length
        if end > len(data):
            return [data]
        if chunk_type[:1].islower():
            payloads.append(data[offset + 8 : offset + 8 + length])
        offset = end
    return payloads


def _privacy_errors(path: Path) -> list[str]:
    try:
        data = path.read_bytes()
    except OSError as exc:
        return [f"cannot privacy-scan {path}: {exc}"]
    payloads = _png_ancillary_payloads(data) if path.suffix.lower() == ".png" else [data]
    errors: list[str] = []
    for pattern, label in _PRIVATE_PATTERNS:
        if any(pattern.search(payload) for payload in payloads):
            errors.append(f"{path}: contains a private {label}")
    return errors


def _release_candidate_matches(
    evidence_commit: str, expected_commit: str, *, repo_root: Path
) -> tuple[bool, str]:
    if evidence_commit == expected_commit:
        return True, "exact"
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", evidence_commit, expected_commit],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    if ancestor.returncode != 0:
        return False, "evidence candidate is not an ancestor of the released commit"
    diff = subprocess.run(
        [
            "git",
            "diff",
            "--quiet",
            evidence_commit,
            expected_commit,
            "--",
            *_RELEASE_RELEVANT_PATHS,
        ],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    if diff.returncode == 0:
        return True, "release-relevant files are identical"
    if diff.returncode == 1:
        return False, "release-relevant files differ from the evidence candidate"
    detail = diff.stderr.strip() or diff.stdout.strip() or "unknown Git error"
    return False, f"cannot compare release-relevant files: {detail}"


def _write_portable_json(
    source: Path,
    destination: Path,
    *,
    repo_root: Path,
    scratch_root: Path | None = None,
) -> None:
    if destination.exists():
        raise HarnessError(f"refusing to replace published evidence: {destination}")
    document = read_json_object(source)
    destination.parent.mkdir(parents=True, exist_ok=True)
    write_json_object(
        destination,
        _portable_json(document, repo_root=repo_root, scratch_root=scratch_root),
    )


def _bind_pty_result_to_capture(pty_result: Path, capture: Path) -> None:
    """Update copied pseudo-terminal metadata for a sanitized public capture."""
    document = read_json_object(pty_result)
    capture_metadata = document.get("capture")
    if not isinstance(capture_metadata, dict):
        return
    capture_metadata["bytes"] = capture.stat().st_size
    capture_metadata["sha256"] = sha256_file(capture)
    write_json_object(pty_result, document, replace=True)


def publish_receipt(
    source_path: Path,
    *,
    public_install_receipt: Path,
    evidence_root: Path = _EVIDENCE_ROOT,
    repo_root: Path = _REPO_ROOT,
) -> Path:
    """Publish one reviewed scratch receipt with checkout-relative evidence paths."""
    source_path = source_path.expanduser().resolve()
    public_install_receipt = public_install_receipt.expanduser().resolve()
    evidence_root = evidence_root.expanduser().resolve()
    repo_root = repo_root.expanduser().resolve()
    if not is_within(evidence_root, repo_root):
        raise HarnessError("published evidence root must stay inside the repository")
    if not is_within(public_install_receipt, evidence_root):
        raise HarnessError("public install receipt must stay inside the evidence root")

    receipt = read_json_object(source_path)
    source_errors = validate_receipt(receipt, verify_files=True, repo_root=repo_root)
    if source_errors:
        raise HarnessError("scratch receipt is invalid:\n- " + "\n- ".join(source_errors))
    tester = validate_tester(_string(receipt.get("tester"), field="tester"))
    number = receipt.get("checklist_item")
    if isinstance(number, bool) or not isinstance(number, int) or number not in range(1, 37):
        raise HarnessError("checklist_item must be from 1 through 36")

    public_install = _install_receipt(public_install_receipt, tester)
    candidate = _object(public_install.get("candidate"), field="install.candidate")
    expected_commit = _string(candidate.get("commit"), field="install.candidate.commit")
    artifact = _object(receipt.get("artifact"), field="artifact")
    if artifact.get("commit") != expected_commit:
        raise HarnessError("scratch receipt does not match the public install receipt candidate")

    tester_root = evidence_root / tester.removeprefix("talaria-")
    evidence = _object(receipt.get("evidence"), field="evidence")
    capture_source = _evidence_path(
        evidence.get("capture_path"), field="evidence.capture_path", repo_root=repo_root
    )
    scratch_root = capture_source.parents[1]
    screenshot_source = _evidence_path(
        evidence.get("screenshot_path"), field="evidence.screenshot_path", repo_root=repo_root
    )
    pty_source = _evidence_path(
        evidence.get("pty_result_path"), field="evidence.pty_result_path", repo_root=repo_root
    )
    capture_destination = tester_root / "raw" / capture_source.name
    screenshot_destination = tester_root / "screenshots" / screenshot_source.name
    pty_destination = tester_root / "pty-results" / pty_source.name
    output = tester_root / "receipts" / f"item-{number:02d}-{tester}.json"

    _copy_public_capture(
        capture_source,
        capture_destination,
        scratch_root=scratch_root,
    )
    _copy_new(screenshot_source, screenshot_destination)
    _write_portable_json(
        pty_source,
        pty_destination,
        repo_root=repo_root,
        scratch_root=scratch_root,
    )
    _bind_pty_result_to_capture(pty_destination, capture_destination)
    evidence["capture_path"] = _repo_relative(capture_destination, repo_root=repo_root)
    evidence["capture_sha256"] = sha256_file(capture_destination)
    evidence["screenshot_path"] = _repo_relative(screenshot_destination, repo_root=repo_root)
    evidence["pty_result_path"] = _repo_relative(pty_destination, repo_root=repo_root)
    evidence["pty_result_sha256"] = sha256_file(pty_destination)
    artifact["install_receipt_path"] = _repo_relative(
        public_install_receipt, repo_root=repo_root
    )
    artifact["install_receipt_sha256"] = sha256_file(public_install_receipt)
    portable_receipt = _portable_json(
        receipt,
        repo_root=repo_root,
        scratch_root=scratch_root,
    )
    if not isinstance(portable_receipt, dict):
        raise HarnessError("published receipt must remain a JSON object")
    receipt = portable_receipt

    errors = validate_receipt(
        receipt,
        verify_files=True,
        expected_commit=expected_commit,
        repo_root=repo_root,
    )
    if errors:
        raise HarnessError("published receipt is invalid:\n- " + "\n- ".join(errors))
    write_json_object(output, receipt)
    return output


def _manifest_candidate(manifest: dict[str, Any]) -> dict[str, Any] | None:
    candidate = manifest.get("current_candidate", manifest.get("candidate"))
    if candidate is None:
        return None
    return _object(candidate, field="manifest.current_candidate")


def verify_run(
    manifest_path: Path = _MANIFEST_PATH,
    *,
    evidence_root: Path = _EVIDENCE_ROOT,
    repo_root: Path = _REPO_ROOT,
    expected_candidate_commit: str | None = None,
) -> list[str]:
    """Bind every active receipt and its files to the manifest's current candidate."""
    manifest_path = manifest_path.expanduser().resolve()
    evidence_root = evidence_root.expanduser().resolve()
    repo_root = repo_root.expanduser().resolve()
    manifest = read_json_object(manifest_path)
    receipt_paths = sorted(evidence_root.glob("*/receipts/*.json"))
    install_paths = sorted(evidence_root.glob("*/install-receipt.json"))
    errors: list[str] = []

    try:
        candidate = _manifest_candidate(manifest)
    except HarnessError as exc:
        errors.append(str(exc))
        candidate = None
    if candidate is None:
        if receipt_paths or install_paths:
            errors.append("manifest candidate is null while receipts exist")
        return errors

    try:
        expected_commit = _string(candidate.get("commit"), field="manifest.candidate.commit")
        expected_wheel = _string(
            candidate.get("wheel_sha256"), field="manifest.candidate.wheel_sha256"
        )
    except HarnessError as exc:
        errors.append(str(exc))
        return errors
    if expected_candidate_commit is not None:
        matches, reason = _release_candidate_matches(
            expected_commit, expected_candidate_commit, repo_root=repo_root
        )
        if not matches:
            errors.append(
                "manifest candidate does not describe the released commit "
                f"{expected_candidate_commit}: {reason}"
            )

    manifest_receipts = manifest.get("receipts")
    if not isinstance(manifest_receipts, list):
        errors.append("manifest.receipts must be an array")
        manifest_receipts = []
    manifest_installs = manifest.get("install_receipts", [])
    if not isinstance(manifest_installs, list):
        errors.append("manifest.install_receipts must be an array")
        manifest_installs = []
    receipt_entries: dict[str, dict[str, Any]] = {}
    for raw_entry in manifest_receipts:
        if isinstance(raw_entry, dict) and isinstance(raw_entry.get("receipt_path"), str):
            receipt_entries[raw_entry["receipt_path"]] = raw_entry
    install_entries: dict[str, dict[str, Any]] = {}
    for raw_entry in manifest_installs:
        if isinstance(raw_entry, dict) and isinstance(raw_entry.get("receipt_path"), str):
            install_entries[raw_entry["receipt_path"]] = raw_entry

    active_receipt_names: set[str] = set()
    for path in receipt_paths:
        relative = _repo_relative(path, repo_root=repo_root)
        active_receipt_names.add(relative)
        receipt = read_json_object(path)
        if _has_superseded_path(receipt):
            errors.append(f"{relative}: active receipt names a superseded evidence origin")
        for error in validate_receipt(
            receipt,
            verify_files=True,
            expected_commit=expected_commit,
            repo_root=repo_root,
        ):
            errors.append(f"{relative}: {error}")
        artifact = receipt.get("artifact")
        if isinstance(artifact, dict) and artifact.get("wheel_sha256") != expected_wheel:
            errors.append(f"{relative}: artifact.wheel_sha256 does not match the release candidate")
        entry = receipt_entries.get(relative)
        if entry is None:
            errors.append(f"receipt is absent from manifest: {relative}")
        else:
            if entry.get("receipt_sha256") != sha256_file(path):
                errors.append(f"manifest receipt digest does not match: {relative}")
            for receipt_field, manifest_field in (
                ("checklist_item", "checklist_item"),
                ("tester", "tester"),
                ("verdict", "verdict"),
            ):
                if entry.get(manifest_field) != receipt.get(receipt_field):
                    errors.append(f"manifest {manifest_field} does not match: {relative}")

    for relative in sorted(set(receipt_entries) - active_receipt_names):
        errors.append(f"manifest names an absent receipt: {relative}")

    active_install_names: set[str] = set()
    for path in install_paths:
        relative = _repo_relative(path, repo_root=repo_root)
        active_install_names.add(relative)
        install = read_json_object(path)
        if _contains_home_path(install):
            errors.append(f"{relative}: install receipt contains the current user's home path")
        try:
            install_candidate = _object(
                install.get("candidate"), field=f"{relative}: candidate"
            )
            if install_candidate.get("commit") != expected_commit:
                errors.append(f"{relative}: candidate commit does not match the manifest")
            if install_candidate.get("wheel_sha256") != expected_wheel:
                errors.append(f"{relative}: wheel digest does not match the manifest")
        except HarnessError as exc:
            errors.append(str(exc))
        entry = install_entries.get(relative)
        if entry is None:
            errors.append(f"install receipt is absent from manifest: {relative}")
        elif entry.get("receipt_sha256") != sha256_file(path):
            errors.append(f"manifest install receipt digest does not match: {relative}")

    for relative in sorted(set(install_entries) - active_install_names):
        errors.append(f"manifest names an absent install receipt: {relative}")
    if manifest.get("status") == "not-run" and (receipt_paths or install_paths):
        errors.append("manifest status is not-run while receipts exist")
    evidence_paths = sorted(
        candidate_path
        for candidate_path in evidence_root.rglob("*")
        if candidate_path.is_file()
    )
    for path in evidence_paths:
        for error in _privacy_errors(path):
            errors.append(error.replace(str(path), _repo_relative(path, repo_root=repo_root), 1))
    return errors


def validate_matrix(directory: Path) -> list[str]:
    items = _checklist()
    expected = {
        (number, tester)
        for number, item in items.items()
        for tester in TESTERS
        if _tester_owns(str(item["owner"]), tester)
    }
    found: dict[tuple[int, str], Path] = {}
    errors: list[str] = []
    for path in sorted(directory.glob("*.json")):
        receipt = read_json_object(path)
        number = receipt.get("checklist_item")
        tester = receipt.get("tester")
        if not isinstance(number, int) or not isinstance(tester, str):
            errors.append(f"{path}: no checklist_item/tester identity")
            continue
        key = (number, tester)
        if key in found:
            errors.append(f"duplicate receipt {key}: {found[key]} and {path}")
            continue
        found[key] = path
        for error in validate_receipt(receipt):
            errors.append(f"{path}: {error}")
        if receipt.get("verdict") not in TERMINAL_VERDICTS:
            errors.append(f"{path}: terminal verdict is {receipt.get('verdict')!r}")
    for key in sorted(expected - set(found)):
        errors.append(f"missing receipt for item {key[0]} and tester {key[1]}")
    for key in sorted(set(found) - expected):
        errors.append(f"unexpected receipt for item {key[0]} and tester {key[1]}")
    return errors


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    record = subparsers.add_parser("record", help="create one immutable item receipt")
    record.add_argument("--install-receipt", type=Path, required=True)
    record.add_argument("--pty-result", type=Path, required=True)
    record.add_argument("--screenshot", type=Path, required=True)
    record.add_argument("--output", type=Path, required=True)
    record.add_argument("--tester", required=True)
    record.add_argument("--item", type=int, required=True)
    record.add_argument("--verdict", choices=tuple(sorted(VERDICTS)), required=True)
    record.add_argument(
        "--session-mode",
        choices=("live", "replay", "install-probe", "failure"),
        required=True,
    )
    record.add_argument("--session-profile", required=True)
    record.add_argument("--route-requested", choices=tuple(_ROUTE_ALIASES), required=True)
    record.add_argument("--route-observed", choices=tuple(_ROUTE_ALIASES), required=True)
    record.add_argument(
        "--route-status", choices=("used", "not-reached", "not-applicable"), required=True
    )
    record.add_argument(
        "--fallback-availability",
        choices=("available", "unavailable", "not-checked", "not-applicable"),
        required=True,
    )
    record.add_argument("--fallback-reason-code", choices=tuple(sorted(FALLBACK_REASON_CODES)))
    record.add_argument("--fallback-reason-detail")
    record.add_argument(
        "--redaction-review", choices=("passed", "withheld", "pending"), required=True
    )
    record.add_argument("--observation", action="append", default=[])

    publish = subparsers.add_parser(
        "publish", help="copy one reviewed scratch receipt into repository evidence"
    )
    publish.add_argument("receipt", type=Path)
    publish.add_argument("--public-install-receipt", type=Path, required=True)
    publish.add_argument("--evidence-root", type=Path, default=_EVIDENCE_ROOT)

    validate = subparsers.add_parser("validate", help="validate one existing receipt")
    validate.add_argument("receipt", type=Path)
    validate.add_argument("--skip-file-checks", action="store_true")
    validate.add_argument("--manifest", type=Path)

    matrix = subparsers.add_parser("validate-matrix", help="require every owned receipt")
    matrix.add_argument("directory", type=Path)
    verify = subparsers.add_parser(
        "verify-run", help="verify active receipts against the generated manifest"
    )
    verify.add_argument("--manifest", type=Path, default=_MANIFEST_PATH)
    verify.add_argument("--evidence-root", type=Path, default=_EVIDENCE_ROOT)
    verify.add_argument("--expect-candidate")
    return parser


def _main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.command == "record":
        path = record_receipt(args)
        print(path)
        return 0 if args.verdict in TERMINAL_VERDICTS else 1
    if args.command == "publish":
        path = publish_receipt(
            args.receipt,
            public_install_receipt=args.public_install_receipt,
            evidence_root=args.evidence_root,
        )
        print(path)
        return 0
    if args.command == "validate":
        expected_commit = None
        if args.manifest is not None:
            manifest_candidate = _manifest_candidate(read_json_object(args.manifest))
            if manifest_candidate is None:
                raise HarnessError("manifest candidate is null")
            expected_commit = _string(
                manifest_candidate.get("commit"), field="manifest.candidate.commit"
            )
        errors = validate_receipt(
            read_json_object(args.receipt),
            verify_files=not args.skip_file_checks,
            expected_commit=expected_commit,
        )
    elif args.command == "validate-matrix":
        errors = validate_matrix(args.directory)
    else:
        errors = verify_run(
            args.manifest,
            evidence_root=args.evidence_root,
            expected_candidate_commit=args.expect_candidate,
        )
    if errors:
        for error in errors:
            print(f"v050_receipt: {error}", file=sys.stderr)
        return 1
    print("valid")
    return 0


def main(argv: list[str] | None = None) -> int:
    try:
        return _main(argv)
    except HarnessError as exc:
        print(f"v050_receipt: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
