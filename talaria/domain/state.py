"""Session state: one immutable value, one pure reducer, no clock, no I/O.

Every function here takes a :class:`SessionState` and returns a new one. That is
not style — AE2 requires replaying a corpus twice to produce identical state, and
the cheapest way to guarantee it is to have no mutable state to diverge. It also
makes the stateful half of the reconciliation catalogue testable one rule at a
time: a rule is a sequence of three or four frames and an assertion about the
value that comes out.

Where this deliberately diverges from Hermes, the docstring on the rule says so.
The two divergences worth knowing before reading:

*Cancelled is sticky.* Hermes's ``interrupted`` latch suppresses writes and is
cleared by the next ``startMessage()`` (``turnController.ts:989``). Talaria keeps
``turn == "cancelled"`` visible until the next ``message.start`` for the same
reason, so a status payload sampled after a cancelled turn reports ``cancelled``
rather than ``idle`` (R4).

*Sub-agent rows outlive their turn.* Hermes drops them at ``idle()`` and archives
the tree to disk via ``spawn_tree.save``. R17 forbids Talaria from authoring
sub-agent state, so there is no archive to move them into — they stay in view
until the next turn starts, which is also what makes AE14's "late progress after
a terminal state" testable rather than vacuous.
"""

from __future__ import annotations

from collections.abc import Callable, Mapping
from dataclasses import dataclass, field, replace
from typing import Any, Literal

from talaria.domain.decode import (
    DecodedFrame,
    NonEventFrame,
    ProtocolErrorFrame,
    UnknownEventFrame,
)
from talaria.domain.models import (
    ConnectionStatus,
    GatewayEvent,
    PendingPrompt,
    PromptKind,
    SubagentState,
    SubagentStatus,
    TranscriptEntry,
    TranscriptKind,
    TurnPhase,
    Usage,
)
from talaria.domain.normalize import (
    AMBIENT_IGNORED_EVENTS,
    SYSTEM_LINE_EVENTS,
    applies_to_focused_session,
    clip_system_line,
    coerce_text,
    is_terminal_status,
    keep_terminal_else,
    normalize_subagent_status,
    push_unique,
    subagent_identity,
    subagent_sort_key,
)

#: Which prompt kind each request/expire event pair belongs to.
_PROMPT_EVENTS: Mapping[str, PromptKind] = {
    "approval.request": "approval",
    "clarify.request": "clarify",
    "secret.request": "secret",
    "sudo.request": "sudo",
    "terminal.read.request": "terminal_read",
}

_EXPIRE_EVENTS: Mapping[str, PromptKind] = {
    "clarify.expire": "clarify",
    "secret.expire": "secret",
    "sudo.expire": "sudo",
    "terminal.read.expire": "terminal_read",
}


@dataclass(frozen=True)
class SessionState:
    """Everything the UI and the status payload are projections of."""

    focused_session_id: str | None = None
    session_title: str | None = None
    connection: ConnectionStatus = "disconnected"

    turn: TurnPhase = "idle"
    turn_index: int = 0

    #: Text streamed since the last segment flush, not yet committed.
    streaming_text: str = ""
    #: Reasoning accumulated this turn, committed at turn end (R6: never dropped).
    reasoning_text: str = ""
    #: Assistant text already committed this turn, used for final-tail dedupe.
    segments: tuple[str, ...] = ()
    #: Segments below this index were sealed by ``message.interim``.
    interim_boundary: int = 0

    transcript: tuple[TranscriptEntry, ...] = ()
    subagents: tuple[SubagentState, ...] = ()
    prompts: tuple[PendingPrompt, ...] = ()
    usage: Usage = field(default_factory=Usage)

    #: Unknown event types seen, in first-seen order, deduplicated.
    unknown_event_types: tuple[str, ...] = ()
    protocol_error_count: int = 0
    protocol_noise_announced: bool = False
    #: Prompt ids already recorded as abandoned, so the expiry path and the
    #: tool-completion path cannot both write the same trace.
    flushed_prompt_ids: frozenset[str] = frozenset()

    last_status_note: str = ""
    last_observed_at: float = 0.0
    entry_seq: int = 0

    cross_session_events_ignored: int = 0
    late_events_ignored: int = 0
    synthetic_turn_starts: int = 0
    rejected_responses: int = 0

    def prompt_for(self, request_id: str) -> PendingPrompt | None:
        for prompt in self.prompts:
            if prompt.request_id == request_id:
                return prompt
        return None

    def subagent_for(self, identity: str) -> SubagentState | None:
        for row in self.subagents:
            if row.id == identity:
                return row
        return None


# ── Transcript helpers ───────────────────────────────────────────────────


def _append(state: SessionState, kind: TranscriptKind, text: str) -> SessionState:
    entry = TranscriptEntry(
        kind=kind, text=text, turn_index=state.turn_index, seq=state.entry_seq
    )
    return replace(
        state,
        transcript=(*state.transcript, entry),
        entry_seq=state.entry_seq + 1,
    )


def _final_tail(final_text: str, committed: tuple[str, ...]) -> str:
    """Strip text the transcript already shows from the turn's final message.

    Re-encodes ``finalTail`` (``turnController.ts:81-93``). ``message.complete``
    carries the whole assistant reply, but streaming already committed part of
    it; without this the transcript shows the opening paragraphs twice.
    """
    tail = final_text
    for text in committed:
        trimmed = text.strip()
        if trimmed and tail.startswith(trimmed):
            tail = tail[len(trimmed) :].lstrip()
    return tail


# ── Local (non-wire) transitions ─────────────────────────────────────────


def focus_session(state: SessionState, session_id: str | None) -> SessionState:
    """Point the state at a session, clearing anything that belonged to the last.

    Re-encodes ``turnController.reset()`` (``:918-938``) — its comment names the
    failure it prevents, session A's state bleeding into session B. v0.1 has no
    session switcher (R2), so the caller here is reconnect, not a UI control.
    """
    if session_id == state.focused_session_id:
        return state
    return replace(
        state,
        focused_session_id=session_id,
        session_title=None,
        turn="idle",
        streaming_text="",
        reasoning_text="",
        segments=(),
        interim_boundary=0,
        subagents=(),
        prompts=(),
        flushed_prompt_ids=frozenset(),
        last_status_note="",
    )


def set_connection(state: SessionState, status: ConnectionStatus) -> SessionState:
    """Record a transport state change and re-arm the once-per-connection latch."""
    if status == state.connection:
        return state
    return replace(state, connection=status, protocol_noise_announced=False)


def cancel_turn(state: SessionState, *, at: float) -> SessionState:
    """Cancel the in-flight turn, leaving a permanent transcript trace (R4).

    Re-encodes ``interruptTurn`` (``turnController.ts:297-351``), specifically
    its "always surface an interruption indicator" branch at ``:322-331``: when
    partial text exists it is preserved and marked, and when nothing was streamed
    a bare note is written anyway, so the transcript never shows a turn that
    simply stopped.
    """
    if state.turn != "streaming":
        return state

    next_state = state
    if state.reasoning_text.strip():
        next_state = _append(next_state, "reasoning", state.reasoning_text.strip())

    partial = state.streaming_text.lstrip()
    if partial:
        next_state = _append(next_state, "assistant", f"{partial}\n\n*[interrupted]*")
    else:
        next_state = _append(next_state, "cancelled", "*[interrupted]*")

    return replace(
        next_state,
        turn="cancelled",
        streaming_text="",
        reasoning_text="",
        segments=(),
        interim_boundary=0,
        last_observed_at=max(state.last_observed_at, at),
    )


#: What is known about one submitted message's delivery.
#:
#: ``confirmed`` is the gateway's own acknowledgement. The other four are the
#: distinct ways a submit ends without one, and they are kept apart because
#: each earns a *different claim* in the transcript — not for tidiness. Two of
#: them are not even the same kind of claim: ``not_sent`` means nothing was
#: written to any socket, so the message was definitely not delivered, while
#: ``no_reply``, ``connection_lost`` and ``unknown`` all mean the request went
#: out and the answer did not come back, so the message may well have been
#: delivered.
#:
#: The practical consequence, and the reason the distinction is load-bearing:
#: a message that was never sent invites exactly one resend, and a message that
#: may already have been delivered invites none, because resending it makes the
#: agent do the work twice.
DeliveryState = Literal["confirmed", "not_sent", "no_reply", "connection_lost", "unknown"]

#: The transcript line each unconfirmed delivery earns.
#:
#: Every one of these is a statement of fact about what happened, so no entry
#: may name a cause the caller did not actually observe. ``unknown`` is the
#: fallback for a call that resolved without a recognized reason: it says only
#: that no acknowledgement arrived, because that is all that is known.
DELIVERY_NOTES: Mapping[DeliveryState, str] = {
    "not_sent": (
        "not sent — this message was never written to a gateway, so nothing "
        "received it; send it again when the connection is back"
    ),
    "no_reply": (
        "delivery unconfirmed — the message was sent and no reply arrived "
        "before the deadline"
    ),
    "connection_lost": (
        "delivery unconfirmed — the connection dropped before the gateway "
        "acknowledged this message"
    ),
    "unknown": "delivery unconfirmed — the gateway never acknowledged this message",
}


def record_submission(
    state: SessionState, text: str, *, at: float, delivery: DeliveryState
) -> SessionState:
    """Write the operator's own message into the transcript (R3).

    The gateway never echoes a submitted prompt back as an event — there is no
    such type in ``KNOWN_EVENT_TYPES`` — so the operator's own line exists only
    if the client writes it. That is why this is a local transition rather than
    something the reducer derives from a frame.

    ``delivery`` is the whole reason this takes an argument at all. When the
    ``prompt.submit`` call resolved to ``unknown`` (AE8), both of the tidy
    options are dishonest: writing the line plainly claims delivery, and writing
    nothing hides a message the agent may be about to answer. So the line is
    written *and* marked, and the marker is a separate transcript entry rather
    than a suffix on the operator's own words, so nothing rewrites what they
    typed.

    It is a :data:`DeliveryState` rather than a boolean because a boolean forced
    one sentence to cover four different events, and the sentence it carried
    ("the connection dropped") was false for three of them. The worst case was a
    submit attempted with no connection at all: nothing was written to any
    socket, and the transcript nevertheless described the message as possibly
    delivered.
    """
    next_state = _append(state, "user", text)
    note = DELIVERY_NOTES.get(delivery)
    if note is not None:
        next_state = _append(next_state, "system", note)
    return replace(next_state, last_observed_at=max(state.last_observed_at, at))


def record_local_note(state: SessionState, text: str, *, at: float) -> SessionState:
    """Append a Talaria-authored system line (reconnects, unknown outcomes).

    Kept distinct from :func:`_apply_system_line` so the two sources of a system
    entry cannot be confused in review: that one renders text the *gateway*
    sent, this one renders text Talaria wrote about its own transport.
    """
    next_state = _append(state, "system", clip_system_line(text))
    return replace(next_state, last_observed_at=max(state.last_observed_at, at))


def respond_to_prompt(state: SessionState, request_id: str) -> tuple[SessionState, bool]:
    """Answer an outstanding prompt. Returns the new state and whether it took.

    A response for a ``request_id`` with no outstanding prompt is **refused
    here**, before it can reach the socket. The gateway tolerates a late respond
    (``_respond(..., allow_expired=True)`` at ``tui_gateway/server.py:10233-10235``
    answers ``{"status": "expired"}``), but tolerating it is not the same as
    routing it correctly — R8 requires that a late response cannot be attached
    to a different request, and the only place that can be guaranteed is the
    registry that knows which ids are live.
    """
    prompt = state.prompt_for(request_id)
    if prompt is None:
        return replace(state, rejected_responses=state.rejected_responses + 1), False

    remaining = tuple(p for p in state.prompts if p.request_id != request_id)
    return replace(state, prompts=remaining), True


# ── The reducer ──────────────────────────────────────────────────────────


def apply_frame(state: SessionState, decoded: DecodedFrame) -> SessionState:
    """Fold one decoded frame into the session state."""
    state = replace(state, last_observed_at=max(state.last_observed_at, decoded.at))

    if isinstance(decoded, NonEventFrame):
        return state

    if isinstance(decoded, ProtocolErrorFrame):
        return _apply_protocol_error(state, decoded)

    if isinstance(decoded, UnknownEventFrame):
        return _apply_unknown_event(state, decoded)

    return _apply_event(state, decoded)


def apply_frames(state: SessionState, frames: list[DecodedFrame]) -> SessionState:
    for decoded in frames:
        state = apply_frame(state, decoded)
    return state


def _apply_protocol_error(state: SessionState, frame: ProtocolErrorFrame) -> SessionState:
    next_state = replace(state, protocol_error_count=state.protocol_error_count + 1)
    if not next_state.protocol_noise_announced:
        # Re-encodes the ``protocolWarned`` latch
        # (``createGatewayEventHandler.ts:1035-1038``) so a noisy connection
        # announces itself once instead of on every frame. Hermes also renders a
        # 120-character preview of the offending payload at ``:1041-1043``;
        # Talaria drops that half — R5 forbids rendering untrusted raw bytes,
        # and a preview is exactly that.
        next_state = _append(
            replace(next_state, protocol_noise_announced=True),
            "system",
            "protocol noise detected on this connection",
        )
    return _append(next_state, "protocol-error", frame.text)


def _apply_unknown_event(state: SessionState, frame: UnknownEventFrame) -> SessionState:
    seen = state.unknown_event_types
    if frame.type not in seen:
        seen = (*seen, frame.type)
    return _append(replace(state, unknown_event_types=seen), "unknown-event", frame.text)


def _apply_event(state: SessionState, event: GatewayEvent) -> SessionState:
    # Adopt the first session named on the wire. Replay has no session.create
    # response to learn the id from, so without this the cross-talk guard below
    # would have nothing to compare against and would pass everything.
    if (
        state.focused_session_id is None
        and event.session_id is not None
        and not event.type.startswith("gateway.")
    ):
        state = replace(state, focused_session_id=event.session_id)

    if not applies_to_focused_session(event, state.focused_session_id):
        return replace(
            state, cross_session_events_ignored=state.cross_session_events_ignored + 1
        )

    handler = _HANDLERS.get(event.type)
    if handler is not None:
        return handler(state, event)

    if event.type in SYSTEM_LINE_EVENTS:
        return _apply_system_line(state, event)

    if event.type in AMBIENT_IGNORED_EVENTS:
        return state

    return state


# ── Turn lifecycle ───────────────────────────────────────────────────────


def _on_message_start(state: SessionState, event: GatewayEvent) -> SessionState:
    """Start a turn, clearing per-turn state but never the transcript.

    Re-encodes ``startMessage`` (``turnController.ts:980-1006``). This is also
    the only transition that clears a ``cancelled`` turn, which is what makes
    cancellation terminal for the turn it cancelled rather than for the session.
    """
    return replace(
        state,
        turn="streaming",
        turn_index=state.turn_index + 1,
        streaming_text="",
        reasoning_text="",
        segments=(),
        interim_boundary=0,
        subagents=(),
    )


def _ensure_streaming(state: SessionState) -> SessionState:
    """Open a turn for a delta that arrived without a ``message.start``.

    Hermes drops these (``thinking.delta`` returns early when not busy,
    ``createGatewayEventHandler.ts:752-754``). Talaria cannot: R6 says transcript
    content is never dropped, and a missing start is one of the sequences AE2
    names. So the turn is synthesized, counted, and marked in the transcript —
    a deterministic, visible outcome rather than a silent one.
    """
    if state.turn == "streaming":
        return state
    opened = replace(
        state,
        turn="streaming",
        turn_index=state.turn_index + 1,
        streaming_text="",
        reasoning_text="",
        segments=(),
        interim_boundary=0,
        synthetic_turn_starts=state.synthetic_turn_starts + 1,
    )
    return _append(opened, "system", "stream began without a message.start event")


def _on_message_delta(state: SessionState, event: GatewayEvent) -> SessionState:
    """Accumulate a streaming delta. ``rendered`` is never read.

    Re-encodes ``recordMessageDelta`` (``turnController.ts:669-687``) including
    the ``#16391`` finding its comment records: ``payload.rendered`` is an
    *incremental* Rich-ANSI fragment, so a client that assigns it to the buffer
    on every tick discards everything streamed so far. Talaria only ever appends
    ``payload.text``.
    """
    if state.turn == "cancelled":
        return replace(state, late_events_ignored=state.late_events_ignored + 1)

    text = event.payload.get("text")
    if not isinstance(text, str) or not text:
        return state

    opened = _ensure_streaming(state)
    return replace(opened, streaming_text=opened.streaming_text + text)


def _on_message_interim(state: SessionState, event: GatewayEvent) -> SessionState:
    """Seal the provisional assistant message as its own committed segment.

    Re-encodes ``recordInterimMessage`` (``turnController.ts:689-713``): the
    interim text is authoritative even when the stream did not carry every
    token, and the sealed segment is marked so the final message's dedupe pass
    leaves it alone.
    """
    if state.turn == "cancelled":
        return replace(state, late_events_ignored=state.late_events_ignored + 1)

    text = coerce_text(event.payload.get("text"))
    if not text:
        return state

    opened = _ensure_streaming(state)
    committed = _append(opened, "assistant", text)
    return replace(
        committed,
        streaming_text="",
        segments=(*opened.segments, text),
        interim_boundary=len(opened.segments) + 1,
    )


def _on_message_complete(state: SessionState, event: GatewayEvent) -> SessionState:
    """End the turn, or record that a completion arrived for a cancelled one.

    The cancelled branch is R4's whole point: a completion that lands after the
    operator cancelled must not resurrect the turn or overwrite its terminal
    state. Usage still merges, because token accounting describes what the
    provider actually billed and is not a claim about the turn's outcome.
    """
    usage_payload = event.payload.get("usage")
    usage = (
        state.usage.merged_with(usage_payload)
        if isinstance(usage_payload, dict)
        else state.usage
    )

    if state.turn == "cancelled":
        return replace(
            state,
            usage=usage,
            late_events_ignored=state.late_events_ignored + 1,
        )

    final = _resolve_final_text(state, event.payload)

    next_state = state
    if state.reasoning_text.strip():
        next_state = _append(next_state, "reasoning", state.reasoning_text.strip())
    if final:
        next_state = _append(next_state, "assistant", final)

    return replace(
        next_state,
        turn="idle",
        streaming_text="",
        reasoning_text="",
        segments=(),
        interim_boundary=0,
        usage=usage,
    )


def _resolve_final_text(state: SessionState, payload: Mapping[str, Any]) -> str:
    """Pick the turn's final text and remove what the transcript already shows.

    Preference order re-encodes ``recordMessageComplete`` (``:566-572``): raw
    ``text`` beats ``rendered``, because ``rendered`` is Rich-generated ANSI for
    terminals that cannot render markdown and passing it through produces escape
    sequences in the transcript.

    The dedupe window re-encodes ``:576-582``: interim-sealed segments are
    normally preserved even when the final text repeats them, unless
    ``response_previewed`` says the final text *is* the same response that was
    published provisionally — in which case every segment is fair game.
    """
    raw = payload.get("text")
    if not isinstance(raw, str):
        raw = payload.get("rendered")
    if not isinstance(raw, str):
        raw = state.streaming_text

    dedupe_start = 0 if payload.get("response_previewed") is True else state.interim_boundary
    final = _final_tail(raw.lstrip(), state.segments[dedupe_start:])

    if not final and state.streaming_text.strip():
        # The gateway sent an empty final message while text was still buffered.
        # Hermes would drop it; R6 says transcript content is never dropped, so
        # the buffered text is committed instead.
        return state.streaming_text.lstrip()
    return final


def _on_reasoning_delta(state: SessionState, event: GatewayEvent) -> SessionState:
    """Accumulate reasoning. Unlike Hermes, never truncated and never gated.

    Hermes gates reasoning capture on a ``showReasoning`` display setting
    (``turnController.ts:716``, ``:767``) and truncates the buffer at 80,000
    characters down to the last 60,000 (``:778-780``). Talaria does neither: R6
    puts reasoning-block *presentation* out of scope while requiring that its
    content is never dropped, and both of Hermes's behaviours drop content.
    The memory consequence is recorded in the catalogue and queued.
    """
    if state.turn == "cancelled":
        return replace(state, late_events_ignored=state.late_events_ignored + 1)
    text = event.payload.get("text")
    if not isinstance(text, str) or not text:
        return state
    opened = _ensure_streaming(state)
    return replace(opened, reasoning_text=opened.reasoning_text + text)


def _on_reasoning_available(state: SessionState, event: GatewayEvent) -> SessionState:
    """Adopt a whole reasoning block, but only if none was captured yet.

    Re-encodes ``recordReasoningAvailable`` (``turnController.ts:715-731``): the
    gateway can send both a stream of deltas and a complete block, and taking
    the block second would duplicate what the deltas already built.
    """
    if state.turn == "cancelled":
        return replace(state, late_events_ignored=state.late_events_ignored + 1)
    text = coerce_text(event.payload.get("text"))
    if not text or state.reasoning_text.strip():
        return state
    opened = _ensure_streaming(state)
    return replace(opened, reasoning_text=text)


def _on_error(state: SessionState, event: GatewayEvent) -> SessionState:
    """Surface a turn error and settle the turn.

    Re-encodes ``recordError`` (``turnController.ts:545-556``), minus its notice
    flush. A cancelled turn stays cancelled: an error arriving after the
    operator cancelled is not a second, different outcome.
    """
    message = coerce_text(event.payload.get("message")) or "unknown error"
    next_state = _append(state, "error", f"error: {clip_system_line(message)}")
    if state.turn == "cancelled":
        return next_state
    return replace(
        next_state,
        turn="idle",
        streaming_text="",
        reasoning_text="",
        segments=(),
        interim_boundary=0,
    )


# ── Tools ────────────────────────────────────────────────────────────────


def _on_tool_start(state: SessionState, event: GatewayEvent) -> SessionState:
    if state.turn == "cancelled":
        return replace(state, late_events_ignored=state.late_events_ignored + 1)
    name = coerce_text(event.payload.get("name")) or "tool"
    context = coerce_text(event.payload.get("context"))
    line = f"⏺ {name} {context}".rstrip()
    return _append(state, "tool", clip_system_line(line))


def _on_tool_complete(state: SessionState, event: GatewayEvent) -> SessionState:
    """Complete a tool, and flush an abandoned clarify prompt if this was one.

    The flush re-encodes ``flushAbandonedClarify``
    (``createGatewayEventHandler.ts:399-426``, called at ``:1122-1127``). Its
    comment names the bug: when the backend's blocking wait times out and
    returns an empty answer, the prompt overlay is left on screen with no way to
    answer it, and the next turn silently clears it — so the question vanishes
    while the agent's follow-up still refers to it.
    """
    name = coerce_text(event.payload.get("name"))
    next_state = _flush_abandoned_clarify(state) if name == "clarify" else state

    if next_state.turn == "cancelled":
        return replace(next_state, late_events_ignored=next_state.late_events_ignored + 1)

    error = coerce_text(event.payload.get("error"))
    summary = coerce_text(event.payload.get("summary"))
    marker = "✗" if error else "✓"
    detail = error or summary
    line = f"⏺ {name or 'tool'} {marker} {detail}".rstrip()
    next_state = _append(next_state, "tool", clip_system_line(line))

    diff = _strip_diff_chrome(coerce_text(event.payload.get("inline_diff")))
    if diff:
        # Hermes wraps this in a markdown ```diff fence, anchors it between
        # streaming segments, and later drops it if the final reply narrates the
        # same patch (``turnController.ts:477-509``, ``:598-609``). All three are
        # presentation decisions this prototype does not make — but the patch
        # itself is transcript content, and R6 says content is never dropped, so
        # it is committed as plain text.
        next_state = _append(next_state, "tool", diff)
    return next_state


def _strip_diff_chrome(diff_text: str) -> str:
    """Drop the gateway's terminal-printer header from an inline diff.

    Re-encodes ``pushInlineDiffSegment``'s first step
    (``turnController.ts:477-486``): ``_emit_inline_diff`` prefixes a ``┊ review
    diff`` line that only makes sense as stdout dressing.
    """
    if not diff_text:
        return ""
    lines = diff_text.split("\n")
    if lines and lines[0].lstrip().startswith("┊"):
        lines = lines[1:]
    return "\n".join(lines).strip()


def _flush_abandoned_clarify(state: SessionState) -> SessionState:
    """Record an abandoned clarify once, from whichever path notices first.

    The dedupe set re-encodes ``persistedAbandonedClarify``
    (``createGatewayEventHandler.ts:399-402``), which exists because two
    independent paths — the clarify tool's own completion and the end of the
    message — can both notice the same abandonment.
    """
    for prompt in state.prompts:
        if prompt.kind != "clarify" or prompt.request_id in state.flushed_prompt_ids:
            continue
        remaining = tuple(p for p in state.prompts if p.request_id != prompt.request_id)
        flushed = _append(
            replace(
                state,
                prompts=remaining,
                flushed_prompt_ids=state.flushed_prompt_ids | {prompt.request_id},
            ),
            "prompt-expired",
            f"clarify prompt timed out unanswered: {prompt.summary}",
        )
        return flushed
    return state


# ── Blocking prompts ─────────────────────────────────────────────────────


def _on_prompt_request(state: SessionState, event: GatewayEvent) -> SessionState:
    """Register an outstanding human-facing prompt, keyed by ``request_id`` (R8).

    ``approval.request`` carries no ``request_id`` at the pin — its payload is
    ``{description, command, choices, allow_permanent, smart_denied}``
    (``createGatewayEventHandler.ts:1130-1147``) and ``approval.respond``
    resolves by session key instead (``tui_gateway/methods_prompt.py:886-900``).
    R8 nevertheless requires a keyed registry, so approvals get a synthesized
    session-scoped key. There can only be one approval outstanding per session
    on this protocol, so the synthesized key is stable rather than a guess.
    """
    kind = _PROMPT_EVENTS[event.type]
    request_id = coerce_text(event.payload.get("request_id"))
    if not request_id and kind == "approval":
        request_id = f"approval:{state.focused_session_id or 'session'}"
    if not request_id:
        return state
    if state.prompt_for(request_id) is not None:
        return state

    summary = _prompt_summary(kind, event.payload)
    raw_choices = event.payload.get("choices")
    choices = (
        tuple(c for c in raw_choices if isinstance(c, str))
        if isinstance(raw_choices, list)
        else ()
    )
    prompt = PendingPrompt(
        request_id=request_id,
        kind=kind,
        summary=summary,
        opened_at=event.at,
        seq=event.seq,
        choices=choices,
    )
    return _append(
        replace(state, prompts=(*state.prompts, prompt)),
        "prompt",
        f"{kind} prompt awaiting an answer: {summary}",
    )


def _prompt_summary(kind: PromptKind, payload: Mapping[str, Any]) -> str:
    """A one-line description that never carries the answer.

    Only fields the gateway sends *outbound* are read. The credential-bearing
    half of every bridge travels the other way (R9), so there is nothing
    sensitive to include here — but reading only the named fields keeps it that
    way if a payload grows.
    """
    if kind == "clarify":
        return coerce_text(payload.get("question")) or "clarification requested"
    if kind == "approval":
        description = coerce_text(payload.get("description"))
        command = coerce_text(payload.get("command"))
        return description or command or "approval requested"
    if kind == "secret":
        env_var = coerce_text(payload.get("env_var"))
        prompt_text = coerce_text(payload.get("prompt"))
        return prompt_text or (f"secret required for {env_var}" if env_var else "secret required")
    if kind == "sudo":
        return "sudo password required"
    return "terminal read requested"


def _on_prompt_expire(state: SessionState, event: GatewayEvent) -> SessionState:
    """Clear the control on expiry, but leave a permanent transcript trace (R8).

    Re-encodes the id-matched clear at ``createGatewayEventHandler.ts:1174-1182``
    — a stale expiry must not close a newer prompt — and extends it to all four
    bridges. The gateway emits ``.expire`` for ``secret``, ``sudo``, ``clarify``
    and ``terminal.read`` alike (``tui_gateway/server.py:2989-2998``); the
    shipping terminal UI only handles the first two.
    """
    request_id = coerce_text(event.payload.get("request_id"))
    if not request_id:
        return state
    prompt = state.prompt_for(request_id)
    if prompt is None:
        return state
    remaining = tuple(p for p in state.prompts if p.request_id != request_id)
    return _append(
        replace(
            state,
            prompts=remaining,
            flushed_prompt_ids=state.flushed_prompt_ids | {request_id},
        ),
        "prompt-expired",
        f"{prompt.kind} prompt expired unanswered: {prompt.summary}",
    )


# ── Sub-agents ───────────────────────────────────────────────────────────


def _upsert_subagent(
    state: SessionState,
    event: GatewayEvent,
    *,
    proposed_status: SubagentStatus,
    create_if_missing: bool,
    detail_line: str | None = None,
    authoritative_status: SubagentStatus | None = None,
) -> SessionState:
    """The one write path for sub-agent rows.

    Three catalogue rules meet here, and they are separated on purpose:

    * **Update-only for late events.** ``create_if_missing=False`` re-encodes
      ``turnController.ts:1021-1027``, whose comment names the failure — a
      ``subagent.complete``/``tool``/``progress`` arriving after the turn ended
      would otherwise resurrect a finished child.
    * **Terminal states are never overwritten.** ``keep_terminal_else`` applies
      to every event that merely *proposes* a status.
      ``subagent.complete`` passes ``authoritative_status`` instead and is
      allowed to write, because ``:609-612``'s guard exists to stop a stale
      ``start``/``spawn_requested`` clobbering a completion, not to stop the
      completion itself.
    * **Partial payloads preserve prior values.** Every optional field falls back
      to what the row already holds (``:1057-1076``).
    """
    identity = subagent_identity(event.payload)
    existing = state.subagent_for(identity)

    if existing is None and not create_if_missing:
        return replace(state, late_events_ignored=state.late_events_ignored + 1)

    goal = coerce_text(event.payload.get("goal"))
    raw_depth = event.payload.get("depth")
    raw_index = event.payload.get("task_index")
    model = coerce_text(event.payload.get("model")) or None
    parent_id = coerce_text(event.payload.get("parent_id")) or None

    if existing is None:
        row = SubagentState(
            id=identity,
            name=goal or "subagent",
            status=proposed_status if authoritative_status is None else authoritative_status,
            depth=_as_index(raw_depth, 0),
            index=_as_index(raw_index, 0),
            started_at=event.at,
            updated_at=event.at,
            parent_id=parent_id,
            model=model,
            detail=(detail_line,) if detail_line else (),
        )
        rows = (*state.subagents, row)
    else:
        if authoritative_status is not None:
            status: SubagentStatus = authoritative_status
        else:
            status = keep_terminal_else(existing.status, proposed_status)
        row = SubagentState(
            id=identity,
            name=goal or existing.name,
            status=status,
            depth=_as_index(raw_depth, existing.depth),
            index=_as_index(raw_index, existing.index),
            started_at=existing.started_at,
            updated_at=event.at,
            parent_id=parent_id or existing.parent_id,
            model=model or existing.model,
            detail=(
                push_unique(existing.detail, detail_line) if detail_line else existing.detail
            ),
        )
        rows = tuple(row if item.id == identity else item for item in state.subagents)

    ordered = tuple(sorted(rows, key=lambda r: subagent_sort_key(r.depth, r.index, r.id)))
    return replace(state, subagents=ordered)


def _as_index(value: Any, fallback: int) -> int:
    if isinstance(value, bool):
        return fallback
    return value if isinstance(value, int) else fallback


def _on_subagent_spawn_requested(state: SessionState, event: GatewayEvent) -> SessionState:
    return _upsert_subagent(
        state, event, proposed_status="queued", create_if_missing=True
    )


def _on_subagent_start(state: SessionState, event: GatewayEvent) -> SessionState:
    return _upsert_subagent(
        state, event, proposed_status="running", create_if_missing=True
    )


def _on_subagent_thinking(state: SessionState, event: GatewayEvent) -> SessionState:
    text = coerce_text(event.payload.get("text"))
    if not text:
        return state
    return _upsert_subagent(
        state,
        event,
        proposed_status="running",
        create_if_missing=False,
        detail_line=clip_system_line(text),
    )


def _on_subagent_tool(state: SessionState, event: GatewayEvent) -> SessionState:
    name = coerce_text(event.payload.get("tool_name")) or "delegate_task"
    preview = coerce_text(event.payload.get("tool_preview")) or coerce_text(
        event.payload.get("text")
    )
    line = f"{name} {preview}".rstrip()
    return _upsert_subagent(
        state,
        event,
        proposed_status="running",
        create_if_missing=False,
        detail_line=clip_system_line(line),
    )


def _on_subagent_progress(state: SessionState, event: GatewayEvent) -> SessionState:
    text = coerce_text(event.payload.get("text"))
    if not text:
        return state
    return _upsert_subagent(
        state,
        event,
        proposed_status="running",
        create_if_missing=False,
        detail_line=clip_system_line(text),
    )


def _on_subagent_complete(state: SessionState, event: GatewayEvent) -> SessionState:
    status = normalize_subagent_status(event.payload.get("status"), "completed")
    summary = coerce_text(event.payload.get("summary")) or coerce_text(
        event.payload.get("text")
    )
    return _upsert_subagent(
        state,
        event,
        proposed_status=status,
        create_if_missing=False,
        detail_line=clip_system_line(summary) if summary else None,
        authoritative_status=status,
    )


# ── Session, transport, ambient ──────────────────────────────────────────


def _on_session_info(state: SessionState, event: GatewayEvent) -> SessionState:
    title = coerce_text(event.payload.get("title")) or state.session_title
    usage_payload = event.payload.get("usage")
    usage = (
        state.usage.merged_with(usage_payload)
        if isinstance(usage_payload, dict)
        else state.usage
    )
    return replace(state, session_title=title, usage=usage)


def _on_gateway_ready(state: SessionState, event: GatewayEvent) -> SessionState:
    return set_connection(state, "connected")


def _on_gateway_protocol_error(state: SessionState, event: GatewayEvent) -> SessionState:
    """Announce protocol noise once per connection, without the payload preview."""
    if state.protocol_noise_announced:
        return state
    return _append(
        replace(state, protocol_noise_announced=True),
        "system",
        "protocol noise detected on this connection",
    )


def _on_status_update(state: SessionState, event: GatewayEvent) -> SessionState:
    """Record a status note, skipping an immediate repeat.

    Re-encodes the ``lastStatusNote`` guard
    (``createGatewayEventHandler.ts:815-821``): the gateway re-sends the same
    status text on a timer, and without the guard the transcript fills with it.
    """
    text = coerce_text(event.payload.get("text"))
    if not text or text == state.last_status_note:
        return state
    return _append(
        replace(state, last_status_note=text), "system", clip_system_line(text)
    )


def _apply_system_line(state: SessionState, event: GatewayEvent) -> SessionState:
    text = coerce_text(event.payload.get("text")) or coerce_text(
        event.payload.get("message")
    ) or coerce_text(event.payload.get("line"))
    if not text:
        return state
    return _append(state, "system", clip_system_line(text))


def _on_moa_reference(state: SessionState, event: GatewayEvent) -> SessionState:
    """Commit one mixture-of-agents reference model's output.

    Re-encodes ``recordMoaReference`` (``turnController.ts:741-764``) as a plain
    labelled transcript entry. Hermes renders these as thinking-style segments
    regardless of the reasoning display setting, because they *are* the process
    the operator opted into by selecting a mixture-of-agents preset; Talaria
    keeps that reasoning — the content is committed, the styling is not.
    """
    if state.turn == "cancelled":
        return replace(state, late_events_ignored=state.late_events_ignored + 1)
    label = coerce_text(event.payload.get("label")) or "reference"
    text = coerce_text(event.payload.get("text"))
    if not text:
        return state
    return _append(state, "reasoning", f"◇ Reference — {label}\n{text}")


def _ignore(state: SessionState, event: GatewayEvent) -> SessionState:
    """Decoded, understood, and deliberately not rendered.

    ``tool.progress`` and ``tool.generating`` are live in-place previews in
    Hermes (``turnController.ts:877-898`` replaces the active tool's context
    rather than appending). Talaria's transcript is append-only, so replaying
    them would turn a progress spinner into transcript spam.
    """
    return state


_Handler = Callable[[SessionState, GatewayEvent], SessionState]

_HANDLERS: Mapping[str, _Handler] = {
    "message.start": _on_message_start,
    "message.delta": _on_message_delta,
    "message.interim": _on_message_interim,
    "message.complete": _on_message_complete,
    "thinking.delta": _on_reasoning_delta,
    "reasoning.delta": _on_reasoning_delta,
    "reasoning.available": _on_reasoning_available,
    "moa.reference": _on_moa_reference,
    "error": _on_error,
    "tool.start": _on_tool_start,
    "tool.complete": _on_tool_complete,
    "tool.progress": _ignore,
    "tool.generating": _ignore,
    "subagent.spawn_requested": _on_subagent_spawn_requested,
    "subagent.start": _on_subagent_start,
    "subagent.thinking": _on_subagent_thinking,
    "subagent.tool": _on_subagent_tool,
    "subagent.progress": _on_subagent_progress,
    "subagent.complete": _on_subagent_complete,
    "approval.request": _on_prompt_request,
    "clarify.request": _on_prompt_request,
    "secret.request": _on_prompt_request,
    "sudo.request": _on_prompt_request,
    "terminal.read.request": _on_prompt_request,
    "clarify.expire": _on_prompt_expire,
    "secret.expire": _on_prompt_expire,
    "sudo.expire": _on_prompt_expire,
    "terminal.read.expire": _on_prompt_expire,
    "session.info": _on_session_info,
    "gateway.ready": _on_gateway_ready,
    "gateway.protocol_error": _on_gateway_protocol_error,
    "status.update": _on_status_update,
}

#: Every expire event must have a registered handler, else an expiry would
#: silently leave a prompt outstanding. Asserted in the test suite rather than
#: at import time so a failure names the missing type.
EXPIRE_EVENT_KINDS: Mapping[str, PromptKind] = _EXPIRE_EVENTS

__all__ = [
    "DELIVERY_NOTES",
    "EXPIRE_EVENT_KINDS",
    "DeliveryState",
    "SessionState",
    "apply_frame",
    "apply_frames",
    "cancel_turn",
    "focus_session",
    "is_terminal_status",
    "record_local_note",
    "record_submission",
    "respond_to_prompt",
    "set_connection",
]
