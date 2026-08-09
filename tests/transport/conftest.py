"""A stub Hermes gateway: a real WebSocket server on loopback.

Not a mock. Every test in this package dials it with the real ``websockets``
client, over a real socket, so the thing being proved — that the credential
arrives as a ``?token=`` **URL query parameter** and nowhere else — is proved
against a server that reads the query the same way Hermes does
(``hermes_cli/web_server.py:14443-14524`` at ``7f4d15515``: ``ws.query_params``,
never a header).

A hand-written double would have made that assertion vacuous. It would be
asserting that Talaria calls a function with a string, not that a server which
only looks at the query string can authenticate the connection.

The stub is deliberately small. It answers ``gateway.ready`` like the real
handler does (``tui_gateway/ws.py``), replies to JSON-RPC requests through an
injectable responder, and can hang up on command. It does **not** re-implement
the gateway's dispatch surface; a test that needs a specific method's semantics
supplies its own responder.

**It also records the whole handshake, not only the request path**, and that
is what makes the "and nowhere else" half of the claim testable. Recording the
path alone left the stub blind to a credential duplicated into a header, a
cookie, or a ``Sec-WebSocket-Protocol`` value: patching the dialler to add an
``X-Talaria-Token`` header on every dial passed the entire suite. A server that
cannot see a header cannot testify that no header carried the token. See
:attr:`StubGateway.handshakes`.
"""

from __future__ import annotations

import asyncio
import http
import json
from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass, field
from typing import Any, Literal
from urllib.parse import parse_qsl, urlsplit

import pytest
import pytest_asyncio
from websockets.asyncio.server import Server, ServerConnection, serve
from websockets.http11 import Request, Response

#: The token the stub accepts unless a test says otherwise. Distinctive enough
#: that a canary sweep over a frame log or an environment can search for it.
STUB_TOKEN = "stub-session-token-7f4d15515"

#: Frame the real handler sends immediately after accepting
#: (``tui_gateway/ws.py``), reproduced so the domain sees the same first event
#: live as it does in a replay corpus.
READY_FRAME: dict[str, Any] = {
    "jsonrpc": "2.0",
    "method": "event",
    "params": {
        "type": "gateway.ready",
        "payload": {"skin": "default", "change_events": True},
    },
}


def event(kind: str, payload: dict[str, Any], *, session: str = "s1") -> dict[str, Any]:
    """One gateway event frame, in the shape the decoder expects."""
    return {
        "jsonrpc": "2.0",
        "method": "event",
        "params": {"type": kind, "session_id": session, "payload": payload},
    }


def ok(request_id: Any, result: dict[str, Any]) -> dict[str, Any]:
    """A success reply, matching ``tui_gateway/server.py:1719-1720``."""
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def err(request_id: Any, code: int, message: str) -> dict[str, Any]:
    """An error reply, matching ``tui_gateway/server.py:1723-1724``."""
    return {"jsonrpc": "2.0", "id": request_id, "error": {"code": code, "message": message}}


@dataclass
class Handshake:
    """One upgrade attempt as the server received it, credential sweep included.

    ``headers`` is a list of pairs rather than a dict because a handshake may
    repeat a header name — ``Cookie`` and ``Sec-WebSocket-Protocol`` both
    legitimately appear more than once — and a dict would silently drop all but
    the last, which is precisely where a duplicated credential would hide.
    """

    path: str
    headers: list[tuple[str, str]]

    @property
    def query(self) -> dict[str, str]:
        return dict(parse_qsl(urlsplit(self.path).query, keep_blank_values=True))

    def everything_but_the_query(self) -> list[str]:
        """Every part of the handshake the credential must **not** appear in.

        The path without its query, plus every header name and value. This is
        the haystack the "and nowhere else" assertions search.
        """
        parts = urlsplit(self.path)
        found = [parts.path, parts.fragment]
        for name, value in self.headers:
            found.append(name)
            found.append(value)
        return [part for part in found if part]


@dataclass
class Session:
    """One accepted connection, and everything it was handed."""

    query: dict[str, str]
    connection: ServerConnection
    received: list[dict[str, Any]] = field(default_factory=list)


class StubGateway:
    """A loopback WebSocket server that behaves like the gateway's ``/api/ws``."""

    def __init__(
        self,
        *,
        token: str = STUB_TOKEN,
        require_auth: bool = True,
        reject_with: Literal["handshake", "close"] = "handshake",
        greeting: list[dict[str, Any]] | None = None,
        responder: Callable[[dict[str, Any], StubGateway], dict[str, Any] | None] | None = None,
        follow_ups: dict[str, list[dict[str, Any]]] | None = None,
    ) -> None:
        self.token = token
        self.require_auth = require_auth
        #: How a bad credential is refused. Hermes calls ``ws.close(code=4401)``
        #: *before* accepting the upgrade, and whether the client observes that
        #: as an HTTP handshake rejection or as a post-accept close is decided
        #: by the ASGI server (see ``transport/attach.py``'s module docstring).
        #: Both shapes are reachable here, because both must classify the same.
        self.reject_with = reject_with
        self.greeting = greeting if greeting is not None else [READY_FRAME]
        self.responder = responder
        #: Frames written **immediately after** the reply to a named method,
        #: with no await between them, keyed by method name.
        #:
        #: This is the only way to reproduce the race KTD2's landing barrier
        #: exists for: a gateway that answers ``session.resume`` and then
        #: streams the next event straight down the same socket. A test that
        #: pushed the event with a separate ``send`` call would have yielded to
        #: the loop in between, which is precisely the window the real gateway
        #: does not leave.
        self.follow_ups = dict(follow_ups or {})

        self._server: Server | None = None
        self._port = 0

        #: Query dictionaries as the server saw them, one per upgrade attempt —
        #: including the ones it then rejected. This is the record the
        #: "credential rides the query string" assertions read.
        self.queries: list[dict[str, str]] = []
        #: The raw request paths, so a test can assert on the whole URL.
        self.paths: list[str] = []
        #: Every handshake as the server saw it, one per upgrade attempt: the
        #: request path plus every request header as a ``(name, value)`` pair,
        #: repeats included. A credential sweep reads this, not ``paths``.
        self.handshakes: list[Handshake] = []
        #: Accepted sessions, in order. ``len(sessions)`` is the reconnect count.
        self.sessions: list[Session] = []
        self.rejections = 0
        #: Set when a connection is accepted, so a test can await an attach
        #: without polling.
        self.accepted = asyncio.Event()

    # ── lifecycle ────────────────────────────────────────────────────────

    async def start(self) -> None:
        self._server = await serve(
            self._handle, "127.0.0.1", 0, process_request=self._process_request
        )
        sockets = getattr(self._server, "sockets", None) or []
        self._port = sockets[0].getsockname()[1]

    async def stop(self) -> None:
        server = self._server
        self._server = None
        if server is not None:
            server.close()
            await server.wait_closed()

    @property
    def url(self) -> str:
        """The endpoint, with **no** credential in it."""
        return f"ws://127.0.0.1:{self._port}/api/ws"

    @property
    def current(self) -> Session:
        return self.sessions[-1]

    # ── behaviour under test ─────────────────────────────────────────────

    def _authorized(self, request: Request) -> bool:
        handshake = Handshake(
            path=request.path, headers=list(request.headers.raw_items())
        )
        self.handshakes.append(handshake)
        self.paths.append(handshake.path)
        self.queries.append(handshake.query)
        return not self.require_auth or handshake.query.get("token") == self.token

    def _process_request(
        self, connection: ServerConnection, request: Request
    ) -> Response | None:
        """Refuse a bad credential before the handshake completes.

        This is where the query is read, which is the point: a server that can
        only see ``request.path`` is a server that cannot be satisfied by a
        credential sent any other way. The whole handshake is recorded here as
        well, so a test can assert the credential appears in the query and in no
        header, cookie, or subprotocol.
        """
        if self._authorized(request):
            return None
        self.rejections += 1
        if self.reject_with == "handshake":
            return connection.respond(http.HTTPStatus.UNAUTHORIZED, "unauthorized\n")
        return None

    async def _handle(self, connection: ServerConnection) -> None:
        request = connection.request
        assert request is not None, "the handler runs only after a completed handshake"
        query = dict(parse_qsl(urlsplit(request.path).query, keep_blank_values=True))

        if self.require_auth and query.get("token") != self.token:
            # The post-accept shape of the same refusal: what ``gateway_ws``
            # writes (``hermes_cli/web_server.py:15615-15617``) if the ASGI
            # server has already completed the upgrade.
            await connection.close(code=4401, reason="unauthorized")
            return

        session = Session(query=query, connection=connection)
        self.sessions.append(session)
        self.accepted.set()

        for frame in self.greeting:
            await connection.send(json.dumps(frame))

        try:
            async for raw in connection:
                text = raw if isinstance(raw, str) else raw.decode("utf-8")
                message = json.loads(text)
                session.received.append(message)
                if self.responder is None:
                    continue
                reply = self.responder(message, self)
                if reply is not None:
                    await connection.send(json.dumps(reply))
                for frame in self.follow_ups.get(str(message.get("method", "")), ()):
                    await connection.send(json.dumps(frame))
        except Exception:  # noqa: BLE001 - the stub never fails a test by raising
            return

    async def send(self, frame: dict[str, Any]) -> None:
        """Push one frame down the current connection."""
        await self.current.connection.send(json.dumps(frame))

    async def send_all(self, frames: list[dict[str, Any]]) -> None:
        for frame in frames:
            await self.send(frame)

    async def send_raw(self, text: str) -> None:
        """Push text that may not be JSON at all (the parse-error path)."""
        await self.current.connection.send(text)

    async def hang_up(self, *, code: int = 1001) -> None:
        """Drop the current connection the way a restarting gateway would."""
        self.accepted.clear()
        await self.current.connection.close(code=code, reason="stub hang-up")

    async def wait_for_attach(self, *, timeout: float = 5.0) -> Session:
        await asyncio.wait_for(self.accepted.wait(), timeout=timeout)
        return self.current

    async def wait_for_request(
        self, method: str, *, timeout: float = 5.0
    ) -> dict[str, Any]:
        """Wait until the current session has received a call to ``method``."""

        async def _poll() -> dict[str, Any]:
            while True:
                for message in self.current.received:
                    if message.get("method") == method:
                        return message
                await asyncio.sleep(0.01)

        return await asyncio.wait_for(_poll(), timeout=timeout)


@pytest_asyncio.fixture
async def gateway() -> AsyncIterator[StubGateway]:
    """A started stub gateway, stopped on the way out."""
    stub = StubGateway()
    await stub.start()
    try:
        yield stub
    finally:
        await stub.stop()


@pytest.fixture
def credentials_file(tmp_path: Any) -> Any:
    """A 0600 credential file holding :data:`STUB_TOKEN`."""
    path = tmp_path / "credentials"
    path.write_text(f'token = "{STUB_TOKEN}"\n', encoding="utf-8")
    path.chmod(0o600)
    return path
