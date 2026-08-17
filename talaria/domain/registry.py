"""Registry rows and per-connection channels: the fleet's bounded summaries (U3).

This module holds the *data* half of the v0.4 registry — one
:class:`RegistryRow` per session of every connected gateway, and one
:class:`ConnectionChannel` per connection. The root object that composes them
(:class:`~talaria.domain.state.FleetState`) and the router that feeds them live
in :mod:`talaria.domain.state`, per the plan's file map; this module depends on
nothing in ``state.py`` so the two cannot import each other in a cycle.

Three requirements shape every field here:

*A row is a fixed-size summary, never a transcript* (R3/PC2). Every collection
on a row is bounded by construction — ``runtime_ids`` keeps the most recent
:data:`MAX_RUNTIME_ALIASES`, ``last_notice`` is one clipped line — so routing a
thousand events at a row cannot grow it.

*Absence of observation is named, never faked* (R24). A row carries three
distinct display classes — live, stale, never-observed — and a seeded row whose
lifecycle nothing has confirmed answers ``never-observed``, not ``idle`` and
not ``stale``. ``status`` is the gateway's own word verbatim (KTD10:
re-encoding, not invention); ``""`` means no gateway has ever said one.

*Ages ride the frame clock* (KTD12/R20). Nothing in this module reads a wall
clock. Every stamp is the ``at`` of the frame or poll that produced it, and
every age is computed against the fleet's frame-clock high-water mark, so a
replayed recording reproduces every age exactly.
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from typing import Final, Literal

from talaria.domain.normalize import clip_detail_line

# ── Vocabulary ───────────────────────────────────────────────────────────

#: The gateway's own live-status words, verbatim (KTD10). ``starting`` is real
#: and reachable on an ordinary poll cadence — U1 sampled it on a first poll —
#: so it renders like any other word rather than being treated as theoretical.
#: A word outside this set is still stored verbatim (re-encoding, not
#: invention); the set exists for tests and renderers that need the closed
#: list the pinned gateway emits.
GATEWAY_STATUS_WORDS: Final[frozenset[str]] = frozenset(
    {"waiting", "starting", "working", "idle"}
)

#: R24's three display classes. Distinct on screen, and distinct here.
ObservationClass = Literal["live", "stale", "never-observed"]

#: KTD8's ownership bookkeeping: the sessions this run created, resumed, or
#: activated are ``we_drive``; everything else is ``not_ours``.
Ownership = Literal["we_drive", "not_ours"]

#: Where the last row-changing observation came from (R20's source half).
ObservationSource = Literal["", "event", "poll", "listing"]

#: One session's registry key: ``(profile, durable_id)``. The profile half is
#: the identity of the observing connection — no listing row carries a profile
#: field (verified live, U1), so a stored id seen on two connections is two
#: rows. The durable half is ``session_key`` when known, else the listing id;
#: runtime ids are routing aliases rebound on resume, never the key (KTD3).
RowKey = tuple[str, str]

# ── Bounds ───────────────────────────────────────────────────────────────

#: Memory bound per connection (PC2): the listing default is 200, plus headroom
#: for live-only sessions. Applies to unprotected rows only — the focused row,
#: rows holding queue items, and rows with in-flight answers are always
#: retained and may exceed the cap outright.
ROW_CAP_PER_CONNECTION: Final[int] = 256

#: How many runtime-id aliases one row may carry. Runtime ids change across
#: resume; the newest few are what routing needs, and an unbounded alias list
#: would let a churning session grow a row without bound (R3).
MAX_RUNTIME_ALIASES: Final[int] = 4

#: How many identity-less notices one connection channel retains for display
#: (R25 wants them surfaced; the count is the unbounded half, the text is not).
CHANNEL_NOTICE_LIMIT: Final[int] = 8

# ── KTD2 poll cadence (pure constants; the app owns the timers) ──────────

#: Client-side coalesce for ``sessions.changed``-triggered polls, matching the
#: server's own broadcast floor (2s, ``server.py:3748`` per U1's correction).
POLL_COALESCE_S: Final[float] = 2.0

#: The periodic backstop: poll at least this often even with no hint.
POLL_BACKSTOP_S: Final[float] = 30.0


# ── The row ──────────────────────────────────────────────────────────────


@dataclass(frozen=True)
class RegistryRow:
    """One session as the fleet knows it: a bounded summary, never a transcript.

    Field groups, per PC2: identity (``profile``/``durable_id``/aliases),
    listing content (``title``/``message_count``/``model`` — ``model`` is an
    observation, not configuration: U1 watched the gateway switch a session's
    model per-turn), lifecycle (``status`` verbatim plus the derived states
    :meth:`lifecycle` layers on top), ownership, waiting-kind, the last
    observation's source and stamp, and one bounded notice line.
    """

    profile: str
    durable_id: str
    runtime_ids: tuple[str, ...] = ()
    title: str = ""
    message_count: int = 0
    model: str = ""
    #: The gateway's status word verbatim (KTD10); ``""`` = never reported.
    status: str = ""
    #: Frame-clock stamp of the first observation of the *current* status —
    #: KTD12's observation floor. A wait age known only from a poll renders as
    #: "waiting ≥ observed span" measured from here; no start time is invented.
    status_floor_at: float = 0.0
    ownership: Ownership = "not_ours"
    #: True between an observed ``message.start`` and its turn's end. With
    #: ``we_drive`` this derives KTD10's ``streaming``.
    open_turn: bool = False
    #: The waiting kind when known from an observed ``*.request`` event;
    #: ``"unobserved"`` when only the poll's flattened ``waiting`` is known;
    #: ``""`` when not waiting.
    waiting_kind: str = ""
    reclaimed_reason: str | None = None
    disconnected: bool = False
    last_event_source: ObservationSource = ""
    #: Frame-clock stamp of the last row-changing observation. 0.0 with
    #: ``observed=False`` means no observation exists — no age is fabricated.
    last_event_at: float = 0.0
    #: When the row entered the registry (listing seed or first event).
    seeded_at: float = 0.0
    #: Whether any lifecycle-confirming observation (event or live poll) ever
    #: happened. False is exactly R24's never-observed class.
    observed: bool = False
    #: Frame-clock moment the row's source stopped (connection drop, reclaim).
    #: ``None`` while the source is current. Never set on a never-observed row
    #: — "stale-since-nothing" is forbidden by R24.
    stale_since: float | None = None
    #: Presence in the most recent successful ``session.list`` and
    #: ``session.active_list`` sweeps, with the poll epoch that saw the row.
    #: The dual-listing retirement rule reads these (KTD2).
    listed: bool = False
    live_listed: bool = False
    listing_epoch: int = -1
    active_epoch: int = -1
    #: One clipped line: expiry, terminal-read failure, unhandled-kind naming.
    last_notice: str = ""

    def lifecycle(self) -> str:
        """KTD10's vocabulary: Talaria-derived states over gateway words.

        Derived states win in a fixed order — a reclaimed row stays
        ``reclaimed(reason)`` however the connection fares afterward, a
        disconnected row cannot claim to be streaming — and the fallthrough is
        the gateway's own word untranslated (``""`` when never observed; the
        renderer names that class rather than inventing ``idle``).
        """
        if self.reclaimed_reason is not None:
            return f"reclaimed({self.reclaimed_reason})"
        if self.disconnected:
            return "disconnected"
        if self.ownership == "we_drive" and self.open_turn:
            return "streaming"
        return self.status

    def observation(self) -> ObservationClass:
        if not self.observed:
            return "never-observed"
        if self.stale_since is not None:
            return "stale"
        return "live"

    def last_seen_at(self) -> float:
        """Ordering key for oldest-first eviction: a never-observed row's
        moment of record is its seeding, an observed row's is its last
        observation."""
        return max(self.last_event_at, self.seeded_at)


def row_age(row: RegistryRow, clock: float) -> float | None:
    """Age of the last observation against the frame clock, or ``None``.

    ``None`` is never-observed: fabricating ``clock - 0.0`` would render a
    fifty-year age as if it were a measurement (R24).
    """
    if not row.observed:
        return None
    return max(0.0, clock - row.last_event_at)


def stale_for(row: RegistryRow, clock: float) -> float | None:
    """How long the row's source has been gone, or ``None`` when not stale."""
    if row.stale_since is None or not row.observed:
        return None
    return max(0.0, clock - row.stale_since)


def waiting_floor_span(row: RegistryRow, clock: float) -> float | None:
    """KTD12's observation floor: the span the row has *observably* waited.

    ``None`` unless the row currently reports ``waiting``. The value is a
    floor ("waiting ≥ span"), never a start time: no poll row carries a start
    stamp (U1 verified none exists at any revision), so the span since first
    observation is the only honest age any client can render.
    """
    if row.status != "waiting":
        return None
    return max(0.0, clock - row.status_floor_at)


# ── The connection channel ───────────────────────────────────────────────


@dataclass(frozen=True)
class ConnectionChannel:
    """Per-connection fleet bookkeeping: R25's channel, poll state, bounds.

    ``generation`` mirrors the transport's per-profile generation counter
    (``ConnectionSet``): a profile's correlator epoch restarts when ``ensure``
    rebuilds its source, so ``(profile, epoch)`` is not globally unique across
    a reconnect-by-ensure and the generation is what distinguishes connection
    identities (U2's review finding for this unit).
    """

    profile: str
    generation: int = 0
    connected: bool = False
    #: R25: events without trustworthy session identity, counted here — the
    #: role ``cross_session_events_ignored`` keeps at the fleet level (KTD3).
    identityless_count: int = 0
    #: The surfaced half of R25: bounded, newest-last notice lines.
    notices: tuple[str, ...] = ()
    #: Cumulative rows evicted by the memory bound; feeds the visible
    #: truncation note — an eviction is never a silent drop.
    evicted_rows: int = 0
    #: Frame-clock stamp of an unconsumed ``sessions.changed`` hint.
    hint_at: float | None = None
    last_poll_at: float | None = None
    #: Set when a ``session.list`` refresh failed: listing-derived fields are
    #: stale from this moment, and the data is marked rather than cleared (R5).
    listing_stale_since: float | None = None
    #: Epochs of the most recent successful sweeps. Retirement requires both
    #: to agree (the dual-listing rule, KTD2).
    listing_epoch: int = -1
    active_epoch: int = -1


def note_identityless(channel: ConnectionChannel, notice: str) -> ConnectionChannel:
    """Count and surface one identity-less event (R25). Bounded on both axes:
    the count is an int, the notices keep the newest :data:`CHANNEL_NOTICE_LIMIT`."""
    notices = (*channel.notices, clip_detail_line(notice))[-CHANNEL_NOTICE_LIMIT:]
    return replace(
        channel,
        identityless_count=channel.identityless_count + 1,
        notices=notices,
    )


def next_poll_due_at(channel: ConnectionChannel) -> float:
    """When the KTD2 poll loop should next call ``session.active_list``.

    Pure arithmetic over frame-clock stamps — the app owns the actual timers.
    A pending hint polls as soon as the 2-second coalesce window since the
    last poll allows; with no hint, the 30-second backstop governs. A channel
    that has never polled is due immediately.
    """
    if channel.last_poll_at is None:
        return 0.0
    if channel.hint_at is not None:
        return max(channel.hint_at, channel.last_poll_at + POLL_COALESCE_S)
    return channel.last_poll_at + POLL_BACKSTOP_S


def truncation_note(channel: ConnectionChannel) -> str:
    """The roster's visible truncation note, ``""`` when nothing was evicted."""
    if channel.evicted_rows <= 0:
        return ""
    return f"{channel.evicted_rows} older sessions not shown"


__all__ = [
    "CHANNEL_NOTICE_LIMIT",
    "GATEWAY_STATUS_WORDS",
    "MAX_RUNTIME_ALIASES",
    "POLL_BACKSTOP_S",
    "POLL_COALESCE_S",
    "ROW_CAP_PER_CONNECTION",
    "ConnectionChannel",
    "ObservationClass",
    "ObservationSource",
    "Ownership",
    "RegistryRow",
    "RowKey",
    "attach_displaces_client",
    "next_poll_due_at",
    "note_identityless",
    "row_age",
    "stale_for",
    "truncation_note",
    "waiting_floor_span",
]


def attach_displaces_client(row: RegistryRow) -> bool:
    """Whether attaching to this row takes a live client's session away.

    One predicate, three callers, and that is the point.
    :func:`~talaria.domain.state.attach_confirm_row` asks it to decide whether
    OP2's dialog fires; the attach handover asks it to decide whether the residue
    line — which says in so many words that a client was displaced — may be
    latched; and the picker asks it to decide whether a row carries the "live
    elsewhere" warning. Those three questions have exactly one answer, and giving
    them more than one was a real defect twice over: the dialog asked "live and
    not ours" while the handover asked only "not ours", so a historical resume of
    a session whose client had since gone showed no dialog (right — KTD8's words
    are "nothing live is stolen") and then told the operator a client was
    displaced anyway; and the picker asked the row's *freshness* class instead,
    so it warned about a steal on a row the confirmation correctly never fired
    for.

    A row can be not-ours and no longer live at the same time. A roster sweep
    that omits a session clears ``live_listed`` and deliberately leaves ``status``
    and ``waiting_kind`` saying what they last knew, so the stale wait is still
    there for a latch to read — which is why the asymmetry was reachable rather
    than theoretical.

    **Known limitation: ownership does not expire.** ``we_drive`` says this run
    once landed on the session, not that it still holds the transport. Another
    client can attach to a session this run drove, and a reconnect leaves every
    such row still reading ``we_drive``; in both shapes a genuine steal answers
    ``False`` here and shows neither dialog nor residue. This is not fixable from
    the data available — KTD8 records that rows carry no transport-identity
    field, which is also why the dialog's own copy says "may be attached to
    another client" rather than asserting it. Revisit if a roster row ever names
    its owning transport.
    """
    return (
        row.ownership != "we_drive"
        and row.live_listed
        and row.reclaimed_reason is None
        and not row.disconnected
    )
