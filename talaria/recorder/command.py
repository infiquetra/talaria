"""``talaria record [endpoint]`` — attach to a Hermes gateway and record every frame.

Ported from ``src/record/command.ts``. This is the instrument, not the
product: a recorded corpus is language- and renderer-neutral, so it settles
questions that would otherwise be argued (which events actually arrive and in
what order, whether a Hermes upgrade changed the protocol).

**The credential does not come from the command line, and resolving it is a
separate step from dialling.** This module used to take a URL string and hand it
straight to the connector, which made the operator's only route to authenticating
``talaria record 'ws://…?token=…'`` — a live credential in the process table for
as long as the recording ran, and in the shell history afterwards. R9 forbids
exactly that. :func:`resolve_record_target` now walks the same
:class:`~talaria.transport.credentials.CredentialProvider` chain the launcher
walks and returns a :class:`RecordTarget`; :func:`run_record` takes that resolved
object and never sees an operator-supplied credential. Keeping resolution out of
the dial loop is also what lets the process-surface sweep build a ``record``
entry point that *holds* a live credential without opening a socket.

**Connector injection.** The TypeScript reference dials the socket itself
(``src/transport/attach.ts``). This module takes the connection as an
injected async context manager instead (:data:`Connector`) rather than
hard-wiring ``websockets.connect``, for two reasons that both trace back to
the plan's milestone ordering: ``talaria/transport/`` (KTD3's ``LiveSource``)
is a milestone-2 unit (U7) that does not exist yet, and this module must stay
testable without opening a real socket — an unattended run has no standing
gateway between units (Unattended execution contract). The default connector
(:func:`websockets_connector`) is a thin wrapper over the ``websockets``
package, which is already a pinned runtime dependency (``pyproject.toml``,
U1) for KTD13's milestone-2 transport.
"""

from __future__ import annotations

from collections.abc import AsyncIterator, Callable, Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

import websockets
import websockets.exceptions

from talaria.recorder.framelog import FrameRecorder, RecorderError, default_log_path
from talaria.transport.attach import AttachTarget, describe_dial_error
from talaria.transport.credentials import (
    Credential,
    CredentialError,
    CredentialProvider,
    LoopbackTokenProvider,
    PrimingProvider,
)

__all__ = [
    "CONNECT_ERRORS",
    "Connector",
    "RecordOutcome",
    "RecordTarget",
    "resolve_record_target",
    "run_record",
    "websockets_connector",
]

#: Exceptions that mean "the connection could not be made or dropped
#: abnormally" -- reported as a failed attach rather than propagated.
CONNECT_ERRORS: tuple[type[BaseException], ...] = (
    OSError,
    websockets.exceptions.WebSocketException,
)


class _AsyncMessageSource(Protocol):
    """What a connector's async context manager yields: an async message iterator."""

    def __aiter__(self) -> AsyncIterator[str | bytes]: ...


class Connector(Protocol):
    """Dials ``url`` and returns an async-context-managed message source."""

    def __call__(self, url: str) -> _ConnectionContext: ...


class _ConnectionContext(Protocol):
    async def __aenter__(self) -> _AsyncMessageSource: ...
    async def __aexit__(self, *exc_info: object) -> bool | None: ...


def websockets_connector(url: str) -> _ConnectionContext:
    """Default :data:`Connector`: a thin wrapper over ``websockets.connect``."""
    return websockets.connect(url)  # type: ignore[return-value]


class RecordOutcome:
    """Why an attach attempt ended, mirroring ``AttachOutcome`` in the
    TypeScript reference: reported rather than raised, so a caller
    distinguishes "never connected" from "connected then dropped".
    """

    def __init__(self, *, exit_code: int, detail: str | None = None) -> None:
        self.exit_code = exit_code
        self.detail = detail


@dataclass(frozen=True)
class RecordTarget:
    """Everything one recording needs to dial: an endpoint, and its credential.

    The two halves are kept apart for the same reason
    :class:`~talaria.transport.attach.AttachTarget` keeps them apart. ``target``
    is credential-free by construction, so it is the value that reaches the
    frame-log header and the operator's screen; :meth:`dial_url` is the single
    place the credential is reattached, and its result is handed to the connector
    and dropped.
    """

    target: AttachTarget
    credential: Credential

    @property
    def endpoint(self) -> str:
        """The endpoint with no credential on it — what the frame log records."""
        return self.target.url

    @property
    def safe_url(self) -> str:
        """The endpoint in the form that may be printed (userinfo withheld too)."""
        return self.target.safe_url

    def dial_url(self) -> str:
        """The one URL that carries the credential. Never stored, never logged."""
        return self.target.dial_url(self.credential)


async def resolve_record_target(
    *,
    credentials_path: Path | None = None,
    environ: Mapping[str, str] | None = None,
    override: str | None = None,
    provider: CredentialProvider | None = None,
) -> RecordTarget:
    """Resolve ``record``'s endpoint and credential the way the launcher does (R4).

    ``override`` is an *endpoint*, never a credential: it goes to
    :meth:`~talaria.transport.attach.AttachTarget.from_environment`, the same
    call :func:`talaria.cli.build_live_app` makes, so the two entry points are
    one code path rather than two that have to be kept in agreement. When it is
    ``None`` the endpoint is resolved from the environment and the credential
    file exactly as a bare ``talaria`` launch resolves it.

    The credential comes from :class:`~talaria.transport.credentials.LoopbackTokenProvider`,
    and it is *primed* rather than merely acquired for the reason
    :func:`talaria.cli._prime_credential` gives: priming resolves the chain while
    the terminal is still an ordinary terminal and then seals the interactive
    level, so nothing can raise a hidden password prompt underneath a running
    recording.

    Raises :class:`~talaria.transport.credentials.CredentialError` — for an
    endpoint that will not parse, a credential file this process will not trust,
    or a chain that produced nothing. Nothing it raises carries a value; the
    caller may print it.
    """
    target = AttachTarget.from_environment(
        environ=environ, credentials_path=credentials_path, override=override
    )
    if target.problem:
        raise CredentialError(target.problem)

    resolver: CredentialProvider = (
        provider
        if provider is not None
        # No ``environ``: the provider has no environment injection point since
        # the last environment-borne level left the chain on 2026-08-07. The
        # ``environ`` this function takes reaches the *endpoint* resolution
        # above and nothing else.
        else LoopbackTokenProvider(credentials_path=credentials_path)
    )
    if isinstance(resolver, PrimingProvider):
        credential = await resolver.prime()
    else:
        credential = await resolver.acquire()
    return RecordTarget(target=target, credential=credential)


async def run_record(
    target: RecordTarget,
    *,
    out: Path | None = None,
    recordings_dir: Path | None = None,
    connector: Connector = websockets_connector,
    log: Callable[[str], None] | None = None,
) -> int:
    """Attach, record every frame until the connection ends, and return an exit code.

    Resolves with 0 when the socket closed normally (including operator
    interrupt), 1 when it never attached or a create/write/flush/close
    failure (R25) surfaced from the recorder. Mirrors the TypeScript
    reference's exit-code contract (``src/record/command.ts``). Operator errors
    — a credential on the command line, a credential the chain cannot supply —
    are refused before this function is reached and exit 2 (KTD7), so that
    contract is unchanged by the credential work.

    Takes a resolved :class:`RecordTarget` rather than a URL string. The
    credential is materialized for exactly one call, :meth:`RecordTarget.dial_url`,
    passed to the connector, and dropped; the printed endpoint and the frame-log
    header are both taken from the credential-free half.
    """
    emit = log if log is not None else print

    if out is not None:
        out_path = out
    elif recordings_dir is not None:
        out_path = default_log_path(recordings_dir)
    else:
        raise ValueError("run_record requires either out= or recordings_dir=")

    emit("talaria record")
    emit(f"  endpoint  {target.safe_url}")
    emit(f"  writing   {out_path}")
    emit("")

    try:
        # ``target.endpoint`` and not the dialled URL: the header records an
        # endpoint that has already had every credential query parameter
        # stripped by ``AttachTarget``. The recorder redacts as well, but handing
        # it a credential and trusting the redaction would make the log's safety
        # depend on one boundary instead of two.
        recorder = FrameRecorder(out_path, target.endpoint)
    except RecorderError as exc:
        emit(f"could not start recording: {exc}")
        return 1

    exit_code = 0
    attach_failed = False
    try:
        async with connector(target.dial_url()) as source:
            emit("attached. recording frames, Ctrl-C to stop.")
            async for message in source:
                raw = message if isinstance(message, str) else message.decode("utf-8")
                recorder.record("in", raw)
    except KeyboardInterrupt:
        pass
    except CONNECT_ERRORS as exc:
        attach_failed = True
        emit("")
        # Routed through ``describe_dial_error`` and not interpolated raw. The
        # one string the connector is handed is the *credentialed* URL, and
        # ``websockets`` writes that whole URL into ``InvalidURI``'s message —
        # so an ``http://`` scheme typo or a missing host used to print the live
        # credential to the operator's terminal and into any captured output.
        cause = describe_dial_error(exc, target.safe_url, credential=target.credential)
        emit(f"could not attach: {cause}")
    finally:
        try:
            recorder.close()
        except RecorderError as exc:
            emit(f"could not finish writing recording: {exc}")
            exit_code = 1

    stats = recorder.stats()
    emit("")
    emit(f"  frames recorded   {stats.frames}")
    emit(f"  values withheld   {stats.redactions}")
    if stats.parse_errors > 0:
        emit(f"  unparseable       {stats.parse_errors}")
    emit(f"  corpus            {recorder.file_path}")

    if attach_failed:
        emit("")
        emit("Talaria dials a gateway it did not launch, so one must be running.")
        emit("Start the Hermes dashboard, refresh the credential file, then record:")
        emit("  talaria refresh-credential")
        emit("  talaria record")
        return 1

    return exit_code
