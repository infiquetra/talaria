"""Sub-agent rows and the count that survives collapsing (KTD8, R14, R16).

The row is exactly the projection's five fields — ``id``, ``name``, ``status``,
``elapsed``, ``detail`` — and this module adds no sixth. In particular it does
not decide what a status *means*: the terminal-state precedence that stops a
late ``subagent.start`` from clobbering a completed row lives in the domain
reducer, where a test can prove it without a screen.

R16's requirement is easy to under-implement. "Collapsed" must not mean "gone":
when the rows are folded away the count stays on screen, because a fan-out the
operator cannot see is the situation the count exists to prevent.
"""

from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.widgets import Static

from talaria.domain.projection import SubagentView
from talaria.ui.literal import literal_text

#: Status column width, so rows line up without a table widget. The longest
#: member of the frozen seven-value enum is ``interrupted`` (11).
_STATUS_WIDTH = 11


def format_row(row_id: str, name: str, status: str, elapsed: float, detail: str | None) -> str:
    """One row's plain text. Pure, so a test asserts on it without a screen."""
    head = f"{status:<{_STATUS_WIDTH}} {elapsed:6.1f}s  {name}"
    if detail:
        return f"{head} — {detail}"
    return head


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
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self.collapsed = False
        self._header: Static | None = None
        self._rows: list[Static] = []
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

    async def apply(self, view: SubagentView) -> None:
        """Render the rows, or the count alone when collapsed."""
        self._view = view
        populated = bool(view.rows)
        self.set_class(populated, "-populated")

        if self._header is not None:
            label = view.collapsed_label if populated else "no sub-agents"
            suffix = "  (collapsed — f2 to expand)" if self.collapsed and populated else ""
            self._header.update(literal_text(f"sub-agents: {label}{suffix}"))

        wanted = [] if self.collapsed else list(view.rows)
        while len(self._rows) > len(wanted):
            await self._rows.pop().remove()
        for index, row in enumerate(wanted):
            text = literal_text(
                format_row(row.id, row.name, row.status, row.elapsed, row.detail)
            )
            if index < len(self._rows):
                self._rows[index].update(text)
            else:
                widget = Static(text, markup=False)
                self._rows.append(widget)
                await self.mount(widget)

    async def toggle_collapsed(self) -> bool:
        """Fold or unfold the rows. The count stays either way (R16)."""
        self.collapsed = not self.collapsed
        if self._view is not None:
            await self.apply(self._view)
        return self.collapsed
