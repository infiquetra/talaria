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
from typing import Any, Final, Literal

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
    clip_detail_line,
    clip_transcript_line,
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
    #: The gateway's live "what are we waiting on" note, replaced in place.
    #:
    #: This is the ``thinking.delta`` channel, and it is chrome rather than
    #: transcript content. Hermes says so itself at ``run_agent.py:1047``: the
    #: ``thinking_callback`` is "bridged to the ``thinking.delta`` event, which
    #: both render as the live spinner/status line". The model's actual thinking
    #: arrives on ``reasoning.delta`` instead, from
    #: ``agent._fire_reasoning_delta`` (``chat_completion_helpers.py:3629-3633``).
    #:
    #: So it is held as one replaceable string rather than accumulated into
    #: :attr:`reasoning_text`, and it never reaches the transcript. Appending it
    #: there was the defect: a live gateway sent ``(◐) indexing...`` on this
    #: channel and the reasoning entry came out as
    #: ``· (◐) indexing...The user wants me to…``.
    thinking_notice: str = ""
    #: Assistant text already committed this turn, used for final-tail dedupe.
    segments: tuple[str, ...] = ()
    #: Segments below this index were sealed by ``message.interim``.
    interim_boundary: int = 0

    transcript: tuple[TranscriptEntry, ...] = ()
    subagents: tuple[SubagentState, ...] = ()
    prompts: tuple[PendingPrompt, ...] = ()
    #: Prompts whose answer is on the wire right now.
    #:
    #: :func:`respond_to_prompt` takes a prompt out of ``prompts`` *before* the
    #: call goes out, so one question cannot collect two answers while the first
    #: is in flight. It parks it here rather than dropping it, because the
    #: gateway can still speak about a request during that window: a
    #: ``<bridge>.expire`` that arrives while the answer is travelling used to
    #: find nothing, write no marker, and record no flushed id — after which an
    #: answer that reached no socket put the control back on screen for a bridge
    #: the gateway had already closed, permanently, with no second expiry ever
    #: coming. Parking the prompt is what gives :func:`_on_prompt_expire`
    #: something to name and what makes ``flushed_prompt_ids`` reachable in the
    #: case :func:`restore_prompt` documents.
    answering: tuple[PendingPrompt, ...] = ()
    usage: Usage = field(default_factory=Usage)

    #: Unknown event types seen, in first-seen order, deduplicated.
    unknown_event_types: tuple[str, ...] = ()
    protocol_error_count: int = 0
    protocol_noise_announced: bool = False
    #: Prompt ids already recorded as abandoned, so the expiry path and the
    #: tool-completion path cannot both write the same trace.
    flushed_prompt_ids: frozenset[str] = frozenset()

    #: Approvals :func:`age_out_approvals` withdrew whose fate is still unknown.
    #:
    #: **This exists because a withdrawal removes the evidence that the session
    #: was blocked.** ``turn_status`` reports ``waiting`` only while ``prompts``
    #: is non-empty, so the instant an approval ages out the turn falls back to
    #: ``streaming`` and the screen says ``working…`` — about a session whose
    #: agent Talaria has no reason to believe resumed. Under the gateway's
    #: default 300-second wait it very likely did resume, because approval fails
    #: closed and returns (``tools/approval.py:4050`` returns ``"outcome":
    #: "timeout"``). Under a deployment that raised that timeout above Talaria's
    #: own :data:`APPROVAL_STALE_AFTER`, it did not: the gateway is still
    #: holding, and ``working…`` describes a session that will never move.
    #:
    #: Talaria cannot tell those two apart, and the honest state is neither
    #: ``waiting`` nor ``working`` but "withdrawn, and what happens next is
    #: unknown". That state is carried here rather than as a fifth ``turn``
    #: value because KTD5 freezes the v1 status contract at four
    #: (``docs/formats/status-line.md``); it is spent on the screen instead, by
    #: :func:`~talaria.ui.prompts.activity_line`.
    withdrawn_approvals: int = 0

    last_status_note: str = ""
    last_observed_at: float = 0.0
    entry_seq: int = 0

    cross_session_events_ignored: int = 0
    late_events_ignored: int = 0
    synthetic_turn_starts: int = 0
    rejected_responses: int = 0
    #: Prompt requests whose ``request_id`` was already outstanding. That is the
    #: gateway re-announcing a live prompt across a reconnect (F6) and keeping
    #: the first record is correct — but it used to be the *same line* that
    #: silently discarded a second approval, so the two are now separated and
    #: this one is counted rather than invisible.
    duplicate_prompts_ignored: int = 0
    #: How many ``approval.request`` events this session has raised. Approval
    #: carries no request id on the wire, so this counter is what makes each
    #: arrival a distinct registry entry instead of the second one colliding
    #: with the first and being thrown away.
    approvals_seen: int = 0

    def prompt_for(self, request_id: str) -> PendingPrompt | None:
        for prompt in self.prompts:
            if prompt.request_id == request_id:
                return prompt
        return None

    def answering_for(self, request_id: str) -> PendingPrompt | None:
        """The prompt with an answer in flight under this id, if any."""
        for prompt in self.answering:
            if prompt.request_id == request_id:
                return prompt
        return None

    def outstanding_approvals(self, session_id: str | None) -> tuple[PendingPrompt, ...]:
        """Every approval the gateway may still be holding for one session.

        **``answering`` is searched as well as ``prompts``, and that is the
        whole point of this method.** "Outstanding" is a statement about the
        *gateway's* queue, not about Talaria's screen, and an approval whose
        answer is in flight has not left that queue yet — the reply that would
        say so has not arrived. Reading ``prompts`` alone made the one approval
        the operator had *just answered* invisible to the rule whose entire job
        is to stop a second one being answered: :func:`respond_to_prompt` moves
        a prompt out of ``prompts`` before the call goes out, so for the length
        of one round trip the count was one short. A second ``approval.request``
        arriving inside that window was marked answerable, offered its
        affirmative buttons, and answered — putting two ``approval.respond``
        calls in flight against a resolver that pops the FIFO head with no
        discriminator. Which command each one released was then decided by
        arrival order.

        Ordered by ``seq``, the frame sequence the prompt arrived on, because
        that is the order the gateway enqueued them and therefore the order its
        resolver pops them in (``tools/approval.py:2214-2222``: oldest first,
        FIFO). Concatenating the two tuples would not give that order —
        ``answering`` holds whatever was answered most recently, which is
        routinely *older* than what is still on screen.
        """
        return tuple(
            sorted(
                (
                    p
                    for p in (*self.prompts, *self.answering)
                    if p.kind == "approval"
                    and (session_id is None or p.session_id == session_id)
                ),
                key=lambda p: (p.seq, p.request_id),
            )
        )

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
        thinking_notice="",
        segments=(),
        interim_boundary=0,
        subagents=(),
        prompts=(),
        answering=(),
        approvals_seen=0,
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
        thinking_notice="",
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


#: The gateway method a composed message is sent with
#: (``tui_gateway/methods_prompt.py:67`` at ``7f4d15515``: ``params`` are
#: ``session_id`` and ``text``).
#:
#: It lives in the domain because both ends of it are domain concerns: the
#: composer's dispatcher writes this frame, and :func:`replayed_submission_text`
#: reads the operator's words back out of a recorded one. ``talaria.ui.app``
#: imports it under the same name, which is where its callers reach for it.
SUBMIT_METHOD: Final[str] = "prompt.submit"


def replayed_submission_text(frame: Any) -> str | None:
    """The operator's words, recovered from a recorded outbound ``prompt.submit``.

    Returns ``None`` for every other frame, so a caller can hand this each
    outbound record without first knowing what it is.

    **Why this can be read at all.** ``record_submission`` exists because the
    gateway never echoes a submitted prompt back as an event, so the operator's
    line is written locally and appears in no inbound frame. A recording of a
    live session does hold it — in the *outbound* half of the frame log, which
    the replay path discards. That is why a replay of a real session used to
    rebuild the agent's side of a conversation and not the question it answered.

    Kept in the domain, and kept to extraction only, so the shape of the frame
    is asserted framework-free and the decision about *when* to apply it stays
    with the caller that knows which mode it is in.
    """
    if not isinstance(frame, Mapping) or frame.get("method") != SUBMIT_METHOD:
        return None
    params = frame.get("params")
    if not isinstance(params, Mapping):
        return None
    text = params.get("text")
    return text if isinstance(text, str) and text else None


def record_replayed_submission(state: SessionState, text: str, *, at: float) -> SessionState:
    """Write a replayed operator line, claiming nothing about its delivery.

    Deliberately not :func:`record_submission` with a ``delivery`` value. That
    argument exists so the transcript can state what was *observed* about a live
    call's outcome, and a replay observed nothing: it is reading a frame that
    was written to a socket some time ago, and the acknowledgement — if one came
    — is a later frame this has not reached yet. Passing ``confirmed`` here would
    put a claim in the transcript that no code checked.

    The delivery *notes* a live run wrote are absent from a replay for the same
    reason the operator's line was: they are locally authored and never crossed
    the wire. That is a real gap in what a frame log can reconstruct, and it is
    recorded as one rather than papered over with a guess.
    """
    return replace(
        _append(state, "user", text), last_observed_at=max(state.last_observed_at, at)
    )


def record_local_note(state: SessionState, text: str, *, at: float) -> SessionState:
    """Append a Talaria-authored system line (reconnects, unknown outcomes).

    Kept distinct from :func:`_apply_system_line` so the two sources of a system
    entry cannot be confused in review: that one renders text the *gateway*
    sent, this one renders text Talaria wrote about its own transport.
    """
    next_state = _append(state, "system", clip_transcript_line(text))
    return replace(next_state, last_observed_at=max(state.last_observed_at, at))


def record_command_result(state: SessionState, text: str, *, at: float) -> SessionState:
    """Append what a slash command displayed (U9, R24).

    Separate from :func:`record_local_note` for one measurable reason: that one
    clips at :data:`~talaria.domain.normalize.TRANSCRIPT_LINE_CLIP`, which is a
    backstop against a runaway line and wrong as a bound on the output of
    ``/status`` or ``/context``. A command's output is the thing the operator
    asked to see, so it is bounded once, by the caller, at the far looser
    :data:`~talaria.domain.commands.COMMAND_OUTPUT_CLIP`.

    The caller has already bounded the text at
    :data:`~talaria.domain.commands.COMMAND_OUTPUT_CLIP`, so nothing is clipped
    a second time here — a second clip would cut a marked truncation in half
    and leave the ellipsis stranded mid-line.
    """
    next_state = _append(state, "system", text)
    return replace(next_state, last_observed_at=max(state.last_observed_at, at))


#: What labels the command in every durable record of an approval.
#:
#: One constant for the transcript's arrival entry and its answered entry, so a
#: reader searching a saved transcript for what was approved has one string to
#: search for rather than two that drifted apart.
APPROVAL_COMMAND_LABEL: Final[str] = "command: "

#: The answer was typed into a control the registry no longer holds.
REFUSED_NOT_OUTSTANDING: Final[str] = (
    "that prompt is no longer waiting for an answer — nothing was sent"
)

#: The answer belongs to a session that is no longer the focused one.
REFUSED_WRONG_SESSION: Final[str] = (
    "that prompt belongs to a session Talaria is no longer showing — nothing was sent"
)

#: Why a queued approval carries no answer control. Shown on the card by the
#: projection and repeated by the refusal below, from this one string, so the
#: screen and the registry cannot come to say different things.
UNCORRELATED_APPROVAL: Final[str] = (
    "more than one approval is waiting and this gateway sends no request id "
    "with an approval, so an answer cannot be aimed at one of them"
)

#: What the operator is told if an answer is attempted anyway.
REFUSED_UNCORRELATED_APPROVAL: Final[str] = (
    f"{UNCORRELATED_APPROVAL} — nothing was sent; deny them all, or let them expire"
)


def respond_to_prompt(
    state: SessionState, request_id: str, *, session_id: str | None = None
) -> tuple[SessionState, str | None]:
    """Answer an outstanding prompt. Returns the new state and any refusal.

    The second element is ``None`` when the answer may be sent, and otherwise
    the operator-facing reason it may not. It is a reason rather than a boolean
    because the three refusals below want three different sentences on screen,
    and a caller that has to re-derive which one fired is a caller that will
    eventually derive the wrong one.

    A response for a ``request_id`` with no outstanding prompt is **refused
    here**, before it can reach the socket. The gateway tolerates a late respond
    (``_respond(..., allow_expired=True)`` at ``tui_gateway/server.py:10228-10235``
    answers ``{"status": "expired"}``), but tolerating it is not the same as
    routing it correctly — R8 requires that a late response cannot be attached
    to a different request, and the only place that can be guaranteed is the
    registry that knows which ids are live.

    ``session_id`` is the other half of R9's correlation clause, and it is
    checked here for the same reason: a caller comparing the two itself would be
    one caller's discipline, while the registry is the one place that knows what
    session each live id belongs to. Passing ``None`` skips the check, which is
    what a caller with no session context (a replay-mode test, the domain suite)
    actually means — it is not an assertion that any session matches.

    **The third refusal is about approval alone, and it is a safety rule rather
    than a race.** ``approval.request`` carries no request id
    (``tui_gateway/server.py:1655-1674``) and ``approval.respond`` takes no
    discriminator: it pops the *oldest* entry in the session's queue
    (``tools/approval.py:2214-2222``). While exactly one approval is
    outstanding, that is unambiguous — the answer lands on that approval, or on
    an empty queue, and the reply's own ``resolved`` count says which. The
    moment a second approval is outstanding it stops being unambiguous, because
    the gateway also removes an entry on timeout and on interrupt **without
    emitting anything** (``tools/approval.py:3336-3344``), so the queue head and
    the oldest card on screen can differ. Answering then approves a command the
    operator was never shown. So Talaria refuses, and the interface offers the
    one answer that needs no correlation instead: deny every queued approval at
    once.

    Every refusal increments ``rejected_responses`` rather than raising. A
    refused response is an ordinary race, not a fault.
    """
    prompt = state.prompt_for(request_id)
    if prompt is None:
        return _refuse(state), REFUSED_NOT_OUTSTANDING
    if session_id is not None and prompt.session_id is not None and prompt.session_id != session_id:
        return _refuse(state), REFUSED_WRONG_SESSION
    if prompt.kind == "approval" and len(state.outstanding_approvals(prompt.session_id)) > 1:
        return _refuse(state), REFUSED_UNCORRELATED_APPROVAL

    return _start_answering(state, (prompt,)), None


@dataclass(frozen=True)
class DenyAllScope:
    """What one ``all: true`` denial covers, split by what may be claimed of it.

    Two groups, because two different questions have two different answers and
    reporting one number for both is how deny-all first came to under-count and
    then came to over-claim.

    ``taken`` is what *this* call moved out of the registry. It is exactly what
    this call may put back (``not_sent``) or settle, and it is the only group
    whose fate this call decides: nothing else in the session had an answer
    travelling for it, so the ``all`` denial is the only respond that reaches
    it.

    ``already_in_flight`` is the approvals a *different* call is still waiting
    on. The gateway's ``all`` reaches them too —
    ``resolve_gateway_approval(..., resolve_all=True)`` takes ``list(queue)``
    and applies the choice to every entry — so they must be *named*: an
    operator told "2 denied" when the denial swept three is being misled about
    a safety action. But they must not be named as **denied**, which is the
    claim the old ``total`` made. Each of them has its own ``approval.respond``
    on the wire, and that respond can carry an affirmative the operator pressed
    a moment earlier. Which one the gateway applies is decided by arrival order
    there, which Talaria neither knows nor waits for. Summing the two produced
    a transcript holding two different fates for one command: ``denied every
    waiting approval: 2 waiting`` beside ``approval answered: once · command:
    rm -rf /``.

    So the counts are reported separately and the second one is reported as
    undecided. That also bounds repeated presses, which the sum did not: a
    second deny-all inside the first one's round trip re-counted every approval
    the first had already claimed, so three approvals and two presses reported
    five denials. ``denied`` only ever counts prompts this call removed from the
    registry, and a press that removes none is refused — so the denials claimed
    across a session can never exceed the approvals that arrived in it.
    """

    taken: tuple[PendingPrompt, ...] = ()
    already_in_flight: tuple[PendingPrompt, ...] = ()

    @property
    def denied(self) -> int:
        """How many approvals **this call** took off the screen and denied."""
        return len(self.taken)

    @property
    def undecided(self) -> int:
        """How many the ``all`` flag also reaches whose outcome cannot be named."""
        return len(self.already_in_flight)


def respond_to_all_approvals(
    state: SessionState, *, session_id: str | None
) -> tuple[SessionState, DenyAllScope]:
    """Take every answerable approval in one session, for the answer that needs
    no aim — and report every approval the gateway will resolve.

    ``approval.respond`` accepts ``all: true``, which routes to
    ``resolve_gateway_approval(..., resolve_all=True)`` and applies one choice
    to every entry in the session's queue (``tools/approval.py:2219-2226``). One
    choice for every entry needs no correlation at all, which is what makes it
    the only safe action while more than one approval is outstanding — and why
    the interface only ever offers it as *deny*: an affirmative applied to a
    command nobody has read is the defect this whole rule exists to prevent.

    An approval whose own answer is already travelling is **named but not
    taken, and not claimed as denied**. Named, because the gateway's ``all``
    reaches it and an operator told "2 denied" when the denial swept three is
    being misled about a safety action. Not taken, because the call that owns
    it will settle or restore it when its reply lands, and a second owner would
    either resurrect a control whose answer is in flight or settle one twice.
    Not claimed as denied, because its own respond may carry an affirmative —
    see :class:`DenyAllScope`.

    Refused when nothing is answerable: deny-all is only ever reached from a
    mounted card, so an empty ``taken`` means the queue emptied underneath the
    operator between the press and the dispatch.
    """
    outstanding = state.outstanding_approvals(session_id)
    live = {p.request_id for p in state.prompts}
    taken = tuple(p for p in outstanding if p.request_id in live)
    if not taken:
        return _refuse(state), DenyAllScope()
    scope = DenyAllScope(
        taken=taken,
        already_in_flight=tuple(p for p in outstanding if p.request_id not in live),
    )
    return _start_answering(state, taken), scope


def _refuse(state: SessionState) -> SessionState:
    return replace(state, rejected_responses=state.rejected_responses + 1)


def _start_answering(
    state: SessionState, prompts: tuple[PendingPrompt, ...]
) -> SessionState:
    """Move prompts out of the registry and into the in-flight set."""
    taken = {p.request_id for p in prompts}
    return replace(
        state,
        prompts=tuple(p for p in state.prompts if p.request_id not in taken),
        answering=(*state.answering, *prompts),
    )


def settle_prompt(state: SessionState, request_id: str) -> SessionState:
    """Drop an in-flight answer once its outcome is known. Idempotent.

    Called for every terminal outcome, including the ones that put the control
    back — an id left in ``answering`` forever would let a much later expiry
    write a marker for a question that was answered minutes ago.
    """
    if state.answering_for(request_id) is None:
        return state
    return replace(
        state, answering=tuple(p for p in state.answering if p.request_id != request_id)
    )


def restore_prompt(state: SessionState, prompt: PendingPrompt) -> SessionState:
    """Put a prompt back after an answer that reached no socket at all.

    :func:`respond_to_prompt` clears the prompt *before* the call goes out, so
    one question cannot collect two answers while the first is in flight — for
    a secret or a sudo password that is the worst retry available. The cost is
    that a call which failed is a question the operator can no longer answer, so
    the one outcome that is *definite* about non-delivery — nothing was written
    to any socket — puts the control back.

    Two conditions refuse the restore, and both are races rather than faults. A
    request id already in ``flushed_prompt_ids`` expired while the call was out,
    so restoring it would put a control on screen that the gateway has stopped
    listening to. An id already outstanding means the gateway re-announced the
    prompt across a reconnect, and the announced one is the live record.

    The first of those guards used to be unreachable, which is worth stating
    because it looked correct and was load-bearing. Both writers of
    ``flushed_prompt_ids`` require the prompt to still be *outstanding*, and by
    construction it is not — this function is only ever called about a prompt
    ``respond_to_prompt`` already removed. An expiry landing in that window
    therefore wrote nothing at all and the control came back for a bridge the
    gateway had closed, forever, because no second expiry is ever emitted. The
    ``answering`` set is what closed that hole; see its field docstring.

    No transcript entry is written. The ``prompt`` line was appended when the
    request first arrived and the transcript is append-only, so a second one
    would show the agent asking twice.
    """
    state = settle_prompt(state, prompt.request_id)
    if prompt.request_id in state.flushed_prompt_ids:
        return state
    if state.prompt_for(prompt.request_id) is not None:
        return state
    return replace(state, prompts=(*state.prompts, prompt))


#: How long an approval may sit unanswered before Talaria stops offering it.
#:
#: **Approval is the one bridge the gateway never announces a timeout for.**
#: ``<bridge>.expire`` is emitted for ``secret``, ``sudo``, ``clarify`` and
#: ``terminal.read`` and for nothing else (``tui_gateway/server.py:2981-2998``
#: at Hermes ``7f4d15515``); ``tools/approval.py`` drops its own entry on
#: timeout and on interrupt through ``_drop_entry()`` with no emit at all. So
#: :data:`_EXPIRE_EVENTS` correctly has no ``approval.expire`` — and nothing
#: else aged an approval out either, which left a card on screen for a question
#: the gateway had stopped holding. That card is not merely stale: a second,
#: genuine approval arriving beside it is marked unanswerable, because the rule
#: that counts outstanding approvals counts the phantom too. The operator then
#: cannot allow the command they actually want to allow, and the only offered
#: action denies it.
#:
#: 300 seconds is the gateway's own default (``_get_approval_timeout()``,
#: ``tools/approval.py:2648-2657``). It is **configurable there**, so this is
#: not a deadline Talaria knows — it is the only number Talaria has any grounds
#: for, and what the operator is told when it passes says exactly that. What is
#: not in doubt is the direction of failure: the gateway fails closed
#: (``"Silence is not consent."``, ``:2976``, recorded as ``"outcome":
#: "timeout"`` at ``:4050``), so an approval Talaria stops offering is one the
#: gateway has most likely already refused rather than one it might still grant.
APPROVAL_STALE_AFTER: Final[float] = 300.0

#: What the transcript says when an approval is withdrawn locally.
#:
#: Every clause is something Talaria observed or can cite, and the sentence
#: stops before the one thing it cannot know. "nothing was sent" is a fact
#: about this process. "the gateway's default wait is 5 minutes" is a fact
#: about the pinned source. "has probably stopped waiting" is a hedge and is
#: written as one. What is deliberately absent is the word *denied*: the
#: gateway fails closed so a denial is the likely outcome, but no reply said so
#: and the timeout is configurable, so claiming it would be inventing an
#: acknowledgement.
APPROVAL_AGED_OUT: Final[str] = (
    "approval no longer offered — nothing was sent; the gateway's default wait "
    "is 5 minutes and it announces no approval timeout, so it has probably "
    "stopped waiting"
)


def age_out_approvals(state: SessionState, *, now: float) -> SessionState:
    """Withdraw approvals older than :data:`APPROVAL_STALE_AFTER`. Pure.

    Only approvals, because only approval lacks a gateway ``.expire`` — aging
    the other four out locally would race the event that is actually coming and
    write a second, differently-worded marker for the same timeout.

    Only prompts still in ``prompts``: an approval in ``answering`` has a call
    of its own outstanding, and that call has a bounded timeout that will settle
    or restore it. Two owners for one entry is the bookkeeping defect this
    module already carries two comments about.

    ``now`` is passed in rather than read, for the reason every clock in this
    package is: the caller knows which clock its ``opened_at`` came from. Live
    frames are stamped with the wall clock and replayed frames with the
    recorded one, and mixing them would age out a whole recorded corpus on the
    first tick. A non-positive ``now`` or ``opened_at`` disables the check
    entirely, which is the honest reading of "this record has no usable time"
    — a corpus whose timestamps did not parse must not be treated as ancient.

    The withdrawal is recorded in ``flushed_prompt_ids`` for the same reason an
    expiry is: it is the latch that stops a late ``restore_prompt`` putting the
    control back after Talaria has told the operator it is gone.
    """
    if now <= 0.0:
        return state
    stale = tuple(
        p
        for p in state.prompts
        if p.kind == "approval" and p.opened_at > 0.0 and now - p.opened_at >= APPROVAL_STALE_AFTER
    )
    if not stale:
        return state
    dropped = {p.request_id for p in stale}
    next_state = replace(
        state,
        prompts=tuple(p for p in state.prompts if p.request_id not in dropped),
        flushed_prompt_ids=state.flushed_prompt_ids | dropped,
        # Counted, not flagged: two approvals can age out on one tick, and the
        # screen names how many were withdrawn rather than saying "an approval"
        # about a number it knows.
        withdrawn_approvals=state.withdrawn_approvals + len(stale),
    )
    for prompt in stale:
        line = f"{APPROVAL_AGED_OUT}: {prompt.summary}"
        if prompt.command:
            line = f"{line}\n{APPROVAL_COMMAND_LABEL}{prompt.command}"
        next_state = _append(next_state, "prompt-expired", line)
    return next_state


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
        return _clear_withdrawal_on_progress(state, handler(state, event))

    if event.type in SYSTEM_LINE_EVENTS:
        return _apply_system_line(state, event)

    if event.type in AMBIENT_IGNORED_EVENTS:
        return state

    return state


def _turn_progress(state: SessionState) -> tuple[Any, ...]:
    """The fields that move when the agent is doing something."""
    return (state.turn, state.turn_index, state.streaming_text, state.segments)


def _clear_withdrawal_on_progress(
    before: SessionState, after: SessionState
) -> SessionState:
    """Retire :attr:`SessionState.withdrawn_approvals` once the agent moves.

    A withdrawal says "what the session does next is unknown". The moment the
    agent produces a token or the turn changes phase, it is no longer unknown —
    it is observed — so the unknown state has to end, or the screen keeps
    hedging over a session it can watch working.

    **The clearing evidence is deliberately narrow.** A heartbeat, an
    ambient event, or another prompt arriving proves the socket is alive and
    proves nothing about the agent, which is exactly the distinction the
    withdrawn state exists to keep. So only the turn phase, the turn index and
    the assistant's own accumulating text count — the case this must not clear
    on is the bad one, where the gateway is still holding the approval and the
    agent is blocked inside the tool call producing nothing at all.
    """
    if not after.withdrawn_approvals:
        return after
    if _turn_progress(after) == _turn_progress(before):
        return after
    return replace(after, withdrawn_approvals=0)


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
        thinking_notice="",
        segments=(),
        interim_boundary=0,
        subagents=(),
    )


def _ensure_streaming(state: SessionState) -> SessionState:
    """Open a turn for a delta that arrived without a ``message.start``.

    Hermes drops these — ``reasoning.delta`` records nothing when no turn is
    open, and its handler for the spinner channel returns early when the UI is
    not busy (``createGatewayEventHandler.ts:752-754``). Talaria cannot: R6 says
    transcript content is never dropped, and a missing start is one of the
    sequences AE2 names. So the turn is synthesized, counted, and marked in the
    transcript — a deterministic, visible outcome rather than a silent one.

    The spinner channel is the one delta that does *not* come here.
    :func:`_on_thinking_delta` carries no transcript content, so opening a turn
    for one would spend a synthetic-start marker on a status frame.
    """
    if state.turn == "streaming":
        return state
    opened = replace(
        state,
        turn="streaming",
        turn_index=state.turn_index + 1,
        streaming_text="",
        reasoning_text="",
        thinking_notice="",
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
        thinking_notice="",
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


def _on_thinking_delta(state: SessionState, event: GatewayEvent) -> SessionState:
    """Replace the live wait-state note. Never appends, never opens a turn.

    ``thinking.delta`` is Hermes's spinner text, not the model's reasoning —
    ``run_agent._emit_wait_notice`` writes it so a long provider stall says what
    it is waiting on instead of showing a bare spinner, and the TUI renders it as
    a status line it overwrites. Talaria's transcript is append-only, so the two
    behaviours that follow from that are the same two ``_ignore`` already applies
    to ``tool.progress``: replay it and a progress spinner becomes transcript
    spam, and synthesize a turn for it and every idle spinner frame writes a
    ``stream began without a message.start event`` line.

    R6 is not weakened by this. Its obligation is that *reasoning-block content*
    is never dropped, and the reasoning block arrives on ``reasoning.delta``,
    which is untouched here and still accumulated in full. This note is not
    dropped either — it is shown on the activity line, which is the region that
    matches what it is: one row, overwritten, describing right now.

    An empty payload clears the note. That is Hermes's own convention — it falls
    back to ``statusFromBusy()`` on an empty value — and the live gateway sends
    exactly that to retire a notice.
    """
    if state.turn == "cancelled":
        return replace(state, late_events_ignored=state.late_events_ignored + 1)
    text = event.payload.get("text")
    if not isinstance(text, str):
        return state
    return replace(state, thinking_notice=clip_detail_line(text.strip()))


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
    next_state = _append(state, "error", f"error: {clip_transcript_line(message)}")
    if state.turn == "cancelled":
        return next_state
    return replace(
        next_state,
        turn="idle",
        streaming_text="",
        reasoning_text="",
        thinking_notice="",
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
    return _append(state, "tool", clip_transcript_line(line))


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
    next_state = _append(next_state, "tool", clip_transcript_line(line))

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
    (``tui_gateway/server.py:1655-1674``) and ``approval.respond`` resolves by
    session key instead (``tui_gateway/methods_prompt.py:886-905``). R8
    nevertheless requires a keyed registry, so approvals get a synthesized key.

    **That key counts arrivals; it used to be one key per session.** A stable
    ``approval:<session_id>`` looked safe on the reasoning that a session can
    only block on one approval at a time, and that reasoning is wrong: every
    guarded call appends its own entry to a per-session *list*
    (``tools/approval.py:3271-3272``) and each one emits its own
    ``approval.request``. With one key the second approval collided with the
    first and was thrown away by the dedupe below — no card, no transcript line,
    no counter — while the operator went on looking at the first command and the
    gateway went on holding two. Pressing "once" then resolved the queue's head.
    Every arrival is now its own entry, so nothing that blocks a session is ever
    discarded. Which of them may be *answered* is decided in
    :func:`respond_to_prompt`, not here.

    The dedupe that remains is the reconnect case (F6): a ``request_id`` already
    outstanding is the gateway re-announcing a live prompt, and keeping the
    first record is correct. It is counted so it stays visible.
    """
    kind = _PROMPT_EVENTS[event.type]
    request_id = coerce_text(event.payload.get("request_id"))
    approvals_seen = state.approvals_seen
    if not request_id and kind == "approval":
        approvals_seen += 1
        request_id = f"approval:{state.focused_session_id or 'session'}#{approvals_seen}"
    if not request_id:
        return state
    if state.prompt_for(request_id) is not None or state.answering_for(request_id) is not None:
        return replace(
            state, duplicate_prompts_ignored=state.duplicate_prompts_ignored + 1
        )
    state = replace(state, approvals_seen=approvals_seen)

    summary = _prompt_summary(kind, event.payload)
    raw_choices = event.payload.get("choices")
    choices = (
        tuple(c for c in raw_choices if isinstance(c, str))
        if isinstance(raw_choices, list)
        else ()
    )
    command = coerce_text(event.payload.get("command")) if kind == "approval" else ""
    prompt = PendingPrompt(
        request_id=request_id,
        kind=kind,
        summary=summary,
        opened_at=event.at,
        seq=event.seq,
        choices=choices,
        session_id=event.session_id or state.focused_session_id,
        command=command,
        read_start=_optional_index(event.payload.get("start")) if kind == "terminal_read" else None,
        read_count=_optional_index(event.payload.get("count")) if kind == "terminal_read" else None,
    )
    return _append(
        replace(state, prompts=(*state.prompts, prompt)),
        "prompt",
        prompt_registration_line(prompt),
    )


def prompt_registration_line(prompt: PendingPrompt) -> str:
    """The transcript entry that records a prompt arriving — the audit anchor.

    **For an approval this is the only place the whole command is written
    down.** The answered line downstream goes through
    :func:`record_local_note`, which clips at
    :data:`~talaria.domain.normalize.TRANSCRIPT_LINE_CLIP`; this entry does not,
    because "which command did I approve" is the question the transcript exists
    to answer afterwards and a clipped answer to it is not an answer. The
    command goes on its own line so a multi-line command stays multi-line —
    :func:`~talaria.domain.projection.transcript_view` splits an entry on
    newlines, so each one becomes its own row rather than a single row with
    ``\\n`` defanged into a control picture.

    **A ``terminal_read`` gets one of these too, and that is a decision rather
    than an oversight.** This buffer is what ``terminal.read`` serves back to
    the agent, so every line here is an input to the next read, and
    :meth:`~talaria.ui.app.TalariaApp._report_prompt_outcome` keeps Talaria's
    own commentary about its *answers* out for exactly that reason. This line
    stays because it records what the *gateway* asked for, not what Talaria
    replied: it is one line per request, it does not compound, and an agent
    reading the operator's screen is a privacy-relevant act that the operator's
    own record should show. Recorded in ``docs/engineering-journal/DECISIONS.md``.
    """
    line = f"{prompt.kind} prompt awaiting an answer: {prompt.summary}"
    if prompt.command:
        return f"{line}\n{APPROVAL_COMMAND_LABEL}{prompt.command}"
    return line


def _optional_index(value: Any) -> int | None:
    """Read an optional non-negative window argument, or ``None``.

    ``bool`` is rejected for the same reason :func:`_as_int` rejects it — it is
    an ``int`` subclass, and a ``True`` where a line number belongs is a
    protocol oddity to ignore rather than to read as line 1. Anything that is
    not an integer becomes ``None``, which the gateway's own contract defines
    as "the visible screen" rather than as an error.
    """
    if isinstance(value, bool) or not isinstance(value, int):
        return None
    return value


def _prompt_summary(kind: PromptKind, payload: Mapping[str, Any]) -> str:
    """A one-line **header** that never carries the answer, and never stands in
    for the command.

    Only fields the gateway sends *outbound* are read. The credential-bearing
    half of every bridge travels the other way (R9), so there is nothing
    sensitive to include here — but reading only the named fields keeps it that
    way if a payload grows.

    For approval this is the *description* — which at the pin is the joined
    pattern warnings that triggered the prompt, not the command
    (``tools/approval.py:3616``, ``:3651-3660``). It is a header and nothing
    more: the command travels beside it in ``PendingPrompt.command`` and is
    rendered whole. This function used to be the *only* approval field the
    interface read, which is how ``rm -rf / --no-preserve-root`` could be
    approved from a card whose one line of text said "recursive delete outside
    the workspace".
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

    **An expiry is honoured for a prompt whose answer is in flight, too.**
    :func:`respond_to_prompt` empties ``prompts`` before the call goes out, so
    for the length of one round trip an arriving ``.expire`` used to match
    nothing and return the state untouched: no marker, and no entry in
    ``flushed_prompt_ids``. That second omission is the damaging one, because
    ``flushed_prompt_ids`` is the only thing standing between an answer that
    reached no socket and a resurrected control — and the gateway never sends a
    second expiry, so the control came back for a bridge that had already
    closed and stayed there for the rest of the session, with the turn pinned at
    ``waiting``. ``answering`` is searched for exactly this window. The marker
    is the same marker, because from the operator's side the same thing
    happened: the question timed out unanswered.
    """
    request_id = coerce_text(event.payload.get("request_id"))
    if not request_id:
        return state
    prompt = state.prompt_for(request_id) or state.answering_for(request_id)
    if prompt is None:
        return state
    return _append(
        replace(
            state,
            prompts=tuple(p for p in state.prompts if p.request_id != request_id),
            answering=tuple(p for p in state.answering if p.request_id != request_id),
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
        detail_line=clip_detail_line(text),
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
        detail_line=clip_detail_line(line),
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
        detail_line=clip_detail_line(text),
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
        detail_line=clip_detail_line(summary) if summary else None,
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
        replace(state, last_status_note=text), "system", clip_transcript_line(text)
    )


def _apply_system_line(state: SessionState, event: GatewayEvent) -> SessionState:
    text = coerce_text(event.payload.get("text")) or coerce_text(
        event.payload.get("message")
    ) or coerce_text(event.payload.get("line"))
    if not text:
        return state
    return _append(state, "system", clip_transcript_line(text))


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
    "thinking.delta": _on_thinking_delta,
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
    "APPROVAL_AGED_OUT",
    "APPROVAL_COMMAND_LABEL",
    "APPROVAL_STALE_AFTER",
    "DELIVERY_NOTES",
    "EXPIRE_EVENT_KINDS",
    "DeliveryState",
    "DenyAllScope",
    "SessionState",
    "age_out_approvals",
    "apply_frame",
    "apply_frames",
    "cancel_turn",
    "focus_session",
    "is_terminal_status",
    "prompt_registration_line",
    "record_local_note",
    "record_replayed_submission",
    "record_submission",
    "replayed_submission_text",
    "respond_to_all_approvals",
    "respond_to_prompt",
    "restore_prompt",
    "set_connection",
]
