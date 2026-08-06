"""The picker — U2's models region and U4's profiles region (KTD3, 2026-08-06 plan).

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

**One region, two modes, and why it is not two regions (U4).** The profile
picker is the same foldable region in a different mode rather than a second
widget. Two regions would need two toggles, two places to keep the KTD3 rule
that neither captures the caret, and — since only one can usefully be open at a
time — a rule about what happens when both are. A mode is that rule, expressed
as data: opening one closes the other by construction.

**Dialability is a *pair* of facts, and only one of them comes from Hermes.**
``gateway_running`` says a gateway for that profile is up. It does not say
where: ``GET /api/profiles`` publishes no host, port or URL at all (measured —
see ``talaria/transport/admin.py``'s docstring for the fourteen keys it does
publish). The endpoint comes from Talaria's own ``[profiles.endpoints]``
configuration, so a row is dialable only when the gateway is running **and**
Talaria knows an address for it, and :class:`ProfileRow` carries the two
reasons separately because they need different fixes: start the gateway, or add
a line to ``config.toml``.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from typing import Literal

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.widgets import Static

from talaria.domain.models_catalog import (
    ModelProvider,
    ProfileDirectory,
    ProviderCatalog,
)
from talaria.ui.literal import literal_text

#: Which listing the region is showing. ``"models"`` is U2's and stays the
#: default, so nothing about the model picker changes for an operator who never
#: types ``/profiles``.
PickerMode = Literal["models", "profiles"]

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

# ── U4's profile mode ─────────────────────────────────────────────────────

#: Header before any profile listing has been read.
PROFILES_NOT_YET_FETCHED = "profiles: not fetched yet"

#: Prefix for the line naming a profile listing Talaria could not read.
PROFILES_FAILURE_PREFIX = "profiles unavailable — "

#: Said for an empty, but successfully fetched, directory.
NO_PROFILES = "profiles: the gateway reports no profiles"

#: Marks the profile this session is already connected to.
CURRENT_PROFILE_MARKER = "* "

#: Why a listed profile cannot be dialled. Two reasons, kept apart because they
#: have two different fixes — start the gateway, or tell Talaria its address.
NOT_RUNNING_SUFFIX = " [gateway not running]"
NO_ENDPOINT_SUFFIX = " [no endpoint configured]"


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


@dataclass(frozen=True)
class ProfileRow:
    """One profile, addressable by the index ``/profiles <n>`` selects.

    ``dialable`` is the conjunction the app checks before anything is dialled,
    and ``undialable_reason`` is the half of it that failed, so the refusal on
    screen names the fix rather than restating the refusal.

    ``endpoint`` is Talaria's own configured address for this profile and is
    ``""`` when there is none — never a guess. Nothing renders it: the listing
    shows the profile's name, and an endpoint is exactly the string an operator
    pastes a token into.
    """

    index: int
    name: str
    model: str
    provider: str
    gateway_running: bool
    endpoint: str
    is_current: bool

    @property
    def dialable(self) -> bool:
        return self.gateway_running and bool(self.endpoint)

    @property
    def undialable_reason(self) -> str:
        """Why this row cannot be dialled, or ``""`` when it can be.

        The gateway's own answer is checked first. When both are missing that
        is the one to say: a configured endpoint for a gateway that is not
        running would still fail, so "start it" is the first step either way.
        """
        if not self.gateway_running:
            return "its gateway is not running"
        if not self.endpoint:
            return "Talaria has no configured endpoint for it"
        return ""


def flatten_profiles(
    directory: ProfileDirectory,
    endpoints: Mapping[str, str],
    *,
    current: str = "",
) -> tuple[ProfileRow, ...]:
    """Every profile, in listing order, numbered from 1 and marked for dialability.

    Pure, and the single place the numbering is assigned — the widget renders
    from it and the app resolves ``/profiles <n>`` against it, so the number on
    screen and the number the app looks up cannot drift (the same contract
    :func:`flatten_selectable` holds for models).
    """
    return tuple(
        ProfileRow(
            index=index,
            name=entry.name,
            model=entry.model,
            provider=entry.provider,
            gateway_running=entry.gateway_running,
            endpoint=endpoints.get(entry.name, ""),
            is_current=bool(current) and entry.name == current,
        )
        for index, entry in enumerate(directory.profiles, start=1)
    )


def format_profile_row(row: ProfileRow) -> str:
    """One profile's row: number, name, its configured model, dialability."""
    marker = CURRENT_PROFILE_MARKER if row.is_current else "  "
    described = row.name
    if row.model:
        described = f"{described} — {row.model}"
        if row.provider:
            described = f"{described} ({row.provider})"
    elif row.provider:
        described = f"{described} ({row.provider})"
    if row.gateway_running and not row.endpoint:
        described += NO_ENDPOINT_SUFFIX
    elif not row.gateway_running:
        described += NOT_RUNNING_SUFFIX
    return f"{marker}{row.index:>3}. {described}"


def profiles_header_line(directory: ProfileDirectory | None, failure: str) -> str:
    """The count that stays on screen when the profile rows are folded away.

    ``failure`` outranks ``directory`` for the reason :func:`header_line`
    documents: a listing held from an earlier read must not paper over a read
    that just failed.
    """
    if failure:
        return f"{PROFILES_FAILURE_PREFIX}{failure}"
    if directory is None:
        return PROFILES_NOT_YET_FETCHED
    if directory.is_empty:
        return NO_PROFILES
    return f"profiles: {len(directory.profiles)} listed"


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
        #: Which listing is on screen (U4). One region, two modes — see the
        #: module docstring.
        self.mode: PickerMode = "models"
        self._header: Static | None = None
        self._warning: Static | None = None
        self._rows: list[Static] = []
        self._catalog: ProviderCatalog | None = None
        self._failure = ""
        self._profiles: ProfileDirectory | None = None
        self._profiles_failure = ""
        self._endpoints: Mapping[str, str] = {}
        self._current_profile = ""

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

    @property
    def profiles(self) -> ProfileDirectory | None:
        return self._profiles

    @property
    def profiles_failure(self) -> str:
        return self._profiles_failure

    # ── rendering ────────────────────────────────────────────────────────

    async def apply(self, catalog: ProviderCatalog | None, *, failure: str = "") -> None:
        """Render the model listing. Safe to call before anything is fetched.

        Rendering is skipped while the region is showing *profiles*: a model
        catalogue arriving from a background fetch must not silently replace
        the listing the operator is currently reading. The value is still
        stored, so switching back shows the newer one.
        """
        self._catalog = catalog
        self._failure = failure
        if self.mode != "models":
            return
        await self._repaint()

    async def apply_profiles(
        self,
        directory: ProfileDirectory | None,
        *,
        failure: str = "",
        endpoints: Mapping[str, str] | None = None,
        current: str = "",
    ) -> None:
        """Render the profile listing (U4). Safe to call before any fetch.

        ``endpoints`` is Talaria's own name-to-address map and decides the
        second half of dialability; ``current`` names the profile this session
        is already on, so the operator is not offered a switch to where they
        already are as though it were a move.
        """
        self._profiles = directory
        self._profiles_failure = failure
        if endpoints is not None:
            self._endpoints = endpoints
        self._current_profile = current
        if self.mode != "profiles":
            return
        await self._repaint()

    async def _repaint(self) -> None:
        """Draw whichever mode is current, from whatever is already held."""
        self.set_class(self.showing, "-showing")

        if self.mode == "profiles":
            catalog = None
            head = profiles_header_line(self._profiles, self._profiles_failure)
            said = ""
        else:
            catalog = self._catalog
            head = header_line(catalog, self._failure)
            said = warning_line(catalog)

        if self._header is not None:
            self._header.update(literal_text(head))
        if self._warning is not None:
            self._warning.update(literal_text(said))
            # Hidden rather than blank when there is nothing to say — the same
            # reason ``PaletteRegion._degraded`` does this: an empty
            # warning-coloured row reads as a message whose text failed to
            # render, which is worse than showing nothing at all.
            self._warning.set_class(bool(said), "-said")

        wanted = self._wanted_profile_lines() if self.mode == "profiles" else (
            self._wanted_lines(catalog)
        )
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

    def _wanted_profile_lines(self) -> list[str]:
        if not self.showing or self._profiles is None:
            return []
        return [
            format_profile_row(row)
            for row in flatten_profiles(
                self._profiles, self._endpoints, current=self._current_profile
            )
        ]

    async def toggle(self, mode: PickerMode = "models") -> bool:
        """Show or hide the region in ``mode``. Renders whatever is held.

        Asking for a mode the region is not showing **switches to it and shows
        it**, rather than toggling: an operator who has ``/models`` open and
        types ``/profiles`` means "show me profiles", and a literal toggle
        would close the region and leave them typing it a second time. Asking
        for the mode already on screen is the ordinary fold.

        Fetching is not this method's job — the app fetches both listings once
        per connection epoch (KTD4), the same way it fetches the command
        catalogue, and this only ever renders what those fetches produced.
        """
        if self.showing and self.mode != mode:
            self.mode = mode
        else:
            self.mode = mode
            self.showing = not self.showing
        await self._repaint()
        return self.showing
