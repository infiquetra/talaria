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

Since #144 this region carries only the status command's rows and its failure
marker. The caret-location row moved to the inspector's context section, and
the seam board that used to render here moved to the inspector's diagnostics
section — the roster, approval-detail, and http-runner rows never duplicate
above the composer.
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
        max-height: 11;
        color: $text-muted;
    }
    StatusRegion > .status--marker {
        color: $warning;
    }
    """

    def __init__(self, *, initial_marker: str = "", **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._rows: list[Static] = []
        self._marker: Static | None = None
        self._initial_marker = initial_marker

    def compose(self) -> ComposeResult:
        self._marker = Static(
            literal_text(_status_failure_marker(self._initial_marker)),
            markup=False,
            classes="status--marker",
        )
        yield self._marker

    @property
    def row_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._rows)

    @property
    def marker_text(self) -> str:
        return "" if self._marker is None else str(self._marker.content)

    def show_configuration_notice(self, message: str) -> None:
        """Keep a malformed optional status command visible without a runner."""
        self._initial_marker = message
        if self._marker is not None:
            self._marker.update(literal_text(_status_failure_marker(message)))

    async def apply(self, result: StatusTickResult) -> None:
        rows = list(result.rows)
        if result.truncated:
            rows.append(f"[!] status truncated — {TRUNCATION_MARKER}")

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
            marker = result.marker or ""
            if result.is_failure:
                marker = _status_failure_marker(marker)
            self._marker.update(literal_text(marker))


def _status_failure_marker(message: str) -> str:
    """Give a status-command failure its required non-colour form once."""
    if not message or message.startswith("[x] status"):
        return message
    if message.startswith("status"):
        return f"[x] {message}"
    return message
