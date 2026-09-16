"""CFG-A1-20 and the gated authentication session (CFG-P1 U2 / KTD7).

Stub-server tests never skip. They fail only when ``talaria.transport.gated_auth``
is missing or when a gated session reuses a ticket or leaks a credential.
"""

from __future__ import annotations

import contextlib
import importlib
import json
from collections.abc import Iterator
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, HTTPServer
from threading import Thread
from typing import Any
from urllib.parse import parse_qs, urlencode, urlparse

import pytest

from talaria.transport.attach import AttachSuccess, AttachTarget, attach
from talaria.transport.credentials import Credential
from tests.transport.conftest import StubGateway

CANARY = "canary-v062-gated-do-not-leak"
ACCESS_CANARY = "canary-v062-access-do-not-leak"
REFRESH_CANARY = "canary-v062-refresh-do-not-leak"
TICKET_ONE = "canary-v062-ticket-one-do-not-leak"
TICKET_TWO = "canary-v062-ticket-two-do-not-leak"
AUTH_CODE = "canary-v062-auth-code-do-not-leak"


@dataclass
class Recorded:
    method: str
    path: str
    headers: dict[str, str]
    body: dict[str, Any] = field(default_factory=dict)


@contextlib.contextmanager
def auth_gateway(
    *,
    auth_required: bool = True,
    password_advertised: bool = False,
    tickets: list[str] | None = None,
) -> Iterator[tuple[str, list[Recorded]]]:
    recorder: list[Recorded] = []
    remaining = list(tickets or [TICKET_ONE, TICKET_TWO])

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, format: str, *args: Any) -> None:
            return

        def _record(self, method: str) -> dict[str, Any]:
            length = int(self.headers.get("Content-Length", "0") or "0")
            raw = self.rfile.read(length) if length else b""
            try:
                parsed = json.loads(raw) if raw else {}
            except json.JSONDecodeError:
                parsed = {}
            if not isinstance(parsed, dict):
                parsed = {}
            recorder.append(Recorded(method, self.path, dict(self.headers), parsed))
            return parsed

        def _send(self, status: int, body: Any) -> None:
            payload = json.dumps(body).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_GET(self) -> None:
            self._record("GET")
            path = urlparse(self.path).path
            if path == "/api/health":
                self._send(200, {"ok": True, "version": "0.21.3", "auth_required": auth_required})
                return
            if path == "/api/config/schema":
                self._send(200, {"fields": {}, "category_order": []})
                return
            if path == "/api/auth/providers":
                self._send(
                    200,
                    {"providers": ["native"], "password": password_advertised},
                )
                return
            if path == "/auth/native/authorize":
                query = parse_qs(urlparse(self.path).query, keep_blank_values=True)
                redirect_uri = (query.get("redirect_uri") or [""])[0]
                state = (query.get("state") or [""])[0]
                if redirect_uri and state:
                    location = (
                        f"{redirect_uri}?{urlencode({'code': AUTH_CODE, 'state': state})}"
                    )
                    self.send_response(302)
                    self.send_header("Location", location)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                self._send(400, {"detail": "redirect_uri and state are required"})
                return
            if path in {"/api/profiles", "/api/config", "/api/auth/me"}:
                self._send(401, {"detail": "Unauthorized"})
                return
            self._send(404, {"detail": "Not Found"})

        def do_POST(self) -> None:
            body = self._record("POST")
            path = urlparse(self.path).path
            if path == "/auth/native/token":
                if body.get("code") != AUTH_CODE:
                    self._send(400, {"detail": "authorization code required"})
                    return
                self._send(
                    200,
                    {"access_token": ACCESS_CANARY, "refresh_token": REFRESH_CANARY},
                )
                return
            if path == "/auth/native/refresh":
                self._send(200, {"access_token": ACCESS_CANARY, "refresh_token": REFRESH_CANARY})
                return
            if path == "/auth/password-login":
                if not password_advertised:
                    self._send(404, {"detail": "Not Found"})
                    return
                self._send(
                    200,
                    {"access_token": ACCESS_CANARY, "refresh_token": REFRESH_CANARY},
                )
                return
            if path == "/api/auth/ws-ticket":
                ticket = remaining.pop(0) if remaining else TICKET_TWO
                self._send(200, {"ticket": ticket})
                return
            if path == "/auth/password-login" and body.get("password") == "wrong":
                self._send(401, {"detail": "Unauthorized"})
                return
            self._send(401, {"detail": "Unauthorized"})

    server = HTTPServer(("127.0.0.1", 0), Handler)
    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}/", recorder
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def load_gated_auth() -> Any:
    try:
        return importlib.import_module("talaria.transport.gated_auth")
    except ImportError as exc:
        raise AssertionError("unimplemented interface: talaria.transport.gated_auth") from exc


def session_class() -> Any:
    module = load_gated_auth()
    cls = getattr(module, "GatedAuthSession", None)
    if cls is None:
        raise AssertionError("unimplemented interface: GatedAuthSession")
    return cls


# ── A1-20 ────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_a1_20_wrong_token_on_protected_routes_is_reauth_and_public_routes_load() -> None:
    with auth_gateway() as (origin, recorder):
        session_cls = session_class()
        session = session_cls(origin, access_token="wrong-token")
        public = await session.probe_public()
        protected = await session.probe_protected()

    assert public["health"]["ok"] is True
    assert public["schema"]["fields"] == {}
    assert protected["state"] in {"reauth", "unauthorized", "re-authenticate"}
    assert any(item.path.startswith("/api/health") for item in recorder)
    assert any("/api/config/schema" in item.path for item in recorder)
    assert CANARY not in repr(session)
    assert "wrong-token" not in repr(session)


@pytest.mark.asyncio
async def test_a1_20_no_token_still_loads_health_and_schema() -> None:
    with auth_gateway() as (origin, recorder):
        session_cls = session_class()
        session = session_cls(origin)
        public = await session.probe_public()

    assert public["health"]["auth_required"] is True
    assert public["schema"] is not None
    health = next(item for item in recorder if urlparse(item.path).path == "/api/health")
    assert "Authorization" not in health.headers or not health.headers.get("Authorization")


# ── gated session: native, password fallback, refresh, unique tickets ──


@pytest.mark.asyncio
async def test_native_authorization_uses_pkce_and_withholds_tokens() -> None:
    with auth_gateway() as (origin, recorder):
        session_cls = session_class()
        session = await session_cls.authorize_native(origin, redirect_host="127.0.0.1")

    authorize = next(
        item for item in recorder if urlparse(item.path).path == "/auth/native/authorize"
    )
    query = parse_qs(urlparse(authorize.path).query)
    assert "code_challenge" in query
    assert "code_challenge_method" in query
    token = next(
        item for item in recorder if urlparse(item.path).path == "/auth/native/token"
    )
    assert token.body.get("code") == AUTH_CODE
    assert AUTH_CODE not in repr(session)
    assert ACCESS_CANARY not in repr(session)
    assert REFRESH_CANARY not in repr(session)


@pytest.mark.asyncio
async def test_password_fallback_is_offered_only_when_advertised() -> None:
    with auth_gateway(password_advertised=False) as (origin, _recorder):
        session_cls = session_class()
        advertised = await session_cls.password_available(origin)
        assert advertised is False
        with pytest.raises((TypeError, ValueError, RuntimeError, LookupError, OSError)):
            await session_cls.password_login(origin, username="operator", password=CANARY)

    with auth_gateway(password_advertised=True) as (origin, recorder):
        session_cls = session_class()
        assert await session_cls.password_available(origin) is True
        session = await session_cls.password_login(origin, username="operator", password=CANARY)
        assert CANARY not in repr(session)
        posted = next(
            item for item in recorder if urlparse(item.path).path == "/auth/password-login"
        )
        assert posted.body.get("password") == CANARY
        assert CANARY not in repr(posted.headers)


@pytest.mark.asyncio
async def test_refresh_rotates_bearer_access_without_exposing_the_refresh_token() -> None:
    with auth_gateway() as (origin, recorder):
        session_cls = session_class()
        session = session_cls(
            origin, access_token="expired-access", refresh_token=REFRESH_CANARY
        )
        await session.refresh()

    refresh = next(item for item in recorder if urlparse(item.path).path == "/auth/native/refresh")
    assert REFRESH_CANARY not in repr(session)
    assert refresh.body.get("refresh_token") == REFRESH_CANARY or "refresh" in refresh.path


@pytest.mark.asyncio
async def test_each_dial_and_reconnect_mints_a_unique_ws_ticket(
    gateway: StubGateway,
) -> None:
    with auth_gateway(tickets=[TICKET_ONE, TICKET_TWO]) as (origin, _recorder):
        session_cls = session_class()
        session = session_cls(origin, access_token=ACCESS_CANARY)
        first = await session.mint_ws_ticket()
        second = await session.mint_ws_ticket()

    assert isinstance(first, Credential)
    assert first.parameter == "ticket"
    assert first.value == TICKET_ONE
    assert second.value == TICKET_TWO
    assert first.value != second.value
    assert TICKET_ONE not in repr(first)
    assert TICKET_TWO not in repr(second)

    gateway.require_auth = False
    target = AttachTarget.from_url(gateway.url)
    outcome = await attach(target, first)
    assert isinstance(outcome, AttachSuccess)
    await outcome.connection.close()
    assert gateway.queries[-1] == {"ticket": TICKET_ONE}


def test_gated_credentials_are_not_loopback_html_tokens() -> None:
    module = load_gated_auth()
    assert getattr(module, "GatedAuthSession", None) is not None
    refresh = importlib.import_module("talaria.transport.refresh")
    assert module is not refresh


def test_loopback_token_provider_remains_importable_beside_gated_auth() -> None:
    load_gated_auth()
    from talaria.transport.credentials import LoopbackTokenProvider

    assert LoopbackTokenProvider is not None
