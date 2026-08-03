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
import time
from collections.abc import Mapping
from typing import Any, ClassVar, Final, Protocol, runtime_checkable

from textual import events
from textual.app import App, ComposeResult
from textual.binding import Binding, BindingType
from textual.containers import Vertical
from textual.timer import Timer

from talaria.domain.models import ConnectionStatus, RunMode
from talaria.domain.normalize import normalize_frame
from talaria.domain.projection import Snapshot, project
from talaria.domain.state import (
    DELIVERY_NOTES,
    DeliveryState,
    SessionState,
    apply_frame,
    cancel_turn,
    record_local_note,
    record_submission,
    set_connection,
)
from talaria.replay.controls import INERT_NOTICE, ReplayControls
from talaria.status.runner import StatusRunner, StatusTickResult
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
            yield StatusRegion(id="status")
        yield Composer(notice=self._idle_notice(), id="composer")

    @property
    def transcript(self) -> TranscriptPane:
        return self.query_one("#transcript", TranscriptPane)

    @property
    def agents(self) -> AgentRows:
        return self.query_one("#agents", AgentRows)

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
        if self._teardown_started or not self._dirty:
            return
        self._dirty = False
        await self.render_snapshot()

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
