"""The ``/config`` view: what is in effect, where it came from, and the
allowlist write that changes it (issue #149, D8 recorded).

One modal screen in the theme-picker family. It displays exactly the four
:data:`~talaria.config.CONFIG_VIEW_KEYS` settings with each one's effective
value and source scope, and it edits the three status keys through
:func:`~talaria.config.save_status_settings` — the same byte-preserving
targeted write the theme selection uses, generalized (D8). ``theme.name`` is
display-only here: selection already persists through the theme picker, and a
second writer would be a second editor.

Everything the screen needs arrives through its constructor, so the screen
never reaches into the application and the mounting seam stays one call:

* ``cfg`` — the same merged configuration the process loaded.
* ``scopes`` — :func:`~talaria.config.setting_scopes`'s provenance walk, keyed
  like :data:`~talaria.config.CONFIG_VIEW_KEYS`.
* ``segments_now`` — the running bar's segment set (startup resolution plus
  any ``/bar`` session toggles). When it differs from what ``cfg`` resolved,
  the segments row's source reads ``session`` (D8), because the session layer
  is what is actually in effect; Apply still writes the startup-resolved
  shape and never touches the running set.
* ``open_theme_picker`` — the mounting seam's callback: awaited when the
  operator opens the picker from this view, returning the newly selected
  theme name or ``None`` when the picker was cancelled.

ADR-0002 holds by construction: this is a presentation surface and it
carries no transport work, only the configuration module's own write path.
"""

from __future__ import annotations

from collections.abc import Awaitable, Callable, Mapping, Sequence
from typing import Any, Literal

from textual import events, on
from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Static

from talaria.config import (
    Config,
    ConfigError,
    SettingScope,
    save_status_settings,
)
from talaria.status.contract import DEFAULT_STATUS_SEGMENTS
from talaria.ui.literal import literal_text

__all__ = ("ConfigViewScreen",)

#: The environment aliases that make a row read-only (D8): an environment
#: value shadows any file write, so the view refuses to offer one. Segments
#: has no alias and can never appear here.
_ENV_ALIASES: dict[str, str] = {
    "command": "TALARIA_STATUS_COMMAND",
    "interval_seconds": "TALARIA_STATUS_INTERVAL_SECONDS",
}

_NO_COMMAND_LABEL = "(no status script)"

_HINT = "tab moves · enter opens or presses · esc cancels"
_SEGMENTS_HINT = "space toggles · shift+↑↓ reorders the shown set"


def _display_command(value: object) -> str:
    """The honest row label for a command value: the text, or the empty state."""
    if not isinstance(value, str) or not value:
        return _NO_COMMAND_LABEL
    return value


def _display_segments(value: object) -> str:
    if isinstance(value, Sequence) and not isinstance(value, (str, bytes)):
        return ", ".join(str(name) for name in value)
    return str(value)


class _SegmentChooser(Vertical, can_focus=True):
    """The ordered multi-select over the seven known segment names (D8).

    Shown rows render first — selected names in display order (the order a
    save writes), then the remaining known names in their default order.
    ``space``/``enter`` toggle the focused row; toggling a name on appends it
    to the end of the selected set; ``shift+up``/``shift+down`` move a
    selected row within the selected set. Focus follows the *name*, not the
    position: a toggled row moves between the groups and keeps its highlight,
    and a reordered row keeps it while swapping, so the operator can press a
    direction twice without hunting for the row.
    """

    def __init__(self, selected: Sequence[str], **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._selected: list[str] = [name for name in selected]
        self._focused_name: str = ""

    @property
    def selected(self) -> tuple[str, ...]:
        """The shown names, in the display order a save would write."""
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
            # An unselected row has no order to change; the keypress is a no-op.
        else:
            return
        await self._paint()

    def _rows(self) -> list[Static]:
        return [node for node in self.query(".chooser--row") if isinstance(node, Static)]

    @property
    def row_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._rows())

    @property
    def active_row_text(self) -> str:
        for row in self._rows():
            if row.has_class("-active"):
                return str(row.content)
        return ""


class ConfigViewScreen(ModalScreen[str | None]):
    """Configuration, one screen: effective values, sources, and the write.

    Dismisses with the notice the app should surface (the last one, or
    ``None`` when nothing happened). The dismiss payload is a message, never
    a value: the writes already happened through
    :func:`~talaria.config.save_status_settings`, and restart-required state
    is read back from configuration on the next start.
    """

    BINDINGS = [("escape", "cancel_view", "Cancel")]

    DEFAULT_CSS = """
    ConfigViewScreen {
        align: center middle;
    }
    ConfigViewScreen > Vertical {
        width: auto;
        min-width: 56;
        max-width: 92;
        height: auto;
        max-height: 30;
        border: round $accent;
        background: $surface;
        padding: 0 1;
    }
    ConfigViewScreen .config--title {
        color: $accent;
        text-style: bold;
    }
    ConfigViewScreen .config--section {
        color: $accent;
        text-style: bold;
        margin-top: 1;
    }
    ConfigViewScreen .config--label {
        color: $text;
        margin-top: 1;
    }
    ConfigViewScreen .config--sub {
        color: $text-muted;
    }
    ConfigViewScreen .config--notice {
        color: $warning;
        margin-top: 1;
    }
    ConfigViewScreen .config--hint {
        color: $text-muted;
    }
    ConfigViewScreen .chooser--row {
        color: $text;
    }
    ConfigViewScreen .chooser--row.-active {
        background: $accent;
        color: $text;
        text-style: bold;
    }
    """

    def __init__(
        self,
        cfg: Config,
        scopes: Mapping[tuple[str, str], str],
        *,
        segments_now: Sequence[str] = (),
        open_theme_picker: Callable[[], Awaitable[str | None]] | None = None,
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._open_theme_picker = open_theme_picker
        # theme.name is display-only: an explicit select persists through the
        # picker, and the callback's return re-reads what that path did.
        self._theme_effective: str = str(cfg.get("theme", "name", default=""))
        self._theme_scope: str = scopes.get(("theme", "name"), "default")
        raw_command = cfg.get("status", "command")
        # The saved state is the raw value, not the display label: an empty
        # command edits as "" — the state the write persists (D8).
        self._command_saved: str = raw_command if isinstance(raw_command, str) else ""
        self._command_effective: str = _display_command(self._command_saved)
        self._interval_saved: int = int(cfg.get("status", "interval_seconds"))
        # What the running process resolved at startup — the "effective now"
        # half of the after-apply line. Apply never rewrites it: the status
        # settings take effect on restart (D8), so "effective now" stays the
        # startup value no matter what was saved.
        self._interval_effective: int = self._interval_saved
        self._segments_saved: tuple[str, ...] = tuple(
            cfg.get("status", "segments", default=())
        )
        self._segments_startup: tuple[str, ...] = self._segments_saved
        # The running set the seam handed in: startup resolution plus any
        # /bar session toggles. Missing (nothing running) reads as the
        # startup resolution, which is also what a save writes.
        self._segments_effective: tuple[str, ...] = tuple(segments_now) or (
            self._segments_saved
        )
        self._scopes = dict(scopes)
        # The keys the last apply wrote — they switch to the
        # "saved / effective now / takes effect on restart" line (D8).
        self._written: set[str] = set()
        self._notice = ""
        self._theme_line: Static | None = None
        self._theme_button: Button | None = None
        self._command_input: Input | None = None
        self._command_line: Static | None = None
        self._interval_input: Input | None = None
        self._interval_line: Static | None = None
        self._segments_line: Static | None = None
        self._notice_line: Static | None = None
        self._chooser: _SegmentChooser | None = None

    # ── read access, so tests never reach through to the framework ───────

    @property
    def theme_line_text(self) -> str:
        return "" if self._theme_line is None else str(self._theme_line.content)

    @property
    def command_sub_text(self) -> str:
        return "" if self._command_line is None else str(self._command_line.content)

    @property
    def interval_sub_text(self) -> str:
        return "" if self._interval_line is None else str(self._interval_line.content)

    @property
    def segments_sub_text(self) -> str:
        return "" if self._segments_line is None else str(self._segments_line.content)

    @property
    def notice_text(self) -> str:
        return self._notice

    @property
    def chooser_selected(self) -> tuple[str, ...]:
        return () if self._chooser is None else self._chooser.selected

    @property
    def chooser_row_texts(self) -> tuple[str, ...]:
        return () if self._chooser is None else self._chooser.row_texts

    @property
    def chooser_active_row_text(self) -> str:
        return "" if self._chooser is None else self._chooser.active_row_text

    # ── scope and readonly rules ─────────────────────────────────────────

    def _scope_of(self, key: str) -> SettingScope | str:
        return self._scopes.get(("status", key), "default")

    def _is_env_readonly(self, key: str) -> bool:
        return self._scope_of(key) == "environment"

    def _command_readonly(self) -> bool:
        return self._is_env_readonly("command")

    def _interval_readonly(self) -> bool:
        return self._is_env_readonly("interval_seconds")

    def _segments_scope(self) -> str:
        if self._segments_effective != self._segments_startup:
            return "session"
        return self._scope_of("segments")

    def _sub_text(self, key: str, saved: str, effective: str) -> str:
        """One status row's line: source and mode, or the after-apply reading.

        The after-apply shape is D8's literal contract — "saved: X ·
        effective now: Y · takes effect on restart" — and only the keys the
        apply wrote switch to it; an unwritten row keeps the honest
        "nothing changed" reading.
        """
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

    # ── composition and paint ────────────────────────────────────────────

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Static(
                literal_text("configuration — effective values and sources"),
                markup=False,
                classes="config--title",
            )
            yield Static(
                literal_text("theme"), markup=False, classes="config--section"
            )
            self._theme_line = Static(
                literal_text(self._theme_row_text()),
                markup=False,
                classes="config--sub",
            )
            yield self._theme_line
            self._theme_button = Button("open the theme picker", id="theme-picker")
            if self._open_theme_picker is None:
                # Without the seam callback the row stays honest rather than
                # promising a picker that was never wired.
                self._theme_button.display = False
            yield self._theme_button

            yield Static(
                literal_text("status — takes effect on restart"),
                markup=False,
                classes="config--section",
            )
            yield Static(
                literal_text("command"), markup=False, classes="config--label"
            )
            self._command_input = Input(
                value=self._command_saved,
                id="command",
                disabled=self._command_readonly(),
            )
            yield self._command_input
            self._command_line = Static(
                literal_text(
                    self._sub_text(
                        "command",
                        _display_command(self._command_saved),
                        self._command_effective,
                    )
                ),
                markup=False,
                classes="config--sub",
            )
            yield self._command_line

            yield Static(
                literal_text("interval_seconds (1–3600)"),
                markup=False,
                classes="config--label",
            )
            self._interval_input = Input(
                value=str(self._interval_saved),
                id="interval",
                disabled=self._interval_readonly(),
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
                classes="config--sub",
            )
            yield self._interval_line

            yield Static(
                literal_text(f"segments ({_SEGMENTS_HINT})"),
                markup=False,
                classes="config--label",
            )
            self._chooser = _SegmentChooser(self._segments_saved, id="segments")
            yield self._chooser
            self._segments_line = Static(
                literal_text(self._segments_sub_text()),
                markup=False,
                classes="config--sub",
            )
            yield self._segments_line

            self._notice_line = Static(
                literal_text(""), markup=False, classes="config--notice"
            )
            yield self._notice_line
            with Horizontal(classes="config--actions"):
                yield Button("apply — save to user configuration", id="apply-user")
                yield Button("save to repository", id="apply-repository")
                yield Button("cancel", id="cancel")
            yield Static(literal_text(_HINT), markup=False, classes="config--hint")

    async def on_mount(self) -> None:
        await self._paint()

    def _theme_row_text(self) -> str:
        return (
            f"theme.name {self._theme_effective} · source: {self._theme_scope} · live"
        )

    def _segments_sub_text(self) -> str:
        effective = _display_segments(self._segments_effective)
        if "segments" in self._written:
            # D8's after-apply reading, with the running set as "effective
            # now": a saved selection and a /bar-toggled running set differ
            # until the restart, and the row says both.
            return (
                f"saved: {_display_segments(self._segments_saved)} · "
                f"effective now: {effective} · takes effect on restart"
            )
        if self._segments_scope() == "session":
            # The running set a /bar toggle changed: the row names the layer
            # actually in effect and Apply writes nothing into it (D8).
            return (
                f"effective now: {effective} · "
                "source: session (/bar) · mode: restart"
            )
        return self._sub_text(
            "segments",
            _display_segments(self._segments_saved),
            effective,
        )

    async def _paint(self) -> None:
        if self._theme_line is not None:
            self._theme_line.update(literal_text(self._theme_row_text()))
        if self._command_line is not None:
            self._command_line.update(
                literal_text(
                    self._sub_text(
                        "command",
                        _display_command(self._command_saved),
                        self._command_effective,
                    )
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

    # ── the modal owns the keyboard, like the picker does ─────────────────

    def on_key(self, event: events.Key) -> None:
        """Stop every key except the two the framework needs.

        Without this, a chord the application binds beneath the modal —
        the inspector toggle, the interrupt — would act on the interface
        *under* the screen the operator is reading. ``tab``/``shift+tab``
        must pass so focus moves, and ``escape`` must pass so the screen's
        own binding cancels; the children that handle keys (the inputs, the
        chooser, the buttons) consume theirs before this handler sees them.
        """
        if event.key in ("escape", "tab", "shift+tab"):
            return
        event.stop()

    # ── actions ──────────────────────────────────────────────────────────

    def action_cancel_view(self) -> None:
        self.dismiss(self._notice or None)

    @on(Button.Pressed, "#cancel")
    def _cancel(self) -> None:
        self.action_cancel_view()

    @on(Button.Pressed, "#theme-picker")
    async def _open_picker(self) -> None:
        if self._open_theme_picker is None:
            return
        name = await self._open_theme_picker()
        if name:
            # The W1 select path already persisted the selection to user
            # scope; this row only re-reads what that path did.
            self._theme_effective = name
            self._theme_scope = "user"
            self._notice = (
                f"theme.name {name} applied live and saved to user configuration"
            )
        else:
            self._notice = "theme picker cancelled — nothing changed"
        await self._paint()

    async def _apply(self, scope: Literal["user", "repository"]) -> None:
        command_input = self._command_input
        interval_input = self._interval_input
        chooser = self._chooser
        if command_input is None or interval_input is None or chooser is None:
            # compose always ran before a button press can arrive
            return

        interval_text = interval_input.value.strip()
        try:
            interval_value = int(interval_text)
        except ValueError:
            interval_value = None
        if interval_value is None or not 1 <= interval_value <= 3600:
            # D8: rejected inline, nothing written.
            self._notice = (
                "status.interval_seconds must be an integer between "
                "1 and 3600 — nothing written"
            )
            await self._paint()
            return

        changes: dict[str, Any] = {}
        if not self._command_readonly():
            command_text = command_input.value
            if command_text != self._command_saved:
                changes["command"] = command_text
        if not self._interval_readonly():
            if interval_value != self._interval_saved:
                changes["interval_seconds"] = interval_value
        chosen = chooser.selected
        if chosen != self._segments_saved:
            changes["segments"] = chosen

        if scope == "user":
            blocked = [key for key in changes if self._scope_of(key) == "repository"]
            if blocked:
                # The shadowing rule the operator selected (D8): the
                # repository file beats the user file, so a user-scope save
                # of these rows would be written and never read. The whole
                # apply refuses — nothing written — and names the key.
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
            # The writer's refusals surface here verbatim — including the
            # "edit the file by hand" outcome for a file it cannot match.
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