"""``TalariaApp`` — the Textual shell, and the thing the gate measures.

The prototype and the framework validation gate are the same build. That is the
plan's central bet: a gate that measures a purpose-built harness proves the
harness, so this app is the real interface and the gate simply runs it against a
bigger corpus with counters attached.

**Two clocks, deliberately separated.** Frames arrive from the source as fast as
the replay speed allows, and each one is folded into :class:`SessionState`
immediately — that path is pure and never touches a widget. Rendering happens on
a fixed coalescing tick (KTD14, ~50ms), which projects the state once and hands
the snapshot to the regions. A 50,000-token turn therefore costs 50,000 cheap
reducer calls and about twenty renders a second, instead of 50,000 renders.

**The app owns no domain logic.** It decodes (via the domain's own seam), folds,
projects, and distributes. Every question of meaning — is this turn cancelled,
which sub-agent status wins, what counts as waiting — was answered in
``talaria.domain`` where it is testable without a screen (ADR-0002).

**In replay nothing here can send.** The mutation controls route through
:meth:`ReplayControls.attempt`, which refuses and returns a notice. The gate
asserts the refusal; R30 asserts that no socket exists to refuse *to*. In live
mode the same two controls route to a :class:`LiveDispatcher` — U7's
``LiveSource`` in production, a double in tests — and the app's whole
contribution is deciding what the transcript is allowed to claim about each
outcome (R3, R4, AE8).
"""

from __future__ import annotations

import asyncio
import json
import time
from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any, ClassVar, Final, Literal, Protocol, runtime_checkable

from textual import events
from textual.app import App, ComposeResult
from textual.binding import Binding, BindingType
from textual.containers import Vertical
from textual.css.query import NoMatches
from textual.timer import Timer

from talaria.domain.models import ConnectionStatus, PendingPrompt, PromptKind, RunMode
from talaria.domain.normalize import normalize_frame
from talaria.domain.projection import (
    DEFAULT_VIEWPORT_ROWS,
    ProjectionUnavailableError,
    PromptRow,
    Snapshot,
    TranscriptView,
    project,
    terminal_read,
)
from talaria.domain.state import (
    APPROVAL_COMMAND_LABEL,
    DELIVERY_NOTES,
    REFUSED_NOT_OUTSTANDING,
    DeliveryState,
    SessionState,
    age_out_approvals,
    apply_frame,
    cancel_turn,
    record_local_note,
    record_submission,
    respond_to_all_approvals,
    respond_to_prompt,
    restore_prompt,
    set_connection,
    settle_prompt,
)
from talaria.replay.controls import INERT_NOTICE, ReplayControls
from talaria.status.runner import StatusRunner, StatusTickResult
from talaria.transport.attach import scrub_urls
from talaria.transport.rpc import (
    LOST_WITH_TRANSPORT,
    NEVER_SENT,
    NO_REPLY_IN_TIME,
    NOT_CONNECTED,
    RpcOutcome,
)
from talaria.transport.source import FrameRecord, FrameSource
from talaria.ui.agents import AgentRows
from talaria.ui.composer import ChatTextArea, Composer
from talaria.ui.prompts import (
    DENY_ALL_CHOICE,
    RESPOND_METHODS,
    UNATTENDED_KINDS,
    PromptCard,
    PromptRegion,
    echoable_answer,
    gateway_refusal,
    respond_params,
)
from talaria.ui.status_region import StatusRegion
from talaria.ui.transcript import DEFAULT_MOUNT_CAP, TranscriptPane

#: KTD14's coalescing boundary. Deltas accumulate in the domain transcript and
#: the UI flushes on this tick rather than per token.
COALESCE_INTERVAL: Final[float] = 0.05

#: The gateway method a composed message is sent with
#: (``tui_gateway/methods_prompt.py:67`` at ``7f4d15515``: ``params`` are
#: ``session_id`` and ``text``).
SUBMIT_METHOD: Final[str] = "prompt.submit"

#: The gateway method that stops the in-flight turn (R4;
#: ``tui_gateway/methods_session.py:2706``). Distinct from ``subagent.interrupt``
#: (``:2806``), which stops one delegated child and belongs to U9's control.
INTERRUPT_METHOD: Final[str] = "session.interrupt"

#: The name this control is registered under in
#: :data:`~talaria.replay.controls.MUTATION_CONTROLS`, so replay refuses it
#: visibly rather than letting a prompt answer quietly go nowhere (AE11).
PROMPT_RESPOND_CONTROL: Final[str] = "prompt-respond"


#: Shown when an answer arrives for a prompt the registry no longer holds.
#:
#: Named rather than silent. The two ways to get here — the prompt expired
#: between the keystroke and the dispatch, or it belongs to a session that is no
#: longer focused — are both races the operator did nothing wrong to cause, and
#: a control that swallows a keystroke and does nothing looks exactly like one
#: that answered.
#:
#: Re-exported from the domain, where the registry that does the refusing
#: chooses the wording for each of its three refusals.
PROMPT_NO_LONGER_LIVE: Final[str] = REFUSED_NOT_OUTSTANDING

#: How a successful whole-queue denial opens. A constant so the transcript and
#: the operator's notice cannot come to say different things about the one
#: action the interface offers when nothing can be aimed.
DENIED_EVERY_APPROVAL: Final[str] = "denied every waiting approval"

#: How the deny-all line names approvals it reached but cannot speak for.
#:
#: ``all: true`` resolves every entry in the gateway's queue, including one
#: whose own ``approval.respond`` is still travelling — and that respond may
#: carry the affirmative the operator pressed a moment earlier. Which of the
#: two the gateway applies is decided by arrival order there. So they are
#: counted separately from the ones this call actually denied and the count is
#: labelled as undecided, rather than folded into a "denied" total that would
#: put two different fates for one command into the same transcript.
ANSWER_ALREADY_TRAVELLING: Final[str] = "already answered, outcome unknown"

#: What the deny-all line says when the reply carried no usable ``resolved``
#: count.
#:
#: Formatting the raw value put Python's ``None`` in front of the operator, and
#: "None resolved" reads in English as "none resolved" — the exact opposite of
#: what it meant. The gateway not answering a question and the gateway answering
#: zero are different facts about a safety action, and one of them was being
#: rendered as the other.
UNCOUNTED_RESOLUTION: Final[str] = "the gateway did not say how many it resolved"

#: Prefix for the line shown when terminal-read cannot be served. Nothing goes
#: to the gateway in this case: its bridge expires on its own after 30 seconds,
#: and silence is a supported outcome while a fabricated screen is not (KTD10).
#:
#: A *prefix* rather than the whole sentence, because the reason comes from the
#: projection's own exception and the two must not say the same thing twice —
#: the combined line is clipped at
#: :data:`~talaria.domain.normalize.SYSTEM_LINE_CLIP`, and a duplicated first
#: clause is what pushes the actual reason past the cut.
TERMINAL_READ_UNAVAILABLE: Final[str] = "terminal read not answered —"


#: What the composer says for each transport state that is not ``connected``.
#: R35 asks these be *distinct and visible*; a single "not connected" line for
#: all four would satisfy the letter and lose the only information the operator
#: needs to know what to do next.
_CONNECTION_NOTICE: Final[Mapping[str, str]] = {
    "disconnected": "disconnected from the gateway",
    "connecting": "connecting to the gateway…",
    "connected": "",
    "reconnecting": "connection lost — reconnecting…",
    "auth_failed": "authentication failed — the gateway rejected the credential",
}


#: Which claim the transcript is allowed to make about a submitted message, for
#: each reason the correlator can resolve a call with (``talaria.transport.rpc``).
#:
#: The correlator already knows why a call ended without an answer; this table is
#: only the translation from its reason to what the transcript may say. Without
#: it the UI collapsed all four onto ``outcome.confirmed`` — one boolean, one
#: hardcoded sentence — and a submit attempted with no connection was written
#: into the transcript as possibly delivered.
_DELIVERY_BY_REASON: Final[Mapping[str, DeliveryState]] = {
    NOT_CONNECTED: "not_sent",
    # NEVER_SENT is deliberately *not* "not_sent". It is set when
    # ``connection.send()`` raises, and a send can raise after a partial write
    # — so the write failing is known, and "nothing reached the gateway" is
    # not. Mapping it to "not_sent" would print "send it again" over a message
    # that may have arrived, which is the same overclaim as the hardcoded
    # "the connection dropped" line, pointing the other way.
    NEVER_SENT: "unknown",
    NO_REPLY_IN_TIME: "no_reply",
    LOST_WITH_TRANSPORT: "connection_lost",
}


def delivery_of(outcome: RpcOutcome) -> DeliveryState:
    """Translate one call outcome into the delivery claim it earns.

    An unrecognized (or absent) reason falls back to ``"unknown"``, whose line
    names no cause at all. That is the conservative direction in both senses
    that matter: it never invents a cause, and it never invites the resend that
    ``"not_sent"`` invites — a resend is only safe when the message is *known*
    not to have gone out, and a reason nobody recognizes is not that.
    """
    if outcome.confirmed:
        return "confirmed"
    if outcome.reason is None:
        return "unknown"
    return _DELIVERY_BY_REASON.get(outcome.reason, "unknown")


#: What one ``*.respond`` call turned out to be, in the four kinds a caller has
#: to treat differently.
#:
#: ``used`` is the only one that may be written down as an answer.
AnswerDisposition = Literal["error", "not_sent", "discarded", "used"]


@dataclass(frozen=True)
class AnswerVerdict:
    """The single reading of a respond outcome that every answer path shares.

    **A shared choke point, not a shared helper.** Two paths answer prompts —
    one prompt at a time, and the whole approval queue at once — and each has
    to combine three independent signals to decide what the transcript may
    claim: the JSON-RPC envelope (did the call fail), U7's delivery table (was
    it acknowledged), and the reply *body* (did the gateway use the answer or
    throw it away). Written twice, the two readings disagreed, and they
    disagreed in the direction that matters: deny-all read none of the three
    and reported a denial the gateway had discarded as applied. LEARNINGS
    already carries the rule from a redaction defect of the same shape — a
    sanitizer attached to one selection rule is not a boundary — so the fix is
    one function both call rather than a second correct copy.

    ``reason`` is the clause the transcript puts after the em dash, and it is
    ``None`` exactly when the gateway confirmed it used the answer. ``restore``
    is true only for ``not_sent``, the one outcome that is *definite* about
    non-delivery and therefore the only one where re-offering the question
    cannot deliver a second answer to it.
    """

    disposition: AnswerDisposition
    reason: str | None = None

    @property
    def restore(self) -> bool:
        return self.disposition == "not_sent"

    @property
    def used(self) -> bool:
        return self.disposition == "used"


def _resolved_clause(outcome: RpcOutcome) -> str:
    """How many queue entries the gateway says it released, or that it did not say.

    ``resolved`` can honestly be smaller than the number Talaria was showing —
    an approval that timed out server-side leaves no trace here — so the count
    is reported rather than assumed. What it can never be is ``None`` spelled
    out: a missing count is the gateway declining to answer, and the sentence
    has to say that rather than print a Python literal that reads as zero.
    """
    body = outcome.result if isinstance(outcome.result, Mapping) else {}
    resolved = body.get("resolved")
    if isinstance(resolved, int) and not isinstance(resolved, bool):
        return f"{resolved} resolved"
    return UNCOUNTED_RESOLUTION


def read_answer(kind: PromptKind, outcome: RpcOutcome) -> AnswerVerdict:
    """Decide what one answered prompt is allowed to claim. See
    :class:`AnswerVerdict` for why this exists once."""
    if outcome.status == "error":
        return AnswerVerdict("error", outcome.notice)
    delivery = delivery_of(outcome)
    if delivery == "not_sent":
        return AnswerVerdict("not_sent", DELIVERY_NOTES["not_sent"])
    refusal = gateway_refusal(kind, outcome.result) if outcome.confirmed else None
    if refusal is not None:
        return AnswerVerdict("discarded", refusal)
    # Confirmed leaves ``reason`` unset; every unconfirmed delivery carries its
    # own note, which is what makes "delivery unconfirmed" appear on both paths
    # for the same transport condition instead of on only one of them.
    return AnswerVerdict("used", DELIVERY_NOTES.get(delivery))


@runtime_checkable
class LiveDispatcher(Protocol):
    """The one thing the UI needs from the live transport: an honest call.

    Declared here rather than imported from ``talaria.transport`` so the UI
    depends on a shape instead of on a class, and so a test can drive the live
    paths with a five-line double. ``LiveSource.call`` satisfies it.
    """

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome: ...


class TalariaApp(App[None]):
    """The replay-driven shell: transcript, sub-agent rows, status region, composer."""

    TITLE = "talaria"

    CSS = """
    Screen {
        layout: vertical;
    }
    #body {
        height: 1fr;
    }
    """

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("ctrl+q", "quit", "quit", priority=True),
        Binding("f8", "toggle_pause", "pause/resume", priority=True),
        Binding("f9", "slow_down", "slower", priority=True),
        Binding("f10", "speed_up", "faster", priority=True),
        Binding("f2", "toggle_agents", "sub-agents", priority=True),
        Binding("f4", "interrupt", "interrupt", priority=True),
        Binding("f5", "follow_bottom", "follow", priority=True),
    ]

    def __init__(
        self,
        source: FrameSource,
        *,
        mode: RunMode = "replay",
        controls: ReplayControls | None = None,
        status_runner: StatusRunner | None = None,
        status_interval: float = 5.0,
        coalesce_interval: float = COALESCE_INTERVAL,
        mount_cap: int = DEFAULT_MOUNT_CAP,
        dispatcher: LiveDispatcher | None = None,
        call_timeout: float | None = 30.0,
    ) -> None:
        super().__init__()
        self.source = source
        self.mode: RunMode = mode
        self.controls = controls if controls is not None else ReplayControls()
        self.status_runner = status_runner
        self.status_interval = status_interval
        self.coalesce_interval = coalesce_interval
        self.mount_cap = mount_cap
        self.dispatcher = dispatcher
        #: How long a live call waits before reporting an ``unknown`` outcome.
        #: Bounded because the gateway's own blocking bridges expire at 30s
        #: (``tui_gateway/server.py:2981-2998``), so a call still outstanding
        #: after that is not going to be answered by waiting longer.
        self.call_timeout = call_timeout

        self.state = SessionState()
        self.snapshot: Snapshot | None = None

        self._dirty = True
        self._teardown_started = False
        self._coalesce_timer: Timer | None = None
        self._pump_task: asyncio.Task[None] | None = None
        self._status_task: asyncio.Task[None] | None = None
        #: In-flight live calls started from a key binding. Held so teardown can
        #: cancel them and so a test can await them without sleeping.
        self._live_tasks: set[asyncio.Task[None]] = set()
        #: Serializes :meth:`render_snapshot` against itself — see its docstring.
        self._render_lock = asyncio.Lock()
        #: Highest connection epoch already announced by :meth:`note_reconnect`.
        #: 0 means none: the first attach opens epoch 1 and is not a reconnect.
        self._last_reconnect_epoch = 0
        #: Request ids whose respond is already in flight. The render tick fires
        #: every 50ms and a terminal-read answer is dispatched from it, so
        #: without this the same read is answered once per tick until the reply
        #: lands — several identical answers to one blocking question.
        self._answering: set[str] = set()

        # ── gate counters ────────────────────────────────────────────────
        #: Coalescing flushes that actually re-rendered. KTD14 measures render
        #: ticks here rather than by sampling the screen, because the flush
        #: callback is the one place a render can originate.
        self.render_ticks = 0
        #: Frames folded into domain state. Compared against the corpus size to
        #: prove nothing was skipped.
        self.frames_applied = 0
        #: Set once the source is exhausted, so a gate run knows when to stop
        #: without polling the source's internals.
        self.replay_complete = asyncio.Event()
        self._started_at = 0.0

    # ── layout ───────────────────────────────────────────────────────────

    def compose(self) -> ComposeResult:
        with Vertical(id="body"):
            yield TranscriptPane(mount_cap=self.mount_cap, id="transcript")
            yield AgentRows(id="agents")
            yield PromptRegion(id="prompts")
            yield StatusRegion(id="status")
        yield Composer(notice=self._idle_notice(), id="composer")

    @property
    def transcript(self) -> TranscriptPane:
        return self.query_one("#transcript", TranscriptPane)

    @property
    def agents(self) -> AgentRows:
        return self.query_one("#agents", AgentRows)

    @property
    def prompts(self) -> PromptRegion:
        return self.query_one("#prompts", PromptRegion)

    @property
    def status_region(self) -> StatusRegion:
        return self.query_one("#status", StatusRegion)

    @property
    def composer(self) -> Composer:
        return self.query_one("#composer", Composer)

    def _idle_notice(self) -> str:
        return INERT_NOTICE if self.mode == "replay" else ""

    # ── lifecycle ────────────────────────────────────────────────────────

    async def on_mount(self) -> None:
        self._started_at = time.monotonic()
        self._coalesce_timer = self.set_interval(self.coalesce_interval, self._render_tick)
        self._pump_task = asyncio.create_task(self._pump())
        if self.status_runner is not None and self.status_runner.enabled:
            self._status_task = asyncio.create_task(self._status_loop())
        self.composer.text_area.focus()

    async def on_unmount(self) -> None:
        await self.shutdown_sources()

    async def shutdown_sources(self) -> None:
        """Stop everything Talaria started. Idempotent (R36).

        The coalescing timer is stopped *first*. Textual keeps servicing timers
        while a screen tears down, and a tick that fires after the widgets are
        gone raises ``NoMatches`` from inside the framework — a teardown-order
        bug that surfaces as a flaky, unrelated-looking test failure.

        Note the attribute names. ``_teardown_started`` is deliberately not
        called ``_closing``: ``textual.message_pump.MessagePump`` already owns an
        instance attribute by that name, and assigning it here convinces the
        framework its own shutdown is already in progress, after which the app
        never finishes closing. See ``tests/ui/test_app_shadowing.py``, which
        fails the build if any attribute or method on this class shadows one of
        Textual's.
        """
        self._teardown_started = True
        if self._coalesce_timer is not None:
            self._coalesce_timer.stop()
            self._coalesce_timer = None
        for task in (self._pump_task, self._status_task, *self._live_tasks):
            if task is not None and not task.done():
                task.cancel()
        self._live_tasks.clear()
        if self.status_runner is not None:
            # Cancelling the status task is not enough on its own to satisfy
            # R36. Cancellation unwinds the tick, and the runner's own teardown
            # is what stops a child that is still running — without this call
            # nothing in production ever invoked aclose(), so a status command
            # outliving Talaria depended entirely on the tick happening to be
            # idle at exit.
            await self.status_runner.aclose()
        await self.source.close()

    # ── the frame pump ───────────────────────────────────────────────────

    async def _pump(self) -> None:
        try:
            async for record in self.source:
                self.ingest(record)
        except asyncio.CancelledError:  # pragma: no cover - teardown path
            raise
        finally:
            await self.source.close()
            self.replay_complete.set()

    def ingest(self, record: FrameRecord) -> None:
        """Fold one frame into domain state. Pure except for the dirty flag.

        Outbound frames are counted but never folded: a recording of what
        Talaria itself sent is not a description of what the session became,
        and replaying it as if it were would double-apply the operator's turn.
        """
        self.frames_applied += 1
        if record.direction == "out":
            return
        decoded = normalize_frame(
            record.frame,
            at=record.at,
            seq=record.seq,
            parse_error=record.parse_error,
        )
        self.state = apply_frame(self.state, decoded)
        self._dirty = True

    # ── the coalescing render tick (KTD14) ───────────────────────────────

    async def _render_tick(self) -> None:
        if self._teardown_started:
            return
        self._age_out_approvals()
        if not self._dirty:
            return
        self._dirty = False
        await self.render_snapshot()

    def _age_out_approvals(self) -> None:
        """Withdraw an approval the gateway has almost certainly stopped holding.

        **Run before the dirty check, not after it.** Ageing something out is
        the one state change with no event behind it, so nothing else marks the
        app dirty when it becomes due — placed after the early return it would
        have fired only when some unrelated frame happened to arrive, which for
        a session blocked on a stale approval is precisely never.

        The clock is the one the prompt's own ``opened_at`` came from. Live
        frames are stamped by ``LiveSource`` with the wall clock; a replayed
        frame carries the time it was recorded at, and reading a wall clock
        there would age out an entire corpus on the first tick and break AE2's
        "replay it twice, get the same state".
        """
        now = time.time() if self.mode == "live" else self.state.last_observed_at
        next_state = age_out_approvals(self.state, now=now)
        if next_state is self.state:
            return
        self.state = next_state
        self._dirty = True

    async def render_snapshot(self) -> None:
        """Project once, then update only the regions the projection says moved.

        **Serialized against itself.** The coalescing timer is not the only
        caller: :meth:`drain` and the gate's forced checkpoints call this
        directly, from a task that is *not* the message pump, so two renders can
        interleave. ``TranscriptPane.apply`` is a read-modify-write over its own
        window bookkeeping across several awaits, and two concurrent passes leave
        the pane holding a window the projection does not have — observed as a
        one-line skew (`'line 38.3' != 'line 38.4'`) in
        ``tests/ui/test_transcript_bounds.py``, intermittently and only under
        whole-suite load. Textual's own timer never re-enters its callback, so
        this lock is uncontended in the ordinary path and costs nothing there.
        """
        async with self._render_lock:
            await self._render_snapshot_locked()

    async def _render_snapshot_locked(self) -> None:
        # Counted here, where a render actually happens, rather than in
        # _render_tick. _render_tick is the callback of a set_interval timer, so
        # a count taken there is bounded by the timer frequency (20/s at a 50ms
        # interval) and can never breach the gate's 25/s ceiling however the
        # renderer behaves. Defeating coalescing entirely — scheduling a render
        # per inbound frame — drove real renders to one per frame while the
        # reported rate went *down*. The point of this metric is to notice
        # exactly that, so it counts renders, not timer firings.
        self.render_ticks += 1
        previous = self.snapshot
        snapshot = project(self.state, mode=self.mode, previous=previous)
        self.snapshot = snapshot

        if "transcript" in snapshot.changed:
            await self.transcript.apply(snapshot.transcript)
        if "subagents" in snapshot.changed:
            await self.agents.apply(snapshot.subagents)
        if {"prompts", "status"} & snapshot.changed:
            # Both regions, because the activity line is a function of the
            # prompts *and* of the derived turn status. Watching only "prompts"
            # leaves "working…" on screen after a turn ends with a prompt still
            # outstanding, which is the one sentence R8 forbids.
            await self.prompts.apply(
                snapshot.prompts,
                snapshot.status.turn,
                focus_new=not self.composer.text.strip(),
            )
        self._answer_unattended_prompts(snapshot)

    # ── the status region (U6) ───────────────────────────────────────────

    async def _status_loop(self) -> None:
        runner = self.status_runner
        if runner is None:  # pragma: no cover - guarded by the caller
            return
        while True:
            await self.status_tick()
            await asyncio.sleep(self.status_interval)

    async def status_tick(self) -> StatusTickResult | None:
        """Run one status tick and render its rows. Returns the result for tests."""
        runner = self.status_runner
        if runner is None or not runner.enabled:
            return None
        if self.snapshot is None:
            self.snapshot = project(self.state, mode=self.mode)
        result = await runner.tick(self.snapshot.status)
        await self.status_region.apply(result)
        return result

    # ── replay controls (R40, AE11) ──────────────────────────────────────

    def action_toggle_pause(self) -> None:
        self.controls.toggle_pause()
        self.composer.show_notice(f"{self._idle_notice()} · {self.controls.label}".strip(" ·"))

    def action_speed_up(self) -> None:
        self.controls.speed_up()
        self.composer.show_notice(f"{self._idle_notice()} · {self.controls.label}".strip(" ·"))

    def action_slow_down(self) -> None:
        self.controls.slow_down()
        self.composer.show_notice(f"{self._idle_notice()} · {self.controls.label}".strip(" ·"))

    async def action_toggle_agents(self) -> None:
        await self.agents.toggle_collapsed()

    def action_follow_bottom(self) -> None:
        self.transcript.follow_bottom()

    def action_interrupt(self) -> None:
        """Stop the in-flight turn (R4) — inert in replay (AE11)."""
        if self.mode == "replay":
            self._refuse_mutation("interrupt")
            return
        self._spawn_live(self.interrupt_live())

    def _refuse_mutation(self, name: str) -> None:
        outcome = self.controls.attempt(name)
        self.composer.show_notice(f"{outcome.notice} — {name} did nothing")

    def _spawn_live(self, coroutine: Any) -> asyncio.Task[None]:
        """Run a live call off the message pump, and remember it for teardown.

        A key binding must not await an RPC inline: the message pump that
        delivered the keypress is the same one that has to keep rendering the
        stream the RPC is about to affect, so blocking it would freeze the
        interface for the duration of the call.
        """
        task: asyncio.Task[None] = asyncio.create_task(coroutine)
        self._live_tasks.add(task)
        task.add_done_callback(self._live_tasks.discard)
        return task

    async def settle_live(self) -> None:
        """Await every in-flight live call. For tests and for orderly teardown."""
        while self._live_tasks:
            await asyncio.gather(*tuple(self._live_tasks), return_exceptions=True)

    # ── the live transport's own state (R35, F6) ─────────────────────────

    def note_connection_state(self, state: ConnectionStatus, detail: str = "") -> None:
        """Fold a transport state change into domain state and show it.

        Wired to ``LiveSource(on_connection=…)``. It is a callback rather than a
        synthetic frame on purpose: a fabricated ``gateway.disconnected`` event
        would land in the recorded corpus as though the gateway had sent it.

        ``detail`` carries the cause for the one distinction the frozen KTD5
        enum cannot express — a gateway that could not be reached versus one
        that hung up — so R35's four states stay four on screen.
        """
        self.state = set_connection(self.state, state)
        self._dirty = True
        line = _CONNECTION_NOTICE[state]
        if detail:
            line = f"{line} · {detail}" if line else detail
        self.composer.show_notice(line)

    def note_reconnect(self, epoch: int) -> None:
        """Mark a successful reconnect in the transcript, once (F6).

        Nothing is cleared and nothing is re-requested here, and that *is* the
        reconciliation: the domain transcript is append-only and the prompt
        registry is keyed by ``request_id``, so a gateway that re-announces an
        outstanding prompt after the socket comes back updates the existing
        entry instead of adding a second one. The failure this avoids is the
        tempting alternative — resetting the session and re-reading history —
        which is precisely how a reconnect duplicates a transcript.

        ``epoch`` is the connection generation the correlator just opened
        (``RpcCorrelator.epoch``): 1 is the first attach of the run, 2 the first
        reconnect, and so on. It is what makes the "once" in the first line an
        enforced property rather than a description of the caller's manners. The
        marker is written only for an epoch newer than the last one marked, so a
        callback delivered twice for one connection — two ``bind`` calls, a
        re-armed reconnect loop — leaves one line, and a stale callback arriving
        after a newer connection is already up leaves none. Epochs only ever
        increase, so a single integer is the whole bookkeeping.
        """
        if epoch <= self._last_reconnect_epoch:
            return
        self._last_reconnect_epoch = epoch
        self.state = record_local_note(
            self.state, "reconnected to the gateway", at=self.state.last_observed_at
        )
        self._dirty = True

    # ── live calls (R3, R4, AE8) ─────────────────────────────────────────

    async def submit_live(self, text: str) -> RpcOutcome | None:
        """Send a composed message and write only what is actually known.

        Three outcomes, three different transcripts:

        * **confirmed** — the operator's line is written and the composer is
          cleared.
        * **refused** — nothing is written and the text is kept, because a
          message the gateway rejected was not said.
        * **unknown** — the line is written *and* marked with the reason the
          correlator actually reported, and the composer is cleared. Keeping the
          text as well would put the same message in two places and invite a
          resend, and a resend of a message that did arrive makes the agent do
          the work twice. The text is not lost either way: it is in the
          transcript.
        * **never sent** — a special case of unknown that is not unknown at all.
          When the call ended before anything reached a socket, the message was
          definitely not delivered, so it is marked as not sent *and* left in the
          composer, where one keypress sends it. This is the only unconfirmed
          case where a resend is the right thing to do, which is exactly why it
          has to be told apart from the ones where it is not.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None
        body = text.strip()
        if not body:
            return None

        outcome = await dispatcher.call(
            SUBMIT_METHOD,
            {"session_id": self.state.focused_session_id or "", "text": body},
            timeout=self.call_timeout,
        )

        if outcome.status == "error":
            self.composer.show_notice(outcome.notice)
            self._dirty = True
            return outcome

        delivery = delivery_of(outcome)
        self.state = record_submission(
            self.state, body, at=self.state.last_observed_at, delivery=delivery
        )
        if delivery == "not_sent":
            # `outcome.notice` ends "It may or may not have taken effect", which
            # is true of every other unknown and false of this one. The domain's
            # own line is shown instead so the screen and the transcript agree.
            self.composer.show_notice(DELIVERY_NOTES["not_sent"])
        else:
            self.composer.clear()
            self.composer.show_notice(outcome.notice)
        self._dirty = True
        return outcome

    async def interrupt_live(self) -> RpcOutcome | None:
        """Cancel the in-flight turn, and only claim it when the gateway agreed.

        The cancelled state is applied **only** on a confirmed reply. Applying
        it on an ``unknown`` would be worse than cosmetic: ``cancelled`` is
        sticky and suppresses later deltas, so an interrupt that never landed
        would silently swallow the rest of a turn that is still streaming.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None

        outcome = await dispatcher.call(
            INTERRUPT_METHOD,
            {"session_id": self.state.focused_session_id or ""},
            timeout=self.call_timeout,
        )

        if outcome.confirmed:
            self.state = cancel_turn(self.state, at=self.state.last_observed_at)
        else:
            self.state = record_local_note(
                self.state, outcome.notice, at=self.state.last_observed_at
            )
        self.composer.show_notice(outcome.notice)
        self._dirty = True
        return outcome

    # ── blocking prompts: the approval path and the four bridges (U8) ────

    def on_prompt_card_answered(self, message: PromptCard.Answered) -> None:
        """An operator answered a control. Route it, off the message pump.

        The value is taken out of the message here and passed straight to the
        coroutine. Nothing in this method logs it, formats it, or stores it on
        the app — for two of the five bridges it is a credential, and the code
        path is the same for all five so that it cannot be right for three of
        them and wrong for the others.

        **In replay the refusal is visible, like every other mutation.** A
        recorded corpus contains the prompts that were outstanding at the time,
        so the controls render; there is no gateway to answer, and a control
        that silently does nothing is exactly what AE11's inert-control rule
        exists to prevent. The value is dropped without being named — a replay
        corpus is a shared artifact, and "you typed this into a dead control" is
        not worth putting on screen.
        """
        message.stop()
        if self.mode == "replay" or self.dispatcher is None:
            # ``prompt-respond`` is the name ``MUTATION_CONTROLS`` already
            # reserved for this control. Passing the prompt's kind instead would
            # read better on screen and would route an unclassified name through
            # the refusal path, which that registry exists to refuse.
            self._refuse_mutation(PROMPT_RESPOND_CONTROL)
            return
        self._spawn_live(self._respond_and_discard(message.request_id, message.value))

    def on_prompt_card_denied_all(self, message: PromptCard.DeniedAll) -> None:
        """Deny every approval queued in the session, as one call.

        Reachable only from a card the projection already marked unanswerable,
        and it is the escape from that state rather than a way around it: one
        choice applied to every queue entry needs no correlation, so it is
        correct whatever order the gateway holds them in.
        """
        message.stop()
        if self.mode == "replay" or self.dispatcher is None:
            self._refuse_mutation(PROMPT_RESPOND_CONTROL)
            return
        self._spawn_live(self._deny_all_and_discard(message.session_id))

    async def _deny_all_and_discard(self, session_id: str | None) -> None:
        await self.deny_all_approvals_live(session_id)

    async def deny_all_approvals_live(self, session_id: str | None) -> RpcOutcome | None:
        """Send one ``approval.respond`` with ``all: true``, denying the queue.

        The answerable queue is taken out of the registry before the call, for
        the same reason a single answer is: a second denial while the first is
        travelling is a second value delivered for questions that already have
        one. If the call reaches no socket, every one of them goes back.

        **This path reads the outcome through :func:`read_answer`, the same
        function the single-answer path uses, and that is the fix rather than an
        implementation detail.** Deny-all is the *only* action the interface
        offers once two approvals queue, so the safety-critical case was funnelled
        into the one path that read neither the reply body nor the delivery
        table: a gateway that answered ``{"status": "expired"}`` — the exact body
        the single-answer path was taught to read — produced "denied every
        waiting approval", and an unconfirmed call produced the same sentence as
        a confirmed one. Two readings of one question drift, and these two
        drifted apart in the direction that grants rather than the direction
        that refuses.

        **The counts are reported as two numbers, and only one of them is
        called a denial.** ``all: true`` resolves every entry in the gateway's
        queue, including an approval whose own answer is still in flight — so
        reporting only the cards this call cleared under-counted a safety
        action by exactly the approvals the operator could least afford to lose
        track of. But summing the two over-claimed in the other direction: an
        in-flight approval's own respond may carry an affirmative, so calling
        it denied put two different fates for one command in one transcript.
        It is named and counted as undecided instead. See
        :class:`~talaria.domain.state.DenyAllScope`.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None

        focused = self.state.focused_session_id
        target = session_id or focused
        next_state, scope = respond_to_all_approvals(self.state, session_id=target)
        self.state = next_state
        self._dirty = True
        if not scope.taken:
            self._notice(PROMPT_NO_LONGER_LIVE)
            return None

        outcome = await dispatcher.call(
            RESPOND_METHODS["approval"],
            respond_params(
                "approval",
                request_id="",
                session_id=target,
                value=DENY_ALL_CHOICE,
                all_approvals=True,
            ),
            timeout=self.call_timeout,
        )

        verdict = read_answer("approval", outcome)
        for prompt in scope.taken:
            self.state = (
                restore_prompt(self.state, prompt)
                if verdict.restore
                else settle_prompt(self.state, prompt.request_id)
            )
        covered = f"{scope.denied} waiting"
        if scope.undecided:
            covered = f"{covered} (+{scope.undecided} {ANSWER_ALREADY_TRAVELLING})"
        if verdict.used:
            line = f"{DENIED_EVERY_APPROVAL}: {covered}"
            if verdict.reason is None:
                line = f"{line}, {_resolved_clause(outcome)}"
            else:
                # An unacknowledged call carries no count, so the delivery note
                # already answers "how many". Appending a second clause saying
                # the gateway did not say would only push the note itself
                # toward ``clip_system_line``'s cut, which is where the reason
                # lives.
                line = f"{line} — {verdict.reason}"
        else:
            line = f"{scope.denied} approvals not denied — {verdict.reason}"
        self.state = record_local_note(self.state, line, at=self.state.last_observed_at)
        self._notice(line)
        self._dirty = True
        return outcome

    async def _respond_and_discard(self, request_id: str, value: str) -> None:
        await self.respond_live(request_id, value)

    async def respond_live(self, request_id: str, value: str) -> RpcOutcome | None:
        """Answer one outstanding prompt, and only the one that asked (R9).

        **The registry is consulted before anything is sent, and it is what
        clears the prompt.** Both halves of the correlation clause are checked
        there — the request id must still be live *and* it must belong to the
        session currently focused — so an answer typed into a control that a
        ``*.expire`` cleared a moment earlier reaches no socket at all (R8), and
        an answer for a session that is no longer the focused one cannot be
        delivered to whatever question the new session happens to be asking.

        **The prompt is cleared before the call goes out, not after it
        succeeds.** One question must not be able to collect two answers while
        the first is in flight; for a sudo password or a secret that is the
        worst retry available. The cost is that a call which fails is a question
        the operator can no longer answer — so the single outcome that is
        *definite* about non-delivery, ``not_sent``, puts the control back
        (:func:`~talaria.domain.state.restore_prompt`). Every other unconfirmed
        outcome leaves it cleared and marks the transcript, because a resend of
        an answer that did arrive is a second value delivered for one question.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None

        prompt = self.state.prompt_for(request_id)
        session_id = self.state.focused_session_id
        next_state, refusal = respond_to_prompt(
            self.state, request_id, session_id=session_id
        )
        if refusal is not None or prompt is None:
            # The refused state is kept, not discarded. It carries the registry's
            # ``rejected_responses`` tally, which is the only observable trace
            # that the guard fired at all — dropping it leaves a counter that
            # can never move and a guard nothing can measure.
            self.state = next_state
            # Nothing is sent, and the refusal is named rather than silent: a
            # control that swallowed a keystroke and did nothing is
            # indistinguishable from one that answered. The registry chose which
            # sentence, because it is the thing that knows which guard fired.
            self._notice(refusal or PROMPT_NO_LONGER_LIVE)
            self._dirty = True
            return None
        self.state = next_state
        self._dirty = True

        outcome = await dispatcher.call(
            RESPOND_METHODS[prompt.kind],
            respond_params(
                prompt.kind,
                request_id=request_id,
                session_id=session_id,
                value=value,
            ),
            timeout=self.call_timeout,
        )

        self._record_prompt_outcome(prompt, value, outcome)
        return outcome

    def _record_prompt_outcome(
        self, prompt: PendingPrompt, value: str, outcome: RpcOutcome
    ) -> None:
        """Write what is known about one answer — never the answer itself.

        The only value that reaches the transcript is one the *gateway* offered
        (:func:`~talaria.ui.prompts.echoable_answer`), which is what keeps the
        approval audit trail useful without making the transcript an egress for
        the operator-typed bridges.

        **What the answer was applied to is written down beside it.** For an
        approval that is the command, because "did I allow that" is the question
        the transcript exists to answer afterwards and the choice alone does not
        answer it. The whole command is already in the arrival entry
        unclipped (:func:`~talaria.domain.state.prompt_registration_line`); the
        copy here is bounded by ``record_local_note``'s system-line clip, which
        marks its own cut.

        :func:`read_answer` decides what may be claimed, and it is the same
        function the deny-all path calls — so an unconfirmed delivery, a
        gateway that discarded the answer, and an outright error make the same
        claim whichever control the operator used.

        **Where the sentence goes is decided once, by
        :meth:`_report_prompt_outcome`, for all four outcome classes.** Round 4
        moved one of them — the answer that reached no socket — off the
        transcript, because a bridge that *serves* the transcript must not write
        into it. That was the loop, and fixing the loop is not the same as
        honouring the rule: refused, discarded and delivery-unconfirmed still
        wrote a line each, one line of self-contamination per failed read, with
        the code's own comment two branches lower stating the rule they broke.
        """
        row = PromptRow(
            request_id=prompt.request_id,
            kind=prompt.kind,
            summary=prompt.summary,
            choices=prompt.choices,
            session_id=prompt.session_id,
        )
        shown = echoable_answer(row, value)
        label = prompt.kind.replace("_", " ")
        applied_to = f" · {APPROVAL_COMMAND_LABEL}{prompt.command}" if prompt.command else ""
        answered = (
            f"{label} answered: {shown}{applied_to}"
            if shown
            else f"{label} answered{applied_to}"
        )
        verdict = read_answer(prompt.kind, outcome)

        if verdict.disposition == "error":
            self.state = settle_prompt(self.state, prompt.request_id)
            self._report_prompt_outcome(
                prompt, f"{answered} — {verdict.reason}", notice=outcome.notice
            )
            self._dirty = True
            return

        if verdict.restore and prompt.kind in UNATTENDED_KINDS:
            # **Neither half of the restore applies to a prompt Talaria answers
            # itself, and both halves did damage.**
            #
            # Restoring re-offers a control to an operator — and there is no
            # operator here. The prompt went straight back into the projection,
            # where ``_answer_unattended_prompts`` re-dispatched it on the very
            # next render, which failed the same way, which restored it again:
            # measured at 136 ``terminal.read.respond`` calls in 400ms, for as
            # long as the socket stayed down.
            #
            # Writing the note is worse, because it is the failure the clean
            # path two branches below already refuses to commit: the line goes
            # into the buffer this bridge *serves*, so each attempt made the
            # answer larger than the one before it — 159 characters to 884
            # across three cycles. The operator is still told, on the notice
            # bar, which the transcript projection does not read.
            self.state = settle_prompt(self.state, prompt.request_id)
            self._notice(f"{label} not answered — {verdict.reason}")
            self._dirty = True
            return

        if verdict.restore:
            # ``restore_prompt`` settles the in-flight entry itself, and it is
            # the one path that may decline to put the control back — an expiry
            # that landed while this call was out already closed the question.
            self.state = restore_prompt(self.state, prompt)
            self.state = record_local_note(
                self.state,
                f"{label} not answered{applied_to} — {verdict.reason}",
                at=self.state.last_observed_at,
            )
            self._notice(DELIVERY_NOTES["not_sent"])
            self._dirty = True
            return

        self.state = settle_prompt(self.state, prompt.request_id)
        if verdict.disposition == "discarded":
            self._report_prompt_outcome(
                prompt,
                f"{label} not answered{applied_to} — {verdict.reason}",
                notice=verdict.reason or "",
            )
            self._dirty = True
            return

        note = verdict.reason
        if note is None and prompt.kind in UNATTENDED_KINDS:
            # A terminal-read that went through cleanly says nothing anywhere.
            # No human was involved, so there is no act to record, and there is
            # nothing to tell the operator either.
            self._dirty = True
            return
        line = answered if note is None else f"{answered} — {note}"
        # **The same sentence on both surfaces, not two readings of one fact.**
        # This used to show ``outcome.notice``, which is the transport layer's
        # own wording — so one ``NO_REPLY_IN_TIME`` produced "delivery
        # unconfirmed — the message was sent and no reply arrived before the
        # deadline" in the transcript and "approval.respond outcome unknown —
        # no reply arrived before the deadline. It may or may not have taken
        # effect." on the notice bar, at the same moment, about the same call.
        # ``submit_live`` already overrides ``outcome.notice`` for exactly this
        # reason and says so in a comment; the prompt path did not inherit it.
        # ``line`` carries no operator-typed value: ``answered`` only ever
        # names a choice the *gateway* offered (:func:`echoable_answer`).
        self._report_prompt_outcome(prompt, line)
        self._dirty = True

    def _report_prompt_outcome(
        self, prompt: PendingPrompt, line: str, *, notice: str | None = None
    ) -> None:
        """Put one outcome sentence where that prompt's kind allows it to go.

        **The transcript is not a neutral log for four of the five bridges and
        is an input for the fifth.** ``terminal.read`` serves this buffer
        straight back to the agent, so a line Talaria writes about its own
        answer becomes part of the next answer: self-contamination, one line per
        failed read, growing with the number of reads rather than with anything
        the session did. Round 3 met the compounding form of this — a restore
        loop that took one answer from 159 characters to 884 in three cycles —
        and round 4 fixed the loop by taking one outcome class off the
        transcript. Three others were still writing.

        So the rule is applied here, once, over every class: a prompt Talaria
        answers itself (:data:`~talaria.ui.prompts.UNATTENDED_KINDS`) reports on
        the notice bar only. The operator still learns what happened — the
        notice bar is not a surface the read projection reads — and the reason
        the notice carries the full sentence for those kinds is that it is now
        the only place carrying it.

        ``notice`` overrides what the operator is shown; the default is the same
        sentence that went to the transcript, which is the property
        ``test_the_notice_bar_and_the_transcript_say_one_thing`` pins.
        """
        if prompt.kind not in UNATTENDED_KINDS:
            self.state = record_local_note(
                self.state, line, at=self.state.last_observed_at
            )
            self._notice(line if notice is None else notice)
            return
        self._notice(line)

    def _notice(self, message: str) -> None:
        """Show a composer notice, unless the composer is no longer mounted.

        A respond can outlive the screen: the call is in flight when the app
        tears down, the reply or the timeout lands afterwards, and the handler
        runs to completion against a composed tree that no longer exists.
        Textual raises ``NoMatches`` from the query in that case, which would
        surface as an unrelated-looking error at the end of an orderly exit
        (R36). Only the *absence* of the widget is tolerated here — nothing else
        is caught, so a genuine rendering failure still raises.
        """
        try:
            composer = self.composer
        except NoMatches:  # pragma: no cover - teardown ordering
            return
        composer.show_notice(message)

    # ── terminal-read: answered from the projection, with no human (F2) ──

    def _answer_unattended_prompts(self, snapshot: Snapshot) -> None:
        """Dispatch an answer for every prompt Talaria answers itself.

        Called from the render pass because that is where a fresh projection
        exists, and a terminal-read is a question *about* that projection. It
        never blocks the pass: the answer is spawned as a live task.

        **This dispatches on sight, so the bound is that every outcome settles
        the prompt.** ``_answering`` covers only the round trip — it is
        discarded in a ``finally`` — so a prompt still in the registry after
        its answer resolves is re-dispatched on the very next tick, forever.
        That is what made the ``restore`` branch in :meth:`_record_prompt_outcome`
        a loop rather than a retry, and the fix belongs there rather than in a
        second latch here: a latch would bound the symptom while leaving a row
        on screen that the projection says is outstanding and nothing will ever
        answer.
        """
        if self.mode != "live" or self.dispatcher is None or self._teardown_started:
            return
        for row in snapshot.prompts.rows:
            if row.kind not in UNATTENDED_KINDS or row.request_id in self._answering:
                continue
            self._answering.add(row.request_id)
            self._spawn_live(self._answer_and_discard(row))

    async def _answer_and_discard(self, row: PromptRow) -> None:
        await self.answer_terminal_read(row)

    def transcript_view_for_read(self) -> TranscriptView | None:
        """The buffer terminal-read serves, or ``None`` when nothing honest is.

        Two conditions, and both are ordinary rather than exceptional. Teardown
        has begun, so the interface the read describes is being dismantled while
        the answer is composed. Or no snapshot exists yet, which is the case for
        a read that arrives before the first render — the state may hold frames,
        but nothing has been on screen, and terminal-read's contract is about
        the screen.

        ``None`` is the honest answer to both, and
        :func:`~talaria.domain.projection.terminal_read` turns it into a raised
        :class:`ProjectionUnavailableError` rather than an empty buffer, because
        "the terminal has no lines" is a claim and this is an absence of one.
        """
        if self._teardown_started or self.snapshot is None:
            return None
        return self.snapshot.transcript

    async def answer_terminal_read(self, row: PromptRow) -> RpcOutcome | None:
        """Serve the transcript buffer, or send nothing at all (KTD10, F2).

        Public because it is reachable from outside the render pass and has to
        be drivable from there. :meth:`shutdown_sources` cancels the in-flight
        live tasks, which covers a read that has not started; a read that is
        already running when teardown begins is the case this method's own guard
        exists for, and a test drives exactly that order.

        The gateway's bridge tolerates silence — the read blocks for 30 seconds
        and then expires (``tui_gateway/server.py:2981-2998``) — so "the
        projection cannot answer" has a correct behaviour that is not an error
        reply and is certainly not a plausible-looking screen. The failure is
        surfaced locally instead, where the operator can see it and the agent
        cannot mistake it for the contents of a terminal.
        """
        try:
            response = terminal_read(
                self.transcript_view_for_read(),
                viewport_rows=self.viewport_rows(),
                start_line=row.read_start,
                count=row.read_count,
            )
        except ProjectionUnavailableError as exc:
            self._answering.discard(row.request_id)
            # Scrubbed for the same reason every other operator-facing failure
            # string is: this text is built from an exception, and an exception
            # is the one place a dial target can arrive somewhere nobody
            # expected it. Both halves are asserted in the test suite — the
            # credential is gone *and* the message still says what went wrong.
            detail = scrub_urls(str(exc))
            line = f"{TERMINAL_READ_UNAVAILABLE} {detail}".strip()
            self.state = record_local_note(
                self.state, line, at=self.state.last_observed_at
            )
            self._notice(line)
            self._dirty = True
            return None

        try:
            return await self.respond_live(
                row.request_id, json.dumps(response.to_json_dict())
            )
        finally:
            self._answering.discard(row.request_id)

    def viewport_rows(self) -> int:
        """How many transcript rows are actually on screen (KTD10).

        Served truthfully because it is a number the UI already knows, and an
        agent paging through scrollback with ``start``/``count`` uses it to
        decide how far to step. A pane that has not been laid out yet reports
        the projection's documented default rather than zero, which would tell
        the agent the screen has no rows at all.
        """
        try:
            height = self.transcript.size.height
        except Exception:  # noqa: BLE001 - queried before the screen exists
            return DEFAULT_VIEWPORT_ROWS
        return height if height > 0 else DEFAULT_VIEWPORT_ROWS

    # ── composer ─────────────────────────────────────────────────────────

    def on_chat_text_area_submitted(self, message: ChatTextArea.Submitted) -> None:
        """Enter on composed text.

        In replay this echoes nothing and keeps the text. Writing the composed
        message into the transcript would render a line identical to one that
        had actually been delivered, and no operator could tell the difference
        afterwards — which is the whole reason AE11 makes inertness visible
        rather than silent.
        """
        message.stop()
        if self.mode == "replay":
            self._refuse_mutation("submit")
            return
        self._spawn_live(self._submit_and_discard(message.text))

    async def _submit_and_discard(self, text: str) -> None:
        """Adapt :meth:`submit_live` to the ``None``-returning task shape."""
        await self.submit_live(text)

    # ── scroll anchoring ─────────────────────────────────────────────────

    def on_mouse_scroll_up(self, event: events.MouseScrollUp) -> None:
        self.transcript.hold_anchor()

    def on_key(self, event: events.Key) -> None:
        # Reading while scrolled away must survive streaming (R38). Page keys
        # reach the app because the transcript is not focused — the composer is
        # — so the anchor is released here rather than inside the pane.
        if event.key in ("pageup", "home"):
            self.transcript.hold_anchor()
        elif event.key == "end":
            self.transcript.follow_bottom()

    # ── helpers used by the gate harness ─────────────────────────────────

    async def drain(self, *, timeout: float = 120.0) -> None:
        """Wait for the source to be exhausted and one final render to land."""
        await asyncio.wait_for(self.replay_complete.wait(), timeout=timeout)
        self._dirty = True
        await self._render_tick()

    def measurements(self) -> dict[str, Any]:
        elapsed = max(1e-9, time.monotonic() - self._started_at)
        return {
            "frames_applied": self.frames_applied,
            "render_ticks": self.render_ticks,
            "elapsed_seconds": elapsed,
            "render_ticks_per_second": self.render_ticks / elapsed,
            "peak_mounted_widgets": self.transcript.peak_mounted,
            "condensed_lines": self.transcript.condensed_count,
        }
