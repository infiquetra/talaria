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
    StatusRegion > .status--caret {
        /* U3/KTD5: a dedicated, fixed-height, one-row slot for the caret
           location word, mounted unconditionally so its presence never
           changes StatusRegion's height (R5). Never reuse
           ``.status--marker`` here — that Static is overwritten by every
           status tick (``apply`` below) and by command failures, so a
           shared slot would either lose the caret word on the next tick
           or suppress a failure the operator needs to see. Non-wrapping
           for the same reason the marker's neighbour rows are: a folded
           second row that ``height: 1`` then hides is a silent
           truncation, not a caret name. */
        height: 1;
        text-wrap: nowrap;
        text-overflow: ellipsis;
        color: $text-muted;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._rows: list[Static] = []
        self._marker: Static | None = None
        self._caret: Static | None = None

    def compose(self) -> ComposeResult:
        self._caret = Static(literal_text(""), markup=False, classes="status--caret")
        yield self._caret
        self._marker = Static(literal_text(""), markup=False, classes="status--marker")
        yield self._marker

    @property
    def row_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._rows)

    @property
    def marker_text(self) -> str:
        return "" if self._marker is None else str(self._marker.content)

    @property
    def caret_text(self) -> str:
        """The current caret-location word (R5/KTD5), empty when the
        composer holds the caret."""
        return "" if self._caret is None else str(self._caret.content)

    def set_caret(self, location: str) -> None:
        """Write where the caret is, or clear the slot when it is the composer.

        ``location`` is empty (never ``"caret: composer"``) when the
        composer holds the caret — R5 requires this slot to *name where
        else* the caret went, and the composer is the caret's home rather
        than a destination worth announcing. Writing here never mounts or
        unmounts anything: the slot exists unconditionally from
        :meth:`compose`, so this can only ever change its text, never the
        region's height.
        """
        if self._caret is None:  # pragma: no cover - compose always runs first
            return
        text = f"caret: {location}" if location else ""
        self._caret.update(literal_text(text))

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
