"""``ReplaySource`` — KTD3's seam, backed by a frame-log v1 recording.

The gate's whole claim (R30) rests on this module: the entire interface runs
from a file, with no socket open anywhere in the process. Nothing here imports
``websockets``, and nothing here dials anything.

**Recorded time and wall time are kept strictly apart.** ``FrameRecord.at``
carries the *recorded* observation time and is what the domain reducer sees, so
replaying one corpus twice produces identical state (AE2). The wall-clock sleep
between two frames is a separate number derived from the recorded gap through
:class:`~talaria.replay.controls.ReplayControls`, and changing it — pausing,
resuming, 8x, unbounded — cannot change domain state. That is why AE11's
"deterministic identical final state at any speed" is a property of the design
rather than a hope.
"""

from __future__ import annotations

import asyncio
from collections import deque
from collections.abc import AsyncIterator, Callable, Iterable, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Final, Literal

from talaria.domain.normalize import parse_frame_time
from talaria.recorder.framelog import FRAME_LOG_VERSION_MULTI_CONNECTION
from talaria.recorder.reader import FrameLogEntry, FrameLogHeader, iter_frame_log, read_header
from talaria.replay.controls import ReplayControls
from talaria.transport.connection_set import TaggedFrame
from talaria.transport.source import Direction, FrameRecord, TerminalCause

#: How many zero-delay frames the source emits before handing control back to
#: the event loop. Small enough that a 50ms render tick is never missed by more
#: than a fraction of its period; large enough that a scheduler hop is not paid
#: per frame.
YIELD_EVERY = 64

__all__ = [
    "YIELD_EVERY",
    "ReplaySource",
    "SidebandAction",
    "SidebandActionKind",
    "build_sideband",
    "load_frame_records",
    "load_header",
    "parse_frame_time",
    "record_from_entry",
]


# ── U6: the sideband timeline (KTD8's branch-hold, AE2 at gate level) ───────
#
# Confirmed-cancel and typed-disconnect are not wire frames: an interrupt
# reply decodes to a `NonEventFrame` the reducer ignores (state.py:1246), and
# a transport callback like `note_connection_state` is never recorded to
# begin with (there is no `gateway.disconnected` event type — inventing one
# would put words in the gateway's mouth, exactly what `note_connection_state`'s
# own docstring in `talaria/ui/app.py` refuses to do). A replay corpus that
# genuinely contained one of these cannot reproduce it from the frame log
# alone, so the gate carries a second, deterministic timeline beside it: one
# scripted, non-wire-frame action per entry, tied to the index of the frame it
# follows. Scope is exactly the two action kinds named below — nothing richer.

SidebandActionKind = Literal["confirmed_cancel", "typed_disconnect", "checkpoint"]


@dataclass(frozen=True)
class SidebandAction:
    """One scripted, non-wire-frame injection, ordered against a frame index.

    ``frame_index`` is 1-based and means "immediately after the frame at this
    position in the corpus has been applied" — matching
    :attr:`ReplaySource.emitted`'s own count, which increments before a frame
    is handed to the consumer. An index at or beyond the corpus length is not
    an error: the action fires once the corpus is exhausted, which is exactly
    right for "the connection dropped after the visible content ended".

    ``cause`` is KTD7's typed end-of-stream cause (only meaningful for
    ``typed_disconnect`` — a confirmed cancel carries no cause, it is the
    operator's own turn ending, not the transport's).
    """

    frame_index: int
    kind: SidebandActionKind
    cause: TerminalCause | None = None

    def __post_init__(self) -> None:
        if self.frame_index < 1:
            raise ValueError(f"frame_index must be >= 1, got {self.frame_index}")
        if self.kind == "typed_disconnect" and self.cause is None:
            raise ValueError("a typed_disconnect action needs a cause (KTD7)")
        if self.kind == "confirmed_cancel" and self.cause is not None:
            raise ValueError("a confirmed_cancel action carries no cause")
        if self.kind == "checkpoint" and self.cause is not None:
            raise ValueError("a checkpoint action carries no cause")


def build_sideband(actions: Iterable[SidebandAction]) -> tuple[SidebandAction, ...]:
    """Validate and order a scripted action track by frame index.

    Two actions cannot target the same frame index — "ordered against frame
    indices" is only a real ordering if it is total, and a tie would make
    replay order (dict/set iteration, insertion order after a rebuild...)
    decide something that is supposed to be a deliberate script instead.
    """
    ordered = tuple(sorted(actions, key=lambda action: action.frame_index))
    seen: set[int] = set()
    for action in ordered:
        if action.frame_index in seen:
            raise ValueError(
                f"two sideband actions target frame index {action.frame_index} — "
                "the track must be a total order"
            )
        seen.add(action.frame_index)
    return ordered


def record_from_entry(entry: FrameLogEntry) -> FrameRecord:
    """Adapt one frame-log record to the seam's :class:`FrameRecord`.

    ``parseError``'s *text* is deliberately dropped. R26 keeps wire content out
    of diagnostics, and the decoder already turns the flag into a fixed
    protocol-error sentence; carrying the recorder's message across the seam
    would open a second route for a payload fragment to reach a rendered line.

    Time parsing is delegated to :func:`talaria.domain.normalize.parse_frame_time`
    rather than reimplemented, so replay and the domain's own fixtures cannot
    disagree about what a recorded timestamp means. Its unparseable-value
    fallback is ``0.0`` rather than a clock read, which is what keeps AE2's
    "replay the same corpus twice, get the same state" true even for a damaged
    record.
    """
    direction: Direction = "out" if entry.dir == "out" else "in"
    return FrameRecord(
        seq=entry.seq,
        at=parse_frame_time(entry.at),
        direction=direction,
        frame=entry.frame,
        parse_error=entry.parse_error is not None,
    )


#: The socket generation every replayed frame carries.
#:
#: **One, not zero, and the difference is rendered.** ``route_frame`` persists a
#: connection's channel only when ``generation > channel.generation``, and both
#: are ``0`` on a fresh fleet — so an epoch of zero replays a two-connection log
#: into a fleet with no channels at all, and the queue then names only the
#: focused profile instead of every connection the recording holds. Measured:
#: epoch 0 gives ``channels=[]``; epoch 1 gives both.
#:
#: A recording has no epoch of its own to carry, and it does not need one: the
#: whole file is one pass over connections that are not being re-dialled, so
#: every frame in it belongs to the same generation by construction. What the
#: number must not be is a value that makes live frames read as superseded.
REPLAY_EPOCH: Final[int] = 1


def profiles_from_entries(entries: Iterable[FrameLogEntry]) -> tuple[str, ...]:
    """One profile per entry, positionally, for :class:`ReplaySource`."""
    return tuple(entry.profile for entry in entries)


#: The methods whose reply lands a session — the recorded evidence of which
#: connection this run was actually driving. Spelled here rather than imported
#: from ``talaria.ui.app`` because replay must not import the UI package: these
#: are wire names, and the direction that matters is that nothing below the seam
#: reaches up (ADR-0002).
LANDING_METHODS: Final[frozenset[str]] = frozenset({"session.create", "session.resume"})


def _frame_method(frame: Any) -> str:
    if not isinstance(frame, dict):
        return ""
    method = frame.get("method")
    return method if isinstance(method, str) else ""


def _frame_session_id(frame: Any) -> str:
    if not isinstance(frame, dict):
        return ""
    params = frame.get("params")
    if not isinstance(params, dict):
        return ""
    session_id = params.get("session_id")
    return session_id if isinstance(session_id, str) else ""


def derive_focus_profile(
    header: FrameLogHeader, entries: Sequence[FrameLogEntry]
) -> str:
    """Which connection's session a replay of this log should show.

    **Replay has no other way to answer this, and the answer is not cosmetic.**
    ``_adopt_profile`` is reached from the two ``/profiles`` picker paths and
    nowhere else, so it never runs in replay; ``focused_profile`` is whatever the
    app was constructed with. Meanwhile ``route_frame`` feeds the focused engine
    only frames whose profile equals it — so a tagged log replayed at the wrong
    focus renders an EMPTY transcript. Measured: the same frames at
    ``focused_profile='default'`` give 0 transcript entries and at the tagged
    profile give 3.

    Three rules, in the plan's own order, each preferring recorded evidence over
    inference:

    1. **A recorded landing reply.** An outbound ``session.create`` or
       ``session.resume`` is this run *choosing* a session, which is the
       strongest statement the log makes about what it was driving.
    2. **The first session named on the wire**, which is the adoption rule the
       live engine already follows when it has no session of its own — so a log
       with no landing call replays the way the run itself behaved.
    3. **The header's first declared connection**, for a log whose frames name
       no session at all. A recording of a connection that only ever carried
       gateway-level traffic still belongs to that connection.

    Returns ``""`` when the log declares nothing and names nothing, leaving the
    caller's own default in place rather than inventing a profile.
    """
    for entry in entries:
        if entry.dir == "out" and _frame_method(entry.frame) in LANDING_METHODS:
            return entry.profile
    for entry in entries:
        if _frame_session_id(entry.frame):
            return entry.profile
    if header.connections:
        return header.connections[0].profile
    return ""


def load_frame_records(path: str | Path) -> tuple[FrameRecord, ...]:
    """Read a whole frame log into seam records.

    Materializes the corpus. That is the right trade here: the gate replays one
    file repeatedly at different speeds and must compare final states, and
    re-reading the file per pass would make disk timing part of the
    measurement.
    """
    return tuple(record_from_entry(entry) for entry in iter_frame_log(path))


def load_header(path: str | Path) -> FrameLogHeader:
    return read_header(path)


class ReplaySource:
    """Yield recorded frames on a scaled clock, honouring pause and speed.

    Satisfies :class:`~talaria.transport.source.FrameSource`. ``close()`` is
    idempotent and cancels an in-flight sleep, so a consumer that stops early
    leaves nothing pending.
    """

    def __init__(
        self,
        records: Sequence[FrameRecord] | Iterable[FrameRecord],
        *,
        controls: ReplayControls | None = None,
        sideband: Sequence[SidebandAction] = (),
        on_sideband: Callable[[SidebandAction], None] | None = None,
        profiles: Sequence[str] = (),
    ) -> None:
        self._records: tuple[FrameRecord, ...] = tuple(records)
        self.controls = controls if controls is not None else ReplayControls()
        self._closed = False
        self._sleep_task: asyncio.Task[None] | None = None
        self._emitted = 0
        #: U6's sideband timeline. Kept as a deque so firing an action pops it
        #: from the front in order — the track is already sorted by
        #: :func:`build_sideband`, and a caller that hands in an already-sorted
        #: tuple (as every builder in this package does) pays nothing extra.
        self._sideband: deque[SidebandAction] = deque(sideband)
        self._on_sideband = on_sideband
        #: One profile per record, positionally. Empty for a version-1 log.
        #: Kept beside the records rather than folded into ``FrameRecord``
        #: because that type is the LIVE seam's — widening it would put a replay
        #: concern on every ``LiveSource`` yield, and the live seam already
        #: carries connection identity in ``TaggedFrame``.
        self._profiles: tuple[str, ...] = tuple(profiles)

    @classmethod
    def from_path(
        cls, path: str | Path, *, controls: ReplayControls | None = None
    ) -> ReplaySource:
        return cls(load_frame_records(path), controls=controls)

    def bind_sideband(
        self, actions: Sequence[SidebandAction], callback: Callable[[SidebandAction], None]
    ) -> None:
        """Arm the sideband timeline after construction.

        Exists for exactly one shape of caller: something that needs the app
        this source feeds to exist *before* it can build the callback (the
        callback closes over the app to apply an action to its state) but
        needs the source to exist *before* the app can be constructed
        (``TalariaApp.__init__`` takes the source as its first argument).
        Safe to call any time before iteration starts — nothing here reads
        ``self._sideband``/``self._on_sideband`` except ``__aiter__`` itself,
        and iteration cannot have started before this method's caller has
        finished constructing both objects.
        """
        self._sideband = deque(actions)
        self._on_sideband = callback

    def __len__(self) -> int:
        return len(self._records)

    @property
    def emitted(self) -> int:
        """How many frames have crossed the seam so far."""
        return self._emitted

    @property
    def closed(self) -> bool:
        return self._closed

    @property
    def records(self) -> tuple[FrameRecord, ...]:
        return self._records

    async def paced(self) -> AsyncIterator[tuple[FrameRecord, str]]:
        """Every record on the scaled clock, with the connection it crossed.

        **The one copy of this loop.** :meth:`__aiter__` and
        :class:`TaggedReplaySource` are both thin wrappers over it, because the
        pacing, the pause handling, the yield-every-N starvation guard and the
        sideband ordering are one rule and a second copy of them is a second
        answer waiting to drift. The profile is ``""`` for a version-1 log,
        where there is one connection and no frame names it.
        """
        previous: float | None = None
        since_yield = 0
        for record in self._records:
            if self._closed:
                return
            await self.controls.wait_while_paused()
            if self._closed:
                return
            if previous is not None:
                delay = self.controls.delay_for(record.at - previous)
                if delay > 0:
                    await self._sleep(delay)
                    if self._closed:
                        return
                    since_yield = 0
                else:
                    # Zero delay means nothing above awaited, and an async
                    # generator that never awaits never returns control to the
                    # event loop. Without this the pump starves its own render
                    # timer: the whole corpus lands in domain state and the
                    # screen updates once at the end, which would make the
                    # gate's render-tick measurement meaningless and, worse,
                    # would freeze a live session during a burst. Yielding
                    # every :data:`YIELD_EVERY` frames keeps the coalescing
                    # tick alive without paying a scheduler hop per frame.
                    since_yield += 1
                    if since_yield >= YIELD_EVERY:
                        since_yield = 0
                        await asyncio.sleep(0)
                        if self._closed:
                            return
            previous = record.at
            self._emitted += 1
            yield record, self._profile_at(self._emitted - 1)
            # Resumes here once the consumer has fully processed the frame
            # just yielded (an `async for … : self.ingest(record)` loop does
            # not ask this generator for the next item until its own loop
            # body — the `ingest` call — has returned), which is exactly what
            # "ordered against frame indices" means: a sideband action tied to
            # this frame's index fires immediately *after* that frame has
            # been applied, deterministically, because it is this source's
            # own pacing that decides it — not a concurrent poll racing
            # against the consumer.
            self._fire_due_sideband()
        # A trailing action scheduled at or beyond the corpus length still
        # fires, once, right after the last frame — "the connection dropped
        # after the visible content ended" is a real scenario this timeline
        # must be able to express, not an off-by-one to silently drop.
        self._fire_due_sideband(flush=True)

    async def __aiter__(self) -> AsyncIterator[FrameRecord]:
        """The single-connection shape: bare records, exactly as before U8.

        A version-1 log has one connection by construction (KTD6), so there is
        nothing for a tag to say and none is invented.
        """
        async for record, _profile in self.paced():
            yield record

    def _profile_at(self, index: int) -> str:
        return self._profiles[index] if index < len(self._profiles) else ""

    def _fire_due_sideband(self, *, flush: bool = False) -> None:
        if self._closed:
            return
        while self._sideband and (flush or self._sideband[0].frame_index == self._emitted):
            action = self._sideband.popleft()
            if self._on_sideband is not None:
                self._on_sideband(action)

    async def _sleep(self, delay: float) -> None:
        """Sleep in a cancellable task so ``close()`` interrupts it at once."""
        task = asyncio.ensure_future(asyncio.sleep(delay))
        self._sleep_task = task
        try:
            await task
        except asyncio.CancelledError:
            if not self._closed:
                raise
        finally:
            self._sleep_task = None

    async def close(self) -> None:
        """Stop the source. Safe to call twice, and safe to call mid-sleep."""
        if self._closed:
            return
        self._closed = True
        # A paused source is parked on the resume event; releasing it lets the
        # loop observe ``_closed`` and return instead of hanging on teardown.
        self.controls.resume()
        task = self._sleep_task
        if task is not None and not task.done():
            task.cancel()


class TaggedReplaySource:
    """A version-2 recording, yielding the connection each frame crossed (KTD6).

    Composes a :class:`ReplaySource` rather than subclassing it, and the reason
    is the type: ``FrameSource`` yields ``FrameRecord`` and ``TaggedFrameSource``
    yields ``TaggedFrame``, so one class cannot honestly be both. Everything that
    is genuinely shared — the scaled clock, the pause handling, the sideband
    ordering — lives in :meth:`ReplaySource.paced` and is used by both.

    Satisfies :class:`~talaria.ui.app.TaggedFrameSource`, which the app's pump
    already branches on: the consumer side of this needed no change, because U7
    built it for the live fleet and a recording is simply a second producer.
    """

    def __init__(
        self,
        inner: ReplaySource,
        *,
        connections: Sequence[str] = (),
        focus_profile: str = "",
    ) -> None:
        self._inner = inner
        self._connections = tuple(connections)
        self._focus_profile = focus_profile

    @classmethod
    def from_path(
        cls, path: str | Path, *, controls: ReplayControls | None = None
    ) -> TaggedReplaySource:
        header = read_header(path)
        entries = tuple(iter_frame_log(path))
        return cls(
            ReplaySource(
                tuple(record_from_entry(entry) for entry in entries),
                controls=controls,
                profiles=profiles_from_entries(entries),
            ),
            connections=tuple(row.profile for row in header.connections),
            focus_profile=derive_focus_profile(header, entries),
        )

    @property
    def connections(self) -> tuple[str, ...]:
        """Every connection the recording declares, in header order.

        **Read by the app to mark them connected at replay start**, which is a
        recorded fact rather than an assumption: a log exists because these
        gateways answered. Without it every channel keeps
        ``ConnectionChannel.connected``'s ``False`` default and the queue says
        "connection down before it was ever polled" about a connection that was
        demonstrably live when the frames were captured. Measured before the
        fix: both connections of a two-connection replay reported exactly that.

        A connection that really did drop mid-recording is marked down again by
        the log's own terminal cause, so starting from the recorded truth costs
        nothing and starting from ``False`` costs a false sentence.
        """
        return self._connections

    @property
    def focus_profile(self) -> str:
        """Which connection's session this replay should show — see
        :func:`derive_focus_profile`. ``""`` when the log settles nothing."""
        return self._focus_profile

    @property
    def controls(self) -> ReplayControls:
        return self._inner.controls

    @property
    def emitted(self) -> int:
        return self._inner.emitted

    @property
    def closed(self) -> bool:
        return self._inner.closed

    @property
    def records(self) -> tuple[FrameRecord, ...]:
        return self._inner.records

    def __len__(self) -> int:
        return len(self._inner)

    def bind_sideband(
        self, actions: Sequence[SidebandAction], callback: Callable[[SidebandAction], None]
    ) -> None:
        self._inner.bind_sideband(actions, callback)

    async def __aiter__(self) -> AsyncIterator[TaggedFrame]:
        async for record, profile in self._inner.paced():
            yield TaggedFrame(profile, record, REPLAY_EPOCH)

    async def close(self) -> None:
        await self._inner.close()


def source_from_path(
    path: str | Path, *, controls: ReplayControls | None = None
) -> ReplaySource | TaggedReplaySource:
    """Open a recording as whichever shape its header declares.

    The version decides, and nothing else — not a guess from whether any entry
    happens to carry a tag. A version-1 log is one connection by construction
    (KTD6) and gets the bare-record source it has always had; a version-2 log
    gets the tagged one. The reader refuses any other version before this
    function sees it.
    """
    header = load_header(path)
    if header.version >= FRAME_LOG_VERSION_MULTI_CONNECTION:
        return TaggedReplaySource.from_path(path, controls=controls)
    return ReplaySource.from_path(path, controls=controls)
