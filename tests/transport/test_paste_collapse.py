"""Large-paste collapse over a real socket (R13, KTD16; AE13).

The two halves of AE13 pull against each other and both are asserted here. A
paste at or above the threshold has to *stop being several hundred lines in the
composer* — otherwise the collapse bought nothing — and a collapse that fails
has to leave every one of those lines exactly where they were, editable, with
nothing partial submitted. A client that satisfies one by giving up the other
is the failure this file is for.

The transcript clause is asserted separately: the pasted body must never appear
there. It is the buffer ``terminal.read`` serves back to the agent (KTD10), so a
paste re-rendered into it would be handed to the model twice — once as the file
the gateway spooled and once as a wall of transcript.

**No Hermes gateway was attached at any point in this run.** The stub's reply
body is transcribed from ``tui_gateway/methods_complete.py:14-39`` at
``7f4d15515``; that is what makes the round trip meaningful, and it is still a
stub.
"""

from __future__ import annotations

import asyncio
from typing import Any

import pytest
import pytest_asyncio
from textual.events import Paste

from talaria.domain.commands import PASTE_COLLAPSE_METHOD, PasteThreshold
from talaria.domain.projection import transcript_view
from talaria.transport.attach import AttachTarget
from talaria.transport.credentials import Credential
from talaria.transport.source import LiveSource
from talaria.ui.app import (
    PASTE_COLLAPSE_IN_FLIGHT,
    PASTE_COLLAPSE_REFUSED,
    PASTE_NOT_COLLAPSED,
    SUBMIT_METHOD,
    TalariaApp,
)
from tests.transport.conftest import STUB_TOKEN, StubGateway, err, event, ok
from tests.ui.conftest import screen_text

FAST_RETRIES = (0.0, 0.01, 0.01)
SCREEN = (100, 30)

#: A paste nobody would want inserted literally. The marker on the first line is
#: what the transcript and screen sweeps search for.
PASTED = "PASTE-CANARY-8xK3 begins here\n" + "\n".join(
    f"    pasted line {index}" for index in range(400)
)

#: The gateway's own placeholder shape (``methods_complete.py:33-35``).
PLACEHOLDER = "[Pasted text #1: 401 lines → /tmp/hermes/pastes/paste_1_101530.txt]"
PASTE_PATH = "/tmp/hermes/pastes/paste_1_101530.txt"


class StubProvider:
    async def acquire(self) -> Credential:
        return Credential("token", STUB_TOKEN, "endpoint-url")


def collapsing_responder(
    message: dict[str, Any], stub: StubGateway
) -> dict[str, Any] | None:
    method = message.get("method")
    rid = message.get("id")
    if method == PASTE_COLLAPSE_METHOD:
        return ok(rid, {"placeholder": PLACEHOLDER, "path": PASTE_PATH, "lines": 401})
    if method == SUBMIT_METHOD:
        return ok(rid, {"status": "ok"})
    return None


def refusing_responder(
    message: dict[str, Any], stub: StubGateway
) -> dict[str, Any] | None:
    if message.get("method") == PASTE_COLLAPSE_METHOD:
        # The gateway's own refusal code for an empty paste
        # (``methods_complete.py:18``), reused here as any refusal.
        return err(message.get("id"), 4004, "paste storage unavailable")
    return collapsing_responder(message, stub)


@pytest_asyncio.fixture
async def collapsing_gateway() -> Any:
    stub = StubGateway(responder=collapsing_responder)
    await stub.start()
    try:
        yield stub
    finally:
        await stub.stop()


@pytest_asyncio.fixture
async def refusing_gateway() -> Any:
    stub = StubGateway(responder=refusing_responder)
    await stub.start()
    try:
        yield stub
    finally:
        await stub.stop()


def live_app(
    gateway: StubGateway, *, threshold: PasteThreshold | None = None
) -> tuple[TalariaApp, LiveSource]:
    source = LiveSource(
        AttachTarget.from_url(gateway.url),
        StubProvider(),
        reconnect_delays=FAST_RETRIES,
    )
    app = TalariaApp(
        source,
        mode="live",
        dispatcher=source,
        coalesce_interval=3600.0,
        paste_threshold=threshold if threshold is not None else PasteThreshold(),
    )
    source.bind(on_connection=app.note_connection_state, on_reconnect=app.note_reconnect)
    return app, source


async def until(predicate: Any, *, timeout: float = 5.0) -> None:
    async def _poll() -> None:
        while not predicate():
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_poll(), timeout=timeout)


async def ready(gateway: StubGateway, app: TalariaApp, pilot: Any) -> None:
    await gateway.wait_for_attach()
    await until(lambda: app.state.connection == "connected")
    before = app.frames_applied
    await gateway.send(event("session.info", {"session_id": "s1"}))
    await until(lambda: app.frames_applied > before)
    await app.render_snapshot()
    await pilot.pause()


async def paste(app: TalariaApp, pilot: Any, text: str) -> None:
    """Deliver a bracketed paste the way the terminal driver does.

    Posted to the *app*, which is where a real bracketed paste lands; Textual
    forwards it to the focused widget from there.
    """
    app.composer.text_area.focus()
    app.post_message(Paste(text))
    await pilot.pause()
    await pilot.pause()
    await app.settle_live()
    await pilot.pause()


def sent(gateway: StubGateway, method: str) -> list[dict[str, Any]]:
    return [
        message.get("params") or {}
        for message in gateway.current.received
        if message.get("method") == method
    ]


def transcript_text(app: TalariaApp) -> str:
    return "\n".join(transcript_view(app.state).lines)


# ── scenario 4: the collapse ─────────────────────────────────────────────


@pytest.mark.asyncio
async def test_a_several_hundred_line_paste_collapses_to_one_line(
    collapsing_gateway: StubGateway,
) -> None:
    app, _source = live_app(collapsing_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(collapsing_gateway, app, pilot)
        await paste(app, pilot, PASTED)

        # The whole text went to the gateway, unaltered — a client that sent a
        # truncated copy would spool the wrong file and look identical here
        # without this equality.
        collapses = sent(collapsing_gateway, PASTE_COLLAPSE_METHOD)
        assert [params.get("text") for params in collapses] == [PASTED]
        # And what is left in the editor is one line: the gateway's placeholder,
        # exactly.
        assert app.composer.text == PLACEHOLDER
        assert "\n" not in app.composer.text
        assert PLACEHOLDER in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_transcript_never_re_renders_the_pasted_body(
    collapsing_gateway: StubGateway,
) -> None:
    """The transcript is what ``terminal.read`` serves back to the agent
    (KTD10). A paste written into it is handed to the model a second time."""
    app, _source = live_app(collapsing_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(collapsing_gateway, app, pilot)
        await paste(app, pilot, PASTED)
        await app.render_snapshot()
        await pilot.pause()

        transcript = transcript_text(app)
        assert "PASTE-CANARY-8xK3" not in transcript
        assert "pasted line 399" not in transcript
        assert "PASTE-CANARY-8xK3" not in screen_text(app)
        # The screen did render: the placeholder proves the negatives above are
        # not being read off a blank interface.
        assert PLACEHOLDER in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_paste_below_the_threshold_is_inserted_literally_and_sends_nothing(
    collapsing_gateway: StubGateway,
) -> None:
    """KTD4's literal insert, unchanged below KTD16's bound."""
    small = "one\ntwo\nthree"
    app, _source = live_app(collapsing_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(collapsing_gateway, app, pilot)
        await paste(app, pilot, small)

        assert app.composer.text == small
        assert sent(collapsing_gateway, PASTE_COLLAPSE_METHOD) == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_byte_bound_trips_on_a_single_wide_line(
    collapsing_gateway: StubGateway,
) -> None:
    """KTD16 needs both bounds: lines alone lets a single enormous line
    through. One line of 600 bytes is that case."""
    wide = "x" * 600
    app, _source = live_app(collapsing_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(collapsing_gateway, app, pilot)
        await paste(app, pilot, wide)

        collapses = sent(collapsing_gateway, PASTE_COLLAPSE_METHOD)
        assert [params.get("text") for params in collapses] == [wide]
        assert app.composer.text == PLACEHOLDER
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_collapsed_paste_submits_the_placeholder_and_not_the_body(
    collapsing_gateway: StubGateway,
) -> None:
    """The point of the round trip. What reaches the model is the one-line
    reference; the body lives in the file the gateway wrote."""
    app, _source = live_app(collapsing_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(collapsing_gateway, app, pilot)
        await paste(app, pilot, PASTED)
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        submitted = sent(collapsing_gateway, SUBMIT_METHOD)
        assert [params.get("text") for params in submitted] == [PLACEHOLDER]
        await app.shutdown_sources()


# ── scenario 5: the collapse fails (AE13) ────────────────────────────────


@pytest.mark.asyncio
async def test_a_refused_collapse_leaves_the_full_text_editable(
    refusing_gateway: StubGateway,
) -> None:
    """AE13. Every character is still there, the editor still accepts edits,
    and the missing capability is on screen."""
    app, _source = live_app(refusing_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(refusing_gateway, app, pilot)
        await paste(app, pilot, PASTED)

        assert app.composer.text == PASTED
        assert PASTE_NOT_COLLAPSED in app.composer.notice
        assert PASTE_NOT_COLLAPSED in screen_text(app)
        # Still editable, not a read-only wall of text.
        assert app.composer.text_area.read_only is False
        app.composer.text_area.focus()
        await pilot.press("!")
        await pilot.pause()
        assert "!" in app.composer.text
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_refused_collapse_submits_nothing_at_all(
    refusing_gateway: StubGateway,
) -> None:
    """AE13's "nothing partial submits". A client that sent the first chunk, or
    a placeholder for a file that was never written, would look the same on
    screen as one that sent nothing."""
    app, _source = live_app(refusing_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(refusing_gateway, app, pilot)
        await paste(app, pilot, PASTED)

        assert sent(refusing_gateway, SUBMIT_METHOD) == []
        assert transcript_view(app.state).lines == () or "PASTE-CANARY-8xK3" not in (
            transcript_text(app)
        )
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_collapse_reply_with_no_placeholder_is_not_applied() -> None:
    """A reply carrying a path and no placeholder is not a partial success:
    there is nothing to put in the composer, so the failure path is the correct
    handling of it."""

    def half_answer(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        if message.get("method") == PASTE_COLLAPSE_METHOD:
            return ok(message.get("id"), {"path": PASTE_PATH, "lines": 401})
        return collapsing_responder(message, stub)

    gateway = StubGateway(responder=half_answer)
    await gateway.start()
    try:
        app, _source = live_app(gateway)
        async with app.run_test(size=SCREEN) as pilot:
            await ready(gateway, app, pilot)
            await paste(app, pilot, PASTED)

            assert app.composer.text == PASTED
            assert PASTE_NOT_COLLAPSED in app.composer.notice
            await app.shutdown_sources()
    finally:
        await gateway.stop()


@pytest.mark.asyncio
async def test_enter_during_the_round_trip_submits_nothing_at_all() -> None:
    """Paste, then Enter — muscle memory, and the hole KTD16 exists to close.

    Talaria inserts the paste literally first and collapses second, so for the
    length of the round trip the composer holds the whole several-hundred-line
    body. An Enter in that window used to send all of it: ``prompt.submit``
    carried 3504 characters, which is the outcome the threshold exists to
    prevent, reached through the front door.

    The reply is withheld deliberately rather than raced, for the same reason
    the deleted-paste test withholds it: otherwise this asserts on whichever of
    the reply and the keystroke the scheduler happened to run first.
    """
    pending: list[Any] = []

    def deferring(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        if message.get("method") == PASTE_COLLAPSE_METHOD:
            pending.append(message.get("id"))
            return None
        return collapsing_responder(message, stub)

    gateway = StubGateway(responder=deferring)
    await gateway.start()
    try:
        app, _source = live_app(gateway)
        async with app.run_test(size=SCREEN) as pilot:
            await ready(gateway, app, pilot)
            app.composer.text_area.focus()
            app.post_message(Paste(PASTED))
            await until(lambda: bool(pending))

            await pilot.press("enter")
            await pilot.pause()

            assert sent(gateway, SUBMIT_METHOD) == []
            assert PASTE_COLLAPSE_IN_FLIGHT in app.composer.notice
            # Nothing was lost either: the body is still in the editor, and the
            # collapse still completes and replaces it.
            assert app.composer.text == PASTED
            await gateway.send(
                ok(pending[0], {"placeholder": PLACEHOLDER, "path": PASTE_PATH, "lines": 401})
            )
            await app.settle_live()
            await pilot.pause()
            assert app.composer.text == PLACEHOLDER

            # And the guard lifts: the same keystroke now sends the placeholder.
            app.composer.text_area.focus()
            await pilot.press("enter")
            await app.settle_live()
            await pilot.pause()
            assert [params.get("text") for params in sent(gateway, SUBMIT_METHOD)] == [
                PLACEHOLDER
            ]
            await app.shutdown_sources()
    finally:
        await gateway.stop()


@pytest.mark.asyncio
async def test_quit_still_works_while_a_collapse_is_in_flight() -> None:
    """The guard above must not take the operator's exit away.

    PC6's four are resolved before it for exactly this reason: they never touch
    a socket, so there is nothing in flight for them to be blocked behind.
    """
    pending: list[Any] = []

    def deferring(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        if message.get("method") == PASTE_COLLAPSE_METHOD:
            pending.append(message.get("id"))
            return None
        return collapsing_responder(message, stub)

    gateway = StubGateway(responder=deferring)
    await gateway.start()
    try:
        app, _source = live_app(gateway)
        async with app.run_test(size=SCREEN) as pilot:
            await ready(gateway, app, pilot)
            app.composer.text_area.focus()
            app.post_message(Paste(PASTED))
            await until(lambda: bool(pending))

            app.composer.text = "/quit"
            await pilot.press("enter")
            await pilot.pause()

            assert app.return_code == 0, "/quit did not exit"
            await app.shutdown_sources()
    finally:
        await gateway.stop()


@pytest.mark.asyncio
async def test_a_refusal_over_an_emptied_composer_does_not_claim_the_paste_is_there(
) -> None:
    """The refusal sentence is a claim about the composer, so it is only made
    after reading it.

    ``PASTE_NOT_COLLAPSED`` says "the paste was left in full". If the operator
    cleared the composer while the round trip was out, that sentence is false —
    the U8 family of defect, an assertion on screen that nothing checked. The
    gateway news is the same either way and is still said.
    """
    pending: list[Any] = []

    def deferring(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        if message.get("method") == PASTE_COLLAPSE_METHOD:
            pending.append(message.get("id"))
            return None
        return collapsing_responder(message, stub)

    gateway = StubGateway(responder=deferring)
    await gateway.start()
    try:
        app, _source = live_app(gateway)
        async with app.run_test(size=SCREEN) as pilot:
            await ready(gateway, app, pilot)
            app.composer.text_area.focus()
            app.post_message(Paste(PASTED))
            await until(lambda: bool(pending))

            app.composer.text = ""
            await gateway.send(err(pending[0], 4004, "paste storage unavailable"))
            await app.settle_live()
            await pilot.pause()

            assert app.composer.text == ""
            assert PASTE_NOT_COLLAPSED not in app.composer.notice
            # The news about the gateway is still delivered, and on screen.
            assert PASTE_COLLAPSE_REFUSED in app.composer.notice
            assert PASTE_COLLAPSE_REFUSED in screen_text(app)
            await app.shutdown_sources()
    finally:
        await gateway.stop()


@pytest.mark.asyncio
async def test_a_successful_collapse_leaves_an_unrelated_warning_on_screen(
    collapsing_gateway: StubGateway,
) -> None:
    """The notice row is shared, and a success has nothing of its own to say.

    A refused ``prompt.submit`` leaves "session is busy" there over a message
    still sitting undelivered in the composer. Clearing the row on a later
    successful collapse removed the only sign that the message had not gone.
    """
    app, _source = live_app(collapsing_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(collapsing_gateway, app, pilot)
        standing = "prompt.submit was refused by the gateway (code 5000): session is busy"
        app.composer.show_notice(standing)
        await pilot.pause()

        await paste(app, pilot, PASTED)

        assert app.composer.text == PLACEHOLDER, "the collapse itself did not happen"
        assert app.composer.notice == standing
        assert "session is busy" in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_later_success_clears_this_features_own_failure_line() -> None:
    """The other half of the rule above: a collapse failure's notice is stale
    once a collapse succeeds, and this feature does clear its own."""
    refuse_first: list[bool] = [True]

    def flaky(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        if message.get("method") == PASTE_COLLAPSE_METHOD and refuse_first[0]:
            refuse_first[0] = False
            return err(message.get("id"), 4004, "paste storage unavailable")
        return collapsing_responder(message, stub)

    gateway = StubGateway(responder=flaky)
    await gateway.start()
    try:
        app, _source = live_app(gateway)
        async with app.run_test(size=SCREEN) as pilot:
            await ready(gateway, app, pilot)
            await paste(app, pilot, PASTED)
            assert PASTE_NOT_COLLAPSED in app.composer.notice

            app.composer.text = ""
            await paste(app, pilot, PASTED)

            assert app.composer.text == PLACEHOLDER
            assert app.composer.notice == ""
            await app.shutdown_sources()
    finally:
        await gateway.stop()


@pytest.mark.asyncio
async def test_a_paste_the_operator_deleted_is_not_replaced_after_the_fact() -> None:
    """The round trip spans an RPC, and the operator is editing throughout it.
    Appending a placeholder for text that is no longer in the composer would
    submit a reference to a file nobody is looking at.

    The reply is **withheld** rather than raced against a ``pause``: the stub
    normally answers the instant it receives the request, so a test that edited
    the composer after posting the paste would be asserting on whichever of the
    two happened to win. Here the responder parks the request id, the test
    edits, and only then is the reply sent — so the window is opened
    deliberately and the assertion is about the code rather than the scheduler.
    """
    pending: list[Any] = []

    def deferring(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        if message.get("method") == PASTE_COLLAPSE_METHOD:
            pending.append(message.get("id"))
            return None
        return collapsing_responder(message, stub)

    gateway = StubGateway(responder=deferring)
    await gateway.start()
    try:
        app, _source = live_app(gateway)
        async with app.run_test(size=SCREEN) as pilot:
            await ready(gateway, app, pilot)
            app.composer.text_area.focus()
            app.post_message(Paste(PASTED))
            await until(lambda: bool(pending))

            app.composer.text = "never mind"
            await gateway.send(
                ok(pending[0], {"placeholder": PLACEHOLDER, "path": PASTE_PATH, "lines": 401})
            )
            await app.settle_live()
            await pilot.pause()

            assert app.composer.text == "never mind"
            assert PLACEHOLDER not in app.composer.text
            # The operator is told where the gateway put it, so the round trip
            # is not silently wasted.
            assert PASTE_PATH in app.composer.notice
            await app.shutdown_sources()
    finally:
        await gateway.stop()
