"""Sub-agent rows and the count that survives collapsing (KTD8, R14, R16).

The row is exactly the projection's five fields — ``id``, ``name``, ``status``,
``elapsed``, ``detail`` — and this module adds no sixth. In particular it does
not decide what a status *means*: the terminal-state precedence that stops a
late ``subagent.start`` from clobbering a completed row lives in the domain
reducer, where a test can prove it without a screen.

R16's requirement is easy to under-implement. "Collapsed" must not mean "gone":
when the rows are folded away the count stays on screen, because a fan-out the
operator cannot see is the situation the count exists to prevent.

U9 adds the one control a row carries: interrupting that child
(``subagent.interrupt``, R15). It is attached to the row and not to a key
binding because the call names a ``subagent_id``, and a global key would have to
guess which of six children the operator meant — see :class:`AgentRow`.
"""

from __future__ import annotations

from typing import ClassVar

from textual import events
from textual.app import ComposeResult
from textual.binding import Binding, BindingType
from textual.containers import Vertical
from textual.message import Message
from textual.widgets import Static

from talaria.domain.models import TERMINAL_SUBAGENT_STATUSES
from talaria.domain.projection import SubagentView
from talaria.ui.focus import CaretReleased, holds_caret
from talaria.ui.literal import literal_text

#: Each lifecycle state keeps a fixed ASCII form when colour is unavailable.
_STATUS_FORMS = {
    "queued": "[..] queued",
    "running": "[>] running",
    "completed": "[ok] completed",
    "error": "[!] error",
    "failed": "[x] failed",
    "interrupted": "[-] interrupted",
    "timeout": "[t] timeout",
}
_STATUS_WIDTH = max(len(form) for form in _STATUS_FORMS.values())


def format_row(row_id: str, name: str, status: str, elapsed: float, detail: str | None) -> str:
    """One row's plain text. Pure, so a test asserts on it without a screen."""
    state = _STATUS_FORMS.get(status, f"[?] {status or 'unknown'}")
    head = f"  {state:<{_STATUS_WIDTH}} {elapsed:6.1f}s  {name}"
    if detail:
        return f"{head} — {detail}"
    return head


class AgentRow(Static):
    """One sub-agent line, and the only control that can stop that child (R15).

    **The action lives on the row rather than on a key binding, and that is a
    correctness property rather than a matter of taste.** ``subagent.interrupt``
    takes a ``subagent_id`` (``tui_gateway/methods_session.py:2806-2814``), so an
    interrupt has to name *which* child — and a global key would have to invent
    a rule for which one it meant. The rule would be wrong exactly when it
    mattered: a fan-out of six with one runaway is the situation the control
    exists for, and "the most recent" or "the first running" is a guess about
    which of the six the operator was looking at.

    A terminal row carries no action at all. Interrupting something that has
    already completed sends a call the gateway answers ``found: false`` to, and
    an affordance that is always refused teaches the operator to ignore it.
    """

    class Interrupt(Message):
        """Stop this one child. Carries the id, never the row's position."""

        def __init__(self, subagent_id: str, name: str) -> None:
            super().__init__()
            self.subagent_id = subagent_id
            self.name = name

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("enter", "interrupt", "interrupt this sub-agent"),
    ]

    def __init__(self, *args: object, **kwargs: object) -> None:
        super().__init__(*args, **kwargs)  # type: ignore[arg-type]
        self.subagent_id = ""
        self.subagent_name = ""
        self.interruptible = False

    def _show_focus_prefix(self, focused: bool) -> None:
        text = str(self.content)
        if len(text) >= 2 and text[1] == " ":
            self.update(literal_text((">" if focused else " ") + text[1:]))

    def on_focus(self, _event: events.Focus) -> None:
        self._show_focus_prefix(True)

    def on_blur(self, _event: events.Blur) -> None:
        self._show_focus_prefix(False)

    def bind_row(self, *, subagent_id: str, name: str, status: str) -> None:
        """Point this widget at a row. Reused across renders, so it is re-pointed
        rather than re-created — a row widget that kept a stale id would
        interrupt whichever child used to occupy that screen line."""
        self.subagent_id = subagent_id
        self.subagent_name = name
        self.interruptible = status not in TERMINAL_SUBAGENT_STATUSES
        # A child that finishes takes its row's control away without taking the
        # row away, and that is a case Textual does not cover. It moves the
        # caret off a widget that was *removed*; a widget that merely stopped
        # being focusable keeps it. The row would then sit outside the focus
        # chain still holding the caret, eating every key the operator typed —
        # so the caret is handed back here, before the row stops accepting it.
        if not self.interruptible and self.has_focus:
            self.post_message(CaretReleased())
        self.can_focus = self.interruptible
        self.set_class(self.interruptible, "-interruptible")

    def action_interrupt(self) -> None:
        if self.interruptible and self.subagent_id:
            self.post_message(self.Interrupt(self.subagent_id, self.subagent_name))

    def on_click(self, event: events.Click) -> None:
        if not self.interruptible or not self.subagent_id:
            return
        event.stop()
        self.post_message(self.Interrupt(self.subagent_id, self.subagent_name))


class AgentRows(Vertical):
    """The sub-agent region: a header line plus one line per row."""

    DEFAULT_CSS = """
    AgentRows {
        height: auto;
        max-height: 10;
        display: none;
    }
    AgentRows.-populated {
        display: block;
    }
    AgentRows > .agents--header {
        color: $text-muted;
    }
    AgentRow.-interruptible:focus {
        background: $accent 20%;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self.collapsed = False
        self._header: Static | None = None
        self._rows: list[AgentRow] = []
        self._view: SubagentView | None = None

    def compose(self) -> ComposeResult:
        self._header = Static(literal_text(""), markup=False, classes="agents--header")
        yield self._header

    @property
    def header_text(self) -> str:
        return "" if self._header is None else str(self._header.content)

    @property
    def row_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._rows)

    @property
    def is_populated(self) -> bool:
        """Whether the region has any sub-agent rows to show or hide (B3).

        The ``-populated`` class is decided from the same predicate in
        :meth:`apply`; this is the public form of that check, so an action
        above can tell a toggle that will be seen from one that changes
        nothing on screen.
        """
        return bool(self._view.rows) if self._view is not None else False

    def row_for(self, subagent_id: str) -> AgentRow | None:
        """The widget currently showing one child, or ``None`` when collapsed."""
        for row in self._rows:
            if row.subagent_id == subagent_id:
                return row
        return None

    async def apply(self, view: SubagentView) -> None:
        """Render the rows, or the count alone when collapsed."""
        self._view = view
        populated = bool(view.rows)
        self.set_class(populated, "-populated")

        if self._header is not None:
            label = view.collapsed_label if populated else "no sub-agents"
            suffix = "  (collapsed — ctrl+g/F2 to expand)" if self.collapsed and populated else ""
            self._header.update(literal_text(f"sub-agents: {label}{suffix}"))

        wanted = [] if self.collapsed else list(view.rows)
        released = False
        while len(self._rows) > len(wanted):
            doomed = self._rows.pop()
            released = released or holds_caret(doomed)
            await doomed.remove()
        for index, row in enumerate(wanted):
            text = literal_text(
                format_row(row.id, row.name, row.status, row.elapsed, row.detail)
            )
            if index < len(self._rows):
                widget = self._rows[index]
                widget.update(text)
                widget._show_focus_prefix(widget.has_focus)
            else:
                widget = AgentRow(text, markup=False)
                self._rows.append(widget)
                await self.mount(widget)
            widget.bind_row(subagent_id=row.id, name=row.name, status=row.status)

        # Collapsing with f2 removes every row at once, so the operator can lose
        # the caret to a keystroke of their own — which is exactly when they are
        # least likely to suspect the interface of having stopped listening.
        if released:
            self.post_message(CaretReleased())

    async def toggle_collapsed(self) -> bool:
        """Fold or unfold the rows. The count stays either way (R16)."""
        self.collapsed = not self.collapsed
        if self._view is not None:
            await self.apply(self._view)
        return self.collapsed
