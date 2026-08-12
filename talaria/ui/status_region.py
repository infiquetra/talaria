"""The status-line region: U6's rows, rendered literally (R22).

Everything reaching this widget was produced by a command the operator
configured, and the whole point of R22 is that Talaria shows what that command
printed rather than obeying it. Two bounds do that work:

* **Literal rendering.** Rows go through :func:`talaria.ui.literal.literal_text`,
  which defangs escape sequences and bypasses Rich's markup parser. A status
  command that prints ``\\x1b[2J`` gets ``␛[2J`` on screen, not a cleared one.
* **A visible row bound.** The runner already truncates to
  :data:`talaria.status.contract.ROW_LIMIT` rows and flags it; this widget
  renders the flag rather than dropping rows quietly, because silent truncation
  and a short status command look identical on screen.

Failure is a first-class rendering, not a blank region. Every non-``ok`` tick
carries a categorical marker and this widget shows it — a status region that
goes empty when the command breaks is the failure mode R21's taxonomy exists to
avoid.
"""

from __future__ import annotations

from textual import events
from textual.app import ComposeResult
from textual.containers import Vertical
from textual.widgets import Static

from talaria.status.contract import TRUNCATION_MARKER
from talaria.status.runner import StatusTickResult
from talaria.ui.literal import literal_text


class StatusRegion(Vertical):
    """Renders one :class:`StatusTickResult` at a time."""

    DEFAULT_CSS = """
    StatusRegion {
        height: auto;
        max-height: 10;
        color: $text-muted;
    }
    StatusRegion > .status--marker {
        color: $warning;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._rows: list[Static] = []
        self._marker: Static | None = None

    def compose(self) -> ComposeResult:
        self._marker = Static(literal_text(""), markup=False, classes="status--marker")
        yield self._marker

    @property
    def row_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._rows)

    @property
    def marker_text(self) -> str:
        return "" if self._marker is None else str(self._marker.content)

    def on_click(self, event: events.Click) -> None:
        """A4 KTD2: click on the status region toggles sub-agent rows.

        Primary is click on the status indicator plus the chord ``ctrl+g``;
        ``F2`` remains as an alias where the desktop delivers it. The click
        routes through the same latch as the keyboard (AE12) via the app's
        :meth:`action_toggle_agents`, so a latched discard notice is not
        overwritten.
        """
        event.stop()
        event.prevent_default()
        app = self.app
        try:
            coro = app.action_toggle_agents()  # type: ignore[attr-defined]
            if hasattr(coro, "__await__"):
                if hasattr(app, "_spawn_live"):
                    app._spawn_live(coro)
                else:
                    import asyncio
                    asyncio.create_task(coro)
        except Exception:  # nosec B110 - click handler must not raise
            pass

    async def apply(self, result: StatusTickResult) -> None:
        rows = list(result.rows)
        if result.truncated:
            rows.append(TRUNCATION_MARKER)

        while len(self._rows) > len(rows):
            await self._rows.pop().remove()
        for index, row in enumerate(rows):
            text = literal_text(row)
            if index < len(self._rows):
                self._rows[index].update(text)
            else:
                widget = Static(text, markup=False)
                self._rows.append(widget)
                await self.mount(widget, before=self._marker)

        if self._marker is not None:
            self._marker.update(literal_text(result.marker or ""))
