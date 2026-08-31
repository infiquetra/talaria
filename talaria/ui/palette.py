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

from textual import events
from textual.app import ComposeResult
from textual.containers import Vertical
from textual.message import Message
from textual.widgets import Static

from talaria.domain.commands import CommandCatalog, CommandEntry
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
THEME_HEADER = "themes: Up/Down preview · Enter use this session · Escape cancel"

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
    """The seven Talaria-local entries, built without importing a private helper."""
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
    """Prefix-filtered runnable entries, case-insensitive, Talaria locals first.

    The plan requires this surface to group the way the F3 browse listing does,
    "so the two surfaces do not disagree about where ``/models`` lives". Browse
    renders ``catalog.entries`` verbatim and ``build_catalog`` seeds that tuple
    with ``_local_entries()``, so browse shows the Talaria locals first. A plain
    ``(category, name)`` sort does not: ``Info`` and ``Session`` both sort before
    ``Talaria``, which put the locals last and moved ``/models`` depending on
    which surface the operator had opened. The explicit rank restores the
    grouping; alphabetical order within a category keeps the listing stable.
    """
    entries = _runnable_entries(catalog)
    if prefix == "":
        filtered = entries
    else:
        lower = prefix.lower()
        filtered = tuple(
            e for e in entries if e.name.lower().removeprefix("/").startswith(lower)
        )
    return tuple(
        sorted(
            filtered,
            key=lambda e: (
                0 if e.category.lower() == "talaria" else 1,
                e.category.lower(),
                e.name.lower(),
            ),
        )
    )


def format_entry(entry: CommandEntry) -> str:
    """One listing row. Pure, so a test asserts on it without a screen."""
    marker = f"{entry.marker:<{_MARKER_WIDTH}}"
    return f"{entry.name:<{_NAME_WIDTH}} {marker} {entry.description}".rstrip()


def format_filtered_entry(entry: CommandEntry) -> str:
    """One filtered row: name and description, no marker (every row is runnable)."""
    return f"{entry.name:<{_NAME_WIDTH}} {entry.description}".rstrip()


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
        self._rows: list[Static] = []
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
                for idx, entry in enumerate(self._filtered):
                    classes = "palette--row"
                    if idx == self._selected:
                        classes += " -active"
                    text = literal_text(format_filtered_entry(entry))
                    widget = Static(text, markup=False, classes=classes)
                    self._rows.append(widget)
                    await self.mount(widget)
            else:
                widget = Static(
                    literal_text(NO_MATCHING), markup=False, classes="palette--row -muted"
                )
                self._rows.append(widget)
                await self.mount(widget)
            return

        wanted = list(catalog.entries) if (self._browse_showing and catalog is not None) else []
        for entry in wanted:
            text = literal_text(format_entry(entry))
            widget = Static(text, markup=False, classes="palette--row")
            self._rows.append(widget)
            await self.mount(widget)

    async def _remove_rows(self) -> None:
        for row in self._rows:
            await row.remove()
        self._rows = []

    @staticmethod
    def _format_theme_row(spec: ThemeSpec, *, active: bool) -> str:
        """One theme row with the visual contract's fixed focus gutter."""
        return f"{'>' if active else ' '} {spec.name}"

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

    def move_selection(self, delta: int) -> None:
        """Move the highlight inside the filtered palette, clamped."""
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
        self._selected = new
        # Update row classes synchronously — no remount needed.
        for idx, row in enumerate(self._rows):
            row.set_class(idx == self._selected, "-active")
        # Scroll the selected row into view so arrow navigation does not
        # leave the highlight off-screen. The region is capped at 14 rows,
        # so with 20 matches the selected index 15 would otherwise be invisible.
        try:
            self.scroll_to_widget(self._rows[new], animate=False)
        except (AttributeError, ValueError):
            pass

    # ── click ────────────────────────────────────────────────────────────

    async def on_click(self, event: events.Click) -> None:
        """Click on a filtered row selects it (same insert rule as Enter)."""
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
        idx: int | None = None
        if isinstance(target, Static) and target in self._rows:
            idx = self._rows.index(target)
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
                    idx = self._rows.index(row)
                    break
        if idx is None:
            # Click was not on a row (e.g., header or empty area) — do not
            # insert and do not crash. Previously this used `row in chain`
            # where chain is an int, raising TypeError: argument of type 'int'
            # is not iterable.
            return
        if not (0 <= idx < len(self._filtered)):
            return
        event.stop()
        self._selected = idx
        # Perform the insert via the app — keep focus in the composer.
        await self._insert_selected()

    async def _insert_selected(self) -> None:
        from textual.app import ScreenStackError
        from textual.dom import NoScreen

        entry = self.selected_entry
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
