"""The command listing — a minimal entry affordance for U9 (R23, AE9).

Deliberately not a fuzzy-search overlay. What R23 and AE9 actually require is
that the operator can *see* what the gateway offers and, for the entries that
cannot work, see that too. A modal search box would satisfy neither better and
would put a second focus owner in front of the composer, which is the one
widget the interface is built around.

So this is a foldable region that stays out of the way until it is asked for: a
header naming the counts by availability, then one line per command carrying
its marker. Three markers, from :data:`~talaria.domain.commands.AVAILABILITY_MARKER`
— blank for a gateway command that dispatches, ``local`` for Talaria's own four
(PC6 asks they be marked local in any listing), and ``unsupported`` for the
entries Hermes's own React client implements and the gateway does not.

**Honest degradation is the whole reason the header is not just a count.** A
catalogue Talaria could not fetch, and a catalogue the gateway itself
annotated with a ``warning`` (its skill scan failing, say — the gateway builds
that field at ``methods_tools.py:346``), both leave the operator with a listing
that is missing commands. Rendering either as a bare, shorter list would be a
silent lie, so both appear as their own line above the rows.
"""

from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.widgets import Static

from talaria.domain.commands import CommandCatalog, CommandEntry
from talaria.ui.literal import literal_text

#: Width of the name column, so descriptions line up without a table widget.
#: Long enough for the longest name the pin's registry carries and short enough
#: to leave a usable description column at 80 cells.
_NAME_WIDTH = 18

#: Width of the marker column. ``unsupported`` is the longest of the three.
_MARKER_WIDTH = 11

#: What the header says when nothing has been fetched yet — distinct from "the
#: gateway has no commands", which is a claim, and from a fetch that failed,
#: which is a different one again.
NOT_YET_FETCHED = "commands: not fetched yet"

#: Prefix for the line naming a catalogue Talaria could not read.
CATALOG_FAILURE_PREFIX = "catalogue unavailable — "

#: Prefix for the gateway's own warning about a catalogue it built incompletely.
CATALOG_WARNING_PREFIX = "the gateway reported: "


def format_entry(entry: CommandEntry) -> str:
    """One listing row. Pure, so a test asserts on it without a screen."""
    marker = f"{entry.marker:<{_MARKER_WIDTH}}"
    return f"{entry.name:<{_NAME_WIDTH}} {marker} {entry.description}".rstrip()


def header_line(catalog: CommandCatalog | None) -> str:
    """The count that stays on screen when the rows are folded away.

    Counted by availability rather than totalled, because "94 commands" over a
    listing where four of them cannot run is the number that misleads.
    """
    if catalog is None:
        return NOT_YET_FETCHED
    return (
        f"commands: {len(catalog.gateway_entries)} from the gateway · "
        f"{len(catalog.local_entries)} local · "
        f"{len(catalog.unsupported_entries)} unsupported"
    )


class PaletteRegion(Vertical):
    """The foldable command listing.

    Named ``PaletteRegion`` rather than ``CommandPalette`` on purpose: Textual
    ships a ``CommandPalette`` of its own, and two classes with one name in one
    interface is how a review reads the wrong file.
    """

    DEFAULT_CSS = """
    PaletteRegion {
        height: auto;
        max-height: 14;
        display: none;
    }
    PaletteRegion.-showing {
        display: block;
    }
    PaletteRegion > .palette--header {
        color: $text-muted;
    }
    PaletteRegion > .palette--degraded {
        color: $warning;
        display: none;
    }
    PaletteRegion > .palette--degraded.-said {
        display: block;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self.showing = False
        self._header: Static | None = None
        self._degraded: Static | None = None
        self._rows: list[Static] = []
        self._catalog: CommandCatalog | None = None

    def compose(self) -> ComposeResult:
        self._header = Static(
            literal_text(NOT_YET_FETCHED), markup=False, classes="palette--header"
        )
        yield self._header
        self._degraded = Static(literal_text(""), markup=False, classes="palette--degraded")
        yield self._degraded

    # ── read access, so tests never reach through to the framework ───────

    @property
    def header_text(self) -> str:
        return "" if self._header is None else str(self._header.content)

    @property
    def degraded_text(self) -> str:
        return "" if self._degraded is None else str(self._degraded.content)

    @property
    def row_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._rows)

    @property
    def catalog(self) -> CommandCatalog | None:
        return self._catalog

    # ── rendering ────────────────────────────────────────────────────────

    async def apply(self, catalog: CommandCatalog | None) -> None:
        """Render the listing. Safe to call before anything has been fetched."""
        self._catalog = catalog
        self.set_class(self.showing, "-showing")

        if self._header is not None:
            self._header.update(literal_text(header_line(catalog)))
        if self._degraded is not None:
            said = self._degraded_line(catalog)
            self._degraded.update(literal_text(said))
            # Hidden rather than blank when there is nothing to say. An empty
            # warning-coloured row above the listing reads as a message whose
            # text failed to render, which is a worse thing to show than
            # nothing at all.
            self._degraded.set_class(bool(said), "-said")

        wanted = list(catalog.entries) if (self.showing and catalog is not None) else []
        while len(self._rows) > len(wanted):
            await self._rows.pop().remove()
        for index, entry in enumerate(wanted):
            text = literal_text(format_entry(entry))
            if index < len(self._rows):
                self._rows[index].update(text)
            else:
                widget = Static(text, markup=False)
                self._rows.append(widget)
                await self.mount(widget)

    @staticmethod
    def _degraded_line(catalog: CommandCatalog | None) -> str:
        if catalog is None:
            return ""
        if not catalog.available:
            return f"{CATALOG_FAILURE_PREFIX}{catalog.failure}"
        if catalog.warning:
            return f"{CATALOG_WARNING_PREFIX}{catalog.warning}"
        return ""

    async def toggle(self) -> bool:
        """Show or hide the whole region.

        Unlike the sub-agent rows, nothing of this survives being folded away.
        That is the difference between the two: a sub-agent count the operator
        cannot see is a fan-out running unwatched, while a command listing
        nobody asked for is only a listing, and the transcript is worth more
        rows than it is.
        """
        self.showing = not self.showing
        await self.apply(self._catalog)
        return self.showing
