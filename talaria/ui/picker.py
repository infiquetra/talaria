"""What the picker offers: models by provider, profiles by name, sessions by row.

**This module used to hold a foldable region, and the region is gone.** KTD3 of
the 2026-08-06 plan modelled the picker on :class:`~talaria.ui.palette.PaletteRegion`
— no focus, no keys, selection by typing ``/models <n>`` — reasoning from
``talaria/ui/palette.py:1-22``'s rejection of a modal search box. That reasoning
holds for a command *listing*, which is something an operator reads. It does not
hold for a picker, which is something an operator operates, and the first live
use said so immediately. KTD3 is overturned; see
``docs/engineering-journal/DECISIONS.md``, "The picker is a modal dialog". The
rows are now rendered and driven by :class:`~talaria.ui.dialog.PickerDialog`,
and what is left here is the pure half: the numbering, the formatting, and the
three :class:`~talaria.domain.selection.PickerSource` implementations that say
what each level of the dialog contains — models, profiles, and the sessions of
the v0.4 registry.

**What is rendered and what is not.** ``ProviderCatalog`` — U1's pure decode of
``GET /api/model/options`` — carries no ``available``/``failure`` pair the way
``CommandCatalog`` does, because the admin HTTP surface's failure vocabulary
(:class:`~talaria.transport.admin.AdminError`) lives one layer below the
decode. The app therefore holds the fetch failure separately and pairs it with
whatever catalogue it has, which is why :func:`header_line` takes ``failure`` as
its own argument instead of expecting the domain type to carry it. R7's three
distinguishable states — never-fetched, fetch-failed, and
fetched-with-a-provider-warning — are rendered from that pair plus each
provider's own ``warning`` field.

**Numbering is still the selection contract, and that is why the dialog emits a
number rather than a model name.** :func:`flatten_selectable` assigns every
model a 1-based index in strict listing order, and that index is the *only*
thing ``/models <n>`` consults to resolve back into a provider slug and a model
name (in ``talaria/ui/app.py``). :class:`ModelPickerSource` builds its rows from
that same function and carries the index as each row's payload, so choosing a
row in the dialog and typing its number into the composer travel the identical
path — including every refusal on it, which is why the stale-epoch and
unauthenticated-provider checks did not have to be written twice.

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

from talaria.domain.models_catalog import (
    ModelProvider,
    ProfileDirectory,
    ProviderCatalog,
)
from talaria.domain.normalize import clip_detail_line
from talaria.domain.registry import attach_displaces_client, row_age, waiting_floor_span
from talaria.domain.selection import Choice, Selection, Stage
from talaria.domain.state import FleetState

#: Which listing the dialog was opened on. ``"models"`` is U2's and stays the
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

#: Said on the row Talaria itself switched this session to. Worded as an event
#: Talaria witnessed rather than a state it is asserting: what is known is that
#: the gateway accepted ``/model <name> --provider <slug>`` on this session, and
#: that is a smaller claim than "this is the model answering you".
SWITCHED_NOTE = "switched here this session"

#: Said on the row the gateway's catalogue reports as the profile's default —
#: which is what ``GET /api/model/options`` publishes and, crucially, *all* it
#: publishes about currency. See :class:`SelectableRow`.
PROFILE_DEFAULT_NOTE = "profile default"

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

#: Why a listed profile cannot be dialled. Two reasons, kept apart because they
#: have two different fixes — start the gateway, or tell Talaria its address.
NOT_RUNNING_SUFFIX = " [gateway not running]"
NO_ENDPOINT_SUFFIX = " [no endpoint configured]"


@dataclass(frozen=True)
class SessionModel:
    """What Talaria last switched *this* session to, and which session that was.

    This type exists because the gateway does not publish the fact. ``/models``
    dispatches ``/model <name> --provider <slug>``, which Hermes applies to the
    running chat session and to nothing else — its own reply says so: "session
    only — add --global to persist". Nothing on the admin HTTP surface reports
    a session's model back: ``GET /api/model/options`` builds its ``model`` and
    ``provider`` fields from ``load_picker_context()``, which reads the
    profile's ``config.yaml`` off disk (``hermes_cli/inventory.py:79-105`` at
    ``f1470ec76``). So a refetch after a switch returns the identical answer it
    returned before, forever, and Talaria's own record of what it sent is the
    only source there is.

    ``session_id`` is carried rather than assumed because the record is about
    one session. A switch to a different profile is a different gateway and a
    different session, and a remembered model from the old one would be a
    confident false statement — the failure mode this whole module is written
    against. :meth:`applies_to` is the check; nothing reads the fields without
    passing it first.
    """

    session_id: str
    provider_slug: str
    model: str

    def applies_to(self, session_id: str) -> bool:
        """True when this record describes the session now in focus."""
        return bool(self.session_id) and self.session_id == session_id


@dataclass(frozen=True)
class SelectableRow:
    """One model, addressable by the 1-based index ``/models <n>`` selects.

    ``index`` is assigned by :func:`flatten_selectable` in strict listing
    order and is the one thing an operator ever types to pick a row — never
    the model name or the provider slug, both of which can carry spaces or
    punctuation a composed command line would have to quote.

    ``is_profile_default`` was called ``is_current`` and the rename is the
    point, not tidying. The gateway's catalogue reports the model a *new*
    session on this profile would start with, and calling that "current" is
    what made the picker mark the wrong row after a live switch: the operator
    picks a model, re-opens ``/models``, and the marker is still on the one
    they left. The field says what it holds now, and :class:`SessionModel`
    holds the other fact separately.
    """

    index: int
    provider_slug: str
    provider_name: str
    model: str
    is_profile_default: bool
    authenticated: bool


def flatten_selectable(catalog: ProviderCatalog) -> tuple[SelectableRow, ...]:
    """Every model across every provider, in listing order, numbered from 1.

    Pure, so a test asserts the numbering and the default-model marking without
    a screen, and so the widget's own rendering and the app's selection lookup
    are guaranteed to agree — both are required to call this rather than walk
    ``catalog.providers`` a second time.
    """
    rows: list[SelectableRow] = []
    index = 1
    for provider in catalog.providers:
        for model in provider.models:
            is_default = (
                model == catalog.current_model and provider.slug == catalog.current_provider
            )
            rows.append(
                SelectableRow(
                    index=index,
                    provider_slug=provider.slug,
                    provider_name=provider.name,
                    model=model,
                    is_profile_default=is_default,
                    authenticated=provider.authenticated,
                )
            )
            index += 1
    return tuple(rows)


def format_provider_header(provider: ModelProvider) -> str:
    """One provider's own row. Pure, so a test asserts on it without a screen."""
    suffix = "" if provider.authenticated else UNAUTHENTICATED_SUFFIX
    return f"{provider.name} ({provider.slug}){suffix}"


def header_line(catalog: ProviderCatalog | None, failure: str) -> str:
    """One line naming what the last read of the model list actually produced.

    ``failure`` takes precedence over ``catalog`` on purpose: a failed refetch
    after a reconnect still has the app holding last epoch's ``catalog`` object
    in memory until the next fetch replaces the pair, and this line must say
    what the *current* fetch did, not what an earlier one produced.
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


def format_profile_label(row: ProfileRow) -> str:
    """One profile described: name, its configured model, and its dialability.

    The number and the current-marker are **not** here, and were until the
    dialog took over the drawing. The dialog renders both itself, from
    :attr:`~talaria.domain.selection.Choice.number` and ``Choice.marked``, so a
    label carrying them too produced a row numbered and marked twice over.
    What is left is the part only this module knows how to say.
    """
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
    return described


def profiles_header_line(directory: ProfileDirectory | None, failure: str) -> str:
    """One line naming what the last read of the profile listing produced.

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


# ── what each level of the dialog contains (KTD3 superseded) ─────────────


class ModelPickerSource:
    """Providers first, then that provider's models.

    Two levels rather than one flat list of every model, which is Hermes's
    shape (``ui-tui/src/components/modelPicker.tsx`` stages ``provider`` then
    ``model``). The flat alternative was measured against a real gateway during
    the row-19 acceptance run: eight providers, about a hundred models. A flat
    list makes the filter carry the whole burden of finding anything, and it
    gives an operator who knows which provider they want no way to say so.

    An unauthenticated provider is **listed and highlightable, not hidden**.
    Its models cannot be selected — the round trip is a guaranteed failure and
    the app refuses it independently — but an operator who cannot see the
    provider cannot learn that authenticating it is what they need to do.

    **Two different rows can be "current" and the source shows both.**
    ``session`` is Talaria's record of the switch it made on this session
    (:class:`SessionModel`); the catalogue's own ``current_model`` is the
    profile's default, which is a fact about the *next* session. The marker and
    the opening highlight follow the session record when there is one, because
    that is the model the operator is talking to right now; the profile default
    keeps its own row and its own note either way, so the operator can see that
    the two have diverged rather than wondering why Talaria disagrees with the
    Hermes dashboard.
    """

    def __init__(self, catalog: ProviderCatalog, session: SessionModel | None = None) -> None:
        self._catalog = catalog
        self._session = session
        self._rows = flatten_selectable(catalog)

    def root(self) -> Stage:
        session = self._session
        choices = tuple(
            Choice(
                key=provider.slug,
                label=format_provider_header(provider),
                detail=self._provider_detail(provider),
                marked=(
                    provider.slug == session.provider_slug
                    if session is not None
                    else provider.slug == self._catalog.current_provider
                ),
                selectable=provider.authenticated and bool(provider.models),
                refusal=self._provider_refusal(provider),
            )
            for provider in self._catalog.providers
        )
        return Stage(title="models — choose a provider", selection=Selection.opened(choices))

    def _provider_detail(self, provider: ModelProvider) -> str:
        """The model count, and **at most one** currency note.

        One note rather than both, unlike the model stage. A provider row is
        already the longest thing the dialog draws — a name, a slug and a count
        — and stacking two notes on it wrapped to a second line at the width
        this dialog is pinned to. The session note wins when it applies because
        the provider stage's job is to get the operator to the right provider,
        and "the one you are talking to" is the better guide for that. The two
        facts are still shown apart one level down, on rows short enough to
        carry them.
        """
        count = len(provider.models)
        detail = f"({count} model{'' if count == 1 else 's'})"
        session = self._session
        if session is not None and provider.slug == session.provider_slug:
            return f"{detail} · {SWITCHED_NOTE}"
        if provider.slug == self._catalog.current_provider:
            return f"{detail} · {PROFILE_DEFAULT_NOTE}"
        return detail

    @staticmethod
    def _provider_refusal(provider: ModelProvider) -> str:
        """Why this provider has nothing to descend into.

        Checked in the order the operator can act on: authentication first,
        because a provider that is not authenticated reports no models either
        and "it has no models" would send them looking for the wrong fix.
        """
        if not provider.authenticated:
            return f"{provider.name} is not authenticated"
        if not provider.models:
            return f"the gateway lists no models for {provider.name}"
        return ""

    def descend(self, depth: int, choice: Choice) -> Stage | str:
        if depth > 0:
            return choice.payload
        session = self._session
        models = tuple(
            Choice(
                key=f"{row.provider_slug}:{row.model}",
                label=row.model,
                payload=str(row.index),
                number=row.index,
                detail=self._model_detail(row),
                marked=(
                    self._is_session_model(row)
                    if session is not None
                    else row.is_profile_default
                ),
            )
            for row in self._rows
            if row.provider_slug == choice.key
        )
        return Stage(title=f"models — {choice.label}", selection=Selection.opened(models))

    def _is_session_model(self, row: SelectableRow) -> bool:
        session = self._session
        if session is None:
            return False
        return row.provider_slug == session.provider_slug and row.model == session.model

    def _model_detail(self, row: SelectableRow) -> str:
        notes = []
        if self._is_session_model(row):
            notes.append(SWITCHED_NOTE)
        if row.is_profile_default:
            notes.append(PROFILE_DEFAULT_NOTE)
        return " · ".join(notes)


class ProfilePickerSource:
    """One level: every profile, dialability marked, current one excluded.

    Flat because a profile listing has no second level to descend into, and
    because the count is bounded by how many profiles the operator has
    configured rather than by what a provider publishes.

    The already-connected profile is listed as unselectable rather than
    dropped. Dropping it would renumber every row below it, and the numbering
    is a contract shared with ``/profiles <n>`` typed into the composer.
    """

    def __init__(
        self,
        directory: ProfileDirectory,
        endpoints: Mapping[str, str],
        *,
        current: str = "",
    ) -> None:
        self._rows = flatten_profiles(directory, endpoints, current=current)

    def root(self) -> Stage:
        choices = tuple(
            Choice(
                key=row.name,
                label=format_profile_label(row),
                payload=str(row.index),
                number=row.index,
                marked=row.is_current,
                selectable=row.dialable and not row.is_current,
                refusal=self._refusal(row),
            )
            for row in self._rows
        )
        return Stage(title="profiles — choose one to dial", selection=Selection.opened(choices))

    @staticmethod
    def _refusal(row: ProfileRow) -> str:
        if row.is_current:
            return f"already connected to {row.name}"
        return row.undialable_reason

    def descend(self, depth: int, choice: Choice) -> Stage | str:
        return choice.payload


# ── the session picker's shared row vocabulary (U7's KTD6, kept) ─────────


#: Said on the row naming the session already in focus.
SESSION_ALREADY_FOCUSED: str = "already the focused session"

#: Shown in place of a session with no title — the gateway sends ``""`` for
#: one rather than omitting the key (``tui_gateway/methods_session.py:196``).
UNTITLED_SESSION: str = "(untitled session)"


# ── U4's registry-backed session rows (v0.4 R3, PC2) ─────────────────────
#
# These replaced the v0.2 rows, which were whatever ``session.list`` said and
# nothing else. That reply carries no lifecycle field at all (verified live,
# U1), so those rows could say when a session started and never what it was
# doing. The registry can, because it also folds ``session.active_list``, and
# R3's second half is exactly that: the picker shows lifecycle, what the
# session is waiting on, where the observation came from, and how old it is.
# The old row helpers were deleted rather than kept beside these — see
# ``docs/engineering-journal/DECISIONS.md`` for that decision and its cost (an
# absolute start timestamp no row shows any more).


#: What a row's status says when the registry has an identity for a session and
#: no lifecycle observation of it (R24's never-observed class). Never ``idle``:
#: a listing proves a session exists, not what it is doing, and rendering the
#: absence as a state is the fabrication R10 exists to prevent.
NEVER_OBSERVED_STATUS: str = "never observed · no lifecycle poll yet"

#: Said after the status block for a live session this run did not open (KTD8).
#: The operator sees before they choose the row what the confirmation dialog
#: will then say — the dialog is the gate, this is the warning.
NOT_OURS_SUFFIX: str = " · live elsewhere"

#: How much of a gateway-supplied title one registry row shows. Bounded, and
#: bounded *here* rather than left to the terminal, because the title is the
#: only variable-width field on the row and everything the operator needs to
#: judge the row — status, source, age — is drawn after it. At 80 columns an
#: unbounded title pushes all three off the screen.
REGISTRY_TITLE_CLIP: int = 28

#: How much of a row's ``last_notice`` reaches the picker. The whole line lives
#: in the transcript; the row carries enough of it to be recognized.
REGISTRY_NOTICE_CLIP: int = 60


@dataclass(frozen=True)
class RegistrySessionRow:
    """One registry row as the picker shows it (R3's second half).

    :attr:`session_id` is the row's **durable** id — the registry key's second
    half — because the selection contract is unchanged: a chosen row emits the
    id a later ``session.resume`` asks for. Every other field is a summary the
    registry already holds; nothing here reads a clock, so the ages are the
    frame-clock spans :mod:`talaria.domain.registry` computed (KTD12).
    """

    session_id: str
    title: str
    lifecycle: str
    observation: str
    waiting_kind: str
    waiting_span: float | None
    source: str
    age: float | None
    notice: str
    message_count: int
    ownership: str
    is_current: bool
    #: Whether choosing this row would take a live client's session away —
    #: :func:`~talaria.domain.registry.attach_displaces_client`'s answer, carried
    #: here rather than re-derived from the flattened strings.
    #:
    #: Re-deriving it was a real defect. This row said "live elsewhere" whenever
    #: the row was not ours and its *observation class* was live — but that class
    #: is R24's data-freshness question (have we seen it lately), not the
    #: gateway's liveness (does the roster still list it). A sweep that omits a
    #: session clears the liveness and leaves the freshness alone, so the picker
    #: warned "live elsewhere", the operator chose the row, and the confirmation
    #: correctly did not fire. The warning and the gate disagreed, which is the
    #: whole failure this field exists to make impossible.
    displaces_client: bool = False


def format_span(seconds: float | None) -> str:
    """A frame-clock span as a short, deterministic label.

    ``None`` renders as ``"—"`` rather than as ``0s``: no observation and an
    observation one instant old are different facts, and the second one is a
    measurement (R24).
    """
    if seconds is None:
        return "—"
    total = int(max(0.0, seconds))
    if total < 60:
        return f"{total}s"
    if total < 3600:
        return f"{total // 60}m"
    return f"{total // 3600}h"


def format_registry_status(row: RegistrySessionRow) -> str:
    """The fixed-width half of a registry row: lifecycle, source, and age.

    The waiting age is rendered as a floor — ``≥`` — and never as a start time.
    No poll row and no approval event carries a start stamp at any gateway
    revision U1 examined, so the span since Talaria first observed the wait is
    the only honest age *any* client can render (KTD12).
    """
    if row.observation == "never-observed":
        status = NEVER_OBSERVED_STATUS
    else:
        lifecycle = row.lifecycle or "unreported"
        if row.waiting_kind:
            lifecycle = f"{lifecycle} ≥{format_span(row.waiting_span)} on {row.waiting_kind}"
        source = row.source or "unattributed"
        status = f"{lifecycle} · {source} {format_span(row.age)}"
        if row.observation == "stale":
            status = f"{status} · stale"
    if row.displaces_client:
        status += NOT_OURS_SUFFIX
    return status


def format_registry_label(row: RegistrySessionRow) -> str:
    """The row's own line: what it is doing, then what it is called.

    Status first. The title is the gateway's, so it is the one field whose
    width Talaria does not control, and a row that puts it first is a row whose
    fixed facts disappear the moment somebody names a session with a sentence.
    """
    title = row.title.strip()
    shown = clip_detail_line(title, REGISTRY_TITLE_CLIP) if title else UNTITLED_SESSION
    return f"{format_registry_status(row)}  {shown}"


def format_registry_detail(row: RegistrySessionRow) -> str:
    """The row's second half: the durable id, its size, and any latched notice."""
    parts = [row.session_id]
    if row.message_count:
        parts.append(f"{row.message_count} msgs")
    if row.notice:
        parts.append(clip_detail_line(row.notice, REGISTRY_NOTICE_CLIP))
    return " · ".join(parts)


def flatten_registry_sessions(
    fleet: FleetState, *, profile: str, current: str = ""
) -> tuple[RegistrySessionRow, ...]:
    """Every registry row of one connection, most recently observed first.

    Pure — the same contract the three flatteners above keep — and ordered
    deterministically so two renders of one registry produce one order:
    observed rows before never-observed ones, then youngest observation first,
    then by durable id so two equal ages cannot swap between renders.
    """
    rows = [
        RegistrySessionRow(
            session_id=key[1],
            title=row.title,
            lifecycle=row.lifecycle(),
            observation=row.observation(),
            waiting_kind=row.waiting_kind,
            waiting_span=waiting_floor_span(row, fleet.clock),
            source=row.last_event_source,
            age=row_age(row, fleet.clock),
            notice=row.last_notice,
            message_count=row.message_count,
            ownership=row.ownership,
            is_current=bool(current) and key[1] == current,
            displaces_client=attach_displaces_client(row),
        )
        for key, row in fleet.rows.items()
        if key[0] == profile
    ]
    rows.sort(key=lambda row: (row.age is None, row.age or 0.0, row.session_id))
    return tuple(rows)


class SessionPickerSource:
    """The ``/sessions`` listing, read from the registry instead of from a reply.

    One level and one selection contract, both unchanged from the v0.2 source
    this replaces: a chosen row emits its **durable** id, the focused row is
    marked and refuses itself. What changed is where a row's words come from.
    The old source could only repeat ``session.list``, which carries no
    lifecycle field at all (verified live, U1), so its rows said when a session
    started and never what it was doing. The registry has folded
    ``session.active_list`` as well, so these rows say both — and where it has
    folded no roster, they say *that*, as never-observed, rather than filling
    the gap with ``idle`` (R24).

    Rows are built by :func:`flatten_registry_sessions` and handed in already
    ordered, so this class holds no policy about which sessions are listed —
    the caller decides which connection's rows it is showing.
    """

    def __init__(
        self, rows: tuple[RegistrySessionRow, ...], *, title: str = ""
    ) -> None:
        self._rows = rows
        self._title = title or "sessions — choose one to switch to"

    def root(self) -> Stage:
        choices = tuple(
            Choice(
                key=row.session_id,
                label=format_registry_label(row),
                payload=row.session_id,
                detail=format_registry_detail(row),
                marked=row.is_current,
                selectable=not row.is_current,
                refusal=SESSION_ALREADY_FOCUSED if row.is_current else "",
            )
            for row in self._rows
        )
        return Stage(title=self._title, selection=Selection.opened(choices))

    def descend(self, depth: int, choice: Choice) -> Stage | str:
        return choice.payload
