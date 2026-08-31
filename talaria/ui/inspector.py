"""Responsive, keyboard-controlled projection of held inspector state."""

from __future__ import annotations

from typing import ClassVar

from textual import events
from textual.app import ComposeResult
from textual.binding import Binding, BindingType
from textual.containers import Vertical, VerticalScroll
from textual.message import Message
from textual.widget import Widget
from textual.widgets import Static

from talaria.domain.changes import (
    ChangedFileView,
    DiffDocument,
    DiffSelection,
    InspectorTaskView,
    InspectorView,
)
from talaria.ui.literal import literal_text

DEFAULT_INSPECTOR_WIDTH = 36
MIN_INSPECTOR_WIDTH = 28
MAX_INSPECTOR_WIDTH = 48
INSPECTOR_WIDTH_STEP = 4
INSPECTOR_DOCK_BREAKPOINT = 120
NARROW_OVERLAY_INSET_BREAKPOINT = 32
EMPTY_SECTION = "[none available from this session]"

_STATUS_GLYPHS: dict[str, str] = {
    "queued": "[..]",
    "running": "[>]",
    "completed": "[ok]",
    "error": "[!]",
    "failed": "[x]",
    "interrupted": "[-]",
    "timeout": "[t]",
    "waiting": "[!]",
    "blocked": "[x]",
    "requested": "[..]",
    "unavailable": "[!]",
}


class InspectorTaskRow(Static):
    """One focusable task row with a permanently reserved focus gutter."""

    can_focus = True

    def __init__(self, task: InspectorTaskView) -> None:
        super().__init__("", markup=False, classes="inspector--task")
        self.task_view = task

    def on_mount(self) -> None:
        self._refresh_line()

    def on_focus(self) -> None:
        self._refresh_line()

    def on_blur(self) -> None:
        self._refresh_line()

    def _refresh_line(self) -> None:
        gutter = ">" if self.has_focus else " "
        glyph = _STATUS_GLYPHS.get(self.task_view.status, "[?]")
        detail = f" — {self.task_view.detail}" if self.task_view.detail else ""
        self.update(
            literal_text(
                f"{gutter} {glyph} {self.task_view.label}  {self.task_view.status}{detail}"
            )
        )


class InspectorFileRow(Static):
    """One held changed file; Enter publishes the immutable U5 selection."""

    class Selected(Message):
        def __init__(self, changed_file: ChangedFileView) -> None:
            super().__init__()
            self.changed_file = changed_file

    can_focus = True
    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("enter", "select_file", "open read-only diff", show=False),
    ]

    def __init__(self, changed_file: ChangedFileView) -> None:
        super().__init__("", markup=False, classes="inspector--file")
        self.changed_file = changed_file

    def on_mount(self) -> None:
        self._refresh_line()

    def on_focus(self) -> None:
        self._refresh_line()

    def on_blur(self) -> None:
        self._refresh_line()

    def action_select_file(self) -> None:
        self.post_message(self.Selected(self.changed_file))

    def on_click(self, event: events.Click) -> None:
        event.stop()
        self.focus()
        self.action_select_file()

    def _refresh_line(self) -> None:
        gutter = ">" if self.has_focus else " "
        self.update(
            literal_text(f"{gutter} {self.changed_file.status} {self.changed_file.path}")
        )


class Inspector(VerticalScroll):
    """Four inspector sections plus session-only responsive geometry.

    The app calls :meth:`set_terminal_width` from its screen resize boundary.
    Keeping that input explicit avoids a timer and avoids mistaking this
    widget's own 28–48-column resize event for the terminal width.
    """

    class FileSelected(Message):
        """A held file selection for the app's read-only diff screen."""

        def __init__(self, selection: DiffSelection) -> None:
            super().__init__()
            self.selection = selection

    DEFAULT_CSS = """
    Inspector {
        height: 1fr;
        width: 36;
        padding: 0 1;
        border: solid $talaria-inspector-border;
        background: $talaria-inspector-background;
        color: $text;
        scrollbar-size: 1 1;
    }
    Inspector.-inspector-hidden {
        display: none;
    }
    Inspector.-inspector-overlay {
        overlay: screen;
        position: absolute;
        height: 100%;
    }
    Inspector > .inspector--heading {
        height: 1;
        margin-top: 1;
        color: $talaria-inspector-heading;
        text-style: bold;
        text-wrap: nowrap;
    }
    Inspector > .inspector--heading-first {
        margin-top: 0;
    }
    Inspector > .inspector--section {
        width: 1fr;
        height: auto;
    }
    Inspector .inspector--task,
    Inspector .inspector--file,
    Inspector .inspector--empty {
        width: 1fr;
        height: 1;
        text-wrap: nowrap;
        text-overflow: ellipsis;
    }
    Inspector .inspector--task:focus,
    Inspector .inspector--file:focus,
    Inspector .inspector--file-selected {
        color: $talaria-inspector-heading;
        text-style: bold;
    }
    Inspector .inspector--context,
    Inspector .inspector--operation {
        width: 1fr;
        height: auto;
    }
    """

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("shift+left", "shrink", "narrower", show=False),
        Binding("shift+right", "widen", "wider", show=False),
        Binding("up", "previous_row", "previous section row", show=False),
        Binding("down", "next_row", "next section row", show=False),
        Binding("escape", "close_overlay", "close inspector", show=False),
    ]

    can_focus = True

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self.panel_width = DEFAULT_INSPECTOR_WIDTH
        self.requested_collapsed = False
        self.auto_collapsed = False
        self.overlay_open = False
        self._diff_open = False
        self._terminal_width = INSPECTOR_DOCK_BREAKPOINT
        self._previous_focus: Widget | None = None
        self._view: InspectorView | None = None
        self._selected_file_key: str | None = None
        self._tasks: Vertical | None = None
        self._context_widget: Static | None = None
        self._files: Vertical | None = None
        self._operation_widget: Static | None = None
        self._task_rows: list[InspectorTaskRow] = []
        self._file_rows: list[InspectorFileRow] = []

    def compose(self) -> ComposeResult:
        yield Static("TASKS", markup=False, classes="inspector--heading inspector--heading-first")
        self._tasks = Vertical(classes="inspector--section")
        yield self._tasks
        yield Static("CONTEXT", markup=False, classes="inspector--heading")
        self._context_widget = Static("", markup=False, classes="inspector--context")
        yield self._context_widget
        yield Static("CHANGED FILES", markup=False, classes="inspector--heading")
        self._files = Vertical(classes="inspector--section")
        yield self._files
        yield Static("OPERATION DETAILS", markup=False, classes="inspector--heading")
        self._operation_widget = Static("", markup=False, classes="inspector--operation")
        yield self._operation_widget

    def on_mount(self) -> None:
        self.set_terminal_width(self.app.size.width)

    @property
    def is_docked(self) -> bool:
        return not self.auto_collapsed and not self.requested_collapsed

    @property
    def is_overlay(self) -> bool:
        return self.auto_collapsed and self.overlay_open

    @property
    def is_effectively_collapsed(self) -> bool:
        return self._diff_open or (not self.is_docked and not self.is_overlay)

    @property
    def document(self) -> DiffDocument:
        """Return the immutable diff set from the most recent projection."""
        return DiffDocument() if self._view is None else self._view.document

    @property
    def is_temporarily_hidden(self) -> bool:
        """Whether a modal diff currently owns the full terminal width."""
        return self._diff_open

    @property
    def effective_width(self) -> int:
        if not self.is_overlay:
            return self.panel_width
        if self._terminal_width < NARROW_OVERLAY_INSET_BREAKPOINT:
            return self._terminal_width
        return min(self.panel_width, self._terminal_width - 2)

    @property
    def selected_file_key(self) -> str | None:
        return self._selected_file_key

    @property
    def task_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._task_rows)

    @property
    def context_text(self) -> str:
        return "" if self._context_widget is None else str(self._context_widget.content)

    @property
    def file_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._file_rows)

    @property
    def operation_text(self) -> str:
        return (
            ""
            if self._operation_widget is None
            else str(self._operation_widget.content)
        )

    async def apply(self, view: InspectorView) -> None:
        """Replace all four sections from one immutable domain projection."""
        self._view = view
        if self._tasks is None or self._context_widget is None or self._files is None:
            return
        if self._operation_widget is None:
            return

        await self._tasks.remove_children()
        self._task_rows = [InspectorTaskRow(task) for task in view.tasks]
        if self._task_rows:
            await self._tasks.mount(*self._task_rows)
        else:
            await self._tasks.mount(_empty_row())

        self._context_widget.update(literal_text("\n".join(_context_lines(view))))

        await self._files.remove_children()
        self._file_rows = [InspectorFileRow(changed_file) for changed_file in view.changed_files]
        if self._file_rows:
            await self._files.mount(*self._file_rows)
        else:
            await self._files.mount(_empty_row())

        if self._selected_file_key not in {file.key for file in view.changed_files}:
            self._selected_file_key = None
        self._refresh_file_selection()
        self._operation_widget.update(literal_text("\n".join(_operation_lines(view))))

    def set_terminal_width(self, width: int) -> None:
        """Apply the inclusive 120-column dock breakpoint synchronously."""
        width = max(0, width)
        was_auto_collapsed = self.auto_collapsed
        self._terminal_width = width
        self.auto_collapsed = width < INSPECTOR_DOCK_BREAKPOINT
        if self.auto_collapsed and not was_auto_collapsed:
            self.overlay_open = False
        elif not self.auto_collapsed:
            self.overlay_open = False
        self._sync_geometry()

    def toggle(self) -> None:
        """Toggle the dock on wide screens or the non-reflowing narrow overlay."""
        if self.auto_collapsed:
            if self.overlay_open:
                self._close_overlay(restore_focus=True)
                return
            self._remember_focus()
            self.overlay_open = True
            self._sync_geometry()
            self.call_after_refresh(self._focus_first_row)
            return

        self.requested_collapsed = not self.requested_collapsed
        self.overlay_open = False
        self._sync_geometry()
        if not self.requested_collapsed:
            self.call_after_refresh(self._focus_first_row)

    def set_diff_open(self, opened: bool) -> None:
        """Hide for a diff modal without changing the operator's preference."""
        self._diff_open = opened
        self._sync_geometry()

    def action_shrink(self) -> None:
        self._resize_by(-INSPECTOR_WIDTH_STEP)

    def action_widen(self) -> None:
        self._resize_by(INSPECTOR_WIDTH_STEP)

    def action_previous_row(self) -> None:
        self._move_focus(-1)

    def action_next_row(self) -> None:
        self._move_focus(1)

    def action_close_overlay(self) -> None:
        if self.is_overlay:
            self._close_overlay(restore_focus=True)

    def on_inspector_file_row_selected(self, message: InspectorFileRow.Selected) -> None:
        message.stop()
        self._selected_file_key = message.changed_file.key
        self._refresh_file_selection()
        self.post_message(self.FileSelected(DiffSelection(message.changed_file.key, 0)))

    def _resize_by(self, delta: int) -> None:
        if not self.is_docked or not self.has_focus_within:
            return
        self.panel_width = min(
            MAX_INSPECTOR_WIDTH,
            max(MIN_INSPECTOR_WIDTH, self.panel_width + delta),
        )
        self._sync_geometry()

    def _sync_geometry(self) -> None:
        visible = (self.is_docked or self.is_overlay) and not self._diff_open
        self.set_class(not visible, "-inspector-hidden")
        self.set_class(self.is_overlay, "-inspector-overlay")
        self.styles.overlay = "screen" if self.is_overlay else "none"
        self.styles.dock = "right" if self.is_docked else "none"
        self.styles.offset = (
            (self._terminal_width - self.effective_width, 0)
            if self.is_overlay
            else (0, 0)
        )
        self.styles.width = self.effective_width
        if self.is_overlay:
            self.border_title = literal_text("Inspector [overlay]")
        else:
            self.border_title = literal_text(f"Inspector [docked {self.panel_width}]")

    def _remember_focus(self) -> None:
        focused = self.app.focused
        if focused is None or self in focused.ancestors_with_self:
            return
        self._previous_focus = focused

    def _close_overlay(self, *, restore_focus: bool) -> None:
        self.overlay_open = False
        self._sync_geometry()
        previous = self._previous_focus
        self._previous_focus = None
        if restore_focus and previous is not None and previous.is_mounted:
            previous.focus()

    def _focusable_rows(self) -> list[Widget]:
        return [*self._task_rows, *self._file_rows]

    def _focus_first_row(self) -> None:
        rows = self._focusable_rows()
        (rows[0] if rows else self).focus()

    def _move_focus(self, delta: int) -> None:
        rows = self._focusable_rows()
        if not rows:
            return
        focused = self.app.focused
        try:
            index = rows.index(focused)  # type: ignore[arg-type]
        except ValueError:
            index = -1 if delta > 0 else 0
        rows[(index + delta) % len(rows)].focus()

    def _refresh_file_selection(self) -> None:
        for row in self._file_rows:
            row.set_class(
                row.changed_file.key == self._selected_file_key,
                "inspector--file-selected",
            )


def _empty_row() -> Static:
    return Static(
        literal_text(f"  {EMPTY_SECTION}"),
        markup=False,
        classes="inspector--empty",
    )


def _context_lines(view: InspectorView) -> tuple[str, ...]:
    context = view.context
    rows: list[tuple[str, str]] = []
    if context.session_id:
        rows.append(("session", context.session_id))
    if context.profile:
        rows.append(("profile", context.profile))
    if context.endpoint:
        rows.append(("endpoint", context.endpoint))
    if context.model:
        rows.append(("model", context.model))
    if context.input_tokens is not None or context.output_tokens is not None:
        rows.append(
            (
                "usage",
                f"{context.input_tokens or 0} input · {context.output_tokens or 0} output",
            )
        )
    if not rows:
        return (f"  {EMPTY_SECTION}",)
    return tuple(f"  {label:<8} {value}" for label, value in rows)


def _operation_lines(view: InspectorView) -> tuple[str, ...]:
    operation = view.selected_operation
    if operation is None:
        return (f"  {EMPTY_SECTION}",)
    first = f"  {operation.name}"
    if operation.context:
        first = f"{first} · {operation.context}"
    rows = [first, f"  {operation.status}"]
    rows.extend(f"  {detail}" for detail in operation.details)
    return tuple(rows)
