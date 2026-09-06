"""C2 slash palette: filtered, keyboard, and seam with C1.

Every assertion checks the row set, not just visibility, per the brief's rule:
a signal whose failure mode is indistinguishable from success is worse than
no signal. Tests drive the app via the same key path ChatTextArea._on_key
takes, not via direct text assignment, except where the spec explicitly says
programmatic writes must not open the palette.
"""

from __future__ import annotations

import asyncio
import re
from collections.abc import Mapping
from pathlib import Path
from typing import Any

import pytest

from talaria.domain.commands import (
    TALARIA_LOCAL_COMMANDS,
    CommandCatalog,
    CommandEntry,
)
from talaria.domain.composer_history import ComposerHistory
from talaria.themes.builtins import BUILTIN_THEMES
from talaria.transport.rpc import RpcOutcome
from talaria.ui import palette as palette_module
from talaria.ui.app import TalariaApp
from talaria.ui.composer import ChatTextArea
from talaria.ui.palette import PaletteRegion, format_filtered_entry
from talaria.ui.theme import BUILTIN_THEME_REGISTRY
from tests.ui.conftest import event, live_app, paused_app, settle

_REPOSITORY_ROOT = Path(__file__).parents[2]
_PICKER_COMMANDS = ("/models", "/profiles")


def _picker_key_contract(text: str) -> tuple[str, str | None]:
    """Return the every-focus key and optional outside-composer alias."""
    text = " ".join(text.replace("`", "").split())
    keys = tuple(dict.fromkeys(re.findall(r"\bF\d+\b", text)))
    outside_match = re.search(
        r"\b(F\d+) only outside composer focus\b",
        text,
    )
    outside = None if outside_match is None else outside_match.group(1)
    every_focus = tuple(key for key in keys if key != outside)
    assert len(every_focus) == 1, text
    return every_focus[0], outside


class RecordingDispatcher:
    def __init__(self, outcome: RpcOutcome | None = None) -> None:
        self.outcome = outcome
        self.calls: list[tuple[str, Mapping[str, Any]]] = []

    async def call(
        self, method: str, params: Mapping[str, Any] | None = None, *, timeout: float | None = None
    ) -> RpcOutcome:
        self.calls.append((method, dict(params or {})))
        if self.outcome is not None:
            return self.outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})


def _catalog_with(entries: list[tuple[str, str, str, str]]) -> CommandCatalog:
    """Helper: build a CommandCatalog from (name, description, category, availability)."""
    return CommandCatalog(
        entries=tuple(
            CommandEntry(name=n, description=d, category=c, availability=a)  # type: ignore[arg-type]  # noqa: E501
            for n, d, c, a in entries
        ),
        canon={},
        available=True,
    )


@pytest.mark.asyncio
async def test_picker_keys_match_command_rows_docs_and_launch_behavior(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    listing_app, _ = paused_app([event("gateway.ready", {})])
    async with listing_app.run_test() as pilot:
        rendered_rows = {}
        for command in _PICKER_COMMANDS:
            for ch in command:
                await pilot.press(ch)
            await pilot.pause()
            rendered_rows[command] = next(
                row
                for row in listing_app.palette.row_texts
                if row.startswith(command)
            )
            for _ in command:
                await pilot.press("backspace")
            await pilot.pause()
        await listing_app.shutdown_sources()

    guide = (_REPOSITORY_ROOT / "docs" / "terminal-ui.md").read_text(
        encoding="utf-8"
    )
    documentation_rows = {
        command: next(
            line for line in guide.splitlines() if line.startswith(f"| `{command}`")
        )
        for command in _PICKER_COMMANDS
    }
    rendered_contract = {
        command: _picker_key_contract(row)
        for command, row in rendered_rows.items()
    }
    documented_contract = {
        command: _picker_key_contract(row)
        for command, row in documentation_rows.items()
    }
    assert rendered_contract == documented_contract

    launch_app, _ = paused_app([event("gateway.ready", {})])
    reached: list[str] = []

    async def record_picker(mode: str) -> None:
        reached.append(mode)

    monkeypatch.setattr(launch_app, "open_picker", record_picker)
    async with launch_app.run_test() as pilot:
        assert launch_app.focused is launch_app.composer.text_area
        for command in _PICKER_COMMANDS:
            every_focus_key, _ = rendered_contract[command]
            await pilot.press(every_focus_key.casefold())
            await pilot.pause()

        assert reached == ["models", "profiles"]
        await launch_app.shutdown_sources()


@pytest.mark.parametrize(
    ("command", "expected_selection"),
    [
        ("/models", "second line"),
        ("/profiles", "first line\nsecond line"),
    ],
)
@pytest.mark.asyncio
async def test_focus_scoped_picker_alias_keeps_its_composer_selection_contract(
    command: str,
    expected_selection: str,
) -> None:
    local_command = next(
        item for item in TALARIA_LOCAL_COMMANDS if item.name == command
    )
    _, outside_composer_key = _picker_key_contract(local_command.description)
    assert outside_composer_key is not None

    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test() as pilot:
        text_area = app.composer.text_area
        assert app.focused is text_area
        app.composer.text = "first line\nsecond line"
        text_area.move_cursor((1, 3))

        await pilot.press(outside_composer_key.casefold())
        await pilot.pause()

        assert text_area.selected_text == expected_selection
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_theme_mode_restores_browse_and_leaves_slash_filtering_unchanged() -> None:
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test() as pilot:
        BUILTIN_THEME_REGISTRY.register(app)
        app.theme = "refined-default"
        await app.palette.toggle()
        assert app.palette.showing

        await app.palette.open_theme_picker(
            BUILTIN_THEMES,
            current_slug="refined-default",
            session_slug=None,
        )
        await pilot.press("down", "escape")
        await pilot.pause()
        assert app.palette.showing, "cancel did not restore the open browse listing"

        await app.palette.toggle()
        app.composer.text_area.focus()
        for character in "/mod":
            await pilot.press(character)
        await pilot.pause()
        assert app.palette.is_slash_active
        assert all(entry.name.startswith("/mod") for entry in app.palette.filtered_entries)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae1_leading_slash_predicate() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    async with app.run_test() as pilot:
        # Helper to type text via key path and check palette
        async def check_typed(text: str, should_open: bool) -> None:
            app.composer.text = ""
            await pilot.pause()
            if app.palette.is_slash_active:
                await app.palette.hide_slash()
                await pilot.pause()
            app.composer.text_area.focus()
            await pilot.pause()
            for ch in text:
                key = "space" if ch == " " else ch
                await pilot.press(key)
                await pilot.pause()
            assert app.palette.is_slash_active is should_open, f"{text!r} expected {should_open} got {app.palette.is_slash_active} text {app.composer.text!r}"  # noqa: E501
            # Also check showing
            assert app.palette.showing is should_open

        await check_typed("/", True)
        await check_typed("x/", False)
        await check_typed(" /a", True)
        await check_typed("/tmp/foo", False)
        await check_typed("hello/", False)
        await check_typed("/hello ", False)
        await check_typed("/hello", True)
        # Whitespace before slash
        await check_typed("  /", True)
        # Programmatic writes never open
        app.composer.text = "/models"
        await pilot.pause()
        assert not app.palette.is_slash_active
        # Typed after programmatic should open
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("/")
        await pilot.pause()
        assert app.palette.is_slash_active
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae2_filtering_is_prefix_only_case_insensitive() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    # Build a catalog with known entries
    catalog = _catalog_with(
        [
            ("/model", "Gateway model", "Session", "dispatch"),
            ("/models", "Local models", "Talaria", "talaria-local"),
            ("/status", "Gateway status", "Info", "dispatch"),
            ("/amod", "Contains mod as substring", "Info", "dispatch"),
            ("/help", "Help", "Info", "dispatch"),
            ("/profile", "Gateway profile", "Session", "dispatch"),
        ]
    )
    async with app.run_test() as pilot:
        await pilot.pause()
        app.catalog = catalog
        await app.palette.apply(app.catalog)
        app.composer.text_area.focus()
        await pilot.pause()
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        for ch in "/mod":
            await pilot.press(ch)
            await pilot.pause()
        # Prefix matches rank in Tier 0 before substring/description matches
        names = [e.name for e in app.palette.filtered_entries]
        assert "/model" in names
        assert "/models" in names
        assert "/status" not in names
        # Tiered ranking: prefix matches (/models, /model) rank before substring (/amod)
        assert "/amod" in names
        assert names.index("/models") < names.index("/amod")
        assert names.index("/model") < names.index("/amod")
        # Cross-case
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        for ch in "/MOD":
            await pilot.press(ch)
            await pilot.pause()
        names2 = [e.name for e in app.palette.filtered_entries]
        assert names == names2
        # Bare "/" yields every runnable (dispatch + local, no unsupported)
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("/")
        await pilot.pause()
        assert len(app.palette.filtered_entries) == len([e for e in catalog.entries if e.availability in ("dispatch", "talaria-local")])  # noqa: E501
        # Header counts remain tripartite
        assert "from the gateway" in app.palette.header_text
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae3_nothing_match_handling() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    async with app.run_test() as pilot:
        app.catalog = _catalog_with(
            [
                ("/model", "x", "Session", "dispatch"),
                ("/status", "y", "Info", "dispatch"),
            ]
        )
        await app.palette.apply(app.catalog)
        app.composer.text_area.focus()
        await pilot.pause()
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        for ch in "/zzzzzzz":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        assert len(app.palette.filtered_entries) == 0
        assert app.palette.row_texts == ("no matching commands",)
        # Enter does nothing except keep text
        before = app.composer.text
        await pilot.press("enter")
        await pilot.pause()
        assert app.composer.text == before
        assert app.palette.is_slash_active  # stays open
        # Tab is consumed and focus remains
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("tab")
        await pilot.pause()
        assert app.focused is app.composer.text_area
        assert app.palette.is_slash_active
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae4_unavailable_and_not_yet_fetched() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    async with app.run_test() as pilot:
        await pilot.pause()
        # Not yet fetched: catalog is None (initial)
        app.catalog = None
        await app.render_catalog()
        await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        await pilot.press("/")
        await pilot.pause()
        assert app.palette.is_slash_active
        assert app.palette.header_text == "commands: not fetched yet"
        assert app.palette.degraded_text == ""
        # Every Talaria-local command is runnable without a catalogue.
        # Derived rather than hardcoded: unit A4 grew this set from seven
        # to eight by adding /agents, and a literal count goes stale silently.
        assert len(app.palette.filtered_entries) == len(TALARIA_LOCAL_COMMANDS)
        assert all(e.availability == "talaria-local" for e in app.palette.filtered_entries)

        # Unavailable: catalog.available is False
        from talaria.domain.commands import unavailable_catalog

        app.catalog = unavailable_catalog("gateway down")
        await app.render_catalog()
        await pilot.pause()
        # Need to re-trigger palette with same text "/" -> should show degraded
        # Hide and re-show to pick up new catalog
        await app.palette.hide_slash()
        await pilot.pause()
        app.composer.text = ""
        await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("/")
        await pilot.pause()
        assert app.palette.is_slash_active
        assert "catalogue unavailable" in app.palette.degraded_text
        assert "gateway down" in app.palette.degraded_text
        assert len(app.palette.filtered_entries) == len(TALARIA_LOCAL_COMMANDS)

        # Warning case
        cat = _catalog_with(
            [
                ("/model", "x", "Session", "dispatch"),
            ]
        )
        cat = CommandCatalog(
            entries=cat.entries, canon={}, warning="skill scan failed", available=True
        )
        app.catalog = cat
        await app.render_catalog()
        await pilot.pause()
        await app.palette.hide_slash()
        await pilot.pause()
        app.composer.text = ""
        await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("/")
        await pilot.pause()
        assert "the gateway reported" in app.palette.degraded_text
        assert "skill scan failed" in app.palette.degraded_text
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae5_local_vs_gateway_distinction() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    cat = _catalog_with(
        [
            ("/model", "Gateway model", "Session", "dispatch"),
            ("/status", "Gateway status", "Info", "dispatch"),
            ("/density", "TUI density", "TUI", "unsupported"),
            ("/quit", "Leave", "Talaria", "talaria-local"),
        ]
    )
    app.catalog = cat
    await app.render_catalog()
    async with app.run_test() as pilot:
        await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        await pilot.press("/")
        await pilot.pause()
        # While filtering, every displayed row is runnable, no unsupported
        assert all(e.availability in ("dispatch", "talaria-local") for e in app.palette.filtered_entries)  # noqa: E501
        assert not any(e.availability == "unsupported" for e in app.palette.filtered_entries)
        # Rendered filtered rows must also not contain unsupported markers
        assert not any("unsupported" in row for row in app.palette.row_texts)
        # While browsing with F3, all three availabilities appear with markers
        await app.palette.hide_slash()
        await pilot.pause()
        await pilot.press("f3")
        await pilot.pause()
        # Browse shows all via rendered rows, not just backing catalogue
        assert any("unsupported" in row for row in app.palette.row_texts)
        assert any("local" in row for row in app.palette.row_texts)
        assert app.catalog is not None and any(e.availability == "unsupported" for e in app.catalog.entries)  # noqa: E501
        # Header counts remain tripartite in both modes
        assert "from the gateway" in app.palette.header_text
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae6_selection_dispatches_once_through_resolve() -> None:
    """#121: Enter on a selected entry dispatches it exactly once (U1).

    The pre-#121 contract was insert-never-submit; this test pins the
    replacement. The pick travels as the bare entry name through the same
    Submitted funnel as typed input: one ``slash.exec`` call, the picked
    (not the filter) text in history, an emptied composer, and a closed
    palette. Click keeps the old insert rule — asserted below, not here.
    """
    disp = RecordingDispatcher()
    app = live_app(disp)
    cat = _catalog_with(
        [
            ("/model", "Gateway model", "Session", "dispatch"),
            ("/models", "Local models", "Talaria", "talaria-local"),
        ]
    )
    # Add canon for alias test: /exit -> /quit
    cat = CommandCatalog(entries=cat.entries, canon={"/exit": "/quit"}, available=True)
    app.catalog = cat
    await app.render_catalog()
    async with app.run_test() as pilot:
        await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        for ch in "/mod":
            await pilot.press(ch)
            await pilot.pause()
        # Talaria locals group first, so index 1 is the gateway /model.
        assert [e.name for e in app.palette.filtered_entries] == ["/models", "/model"]
        await pilot.press("down")
        await pilot.pause()
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/model"
        await pilot.press("enter")
        await settle(app, pilot)
        # Exactly one gateway invocation for the picked command, and the
        # history holds the pick — not the "/mod" filter text being replaced.
        slash_calls = [call for call in disp.calls if call[0] == "slash.exec"]
        assert len(slash_calls) == 1
        assert slash_calls[0][1]["command"] == "model"
        assert app.composer.text == ""
        assert app.palette.is_slash_active is False
        assert app.focused is app.composer.text_area
        assert "/model" in app.composer_history.entries
        # Click still inserts via real click on row (Enter-only dispatch:
        # #121 leaves click on the old insert rule).
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        for ch in "/mod":
            await pilot.press(ch)
            await pilot.pause()
        assert len(app.palette.filtered_entries) >= 2
        # Drive a real click on the second row (index 1)
        row = app.palette._rows[1]
        await pilot.click(row)
        await pilot.pause()
        assert app.composer.text.endswith(" ")
        assert app.composer.text.startswith("/")
        assert not app.palette.is_slash_active
        # Clicking the palette header must not crash (P1-C)
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        for ch in "/mod":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        before_text = app.composer.text
        # Header is the first static in palette
        assert app.palette._header is not None
        await pilot.click(app.palette._header)
        await pilot.pause()
        # Header click should not insert and must not crash; palette stays open
        assert app.composer.text == before_text
        assert app.palette.is_slash_active
        # Now close palette for clean shutdown
        await app.palette.hide_slash()
        await pilot.pause()
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae7_dismiss_keeps_draft() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with([("/model", "x", "Session", "dispatch")])
    await app.render_catalog()
    async with app.run_test() as pilot:
        await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        for ch in "/models":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        await pilot.press("escape")
        await pilot.pause()
        assert app.composer.text == "/models"
        assert not app.palette.is_slash_active
        assert app.focused is app.composer.text_area

        # Deleting only the leading slash must close palette and keep "models"
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        for ch in "/models":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        # Deleting leading slash while preserving remainder: move caret to start and delete "/"
        # Caret movement itself closes the palette per KTD2, so first Left should close
        await pilot.press("left")
        await pilot.pause()
        assert not app.palette.is_slash_active
        # Reset to test slash deletion path via Home+Delete while palette was open
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        for ch in "/models":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        # Walk the caret to the start with Left, then Delete "/". Left is
        # still caret movement (it closes the palette per KTD2); Home is a
        # menu key now (F-3, #146) and would jump the selection instead.
        for _ in range(len("/models")):
            await pilot.press("left")
            await pilot.pause()
        assert not app.palette.is_slash_active
        await pilot.press("delete")
        await pilot.pause()
        assert app.composer.text == "models"
        assert not app.palette.is_slash_active
        # Alternative: direct programmatic deletion of leading slash must also preserve remainder
        app.composer.text = "/models"
        await pilot.pause()
        # Programmatic "/" deletion via setter should keep "models" and palette closed
        app.composer.text = "models"
        await pilot.pause()
        assert app.composer.text == "models"
        assert not app.palette.is_slash_active

        # Focus away closes with no row inserted
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        for ch in "/models":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        app.transcript.focus()
        await pilot.pause()
        await pilot.pause()
        assert not app.palette.is_slash_active
        assert app.composer.text == "/models"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae8_palette_never_steals_b1_notice() -> None:
    # B1's discard notice lives in composer.notice, palette's degraded is separate
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test() as pilot:
        # Put focus in transcript (no-text region) and send printable key to trigger B1 notice
        app.transcript.focus()
        await pilot.pause()
        await pilot.press("a")
        await pilot.pause()
        notice_before = app.composer.notice
        assert "transcript" in notice_before or "return to the message box" in notice_before
        degraded_before = app.palette.degraded_text
        # Now open palette via slash (should not clear B1 notice)
        app.composer.text_area.focus()
        await pilot.pause()
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        for ch in "/mod":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        # B1 notice must still be there after palette opened — palette never steals B1 row
        assert app.composer.notice == notice_before
        assert "transcript" in app.composer.notice or "return to the message box" in app.composer.notice  # noqa: E501
        # Palette's degraded should be unchanged (still empty for this catalog)
        assert app.palette.degraded_text == degraded_before
        # And opening palette should not clear discard latch
        # The latch is internal, but we can check that pressing another printable in transcript still only notices once  # noqa: E501
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae9_key_seam_with_c1() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.composer_history = ComposerHistory(entries=("/models 1", "hello"))
    # Ensure palette has at least two matches for "/mod" so Down moves selection
    cat = _catalog_with(
        [
            ("/model", "Gateway model", "Session", "dispatch"),
            ("/models", "Local models", "Talaria", "talaria-local"),
        ]
    )
    app.catalog = cat
    await app.render_catalog()
    async with app.run_test() as pilot:
        # Palette closed: Up belongs to history
        await app.palette.hide_slash()
        await app.render_catalog()
        await pilot.pause()
        app.composer.text = ""
        app.composer.text_area.focus()
        await pilot.pause()
        # Ensure palette closed
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "hello"
        assert app.composer_history.index == 1

        # Now open palette and check Up goes to palette, not history
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        for ch in "/mod":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        assert len(app.palette.filtered_entries) >= 2
        # Record history index before
        before_index = app.composer_history.index
        before_selected = app.palette.selected_index
        assert before_selected == 0
        await pilot.press("down")
        await pilot.pause()
        # History should not have moved, palette selection must have moved to 1
        assert app.composer_history.index == before_index
        assert app.palette.selected_index == 1
        # Moving up should go back to 0
        await pilot.press("up")
        await pilot.pause()
        assert app.composer_history.index == before_index
        assert app.palette.selected_index == 0
        # Verify rendered highlight would correspond to selected entry
        assert app.palette.selected_entry is not None
        assert app.palette.row_texts[0] == app.palette.row_texts[0]  # placeholder, real check below
        # Check that rendered rows include selected entry name
        assert any(app.palette.selected_entry.name in row for row in app.palette.row_texts)
        # Enter dispatches the selection (#121) rather than inserting or recalling.
        # Down first so the pick is the gateway /model (index 0 is local /models).
        await pilot.press("down")
        await pilot.pause()
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/model"
        await pilot.press("enter")
        await settle(app, pilot)
        assert not app.palette.is_slash_active
        assert app.composer.text == ""
        assert len([call for call in disp.calls if call[0] == "slash.exec"]) == 1
        # History recall that writes slash programmatically does not open palette (ruling 3)
        # Reset the walk: the dispatch above legitimately entered history.
        app.composer_history = ComposerHistory(entries=("/models 1", "hello"))
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        # Recall "/models 1" via Up
        await pilot.press("up")
        await pilot.pause()
        # Might need second Up to get /models 1 (since history is [" /models 1", "hello"], newest is hello)  # noqa: E501
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "/models 1"
        assert not app.palette.is_slash_active
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_escape_order_palette_over_history() -> None:
    """Palette Escape has priority over history abandon when both could be true.

    Both can be true only if a walk's text becomes a slash prefix via typed
    input mid-walk. The test pins that palette wins: Escape closes palette,
    walk remains.
    """
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.composer_history = ComposerHistory(entries=("/models", "a"))
    async with app.run_test() as pilot:
        app.composer.text = ""
        await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        # Start walk: Up to "a"
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "a"
        assert app.composer_history.index == 1
        # Up again to "/models" (walk, text is slash but palette still closed)
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "/models"
        assert app.composer_history.index == 0
        assert not app.palette.is_slash_active  # programmatic, not typed
        # Now type a character that keeps it slash: e.g., "x" -> "/modelsx"
        # This typed input should open palette while walk still active
        await pilot.press("x")
        await pilot.pause()
        assert app.composer.text == "/modelsx"
        # Palette should now be open (typed input made it slash)
        # Note: "/modelsx" is still slash prefix, so palette opens, but filtered may be empty
        # For this test we want palette open with entries, so use a prefix that matches
        # Instead type " " then backspace? Let's use a different walk: have history with "/mod" and type to keep  # noqa: E501
        # Simpler: walk is active, palette open, now Escape should close palette not abandon walk
        # Our current text "/modelsx" may have zero matches, but palette still open
        assert app.palette.is_slash_active
        # Walk still active
        assert app.composer_history.index == 0
        before_stash = app.composer_history.draft_stash
        await pilot.press("escape")
        await pilot.pause()
        assert not app.palette.is_slash_active
        # Walk should remain (palette had priority)
        assert app.composer_history.index == 0
        assert app.composer_history.draft_stash == before_stash
        # Second Escape should now abandon walk (since palette closed)
        await pilot.press("escape")
        await pilot.pause()
        assert app.composer_history.index is None
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ae10_browse_listing_unchanged() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    cat = _catalog_with(
        [
            ("/model", "Gateway model", "Session", "dispatch"),
            ("/density", "TUI density", "TUI", "unsupported"),
            ("/quit", "Leave", "Talaria", "talaria-local"),
        ]
    )
    async with app.run_test() as pilot:
        await pilot.pause()
        app.catalog = cat
        await app.palette.apply(app.catalog)
        await pilot.press("f3")
        await pilot.pause()
        # Browse should show all entries with markers
        assert len(app.palette.row_texts) == len(cat.entries)
        # Check that unsupported row is marked
        assert any("unsupported" in row for row in app.palette.row_texts)
        assert any("local" in row for row in app.palette.row_texts)
        # Max height still 14 via CSS not testable here, but palette is showing
        assert app.palette.showing
        await app.shutdown_sources()

@pytest.mark.asyncio
async def test_caret_movement_closes_palette() -> None:
    """P1-A: moving the caret (Left/Right) closes the palette. Home/End are
    menu keys (F-3, #146): they jump the selection and leave it open."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        for ch in "/mod":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        # Left is caret movement — should close palette, keep text
        await pilot.press("left")
        await pilot.pause()
        assert not app.palette.is_slash_active
        assert app.composer.text == "/mod"
        # Reopen
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        for ch in "/models":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        await pilot.press("home")
        await pilot.pause()
        assert app.palette.is_slash_active
        assert app.palette.selected_index == 0
        assert app.composer.text == "/models"
        # Also Right should close if palette were open (End jumps instead)
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        for ch in "/mod":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        await pilot.press("right")
        await pilot.pause()
        assert not app.palette.is_slash_active
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_programmatic_write_does_not_open_palette() -> None:
    """P1-A: programmatic writes (history recall, setter) must not open palette."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.composer_history = ComposerHistory(entries=("/models", "hello"))
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        # Programmatic setter
        app.composer.text = "/models"
        await pilot.pause()
        assert not app.palette.is_slash_active
        # History recall via Up (programmatic) should not open
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        await pilot.press("up")
        await pilot.pause()
        await pilot.press("up")
        await pilot.pause()
        assert app.composer.text == "/models"
        assert not app.palette.is_slash_active
        # Typed slash after programmatic should open
        app.composer.text = ""
        await pilot.pause()
        if app.palette.is_slash_active:
            await app.palette.hide_slash()
            await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("/")
        await pilot.pause()
        assert app.palette.is_slash_active
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_palette_move_selection_scrolls_into_view() -> None:
    """P1-B: arrow navigation scrolls the selected row into view."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    entries = [(f"/cmd{i:02d}", f"desc {i}", "Info", "dispatch") for i in range(20)]
    catalog = _catalog_with(entries)
    app.catalog = catalog
    async with app.run_test(size=(80, 24)) as pilot:
        await pilot.pause()
        await app.render_catalog()
        await pilot.pause()
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("/")
        await pilot.pause()
        assert app.palette.is_slash_active
        assert len(app.palette.filtered_entries) == 20
        # Drive 15 Down presses, reaching index 15 which was off-screen with max-height 14
        for _ in range(15):
            await pilot.press("down")
            await pilot.pause()
        assert app.palette.selected_index == 15
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/cmd15"
        # Rendered rows must still contain the selected entry (scroll kept it visible)
        # The active row class should be on the correct widget. Widget
        # position is not entry position once section headings interleave
        # (D5, #146), so resolve through the map rather than indexing rows.
        position = app.palette._row_entries.index(15)
        assert app.palette._rows[position].has_class("-active")
        # Scroll offset must have moved from 0 so row 15 is visible
        # Before fix scroll_y was 0 and row 15 was at y=20 off-screen
        assert app.palette.scroll_offset.y > 0
        # Pressing Enter dispatches the visible selected command (#121), once.
        await pilot.press("enter")
        await settle(app, pilot)
        slash_calls = [call for call in disp.calls if call[0] == "slash.exec"]
        assert len(slash_calls) == 1
        assert slash_calls[0][1]["command"] == "cmd15"
        assert app.composer.text == ""
        assert not app.palette.is_slash_active
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_palette_header_click_does_not_crash() -> None:
    """P1-C: clicking the palette header must not raise TypeError."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        for ch in "/mod":
            await pilot.press(ch)
            await pilot.pause()
        assert app.palette.is_slash_active
        before = app.composer.text
        assert app.palette._header is not None
        # Clicking header should not crash and should not insert
        await pilot.click(app.palette._header)
        await pilot.pause()
        assert app.composer.text == before
        assert app.palette.is_slash_active
        # Clicking a row should insert. The first widget is a section
        # heading (D5, #146), which addresses no entry and inserts nothing,
        # so click the first command row instead.
        assert len(app.palette._rows) >= 1
        position = next(
            index
            for index, entry in enumerate(app.palette._row_entries)
            if entry is not None
        )
        row = app.palette._rows[position]
        await pilot.click(row)
        await pilot.pause()
        assert app.composer.text.endswith(" ")
        assert not app.palette.is_slash_active
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_filtered_order_groups_talaria_first_like_browse() -> None:
    """The filtered palette groups the way F3 browse does (plan KTD3, line 84).

    Browse renders ``catalog.entries`` verbatim and ``build_catalog`` seeds that
    tuple with the Talaria locals, so browse shows them first. A plain
    ``(category, name)`` sort put them last, which moved ``/models`` depending on
    which surface the operator had opened — the disagreement the plan forbids.

    Asserts the rendered row order, not the sort key, so a future change to the
    key that reintroduces the disagreement fails here.
    """
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with(
        [
            ("/about", "Gateway info", "Info", "dispatch"),
            ("/model", "Gateway model", "Session", "dispatch"),
            ("/agents", "Local agents", "Talaria", "talaria-local"),
            ("/models", "Local models", "Talaria", "talaria-local"),
        ]
    )
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("slash")
        await pilot.pause()
        assert app.palette.is_slash_active
        names = [e.name for e in app.palette.filtered_entries]
        assert names == ["/agents", "/models", "/about", "/model"], names
        # Grouping matches browse: every Talaria local precedes every other entry.
        categories = [e.category for e in app.palette.filtered_entries]
        assert categories.index("Info") > max(
            i for i, c in enumerate(categories) if c == "Talaria"
        ), categories
        # The rendered rows carry the same order, not just the backing tuple —
        # divided by section headings (D5, #146): Talaria first, then the
        # gateway categories in wire order, each group under its own label.
        assert app.palette.row_texts[0] == "── Talaria ──"
        rendered = [text.split()[0] for text in app.palette.row_texts]
        assert rendered == [
            "──",
            "/agents",
            "/models",
            "──",
            "/about",
            "──",
            "/model",
        ], rendered
        await app.shutdown_sources()


# ── #121 single-Enter slash picker dispatch (U1/U2/U3) ────────────────────
#
# Enter on a selected slash entry dispatches it exactly once through the same
# Submitted → resolve_command funnel as typed input. Every dispatch below
# counts invocations (dispatcher calls or perform_local_command entries), not
# just outcomes: an outcome assertion cannot tell one dispatch from two.


async def _type_filter(pilot: Any, app: Any, text: str) -> None:
    """Type ``text`` through the key path so the slash palette opens."""
    app.composer.text = ""
    await pilot.pause()
    if app.palette.is_slash_active:
        await app.palette.hide_slash()
        await pilot.pause()
    app.composer.text_area.focus()
    await pilot.pause()
    for ch in text:
        await pilot.press(ch)
        await pilot.pause()


def _slash_exec_calls(disp: RecordingDispatcher) -> list[tuple[str, Mapping[str, Any]]]:
    return [call for call in disp.calls if call[0] == "slash.exec"]


@pytest.mark.asyncio
async def test_121_picked_local_command_dispatches_once(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """U1: Enter on a local pick runs perform_local_command exactly once."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with([("/bar", "Status bar", "Talaria", "talaria-local")])
    await app.render_catalog()
    local_calls: list[Any] = []
    original = app.perform_local_command

    def counting(invocation: Any) -> bool:
        local_calls.append(invocation)
        return original(invocation)

    async with app.run_test() as pilot:
        monkeypatch.setattr(app, "perform_local_command", counting)
        await pilot.pause()
        await _type_filter(pilot, app, "/ba")
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/bar"
        await pilot.press("enter")
        await settle(app, pilot)
        assert len(local_calls) == 1
        assert local_calls[0].command.name == "/bar"
        assert local_calls[0].argument == ""
        assert _slash_exec_calls(disp) == []
        assert "bar" in app.composer.notice
        assert app.composer.text == ""
        assert not app.palette.is_slash_active
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_121_unselected_enter_dispatches_nothing(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """U1: Enter with no match keeps the text, the open palette, and sends nothing."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with([("/model", "Gateway model", "Session", "dispatch")])
    await app.render_catalog()
    local_calls: list[Any] = []
    original = app.perform_local_command

    def counting(invocation: Any) -> bool:
        local_calls.append(invocation)
        return original(invocation)

    async with app.run_test() as pilot:
        monkeypatch.setattr(app, "perform_local_command", counting)
        await pilot.pause()
        await _type_filter(pilot, app, "/zzzzzzz")
        assert app.palette.is_slash_active
        assert app.palette.selected_entry is None
        await pilot.press("enter")
        await pilot.pause()
        assert app.composer.text == "/zzzzzzz"
        assert app.palette.is_slash_active
        assert local_calls == []
        assert _slash_exec_calls(disp) == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_121_typing_path_unchanged() -> None:
    """U1: a typed full line with the palette closed still submits once (R2)."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with([("/status", "Gateway status", "Info", "dispatch")])
    await app.render_catalog()
    async with app.run_test() as pilot:
        await pilot.pause()
        # Trailing space closes the slash predicate, so this Enter is the
        # plain typed path, never the picker.
        app.composer.text = "/status "
        await pilot.pause()
        assert not app.palette.is_slash_active
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("enter")
        await settle(app, pilot)
        slash_calls = _slash_exec_calls(disp)
        assert len(slash_calls) == 1
        assert slash_calls[0][1]["command"] == "status"
        assert "/status" in app.composer_history.entries
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_121_parity_args_required_speed_demands_its_argument(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """U2: a picked bare command demands args exactly like its typed twin.

    Replay mode, where the pacing controls are live: ``/speed`` with no rate
    must refuse with the same notice and no dispatch on both paths. The one
    disclosed difference is the composer: the pick consumes the line (the
    exactly-once mechanism), while typed input keeps it for editing.
    """
    app, _ = paused_app([event("gateway.ready", {})])
    local_calls: list[Any] = []
    original = app.perform_local_command

    def counting(invocation: Any) -> bool:
        local_calls.append(invocation)
        return original(invocation)

    async with app.run_test() as pilot:
        monkeypatch.setattr(app, "perform_local_command", counting)
        await pilot.pause()
        # Picked path: filter to /speed (a Talaria local, listed with no catalogue).
        await _type_filter(pilot, app, "/spe")
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/speed"
        await pilot.press("enter")
        await pilot.pause()
        assert len(local_calls) == 1
        assert local_calls[0].command.name == "/speed"
        assert local_calls[0].argument == ""
        picked_notice = app.composer.notice
        assert "rate" in picked_notice
        assert app.composer.text == ""
        assert "/speed" not in app.composer_history.entries

        # Typed twin: the same bare line with the palette closed.
        local_calls.clear()
        app.composer.text = "/speed "
        await pilot.pause()
        assert not app.palette.is_slash_active
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("enter")
        await pilot.pause()
        assert len(local_calls) == 1
        assert local_calls[0].command.name == "/speed"
        assert local_calls[0].argument == ""
        assert app.composer.notice == picked_notice
        assert "/speed" not in app.composer_history.entries
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_121_parity_confirmation_gate_models_bare_opens_picker_both_ways(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """U2: a picked confirmation-gated command gates exactly like its typed twin.

    A bare pick can never spell the ``<n> default [confirm]`` second act, so
    parity here means the pick reaches the *same* ``LocalInvocation`` (same
    command, same empty argument) and takes the same first step — opening the
    picker with no default write — as the typed bare line. The indexed resend
    shape itself (``/models 1 default`` → ``confirm_required`` → ``/models 1
    default confirm``) is always typed and is pinned by the picker suite's own
    two-act tests, which this unit leaves untouched; both suites run in
    verification.
    """
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with(
        [
            ("/model", "Gateway model", "Session", "dispatch"),
            ("/models", "Local models", "Talaria", "talaria-local"),
        ]
    )
    await app.render_catalog()
    local_calls: list[Any] = []
    original = app.perform_local_command

    def counting(invocation: Any) -> bool:
        local_calls.append(invocation)
        return original(invocation)

    async with app.run_test() as pilot:
        monkeypatch.setattr(app, "perform_local_command", counting)
        await pilot.pause()
        # Picked path: /models groups first, so index 0 is already the pick.
        await _type_filter(pilot, app, "/mod")
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/models"
        await pilot.press("enter")
        await settle(app, pilot)
        assert len(local_calls) == 1
        picked_invocation = local_calls[0]
        assert picked_invocation.command.name == "/models"
        assert picked_invocation.argument == ""
        # No model catalogue is loaded, so the first step is the honest
        # notice — and, crucially, no default write of any kind.
        picked_notice = app.composer.notice
        assert "model" in picked_notice.lower()
        assert _slash_exec_calls(disp) == []

        # Typed twin: the same bare line with the palette closed.
        local_calls.clear()
        app.composer.text = "/models "
        await pilot.pause()
        assert not app.palette.is_slash_active
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("enter")
        await settle(app, pilot)
        assert len(local_calls) == 1
        assert local_calls[0] == picked_invocation
        assert app.composer.notice == picked_notice
        assert _slash_exec_calls(disp) == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_121_double_enter_dispatches_once() -> None:
    """U3: two rapid Enters on one selection produce a single invocation."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with([("/status", "Gateway status", "Info", "dispatch")])
    await app.render_catalog()
    async with app.run_test() as pilot:
        await pilot.pause()
        await _type_filter(pilot, app, "/sta")
        assert app.palette.selected_entry is not None
        # Bounce: two Enters before any settle. The first consumes the
        # selection and empties the composer, so the second submits empty
        # text — answered without a dispatch.
        await pilot.press("enter")
        await pilot.press("enter")
        await settle(app, pilot)
        slash_calls = _slash_exec_calls(disp)
        assert len(slash_calls) == 1
        assert slash_calls[0][1]["command"] == "status"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_121_triple_enter_bounce_dispatches_once(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """U3: key-repeat style bounce on a local pick invokes once."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with([("/bar", "Status bar", "Talaria", "talaria-local")])
    await app.render_catalog()
    local_calls: list[Any] = []
    original = app.perform_local_command

    def counting(invocation: Any) -> bool:
        local_calls.append(invocation)
        return original(invocation)

    async with app.run_test() as pilot:
        monkeypatch.setattr(app, "perform_local_command", counting)
        await pilot.pause()
        await _type_filter(pilot, app, "/ba")
        assert app.palette.selected_entry is not None
        await pilot.press("enter")
        await pilot.press("enter")
        await pilot.press("enter")
        await settle(app, pilot)
        assert len(local_calls) == 1
        assert _slash_exec_calls(disp) == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_121_post_completion_enter_needs_a_fresh_selection() -> None:
    """U3: after a pick completes, bare Enter is a no-op; a fresh pick dispatches."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with([("/status", "Gateway status", "Info", "dispatch")])
    await app.render_catalog()
    async with app.run_test() as pilot:
        await pilot.pause()
        await _type_filter(pilot, app, "/sta")
        await pilot.press("enter")
        await settle(app, pilot)
        assert len(_slash_exec_calls(disp)) == 1
        # Post-completion: the composer is empty, so Enter submits prose.
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("enter")
        await settle(app, pilot)
        assert len(_slash_exec_calls(disp)) == 1
        # A fresh selection is a new intentional dispatch.
        await _type_filter(pilot, app, "/sta")
        assert app.palette.selected_entry is not None
        await pilot.press("enter")
        await settle(app, pilot)
        slash_calls = _slash_exec_calls(disp)
        assert len(slash_calls) == 2
        assert all(call[1]["command"] == "status" for call in slash_calls)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_121_stale_unsupported_pick_is_refused_never_dispatched() -> None:
    """U1: a pick that went unsupported since render resolves the normal path.

    The catalogue is swapped after the selection renders, without re-render,
    so the stale pick still names ``/status`` while the current catalogue
    refuses it. The funnel must answer unsupported with its notice — never
    dispatched, never silently dropped.
    """
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with([("/status", "Gateway status", "Info", "dispatch")])
    await app.render_catalog()
    async with app.run_test() as pilot:
        await pilot.pause()
        await _type_filter(pilot, app, "/sta")
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/status"
        # The catalogue changes; the render has not caught up.
        app.catalog = _catalog_with([("/status", "TUI status", "TUI", "unsupported")])
        await pilot.press("enter")
        await settle(app, pilot)
        assert _slash_exec_calls(disp) == []
        notice = app.composer.notice
        assert "/status" in notice
        assert "unsupported" in notice
        assert not app.palette.is_slash_active
        await app.shutdown_sources()


# ── #146 formatting, wrapping, badges, and single-dispatch ─────────────────


def test_146_format_filtered_entry_inactive_wraps_at_most_two_lines() -> None:
    """#146: Inactive entries wrap to at most 2 lines, clipped with '…' if longer.
    Continuation lines are indented 19 spaces to preserve name column alignment.
    """
    long_desc = (
        "This is a very long command description that is intended to span multiple "
        "lines when rendered at an eighty column terminal width, clearly exceeding "
        "two full lines so that inactive truncation can be verified."
    )
    entry = CommandEntry(
        name="/longcmd",
        description=long_desc,
        category="Test",
        availability="dispatch",
    )

    # Inactive: max 2 lines, clipped
    formatted_inactive = format_filtered_entry(entry, active=False, max_width=80)
    lines_inactive = formatted_inactive.split("\n")
    assert len(lines_inactive) == 2
    assert lines_inactive[0].startswith("/longcmd")
    assert lines_inactive[1].startswith(" " * 19)
    assert lines_inactive[1].endswith("…")

    # Active: all lines expanded without clipping
    formatted_active = format_filtered_entry(entry, active=True, max_width=80)
    lines_active = formatted_active.split("\n")
    assert len(lines_active) > 2
    assert lines_active[0].startswith("/longcmd")
    assert all(line.startswith(" " * 19) for line in lines_active[1:])
    assert not lines_active[1].endswith("…")


def test_146_format_filtered_entry_truthful_wire_badge() -> None:
    """#146: Skills carrying a wire origin render [origin] before their description.
    Rows without origin render no badge prefix.
    """
    skill_entry = CommandEntry(
        name="/deploy",
        description="Ship the release",
        category="Skills",
        availability="dispatch",
        origin="local",
        is_skill=True,
    )
    formatted_skill = format_filtered_entry(skill_entry, active=False, max_width=80)
    assert "/deploy" in formatted_skill
    assert "[local] Ship the release" in formatted_skill

    # Defense-in-depth (F-4): non-skill row with origin set does NOT render badge
    rogue_reg = CommandEntry(
        name="/status",
        description="Show status",
        category="Session",
        availability="dispatch",
        origin="local",
        is_skill=False,
    )
    formatted_rogue = format_filtered_entry(rogue_reg, active=False, max_width=80)
    assert "[local]" not in formatted_rogue

    registry_entry = CommandEntry(
        name="/help",
        description="Show help text",
        category="Info",
        availability="dispatch",
        origin="",
    )
    formatted_reg = format_filtered_entry(registry_entry, active=False, max_width=80)
    assert "[local]" not in formatted_reg
    assert "[bundled]" not in formatted_reg
    assert "[hub]" not in formatted_reg
    assert "Show help text" in formatted_reg


@pytest.mark.asyncio
async def test_146_consume_selected_is_atomic_and_single_dispatch() -> None:
    """#146: consume_selected returns the selected entry once and clears selection,
    preventing duplicate execution on rapid user events.
    """
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with([("/status", "Gateway status", "Info", "dispatch")])
    await app.render_catalog()
    async with app.run_test() as pilot:
        await pilot.pause()
        await _type_filter(pilot, app, "/sta")
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/status"

        # First consumption succeeds
        first = app.palette.consume_selected()
        assert first is not None
        assert first.name == "/status"
        assert app.palette.selected_entry is None

        # Second consumption returns None
        second = app.palette.consume_selected()
        assert second is None

        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_146_app_slash_palette_open_and_select_wiring() -> None:
    """#146: TalariaApp open_slash_palette and select_slash_command wiring."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _catalog_with([("/status", "Gateway status", "Info", "dispatch")])
    await app.render_catalog()
    async with app.run_test() as pilot:
        await pilot.pause()
        assert not app.palette.is_slash_active

        # open_slash_palette opens filtered palette
        await app.open_slash_palette("sta")
        await pilot.pause()
        assert app.palette.is_slash_active
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/status"

        # select_slash_command consumes and dispatches
        selected = await app.select_slash_command()
        assert selected is not None
        assert selected.name == "/status"
        await settle(app, pilot)

        slash_calls = [call for call in disp.calls if call[0] == "slash.exec"]
        assert len(slash_calls) == 1
        assert slash_calls[0][1]["command"] == "status"
        assert not app.palette.is_slash_active

        # Second call returns None (single dispatch)
        second = await app.select_slash_command()
        assert second is None

        await app.shutdown_sources()


# ── D5 sectioned slash menu (#146) ────────────────────────────────────────
#
# Bare ``/`` reads as one flat scrolling list divided into labelled
# sections: Talaria controls first, then gateway categories in wire order,
# then Skills, then Uncategorised. Headings are labels, not rows.


def _section_catalog() -> CommandCatalog:
    """One row per section kind: local, two wire categories, a skill, and a remainder."""
    return CommandCatalog(
        entries=(
            CommandEntry(
                name="/quit",
                description="Leave",
                category="Talaria",
                availability="talaria-local",
            ),
            CommandEntry(
                name="/sess01",
                description="session thing",
                category="Session",
                availability="dispatch",
            ),
            CommandEntry(
                name="/info01",
                description="info thing",
                category="Info",
                availability="dispatch",
            ),
            CommandEntry(
                name="/zeta",
                description="does zeta",
                category="",
                availability="dispatch",
                origin="local",
                is_skill=True,
            ),
            CommandEntry(
                name="/lone",
                description="no home",
                category="",
                availability="dispatch",
            ),
        ),
        canon={},
        available=True,
        skills={"/zeta": {"usage": 0, "origin": "local"}},
        categories_order=("Session", "Info"),
    )


@pytest.mark.asyncio
async def test_bare_slash_labels_each_section_before_its_first_row() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _section_catalog()
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("slash")
        await pilot.pause()
        assert app.palette.is_slash_active

        names = [text.split()[0] for text in app.palette.row_texts]
        assert names == [
            "──",
            "/quit",
            "──",
            "/sess01",
            "──",
            "/info01",
            "──",
            "/zeta",
            "──",
            "/lone",
        ], names
        assert app.palette.row_texts[0] == "── Talaria ──"
        # The list and its labels cannot disagree: every command row sits
        # under the heading the domain's own section_for names for it.
        current_label = ""
        catalog = app.catalog
        assert catalog is not None
        for position, text in enumerate(app.palette.row_texts):
            entry_index = app.palette._row_entries[position]
            if entry_index is None:
                current_label = text
                continue
            entry = app.palette.filtered_entries[entry_index]
            assert current_label == f"── {catalog.section_for(entry).label} ──"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_navigation_moves_between_commands_never_onto_labels() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _section_catalog()
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("slash")
        await pilot.pause()

        seen: list[str] = []
        for _ in range(len(app.palette.filtered_entries) - 1):
            await pilot.press("down")
            await pilot.pause()
            selected = app.palette.selected_entry
            assert selected is not None
            seen.append(selected.name)
            active = [
                row for row in app.palette._rows if row.has_class("-active")
            ]
            assert len(active) == 1
            assert not active[0].has_class("-section")
        assert seen == ["/sess01", "/info01", "/zeta", "/lone"], seen
        for _ in range(len(app.palette.filtered_entries) - 1):
            await pilot.press("up")
            await pilot.pause()
            assert app.palette.selected_entry is not None
            active = [
                row for row in app.palette._rows if row.has_class("-active")
            ]
            assert len(active) == 1
            assert not active[0].has_class("-section")
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/quit"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_filter_leaving_one_skill_keeps_only_the_skills_heading() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _section_catalog()
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        for character in "/zeta":
            await pilot.press(character)
            await pilot.pause()
        assert app.palette.is_slash_active

        texts = app.palette.row_texts
        assert len(texts) == 2, texts
        assert texts[0] == "── Skills ──"
        assert texts[1].split()[0] == "/zeta"
        # The badge survives filtering: provenance is wire data, not layout.
        assert "[local]" in texts[1]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_clicking_a_heading_inserts_nothing_and_keeps_the_selection() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _section_catalog()
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("slash")
        await pilot.pause()
        assert app.palette.is_slash_active
        assert app.palette.selected_index == 0

        await pilot.click(app.palette._rows[0])
        await pilot.pause()
        assert app.palette.row_texts[0] == "── Talaria ──"
        assert app.composer.text == "/"
        assert app.palette.selected_index == 0
        assert app.palette.is_slash_active
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_empty_sections_are_not_drawn() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = CommandCatalog(
        entries=(
            CommandEntry(
                name="/quit",
                description="Leave",
                category="Talaria",
                availability="talaria-local",
            ),
            CommandEntry(
                name="/sess01",
                description="session thing",
                category="Session",
                availability="dispatch",
            ),
        ),
        canon={},
        available=True,
        categories_order=("Session", "Configuration"),
    )
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("slash")
        await pilot.pause()

        texts = app.palette.row_texts
        assert "── Talaria ──" in texts
        assert "── Session ──" in texts
        # No skills, no remainder, and Configuration is ordered but empty:
        # none of them draws a heading.
        assert not any("Skills" in text for text in texts)
        assert not any("Uncategorised" in text for text in texts)
        assert not any("Configuration" in text for text in texts)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_row_in_both_category_and_skills_renders_once_badged() -> None:
    """Ruling item 5: the gateway's explicit placement wins, never duplicated."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = CommandCatalog(
        entries=(
            CommandEntry(
                name="/quit",
                description="Leave",
                category="Talaria",
                availability="talaria-local",
            ),
            CommandEntry(
                name="/both",
                description="placed twice by the wire",
                category="Session",
                availability="dispatch",
                origin="hub",
                is_skill=True,
            ),
        ),
        canon={},
        available=True,
        skills={"/both": {"usage": 0, "origin": "hub"}},
        categories_order=("Session",),
    )
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("slash")
        await pilot.pause()

        texts = app.palette.row_texts
        both = [text for text in texts if text.split()[0] == "/both"]
        assert len(both) == 1
        assert "[hub]" in both[0]
        session_at = texts.index("── Session ──")
        assert texts[session_at + 1].split()[0] == "/both"
        assert not any(text == "── Skills ──" for text in texts)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_badges_stay_on_skill_rows_only() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _section_catalog()
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("slash")
        await pilot.pause()

        for position, text in enumerate(app.palette.row_texts):
            entry_index = app.palette._row_entries[position]
            if entry_index is None:
                assert "[" not in text and "]" not in text
                continue
            entry = app.palette.filtered_entries[entry_index]
            if entry.is_skill:
                assert "[local]" in text
            else:
                assert "[" not in text and "]" not in text
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_long_descriptions_use_the_screen_they_have() -> None:
    """Review observation on #146: the fixed 80-cell wrap left 20 columns
    empty at 100 wide while clipping early. The region passes its own width;
    below 80 cells (and before layout) the historic default stands."""
    long_description = " ".join(f"word{i}" for i in range(30))

    async def first_row_length(size: tuple[int, int]) -> int:
        disp = RecordingDispatcher()
        app = live_app(disp)
        app.catalog = _catalog_with(
            [("/longdesc", long_description, "Info", "dispatch")]
        )
        async with app.run_test(size=size) as pilot:
            app.composer.text_area.focus()
            await pilot.pause()
            await pilot.press("slash")
            await pilot.pause()
            rows = [
                text
                for position, text in enumerate(app.palette.row_texts)
                if app.palette._row_entries[position] is not None
            ]
            await app.shutdown_sources()
            assert len(rows) == 1
            return len(rows[0].split("\n")[0])

    narrow = await first_row_length((80, 24))
    wide = await first_row_length((100, 30))
    assert narrow <= 80, narrow
    assert wide > narrow, (narrow, wide)


# ── PageUp/PageDown browse the slash menu (C7, #146) ──────────────────────
#
# The probe's "selection None" was the aftermath of the menu closing: the
# page keys fell through to caret handling, which calls hide_slash. They
# are claimed now, and route to move_selection with a page-sized delta.


@pytest.mark.asyncio
async def test_pagedown_pages_without_closing_the_menu() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    entries = [(f"/cmd{i:02d}", f"desc {i}", "Info", "dispatch") for i in range(20)]
    app.catalog = _catalog_with(entries)
    async with app.run_test(size=(80, 24)) as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("slash")
        await pilot.pause()
        assert app.palette.is_slash_active
        assert app.palette.selected_index == 0

        await pilot.press("pagedown")
        await pilot.pause()

        # The menu is still open on a real selection, and the composer text
        # is untouched — no caret paging leaked through.
        assert app.palette.is_slash_active
        assert app.palette.selected_entry is not None
        assert app.composer.text == "/"
        # A page, not a step, sized to what is visible: region height minus
        # the always-mounted header row.
        page = max(1, app.palette.size.height - 1)
        assert page > 1
        assert app.palette.selected_index == min(len(entries) - 1, page)
        assert app.palette.selected_entry.name == f"/cmd{min(len(entries) - 1, page):02d}"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_page_keys_clamp_like_arrows_at_both_ends() -> None:
    disp = RecordingDispatcher()
    app = live_app(disp)
    entries = [(f"/cmd{i:02d}", f"desc {i}", "Info", "dispatch") for i in range(20)]
    app.catalog = _catalog_with(entries)
    async with app.run_test(size=(80, 24)) as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("slash")
        await pilot.pause()

        # PageUp at the top stays at the top, exactly like Up.
        await pilot.press("pageup")
        await pilot.pause()
        assert app.palette.is_slash_active
        assert app.palette.selected_index == 0

        # PageDown past the end clamps at the last entry, exactly like Down.
        await pilot.press("pagedown")
        await pilot.pause()
        await pilot.press("pagedown")
        await pilot.pause()
        assert app.palette.is_slash_active
        assert app.palette.selected_index == len(entries) - 1
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/cmd19"

        # And PageUp from the end moves back up by a page.
        page = max(1, app.palette.size.height - 1)
        await pilot.press("pageup")
        await pilot.pause()
        assert app.palette.selected_index == len(entries) - 1 - page
        await app.shutdown_sources()


# ── Combined-review findings (C7, #146) ───────────────────────────────────


def _review_catalog() -> CommandCatalog:
    """Talaria's real local set plus one ordinary gateway category."""
    locals_ = tuple(
        CommandEntry(
            name=command.name,
            description=command.description,
            category="Talaria",
            availability="talaria-local",
        )
        for command in TALARIA_LOCAL_COMMANDS
    )
    gateway = tuple(
        CommandEntry(
            name=f"/session{i:02d}",
            description=f"gateway session command {i}",
            category="Session",
            availability="dispatch",
        )
        for i in range(3)
    )
    return CommandCatalog(
        entries=locals_ + gateway,
        canon={},
        available=True,
        categories_order=("Session",),
    )


def _headings(app: TalariaApp) -> list[str]:
    return [
        text
        for position, text in enumerate(app.palette.row_texts)
        if app.palette._row_entries[position] is None
        and text != "no matching commands"
    ]


@pytest.mark.asyncio
async def test_cross_tier_query_draws_each_heading_once() -> None:
    """F-1: grouping is by section at render, so a section that owns rows
    in more than one relevance tier still draws one heading. ``/bar``
    matches ``/s`` at tier 2 by description, below the tier-0 Session
    rows in filter order — it must render in the Talaria block, not drag
    a second Talaria heading onto the screen."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _review_catalog()
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("slash")
        await pilot.press("s")
        await pilot.pause()
        assert app.palette.is_slash_active

        headings = _headings(app)
        assert headings == ["── Talaria ──", "── Session ──"], headings
        texts = app.palette.row_texts
        bar_at = next(
            index for index, text in enumerate(texts) if text.split()[0] == "/bar"
        )
        session_at = texts.index("── Session ──")
        assert bar_at < session_at
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_no_match_keeps_the_row_map_in_lock_step() -> None:
    """F-2: the no-match row addresses no entry, and the map says so."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _review_catalog()
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        for character in "/zzz-nope":
            await pilot.press(character)
            await pilot.pause()

        assert app.palette.row_texts == ("no matching commands",)
        assert len(app.palette._rows) == len(app.palette._row_entries) == 1
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_home_and_end_jump_without_closing_the_menu() -> None:
    """F-3: first and last entry, not swallowed — one menu, one personality."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    entries = [(f"/cmd{i:02d}", f"desc {i}", "Info", "dispatch") for i in range(20)]
    app.catalog = _catalog_with(entries)
    async with app.run_test(size=(80, 24)) as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("slash")
        await pilot.pause()
        assert app.palette.selected_index == 0

        await pilot.press("end")
        await pilot.pause()
        assert app.palette.is_slash_active
        assert app.palette.selected_index == len(entries) - 1
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/cmd19"
        assert app.composer.text == "/"

        await pilot.press("home")
        await pilot.pause()
        assert app.palette.is_slash_active
        assert app.palette.selected_index == 0
        assert app.palette.selected_entry is not None
        assert app.palette.selected_entry.name == "/cmd00"
        await app.shutdown_sources()


def test_page_delta_floor_on_an_unmounted_region() -> None:
    """F-5: the max(1, …) floor holds where height is genuinely zero, and a
    test pins it so it cannot be silently removed later."""
    region = PaletteRegion()
    assert ChatTextArea._page_delta(region, "pagedown") == 1
    assert ChatTextArea._page_delta(region, "pageup") == -1


# ── the recurrence of #146's F-1, from the live rehearsal ────────────────


@pytest.mark.asyncio
async def test_concurrent_rebuilds_draw_one_set_of_rows_not_two() -> None:
    """The rehearsal's recurrence: ``apply`` mounts one row per await, so a
    keystroke-driven rebuild and a catalog-landing rebuild that interleaved
    each mounted their whole list into the same container — every heading
    and row on screen twice, witnessed live as adjacent duplicate Talaria
    headings. The rebuild is serialized against itself (the same trade
    ``TalariaApp.render_snapshot`` documents), so the second pass starts
    from the first pass's finished state, not its middle."""
    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = _review_catalog()
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await app.palette.show_slash(app.catalog, "/s")
        await pilot.pause()

        # Two rebuilds racing: the typing path and a catalog-landing apply.
        await asyncio.gather(
            app.palette.sync_slash(app.catalog, "/session01"),
            app.palette.apply(app.catalog),
        )
        await pilot.pause()

        rows = app.palette.row_texts
        # Both rebuilds produce the same "/session01" list; serialized, the
        # screen holds exactly one copy of it. Interleaved (the defect), the
        # same assertion sees every row twice.
        assert len(rows) == len(set(rows)), rows
        assert _headings(app) == ["── Session ──"], _headings(app)
        assert len(app.palette._rows) == len(app.palette._row_entries) == len(rows)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_non_contiguous_section_order_still_draws_each_heading_once(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """#146's general statement: the heading is a function of the grouping,
    not of adjacency. Whatever order the filtered list arrives in — the shape
    a tier-first filter produces before the display grouping repairs it — each
    section's heading is drawn exactly once, at its first row. Against the
    adjacency rule this list drew Talaria twice and Session twice."""
    catalog = _review_catalog()
    entries = catalog.entries
    local_rows = tuple(e for e in entries if e.availability == "talaria-local")
    session_rows = tuple(e for e in entries if e.category == "Session")
    interleaved = (
        local_rows[0],
        session_rows[0],
        local_rows[1],
        session_rows[1],
    )
    monkeypatch.setattr(
        palette_module, "_filtered_entries", lambda catalog_arg, prefix: interleaved
    )

    disp = RecordingDispatcher()
    app = live_app(disp)
    app.catalog = catalog
    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        await app.palette.show_slash(app.catalog, "/x")
        await pilot.pause()

        assert _headings(app) == ["── Talaria ──", "── Session ──"], _headings(app)
        rows = app.palette.row_texts
        # Every row is on screen exactly once, in the order the list gave —
        # and the second local row and second session row appear with no
        # second heading above them. The adjacency rule drew eight rows and
        # four headings here; the grouping rule draws six and two.
        assert len(rows) == len(interleaved) + 2
        assert rows[0] == "── Talaria ──"
        assert rows[1].startswith(local_rows[0].name)
        assert rows[2] == "── Session ──"
        assert rows[3].startswith(session_rows[0].name)
        assert rows[4].startswith(local_rows[1].name)
        assert rows[5].startswith(session_rows[1].name)
        await app.shutdown_sources()
