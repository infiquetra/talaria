"""Black-box CFG-P4 driver: live-shape diagnosis versus fixture remount.

Launches a supplied executable from outside the worktree. Never uses
``~/.talaria``. Never calls ``open_target_switch`` or ``open_reveal``.
Diagnostic records are metadata-only.
"""

from __future__ import annotations

import asyncio
import json
import subprocess
from collections.abc import Mapping
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlparse
from urllib.request import Request

from scripts.acceptance.v062_configuration import HarnessError, scan_for_canaries
from scripts.acceptance.v064_config_ui import (
    FORBIDDEN_WRITE_TARGETS,
    LEGAL_LOCAL_A_INSTALLED,
    LEGAL_LOCAL_E_INSTALLED,
    PLACEHOLDER_SCHEMA_KEY,
    TargetMountObservation,
    assert_write_target_allowed,
    cleanup_isolated_run,
    interpreter_for,
    isolated_child_env,
    launch_supplied_executable,
    observe_candidate_target_mount,
    open_json_request,
    require_http_url,
    require_legal_p3_name,
    require_outside_worktree,
    write_isolated_config,
)

CLEANUP_SCHEMA_VERSION = "talaria-v0.6.5-configuration-ui-cleanup-v1"
ORACLE_KEYS = (
    "target_connection",
    "target_profile",
    "schema_route_status_or_reason",
    "schema_response_bytes",
    "schema_top_level_keys",
    "schema_field_count",
    "schema_category_count",
    "schema_decode_result",
    "config_load_result",
    "env_route_status_or_reason",
    "env_row_count",
    "generation",
    "selected_at_reconcile",
    "projected_group_titles",
)
SELECTED_SCHEMA_KEY = "agent.max_turns"
SELECTED_ENV_LABEL = "synthetic-env"
PLACEHOLDER_PROFILE = "placeholder-old-target"

# T3R measured wrapper: {fields, category_order} with fields a JSON object.
# Values are synthetic; none are copied from Hermes.
_LIVE_SCHEMA: dict[str, Any] = {
    "fields": {
        SELECTED_SCHEMA_KEY: {
            "type": "number",
            "description": "synthetic-p4-field",
            "category": "agent",
        }
    },
    "category_order": ["agent"],
}
_LIVE_ENV: dict[str, Any] = {
    SELECTED_ENV_LABEL: {"is_set": True, "redacted_value": "sk-…p4"}
}


def require_legal_p4_name(name: str) -> str:
    return require_legal_p3_name(name)


@dataclass
class LiveShapeDashboard:
    """Loopback fixture: measured live schema/env shape; reject-once Save."""

    host: str = "127.0.0.1"
    _server: ThreadingHTTPServer | None = None
    _thread: Any = None
    put_counts: dict[str, int] = field(default_factory=dict)
    configs: dict[str, dict[str, Any]] = field(default_factory=dict)
    forbidden_writes: list[str] = field(default_factory=list)
    records: list[dict[str, Any]] = field(default_factory=list)

    def __post_init__(self) -> None:
        self.configs = {
            PLACEHOLDER_PROFILE: {"hermes": {"schema": "v1"}},
            LEGAL_LOCAL_A_INSTALLED: {"agent": {"max_turns": 12}},
            LEGAL_LOCAL_E_INSTALLED: {"agent": {"max_turns": 8}},
        }

    @property
    def origin(self) -> str:
        if self._server is None:
            raise HarnessError("dashboard is not running")
        host, port = self._server.server_address[:2]
        host_text = host.decode("ascii") if isinstance(host, bytes) else str(host)
        return f"http://{host_text}:{int(port)}"

    def __enter__(self) -> LiveShapeDashboard:
        import threading

        harness = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, format: str, *args: object) -> None:  # noqa: A003
                return

            def _profile(self) -> str:
                query = parse_qs(urlparse(self.path).query)
                values = query.get("profile") or [PLACEHOLDER_PROFILE]
                return values[0]

            def _send(self, status: int, body: Any, *, profile: str) -> None:
                payload = json.dumps(body).encode("utf-8")
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                harness.records.append(
                    {
                        "method": self.command,
                        "path": urlparse(self.path).path,
                        "profile": profile,
                        "status": status,
                        "body_class": "json-no-secret",
                    }
                )

            def do_GET(self) -> None:  # noqa: N802
                profile = self._profile()
                route = urlparse(self.path).path
                if route in {"/api/health", "/health"}:
                    self._send(200, {"ok": True}, profile=profile)
                    return
                if route == "/api/profiles":
                    self._send(
                        200,
                        {
                            "profiles": [
                                {"name": PLACEHOLDER_PROFILE},
                                {"name": LEGAL_LOCAL_A_INSTALLED},
                                {"name": LEGAL_LOCAL_E_INSTALLED},
                            ]
                        },
                        profile=profile,
                    )
                    return
                if route in {"/api/config/schema", "/api/schema"}:
                    if profile == PLACEHOLDER_PROFILE:
                        self._send(
                            200,
                            {PLACEHOLDER_SCHEMA_KEY: {"type": "string"}},
                            profile=profile,
                        )
                        return
                    self._send(200, dict(_LIVE_SCHEMA), profile=profile)
                    return
                if route == "/api/config":
                    self._send(
                        200,
                        {
                            "saved": harness.configs.get(profile, {}),
                            "effective": harness.configs.get(profile, {}),
                            "defaults": {},
                        },
                        profile=profile,
                    )
                    return
                if route == "/api/env":
                    if profile in {LEGAL_LOCAL_A_INSTALLED, LEGAL_LOCAL_E_INSTALLED}:
                        self._send(200, dict(_LIVE_ENV), profile=profile)
                        return
                    self._send(200, {}, profile=profile)
                    return
                self._send(404, {"detail": "not found"}, profile=profile)

            def do_PUT(self) -> None:  # noqa: N802
                profile = self._profile()
                length = int(self.headers.get("Content-Length", "0"))
                raw = self.rfile.read(length) if length else b"{}"
                if profile in FORBIDDEN_WRITE_TARGETS:
                    harness.forbidden_writes.append(profile)
                    self._send(400, {"detail": "forbidden target"}, profile=profile)
                    return
                try:
                    assert_write_target_allowed(profile)
                except HarnessError:
                    harness.forbidden_writes.append(profile)
                    self._send(400, {"detail": "forbidden target"}, profile=profile)
                    return
                count = harness.put_counts.get(profile, 0) + 1
                harness.put_counts[profile] = count
                if count == 1:
                    self._send(409, {"detail": "rejected-once"}, profile=profile)
                    return
                try:
                    body = json.loads(raw.decode("utf-8") or "{}")
                except json.JSONDecodeError:
                    body = {}
                if not isinstance(body, dict):
                    body = {}
                harness.configs[profile] = body
                self._send(200, {"ok": True}, profile=profile)

        self._server = ThreadingHTTPServer((self.host, 0), Handler)
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._thread.start()
        return self

    def __exit__(self, *exc: object) -> None:
        if self._server is not None:
            self._server.shutdown()
            self._server.server_close()
        if self._thread is not None:
            self._thread.join(timeout=2)
        self._server = None
        self._thread = None

    def saved_config(self, profile: str) -> dict[str, Any]:
        return dict(self.configs.get(profile, {}))

    def put(self, profile: str, body: Mapping[str, Any]) -> tuple[int, dict[str, Any]]:
        status, payload = open_json_request(
            Request(
                f"{self.origin}/api/config?profile={profile}",
                data=json.dumps(dict(body)).encode("utf-8"),
                method="PUT",
                headers={"Content-Type": "application/json"},
            )
        )
        if not isinstance(payload, dict):
            raise HarnessError("PUT response is not a JSON object")
        return status, payload


@dataclass(frozen=True)
class DiagnosticOracle:
    target_connection: str
    target_profile: str
    schema_route_status_or_reason: str
    schema_response_bytes: int
    schema_top_level_keys: tuple[str, ...]
    schema_field_count: int
    schema_category_count: int
    schema_decode_result: str
    config_load_result: str
    env_route_status_or_reason: str
    env_row_count: int
    generation: int
    selected_at_reconcile: str
    projected_group_titles: tuple[str, ...]

    def as_record(self) -> dict[str, Any]:
        record = {
            "target_connection": self.target_connection,
            "target_profile": self.target_profile,
            "schema_route_status_or_reason": self.schema_route_status_or_reason,
            "schema_response_bytes": self.schema_response_bytes,
            "schema_top_level_keys": list(self.schema_top_level_keys),
            "schema_field_count": self.schema_field_count,
            "schema_category_count": self.schema_category_count,
            "schema_decode_result": self.schema_decode_result,
            "config_load_result": self.config_load_result,
            "env_route_status_or_reason": self.env_route_status_or_reason,
            "env_row_count": self.env_row_count,
            "generation": self.generation,
            "selected_at_reconcile": self.selected_at_reconcile,
            "projected_group_titles": list(self.projected_group_titles),
        }
        extra = set(record) - set(ORACLE_KEYS)
        if extra:
            raise HarnessError(f"oracle contains undeclared keys: {sorted(extra)}")
        if scan_for_canaries(record, ("password", "token", "sk-", "secret-value")):
            raise HarnessError("oracle must not carry secrets")
        return record


@dataclass(frozen=True)
class LiveDiagnosisObservation:
    launched: bool
    selected_schema_mounted: bool
    selected_env_mounted: bool
    placeholder_schema_present: bool
    oracle: DiagnosticOracle


class _LoopbackSettings:
    def __init__(self, origin: str) -> None:
        self.origin = origin

    async def get_schema(self, target: Any) -> Any:
        _status, body = _get_json(
            f"{self.origin}/api/config/schema?profile={target.profile_name}"
        )
        return body

    async def get_config(self, target: Any) -> Any:
        _status, body = _get_json(
            f"{self.origin}/api/config?profile={target.profile_name}"
        )
        return body

    async def get_env(self, target: Any) -> Any:
        _status, body = _get_json(f"{self.origin}/api/env?profile={target.profile_name}")
        return body


def _get_json(url: str) -> tuple[int, Any]:
    require_http_url(url)
    from urllib.error import HTTPError
    from urllib.request import urlopen

    request = Request(url)
    try:
        # nosec B310 - require_http_url allowlists http/https immediately above.
        with urlopen(request, timeout=2) as response:  # nosec B310
            raw = response.read()
            return response.status, json.loads(raw.decode("utf-8"))
    except HTTPError as exc:
        raw = exc.read()
        try:
            return exc.code, json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError:
            return exc.code, {}


def _schema_metadata(
    body: Any, *, status: int
) -> tuple[str, int, tuple[str, ...], int, int, str]:
    rendered = json.dumps(body, default=str)
    keys = tuple(sorted(body)) if isinstance(body, Mapping) else ()
    fields = body.get("fields") if isinstance(body, Mapping) else None
    field_count = len(fields) if isinstance(fields, (list, dict)) else 0
    order = body.get("category_order") if isinstance(body, Mapping) else None
    category_count = len(order) if isinstance(order, list) else 0
    decode_result = "ok"
    try:
        from talaria.domain.settings import decode_settings_schema

        decode_settings_schema(body)
    except Exception as exc:
        decode_result = type(exc).__name__
    size = len(rendered.encode("utf-8"))
    return str(status), size, keys, field_count, category_count, decode_result


def _env_metadata(body: Any, *, status: int) -> tuple[str, int]:
    decode_rows = 0
    reason = str(status)
    try:
        from talaria.domain.settings import decode_env_listing

        listing = decode_env_listing(body)
        decode_rows = len(listing)
    except Exception as exc:
        reason = type(exc).__name__
        decode_rows = 0
    return reason, decode_rows


def _config_metadata(body: Any) -> str:
    if isinstance(body, Mapping) and {"saved", "effective"} <= set(body):
        return "ok"
    return type(body).__name__


def collect_installed_identity(
    executable: Path,
    *,
    worktree: Path,
    scratch: Path,
) -> dict[str, Any]:
    """Import identity from a temp cwd with PYTHONPATH unset."""
    exe = require_outside_worktree(executable, worktree=worktree)
    interpreter = interpreter_for(exe)
    cwd = scratch / "identity-cwd"
    cwd.mkdir(parents=True, exist_ok=True)
    config_dir = scratch / "identity-config"
    env = isolated_child_env(config_dir=config_dir, scratch=scratch)
    if "PYTHONPATH" in env:
        raise HarnessError("identity env must not set PYTHONPATH")
    completed = subprocess.run(
        [
            str(interpreter),
            "-c",
            (
                "import json, os, talaria; "
                "print(json.dumps({"
                "'version': talaria.__version__, "
                "'file': talaria.__file__, "
                "'pythonpath_set': bool(os.environ.get('PYTHONPATH')),"
                "}))"
            ),
        ],
        cwd=cwd,
        env=env,
        text=True,
        capture_output=True,
        check=False,
        timeout=15,
    )
    cleanup_isolated_run(config_dir=config_dir, scratch=scratch)
    if completed.returncode != 0:
        raise HarnessError(
            "identity import failed: "
            + (completed.stderr.strip() or completed.stdout.strip() or "no output")
        )
    payload = json.loads(completed.stdout)
    file_text = str(payload.get("file") or "")
    kind = "uv-tool" if "/uv/tools/talaria/" in file_text else "other"
    if str(worktree.resolve()) in file_text:
        kind = "worktree-contaminated"
    record = {
        "version": payload.get("version"),
        "talaria_file_kind": kind,
        "pythonpath_set": payload.get("pythonpath_set"),
    }
    if scan_for_canaries(record, ("password", "token", "sk-")):
        raise HarnessError("identity record must not carry secrets")
    return record


def observe_fixture_remount_control(*, scratch: Path) -> TargetMountObservation:
    """Existing v0.6.4 fixture remount stays the green control."""
    return observe_candidate_target_mount(scratch=scratch)


def observe_live_diagnosis(*, scratch: Path, worktree: Path) -> LiveDiagnosisObservation:
    """Drive the candidate identity loader against the T3R live HTTP shape."""
    from talaria.domain.settings import ConfigTarget
    from talaria.replay.controls import ReplayControls
    from talaria.replay.source import ReplaySource
    from talaria.transport.source import FrameRecord
    from talaria.ui.app import TalariaApp

    stub_root = scratch / "outside"
    stub_root.mkdir()
    stub = stub_root / "talaria"
    stub.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    stub.chmod(0o755)
    exe = require_outside_worktree(stub, worktree=worktree)
    config_dir = scratch / "config-live"
    launched = False
    with LiveShapeDashboard() as dashboard:
        write_isolated_config(config_dir, dashboard_origin=dashboard.origin)
        env = isolated_child_env(config_dir=config_dir, scratch=scratch)
        child = launch_supplied_executable(exe, env=env, cwd=scratch)
        try:
            child.wait(timeout=0.4)
        except subprocess.TimeoutExpired:
            child.terminate()
            child.wait(timeout=2)
        launched = True

        schema_status, schema_body = _get_json(
            f"{dashboard.origin}/api/config/schema?profile={LEGAL_LOCAL_A_INSTALLED}"
        )
        config_status, config_body = _get_json(
            f"{dashboard.origin}/api/config?profile={LEGAL_LOCAL_A_INSTALLED}"
        )
        env_status, env_body = _get_json(
            f"{dashboard.origin}/api/env?profile={LEGAL_LOCAL_A_INSTALLED}"
        )
        (
            schema_reason,
            schema_bytes,
            schema_keys,
            field_count,
            category_count,
            decode_result,
        ) = _schema_metadata(schema_body, status=schema_status)
        env_reason, env_rows = _env_metadata(env_body, status=env_status)
        config_result = (
            _config_metadata(config_body) if config_status == 200 else str(config_status)
        )

        origin = dashboard.origin

        def factory(_endpoint: str) -> _LoopbackSettings:
            return _LoopbackSettings(origin)

        controls = ReplayControls(paused=True)
        source = ReplaySource(
            (
                FrameRecord(
                    seq=1,
                    at=1.0,
                    direction="in",
                    frame={
                        "jsonrpc": "2.0",
                        "method": "event",
                        "params": {
                            "type": "gateway.ready",
                            "session_id": "s1",
                            "payload": {},
                        },
                    },
                ),
            ),
            controls=controls,
        )
        app = TalariaApp(
            source,
            mode="replay",
            controls=controls,
            settings_factory=factory,
            profile_endpoints={LEGAL_LOCAL_A_INSTALLED: origin},
            current_profile="default",
            theme_config_dir=config_dir,
            launch_cwd=scratch,
        )
        target = ConfigTarget(connection_id="local", profile_name=LEGAL_LOCAL_A_INSTALLED)
        identity = asyncio.run(app._settings_workspace_identity(target))
        view = identity.view
        titles = tuple(group.title for group in view.groups)
        keys = tuple(row.key for group in view.groups for row in group.rows)
        oracle = DiagnosticOracle(
            target_connection="local",
            target_profile=LEGAL_LOCAL_A_INSTALLED,
            schema_route_status_or_reason=schema_reason,
            schema_response_bytes=schema_bytes,
            schema_top_level_keys=schema_keys,
            schema_field_count=field_count,
            schema_category_count=category_count,
            schema_decode_result=decode_result,
            config_load_result=config_result,
            env_route_status_or_reason=env_reason,
            env_row_count=env_rows,
            generation=0,
            selected_at_reconcile=view.header.selected_profile,
            projected_group_titles=titles,
        )
        oracle.as_record()
        cleanup_isolated_run(config_dir=config_dir, scratch=scratch)
        return LiveDiagnosisObservation(
            launched=launched,
            selected_schema_mounted=SELECTED_SCHEMA_KEY in keys,
            selected_env_mounted=any(
                group.owner == "environment" and group.rows for group in view.groups
            ),
            placeholder_schema_present=PLACEHOLDER_SCHEMA_KEY in keys,
            oracle=oracle,
        )


def cleanup_p4_run(*, config_dir: Path, scratch: Path) -> dict[str, Any]:
    receipt = cleanup_isolated_run(config_dir=config_dir, scratch=scratch)
    receipt["schema_version"] = CLEANUP_SCHEMA_VERSION
    if scan_for_canaries(receipt, ("password", "token", "sk-")):
        raise HarnessError("cleanup receipt must not carry secrets")
    return receipt
