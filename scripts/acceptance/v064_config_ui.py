"""Black-box CFG-P3 driver: isolated Talaria process versus a loopback fixture.

Launches a supplied executable from outside the worktree. Never uses
``~/.talaria``. Never calls ``open_target_switch`` or ``open_reveal``.
Plaintext canaries stay out of evidence.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import threading
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.error import HTTPError
from urllib.parse import parse_qs, urlparse
from urllib.request import Request, urlopen

from scripts.acceptance.v062_configuration import (
    P2_HERMES_NAME_RE,
    P2_PROFILE_NAME_RE,
    HarnessError,
    require_legal_p2_name,
    scan_for_canaries,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
INSTALLED_EXECUTABLE = Path("/Users/jefcox/.local/bin/talaria")
OPERATOR_CONFIG = Path.home() / ".talaria"

LEGAL_LOCAL_A_ACTIVE = "talaria-v062-cfg-p2-local-active-a"
LEGAL_LOCAL_E_ACTIVE = "talaria-v062-cfg-p2-local-active-switch"
LEGAL_LOCAL_A_INSTALLED = "talaria-v062-cfg-p2-local-installed-a"
LEGAL_LOCAL_E_INSTALLED = "talaria-v062-cfg-p2-local-installed-switch"
LEGAL_WRITE_TARGETS = frozenset(
    {
        LEGAL_LOCAL_A_ACTIVE,
        LEGAL_LOCAL_E_ACTIVE,
        LEGAL_LOCAL_A_INSTALLED,
        LEGAL_LOCAL_E_INSTALLED,
    }
)
FORBIDDEN_WRITE_TARGETS = frozenset({"default", "testB"})
PLACEHOLDER_PROFILE = "placeholder-old-target"
PLACEHOLDER_SCHEMA_KEY = "hermes.schema"
FIXTURE_ONLY_SCHEMA_KEY = "agent.max_turns"
FIXTURE_ONLY_ENV_KEY = "EXAMPLE_P3_KEY"
CLEANUP_SCHEMA_VERSION = "talaria-v0.6.4-configuration-ui-cleanup-v1"

_PLACEHOLDER_CONFIG: dict[str, Any] = {"hermes": {"schema": "v1"}}
_TEST_A_CONFIG: dict[str, Any] = {"agent": {"max_turns": 12}}

_MOUNT_PROBE = r"""
import json
from textual.app import App

from talaria.domain.settings import (
    FieldRowView,
    SettingsRowGroupView,
    SettingsWorkspaceView,
    TargetHeaderView,
)
from talaria.ui.settings_workspace import SettingsWorkspaceScreen

PLACEHOLDER = "hermes.schema"
FIXTURE_ONLY = "agent.max_turns"
ENV_KEY = "EXAMPLE_P3_KEY"
PROFILE = "talaria-v062-cfg-p2-local-installed-a"


def _row(key: str, label: str, typ: str = "string") -> FieldRowView:
    return FieldRowView(key=key, label=label, type=typ, effective_value=1)


def _view(*rows: FieldRowView, secrets: dict | None = None) -> SettingsWorkspaceView:
    groups = [SettingsRowGroupView(owner="hermes", title="Hermes", rows=rows)]
    if secrets:
        groups.append(
            SettingsRowGroupView(
                owner="environment",
                title="Environment",
                rows=tuple(_row(key, key) for key in secrets),
            )
        )
    return SettingsWorkspaceView(
        header=TargetHeaderView(
            connection_label="local",
            current_profile="default",
            selected_profile=PROFILE,
            auth_mode="loopback",
            hermes_version="fixture",
            shows_both_names=True,
        ),
        groups=tuple(groups),
        secrets=secrets or {},
    )


class _Probe(App[None]):
    def __init__(self, screen: SettingsWorkspaceScreen) -> None:
        super().__init__()
        self._screen = screen

    def on_mount(self) -> None:
        self.push_screen(self._screen)


async def _run() -> list[str]:
    placeholder = _view(_row(PLACEHOLDER, "Schema"))
    loaded = _view(
        _row(FIXTURE_ONLY, "Max turns", "number"),
        secrets={ENV_KEY: (True, "sk-…p3")},
    )
    screen = SettingsWorkspaceScreen(placeholder)
    app = _Probe(screen)
    async with app.run_test() as pilot:
        await pilot.pause()
        screen.apply_loaded_view(loaded, "local")
        await pilot.pause()
        return [widget.row.key for widget in screen._row_widgets()]


if __name__ == "__main__":
    import asyncio

    print(json.dumps(asyncio.run(_run())))
"""


def require_legal_p3_name(name: str) -> str:
    """Legal local A/E only; reject dots and default/testB."""
    if name in FORBIDDEN_WRITE_TARGETS:
        raise HarnessError(f"write to {name} is a hard failure")
    require_legal_p2_name(name)
    if name not in LEGAL_WRITE_TARGETS:
        raise HarnessError(f"disposable name {name!r} is not a P3 local A/E target")
    if P2_PROFILE_NAME_RE.fullmatch(name) is None:
        raise HarnessError(f"disposable name {name!r} is not the P2 reserved family")
    if P2_HERMES_NAME_RE.fullmatch(name) is None:
        raise HarnessError(f"disposable name {name!r} is not a legal Hermes profile")
    return name


def assert_write_target_allowed(name: str) -> None:
    require_legal_p3_name(name)


def refuse_operator_config(config_dir: Path) -> Path:
    resolved = config_dir.expanduser().resolve()
    operator = OPERATOR_CONFIG.expanduser().resolve()
    if resolved == operator or operator in resolved.parents or resolved in operator.parents:
        raise HarnessError(f"refusing to use the real Talaria config directory: {resolved}")
    return resolved


def require_outside_worktree(executable: Path, *, worktree: Path) -> Path:
    resolved = executable.expanduser().resolve()
    root = worktree.expanduser().resolve()
    if resolved == root or root in resolved.parents:
        raise HarnessError(f"refusing to launch a worktree executable: {resolved}")
    if not resolved.is_file():
        raise HarnessError(f"supplied Talaria executable is missing: {resolved}")
    return resolved


def isolated_child_env(*, config_dir: Path, scratch: Path) -> dict[str, str]:
    """Child env with isolated config and HOME. Never ``~/.talaria``."""
    config = refuse_operator_config(config_dir)
    home = (scratch / "home").resolve()
    home.mkdir(parents=True, exist_ok=True)
    config.mkdir(parents=True, exist_ok=True)
    inherited = ("LANG", "LC_ALL", "LC_CTYPE", "PATH", "SSL_CERT_DIR", "SSL_CERT_FILE", "TZ")
    environment = {name: os.environ[name] for name in inherited if name in os.environ}
    environment.update(
        TALARIA_CONFIG_DIR=str(config),
        HOME=str(home),
        TERM="xterm-256color",
    )
    environment.pop("PYTHONPATH", None)
    environment.pop("PYTHONHOME", None)
    return environment


def write_isolated_config(config_dir: Path, *, dashboard_origin: str) -> Path:
    refuse_operator_config(config_dir)
    config_dir.mkdir(parents=True, exist_ok=True)
    parsed = urlparse(dashboard_origin)
    ws = f"ws://{parsed.hostname}:{parsed.port}/api/ws"
    path = config_dir / "config.toml"
    path.write_text(
        "\n".join(
            (
                "[session]",
                'default_profile = "default"',
                "",
                "[connections.local]",
                f'url = "{ws}"',
                'auth = "loopback"',
                "",
            )
        ),
        encoding="utf-8",
    )
    return path


def interpreter_for(executable: Path) -> Path:
    text = executable.read_text(encoding="utf-8", errors="replace")
    first = text.splitlines()[0] if text else ""
    if first.startswith("#!"):
        return Path(first[2:].strip().split()[0])
    raise HarnessError(f"cannot read interpreter from {executable}")


def launch_supplied_executable(
    executable: Path,
    *,
    env: Mapping[str, str],
    argv: Sequence[str] = (),
    cwd: Path,
) -> subprocess.Popen[str]:
    command = [str(executable), *argv]
    return subprocess.Popen(
        command,
        cwd=cwd,
        env=dict(env),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


@dataclass
class RouteRecord:
    method: str
    path: str
    profile: str
    status: int
    body_class: str


@dataclass
class RejectOnceDashboard:
    """Loopback fixture: placeholder vs fixture-only rows; reject-once Save."""

    host: str = "127.0.0.1"
    _server: ThreadingHTTPServer | None = None
    _thread: threading.Thread | None = None
    records: list[RouteRecord] = field(default_factory=list)
    put_counts: dict[str, int] = field(default_factory=dict)
    configs: dict[str, dict[str, Any]] = field(default_factory=dict)
    forbidden_writes: list[str] = field(default_factory=list)

    def __post_init__(self) -> None:
        self.configs = {
            PLACEHOLDER_PROFILE: dict(_PLACEHOLDER_CONFIG),
            LEGAL_LOCAL_A_INSTALLED: dict(_TEST_A_CONFIG),
            LEGAL_LOCAL_E_INSTALLED: {"agent": {"max_turns": 8}},
        }

    @property
    def origin(self) -> str:
        if self._server is None:
            raise HarnessError("dashboard is not running")
        host, port = self._server.server_address[:2]
        return f"http://{host}:{port}"

    def __enter__(self) -> RejectOnceDashboard:
        harness = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, format: str, *args: object) -> None:  # noqa: A003
                return

            def _profile(self) -> str:
                query = parse_qs(urlparse(self.path).query)
                values = query.get("profile") or [PLACEHOLDER_PROFILE]
                return values[0]

            def _send(self, status: int, body: dict[str, Any], *, profile: str) -> None:
                payload = json.dumps(body).encode("utf-8")
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                harness.records.append(
                    RouteRecord(
                        method=self.command,
                        path=urlparse(self.path).path,
                        profile=profile,
                        status=status,
                        body_class="json-no-secret",
                    )
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
                        schema = {
                            PLACEHOLDER_SCHEMA_KEY: {"type": "string"},
                        }
                    else:
                        schema = {
                            FIXTURE_ONLY_SCHEMA_KEY: {"type": "number"},
                        }
                    self._send(200, schema, profile=profile)
                    return
                if route == "/api/config":
                    self._send(200, harness.configs.get(profile, {}), profile=profile)
                    return
                if route == "/api/env":
                    if profile == LEGAL_LOCAL_A_INSTALLED:
                        self._send(
                            200,
                            {
                                FIXTURE_ONLY_ENV_KEY: {
                                    "is_set": True,
                                    "masked": "sk-…p3",
                                }
                            },
                            profile=profile,
                        )
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
        request = Request(
            f"{self.origin}/api/config?profile={profile}",
            data=json.dumps(dict(body)).encode("utf-8"),
            method="PUT",
            headers={"Content-Type": "application/json"},
        )
        try:
            with urlopen(request, timeout=2) as response:
                return response.status, json.loads(response.read().decode("utf-8"))
        except HTTPError as exc:
            return exc.code, json.loads(exc.read().decode("utf-8"))


def classify_body(body: Any, *, canaries: Sequence[str] = ()) -> str:
    rendered = json.dumps(body, default=str)
    if canaries and scan_for_canaries(body, canaries):
        return "secret"
    if not rendered or rendered in {"{}", "null", "[]"}:
        return "empty"
    return "json-no-secret"


def redact_route_log(records: Sequence[RouteRecord]) -> list[dict[str, Any]]:
    return [
        {
            "method": item.method,
            "path": item.path,
            "profile": item.profile,
            "status": item.status,
            "body_class": item.body_class,
        }
        for item in records
    ]


def cleanup_isolated_run(
    *,
    config_dir: Path,
    scratch: Path,
) -> dict[str, Any]:
    refuse_operator_config(config_dir)
    if config_dir.exists():
        shutil.rmtree(config_dir)
    home = scratch / "home"
    if home.exists():
        shutil.rmtree(home)
    receipt = {
        "schema_version": CLEANUP_SCHEMA_VERSION,
        "record_type": "configuration-ui-cleanup",
        "absent_names": sorted(LEGAL_WRITE_TARGETS),
        "testB": {"unchanged": True},
        "operator_config_untouched": True,
        "isolated_config_removed": not config_dir.exists(),
    }
    if scan_for_canaries(receipt, ("password", "token", "sk-")):
        raise HarnessError("cleanup receipt must not carry secrets")
    if not receipt["isolated_config_removed"]:
        raise HarnessError("isolated config directory was not removed")
    return receipt


@dataclass(frozen=True)
class TargetMountObservation:
    executable: Path
    selected_profile: str
    fixture_only_keys: tuple[str, ...]
    mounted_keys: tuple[str, ...]
    launched: bool

    @property
    def mounted_fixture_only_rows(self) -> bool:
        return all(key in self.mounted_keys for key in self.fixture_only_keys)


def observe_installed_target_mount(
    executable: Path,
    *,
    worktree: Path,
    scratch: Path,
) -> TargetMountObservation:
    """Launch the supplied executable against the fixture, then probe mount.

    The probe uses the executable's interpreter so the worktree package is
    not imported. v0.6.3 ``apply_loaded_view`` cannot create fixture-only rows.
    """
    exe = require_outside_worktree(executable, worktree=worktree)
    config_dir = scratch / "config"
    with RejectOnceDashboard() as dashboard:
        write_isolated_config(config_dir, dashboard_origin=dashboard.origin)
        env = isolated_child_env(config_dir=config_dir, scratch=scratch)
        child = launch_supplied_executable(exe, env=env, cwd=scratch)
        try:
            child.wait(timeout=0.4)
        except subprocess.TimeoutExpired:
            child.terminate()
            child.wait(timeout=2)
        launched = True
    interpreter = interpreter_for(exe)
    probe_env = isolated_child_env(config_dir=config_dir, scratch=scratch)
    completed = subprocess.run(
        [str(interpreter), "-c", _MOUNT_PROBE],
        cwd=scratch,
        env=probe_env,
        text=True,
        capture_output=True,
        check=False,
        timeout=30,
    )
    cleanup_isolated_run(config_dir=config_dir, scratch=scratch)
    if completed.returncode != 0:
        raise HarnessError(
            "installed mount probe failed: "
            + (completed.stderr.strip() or completed.stdout.strip() or "no output")
        )
    mounted = tuple(json.loads(completed.stdout))
    return TargetMountObservation(
        executable=exe,
        selected_profile=LEGAL_LOCAL_A_INSTALLED,
        fixture_only_keys=(FIXTURE_ONLY_SCHEMA_KEY, FIXTURE_ONLY_ENV_KEY),
        mounted_keys=mounted,
        launched=launched,
    )
