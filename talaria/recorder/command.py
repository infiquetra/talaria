"""``talaria record <url>`` — attach to a Hermes gateway and record every frame.

Ported from ``src/record/command.ts``. This is the instrument, not the
product: a recorded corpus is language- and renderer-neutral, so it settles
questions that would otherwise be argued (which events actually arrive and in
what order, whether a Hermes upgrade changed the protocol).

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

from collections.abc import AsyncIterator, Callable
from pathlib import Path
from typing import Protocol

import websockets
import websockets.exceptions

from talaria.recorder.framelog import FrameRecorder, RecorderError, default_log_path
from talaria.recorder.redact import redact_url

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


async def run_record(
    url: str,
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
    reference's exit-code contract (``src/record/command.ts``).
    """
    emit = log if log is not None else print

    if out is not None:
        out_path = out
    elif recordings_dir is not None:
        out_path = default_log_path(recordings_dir)
    else:
        raise ValueError("run_record requires either out= or recordings_dir=")

    emit("talaria record")
    emit(f"  endpoint  {redact_url(url)}")
    emit(f"  writing   {out_path}")
    emit("")

    try:
        recorder = FrameRecorder(out_path, url)
    except RecorderError as exc:
        emit(f"could not start recording: {exc}")
        return 1

    exit_code = 0
    attach_failed = False
    try:
        async with connector(url) as source:
            emit("attached. recording frames, Ctrl-C to stop.")
            async for message in source:
                raw = message if isinstance(message, str) else message.decode("utf-8")
                recorder.record("in", raw)
    except KeyboardInterrupt:
        pass
    except CONNECT_ERRORS as exc:
        attach_failed = True
        emit("")
        emit(f"could not attach: {exc}")
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
        emit("Start the Hermes dashboard, then pass its websocket url:")
        emit("  talaria record 'ws://127.0.0.1:<port>/api/ws?token=<token>'")
        return 1

    return exit_code
