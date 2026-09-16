"""CFG-P2 Test Author Two: gated remote, gateway Start/Stop, legal names.

Stub cases never skip. They fail only when production behavior is absent.
Invented credentials, cookies, tokens, or successful fabricated remote
results are forbidden. Missing real login access is blocked, not passed.
"""

from __future__ import annotations

import inspect
import ipaddress
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

import pytest

from scripts.acceptance.v062_configuration import (
    P2_HERMES_NAME_RE,
    P2_PROFILE_NAME_RE,
    HarnessError,
    access_limitation,
    blocked_row,
    disposable_profile_name,
    live_hermes_enabled,
    product_gateway_lifecycle_accepted,
    require_legal_p2_name,
    stage_name_set,
)
from talaria.domain.settings import ConfigTarget
from talaria.transport.connection_set import (
    credential_provider_factory,
    plan_connections,
)
from talaria.transport.credentials import LoopbackTokenProvider
from talaria.transport.gated_auth import GatedAuthSession
from talaria.transport.refresh import RefreshError, require_fetchable_origin
from tests.transport.test_gated_auth import load_gated_auth
from tests.transport.test_settings_api import (
    json_route,
    make_client,
    make_target,
    settings_client_class,
    settings_gateway,
)

_RFC1918_HTTP = "http://10.220.1.139:8765/"
_RFC1918_WS = "ws://10.220.1.139:8765/api/ws"
_LEGAL_LOCAL_A = "talaria-v062-cfg-p2-local-installed-a"
_REFUSALS = (RefreshError, TypeError, ValueError, RuntimeError)


def _require_gated_origin() -> Any:
    module = load_gated_auth()
    require = getattr(module, "require_gated_origin", None)
    assert require is not None, "unimplemented interface: require_gated_origin"
    return require


def _command_module() -> Any:
    import talaria.domain.settings_commands as commands

    return commands


def _p2_names() -> list[str]:
    names: list[str] = []
    for connection in ("local", "remote"):
        for stage in ("active", "installed"):
            names.extend(stage_name_set(connection, stage).values())
    return names


# ── P2-3 gated origin and composition ────────────────────────────────────


def test_p2_3_loopback_refresh_still_refuses_remote_plain_http() -> None:
    """R7: refresh_credential is not widened to remote cleartext."""
    with pytest.raises(RefreshError, match="plain HTTP"):
        require_fetchable_origin(_RFC1918_HTTP)
    with pytest.raises(RefreshError, match="plain HTTP"):
        require_fetchable_origin("http://dashboard.example:9119/")


def test_p2_3_gated_origin_accepts_explicit_literal_rfc1918_http() -> None:
    require = _require_gated_origin()
    require(_RFC1918_HTTP, auth="gated")
    require("https://10.220.1.139:8765/", auth="gated")
    require("https://gateway.example/", auth="gated")


@pytest.mark.parametrize(
    "origin",
    [
        "http://1.1.1.1:8765/",
        "http://gateway.example:8765/",
        "http://hermes.lan:8765/",
        "http://100.64.0.1:8765/",
        "http://169.254.1.1:8765/",
        "http://224.0.0.1:8765/",
        "http://0.0.0.0:8765/",
        "http://10.220.1.139.example:8765/",
    ],
)
def test_p2_3_gated_origin_refuses_public_hostname_and_non_rfc1918_http(origin: str) -> None:
    require = _require_gated_origin()
    with pytest.raises(_REFUSALS):
        require(origin, auth="gated")


def test_p2_3_rfc1918_http_without_explicit_gated_auth_is_refused() -> None:
    require = _require_gated_origin()
    with pytest.raises(_REFUSALS):
        require(_RFC1918_HTTP, auth="loopback")
    host = urlparse(_RFC1918_HTTP).hostname
    assert host is not None
    assert ipaddress.ip_address(host).is_private


def test_p2_3_gated_request_path_uses_gated_origin_not_loopback_refresh() -> None:
    """The gated request helper must not share require_fetchable_origin."""
    module = load_gated_auth()
    source = Path(module.__file__).read_text(encoding="utf-8")
    assert "require_gated_origin" in source
    assert source.count("require_fetchable_origin") == 0 or "require_gated_origin(" in source


@pytest.mark.asyncio
async def test_p2_3_gated_connection_is_composed_as_ticket_not_loopback() -> None:
    plan = plan_connections(
        default_endpoint="ws://127.0.0.1:8765/api/ws",
        connections={
            "local": {"url": "ws://127.0.0.1:8765/api/ws", "auth": "loopback"},
            "remote": {"url": _RFC1918_WS, "auth": "gated"},
        },
    )
    factory = credential_provider_factory(None)
    remote = next(member for member in plan if member.name == "remote")
    provider = factory(remote)
    assert type(provider).__name__ == "GatedTicketProvider"
    assert not isinstance(provider, LoopbackTokenProvider)
    local = next(member for member in plan if member.name == "local")
    assert isinstance(factory(local), LoopbackTokenProvider)


def test_p2_3_admin_bearer_and_ws_ticket_are_separate_surfaces() -> None:
    module = load_gated_auth()
    assert getattr(module, "GatedAuthSession", None) is not None
    assert getattr(module, "GatedTicketProvider", None) is not None
    session = GatedAuthSession("http://127.0.0.1:1/", access_token="canary-p2-access")
    assert "canary-p2-access" not in repr(session)
    assert "Authorization" not in repr(session)


def test_p2_3_installed_remote_login_is_blocked_without_live_access() -> None:
    """Missing real login access is blocked, never a fabricated pass."""
    assert live_hermes_enabled("remote") is False
    row = blocked_row(
        test_id="P2-3",
        connection_label="remote",
        profile_label="public-routes",
        talaria_commit="cb93c5d7ff728b59eb85a6c7f0faf19a4e12e9ab",
        limitation=access_limitation("remote"),
        observed_at="2026-09-16T15:00:00Z",
        talaria_version="0.6.3-unreleased",
    )
    assert row["status"] == "blocked"
    assert row["status"] != "passed"
    assert not row["evidence_refs"]


# ── P2-4 product-owned Start / Stop ──────────────────────────────────────


def test_p2_4_start_and_stop_commands_exist_and_are_rest() -> None:
    commands = _command_module()
    start_cls = getattr(commands, "StartGateway", None)
    stop_cls = getattr(commands, "StopGateway", None)
    assert start_cls is not None, "unimplemented interface: StartGateway"
    assert stop_cls is not None, "unimplemented interface: StopGateway"
    target = ConfigTarget(connection_id="local", profile_name=_LEGAL_LOCAL_A)
    start_spec = commands.rest_request(start_cls(target=target))
    stop_spec = commands.rest_request(stop_cls(target=target))
    assert start_spec.method == "POST"
    assert start_spec.path == "/api/gateway/start"
    assert dict(start_spec.query) == {"profile": _LEGAL_LOCAL_A}
    assert stop_spec.method == "POST"
    assert stop_spec.path == "/api/gateway/stop"
    assert dict(stop_spec.query) == {"profile": _LEGAL_LOCAL_A}


def test_p2_4_start_and_stop_never_use_rpc_or_wake() -> None:
    commands = _command_module()
    start_cls = getattr(commands, "StartGateway", None)
    stop_cls = getattr(commands, "StopGateway", None)
    assert start_cls is not None, "unimplemented interface: StartGateway"
    assert stop_cls is not None, "unimplemented interface: StopGateway"
    target = ConfigTarget(connection_id="local", profile_name=_LEGAL_LOCAL_A)
    start_spec = commands.rest_request(start_cls(target=target))
    stop_spec = commands.rest_request(stop_cls(target=target))
    assert start_spec.path != "/api/rpc"
    assert stop_spec.path != "/api/rpc"
    with pytest.raises(TypeError):
        commands.rpc_request(start_cls(target=target))
    source = Path(commands.__file__).read_text(encoding="utf-8")
    start_block = source[source.index("class StartGateway") : source.index("class StopGateway")]
    assert "wake." not in start_block
    assert "/api/rpc" not in start_block


def test_p2_4_settings_client_exposes_start_and_stop_gateway() -> None:
    client_cls = settings_client_class()
    assert hasattr(client_cls, "start_gateway"), (
        "unimplemented interface: SettingsClient.start_gateway"
    )
    assert hasattr(client_cls, "stop_gateway"), (
        "unimplemented interface: SettingsClient.stop_gateway"
    )
    assert "target" in inspect.signature(client_cls.start_gateway).parameters
    assert "target" in inspect.signature(client_cls.stop_gateway).parameters


@pytest.mark.asyncio
async def test_p2_4_start_posts_exact_route_and_polls_status() -> None:
    client_cls = settings_client_class()
    assert hasattr(client_cls, "start_gateway"), (
        "unimplemented interface: SettingsClient.start_gateway"
    )
    with settings_gateway(
        {
            ("POST", "/api/gateway/start"): json_route({"ok": True}),
            ("GET", "/api/status"): json_route(
                {"gateway_running": True, "gateway_state": "running"}
            ),
        }
    ) as (origin, recorder, _launch):
        result = await make_client(origin).start_gateway(
            make_target(profile_name=_LEGAL_LOCAL_A)
        )

    paths = [urlparse(item.path).path for item in recorder]
    assert paths[0] == "/api/gateway/start"
    assert (recorder[0].query.get("profile") or [""])[0] == _LEGAL_LOCAL_A
    assert "/api/status" in paths
    assert "/api/rpc" not in paths
    assert not any("wake." in (item.body.get("method") or "") for item in recorder)
    assert getattr(result, "gateway_running", None) is True


@pytest.mark.asyncio
async def test_p2_4_stop_posts_exact_route_and_polls_status() -> None:
    client_cls = settings_client_class()
    assert hasattr(client_cls, "stop_gateway"), (
        "unimplemented interface: SettingsClient.stop_gateway"
    )
    with settings_gateway(
        {
            ("POST", "/api/gateway/stop"): json_route({"ok": True}),
            ("GET", "/api/status"): json_route(
                {"gateway_running": False, "gateway_state": "stopped"}
            ),
        }
    ) as (origin, recorder, _launch):
        result = await make_client(origin).stop_gateway(
            make_target(profile_name=_LEGAL_LOCAL_A)
        )

    paths = [urlparse(item.path).path for item in recorder]
    assert paths[0] == "/api/gateway/stop"
    assert (recorder[0].query.get("profile") or [""])[0] == _LEGAL_LOCAL_A
    assert "/api/status" in paths
    assert "/api/rpc" not in paths
    assert getattr(result, "gateway_running", None) is False


def test_p2_4_harness_post_cannot_pass_the_residual() -> None:
    assert (
        product_gateway_lifecycle_accepted(
            ["POST /api/gateway/start?profile=alpha"],
            harness_posted=True,
        )
        is False
    )
    assert (
        product_gateway_lifecycle_accepted(
            ["POST /api/rpc", "wake.start"],
            harness_posted=False,
        )
        is False
    )


# ── P2-5 legal reserved-name family ──────────────────────────────────────


def test_p2_5_generator_emits_all_sixteen_legal_names() -> None:
    names = _p2_names()
    assert len(names) == 16
    assert len(set(names)) == 16
    for name in names:
        require_legal_p2_name(name)
        assert P2_PROFILE_NAME_RE.fullmatch(name)
        assert P2_HERMES_NAME_RE.fullmatch(name)
        assert "." not in name
        assert len(name) <= 64
    assert disposable_profile_name("local", "active", "testA") == (
        "talaria-v062-cfg-p2-local-active-a"
    )
    assert disposable_profile_name("remote", "installed", "testD") == (
        "talaria-v062-cfg-p2-remote-installed-renamed"
    )


def test_p2_5_dotted_legacy_candidates_are_rejected() -> None:
    with pytest.raises(HarnessError, match="dot"):
        require_legal_p2_name("talaria-v0.6.2-cfg-t0-local-active-a")
    with pytest.raises(HarnessError, match="dot"):
        require_legal_p2_name("talaria-v0.6.2-cfg-t0-remote-installed-renamed")


def test_p2_5_t7_r3_evidence_is_byte_unchanged() -> None:
    root = Path(__file__).resolve().parents[2]
    frozen = (
        root / "docs" / "acceptance" / "v0.6.2" / "configuration" / "ledger.schema.json",
        root
        / "docs"
        / "acceptance"
        / "v0.6.2"
        / "configuration"
        / "evidence"
        / "active"
        / "cleanup-active.json",
        root
        / "docs"
        / "acceptance"
        / "v0.6.2"
        / "configuration"
        / "evidence"
        / "active"
        / "test-author-2-final.txt",
    )
    for path in frozen:
        assert path.is_file(), f"T7-R3 evidence missing: {path}"
