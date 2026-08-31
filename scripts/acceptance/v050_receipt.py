#!/usr/bin/env python3
"""Create and validate one evidence receipt per v0.5.0 checklist item and tester."""

from __future__ import annotations

import argparse
import datetime as dt
import sys
from pathlib import Path
from typing import Any

from scripts.acceptance.v050_common import (
    FALLBACK_MODEL_ROUTE,
    FALLBACK_REASON_CODES,
    PRIMARY_MODEL_ROUTE,
    RELEASE_VERSION,
    TESTERS,
    HarnessError,
    is_within,
    read_json_object,
    sha256_file,
    validate_tester,
    write_json_object,
)

_REPO_ROOT = Path(__file__).resolve().parents[2]
_CHECKLIST_PATH = _REPO_ROOT / "docs" / "acceptance" / "v0.5.0" / "checklist-items.json"
_ROUTE_ALIASES: dict[str, str | None] = {
    "primary": PRIMARY_MODEL_ROUTE,
    "fallback": FALLBACK_MODEL_ROUTE,
    "none": None,
}


def _utc_now() -> str:
    return dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat()


def _object(value: Any, *, field: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise HarnessError(f"receipt field {field} must be an object")
    return value


def _string(value: Any, *, field: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise HarnessError(f"receipt field {field} must be a non-empty string")
    return value


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


def validate_receipt(receipt: dict[str, Any], *, verify_files: bool = True) -> list[str]:
    """Return every receipt defect; an empty list means the receipt is coherent."""
    errors: list[str] = []
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
    if verdict not in {"pass", "fail", "blocked", "reserved"}:
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
        capture_path = Path(_string(evidence.get("capture_path"), field="evidence.capture_path"))
        capture_hash = _string(evidence.get("capture_sha256"), field="evidence.capture_sha256")
        screenshot_path = Path(
            _string(evidence.get("screenshot_path"), field="evidence.screenshot_path")
        )
        screenshot_hash = _string(
            evidence.get("screenshot_sha256"), field="evidence.screenshot_sha256"
        )
        if verify_files:
            for path, expected_hash, label in (
                (capture_path, capture_hash, "capture"),
                (screenshot_path, screenshot_hash, "screenshot"),
            ):
                if not path.is_file():
                    errors.append(f"{label} file is missing: {path}")
                elif sha256_file(path) != expected_hash:
                    errors.append(f"{label} hash does not match its file: {path}")
        if verdict == "pass" and redaction != "passed":
            errors.append("a receipt cannot pass before capture and screenshot redaction review")
    except HarnessError as exc:
        errors.append(str(exc))
    return errors


def _install_receipt(path: Path, tester: str) -> dict[str, Any]:
    receipt = read_json_object(path)
    if receipt.get("schema_version") != "talaria-v0.5.0-install-v1":
        raise HarnessError(f"{path}: not a Talaria v0.5.0 install receipt")
    if receipt.get("tester") != tester:
        raise HarnessError(f"{path}: install receipt belongs to a different tester")
    return receipt


def _path(value: Any, *, field: str) -> Path:
    return Path(_string(value, field=field)).expanduser().resolve()


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
    if not is_within(capture_path, scratch_root / "raw"):
        raise HarnessError("raw capture escaped the tester scratch raw directory")
    screenshot = args.screenshot.expanduser().resolve()
    if not is_within(screenshot, scratch_root / "screenshots"):
        raise HarnessError("screenshot must remain in the tester scratch screenshots directory")
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
        if receipt.get("verdict") not in {"pass", "reserved"}:
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
    record.add_argument("--verdict", choices=("pass", "fail", "blocked", "reserved"), required=True)
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

    validate = subparsers.add_parser("validate", help="validate one existing receipt")
    validate.add_argument("receipt", type=Path)
    validate.add_argument("--skip-file-checks", action="store_true")

    matrix = subparsers.add_parser("validate-matrix", help="require every owned receipt")
    matrix.add_argument("directory", type=Path)
    return parser


def _main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.command == "record":
        path = record_receipt(args)
        print(path)
        return 0 if args.verdict in {"pass", "reserved"} else 1
    if args.command == "validate":
        errors = validate_receipt(
            read_json_object(args.receipt), verify_files=not args.skip_file_checks
        )
    else:
        errors = validate_matrix(args.directory)
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
