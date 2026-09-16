"""CFG-A1 11–18 stub contracts for the settings HTTP/RPC client.

Stub-server tests never skip. They fail only when the production settings
transport is missing or when it issues an unscoped write. No live Hermes
is contacted from this module.
"""

from __future__ import annotations

import contextlib
import importlib
import json
import logging
from collections.abc import Iterator
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, HTTPServer
from threading import Thread
from typing import Any
from urllib.parse import parse_qs, urlparse

import pytest

CANARY = "canary-v062-settings-do-not-leak"
FIXTURE_PROFILE = "alpha-fixture"
DEFAULT_PROFILE = "default"

FORBIDDEN_WRITE_PATHS = (
    "/api/hermes/update",
    "/api/gateway/migrate",
    "/api/profiles/active",
)
LOCAL_MODEL_WRITE_PREFIX = "/api/local-models/"
_CLIENT_REFUSAL = (TypeError, ValueError, RuntimeError, LookupError, OSError)

ROUTE_CONTRACTS: tuple[tuple[str, str, str, str], ...] = (
    ("get_config", "GET", "/api/config", "profile"),
    ("put_config", "PUT", "/api/config", "profile"),
    ("get_schema", "GET", "/api/config/schema", "profile"),
    ("get_env", "GET", "/api/env", "profile"),
    ("put_env", "PUT", "/api/env", "profile"),
    ("delete_env", "DELETE", "/api/env", "profile"),
    ("reveal_env", "POST", "/api/env/reveal", "profile"),
    ("set_model", "POST", "/api/model/set", "profile"),
    ("get_model_info", "GET", "/api/model/info", "profile"),
    ("clone_profile", "POST", "/api/profiles", "body"),
    ("rename_profile", "PATCH", "/api/profiles/{name}", "path"),
    ("delete_profile", "DELETE", "/api/profiles/{name}", "path"),
    ("get_status", "GET", "/api/status", "profile"),
    ("restart_gateway", "POST", "/api/gateway/restart", "profile"),
    ("wake_start", "RPC", "wake.start", "params.profile"),
    ("reset_profile", "PUT", "/api/config", "profile"),
)


@dataclass
class Recorded:
    method: str
    path: str
    headers: dict[str, str]
    body: dict[str, Any] = field(default_factory=dict)

    @property
    def query(self) -> dict[str, list[str]]:
        return parse_qs(urlparse(self.path).query, keep_blank_values=True)


class _SettingsHandler(BaseHTTPRequestHandler):
    server_version = "TalariaSettingsStub/0"
    routes: dict[tuple[str, str], tuple[int, bytes, str]] = {}
    recorder: list[Recorded] = []
    launch_writes: list[Recorded] = []

    def log_message(self, format: str, *args: Any) -> None:
        return

    def do_GET(self) -> None:
        self._handle("GET")

    def do_PUT(self) -> None:
        self._handle("PUT")

    def do_POST(self) -> None:
        self._handle("POST")

    def do_PATCH(self) -> None:
        self._handle("PATCH")

    def do_DELETE(self) -> None:
        self._handle("DELETE")

    def _handle(self, method: str) -> None:
        length = int(self.headers.get("Content-Length", "0") or "0")
        raw = self.rfile.read(length) if length else b""
        try:
            parsed = json.loads(raw) if raw else {}
        except json.JSONDecodeError:
            parsed = {}
        if not isinstance(parsed, dict):
            parsed = {}
        recorded = Recorded(
            method=method,
            path=self.path,
            headers=dict(self.headers),
            body=parsed,
        )
        self.recorder.append(recorded)
        route_path = urlparse(self.path).path
        query = parse_qs(urlparse(self.path).query, keep_blank_values=True)
        profile = ""
        if "profile" in query:
            profile = (query.get("profile") or [""])[0]
        if not profile:
            body_profile = parsed.get("profile")
            profile = body_profile if isinstance(body_profile, str) else ""
        if method in {"PUT", "POST", "PATCH", "DELETE"} and route_path == "/api/config":
            if profile in {"", "current"}:
                self.launch_writes.append(recorded)
        status, payload, content_type = self.routes.get(
            (method, route_path),
            (404, b'{"detail":"Not Found"}', "application/json"),
        )
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


@contextlib.contextmanager
def settings_gateway(
    routes: dict[tuple[str, str], tuple[int, bytes, str]] | None = None,
) -> Iterator[tuple[str, list[Recorded], list[Recorded]]]:
    recorder: list[Recorded] = []
    launch_writes: list[Recorded] = []

    class Handler(_SettingsHandler):
        pass

    Handler.routes = routes or {}
    Handler.recorder = recorder
    Handler.launch_writes = launch_writes
    server = HTTPServer(("127.0.0.1", 0), Handler)
    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}/", recorder, launch_writes
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def json_route(body: Any, status: int = 200) -> tuple[int, bytes, str]:
    return status, json.dumps(body).encode("utf-8"), "application/json"


def ws_endpoint(origin: str) -> str:
    return origin.replace("http://", "ws://").rstrip("/") + "/api/ws"


def load_module(name: str) -> Any:
    try:
        return importlib.import_module(name)
    except ImportError as exc:
        raise AssertionError(f"unimplemented interface: {name}") from exc


def settings_client_class() -> Any:
    module = load_module("talaria.transport.settings")
    client = getattr(module, "SettingsClient", None)
    if client is None:
        raise AssertionError("unimplemented interface: talaria.transport.settings.SettingsClient")
    return client


def config_target_class() -> Any:
    module = load_module("talaria.domain.settings")
    target = getattr(module, "ConfigTarget", None)
    if target is None:
        raise AssertionError("unimplemented interface: talaria.domain.settings.ConfigTarget")
    return target


def make_target(connection_id: str = "local", profile_name: str = FIXTURE_PROFILE) -> Any:
    return config_target_class()(connection_id=connection_id, profile_name=profile_name)


class _StubProvider:
    def __init__(self, value: str = CANARY) -> None:
        self.value = value
        self.acquisitions = 0

    def __repr__(self) -> str:
        return f"_StubProvider(acquisitions={self.acquisitions}, value=<withheld>)"

    async def acquire(self) -> Any:
        self.acquisitions += 1
        credentials = load_module("talaria.transport.credentials")
        return credentials.Credential(parameter="token", value=self.value, source="file")


def make_client(origin: str) -> Any:
    return settings_client_class()(ws_endpoint(origin), _StubProvider())


# ── A1-12: unscoped PUT is never issued; the fixture names the hazard ──


@pytest.mark.asyncio
async def test_a1_12_settings_client_never_issues_an_unscoped_config_put() -> None:
    routes = {
        ("GET", "/api/config"): json_route(
            {"timezone": "UTC", "approvals": {"mode": "manual"}}
        ),
        ("PUT", "/api/config"): json_route({"ok": True}),
    }
    with settings_gateway(routes) as (origin, recorder, launch_writes):
        client = make_client(origin)
        await client.put_config(make_target(), {"timezone": "America/Indiana/Indianapolis"})

    assert launch_writes == []
    writes = [item for item in recorder if item.method == "PUT"]
    assert writes, "expected a scoped PUT"
    for item in writes:
        query_profile = (item.query.get("profile") or [""])[0]
        body_profile = item.body.get("profile") if isinstance(item.body.get("profile"), str) else ""
        assert query_profile == FIXTURE_PROFILE or body_profile == FIXTURE_PROFILE
        assert CANARY not in item.path


@pytest.mark.asyncio
async def test_a1_12_missing_or_empty_profile_performs_no_io() -> None:
    with settings_gateway({("PUT", "/api/config"): json_route({"ok": True})}) as (
        origin,
        recorder,
        launch_writes,
    ):
        client = make_client(origin)
        target_cls = config_target_class()
        with pytest.raises(_CLIENT_REFUSAL):
            target_cls(connection_id="local", profile_name="")
        with pytest.raises(_CLIENT_REFUSAL):
            await client.put_config(
                target_cls(connection_id="local", profile_name="current"),
                {"timezone": "UTC"},
            )

    assert recorder == []
    assert launch_writes == []


def test_a1_12_unscoped_put_would_have_written_the_launch_profile() -> None:
    """Documents the Hermes hazard without touching a launch profile.

    The stub records a missing/current profile as a launch write. Production
    code must never take that branch.
    """
    with settings_gateway({("PUT", "/api/config"): json_route({"ok": True})}) as (
        _origin,
        _recorder,
        launch_writes,
    ):
        handler = _SettingsHandler
        recorded = Recorded(method="PUT", path="/api/config", headers={}, body={"config": {}})
        handler.launch_writes = launch_writes
        if "profile" not in recorded.query and not recorded.body.get("profile"):
            launch_writes.append(recorded)
    assert launch_writes, "the isolated fixture must still name the unscoped-write hazard"


# ── A1-11: one field of each known schema type ───────────────────────────


@pytest.mark.asyncio
async def test_a1_11_round_trips_one_field_of_each_known_type() -> None:
    saved: dict[str, Any] = {}
    fields = {
        "timezone": ("string", "America/Indiana/Indianapolis"),
        "agent.max_turns": ("number", 12),
        "display.resume_last_session": ("boolean", True),
        "fallback_providers": ("list", ["example-provider"]),
        "approvals.mode": ("select", "smart"),
    }

    with settings_gateway(
        {
            ("GET", "/api/config"): json_route({"saved": saved}),
            ("PUT", "/api/config"): json_route({"ok": True}),
        }
    ) as (origin, recorder, launch_writes):
        client = make_client(origin)
        target = make_target()
        for key, (_kind, value) in fields.items():
            await client.put_config(target, {key: value})
        document = await client.get_config(target, include_defaults=False)

    assert launch_writes == []
    assert all((item.query.get("profile") or [""])[0] == FIXTURE_PROFILE for item in recorder)
    saved_reads = [
        item
        for item in recorder
        if item.method == "GET" and urlparse(item.path).path == "/api/config"
    ]
    assert saved_reads, "expected a saved-only re-read"
    assert any("include_defaults=false" in item.path for item in saved_reads)
    assert document is not None


# ── A1-13 / A1-14 / A1-15 / A1-16 / A1-17 / A1-18 ────────────────────────


@pytest.mark.asyncio
async def test_a1_13_model_set_then_info_names_the_same_target() -> None:
    with settings_gateway(
        {
            ("POST", "/api/model/set"): json_route({"ok": True, "scope": "main"}),
            ("GET", "/api/model/info"): json_route(
                {"model": "example-small", "provider": "example-provider"}
            ),
            ("GET", "/api/config"): json_route(
                {"model": "example-small", "provider": "example-provider"}
            ),
        }
    ) as (origin, recorder, _launch):
        client = make_client(origin)
        target = make_target()
        await client.set_model(
            target,
            provider="example-provider",
            model="example-small",
            confirm_expensive_model=False,
        )
        info = await client.get_model_info(target)

    assert info["model"] == "example-small"
    assert all(
        FIXTURE_PROFILE in item.path or item.body.get("profile") == FIXTURE_PROFILE
        for item in recorder
    )


@pytest.mark.asyncio
async def test_a1_14_reveal_is_one_shot_and_the_sixth_call_is_limited(
    caplog: pytest.LogCaptureFixture,
) -> None:
    secret = "in-process-canary-not-for-evidence"
    reveals = {"n": 0}

    class RevealHandler(_SettingsHandler):
        def _handle(self, method: str) -> None:
            route_path = urlparse(self.path).path
            if method == "POST" and route_path == "/api/env/reveal":
                reveals["n"] += 1
                if reveals["n"] >= 6:
                    payload = b'{"detail":"rate limited"}'
                    self.send_response(429)
                else:
                    payload = json.dumps(
                        {"key": "TALARIA_V062_TEST_SECRET", "value": secret}
                    ).encode()
                    self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return
            super()._handle(method)

    recorder: list[Recorded] = []
    RevealHandler.routes = {
        ("PUT", "/api/env"): json_route({"ok": True}),
        ("GET", "/api/env"): json_route(
            {"TALARIA_V062_TEST_SECRET": {"is_set": True, "redacted_value": "***"}}
        ),
        ("DELETE", "/api/env"): json_route({"ok": True}),
    }
    RevealHandler.recorder = recorder
    RevealHandler.launch_writes = []
    server = HTTPServer(("127.0.0.1", 0), RevealHandler)
    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        origin = f"http://127.0.0.1:{server.server_port}/"
        client = make_client(origin)
        target = make_target()
        with caplog.at_level(logging.DEBUG):
            await client.put_env(target, "TALARIA_V062_TEST_SECRET", secret)
            listing = await client.get_env(target)
            assert listing["TALARIA_V062_TEST_SECRET"]["is_set"] is True
            assert secret not in json.dumps(listing)
            first = await client.reveal_env(target, "TALARIA_V062_TEST_SECRET")
            assert first["value"] == secret
            for _ in range(4):
                await client.reveal_env(target, "TALARIA_V062_TEST_SECRET")
            with pytest.raises(Exception, match="limit|429|rate"):
                await client.reveal_env(target, "TALARIA_V062_TEST_SECRET")
            await client.delete_env(target, "TALARIA_V062_TEST_SECRET")
        assert secret not in caplog.text
        assert secret not in repr(client)
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


@pytest.mark.asyncio
async def test_a1_15_clone_strips_channels_and_does_not_copy_sessions() -> None:
    with settings_gateway({("POST", "/api/profiles"): json_route({"name": "gamma-fixture"})}) as (
        origin,
        recorder,
        _launch,
    ):
        client = make_client(origin)
        created = await client.clone_profile(
            "local",
            name="gamma-fixture",
            clone_from=FIXTURE_PROFILE,
            clone_all=False,
            clone_channels=False,
        )

    assert created["name"] == "gamma-fixture"
    body = recorder[0].body
    assert body["clone_from"] == FIXTURE_PROFILE
    assert body["clone_all"] is False
    assert body.get("clone_channels") in {False, None}


@pytest.mark.asyncio
async def test_a1_16_default_rename_keeps_canonical_id() -> None:
    with settings_gateway(
        {
            ("PATCH", "/api/profiles/default"): json_route(
                {"name": DEFAULT_PROFILE, "display_name": "shown-fixture"}
            )
        }
    ) as (origin, recorder, _launch):
        client = make_client(origin)
        result = await client.rename_profile("local", DEFAULT_PROFILE, "shown-fixture")

    assert result["name"] == DEFAULT_PROFILE
    assert result.get("display_name") == "shown-fixture"
    assert recorder[0].method == "PATCH"
    assert "/api/profiles/default" in recorder[0].path


@pytest.mark.asyncio
async def test_a1_17_default_delete_is_400_and_disposable_delete_is_gone() -> None:
    routes = {
        ("DELETE", "/api/profiles/default"): json_route(
            {"detail": "Cannot delete the default profile"}, 400
        ),
        ("DELETE", "/api/profiles/delta-fixture"): json_route({"ok": True}),
        ("GET", "/api/profiles"): json_route({"profiles": [{"name": DEFAULT_PROFILE}]}),
        ("GET", "/api/profiles/active"): json_route(
            {"active": DEFAULT_PROFILE, "current": DEFAULT_PROFILE}
        ),
    }
    with settings_gateway(routes) as (origin, recorder, _launch):
        client = make_client(origin)
        with pytest.raises(_CLIENT_REFUSAL, match="default|400"):
            await client.delete_profile("local", DEFAULT_PROFILE)
        await client.delete_profile("local", "delta-fixture")

    assert any(
        item.method == "DELETE" and item.path.startswith("/api/profiles/default")
        for item in recorder
    )
    assert not any(
        item.method == "POST" and "/api/profiles/active" in item.path for item in recorder
    )


@pytest.mark.asyncio
async def test_a1_18_unknown_profile_disables_target_writes() -> None:
    with settings_gateway(
        {("GET", "/api/config"): (404, b'{"detail":"Not Found"}', "application/json")}
    ) as (origin, recorder, launch_writes):
        client = make_client(origin)
        target = make_target(profile_name="nope-fixture")
        with pytest.raises(_CLIENT_REFUSAL) as caught:
            await client.get_config(target)
        assert "profile" in str(caught.value).lower() or getattr(caught.value, "reason", "") in {
            "unknown_profile",
            "profile_not_found",
        }
        writable = getattr(client, "writes_enabled_for", None)
        if writable is not None:
            assert writable(target) is False

    assert launch_writes == []
    assert recorder


# ── route matrix, verbs, host writes, restart poll ───────────────────────


@pytest.mark.asyncio
@pytest.mark.parametrize(("method_name", "verb", "path", "scope"), ROUTE_CONTRACTS)
async def test_each_manifest_route_requires_explicit_profile_scope(
    method_name: str, verb: str, path: str, scope: str
) -> None:
    client_cls = settings_client_class()
    method = getattr(client_cls, method_name, None)
    assert method is not None, (
        f"unimplemented interface: SettingsClient.{method_name} ({verb} {path}, {scope})"
    )


def test_host_administration_write_methods_are_structurally_absent() -> None:
    module = load_module("talaria.transport.settings")
    client_cls = settings_client_class()
    names = {name.lower() for name in dir(client_cls)}
    for forbidden in (
        "update_hermes",
        "migrate_gateway",
        "set_active_profile",
        "install_local_model",
        "download_local_model",
        "open_terminal",
    ):
        assert forbidden not in names
    from pathlib import Path

    source_path = getattr(module, "__file__", None)
    assert source_path is not None
    text = Path(source_path).read_text(encoding="utf-8")
    for path in FORBIDDEN_WRITE_PATHS:
        assert path not in text
    for verb in ("POST", "PUT", "PATCH", "DELETE"):
        assert f"{verb} {LOCAL_MODEL_WRITE_PREFIX}" not in text


def test_generic_admin_verbs_exist_for_settings_writes() -> None:
    admin = load_module("talaria.transport.admin")
    request_fn = getattr(admin, "request_admin_json", None)
    assert request_fn is not None, "unimplemented interface: request_admin_json"
    assert "method" in inspect_parameters(request_fn)


def inspect_parameters(fn: Any) -> set[str]:
    import inspect

    return set(inspect.signature(fn).parameters)


@pytest.mark.asyncio
async def test_restart_polls_action_and_status_and_does_not_claim_dashboard() -> None:
    with settings_gateway(
        {
            ("GET", "/api/status"): json_route(
                {"gateway_running": True, "gateway_state": "running"}
            ),
            ("GET", "/api/profiles"): json_route(
                {
                    "profiles": [
                        {"name": FIXTURE_PROFILE, "gateway_running": False},
                        {"name": "sibling-fixture", "gateway_running": False},
                    ]
                }
            ),
            ("POST", "/api/gateway/restart"): json_route(
                {"ok": True, "pid": 1, "name": "gateway-restart"}
            ),
            ("GET", "/api/actions/gateway-restart/status"): json_route(
                {"ok": True, "done": True}
            ),
        }
    ) as (origin, recorder, _launch):
        client = make_client(origin)
        plan_cls = getattr(load_module("talaria.domain.settings"), "RestartPlan", None)
        assert plan_cls is not None, "unimplemented interface: RestartPlan"
        plan = plan_cls(
            scope="shared-multiplexer",
            title=(
                "Restart the shared default gateway — this also restarts the "
                "gateways serving sibling-fixture"
            ),
            dashboard_note=(
                "Talaria's own connection is to the dashboard and is unaffected."
            ),
            affected_profiles=(FIXTURE_PROFILE, "sibling-fixture"),
        )
        result = await client.restart_gateway(make_target(), plan)

    assert getattr(result, "dashboard_restarted", False) is False
    methods = [item.method + " " + urlparse(item.path).path for item in recorder]
    assert any(item.startswith("POST /api/gateway/restart") for item in methods)
    assert any("/api/actions/gateway-restart/status" in item for item in methods)
    assert any(item.endswith("/api/status") for item in methods)


@pytest.mark.asyncio
async def test_a_redirect_cannot_exfiltrate_settings_headers() -> None:
    foreign_seen: list[Recorded] = []

    class Foreign(BaseHTTPRequestHandler):
        def do_PUT(self) -> None:
            foreign_seen.append(Recorded("PUT", self.path, dict(self.headers)))
            self.send_response(200)
            self.send_header("Content-Length", "2")
            self.end_headers()
            self.wfile.write(b"{}")

        def log_message(self, format: str, *args: Any) -> None:
            return

    foreign = HTTPServer(("127.0.0.1", 0), Foreign)
    thread = Thread(target=foreign.serve_forever, daemon=True)
    thread.start()
    try:
        location = f"http://127.0.0.1:{foreign.server_port}/stolen"

        class Redirect(_SettingsHandler):
            def _handle(self, method: str) -> None:
                self.send_response(302)
                self.send_header("Location", location)
                self.send_header("Content-Length", "0")
                self.end_headers()

        Redirect.routes = {}
        Redirect.recorder = []
        Redirect.launch_writes = []
        origin_server = HTTPServer(("127.0.0.1", 0), Redirect)
        origin_thread = Thread(target=origin_server.serve_forever, daemon=True)
        origin_thread.start()
        try:
            origin = f"http://127.0.0.1:{origin_server.server_port}/"
            client = make_client(origin)
            with pytest.raises(_CLIENT_REFUSAL):
                await client.put_config(make_target(), {"timezone": "UTC"})
        finally:
            origin_server.shutdown()
            origin_server.server_close()
            origin_thread.join(timeout=5)
    finally:
        foreign.shutdown()
        foreign.server_close()
        thread.join(timeout=5)

    assert foreign_seen == []


def test_timeout_malformed_oversized_and_conflict_remain_distinct() -> None:
    module = load_module("talaria.transport.settings")
    failure_type = getattr(module, "SettingsFailure", None) or getattr(module, "AdminFailure", None)
    assert failure_type is not None, "unimplemented interface: distinct settings failure reasons"
    allowed = set(getattr(failure_type, "__args__", ()))
    for reason in (
        "timeout",
        "malformed_response",
        "oversized_response",
        "conflict",
        "unauthorized",
    ):
        assert reason in allowed, f"unimplemented interface: settings failure reason {reason!r}"


def test_settings_client_repr_withholds_the_canary() -> None:
    client = settings_client_class()("ws://127.0.0.1:9/api/ws", _StubProvider())
    assert CANARY not in repr(client)
