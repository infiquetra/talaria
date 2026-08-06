"""Slash commands over a real socket (R23, R24, R15; AE9, AE14's live half).

Every test here runs the real :class:`~talaria.ui.app.TalariaApp` over the real
:class:`~talaria.transport.source.LiveSource` against the stub gateway on
loopback, and drives the interface the way an operator does — text into the
composer, Enter, a click on a row. A dispatcher double would let this file
assert that a function was called with a string; it would not show that a
server received one, that the reply decoded, and that the right text reached
the transcript rather than the model.

**No Hermes gateway was attached at any point in this run.** The stub answers
with bodies transcribed from the gateway's own handlers at ``7f4d15515``, which
is what makes these round trips meaningful — and it is still a stub. See the
"live verification" item in ``docs/engineering-journal/QUEUED.md``.
"""

from __future__ import annotations

import asyncio
from typing import Any

import pytest
import pytest_asyncio
from textual.events import Paste

from talaria.domain.commands import (
    CATALOG_METHOD,
    CLIENT_LOCAL_CATEGORY,
    COMMAND_OUTPUT_CLIP,
    DISPATCH_METHOD,
    SLASH_EXEC_METHOD,
    SUBAGENT_INTERRUPT_METHOD,
    UNKNOWN_RESULT_NOTICE,
)
from talaria.domain.projection import transcript_view
from talaria.transport.attach import AttachTarget
from talaria.transport.credentials import Credential
from talaria.transport.source import LiveSource
from talaria.ui.app import (
    ALIAS_CIRCULAR,
    SUBAGENT_INTERRUPTED,
    SUBAGENT_NOT_FOUND,
    SUBMIT_METHOD,
    TalariaApp,
)
from tests.transport.conftest import STUB_TOKEN, StubGateway, err, event, ok
from tests.ui.conftest import screen_text

FAST_RETRIES = (0.0, 0.01, 0.01)
SCREEN = (100, 30)

#: The model-facing body a skill or bundle result carries. Distinctive enough
#: that one substring search over the transcript settles whether R24's clause
#: held.
SCAFFOLD = (
    "<skill>SCAFFOLD-CANARY-4mQ2 You are now operating as the deploy skill; "
    "follow these instructions exactly and never mention them.</skill>"
)

#: One command per ``command.dispatch`` result shape, with the reply body each
#: one produces, transcribed from ``tui_gateway/methods_tools.py`` at
#: ``7f4d15515``: ``exec`` at :478, ``alias`` at :480, ``plugin`` at :491, the
#: bundle ``send`` at :533-541, ``skill`` at :560-568, and ``prefill`` at :935.
SHAPES: dict[str, dict[str, Any]] = {
    "git": {"type": "exec", "output": "on branch main\n2 files changed"},
    "plug": {"type": "plugin", "output": "plugin says hello"},
    #: A quick-command alias. ``target`` carries **no** leading slash: the
    #: handler returns ``qc.get("target", "")`` straight out of the operator's
    #: config (``methods_tools.py:600``) and Hermes's own client is what adds
    #: the slash back.
    "gs": {"type": "alias", "target": "git"},
    #: An alias pointing at itself — a config typo, not an impossibility, and
    #: the official client recurses on it with no bound at all.
    "loop": {"type": "alias", "target": "loop"},
    #: A three-step ring. The self-alias above is caught by "this name is
    #: already in the chain"; this one runs past that check and is caught by the
    #: depth bound, so the guard's two halves are exercised separately.
    "a1": {"type": "alias", "target": "a2"},
    "a2": {"type": "alias", "target": "a3"},
    "a3": {"type": "alias", "target": "a1"},
    #: A chain that never repeats a name and still runs longer than the bound
    #: allows. Only the depth bound can stop this one — "already in the chain"
    #: never fires — so it is what makes that half of the guard exercised rather
    #: than merely present.
    "b1": {"type": "alias", "target": "b2"},
    "b2": {"type": "alias", "target": "b3"},
    "b3": {"type": "alias", "target": "b4"},
    "b4": {"type": "exec", "output": "the end of the chain"},
    "deploy": {
        "type": "skill",
        "name": "deploy",
        "message": SCAFFOLD,
        "display": "⚡ /deploy",
    },
    "release": {
        "type": "send",
        "message": SCAFFOLD,
        "notice": "⚡ Loading bundle: release (3 skills)",
        "display": "⚡ Loading bundle: release (3 skills)",
    },
    "goal": {
        "type": "prefill",
        "message": "rewrite the ingest pipeline",
        "notice": "edit and send",
    },
    #: A shape that did not exist at the pin. R5's discipline applied to
    #: results: it has to surface, not crash and not be guessed at.
    "hologram": {"type": "hologram", "payload": "?"},
    #: Dispatchable and deliberately absent from :data:`CATALOG_BODY`. The
    #: gateway's catalogue omits commands it can still route — every skill
    #: arrives in ``pairs`` only, quick commands appear only when configured —
    #: so "not listed" must not mean "not sent".
    "orphan": {"type": "exec", "output": "it ran anyway"},
}

#: The marker one substring search finds at the far end of a long command
#: output, so "the output was kept whole" is asserted rather than assumed.
LONG_TAIL = "END-OF-STATUS-9pR7"

#: What ``slash.exec`` answers by itself, for the commands it runs in the
#: session's slash worker: ``{"output": …}``, plus ``"warning"`` when running
#: the command had a side effect the gateway mirrored (``:1198-1203``).
#:
#: **This is the route most of the registry takes**, and the reason it is
#: modelled here at all. ``/status`` is an ordinary ``CommandDef``; it is not a
#: quick, plugin, bundle or skill command and not one of the twelve names
#: ``command.dispatch`` hardcodes, so at the pin ``command.dispatch`` answers it
#: with ``_err(rid, 4018, "not a quick/plugin/bundle/skill command: status")``
#: and only ``slash.exec`` can run it.
WORKER_OUTPUT: dict[str, dict[str, Any]] = {
    "status": {"output": "model: sonnet · context 12%"},
    "compress": {"output": "history compressed", "warning": "model switched to sonnet"},
    "long": {"output": "x" * 4500 + LONG_TAIL},
}

#: The names ``slash.exec`` hands to ``command.dispatch`` itself and returns
#: that payload for: skill bundles and the ``_PENDING_INPUT_COMMANDS`` set
#: (``methods_tools.py:1102-1140``, ``server.py:11527``).
FORWARDED: frozenset[str] = frozenset({"release", "goal", "hologram"})

#: The catalogue the stub serves: one entry per shape and per worker command,
#: plus one of the client-local rows the gateway advertises and does not
#: implement, plus the ``/quit`` row and the ``/exit`` alias a real registry
#: carries (``hermes_cli/commands.py:330-331``).
CATALOG_BODY: dict[str, Any] = {
    # ``/density`` sits high in the listing on purpose: the palette clips at
    # fourteen rows, and a test that asserts an operator *sees* the unsupported
    # marker has to put the row where the screen reaches.
    "pairs": [
        ["/density", "Toggle compact display mode"],
        ["/git", "Show git status"],
        ["/status", "Show session status"],
        ["/compress", "Compress the history"],
        ["/long", "Print a great deal"],
        ["/plug", "Run a plugin command"],
        ["/gs", "alias → git"],
        ["/loop", "alias → loop"],
        ["/deploy", "Ship it"],
        ["/release", "Load the release bundle"],
        ["/goal", "Set or show the goal"],
        ["/hologram", "Something new"],
        ["/sessions", "Browse and resume previous sessions"],
        ["/quit", "Exit the CLI"],
        ["/widgets", "Reload TUI widgets"],
    ],
    "categories": [
        {
            "name": "Session",
            "pairs": [
                ["/status", "Show session status"],
                ["/compress", "Compress the history"],
                ["/long", "Print a great deal"],
                ["/git", "Show git status"],
                ["/plug", "Run a plugin command"],
                ["/goal", "Set or show the goal"],
                ["/hologram", "Something new"],
                # Filed under ``Session`` because the registry defines it and
                # the catalogue builder's dedup guard drops the ``TUI`` extra of
                # the same name. This is what a real gateway serves.
                ["/sessions", "Browse and resume previous sessions"],
            ],
        },
        {"name": "Exit", "pairs": [["/quit", "Exit the CLI"]]},
        {"name": "User commands", "pairs": [["/gs", "alias → git"], ["/loop", "alias → loop"]]},
        {
            "name": CLIENT_LOCAL_CATEGORY,
            "pairs": [
                ["/density", "Toggle compact display mode"],
                # A ``TUI``-categorised row that is *not* one of the four names.
                # Category alone would refuse it sight unseen.
                ["/widgets", "Reload TUI widgets"],
            ],
        },
    ],
    "canon": {
        "/g": "/gs",
        "/gs": "/gs",
        "/git": "/git",
        "/status": "/status",
        "/quit": "/quit",
        "/exit": "/quit",
    },
    "skills": {"/deploy": {"usage": 1, "origin": "local"}},
    "skill_count": 1,
    "warning": "",
}


class StubProvider:
    """The loopback token, minted per dial as KTD11 requires."""

    async def acquire(self) -> Credential:
        return Credential("token", STUB_TOKEN, "endpoint-url")


def slash_name(params: dict[str, Any]) -> str:
    """The command word out of a ``slash.exec`` request, as the handler reads it
    (``methods_tools.py:1087-1090``: split the line, lowercase the first word)."""
    parts = str(params.get("command", "")).split(maxsplit=1)
    return parts[0].lower() if parts else ""


def command_responder(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
    """Answer the five methods U9 speaks, split the way the gateway splits them.

    ``slash.exec`` serves the registry commands from its worker, forwards
    bundles and pending-input commands to ``command.dispatch`` and returns that
    payload, and refuses everything else — which is what makes the client's
    fallback a real path here rather than dead code. ``command.dispatch`` looks
    its reply up by the command name, so one responder serves every shape
    without the client getting a hint about which one is coming.
    """
    method = message.get("method")
    rid = message.get("id")
    params = message.get("params") or {}
    if method == CATALOG_METHOD:
        return ok(rid, CATALOG_BODY)
    if method == SLASH_EXEC_METHOD:
        name = slash_name(params)
        if name in WORKER_OUTPUT:
            return ok(rid, WORKER_OUTPUT[name])
        if name in FORWARDED:
            return ok(rid, SHAPES[name])
        # The shape of the handler's own refusal for a skill command
        # (``methods_tools.py:1146``). The client falls back from here.
        return err(rid, 4018, f"skill command: use command.dispatch for /{name}")
    if method == DISPATCH_METHOD:
        body = SHAPES.get(str(params.get("name", "")))
        if body is None:
            # The gateway's own refusal for a name it cannot route
            # (``methods_tools.py:1070``).
            return err(rid, 4018, f"not a quick/plugin/bundle/skill command: {params.get('name')}")
        return ok(rid, body)
    if method == SUBMIT_METHOD:
        return ok(rid, {"status": "ok"})
    if method == SUBAGENT_INTERRUPT_METHOD:
        return ok(rid, {"found": True, "subagent_id": params.get("subagent_id", "")})
    return None


@pytest_asyncio.fixture
async def command_gateway() -> Any:
    stub = StubGateway(responder=command_responder)
    await stub.start()
    try:
        yield stub
    finally:
        await stub.stop()


def live_app(gateway: StubGateway) -> tuple[TalariaApp, LiveSource]:
    source = LiveSource(
        AttachTarget.from_url(gateway.url),
        StubProvider(),
        reconnect_delays=FAST_RETRIES,
    )
    app = TalariaApp(
        source,
        mode="live",
        dispatcher=source,
        # The render tick is parked so every render in these tests is one a
        # test asked for, as in ``test_bridges.py``.
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
    before = app.frames_applied
    await gateway.send(frame)
    await until(lambda: app.frames_applied > before)


async def ready(gateway: StubGateway, app: TalariaApp, pilot: Any) -> None:
    """Attached, catalogue fetched, one session focused.

    Waits for an *available* catalogue, not merely a non-``None`` one. The
    mount-time attempt resolves ``not connected`` before the handshake
    completes and leaves an unavailable catalogue behind; waiting on
    ``is not None`` would race every alias resolution against the retry the
    connect callback fires.
    """
    await gateway.wait_for_attach()
    await until(lambda: app.catalog is not None and app.catalog.available)
    await push(gateway, app, event("session.info", {"session_id": "s1"}))
    await app.render_snapshot()
    await pilot.pause()


async def run_command(app: TalariaApp, pilot: Any, text: str) -> None:
    """Type a command and press Enter, the way an operator does."""
    app.composer.text = text
    app.composer.text_area.focus()
    await pilot.press("enter")
    await app.settle_live()
    await app.render_snapshot()
    await pilot.pause()


def transcript_text(app: TalariaApp) -> str:
    return "\n".join(transcript_view(app.state).lines)


def sent(gateway: StubGateway, method: str) -> list[dict[str, Any]]:
    return [
        message.get("params") or {}
        for message in gateway.current.received
        if message.get("method") == method
    ]


# ── scenario 1: the catalogue, all six shapes, and the local entry ───────


@pytest.mark.asyncio
async def test_the_catalogue_is_fetched_once_at_startup(
    command_gateway: StubGateway,
) -> None:
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)

        assert len(sent(command_gateway, CATALOG_METHOD)) == 1
        catalog = app.catalog
        assert catalog is not None and catalog.available
        assert {entry.name for entry in catalog.gateway_entries} == {
            "/status",
            "/compress",
            "/long",
            "/git",
            "/plug",
            "/gs",
            "/loop",
            "/deploy",
            "/release",
            "/goal",
            "/hologram",
            "/sessions",
            "/widgets",
        }
        await app.shutdown_sources()


@pytest.mark.parametrize(
    ("typed", "wire_name", "expected"),
    [
        ("/git", "git", "/git: on branch main"),
        ("/plug", "plug", "/plug: plugin says hello"),
        ("/deploy", "deploy", "/deploy: ⚡ /deploy"),
    ],
)
@pytest.mark.asyncio
async def test_a_shape_slash_exec_refuses_falls_back_to_command_dispatch(
    command_gateway: StubGateway, typed: str, wire_name: str, expected: str
) -> None:
    """R24, through the fallback leg. Three shapes, one renderer — and the
    transcript line each produces is asserted in full, because "something was
    written" is satisfied by writing the wrong thing.

    ``slash.exec`` is tried first and refused for each of these, exactly as it
    refuses a skill command at the pin, so both calls are asserted: a client
    that skipped straight to ``command.dispatch`` would produce the same
    transcript and be wrong about most of the catalogue.
    """
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, typed)

        assert sent(command_gateway, SLASH_EXEC_METHOD) == [
            {"command": wire_name, "session_id": "s1"}
        ]
        assert sent(command_gateway, DISPATCH_METHOD) == [
            {"name": wire_name, "arg": "", "session_id": "s1"}
        ]
        assert expected in transcript_text(app)
        assert expected.split(": ", 1)[1] in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.parametrize(
    ("typed", "wire_command", "expected"),
    [
        ("/release", "release", "/release: ⚡ Loading bundle: release (3 skills)"),
        ("/goal", "goal", "/goal: (prefilled into the composer)"),
    ],
)
@pytest.mark.asyncio
async def test_a_shape_slash_exec_forwards_arrives_without_a_second_call(
    command_gateway: StubGateway, typed: str, wire_command: str, expected: str
) -> None:
    """``slash.exec`` hands a bundle or a pending-input command to
    ``command.dispatch`` itself and returns that payload, so the client decodes
    a dispatch shape off a ``slash.exec`` reply and must not call again."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, typed)

        assert sent(command_gateway, SLASH_EXEC_METHOD) == [
            {"command": wire_command, "session_id": "s1"}
        ]
        assert sent(command_gateway, DISPATCH_METHOD) == []
        assert expected in transcript_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_ordinary_registry_command_runs_over_slash_exec(
    command_gateway: StubGateway,
) -> None:
    """The reason ``slash.exec`` is called at all.

    ``/status`` is an ordinary ``CommandDef``: not quick, not a plugin, not a
    bundle, not a skill, and not one of the twelve names ``command.dispatch``
    hardcodes. At ``7f4d15515`` that handler's last line refuses it — ``not a
    quick/plugin/bundle/skill command`` — and most of the 82 rows a real
    catalogue carries are in the same position. A client that only called
    ``command.dispatch`` would list every one of them as working and refuse
    every one on use.
    """
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, "/status")

        assert sent(command_gateway, SLASH_EXEC_METHOD) == [
            {"command": "status", "session_id": "s1"}
        ]
        assert sent(command_gateway, DISPATCH_METHOD) == []
        assert "/status: model: sonnet · context 12%" in transcript_text(app)
        assert "model: sonnet" in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_argument_travels_with_the_command_over_slash_exec(
    command_gateway: StubGateway,
) -> None:
    """``slash.exec`` takes one ``command`` string and splits the name off
    itself, so the argument has to be inside it rather than beside it."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, "/status --verbose")

        assert sent(command_gateway, SLASH_EXEC_METHOD) == [
            {"command": "status --verbose", "session_id": "s1"}
        ]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_slash_exec_warning_reaches_the_notice_and_not_the_line(
    command_gateway: StubGateway,
) -> None:
    """The gateway's ``warning`` is about the session it had to mirror a side
    effect into, not about what the command printed."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, "/compress")

        assert "/compress: history compressed" in transcript_text(app)
        assert "model switched to sonnet" not in transcript_text(app)
        assert "model switched to sonnet" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_command_output_is_kept_whole_up_to_the_bound(
    command_gateway: StubGateway,
) -> None:
    """``record_command_result`` exists because ``record_local_note`` clips at
    120 characters, which is right for a transport note and wrong for the output
    of ``/status``. Both halves are asserted: 4500 characters survive well past
    120, and the bound above them still holds.
    """
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, "/long")

        line = next(line for line in transcript_view(app.state).lines if "/long:" in line)
        assert len(line) > 4000, "the output was clipped like a transport note"
        # The bound plus the transcript's own furniture: "— /long: " and the
        # ellipsis that marks the cut.
        assert len(line) <= COMMAND_OUTPUT_CLIP + len("— /long: …")
        assert LONG_TAIL not in line, "the bound did not hold"
        assert line.endswith("…"), "the truncation was not marked"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_alias_runs_what_it_points_at(
    command_gateway: StubGateway,
) -> None:
    """An ``alias`` result names a target and runs nothing. Hermes's own client
    re-dispatches it (``createSlashHandler.ts:100-102``); a client that renders
    the target and stops has turned a working quick command into a dead end that
    looks like a result."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, "/g")

        assert [params.get("command") for params in sent(command_gateway, SLASH_EXEC_METHOD)] == [
            "gs",
            "git",
        ]
        transcript = transcript_text(app)
        assert "/gs: alias → git" in transcript
        assert "/git: on branch main" in transcript, "the alias did not run"
        assert "on branch main" in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_alias_that_points_at_itself_stops_and_says_so(
    command_gateway: StubGateway,
) -> None:
    """A quick command aliased to itself is a config typo, and the official
    handler recurses on it with no bound at all."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, "/loop")

        assert len(sent(command_gateway, DISPATCH_METHOD)) == 1
        assert ALIAS_CIRCULAR in transcript_text(app)
        assert ALIAS_CIRCULAR in screen_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_ring_of_aliases_terminates(command_gateway: StubGateway) -> None:
    """A three-step ring: no name repeats until the fourth hop, which is where
    "already in the chain" catches it."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, "/a1")

        assert [params.get("name") for params in sent(command_gateway, DISPATCH_METHOD)] == [
            "a1",
            "a2",
            "a3",
        ]
        assert ALIAS_CIRCULAR in transcript_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_chain_deeper_than_the_bound_stops_before_the_end_of_it(
    command_gateway: StubGateway,
) -> None:
    """The bound's own case, and the only one it has.

    ``/b1`` → ``/b2`` → ``/b3`` → ``/b4`` never repeats a name, so "already in
    the chain" never fires; without the bound Talaria would keep going. This is
    what a bound is for — capping the RPC work one line of an operator's
    quick-commands config can start — and without this test the constant was a
    guard nothing exercised: raising it to 100 left every other test green.
    """
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, "/b1")

        assert [params.get("name") for params in sent(command_gateway, DISPATCH_METHOD)] == [
            "b1",
            "b2",
            "b3",
        ]
        assert "the end of the chain" not in transcript_text(app)
        assert ALIAS_CIRCULAR in transcript_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_prefill_lands_in_the_composer_and_is_not_submitted(
    command_gateway: StubGateway,
) -> None:
    """The sixth shape. Its text goes where the operator can edit it, and
    ``prompt.submit`` is not called — a prefill is an offer, not an act."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, "/goal")

        assert app.composer.text == "rewrite the ingest pipeline"
        assert sent(command_gateway, SUBMIT_METHOD) == []
        await app.shutdown_sources()


@pytest.mark.parametrize("typed", ["/deploy", "/release"])
@pytest.mark.asyncio
async def test_a_skill_and_a_bundle_submit_their_scaffold_without_rendering_it(
    command_gateway: StubGateway, typed: str
) -> None:
    """R24's security-shaped clause. The expanded body is a system-prompt
    fragment: it must reach the model and must never reach the screen.

    Both halves are asserted, and they are asserted about different surfaces —
    a test that only checked the transcript would pass over a client that
    rendered nothing *and* sent nothing.
    """
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, typed)

        submitted = sent(command_gateway, SUBMIT_METHOD)
        assert [params.get("text") for params in submitted] == [SCAFFOLD]
        assert SCAFFOLD not in transcript_text(app)
        assert "SCAFFOLD-CANARY-4mQ2" not in screen_text(app)
        # The display projection *is* on screen, so the absence above is not
        # the absence of a rendered command.
        assert typed in transcript_text(app)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_client_local_entry_is_listed_unsupported_and_never_dispatched(
    command_gateway: StubGateway,
) -> None:
    """AE9. The operator sees ``/density`` on screen, sees that it is
    unsupported, and typing it sends nothing."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)

        await pilot.press("f3")
        await pilot.pause()
        screen = screen_text(app)
        assert "/density" in screen
        assert "unsupported" in screen
        # A dispatchable neighbour on the same screen, so "unsupported" is not
        # being read off a listing that failed to render its other rows.
        assert "/git" in screen

        await run_command(app, pilot, "/density on")
        assert sent(command_gateway, DISPATCH_METHOD) == []
        assert sent(command_gateway, SLASH_EXEC_METHOD) == []
        assert "unsupported" in app.composer.notice
        # And durably, not only in the notice row. A refused *gateway* command
        # gets a transcript line; the same honest refusal reached the other way
        # used to leave nothing behind, so the next notice erased it.
        assert "/density is unsupported" in transcript_text(app)
        # Nothing was lost: the text the operator typed is still there.
        assert app.composer.text == "/density on"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_sessions_is_dispatched_because_the_registry_owns_that_name(
    command_gateway: StubGateway,
) -> None:
    """The plan lists four client-local extras; at the pin only three arrive.

    The registry already defines ``CommandDef("sessions", …, "Session")``
    (``hermes_cli/commands.py:180``), so the catalogue builder's dedup guard
    drops the ``TUI`` extra of that name and a real gateway serves ``/sessions``
    as an ordinary command. Refusing it would be Talaria's copy of a stale list
    outranking the gateway's own registry.
    """
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        catalog = app.catalog
        assert catalog is not None
        entry = catalog.entry_for("/sessions")
        assert entry is not None
        assert entry.category == "Session"
        assert entry.availability == "dispatch"

        await run_command(app, pilot, "/sessions")
        assert sent(command_gateway, SLASH_EXEC_METHOD) == [
            {"command": "sessions", "session_id": "s1"}
        ]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_exit_leaves_talaria_instead_of_going_to_the_gateway(
    command_gateway: StubGateway,
) -> None:
    """``/exit`` is the registry's own alias for ``/quit``, so a real catalogue
    carries ``canon["/exit"] = "/quit"``.

    Resolving the alias and then dispatching the result sent ``quit`` over the
    socket, where the gateway does not implement it: the operator typed the exit
    command, got error 4018, and stayed exactly where they were.
    """
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        before = len(command_gateway.current.received)

        await run_command(app, pilot, "/exit")

        assert app.return_code == 0, "/exit did not exit"
        assert len(command_gateway.current.received) == before
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_talaria_local_four_are_marked_local_in_the_listing(
    command_gateway: StubGateway,
) -> None:
    """PC6's "marked local in any listing", on the rendered screen."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await pilot.press("f3")
        await pilot.pause()

        rows = app.palette.row_texts
        local_names = {"/quit", "/pause", "/resume", "/speed"}
        local_rows = [row for row in rows if row.split()[0] in local_names]
        assert len(local_rows) == 4
        assert all("local" in row for row in local_rows)
        screen = screen_text(app)
        assert "/quit" in screen and "local" in screen
        await app.shutdown_sources()


# ── scenario 2: an unknown result shape ──────────────────────────────────


@pytest.mark.asyncio
async def test_an_unknown_result_shape_surfaces_instead_of_crashing(
    command_gateway: StubGateway,
) -> None:
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, "/hologram")

        line = transcript_text(app)
        assert UNKNOWN_RESULT_NOTICE in line
        assert "hologram" in line
        assert UNKNOWN_RESULT_NOTICE in screen_text(app)
        # Nothing was guessed at and sent onward.
        assert sent(command_gateway, SUBMIT_METHOD) == []
        assert app.composer.text == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_command_the_catalogue_omitted_is_dispatched_and_said_to_be_unlisted(
    command_gateway: StubGateway,
) -> None:
    """The gateway's registry is the authority and its catalogue omits whole
    classes of command, so an unlisted name is still sent. The operator is told
    which of the two was confused, rather than left to guess."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        assert app.catalog is not None
        assert app.catalog.entry_for("/orphan") is None, "the fixture stopped omitting it"

        await run_command(app, pilot, "/orphan")

        assert sent(command_gateway, DISPATCH_METHOD) == [
            {"name": "orphan", "arg": "", "session_id": "s1"}
        ]
        assert "/orphan: it ran anyway" in transcript_text(app)
        assert "catalogue did not list" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_refused_command_is_named_and_the_text_is_kept(
    command_gateway: StubGateway,
) -> None:
    """A command the gateway rejects leaves the operator able to fix the typo."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await run_command(app, pilot, "/nosuch")

        # Both legs were tried and both refused, and it is the *fallback's*
        # refusal that is written down: that is the one that says why the
        # command did not run.
        assert sent(command_gateway, SLASH_EXEC_METHOD) == [
            {"command": "nosuch", "session_id": "s1"}
        ]
        assert sent(command_gateway, DISPATCH_METHOD) == [
            {"name": "nosuch", "arg": "", "session_id": "s1"}
        ]
        assert "not a quick/plugin/bundle/skill command" in transcript_text(app)
        assert app.composer.text == "/nosuch"
        await app.shutdown_sources()


# ── scenario 3: the local set never reaches the socket ───────────────────


@pytest.mark.asyncio
async def test_quit_acts_without_touching_the_socket(
    command_gateway: StubGateway,
) -> None:
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        before = len(command_gateway.current.received)

        await run_command(app, pilot, "/quit")

        assert app.return_code == 0, "/quit did not exit"
        assert len(command_gateway.current.received) == before
        await app.shutdown_sources()


@pytest.mark.parametrize("key", ["f8", "f9", "f10"])
@pytest.mark.asyncio
async def test_the_pacing_function_keys_refuse_a_live_session_too(
    command_gateway: StubGateway, key: str
) -> None:
    """The same refusal, reached by the other route.

    Before U9 these three flipped ``ReplayControls`` in a live session and
    reported ``paused · 1x``. Nothing reads those controls in live mode —
    ``ReplaySource`` is their only consumer — so the interface said a control
    had worked and nothing had happened, which is the inert-control failure
    AE11 names, arrived at from the direction replay mode never covers.
    """
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        speed_before = app.controls.speed
        paused_before = app.controls.paused

        await pilot.press(key)
        await pilot.pause()

        assert app.controls.speed == speed_before
        assert app.controls.paused is paused_before
        assert "live" in app.composer.notice
        assert "paused" not in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.parametrize("typed", ["/pause", "/resume", "/speed 4"])
@pytest.mark.asyncio
async def test_a_pacing_control_in_a_live_session_refuses_out_loud(
    command_gateway: StubGateway, typed: str
) -> None:
    """The three that scale a *recorded* clock. A live session has none, so
    they say so rather than looking like they worked — the inert-control
    failure AE11 names, reached from the other direction."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        before = len(command_gateway.current.received)
        speed_before = app.controls.speed
        paused_before = app.controls.paused

        await run_command(app, pilot, typed)

        assert len(command_gateway.current.received) == before
        assert "live" in app.composer.notice
        assert app.controls.speed == speed_before
        assert app.controls.paused is paused_before
        await app.shutdown_sources()


# ── scenario 6: one sub-agent's interrupt, from its own row (AE14) ───────


async def spawn_child(gateway: StubGateway, app: TalariaApp, pilot: Any) -> None:
    await push(
        gateway,
        app,
        event("subagent.start", {"subagent_id": "child-1", "goal": "reviewer"}),
    )
    await app.render_snapshot()
    await pilot.pause()


@pytest.mark.asyncio
async def test_a_row_action_interrupts_that_child_and_leaves_the_parent_in_place(
    command_gateway: StubGateway,
) -> None:
    """AE14's live half.

    Three separate claims, because collapsing them is how the wrong call gets
    made: the right *method* goes out with the right *id*; the parent's
    transcript is unchanged; and the parent's turn is not cancelled. That last
    one is the dangerous confusion — ``session.interrupt`` is one keystroke
    away and its cancelled state is sticky, so it would silently swallow the
    rest of the parent's reply.
    """
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await push(command_gateway, app, event("message.start", {}))
        await push(
            command_gateway, app, event("message.delta", {"text": "the parent is talking"})
        )
        await spawn_child(command_gateway, app, pilot)

        parent_lines = transcript_view(app.state).lines
        assert any("the parent is talking" in line for line in parent_lines)

        row = app.agents.row_for("child-1")
        assert row is not None, "the sub-agent row is not on screen"
        assert row.interruptible is True
        await pilot.click(row)
        await app.settle_live()
        await app.render_snapshot()
        await pilot.pause()

        assert sent(command_gateway, SUBAGENT_INTERRUPT_METHOD) == [
            {"subagent_id": "child-1"}
        ]
        assert sent(command_gateway, "session.interrupt") == []
        assert f"{SUBAGENT_INTERRUPTED} child-1" in transcript_text(app)
        # The parent is untouched: its in-flight text is still streaming, its
        # turn was never cancelled, and exactly one line was added — the note
        # about the child. ``session.interrupt`` would have flipped the turn to
        # ``cancelled``, which is sticky and suppresses every later delta.
        after = transcript_view(app.state).lines
        assert "the parent is talking" in after
        assert app.state.streaming_text == "the parent is talking"
        assert app.state.turn == "streaming"
        assert len(after) == len(parent_lines) + 1
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_row_action_is_reachable_from_the_keyboard(
    command_gateway: StubGateway,
) -> None:
    """A mouse is not in the supported matrix's guarantees and a click is not the
    only way an operator works. Enter on the focused row sends the same call."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await spawn_child(command_gateway, app, pilot)

        row = app.agents.row_for("child-1")
        assert row is not None
        assert row.can_focus is True
        row.focus()
        await pilot.pause()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert sent(command_gateway, SUBAGENT_INTERRUPT_METHOD) == [
            {"subagent_id": "child-1"}
        ]
        # Enter reached the row, not the composer: nothing was submitted.
        assert sent(command_gateway, SUBMIT_METHOD) == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_finished_child_carries_no_interrupt_affordance(
    command_gateway: StubGateway,
) -> None:
    """An affordance that is always refused teaches the operator to ignore it,
    and ``subagent.interrupt`` answers ``found: false`` for a child that has
    already completed."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        await spawn_child(command_gateway, app, pilot)
        await push(
            command_gateway,
            app,
            event("subagent.complete", {"subagent_id": "child-1", "status": "completed"}),
        )
        await app.render_snapshot()
        await pilot.pause()

        row = app.agents.row_for("child-1")
        assert row is not None
        assert row.interruptible is False
        await pilot.click(row)
        await app.settle_live()
        await pilot.pause()
        assert sent(command_gateway, SUBAGENT_INTERRUPT_METHOD) == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_child_the_gateway_does_not_have_is_not_reported_as_interrupted() -> None:
    """``found: false`` is a real answer, not a failure — the child had already
    finished (``methods_session.py:2806-2814``).

    Reporting it as an interrupt records an act that never happened, in the
    transcript ``terminal.read`` serves back to the agent. Nothing exercised
    this branch: flipping ``found`` to a constant ``True`` left the whole suite
    green.
    """

    def not_found(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        if message.get("method") == SUBAGENT_INTERRUPT_METHOD:
            return ok(message.get("id"), {"found": False})
        return command_responder(message, stub)

    gateway = StubGateway(responder=not_found)
    await gateway.start()
    try:
        app, _source = live_app(gateway)
        async with app.run_test(size=SCREEN) as pilot:
            await ready(gateway, app, pilot)
            await spawn_child(gateway, app, pilot)

            row = app.agents.row_for("child-1")
            assert row is not None
            await pilot.click(row)
            await app.settle_live()
            await app.render_snapshot()
            await pilot.pause()

            transcript = transcript_text(app)
            assert f"{SUBAGENT_NOT_FOUND} child-1" in transcript
            assert SUBAGENT_INTERRUPTED not in transcript
            assert SUBAGENT_NOT_FOUND in screen_text(app)
            await app.shutdown_sources()
    finally:
        await gateway.stop()


# ── an honestly degraded catalogue ───────────────────────────────────────


@pytest.mark.asyncio
async def test_a_catalogue_the_gateway_refuses_is_named_not_left_empty() -> None:
    """An empty listing says the gateway offers no commands, which is a claim.
    An unavailable one says Talaria could not read the listing, which is what
    happened — and the local four are still usable."""

    def refusing(message: dict[str, Any], stub: StubGateway) -> dict[str, Any] | None:
        if message.get("method") == CATALOG_METHOD:
            return err(message.get("id"), 5020, "registry import failed")
        return command_responder(message, stub)

    gateway = StubGateway(responder=refusing)
    await gateway.start()
    try:
        app, _source = live_app(gateway)
        async with app.run_test(size=SCREEN) as pilot:
            await gateway.wait_for_attach()
            await until(lambda: app.catalog is not None)
            await pilot.press("f3")
            await pilot.pause()

            catalog = app.catalog
            assert catalog is not None
            assert catalog.available is False
            assert "registry import failed" in catalog.failure
            screen = screen_text(app)
            assert "catalogue unavailable" in screen
            assert "/quit" in screen, "the local set went with the catalogue"
            await app.shutdown_sources()
    finally:
        await gateway.stop()


@pytest.mark.asyncio
async def test_a_multi_line_paste_beginning_with_a_slash_is_not_dispatched(
    command_gateway: StubGateway,
) -> None:
    """A pasted script whose first line starts with a slash is a message.
    Dispatching its first word and discarding the rest is the worst available
    reading of it."""
    app, _source = live_app(command_gateway)
    async with app.run_test(size=SCREEN) as pilot:
        await ready(command_gateway, app, pilot)
        app.composer.text_area.focus()
        app.post_message(Paste("/etc/hosts is wrong\nand so is /etc/resolv.conf"))
        await pilot.pause()
        await pilot.pause()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert sent(command_gateway, DISPATCH_METHOD) == []
        assert [params.get("text") for params in sent(command_gateway, SUBMIT_METHOD)] == [
            "/etc/hosts is wrong\nand so is /etc/resolv.conf"
        ]
        await app.shutdown_sources()
