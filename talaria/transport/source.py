"""KTD3's one frame-source seam: ``FrameSource``.

Everything above this seam is identical code in replay and live mode, which is
what makes R31 and AE16 testable rather than aspirational. The seam is
deliberately narrow — an async iterator of :class:`FrameRecord` plus an
idempotent :meth:`FrameSource.close` — because every widening is a place where
replay and live can quietly diverge.

Why the record carries ``parse_error`` as a flag rather than a message: the
frame log records an unparseable frame as a *withheld hole* (``frame: null``
plus a categorical diagnostic, R26), and the decoder turns that flag into a
``ProtocolErrorFrame`` whose text is a fixed sentence. Passing the recorder's
diagnostic string through here would create a second path by which wire content
could reach a rendered line.

``close()`` is part of the protocol, not a courtesy. R36 requires teardown to
stop what Talaria started, and a consumer that stops iterating without closing
is a test failure rather than a tolerated pattern — :class:`ReplaySource` and
:class:`LiveSource` both assert it.

**U7 added :class:`LiveSource` to this module rather than beside it**, because
the plan's file list puts it here and because the two implementations of one
protocol are easier to keep honest when a reader can see both. What did *not*
move here is the socket: ``talaria.transport.attach`` and the ``websockets``
client library are imported lazily, inside the methods that dial, so importing
this module — which ``talaria.replay.source`` does — never loads a socket
library. ``tests/transport/test_source_equivalence.py`` pins that in a fresh
interpreter, because it is exactly the kind of claim that decays silently.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
from collections.abc import AsyncIterator, Callable, Mapping
from dataclasses import dataclass
from time import time as _wall_clock
from typing import TYPE_CHECKING, Any, Final, Literal, Protocol, runtime_checkable

from talaria.transport.rpc import (
    LOST_WITH_TRANSPORT,
    NEVER_SENT,
    NO_REPLY_IN_TIME,
    NOT_CONNECTED,
    RpcCorrelator,
    RpcOutcome,
    unknown_outcome,
)

if TYPE_CHECKING:  # pragma: no cover - types only, no runtime import
    from talaria.recorder.framelog import FrameRecorder
    from talaria.transport.attach import AttachTarget, Dialer, GatewayConnection
    from talaria.transport.credentials import CredentialProvider

#: Which way a frame travelled. Matches frame-log v1's ``dir`` field and
#: :data:`talaria.domain.models.Direction`; re-declared here so the transport
#: package does not import the domain to name a two-member enum.
Direction = Literal["in", "out"]


@dataclass(frozen=True)
class FrameRecord:
    """One frame as it crossed the seam, before any protocol interpretation.

    ``at`` is epoch seconds taken from the record that carried the frame — the
    recorded time in replay, the receive time live — never a clock read at
    render time. That is what makes replaying one corpus twice produce
    identical domain state (AE2).
    """

    seq: int
    at: float
    direction: Direction
    frame: Any
    parse_error: bool = False


@runtime_checkable
class FrameSource(Protocol):
    """An async iterator of frames that can be closed exactly once.

    Declared as a ``Protocol`` rather than an abstract base class so a test
    double is a source without inheriting anything — the seam is a shape, not a
    hierarchy.
    """

    def __aiter__(self) -> AsyncIterator[FrameRecord]:
        """Iterate frames in recorded order."""
        ...

    async def close(self) -> None:
        """Stop the source. Idempotent; cancels any in-flight read."""
        ...


# ── The live half of the seam (U7) ───────────────────────────────────────

#: R35's four visible transport states plus ``connecting``. Re-declared here
#: rather than imported from :data:`talaria.domain.models.ConnectionStatus` for
#: the same reason :data:`Direction` is: the transport package does not import
#: the domain to name an enum. ``tests/transport/test_reconnect.py`` asserts the
#: two spellings stay identical, so the duplication cannot drift.
LiveConnectionState = Literal[
    "disconnected", "connecting", "connected", "reconnecting", "auth_failed"
]

#: How R35's four *conditions* map onto the five states above, since the frozen
#: status contract (KTD5) has no ``connect_failed`` member and adding one would
#: be a ``version: 2`` change to a document external consumers already read:
#:
#: * authentication failure  -> ``auth_failed``
#: * initial connect failure -> ``disconnected`` with :attr:`LiveSource.failure_kind`
#:   ``"connect_failed"`` and a never-opened epoch
#: * disconnect              -> ``disconnected`` after an epoch was opened
#: * reconnect               -> ``reconnecting``
#:
#: The cause travels beside the state as a ``detail`` string on the callback, so
#: "could not reach the gateway" and "the gateway hung up" stay distinguishable
#: on screen without inventing a sixth enum member.
#:
#: ``credential_unavailable`` is the fourth kind and it is **not** an
#: authentication failure. It means the credential could not be acquired *here* —
#: a credential file with loose permissions, a malformed one, or no source at
#: all — so no dial was made and no gateway formed an opinion. Reporting that as
#: ``auth_failed`` put "the gateway rejected the credential" on screen for a
#: local ``chmod`` problem, sending the operator to rotate a token that was never
#: presented to anything.
DialFailureKindOrNone = Literal[
    "", "auth_failed", "connect_failed", "credential_unavailable"
]

#: KTD13's backpressure bounds — AE16's previously-missing numbers. Reads pause
#: when either is reached and resume at half.
MAX_QUEUED_FRAMES: Final[int] = 1_000
MAX_QUEUED_BYTES: Final[int] = 8 * 1024 * 1024

#: Wall-clock waits between reconnect attempts, in order. The first is zero
#: because a socket dropped by a gateway restart is usually back within the time
#: it takes to notice, and the sequence is finite because a client that retries
#: forever hides a gateway that is never coming back.
RECONNECT_DELAYS: Final[tuple[float, ...]] = (0.0, 0.5, 1.0, 2.0, 5.0)

#: Pushed onto the frame queue to end iteration. A distinct object rather than
#: ``None``, because ``None`` is a legal decoded frame body.
_END = object()

#: How an endpoint switch ended (U4, KTD5/KTD6). Every member is a state the
#: operator can be told about and act on, which is the whole requirement: a
#: switch that fails leaves the previous connection closed and the new one
#: unmade, and the one thing that must never happen is that arrangement being
#: reported as nothing at all.
#:
#: * ``switched`` — the new gateway answered and the reader is running on it.
#: * ``unsupported`` — this source was built without a per-endpoint credential
#:   resolver, so KTD6 cannot be honoured and nothing was dropped.
#: * ``refused_endpoint`` — the endpoint would not parse. Nothing was dropped.
#: * ``credential_unavailable`` — no credential could be produced *for the new
#:   endpoint*. Nothing was dialled, so no gateway formed an opinion.
#: * ``auth_failed`` — the new gateway refused the credential.
#: * ``connect_failed`` — nothing answered at the new endpoint.
#: * ``closed`` — the source is shut; a switch is not a way to reopen it.
SwitchReason = Literal[
    "switched",
    "unsupported",
    "refused_endpoint",
    "credential_unavailable",
    "auth_failed",
    "connect_failed",
    "closed",
]

#: The reasons that leave the previous connection closed and no new one made.
#: Named once so a caller can ask the question without re-deriving the set, and
#: so adding a reason forces a decision about which side of this line it is on.
DISCONNECTING_SWITCH_REASONS: Final[frozenset[str]] = frozenset(
    {"credential_unavailable", "auth_failed", "connect_failed"}
)


@dataclass(frozen=True)
class SwitchReport:
    """What an endpoint switch did, in terms a caller can put on screen.

    ``state`` is the transport state left behind, so the report answers "what
    happened" and "where am I now" together — the second question being the one
    a half-completed switch makes urgent.

    ``detail`` never carries a whole endpoint: it is either a fixed sentence or
    a value already routed through
    :func:`~talaria.transport.attach.scrub_urls`, because an endpoint is exactly
    the string an operator pastes a token into.
    """

    reason: SwitchReason
    state: LiveConnectionState
    detail: str = ""

    @property
    def ok(self) -> bool:
        return self.reason == "switched"

    @property
    def left_disconnected(self) -> bool:
        """Whether the old connection is gone and no new one replaced it."""
        return self.reason in DISCONNECTING_SWITCH_REASONS


class LiveSource:
    """KTD3's seam over an authenticated socket (R31, R35, AE8, AE16).

    Everything above this object is the same code replay drives. What it adds
    below the seam is the part replay has no analogue for: a credential
    acquired per dial, RPC correlation with honest lost outcomes, reconnect,
    backpressure, and recording of **both** directions through the U2 redaction
    boundary.

    **Connection state is published by callback, not by synthetic frames.**
    Injecting a fabricated ``gateway.disconnected`` event into the stream would
    be the cheap way to get the state into domain state, and it would put a
    frame in the recorded corpus that no gateway ever sent — poisoning the one
    artifact whose value is that it is a faithful record. So ``on_connection``
    is a separate channel, and the frame stream carries only what arrived.

    **Frames are queued, and reads pause when the queue fills.** A model reply
    arrives as hundreds of small frames in a burst (Hermes coalesces them at
    ~30fps on its side, ``tui_gateway/ws.py``), while the UI renders on a 50ms
    boundary. Without a bound, a consumer slower than the socket grows the queue
    without limit; with one, the socket's own flow control does the waiting.
    """

    def __init__(
        self,
        target: AttachTarget,
        provider: CredentialProvider,
        *,
        dialer: Dialer | None = None,
        recorder: FrameRecorder | None = None,
        correlator: RpcCorrelator | None = None,
        on_connection: Callable[[LiveConnectionState, str], None] | None = None,
        on_reconnect: Callable[[int], None] | None = None,
        clock: Callable[[], float] = _wall_clock,
        max_queued_frames: int = MAX_QUEUED_FRAMES,
        max_queued_bytes: int = MAX_QUEUED_BYTES,
        reconnect_delays: tuple[float, ...] = RECONNECT_DELAYS,
        sleep: Callable[[float], Any] = asyncio.sleep,
        credential_factory: Callable[[str], CredentialProvider] | None = None,
    ) -> None:
        self.target = target
        self.provider = provider
        #: KTD6's seam (U4): given an endpoint, produce the credential provider
        #: for **that** endpoint. Called once per switch and its result is used
        #: for every dial afterwards, so the credential is re-resolved for the
        #: gateway it is being sent to and no value survives the switch — each
        #: profile's dashboard mints its own token, and carrying one across
        #: would present profile A's credential to profile B and read the
        #: refusal as an authentication problem.
        #:
        #: ``None`` means this source cannot switch endpoints, which is the
        #: honest default: replay has no endpoints and most tests have no
        #: second gateway. A switch asked of such a source is refused by name
        #: rather than performed with the credential it already had.
        self._credential_factory = credential_factory
        self.correlator = correlator if correlator is not None else RpcCorrelator()

        self._dialer = dialer
        self._recorder = recorder
        self._on_connection = on_connection
        self._on_reconnect = on_reconnect
        self._clock = clock
        self._sleep = sleep
        self._max_queued_frames = max_queued_frames
        self._max_queued_bytes = max_queued_bytes
        self._reconnect_delays = reconnect_delays

        self._connection: GatewayConnection | None = None
        #: The correlator epoch that was opened for :attr:`_connection`, captured
        #: at dial time. The reader hands *this* to
        #: :meth:`~talaria.transport.rpc.RpcCorrelator.resolve`, never
        #: ``correlator.epoch`` — reading the correlator's own current epoch
        #: makes the stale-reply guard compare a value with itself, which is a
        #: guard that can never fire and can never be trusted (KTD13).
        self._connection_epoch = 0
        self._state: LiveConnectionState = "disconnected"
        self._queue: asyncio.Queue[Any] = asyncio.Queue()
        self._queued_bytes = 0
        self._reads_allowed = asyncio.Event()
        self._reads_allowed.set()
        self._reader: asyncio.Task[None] | None = None
        self._closed = False
        self._ended = False
        self._started = False
        self._seq = 0
        #: Serialises endpoint switches against each other. A switch drops a
        #: connection and dials a new one across several awaits; two of them
        #: interleaved would have the second cancel a reader the first is
        #: about to replace.
        self._switch_lock = asyncio.Lock()

        # ── counters, all public: every one of them is the observable trace
        # of a behaviour that is otherwise invisible from outside.
        self.frames_received = 0
        self.frames_sent = 0
        self.reconnects = 0
        self.read_pauses = 0
        self.peak_queued_frames = 0
        self.peak_queued_bytes = 0
        self.close_errors = 0
        #: How many endpoint switches succeeded (U4). Counted separately from
        #: :attr:`reconnects`: a reconnect returns to the same gateway and a
        #: switch deliberately does not, and collapsing them would make the
        #: only evidence that a switch happened indistinguishable from the
        #: socket having blinked.
        self.switches = 0
        self.last_failure: str = ""
        #: Which kind of dial failure was last observed. Kept beside the state
        #: because :data:`LiveConnectionState` deliberately has no
        #: ``connect_failed`` member — see :data:`DialFailureKindOrNone`.
        self.failure_kind: DialFailureKindOrNone = ""

    # ── observable state ─────────────────────────────────────────────────

    @property
    def state(self) -> LiveConnectionState:
        return self._state

    @property
    def epoch(self) -> int:
        return self.correlator.epoch

    @property
    def closed(self) -> bool:
        return self._closed

    @property
    def connected(self) -> bool:
        return self._state == "connected" and self._connection is not None

    @property
    def safe_url(self) -> str:
        return self.target.safe_url

    def bind(
        self,
        *,
        on_connection: Callable[[LiveConnectionState, str], None] | None = None,
        on_reconnect: Callable[[int], None] | None = None,
    ) -> None:
        """Attach the observers after construction.

        The consumer of a source is normally built *from* it — the app takes the
        source as a constructor argument — so wiring the source's callbacks to
        the app's methods at construction time is circular. This is the seam
        that breaks the cycle, rather than a mutable holder passed through two
        constructors to achieve the same thing less legibly.
        """
        if on_connection is not None:
            self._on_connection = on_connection
        if on_reconnect is not None:
            self._on_reconnect = on_reconnect

    def _set_state(self, state: LiveConnectionState, detail: str = "") -> None:
        if state == self._state and not detail:
            return
        self._state = state
        if self._on_connection is not None:
            self._on_connection(state, detail)

    # ── dialling ─────────────────────────────────────────────────────────

    async def _dial(self) -> bool:
        """Acquire a credential and dial once. Returns whether it connected.

        The credential is a local: it is acquired here, handed to
        :func:`~talaria.transport.attach.attach`, and unreachable when this
        method returns. KTD11's per-dial rule is what makes that safe — there is
        never a stored credential to leak, because there is never a stored
        credential.
        """
        from talaria.transport.attach import AttachSuccess, attach, scrub_urls
        from talaria.transport.credentials import CredentialError

        try:
            credential = await self.provider.acquire()
        except CredentialError as exc:
            # A *local* failure: nothing was dialled, so no gateway rejected
            # anything. Calling this ``auth_failed`` put "the gateway rejected
            # the credential" on screen for a credential file with the wrong
            # mode, which sends the operator to the wrong machine entirely.
            self.last_failure = scrub_urls(str(exc))
            self.failure_kind = "credential_unavailable"
            self._set_state("disconnected", self.last_failure)
            return False

        outcome = await attach(self.target, credential, dialer=self._dialer)
        if isinstance(outcome, AttachSuccess):
            self._connection = outcome.connection
            self._connection_epoch = self.correlator.open_epoch()
            self.failure_kind = ""
            self.last_failure = ""
            self._set_state("connected")
            return True

        self.last_failure = outcome.detail
        self.failure_kind = outcome.kind
        # ``connect_failed`` has no state of its own; it is ``disconnected``
        # plus a named cause, so the frozen KTD5 enum stays frozen.
        self._set_state(
            "auth_failed" if outcome.kind == "auth_failed" else "disconnected",
            outcome.detail,
        )
        return False

    async def start(self) -> LiveConnectionState:
        """Make the first connection. Idempotent; returns the resulting state.

        A closed source never dials. Without that guard, ``close()`` followed by
        iteration would open a socket during teardown — the exact thing R36 says
        must not happen, reached through the door marked "lazy start".
        """
        if self._closed or self._started:
            return self._state
        self._started = True
        self._set_state("connecting")
        if not await self._dial():
            self._end()
            return self._state
        self._reader = asyncio.create_task(self._read_loop())
        return self._state

    # ── the seam ─────────────────────────────────────────────────────────

    async def __aiter__(self) -> AsyncIterator[FrameRecord]:
        """Yield frames as they arrive, in wire order."""
        await self.start()
        while True:
            item = await self._queue.get()
            if item is _END:
                return
            record, size = item
            self._queued_bytes = max(0, self._queued_bytes - size)
            self._resume_reads_if_drained()
            yield record

    async def close(self) -> None:
        """Stop everything this source started. Idempotent (R36).

        Order matters. Reads are unblocked first so a paused reader can observe
        the close instead of parking on an event nobody will set again; the
        in-flight calls are abandoned before the socket goes, so no caller is
        left awaiting a future the closed socket can no longer answer.
        """
        if self._closed:
            return
        self._closed = True
        self._reads_allowed.set()

        self.correlator.abandon_all("the transport was closed")

        reader = self._reader
        self._reader = None
        if reader is not None and not reader.done():
            reader.cancel()

        await self._drop_connection()
        # ``auth_failed`` survives the close. Both are true afterwards — the
        # socket is shut and the credential was refused — but only one of them
        # tells the operator what to fix, and every consumer tears the source
        # down after a failed attach, so overwriting here would erase the reason
        # on the way out every single time.
        if self._state != "auth_failed":
            self._set_state("disconnected")
        self._end()

        if self._recorder is not None:
            self._recorder.close()

    async def _drop_connection(self) -> None:
        from talaria.transport.attach import scrub_urls

        connection = self._connection
        self._connection = None
        self._connection_epoch = 0
        if connection is None:
            return
        try:
            await connection.close()
        except Exception as exc:  # noqa: BLE001 - teardown never raises onward (R36)
            # Counted rather than dropped silently. R36 says teardown must not
            # raise on the way out — a close that fails on an already-dead
            # socket is the ordinary case, not an incident — but a *silent*
            # swallow would also hide a close that never happens, which is the
            # leak R36 exists to prevent.
            self.close_errors += 1
            self.last_failure = scrub_urls(str(exc)) or type(exc).__name__

    def _end(self) -> None:
        """Terminate iteration exactly once."""
        if self._ended:
            return
        self._ended = True
        self._queue.put_nowait(_END)

    # ── outbound calls ───────────────────────────────────────────────────

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome:
        """Send one JSON-RPC request and report what is known about it.

        Every exit is an :class:`~talaria.transport.rpc.RpcOutcome`, and none of
        them overstates. A call made while disconnected, a send that raised, and
        a deadline that expired are all ``unknown`` rather than errors, because
        each of them leaves open the possibility that the gateway acted.

        The outbound frame is written to the recorder **before** it is sent, so
        a send that fails still leaves a record of the attempt — and so the
        redaction boundary sees it either way (R9, R26).
        """
        connection = self._connection
        if connection is None or self._state != "connected":
            return unknown_outcome(method, NOT_CONNECTED, epoch=self.correlator.epoch)

        request, future = self.correlator.begin(method, params)
        raw = json.dumps(request.to_frame())
        self._record("out", raw)

        try:
            await connection.send(raw)
        except Exception:  # noqa: BLE001 - a write failure is an unknown outcome
            return self.correlator.discard(request, NEVER_SENT)
        self.frames_sent += 1

        if timeout is None:
            return await future
        try:
            return await asyncio.wait_for(asyncio.shield(future), timeout)
        except TimeoutError:
            return self.correlator.discard(request, NO_REPLY_IN_TIME)

    # ── the reader ───────────────────────────────────────────────────────

    async def _read_loop(self) -> None:
        while not self._closed:
            await self._reads_allowed.wait()
            connection = self._connection
            # Captured together with the connection, and passed down rather than
            # re-read: the pair is what makes a reply attributable to the socket
            # it actually arrived on.
            epoch = self._connection_epoch
            if connection is None:
                return
            try:
                message = await connection.recv()
            except asyncio.CancelledError:
                raise
            except Exception as exc:  # noqa: BLE001 - every read failure is a drop
                if self._closed:
                    return
                if not await self._handle_disconnect(exc):
                    return
                continue
            text = message if isinstance(message, str) else message.decode("utf-8", "replace")
            self._ingest(text, epoch)

    def _ingest(self, raw: str, epoch: int) -> None:
        """Record, decode, correlate, and queue one inbound frame.

        ``epoch`` is the connection's own epoch, captured when it was dialled.
        """
        self._record("in", raw)
        self.frames_received += 1
        self._seq += 1

        try:
            frame: Any = json.loads(raw)
            parse_error = False
        except json.JSONDecodeError:
            # Withheld, not previewed: the recorder already stored it as a
            # categorical hole (R26), and the decoder turns the flag into a
            # fixed sentence.
            frame = None
            parse_error = True

        if not parse_error:
            # The connection's own epoch, not ``self.correlator.epoch``. Those
            # two are equal for as long as the reader is the only thing that
            # opens epochs — but passing the correlator its own current value
            # made ``resolve``'s stale check ``epoch != self._epoch`` compare a
            # number with itself, so KTD13's guard could not fire from any live
            # path and ``stale_epoch_replies`` could only move in a hand-written
            # unit test. A guard that cannot be exercised is a guard nobody can
            # trust, which is the reason ids restart per epoch in the first
            # place (see ``transport/rpc.py``'s module docstring).
            #
            # ``seq`` is this frame's own position in the log — the same
            # counter :class:`~talaria.domain.models.GatewayEvent.seq` is
            # stamped from — carried onto the reply's ``RpcOutcome`` so a
            # caller can tell a concurrently-arriving event apart from one
            # that arrived after the reply (B3), which asyncio's own
            # scheduling order does not reliably distinguish.
            self.correlator.resolve(frame, epoch=epoch, seq=self._seq)

        record = FrameRecord(
            seq=self._seq,
            at=self._clock(),
            direction="in",
            frame=frame,
            parse_error=parse_error,
        )
        # Sized from the bytes that arrived rather than from the decoded object.
        # Measuring the object would mean serializing it again per frame, on the
        # hot path, to refine a threshold whose job is to stop unbounded growth.
        self._enqueue(record, len(raw))

    def _record(self, direction: Direction, raw: str) -> None:
        if self._recorder is not None:
            self._recorder.record(direction, raw)

    # ── backpressure (KTD13) ─────────────────────────────────────────────

    def _enqueue(self, record: FrameRecord, size: int) -> None:
        self._queue.put_nowait((record, size))
        self._queued_bytes += size
        self.peak_queued_frames = max(self.peak_queued_frames, self._queue.qsize())
        self.peak_queued_bytes = max(self.peak_queued_bytes, self._queued_bytes)
        if not self._reads_allowed.is_set():
            return
        if self._queue.qsize() >= self._max_queued_frames or (
            self._queued_bytes >= self._max_queued_bytes
        ):
            self._reads_allowed.clear()
            self.read_pauses += 1

    def _resume_reads_if_drained(self) -> None:
        """Resume when the queue is below half of *both* bounds.

        KTD13 says "resume at half of whichever bound tripped". Requiring both
        is the same rule stated conservatively: the bound that did not trip is
        already below its half, so the extra condition never delays a resume it
        should not — and it removes the state needed to remember which one it
        was.
        """
        if self._reads_allowed.is_set():
            return
        if (
            self._queue.qsize() <= self._max_queued_frames // 2
            and self._queued_bytes <= self._max_queued_bytes // 2
        ):
            self._reads_allowed.set()

    # ── reconnect (R35, F6) ──────────────────────────────────────────────

    async def _handle_disconnect(self, exc: BaseException) -> bool:
        """React to a dropped socket. Returns whether reading should continue.

        The in-flight calls are abandoned **before** the first re-dial. Doing it
        after would leave a window in which a new epoch is open while futures
        from the dead one are still outstanding — the exact arrangement in which
        a reused identifier can satisfy the wrong call. The ordering is pinned by
        ``tests/transport/test_reconnect.py``, which reads the correlator from
        inside the ``connected`` callback of the *next* epoch: anything still
        outstanding at that instant is in the window this ordering closes.
        """
        from talaria.transport.attach import classify_dial_error, scrub_urls

        self.last_failure = scrub_urls(str(exc)) or type(exc).__name__
        self.correlator.abandon(self._connection_epoch, LOST_WITH_TRANSPORT)
        await self._drop_connection()

        if classify_dial_error(exc) == "auth_failed":
            # The gateway hung up with an authentication close code. Reconnecting
            # would present the same credential to the same refusal on a timer,
            # so this stops and says which of R35's states it is in.
            self.failure_kind = "auth_failed"
            self._set_state("auth_failed", self.last_failure)
            self._end()
            return False

        self._set_state("reconnecting")

        for delay in self._reconnect_delays:
            if self._closed:
                return False
            if delay:
                await self._sleep(delay)
            if self._closed:
                return False
            self._set_state("reconnecting")
            if await self._dial():
                self.reconnects += 1
                if self._on_reconnect is not None:
                    self._on_reconnect(self.correlator.epoch)
                return True
            if self._state == "auth_failed" or self.failure_kind == "credential_unavailable":
                # A credential the gateway rejected will not become acceptable by
                # being presented again on a timer, and a credential this machine
                # cannot produce will not appear on one either — the second case
                # would additionally re-run the interactive prompt once per retry
                # slot. Stop, and say why.
                self._end()
                return False

        self._set_state("disconnected")
        self._end()
        return False

    # ── endpoint switching (U4, KTD5/KTD6) ───────────────────────────────

    async def switch_to_endpoint(self, endpoint: str) -> SwitchReport:
        """Retarget this source at a different gateway, and report what happened.

        This is what "switching profile" means in Talaria. Hermes's
        ``POST /api/profiles/active`` is never called (KTD5): it sets a sticky
        preference for later CLI invocations and does not retarget a running
        dashboard, so it would change a setting on the machine and nothing
        about the session on screen. Dialling elsewhere is the only act that
        actually moves the operator.

        **The credential is re-resolved for the new endpoint (KTD6).** The
        injected factory builds a fresh provider before anything is dropped, so
        nothing acquired for the old gateway can be presented to the new one.
        When that provider cannot produce a credential the result is
        ``credential_unavailable`` with the reason attached — the same named
        state :meth:`_dial` already reports for a local credential failure, and
        deliberately *not* ``auth_failed``: no gateway was asked, so no gateway
        refused anything.

        **The order is: check, then drop.** Every refusal that can be decided
        without a socket — a closed source, no factory, an endpoint that will
        not parse — is decided *before* the existing connection is touched, so
        those three leave the operator exactly where they were.
        :attr:`SwitchReport.left_disconnected` is what distinguishes them from
        the three that do not.

        **A failed switch does not crawl back to the old endpoint.** It could:
        the previous target is still known here. It does not, for two reasons.
        The retry would be a second dial that can fail on its own, turning one
        legible failure into two; and the operator asked to be somewhere else,
        so leaving the source pointed at what they asked for is what makes a
        second ``/profiles`` selection — or the ordinary reconnect path — retry
        the thing they wanted rather than silently undo it. What is *not*
        allowed is silence, which is why every exit is a named reason and a
        named state.
        """
        from talaria.transport.attach import AttachTarget

        if self._closed:
            return SwitchReport("closed", self._state, "the transport is closed")
        if self._credential_factory is None:
            return SwitchReport(
                "unsupported",
                self._state,
                "this session cannot switch gateways: no per-endpoint credential resolver",
            )

        async with self._switch_lock:
            target = AttachTarget.from_url(endpoint)
            if target.problem:
                # ``target.problem`` never contains the offending URL — see
                # ``AttachTarget`` — so it is safe to carry onto the screen.
                return SwitchReport("refused_endpoint", self._state, target.problem)

            provider = self._credential_factory(endpoint)

            # The reader is stopped *before* the socket is dropped. Left
            # running, its own ``recv`` would raise on the closed connection
            # and hand the drop to ``_handle_disconnect``, which would start
            # the reconnect loop against whichever target this method has by
            # then installed — two dialers racing for one connection slot.
            await self._stop_reader()
            self.correlator.abandon(self._connection_epoch, LOST_WITH_TRANSPORT)
            await self._drop_connection()

            self.target = target
            self.provider = provider
            self._set_state("connecting", f"switching to {target.safe_url}")

            if await self._dial():
                self.switches += 1
                self._reader = asyncio.create_task(self._read_loop())
                if self._on_reconnect is not None:
                    # The consumer's reconnect bookkeeping is epoch-keyed, and a
                    # switch opens a new epoch exactly as a reconnect does. It
                    # matters that this fires: the model catalogue is cached per
                    # epoch (KTD4) and a switch is precisely the case where the
                    # previous gateway's list must not be reused.
                    self._on_reconnect(self.correlator.epoch)
                return SwitchReport("switched", self._state, "")

            # ``_dial`` has already set a named state and a failure kind. The
            # report restates them rather than inventing a parallel vocabulary.
            reason: SwitchReason
            if self.failure_kind == "credential_unavailable":
                reason = "credential_unavailable"
            elif self.failure_kind == "auth_failed" or self._state == "auth_failed":
                reason = "auth_failed"
            else:
                reason = "connect_failed"
            return SwitchReport(reason, self._state, self.last_failure)

    async def _stop_reader(self) -> None:
        """Cancel the read loop and wait for it to actually be gone.

        Awaiting the cancellation rather than firing and forgetting is the
        point: a reader still inside ``recv`` when the connection is replaced
        can ingest a frame from the old gateway and stamp it with the new
        epoch, which is the one thing epochs exist to prevent.
        """
        reader = self._reader
        self._reader = None
        if reader is None or reader.done():
            return
        reader.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await reader
