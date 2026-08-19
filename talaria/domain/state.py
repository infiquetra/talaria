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

from collections.abc import Callable, Iterable, Mapping
from dataclasses import dataclass, field, replace
from typing import Any, Final, Literal

from talaria.domain.compat import SeamBoard
from talaria.domain.decode import (
    CONNECTION_BROADCAST_EVENT_TYPES,
    DecodedFrame,
    NonEventFrame,
    ProtocolErrorFrame,
    UnknownEventFrame,
)
from talaria.domain.history import decode_history, element_count
from talaria.domain.models import (
    ConnectionStatus,
    GatewayEvent,
    PendingPrompt,
    PromptKind,
    SubagentState,
    SubagentStatus,
    TerminalCause,
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
    coerce_text_exact,
    is_terminal_status,
    keep_terminal_else,
    normalize_subagent_status,
    push_unique,
    subagent_identity,
    subagent_sort_key,
)
from talaria.domain.queue import (
    APPROVAL_DETAIL_TRIGGER_STATUSES,
    QUEUEABLE_KINDS,
    QUEUED_BEHIND_APPROVAL,
    REFUSED_APPROVAL_NOT_HEAD,
    REFUSED_UNCORRELATED_APPROVAL,
    UNCORRELATED_APPROVAL,
    UNOBSERVED_KIND,
    ItemKey,
    NeedsYouQueue,
    PendingApprovalDirectory,
    PolledApproval,
    approval_block_reason,
    approval_detail_due,
    build_queue,
    merge_polled_approvals,
    prompt_feed_rows,
    unresolvable_kind_notice,
)
from talaria.domain.registry import (
    MAX_RUNTIME_ALIASES,
    ROW_CAP_PER_CONNECTION,
    ConnectionChannel,
    ObservationSource,
    RegistryRow,
    RowKey,
    attach_displaces_client,
    note_identityless,
)
from talaria.domain.session_list import ActiveSessionDirectory, SessionDirectory

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
    #: The gateway's **durable** identity for the focused session (R6/R7).
    #:
    #: Kept alongside :attr:`focused_session_id`, not instead of it, because the
    #: two answer different questions and a resume reply can carry different
    #: values for each (``tui_gateway/methods_session.py:494-506``). The runtime
    #: id is what events are stamped with, so it is what
    #: :func:`applies_to_focused_session` must compare against; the
    #: ``session_key`` is what survives the process, so it is what the picker
    #: names a session by and what a later ``session.resume`` asks for. Reading
    #: the durable id off ``focused_session_id`` would silently point the
    #: switcher at an id the gateway forgets when the socket closes.
    session_key: str | None = None
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

    #: Bumped every time :attr:`streaming_text` is *replaced* rather than
    #: appended to (KTD3) — sealed by ``message.interim``, cleared at a new
    #: turn's start, or committed and cleared on any terminal path
    #: (``message.complete``, ``_on_error``, ``cancel_turn``, a typed
    #: transport terminal cause). Never bumped by ``message.delta``'s plain
    #: append. A block-rendering consumer of :class:`~talaria.domain.projection.ProvisionalTail`
    #: uses this to tell "the same tail grew" (append the suffix) from "the
    #: tail was replaced" (re-render from the authoritative text) without
    #: diffing the string itself — the pane receives snapshots, not deltas.
    assistant_stream_generation: int = 0
    #: The same counter for :attr:`reasoning_text`. Two counters, not one,
    #: because KTD2's two provisional tail documents render independently and
    #: neither may be told to replace on the other's account (R18).
    reasoning_stream_generation: int = 0

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
    #: How many unknown-event occurrences were suppressed after the first one
    #: of each type. Counted but never rendered — it exists so that a future
    #: diagnostic can answer "how much did we suppress" without the suppression
    #: having thrown the answer away (KTD4).
    unknown_event_repeats: int = 0
    protocol_error_count: int = 0
    protocol_noise_announced: bool = False
    #: Prompt ids already recorded as abandoned, so the expiry path and the
    #: tool-completion path cannot both write the same trace.
    #:
    #: Not one shape of key. ``clarify``/``secret``/``sudo``/``terminal_read``
    #: entries are session-qualified (:func:`_flush_key`) because those ids
    #: come from the gateway with no cross-session uniqueness promise;
    #: ``approval`` entries stay bare because their synthesized id
    #: (``approval:<session>#<n>``) is already globally unique. See
    #: :func:`restore_prompt`, which is the one place both shapes are read.
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
    #:
    #: Counts only the FOCUSED session's withdrawals: it is screen state, and
    #: the screen shows one session. :func:`age_out_approvals` removes every
    #: session's stale approvals (the round-eight split) but increments this
    #: only for the focused one — a foreign withdrawal's notice is its derived
    #: queue item leaving :func:`fleet_queue`, not a hedge on this session's
    #: activity line.
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
    #: How many ``approval.request`` events this *process* has seen. Approval
    #: carries no request id on the wire, so this counter is what makes each
    #: arrival a distinct registry entry instead of the second one colliding
    #: with the first and being thrown away.
    #:
    #: It counts across sessions rather than per session, and
    #: :func:`focus_session` deliberately leaves it alone. The synthesized id is
    #: session-qualified already, but a switcher can return to a session it has
    #: visited before; a counter that restarted per landing would then mint an
    #: id that session's retained tombstone in ``flushed_prompt_ids`` already
    #: holds, and the returning session's first approval would be swallowed as
    #: an already-closed prompt.
    approvals_seen: int = 0

    def prompt_for(self, request_id: str, session_id: str | None = None) -> PendingPrompt | None:
        """The outstanding prompt for this id, optionally scoped to a session.

        **``session_id`` defaults to ``None``, meaning "any session".** That
        keeps every caller that predates cross-session retention unchanged.
        Once :func:`focus_session` stopped clearing ``prompts`` on a switch,
        two sessions can each hold their own, independently arrived prompt
        under the same bare ``request_id`` (the gateway makes no
        cross-session uniqueness promise for it) — a caller that knows which
        session it means passes it, and gets that session's entry rather than
        whichever one happens to be first in the tuple.
        """
        for prompt in self.prompts:
            if prompt.request_id == request_id and _prompt_matches_session(prompt, session_id):
                return prompt
        return None

    def answering_for(self, request_id: str, session_id: str | None = None) -> PendingPrompt | None:
        """The prompt with an answer in flight under this id, if any.

        See :meth:`prompt_for` for what ``session_id`` does.
        """
        for prompt in self.answering:
            if prompt.request_id == request_id and _prompt_matches_session(prompt, session_id):
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
    """Remove text the transcript already shows from the turn's final message.

    Re-encodes ``finalTail`` (``turnController.ts:81-93``). ``message.complete``
    carries the whole assistant reply, but streaming already committed part of
    it; without this the transcript shows the opening paragraphs twice.

    **The match is exact, not whitespace-trimmed (KTD7).** A committed segment
    is compared against the head of the final text byte-for-byte. Comparing
    stripped forms was the old behaviour, and it breaks the moment a
    committed segment carries meaningful leading whitespace — a four-space
    indented code block, say — because the final text's *own* leading
    whitespace at that position no longer matches a trimmed comparison
    target, the "already shown" prefix goes unrecognized, and the whole reply
    duplicates instead of just its un-dealt-with tail.
    """
    tail = final_text
    for text in committed:
        if text and tail.startswith(text):
            tail = tail[len(text) :]
    return tail


def _commit_partial_streams(state: SessionState) -> SessionState:
    """Commit in-flight streaming and reasoning text as entries, exactly.

    Shared by every terminal path that must not lose partial content (R6):
    the domain error transition (:func:`_on_error`) and the transport's typed
    terminal causes (KTD7, :func:`set_connection`). ``cancel_turn`` does not
    call this — it has its own "*[interrupted]*" marker and bare-cancelled
    fallback, both pinned by regression — but it commits exactly the same two
    buffers, in the same order, and exactly (no stripping).

    Content is committed byte-for-byte. Leading indentation is markdown
    structure (an indented code block), not incidental whitespace, and a
    trailing blank line closes a construct; stripping either one here would
    silently rewrite what the model said on the one path meant to preserve it.

    The caller still owns clearing the buffers and settling the turn — this
    only appends; it never mutates ``streaming_text``/``reasoning_text``
    themselves, so a caller that decides not to clear them (there is none
    today) is free not to.
    """
    next_state = state
    if state.reasoning_text:
        next_state = _append(next_state, "reasoning", state.reasoning_text)
    if state.streaming_text:
        next_state = _append(next_state, "assistant", state.streaming_text)
    return next_state


# ── Local (non-wire) transitions ─────────────────────────────────────────


#: Why a switch is refused while an answer is still travelling.
#:
#: Named here rather than in the UI because the rule is the registry's: the
#: domain decides that a session may not be swapped out from under a call that
#: is still on the wire, and the caller only repeats the sentence.
REFUSED_SWITCH_WHILE_ANSWERING: Final[str] = (
    "an answer is still travelling to the gateway — the session was not "
    "switched; try again in a moment"
)


def switch_refusal(state: SessionState) -> str:
    """Why a session switch must not happen right now, or ``""``.

    **The one refusal is an answer already on the wire.**
    :func:`respond_to_prompt` parks the prompt in ``answering`` before the call
    goes out and the outcome is applied when the call returns — to *whatever*
    state exists by then. Switch in that window and the late outcome is applied
    to the session that was switched to: ``restore_prompt`` puts session A's
    control on session B's screen, and the answered/failed line lands in
    session B's transcript (``talaria.ui.app.TalariaApp.respond_live``). No
    ordering of the clears inside :func:`focus_session` can prevent that,
    because the mutation happens after the switch, so the switch is what has to
    wait. The window it costs is one RPC round trip, bounded by the call
    timeout.

    Callers ask before dispatching so the refusal can be surfaced without a
    wire call; :func:`focus_session` enforces it again, so a caller that
    forgets cannot corrupt the transcript.
    """
    if state.answering:
        return REFUSED_SWITCH_WHILE_ANSWERING
    return ""


def focus_session(state: SessionState, session_id: str | None) -> SessionState:
    """Point the state at a session, clearing anything that belonged to the last.

    Re-encodes ``turnController.reset()`` (``:918-938``) — its comment names the
    failure it prevents, session A's state bleeding into session B. Every
    landing calls it, not just reconnect: ``_land_session``
    (``talaria/ui/app.py:2641-2660``) is the single path startup, ``--resume``
    and the ``/sessions`` switcher all reach the focused session through, so
    this runs on the first landing of a process as well as on every switch
    after it. (The docstring here used to name reconnect as the only caller,
    which was false the day it was written.)

    **A switch is refused outright while an answer is in flight** — see
    :func:`switch_refusal` for the transcript corruption that buys. The refusal
    returns the state unchanged, which is also what an unchanged focus returns;
    a caller that needs to tell "refused" from "already there" asks
    :func:`switch_refusal` first.

    **Three things deliberately survive the switch: ``prompts``,
    ``flushed_prompt_ids`` and ``approvals_seen``.** ``withdrawn_approvals``
    does not — see the end of this docstring for why.

    ``prompts`` is kept. The gateway does not re-announce an outstanding
    bridge across a switch — it started blocking on ``.request`` and has no
    reason to know Talaria stopped showing it — so clearing the registry here
    used to orphan the control forever: switching away and back found an empty
    ``prompts`` for a question the gateway was still holding open, with no
    second ``.request`` ever coming to restore it. Safe to keep only because
    :func:`~talaria.domain.projection.prompt_view` filters *rendering* to the
    focused session: a prompt belonging to the session just switched away from
    stays in the registry, answerable again the moment that session is
    refocused, but is not shown while it is not.

    ``flushed_prompt_ids`` is kept. It is the tombstone set that stops a late
    outcome resurrecting a control the operator has already been told is gone
    (:func:`restore_prompt`), and dropping it is what makes a closed prompt
    come back. Keeping it is only safe because the ids in it stay unique across
    a switch, which is what ``approvals_seen`` below is for.

    ``approvals_seen`` is kept, and that is what keeps the synthesized approval
    ids unique. The ids are session-qualified (``approval:<session>#<n>``,
    :func:`_on_prompt_request`), but qualification alone is not enough once the
    switcher can return to a session it has already visited: with the counter
    reset per landing, coming back to session A would mint ``approval:A#1`` a
    second time and the retained tombstone from the first visit would swallow
    the new prompt. A counter that only ever climbs cannot collide with its own
    past.

    ``withdrawn_approvals`` does **not** survive: it is a count of approvals
    *this* session had withdrawn from under it, and carrying it into the next
    session makes that session's screen hedge about a withdrawal that never
    happened there. It is reset (the defect this function shipped with — it
    cleared ``prompts`` but left the counter, so the switched-to session opened
    saying an approval had been withdrawn).

    **``assistant_stream_generation``/``reasoning_stream_generation`` are
    bumped, exactly as every other site in this file that clears
    ``streaming_text``/``reasoning_text`` already does (KTD3, CR2 finding
    2).** Session A's tail is cleared to empty here, but a generation left
    unchanged tells a block-rendering consumer of
    :class:`~talaria.domain.projection.ProvisionalTail` "the same tail grew" —
    append, don't re-render — which is the wrong instruction for a tail that
    just changed identity from session A's to session B's. Left alone, session
    B's pane could keep showing session A's stale text after the switch. This
    was the one buffer-clearing site in the diff that introduced the
    generation counters without also bumping them.
    """
    if session_id == state.focused_session_id:
        return state
    if switch_refusal(state):
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
        # ``prompts`` deliberately absent — see the docstring above.
        # Already empty — the refusal above is what guarantees it — and stated
        # anyway, so the invariant is visible where the clears are read.
        answering=(),
        withdrawn_approvals=0,
        last_status_note="",
        assistant_stream_generation=state.assistant_stream_generation + 1,
        reasoning_stream_generation=state.reasoning_stream_generation + 1,
    )


#: How the transcript says the gateway did not send the whole history.
#:
#: A resume that withheld its messages is not the same thing as a session with
#: no messages, and a client that renders both as an empty pane is telling the
#: operator something false about their own conversation.
WITHHELD_HISTORY_PREFIX: Final[str] = "earlier history withheld:"


def withheld_history_line(count: int) -> str:
    """Name the messages the reply did not carry, counting them when it can.

    ``count`` is how many elements are missing — ``message_count`` minus the
    elements actually delivered. The gateway's omission shape sends an **empty**
    ``messages`` array with the full ``message_count``
    (``tui_gateway/methods_session.py:494-500``), so the ordinary case is the
    whole history and the subtraction is the whole history's size.

    A non-positive count still gets a line. ``messages_omitted`` is the
    gateway's own statement that something was held back; contradicting it with
    silence because the arithmetic did not agree would drop the one fact this
    line exists to carry.
    """
    if count <= 0:
        return f"{WITHHELD_HISTORY_PREFIX} the gateway did not send this session's earlier messages"
    noun = "message" if count == 1 else "messages"
    return f"{WITHHELD_HISTORY_PREFIX} the gateway did not send {count} earlier {noun}"


def land_session(
    state: SessionState, session_id: str | None, *, session_key: str | None = None
) -> SessionState:
    """Focus a session the gateway just handed back, keeping both its ids (KTD3).

    The one path startup, ``--resume`` and the ``/sessions`` switcher all reach
    a focused session through, and the only caller that clears the transcript.

    **A different session begins a fresh transcript buffer.**
    :func:`focus_session` deliberately keeps the transcript, because its other
    reading is a reconnect to the *same* session, where the history on screen is
    still this session's history and throwing it away would blank the pane on
    every dropped socket. Landing a different session is the opposite case:
    seeding session B's history onto session A's retained transcript produces
    the merged two-session view the project's non-goals forbid, with no marker
    saying where one conversation ended and the next began.

    So the distinction lives here rather than inside :func:`focus_session`:
    same id keeps everything (the reconnect reading), different id starts
    empty (the switch reading).

    **The first landing of a process clears nothing**, because there is no
    previous session to have belonged to. What the transcript holds at that
    point is Talaria's own local notes from before any session existed — the
    compatibility check's blocking verdicts, "there is no previous session to
    resume" — and those describe *this launch*, not a conversation being
    switched away from. Clearing them made the one line explaining a degraded
    startup vanish at the moment the session opened.

    ``entry_seq`` keeps climbing across the clear rather than restarting at
    zero. The counter is an identity, not a position — the renderer keys
    widgets off it — and a fresh buffer that reissues ``seq=0`` hands the pane
    two different entries with the same key inside one process.

    **A refused switch changes nothing at all**, including the transcript. The
    refusal is :func:`switch_refusal`'s — an answer already travelling to the
    gateway — and clearing history for a switch that then does not happen would
    destroy the operator's conversation to no purpose.
    """
    if switch_refusal(state):
        return state
    key = session_key or None
    if session_id == state.focused_session_id:
        return replace(state, session_key=key or state.session_key)
    previous = state if state.focused_session_id is None else replace(state, transcript=())
    return replace(focus_session(previous, session_id), session_key=key)


def seed_history(
    state: SessionState,
    messages: Any,
    *,
    omitted: bool = False,
    count: int = 0,
) -> SessionState:
    """Project a resume reply's history into committed transcript entries (KTD2).

    A dedicated pure transition, not synthesized ``GatewayEvent``s pushed
    through :func:`apply_frame`. Fabricating events would run the reducers'
    turn, prompt and segment bookkeeping over history that already happened:
    ``message.start`` would advance :attr:`SessionState.turn_index` once per
    resumed turn, ``message.complete`` would run the final-tail dedupe against
    segments from a different process, and a resumed approval request would put
    a control on screen for a question the gateway stopped waiting on hours
    ago. The entries are what survived; the machinery that produced them is
    not.

    Committed entries are the whole of what the projection serves as history
    (:func:`~talaria.domain.projection.transcript_view` reads
    ``state.transcript`` and nothing else for committed content), so appending
    here is sufficient for the seeded conversation to be on screen.

    **Append-only, and no deduplication.** A live event that repeats a seeded
    message appends a second entry. The gateway owns history truth; a client
    that silently swallowed a "duplicate" would be deciding that its own guess
    about identity beats what the gateway just said, and the failure mode is
    invisible — a real repeated message vanishing.

    ``count`` is the reply's ``message_count`` and ``omitted`` its
    ``messages_omitted``. They are read together: the withheld line names
    ``count`` minus the elements delivered, so the ordinary omission shape
    (empty array, full count) names the whole history and a partial delivery
    names the difference.

    **The withheld notice is appended before the delivered lines, not after.**
    It describes messages that precede the delivered ones in time — the
    gateway held back the *earlier* part of the conversation, not the later
    part — so a notice appended last would sit below messages it is supposed
    to introduce, reading as though it followed them instead of preceding
    them.
    """
    lines = decode_history(messages)
    next_state = state
    if omitted:
        next_state = _append(
            next_state, "system", withheld_history_line(count - element_count(messages))
        )
    for kind, text in lines:
        next_state = _append(next_state, kind, text)
    return next_state


def set_connection(
    state: SessionState,
    status: ConnectionStatus,
    *,
    cause: TerminalCause | None = None,
    at: float | None = None,
) -> SessionState:
    """Record a transport state change and re-arm the once-per-connection latch.

    ``cause`` is KTD7's typed end-of-stream cause. When it is one of the four
    terminal members (``auth_failed``, ``dial_failed``, ``orderly_close``,
    ``reconnect_exhausted``), the transport is telling the domain the stream
    genuinely will not resume — so, exactly like ``_on_error`` and
    ``cancel_turn``, any partial ``streaming_text``/``reasoning_text`` is
    committed as transcript entries before it is cleared (R6), and a
    streaming turn settles to ``idle`` because nothing more is coming for it.
    A turn already ``cancelled`` stays ``cancelled`` — the operator's own
    outcome is not overwritten by a connection that dropped afterwards.

    ``cause=None`` (the default, and every non-terminal status change —
    ``connecting``, ``connected``, ``reconnecting``, or a reconnect attempt
    that failed but will retry) commits nothing and clears nothing. That is
    what makes a transient reconnect that resumes the same response arrive
    without duplication: the segment/interim machinery is the dedupe
    backstop for whatever text *does* arrive, and a cause-less call never
    touches the buffers it would otherwise need to dedupe against.

    The early return below is gated on ``cause is None`` as well as on the
    status being unchanged — a repeated terminal notification (``close()``
    called again after the stream already ended, say) must still be allowed
    through so the commit is never silently skipped, even though it is a
    no-op in practice once the buffers are already empty.
    """
    if cause is None and status == state.connection:
        return state

    next_state = state
    if cause is not None:
        next_state = _commit_partial_streams(next_state)
        next_state = replace(
            next_state,
            turn="idle" if next_state.turn == "streaming" else next_state.turn,
            streaming_text="",
            reasoning_text="",
            thinking_notice="",
            segments=(),
            interim_boundary=0,
            last_observed_at=(
                max(next_state.last_observed_at, at)
                if at is not None
                else next_state.last_observed_at
            ),
            assistant_stream_generation=next_state.assistant_stream_generation + 1,
            reasoning_stream_generation=next_state.reasoning_stream_generation + 1,
        )

    return replace(
        next_state,
        connection=status,
        protocol_noise_announced=False,
        unknown_event_types=(),
        unknown_event_repeats=0,
    )


def cancel_turn(state: SessionState, *, at: float) -> SessionState:
    """Cancel the in-flight turn, leaving a permanent transcript trace (R4).

    Re-encodes ``interruptTurn`` (``turnController.ts:297-351``), specifically
    its "always surface an interruption indicator" branch at ``:322-331``: when
    partial text exists it is preserved and marked, and when nothing was streamed
    a bare note is written anyway, so the transcript never shows a turn that
    simply stopped.

    **Preserved exactly, no stripping (KTD7).** The commit behaviour here —
    both buffers committed before clearing — is pinned by regression; the one
    change this made is removing the ``.strip()``/``.lstrip()`` that used to
    run on the way out. Leading indentation and trailing blank lines are
    markdown structure, and the guard below (a plain truthiness check rather
    than a stripped one) is what lets a whitespace-only reasoning or
    streaming buffer still leave its trace, matching every other
    content-channel path.
    """
    if state.turn != "streaming":
        return state

    next_state = state
    if state.reasoning_text:
        next_state = _append(next_state, "reasoning", state.reasoning_text)

    if state.streaming_text:
        next_state = _append(
            next_state, "assistant", f"{state.streaming_text}\n\n*[interrupted]*"
        )
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
        assistant_stream_generation=next_state.assistant_stream_generation + 1,
        reasoning_stream_generation=next_state.reasoning_stream_generation + 1,
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

#: The approval-correlation vocabulary is defined in
#: :mod:`talaria.domain.queue` and imported above, not restated here.
#:
#: Both readers need the identical sentence: the queue decides which item may be
#: *offered*, this registry decides which answer may be *sent*, and a screen that
#: offers what the registry refuses is the exact defect
#: :class:`~talaria.domain.projection.PromptRow`'s ``answerable`` field exists to
#: prevent. ``queue`` is the leaf of the two, so the strings live there and every
#: existing importer of ``state.UNCORRELATED_APPROVAL`` keeps working through
#: this module's re-export.


def respond_to_prompt(
    state: SessionState,
    request_id: str,
    *,
    session_id: str | None = None,
    foreign_approvals: int = 0,
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
    than a race.** ``approval.request`` carries no request id at the pinned read
    (``tui_gateway/server.py:1655-1674``) and ``approval.respond`` pops the
    *oldest* entry in the session's queue (``tools/approval.py:2214-2222``).
    While exactly one approval is outstanding, that is unambiguous — the answer
    lands on that approval, or on an empty queue, and the reply's own
    ``resolved`` count says which. The moment a second approval is outstanding it
    stops being unambiguous, because the gateway also removes an entry on timeout
    and on interrupt **without emitting anything**
    (``tools/approval.py:3336-3344``), so the queue head and the oldest card on
    screen can differ. Answering then approves a command the operator was never
    shown.

    **R18, amended 2026-08-17, splits that refusal in two.** The running
    revision synthesizes a ``request_id`` on every approval entry
    (``tools/approval.py:2596``, ``uuid4`` by ``setdefault``) and emits it with
    the request event, and ``approval.respond`` forwards the parameter. So there
    are now two cases where there used to be one:

    * **The head, with an id the gateway itself sent** — answerable. The answer
      names the entry it means (:attr:`PendingPrompt.observed_request_id`), which
      is precisely the correlation whose absence the old refusal stood in for.
    * **The head, with no observed id** — refused exactly as before, with the
      deny-all fallback unchanged, because a synthesized key is Talaria's own and
      putting it on the wire would claim a correlation that is not happening.

    **A session's second approval is refused in both cases**, which is a
    tightening rather than a loosening: the gateway holds approvals in a queue
    and resolves from its head, so an answer aimed at the second would land on
    the first. That is R18's head-of-queue rule, and
    :func:`~talaria.domain.queue.build_queue` renders the same sentence on the
    same item, from the same constants.

    ``foreign_approvals`` is how many approvals this session is known to have at
    the gateway that are **not** in this registry — learned from an
    ``approval.pending`` poll, which is the fleet's business and not a session's.
    It defaults to zero, which is what a focused-session-only caller knows.

    It exists because the registry and the queue were counting different things.
    The gateway keeps **one** approval queue per session, and a poll's rows are
    entries in that same queue — so a session holding one driven approval and one
    polled one has two, and the registry, seeing one, called it a lone approval
    and allowed the answer. That answer would have landed on whichever entry the
    gateway had at its head, which may be the polled one. Ordering the two is not
    possible: a polled row carries no start stamp, only an observation floor
    (KTD12), so "which is older" is not a question the data answers. A count above
    zero therefore means *not provably the head*, and the answer is refused —
    conservative in the direction that cannot answer the wrong question.

    **Nothing supplies it yet.** U6 owns the rule and its agreement with the
    queue; wiring the fleet's own count into the answer path belongs to the unit
    that answers from the queue.

    Every refusal increments ``rejected_responses`` rather than raising. A
    refused response is an ordinary race, not a fault.
    """
    # Looked up scoped to ``session_id`` (A1 sweep): with two sessions each
    # holding their own prompt under the same bare id, an unscoped lookup
    # would return whichever is first in ``prompts`` regardless of which
    # session actually asked, misrouting the correlation check below against
    # the wrong entry. Scoping the lookup finds the asking session's own
    # entry directly.
    prompt = state.prompt_for(request_id, session_id=session_id)
    if prompt is None:
        # Nothing of this id belongs to ``session_id``. A same-id prompt
        # elsewhere still names the real reason (R9) rather than the less
        # precise "not outstanding" — checked unscoped only to pick the
        # refusal's wording, never to let an unscoped match through to an
        # answer.
        if session_id is not None and state.prompt_for(request_id) is not None:
            return _refuse(state), REFUSED_WRONG_SESSION
        return _refuse(state), REFUSED_NOT_OUTSTANDING
    if session_id is not None and prompt.session_id is not None and prompt.session_id != session_id:
        return _refuse(state), REFUSED_WRONG_SESSION
    if prompt.kind == "approval":
        # ``outstanding_approvals`` is ordered by the frame sequence the requests
        # arrived on, which is the order the gateway enqueued them and therefore
        # the order its resolver pops them in.
        #
        # The decision itself is :func:`~talaria.domain.queue.approval_block_reason`
        # and not two branches written out here. It was two branches written out
        # here until 2026-08-17, beneath a docstring on that function claiming
        # three callers when it had two — and the two disagreed in a reachable
        # state: a settled head left the queue offering the second approval while
        # this function refused it. Only the mapping from the reason onto this
        # module's "nothing was sent" wording is local, because these sentences
        # are about a refusal and the queue's are about a control not offered.
        queued = state.outstanding_approvals(prompt.session_id)
        locally_head = bool(queued) and queued[0].request_id == prompt.request_id
        reason = approval_block_reason(
            is_head=locally_head and not foreign_approvals,
            queued_count=len(queued) + foreign_approvals,
            observed_request_id=prompt.observed_request_id,
        )
        if reason == QUEUED_BEHIND_APPROVAL:
            return _refuse(state), REFUSED_APPROVAL_NOT_HEAD
        if reason == UNCORRELATED_APPROVAL:
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
    """Move prompts out of the registry and into the in-flight set.

    **Removed by identity, not by bare id (A1 sweep).** ``prompts`` is
    already the exact set of objects being moved — a caller-scoped selection
    from :func:`respond_to_prompt` or :func:`respond_to_all_approvals`. A
    bare-id removal would also drop a *different* session's unrelated entry
    that happens to share the same wire id, silently losing a control that
    was never being answered at all.
    """
    taken_ids = {id(p) for p in prompts}
    return replace(
        state,
        prompts=tuple(p for p in state.prompts if id(p) not in taken_ids),
        answering=(*state.answering, *prompts),
    )


def settle_prompt(
    state: SessionState, request_id: str, session_id: str | None = None
) -> SessionState:
    """Drop an in-flight answer once its outcome is known. Idempotent.

    Called for every terminal outcome, including the ones that put the control
    back — an id left in ``answering`` forever would let a much later expiry
    write a marker for a question that was answered minutes ago.

    ``session_id``, when given, scopes both the check and the removal to that
    session's own entry (A1 sweep): two sessions can each have their own
    answer in flight under the same bare id, and an unscoped removal would
    drop both when only one call actually settled.
    """
    target = state.answering_for(request_id, session_id=session_id)
    if target is None:
        return state
    return replace(
        state,
        answering=tuple(
            p for p in state.answering if not (p.request_id == request_id and p is target)
        ),
    )


def latch_resolved_prompts(
    state: SessionState, prompts: Iterable[PendingPrompt]
) -> SessionState:
    """Tombstone the prompts a single resolution already covered. Pure.

    **This is the queued defect "a deny-all that succeeds can re-offer a
    control the gateway already resolved".** ``approval.respond {all: true}``
    resolves *every* entry in the gateway's queue
    (``resolve_gateway_approval(..., resolve_all=True)`` over ``list(queue)``),
    including an approval whose own single answer is still on the wire. That
    single answer keeps its own owner, and when it comes back a definite
    ``not_sent`` its owner does the correct thing for a call that reached no
    socket: :func:`restore_prompt` puts the control back. Correct in isolation
    and wrong here — the deny-all already resolved that entry, so the operator
    is handed a live-looking control for a question the gateway has stopped
    waiting on, and the gateway sends no second expiry to take it away again.
    The ids the sweep resolved are latched instead, which is the same mechanism
    an expiry uses and the same one :func:`restore_prompt` already consults.
    (KTD4/KTD8, U2.)

    Only the ``flushed_prompt_ids`` set is touched. Nothing is removed from
    ``prompts`` or ``answering``: an id whose own call is still travelling is
    still that call's to settle, and taking it away here would leave the reply
    with nothing to land on. The latch is a refusal to *resurrect*, not a
    clear.

    Idempotent, and safe to call with ids that were never outstanding —
    ``flushed_prompt_ids`` is only ever read as "may this come back", and an id
    that never left cannot come back.

    **Takes prompts, not bare ids, and qualifies each key itself (A4).**
    Every non-approval bridge's tombstone must be session-qualified the same
    way :func:`_flush_key` qualifies one and :func:`restore_prompt` reads it
    back — a bare non-approval key here would let one session's latch block a
    different session's own, independently arrived id of the same kind.
    Taking the prompt rather than a caller-supplied id and session means the
    qualification always matches the prompt's *own* recorded session, not
    whatever the caller happened to be tracking. Approval ids stay bare:
    their synthesized ``approval:<session>#<n>`` shape is already globally
    unique, and qualifying it a second time would just be a longer bare id.
    """
    keys = frozenset(
        p.request_id if p.kind == "approval" else _flush_key(p.session_id, p.request_id)
        for p in prompts
    )
    if not keys:
        return state
    return replace(state, flushed_prompt_ids=state.flushed_prompt_ids | keys)


def _prompt_matches_session(prompt: PendingPrompt, session_id: str | None) -> bool:
    """Whether ``prompt`` is a candidate for a lookup scoped to ``session_id``.

    ``None`` on either side means "unscoped" — a query with no session to
    correlate against matches anything, and a prompt with no session of its
    own (replay with no ``session_id`` on its events, or before
    :attr:`SessionState.focused_session_id` was ever learned) matches any
    query. This is the identity half of the same rule
    :func:`~talaria.domain.projection._focused_prompts` applies to display:
    a prompt is *shown* under this rule, and now it is also *found* under it.
    """
    return session_id is None or prompt.session_id is None or prompt.session_id == session_id


def _flush_key(session_id: str | None, request_id: str) -> str:
    """Session-qualify a tombstone key for a bridge whose ``request_id`` comes
    from the gateway unmodified — ``clarify``, ``secret``, ``sudo`` and
    ``terminal_read``.

    Now that :func:`focus_session` retains ``flushed_prompt_ids`` across a
    switch, a bare ``request_id`` is not enough: the gateway hands these ids
    out per session with no global-uniqueness promise, so session A's
    ``req-1`` expiring must not tombstone session B's own, independently
    arrived ``req-1``. Qualifying by the prompt's own session is the same
    discipline the synthesized approval id already has
    (``approval:<session>#<n>``, :func:`_on_prompt_request`) — which is also
    why ``approval`` ids are never passed through here: they are already
    globally unique via the monotonic ``approvals_seen`` counter, so
    :func:`latch_resolved_prompts` and :func:`age_out_approvals` (both
    approval-only) write and are read back as bare ids, and this helper is
    only ever consulted for the other four bridges.
    """
    return f"{session_id or ''}:{request_id}"


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

    **The check reads both a bare and a session-qualified key** — see
    :func:`_flush_key` — because the writers are not uniform: ``clarify``,
    ``secret``, ``sudo`` and ``terminal_read`` tombstones are qualified by the
    prompt's own session, while ``approval`` tombstones stay bare because their
    synthesized id is already globally unique. Checking both is cheap and
    correct for either shape; checking only one would let the other bridge's
    tombstone go unread.

    No transcript entry is written. The ``prompt`` line was appended when the
    request first arrived and the transcript is append-only, so a second one
    would show the agent asking twice.
    """
    state = settle_prompt(state, prompt.request_id, session_id=prompt.session_id)
    if prompt.request_id in state.flushed_prompt_ids:
        return state
    if _flush_key(prompt.session_id, prompt.request_id) in state.flushed_prompt_ids:
        return state
    # Scoped to this prompt's own session (A1 sweep): an unscoped check would
    # refuse to restore session B's prompt because session A happens to hold
    # an outstanding entry under the same bare id, even though B's own id is
    # not outstanding at all.
    if state.prompt_for(prompt.request_id, session_id=prompt.session_id) is not None:
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

    **The removal is unconditional at the threshold; only the presentation
    stays focus-scoped (the round-eight split, 2026-08-18).** ``prompts``
    can hold a foreign session's retained approval since
    :func:`focus_session` stopped clearing it. The A3 guard this paragraph
    used to state deferred a foreign approval's whole age-out "until its own
    session is focused again" — and a session whose runtime id the alias
    window trimmed can never be focused again, so the deferral was permanent
    and the registry grew one approval per land-approve-switch-away cycle
    (measured: 11 of 12 prompts surviving age-out at 1000x the threshold).
    So the ``stale`` filter below takes every session's stale approvals,
    focused or not — pinned by
    ``test_age_out_removes_a_foreign_sessions_stale_approval_silently``,
    ``test_a_trimmed_id_phantom_off_focus_ages_out_at_threshold`` and the
    inverted probe
    ``test_twelve_switch_away_cycles_age_out_to_zero_at_threshold``.

    What A3 rightly protected is kept, as the ``presented`` split below: the
    *effects* — ``withdrawn_approvals`` and the transcript line — still have
    nowhere to go but the one session ``state`` represents. Incrementing the
    focused session's counter or writing a foreign command into its
    transcript for another session's withdrawal is exactly the merged
    multi-session view the plan's non-goals forbid
    (``docs/plans/2026-08-08-talaria-v0-2-answerability-and-session-story-
    plan.md:519``), so they fire only for prompts whose session IS the
    focused one — pinned by
    ``test_age_out_still_counts_and_writes_the_line_for_the_focused_session``.
    A foreign withdrawal is silent *here* but not invisible: the fleet queue
    derives its items from ``prompts`` on every read (:func:`fleet_queue`),
    so removing the prompt withdraws the item from the owning row's own
    surface — the removal is the notice.
    """
    if now <= 0.0:
        return state
    stale = tuple(
        p
        for p in state.prompts
        if p.kind == "approval"
        and p.opened_at > 0.0
        and now - p.opened_at >= APPROVAL_STALE_AFTER
    )
    if not stale:
        return state
    dropped = {p.request_id for p in stale}
    presented = tuple(
        p
        for p in stale
        if p.session_id is None or p.session_id == state.focused_session_id
    )
    next_state = replace(
        state,
        prompts=tuple(p for p in state.prompts if p.request_id not in dropped),
        flushed_prompt_ids=state.flushed_prompt_ids | dropped,
        # Counted, not flagged: two approvals can age out on one tick, and the
        # screen names how many were withdrawn rather than saying "an approval"
        # about a number it knows. Only ``presented`` is counted: the counter
        # is the focused session's screen state, not the fleet's.
        withdrawn_approvals=state.withdrawn_approvals + len(presented),
    )
    for prompt in presented:
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
        # Apply the same cross-session guard ``_apply_event`` already enforces
        # (normalize.py:131-145), using the frame's own type and session_id.
        # ``UnknownEventFrame`` carries a session_id, so routing it past the
        # guard would let a background session's unknown event write a row into
        # the foreground session's transcript and corrupt the per-type latch's
        # repeat count in the same motion (KTD5).
        # ``gateway.``-prefixed types are never session-scoped and always pass.
        if (
            not decoded.type.startswith("gateway.")
            and decoded.session_id is not None
            and state.focused_session_id is not None
            and decoded.session_id != state.focused_session_id
        ):
            return replace(
                state, cross_session_events_ignored=state.cross_session_events_ignored + 1
            )
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
    """Announce an unknown event type once per connection, not once per occurrence.

    Re-encodes the same latch ``_apply_protocol_error`` already uses for
    ``protocol_noise_announced``: the first arrival of a type appends a transcript
    row and records the type in ``unknown_event_types``; later arrivals of the
    same type are counted in ``unknown_event_repeats`` and produce no row. The
    latch is per type — two distinct unknown types each get their own first row
    — and its lifetime is the connection (reset alongside
    ``protocol_noise_announced`` in :func:`set_connection`).

    R5 still holds in full: the type is surfaced by name rather than dropped.
    It is surfaced once, and the recurrence is counted rather than discarded.
    """
    if frame.type in state.unknown_event_types:
        return replace(state, unknown_event_repeats=state.unknown_event_repeats + 1)
    return _append(
        replace(state, unknown_event_types=(*state.unknown_event_types, frame.type)),
        "unknown-event",
        frame.text,
    )


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
        assistant_stream_generation=state.assistant_stream_generation + 1,
        reasoning_stream_generation=state.reasoning_stream_generation + 1,
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
        assistant_stream_generation=state.assistant_stream_generation + 1,
        reasoning_stream_generation=state.reasoning_stream_generation + 1,
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

    ``coerce_text_exact``, not ``coerce_text`` (KTD7): this is the content
    channel, and stripping it is what used to turn a four-space indented
    reply into an unindented one the moment it sealed.
    """
    if state.turn == "cancelled":
        return replace(state, late_events_ignored=state.late_events_ignored + 1)

    text = coerce_text_exact(event.payload.get("text"))
    if not text:
        return state

    opened = _ensure_streaming(state)
    committed = _append(opened, "assistant", text)
    return replace(
        committed,
        streaming_text="",
        segments=(*opened.segments, text),
        interim_boundary=len(opened.segments) + 1,
        # Sealing the provisional text is a replacement, not a growth step
        # (KTD3): the tail that follows starts over, empty, at a new
        # generation, so a stale append against the sealed text is never
        # mistaken for the next turn's content continuing to grow.
        assistant_stream_generation=committed.assistant_stream_generation + 1,
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
    if state.reasoning_text:
        next_state = _append(next_state, "reasoning", state.reasoning_text)
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
        assistant_stream_generation=next_state.assistant_stream_generation + 1,
        reasoning_stream_generation=next_state.reasoning_stream_generation + 1,
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

    **No left-strip on the raw text (KTD7).** The content channel is
    preserved exactly; ``_final_tail`` now compares byte-for-byte rather than
    on stripped forms, so it no longer needs a pre-stripped ``raw`` to line
    its comparisons up.
    """
    raw = payload.get("text")
    if not isinstance(raw, str):
        raw = payload.get("rendered")
    if not isinstance(raw, str):
        raw = state.streaming_text

    dedupe_start = 0 if payload.get("response_previewed") is True else state.interim_boundary
    final = _final_tail(raw, state.segments[dedupe_start:])

    if not final and state.streaming_text:
        # The gateway sent an empty final message while text was still buffered.
        # Hermes would drop it; R6 says transcript content is never dropped, so
        # the buffered text is committed instead, exactly as accumulated.
        return state.streaming_text
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

    ``coerce_text_exact`` (KTD7): this lands in ``reasoning_text``, the same
    content-channel buffer ``reasoning.delta`` accumulates into and every
    terminal path later commits exactly — stripping it here would have
    already lost the whitespace those paths exist to preserve.
    """
    if state.turn == "cancelled":
        return replace(state, late_events_ignored=state.late_events_ignored + 1)
    text = coerce_text_exact(event.payload.get("text"))
    if not text or state.reasoning_text:
        return state
    opened = _ensure_streaming(state)
    return replace(opened, reasoning_text=text)


def _on_error(state: SessionState, event: GatewayEvent) -> SessionState:
    """Surface a turn error and settle the turn.

    Re-encodes ``recordError`` (``turnController.ts:545-556``), minus its notice
    flush. A cancelled turn stays cancelled: an error arriving after the
    operator cancelled is not a second, different outcome.

    **Commits the partial streaming and reasoning buffers before clearing
    (R6, KTD7).** An error is one of the ways a turn ends without the
    gateway ever sending ``message.complete``, so whatever was streamed so
    far used to vanish silently on this path — the defect KTD7 exists to
    close. The error line itself is appended *after* the partial content, so
    the transcript reads as "here is what arrived, and here is why it
    stopped," the same order ``cancel_turn`` already uses for its own marker.
    """
    message = coerce_text(event.payload.get("message")) or "unknown error"
    next_state = _commit_partial_streams(state)
    next_state = _append(next_state, "error", f"error: {clip_transcript_line(message)}")
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
        assistant_stream_generation=next_state.assistant_stream_generation + 1,
        reasoning_stream_generation=next_state.reasoning_stream_generation + 1,
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
    next_state = (
        _flush_abandoned_clarify(state, event.session_id or state.focused_session_id)
        if name == "clarify"
        else state
    )

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


def _flush_abandoned_clarify(
    state: SessionState, session_id: str | None
) -> SessionState:
    """Record an abandoned clarify once, from whichever path notices first.

    The dedupe set re-encodes ``persistedAbandonedClarify``
    (``createGatewayEventHandler.ts:399-402``), which exists because two
    independent paths — the clarify tool's own completion and the end of the
    message — can both notice the same abandonment.

    The tombstone is session-qualified (:func:`_flush_key`): ``clarify``'s
    ``request_id`` comes from the gateway unmodified, and now that
    :func:`focus_session` retains ``flushed_prompt_ids`` across a switch, an
    unqualified key would let one session's abandoned clarify block a later,
    unrelated session's clarify that happens to reuse the same id.

    **``session_id`` scopes which clarify this call may flush at all (A2).**
    The caller is a ``tool.complete`` for one specific session; without this,
    the loop below took the *first* clarify anywhere in ``prompts``, so
    session B's own tool completing could flush session A's retained,
    still-outstanding clarify — a control the operator can still see and
    answer, cleared out from under them by a different conversation entirely.
    """
    for prompt in state.prompts:
        if prompt.kind != "clarify" or not _prompt_matches_session(prompt, session_id):
            continue
        key = _flush_key(prompt.session_id, prompt.request_id)
        if key in state.flushed_prompt_ids:
            continue
        remaining = tuple(p for p in state.prompts if p is not prompt)
        flushed = _append(
            replace(
                state,
                prompts=remaining,
                flushed_prompt_ids=state.flushed_prompt_ids | {key},
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
    outstanding **in this same session** is the gateway re-announcing a live
    prompt, and keeping the first record is correct. It is counted so it
    stays visible.

    **Registration identity is (session, request id), not request id alone
    (A1).** Now that :func:`focus_session` retains ``prompts`` across a
    switch, session A's ``req-1`` can still be outstanding when session B's
    own, independently arrived ``req-1`` registers — the gateway makes no
    cross-session uniqueness promise for these ids. A bare-id dedupe read
    that as the reconnect case above and threw B's prompt away: no card, no
    counter, and the gateway left holding a control nothing in Talaria would
    ever answer. Scoping the check to ``session_id`` — the same value this
    registration stores on the ``PendingPrompt`` below — lets both entries
    coexist, the same way two sessions' approvals already coexist under
    their own synthesized keys.
    """
    kind = _PROMPT_EVENTS[event.type]
    request_id = coerce_text(event.payload.get("request_id"))
    #: What the *gateway* sent, before any local synthesis below (R18 as amended
    #: 2026-08-17). Kept separately because the two diverge exactly where the
    #: safety rule turns: a synthesized key is a registry key and never goes on
    #: the wire, while an observed id is what makes an answer aimable.
    observed_request_id = request_id
    approvals_seen = state.approvals_seen
    session_id = event.session_id or state.focused_session_id
    if not request_id and kind == "approval":
        # Session-qualified *and* monotonically counted. The qualifier is the
        # event's own session when it names one, so the key says which
        # conversation blocked; the counter never restarts (see
        # ``SessionState.approvals_seen``), so returning to a session cannot
        # mint a key its own tombstone in ``flushed_prompt_ids`` already holds.
        approvals_seen += 1
        session_key = session_id or "session"
        request_id = f"approval:{session_key}#{approvals_seen}"
    if not request_id:
        return state
    if (
        state.prompt_for(request_id, session_id=session_id) is not None
        or state.answering_for(request_id, session_id=session_id) is not None
    ):
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
        session_id=session_id,
        command=command,
        read_start=_optional_index(event.payload.get("start")) if kind == "terminal_read" else None,
        read_count=_optional_index(event.payload.get("count")) if kind == "terminal_read" else None,
        observed_request_id=observed_request_id,
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

    The tombstone is session-qualified (:func:`_flush_key`). None of the four
    bridges this handles carry a request id the gateway promises is unique
    across sessions, and now that :func:`focus_session` retains
    ``flushed_prompt_ids`` across a switch, an unqualified key would let
    session A's expired ``req-1`` block session B's own, independently
    arrived ``req-1`` forever.
    """
    request_id = coerce_text(event.payload.get("request_id"))
    if not request_id:
        return state
    # Scoped to the event's own session (A1 sweep — this handler was still
    # matching by bare id): with session A's retained prompt and session B's
    # own arrival sharing an id, an unscoped match could clear and tombstone
    # the WRONG session's entry, or clear both when only one actually
    # expired. Identity (``is prompt``), not id, drives the removal, so only
    # the entry this expiry actually names is taken out.
    session_id = event.session_id or state.focused_session_id
    prompt = state.prompt_for(request_id, session_id=session_id) or state.answering_for(
        request_id, session_id=session_id
    )
    if prompt is None:
        return state
    return _append(
        replace(
            state,
            prompts=tuple(p for p in state.prompts if p is not prompt),
            answering=tuple(p for p in state.answering if p is not prompt),
            flushed_prompt_ids=(
                state.flushed_prompt_ids | {_flush_key(prompt.session_id, request_id)}
            ),
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
    """Fold a ``session.info`` update in. Already filtered to the focused
    session by :func:`_apply_event`'s cross-talk guard, so nothing here
    re-checks which session this is about.

    **``session_key`` is refreshed from the event's ``stored_session_id``
    (P1, U7 round two).** The installed gateway sends a fresh one whenever
    the durable key changes under a live session — most concretely, a
    compression rotation (``tui_gateway/server.py:5200``) — and until this
    read existed the picker kept comparing rows against the *old* key
    forever: the freshly-current row never matched, stayed selectable, and
    choosing it retained the transcript before seeding the same history a
    second time. Folded the same way ``title`` already is: an empty or
    missing value keeps what is there rather than blanking a real key with a
    blank one.
    """
    title = coerce_text(event.payload.get("title")) or state.session_title
    usage_payload = event.payload.get("usage")
    usage = (
        state.usage.merged_with(usage_payload)
        if isinstance(usage_payload, dict)
        else state.usage
    )
    session_key = coerce_text(event.payload.get("stored_session_id")) or state.session_key
    return replace(state, session_title=title, usage=usage, session_key=session_key)


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

#: The request half of the same table, published for callers that must turn an
#: event type into the prompt kind it registers without a second copy of the
#: mapping (U4's attach handover asks "which kinds did this reply hydrate").
PROMPT_EVENT_KINDS: Mapping[str, PromptKind] = _PROMPT_EVENTS


# ── The fleet root (U3, KTD3) ────────────────────────────────────────────
#
# ``FleetState`` inverts the domain root: the registry of every session on
# every connection is the container, and the focused session is a cursor into
# it. The existing ``SessionState`` reducer above is NOT rewritten — it remains
# the focused session's engine, fed exactly as today (R2 by construction,
# minimal blast radius). The cross-talk guard's *protection* survives inside
# that engine; its *discard* is replaced here, one level up: the router sends
# foreign-session events to registry rows instead of dropping them (R4), and
# ``cross_session_events_ignored`` stops counting routable events — its R25
# role (the identity-less counter) moves to the per-connection channel.
#
# Scope note (recorded deliberately): the live launch in ``talaria/cli.py``
# still builds one ``LiveSource`` feeding ``SessionState`` directly. Wiring
# ``ConnectionSet``/``FleetState`` into the app is not done in this unit — the
# plan's U3 file map is domain-only, and every surface that would consume the
# fleet (picker, needs-you bar, confirm flow) lands in U4–U7, which own the
# app files. Until then the legacy single-connection path keeps its shipped
# behavior, and this root is exercised by the domain suite and by replay.
#
# U4's amendment to that note: the app now *holds* a ``FleetState`` — the
# focused engine is its ``focused`` field, and the registry is seeded from the
# two listing sweeps the ``/sessions`` picker already makes. What U4 did not
# do is route inbound frames through :func:`route_frame`, and it named U6's
# feed wiring as the owner of that move rather than doing it inside a
# focus-movement unit.
#
# U6's amendment: the move is done. ``TalariaApp.ingest`` routes every frame
# through :func:`route_frame`, so an identified background event now updates
# its registry row instead of being discarded, and
# ``cross_session_events_ignored`` no longer counts it — the change U4 named
# and deferred, made where the queue needs it. The counter's shipped meaning
# for an *identity-less* frame is untouched (R25): those are still counted and
# surfaced on the connection channel. Pinned by
# ``test_an_identified_background_event_updates_its_row_and_is_not_counted_ignored``
# and ``test_an_identityless_frame_is_still_counted_ignored_through_the_app``.


#: One in-flight answer's fleet-scoped identity (KTD9): the connection it was
#: sent on, the session it names, and the request key that names the question.
#:
#: Distinct from :data:`~talaria.domain.registry.RowKey` on purpose. A row key
#: says *which row*; this says *which answer*, and two answers against one row
#: (a second approval in the same session) must be able to coexist and to end
#: independently.
AnswerKey = tuple[str, str, str]


@dataclass(frozen=True)
class FleetState:
    """The domain root: registry rows, per-connection channels, one focused engine.

    ``focused`` is the untouched v0.1–v0.3 :class:`SessionState`;
    ``focused_profile`` names the connection that feeds it. ``rows`` is keyed
    ``(profile, durable_id)`` (KTD3); ``aliases`` maps ``(profile, runtime_id)``
    to a key, because runtime ids change across resume and are rebound, never
    the key. ``queue_item_keys`` and :meth:`answering_keys` are the protection
    sets the memory bound and retirement must never evict — populated by the
    queue (U6) and the fleet-scoped answering bookkeeping (U4/KTD9); U3
    enforces the protection, and U4 feeds the answering half.

    ``answers_in_flight`` is that answering half, and it is stored as a
    *mapping* rather than as a bare set of row keys on purpose (U4/KTD9). Its
    keys are the fleet-scoped answer identity KTD9 specifies —
    ``(profile, session, request key)`` — which is what
    :func:`fleet_switch_refusal` needs; its values are the registry keys the
    protection is about, resolved at record time per the contract in
    :meth:`protected_keys`. One structure carrying both means there is exactly
    one thing :func:`_rebind_durable` has to re-anchor, which is the whole
    lesson of that method's history section.

    ``clock`` is the frame-clock high-water mark: the largest ``at`` this
    fleet has folded, and the only value ages are computed against (KTD12).
    """

    focused: SessionState = field(default_factory=SessionState)
    focused_profile: str = "default"
    rows: Mapping[RowKey, RegistryRow] = field(default_factory=dict)
    aliases: Mapping[tuple[str, str], RowKey] = field(default_factory=dict)
    channels: Mapping[str, ConnectionChannel] = field(default_factory=dict)
    queue_item_keys: frozenset[RowKey] = frozenset()
    answers_in_flight: Mapping[AnswerKey, RowKey] = field(default_factory=dict)
    #: When each in-flight answer went on the wire, on the frame clock (U6).
    #:
    #: Kept beside ``answers_in_flight`` rather than inside it because the two
    #: are re-anchored differently and only one of them is re-anchored at all:
    #: an :data:`AnswerKey` never moves — it is the identity of a *call* — while
    #: the row key that call protects moves whenever its row learns a durable id.
    #: R18's requested-with-age reads this; an answer recorded without a stamp
    #: renders as requested with the age unobserved, never as zero seconds old.
    answer_started_at: Mapping[AnswerKey, float] = field(default_factory=dict)
    #: Per-connection approval detail from the KTD11-gated ``approval.pending``
    #: poll, newest reply per row, first-sighting stamps preserved (KTD12).
    approval_detail: Mapping[RowKey, tuple[PolledApproval, ...]] = field(
        default_factory=dict
    )
    #: One seam board per connection (U5's type, U6's plurality — see
    #: :func:`fleet_seam_board`). A capability is a property of the install a
    #: connection reached, and a fleet dials more than one install.
    seam_boards: Mapping[str, SeamBoard] = field(default_factory=dict)
    #: Queue items whose outcome came back ambiguous and was latched. Bounded,
    #: newest last; see :func:`settle_queue_item` for why a tombstone is the
    #: right shape, which key shapes it latches exactly, and why the queue
    #: ignores a tombstone on the constant roster key.
    settled_items: tuple[ItemKey, ...] = ()
    clock: float = 0.0

    @property
    def answering_keys(self) -> frozenset[RowKey]:
        """Rows with an answer on the wire right now, as registry keys.

        Derived from ``answers_in_flight`` rather than stored beside it: two
        copies of one fact would be two things to re-anchor at a rebind, and
        the second one is exactly what nobody would remember to write.
        """
        return frozenset(self.answers_in_flight.values())

    def focused_key(self) -> RowKey | None:
        """The focused cursor's registry key, ``None`` before any adoption."""
        durable = self.focused.session_key or self.focused.focused_session_id
        if durable is None:
            return None
        alias = self.aliases.get((self.focused_profile, durable))
        return alias if alias is not None else (self.focused_profile, durable)

    def protected_keys(self) -> frozenset[RowKey]:
        """Rows the bound and retirement may never remove: the focused row,
        rows holding queue items, rows with in-flight answers (PC2/KTD2).

        **Protection has two mechanisms, and both are load-bearing.** Read them
        together; each alone was demonstrably insufficient.

        1. :func:`_rebind_durable` *re-anchors* these sets when it moves a row.
           That is the primary guarantee, and it is complete because a rebind is
           the only place a row's key ever changes.
        2. This method also *resolves* each key through the alias map, as the
           focused cursor always did. That covers a key recorded against an
           alias that has not yet been trimmed, and it costs nothing.

        The history is worth keeping, because the obvious simplification here is
        fatal. Resolution came first, replacing a raw membership test that let a
        queue item's row be retired: a row created by an event is keyed by its
        runtime id, the first poll that learns its ``session_key`` rebinds it,
        and a protection recorded before that moment named a key no row answered
        to. Resolution fixed that — and then broke against the *other* fix of
        the same round, because the alias index is deliberately bounded: once a
        protected runtime id aged out of the row's window, the trim reclaimed
        the only entry that led to the live row, and the row became retirable
        again. Two correct mechanisms, each undoing the other.

        So re-anchoring is not redundant with resolution, and deleting it
        because this docstring once argued for resolution alone would reopen the
        defect. The test that catches it needs a rebind **plus** enough resumes
        to age the original id out; a single-rebind test passes without the
        re-anchoring and proves nothing.

        **Contract for units that record protection** (U4's answering
        bookkeeping, anything that records a key by hand): record the row's
        *registry key* as resolved at record time, not a bare runtime id you
        happen to hold. A key that is merely an alias of an already-durable-keyed
        row is re-anchored by nothing — no rebind ever moves it — so it loses
        protection when that alias trims.

        **U6's queue protects its rows by derivation rather than by record**, and
        that is the one exception to the contract above rather than a violation
        of it. The queue is not stored — :func:`fleet_queue` rebuilds it from the
        rows, the prompt registry and the polled detail on every read — so there
        is no moment at which a key could be recorded, no copy to keep in step,
        and nothing to re-anchor: an item follows its row through a rebind
        because it is *made of* that row. ``queue_item_keys`` stays for callers
        that must protect a row for a reason the queue cannot see, and the two
        sets are unioned rather than one replacing the other.
        """
        resolved = {self._resolve_protection(key) for key in self.queue_item_keys}
        resolved |= {self._resolve_protection(key) for key in self.answering_keys}
        resolved |= {item.row_key for item in fleet_queue(self).items}
        focused = self.focused_key()
        if focused is not None:
            resolved.add(focused)
        return frozenset(resolved)

    def _resolve_protection(self, key: RowKey) -> RowKey:
        """Where ``key`` points now, following one alias hop if it moved."""
        return self.aliases.get(key, key)

    def channel(self, profile: str) -> ConnectionChannel:
        return self.channels.get(profile) or ConnectionChannel(profile=profile)


def _with_channel(fleet: FleetState, channel: ConnectionChannel) -> FleetState:
    channels = dict(fleet.channels)
    channels[channel.profile] = channel
    return replace(fleet, channels=channels)


def _with_row(fleet: FleetState, key: RowKey, row: RegistryRow) -> FleetState:
    rows = dict(fleet.rows)
    rows[key] = row
    return replace(fleet, rows=rows)


def _bind_alias(fleet: FleetState, profile: str, runtime_id: str, key: RowKey) -> FleetState:
    """Point a runtime id at a row, keeping both sides of the alias bounded.

    The row keeps its newest ``MAX_RUNTIME_ALIASES`` ids; an id the row drops
    is also dropped from the fleet map, so a churning session cannot grow the
    alias index without bound (R3's fixed-size discipline, applied fleet-wide).
    """
    row = fleet.rows[key]
    if runtime_id != key[1] or runtime_id in row.runtime_ids:
        ids = tuple(i for i in row.runtime_ids if i != runtime_id) + (runtime_id,)
        kept = ids[-MAX_RUNTIME_ALIASES:]
        dropped = set(ids) - set(kept)
        aliases = {
            pair: target
            for pair, target in fleet.aliases.items()
            if not (pair[0] == profile and pair[1] in dropped and target == key)
        }
        aliases[(profile, runtime_id)] = key
        fleet = replace(_with_row(fleet, key, replace(row, runtime_ids=kept)), aliases=aliases)
        return fleet
    aliases = dict(fleet.aliases)
    aliases[(profile, runtime_id)] = key
    return replace(fleet, aliases=aliases)


def _resolve_key(fleet: FleetState, profile: str, session_id: str) -> RowKey | None:
    alias = fleet.aliases.get((profile, session_id))
    if alias is not None and alias in fleet.rows:
        return alias
    direct = (profile, session_id)
    if direct in fleet.rows:
        return direct
    return None


def _rebind_durable(fleet: FleetState, old_key: RowKey, durable_id: str) -> FleetState:
    """Move a row to its learned durable key (KTD3: runtime ids never the key).

    If a row already sits at the durable key — a listing-seeded row meeting
    its event-created runtime twin — the two merge: the moved row's observed
    fields win (it is the one events fed), listing markers and aliases union,
    and the earliest seeding is kept.
    """
    profile = old_key[0]
    new_key: RowKey = (profile, durable_id)
    if new_key == old_key or old_key not in fleet.rows:
        return fleet
    moved = fleet.rows[old_key]
    existing = fleet.rows.get(new_key)
    merged = replace(
        moved,
        durable_id=durable_id,
        runtime_ids=moved.runtime_ids,
    )
    if existing is not None:
        merged = replace(
            merged,
            title=merged.title or existing.title,
            model=merged.model or existing.model,
            message_count=max(merged.message_count, existing.message_count),
            seeded_at=(
                min(merged.seeded_at, existing.seeded_at)
                if merged.seeded_at and existing.seeded_at
                else (merged.seeded_at or existing.seeded_at)
            ),
            listed=merged.listed or existing.listed,
            live_listed=merged.live_listed or existing.live_listed,
            listing_epoch=max(merged.listing_epoch, existing.listing_epoch),
            active_epoch=max(merged.active_epoch, existing.active_epoch),
            runtime_ids=tuple(
                dict.fromkeys((*existing.runtime_ids, *merged.runtime_ids))
            )[-MAX_RUNTIME_ALIASES:],
        )
    rows = {k: v for k, v in fleet.rows.items() if k != old_key}
    rows[new_key] = merged
    # Ids the merge trimmed out of ``runtime_ids`` must lose their alias entries
    # too. ``_bind_alias`` cleans up the ids *it* pushes out, but it derives the
    # dropped set from the row's current ``runtime_ids`` — so an id this merge
    # trimmed is invisible to it forever after. That was an unbounded leak on
    # the most ordinary churn there is: fifty resumes of one session left fifty
    # permanent alias entries pointing at a row holding four runtime ids.
    trimmed = (
        set(moved.runtime_ids) | set(existing.runtime_ids if existing else ())
    ) - set(merged.runtime_ids)
    aliases = {
        pair: (new_key if target == old_key else target)
        for pair, target in fleet.aliases.items()
        if not (
            pair[0] == profile
            and pair[1] in trimmed
            and target in (old_key, new_key)
        )
    }
    # Protection is *re-anchored* here, not merely resolvable through the alias
    # map at read time. Resolving alone was not enough, and the way it failed is
    # worth keeping: the alias index is deliberately bounded, so once a protected
    # runtime id aged out of the row's four-id window the trim deleted the only
    # entry that led from the recorded key to the live row — and the row a queue
    # item still referenced became retirable again. Two correct mechanisms, each
    # undoing the other.
    #
    # A rebind is the one place a row's key ever changes, so rewriting the
    # protection sets here is complete rather than partial, and it holds across
    # arbitrarily many rebinds because each one re-anchors again. The read-time
    # resolution stays as well: it costs nothing and it covers a key recorded
    # against an alias that has not yet been trimmed.
    def _moved(keys: frozenset[RowKey]) -> frozenset[RowKey]:
        return frozenset(new_key if key == old_key else key for key in keys)

    # Feed B's approval detail is keyed by row, so it moves with the row for the
    # identical reason. Detail left behind at the old key would strand every
    # polled approval of a session the moment its first poll taught the registry
    # its durable id — which is the very first poll after the row was created by
    # an event, i.e. the ordinary case rather than an edge one. Merging keeps the
    # earlier first-sighting stamps, so the observation floor does not jump
    # forward when a row is re-keyed (KTD12).
    detail = {
        key: value for key, value in fleet.approval_detail.items() if key != old_key
    }
    if old_key in fleet.approval_detail:
        detail[new_key] = merge_polled_approvals(
            fleet.approval_detail.get(new_key, ()), fleet.approval_detail[old_key]
        )

    return replace(
        fleet,
        rows=rows,
        aliases=aliases,
        approval_detail=detail,
        queue_item_keys=_moved(fleet.queue_item_keys),
        # The answering half is re-anchored here too, and for the identical
        # reason — it is a mapping only so that one entry can carry both the
        # answer's own identity and the row key the protection is about.
        answers_in_flight={
            answer: (new_key if target == old_key else target)
            for answer, target in fleet.answers_in_flight.items()
        },
        # And the settled tombstones, which are keyed by session rather than by
        # row and were therefore missed when this list was first written. A
        # settle recorded before the row's first poll was undone BY that poll:
        # the tombstone kept naming the runtime id while the item it was meant
        # to suppress was rebuilt under the durable one, so a latched approval
        # came back and was offered again. "Settles and latches, never restores"
        # has to survive the ordinary case, and a rebind on the first poll after
        # an event-created row is the ordinary case.
        settled_items=tuple(
            (profile, new_key[1] if (profile, session) == old_key else session, request)
            for profile, session, request in fleet.settled_items
        ),
    )


def _fresh_observation(
    row: RegistryRow, *, at: float, source: ObservationSource
) -> RegistryRow:
    """Fold one current-source observation: the row is live again (R20 — a
    fresh observation is exactly what clears stale-since)."""
    return replace(
        row,
        observed=True,
        last_event_at=at,
        last_event_source=source,
        stale_since=None,
        disconnected=False,
        reclaimed_reason=None,
    )


def _apply_event_to_row(
    fleet: FleetState,
    profile: str,
    decoded: GatewayEvent | UnknownEventFrame,
    *,
    stale_generation: bool,
) -> FleetState:
    """Route one identified event to its registry row, creating it if unknown (R4)."""
    session_id = decoded.session_id
    if session_id is None:  # callers routed identity-less traffic away already
        return _count_identityless(fleet, profile, f"identity-less event: {decoded.type}")
    key = _resolve_key(fleet, profile, session_id)
    if key is None:
        key = (profile, session_id)
        fleet = _with_row(
            fleet,
            key,
            RegistryRow(
                profile=profile,
                durable_id=session_id,
                runtime_ids=(session_id,),
                seeded_at=decoded.at,
            ),
        )
        aliases = dict(fleet.aliases)
        aliases[(profile, session_id)] = key
        fleet = replace(fleet, aliases=aliases)
        fleet = _enforce_row_bound(fleet, profile)
        if key not in fleet.rows:
            # The bound evicted the row it just created (a cap of protected
            # rows only). The eviction was counted and is visible; stop here.
            return fleet
    else:
        fleet = _bind_alias(fleet, profile, session_id, key)

    row = fleet.rows[key]
    if stale_generation:
        # A frame from a connection generation ``ensure`` has since replaced
        # (U2's finding: (profile, epoch) is not unique across a rebuild). The
        # observation is real — the gateway did emit it — but it must not
        # un-stale a row whose current source is gone or replaced.
        row = replace(
            row,
            observed=True,
            last_event_at=max(row.last_event_at, decoded.at),
            last_event_source="event",
        )
    else:
        row = _fresh_observation(row, at=decoded.at, source="event")

    if isinstance(decoded, GatewayEvent):
        etype = decoded.type
        payload = decoded.payload
        if etype in _PROMPT_EVENTS:
            kind = _PROMPT_EVENTS[etype]
            row = replace(row, waiting_kind=kind)
            if kind not in QUEUEABLE_KINDS:
                # R14's whole bargain for a known prompt kind the queue
                # suppresses — today ``terminal_read`` alone, checked by the
                # ``QUEUEABLE_KINDS`` membership rather than a name so a fifth
                # kind inherits it. The naming must happen HERE because no other
                # writer is reachable for a background session: the focused
                # engine's cross-session guard (``apply_frame``) drops the event
                # before a prompt registers, so ``latch_unservable_prompt``
                # never runs, and ``_name_unrenderable_wait`` runs only for
                # unknown event types. Round nine's defect was this branch
                # setting the kind and writing no notice: the queue suppressed
                # the roster wait and nothing anywhere said so. Pinned by the
                # ``terminal_read`` leg of
                # ``test_an_unrenderable_kind_is_named_on_its_row_and_never_queued``.
                row = replace(row, last_notice=unresolvable_kind_notice(kind))
        elif etype in _EXPIRE_EVENTS:
            row = replace(
                row,
                waiting_kind="",
                last_notice=clip_detail_line(f"{_EXPIRE_EVENTS[etype]} prompt expired"),
            )
        elif etype == "message.start":
            row = replace(row, open_turn=True, waiting_kind="")
        elif etype in ("message.complete", "error"):
            row = replace(row, open_turn=False)
        elif etype == "session.title":
            row = replace(row, title=coerce_text(payload.get("title")) or row.title)
        elif etype == "session.info":
            row = replace(row, title=coerce_text(payload.get("title")) or row.title)
            durable = coerce_text(payload.get("stored_session_id"))
            fleet = _with_row(fleet, key, row)
            if durable and durable != key[1]:
                return _rebind_durable(fleet, key, durable)
            return fleet
        elif etype == "session.reclaimed":
            # Retirement, KTD10: the row latches ``reclaimed(reason)`` and
            # shows stale-since the reap — never a silent removal (the
            # dual-listing rule, not this event, is what may drop the row).
            reason = coerce_text(payload.get("reason")) or "reclaimed"
            row = replace(
                row,
                reclaimed_reason=clip_detail_line(reason),
                stale_since=decoded.at,
                live_listed=False,
                open_turn=False,
            )
    else:
        # An UnknownEventFrame updates the row as a generic observation — AE1's
        # unknown-kind half: the row moved, nothing dropped — and, when the type
        # names a blocking bridge Talaria renders no card for, says so on the row.
        row = _name_unrenderable_wait(row, decoded.type)
    return _with_row(fleet, key, row)


def _name_unrenderable_wait(row: RegistryRow, etype: str) -> RegistryRow:
    """Name a blocking kind Talaria has no card for on its row (KTD2/R14).

    The running gateway blocks on three such kinds — ``preview.read``,
    ``window.read`` and ``mcp.setup``, whose respond bridges are registered at
    ``methods_prompt.py:1412``, ``:1420`` and ``:1429`` — and the pinned read has
    none of them, so their request events arrive as unknown types. They are
    **named on the row and never queued**: the queue holds only resolvable items
    (R17), and a row nobody can act on is a dead end wearing the clothes of a
    task.

    The rule is "an unknown ``<kind>.request``", not a list of three, so a fourth
    kind a later gateway grows is named on the day it arrives rather than on the
    day somebody edits a tuple. Its matching ``.expire`` clears the wait, which
    is the only other thing this row can honestly learn about a bridge it cannot
    render.

    **The kind word is the event type's own stem, and it is inferred rather than
    verified.** U1 induced none of these three — they are desktop-GUI tools the
    throwaway agent had no reason to call — so the request-event names are read
    off the respond-bridge names by the convention ``terminal.read.request``
    already follows. If a gateway spells one differently, the row still names
    whatever stem did arrive, which is the honest degradation.
    """
    if etype.endswith(".request"):
        kind = etype[: -len(".request")]
        return replace(row, waiting_kind=kind, last_notice=unresolvable_kind_notice(kind))
    if etype.endswith(".expire"):
        kind = etype[: -len(".expire")]
        if row.waiting_kind and row.waiting_kind != kind:
            return row
        return replace(
            row,
            waiting_kind="",
            last_notice=clip_detail_line(f"{kind} prompt expired unanswered"),
        )
    return row


def _count_identityless(fleet: FleetState, profile: str, notice: str) -> FleetState:
    """R25: surface on the connection channel, count, touch no row."""
    return _with_channel(fleet, note_identityless(fleet.channel(profile), notice))


def _feed_focused(
    fleet: FleetState, decoded: DecodedFrame, *, stale_generation: bool = False
) -> FleetState:
    """Feed the focused engine exactly as today, then mirror its row summary.

    The mirror only writes once a focused session exists to key a row by, so
    pre-adoption traffic creates no row (R25's "creates no row" holds
    structurally for the focused connection too).

    ``stale_generation`` governs the **row mirror only**, never the engine feed.
    A frame from a socket since replaced is still a real frame the focused
    transcript must show — R2 governs the transcript, and changing what the
    engine sees is exactly what this unit promised not to do. But it is not
    evidence that the *connection* is live again, and the row's freshness is a
    claim about the connection. Letting it through un-staled a focused row the
    moment a late frame from a dead socket arrived, while the background rows it
    shared a connection with correctly stayed stale — the same row, two
    contradictory answers.
    """
    fleet = replace(fleet, focused=apply_frame(fleet.focused, decoded))
    key = fleet.focused_key()
    if key is None:
        return fleet
    runtime = fleet.focused.focused_session_id
    row = fleet.rows.get(key)
    if row is None:
        row = RegistryRow(profile=key[0], durable_id=key[1], seeded_at=decoded.at)
        fleet = _with_row(fleet, key, row)
    if runtime is not None:
        fleet = _bind_alias(fleet, key[0], runtime, key)
        row = fleet.rows[key]
    if not stale_generation:
        row = _fresh_observation(row, at=decoded.at, source="event")
    row = replace(
        row,
        ownership="we_drive",
        open_turn=fleet.focused.turn == "streaming",
        title=fleet.focused.session_title or row.title,
    )
    fleet = _with_row(fleet, key, row)
    durable = fleet.focused.session_key
    if durable and durable != key[1]:
        fleet = _rebind_durable(fleet, key, durable)
    return fleet


def _is_focused_traffic(fleet: FleetState, session_id: str) -> bool:
    """Whether an identified event on the focused connection belongs to the
    focused engine. Matches the engine's own guard: the exact runtime id, or
    the adoption case (no session adopted yet — the engine adopts the first
    session named on the wire, exactly as replay always has)."""
    focused_id = fleet.focused.focused_session_id
    return focused_id is None or session_id == focused_id


def route_frame(
    fleet: FleetState,
    decoded: DecodedFrame,
    *,
    profile: str,
    generation: int = 0,
) -> FleetState:
    """Fold one connection-tagged frame into the fleet (U3's router, KTD3).

    Replaces the cross-talk *discard* one level above the focused engine:

    * focused-session traffic on the focused connection feeds the engine
      exactly as today, then mirrors the focused row's summary;
    * an identified foreign event updates its row, creating it when the
      session is unknown (R4) — the focused transcript does not change and
      nothing is counted as ignored;
    * an identity-less frame — a protocol error, or a session-scoped event
      with no usable session id — surfaces on the connection channel, is
      counted, and creates no row (R25). ``gateway.*`` and the broadcast
      types are connection traffic by contract, not identity-less defects.
      (The third R25 clause — an id that conflicts with what the connection
      can own — cannot arise under ``(profile, durable_id)`` keys: every id
      is scoped to the connection that observed it, so there is nothing to
      conflict with; noted here so the omission reads as considered.)

    ``generation`` is the transport's per-profile connection generation
    (U2). Frames from a superseded generation still count as observations
    but never clear a row's stale-since — the epoch alone cannot distinguish
    connection identities across a reconnect-by-ensure.

    Nothing here maintains the needs-you queue, and that is U6's design rather
    than an omission: the queue is derived from the rows this router writes, so
    an event that changes what needs a person changes the queue in the same
    reduction, with no second structure to keep in step.
    """
    fleet = replace(fleet, clock=max(fleet.clock, decoded.at))
    channel = fleet.channel(profile)
    if generation > channel.generation:
        channel = replace(channel, generation=generation)
        fleet = _with_channel(fleet, channel)
    stale_generation = generation < channel.generation
    on_focused_connection = profile == fleet.focused_profile

    if isinstance(decoded, NonEventFrame):
        if on_focused_connection:
            return replace(fleet, focused=apply_frame(fleet.focused, decoded))
        return fleet

    if isinstance(decoded, ProtocolErrorFrame):
        fleet = _count_identityless(fleet, profile, decoded.text)
        if on_focused_connection:
            # The session-less protocol error already takes the v0.1 R5 path
            # through the focused transcript; that rendering is unchanged.
            return replace(fleet, focused=apply_frame(fleet.focused, decoded))
        return fleet

    etype = decoded.type
    session_id = decoded.session_id

    if etype.startswith("gateway.") or etype in CONNECTION_BROADCAST_EVENT_TYPES:
        if etype == "sessions.changed":
            fleet = _with_channel(fleet, replace(fleet.channel(profile), hint_at=decoded.at))
        if on_focused_connection:
            return replace(fleet, focused=apply_frame(fleet.focused, decoded))
        return fleet

    if session_id is None:
        # Identity-less session-scoped traffic: counted and surfaced on the
        # channel (R25). On the focused connection the engine still sees the
        # frame — its guard passes session-less events, and that shipped
        # behavior is R2's to keep, not this router's to change.
        fleet = _count_identityless(fleet, profile, f"identity-less event: {etype}")
        if on_focused_connection:
            return _feed_focused(fleet, decoded, stale_generation=stale_generation)
        return fleet

    if on_focused_connection and _is_focused_traffic(fleet, session_id):
        return _feed_focused(fleet, decoded, stale_generation=stale_generation)

    return _apply_event_to_row(fleet, profile, decoded, stale_generation=stale_generation)


def route_frames(
    fleet: FleetState,
    frames: Iterable[DecodedFrame],
    *,
    profile: str,
    generation: int = 0,
) -> FleetState:
    for decoded in frames:
        fleet = route_frame(fleet, decoded, profile=profile, generation=generation)
    return fleet


# ── Seeding, polling, retirement (KTD2) ──────────────────────────────────


def seed_from_listing(
    fleet: FleetState,
    directory: SessionDirectory,
    *,
    profile: str,
    at: float,
    poll_epoch: int,
) -> FleetState:
    """Fold one successful ``session.list`` sweep in (R5).

    A listing row's id is the stored — durable — id, so it keys directly.
    A row this sweep creates enters as **never-observed**: a listing proves
    existence, not lifecycle, and R24 forbids rendering that as ``idle``.
    Rows of this profile absent from the sweep lose their ``listed`` marker;
    the dual-listing retirement below decides whether they drop.
    """
    fleet = replace(fleet, clock=max(fleet.clock, at))
    seen: set[RowKey] = set()
    for summary in directory.sessions:
        key: RowKey = (profile, summary.session_id)
        existing = _resolve_key(fleet, profile, summary.session_id)
        if existing is not None:
            key = existing
        seen.add(key)
        row = fleet.rows.get(key) or RegistryRow(
            profile=profile, durable_id=key[1], seeded_at=at
        )
        row = replace(
            row,
            title=summary.title or row.title,
            message_count=max(row.message_count, summary.message_count),
            listed=True,
            listing_epoch=poll_epoch,
        )
        fleet = _with_row(fleet, key, row)
    for key, row in list(fleet.rows.items()):
        if key[0] == profile and key not in seen and row.listed:
            fleet = _with_row(fleet, key, replace(row, listed=False))
    channel = replace(
        fleet.channel(profile), listing_epoch=poll_epoch, listing_stale_since=None
    )
    fleet = _with_channel(fleet, channel)
    fleet = _retire_absent_rows(fleet, profile)
    return _enforce_row_bound(fleet, profile)


def apply_active_list(
    fleet: FleetState,
    directory: ActiveSessionDirectory,
    *,
    profile: str,
    at: float,
    poll_epoch: int,
    polled_at: float | None = None,
) -> FleetState:
    """Fold one successful ``session.active_list`` poll in (KTD2's live feed).

    Every reported row is a lifecycle-confirming observation: status is
    stored verbatim (KTD10), the observation floor advances only when the
    status word changes (KTD12 — "waiting ≥ observed span" measures from the
    first poll that saw the wait), and a ``waiting`` row whose kind no event
    has named carries the flattened ``unobserved`` kind (the gateway exposes
    nothing finer for sessions other clients drive).

    **Two stamps, because they are two clocks.** ``at`` is the frame clock and
    stamps everything an operator reads as an age — R20 and KTD12 both rest on a
    replayed recording reproducing those exactly. ``polled_at`` stamps only
    ``last_poll_at``, which nothing reads as an age: its single consumer is
    :func:`~talaria.domain.registry.next_poll_due_at`, the poll schedule.

    They were one parameter until 2026-08-19, and the conflation made KTD2's
    30-second backstop unable to fire. The frame clock advances on focused
    traffic only — ``route_frame`` sends everything else to its registry row —
    so a *background* connection's schedule was driven entirely by traffic on the
    connection it is not. Measured: sixty frames spanning sixty seconds of frame
    time on a background connection left the focused clock at 0.0 and that
    connection's poll due at 130.0, while the registry learned a row from the
    same frames. A fleet quiet everywhere never polled at all, which is when a
    poll is the only way to learn anything.

    ``polled_at`` defaults to ``at`` so every existing caller keeps its current
    behaviour, and the live poll loop passes the wall clock. That is honest
    rather than expedient: the loop is live-only (replay has no gateway to poll),
    so no determinism claim touches this stamp.
    """
    if polled_at is None:
        polled_at = at
    fleet = replace(fleet, clock=max(fleet.clock, at))
    seen: set[RowKey] = set()
    for active in directory.sessions:
        durable = active.durable_id
        key: RowKey = (profile, durable)
        existing = _resolve_key(fleet, profile, active.session_id)
        if existing is not None and existing != key:
            fleet = _rebind_durable(fleet, existing, durable)
        row = fleet.rows.get(key) or RegistryRow(
            profile=profile, durable_id=durable, seeded_at=at
        )
        if key not in fleet.rows:
            fleet = _with_row(fleet, key, row)
        fleet = _bind_alias(fleet, profile, active.session_id, key)
        row = fleet.rows[key]
        seen.add(key)
        status_changed = active.status != row.status or not row.observed
        # Captured before ``_fresh_observation`` clears it: whether the stream
        # that named ``waiting_kind`` broke since the naming. The clear for an
        # event-named kind is itself an event (``*.expire``, or the answered
        # bridge's turn moving on), emitted once, to the session's transport of
        # that moment (``tui_gateway/server.py``: ``_block``'s expire emission,
        # ``write_json``'s session-transport routing) — a connection dropped
        # across it never sees it, and nothing replays it.
        stream_broke = row.disconnected
        row = _fresh_observation(row, at=at, source="poll")
        waiting_kind = row.waiting_kind
        if active.status == "waiting":
            if waiting_kind and stream_broke:
                # Round nine's stale-kind finding, the reachable half: a kind
                # named before a drop would otherwise ride the row into every
                # later wait — for a suppressed kind (``terminal_read``) that
                # silently starves the queue of this row's waits until a
                # ``message.start`` Talaria may never receive. After a break the
                # honest claim is exactly the poll's own: the session waits, on
                # a kind no surviving stream has named — ``unobserved``, the
                # word this module already uses for that claim. If the named
                # prompt is in fact still live, attaching either re-announces it
                # (approval and clarify, the only kinds the gateway replays —
                # ``tui_gateway/server.py:8782-8784`` — the F6 path) or latches
                # the attach-residue notice for it. The break itself reaches the
                # row through ``fleet_connection_lost``, which
                # ``TalariaApp.note_connection_state`` calls on every disconnect
                # — round ten found this rule reading a flag no production path
                # ever set. Pinned by
                # ``test_a_kind_named_before_a_connection_drop_does_not_suppress_a_later_wait``
                # and, for the wiring,
                # ``test_a_dropped_connection_marks_the_fleet_rows_not_just_the_focused_session``.
                waiting_kind = UNOBSERVED_KIND
            waiting_kind = waiting_kind or UNOBSERVED_KIND
        else:
            # The other reachable half: an ANSWERED terminal read emits no
            # ``.expire`` (the gateway's expire fires only on timeout), so
            # nothing event-shaped ends the named wait. A fresh poll's status is
            # the gateway's own lifecycle word — a row polled ``working`` or
            # ``idle`` is waiting on nothing, so whatever kind an event once
            # named is over and clears, ``unobserved`` included. Pinned by
            # ``test_a_poll_reporting_any_other_status_ends_the_named_wait``.
            waiting_kind = ""
        row = replace(
            row,
            status=active.status,
            status_floor_at=at if status_changed else row.status_floor_at,
            waiting_kind=waiting_kind,
            title=active.title or row.title,
            model=active.model or row.model,
            message_count=max(row.message_count, active.message_count),
            live_listed=True,
            active_epoch=poll_epoch,
        )
        fleet = _with_row(fleet, key, row)
        if row.status not in APPROVAL_DETAIL_TRIGGER_STATUSES:
            # The only clearing path a polled approval has. Detail is refreshed
            # ONLY at a waiting-or-working row (``approval_detail_due``) and
            # cleared ONLY by a refresh (``merge_polled_approvals``), so a row
            # that left those statuses holding detail could never be refreshed
            # and therefore never shown to have resolved: its approvals stayed
            # in the queue, answerable, ageing without bound, for the life of
            # the run. That is a queue that lies, which is the failure R14
            # exists to prevent.
            #
            # Clearing is the honest reading rather than a convenience. These
            # are the gateway's own lifecycle words, and an approval-blocked
            # session reports ``working`` — approvals ride ``tools.approval``'s
            # registry, not ``_block()`` — so a session the gateway calls
            # ``idle`` or ``starting`` is holding no approval. It is the same
            # principle ``merge_polled_approvals`` already applies to a fresh
            # reply that omits an approval: absence from current evidence is
            # resolution. The queue is derived, so the item goes in the same
            # reduction (R19).
            #
            # A DISCONNECTED row is deliberately not cleared here: there the
            # evidence is missing rather than negative, and dropping a real
            # approval silently is the other half of R14's bargain. The queue
            # refuses those instead — see ``APPROVAL_ON_DOWN_CONNECTION``.
            # Pinned by
            # ``test_a_poll_outside_the_trigger_statuses_clears_stale_approval_detail``.
            fleet = replace(fleet, approval_detail=_drop_detail(fleet, {key}))
    for key, row in list(fleet.rows.items()):
        if key[0] == profile and key not in seen and row.live_listed:
            fleet = _with_row(fleet, key, replace(row, live_listed=False))
    # A poll that answered is evidence the connection is up, and it is the only
    # such evidence the fleet ever records. ``connected`` defaults to False and
    # was written by nothing but ``fleet_connection_restored``, a reconnect
    # reducer with no caller — so every successfully polled connection described
    # itself as down. Nothing read that field until U6's queue notices did, at
    # which point a healthy fleet started reporting "part of the fleet could not
    # be asked" forever. Setting it here is not an assumption: this line is only
    # reached because a roster reply came back.
    channel = replace(
        fleet.channel(profile),
        active_epoch=poll_epoch,
        # The SCHEDULE clock, not the frame clock — see this function's own
        # docstring for the measurement that separated them.
        last_poll_at=polled_at,
        hint_at=None,
        connected=True,
    )
    fleet = _with_channel(fleet, channel)
    fleet = _retire_absent_rows(fleet, profile)
    return _enforce_row_bound(fleet, profile)


def listing_failed(fleet: FleetState, *, profile: str, at: float) -> FleetState:
    """A refused or failed ``session.list``: listing-derived fields are marked
    stale from this moment — marked, never cleared (R5/R20)."""
    fleet = replace(fleet, clock=max(fleet.clock, at))
    channel = fleet.channel(profile)
    if channel.listing_stale_since is None:
        channel = replace(channel, listing_stale_since=at)
    return _with_channel(fleet, channel)


def _retire_absent_rows(fleet: FleetState, profile: str) -> FleetState:
    """The dual-listing retirement rule (KTD2/U3).

    A row is dropped only when a successful ``session.list`` and
    ``session.active_list`` of the **same epoch** both failed to mention it,
    and it is not focused, holds no queue item, and has no in-flight answer.
    Live-only rows (in the active list, not the historical listing) are kept;
    ``session.reclaimed`` and connection loss mark stale-since and never
    remove.
    """
    channel = fleet.channel(profile)
    if channel.listing_epoch < 0 or channel.listing_epoch != channel.active_epoch:
        return fleet
    protected = fleet.protected_keys()
    doomed = [
        key
        for key, row in fleet.rows.items()
        if key[0] == profile
        and not row.listed
        and not row.live_listed
        and key not in protected
    ]
    if not doomed:
        return fleet
    rows = {k: v for k, v in fleet.rows.items() if k not in doomed}
    doomed_set = set(doomed)
    aliases = {p: t for p, t in fleet.aliases.items() if t not in doomed_set}
    return replace(
        fleet, rows=rows, aliases=aliases, approval_detail=_drop_detail(fleet, doomed_set)
    )


def _drop_detail(
    fleet: FleetState, doomed: set[RowKey]
) -> Mapping[RowKey, tuple[PolledApproval, ...]]:
    """Forget the polled approval detail of rows that just left the registry.

    A row is a fixed-size summary and its detail is part of that size (R3). A
    retired or evicted row keeping its approvals would leave the one unbounded
    structure in the fleet pointing at sessions nothing else remembers.
    """
    if not doomed:
        return fleet.approval_detail
    return {key: value for key, value in fleet.approval_detail.items() if key not in doomed}


def _enforce_row_bound(fleet: FleetState, profile: str) -> FleetState:
    """The 256-rows-per-connection memory bound (PC2).

    Applies to unprotected rows only; eviction is oldest-first by last
    observation; every eviction increments the channel's visible truncation
    count — never a silent drop. Protected rows may exceed the cap outright.
    """
    keys = [key for key in fleet.rows if key[0] == profile]
    overflow = len(keys) - ROW_CAP_PER_CONNECTION
    if overflow <= 0:
        return fleet
    protected = fleet.protected_keys()
    evictable = sorted(
        (key for key in keys if key not in protected),
        key=lambda key: (fleet.rows[key].last_seen_at(), key),
    )
    doomed = set(evictable[:overflow])
    if not doomed:
        return fleet
    rows = {k: v for k, v in fleet.rows.items() if k not in doomed}
    aliases = {p: t for p, t in fleet.aliases.items() if t not in doomed}
    channel = fleet.channel(profile)
    channel = replace(channel, evicted_rows=channel.evicted_rows + len(doomed))
    fleet = replace(
        fleet, rows=rows, aliases=aliases, approval_detail=_drop_detail(fleet, doomed)
    )
    return _with_channel(fleet, channel)


# ── Connection lifecycle at fleet level ──────────────────────────────────


def fleet_connection_lost(fleet: FleetState, *, profile: str, at: float) -> FleetState:
    """A dropped connection marks every row it fed stale-since, never
    frozen-fresh (AE4's first half at row level). Never-observed rows stay
    never-observed — stale-since-nothing is forbidden (R24)."""
    fleet = replace(fleet, clock=max(fleet.clock, at))
    for key, row in list(fleet.rows.items()):
        if key[0] != profile:
            continue
        stale_since = row.stale_since
        if row.observed and stale_since is None:
            stale_since = at
        fleet = _with_row(
            fleet, key, replace(row, disconnected=True, stale_since=stale_since)
        )
    return _with_channel(fleet, replace(fleet.channel(profile), connected=False))


def fleet_connection_restored(
    fleet: FleetState, *, profile: str, generation: int, at: float
) -> FleetState:
    """Reconnect: the channel is current again under a new generation, and
    rows stay stale until a fresh poll or event re-confirms each one — a
    reconnect proves the socket, not the sessions (R20)."""
    fleet = replace(fleet, clock=max(fleet.clock, at))
    channel = replace(
        fleet.channel(profile),
        connected=True,
        generation=max(generation, fleet.channel(profile).generation),
    )
    return _with_channel(fleet, channel)


def mark_we_drive(
    fleet: FleetState, *, profile: str, session_id: str, at: float
) -> FleetState:
    """Record ownership (KTD8): this run created, resumed, or activated the
    session. Creates the row if the session is not yet known — ownership is
    Talaria's own bookkeeping, not a gateway observation, so the row stays
    never-observed until something confirms its lifecycle."""
    fleet = replace(fleet, clock=max(fleet.clock, at))
    key = _resolve_key(fleet, profile, session_id)
    if key is None:
        key = (profile, session_id)
        fleet = _with_row(
            fleet,
            key,
            RegistryRow(profile=profile, durable_id=session_id, seeded_at=at),
        )
        aliases = dict(fleet.aliases)
        aliases[(profile, session_id)] = key
        fleet = replace(fleet, aliases=aliases)
    return _with_row(fleet, key, replace(fleet.rows[key], ownership="we_drive"))


# ── U4: fleet-scoped answering, the steal confirm, the attach handover ────


#: Why a switch is refused while an answer is travelling for some *other*
#: session than the focused one (R6/KTD9).
#:
#: Separate wording from :data:`REFUSED_SWITCH_WHILE_ANSWERING` because the two
#: name different situations and the operator can act on the difference: the
#: focused sentence is about the card in front of them, this one is about an
#: answer they sent from the queue into a session they are not looking at.
REFUSED_SWITCH_WHILE_FLEET_ANSWERING: Final[str] = (
    "an answer is still travelling to the gateway for another session — the "
    "session was not switched; try again in a moment"
)


def fleet_answer_key(profile: str, session_id: str, request_key: str) -> AnswerKey:
    """The fleet-scoped identity of one in-flight answer (KTD9)."""
    return (profile, session_id, request_key)


def begin_fleet_answer(
    fleet: FleetState,
    *,
    profile: str,
    session_id: str,
    request_key: str,
    at: float = 0.0,
) -> FleetState:
    """Record one answer as on the wire, protecting the row it names (KTD9).

    **The recorded value is the row's registry key as resolved right now**, per
    the contract :meth:`FleetState.protected_keys` states: an alias is followed
    to the row it points at, and a session with no row yet is recorded under the
    key a row for it would take. Both cases are then re-anchored by
    :func:`_rebind_durable` if the row ever learns a durable identity, which is
    the only moment a row's key changes.

    ``session_id`` should be the **durable** id whenever the caller holds one
    (``session_key``, or the id a listing named), for the reason that contract
    spells out: a runtime id that is merely an alias of an already-durable-keyed
    row is re-anchored by nothing, because no rebind ever moves it, and it
    silently loses protection when the alias ages out of the row's four-slot
    window. Passing the durable id makes the recorded key the row's own.

    Recording an answer against a session with no row does **not** invent one.
    A row is a claim that the gateway told Talaria about a session; an answer in
    flight is Talaria's own bookkeeping. The protection is simply inert until a
    row exists under that key, which is exactly what it should be.

    ``at`` is the frame-clock moment the call went out, and it is what R18's
    requested-with-age renders. It defaults to ``0.0`` — meaning "no stamp was
    recorded" — because a caller with no clock in hand (the domain suite, a
    replay-mode path) is telling the truth by saying so, and the queue renders
    such an item as requested with the age unobserved rather than as an answer
    sent at the epoch.
    """
    key = _resolve_key(fleet, profile, session_id) or (profile, session_id)
    answer = fleet_answer_key(profile, session_id, request_key)
    answers = dict(fleet.answers_in_flight)
    answers[answer] = key
    stamps = dict(fleet.answer_started_at)
    if at:
        stamps[answer] = at
    return replace(
        fleet,
        answers_in_flight=answers,
        answer_started_at=stamps,
        clock=max(fleet.clock, at),
    )


def end_fleet_answer(
    fleet: FleetState, *, profile: str, session_id: str, request_key: str
) -> FleetState:
    """Retire one in-flight answer — the round trip ended, however it ended.

    Removing the entry is what releases both the switch-refusal window and the
    row's protection, and a row still named by a *second* outstanding answer
    keeps its protection because the protection set is derived from what is
    left rather than from a counter that could drift.
    """
    answer = fleet_answer_key(profile, session_id, request_key)
    if answer not in fleet.answers_in_flight:
        return fleet
    answers = {k: v for k, v in fleet.answers_in_flight.items() if k != answer}
    stamps = {k: v for k, v in fleet.answer_started_at.items() if k != answer}
    return replace(fleet, answers_in_flight=answers, answer_started_at=stamps)


def fleet_switch_refusal(fleet: FleetState) -> str:
    """Why a session switch must not happen right now, fleet-wide, or ``""``.

    The focused engine's own rule first, verbatim
    (:func:`switch_refusal`) — that sentence is the shipped one and every
    surface that already says it goes on saying it. Then R6's fleet half: an
    answer travelling for *any* session on *any* connection occupies the same
    window, because the late outcome is applied to whatever state exists when
    the reply lands, and a switch inside that window moves the state out from
    under it. A queue answer aimed at a background session never enters the
    focused engine's ``answering`` tuple at all, so without this half it would
    occupy no window whatsoever.
    """
    focused = switch_refusal(fleet.focused)
    if focused:
        return focused
    if fleet.answers_in_flight:
        return REFUSED_SWITCH_WHILE_FLEET_ANSWERING
    return ""


def fleet_row(
    fleet: FleetState, *, profile: str, session_id: str
) -> RegistryRow | None:
    """The registry row a session id names on one connection, or ``None``.

    Public because callers outside the domain legitimately need to *read* a row
    before acting — and because the alternative, reaching into ``fleet.rows``
    with a raw tuple, skips the alias resolution and silently misses a row whose
    runtime id has been rebound to a durable key.
    """
    key = _resolve_key(fleet, profile, session_id)
    return fleet.rows[key] if key is not None else None


def roster_staleness(fleet: FleetState, profile: str) -> str:
    """How current this connection's lifecycle data is: current, stale, never.

    Derived, not stored. The two sweeps of one open share a poll epoch, and a
    failed ``session.active_list`` advances no active epoch — so a listing epoch
    ahead of the active epoch *is* the record of a roster that did not answer,
    and no separate flag can drift from it.

    Three answers because there are three situations and they are not the same
    sentence. ``never`` is a connection whose roster has never once answered:
    rows carry identity only. ``stale`` is a roster that answered before and did
    not this time: the rows still say what they last knew, which is data, but
    data of unknown age. ``current`` is the ordinary case. Reporting ``stale``
    as ``never`` was a real defect — the picker told the operator its rows
    showed identity only while displaying a lifecycle for every one of them.
    """
    channel = fleet.channel(profile)
    if channel.active_epoch < 0:
        return "never"
    if channel.listing_epoch > channel.active_epoch:
        return "stale"
    return "current"


def attach_confirm_row(
    fleet: FleetState, *, profile: str, session_id: str
) -> RegistryRow | None:
    """The row whose attach needs OP2's confirmation, or ``None`` (KTD8).

    The dialog fires for exactly one shape: a session the gateway currently
    reports **live** (present in the most recent ``session.active_list`` sweep,
    not reclaimed, not on a connection Talaria has lost) that this run did not
    create, resume or activate. Attaching that steals its event stream, and the
    displaced client is told nothing — U1 measured the silence at four ownership
    flips.

    A historical session — in ``session.list`` and not in ``active_list`` — is
    deliberately **not** confirmed: nothing live is stolen, and a dialog on
    every resume trains the operator to click through the one that matters.
    Neither is a row this run already drives, nor a session with no row at all
    (the roster is unavailable or the session is unknown): a confirmation whose
    premise Talaria cannot check would be a claim, and R24's rule is that an
    absence of observation is named rather than acted on as if it were a
    sighting.
    """
    row = fleet_row(fleet, profile=profile, session_id=session_id)
    return row if row is not None and attach_displaces_client(row) else None


#: Which reply field hydrates which prompt event on a confirmed attach.
#:
#: Exactly two, and the pair is the gateway's, not a choice: the shared
#: live-session payload builder behind ``session.create``, ``session.resume``
#: and ``session.activate`` adds ``pending_approval`` and ``pending_clarify``
#: when they exist (``tui_gateway/server.py:8708-8711`` at the pinned read) and
#: nothing for sudo, secret, or any other bridge. On the revision this machine
#: actually serves today it adds neither (U1's topology verification), so the
#: empty case is day-one behaviour rather than an edge case.
ACTIVATION_HYDRATION_FIELDS: Final[Mapping[str, str]] = {
    "pending_approval": "approval.request",
    "pending_clarify": "clarify.request",
}


def activation_hydration_events(
    result: Any, *, session_id: str, at: float, seq: int
) -> tuple[GatewayEvent, ...]:
    """Turn a landing reply's hydratable prompts into request events (KTD8).

    Returned as ordinary :class:`~talaria.domain.models.GatewayEvent` values so
    the caller folds them through :func:`apply_frame` — the same registration
    path a live ``approval.request`` takes. Nothing about a hydrated card is
    special-cased downstream: one registration path means one dedupe rule, one
    transcript line, one answer path, and no second implementation to drift.

    A field the reply does not carry produces no event; a field carrying
    something that is not a mapping produces no event either, because the only
    honest reading of a shape the gateway did not send is that it was not sent.
    """
    if not isinstance(result, Mapping):
        return ()
    events: list[GatewayEvent] = []
    for field_name, event_type in ACTIVATION_HYDRATION_FIELDS.items():
        payload = result.get(field_name)
        if not isinstance(payload, Mapping):
            continue
        events.append(
            GatewayEvent(
                type=event_type,
                session_id=session_id,
                payload=dict(payload),
                at=at,
                seq=seq + len(events),
            )
        )
    return tuple(events)


#: How a row says an attach recovered nothing for a prompt it was waiting on.
#:
#: The wording latches the outcome rather than describing an attempt: the
#: gateway announced that prompt to the transport it has now displaced, no
#: method re-announces it, and nothing Talaria can send would recover it. That
#: is a resolved-failed, and the terminal-read settle precedent says a failure
#: settles visibly instead of leaving a control the operator can never answer.
ATTACH_RESIDUE_PREFIX: Final[str] = "attach recovered no card"

#: What the residue line calls a waiting kind the gateway never named. A polled
#: row reports the flattened ``waiting`` and nothing finer (U1 verified the row
#: carries no kind), so this is the honest noun for it.
ATTACH_RESIDUE_UNKNOWN_KIND: Final[str] = "prompt of an unknown kind"


def attach_residue_notice(kind: str) -> str:
    """The one line a row and the transcript both carry for lost residue."""
    named = ATTACH_RESIDUE_UNKNOWN_KIND if kind in ("", "unobserved") else f"{kind} prompt"
    return (
        f"{ATTACH_RESIDUE_PREFIX} for the {named} this session was waiting on — "
        "resolved-failed: it was announced to the client this attach displaced"
    )


def latch_attach_residue(
    fleet: FleetState,
    *,
    profile: str,
    session_id: str,
    hydrated: frozenset[str],
    at: float,
) -> tuple[FleetState, str]:
    """Latch the visible resolved-failed for a wait the attach did not recover.

    Returns the fleet and the line, ``""`` when there is nothing to latch —
    the caller writes the line to the transcript as well, so one sentence
    reaches both surfaces and neither can drift from the other.

    Nothing is latched when the row was not waiting, or when the kind it was
    waiting on is among the kinds this reply hydrated. Everything else latches,
    including a *clarify* that failed to hydrate: on the revision serving this
    machine the reply carries no hydration fields at all, so "clarify always
    comes back" would be a claim about a gateway Talaria is not talking to.

    **A wait whose kind was never named is the one case decided differently,
    and deliberately.** A polled row flattens every kind to the word
    ``waiting``, so Talaria cannot say whether the thing it was waiting on is
    the thing that just hydrated. When the reply hydrated *something*, the
    latch is withheld: telling the operator a prompt was lost while two
    recovered cards are on the screen in front of them is a contradiction, and
    the cost of being wrong is bounded — the next roster poll reports the
    session as ``waiting`` again and the row says so. When the reply hydrated
    *nothing*, the latch stands, because then there is no reading under which
    anything came back.
    """
    key = _resolve_key(fleet, profile, session_id)
    if key is None:
        return fleet, ""
    row = fleet.rows[key]
    waiting = row.waiting_kind or ("unobserved" if row.status == "waiting" else "")
    if not waiting or waiting in hydrated:
        return fleet, ""
    if waiting == "unobserved" and hydrated:
        return fleet, ""
    line = attach_residue_notice(waiting)
    row = replace(
        row,
        waiting_kind="",
        last_notice=clip_detail_line(line),
        last_event_at=max(row.last_event_at, at),
    )
    return _with_row(fleet, key, row), line


# ── U6: the needs-you queue's fleet half (R13–R15, R18–R21, KTD2, KTD9) ──
#
# The queue itself is derived, never stored: :mod:`talaria.domain.queue` builds
# it from the rows, the prompt registry, the polled approval detail and the seam
# boards, and everything below is the fleet's side of that — the inputs, the
# gated poll's target list, and the one stored consequence
# (``queue_item_keys``) that the memory bound and the retirement rule read.
#
# Deriving rather than storing is what makes R19's "one render boundary" true by
# construction: an expiry removes the prompt, and the item is gone in the same
# reduction because there was never a second copy of it to forget to update.


#: How many settled-item tombstones one fleet keeps. Bounded like every other
#: collection here (R3).
#:
#: **Eviction costs more than a re-latch, and this comment said otherwise until
#: 2026-08-17.** Losing a tombstone does not merely mean re-recording it: the
#: item comes back into the queue and is offered again, which for an approval
#: whose outcome was *ambiguous* is the retry :func:`settle_queue_item` exists to
#: prevent. Eviction is oldest-first and global rather than per session, so a
#: settled approval on one session can be evicted by 256 settles on another.
#:
#: Left bounded at 256 rather than fixed here, deliberately: reaching it needs
#: 256 ambiguous approval outcomes in one run, ``latch_unservable_prompt`` does
#: not consume the bound at all, and the honest repairs — per-session eviction,
#: or an unbounded set with a different retirement rule — revisit a memory-bound
#: decision U3 made and had reviewed. Revisit when a real run gets within sight
#: of the bound, or when the queue gains a second consumer that settles often.
SETTLED_ITEM_LIMIT: Final[int] = 256


def fleet_seam_board(fleet: FleetState, profile: str) -> SeamBoard | None:
    """This connection's seam board, or ``None`` when it has never been probed.

    ``None`` is deliberate and is not the same value as an empty board: an empty
    board says every seam is never-observed, which is a probe story; ``None``
    says there is no probe story at all for this connection. The queue's notices
    tell the operator those apart (R24), and collapsing them would let a
    connection nobody ever probed look exactly like one that was probed and
    answered nothing.
    """
    return fleet.seam_boards.get(profile)


def record_seam_board(fleet: FleetState, *, profile: str, board: SeamBoard) -> FleetState:
    """Store one connection's probe story on the fleet (U5's board, per profile)."""
    boards = dict(fleet.seam_boards)
    boards[profile] = board
    return replace(fleet, seam_boards=boards)


def approval_detail_targets(fleet: FleetState, profile: str) -> tuple[str, ...]:
    """Which sessions of one connection ``approval.pending`` may be called for.

    The KTD11 gate, as amended by the operator ruling of 2026-08-17 — see
    :data:`~talaria.domain.queue.APPROVAL_DETAIL_TRIGGER_STATUSES` for the
    ruling's own words. This function is the only place the fleet decides to make
    that call, so the safety property is checkable by reading one predicate:
    :func:`~talaria.domain.queue.approval_detail_due`.

    Returns **runtime** session ids, and the distinction is not cosmetic —
    naming the wrong one answers ``4001 session not found`` for every call, and
    :func:`apply_approval_pending` reads a non-answering reply as "changes
    nothing", so the queue would simply stay empty while this path looked alive.

    This function returned *durable* ids until 2026-08-19, on a docstring that
    said durable "is what a caller must name on the wire". Measured against the
    running gateway, that is false:

    * ``tui_gateway/server.py:7025-7039`` registers ``_sessions[sid]`` — keyed by
      the runtime id — and stores the durable id as a *field* named
      ``session_key``. The function takes ``sid`` and ``key`` as two separate
      parameters.
    * ``server.py:2507-2509`` resolves ``params["session_id"]`` against that same
      ``_sessions`` mapping, and ``approval.pending``
      (``tui_gateway/methods_prompt.py:1454-1460``) then passes
      ``session["session_key"]`` onward — which only makes sense if the two
      differ.
    * Two local recordings settle it on the wire rather than in source: the same
      durable session ``20260806_120031_fd736f`` resumed 66 seconds apart
      returned runtime ids ``99dcf142`` and ``fd0367c9``. The runtime id is
      reminted on every resume.

    The *other* half of the old sentence stays true and is now separate: the
    reply's detail is stored under the durable key, because
    :func:`apply_approval_pending` resolves what it is given through
    ``_resolve_key``, which accepts an alias. So the caller names the runtime id
    and the fold files it under the durable one, with nothing to translate.

    **A due row with no runtime id is skipped here and NAMED in the queue**, by
    :func:`~talaria.domain.queue.connection_notices`. It cannot be asked about at
    all — the wire has no other handle for it — and a silent skip would be R14's
    forbidden reading, where an empty queue means "we could not ask".

    An empty tuple is the ordinary answer for a quiet or unprobed connection, and
    it is also the answer whenever the seam is anything other than present — an
    absent method is named in the queue's notices, never retried blind.
    """
    board = fleet_seam_board(fleet, profile)
    if board is None:
        return ()
    try:
        seam = board.observation_for("approval-detail").status
    except KeyError:  # pragma: no cover - boards are built from the catalogue
        return ()
    return tuple(
        wire_handle(key, row)
        for key, row in sorted(fleet.rows.items())
        if key[0] == profile and approval_detail_due(row, seam=seam)
    )


def wire_handle(key: RowKey, row: RegistryRow) -> str:
    """The session id this row must be named by on the wire.

    Newest runtime id when the row has one — ``_bind_alias`` keeps the most
    recent ``MAX_RUNTIME_ALIASES`` in arrival order, and only the newest names a
    session the gateway still holds in ``_sessions``.

    **Falling back to the durable id is correct rather than a guess**, and the
    first draft of this got it wrong by skipping such rows entirely.
    ``_bind_alias`` records nothing when the runtime id *equals* the durable id
    (``state.py:2794``), because there is no alias to make — so an empty
    ``runtime_ids`` means the two coincide and the key itself is the handle, not
    that no handle exists.

    Every row this is called for has been through :func:`apply_active_list`,
    which is the only place ``RegistryRow.status`` is ever written
    (``state.py:3408``) and which binds the alias in the same fold. So a row that
    :func:`~talaria.domain.queue.approval_detail_due` admits — status ``waiting``
    or ``working`` — always has one of the two handles above. A row with neither
    cannot exist while that remains true.
    """
    return row.runtime_ids[-1] if row.runtime_ids else key[1]


def apply_approval_pending(
    fleet: FleetState,
    directory: PendingApprovalDirectory,
    *,
    profile: str,
    session_id: str,
    at: float,
) -> FleetState:
    """Fold one ``approval.pending`` reply for one session into the fleet.

    A reply that did not answer (``answered=False`` — no ``approvals`` key, or a
    shape the pin does not describe) changes **nothing**: what was known stays
    known and the seam's own line says the detail is unavailable. Reading a
    malformed reply as "this session has no approvals" would be the fabricated
    zero R10 exists to prevent.

    A reply that did answer replaces the row's detail wholesale, which is how an
    answered approval leaves the queue: the gateway stops listing it, so it stops
    being an item. That is R18's confirmed resolution, and it is why nothing here
    clears an item optimistically.

    The reply does **not** make the row observed. It is detail about a wait, not
    a lifecycle report, and inventing an observation from it would let a session
    whose roster never answered render live on the strength of an approval list.
    """
    fleet = replace(fleet, clock=max(fleet.clock, at))
    if not directory.answered:
        return fleet
    key = _resolve_key(fleet, profile, session_id)
    if key is None:
        return fleet
    detail = dict(fleet.approval_detail)
    merged = merge_polled_approvals(detail.get(key, ()), directory.approvals)
    if merged:
        detail[key] = merged
    else:
        detail.pop(key, None)
    return replace(fleet, approval_detail=detail)


def fleet_queue(fleet: FleetState) -> NeedsYouQueue:
    """The needs-you queue as this fleet stands: both feeds, one identity (R13).

    Pure and derived on every call. The cost is one pass over the rows and the
    prompt registry, and the alternative — a stored queue kept in step by hand —
    is a second copy of the truth that some path will eventually forget to
    update, which for this surface means either a phantom task or a silent one.
    """
    return build_queue(
        rows=fleet.rows,
        aliases=fleet.aliases,
        focused_profile=fleet.focused_profile,
        focused_session_id=fleet.focused.session_key or fleet.focused.focused_session_id,
        feed_a=prompt_feed_rows(fleet.focused.prompts, fleet.focused.answering),
        approval_detail=fleet.approval_detail,
        channels=fleet.channels,
        boards=fleet.seam_boards,
        answers=_answer_stamps(fleet),
        settled=frozenset(fleet.settled_items),
    )


def _answer_stamps(fleet: FleetState) -> Mapping[ItemKey, float | None]:
    """Every in-flight answer, with its send stamp where one was recorded."""
    return {answer: fleet.answer_started_at.get(answer) for answer in fleet.answers_in_flight}


def sync_queue(fleet: FleetState) -> FleetState:
    """Write the queue's current row keys into ``queue_item_keys``.

    **Not required for protection and never called by a reduction here** —
    :meth:`FleetState.protected_keys` derives the queue's rows itself, precisely
    so no path can forget to call this and leave a queue item's row evictable.
    What this offers is a *materialized* copy for a caller that wants to inspect
    or persist which rows the queue is currently holding without rebuilding it,
    and it is unioned with — never a replacement for — the keys anything else
    recorded by hand.
    """
    return replace(
        fleet,
        queue_item_keys=fleet.queue_item_keys
        | frozenset(item.row_key for item in fleet_queue(fleet).items),
    )


def settle_queue_item(
    fleet: FleetState, *, profile: str, session_id: str, request_key: str, at: float
) -> FleetState:
    """Latch one item as settled after an outcome that could not be read (R18).

    The recorded decision, generalized from the approval path: an ambiguous
    outcome **settles and latches, never restores**. The gateway may or may not
    have applied the answer, and Talaria cannot tell; re-offering the control
    would invite a second answer to a question that may already have been
    answered, which for an approval is the worst retry available.

    A tombstone rather than a removal, because the queue is derived — there is
    nothing to remove. For the two keyed item shapes a tombstone is exact: an
    approval's request id is a per-entry uuid (``tools/approval.py:2596``) and a
    bridge prompt's request id is the registry key of one request
    (``models.py``, ``PendingPrompt.request_id``/``observed_request_id``), so a
    genuinely new prompt arrives under a key no tombstone names. The flattened
    roster key is the exception, and an earlier draft of this sentence claimed
    it was not: ``ROSTER_REQUEST_KEY`` is a module constant
    (``talaria/domain/queue.py:104``) — per session, but one key for *every*
    wait that session ever has — so a roster tombstone would name every later
    wait there. The queue therefore ignores a settled roster item entirely
    (``_hide`` in :mod:`talaria.domain.queue`, pinned by
    ``test_a_settled_roster_wait_is_not_a_permanent_tombstone``): a latch exists
    to withhold a control whose answer's outcome was ambiguous, and a roster
    item carries no control. The undercount stays on record because it is the
    evidence that counting key shapes here without citing them was unsafe.
    """
    # Resolved rather than taken verbatim, because callers hold whichever id
    # they were working with and the queue's identity space is the DURABLE one
    # (``build_queue`` keys both feeds from ``row_key[1]``). ``apply_approval_pending``
    # already resolves for the same reason; a latch filed under a runtime id
    # would name an item the queue never builds, so the control would stay
    # offered and the latch would silently do nothing. A session with no row
    # falls back to the id as given — there is nothing better to key it by, and
    # recording the latch beats dropping it.
    resolved = _resolve_key(fleet, profile, session_id)
    key: ItemKey = (profile, resolved[1] if resolved else session_id, request_key)
    if key in fleet.settled_items:
        return replace(fleet, clock=max(fleet.clock, at))
    settled = (*fleet.settled_items, key)[-SETTLED_ITEM_LIMIT:]
    return replace(fleet, settled_items=settled, clock=max(fleet.clock, at))


#: How a row and the transcript say a prompt Talaria could not serve is over.
#:
#: "resolved-failed" rather than "failed": the operator is being told the
#: question is finished, not that a retry is pending. The terminal-read bridge is
#: the case this exists for and the precedent everything else follows — see
#: :func:`latch_unservable_prompt`.
UNSERVABLE_PROMPT_PREFIX: Final[str] = "resolved-failed"


def unservable_prompt_line(kind: str, reason: str) -> str:
    """The one sentence both surfaces carry for a prompt Talaria cannot serve."""
    detail = clip_detail_line(reason) if reason else ""
    line = f"{UNSERVABLE_PROMPT_PREFIX}: the {kind} prompt could not be answered here"
    return f"{line} — {detail}" if detail else line


def settle_unservable_prompt(
    state: SessionState, prompt: PendingPrompt, *, reason: str
) -> tuple[SessionState, str]:
    """Take a prompt Talaria cannot answer out of the registry, visibly.

    **This is the recorded unavailable-projection defect.** The terminal-read
    bridge's unavailable path surfaced its failure locally and left the prompt
    *registered*, so the render pass that dispatches unattended prompts on sight
    found it again on the very next tick, and again after that: one failure line
    per tick, forever, for a question nothing would ever answer. Surfacing a
    failure is not the same as settling one.

    So the prompt leaves the registry, is tombstoned in ``flushed_prompt_ids`` so
    a late restore cannot resurrect it (the same mechanism an expiry uses), and
    the transcript keeps a ``prompt-expired`` entry naming the outcome. R14's
    words for this shape are "fails visibly per the bridge's failure contract and
    is never left silently registered", and every half of that sentence is one of
    those three steps.

    Returns the state and the line, ``""`` when there was nothing to settle, so
    the caller can write the same sentence to the row it belongs to and the two
    surfaces cannot drift.
    """
    target = next((p for p in state.prompts if p is prompt), None)
    if target is None:
        target = state.prompt_for(prompt.request_id, session_id=prompt.session_id)
    if target is None:
        return state, ""
    line = unservable_prompt_line(target.kind, reason)
    state = latch_resolved_prompts(state, (target,))
    state = replace(state, prompts=tuple(p for p in state.prompts if p is not target))
    return _append(state, "prompt-expired", line), line


def latch_unservable_prompt(
    fleet: FleetState,
    prompt: PendingPrompt,
    *,
    profile: str,
    reason: str,
    at: float,
) -> tuple[FleetState, str]:
    """Settle an unservable prompt and name it on its session's row (R14).

    Both surfaces, one sentence: the session's transcript (through
    :func:`settle_unservable_prompt`) and the bounded ``last_notice`` of the
    registry row the prompt belongs to.

    **Only the focused case ever reaches here.** The sole caller is
    ``answer_terminal_read`` (``talaria/ui/app.py``), dispatched over
    ``prompt_view`` rows, and ``prompt_view`` reads
    :func:`~talaria.domain.projection._focused_prompts` — while a background
    terminal-read never registers a prompt at all: :func:`route_frame`, which
    the app folds every inbound frame through, sends a non-focused session's
    event to its registry row and never to the focused engine, so there is no
    prompt object to hand this function
    (``test_a_background_terminal_read_names_its_row_with_the_machine_wording``
    asserts ``prompt_for`` finds nothing). The background case is named on its
    row by the known-prompt branch of :func:`_apply_event_to_row` instead.

    Two earlier drafts of this paragraph were wrong in opposite directions and
    the corrections are kept because each names a real trap. The first called
    the background case "the structural case" for this function; as wired it is
    the one case this function can never see. The second said the focused
    engine's cross-session guard in :func:`apply_frame` was what dropped it —
    true while the app still folded with :func:`apply_frame`, and the reason
    round ten found the row branch reachable only from tests. With the routing
    wired the frame does not reach that guard at all.
    """
    focused, line = settle_unservable_prompt(fleet.focused, prompt, reason=reason)
    fleet = replace(fleet, focused=focused, clock=max(fleet.clock, at))
    if not line:
        return fleet, ""
    session_id = prompt.session_id or fleet.focused.focused_session_id
    if session_id is not None:
        key = _resolve_key(fleet, profile, session_id)
        if key is not None:
            row = fleet.rows[key]
            fleet = _with_row(
                fleet,
                key,
                replace(
                    row,
                    waiting_kind="" if row.waiting_kind == prompt.kind else row.waiting_kind,
                    last_notice=clip_detail_line(line),
                ),
            )
    return fleet, line


__all__ = [
    "ACTIVATION_HYDRATION_FIELDS",
    "APPROVAL_AGED_OUT",
    "APPROVAL_COMMAND_LABEL",
    "APPROVAL_STALE_AFTER",
    "ATTACH_RESIDUE_PREFIX",
    "ATTACH_RESIDUE_UNKNOWN_KIND",
    "DELIVERY_NOTES",
    "EXPIRE_EVENT_KINDS",
    "PROMPT_EVENT_KINDS",
    "SETTLED_ITEM_LIMIT",
    "UNSERVABLE_PROMPT_PREFIX",
    "apply_approval_pending",
    "approval_detail_targets",
    "fleet_queue",
    "fleet_row",
    "fleet_seam_board",
    "latch_unservable_prompt",
    "record_seam_board",
    "settle_queue_item",
    "settle_unservable_prompt",
    "sync_queue",
    "unservable_prompt_line",
    "REFUSED_APPROVAL_NOT_HEAD",
    "REFUSED_SWITCH_WHILE_ANSWERING",
    "REFUSED_SWITCH_WHILE_FLEET_ANSWERING",
    "REFUSED_UNCORRELATED_APPROVAL",
    "UNCORRELATED_APPROVAL",
    "WITHHELD_HISTORY_PREFIX",
    "AnswerKey",
    "DeliveryState",
    "DenyAllScope",
    "FleetState",
    "SessionState",
    "activation_hydration_events",
    "apply_active_list",
    "age_out_approvals",
    "apply_frame",
    "apply_frames",
    "attach_confirm_row",
    "attach_residue_notice",
    "begin_fleet_answer",
    "cancel_turn",
    "end_fleet_answer",
    "fleet_answer_key",
    "fleet_switch_refusal",
    "latch_attach_residue",
    "fleet_connection_lost",
    "fleet_connection_restored",
    "focus_session",
    "is_terminal_status",
    "land_session",
    "listing_failed",
    "mark_we_drive",
    "route_frame",
    "route_frames",
    "seed_from_listing",
    "latch_resolved_prompts",
    "prompt_registration_line",
    "record_local_note",
    "record_replayed_submission",
    "record_submission",
    "replayed_submission_text",
    "respond_to_all_approvals",
    "respond_to_prompt",
    "restore_prompt",
    "seed_history",
    "set_connection",
    "switch_refusal",
    "withheld_history_line",
]
