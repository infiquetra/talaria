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

from collections.abc import Sequence

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

    def __init__(self, *, initial_marker: str = "", **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._rows: list[Static] = []
        self._seam_rows: list[Static] = []
        self._marker: Static | None = None
        self._initial_marker = initial_marker

    def compose(self) -> ComposeResult:
        self._marker = Static(
            literal_text(self._initial_marker), markup=False, classes="status--marker"
        )
        yield self._marker

    @property
    def row_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._rows)

    @property
    def seam_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._seam_rows)

    @property
    def marker_text(self) -> str:
        return "" if self._marker is None else str(self._marker.content)

    def show_configuration_notice(self, message: str) -> None:
        """Keep a malformed optional status command visible without a runner."""
        self._initial_marker = message
        if self._marker is not None:
            self._marker.update(literal_text(message))

    async def apply_seams(self, lines: Sequence[str]) -> None:
        """Render one connection's seam board (v0.4 U5: R9, R10, R12, R24).

        Seam rows sit below the status command's own rows and above the marker,
        and they are a *separate* list rather than appended to ``rows`` for one
        reason that matters: a status command's next tick replaces its rows, and
        seam lines that shared that list would blink out and back on every tick
        of an unrelated command.

        Each line goes through :func:`~talaria.ui.literal.literal_text` like
        every other untrusted string in this package (R23). Seam lines are
        Talaria's own sentences with a bounded gateway-supplied diagnostic
        embedded, and the diagnostic has already been de-credentialled by
        :func:`~talaria.domain.compat.redact_probe_detail` — the defanging here
        is the second half, neutralizing markup and control sequences at the
        render boundary. Neither half is sufficient alone: the redactor does not
        defang and ``literal_text`` does not withhold.

        An empty ``lines`` removes every seam row. That is the honest rendering
        of "there is no board", and it is not the same thing as a board whose
        seams are never-observed — those have rows, and the rows say so.
        """
        while len(self._seam_rows) > len(lines):
            await self._seam_rows.pop().remove()
        for index, line in enumerate(lines):
            text = literal_text(line)
            if index < len(self._seam_rows):
                self._seam_rows[index].update(text)
            else:
                widget = Static(text, markup=False, classes="status--seam")
                self._seam_rows.append(widget)
                await self.mount(widget, before=self._marker)

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
                # Before the first seam row when there is one, so a status
                # command that grows a row does not interleave itself into the
                # seam board. With no seam rows this is the marker, exactly as
                # before U5.
                anchor = self._seam_rows[0] if self._seam_rows else self._marker
                await self.mount(widget, before=anchor)

        if self._marker is not None:
            self._marker.update(literal_text(result.marker or ""))
