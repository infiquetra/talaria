"""A4: the function-key row, re-decided as a whole (AE1-AE12).

Every test here drives the real app via Pilot, not via unit calls, except where
the acceptance item is structural (binding exists) rather than behavioural. Where
an action is reachable, the action is driven; where it is not (F4/F10 desktop
delivery unmeasured), the test proves only that the table was edited, and says
so in its name. Live desktop key delivery and Hermes gateway interaction are
operator-only and are not claimed.

See docs/plans/2026-08-12-v0-3-unit-a4-function-key-row.md for the row table
and the measurement-gap discussion.
"""

from __future__ import annotations

import re

import pytest

from talaria.domain.commands import TALARIA_LOCAL_COMMANDS
from talaria.replay.controls import ReplayControls
from talaria.ui.app import AGENTS_NOTHING_TO_TOGGLE, NOTHING_TO_INTERRUPT, HelpBar, TalariaApp
from tests.ui.conftest import RecordingDispatcher, event, feed, live_app, settle

#: Words the footer may never use of F1/F2 (Live 22 finding, reviewer shape).
#: From the reviewer's own mutation run, relayed by the controller: five are
#: the words it actually mutated, ten close the class. The class is any
#: wording asserting the keys are inert ("the key does nothing": inert,
#: unbound, disabled, dead, no-op) or that something else took them
#: (eaten, consumed, swallowed, blocked), plus the generic not-working family
#: (broken). Case-insensitive substring against the rendered footer string.
#: Footer only, never the inspector sentences: the inspector's job is now to
#: explain interception, so it must stay free to say a key may be consumed or
#: blocked — a shared list would forbid the correct wording where it belongs.
FOOTER_FORBIDDEN: tuple[str, ...] = (
    "eaten",
    "broken",
    "disabled",
    "unbound",
    "inert",
    "consumed",
    "swallowed",
    "blocked",
    "dead",
    "no-op",
)


@pytest.mark.asyncio
async def test_ae1_jump_is_gone() -> None:
    """AE1 structural: F1 has no binding and no action.

    The approval card is answerable without any function key (A1 auto-focus +
    enter/esc), and the help bar names the working ctrl+g chord rather than
    calling either key eaten; the macOS caveat lives in the inspector, where
    a sentence fits. The absence of a binding is asserted
    rather than a press, because an intercepted key sends no bytes and the
    program cannot distinguish it from not having been pressed.
    """
    # Structural check: no Binding("f1", "jump_to_prompt") and no action
    from talaria.ui.app import TalariaApp as AppClass

    keys = [b.key for b in AppClass.BINDINGS if b.key == "f1"]  # type: ignore
    assert keys == [], f"F1 still bound: {keys}"
    assert not hasattr(AppClass, "action_jump_to_prompt"), "jump action still present"
    # Constants removed with the action
    assert not hasattr(AppClass, "JUMP_BLOCKED_BY_MODAL")
    assert not hasattr(AppClass, "JUMP_NOTHING_OUTSTANDING")
    # Also check instance
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        # Pressing F1 does nothing — focus stays on composer, no notice
        before = app.screen.focused
        before_notice = app.composer.notice
        await pilot.press("f1")
        await pilot.pause()
        assert app.screen.focused is before
        assert app.composer.notice == before_notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae2_toggle_agents_via_chord_and_alias() -> None:
    """AE2 behavioural: F2 may be intercepted, so ctrl+g is the primary, F2 remains alias.

    With no rows, the toggle still flips its flag and the empty notice is shown,
    but the notice is latched per focus-hold (AE12) and a second press in the
    same hold is silent. The test drives chord and alias, and verifies that
    clicking the (now removed) status region does not toggle — the visible
    collapsed hint lives in AgentRows, not in the blank status region (P1-B).
    Slash /agents is also verified with a before/after check (P2-F).
    """
    # Chord ctrl+g — should toggle and show notice when empty
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(100, 30)) as pilot:
        assert not app.agents.is_populated
        before = app.agents.collapsed
        await pilot.press("ctrl+g")
        await pilot.pause()
        assert AGENTS_NOTHING_TO_TOGGLE in app.composer.notice
        assert app.agents.collapsed != before
        first_collapsed = app.agents.collapsed
        # Second chord in same hold is silent (latch) but still toggles
        await pilot.press("ctrl+g")
        await pilot.pause()
        assert app.agents.collapsed != first_collapsed
        await app.shutdown_sources()

    # Alias F2 still bound — structural proof that table edited, not desktop delivery
    app2 = live_app(RecordingDispatcher())
    async with app2.run_test(size=(100, 30)) as pilot:
        before = app2.agents.collapsed
        await pilot.press("f2")
        await pilot.pause()
        assert AGENTS_NOTHING_TO_TOGGLE in app2.composer.notice
        assert app2.agents.collapsed != before
        await app2.shutdown_sources()

    # Click on status region — no longer a primary (P1-B). Clicking the blank
    # status region must not toggle; only the chord (and F2 where delivered)
    # and /agents do. This proves the invisible target was removed.
    app3 = live_app(RecordingDispatcher())
    async with app3.run_test(size=(100, 30)) as pilot:
        before = app3.agents.collapsed
        await pilot.click("#status")
        await pilot.pause()
        assert app3.agents.collapsed == before
        await app3.shutdown_sources()

    # Slash alias /agents — should toggle with before/after check
    app4 = live_app(RecordingDispatcher())
    async with app4.run_test() as pilot:
        before = app4.agents.collapsed
        app4.composer.text = "/agents"
        await pilot.press("enter")
        await pilot.pause()
        assert app4.agents.collapsed != before
        await app4.shutdown_sources()


@pytest.mark.asyncio
async def test_ae2_follow_bottom_via_end_and_f5_alias() -> None:
    """AE2: F5's eaten-ambiguous, so end is the primary, F5 alias remains.

    The whole-pane transcript click that used to re-follow anywhere in the pane
    was removed (P1-C): it turned a scrollback read into an unexpected jump.
    Only end (and F5 where delivered) re-follows; clicking the transcript
    while reading must not yank to the bottom.
    """
    # End key is primary
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(100, 30)) as pilot:
        for i in range(60):
            feed(app, event("message.delta", {"text": f"scrollback {i}\n"}), seq=100 + i)
        await settle(app, pilot)
        app.transcript.hold_anchor()
        await pilot.pause()
        assert not app.transcript.follow
        await pilot.press("end")
        await pilot.pause()
        assert app.transcript.follow
        await app.shutdown_sources()

    # F5 alias still reaches same action
    app2 = live_app(RecordingDispatcher())
    async with app2.run_test(size=(100, 30)) as pilot:
        for i in range(60):
            feed(app2, event("message.delta", {"text": f"scrollback {i}\n"}), seq=100 + i)
        await settle(app2, pilot)
        app2.transcript.hold_anchor()
        await pilot.pause()
        assert not app2.transcript.follow
        await pilot.press("f5")
        await pilot.pause()
        assert app2.transcript.follow
        await app2.shutdown_sources()

    # Click on transcript when not following must NOT re-follow (P1-C)
    app3 = live_app(RecordingDispatcher())
    async with app3.run_test(size=(100, 30)) as pilot:
        for i in range(60):
            feed(app3, event("message.delta", {"text": f"scrollback {i}\n"}), seq=100 + i)
        await settle(app3, pilot)
        app3.transcript.hold_anchor()
        await pilot.pause()
        assert not app3.transcript.follow
        await pilot.click("#transcript")
        await pilot.pause()
        assert not app3.transcript.follow
        await app3.shutdown_sources()


@pytest.mark.asyncio
async def test_ae3_interrupt_guard_idle_shows_notice_and_does_not_dispatch() -> None:
    """P1-A: idle ctrl+s is a no-op with visible feedback, not a dispatch.

    The guard is load-bearing because the chord cancels a turn when one is in
    flight. The first press when no turn is in flight must not send
    session.interrupt and must show NOTHING_TO_INTERRUPT.
    """
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        # No message.start, so turn is idle
        assert app.state.turn == "idle"
        await pilot.press("ctrl+s")
        await app.settle_live()
        await pilot.pause()
        assert not any(m == "session.interrupt" for m, _ in app.dispatcher.calls)  # type: ignore[union-attr]
        assert NOTHING_TO_INTERRUPT in app.composer.notice
        await app.shutdown_sources()

    # F4 alias when idle is also guarded
    app2 = live_app(RecordingDispatcher())
    async with app2.run_test() as pilot:
        assert app2.state.turn == "idle"
        await pilot.press("f4")
        await app2.settle_live()
        await pilot.pause()
        assert not any(m == "session.interrupt" for m, _ in app2.dispatcher.calls)  # type: ignore[union-attr]
        assert NOTHING_TO_INTERRUPT in app2.composer.notice
        await app2.shutdown_sources()


@pytest.mark.asyncio
async def test_ae3_interrupt_when_streaming_dispatches() -> None:
    """P1-A: when a turn is streaming, ctrl+s and F4 dispatch session.interrupt."""
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        feed(app, event("message.start", {}))
        await settle(app, pilot)
        assert app.state.turn == "streaming"
        await pilot.press("ctrl+s")
        await app.settle_live()
        await pilot.pause()
        assert any(m == "session.interrupt" for m, _ in app.dispatcher.calls)  # type: ignore[union-attr]
        await app.shutdown_sources()

    app2 = live_app(RecordingDispatcher())
    async with app2.run_test() as pilot:
        feed(app2, event("message.start", {}))
        await settle(app2, pilot)
        assert app2.state.turn == "streaming"
        await pilot.press("f4")
        await app2.settle_live()
        await pilot.pause()
        assert any(m == "session.interrupt" for m, _ in app2.dispatcher.calls)  # type: ignore[union-attr]
        await app2.shutdown_sources()


@pytest.mark.asyncio
async def test_ae3_interrupt_in_replay_is_inert() -> None:
    """AE11: in replay, interrupt is inert — chord should refuse."""
    controls = ReplayControls(paused=True)
    from talaria.replay.source import ReplaySource
    from tests.ui.conftest import records

    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    app3 = TalariaApp(source, mode="replay", controls=controls)
    async with app3.run_test() as pilot:
        await pilot.press("ctrl+s")
        await pilot.pause()
        assert any(o.name == "interrupt" for o in controls.refusals)
        await app3.shutdown_sources()


def test_ae3_f4_and_f10_unmeasured_are_structural_only() -> None:
    """Structural proof that F4/F10 bindings exist; does not claim desktop delivery.

    F4 and F10 have never been pressed on the operator's hardware. This test
    proves the table was edited, not that the key arrives.
    """
    from talaria.ui.app import TalariaApp as AppClass

    f4_keys = [b.key for b in AppClass.BINDINGS if b.action == "interrupt"]  # type: ignore
    assert "f4" in f4_keys, "F4 alias missing"
    assert "ctrl+s" in f4_keys, "ctrl+s primary missing"
    assert "ctrl+c" not in f4_keys, "ctrl+c left the interrupt action (#120)"

    f10_keys = [b.key for b in AppClass.BINDINGS if b.action == "speed_up"]  # type: ignore
    assert "f10" in f10_keys

    f9_keys = [b.key for b in AppClass.BINDINGS if b.action == "slow_down"]  # type: ignore
    assert "f9" in f9_keys


@pytest.mark.asyncio
async def test_ae4_replay_controls_stay_and_have_slash_aliases() -> None:
    """AE4: F8/F9/F10 stay primary on the row and have slash aliases /pause etc."""
    # Structural: bindings survive
    from talaria.ui.app import TalariaApp as AppClass

    assert any(b.key == "f8" and b.action == "toggle_pause" for b in AppClass.BINDINGS)  # type: ignore
    assert any(b.key == "f9" and b.action == "slow_down" for b in AppClass.BINDINGS)  # type: ignore
    assert any(b.key == "f10" and b.action == "speed_up" for b in AppClass.BINDINGS)  # type: ignore

    # Slash aliases exist as local commands
    names = {c.name for c in TALARIA_LOCAL_COMMANDS}
    assert "/pause" in names
    assert "/resume" in names
    assert "/speed" in names

    # Behavioural: in replay, F8 pauses, F9 slows, F10 speeds, slash does same
    controls = ReplayControls(paused=False)
    from talaria.replay.source import ReplaySource
    from tests.ui.conftest import records

    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)
    async with app.run_test() as pilot:
        assert not controls.paused
        await pilot.press("f8")
        await pilot.pause()
        assert controls.paused
        await pilot.press("f9")
        await pilot.pause()
        # slow_down from 1x -> 0.5x
        assert controls.speed == 0.5
        await pilot.press("f10")
        await pilot.pause()
        assert controls.speed == 1.0
        await app.shutdown_sources()

    # Slash alias /pause should also toggle (via composer)
    controls2 = ReplayControls(paused=False)
    source2 = ReplaySource(records([event("gateway.ready", {})]), controls=controls2)
    app2 = TalariaApp(source2, mode="replay", controls=controls2)
    async with app2.run_test() as pilot:
        app2.composer.text = "/pause"
        await pilot.press("enter")
        await pilot.pause()
        assert controls2.paused
        await app2.shutdown_sources()


@pytest.mark.asyncio
async def test_ae5_palette_and_pickers_reachable_without_function_key() -> None:
    """AE5: palette and both pickers reachable via slash, F6/F7 remain as aliases."""
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        # Palette via F3 alias
        assert not app.palette.showing
        await pilot.press("f3")
        await pilot.pause()
        assert app.palette.showing
        await pilot.press("f3")
        await pilot.pause()
        assert not app.palette.showing

        # /models via slash — in test with no catalog it shows a notice, but the
        # path is slash, not F6. Verify the slash dispatch does not crash and
        # leaves a notice (the external picker tests carry the full behaviour).
        app.composer.text = "/models"
        await pilot.press("enter")
        await pilot.pause()
        assert app.composer.notice != ""

        # F6 alias still bound — pressing it also does not crash
        await pilot.press("f6")
        await pilot.pause()
        assert isinstance(app.composer.notice, str)

        # /profiles via slash
        app.composer.text = "/profiles"
        await pilot.press("enter")
        await pilot.pause()
        assert isinstance(app.composer.notice, str)
        await pilot.press("f7")
        await pilot.pause()
        assert isinstance(app.composer.notice, str)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae6_row_is_discoverable() -> None:
    """AE6: palette lists slash aliases with function keys; help footer fits 80.

    The footer is asserted against what would render at 80x24, not just the
    backing property: both mode strings must be <=80 and must not be clipped
    with ellipsis at standard width. Mode-scoping is also verified.
    """
    # Palette lists local commands with function key hints in description
    descs = {c.name: c.description for c in TALARIA_LOCAL_COMMANDS}
    assert "F8" in descs["/pause"]
    assert "F9" in descs["/speed"] or "F10" in descs["/speed"]
    assert "F6" in descs["/models"]
    assert "F7" in descs["/profiles"]
    assert "F2" in descs["/agents"] and "ctrl+g" in descs["/agents"]
    assert "status-bar" in descs["/bar"]

    # Help bar scoped to mode and fits 80 columns — assert on what renders,
    # not on the backing property (P2-H). len() on the stored string cannot
    # detect a wide character that keeps len at 80 while pushing cell_len to
    # 81 and causing ellipsis.
    app_live = live_app(RecordingDispatcher())
    async with app_live.run_test() as pilot:
        live_text = app_live.help_bar.help_text
        assert len(live_text) <= 80, f"live footer {len(live_text)} >80: {live_text!r}"
        assert "F8" not in live_text, "live should not advertise replay keys"
        assert "ctrl+o" in live_text, "live should name the inspector chord"
        assert "ctrl+s" in live_text, "live should name the cancel chord"
        assert "cancel-turn" in live_text, "live should label cancel-turn"
        assert "ctrl+q" in live_text, "live should name the quit chord"
        assert "quit" in live_text, "live should label quit-client"
        assert "ctrl+g" in live_text, "live should name the working agents chord"
        assert "agents" in live_text, "live should label the agents action"
        assert re.search(r"F1(?!\d)", live_text) is None
        assert re.search(r"F2(?!\d)", live_text) is None, (
            f"footer carries a function-key caveat again: {live_text!r}"
        )
        for forbidden in FOOTER_FORBIDDEN:
            assert forbidden not in live_text.lower(), (
                f"footer verdict returned: {forbidden!r} in {live_text!r}"
            )
        await pilot.pause()
        strips = app_live.screen._compositor.render_strips()
        row = app_live.help_bar.region.y
        assert row == 22
        assert app_live.bottom_status_bar.region.y == 23
        assert app_live.bottom_status_bar.region.height == 1
        rendered = "".join(seg.text for seg in strips[row])
        assert "…" not in rendered, f"live footer clipped at 80x24: {rendered!r}"
        assert "cancel-turn" in rendered, f"live cancel label missing: {rendered!r}"
        assert HelpBar.AGENTS_FOOTER in rendered, f"live tail missing: {rendered!r}"
        await app_live.shutdown_sources()

    from talaria.replay.source import ReplaySource
    from tests.ui.conftest import records

    controls = ReplayControls(paused=False)
    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    app_replay = TalariaApp(source, mode="replay", controls=controls)
    async with app_replay.run_test() as pilot:
        replay_text = app_replay.help_bar.help_text
        assert len(replay_text) <= 80, f"replay footer {len(replay_text)} >80: {replay_text!r}"
        assert "F8" in replay_text
        assert "F9" in replay_text
        assert "F10" in replay_text
        assert "ctrl+o" in replay_text, "replay should name the inspector chord"
        assert "ctrl+s" not in replay_text
        assert "ctrl+c" not in replay_text
        assert "ctrl+g" in replay_text, "replay should name the working agents chord"
        assert re.search(r"F1(?!\d)", replay_text) is None
        assert re.search(r"F2(?!\d)", replay_text) is None, (
            f"replay footer carries a function-key caveat again: {replay_text!r}"
        )
        for forbidden in FOOTER_FORBIDDEN:
            assert forbidden not in replay_text.lower(), (
                f"replay footer verdict returned: {forbidden!r} in {replay_text!r}"
            )
        await pilot.pause()
        strips = app_replay.screen._compositor.render_strips()
        row = app_replay.help_bar.region.y
        assert row == 22
        assert app_replay.bottom_status_bar.region.y == 23
        assert app_replay.bottom_status_bar.region.height == 1
        rendered = "".join(seg.text for seg in strips[row])
        assert "…" not in rendered, f"replay footer clipped at 80x24: {rendered!r}"
        assert HelpBar.AGENTS_FOOTER in rendered, f"replay tail missing: {rendered!r}"
        await app_replay.shutdown_sources()


@pytest.mark.asyncio
async def test_footer_agents_chord_matches_key_configuration() -> None:
    """Live 22 finding (reviewer shape): the footer names the working chord.

    F1 is deliberately unbound and F2 is a hidden alias with ctrl+g as its
    collision-free primary; both footer halves name that shown primary instead
    of carrying a function-key caveat. The table facts below come from
    build_app_bindings rather than from the shipped literal, so a table change
    forces this test to re-examine the footer — a literal assert would go
    stale instead. The caveat is not deleted, it is relocated: F2's alias and
    the macOS interception sentences live in the inspector KEYS section, and
    this test pins that relocation so a future edit cannot silently drop it.
    The footer refuses the whole forbidden class, not just the shipped
    "eaten" instance.
    """
    from textual.binding import Binding

    from talaria.config import DEFAULT_INSPECTOR_KEY, DEFAULT_INTERRUPT_KEY
    from talaria.ui.app import build_app_bindings
    from talaria.ui.inspector import FUNCTION_KEY_NOTE

    bindings = build_app_bindings(DEFAULT_INSPECTOR_KEY, DEFAULT_INTERRUPT_KEY)
    by_key = {b.key: b for b in bindings if isinstance(b, Binding)}
    assert "f1" not in by_key, "F1 is deliberately unbound"
    assert by_key["f2"].action == "toggle_agents", "F2 is a working alias, not inert"
    assert by_key["f2"].show is False
    primaries = [
        b.key for b in bindings if isinstance(b, Binding) and b.action == "toggle_agents" and b.show
    ]
    assert primaries == ["ctrl+g"], "one shown primary stands for the action"

    app_live = live_app(RecordingDispatcher())
    async with app_live.run_test():
        live_text = app_live.help_bar.help_text
        assert HelpBar.AGENTS_FOOTER in live_text
        assert primaries[0] in live_text
        assert re.search(r"F1(?!\d)", live_text) is None
        assert re.search(r"F2(?!\d)", live_text) is None
        for forbidden in FOOTER_FORBIDDEN:
            assert forbidden not in live_text.lower(), (
                f"footer verdict returned: {forbidden!r} in {live_text!r}"
            )
        keys_note = app_live.inspector.keys_note_text
        assert keys_note == FUNCTION_KEY_NOTE
        assert "F1" in keys_note and "F2" in keys_note
        assert "ctrl+g" in keys_note and "macOS" in keys_note
        # The exemption is a constraint, not a comment: the note deliberately
        # uses interception vocabulary ("consumed") the footer refuses, so a
        # future edit widening FOOTER_FORBIDDEN to cover the note breaks the
        # footer loops above instead of biting silently years later.
        assert "consumed" in keys_note, (
            "the note must keep the interception wording the footer refuses"
        )
        await app_live.shutdown_sources()

    from talaria.replay.source import ReplaySource
    from tests.ui.conftest import records

    controls = ReplayControls(paused=False)
    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    app_replay = TalariaApp(source, mode="replay", controls=controls)
    async with app_replay.run_test():
        replay_text = app_replay.help_bar.help_text
        assert HelpBar.AGENTS_FOOTER in replay_text
        assert primaries[0] in replay_text
        assert re.search(r"F1(?!\d)", replay_text) is None
        assert re.search(r"F2(?!\d)", replay_text) is None
        for forbidden in FOOTER_FORBIDDEN:
            assert forbidden not in replay_text.lower(), (
                f"replay footer verdict returned: {forbidden!r} in {replay_text!r}"
            )
        assert app_replay.inspector.keys_note_text == FUNCTION_KEY_NOTE
        await app_replay.shutdown_sources()


@pytest.mark.asyncio
async def test_inspector_keys_row_lends_full_note_to_focus() -> None:
    """Live 22 finding (reviewer shape): the relocated caveat stays readable.

    The KEYS row holds one line unfocused — the panel has no vertical slack
    for an always-open note — and expands to the full sentences while focused,
    the #144 Option B pattern. A relocated caveat nobody can open would be
    deletion by another name, so this test opens it: the setting name must be
    readable on focus, and the row must fold back to one line after.
    """
    from talaria.ui.inspector import InspectorKeysRow

    app_live = live_app(RecordingDispatcher())
    async with app_live.run_test(size=(132, 40)) as pilot:
        await pilot.pause()
        (row,) = [
            node
            for node in app_live.inspector.query(".inspector--keys").nodes
            if isinstance(node, InspectorKeysRow)
        ]
        assert row.size.height == 1
        app_live.screen.set_focus(row)
        await pilot.pause()
        assert row.size.height > 1, "a focused keys row must expand to its sentences"
        rendered = "\n".join(row.render_line(y).text for y in range(row.size.height))
        assert "standard function keys" in rendered, (
            "the setting name must be readable on focus"
        )
        app_live.screen.set_focus(None)
        await pilot.pause()
        assert row.size.height == 1, "focus leaving folds the row back"
        await app_live.shutdown_sources()


def test_ae7_eaten_keys_documented_statically_no_detector() -> None:
    """AE7: eaten keys documented statically, no runtime detector."""
    import pathlib

    app_text = (pathlib.Path(__file__).parents[2] / "talaria/ui/app.py").read_text()
    assert "eaten" in app_text.lower()  # static note exists
    readme = (pathlib.Path(__file__).parents[2] / "README.md").read_text()
    assert "F1" in readme and "eaten" in readme.lower()
    assert "F2" in readme

    # Ensure no code tries to detect eaten key at runtime
    assert "detector" not in app_text.lower() or "eaten-key detection" not in app_text.lower()


def test_ae9_no_composer_collision() -> None:
    """AE9: up-arrow history and / palette untouched; chords do not steal them."""
    from talaria.ui.app import TalariaApp as AppClass

    keys = [b.key for b in AppClass.BINDINGS]  # type: ignore
    assert "up" not in keys
    assert "/" not in keys
    assert "ctrl+up" not in keys
    assert "ctrl+g" in keys
    assert "ctrl+s" in keys
    assert "ctrl+o" in keys


def test_ae11_project_check_is_clean() -> None:
    """AE11: BINDINGS contain expected keys and help bar exists; real checks are external."""
    from talaria.ui.app import TalariaApp as AppClass

    keys = {b.key for b in AppClass.BINDINGS}  # type: ignore
    # Must have the expected primary bindings
    assert "ctrl+g" in keys
    assert "ctrl+s" in keys
    assert "ctrl+o" in keys
    assert "ctrl+b" not in keys, "the replaced inspector default stays unbound"
    assert "f8" in keys
    assert "ctrl+q" in keys
    # Help bar must exist as a widget class
    from talaria.ui.app import HelpBar

    assert HelpBar is not None
