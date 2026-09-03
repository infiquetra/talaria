"""#120 Herdr-safe keyboard controls: configuration, defaults, and focus contexts.

U1 is the ``[keys]`` surface feeding BINDINGS construction: the inspector
toggle defaults to ``ctrl+o`` and the interrupt to ``ctrl+s``, an override
takes effect per instance, and every invalid value falls back with a notice.
U2 is the cancel-vs-quit contract: the footer names both behaviors, idle
still yields nothing-to-interrupt, replay still refuses, and ``ctrl+c`` —
which left the interrupt action — neither interrupts nor quits. U3 is the
focus proof: the chords fire from the composer, the transcript, the model
picker, and a confirm dialog without disturbing picker or dialog state.

U4 (XON/XOFF terminal capture) cannot be covered here: a eaten key sends no
bytes, so no Pilot press can prove the terminal delivered it. That leg is a
live operator protocol, not a test in this file.
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

import pytest

from talaria.config import (
    DEFAULT_INSPECTOR_KEY,
    DEFAULT_INTERRUPT_KEY,
    resolve_keybindings,
)
from talaria.domain.models_catalog import ModelProvider, ProviderCatalog
from talaria.ui.app import ModelAdmin, TalariaApp
from talaria.ui.dialog import ConfirmDialog, PickerDialog
from tests.ui.conftest import RecordingDispatcher, event, feed, live_app, settle


class FakeAdminClient:
    """The ``ModelAdmin`` double ``tests/ui/test_picker.py`` proves sufficient."""

    def __init__(self, catalog: ProviderCatalog) -> None:
        self._catalog = catalog

    async def model_options(self, *, profile: str | None = None) -> ProviderCatalog:
        return self._catalog


def _catalog() -> ProviderCatalog:
    return ProviderCatalog(
        providers=(
            ModelProvider(
                slug="anthropic",
                name="Anthropic",
                models=("sonnet", "opus"),
                authenticated=True,
            ),
        ),
        current_provider="anthropic",
        current_model="opus",
    )


assert isinstance(FakeAdminClient(_catalog()), ModelAdmin)


def _interrupt_calls(dispatcher: RecordingDispatcher) -> list[str]:
    return [method for method, _ in dispatcher.operator_calls]


# ── U1: the config surface ───────────────────────────────────────────────


def test_defaults_resolve_to_ctrl_o_and_ctrl_s() -> None:
    bindings, notices = resolve_keybindings(None)
    assert (bindings.toggle_inspector, bindings.interrupt) == ("ctrl+o", "ctrl+s")
    assert notices == ()

    assert DEFAULT_INSPECTOR_KEY == "ctrl+o"
    assert DEFAULT_INTERRUPT_KEY == "ctrl+s"


def test_empty_null_unknown_and_duplicate_values_fall_back_with_notices() -> None:
    cases: list[Mapping[str, Any] | None] = [
        {},
        {"toggle_inspector": "", "interrupt": ""},
        {"toggle_inspector": None, "interrupt": 7},
        {"toggle_inspector": "ctrl+banana", "interrupt": "ctrl+s"},
        {"toggle_inspector": "hyper+x", "interrupt": "ctrl+s"},
        {"toggle_inspector": "ctrl+x", "interrupt": "ctrl+x"},
        {"toggle_inspector": "ctrl+q", "interrupt": "ctrl+s"},
        {"toggle_inspector": "ctrl+o", "interrupt": "ctrl+q"},
    ]
    for raw in cases:
        bindings, notices = resolve_keybindings(raw)
        assert (bindings.toggle_inspector, bindings.interrupt) == (
            "ctrl+o",
            "ctrl+s",
        ), raw
        if raw:
            assert notices, f"silent fallback for {raw!r}"
            assert any("keys." in notice for notice in notices), notices


def test_a_valid_override_takes_effect_quietly() -> None:
    bindings, notices = resolve_keybindings(
        {"toggle_inspector": "Ctrl+X", "interrupt": "f4"}
    )
    assert (bindings.toggle_inspector, bindings.interrupt) == ("ctrl+x", "f4")
    assert notices == ()


def test_a_non_table_resolves_to_defaults() -> None:
    bindings, notices = resolve_keybindings("ctrl+x")  # type: ignore[arg-type]
    assert (bindings.toggle_inspector, bindings.interrupt) == ("ctrl+o", "ctrl+s")
    assert notices == ()


@pytest.mark.asyncio
async def test_default_chords_drive_the_real_app() -> None:
    app = live_app(RecordingDispatcher())
    assert app.inspector_key == "ctrl+o"
    assert app.interrupt_key == "ctrl+s"
    async with app.run_test(size=(132, 30)) as pilot:
        assert app.inspector.is_docked
        await pilot.press("ctrl+o")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_configured_override_moves_both_chords() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(
        dispatcher,
        keybindings={"toggle_inspector": "ctrl+x", "interrupt": "ctrl+y"},
    )
    assert (app.inspector_key, app.interrupt_key) == ("ctrl+x", "ctrl+y")
    async with app.run_test(size=(132, 30)) as pilot:
        assert "ctrl+x inspector" in app.help_bar.help_text
        assert "ctrl+y cancel-turn" in app.help_bar.help_text

        await pilot.press("ctrl+o")
        await pilot.pause()
        assert app.inspector.is_docked, "the default chord must not fire under override"

        await pilot.press("ctrl+x")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed

        feed(app, event("message.start", {}))
        await settle(app, pilot)
        await pilot.press("ctrl+s")
        await app.settle_live()
        await pilot.pause()
        assert "session.interrupt" not in _interrupt_calls(dispatcher)

        await pilot.press("ctrl+y")
        await app.settle_live()
        await pilot.pause()
        assert "session.interrupt" in _interrupt_calls(dispatcher)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_invalid_override_falls_back_and_says_so() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(
        dispatcher,
        keybindings={"toggle_inspector": "ctrl+banana", "interrupt": "ctrl+q"},
    )
    assert (app.inspector_key, app.interrupt_key) == ("ctrl+o", "ctrl+s")
    assert app._keybinding_notices, "a fallback with no notice is a silent remap"
    async with app.run_test(size=(132, 30)) as pilot:
        await pilot.press("ctrl+o")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_override_never_leaks_into_another_app() -> None:
    first = live_app(
        RecordingDispatcher(),
        keybindings={"toggle_inspector": "ctrl+x", "interrupt": "ctrl+y"},
    )
    second = live_app(RecordingDispatcher())
    assert (second.inspector_key, second.interrupt_key) == ("ctrl+o", "ctrl+s")
    interrupt_keys = [
        key
        for key, bindings in second._bindings.key_to_bindings.items()
        for binding in bindings
        if binding.action == "interrupt"
    ]
    assert "ctrl+s" in interrupt_keys
    assert "ctrl+y" not in interrupt_keys
    await first.shutdown_sources()
    await second.shutdown_sources()


# ── U2: cancel-turn vs quit-client ───────────────────────────────────────


@pytest.mark.asyncio
async def test_footer_names_cancel_turn_and_quit_client() -> None:
    app = live_app(RecordingDispatcher())
    async with app.run_test() as pilot:
        text = app.help_bar.help_text
        assert "ctrl+s" in text and "cancel-turn" in text
        assert "ctrl+q" in text and "quit" in text
        assert "ctrl+c" not in text, "the released chord must not be advertised"
        assert "cancel-turn" in text and text.index("cancel-turn") != text.index("quit")
        await pilot.pause()
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_replaced_ctrl_b_toggles_nothing() -> None:
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(132, 30)) as pilot:
        assert app.inspector.is_docked
        await pilot.press("ctrl+b")
        await pilot.pause()
        assert app.inspector.is_docked, "the replaced default stays unbound"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_ctrl_c_while_streaming_neither_interrupts_nor_quits() -> None:
    """``ctrl+c`` left the interrupt action: a habitual press must be safe.

    From the composer it reaches the text area's copy binding; from anywhere
    else it reaches Textual's system quit hint, which names ``ctrl+q``
    instead of acting. Both paths must dispatch nothing and leave the client
    running — proved by driving the real chords afterwards.
    """
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)
    async with app.run_test(size=(132, 30)) as pilot:
        feed(app, event("message.start", {}))
        await settle(app, pilot)
        assert app.state.turn == "streaming"

        app.composer.text_area.focus()
        await pilot.pause()
        await pilot.press("ctrl+c")
        await app.settle_live()
        await pilot.pause()
        assert "session.interrupt" not in _interrupt_calls(dispatcher)

        app.transcript.focus()
        await pilot.pause()
        await pilot.press("ctrl+c")
        await app.settle_live()
        await pilot.pause()
        assert "session.interrupt" not in _interrupt_calls(dispatcher)

        await pilot.press("ctrl+o")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed, "client still answers"
        await app.shutdown_sources()


# ── U3: the chords work from every focus context ─────────────────────────


@pytest.mark.asyncio
async def test_inspector_chord_from_composer_focus_keeps_typed_text() -> None:
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(132, 30)) as pilot:
        app.composer.text_area.focus()
        await pilot.pause()
        app.composer.text = "draft in progress"
        await pilot.press("ctrl+o")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed
        assert app.composer.text == "draft in progress"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_inspector_chord_from_transcript_focus() -> None:
    app = live_app(RecordingDispatcher())
    async with app.run_test(size=(132, 30)) as pilot:
        for index in range(60):
            feed(app, event("message.delta", {"text": f"scrollback {index}\n"}), seq=100 + index)
        await settle(app, pilot)
        app.transcript.focus()
        await pilot.pause()
        await pilot.press("ctrl+o")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed
        await app.shutdown_sources()


async def _open_models_picker(app: TalariaApp, pilot: Any) -> PickerDialog:
    app.composer.text_area.focus()
    await app.load_model_catalog()
    app.composer.text = "/models"
    await pilot.press("enter")
    await app.settle_live()
    await pilot.pause()
    screen = app.screen
    assert isinstance(screen, PickerDialog), f"expected the picker, got {type(screen).__name__}"
    return screen


@pytest.mark.asyncio
async def test_inspector_chord_with_picker_open_leaves_selection_alone() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher, admin_client=FakeAdminClient(_catalog()))
    async with app.run_test(size=(132, 30)) as pilot:
        dialog = await _open_models_picker(app, pilot)
        await pilot.press("enter")
        await pilot.pause()
        reopened = app.screen
        assert isinstance(reopened, PickerDialog)
        dialog = reopened
        before_rows = dialog.row_texts
        before_active = dialog.active_row_text

        await pilot.press("ctrl+o")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed
        assert isinstance(app.screen, PickerDialog), "the picker must stay open"
        assert dialog.row_texts == before_rows, "picker rows moved under the chord"
        assert dialog.active_row_text == before_active, "picker highlight moved"

        await pilot.press("escape")
        await pilot.pause()
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_chords_with_confirm_dialog_open() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher)
    async with app.run_test(size=(132, 30)) as pilot:
        feed(app, event("message.start", {}))
        await settle(app, pilot)
        assert app.state.turn == "streaming"

        app.push_screen(ConfirmDialog(title="probe", body=("line",)), lambda _answer: None)
        await pilot.pause()
        assert isinstance(app.screen, ConfirmDialog)

        await pilot.press("ctrl+o")
        await pilot.pause()
        assert app.inspector.is_effectively_collapsed
        assert isinstance(app.screen, ConfirmDialog), "the dialog must stay up"

        await pilot.press("ctrl+s")
        await app.settle_live()
        await pilot.pause()
        assert "session.interrupt" in _interrupt_calls(dispatcher)
        assert isinstance(app.screen, ConfirmDialog), "cancel must not dismiss"

        await pilot.press("escape")
        await pilot.pause()
        await app.shutdown_sources()


def test_build_app_bindings_covers_both_configurable_chords() -> None:
    from textual.binding import Binding

    from talaria.ui.app import build_app_bindings

    table = build_app_bindings("ctrl+x", "ctrl+y")
    by_action: dict[str, list[str]] = {}
    for entry in table:
        assert isinstance(entry, Binding)
        by_action.setdefault(entry.action, []).append(entry.key)
    assert by_action["toggle_inspector"] == ["ctrl+x"]
    assert by_action["interrupt"] == ["ctrl+y", "f4"]
    assert by_action["quit"] == ["ctrl+q"]
