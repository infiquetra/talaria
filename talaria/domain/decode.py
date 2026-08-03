"""The wire boundary: raw parsed JSON in, typed domain objects out (KTD2).

Two requirements meet here and pull in opposite directions.

R5 wants an unrecognized event type **surfaced by type and never silently
discarded**, and a malformed frame surfaced **as a protocol error without
rendering untrusted raw bytes**. So this module never raises on bad input — it
classifies. Every frame leaves here as one of four things, and three of them are
renderable.

R26 wants the diagnostic itself to withhold the payload. That is why
:class:`ProtocolErrorFrame` carries a member of a closed reason enum rather than
a message built from the frame: there is no string-formatting path in this module
that can interpolate wire content into an error, so no future edit can leak one
by accident.

The fourth outcome, :class:`NonEventFrame`, exists because a recorded corpus is
not all events. RPC responses and the client's own outbound method calls are
well-formed traffic that this layer has no opinion about; classifying them as
errors would fill the transcript with protocol errors on a healthy connection.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any, Literal

from talaria.domain.models import GatewayEvent

# ── Known inbound event types ────────────────────────────────────────────

#: The 45 event types the shipping Hermes terminal UI handles, read from the
#: ``switch`` in ``ui-tui/src/app/createGatewayEventHandler.ts`` at ``7f4d15515``.
#: (``docs/analysis/hermes-gateway-protocol-surface.md`` heads its table "44
#: event types"; the switch carries 45 case labels at the pin. The catalogue
#: records the correction.)
_HANDLED_BY_HERMES_TUI: frozenset[str] = frozenset(
    {
        "approval.request",
        "background.complete",
        "billing.step_up.verification",
        "browser.progress",
        "clarify.request",
        "error",
        "gateway.protocol_error",
        "gateway.ready",
        "gateway.start_timeout",
        "gateway.stderr",
        "message.complete",
        "message.delta",
        "message.interim",
        "message.start",
        "moa.aggregating",
        "moa.phase",
        "moa.progress",
        "moa.reference",
        "notification.clear",
        "notification.show",
        "reaction",
        "reasoning.available",
        "reasoning.delta",
        "review.summary",
        "secret.expire",
        "secret.request",
        "session.info",
        "skin.changed",
        "status.update",
        "subagent.complete",
        "subagent.progress",
        "subagent.spawn_requested",
        "subagent.start",
        "subagent.thinking",
        "subagent.tool",
        "sudo.expire",
        "sudo.request",
        "thinking.delta",
        "tool.complete",
        "tool.generating",
        "tool.progress",
        "tool.start",
        "voice.status",
        "voice.transcript",
        "wake.detected",
    }
)

#: Three event types the gateway emits that the shipping terminal UI does **not**
#: handle. ``terminal.read.request`` is the fourth blocking bridge, raised at
#: ``tui_gateway/server.py:5523-5528``; ``clarify.expire`` and
#: ``terminal.read.expire`` are emitted by the same expiry path that produces the
#: two ``.expire`` events Hermes does handle (``tui_gateway/server.py:2981-2998``
#: names all four). Talaria knows them, so they are ordinary traffic here rather
#: than a stream of unknown-type markers.
_UNHANDLED_BY_HERMES_TUI: frozenset[str] = frozenset(
    {"terminal.read.request", "clarify.expire", "terminal.read.expire"}
)

#: Everything Talaria recognizes. A type outside this set is surfaced by name
#: (R5) rather than dropped — the gateway registers far more methods than any
#: client uses, so unknown events are expected traffic, not a defect.
KNOWN_EVENT_TYPES: frozenset[str] = _HANDLED_BY_HERMES_TUI | _UNHANDLED_BY_HERMES_TUI


# ── Protocol errors ──────────────────────────────────────────────────────

ProtocolErrorReason = Literal[
    "frame-unparseable",
    "frame-not-an-object",
    "frame-not-recognizable",
    "event-params-not-an-object",
    "event-type-missing",
    "event-type-not-a-string",
    "event-payload-not-an-object",
]

#: Fixed sentences, one per reason. No parameter, no interpolation, no path by
#: which a payload fragment can reach a rendered line (R5, R26).
PROTOCOL_ERROR_TEXT: Mapping[ProtocolErrorReason, str] = {
    "frame-unparseable": "protocol error: a frame could not be parsed as JSON (contents withheld)",
    "frame-not-an-object": "protocol error: a frame was not a JSON object (contents withheld)",
    "frame-not-recognizable": (
        "protocol error: a frame was neither an event nor a JSON-RPC message (contents withheld)"
    ),
    "event-params-not-an-object": "protocol error: an event carried no parameters object",
    "event-type-missing": "protocol error: an event carried no type",
    "event-type-not-a-string": "protocol error: an event type was not a string",
    "event-payload-not-an-object": "protocol error: an event payload was not an object",
}


@dataclass(frozen=True)
class ProtocolErrorFrame:
    """A frame that could not be understood. Carries a reason, never content."""

    reason: ProtocolErrorReason
    at: float
    seq: int

    @property
    def text(self) -> str:
        return PROTOCOL_ERROR_TEXT[self.reason]


@dataclass(frozen=True)
class UnknownEventFrame:
    """A well-formed event whose type Talaria does not model (R5)."""

    type: str
    session_id: str | None
    at: float
    seq: int

    @property
    def text(self) -> str:
        return f"unknown event type: {self.type}"


@dataclass(frozen=True)
class NonEventFrame:
    """Well-formed JSON-RPC traffic that is not an event.

    ``kind`` distinguishes a reply the client was waiting for from a method call
    the client sent. Neither changes transcript state, and neither is an error.
    """

    kind: Literal["response", "request"]
    method: str | None
    at: float
    seq: int


DecodedFrame = GatewayEvent | UnknownEventFrame | NonEventFrame | ProtocolErrorFrame


def decode_frame(
    frame: Any,
    *,
    at: float,
    seq: int,
    parse_error: bool = False,
) -> DecodedFrame:
    """Classify one recorded or received frame.

    ``parse_error`` is the frame-log's own signal that the recorder could not
    parse the bytes and therefore withheld them (``frame`` is ``None`` in that
    record). It is passed through as a protocol error rather than being
    mistaken for an empty frame.
    """
    if parse_error:
        return ProtocolErrorFrame(reason="frame-unparseable", at=at, seq=seq)

    if not isinstance(frame, dict):
        return ProtocolErrorFrame(reason="frame-not-an-object", at=at, seq=seq)

    method = frame.get("method")

    if method == "event":
        return _decode_event(frame, at=at, seq=seq)

    if isinstance(method, str):
        return NonEventFrame(kind="request", method=method, at=at, seq=seq)

    if "result" in frame or "error" in frame:
        return NonEventFrame(kind="response", method=None, at=at, seq=seq)

    return ProtocolErrorFrame(reason="frame-not-recognizable", at=at, seq=seq)


def _decode_event(frame: Mapping[str, Any], *, at: float, seq: int) -> DecodedFrame:
    params = frame.get("params")
    if not isinstance(params, dict):
        return ProtocolErrorFrame(reason="event-params-not-an-object", at=at, seq=seq)

    if "type" not in params:
        return ProtocolErrorFrame(reason="event-type-missing", at=at, seq=seq)

    event_type = params["type"]
    if not isinstance(event_type, str):
        return ProtocolErrorFrame(reason="event-type-not-a-string", at=at, seq=seq)

    payload = params.get("payload")
    if payload is None:
        payload = {}
    if not isinstance(payload, dict):
        return ProtocolErrorFrame(reason="event-payload-not-an-object", at=at, seq=seq)

    raw_session = params.get("session_id")
    session_id = raw_session if isinstance(raw_session, str) and raw_session else None

    if event_type not in KNOWN_EVENT_TYPES:
        return UnknownEventFrame(type=event_type, session_id=session_id, at=at, seq=seq)

    return GatewayEvent(
        type=event_type,
        session_id=session_id,
        payload=payload,
        at=at,
        seq=seq,
    )


# ── command.dispatch results (R24) ───────────────────────────────────────

#: The six structured result shapes ``command.dispatch`` returns, from
#: ``ui-tui/src/gatewayTypes.ts:70-75`` and the emitting sites in
#: ``tui_gateway/methods_tools.py`` at ``7f4d15515``.
DispatchResultType = Literal["exec", "plugin", "alias", "skill", "send", "prefill"]

DISPATCH_RESULT_TYPES: frozenset[str] = frozenset(
    {"exec", "plugin", "alias", "skill", "send", "prefill"}
)


@dataclass(frozen=True)
class DispatchResult:
    """One dispatch outcome, handled generically (R24).

    The three text fields separate three different destinations, because
    conflating them is how model-facing scaffolding ends up on screen:

    * ``display_text`` is the only field a transcript may render.
    * ``submit_text`` is sent to the model and never displayed.
    * ``prefill_text`` goes into the composer for the operator to edit.
    """

    type: DispatchResultType
    display_text: str
    submit_text: str | None = None
    prefill_text: str | None = None
    alias_target: str | None = None
    skill_name: str | None = None
    notice: str | None = None


@dataclass(frozen=True)
class UnknownDispatchResult:
    """A seventh shape the gateway grew after this pin. Named, not guessed at."""

    type: str


def decode_dispatch_result(result: Any) -> DispatchResult | UnknownDispatchResult:
    """Decode a ``command.dispatch`` response into a generic result.

    The ``display`` field is preferred over ``message`` wherever the gateway
    supplies one, which is R24's "model-facing skill or bundle scaffolding is
    never rendered". Hermes emits ``display`` alongside ``message`` for exactly
    this reason, with the comment "UIs render this, never ``message``"
    (``tui_gateway/methods_tools.py:538-541`` for a bundle, ``:565-568`` for a
    skill).

    A ``skill`` result that carries no ``display`` falls back to the command
    name, **not** to ``message``: the fallback has to stay safe, because the
    whole point of the field is that ``message`` is the expanded scaffold.
    """
    if not isinstance(result, dict):
        return UnknownDispatchResult(type="")

    raw_type = result.get("type")
    if not isinstance(raw_type, str) or raw_type not in DISPATCH_RESULT_TYPES:
        return UnknownDispatchResult(type=raw_type if isinstance(raw_type, str) else "")

    display = _text(result.get("display"))
    message = _text(result.get("message"))
    notice = _text(result.get("notice")) or None

    if raw_type in {"exec", "plugin"}:
        return DispatchResult(
            type="exec" if raw_type == "exec" else "plugin",
            display_text=_text(result.get("output")),
            notice=notice,
        )

    if raw_type == "alias":
        target = _text(result.get("target"))
        return DispatchResult(
            type="alias",
            display_text=f"alias → {target}" if target else "alias → (unnamed)",
            alias_target=target,
            notice=notice,
        )

    if raw_type == "skill":
        name = _text(result.get("name"))
        return DispatchResult(
            type="skill",
            display_text=display or (f"/{name}" if name else "skill"),
            submit_text=message or None,
            skill_name=name or None,
            notice=notice,
        )

    if raw_type == "send":
        return DispatchResult(
            type="send",
            display_text=display or message,
            submit_text=message or None,
            notice=notice,
        )

    return DispatchResult(
        type="prefill",
        display_text=message,
        prefill_text=message or None,
        notice=notice,
    )


def _text(value: Any) -> str:
    """Strings pass through; everything else becomes empty.

    Deliberately not ``str(value)``: coercing an arbitrary object into the
    display path is how a dict's ``repr`` ends up rendered as if it were text.
    """
    return value if isinstance(value, str) else ""
