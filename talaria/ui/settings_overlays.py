"""Focused settings overlays. Cancellation never commits.

Each overlay takes ``(view, on_result)`` and reports a small frozen result
record. Widgets request typed outcomes; they do not hold protocol state.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass

from textual import events, on
from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Static

from talaria.domain.settings import (
    ModelPickerView,
    ResetConfirmView,
    RestartConfirmView,
    RevealSecretView,
    TargetSwitchPrompt,
)
from talaria.ui.literal import literal_text

__all__ = (
    "ModelPickerOverlay",
    "ModelPickResult",
    "ResetConfirmOverlay",
    "ResetConfirmResult",
    "RestartConfirmOverlay",
    "RestartConfirmResult",
    "RevealSecretOverlay",
    "RevealSecretResult",
    "SwitchChoice",
    "TargetSwitchOverlay",
)


@dataclass(frozen=True)
class SwitchChoice:
    """Save / Discard / Stay. ``None`` from the overlay means cancelled."""

    choice: str


@dataclass(frozen=True)
class ResetConfirmResult:
    confirmed: bool


@dataclass(frozen=True)
class RestartConfirmResult:
    confirmed: bool


@dataclass(frozen=True)
class RevealSecretResult:
    reveal: bool


@dataclass(frozen=True)
class ModelPickResult:
    provider: str
    model: str


def _own_keyboard(event: events.Key) -> None:
    if event.key in ("escape", "tab", "shift+tab"):
        return
    event.stop()


class TargetSwitchOverlay(ModalScreen[SwitchChoice | None]):
    """Leave-target prompt: Save to the old profile, Discard, or Stay."""

    BINDINGS = [("escape", "cancel_overlay", "Cancel")]

    DEFAULT_CSS = """
    TargetSwitchOverlay {
        align: center middle;
    }
    TargetSwitchOverlay > Vertical {
        width: auto;
        min-width: 44;
        max-width: 72;
        height: auto;
        border: round $accent;
        background: $surface;
        padding: 0 1;
    }
    """

    def __init__(
        self,
        view: TargetSwitchPrompt,
        on_result: Callable[[SwitchChoice | None], None] | None = None,
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._view = view
        self._on_result = on_result

    def compose(self) -> ComposeResult:
        old = self._view.old_target.profile_name
        with Vertical():
            yield Static(
                literal_text(f"Unsaved edits on {old}"),
                markup=False,
                classes="settings--overlay-title",
            )
            yield Static(
                literal_text(f"Save to {old}"),
                markup=False,
            )
            with Horizontal():
                yield Button(f"Save to {old}", id="switch-save", compact=True)
                yield Button("Discard", id="switch-discard", compact=True)
                yield Button("Stay", id="switch-stay", compact=True)

    def on_key(self, event: events.Key) -> None:
        _own_keyboard(event)

    def _finish(self, result: SwitchChoice | None) -> None:
        if self._on_result is not None:
            self._on_result(result)
        self.dismiss(result)

    def action_cancel_overlay(self) -> None:
        self._finish(None)

    @on(Button.Pressed, "#switch-save")
    def _save(self) -> None:
        self._finish(SwitchChoice("save"))

    @on(Button.Pressed, "#switch-discard")
    def _discard(self) -> None:
        self._finish(SwitchChoice("discard"))

    @on(Button.Pressed, "#switch-stay")
    def _stay(self) -> None:
        self._finish(SwitchChoice("stay"))


class ResetConfirmOverlay(ModalScreen[ResetConfirmResult | None]):
    """Profile-explicit Reset. The target name is on the confirmation."""

    BINDINGS = [("escape", "cancel_overlay", "Cancel")]

    DEFAULT_CSS = """
    ResetConfirmOverlay {
        align: center middle;
    }
    ResetConfirmOverlay > Vertical {
        width: auto;
        min-width: 40;
        max-width: 72;
        height: auto;
        border: round $accent;
        background: $surface;
        padding: 0 1;
    }
    """

    def __init__(
        self,
        view: ResetConfirmView,
        on_result: Callable[[ResetConfirmResult | None], None] | None = None,
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._view = view
        self._on_result = on_result

    def compose(self) -> ComposeResult:
        name = self._view.target.profile_name
        with Vertical():
            yield Static(
                literal_text(f"Reset {name}"),
                markup=False,
                classes="settings--overlay-title",
            )
            yield Static(
                literal_text(
                    f"Restore selected keys on {name} to server defaults."
                ),
                markup=False,
            )
            with Horizontal():
                yield Button("Reset", id="reset-confirm", compact=True)
                yield Button("Cancel", id="reset-cancel", compact=True)

    def on_key(self, event: events.Key) -> None:
        _own_keyboard(event)

    def _finish(self, result: ResetConfirmResult | None) -> None:
        if self._on_result is not None:
            self._on_result(result)
        self.dismiss(result)

    def action_cancel_overlay(self) -> None:
        self._finish(None)

    @on(Button.Pressed, "#reset-confirm")
    def _confirm(self) -> None:
        self._finish(ResetConfirmResult(confirmed=True))

    @on(Button.Pressed, "#reset-cancel")
    def _cancel(self) -> None:
        self._finish(None)


class RestartConfirmOverlay(ModalScreen[RestartConfirmResult | None]):
    """Restart confirmation carrying the computed D8 plan."""

    BINDINGS = [("escape", "cancel_overlay", "Cancel")]

    DEFAULT_CSS = """
    RestartConfirmOverlay {
        align: left top;
    }
    RestartConfirmOverlay > Vertical {
        width: 100%;
        height: auto;
        border: round $accent;
        background: $surface;
        padding: 0 1;
    }
    """

    def __init__(
        self,
        view: RestartConfirmView,
        on_result: Callable[[RestartConfirmResult | None], None] | None = None,
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._view = view
        self._on_result = on_result

    def compose(self) -> ComposeResult:
        plan = self._view.plan
        with Vertical():
            yield Static(
                literal_text(plan.title),
                markup=False,
                classes="settings--overlay-title",
            )
            yield Static(literal_text(plan.dashboard_note), markup=False)
            with Horizontal():
                yield Button("Restart", id="restart-confirm", compact=True)
                yield Button("Cancel", id="restart-cancel", compact=True)

    def on_key(self, event: events.Key) -> None:
        _own_keyboard(event)

    def _finish(self, result: RestartConfirmResult | None) -> None:
        if self._on_result is not None:
            self._on_result(result)
        self.dismiss(result)

    def action_cancel_overlay(self) -> None:
        self._finish(None)

    @on(Button.Pressed, "#restart-confirm")
    def _confirm(self) -> None:
        self._finish(RestartConfirmResult(confirmed=True))

    @on(Button.Pressed, "#restart-cancel")
    def _cancel(self) -> None:
        self._finish(None)


class RevealSecretOverlay(ModalScreen[RevealSecretResult | None]):
    """One-shot reveal: key and masked value only. Never plaintext."""

    BINDINGS = [("escape", "cancel_overlay", "Cancel")]

    DEFAULT_CSS = """
    RevealSecretOverlay {
        align: center middle;
    }
    RevealSecretOverlay > Vertical {
        width: auto;
        min-width: 36;
        max-width: 72;
        height: auto;
        border: round $accent;
        background: $surface;
        padding: 0 1;
    }
    """

    def __init__(
        self,
        view: RevealSecretView,
        on_result: Callable[[RevealSecretResult | None], None] | None = None,
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._view = view
        self._on_result = on_result

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Static(
                literal_text(self._view.key),
                markup=False,
                classes="settings--overlay-title",
            )
            yield Static(literal_text(self._view.masked), markup=False)
            with Horizontal():
                yield Button("Reveal once", id="reveal-once", compact=True)
                yield Button("Cancel", id="reveal-cancel", compact=True)

    def on_key(self, event: events.Key) -> None:
        _own_keyboard(event)

    def _finish(self, result: RevealSecretResult | None) -> None:
        if self._on_result is not None:
            self._on_result(result)
        self.dismiss(result)

    def action_cancel_overlay(self) -> None:
        self._finish(None)

    @on(Button.Pressed, "#reveal-once")
    def _reveal(self) -> None:
        self._finish(RevealSecretResult(reveal=True))

    @on(Button.Pressed, "#reveal-cancel")
    def _cancel(self) -> None:
        self._finish(None)


class ModelPickerOverlay(ModalScreen[ModelPickResult | None]):
    """Catalog-order model options; apply assigns the highlighted row."""

    BINDINGS = [("escape", "cancel_overlay", "Cancel")]

    DEFAULT_CSS = """
    ModelPickerOverlay {
        align: center middle;
    }
    ModelPickerOverlay > Vertical {
        width: auto;
        min-width: 40;
        max-width: 72;
        height: auto;
        max-height: 20;
        border: round $accent;
        background: $surface;
        padding: 0 1;
    }
    """

    def __init__(
        self,
        view: ModelPickerView,
        on_result: Callable[[ModelPickResult | None], None] | None = None,
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._view = view
        self._on_result = on_result
        self._selected = 0
        for index, option in enumerate(view.options):
            if (
                option.provider == view.current_provider
                and option.model == view.current_model
            ):
                self._selected = index
                break

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Static(
                literal_text("Select model"),
                markup=False,
                classes="settings--overlay-title",
            )
            for index, option in enumerate(self._view.options):
                marker = ""
                if (
                    option.provider == self._view.current_provider
                    and option.model == self._view.current_model
                ):
                    marker = " (current)"
                yield Button(
                    f"{option.provider} {option.model}{marker}",
                    id=f"model-option-{index}",
                    compact=True,
                )
            yield Button("Apply", id="model-apply", compact=True)

    def on_key(self, event: events.Key) -> None:
        _own_keyboard(event)

    def _finish(self, result: ModelPickResult | None) -> None:
        if self._on_result is not None:
            self._on_result(result)
        self.dismiss(result)

    def action_cancel_overlay(self) -> None:
        self._finish(None)

    def on_button_pressed(self, event: Button.Pressed) -> None:
        button_id = event.button.id or ""
        if button_id.startswith("model-option-"):
            try:
                self._selected = int(button_id.removeprefix("model-option-"))
            except ValueError:
                return
            return
        if button_id == "model-apply":
            if not self._view.options:
                self._finish(None)
                return
            index = min(self._selected, len(self._view.options) - 1)
            option = self._view.options[index]
            self._finish(ModelPickResult(provider=option.provider, model=option.model))
