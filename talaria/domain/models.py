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
    """An outstanding human-facing prompt, keyed by ``request_id`` (R8)."""

    request_id: str
    kind: PromptKind
    summary: str
    opened_at: float
    seq: int
    choices: tuple[str, ...] = field(default=())


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
