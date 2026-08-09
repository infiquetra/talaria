"""Typed protocol and domain models — frozen dataclasses, no validation library.

KTD2 puts the wire boundary here as hand-written frozen dataclasses rather than
Pydantic models. The reason is not weight: it is that a coercing validator would
quietly repair exactly the malformations R5 and R37 require Talaria to *surface*.
A decoder that turns ``{"type": 7}`` into ``"7"`` cannot report a protocol error,
and one that fills a missing field with a default cannot report a missing start.

Nothing in this module reads a clock. Every time value arrives from the frame
record that carried the event, so replaying one corpus twice produces identical
state — the determinism AE2 asks for, which a ``time.time()`` call anywhere below
the transport boundary would silently break.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any, Literal

# ── Wire-level vocabulary ────────────────────────────────────────────────

#: Which way a frame travelled, matching frame-log v1's ``dir`` field.
Direction = Literal["in", "out"]

#: The seven-member sub-agent status enum, re-encoded from Hermes's
#: ``KNOWN_SUBAGENT_STATUSES`` (``ui-tui/src/app/createGatewayEventHandler.ts:364-372``
#: at ``7f4d15515``). Frozen by KTD8: adding a member is a protocol change, not
#: an implementation detail.
SubagentStatus = Literal[
    "completed",
    "error",
    "failed",
    "interrupted",
    "queued",
    "running",
    "timeout",
]

KNOWN_SUBAGENT_STATUSES: frozenset[str] = frozenset(
    {"completed", "error", "failed", "interrupted", "queued", "running", "timeout"}
)

#: The five statuses a later live event may never overwrite
#: (``createGatewayEventHandler.ts:609-610``). Hermes's own comment names the
#: clobber this prevents: "a stale ``subagent.start`` / ``spawn_requested`` can
#: clobber a terminal state from complete (failed/interrupted/timeout/error)".
TERMINAL_SUBAGENT_STATUSES: frozenset[str] = frozenset(
    {"completed", "error", "failed", "interrupted", "timeout"}
)

#: The turn's own lifecycle. ``waiting`` is deliberately absent: waiting on a
#: human is a property of the prompt registry, not of the turn, and deriving it
#: in the projection keeps one source of truth. :data:`TurnStatus` below is the
#: four-value enum KTD5's status payload publishes.
TurnPhase = Literal["idle", "streaming", "cancelled"]

#: KTD5's frozen ``turn`` field. Derived, never stored.
TurnStatus = Literal["idle", "streaming", "waiting", "cancelled"]

#: KTD5's frozen ``connection`` field.
ConnectionStatus = Literal[
    "disconnected", "connecting", "connected", "reconnecting", "auth_failed"
]

#: KTD7's typed end-of-stream cause. Every member names a way the transport
#: knows the stream will not resume on its own, which is exactly the fact the
#: domain needs to decide whether a partial ``streaming_text``/``reasoning_text``
#: buffer is lost content (R6) or merely mid-retry:
#:
#: * ``auth_failed`` — the gateway rejected the credential (on the first dial,
#:   or on a reconnect attempt, or read off a socket the gateway closed with an
#:   authentication close code).
#: * ``dial_failed`` — nothing was dialled (no local credential) or nothing
#:   answered, and this was the attempt after which the source gave up rather
#:   than one it will retry.
#: * ``orderly_close`` — :meth:`~talaria.transport.source.LiveSource.close` ran
#:   its real body, i.e. an explicit teardown (most often the operator quitting
#:   the app) rather than idempotent cleanup after an already-terminal state.
#: * ``reconnect_exhausted`` — every delay in the reconnect schedule was tried
#:   and none produced a connection.
#:
#: Re-declared (not re-used) in ``talaria/transport/source.py`` for the same
#: reason :data:`ConnectionStatus`/``LiveConnectionState`` are: the transport
#: package does not import the domain to name an enum.
#: ``tests/transport/test_source.py`` pins the two spellings identical.
#:
#: A transient status change — ``connecting``, ``connected``, ``reconnecting``,
#: or a reconnect attempt that failed but will retry — carries no cause at all
#: (``None``), which is what lets a reconnect that resumes the same response
#: commit nothing: the segment/interim machinery is the dedupe backstop for
#: content that does arrive, and a cause-less status change never asks the
#: domain to commit or clear anything.
TerminalCause = Literal["auth_failed", "dial_failed", "orderly_close", "reconnect_exhausted"]

#: KTD5's frozen ``mode`` field.
RunMode = Literal["replay", "live"]

#: The five human-facing blocking prompts. ``approval`` is listed alongside the
#: other four even though the gateway does not key it by ``request_id`` — see
#: the reconciliation catalogue's rule on synthesized approval keys.
PromptKind = Literal["approval", "clarify", "secret", "sudo", "terminal_read"]

#: Every transcript entry carries one of these. The set is closed so the
#: plain-text renderer (R6) and the terminal-read buffer (KTD10) agree on what
#: can appear, and so an unknown event cannot invent a new kind at runtime.
TranscriptKind = Literal[
    "user",
    "assistant",
    "reasoning",
    "tool",
    "subagent",
    "system",
    "prompt",
    "prompt-expired",
    "cancelled",
    "error",
    "protocol-error",
    "unknown-event",
]


@dataclass(frozen=True)
class GatewayEvent:
    """One decoded ``{"method": "event", "params": {...}}`` frame.

    ``at`` and ``seq`` come from the frame record, not from a clock: ``at`` is
    epoch seconds parsed from frame-log v1's ISO ``at`` field, and ``seq`` is
    that record's gapless sequence number. Both exist so state transitions are
    reproducible across replays.
    """

    type: str
    session_id: str | None
    payload: Mapping[str, Any]
    at: float
    seq: int


@dataclass(frozen=True)
class Usage:
    """Token accounting, merged field-wise and never replaced wholesale.

    Hermes merges rather than assigns in two places (``session.info`` at
    ``createGatewayEventHandler.ts:743`` and ``message.complete`` at ``:1362``),
    which is what stops a partial usage payload from zeroing a running total.
    """

    input_tokens: int = 0
    output_tokens: int = 0
    observed: bool = False

    def merged_with(self, other: Mapping[str, Any]) -> Usage:
        input_tokens = _as_int(other.get("input_tokens"), self.input_tokens)
        output_tokens = _as_int(other.get("output_tokens"), self.output_tokens)
        return Usage(
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            observed=True,
        )


@dataclass(frozen=True)
class TranscriptEntry:
    """One committed line-group of the conversation.

    ``text`` is already plain text (R6). Nothing downstream re-parses it, and
    nothing upstream may put untrusted raw bytes in it (R5, R26).
    """

    kind: TranscriptKind
    text: str
    turn_index: int
    seq: int


@dataclass(frozen=True)
class SubagentState:
    """Full sub-agent record. :class:`SubagentRow` is the five-field projection."""

    id: str
    name: str
    status: SubagentStatus
    depth: int
    index: int
    started_at: float
    updated_at: float
    parent_id: str | None = None
    model: str | None = None
    detail: tuple[str, ...] = ()

    @property
    def is_terminal(self) -> bool:
        return self.status in TERMINAL_SUBAGENT_STATUSES


@dataclass(frozen=True)
class SubagentRow:
    """KTD8's five-field row: ``{id, name, status, elapsed, detail?}``."""

    id: str
    name: str
    status: SubagentStatus
    elapsed: float
    detail: str | None = None

    @property
    def is_terminal(self) -> bool:
        return self.status in TERMINAL_SUBAGENT_STATUSES


@dataclass(frozen=True)
class PendingPrompt:
    """An outstanding human-facing prompt, keyed by ``request_id`` (R8).

    Four of the fields are kind-specific, and they are plain optional members
    rather than a per-kind hierarchy because the registry's whole value is that
    one lookup answers for all five bridges. ``choices`` belongs to approval and
    clarify; ``command`` belongs to approval alone; ``read_start`` and
    ``read_count`` belong to terminal-read alone.

    ``session_id`` is the second half of R9's correlation clause. The reducer
    already drops an event addressed to a session that is not focused, so a
    prompt cannot be *registered* for the wrong session — but a prompt already
    on screen when the focus moves is a different situation, and an answer must
    not be routed by request id alone. Storing the session the prompt was raised
    for is what turns that from a property of the ordering into something a
    respond can check.
    """

    request_id: str
    kind: PromptKind
    summary: str
    opened_at: float
    seq: int
    choices: tuple[str, ...] = field(default=())
    session_id: str | None = None
    #: approval only: the command the agent is asking permission to run, exactly
    #: as the gateway sent it (redacted there, ``tools/approval.py:3651-3660``).
    #:
    #: It is a **separate field from** ``summary`` rather than folded into it,
    #: and that separation is the whole safety property. The gateway sends both
    #: ``command`` and ``description``, and ``description`` is essentially
    #: always populated — with the *pattern warnings* that triggered the prompt
    #: (``"; ".join(desc for _, desc, _ in warnings)`` at ``:3616``), not with
    #: the command. A one-line summary that prefers ``description`` therefore
    #: shows the operator "recursive delete outside the workspace" and never
    #: shows them ``rm -rf /``. One string cannot carry both, because the
    #: description belongs on one line and the command has to be wrapped whole.
    command: str = ""
    #: terminal-read only: the window the agent asked for. ``None`` means the
    #: agent omitted the argument, which the gateway's contract defines as "the
    #: visible screen" — a different question from "line 0", so the absence is
    #: carried through rather than defaulted here.
    read_start: int | None = None
    read_count: int | None = None


def _as_int(value: Any, fallback: int) -> int:
    """Accept only a real integer; anything else preserves the prior value.

    ``bool`` is rejected explicitly because it is an ``int`` subclass in Python,
    and a ``True`` arriving where a token count belongs is a protocol oddity to
    ignore rather than to record as ``1``.
    """
    if isinstance(value, bool):
        return fallback
    if isinstance(value, int):
        return value
    if isinstance(value, float) and value.is_integer():
        return int(value)
    return fallback
