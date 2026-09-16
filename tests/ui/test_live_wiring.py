"""R3 and R4 at the key-binding seam: what Enter and F4 actually reach.

``tests/transport/test_reconnect.py`` proves these paths end to end over a real
socket. This file proves the half a socket cannot: that the *keys* are bound to
them at all, that replay still refuses, and that each outcome shape produces the
transcript it is allowed to produce. The dispatcher here is a double, so the
outcome is chosen rather than provoked — which is the only way to exercise a
gateway error and a lost outcome deterministically from a keypress.

The frame source is a paused :class:`~talaria.replay.source.ReplaySource`. That
is not a contradiction: the seam is a frame source, and which one is feeding the
transcript has nothing to do with which dispatcher the composer submits through.
Pairing a paused source with a live dispatcher is what isolates the binding.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import replace
from typing import Any

import pytest
from textual.css.query import NoMatches
from textual.pilot import Pilot
from textual.widgets import Button, Input

from talaria.domain.commands import CATALOG_METHOD, LocalInvocation, resolve_command
from talaria.domain.compat import SeamObservation, apply_probe_round, empty_board
from talaria.domain.models import ConnectionStatus
from talaria.replay.controls import INERT_NOTICE, ReplayControls
from talaria.replay.source import ReplaySource
from talaria.status.runner import StatusTickResult
from talaria.transport.connection_set import EnsureReport
from talaria.transport.rpc import (
    LOST_WITH_TRANSPORT,
    NEVER_SENT,
    NO_REPLY_IN_TIME,
    NOT_CONNECTED,
    RpcOutcome,
    unknown_outcome,
)
from talaria.transport.settings import SettingsError
from talaria.ui.app import INTERRUPT_METHOD, SUBMIT_METHOD, LiveDispatcher, TalariaApp
from talaria.ui.settings_overlays import TargetSwitchOverlay
from talaria.ui.settings_widgets import row_widget_id
from talaria.ui.settings_workspace import SettingsWorkspaceScreen
from tests.ui.conftest import event, paused_app, records, screen_text


class RecordingDispatcher:
    """A dispatcher double that records calls and returns a chosen outcome."""

    def __init__(self, outcome: RpcOutcome | None = None) -> None:
        self.outcome = outcome
        self.calls: list[tuple[str, Mapping[str, Any]]] = []

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome:
        self.calls.append((method, dict(params or {})))
        if self.outcome is not None:
            return self.outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})

    @property
    def operator_calls(self) -> list[tuple[str, Mapping[str, Any]]]:
        """Every call except the startup catalogue fetch.

        ``TalariaApp`` reads ``commands.catalog`` once when it mounts in live
        mode (U9), so a raw ``calls`` list starts with a call no operator made.
        These tests are about what one operator action sent, so they read this;
        ``calls`` stays raw, and the fetch itself is asserted over a real socket
        in ``tests/transport/test_commands.py``.
        """
        return [call for call in self.calls if call[0] != CATALOG_METHOD]


def test_the_dispatcher_double_satisfies_the_protocol() -> None:
    """Guard the guard: a double outside the protocol proves nothing about it."""
    assert isinstance(RecordingDispatcher(), LiveDispatcher)


@pytest.mark.asyncio
async def test_theme_command_opens_locally_without_a_gateway_call() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.composer.text = "/theme"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert app.palette.is_theme_active
        assert dispatcher.operator_calls == []
        await app.shutdown_sources()


def live_app(dispatcher: RecordingDispatcher, frames: list[Any] | None = None) -> TalariaApp:
    controls = ReplayControls(paused=True)
    source = ReplaySource(records(frames or [event("gateway.ready", {})]), controls=controls)
    return TalariaApp(source, mode="live", controls=controls, dispatcher=dispatcher)


# ── R3: Enter submits, live ──────────────────────────────────────────────


@pytest.mark.asyncio
async def test_enter_in_live_mode_dispatches_prompt_submit() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.state = app.state.__class__(focused_session_id="s1")
        app.composer.text = "run the tests"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert dispatcher.operator_calls == [
            (SUBMIT_METHOD, {"session_id": "s1", "text": "run the tests"})
        ]
        assert app.composer.text == ""
        assert [e.text for e in app.state.transcript if e.kind == "user"] == ["run the tests"]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_enter_on_blank_text_sends_nothing() -> None:
    """An empty ``prompt.submit`` is noise the gateway would have to reject."""
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        app.composer.text = "   \n  "
        app.composer.text_area.focus()
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert dispatcher.operator_calls == []
        assert app.state.transcript == ()
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_surrounding_whitespace_is_trimmed_but_indentation_survives() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test():
        await app.submit_live("\n  def f():\n      return 1\n\n")
        assert dispatcher.operator_calls[0][1]["text"] == "def f():\n      return 1"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_lost_submit_is_written_once_and_marked() -> None:
    dispatcher = RecordingDispatcher(
        unknown_outcome(SUBMIT_METHOD, LOST_WITH_TRANSPORT, epoch=1)
    )
    app = live_app(dispatcher)

    async with app.run_test():
        outcome = await app.submit_live("did this arrive?")
        assert outcome is not None and outcome.status == "unknown"

        texts = [entry.text for entry in app.state.transcript]
        assert texts.count("did this arrive?") == 1
        marker = next(text for text in texts if text.startswith("delivery unconfirmed"))
        assert "the connection dropped" in marker
        # Cleared, not kept: the text is safe in the transcript, and leaving it
        # in the composer as well invites a resend of a message that may have
        # already been delivered.
        assert app.composer.text == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_slow_gateway_does_not_make_the_transcript_claim_a_dropped_connection() -> None:
    """The correlator knows which unknown this is, so the transcript must say so.

    Connected, gateway slow, no reply inside ``call_timeout``: the socket is
    still open and still streaming. A line reading "the connection dropped" is
    a statement about the transport that did not happen — the operator reads it,
    concludes the session is gone, and reattaches a connection that was fine.
    """
    dispatcher = RecordingDispatcher(
        unknown_outcome(SUBMIT_METHOD, NO_REPLY_IN_TIME, epoch=1)
    )
    app = live_app(dispatcher)

    async with app.run_test():
        outcome = await app.submit_live("this one is slow")
        assert outcome is not None and outcome.status == "unknown"

        texts = [entry.text for entry in app.state.transcript]
        assert texts.count("this one is slow") == 1
        marker = next(text for text in texts if text.startswith("delivery unconfirmed"))
        assert "no reply arrived before the deadline" in marker
        assert "connection dropped" not in marker, (
            "the transcript blamed a transport failure that did not occur"
        )
        # Still unconfirmed, so still no invitation to resend.
        assert app.composer.text == ""
        await app.shutdown_sources()


@pytest.mark.parametrize("reason", [NOT_CONNECTED])
@pytest.mark.asyncio
async def test_a_submit_that_never_reached_a_socket_says_it_was_not_sent(reason: str) -> None:
    """A message known *not* to have been sent must not read as possibly delivered.

    ``NOT_CONNECTED`` is the only reason that earns this line: ``begin()`` is
    never called, so nothing was written to any socket. Nothing received the
    message, so the transcript says exactly that — and what was typed stays in
    the composer, because this is the one unconfirmed case where a resend is
    right rather than a way to make the agent do the work twice.

    ``NEVER_SENT`` deliberately does *not* appear here; see the test below.
    """
    dispatcher = RecordingDispatcher(unknown_outcome(SUBMIT_METHOD, reason, epoch=1))
    app = live_app(dispatcher)

    async with app.run_test():
        app.composer.text = "nobody heard this"
        outcome = await app.submit_live(app.composer.text)
        assert outcome is not None and outcome.status == "unknown"

        texts = [entry.text for entry in app.state.transcript]
        assert texts.count("nobody heard this") == 1
        assert not any("delivery unconfirmed" in text for text in texts), (
            "a message that was never sent was written as possibly delivered"
        )
        marker = next(text for text in texts if text.startswith("not sent"))
        assert "never written to a gateway" in marker
        assert app.composer.text == "nobody heard this", (
            "the one case that invites a resend threw away what was typed"
        )
        assert "not sent" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_failed_write_does_not_claim_the_message_was_never_sent() -> None:
    """A send that raised is not a send that did not happen.

    ``NEVER_SENT`` is set when ``connection.send()`` raises, and a send can
    raise *after* a partial write — so what is known is that the write failed,
    not that nothing reached the gateway. Reporting it as "never written to a
    gateway, send it again" is the same overclaim as the hardcoded "the
    connection dropped" line this unit removed, pointing the other way: it
    invites a resend of a message that may have arrived, which makes the agent
    do the work twice.

    So the transcript must name no cause it cannot support, and the composer
    must not hold the text back for a resend.
    """
    dispatcher = RecordingDispatcher(unknown_outcome(SUBMIT_METHOD, NEVER_SENT, epoch=1))
    app = live_app(dispatcher)

    async with app.run_test():
        app.composer.text = "this may or may not have landed"
        outcome = await app.submit_live(app.composer.text)
        assert outcome is not None and outcome.status == "unknown"

        texts = [entry.text for entry in app.state.transcript]
        assert texts.count("this may or may not have landed") == 1
        assert not any("never written to a gateway" in text for text in texts), (
            "a write that raised was reported as a message that was never sent"
        )
        assert not any(text.startswith("not sent") for text in texts)
        assert any("delivery unconfirmed" in text for text in texts), (
            "an unknown delivery must still be marked unconfirmed"
        )
        assert app.composer.text == "", "an unconfirmed send must not stage a resend"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_unknown_with_no_recognized_reason_names_no_cause_at_all() -> None:
    """The fallback claims the least, in both directions.

    It must not invent a cause (there is none to report), and it must not read
    as never-sent either: a reason nobody recognizes is not evidence the message
    stayed home, so it earns no invitation to resend.
    """
    dispatcher = RecordingDispatcher(
        RpcOutcome(status="unknown", method=SUBMIT_METHOD, request_id="7", epoch=1)
    )
    app = live_app(dispatcher)

    async with app.run_test():
        app.composer.text = "into the void"
        outcome = await app.submit_live(app.composer.text)
        assert outcome is not None and outcome.status == "unknown"

        texts = [entry.text for entry in app.state.transcript]
        marker = next(text for text in texts if text.startswith("delivery unconfirmed"))
        assert marker == "delivery unconfirmed — the gateway never acknowledged this message"
        assert app.composer.text == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_refused_submit_writes_nothing_at_all() -> None:
    dispatcher = RecordingDispatcher(
        RpcOutcome(
            status="error",
            method=SUBMIT_METHOD,
            request_id="1",
            epoch=1,
            error_code=4090,
            error_message="too many active sessions",
        )
    )
    app = live_app(dispatcher)

    async with app.run_test():
        app.composer.text = "keep me"
        outcome = await app.submit_live(app.composer.text)

        assert outcome is not None and outcome.status == "error"
        assert app.state.transcript == ()
        assert app.composer.text == "keep me", "a refused send lost what was typed"
        assert "4090" in app.composer.notice
        await app.shutdown_sources()


# ── R4: interrupt, live ──────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_f4_in_live_mode_dispatches_session_interrupt() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test() as pilot:
        # Make the turn genuinely in flight (P1-A guard) — idle interrupt is a no-op
        from tests.ui.conftest import event, feed, settle

        feed(app, event("message.start", {}))
        await settle(app, pilot)
        await pilot.press("f4")
        await app.settle_live()
        await pilot.pause()

        assert [method for method, _ in dispatcher.operator_calls] == [INTERRUPT_METHOD]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_replay_refusal_is_unchanged_by_the_live_path() -> None:
    """AE11 must still hold: adding a live branch must not arm the replay one."""
    controls = ReplayControls(paused=True)
    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)

    async with app.run_test() as pilot:
        app.composer.text = "please send this"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.press("f4")
        await pilot.pause()

        assert [outcome.name for outcome in controls.refusals] == ["submit", "interrupt"]
        assert app.composer.text == "please send this"
        assert app.state.transcript == ()
        assert INERT_NOTICE in app.composer.notice
        await app.shutdown_sources()


# ── R35: the connection state reaches the screen ─────────────────────────


#: The four states R35 requires be distinguishable on screen. ``connected`` is
#: not among them: its line is empty by design (nothing to say), which is what
#: ``test_connecting_clears_when_the_connection_lands`` asserts.
_UNCONNECTED_STATES: tuple[ConnectionStatus, ...] = (
    "connecting",
    "reconnecting",
    "auth_failed",
    "disconnected",
)


@pytest.mark.asyncio
async def test_the_four_transport_states_render_four_different_lines() -> None:
    """R35 asks for four *distinguishable* states, so this compares them.

    The previous version of this test could not: each expected fragment was a
    substring of its own state's name, and "connecting" is a substring of
    "reconnecting", so setting the connecting and reconnecting notices to one
    identical string passed every case. Distinctness is a property of the four
    lines together and cannot be checked one line at a time — hence one test
    that collects all four.

    Containment is checked as well as equality because a line that is wholly
    inside another ("connecting" inside "reconnecting…") leaves an operator who
    reads one line unable to say which state produced it.
    """
    app = live_app(RecordingDispatcher())
    notices: dict[str, str] = {}

    async with app.run_test():
        for state in _UNCONNECTED_STATES:
            app.note_connection_state(state)
            assert app.state.connection == state
            notices[state] = app.composer.notice
        await app.shutdown_sources()

    assert all(notices.values()), f"a transport state rendered no line at all: {notices}"
    assert len(set(notices.values())) == len(_UNCONNECTED_STATES), (
        f"two transport states render the same line: {notices}"
    )
    for shown, line in notices.items():
        swallowed_by = [
            name for name, other in notices.items() if name != shown and line in other
        ]
        assert not swallowed_by, (
            f"the {shown} line is swallowed whole by {swallowed_by}: {notices}"
        )


@pytest.mark.parametrize(
    ("state", "fragment"),
    [
        ("connecting", "connecting to the gateway"),
        ("reconnecting", "connection lost"),
        ("auth_failed", "authentication failed"),
        ("disconnected", "disconnected from the gateway"),
    ],
)
@pytest.mark.asyncio
async def test_each_transport_state_names_its_own_condition(
    state: str, fragment: str
) -> None:
    """Distinct is necessary but not sufficient: each line must also be *about*
    its state. The fragments are chosen so that no two states could satisfy
    both of their assertions with one shared string."""
    app = live_app(RecordingDispatcher())

    async with app.run_test():
        app.note_connection_state(state)  # type: ignore[arg-type]
        assert app.state.connection == state
        assert fragment in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_connect_failure_shows_its_cause_beside_the_state() -> None:
    """The distinction KTD5's frozen enum cannot carry (R35)."""
    app = live_app(RecordingDispatcher())

    async with app.run_test():
        app.note_connection_state("disconnected", "ws://127.0.0.1:9119/api/ws: refused")
        assert "refused" in app.composer.notice
        assert app.state.connection == "disconnected"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_connecting_clears_when_the_connection_lands() -> None:
    app = live_app(RecordingDispatcher())

    async with app.run_test():
        app.note_connection_state("connecting")
        assert app.composer.notice != ""
        app.note_connection_state("connected")
        assert app.composer.notice == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_reconnect_leaves_exactly_one_marker() -> None:
    app = live_app(RecordingDispatcher())

    async with app.run_test():
        app.note_reconnect(2)
        assert [entry.text for entry in app.state.transcript] == [
            "reconnected to the gateway"
        ]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_reconnect_epoch_is_what_makes_the_marker_once_per_connection() -> None:
    """The argument is the connection generation, and it decides.

    Two callbacks for connection 2 are one reconnect and earn one line — the
    shape a re-armed reconnect loop or a second ``bind`` produces. A callback
    naming a connection already superseded earns none, because that reconnect is
    not news. A genuinely newer connection earns its own line.
    """
    app = live_app(RecordingDispatcher())

    async with app.run_test():
        app.note_reconnect(2)
        app.note_reconnect(2)
        assert len(app.state.transcript) == 1, "one connection announced itself twice"

        app.note_reconnect(1)
        assert len(app.state.transcript) == 1, "a stale reconnect callback wrote a marker"

        app.note_reconnect(3)
        assert len(app.state.transcript) == 2, "a genuine later reconnect went unannounced"
        await app.shutdown_sources()


# ── #122 (U3): failures stay visible in live mode while diagnostics move ──


@pytest.mark.asyncio
async def test_live_failure_paths_stay_visible_while_diagnostics_move() -> None:
    """A status failure and a transport notice land while the seams are moved."""
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)

    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.pause()
        clock = app.state.last_observed_at
        board = apply_probe_round(
            empty_board(app.fleet_profile),
            (
                SeamObservation(
                    seam="roster",
                    status="present",
                    source="probe roster",
                    trigger="attach",
                ),
                SeamObservation(
                    seam="approval-detail",
                    status="present",
                    source="probe approval-detail",
                    trigger="attach",
                ),
            ),
            at=clock,
        )
        app.fleet = replace(app.fleet, seam_boards={app.fleet_profile: board})
        await app._render_seams()
        await pilot.pause()
        assert len(app.inspector.diag_texts) == 4

        await app.status_region.apply(
            StatusTickResult(outcome="timeout", marker="status: slow command")
        )
        app.note_connection_state("reconnecting")
        await pilot.pause()

        assert app.status_region.marker_text == "[x] status: slow command"
        assert "connection lost" in app.composer.notice
        assert app.status_region.row_texts == ()
        assert len(app.inspector.diag_texts) == 4
        await app.shutdown_sources()


# ── P2-1/P2-2: settings command dispatch through the live app (dev-4) ─────
#
# The workspace emits typed commands; the app routes Save/Reveal through the
# injected settings client. The routing pins below already hold and must
# survive the P2 refactor; the post-switch save proves Save selection is
# connected to live target loading, which needs the P2-1 control.

_P2_TEST_A = "talaria-v062-cfg-p2-local-active-a"
_P2_TEST_E = "talaria-v062-cfg-p2-local-active-switch"
_P2_URL = "http://127.0.0.1:8765"
_P2_SIZE = (120, 36)


class _P2Connections:
    def __init__(self, home: str) -> None:
        self._home = home

    @property
    def home(self) -> str:
        return self._home

    async def ensure(self, profile: str) -> EnsureReport:
        return EnsureReport(profile, "already_up", "connected")


class _P2SettingsClient:
    def __init__(self) -> None:
        self.calls: list[tuple[str, str, str]] = []
        self.saved: dict[str, Any] = {"agent": {"max_turns": 25}}
        self.put_failure: SettingsError | None = None

    def factory(self, endpoint: str) -> _P2SettingsClient:
        del endpoint
        return self

    async def get_schema(self, target: Any) -> Any:
        self.calls.append(("get_schema", target.profile_name, ""))
        return {
            "fields": {
                "agent.max_turns": {
                    "type": "number",
                    "description": "Maximum agent turns.",
                    "category": "agent",
                }
            },
            "category_order": ["agent"],
        }

    async def get_config(self, target: Any) -> Any:
        self.calls.append(("get_config", target.profile_name, ""))
        saved = dict(self.saved)
        return {"saved": saved, "effective": dict(saved), "defaults": {}}

    async def put_config(self, target: Any, patch: Any) -> Any:
        self.calls.append(("put_config", target.profile_name, ""))
        if self.put_failure is not None:
            raise self.put_failure
        self.saved = {**self.saved, **{k: v for k, v in dict(patch).items()}}
        return {"ok": True}

    async def reveal_env(self, target: Any, key: str) -> Any:
        self.calls.append(("reveal_env", target.profile_name, key))
        return {"key": key, "value": "p2-wiring-value"}


def _p2_app(fake: _P2SettingsClient) -> tuple[TalariaApp, Any]:
    return paused_app(
        [event("gateway.ready", {})],
        profile_endpoints={_P2_TEST_A: _P2_URL, _P2_TEST_E: _P2_URL},
        current_profile=_P2_TEST_A,
        connections=_P2Connections(home="local"),
        settings_factory=fake.factory,
    )


async def _p2_open(pilot: Pilot[None], app: TalariaApp) -> None:
    invocation = resolve_command("/config", None)
    assert isinstance(invocation, LocalInvocation)
    assert app.perform_local_command(invocation) is True
    for _ in range(30):
        await pilot.pause()
        if isinstance(app.screen, SettingsWorkspaceScreen):
            return
    raise AssertionError("live /config never mounted the workspace")


def _p2_editor(app: TalariaApp) -> Input:
    row = app.screen.query_one(f"#{row_widget_id('agent.max_turns')}")
    editor: Input = row.query_one(Input)
    return editor


@pytest.mark.asyncio
async def test_settings_save_dispatches_put_then_reread() -> None:
    """The dispatch route P2 must preserve: footer Save sends one sparse
    PUT for the selected profile, then re-reads that same profile."""
    fake = _P2SettingsClient()
    app, _ = _p2_app(fake)
    async with app.run_test(size=_P2_SIZE) as pilot:
        await _p2_open(pilot, app)
        _p2_editor(app).value = "40"

        await pilot.click("#settings-save")
        for _ in range(30):
            await pilot.pause()
            puts = [c for c in fake.calls if c[0] == "put_config"]
            rereads = [
                c for c in fake.calls if c == ("get_config", _P2_TEST_A, "")
            ]
            if puts and len(rereads) >= 2:
                break

        puts = [c for c in fake.calls if c[0] == "put_config"]
        assert puts == [("put_config", _P2_TEST_A, "")]
        assert ("get_config", _P2_TEST_A, "") in fake.calls
        assert fake.saved == {"agent": {"max_turns": 40}}


@pytest.mark.asyncio
async def test_settings_reveal_dispatch_calls_reveal_env_once() -> None:
    """Reveal dispatch reaches the injected client exactly once with the
    selected target and key. (Value handoff to the overlay is P2-2 work,
    proven in the acceptance file; this pins the call itself.)"""
    fake = _P2SettingsClient()
    app, _ = _p2_app(fake)
    async with app.run_test(size=_P2_SIZE) as pilot:
        await _p2_open(pilot, app)
        screen = app.screen
        assert isinstance(screen, SettingsWorkspaceScreen)
        screen.open_reveal("EXAMPLE_P2_KEY")
        await pilot.pause()

        await pilot.click("#reveal-once")
        for _ in range(30):
            await pilot.pause()
            if [c for c in fake.calls if c[0] == "reveal_env"]:
                break

        assert [c for c in fake.calls if c[0] == "reveal_env"] == [
            ("reveal_env", _P2_TEST_A, "EXAMPLE_P2_KEY")
        ]


@pytest.mark.asyncio
async def test_failed_settings_save_renders_rejection_without_switching() -> None:
    """A refused PUT surfaces the server detail and keeps selection and
    edits — the failure branch every P2-1 save path must preserve."""
    fake = _P2SettingsClient()
    fake.put_failure = SettingsError("http_error", "PUT refused: locked")
    app, _ = _p2_app(fake)
    async with app.run_test(size=_P2_SIZE) as pilot:
        await _p2_open(pilot, app)
        _p2_editor(app).value = "40"

        await pilot.click("#settings-save")
        for _ in range(30):
            await pilot.pause()
            if "locked" in screen_text(app):
                break

        assert "locked" in screen_text(app)
        assert f"selected: {_P2_TEST_A}" in screen_text(app)
        assert _p2_editor(app).value == "40"
        assert [c for c in fake.calls if c[0] == "put_config"] == [
            ("put_config", _P2_TEST_A, "")
        ]


@pytest.mark.asyncio
async def test_post_switch_footer_save_targets_the_new_profile() -> None:
    """Save selection follows live target loading: after a completed
    switch to testE, a footer save PUTs testE — never the stale testA."""
    fake = _P2SettingsClient()
    app, _ = _p2_app(fake)
    async with app.run_test(size=_P2_SIZE) as pilot:
        await _p2_open(pilot, app)
        _p2_editor(app).value = "40"
        try:
            app.screen.query_one("#settings-target", Button)
        except NoMatches:
            pytest.fail(
                "live /config exposes no reachable target control (P2-1)"
            )
        await pilot.click("#settings-target")
        await pilot.pause()
        for button in app.screen.query(Button):
            if str(button.label).strip() == f"local / {_P2_TEST_E}":
                button.press()
                break
        for _ in range(30):
            await pilot.pause()
            if isinstance(app.screen, TargetSwitchOverlay):
                break
        assert isinstance(app.screen, TargetSwitchOverlay)

        await pilot.click("#switch-discard")
        for _ in range(60):
            await pilot.pause()
            if f"selected: {_P2_TEST_E}" in screen_text(app):
                break
        assert f"selected: {_P2_TEST_E}" in screen_text(app)

        _p2_editor(app).value = "41"
        await pilot.click("#settings-save")
        for _ in range(30):
            await pilot.pause()
            if [c for c in fake.calls if c[0] == "put_config"]:
                break

        puts = [c for c in fake.calls if c[0] == "put_config"]
        assert puts == [("put_config", _P2_TEST_E, "")]
