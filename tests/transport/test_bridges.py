"""The five blocking-prompt round trips, over a real socket (R7, R8, R9; PC9, F2).

Every test here runs the real :class:`~talaria.ui.app.TalariaApp` over the real
:class:`~talaria.transport.source.LiveSource` against the stub gateway on
loopback. That matters more for this unit than for most: the four bridges are
the only place Talaria sends a *credential* to the gateway, and a test with a
dispatcher double asserts that a function was called with a string rather than
that a server received one.

``tests/ui/test_prompts.py`` owns the half a socket cannot reach — which control
renders, what the screen shows, which outcomes re-offer the question.
"""

from __future__ import annotations

import asyncio
import json
import re
from pathlib import Path
from typing import Any

import pytest
import pytest_asyncio

from talaria.domain.models import PromptKind
from talaria.domain.projection import PromptRow, terminal_read, transcript_view
from talaria.domain.state import fleet_row, record_local_note
from talaria.recorder.framelog import FrameRecorder
from talaria.recorder.redact import is_suspicious_key
from talaria.transport.attach import AttachTarget
from talaria.transport.credentials import Credential
from talaria.transport.source import LiveSource
from talaria.ui.app import (
    PROMPT_NO_LONGER_LIVE,
    TERMINAL_READ_UNAVAILABLE,
    TalariaApp,
)
from talaria.ui.prompts import RESPOND_METHODS, decline_value
from tests.transport.conftest import STUB_TOKEN, StubGateway, event, ok
from tests.ui.conftest import screen_text

FAST_RETRIES = (0.0, 0.01, 0.01)

#: The terminal these tests run in, and the transcript rows that leaves.
#: Written as literals because ``viewport_rows`` is the thing under test —
#: see ``test_the_served_viewport_is_the_rows_on_screen``, which is what
#: holds this pair to the actual screen.
SCREEN = (80, 24)
#: 17, not 18: A4 added a one-row help footer (``talaria/ui/app.py:HelpBar``),
#: costing one chrome row at every terminal height. B1 had reclaimed a row;
#: the footer returns it, so 18 becomes 17 (and 28 becomes 27).
SCREEN_ROWS = 17

#: The one value swept for across the frame log, the transcript, the status
#: document and every notice. Distinctive enough that a substring search over
#: raw file bytes is conclusive.
CANARY = "canary-Rm4pZ9wx-must-not-be-recorded"


class StubProvider:
    """The loopback token, minted per dial as KTD11 requires."""

    async def acquire(self) -> Credential:
        return Credential("token", STUB_TOKEN, "file")


def bridge_responder(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
    """Answer every ``*.respond`` the way ``_respond`` does on success.

    ``{"status": "ok"}`` is the literal body of the gateway's own reply
    (``tui_gateway/server.py:10239``), so a confirmed outcome here is confirmed
    for the same reason it would be in production.
    """
    if message.get("method") in set(RESPOND_METHODS.values()):
        return ok(message.get("id"), {"status": "ok"})
    return None


@pytest_asyncio.fixture
async def bridge_gateway() -> Any:
    stub = StubGateway(responder=bridge_responder)
    await stub.start()
    try:
        yield stub
    finally:
        await stub.stop()


def live_app(
    gateway: StubGateway, *, recorder: FrameRecorder | None = None
) -> tuple[TalariaApp, LiveSource]:
    source = LiveSource(
        AttachTarget.from_url(gateway.url),
        StubProvider(),
        recorder=recorder,
        reconnect_delays=FAST_RETRIES,
    )
    app = TalariaApp(
        source,
        mode="live",
        dispatcher=source,
        # The render tick is parked so every render in these tests is one a
        # test asked for. See ``tests/ui/test_prompts.py`` for why.
        coalesce_interval=3600.0,
    )
    source.bind(on_connection=app.note_connection_state, on_reconnect=app.note_reconnect)
    return app, source


async def until(predicate: Any, *, timeout: float = 5.0) -> None:
    async def _poll() -> None:
        while not predicate():
            await asyncio.sleep(0.005)

    await asyncio.wait_for(_poll(), timeout=timeout)


async def push(gateway: StubGateway, app: TalariaApp, frame: dict[str, Any]) -> None:
    """Send one event down the wire and wait for the app to have folded it."""
    before = app.frames_applied
    await gateway.send(frame)
    await until(lambda: app.frames_applied > before)


# ── the five round trips ─────────────────────────────────────────────────

#: Each bridge's request event, the registry key it lands under, the answer the
#: operator gives, and the method and field the gateway must receive.
#:
#: The last two are **literals**, transcribed from the gateway's own handler
#: registrations, and not looked up from :data:`RESPOND_METHODS` /
#: :data:`RESPOND_VALUE_FIELDS`. Reading them from the same tables the sender
#: uses makes the assertion compare the mapping with itself: renaming a field
#: renames it on both sides and the test stays green while Talaria sends a
#: password in a key the gateway does not read. Measured — that is exactly what
#: the first version of this file did.
ROUND_TRIPS: tuple[tuple[PromptKind, str, dict[str, Any], str, str, str, str], ...] = (
    (
        "approval",
        "approval.request",
        {"description": "rm -rf build", "choices": ["once", "deny"], "allow_permanent": True},
        "approval:s1#1",
        "once",
        "approval.respond",
        "choice",
    ),
    (
        "clarify",
        "clarify.request",
        {"request_id": "c-1", "question": "which branch?"},
        "c-1",
        "main",
        "clarify.respond",
        "answer",
    ),
    (
        "secret",
        "secret.request",
        {"request_id": "s-1", "env_var": "OPENAI_API_KEY", "prompt": "API key"},
        "s-1",
        CANARY,
        "secret.respond",
        "value",
    ),
    ("sudo", "sudo.request", {"request_id": "u-1"}, "u-1", CANARY, "sudo.respond", "password"),
)


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "kind,request_type,payload,request_id,answer,method,field", ROUND_TRIPS
)
async def test_each_bridge_answers_with_its_own_method_and_field(
    bridge_gateway: StubGateway,
    kind: PromptKind,
    request_type: str,
    payload: dict[str, Any],
    request_id: str,
    answer: str,
    method: str,
    field: str,
) -> None:
    app, source = live_app(bridge_gateway)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        await push(bridge_gateway, app, event(request_type, payload))
        await app.render_snapshot()

        outcome = await app.respond_live(request_id, answer)

        assert outcome is not None and outcome.confirmed
        assert outcome.result == {"status": "ok"}

        sent = [
            m
            for m in bridge_gateway.current.received
            if m.get("method", "").endswith(".respond")
        ]
        assert len(sent) == 1
        assert sent[0]["method"] == method
        assert sent[0]["params"][field] == answer
        assert app.state.prompt_for(request_id) is None
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_approval_is_the_one_bridge_addressed_by_session(
    bridge_gateway: StubGateway,
) -> None:
    """``approval.request`` carries no request id and ``approval.respond``
    resolves by session key (``tui_gateway/methods_prompt.py:887-905``). The
    key Talaria files it under is synthesized and must not reach the wire, or a
    reader concludes correlation is happening where it is not."""
    app, source = live_app(bridge_gateway)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        await push(
            bridge_gateway,
            app,
            event("approval.request", {"description": "curl | sh", "choices": ["deny"]}),
        )
        await app.render_snapshot()
        await app.respond_live("approval:s1#1", "deny")

        sent = next(
            m for m in bridge_gateway.current.received if m.get("method") == "approval.respond"
        )
        assert sent["params"] == {"session_id": "s1", "choice": "deny"}
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_approval_answer_aims_at_the_id_the_gateway_sent(
    bridge_gateway: StubGateway,
) -> None:
    """R18 as amended 2026-08-17, over the socket.

    The running revision synthesizes a ``request_id`` on every approval entry
    (``tools/approval.py:2596``) and emits it with the request; the answer
    carries it back, because the gateway removes queue heads on timeout and
    interrupt without emitting anything and an uncorrelated answer can authorize
    a command the operator was never shown.

    Delete the ``observed_request_id`` argument in ``respond_live``'s
    ``respond_params`` call and this test fails: the params come back as the
    session-only pair.
    """
    app, source = live_app(bridge_gateway)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        await push(
            bridge_gateway,
            app,
            event(
                "approval.request",
                {"request_id": "gw-uuid-1", "description": "curl | sh", "choices": ["deny"]},
            ),
        )
        await app.render_snapshot()
        # Registered under the gateway's own id, not a synthesized one.
        assert app.state.prompt_for("gw-uuid-1") is not None
        await app.respond_live("gw-uuid-1", "deny")

        sent = next(
            m for m in bridge_gateway.current.received if m.get("method") == "approval.respond"
        )
        assert sent["params"] == {
            "session_id": "s1",
            "choice": "deny",
            "request_id": "gw-uuid-1",
        }
        await app.shutdown_sources()


# ── R9: the value the operator typed never reaches disk ──────────────────


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "request_type,payload,request_id,method,field",
    [
        (
            "secret.request",
            {"request_id": "s-1", "env_var": "TOKEN"},
            "s-1",
            "secret.respond",
            "value",
        ),
        ("sudo.request", {"request_id": "u-1"}, "u-1", "sudo.respond", "password"),
        (
            "clarify.request",
            {"request_id": "c-1", "question": "paste the token"},
            "c-1",
            "clarify.respond",
            "answer",
        ),
    ],
)
async def test_a_respond_value_is_withheld_from_the_recording(
    bridge_gateway: StubGateway,
    tmp_path: Path,
    request_type: str,
    payload: dict[str, Any],
    request_id: str,
    method: str,
    field: str,
) -> None:
    """AE3, extended to the direction that actually carries credentials.

    ``docs/formats/frame-log.md`` says outright that the recorder was
    receive-only when the deny-set was written, so all four fields "travel in
    the other direction" and redaction was proven by unit test rather than by
    observed traffic. This is that traffic: a real answer, over a real socket,
    swept out of the real file the real recorder wrote.
    """
    log = tmp_path / "frames.jsonl"
    recorder = FrameRecorder(log, bridge_gateway.url)
    app, source = live_app(bridge_gateway, recorder=recorder)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        await push(bridge_gateway, app, event(request_type, payload))
        await app.render_snapshot()
        await app.respond_live(request_id, CANARY)
        await app.shutdown_sources()

    raw = log.read_bytes()
    assert CANARY.encode() not in raw

    records = [json.loads(line) for line in raw.decode().splitlines()]
    respond = next(
        r for r in records if (r.get("frame") or {}).get("method") == method
    )
    # Withheld, and *marked* as withheld: a reader must see a hole rather than
    # clean-looking data (``docs/formats/frame-log.md``).
    assert respond["frame"]["params"][field] == "[redacted]"
    # The *reason*, not just the path. Only ``password`` is also caught by the
    # key-name net, so for the other three the explicit deny-set entry is the
    # only thing standing between the operator's answer and the disk — and an
    # assertion that accepts either mechanism cannot tell a working rule from a
    # deleted one. See ``test_only_one_of_the_four_fields_has_a_second_layer``.
    assert [r["reason"] for r in respond["redactions"] if r["path"] == f"params.{field}"] == [
        f"deny-set:{method}"
    ]
    # The rest of the frame survived, or the recording is useless as evidence
    # that the answer was sent at all.
    assert respond["frame"]["method"] == method
    assert respond["dir"] == "out"


@pytest.mark.asyncio
async def test_a_terminal_read_value_is_withheld_from_the_recording(
    bridge_gateway: StubGateway,
    tmp_path: Path,
) -> None:
    """The fourth blocking bridge, over a real socket (U1).

    ``terminal.read.respond`` carries the serialized terminal buffer rather
    than an operator's typed answer, so the canary has to reach the buffer a
    different way than the other three tests use: :func:`record_local_note`,
    the same function the app itself calls for reconnect and command-output
    lines, appends a system entry that is "locally authored and never crossed
    the wire" (its own docstring). That is exactly the property this test
    needs — the canary must be *absent* from every incoming frame the
    recorder wrote, so its only path onto disk is the outbound respond this
    deny-set entry exists to catch.
    """
    log = tmp_path / "frames.jsonl"
    recorder = FrameRecorder(log, bridge_gateway.url)
    app, source = live_app(bridge_gateway, recorder=recorder)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        app.state = record_local_note(app.state, CANARY, at=app.state.last_observed_at)
        await app.render_snapshot()
        await push(
            bridge_gateway,
            app,
            event("terminal.read.request", {"request_id": "t-redact"}),
        )
        await app.render_snapshot()
        await app.settle_live()
        await app.shutdown_sources()

    raw = log.read_bytes()
    assert CANARY.encode() not in raw

    records = [json.loads(line) for line in raw.decode().splitlines()]
    respond = next(
        r for r in records if (r.get("frame") or {}).get("method") == "terminal.read.respond"
    )
    # Withheld, and *marked* as withheld: a reader must see a hole rather than
    # clean-looking data (``docs/formats/frame-log.md``).
    assert respond["frame"]["params"]["text"] == "[redacted]"
    # The *reason*, not just the path. ``text`` is one of the two fields the
    # key-name net does not catch (``talaria/recorder/redact.py:396``), so the
    # explicit deny-set entry is the sole control — an assertion that accepted
    # either mechanism could not tell a working rule from a deleted one. See
    # ``test_only_one_of_the_four_fields_has_a_second_layer``.
    assert [r["reason"] for r in respond["redactions"] if r["path"] == "params.text"] == [
        "deny-set:terminal.read.respond"
    ]
    # The rest of the frame survived, or the recording is useless as evidence
    # that the answer was sent at all.
    assert respond["frame"]["method"] == "terminal.read.respond"
    assert respond["dir"] == "out"


@pytest.mark.asyncio
async def test_the_answer_is_absent_from_every_other_surface(
    bridge_gateway: StubGateway,
) -> None:
    """The surfaces enumerated in QUEUED.md's egress audit, swept as a set
    rather than one at a time: the status document, the transcript, the retained
    transport failure string, and the composer notice."""
    app, source = live_app(bridge_gateway)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        await push(bridge_gateway, app, event("sudo.request", {"request_id": "u-1"}))
        await app.render_snapshot()
        await app.respond_live("u-1", CANARY)
        await app.render_snapshot()

        assert app.snapshot is not None
        assert CANARY not in json.dumps(app.snapshot.status.to_json_dict())
        assert all(CANARY not in entry.text for entry in app.state.transcript)
        assert all(CANARY not in summary for summary in app.snapshot.prompts.summaries)
        assert CANARY not in app.composer.notice
        assert CANARY not in source.last_failure
        assert CANARY not in repr(source.target)
        await app.shutdown_sources()


# ── F2 / KTD10: terminal-read, served from the projection ────────────────


@pytest.mark.asyncio
async def test_a_terminal_read_answers_from_the_projection_with_no_human(
    bridge_gateway: StubGateway,
) -> None:
    app, source = live_app(bridge_gateway)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        for index in range(6):
            await push(
                bridge_gateway,
                app,
                event("status.update", {"kind": "note", "text": f"line {index}"}),
            )
        await push(
            bridge_gateway,
            app,
            event("terminal.read.request", {"request_id": "t-1", "start": 1, "count": 3}),
        )

        await app.render_snapshot()
        await app.settle_live()

        sent = next(
            m
            for m in bridge_gateway.current.received
            if m.get("method") == "terminal.read.respond"
        )
        answered = json.loads(sent["params"]["text"])

        # Compared against the projection computed independently from the same
        # state — the *arguments* are what is being checked, so the comparison
        # is against a second evaluation rather than against the value the app
        # happened to send.
        expected = terminal_read(
            transcript_view(app.state),
            viewport_rows=app.viewport_rows(),
            start_line=1,
            count=3,
        )
        assert (answered["start"], answered["end"]) == (1, 4)
        assert answered["text"] == expected.text
        assert answered["total_lines"] == expected.total_lines
        assert answered["cursor_row"] is None

        # No operator was involved and none was asked to be (F2).
        assert list(app.prompts.card_ids) == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_terminal_read_with_no_window_answers_the_visible_screen(
    bridge_gateway: StubGateway,
) -> None:
    """Omitting both arguments is a different question from asking for line 0,
    and the gateway's contract says so (``tools/read_terminal_tool.py:18-31``).
    The absence has to survive the registry to be asked correctly."""
    app, source = live_app(bridge_gateway)

    async with app.run_test(size=SCREEN):
        await until(lambda: source.state == "connected")
        for index in range(40):
            await push(
                bridge_gateway,
                app,
                event("status.update", {"kind": "note", "text": f"line {index}"}),
            )
        await push(bridge_gateway, app, event("terminal.read.request", {"request_id": "t-2"}))
        await app.render_snapshot()
        await app.settle_live()

        assert app.state.prompt_for("t-2") is None  # answered and cleared

        sent = next(
            m
            for m in bridge_gateway.current.received
            if m.get("method") == "terminal.read.respond"
        )
        answered = json.loads(sent["params"]["text"])

        # Compared against the transcript and against a *literal* row count, not
        # against ``app.viewport_rows()``: reading the expected value out of the
        # code under test makes every one of these assertions true by
        # construction. ``test_the_served_viewport_is_the_rows_on_screen`` is
        # what pins the literal to the screen.
        rows = SCREEN_ROWS
        total = transcript_view(app.state).total_lines
        assert total > rows, "the precondition is a transcript longer than the screen"
        assert answered["total_lines"] == total
        assert answered["viewport_rows"] == rows
        assert (answered["start"], answered["end"]) == (total - rows, total)
        # The discriminating half: a client that read "no arguments" as "from
        # line 0" would answer with the whole scrollback.
        assert answered["start"] > 0
        assert len(answered["text"].splitlines()) == rows
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_served_viewport_is_the_rows_on_screen(
    bridge_gateway: StubGateway,
) -> None:
    """``viewport_rows`` is what an agent steps scrollback by, and it was
    compared with itself.

    Every earlier assertion took ``rows = app.viewport_rows()`` and moved the
    expectation with it, so replacing the whole function body with ``return 1``
    left the suite green — even the guard ``assert total > rows`` survives, at
    ``40 > 1``. The number is pinned here two ways that do not go through the
    function: against a literal for a named terminal size, and against a count
    of the transcript rows visible in the rendered screenshot. A constant fails
    the second size, and any constant at all fails the difference at the end.
    """
    measured: dict[int, tuple[int, int, int]] = {}

    # 17 and 27, not 18 and 28: A4 added HelpBar (see ``SCREEN_ROWS`` above).
    for index, (height, expected) in enumerate(((24, 17), (34, 27))):
        app, source = live_app(bridge_gateway)
        request_id = f"t-size-{index}"
        async with app.run_test(size=(80, height)):
            await until(lambda live=source: live.state == "connected")
            for line in range(60):
                await push(
                    bridge_gateway,
                    app,
                    event("status.update", {"kind": "note", "text": f"scrollback {line}"}),
                )
            await push(
                bridge_gateway, app, event("terminal.read.request", {"request_id": request_id})
            )
            await app.render_snapshot()
            await app.settle_live()

            # Each pass dials a new connection, so ``current.received`` holds
            # this app's frames alone.
            sent = next(
                m
                for m in bridge_gateway.current.received
                if m.get("method") == "terminal.read.respond"
            )
            assert sent["params"]["request_id"] == request_id
            answered = json.loads(sent["params"]["text"])
            on_screen = len(
                [
                    row
                    for row in screen_text(app).splitlines()
                    if re.search(r"scrollback \d+", row)
                ]
            )
            measured[height] = (expected, answered["viewport_rows"], on_screen)
            await app.shutdown_sources()

    for height, (expected, served, on_screen) in measured.items():
        assert served == expected, f"{height} rows of terminal served {served}"
        # The independent measurement: how many transcript rows the operator can
        # actually read. A number that is not this one is not "the viewport".
        assert on_screen == expected, f"{height} rows of terminal showed {on_screen}"

    # Ten more rows of terminal is ten more rows of transcript. No constant
    # satisfies this, whatever value it happens to be.
    assert measured[34][1] - measured[24][1] == 34 - 24


@pytest.mark.asyncio
async def test_a_terminal_read_is_answered_exactly_once() -> None:
    """One terminal-read is answered once, and Talaria never re-answers it.

    **Two guards, and only one of them is the one being tested here.** The wire
    is protected by the registry: :func:`respond_to_prompt` clears the prompt
    before the call goes out, so a duplicate dispatch is refused before it
    reaches a socket. That holds under any schedule, and it is why
    ``len(replies) == 1`` is an invariant rather than a race outcome.

    What the in-flight set adds is that the duplicate is never *attempted*, and
    the observable difference is ``rejected_responses`` — Talaria refusing an
    answer to a question it had already answered itself, plus the notice that
    refusal puts on screen. Measured directly, back-to-back renders with the
    reply held open: **one dispatch guarded, four unguarded**, ending in three
    self-inflicted refusals and "that prompt is no longer waiting" in the
    composer. Both preconditions have to be driven — an instant responder clears
    the registry before the next render, and a render loop that yields lets the
    spawned answer run — so the reply is held and the renders are consecutive.
    """
    release = asyncio.Event()
    replies: list[dict[str, Any]] = []

    async def slow_responder(message: dict[str, Any], stub: StubGateway) -> None:
        await release.wait()
        await stub.send(ok(message.get("id"), {"status": "ok"}))

    def responder(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        if message.get("method") == "terminal.read.respond":
            replies.append(message)
            asyncio.get_running_loop().create_task(slow_responder(message, stub))
        return None

    stub = StubGateway(responder=responder)
    await stub.start()
    try:
        app, source = live_app(stub)
        async with app.run_test():
            await until(lambda: source.state == "connected")
            await push(stub, app, event("terminal.read.request", {"request_id": "t-3"}))

            # Consecutive, with nothing between them that lets the spawned
            # answer run. A sleep here removes the race entirely.
            for _ in range(4):
                await app.render_snapshot()

            release.set()
            await app.settle_live()

            assert len(replies) == 1
            assert app.state.rejected_responses == 0
            assert app.composer.notice != PROMPT_NO_LONGER_LIVE
            await app.shutdown_sources()
    finally:
        release.set()
        await stub.stop()


@pytest.mark.asyncio
async def test_an_unavailable_projection_sends_nothing_and_says_so_locally(
    bridge_gateway: StubGateway,
) -> None:
    """KTD10's explicit clause. The bridge expires on its own after 30 seconds,
    so silence is a supported outcome; a plausible screen is a confident wrong
    answer the agent will act on.

    The order driven here is the one the guard exists for: teardown has begun
    while an answer that was already spawned is still composing itself.
    """
    app, source = live_app(bridge_gateway)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        await app.render_snapshot()
        assert app.transcript_view_for_read() is not None

        await app.shutdown_sources()
        assert app.transcript_view_for_read() is None

        row = PromptRow(request_id="t-9", kind="terminal_read", summary="terminal read requested")
        outcome = await app.answer_terminal_read(row)

        assert outcome is None
        assert not [
            m
            for m in bridge_gateway.current.received
            if m.get("method") == "terminal.read.respond"
        ]
        # Surfaced locally, on both surfaces, and the message still says what
        # went wrong rather than being scrubbed into silence.
        assert app.composer.notice.startswith(TERMINAL_READ_UNAVAILABLE)
        local = [e.text for e in app.state.transcript if e.kind == "system"]
        assert any(text.startswith(TERMINAL_READ_UNAVAILABLE) for text in local)
        # The *exception's own* words, not the constant's. Both halves of the
        # scrub rule are the claim: the message survives the redaction, and a
        # scrub that ate it would leave the constant alone and look fine.
        assert any("no terminal-read response is sent" in text for text in local)


@pytest.mark.asyncio
async def test_an_unavailable_projection_settles_the_prompt_instead_of_re_dispatching(
    bridge_gateway: StubGateway,
) -> None:
    """The recorded unavailable-projection defect, and U6's fix (R14).

    Surfacing the failure was never the missing half — leaving the prompt
    *registered* was. This method is dispatched from the render pass on sight, so
    a prompt still outstanding afterwards is re-dispatched on the very next tick:
    one failure line per tick, forever, for a question nothing will ever answer.

    Delete the ``latch_unservable_prompt`` call in ``answer_terminal_read``'s
    unavailable branch and this test fails on the first assertion — the prompt is
    still in the registry — and then again on the transcript count.
    """
    app, source = live_app(bridge_gateway)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        await push(
            bridge_gateway,
            app,
            event("terminal.read.request", {"request_id": "t-settle"}),
        )
        assert app.state.prompt_for("t-settle") is not None

        await app.shutdown_sources()
        assert app.transcript_view_for_read() is None

        row = PromptRow(
            request_id="t-settle", kind="terminal_read", summary="terminal read requested"
        )
        assert await app.answer_terminal_read(row) is None

        # Settled: out of the registry, tombstoned, and named on both surfaces.
        assert app.state.prompt_for("t-settle") is None
        assert app.state.answering_for("t-settle") is None
        latched = [e.text for e in app.state.transcript if e.kind == "prompt-expired"]
        assert len(latched) == 1
        assert latched[0].startswith("resolved-failed")
        # The row half of the same sentence, asserted here rather than deferred.
        # This comment used to say the app "still folds inbound frames with
        # ``apply_frame`` rather than routing them", so no registry row existed
        # to carry the notice. Both halves stopped being true in the commit that
        # wired ``TalariaApp.ingest`` through ``route_frame``: the run has a row,
        # and the row carries the sentence. Kept as a correction rather than a
        # silent swap, because a stale comment describing the old fold is
        # exactly what let the routing gap survive nine review rounds.
        focused_id = app.state.focused_session_id
        assert focused_id is not None
        registry_row = fleet_row(
            app.fleet, profile=app.fleet_profile, session_id=focused_id
        )
        assert registry_row is not None, "the routed frame created no row"
        assert registry_row.last_notice.startswith("resolved-failed"), (
            f"the row does not carry the failure sentence: {registry_row.last_notice!r}"
        )

        # And it cannot come back: a second dispatch finds nothing to answer, so
        # the render pass that runs on every tick writes no second failure line.
        assert await app.answer_terminal_read(row) is None
        assert len([e for e in app.state.transcript if e.kind == "prompt-expired"]) == 1


# ── R8: the expiry arrives on the wire, and the late answer sends nothing ─


@pytest.mark.asyncio
async def test_a_wire_expiry_clears_the_prompt_and_the_late_answer_sends_nothing(
    bridge_gateway: StubGateway,
) -> None:
    """All four bridges emit ``.expire`` from one place
    (``tui_gateway/server.py:2983-2998``); the shipping terminal UI handles two
    of them. This drives the pair the shipping client drops."""
    app, source = live_app(bridge_gateway)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        await push(
            bridge_gateway,
            app,
            event("clarify.request", {"request_id": "c-1", "question": "which branch?"}),
        )
        await app.render_snapshot()
        assert list(app.prompts.card_ids) == ["c-1"]

        await push(bridge_gateway, app, event("clarify.expire", {"request_id": "c-1"}))
        await app.render_snapshot()
        assert list(app.prompts.card_ids) == []

        outcome = await app.respond_live("c-1", CANARY)

        assert outcome is None
        assert not [
            m for m in bridge_gateway.current.received if m.get("method") == "clarify.respond"
        ]
        assert app.state.rejected_responses == 1
        assert any(e.kind == "prompt-expired" for e in app.state.transcript)
        await app.shutdown_sources()


# ── U2/R3: what a decline puts on the wire, read off the socket ──────────


@pytest.mark.asyncio
async def test_a_declined_approval_carries_the_explicit_deny_choice(
    bridge_gateway: StubGateway,
) -> None:
    """R3's safety clause, asserted where it can actually be falsified.

    The gateway's approval consumer blocks on ``None`` and on ``"deny"`` and
    returns *approved* for every other resolved choice
    (``tools/approval.py:3291``, ``:3320``). So the empty field value that
    declines the other three bridges would, on this one, authorize the command
    the operator pressed escape to refuse. The received frame is read off the
    socket rather than from the sender's own table, for the reason the round
    trips above are: a table compared with itself agrees with itself.
    """
    app, source = live_app(bridge_gateway)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        await push(
            bridge_gateway,
            app,
            event(
                "approval.request",
                {"description": "rm -rf /", "choices": ["once", "deny"]},
            ),
        )
        await app.render_snapshot()

        outcome = await app.respond_live(
            "approval:s1#1", decline_value("approval") or "", declined=True
        )

        assert outcome is not None and outcome.confirmed
        sent = next(
            m
            for m in bridge_gateway.current.received
            if m.get("method") == "approval.respond"
        )
        assert sent["params"] == {"session_id": "s1", "choice": "deny"}
        assert sent["params"]["choice"] != ""
        assert app.state.prompt_for("approval:s1#1") is None
        await app.shutdown_sources()


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "kind,request_type,payload,request_id,method,field",
    (
        (
            "clarify",
            "clarify.request",
            {"request_id": "c-1", "question": "which branch?"},
            "c-1",
            "clarify.respond",
            "answer",
        ),
        (
            "secret",
            "secret.request",
            {"request_id": "s-1", "env_var": "OPENAI_API_KEY", "prompt": "API key"},
            "s-1",
            "secret.respond",
            "value",
        ),
        (
            "sudo",
            "sudo.request",
            {"request_id": "u-1"},
            "u-1",
            "sudo.respond",
            "password",
        ),
    ),
)
async def test_the_other_three_bridges_decline_with_an_empty_field(
    bridge_gateway: StubGateway,
    kind: PromptKind,
    request_type: str,
    payload: dict[str, Any],
    request_id: str,
    method: str,
    field: str,
) -> None:
    """The reference client's own escape path sends the field empty
    (``ui-tui/src/app/useInputHandlers.ts:119``, ``:128`` at Hermes
    ``7f4d15515``). The field name and the method are literals here for the
    same reason the round trips above spell theirs out."""
    app, source = live_app(bridge_gateway)

    async with app.run_test():
        await until(lambda: source.state == "connected")
        await push(bridge_gateway, app, event(request_type, payload))
        await app.render_snapshot()

        outcome = await app.respond_live(
            request_id, decline_value(kind) or "", declined=True
        )

        assert outcome is not None and outcome.confirmed
        sent = [
            m
            for m in bridge_gateway.current.received
            if m.get("method", "").endswith(".respond")
        ]
        assert len(sent) == 1
        assert sent[0]["method"] == method
        assert sent[0]["params"] == {"request_id": request_id, field: ""}
        assert app.state.prompt_for(request_id) is None
        await app.shutdown_sources()


def test_only_one_of_the_four_fields_has_a_second_layer() -> None:
    """Which bridge fields the key-name net would catch if the deny-set went
    stale, stated rather than assumed.

    ``password`` is a credential-shaped name and the net catches it; ``value``,
    ``text`` and ``answer`` are not, and nothing behind the explicit rule covers
    them. That asymmetry is why the recording tests assert the *reason* a value
    was withheld: for three of the four, "withheld somehow" and "withheld by the
    rule that exists for it" are the same observation right up until the rule is
    deleted.
    """
    assert is_suspicious_key("password")
    assert not is_suspicious_key("value")
    assert not is_suspicious_key("text")
    assert not is_suspicious_key("answer")
