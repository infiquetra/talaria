"""Sanitized v0.6.2 configuration evidence harness (CFG-P1 Test Author Two).

Owns ledger validation, coverage-shape checks, stage cleanup receipts, the
A1-11–19 remote reuse map for A1-27, A1-28 pair enumeration, A1-19-only
multiplexer preflight, and a request guard that refuses host-administration
writes. It never implements Talaria production settings clients.

Credentials never enter this module's outputs. Live Hermes mutation is
opt-in through ``V062_CFG_LIVE_HERMES`` (not a ``TALARIA_*`` export) and still
requires an operator-prepared 0600 credentials path — never a token value.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any, Final, Literal
from urllib.parse import urlparse

from jsonschema import Draft202012Validator
from jsonschema.exceptions import ValidationError

REPO_ROOT = Path(__file__).resolve().parents[2]
LEDGER_SCHEMA_PATH = (
    REPO_ROOT / "docs" / "acceptance" / "v0.6.2" / "configuration" / "ledger.schema.json"
)
EVIDENCE_ROOT_REL = "docs/acceptance/v0.6.2/configuration"
LEDGER_SCHEMA_VERSION = "talaria-v0.6.2-configuration-ledger-v1"
CLEANUP_SCHEMA_VERSION = "talaria-v0.6.2-configuration-cleanup-v1"
CLEANUP_RECORD_TYPE = "configuration-cleanup"

ConnectionLabel = Literal["local", "remote", "stub", "installed"]
StageName = Literal["active", "installed"]
LedgerStatus = Literal["passed", "failed", "blocked", "skipped", "not_run"]

LOCAL_API_BASE = "http://127.0.0.1:8765"
REMOTE_API_BASE = "http://10.220.1.139:8765"
LOCAL_WS_ENDPOINT = "ws://127.0.0.1:8765/api/ws"
REMOTE_WS_ENDPOINT = "ws://10.220.1.139:8765/api/ws"

LIVE_HERMES_FLAG = "V062_CFG_LIVE_HERMES"
LIVE_CREDENTIALS_PATH_FLAG = "V062_CFG_CREDENTIALS_PATH"
TEST_SECRET_KEY = "TALARIA_V062_TEST_SECRET"

A1_LOCAL_CONTRACT_IDS: Final[tuple[str, ...]] = tuple(
    f"A1-{index}" for index in range(11, 21)
)
A1_REMOTE_REUSE_IDS: Final[tuple[str, ...]] = tuple(
    f"A1-{index}" for index in range(11, 20)
)

FORBIDDEN_MUTATIONS: Final[frozenset[tuple[str, str]]] = frozenset(
    {
        ("POST", "/api/hermes/update"),
        ("POST", "/api/gateway/migrate"),
        ("POST", "/api/profiles/active"),
        ("POST", "/api/profiles/import"),
    }
)
FORBIDDEN_PATH_PREFIX_MUTATIONS: Final[tuple[str, ...]] = ("/api/local-models/",)
READONLY_HOST_STATUS_PATHS: Final[frozenset[str]] = frozenset(
    {
        "/api/hermes/update/check",
        "/api/hermes/update/receipt",
        "/api/gateway/migrate/plan",
        "/api/local-models/status",
        "/api/local-models/catalog",
        "/api/local-models/hardware",
        "/api/local-models/jobs",
        "/api/local-models/search",
    }
)

_PROFILE_NAME_RE = re.compile(
    r"\Atalaria-v0\.6\.2-cfg-t0-(local|remote)-(active|installed)-(a|switch|clone|renamed)\Z"
)
P2_PROFILE_NAME_RE = re.compile(
    r"\Atalaria-v062-cfg-p2-(local|remote)-(active|installed)-(a|switch|clone|renamed)\Z"
)
P2_HERMES_NAME_RE = re.compile(r"\A[a-z0-9][a-z0-9_-]{0,63}\Z")
_CANARY_HINTS = (
    "token",
    "ticket",
    "password",
    "refresh_token",
    "access_token",
    "authorization",
    "api_key",
)


class HarnessError(RuntimeError):
    """A contract violation that makes configuration evidence untrustworthy."""


def ledger_schema() -> dict[str, Any]:
    raw = json.loads(LEDGER_SCHEMA_PATH.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise HarnessError(f"{LEDGER_SCHEMA_PATH}: expected a JSON object")
    return raw


def validate_ledger_document(document: Mapping[str, Any]) -> None:
    rows = document.get("rows")
    if not isinstance(rows, list) or not rows:
        raise HarnessError("ledger.rows must be a non-empty array")
    try:
        Draft202012Validator(ledger_schema()).validate(document)
    except ValidationError as exc:
        raise HarnessError(f"ledger schema: {exc.message}") from exc
    for index, row in enumerate(rows):
        if not isinstance(row, Mapping):
            raise HarnessError(f"ledger.rows[{index}] must be an object")
        validate_ledger_row(row)


def validate_ledger_row(row: Mapping[str, Any]) -> None:
    required = (
        "test_id",
        "status",
        "talaria_version",
        "talaria_commit",
        "hermes_version",
        "connection_label",
        "profile_label",
        "observed_at",
        "evidence_refs",
        "limitation",
    )
    missing = [key for key in required if key not in row]
    if missing:
        raise HarnessError(f"ledger row missing {missing}")
    status = row["status"]
    refs = row["evidence_refs"]
    if status == "passed" and (not isinstance(refs, list) or len(refs) == 0):
        raise HarnessError("passed requires non-empty evidence_refs")
    if not isinstance(row["profile_label"], str) or not row["profile_label"]:
        raise HarnessError("profile_label must be a non-empty string")
    _refuse_secret_payload(row)


def disposable_profile_name(connection: ConnectionLabel, stage: StageName, alias: str) -> str:
    """Exact reserved disposable name for one connection/stage/alias."""
    suffix = {
        "testA": "a",
        "testE": "switch",
        "testC": "clone",
        "testD": "renamed",
    }.get(alias)
    if suffix is None or connection not in {"local", "remote"}:
        raise HarnessError(f"unsupported disposable alias {alias!r} on {connection}")
    name = f"talaria-v0.6.2-cfg-t0-{connection}-{stage}-{suffix}"
    if _PROFILE_NAME_RE.fullmatch(name) is None:
        raise HarnessError(f"disposable name {name!r} failed the reserved pattern")
    return name


def require_legal_p2_name(name: str) -> str:
    """Reject dotted legacy candidates and any name outside the P2 family."""
    if "." in name:
        raise HarnessError(f"disposable name {name!r} contains a dot")
    if len(name) > 64:
        raise HarnessError(f"disposable name {name!r} exceeds 64 characters")
    if P2_PROFILE_NAME_RE.fullmatch(name) is None:
        raise HarnessError(f"disposable name {name!r} is not the P2 reserved family")
    if P2_HERMES_NAME_RE.fullmatch(name) is None:
        raise HarnessError(f"disposable name {name!r} is not a legal Hermes profile")
    return name


def product_gateway_lifecycle_accepted(
    recorded_paths: Sequence[str], *, harness_posted: bool
) -> bool:
    """A harness POST is setup only and cannot pass P2-4."""
    if harness_posted:
        return False
    if any("/api/rpc" in path or "wake." in path for path in recorded_paths):
        return False
    return any(
        "/api/gateway/start" in path or "/api/gateway/stop" in path
        for path in recorded_paths
    )


def stage_name_set(connection: ConnectionLabel, stage: StageName) -> dict[str, str]:
    return {
        alias: disposable_profile_name(connection, stage, alias)
        for alias in ("testA", "testE", "testC", "testD")
    }


def remote_reuse_map() -> dict[str, str]:
    """A1-27 reuses A1-11–19 against the remote connection; it does not invent IDs."""
    return {source: "A1-27" for source in A1_REMOTE_REUSE_IDS}


def switch_pairs() -> tuple[tuple[str, str, str, str], ...]:
    """A1-28 ordered pairs: every connection×mutable-profile combination.

    Save is legal only to testA or testE created by this run.
    """
    connections = ("local", "remote")
    profiles = ("testA", "testE")
    pairs: list[tuple[str, str, str, str]] = []
    for from_connection in connections:
        for from_profile in profiles:
            for to_connection in connections:
                for to_profile in profiles:
                    if (from_connection, from_profile) == (to_connection, to_profile):
                        continue
                    pairs.append((from_connection, from_profile, to_connection, to_profile))
    return tuple(pairs)


def multiplexer_preflight_is_batch_gate() -> bool:
    """A1-19/J7 only. Other scenarios must not wait on this check."""
    return False


def live_hermes_enabled(connection: ConnectionLabel) -> bool:
    raw = os.environ.get(LIVE_HERMES_FLAG, "").strip().lower()
    if not raw:
        return False
    if connection == "local":
        return raw in {"local", "all", "1", "true"}
    if connection == "remote":
        return raw in {"remote", "all"}
    return False


def live_credentials_path() -> Path | None:
    raw = os.environ.get(LIVE_CREDENTIALS_PATH_FLAG, "").strip()
    if not raw:
        return None
    path = Path(raw)
    if not path.is_file():
        raise HarnessError("live credentials path is not a file")
    mode = path.stat().st_mode & 0o777
    if mode & 0o077:
        raise HarnessError("live credentials file must be no looser than 0600")
    return path


def access_limitation(connection: ConnectionLabel) -> str:
    return (
        f"blocked: authenticated {connection} Hermes access is absent (CFG-T0); "
        f"set {LIVE_HERMES_FLAG} and {LIVE_CREDENTIALS_PATH_FLAG} only after a "
        "safe temporary session exists. Do not export a credential value."
    )


def assert_request_allowed(method: str, path: str) -> None:
    verb = method.upper()
    route = urlparse(path).path or path
    if route.startswith("/api/profiles/") and route.endswith("/open-terminal") and verb == "POST":
        raise HarnessError("harness must not invoke profile open-terminal")
    if (verb, route) in FORBIDDEN_MUTATIONS:
        raise HarnessError(f"harness must not invoke {verb} {route}")
    if verb in {"POST", "PUT", "PATCH", "DELETE"}:
        if any(route.startswith(prefix) for prefix in FORBIDDEN_PATH_PREFIX_MUTATIONS):
            raise HarnessError(f"harness must not mutate local-models via {verb} {route}")


def readonly_host_status_allowed(path: str) -> bool:
    route = urlparse(path).path or path
    return route in READONLY_HOST_STATUS_PATHS


def scan_for_canaries(payload: Any, canaries: Sequence[str]) -> list[str]:
    """Return every planted canary found in a JSON-able payload or text."""
    rendered = _render_for_scan(payload)
    return [canary for canary in canaries if canary and canary in rendered]


def validate_coverage_document(document: Mapping[str, Any]) -> None:
    required = (
        "tier1",
        "live_schema",
        "exclusions",
    )
    missing = [key for key in required if key not in document]
    if missing:
        raise HarnessError(f"coverage.json missing {missing}")
    tier1 = document["tier1"]
    if not isinstance(tier1, list) or not tier1:
        raise HarnessError("coverage.tier1 must be a non-empty list")
    for entry in tier1:
        if not isinstance(entry, Mapping):
            raise HarnessError("coverage.tier1 entries must be objects")
        for key in (
            "id",
            "source_ref",
            "owner",
            "scope",
            "route",
            "control",
            "timing_class",
            "disposition",
        ):
            if key not in entry:
                raise HarnessError(f"coverage.tier1 entry missing {key}")
        if entry["disposition"] == "unresolved":
            raise HarnessError("coverage.tier1 disposition must not be unresolved")
    live = document["live_schema"]
    if not isinstance(live, Mapping):
        raise HarnessError("coverage.live_schema must be an object")
    for key in ("field_count", "category_count", "category_order", "known_types", "unknown_types"):
        if key not in live:
            raise HarnessError(f"coverage.live_schema missing {key}")


def validate_cleanup_receipt(receipt: Mapping[str, Any], *, stage: StageName) -> None:
    if receipt.get("schema_version") != CLEANUP_SCHEMA_VERSION:
        raise HarnessError(
            f"cleanup receipt schema_version must be {CLEANUP_SCHEMA_VERSION}"
        )
    if receipt.get("record_type") != CLEANUP_RECORD_TYPE:
        raise HarnessError(f"cleanup receipt record_type must be {CLEANUP_RECORD_TYPE}")
    if receipt.get("stage") != stage:
        raise HarnessError(f"cleanup receipt stage must be {stage}")
    names = receipt.get("absent_names")
    if not isinstance(names, list) or not names:
        raise HarnessError("cleanup receipt must list absent disposable names")
    test_b = receipt.get("testB")
    if not isinstance(test_b, Mapping) or test_b.get("unchanged") is not True:
        raise HarnessError("cleanup receipt must prove testB unchanged")
    if "safe_hash" in test_b:
        raise HarnessError("cleanup receipt must not invent a testB digest")
    if receipt.get("created_ids_deleted") is not True:
        raise HarnessError("cleanup receipt must delete only run-created server IDs")
    _refuse_secret_payload(receipt)


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def blocked_row(
    *,
    test_id: str,
    connection_label: ConnectionLabel,
    profile_label: str,
    talaria_commit: str,
    limitation: str,
    observed_at: str,
    talaria_version: str = "0.6.2-unreleased",
    hermes_version: str = "unverified",
) -> dict[str, Any]:
    return {
        "test_id": test_id,
        "status": "blocked",
        "talaria_version": talaria_version,
        "talaria_commit": talaria_commit,
        "hermes_version": hermes_version,
        "connection_label": connection_label,
        "profile_label": profile_label,
        "observed_at": observed_at,
        "evidence_refs": [],
        "limitation": limitation,
    }


def _refuse_secret_payload(payload: Mapping[str, Any]) -> None:
    rendered = _render_for_scan(payload).lower()
    for hint in _CANARY_HINTS:
        if f'"{hint}":' in rendered and "redacted" not in rendered:
            # Presence of a key name is allowed in a route label; a value-shaped
            # assignment of those keys is not.
            if re.search(rf'"{hint}"\s*:\s*"[^"]+"', rendered):
                raise HarnessError(f"evidence payload must not carry {hint} values")


def _render_for_scan(payload: Any) -> str:
    if isinstance(payload, str):
        return payload
    return json.dumps(payload, default=str, sort_keys=True)

