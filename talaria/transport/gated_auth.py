"""Gated dashboard authentication: native PKCE, password fallback, tickets.

Loopback ``?token=`` stays in :mod:`talaria.transport.credentials`. This module
is the KTD7 session seam for a remote dashboard that rotates Bearer access,
persists refresh material only through the credential file, and mints a fresh
single-use WS ticket on every dial.
"""

from __future__ import annotations

import asyncio
import base64
import hashlib
import http.client
import json
import secrets
import urllib.error
import urllib.request
from collections.abc import Mapping
from http.server import BaseHTTPRequestHandler, HTTPServer
from threading import Thread
from typing import Any
from urllib.parse import parse_qs, urlencode, urljoin, urlparse, urlsplit

from talaria.transport.admin import MAX_RESPONSE_BYTES
from talaria.transport.credentials import Credential
from talaria.transport.refresh import RefreshError, require_fetchable_origin

__all__ = [
    "GatedAuthError",
    "GatedAuthSession",
    "GatedTicketProvider",
]


class _NoRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(
        self,
        req: urllib.request.Request,
        fp: Any,
        code: int,
        msg: str,
        headers: http.client.HTTPMessage,
        newurl: str,
    ) -> urllib.request.Request | None:
        return None


_OPENER = urllib.request.build_opener(_NoRedirects)


class GatedAuthError(RuntimeError):
    """A gated-auth call that did not produce a session or ticket."""

    def __init__(self, reason: str, message: str) -> None:
        super().__init__(message)
        self.reason = reason

    def __repr__(self) -> str:
        return f"GatedAuthError(reason={self.reason!r}, message={str(self)!r})"


def _host_of(origin: str) -> str:
    try:
        return urlsplit(origin).hostname or "an unnamed host"
    except ValueError:
        return "an unnamed host"


def _build_url(origin: str, path: str, params: dict[str, str] | None) -> str:
    candidate = urljoin(origin, path)
    origin_parts, candidate_parts = urlsplit(origin), urlsplit(candidate)
    if (origin_parts.scheme, origin_parts.netloc) != (
        candidate_parts.scheme,
        candidate_parts.netloc,
    ):
        raise GatedAuthError(
            "refused_origin",
            f"refusing to send a credential to a different origin for path {path!r}",
        )
    return f"{candidate}?{urlencode(params)}" if params else candidate


def _request_json(
    origin: str,
    path: str,
    *,
    method: str = "GET",
    params: dict[str, str] | None = None,
    body: Mapping[str, Any] | None = None,
    token: str | None = None,
    timeout: float = 15.0,
) -> Any:
    try:
        require_fetchable_origin(origin)
    except RefreshError as exc:
        raise GatedAuthError("refused_origin", str(exc)) from exc

    url = _build_url(origin, path, params)
    headers = {"Accept": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    payload: bytes | None = None
    verb = method.upper()
    if verb in {"POST", "PUT", "PATCH"} or body is not None:
        payload = json.dumps(body if body is not None else {}).encode("utf-8")
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(url, data=payload, headers=headers, method=verb)
    try:
        with _OPENER.open(request, timeout=timeout) as response:  # nosec B310
            raw = response.read(MAX_RESPONSE_BYTES + 1)
            charset = response.headers.get_content_charset() or "utf-8"
    except TimeoutError as exc:
        raise GatedAuthError("timeout", f"{path} timed out waiting for {_host_of(origin)}") from exc
    except urllib.error.HTTPError as exc:
        try:
            exc.read(MAX_RESPONSE_BYTES)
        except (OSError, http.client.HTTPException):
            pass
        if exc.code == 401:
            raise GatedAuthError("unauthorized", f"unauthorized at {path}") from exc
        if exc.code == 404:
            raise GatedAuthError(
                "absent_capability", f"this gateway does not serve {path}"
            ) from exc
        raise GatedAuthError(
            "http_error", f"the gateway answered HTTP {exc.code} at {path}"
        ) from exc
    except OSError as exc:
        raise GatedAuthError(
            "unreachable",
            f"no gateway answered at {_host_of(origin)} for {path}",
        ) from exc

    if len(raw) > MAX_RESPONSE_BYTES:
        raise GatedAuthError(
            "oversized_response",
            f"{path} answered with more than {MAX_RESPONSE_BYTES} bytes",
        )
    try:
        return json.loads(raw.decode(charset, "replace"))
    except (json.JSONDecodeError, LookupError) as exc:
        raise GatedAuthError("malformed_response", f"{path} did not answer with JSON") from exc


def _pkce_pair() -> tuple[str, str]:
    verifier = secrets.token_urlsafe(64)
    digest = hashlib.sha256(verifier.encode("ascii")).digest()
    challenge = base64.urlsafe_b64encode(digest).rstrip(b"=").decode("ascii")
    return verifier, challenge


def _code_from_location(location: str, *, expected_state: str, callback_origin: str) -> str | None:
    """Return the authorization code if ``location`` is our loopback callback."""
    parts = urlsplit(location)
    callback = urlsplit(callback_origin)
    if (parts.scheme, parts.hostname) != (callback.scheme, callback.hostname):
        return None
    if parts.port != callback.port:
        return None
    query = parse_qs(parts.query, keep_blank_values=True)
    states = query.get("state") or []
    codes = query.get("code") or []
    if not codes or not states or states[0] != expected_state:
        return None
    code = codes[0]
    return code or None


def _get_authorize(
    origin: str,
    *,
    params: dict[str, str],
    timeout: float = 15.0,
) -> tuple[int, str | None]:
    """GET ``/auth/native/authorize`` without following redirects.

    Returns ``(status, Location-or-None)``. A 3xx is the RFC 8252 user-agent
    redirect; the Location is returned so a loopback ``code`` can be read
    without inventing one.
    """
    try:
        require_fetchable_origin(origin)
    except RefreshError as exc:
        raise GatedAuthError("refused_origin", str(exc)) from exc

    url = _build_url(origin, "/auth/native/authorize", params)
    request = urllib.request.Request(url, headers={"Accept": "application/json"}, method="GET")
    try:
        with _OPENER.open(request, timeout=timeout) as response:  # nosec B310
            response.read(MAX_RESPONSE_BYTES + 1)
            return int(response.status), response.headers.get("Location")
    except urllib.error.HTTPError as exc:
        try:
            exc.read(MAX_RESPONSE_BYTES)
        except (OSError, http.client.HTTPException):
            pass
        return int(exc.code), exc.headers.get("Location") if exc.headers is not None else None
    except TimeoutError as exc:
        raise GatedAuthError(
            "timeout", f"/auth/native/authorize timed out waiting for {_host_of(origin)}"
        ) from exc
    except OSError as exc:
        raise GatedAuthError(
            "unreachable",
            f"no gateway answered at {_host_of(origin)} for /auth/native/authorize",
        ) from exc


class GatedAuthSession:
    """One dashboard origin's Bearer/refresh/ticket session."""

    def __init__(
        self,
        origin: str,
        access_token: str | None = None,
        refresh_token: str | None = None,
    ) -> None:
        self.origin = origin if origin.endswith("/") else f"{origin}/"
        self._access_token = access_token or ""
        self._refresh_token = refresh_token or ""

    def __repr__(self) -> str:
        return f"GatedAuthSession(origin={self.origin!r})"

    async def probe_public(self) -> dict[str, Any]:
        health = await asyncio.to_thread(_request_json, self.origin, "/api/health")
        schema = await asyncio.to_thread(_request_json, self.origin, "/api/config/schema")
        return {"health": health, "schema": schema}

    async def probe_protected(self) -> dict[str, Any]:
        try:
            await asyncio.to_thread(
                _request_json,
                self.origin,
                "/api/auth/me",
                token=self._access_token or None,
            )
        except GatedAuthError as exc:
            if exc.reason == "unauthorized":
                return {"state": "reauth"}
            raise
        return {"state": "authenticated"}

    @classmethod
    async def authorize_native(
        cls, origin: str, *, redirect_host: str = "127.0.0.1"
    ) -> GatedAuthSession:
        verifier, challenge = _pkce_pair()
        state = secrets.token_urlsafe(32)
        loop = asyncio.get_running_loop()
        delivered: asyncio.Future[str] = loop.create_future()

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                query = parse_qs(urlparse(self.path).query, keep_blank_values=True)
                codes = query.get("code") or []
                states = query.get("state") or []
                self.send_response(200)
                self.send_header("Content-Type", "text/plain")
                self.send_header("Content-Length", "2")
                self.end_headers()
                self.wfile.write(b"ok")
                if codes and states and states[0] == state and codes[0]:
                    code = codes[0]

                    def _set() -> None:
                        if not delivered.done():
                            delivered.set_result(code)

                    loop.call_soon_threadsafe(_set)

            def log_message(self, format: str, *args: Any) -> None:
                return

        server = HTTPServer((redirect_host, 0), Handler)
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        redirect_uri = f"http://{redirect_host}:{server.server_port}/callback"
        try:
            _status, location = await asyncio.to_thread(
                _get_authorize,
                origin,
                params={
                    "response_type": "code",
                    "code_challenge": challenge,
                    "code_challenge_method": "S256",
                    "redirect_uri": redirect_uri,
                    "state": state,
                },
            )
            code: str | None = None
            if location:
                code = _code_from_location(
                    location, expected_state=state, callback_origin=redirect_uri
                )
            if code is None:
                try:
                    code = await asyncio.wait_for(delivered, timeout=0.25)
                except TimeoutError:
                    code = None
            if not code:
                raise GatedAuthError(
                    "absent_capability",
                    "authorization code was not delivered to the loopback callback; "
                    "refusing to invent one",
                )
            tokens = await asyncio.to_thread(
                _request_json,
                origin,
                "/auth/native/token",
                method="POST",
                body={
                    "grant_type": "authorization_code",
                    "code": code,
                    "code_verifier": verifier,
                    "redirect_uri": redirect_uri,
                },
            )
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)

        if not isinstance(tokens, dict):
            raise GatedAuthError(
                "malformed_response", "/auth/native/token did not answer with JSON"
            )
        access = tokens.get("access_token")
        refresh = tokens.get("refresh_token")
        if not isinstance(access, str) or not access:
            raise GatedAuthError(
                "malformed_response", "/auth/native/token did not return an access token"
            )
        return cls(
            origin,
            access_token=access,
            refresh_token=refresh if isinstance(refresh, str) else "",
        )

    @classmethod
    async def password_available(cls, origin: str) -> bool:
        providers = await asyncio.to_thread(_request_json, origin, "/api/auth/providers")
        if not isinstance(providers, dict):
            return False
        return providers.get("password") is True

    @classmethod
    async def password_login(
        cls, origin: str, *, username: str, password: str
    ) -> GatedAuthSession:
        if not await cls.password_available(origin):
            raise GatedAuthError(
                "absent_capability",
                "password login is not advertised by this dashboard",
            )
        tokens = await asyncio.to_thread(
            _request_json,
            origin,
            "/auth/password-login",
            method="POST",
            body={"username": username, "password": password},
        )
        if not isinstance(tokens, dict):
            raise GatedAuthError(
                "malformed_response", "/auth/password-login did not answer with JSON"
            )
        access = tokens.get("access_token")
        refresh = tokens.get("refresh_token")
        return cls(
            origin,
            access_token=access if isinstance(access, str) else "",
            refresh_token=refresh if isinstance(refresh, str) else "",
        )

    async def refresh(self) -> None:
        tokens = await asyncio.to_thread(
            _request_json,
            self.origin,
            "/auth/native/refresh",
            method="POST",
            body={"refresh_token": self._refresh_token},
        )
        if not isinstance(tokens, dict):
            raise GatedAuthError(
                "malformed_response", "/auth/native/refresh did not answer with JSON"
            )
        access = tokens.get("access_token")
        if isinstance(access, str):
            self._access_token = access
        rotated = tokens.get("refresh_token")
        if isinstance(rotated, str):
            self._refresh_token = rotated

    async def mint_ws_ticket(self) -> Credential:
        payload = await asyncio.to_thread(
            _request_json,
            self.origin,
            "/api/auth/ws-ticket",
            method="POST",
            token=self._access_token or None,
        )
        if not isinstance(payload, dict) or not isinstance(payload.get("ticket"), str):
            raise GatedAuthError(
                "malformed_response", "/api/auth/ws-ticket did not return a ticket"
            )
        return Credential(parameter="ticket", value=payload["ticket"], source="file")


class GatedTicketProvider:
    """Per-dial WS ticket source. Each :meth:`acquire` mints a fresh ticket."""

    def __init__(self, session: GatedAuthSession) -> None:
        self._session = session

    def __repr__(self) -> str:
        return "GatedTicketProvider(value=<withheld>)"

    async def acquire(self) -> Credential:
        return await self._session.mint_ws_ticket()
