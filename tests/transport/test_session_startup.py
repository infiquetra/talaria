"""KTD7's startup paths over a real socket (R2, R34; AE7, AE12).

The live launcher resolves one of three startup paths and opens exactly one
session. These tests drive the real :class:`~talaria.ui.app.TalariaApp` over the
real :class:`~talaria.transport.source.LiveSource` against the stub gateway, and
assert against the **server's** received-call record: which methods were invoked,
in which order, with which parameters.

**No Hermes gateway was attached at any point in this run.** R2 — a real attach
resolving the precedence chain against a running gateway and landing in the
expected session — is therefore *not* proved here. It is proved elsewhere: R2 is
graded *measured* in ``docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md``
(evidence-table row 17) on live frame-log recordings cited by digest, which is
evidence this module neither produces nor can produce. What is proved here is the
call sequence and what the interface does with each outcome.

The clause "and is recorded as unmet in [the verdict]" used to close that first
paragraph. It was true when written and stopped being true on 2026-08-04, when
Talaria attached to a real gateway; the sentence before it — that no gateway was
attached in *this* run — was and remains true, and is unchanged.
"""

from __future__ import annotations

import asyncio
from collections.abc import Callable
from typing import Any

import pytest
import pytest_asyncio

from talaria.domain.commands import CommandCatalog
from talaria.domain.startup import StartupSelection
from talaria.transport.attach import AttachTarget
from talaria.transport.compat_check import METHOD_NOT_FOUND
from talaria.transport.rpc import RpcOutcome
from talaria.transport.source import FrameRecord, LiveSource
from talaria.ui.app import (
    BACKGROUND_FAILED,
    COMPAT_BLOCKED,
    CREATE_METHOD,
    MOST_RECENT_METHOD,
    NO_SESSION_TO_RESUME,
    RESUME_METHOD,
    RESUMED_SESSION_ANNOUNCEMENT,
    RESUMED_SESSION_ANNOUNCEMENT_RUNTIME,
    SESSION_START_FAILED,
    STREAM_FAILURE_EXIT_CODE,
    TalariaApp,
)
from tests.transport.conftest import StubGateway, err, ok
from tests.transport.test_compat_baseline import (
    EXPECTED_PROBE_SET,
    FORBIDDEN_AT_STARTUP,
    HEALTHY,
    StubProvider,
)

FAST_RETRIES = (0.0, 0.01, 0.01)

#: What ``session.create`` answers, transcribed from
#: ``tui_gateway/methods_session.py:14-158`` at ``7f4d15515``.
#:
#: **``session_id`` and ``stored_session_id`` are distinct on purpose** (P1,
#: U7 round two). An earlier version of this fixture set them equal, which
#: masked ``_land_session`` never reading ``stored_session_id`` at all —
#: every assertion that happened to compare ``focused_session_id`` also
#: passed for ``session_key``, whether or not the durable id was actually
#: read. See ``test_a_bare_launch_adopts_the_durable_stored_session_id``.
CREATED: dict[str, Any] = {
    "session_id": "s-new-001",
    "stored_session_id": "s-new-stored-001",
    "message_count": 0,
    "messages": [],
    "info": {"model": "hermes-4"},
}

#: What ``session.resume`` answers (``:306-699``). ``session_id`` is the id the
#: gateway actually resumed, which is why the app reads it back rather than
#: reusing what it asked for, and ``session_key`` is the durable identity that
#: outlives the process (R6/R7).
#:
#: **The messages are real bodies, and that is a deliberate repair.** This
#: fixture used to read ``"message_count": 3`` with ``"messages": []``, which is
#: a reply no gateway can send: an empty array is the *omission* shape and it
#: arrives with ``messages_omitted: true`` (``methods_session.py:494-500``). An
#: internally inconsistent fixture meant no test in this suite ever saw resumed
#: history at all, which is how ``--resume`` shipped rendering an empty pane.
#: The three element shapes below are the ones live replies carry — text row,
#: tool row, assistant row — pinned element by element in
#: ``tests/domain/test_history_decoder.py``.
RESUMED_MESSAGES: list[dict[str, Any]] = [
    {"role": "user", "text": "what changed in the config?", "row_id": 11},
    {"role": "tool", "name": "read_file", "context": "config.toml"},
    {"role": "assistant", "text": "two keys were added.", "row_id": 12},
]

#: What the three messages above are expected to put on screen, in order.
RESUMED_LINES: tuple[str, ...] = (
    "› what changed in the config?",
    "⏺ read_file config.toml",
    "two keys were added.",
)

RESUMED: dict[str, Any] = {
    "session_id": "s-live-042",
    "resumed": "s-stored-042",
    "message_count": 3,
    "messages": RESUMED_MESSAGES,
    "messages_omitted": False,
    "info": {},
    "inflight": None,
    "running": False,
    "session_key": "s-stored-042",
    "started_at": 1785000000.0,
    "status": "idle",
}


#: The stub's responder shape: one request in, one reply frame or ``None`` out.
Responder = Callable[[dict[str, Any], StubGateway], "dict[str, Any] | None"]


def startup_responder(
    *,
    most_recent: dict[str, Any] | None = None,
    create: dict[str, Any] | None = None,
    resume: dict[str, Any] | None = None,
    refuse: str = "",
) -> Responder:
    """A gateway that answers the compatibility probes and the startup calls."""
    recent = most_recent if most_recent is not None else HEALTHY["session.most_recent"]

    def _respond(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        method = str(message.get("method", ""))
        rid = message.get("id")
        if method == refuse:
            return err(rid, 4090, "too many active sessions")
        if method == MOST_RECENT_METHOD:
            return ok(rid, recent)
        if method == CREATE_METHOD:
            return ok(rid, create if create is not None else CREATED)
        if method == RESUME_METHOD:
            return ok(rid, resume if resume is not None else RESUMED)
        if method in HEALTHY:
            return ok(rid, HEALTHY[method])
        return err(rid, METHOD_NOT_FOUND, f"unknown method: {method}")

    return _respond


def live_app(gateway: StubGateway, selection: StartupSelection) -> tuple[TalariaApp, LiveSource]:
    source = LiveSource(
        AttachTarget.from_url(gateway.url),
        StubProvider(),
        reconnect_delays=FAST_RETRIES,
    )
    app = TalariaApp(
        source,
        mode="live",
        dispatcher=source,
        startup=selection,
        coalesce_interval=3600.0,
    )
    source.bind(on_connection=app.note_connection_state, on_reconnect=app.note_reconnect)
    return app, source


async def until(predicate: Any, *, timeout: float = 10.0) -> None:
    async def _poll() -> None:
        while not predicate():
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_poll(), timeout=timeout)


def sent(gateway: StubGateway) -> list[str]:
    return [str(m.get("method", "")) for m in gateway.current.received]


def params_of(gateway: StubGateway, method: str) -> dict[str, Any]:
    for message in gateway.current.received:
        if message.get("method") == method:
            body = message.get("params")
            return dict(body) if isinstance(body, dict) else {}
    raise AssertionError(f"{method} was never sent")


@pytest_asyncio.fixture
async def gateway_for_startup() -> Any:
    stub = StubGateway(responder=startup_responder())
    await stub.start()
    try:
        yield stub
    finally:
        await stub.stop()


# ── the three paths (AE12) ───────────────────────────────────────────────


@pytest.mark.asyncio
async def test_a_bare_launch_creates_one_session_and_focuses_it(
    gateway_for_startup: StubGateway,
) -> None:
    app, _source = live_app(gateway_for_startup, StartupSelection(mode="new"))

    async with app.run_test():
        await until(lambda: app._startup_done)
        assert app.state.focused_session_id == "s-new-001"
        calls = sent(gateway_for_startup)
        assert calls.count(CREATE_METHOD) == 1
        assert RESUME_METHOD not in calls
        # Exactly one ``session.most_recent``: the read-only compatibility
        # probe. A second would mean the default path had gone looking for a
        # session to resume, which is ``--resume``'s job and not this one's.
        assert calls.count(MOST_RECENT_METHOD) == 1
        assert params_of(gateway_for_startup, CREATE_METHOD)["cols"] > 0
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_bare_launch_adopts_the_durable_stored_session_id(
    gateway_for_startup: StubGateway,
) -> None:
    """P1, U7 round two. ``session.create``'s reply carries the durable id as
    ``stored_session_id`` — never as ``session_key`` or ``resumed``, which
    only ``session.resume`` sends (``talaria/domain/compat.py:160``,
    ``:188``). Without reading it, a newly created session has no durable
    key at all, the picker can never recognize its own row as current, and
    choosing it retains-then-reseeds the same transcript.
    """
    app, _source = live_app(gateway_for_startup, StartupSelection(mode="new"))

    async with app.run_test():
        await until(lambda: app._startup_done)
        assert app.state.focused_session_id == "s-new-001"
        assert app.state.session_key == "s-new-stored-001"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_explicit_session_resumes_that_id_and_never_asks_for_the_recent_one(
    gateway_for_startup: StubGateway,
) -> None:
    app, _source = live_app(
        gateway_for_startup, StartupSelection(mode="session", session_id="s-asked-for")
    )

    async with app.run_test():
        await until(lambda: app._startup_done)
        assert params_of(gateway_for_startup, RESUME_METHOD)["session_id"] == "s-asked-for"
        calls = sent(gateway_for_startup)
        assert CREATE_METHOD not in calls, "an explicit --session created a new session"
        assert calls.count(MOST_RECENT_METHOD) == 1, (
            "an explicit --session asked the gateway which session was most recent "
            "(one call is the read-only compatibility probe; a second is a lookup)"
        )
        # The id the *gateway* resumed, not the one that was asked for: those
        # differ whenever a stored id maps onto a live one.
        assert app.state.focused_session_id == "s-live-042"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_resume_reads_the_most_recent_session_then_resumes_it(
    gateway_for_startup: StubGateway,
) -> None:
    app, _source = live_app(gateway_for_startup, StartupSelection(mode="resume"))

    async with app.run_test():
        await until(lambda: app._startup_done)
        calls = sent(gateway_for_startup)
        # Two ``session.most_recent`` calls, and the count is the assertion: one
        # is the read-only compatibility probe every launch makes, the other is
        # the lookup that only ``--resume`` performs. Asserting mere presence
        # would be satisfied by the probe alone, so this test would still pass
        # over a resume path that never looked anything up.
        assert calls.count(MOST_RECENT_METHOD) == 2
        last_lookup = len(calls) - 1 - calls[::-1].index(MOST_RECENT_METHOD)
        assert last_lookup < calls.index(RESUME_METHOD)
        assert params_of(gateway_for_startup, RESUME_METHOD)["session_id"] == "s-9f12"
        assert CREATE_METHOD not in calls
        assert app.state.focused_session_id == "s-live-042"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_resume_with_nothing_to_resume_says_so_and_creates_nothing() -> None:
    """The substitution this refuses is the one nobody notices for three turns.

    ``session.most_recent`` answers ``{"session_id": null}`` on a machine with no
    prior session (``tui_gateway/methods_session.py:234``). Quietly creating one
    would put the operator in a brand-new conversation that looks exactly like
    the resumed one until they ask it about something they said yesterday.
    """
    stub = StubGateway(responder=startup_responder(most_recent={"session_id": None}))
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="resume"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            calls = sent(stub)
            assert MOST_RECENT_METHOD in calls, "resume never asked for a target"
            assert CREATE_METHOD not in calls
            assert RESUME_METHOD not in calls
            assert app.state.focused_session_id is None
            assert any(
                NO_SESSION_TO_RESUME in entry.text for entry in app.state.transcript
            )
            await app.shutdown_sources()
    finally:
        await stub.stop()


# ── failure is reported, not swallowed ───────────────────────────────────


@pytest.mark.asyncio
async def test_a_refused_session_create_is_named_in_the_transcript() -> None:
    stub = StubGateway(responder=startup_responder(refuse=CREATE_METHOD))
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="new"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            assert app.state.focused_session_id is None
            line = next(
                entry.text
                for entry in app.state.transcript
                if entry.text.startswith(SESSION_START_FAILED)
            )
            assert "4090" in line
            assert "too many active sessions" in line
            await app.shutdown_sources()
    finally:
        await stub.stop()


@pytest.mark.asyncio
async def test_a_reply_that_names_no_session_does_not_focus_an_empty_one() -> None:
    """Focusing ``""`` would leave every later call carrying an empty session id
    and the interface claiming to be in a session that does not exist."""
    stub = StubGateway(responder=startup_responder(create={"message_count": 0}))
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="new"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            assert app.state.focused_session_id is None
            assert SESSION_START_FAILED in app.composer.notice
            await app.shutdown_sources()
    finally:
        await stub.stop()


# ── the compatibility check runs first, and only reads (R34, AE7) ────────


@pytest.mark.asyncio
async def test_the_startup_sequence_probes_before_it_opens_anything(
    gateway_for_startup: StubGateway,
) -> None:
    """Order, asserted on the wire: five read-only probes, then the open.

    Creating a session and *then* discovering the gateway is incompatible is the
    wrong order for the one operation R34 exists to avoid.
    """
    app, _source = live_app(gateway_for_startup, StartupSelection(mode="new"))

    async with app.run_test():
        await until(lambda: app._startup_done)
        calls = sent(gateway_for_startup)
        for probe in EXPECTED_PROBE_SET:
            assert calls.index(probe) < calls.index(CREATE_METHOD), (
                f"{probe} was probed after the session was created"
            )
        assert app.compat is not None and app.compat.ready
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_no_mutating_method_reaches_the_wire_beyond_the_one_that_was_asked_for(
    gateway_for_startup: StubGateway,
) -> None:
    """A launch invokes exactly one evidence-only method: the session open.

    The negative is paired with the positive that the open *did* happen, because
    a run that dialled and did nothing would satisfy "no mutating method" and
    also fail to start a session.
    """
    app, _source = live_app(gateway_for_startup, StartupSelection(mode="new"))

    async with app.run_test():
        await until(lambda: app._startup_done)
        calls = sent(gateway_for_startup)
        assert CREATE_METHOD in calls
        unexpected = [
            name for name in FORBIDDEN_AT_STARTUP if name != CREATE_METHOD and name in calls
        ]
        assert unexpected == [], f"the launch invoked {unexpected}"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_missing_gateway_method_is_put_in_front_of_the_operator() -> None:
    """AE7's UI half: the gap is named on screen, and the launch continues.

    Blocking the *verdict* is what AE7 asks for. Refusing to start would be a
    different and worse product decision — the operator would lose the session
    for a response key that grew.
    """

    def _missing_agents(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        if message.get("method") == "agents.list":
            return err(message.get("id"), METHOD_NOT_FOUND, "unknown method: agents.list")
        return startup_responder()(message, stub)

    stub = StubGateway(responder=_missing_agents)
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="new"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            assert app.compat is not None and not app.compat.ready
            assert app.compat.missing == ("agents.list",)
            line = next(
                entry.text
                for entry in app.state.transcript
                if entry.text.startswith(COMPAT_BLOCKED)
            )
            assert "agents.list" in line
            # Still opened, and still usable: the gap blocks the verdict, not
            # the client.
            assert app.state.focused_session_id == "s-new-001"
            await app.shutdown_sources()
    finally:
        await stub.stop()


# ── reconnect must not re-open the session (F6) ──────────────────────────


@pytest.mark.asyncio
async def test_a_reconnect_never_reopens_the_session(
    gateway_for_startup: StubGateway,
) -> None:
    """The session survives a dropped socket; a second ``session.create`` would
    abandon the operator's conversation for a fresh one, silently.

    The whole startup sequence is guarded, not just the create: no probe and no
    open is repeated on the second connection. Asserting the *empty* call log of
    the reconnected session alone would be satisfied by a reconnect that never
    happened, so the reconnect itself is asserted first, from the transport's
    own counter and from the marker the app writes.
    """
    app, source = live_app(gateway_for_startup, StartupSelection(mode="new"))

    async with app.run_test():
        await until(lambda: app._startup_done)
        assert sent(gateway_for_startup).count(CREATE_METHOD) == 1

        await gateway_for_startup.hang_up()
        await until(lambda: source.reconnects >= 1)
        await gateway_for_startup.wait_for_attach()
        assert len(gateway_for_startup.sessions) == 2, "the socket never came back"
        assert any(
            entry.text == "reconnected to the gateway" for entry in app.state.transcript
        )

        await asyncio.sleep(0.1)
        second = sent(gateway_for_startup)
        assert CREATE_METHOD not in second, "the reconnect created a second session"
        assert RESUME_METHOD not in second
        assert app.state.focused_session_id == "s-new-001"
        await app.shutdown_sources()


# ── a background task that dies must not leave a healthy-looking client ──


class ExplodingDispatcher:
    """Raises rather than returning an outcome. Records what it was asked for.

    A real :class:`~talaria.transport.source.LiveSource` is total — every exit
    from ``call`` returns an :class:`~talaria.transport.rpc.RpcOutcome` — so
    reaching this state takes a defect *inside* the startup sequence or the
    catalogue fetch rather than a misbehaving gateway. That is exactly the case
    the supervision exists for: the failure that is nobody's plan.
    """

    def __init__(self) -> None:
        self.asked: list[str] = []

    async def call(self, method: str, params: Any = None, *, timeout: Any = None) -> Any:
        self.asked.append(method)
        raise RuntimeError("induced startup-sequence failure")


class IdleSource:
    """Never yields a frame and never ends, so the pump cannot end the app."""

    def __init__(self) -> None:
        self.closed = False

    async def __aiter__(self) -> Any:
        while True:  # pragma: no cover - cancelled at teardown
            await asyncio.sleep(3600)
            yield None

    async def close(self) -> None:
        self.closed = True


@pytest.mark.asyncio
async def test_a_startup_sequence_that_raises_is_named_and_brings_the_app_down() -> None:
    """The ``_pump`` defect, one function away, measured before it was fixed.

    ``begin_live_startup`` fires ``asyncio.create_task`` and nothing awaits the
    result outside teardown. An exception inside it used to land in a future
    nobody retrieved: measured, the task finished with a ``RuntimeError``, the
    app kept running with ``return_code`` ``None``, ``compat`` was ``None``, the
    session was never opened, and the transcript said nothing — a client that
    looks connected and is attached to nothing. asyncio's own
    "Task exception was never retrieved" went to stderr, underneath a
    full-screen Textual app.

    Three observations, so a client that simply failed to start cannot pass:
    the sequence really was attempted (the dispatcher was asked for the first
    probe), the failure is named in the transcript, and the process exit code is
    the stream-failure one rather than a clean zero.
    """
    dispatcher = ExplodingDispatcher()
    app = TalariaApp(
        IdleSource(),
        mode="live",
        dispatcher=dispatcher,
        startup=StartupSelection(mode="new"),
        coalesce_interval=3600.0,
    )
    # Pre-satisfy the mount-time catalogue fetch so it does not also explode and
    # win the race to report. The sibling task is exercised on its own below,
    # which is what keeps the two paths mutated separately.
    app.catalog = CommandCatalog(available=True)

    async with app.run_test():
        app.begin_live_startup()
        await until(lambda: bool(app.background_failure))

        assert dispatcher.asked, "the startup sequence never reached the dispatcher"
        assert "the live startup sequence" in app.background_failure
        assert "induced startup-sequence failure" in app.background_failure
        assert any(
            entry.text.startswith(BACKGROUND_FAILED) for entry in app.state.transcript
        ), "nothing in the transcript said the startup sequence had died"
        await until(lambda: app.return_code == STREAM_FAILURE_EXIT_CODE)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_catalogue_fetch_that_raises_is_named_too() -> None:
    """The same defect in the sibling task, mutated separately (lesson 5).

    ``fetch_catalog`` starts its own unawaited task. Supervising only the
    startup sequence would leave this one silent, and one shared fix has to be
    proved on both paths or half of it is untested.
    """
    dispatcher = ExplodingDispatcher()
    app = TalariaApp(
        IdleSource(), mode="live", dispatcher=dispatcher, coalesce_interval=3600.0
    )

    async with app.run_test():
        app.fetch_catalog()
        await until(lambda: bool(app.background_failure))

        assert app.startup is None, "this app was given a startup selection after all"
        assert "the catalogue fetch" in app.background_failure
        await until(lambda: app.return_code == STREAM_FAILURE_EXIT_CODE)
        await app.shutdown_sources()


# ── U6: the resumed conversation reaches the transcript and the screen ───


@pytest.mark.asyncio
async def test_resume_puts_the_resumed_conversation_on_the_rendered_screen(
    gateway_for_startup: StubGateway,
) -> None:
    """R6, and the gap the grounding found: no test asserted resumed history.

    The assertion is taken from the **pane**, not from the projection, because
    a projection that is correct while the pane never mounts it is exactly the
    shape the shipped defect had.
    """
    app, _source = live_app(gateway_for_startup, StartupSelection(mode="resume"))

    async with app.run_test() as pilot:
        await until(lambda: app._startup_done)
        await pilot.pause()
        await app.render_snapshot()
        await pilot.pause()

        assert [(e.kind, e.text) for e in app.state.transcript] == [
            # B5: the resumed session names itself before its history lands
            # (plan ``2026-08-11-v0-3-unit-b5-resumed-session-names-itself``).
            (
                "system",
                RESUMED_SESSION_ANNOUNCEMENT.format(session_key="s-stored-042"),
            ),
            ("user", "what changed in the config?"),
            ("tool", "⏺ read_file config.toml"),
            ("assistant", "two keys were added."),
        ]
        assert app.transcript.rendered_lines == (
            f"— {RESUMED_SESSION_ANNOUNCEMENT.format(session_key='s-stored-042')}",
            *RESUMED_LINES,
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_resume_keeps_the_runtime_id_and_the_durable_one_apart(
    gateway_for_startup: StubGateway,
) -> None:
    """R6/R7. The reply's ``session_id`` is what the gateway streams events for;
    its ``session_key`` is what survives the process and names the session in
    the picker. Reading one off the other points the switcher at an id the
    gateway forgets when the socket closes."""
    app, _source = live_app(gateway_for_startup, StartupSelection(mode="resume"))

    async with app.run_test():
        await until(lambda: app._startup_done)
        assert app.state.focused_session_id == "s-live-042"
        assert app.state.session_key == "s-stored-042"
        assert app.state.focused_session_id != app.state.session_key, (
            "the fixture no longer distinguishes the two ids, so this proves nothing"
        )

        # The discriminating half: correlation runs on the runtime id. An event
        # stamped with the durable id is another session's traffic as far as
        # this connection is concerned, and a client that correlated on
        # ``session_key`` would fold it and drop the real stream instead.
        before = app.state.cross_session_events_ignored
        await gateway_for_startup.send(
            {
                "method": "event",
                "params": {
                    "type": "message.complete",
                    "session_id": "s-stored-042",
                    "payload": {"text": "addressed to the durable id"},
                },
            }
        )
        await until(lambda: app.state.cross_session_events_ignored > before)
        assert all(
            e.text != "addressed to the durable id" for e in app.state.transcript
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_event_racing_the_resume_reply_lands_after_the_seeded_history() -> None:
    """KTD2's landing barrier, end to end over a real socket.

    The stub writes the ``session.resume`` reply and the next event back to back
    on the same connection, which is what a busy gateway does: the turn that was
    already running keeps streaming while the resume is answered.

    **What this test does and does not prove.** It proves the outcome — the
    event lands after the history it follows — and it proves the barrier is
    *engaged* on the real startup path, by asserting that inbound frames were
    actually held while the landing was in flight. It does **not** reproduce the
    losing schedule: with this stub the awaiting coroutine reliably resumes
    before the event is read off the socket, so the ordering assertion here
    passes with the barrier removed as well. The mechanism is pinned
    deterministically by
    :func:`test_a_frame_arriving_during_a_landing_is_folded_after_the_seed`,
    which fails without the barrier. Both are kept: one says the real path is
    correct, the other says why.
    """
    live_line = "the turn that was already running"
    stub = StubGateway(
        responder=startup_responder(),
        follow_ups={
            RESUME_METHOD: [
                {
                    "method": "event",
                    "params": {
                        "type": "message.complete",
                        "session_id": "s-live-042",
                        "payload": {"text": live_line},
                    },
                }
            ]
        },
    )
    await stub.start()
    held: list[FrameRecord] = []
    try:
        app, _source = live_app(stub, StartupSelection(mode="resume"))
        folded = app.ingest

        def _watch(record: FrameRecord) -> None:
            if app._landing_depth and record.direction == "in":
                held.append(record)
            folded(record)

        app.ingest = _watch  # type: ignore[method-assign]
        async with app.run_test():
            await until(lambda: app._startup_done)
            await until(lambda: any(e.text == live_line for e in app.state.transcript))

            texts = [e.text for e in app.state.transcript]
            # B5: the resumed session's self-announcement leads the resumed
            # history (plan ``2026-08-11-v0-3-unit-b5-resumed-session-names-itself``).
            assert texts[0] == RESUMED_SESSION_ANNOUNCEMENT.format(
                session_key="s-stored-042"
            ), texts
            assert texts[1:4] == [
                "what changed in the config?",
                "⏺ read_file config.toml",
                "two keys were added.",
            ], texts
            assert texts[-1] == live_line, (
                "the event that followed the reply landed before the history it "
                f"follows: {texts}"
            )
            assert len(texts) == 5, texts
            assert held, (
                "no inbound frame arrived while the landing was in flight, so this "
                "run did not exercise the barrier at all"
            )
            await app.shutdown_sources()
    finally:
        await stub.stop()


@pytest.mark.asyncio
async def test_a_frame_arriving_during_a_landing_is_folded_after_the_seed() -> None:
    """The barrier's mechanism, driven rather than raced (KTD2).

    The window is real but narrow: the transport resolves the RPC future
    (``transport/source.py:589``) and enqueues later frames independently
    (``:601``) while the frame pump runs concurrently, so which of the two wins
    is a scheduling accident that depends on how many await layers sit between
    the reply and the coroutine that seeds. A test that hopes to lose that race
    proves nothing on the runs where it wins, so this one holds the landing open
    and delivers the frame inside it — the same sequence, with the timing taken
    out.

    Without the barrier this fails: the event is folded first, the reducer
    adopts its session id, and the resumed history is appended *below* a line
    from the turn that came after it.
    """
    app: TalariaApp = TalariaApp(IdleSource(), mode="live", coalesce_interval=3600.0)
    live_line = "the turn that was already running"
    event = FrameRecord(
        seq=1,
        at=1785000000.0,
        direction="in",
        frame={
            "method": "event",
            "params": {
                "type": "message.complete",
                "session_id": "s-live-042",
                "payload": {"text": live_line},
            },
        },
    )

    async with app.run_test():
        with app._landing():
            app.ingest(event)
            during = [entry.text for entry in app.state.transcript]
            assert during == [], f"the barrier let the event through: {during}"
            app._land_session(
                RpcOutcome(
                    status="ok",
                    method=RESUME_METHOD,
                    request_id="1",
                    epoch=1,
                    result=RESUMED,
                )
            )
        assert [entry.text for entry in app.state.transcript] == [
            # B5: the resumed session names itself first, before the seeded
            # history and the folded event (plan
            # ``2026-08-11-v0-3-unit-b5-resumed-session-names-itself``).
            RESUMED_SESSION_ANNOUNCEMENT.format(session_key="s-stored-042"),
            "what changed in the config?",
            "⏺ read_file config.toml",
            "two keys were added.",
            live_line,
        ]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_resume_that_withheld_its_history_says_so_on_screen() -> None:
    """The gateway's omission shape is an **empty** ``messages`` array carrying
    the full ``message_count`` (``methods_session.py:494-500``). A client that
    renders that as an empty pane tells the operator their conversation is
    gone."""
    withheld = {
        **RESUMED,
        "message_count": 7,
        "messages": [],
        "messages_omitted": True,
    }
    stub = StubGateway(responder=startup_responder(resume=withheld))
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="resume"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            assert len(app.state.transcript) == 2
            # B5: the resumed session names itself before the withheld-history
            # notice (plan ``2026-08-11-v0-3-unit-b5-resumed-session-names-itself``).
            assert app.state.transcript[0].kind == "system"
            assert app.state.transcript[0].text == RESUMED_SESSION_ANNOUNCEMENT.format(
                session_key="s-stored-042"
            )
            line = app.state.transcript[1]
            assert line.kind == "system"
            assert "7 earlier messages" in line.text
            await app.shutdown_sources()
    finally:
        await stub.stop()


@pytest.mark.asyncio
async def test_a_new_session_seeds_nothing_and_says_nothing_about_history() -> None:
    """``session.create`` answers ``message_count: 0`` with an empty array and
    no ``messages_omitted`` key at all. Nothing to seed, and nothing withheld."""
    stub = StubGateway(responder=startup_responder())
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="new"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            assert app.state.focused_session_id == "s-new-001"
            assert app.state.transcript == ()
            await app.shutdown_sources()
    finally:
        await stub.stop()


# ── B5: a resumed session names itself on arrival ────────────────────────


@pytest.mark.asyncio
async def test_a_resume_landing_names_the_resumed_session_once(
    gateway_for_startup: StubGateway,
) -> None:
    """AE1. A confirmed ``--resume`` landing appends exactly one system row,
    naming the resumed session by its durable id — the id the picker names a
    session by and a later resume asks for (KTD2)."""
    app, _source = live_app(gateway_for_startup, StartupSelection(mode="resume"))

    async with app.run_test():
        await until(lambda: app._startup_done)
        system_rows = [e.text for e in app.state.transcript if e.kind == "system"]
        assert system_rows == [
            RESUMED_SESSION_ANNOUNCEMENT.format(session_key="s-stored-042")
        ], system_rows
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_picker_switch_to_a_different_session_announces_the_same_row(
    gateway_for_startup: StubGateway,
) -> None:
    """AE2, both halves. A picker-driven switch to a *different* session lands
    through the same ``session.resume`` path ``--resume`` uses (KTD3) and
    appends the same announcement row; a ``--new`` session appends none — the
    operator created it, so there is no identity question."""
    app, _source = live_app(gateway_for_startup, StartupSelection(mode="new"))

    async with app.run_test():
        await until(lambda: app._startup_done)
        assert app.state.focused_session_id == "s-new-001"
        # Asserted through a local so mypy's empty-tuple narrowing cannot
        # survive the switch and poison the comprehension below.
        new_transcript = app.state.transcript
        assert len(new_transcript) == 0, "--new announced a resumed session"

        # The picker's own call (``switch_session`` is what the dismissal
        # handler invokes), to a session that is not the one focused.
        await app.switch_session("s-live-042")
        assert app.state.focused_session_id == "s-live-042"
        system_rows = [e.text for e in app.state.transcript if e.kind == "system"]
        assert system_rows == [
            RESUMED_SESSION_ANNOUNCEMENT.format(session_key="s-stored-042")
        ], system_rows
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_switch_to_the_already_focused_session_announces_nothing(
    gateway_for_startup: StubGateway,
) -> None:
    """AE2a (KTD3b). Landing the session already focused keeps the transcript
    exactly as it is — no announcement, no reseed — so repeated picker presses
    cannot add rows to a transcript the release is trying to quieten."""
    app, _source = live_app(gateway_for_startup, StartupSelection(mode="resume"))

    async with app.run_test():
        await until(lambda: app._startup_done)
        before = [(e.kind, e.text) for e in app.state.transcript]
        assert before[0] == (
            "system",
            RESUMED_SESSION_ANNOUNCEMENT.format(session_key="s-stored-042"),
        )

        await app.switch_session("s-live-042")
        assert app.state.focused_session_id == "s-live-042"
        assert [(e.kind, e.text) for e in app.state.transcript] == before, (
            "landing the already-focused session changed the transcript"
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_announcement_precedes_the_seeded_history(
    gateway_for_startup: StubGateway,
) -> None:
    """AE3. The announcement lands *before* the resumed conversation, never
    interleaved with it — KTD3a's insertion point (after ``land_session``,
    before ``seed_history``)."""
    app, _source = live_app(gateway_for_startup, StartupSelection(mode="resume"))

    async with app.run_test():
        await until(lambda: app._startup_done)
        texts = [e.text for e in app.state.transcript]
        assert texts[0] == RESUMED_SESSION_ANNOUNCEMENT.format(
            session_key="s-stored-042"
        ), texts
        assert texts[1:4] == [
            "what changed in the config?",
            "⏺ read_file config.toml",
            "two keys were added.",
        ], texts
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_resume_reply_without_a_durable_id_names_the_runtime_id() -> None:
    """AE4. A resume reply carrying no durable id still announces, naming the
    runtime id and labelling it as such. No silent path."""
    no_durable = {**RESUMED}
    no_durable.pop("session_key")
    no_durable.pop("resumed")
    # ``stored_session_id`` is the third durable key ``_land_session`` reads
    # and was never present in ``RESUMED`` — after the two pops, all three
    # are absent.
    assert "stored_session_id" not in no_durable
    stub = StubGateway(responder=startup_responder(resume=no_durable))
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="resume"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            assert app.state.focused_session_id == "s-live-042"
            system_rows = [e.text for e in app.state.transcript if e.kind == "system"]
            assert system_rows == [
                RESUMED_SESSION_ANNOUNCEMENT_RUNTIME.format(session_id="s-live-042")
            ], system_rows
            await app.shutdown_sources()
    finally:
        await stub.stop()


@pytest.mark.asyncio
async def test_a_refused_resume_announces_nothing() -> None:
    """AE5. An unconfirmed landing appends nothing — the refusal still reports
    through ``_report_startup_failure``, and no announcement row appears."""
    stub = StubGateway(responder=startup_responder(refuse=RESUME_METHOD))
    await stub.start()
    try:
        app, _source = live_app(stub, StartupSelection(mode="resume"))
        async with app.run_test():
            await until(lambda: app._startup_done)
            assert app.state.focused_session_id is None
            assert any(
                e.text.startswith(SESSION_START_FAILED) for e in app.state.transcript
            ), "the refusal was not reported"
            assert all(
                "resumed session" not in e.text for e in app.state.transcript
            ), "a refused landing announced itself"
            await app.shutdown_sources()
    finally:
        await stub.stop()
