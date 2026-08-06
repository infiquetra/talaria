"""The model picker — U2's foldable region (KTD3, 2026-08-06 model-picker plan).

**Same shape as :class:`~talaria.ui.palette.PaletteRegion`, and for the same
reason.** ``talaria/ui/palette.py:1-22`` rejected a modal search box because it
puts a second focus owner in front of the composer, and the model picker faces
the identical tradeoff. KTD3 takes the identical answer *and goes one step
further*: not only does the region never capture the caret, selection itself
never touches a widget at all. There is no ``on_click`` here and no row is ever
``can_focus``. The operator opens the region with ``/models`` and selects a row
by typing ``/models <index>`` — a second command, not a keypress aimed at a
list. That is what "selection by command rather than by a captured caret"
(KTD3) means concretely: the composer is the only thing in this interface that
is ever focused, and this module has no code path that could change that (R5).

**What is rendered and what is not.** ``ProviderCatalog`` — U1's pure decode of
``GET /api/model/options`` — carries no ``available``/``failure`` pair the way
``CommandCatalog`` does, because the admin HTTP surface's failure vocabulary
(:class:`~talaria.transport.admin.AdminError`) lives one layer below the
decode. The app therefore holds the fetch failure separately and passes it to
:meth:`PickerRegion.apply` alongside whatever catalogue it has, which is why
that method takes ``failure`` as its own argument instead of expecting the
domain type to carry it. R7's three distinguishable states —
never-fetched, fetch-failed, and fetched-with-a-provider-warning — are
rendered from that pair plus each provider's own ``warning`` field.

**Numbering is the selection contract.** :func:`flatten_selectable` assigns
every model a 1-based index in strict listing order, and that is the *only*
thing ``/models <n>`` consults to resolve an index back into a provider slug
and a model name (in ``talaria/ui/app.py``). :meth:`PickerRegion.apply` builds
its on-screen rows from that same function rather than walking the catalogue a
second time, so the numbers on screen and the numbers the app resolves against
cannot drift apart from each other.
"""

from __future__ import annotations

from dataclasses import dataclass

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.widgets import Static

from talaria.domain.models_catalog import ModelProvider, ProviderCatalog
from talaria.ui.literal import literal_text

#: What the header says when nothing has been fetched yet — distinct from "the
#: gateway has no providers", which is a claim, and from a fetch that failed,
#: which is a different one again (R7).
NOT_YET_FETCHED = "models: not fetched yet"

#: Prefix for the line naming a catalogue Talaria could not read.
CATALOG_FAILURE_PREFIX = "models unavailable — "

#: Prefix for the line naming a provider-level warning the gateway attached.
PROVIDER_WARNING_PREFIX = "the gateway reported: "

#: Said in place of "no providers at all" for an empty, but successfully
#: fetched, catalogue.
NO_PROVIDERS = "models: the gateway reports no providers"

#: Marks the row matching the catalogue's ``current_model``/``current_provider``.
CURRENT_MARKER = "* "

#: Marks a provider the gateway reports as not yet authenticated — selecting
#: one of its models is a guaranteed failure, and saying so here is cheaper
#: than the round trip that would otherwise say it.
UNAUTHENTICATED_SUFFIX = " [unauthenticated]"

#: Said in place of a provider's model list when the gateway sent none.
NO_MODELS_LINE = "    (no models)"


@dataclass(frozen=True)
class SelectableRow:
    """One model, addressable by the 1-based index ``/models <n>`` selects.

    ``index`` is assigned by :func:`flatten_selectable` in strict listing
    order and is the one thing an operator ever types to pick a row — never
    the model name or the provider slug, both of which can carry spaces or
    punctuation a composed command line would have to quote.
    """

    index: int
    provider_slug: str
    provider_name: str
    model: str
    is_current: bool
    authenticated: bool


def flatten_selectable(catalog: ProviderCatalog) -> tuple[SelectableRow, ...]:
    """Every model across every provider, in listing order, numbered from 1.

    Pure, so a test asserts the numbering and the current-selection marking
    without a screen, and so the widget's own rendering and the app's
    selection lookup are guaranteed to agree — both are required to call this
    rather than walk ``catalog.providers`` a second time.
    """
    rows: list[SelectableRow] = []
    index = 1
    for provider in catalog.providers:
        for model in provider.models:
            is_current = (
                model == catalog.current_model and provider.slug == catalog.current_provider
            )
            rows.append(
                SelectableRow(
                    index=index,
                    provider_slug=provider.slug,
                    provider_name=provider.name,
                    model=model,
                    is_current=is_current,
                    authenticated=provider.authenticated,
                )
            )
            index += 1
    return tuple(rows)


def format_provider_header(provider: ModelProvider) -> str:
    """One provider's own row. Pure, so a test asserts on it without a screen."""
    suffix = "" if provider.authenticated else UNAUTHENTICATED_SUFFIX
    return f"{provider.name} ({provider.slug}){suffix}"


def format_model_row(row: SelectableRow) -> str:
    """One model's row, current-marked and numbered for selection."""
    marker = CURRENT_MARKER if row.is_current else "  "
    return f"{marker}{row.index:>3}. {row.model}"


def header_line(catalog: ProviderCatalog | None, failure: str) -> str:
    """The count that stays on screen when the rows are folded away.

    ``failure`` takes precedence over ``catalog`` on purpose: a failed refetch
    after a reconnect still has the app holding last epoch's ``catalog`` object
    in memory only until :meth:`PickerRegion.apply` is called with the new
    pair, and the header must say what the *current* fetch did, not what an
    earlier one produced.
    """
    if failure:
        return f"{CATALOG_FAILURE_PREFIX}{failure}"
    if catalog is None:
        return NOT_YET_FETCHED
    if catalog.is_empty:
        return NO_PROVIDERS
    return f"models: {len(catalog.providers)} providers"


def warning_line(catalog: ProviderCatalog | None) -> str:
    """The gateway's own provider-level warnings, joined onto one line.

    Distinct from ``failure`` (R7): this is a catalogue Talaria *did* read,
    annotated by the gateway itself, the same distinction
    ``talaria/ui/palette.py`` draws between ``CATALOG_FAILURE_PREFIX`` and
    ``CATALOG_WARNING_PREFIX``.
    """
    if catalog is None:
        return ""
    warnings = [provider.warning for provider in catalog.providers if provider.warning]
    if not warnings:
        return ""
    return f"{PROVIDER_WARNING_PREFIX}{'; '.join(warnings)}"


class PickerRegion(Vertical):
    """The foldable model picker: providers, their models, current marked.

    Named ``PickerRegion`` to sit beside ``PaletteRegion`` rather than clash
    with it — see that module's docstring for why a name collision with a
    framework class is worth avoiding on its own; this is the same argument
    applied to avoiding a collision with its sibling.
    """

    DEFAULT_CSS = """
    PickerRegion {
        height: auto;
        max-height: 14;
        display: none;
    }
    PickerRegion.-showing {
        display: block;
    }
    PickerRegion > .picker--header {
        color: $text-muted;
    }
    PickerRegion > .picker--warning {
        color: $warning;
        display: none;
    }
    PickerRegion > .picker--warning.-said {
        display: block;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self.showing = False
        self._header: Static | None = None
        self._warning: Static | None = None
        self._rows: list[Static] = []
        self._catalog: ProviderCatalog | None = None
        self._failure = ""

    def compose(self) -> ComposeResult:
        self._header = Static(
            literal_text(NOT_YET_FETCHED), markup=False, classes="picker--header"
        )
        yield self._header
        self._warning = Static(literal_text(""), markup=False, classes="picker--warning")
        yield self._warning

    # ── read access, so tests never reach through to the framework ───────

    @property
    def header_text(self) -> str:
        return "" if self._header is None else str(self._header.content)

    @property
    def warning_text(self) -> str:
        return "" if self._warning is None else str(self._warning.content)

    @property
    def row_texts(self) -> tuple[str, ...]:
        return tuple(str(row.content) for row in self._rows)

    @property
    def catalog(self) -> ProviderCatalog | None:
        return self._catalog

    @property
    def failure(self) -> str:
        return self._failure

    # ── rendering ────────────────────────────────────────────────────────

    async def apply(self, catalog: ProviderCatalog | None, *, failure: str = "") -> None:
        """Render the listing. Safe to call before anything has been fetched."""
        self._catalog = catalog
        self._failure = failure
        self.set_class(self.showing, "-showing")

        if self._header is not None:
            self._header.update(literal_text(header_line(catalog, failure)))
        if self._warning is not None:
            said = warning_line(catalog)
            self._warning.update(literal_text(said))
            # Hidden rather than blank when there is nothing to say — the same
            # reason ``PaletteRegion._degraded`` does this: an empty
            # warning-coloured row reads as a message whose text failed to
            # render, which is worse than showing nothing at all.
            self._warning.set_class(bool(said), "-said")

        wanted = self._wanted_lines(catalog)
        while len(self._rows) > len(wanted):
            await self._rows.pop().remove()
        for index, text in enumerate(wanted):
            rendered = literal_text(text)
            if index < len(self._rows):
                self._rows[index].update(rendered)
            else:
                widget = Static(rendered, markup=False)
                self._rows.append(widget)
                await self.mount(widget)

    def _wanted_lines(self, catalog: ProviderCatalog | None) -> list[str]:
        if not self.showing or catalog is None:
            return []
        lines: list[str] = []
        rows = iter(flatten_selectable(catalog))
        for provider in catalog.providers:
            lines.append(format_provider_header(provider))
            if not provider.models:
                lines.append(NO_MODELS_LINE)
                continue
            for _ in provider.models:
                lines.append(format_model_row(next(rows)))
        return lines

    async def toggle(self) -> bool:
        """Show or hide the whole region. Renders whatever is already held.

        Fetching is not this method's job — the app fetches the model
        catalogue once per connection epoch (KTD4), the same way it fetches
        the command catalogue, and this only ever renders what that fetch
        already produced.
        """
        self.showing = not self.showing
        await self.apply(self._catalog, failure=self._failure)
        return self.showing
