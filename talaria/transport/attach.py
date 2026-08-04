"""Dialling the authenticated socket, as an outcome rather than an exception.

Talaria owns its own process lifetime and dials a gateway it did not launch
(ADR-0001). The Hermes side of that relationship is small and pinned: the
dashboard mounts ``/api/ws`` and hands the socket to ``tui_gateway.ws.handle_ws``
(``hermes_cli/web_server.py:15609-15625`` at ``7f4d15515``), the upgrade
credential is read exclusively from ``ws.query_params``
(``:14443-14524``), and the wire protocol above it is newline-delimited JSON-RPC
identical to stdio (``tui_gateway/ws.py`` module docstring).

**The outcome-not-exception shape** is carried over from the TypeScript
reference (``src/transport/attach.ts``): a failure to connect is a state the
interface has to render, not an exception to swallow. R35 makes that explicit by
naming *authentication failed* and *connect failed* as distinct visible states,
so this module classifies rather than propagates.

**Where the credential lives, and where it does not.** The credential is put
into the URL for exactly one call — the dial — and nowhere else.
:class:`AttachTarget` strips any credential-bearing query parameter from the
endpoint the moment it is constructed, so the object every other module holds is
credential-free by construction, and there is no code path in which a caller can
accidentally hand a live token to the recorder, a log line, or a child process.
:func:`AttachTarget.dial_url` is the single place the value is reattached, and
its result is passed to the dialler and immediately dropped.

**The dialler is handed the one credentialed string in the system, so its
exceptions are treated as hostile.** This was wrong for a while, and the comment
that made it wrong said the opposite: that "the dialler is never handed anything
but the URL", as though the URL were the safe value. It is the *credentialed*
URL, and ``websockets`` raises ``InvalidURI`` whose text is that whole URL — so
an ``http://`` scheme typo, or an endpoint with the host left out, put the live
token straight into the operator-facing failure detail, into
``LiveSource.last_failure``, and into the composer notice. Every exception string
that becomes a detail now goes through :func:`scrub_urls`, and :func:`attach`
additionally replaces the literal credential it just used. See
``tests/transport/test_attach.py`` for both shapes.

**A malformed endpoint is an outcome, not an exception.** ``urlsplit`` raises
``ValueError`` on a URL such as ``ws://[bad::/api/ws``, and a bare ``ValueError``
escaping :meth:`AttachTarget.from_url` would be the one failure in this module a
caller had to wrap in ``try``. :class:`AttachTarget` therefore carries a
:attr:`~AttachTarget.problem` string instead, and :func:`attach` turns it into an
ordinary :class:`AttachFailure` without dialling. The same applies to a
credential file that :func:`~talaria.transport.credentials.resolve_endpoint`
refuses: the refusal reaches the operator as a named failure state rather than a
traceback.

**Classifying an authentication failure is genuinely ambiguous at this pin, and
the ambiguity is recorded rather than hidden.** The gateway calls
``await ws.close(code=4401)`` *before* accepting the upgrade. Whether the client
observes that as a WebSocket close with code 4401 or as an HTTP handshake
rejection is decided by the ASGI server, not by Hermes, and this repository has
not measured which one uvicorn produces. So both are classified:
:data:`AUTH_CLOSE_CODES` covers the close-code form and
:data:`AUTH_HTTP_STATUSES` covers the handshake form. ``4403`` — which the same
endpoint uses for "embedded chat disabled" and for a rejected request origin — is
deliberately *not* an authentication failure: telling an operator to check their
token when the feature is switched off would send them to the wrong place.
"""

from __future__ import annotations

import asyncio
import re
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal, Protocol, runtime_checkable
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

from talaria.recorder.redact import REDACTED, URL_ONLY_DENIED_QUERY_KEYS, redact_url
from talaria.transport.credentials import Credential, CredentialError, resolve_endpoint

__all__ = [
    "AUTH_CLOSE_CODES",
    "AUTH_HTTP_STATUSES",
    "CREDENTIAL_QUERY_KEYS",
    "AttachFailure",
    "AttachOutcome",
    "AttachSuccess",
    "AttachTarget",
    "DialFailureKind",
    "Dialer",
    "GatewayConnection",
    "attach",
    "classify_dial_error",
    "scrub_urls",
    "strip_credential_query",
    "websockets_dialer",
]

#: Every query parameter name the Hermes upgrade path treats as a credential
#: (``hermes_cli/web_server.py:14443-14524``). ``token`` is the only one v0.1
#: produces; the other two are stripped anyway, because an endpoint an operator
#: pasted from a browser session may carry them and they must not survive into a
#: recorded header or a child environment.
CREDENTIAL_QUERY_KEYS: frozenset[str] = frozenset({"token"}) | URL_ONLY_DENIED_QUERY_KEYS

#: WebSocket close codes that mean "your credential was not accepted".
#: ``gateway_ws`` closes with 4401 when ``_ws_auth_ok`` is false
#: (``hermes_cli/web_server.py:15615-15617``).
AUTH_CLOSE_CODES: frozenset[int] = frozenset({4401})

#: HTTP handshake statuses that mean the same thing, for the case where the ASGI
#: server renders the pre-accept close as an HTTP rejection.
AUTH_HTTP_STATUSES: frozenset[int] = frozenset({401, 403})

#: Which of R35's two failure states a dial landed in.
DialFailureKind = Literal["auth_failed", "connect_failed"]


@runtime_checkable
class GatewayConnection(Protocol):
    """The three operations the transport needs from a live socket.

    Narrow on purpose, exactly as the frame-source seam is: the wider this gets,
    the more of ``websockets`` a stub has to imitate, and the less a stub proves.
    """

    async def send(self, message: str) -> None: ...

    async def recv(self) -> str | bytes: ...

    async def close(self) -> None: ...


#: Dials a URL and returns a live connection, or raises.
Dialer = Callable[[str], Awaitable[GatewayConnection]]


async def websockets_dialer(url: str) -> GatewayConnection:
    """The default dialler: ``websockets.asyncio.client.connect`` (KTD13).

    Imported inside the function rather than at module import so that nothing in
    the replay path pays for the client library, and so ``talaria.transport``
    stays importable in an environment that has not installed it.
    """
    from websockets.asyncio.client import connect

    return await connect(url)


#: Anything shaped like an absolute URL inside a longer sentence. Deliberately
#: greedy up to whitespace and quoting: ``websockets`` writes the dialled URI
#: bare into its ``InvalidURI`` message, and a pattern that stopped at ``?``
#: would leave the query — which is where the credential is — behind.
_URL_IN_TEXT = re.compile(r"[A-Za-z][A-Za-z0-9+.\-]*://[^\s'\"<>]+")

#: Below this length a string is not a plausible gateway credential, and
#: replacing every occurrence of it would shred the message rather than redact
#: it — a two-character "token" would blank out most of an English sentence.
#: The URL scrub above is what actually covers the reachable leak; the literal
#: replacement is the second layer, so it can afford to be conservative.
_MIN_SCRUBBABLE_CREDENTIAL = 8


def scrub_urls(text: str) -> str:
    """Redact every URL embedded in ``text`` through the recorder's own redactor.

    Used on exception strings before they become anything an operator can see.
    The recorder's :func:`~talaria.recorder.redact.redact_url` is reused rather
    than reimplemented so a URL withheld from the frame log and a URL withheld
    from a failure notice cannot disagree about what counts as a credential.

    A URL that :func:`urllib.parse.urlsplit` cannot read at all is withheld
    whole: an unparseable string is exactly the case where guessing at which
    part is the secret is worst.
    """

    def _replace(match: re.Match[str]) -> str:
        try:
            return redact_url(match.group(0))
        except ValueError:
            return REDACTED

    return _URL_IN_TEXT.sub(_replace, text)


def _scrub_credential(text: str, credential: Credential | None) -> str:
    """Replace a known credential value wherever it survived :func:`scrub_urls`."""
    if credential is None or len(credential.value) < _MIN_SCRUBBABLE_CREDENTIAL:
        return text
    return text.replace(credential.value, REDACTED)


def strip_credential_query(url: str) -> str:
    """Return ``url`` with every credential-bearing query parameter removed.

    Byte-preserving when there is nothing to remove: rebuilding a URL through
    ``urlencode`` normalizes percent-escapes, and a normalized endpoint would
    differ from the one the operator configured for no reason a reader could
    explain.
    """
    parts = urlsplit(url)
    if not parts.query:
        return url
    pairs = parse_qsl(parts.query, keep_blank_values=True)
    kept = [(name, value) for name, value in pairs if name.lower() not in CREDENTIAL_QUERY_KEYS]
    if len(kept) == len(pairs):
        return url
    return urlunsplit(
        (parts.scheme, parts.netloc, parts.path, urlencode(kept), parts.fragment)
    )


@dataclass(frozen=True)
class AttachTarget:
    """A gateway endpoint that provably carries no credential.

    Construct through :meth:`from_url` or :meth:`from_environment`; the
    stripping happens there, so ``url`` is an invariant rather than a habit.

    Neither constructor raises. An endpoint that cannot be parsed, and a
    credential file that cannot be trusted to supply one, both produce a target
    whose :attr:`problem` is set and whose :attr:`url` is empty — which
    :func:`attach` reports as an ordinary failure. A target with a ``problem``
    is never dialled, so the empty ``url`` is unreachable rather than tolerated.
    """

    url: str
    #: Why this endpoint is unusable, or ``""`` when it is usable. Never carries
    #: the offending URL: the string that failed to parse is also the string
    #: that may have had a credential in it.
    problem: str = ""

    @classmethod
    def from_url(cls, url: str) -> AttachTarget:
        try:
            return cls(url=strip_credential_query(url))
        except ValueError as exc:
            return cls(url="", problem=f"the gateway endpoint is not a valid URL: {exc}")

    @classmethod
    def from_environment(
        cls,
        *,
        environ: Any = None,
        credentials_path: Path | None = None,
        override: str | None = None,
    ) -> AttachTarget:
        try:
            return cls(
                url=resolve_endpoint(
                    environ=environ, credentials_path=credentials_path, override=override
                )
            )
        except CredentialError as exc:
            # A refused credential file, or an endpoint that will not parse.
            # ``resolve_endpoint`` never puts a value in its message.
            return cls(url="", problem=str(exc))

    @property
    def safe_url(self) -> str:
        """The endpoint as it may be recorded, logged, or shown.

        Already credential-free, but routed through the recorder's own
        :func:`~talaria.recorder.redact.redact_url` anyway, so a URL carrying
        ``user:password@`` userinfo is withheld too — and so this property and
        the frame log's ``endpoint`` header agree by construction rather than by
        two modules happening to make the same choice.
        """
        try:
            return redact_url(self.url)
        except ValueError:  # pragma: no cover - `url` is empty when unparseable
            return REDACTED

    def dial_url(self, credential: Credential) -> str:
        """The one URL that carries the credential. Never stored, never logged."""
        parts = urlsplit(self.url)
        pairs = [
            (name, value)
            for name, value in parse_qsl(parts.query, keep_blank_values=True)
            if name.lower() not in CREDENTIAL_QUERY_KEYS
        ]
        pairs.append((credential.parameter, credential.value))
        return urlunsplit(
            (parts.scheme, parts.netloc, parts.path, urlencode(pairs), parts.fragment)
        )


@dataclass(frozen=True)
class AttachSuccess:
    """A live connection, and the endpoint in a form that is safe to show.

    Carries the *redacted* endpoint and not the URL that was dialled, so no
    caller downstream can leak a credential it was never handed.
    """

    connection: GatewayConnection
    safe_url: str


@dataclass(frozen=True)
class AttachFailure:
    """Why a dial did not produce a connection (R35's two named states)."""

    kind: DialFailureKind
    detail: str


#: Discriminated by ``isinstance``. A boolean ``ok`` field was considered and
#: dropped: a caller that reads the flag and then reaches for ``.connection``
#: type-checks, while the ``isinstance`` form does not.
AttachOutcome = AttachSuccess | AttachFailure


def _close_code(exc: BaseException) -> int | None:
    """The WebSocket close code an exception carries, if any."""
    for attribute in ("rcvd", "sent"):
        frame = getattr(exc, attribute, None)
        code = getattr(frame, "code", None)
        if isinstance(code, int):
            return code
    code = getattr(exc, "code", None)
    return code if isinstance(code, int) else None


def _http_status(exc: BaseException) -> int | None:
    """The HTTP handshake status an exception carries, if any.

    ``InvalidStatus`` (websockets >= 14) carries ``response.status_code``;
    ``InvalidStatusCode`` (the older name, still exported) carries
    ``status_code`` directly. Both are read because the exception a caller sees
    depends on which the installed library raises, and pinning that would make
    this module fail on an upgrade for no behavioural reason.
    """
    response = getattr(exc, "response", None)
    status = getattr(response, "status_code", None)
    if isinstance(status, int):
        return status
    status = getattr(exc, "status_code", None)
    return status if isinstance(status, int) else None


def classify_dial_error(exc: BaseException) -> DialFailureKind:
    """Sort a dial failure into R35's two visible states.

    The default is ``connect_failed``. That direction is chosen deliberately: an
    unrecognized failure told to the operator as "authentication failed" sends
    them to rotate a credential that was never the problem, while the same
    failure told as "could not connect" sends them to check that the gateway is
    running — which is the more likely cause and the cheaper thing to check.
    """
    code = _close_code(exc)
    if code is not None and code in AUTH_CLOSE_CODES:
        return "auth_failed"
    status = _http_status(exc)
    if status is not None and status in AUTH_HTTP_STATUSES:
        return "auth_failed"
    return "connect_failed"


def describe_dial_error(
    exc: BaseException, safe_url: str, *, credential: Credential | None = None
) -> str:
    """A one-line cause that names the endpoint and never the credential.

    The exception text is included because a ``ConnectionRefusedError`` says
    something the endpoint alone does not — but it is scrubbed first, because
    the one string the dialler is handed is the *credentialed* URL. ``websockets``
    puts that whole URL into ``InvalidURI``'s message, so an ``http://`` scheme
    typo or a missing host used to hand the live token to the operator's screen.
    :func:`scrub_urls` withholds any URL in the text and ``credential``, when the
    caller has it, is replaced literally as a second layer.
    """
    code = _close_code(exc)
    status = _http_status(exc)
    detail = _scrub_credential(scrub_urls(str(exc)), credential) or type(exc).__name__
    if code is not None:
        return f"{safe_url}: closed with code {code} ({detail})"
    if status is not None:
        return f"{safe_url}: handshake rejected with HTTP {status} ({detail})"
    return f"{safe_url}: {detail}"


async def attach(
    target: AttachTarget,
    credential: Credential,
    *,
    dialer: Dialer | None = None,
) -> AttachOutcome:
    """Dial once. Never raises for a connection problem; classifies instead.

    The credential is materialized into a URL here and is unreachable
    afterwards — the returned :class:`AttachSuccess` carries only the redacted
    endpoint, so nothing downstream can leak what it never received.

    ``asyncio.CancelledError`` is re-raised with the other exceptions that are
    not about this connection. Reporting a cancelled dial as
    ``connect_failed: CancelledError`` would tell the operator the gateway was
    unreachable when in fact Talaria itself was being torn down, and — worse —
    would swallow the cancellation, leaving the task that requested teardown
    waiting on a coroutine that decided to return normally instead.
    """
    if target.problem:
        # Never dialled: there is no endpoint to dial. The credential was
        # acquired by the caller and is dropped here unused.
        return AttachFailure(kind="connect_failed", detail=target.problem)

    dial = dialer if dialer is not None else websockets_dialer
    url = target.dial_url(credential)
    try:
        connection = await dial(url)
    except BaseException as exc:  # noqa: BLE001 - classified, never swallowed
        if isinstance(exc, KeyboardInterrupt | SystemExit | MemoryError | asyncio.CancelledError):
            raise
        return AttachFailure(
            kind=classify_dial_error(exc),
            detail=describe_dial_error(exc, target.safe_url, credential=credential),
        )
    return AttachSuccess(connection=connection, safe_url=target.safe_url)
