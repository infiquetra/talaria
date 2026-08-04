"""KTD7's startup paths over a real socket (R2, R34; AE7, AE12).

The live launcher resolves one of three startup paths and opens exactly one
session. These tests drive the real :class:`~talaria.ui.app.TalariaApp` over the
real :class:`~talaria.transport.source.LiveSource` against the stub gateway, and
assert against the **server's** received-call record: which methods were invoked,
in which order, with which parameters.

**No Hermes gateway was attached at any point in this run.** R2 — a real attach
resolving the precedence chain against a running gateway and landing in the
expected session — is therefore *not* proved here and is recorded as unmet in
``docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md``. What is proved is the
call sequence and what the interface does with each outcome.
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
from talaria.transport.source import LiveSource
from talaria.ui.app import (
    BACKGROUND_FAILED,
    COMPAT_BLOCKED,
    CREATE_METHOD,
    MOST_RECENT_METHOD,
    NO_SESSION_TO_RESUME,
    RESUME_METHOD,
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
CREATED: dict[str, Any] = {
    "session_id": "s-new-001",
    "stored_session_id": "s-new-001",
    "message_count": 0,
    "messages": [],
    "info": {"model": "hermes-4"},
}

#: What ``session.resume`` answers (``:306-699``). ``session_id`` is the id the
#: gateway actually resumed, which is why the app reads it back rather than
#: reusing what it asked for.
RESUMED: dict[str, Any] = {
    "session_id": "s-live-042",
    "resumed": "full",
    "message_count": 3,
    "messages": [],
    "info": {},
    "inflight": None,
    "running": False,
    "session_key": "k",
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
