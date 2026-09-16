"""Settings row widgets and the Talaria-owned configuration branch.

Widgets consume :class:`~talaria.domain.settings.FieldRowView` records and
expose editors. They hold no protocol or session state (ADR-0002).
"""

from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from typing import Any, Literal

from textual import events, on
from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.widgets import Button, Input, Static

from talaria.config import (
    ConfigError,
    SettingScope,
    save_status_settings,
)
from talaria.domain.settings import FieldRowView, effect_summary_label
from talaria.status.contract import DEFAULT_STATUS_SEGMENTS
from talaria.ui.config_view import ConfigViewResult
from talaria.ui.literal import literal_text

__all__ = (
    "FieldRowWidget",
    "SettingsGroupWidget",
    "TalariaOwnedSettings",
    "coerce_row_value",
    "display_row_value",
    "nested_assign",
    "row_widget_id",
)


_ENV_ALIASES: dict[str, str] = {
    "command": "TALARIA_STATUS_COMMAND",
    "interval_seconds": "TALARIA_STATUS_INTERVAL_SECONDS",
}

_NO_COMMAND_LABEL = "(no status script)"
_SEGMENTS_HINT = "space toggles · shift+↑↓ reorders the shown set"


def row_widget_id(key: str) -> str:
    return "settings-row-" + key.replace(".", "-")


def display_row_value(row: FieldRowView) -> str:
    value = row.pending_value if row.pending_value is not None else row.saved_value
    if isinstance(value, bool):
        return "true" if value else "false"
    if value is None:
        return ""
    return str(value)


def coerce_row_value(row: FieldRowView, text: str) -> object:
    """Coerce an editor string back toward the row's schema type."""
    if row.type == "number":
        stripped = text.strip()
        if not stripped:
            return text
        try:
            number = float(stripped)
        except ValueError:
            return text
        if number.is_integer() and "." not in stripped and "e" not in stripped.lower():
            return int(number)
        return number
    if row.type == "boolean":
        lowered = text.strip().lower()
        if lowered in {"true", "1", "yes"}:
            return True
        if lowered in {"false", "0", "no"}:
            return False
        return text
    return text


def nested_assign(tree: dict[str, Any], dotted: str, value: object) -> None:
    parts = dotted.split(".")
    node = tree
    for part in parts[:-1]:
        existing = node.get(part)
        if not isinstance(existing, dict):
            existing = {}
            node[part] = existing
        node = existing
    node[parts[-1]] = value


class FieldRowWidget(Vertical):
    """One projected field: label, help, chips, and an editor or read-only."""

    def __init__(self, row: FieldRowView, owner: str, **kwargs: object) -> None:
        super().__init__(id=row_widget_id(row.key), **kwargs)  # type: ignore[arg-type]
        self.row = row
        self.owner = owner
        self.editor: Input | None = None

    def compose(self) -> ComposeResult:
        yield Static(
            literal_text(self.row.label), markup=False, classes="settings--label"
        )
        if self.row.help_text:
            yield Static(
                literal_text(self.row.help_text),
                markup=False,
                classes="settings--help",
            )
        chips = " · ".join(
            part
            for part in (self.owner, self.row.provenance)
            if part
        )
        if chips:
            yield Static(
                literal_text(chips), markup=False, classes="settings--chips"
            )
        if self.row.effect:
            try:
                effect = effect_summary_label(self.row.effect)
            except ValueError:
                effect = self.row.effect
            yield Static(
                literal_text(effect), markup=False, classes="settings--effect"
            )
        if self.row.read_only:
            yield Static(
                literal_text("read-only"), markup=False, classes="settings--readonly"
            )
            shown = (
                display_row_value(self.row)
                if self.row.saved_value is not None or self.row.effective_value is not None
                else ""
            )
            if shown:
                yield Static(
                    literal_text(shown), markup=False, classes="settings--value"
                )
        else:
            self.editor = Input(value=display_row_value(self.row))
            yield self.editor
        if self.row.validation_message:
            yield Static(
                literal_text(self.row.validation_message),
                markup=False,
                classes="settings--validation",
            )


class SettingsGroupWidget(Vertical):
    """Ownership group. Host groups mount no action buttons."""

    def __init__(
        self, owner: str, title: str, rows: Sequence[FieldRowView], **kwargs: object
    ) -> None:
        super().__init__(id=f"settings-group-{owner}", **kwargs)  # type: ignore[arg-type]
        self.owner = owner
        self.title = title
        self.rows = rows
        self.row_widgets: list[FieldRowWidget] = []

    def compose(self) -> ComposeResult:
        yield Static(
            literal_text(self.title), markup=False, classes="settings--group-title"
        )
        yield Static(
            literal_text(self.owner), markup=False, classes="settings--owner"
        )
        for row in self.rows:
            widget = FieldRowWidget(row, self.owner)
            self.row_widgets.append(widget)
            yield widget


class _SegmentChooser(Vertical, can_focus=True):
    """Ordered multi-select over the seven known status-bar segment names."""

    def __init__(self, selected: Sequence[str], **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._selected: list[str] = [name for name in selected]
        self._focused_name: str = ""

    @property
    def selected(self) -> tuple[str, ...]:
        return tuple(self._selected)

    def _display_order(self) -> list[str]:
        tail = [name for name in DEFAULT_STATUS_SEGMENTS if name not in self._selected]
        return self._selected + tail

    async def on_mount(self) -> None:
        order = self._display_order()
        self._focused_name = order[0] if order else ""
        await self._paint()

    async def _paint(self) -> None:
        order = self._display_order()
        if self._focused_name not in order:
            self._focused_name = order[0] if order else ""
        await self.remove_children()
        for name in order:
            marker = "[x]" if name in self._selected else "[ ]"
            classes = "chooser--row"
            if name == self._focused_name:
                classes += " -active"
            await self.mount(
                Static(literal_text(f"{marker} {name}"), markup=False, classes=classes)
            )

    async def on_key(self, event: events.Key) -> None:
        order = self._display_order()
        if not order or self._focused_name not in order:
            return
        key = event.key
        if key in ("up", "down"):
            event.stop()
            position = order.index(self._focused_name)
            if key == "up":
                position = max(0, position - 1)
            else:
                position = min(len(order) - 1, position + 1)
            self._focused_name = order[position]
        elif key in ("space", "enter"):
            event.stop()
            name = self._focused_name
            if name in self._selected:
                self._selected.remove(name)
            else:
                self._selected.append(name)
        elif key in ("shift+up", "shift+down"):
            event.stop()
            name = self._focused_name
            if name in self._selected:
                index = self._selected.index(name)
                if key == "shift+up" and index > 0:
                    swap = index - 1
                elif key == "shift+down" and index < len(self._selected) - 1:
                    swap = index + 1
                else:
                    swap = index
                if swap != index:
                    self._selected[index], self._selected[swap] = (
                        self._selected[swap],
                        self._selected[index],
                    )
        else:
            return
        await self._paint()


class TalariaOwnedSettings(Vertical):
    """The existing /config Talaria branch: theme, status, apply.

    Lives inside the settings workspace when the app mounts it. Writes go
    through :func:`~talaria.config.save_status_settings`; the parent screen
    dismisses with :class:`ConfigViewResult` so the mounting seam is unchanged.
    """

    def __init__(
        self,
        *,
        theme_name: str,
        status_command: str,
        status_interval_seconds: int,
        status_segments: Sequence[str],
        scopes: Mapping[tuple[str, str], str],
        segments_now: Sequence[str] = (),
        on_dismiss: Callable[[ConfigViewResult | None], None] | None = None,
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._on_dismiss = on_dismiss
        self._theme_effective = theme_name
        self._theme_scope = scopes.get(("theme", "name"), "default")
        self._command_saved = status_command
        self._command_effective = (
            status_command if status_command else _NO_COMMAND_LABEL
        )
        self._interval_saved = status_interval_seconds
        self._interval_effective = status_interval_seconds
        self._segments_saved = tuple(status_segments)
        self._segments_startup = self._segments_saved
        self._segments_effective = tuple(segments_now) or self._segments_saved
        self._scopes = dict(scopes)
        self._written: set[str] = set()
        self._notice = ""
        self._theme_line: Static | None = None
        self._command_input: Input | None = None
        self._command_line: Static | None = None
        self._interval_input: Input | None = None
        self._interval_line: Static | None = None
        self._segments_line: Static | None = None
        self._notice_line: Static | None = None
        self._chooser: _SegmentChooser | None = None

    @property
    def notice_text(self) -> str:
        return self._notice

    def _scope_of(self, key: str) -> SettingScope | str:
        return self._scopes.get(("status", key), "default")

    def _is_env_readonly(self, key: str) -> bool:
        return self._scope_of(key) == "environment"

    def _theme_row_text(self) -> str:
        return (
            f"theme.name {self._theme_effective} · source: {self._theme_scope} · live"
        )

    def _sub_text(self, key: str, saved: str, effective: str) -> str:
        if key in self._written:
            return (
                f"saved: {saved} · effective now: {effective} · "
                "takes effect on restart"
            )
        if self._is_env_readonly(key):
            return (
                f"set by environment variable {_ENV_ALIASES[key]} — edit the "
                "environment; a file write would be shadowed"
            )
        scope = self._scope_of(key)
        if scope == "repository":
            return (
                f"effective: {effective} · source: repository file · mode: restart · "
                "a user-file save would be shadowed — save to repository instead"
            )
        return f"effective: {effective} · source: {scope} · mode: restart"

    def _segments_sub_text(self) -> str:
        effective = ", ".join(self._segments_effective)
        if "segments" in self._written:
            return (
                f"saved: {', '.join(self._segments_saved)} · "
                f"effective now: {effective} · takes effect on restart"
            )
        if self._segments_effective != self._segments_startup:
            return (
                f"effective now: {effective} · "
                "source: session (/bar) · mode: restart"
            )
        return self._sub_text("segments", ", ".join(self._segments_saved), effective)

    def compose(self) -> ComposeResult:
        yield Static(
            literal_text("Talaria"), markup=False, classes="settings--group-title"
        )
        yield Static(
            literal_text("talaria-user"), markup=False, classes="settings--owner"
        )
        self._theme_line = Static(
            literal_text(self._theme_row_text()),
            markup=False,
            classes="settings--help",
        )
        yield self._theme_line
        yield Button(
            "open the theme picker (closes this view)",
            id="theme-picker",
            compact=True,
        )
        yield Static(
            literal_text("status — takes effect on restart"),
            markup=False,
            classes="settings--group-title",
        )
        yield Static(literal_text("command"), markup=False, classes="settings--label")
        self._command_input = Input(
            value=self._command_saved,
            id="command",
            disabled=self._is_env_readonly("command"),
        )
        yield self._command_input
        command_saved = (
            self._command_saved if self._command_saved else _NO_COMMAND_LABEL
        )
        self._command_line = Static(
            literal_text(
                self._sub_text("command", command_saved, self._command_effective)
            ),
            markup=False,
            classes="settings--help",
        )
        yield self._command_line
        yield Static(
            literal_text("interval_seconds (1–3600)"),
            markup=False,
            classes="settings--label",
        )
        self._interval_input = Input(
            value=str(self._interval_saved),
            id="interval",
            disabled=self._is_env_readonly("interval_seconds"),
        )
        yield self._interval_input
        self._interval_line = Static(
            literal_text(
                self._sub_text(
                    "interval_seconds",
                    str(self._interval_saved),
                    str(self._interval_effective),
                )
            ),
            markup=False,
            classes="settings--help",
        )
        yield self._interval_line
        yield Static(
            literal_text(f"segments ({_SEGMENTS_HINT})"),
            markup=False,
            classes="settings--label",
        )
        self._chooser = _SegmentChooser(self._segments_saved, id="segments")
        yield self._chooser
        self._segments_line = Static(
            literal_text(self._segments_sub_text()),
            markup=False,
            classes="settings--help",
        )
        yield self._segments_line
        self._notice_line = Static(
            literal_text(""), markup=False, classes="settings--notice"
        )
        yield self._notice_line
        with Horizontal(classes="settings--talaria-actions"):
            yield Button(
                "apply — save to user configuration",
                id="apply-user",
                compact=True,
            )
            yield Button(
                "save to repository", id="apply-repository", compact=True
            )
            yield Button("cancel", id="cancel", compact=True)

    async def on_mount(self) -> None:
        await self._paint()

    async def _paint(self) -> None:
        if self._theme_line is not None:
            self._theme_line.update(literal_text(self._theme_row_text()))
        command_saved = (
            self._command_saved if self._command_saved else _NO_COMMAND_LABEL
        )
        if self._command_line is not None:
            self._command_line.update(
                literal_text(
                    self._sub_text("command", command_saved, self._command_effective)
                )
            )
        if self._interval_line is not None:
            self._interval_line.update(
                literal_text(
                    self._sub_text(
                        "interval_seconds",
                        str(self._interval_saved),
                        str(self._interval_effective),
                    )
                )
            )
        if self._segments_line is not None:
            self._segments_line.update(literal_text(self._segments_sub_text()))
        if self._notice_line is not None:
            self._notice_line.update(literal_text(self._notice))

    def _dismiss(self, result: ConfigViewResult | None) -> None:
        if self._on_dismiss is not None:
            self._on_dismiss(result)

    @on(Button.Pressed, "#cancel")
    def _cancel(self) -> None:
        if self._notice:
            self._dismiss(ConfigViewResult(notice=self._notice))
        else:
            self._dismiss(None)

    @on(Button.Pressed, "#theme-picker")
    def _open_picker_from_row(self) -> None:
        self._dismiss(
            ConfigViewResult(notice=self._notice, open_theme_picker=True)
        )

    async def _apply(self, scope: Literal["user", "repository"]) -> None:
        command_input = self._command_input
        interval_input = self._interval_input
        chooser = self._chooser
        if command_input is None or interval_input is None or chooser is None:
            return

        interval_text = interval_input.value.strip()
        try:
            interval_value = int(interval_text)
        except ValueError:
            interval_value = None
        if interval_value is None or not 1 <= interval_value <= 3600:
            self._notice = (
                "status.interval_seconds must be an integer between "
                "1 and 3600 — nothing written"
            )
            await self._paint()
            return

        changes: dict[str, Any] = {}
        if not self._is_env_readonly("command"):
            command_text = command_input.value
            if command_text != self._command_saved:
                changes["command"] = command_text
        if not self._is_env_readonly("interval_seconds"):
            if interval_value != self._interval_saved:
                changes["interval_seconds"] = interval_value
        chosen = chooser.selected
        if chosen != self._segments_saved:
            changes["segments"] = chosen

        if scope == "user":
            blocked = [key for key in changes if self._scope_of(key) == "repository"]
            if blocked:
                named = ", status.".join(sorted(blocked))
                self._notice = (
                    f"status.{named} is set by the repository file; a "
                    "user-file write would be shadowed — save to repository"
                )
                await self._paint()
                return

        if not changes:
            self._notice = "nothing changed — no write"
            await self._paint()
            return

        try:
            save_status_settings(changes, scope)
        except ConfigError as exc:
            self._notice = str(exc)
            await self._paint()
            return

        self._written = set(changes)
        if "command" in changes:
            self._command_saved = str(changes["command"])
        if "interval_seconds" in changes:
            self._interval_saved = int(changes["interval_seconds"])
        if "segments" in changes:
            self._segments_saved = tuple(changes["segments"])
        self._notice = f"saved to {scope} configuration"
        await self._paint()

    @on(Button.Pressed, "#apply-user")
    async def _apply_user(self) -> None:
        await self._apply("user")

    @on(Button.Pressed, "#apply-repository")
    async def _apply_repository(self) -> None:
        await self._apply("repository")
