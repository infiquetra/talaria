"""The command listing — a minimal entry affordance for U9 (R23, AE9).

Deliberately not a fuzzy-search overlay. What R23 and AE9 actually require is
that the operator can *see* what the gateway offers and, for the entries that
cannot work, see that too. A modal search box would satisfy neither better and
would put a second focus owner in front of the composer, which is the one
widget the interface is built around.

So this is a foldable region that stays out of the way until it is asked for: a
header naming the counts by availability, then one line per command carrying
its marker — and, in the slash-command menu, one unselectable label row per
section (D5, #146), so the flat scrolling list reads divided the way Jeff
selected it. Three markers, from :data:`~talaria.domain.commands.AVAILABILITY_MARKER`
— blank for a gateway command that dispatches, ``local`` for each of Talaria's
own controls in :data:`~talaria.domain.commands.TALARIA_LOCAL_COMMANDS`
(PC6 asks they be marked local in any listing), and ``unsupported`` for the
entries Hermes's own React client implements and the gateway does not.

**Honest degradation is the whole reason the header is not just a count.** A
catalogue Talaria could not fetch, and a catalogue the gateway itself
annotated with a ``warning`` (its skill scan failing, say — the gateway builds
that field at ``methods_tools.py:346``), both leave the operator with a listing
that is missing commands. Rendering either as a bare, shorter list would be a
silent lie, so both appear as their own line above the rows.

C2 adds a filtered mode: when the composer holds a slash prefix (``/`` or
``/name`` with no trailing argument) the same region shows a live-filtered
view of the runnable catalogue (local plus gateway dispatchable, unsupported
omitted, prefix case-insensitive, sorted by category then name). The browse
listing (``F3``) remains unchanged and reuses the same header and degraded
handling. The two modes share one widget so they cannot disagree about counts
or ordering.
"""

from __future__ import annotations

import re
import textwrap

from textual import events
from textual.app import ComposeResult
from textual.containers import Vertical
from textual.message import Message
from textual.widgets import Static

from talaria.domain.commands import (
    CommandCatalog,
    CommandEntry,
    CommandSection,
    filter_commands,
)
from talaria.themes import ThemeSpec
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

#: Shown when the filter has excluded every row. Lowercase, contains the
#: phrase the plan requires tests to assert on.
NO_MATCHING = "no matching commands"

#: Theme mode reuses the palette region but gives it its own honest header.
THEME_HEADER = "themes: Up/Down preview · Enter select and save · Escape cancel"

#: Regex for the slash-name class that KTD2 defines: slash, then a letter,
#: then letters/digits/underscore/hyphen, end-anchored, no trailing space.
_SLASH_RE = re.compile(r"^/[A-Za-z][A-Za-z0-9_-]*$")


def is_slash_prefix_text(text: str) -> bool:
    """Whether ``text`` satisfies KTD2's open predicate (text half only).

    ``text.lstrip()`` must be ``/`` or match ``^/[A-Za-z][A-Za-z0-9_-]*$``.
    Leading whitespace is tolerated, trailing whitespace closes. No caret
    check — the caller decides whether the composer owns the caret.
    """
    stripped = text.lstrip()
    if stripped == "/":
        return True
    return bool(_SLASH_RE.match(stripped))


def slash_prefix_from_text(text: str) -> str | None:
    """The filter prefix for ``text``, or ``None`` when the predicate is false.

    Returns ``""`` for bare ``/``, otherwise the lowercased name after the
    slash. The caller must have checked :func:`is_slash_prefix_text` or be
    prepared for ``None``.
    """
    if not is_slash_prefix_text(text):
        return None
    stripped = text.lstrip()
    if stripped == "/":
        return ""
    return stripped[1:].lower()


def _local_entries_tuple() -> tuple[CommandEntry, ...]:
    """The Talaria-local entries, built without importing a private helper."""
    from talaria.domain.commands import TALARIA_LOCAL_COMMANDS

    return tuple(
        CommandEntry(
            name=cmd.name,
            description=(
                f"{cmd.description} {cmd.argument_hint}".strip()
                if cmd.argument_hint
                else cmd.description
            ),
            category="Talaria",
            availability="talaria-local",
            origin="",
        )
        for cmd in TALARIA_LOCAL_COMMANDS
    )


def _runnable_entries(catalog: CommandCatalog | None) -> tuple[CommandEntry, ...]:
    """Runnable entries: local plus gateway dispatchable, unsupported omitted."""
    if catalog is None:
        return _local_entries_tuple()
    return tuple(e for e in catalog.entries if e.availability in ("dispatch", "talaria-local"))


def _filtered_entries(
    catalog: CommandCatalog | None, prefix: str
) -> tuple[CommandEntry, ...]:
    """Filtered runnable entries across name and description via domain filter."""
    return filter_commands(catalog, prefix)


def format_entry(entry: CommandEntry) -> str:
    """One listing row. Pure, so a test asserts on it without a screen."""
    marker = f"{entry.marker:<{_MARKER_WIDTH}}"
    return f"{entry.name:<{_NAME_WIDTH}} {marker} {entry.description}".rstrip()


def format_filtered_entry(
    entry: CommandEntry, *, active: bool = False, max_width: int = 80
) -> str:
    """One filtered row: name, optional truthful badge, and wrapped description.

    When active (highlighted), the full description is expanded.
    When inactive, the description wraps up to 2 lines, clipping with '…'
    if it exceeds 2 lines. Continuation lines are indented 19 spaces
    to align with the description column.
    """
    badge_prefix = f"[{entry.badge}] " if (entry.is_skill and entry.badge) else ""
    full_desc = f"{badge_prefix}{entry.description}".strip()
    desc_width = max(20, max_width - 19)

    if not full_desc:
        wrapped = [""]
    else:
        wrapped = textwrap.wrap(full_desc, width=desc_width)
        if not wrapped:
            wrapped = [""]

    if active:
        lines = wrapped
    else:
        if len(wrapped) <= 2:
            lines = wrapped
        else:
            line2 = wrapped[1].rstrip()
            if len(line2) + 1 > desc_width:
                line2 = line2[: desc_width - 1] + "…"
            else:
                line2 = line2 + "…"
            lines = [wrapped[0], line2]

    first_line = f"{entry.name:<{_NAME_WIDTH}} {lines[0]}".rstrip()
    if len(lines) == 1:
        return first_line
    continuation = [f"{' ' * 19}{line}".rstrip() for line in lines[1:]]
    return "\n".join([first_line] + continuation)


def format_section_heading(section: CommandSection) -> str:
    """One section label row for the slash-command menu (D5, #146).

    The label is the section's own display name — a gateway category name,
    ``Talaria``, ``Skills``, or ``Uncategorised`` — decided by
    :meth:`~talaria.domain.commands.CommandCatalog.section_for`, never here.
    No count: a count would have to say whether it covers the section or the
    filtered view, and the ruling allows counts without requiring them.
    """
    return f"── {section.label} ──"


def _entry_text_width(region_width: int) -> int:
    """Width one filtered row may wrap to, from the region's own width.

    The 80-cell default stands on narrow screens and before first layout,
    so wrapping never narrows below what the existing tests pin; on a wider
    screen the description column grows into the space instead of leaving it
    unused (review observation on #146: at 100 columns the old fixed wrap
    left 20 cells empty while clipping long descriptions early).
    """
    return max(80, region_width or 80)


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
        overflow-y: auto;
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
    PaletteRegion > .palette--row {
        color: $text;
    }
    PaletteRegion > .palette--row.-active {
        background: $accent;
        color: $text;
        text-style: bold;
    }
    PaletteRegion > .palette--row.-muted {
        color: $text-muted;
        text-style: italic;
    }
    PaletteRegion > .palette--row.-section {
        color: $accent;
        text-style: bold;
    }
    """

    class ThemeSelected(Message):
        """The highlighted theme was accepted for the current session."""

        def __init__(self, slug: str) -> None:
            super().__init__()
            self.slug = slug

    class ThemeCancelled(Message):
        """Theme browsing was cancelled and the open-time state was restored."""

        def __init__(self, slug: str, session_slug: str | None) -> None:
            super().__init__()
            self.slug = slug
            self.session_slug = session_slug

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._browse_showing = False
        self._slash_prefix: str | None = None
        self._filtered: tuple[CommandEntry, ...] = ()
        self._selected: int | None = None
        self._header: Static | None = None
        self._degraded: Static | None = None
        #: Wrap width the mounted slash rows were rendered at, so a resize
        #: that changes nothing re-renders nothing (and keeps the scroll
        #: position a rebuild would reset).
        self._wrap_width: int | None = None
        self._rows: list[Static] = []
        #: Entry index into :attr:`_filtered` per mounted row widget, in mount
        #: order — ``None`` for heading, theme, and browse rows, which address
        #: no slash-menu entry. Selection, dispatch, and click all resolve
        #: through this map rather than through widget position, so section
        #: headings (D5, #146) can sit between command rows without shifting
        #: what a highlight, a click, or Enter means. Kept in lock-step with
        #: :attr:`_rows` at every site that mounts or clears rows.
        self._row_entries: list[int | None] = []
        self._catalog: CommandCatalog | None = None
        self._theme_specs: tuple[ThemeSpec, ...] = ()
        self._theme_selected: int | None = None
        self._theme_restore_slug = ""
        self._theme_restore_session_slug: str | None = None
        self._theme_restore_browse = False
        # Browse and slash-filtered modes leave focus in the composer. Theme
        # mode enables focus only for its lifetime so its keys stay local.
        self.can_focus = False

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

    @property
    def showing(self) -> bool:
        """Whether the region is currently visible (browse or slash)."""
        return (
            self._browse_showing
            or self._slash_prefix is not None
            or self.is_theme_active
        )

    @showing.setter
    def showing(self, value: bool) -> None:
        self._browse_showing = bool(value)

    @property
    def is_slash_active(self) -> bool:
        return self._slash_prefix is not None

    @property
    def is_theme_active(self) -> bool:
        return bool(self._theme_specs)

    @property
    def theme_specs(self) -> tuple[ThemeSpec, ...]:
        return self._theme_specs

    @property
    def selected_theme(self) -> ThemeSpec | None:
        selected = self._theme_selected
        if selected is None or not (0 <= selected < len(self._theme_specs)):
            return None
        return self._theme_specs[selected]

    @property
    def slash_prefix(self) -> str | None:
        return self._slash_prefix

    @property
    def filtered_entries(self) -> tuple[CommandEntry, ...]:
        return self._filtered

    @property
    def selected_index(self) -> int | None:
        return self._selected

    @property
    def selected_entry(self) -> CommandEntry | None:
        if self._slash_prefix is None or not self._filtered or self._selected is None:
            return None
        if 0 <= self._selected < len(self._filtered):
            return self._filtered[self._selected]
        return None

    def consume_selected(self) -> CommandEntry | None:
        """Atomically return the selected entry and clear the selection.

        Guarantees that a selected command entry can only be consumed once,
        preventing duplicate dispatches from rapid key or click events.
        """
        entry = self.selected_entry
        self._selected = None
        return entry

    # ── rendering ────────────────────────────────────────────────────────

    async def apply(self, catalog: CommandCatalog | None) -> None:
        """Render the listing. Safe to call before anything has been fetched."""
        self._catalog = catalog
        if self.is_theme_active:
            self.set_class(True, "-showing")
            if self._header is not None:
                self._header.update(literal_text(THEME_HEADER))
            if self._degraded is not None:
                self._degraded.update(literal_text(""))
                self._degraded.set_class(False, "-said")
            await self._remove_rows()
            for index, spec in enumerate(self._theme_specs):
                active = index == self._theme_selected
                classes = "palette--row"
                if active:
                    classes += " -active"
                widget = Static(
                    literal_text(self._format_theme_row(spec, active=active)),
                    markup=False,
                    classes=classes,
                )
                self._rows.append(widget)
                # Theme rows address no slash-menu entry; the map stays
                # lock-step so positional theme logic never drifts from it.
                self._row_entries.append(None)
                await self.mount(widget)
            return

        # Recompute filtered when slash is active and catalog may have changed.
        if self._slash_prefix is not None:
            self._filtered = _filtered_entries(catalog, self._slash_prefix)
            if self._filtered:
                if self._selected is None or self._selected >= len(self._filtered):
                    self._selected = 0
            else:
                self._selected = None
        else:
            self._filtered = ()
            self._selected = None

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

        # Rebuild rows from scratch — the list is at most ~100 and the
        # correctness of highlight and muted handling matters more than diffing.
        await self._remove_rows()

        if self._slash_prefix is not None:
            if self._filtered:
                # Section labels come from the same function the filter's
                # ordering uses (``catalog.section_for``), so the list and
                # its labels cannot disagree about where a row lives (D5,
                # #146). A heading is emitted when the section changes, so a
                # section with no surviving row draws no heading, and
                # Uncategorised appears only when non-empty — both fall out
                # of the iteration rather than needing their own rules.
                # Without a catalogue every filtered row is Talaria-local,
                # which ``section_for`` answers on an empty catalog without
                # consulting the wire.
                section_catalog = catalog if catalog is not None else CommandCatalog()
                width = _entry_text_width(self.size.width)
                self._wrap_width = width
                current_key: str | None = None
                for idx, entry in enumerate(self._filtered):
                    section = section_catalog.section_for(entry)
                    if section.key != current_key:
                        current_key = section.key
                        heading = Static(
                            literal_text(format_section_heading(section)),
                            markup=False,
                            classes="palette--row -section",
                        )
                        self._rows.append(heading)
                        self._row_entries.append(None)
                        await self.mount(heading)
                    active = idx == self._selected
                    classes = "palette--row"
                    if active:
                        classes += " -active"
                    text = literal_text(
                        format_filtered_entry(entry, active=active, max_width=width)
                    )
                    widget = Static(text, markup=False, classes=classes)
                    self._rows.append(widget)
                    self._row_entries.append(idx)
                    await self.mount(widget)
            else:
                widget = Static(
                    literal_text(NO_MATCHING), markup=False, classes="palette--row -muted"
                )
                self._rows.append(widget)
                await self.mount(widget)
            return

        # The F3 browse listing is deliberately unsectioned (architect ruling
        # on #146): headings render in the slash-command menu only.
        wanted = list(catalog.entries) if (self._browse_showing and catalog is not None) else []
        for entry in wanted:
            text = literal_text(format_entry(entry))
            widget = Static(text, markup=False, classes="palette--row")
            self._rows.append(widget)
            self._row_entries.append(None)
            await self.mount(widget)

    async def _remove_rows(self) -> None:
        for row in self._rows:
            await row.remove()
        self._rows = []
        self._row_entries = []

    @staticmethod
    def _format_theme_row(spec: ThemeSpec, *, active: bool) -> str:
        """One theme row with the visual contract's fixed focus gutter.

        A spec carrying a description (today only the Homebrew built-in
        per D4) states it on its own row; undescribed themes render exactly
        as before. The palette itself is unchanged.
        """
        row = f"{'>' if active else ' '} {spec.name}"
        if spec.description.strip():
            row += f" — {spec.description.strip()}"
        return row

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
        self._browse_showing = not self._browse_showing
        await self.apply(self._catalog)
        return self.showing

    # ── theme-picker mode ────────────────────────────────────────────────

    async def open_theme_picker(
        self,
        specs: tuple[ThemeSpec, ...],
        *,
        current_slug: str,
        session_slug: str | None,
    ) -> None:
        """Open four-row theme browsing and capture the exact restore point."""
        if not specs:
            raise ValueError("theme picker requires at least one theme")
        slugs = tuple(spec.slug for spec in specs)
        if current_slug not in slugs:
            raise ValueError(f"current theme {current_slug!r} is not in the picker")

        self._theme_restore_browse = self._browse_showing
        self._browse_showing = False
        self._slash_prefix = None
        self._filtered = ()
        self._selected = None
        self._theme_specs = specs
        self._theme_selected = slugs.index(current_slug)
        self._theme_restore_slug = current_slug
        self._theme_restore_session_slug = session_slug
        self.can_focus = True
        await self.apply(self._catalog)
        self.focus()

    def move_theme_selection(self, delta: int) -> None:
        """Move one row, previewing the resulting theme immediately."""
        if not self.is_theme_active or self._theme_selected is None:
            return
        current = self._theme_selected
        selected = min(max(current + delta, 0), len(self._theme_specs) - 1)
        if selected == current:
            return
        self._theme_selected = selected
        for index, row in enumerate(self._rows):
            active = index == selected
            row.set_class(active, "-active")
            row.update(
                literal_text(self._format_theme_row(self._theme_specs[index], active=active))
            )
        self._preview_selected_theme()
        try:
            self.scroll_to_widget(self._rows[selected], animate=False)
        except (AttributeError, ValueError):
            pass

    def _preview_selected_theme(self) -> None:
        selected = self.selected_theme
        if selected is not None:
            self.app.theme = selected.slug

    async def accept_theme_selection(self) -> None:
        """Keep the preview as the current session choice and close the mode."""
        selected = self.selected_theme
        if selected is None:
            return
        slug = selected.slug
        await self._close_theme_picker()
        self.post_message(self.ThemeSelected(slug))

    async def cancel_theme_selection(self) -> None:
        """Restore both the applied theme and the open-time session choice."""
        if not self.is_theme_active:
            return
        slug = self._theme_restore_slug
        session_slug = self._theme_restore_session_slug
        self.app.theme = slug
        await self._close_theme_picker()
        self.post_message(self.ThemeCancelled(slug, session_slug))

    async def _close_theme_picker(self) -> None:
        self._theme_specs = ()
        self._theme_selected = None
        self._theme_restore_slug = ""
        self._theme_restore_session_slug = None
        self._browse_showing = self._theme_restore_browse
        self._theme_restore_browse = False
        self.can_focus = False
        await self.apply(self._catalog)
        try:
            self.app.composer.text_area.focus()  # type: ignore[attr-defined]
        except AttributeError:
            pass

    async def on_key(self, event: events.Key) -> None:
        """Keep preview, acceptance, and cancellation inside theme mode."""
        if not self.is_theme_active:
            return
        if event.key == "up":
            self.move_theme_selection(-1)
        elif event.key == "down":
            self.move_theme_selection(1)
        elif event.key == "enter":
            await self.accept_theme_selection()
        elif event.key == "escape":
            await self.cancel_theme_selection()
        else:
            return
        event.stop()
        event.prevent_default()

    # ── slash-filtered mode ──────────────────────────────────────────────

    async def show_slash(self, catalog: CommandCatalog | None, prefix: str) -> None:
        """Open the filtered palette on ``prefix`` (``""`` for bare ``/``)."""
        self._slash_prefix = prefix
        self._filtered = _filtered_entries(catalog, prefix)
        self._selected = 0 if self._filtered else None
        await self.apply(catalog)

    async def hide_slash(self) -> None:
        """Close the filtered palette, returning to browse or hidden."""
        if self._slash_prefix is None:
            return
        self._slash_prefix = None
        self._filtered = ()
        self._selected = None
        await self.apply(self._catalog)

    async def sync_slash(self, catalog: CommandCatalog | None, text: str) -> None:
        """Open, update, or close the slash palette based on ``text``.

        The caller is the typed-input path (a key or paste that changed the
        composer's text). Programmatic writes must not call this — ruling 3.
        """
        if is_slash_prefix_text(text):
            prefix = slash_prefix_from_text(text)
            # ``prefix`` is None only when predicate is false, which we already
            # ruled out, but guard defensively.
            if prefix is None:
                await self.hide_slash()
                return
            if self._slash_prefix != prefix:
                self._slash_prefix = prefix
                self._filtered = _filtered_entries(catalog, prefix)
                self._selected = 0 if self._filtered else None
                await self.apply(catalog)
            else:
                # Prefix unchanged but catalog may have changed externally
                # (fetch landed while open). Re-apply to pick up new rows.
                await self.apply(catalog)
        else:
            if self._slash_prefix is not None:
                await self.hide_slash()

    def _widget_for_entry(self, entry_index: int) -> Static | None:
        """The mounted command row for one filtered entry, if it is mounted.

        Headings sit between command rows, so widget position is not entry
        position: this resolves through :attr:`_row_entries` instead. A
        heading has no entry and is never returned — the highlight cannot
        land on a label, and neither Down nor Up ever stops on one.
        """
        try:
            position = self._row_entries.index(entry_index)
        except ValueError:
            return None
        if 0 <= position < len(self._rows):
            return self._rows[position]
        return None

    def move_selection(self, delta: int) -> None:
        """Move the highlight inside the filtered palette, clamped.

        Moves in entry space, so section headings are skipped structurally
        rather than by inspection: there is no entry index that names a
        heading, and therefore no delta that can land on one.
        """
        if self._slash_prefix is None or not self._filtered:
            return
        current = self._selected if self._selected is not None else 0
        new = current + delta
        if new < 0:
            new = 0
        if new >= len(self._filtered):
            new = len(self._filtered) - 1
        if new == current:
            return
        old_idx = current
        self._selected = new
        width = _entry_text_width(self.size.width)
        self._wrap_width = width
        old_row = self._widget_for_entry(old_idx)
        if old_row is not None and 0 <= old_idx < len(self._filtered):
            old_row.set_class(False, "-active")
            old_row.update(
                literal_text(
                    format_filtered_entry(
                        self._filtered[old_idx], active=False, max_width=width
                    )
                )
            )
        new_row = self._widget_for_entry(new)
        if new_row is not None and 0 <= new < len(self._filtered):
            new_row.set_class(True, "-active")
            new_row.update(
                literal_text(
                    format_filtered_entry(
                        self._filtered[new], active=True, max_width=width
                    )
                )
            )
        # Scroll the selected row into view so arrow navigation does not
        # leave the highlight off-screen. The region is capped at 14 rows,
        # so with 20 matches the selected index 15 would otherwise be invisible.
        if new_row is not None:
            try:
                self.scroll_to_widget(new_row, animate=False)
            except (AttributeError, ValueError):
                pass

    async def on_resize(self, event: events.Resize) -> None:
        """Re-wrap slash rows after a width change (review observation #146).

        Rows pre-wrap descriptions at render time, but the open-time render
        runs before first layout, when the region still reports width 0 and
        the 80-cell fallback stands in — so without this a wide screen keeps
        the narrow wrap. Re-apply rebuilds rows at the real width; the
        selection survives (apply preserves a valid ``_selected``). Resizes
        that change nothing skip the rebuild, because rebuilding resets the
        scroll position the operator is reading at.
        """
        if self._slash_prefix is None or self.is_theme_active:
            return
        width = _entry_text_width(self.size.width)
        if width == self._wrap_width:
            return
        await self.apply(self._catalog)

    # ── click ────────────────────────────────────────────────────────────

    async def on_click(self, event: events.Click) -> None:
        """Click on a filtered row inserts it; Enter dispatches (#121)."""
        if self.is_theme_active:
            target = event.widget
            if isinstance(target, Static) and target in self._rows:
                selected = self._rows.index(target)
                delta = selected - (self._theme_selected or 0)
                self.move_theme_selection(delta)
                event.stop()
            return
        if self._slash_prefix is None or not self._filtered:
            return
        # Textual delivers Click with event.widget as the widget under the cursor.
        # For a row click that is the row's Static; for a header click it is the
        # header Static, not a row. Click.chain is an integer (click count), not
        # an ancestry chain.
        target = event.widget
        position: int | None = None
        if isinstance(target, Static) and target in self._rows:
            position = self._rows.index(target)
        else:
            # Check if target is inside a row (e.g., if rows had children)
            # No try/except here on purpose. ``ancestors`` is a list of widgets
            # and ``getattr`` supplies a default, so neither AttributeError nor
            # TypeError is reachable — and a TypeError guard would swallow the
            # exact failure this branch was repaired for (``row in event.chain``
            # where ``chain`` is the click count, an int). Verified: with the
            # guard present, restoring the original defect leaves
            # test_palette_header_click_does_not_crash green; without it, the
            # same mutation turns it red.
            for row in self._rows:
                if target is not None and row in getattr(target, "ancestors", []):
                    position = self._rows.index(row)
                    break
        if position is None:
            # Click was not on a row (e.g., header or empty area) — do not
            # insert and do not crash. Previously this used `row in chain`
            # where chain is an int, raising TypeError: argument of type 'int'
            # is not iterable.
            return
        # Widget position is not entry position: headings sit between command
        # rows, so resolve through the map. A heading (or any row addressing
        # no entry) ends here — clicking a label inserts nothing, and since
        # the selection never names a heading, Enter after such a click
        # cannot dispatch one either.
        if not (0 <= position < len(self._row_entries)):
            return
        idx = self._row_entries[position]
        if idx is None or not (0 <= idx < len(self._filtered)):
            return
        event.stop()
        self._selected = idx
        # Perform the insert via the app — keep focus in the composer.
        await self._insert_selected()

    async def _insert_selected(self) -> None:
        from textual.app import ScreenStackError
        from textual.dom import NoScreen

        entry = self.consume_selected()
        if entry is None:
            return
        try:
            app = self.app
        except (NoScreen, ScreenStackError, AttributeError):
            return
        try:
            catalog: CommandCatalog | None = getattr(app, "catalog", None)
            composer = getattr(app, "composer", None)
            if composer is None:
                return
            # Canonicalise via the catalogue's own map, then ensure single slash.
            if catalog is not None:
                canon = catalog.canonical(entry.name)
            else:
                canon = entry.name
            canon = canon.lstrip("/")
            text = f"/{canon} "
            composer.text = text
            try:
                # Place caret at end (single-line, row 0).
                composer.text_area.cursor_location = (0, len(text))
            except (ValueError, AttributeError, NoScreen, ScreenStackError):
                pass
            try:
                composer.text_area.focus()
            except (NoScreen, ScreenStackError, AttributeError):
                pass
            await self.hide_slash()
        except (NoScreen, ScreenStackError, AttributeError):
            return
