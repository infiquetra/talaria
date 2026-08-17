"""The fleet's transport root: one live gateway connection per configured profile.

v0.4's KTD1 in one object. Talaria's connection inventory is the operator's
``[profiles.endpoints]`` map from ``config.toml`` **plus** the credential
file's own gateway as the default entry; with no config file that inventory is
the single default entry and the shipped v0.1–v0.3 behaviour is unchanged. A
:class:`ConnectionSet` owns one
:class:`~talaria.transport.source.LiveSource` per inventory entry, so each
connection carries its own credential, its own correlator and epoch, its own
reconnect schedule and its own probe state — and every frame leaves this seam
already stamped with the connection it crossed, so the domain router never has
to guess a frame's origin.

**Why N connections rather than one plus a ``profile`` parameter.** The gateway
routes session events to the one client transport that owns the session and
broadcasts nothing else, and a gateway process can only enumerate *its own*
sessions. So an app-global connection using ``session.create``'s ``profile``
parameter would still see neither the sessions other clients drive nor the
sessions living in another profile's gateway process. Reaching another
profile's fleet means dialling that profile's gateway. (Recorded in the v0.4
plan's KTD1, on the evidence of ``docs/analysis/2026-08-17-v0-4-topology-verification.md``.)

**Three rules keep the fleet honest, and each is enforced here rather than
documented.**

* **Failure is per connection.** One gateway that is down, refuses its
  credential, or never answers marks *that* connection and nothing else. It
  never stalls the merged frame stream, never delays another connection's
  dial, and never removes another connection's epoch bookkeeping.
* **A shared endpoint needs a shared credential.** Two configured profiles
  whose endpoints parse equal share one connection only when their resolved
  tokens are byte-identical. When they differ the launch refuses loud and names
  both profiles: silently picking one profile's credential for another
  profile's gateway is the failure this rule exists to prevent, and it would
  present as an authentication error against a gateway the operator never
  misconfigured.
* **A profile is never dialled with another profile's credential.** Each entry
  resolves its own ``[profiles.<name>]`` entry (KTD5). A profile with an
  endpoint and no credential renders ``credential_unavailable`` **by name**
  while every other connection stays up.
"""

from __future__ import annotations

import asyncio
import contextlib
import hmac
from collections.abc import AsyncIterator, Callable, Iterable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any, Final, Literal

from talaria.transport.attach import AttachTarget
from talaria.transport.credentials import (
    DEFAULT_PROFILE_NAME,
    CredentialError,
    CredentialProvider,
    LoopbackTokenProvider,
)
from talaria.transport.source import (
    FrameRecord,
    LiveConnectionState,
    LiveSource,
    TerminalCause,
)

if TYPE_CHECKING:  # pragma: no cover - types only
    from talaria.recorder.framelog import FrameRecorder, RecordedConnection
    from talaria.transport.attach import Dialer

__all__ = [
    "MAX_MERGED_FRAMES",
    "ConnectionConflictError",
    "ConnectionEntry",
    "ConnectionSet",
    "ConnectionSetError",
    "ConnectionStatus",
    "EnsureReason",
    "EnsureReport",
    "PlannedConnection",
    "TaggedFrame",
    "build_source_factory",
    "credential_provider_factory",
    "plan_connections",
    "recorded_connections",
    "resolve_connections",
]


class ConnectionSetError(Exception):
    """The connection inventory cannot be built as configured."""


class ConnectionConflictError(ConnectionSetError):
    """Two configured profiles claim one gateway with different credentials.

    Names both profiles and never either token. It is deliberately fatal to the
    launch rather than a per-connection refusal: whichever credential were
    picked would be presented to a gateway that did not mint it, and the
    operator would be debugging an authentication failure whose cause is two
    lines of their own configuration.
    """


#: How many merged frames may sit between the connections and their consumer
#: before the pumps stop pulling. Mirrors
#: :data:`~talaria.transport.source.MAX_QUEUED_FRAMES`: without a bound here,
#: a consumer slower than the fleet would drain every source's own bounded
#: queue into an unbounded one, and the per-connection backpressure that stops
#: a socket would never trip.
MAX_MERGED_FRAMES: Final[int] = 1_000

@dataclass(frozen=True)
class _ConnectionEnded:
    """One connection's stream is over; the merged stream's may not be.

    ``generation`` counts how many sources this profile has had. A stale
    marker — one left in the queue by a source :meth:`ConnectionSet.ensure`
    has already replaced — is ignored on that number, because acting on it
    would end the merged stream while the *replacement* connection is live and
    streaming.
    """

    profile: str
    generation: int


@dataclass(frozen=True)
class ConnectionEntry:
    """One connection the fleet holds, after endpoint and credential resolution.

    ``profile`` is the canonical identity: the name frames, registry rows and
    the recorder all carry. ``aliases`` are the other configured profile names
    that resolved to this same gateway with the same credential — they address
    the connection, they do not double it.

    ``credential_profile`` names the credential file entry this connection
    dials with: ``None`` is the file's top-level ``token`` (the default
    profile's entry), a string is ``[profiles.<name>].token``.

    ``problem`` is set for an entry that is known-unusable before anything is
    dialled — an endpoint that will not parse. Such an entry stays in the
    inventory, named, rather than disappearing: a profile the operator
    configured and Talaria silently dropped is the one failure mode R10 exists
    to prevent.
    """

    profile: str
    endpoint: str
    credential_profile: str | None = None
    aliases: tuple[str, ...] = ()
    problem: str = ""

    @property
    def names(self) -> tuple[str, ...]:
        """Every name that addresses this connection, canonical first."""
        return (self.profile, *self.aliases)

    @property
    def dialable(self) -> bool:
        return not self.problem and bool(self.endpoint)

    @property
    def safe_endpoint(self) -> str:
        """The endpoint in the form that may be shown, logged or recorded."""
        return AttachTarget.from_url(self.endpoint).safe_url if self.endpoint else ""


@dataclass(frozen=True)
class TaggedFrame:
    """One frame, with the identity of the connection it crossed (KTD1).

    The tag is applied here, at the seam, and not derived later from the
    frame's content — no gateway listing carries a profile field, so a stored
    session id seen on two connections is genuinely two different sessions and
    only the observing connection distinguishes them.

    ``epoch`` is the connection's correlator epoch at the moment the frame was
    taken off that connection's own queue, so a consumer can tell a frame from
    the socket it is talking to now from one that arrived on a socket since
    replaced.
    """

    profile: str
    record: FrameRecord
    epoch: int


@dataclass
class ConnectionStatus:
    """What is currently true of one connection, in terms fit for a screen.

    ``endpoint`` is the redacted form. ``detail`` is either a fixed sentence or
    a value already routed through the transport's own URL scrubbing, so this
    object never carries a credential or a whole endpoint into a render.
    """

    profile: str
    endpoint: str
    aliases: tuple[str, ...] = ()
    state: LiveConnectionState = "disconnected"
    detail: str = ""
    cause: TerminalCause | None = None
    failure_kind: str = ""
    epoch: int = 0
    #: Set when the entry could not be dialled at all as configured.
    problem: str = ""
    #: Whether this connection's stream has ended and will not resume without
    #: a fresh :meth:`ConnectionSet.ensure`.
    ended: bool = False

    @property
    def connected(self) -> bool:
        return self.state == "connected"


#: Why an :meth:`ConnectionSet.ensure` ended as it did. Every member is a state
#: the operator can be told about and act on; there is no silent outcome.
#:
#: * ``already_up`` — the connection was live; it stayed live and became home.
#: * ``connected`` — it was dialled now and answered.
#: * ``unknown_profile`` — no inventory entry answers to that name.
#: * ``refused_endpoint`` — the configured endpoint would not parse.
#: * ``credential_unavailable`` — no credential could be produced *for this
#:   profile*. Nothing was dialled, so no gateway formed an opinion.
#: * ``auth_failed`` — the gateway refused the credential.
#: * ``connect_failed`` — nothing answered at the endpoint.
#: * ``closed`` — the set is shut; ensure is not a way to reopen it.
EnsureReason = Literal[
    "already_up",
    "connected",
    "unknown_profile",
    "refused_endpoint",
    "credential_unavailable",
    "auth_failed",
    "connect_failed",
    "closed",
]


@dataclass(frozen=True)
class EnsureReport:
    """What ``/profiles <n>`` did, in terms the operator can act on.

    The vital property is what it does **not** report: no outcome here implies
    anything happened to another connection. An ensure that fails leaves every
    other gateway exactly as it was, which is the whole difference between this
    and the drop-switch it replaces.
    """

    profile: str
    reason: EnsureReason
    state: LiveConnectionState = "disconnected"
    detail: str = ""

    @property
    def ok(self) -> bool:
        return self.reason in {"already_up", "connected"}


# ── the inventory: planning, then credential-aware resolution ─────────────


@dataclass(frozen=True)
class PlannedConnection:
    """One inventory row before same-endpoint groups have been resolved.

    Separate from :class:`ConnectionEntry` because planning is pure — it reads
    configuration and parses URLs — while resolving reads credentials. Keeping
    the two apart is what lets the endpoint half be tested without a credential
    file and the credential half without a config file.
    """

    name: str
    endpoint: str
    credential_profile: str | None
    problem: str = ""

    @property
    def dialable_endpoint(self) -> bool:
        return not self.problem and bool(self.endpoint)


def _normalize(endpoint: str) -> tuple[str, str]:
    """``(usable url, problem)`` for one configured endpoint.

    The URL is routed through :class:`~talaria.transport.attach.AttachTarget`,
    which strips any credential-bearing query component, so two rows that
    differ only in a pasted ``?token=`` are recognised as one gateway rather
    than dialled twice. ``problem`` never contains the offending URL — an
    endpoint is exactly the string an operator pastes a token into.
    """
    target = AttachTarget.from_url(endpoint)
    return (target.url, target.problem)


def plan_connections(
    *,
    default_endpoint: str,
    config_endpoints: Mapping[str, str] | None = None,
    default_profile: str = DEFAULT_PROFILE_NAME,
) -> tuple[PlannedConnection, ...]:
    """The inventory before credentials are consulted (KTD1).

    Order is the credential file's own gateway first, then
    ``[profiles.endpoints]`` in config order. That order is what decides the
    canonical name of a shared connection, so it is fixed here rather than left
    to a dict's iteration accident.

    A config row naming the *default profile* does not add a second entry: it
    supplies that entry's endpoint, because ``config.toml`` is where endpoints
    live (KTD5) and the credential file's ``url`` key is only the fallback for
    a profile nobody configured.
    """
    rows = dict(config_endpoints or {})
    members: list[PlannedConnection] = []

    default_url, default_problem = _normalize(rows.pop(default_profile, default_endpoint))
    members.append(
        PlannedConnection(
            name=default_profile,
            endpoint=default_url,
            # Named, not ``None``.  The first draft passed ``None`` with a
            # comment reasoning that a provider for ``default`` would read
            # ``[profiles.default]`` first and fall back to the top-level pair
            # anyway — true of a *named* provider, and the whole point is that
            # ``None`` does not build one.  ``_resolve_profile`` was bypassed
            # entirely, so ``refresh-credential --profile default`` wrote an
            # entry no dial ever read: a successful-looking pairing followed by
            # auth failures against a stale top-level token.  Naming the
            # profile makes the documented one-way fallback actually apply.
            credential_profile=default_profile,
            problem=default_problem,
        )
    )

    for name, endpoint in rows.items():
        url, problem = _normalize(endpoint)
        members.append(
            PlannedConnection(name=name, endpoint=url, credential_profile=name, problem=problem)
        )
    return tuple(members)


async def _token_for(
    member: PlannedConnection, provider_for: Callable[[PlannedConnection], CredentialProvider]
) -> str | None:
    """This member's token, or ``None`` when it has none to compare.

    Acquired and immediately compared, never stored: KTD11's per-dial rule
    stands, and the value returned here lives only until
    :func:`resolve_connections` has answered "same or not".
    """
    try:
        credential = await provider_for(member).acquire()
    except CredentialError:
        # Not an error here. A profile with no credential simply cannot be
        # shown to share another's, so it does not fold — and it reports
        # ``credential_unavailable`` by name when it is dialled.
        return None
    return credential.value


async def resolve_connections(
    members: Sequence[PlannedConnection],
    provider_for: Callable[[PlannedConnection], CredentialProvider],
) -> tuple[ConnectionEntry, ...]:
    """Fold same-endpoint members into one connection each, or refuse loud.

    Two members share a connection only when both produce a credential and the
    two are byte-identical (compared with :func:`hmac.compare_digest`, so the
    comparison itself leaks nothing through timing). A pair that produces two
    *different* credentials for one gateway raises
    :class:`ConnectionConflictError` naming both profiles — see that class for
    why this is fatal rather than a per-connection refusal.

    A member that cannot produce a credential at all does not fold and does not
    refuse the launch: it becomes its own entry, is dialled, and reports
    ``credential_unavailable`` by name. That is KTD5's per-profile semantic
    isolation — one unpaired profile never costs the operator the rest of the
    fleet.
    """
    groups: dict[str, list[PlannedConnection]] = {}
    order: list[str] = []
    for member in members:
        key = member.endpoint if member.dialable_endpoint else f"\0unusable:{member.name}"
        if key not in groups:
            groups[key] = []
            order.append(key)
        groups[key].append(member)

    entries: list[ConnectionEntry] = []
    for key in order:
        group = groups[key]
        if len(group) == 1:
            entries.append(_entry_for(group[0]))
            continue

        tokens = [await _token_for(member, provider_for) for member in group]
        paired = [
            (member, token)
            for member, token in zip(group, tokens, strict=True)
            if token is not None
        ]
        if paired:
            canonical, canonical_token = paired[0]
            aliases: list[str] = []
            for member, token in paired[1:]:
                if not hmac.compare_digest(canonical_token, token):
                    raise ConnectionConflictError(
                        f"profiles {canonical.name!r} and {member.name!r} name one "
                        f"gateway ({AttachTarget.from_url(canonical.endpoint).safe_url}) "
                        "with two different credentials. Talaria will not pick one of "
                        "them for the other's gateway — give the two profiles separate "
                        "endpoints, or pair them against the same one"
                    )
                aliases.append(member.name)
            entries.append(_entry_for(canonical, aliases=tuple(aliases)))
            folded = {canonical.name, *aliases}
        else:
            folded = set()

        # Members with no credential of their own stay as their own entries so
        # each is named where it fails, rather than being reported under a
        # profile name whose credential it never had.
        for member in group:
            if member.name not in folded:
                entries.append(_entry_for(member))

    return tuple(entries)


def _entry_for(member: PlannedConnection, *, aliases: tuple[str, ...] = ()) -> ConnectionEntry:
    return ConnectionEntry(
        profile=member.name,
        endpoint=member.endpoint,
        credential_profile=member.credential_profile,
        aliases=aliases,
        problem=member.problem,
    )


# ── wiring helpers ────────────────────────────────────────────────────────


def credential_provider_factory(
    credentials_path: Path | None,
) -> Callable[[ConnectionEntry | PlannedConnection], CredentialProvider]:
    """A per-entry credential provider, bound to that entry's own file entry.

    Never one shared provider. Each profile's dashboard mints its own token,
    and a provider that has cached one value would carry it to the next
    gateway, where the refusal reads as an authentication problem rather than
    as "that credential was never for this gateway".

    No provider built here prompts. A fleet launch dials several gateways at
    once, so an interactive prompt would be issued once per unpaired profile,
    underneath an interface that cannot display any of them.
    """

    def build(entry: ConnectionEntry | PlannedConnection) -> CredentialProvider:
        return LoopbackTokenProvider(
            credentials_path=credentials_path,
            allow_prompt=False,
            profile=entry.credential_profile,
        )

    return build


def recorded_connections(
    entries: Sequence[ConnectionEntry],
) -> tuple[RecordedConnection, ...]:
    """The recorder's view of an inventory (KTD6).

    Built from the *resolved* entries, so two configured names that folded into
    one connection appear once, under the canonical name frames will actually
    carry — a ``connections`` header listing a profile no frame ever names
    would be a header that disagrees with its own log.
    """
    from talaria.recorder.framelog import RecordedConnection as _RecordedConnection

    return tuple(
        _RecordedConnection(profile=entry.profile, endpoint=entry.endpoint)
        for entry in entries
    )


def build_source_factory(
    *,
    provider_for: Callable[[ConnectionEntry], CredentialProvider],
    recorder: FrameRecorder | None = None,
    dialer: Dialer | None = None,
) -> Callable[[ConnectionEntry], LiveSource]:
    """Build the per-connection :class:`~talaria.transport.source.LiveSource`.

    The recorder handed to each source is a **view** onto one shared frame log
    (KTD6), bound to that connection's canonical profile name, so the frame's
    connection identity is stamped by the object that knows it rather than
    passed in by the source that would have to be trusted with it.
    """

    def build(entry: ConnectionEntry) -> LiveSource:
        return LiveSource(
            AttachTarget.from_url(entry.endpoint),
            provider_for(entry),
            dialer=dialer,
            recorder=recorder.view(entry.profile) if recorder is not None else None,
        )

    return build


# ── the set itself ────────────────────────────────────────────────────────


class ConnectionSet:
    """N concurrent gateway connections, merged into one tagged frame stream.

    Construction dials nothing. :meth:`start` opens every dialable entry
    concurrently and reports each outcome separately; :meth:`ensure` opens or
    re-opens exactly one and leaves the rest untouched, which is what
    ``/profiles`` now selects with.
    """

    def __init__(
        self,
        entries: Iterable[ConnectionEntry],
        source_factory: Callable[[ConnectionEntry], LiveSource],
        *,
        on_connection: Callable[[str, LiveConnectionState, str, TerminalCause | None], None]
        | None = None,
        max_merged_frames: int = MAX_MERGED_FRAMES,
        recorder: FrameRecorder | None = None,
    ) -> None:
        self._entries: tuple[ConnectionEntry, ...] = tuple(entries)
        self._factory = source_factory
        self._on_connection = on_connection
        self._max_merged_frames = max_merged_frames
        #: The shared frame log, held open for this set's whole lifetime.
        #:
        #: Passing it here is not duplication of what ``source_factory``
        #: already closes over: the factory hands each source a *view*, whose
        #: lifetime is one connection, and reconnecting a profile retires the
        #: old source before building the new one. Without a run-scoped hold
        #: that retire can be the last view, close the log, and leave the
        #: replacement writing to a closed file. Optional so a set with no
        #: recording, and every existing caller, is unaffected.
        self._recorder = recorder
        if recorder is not None:
            recorder.hold()

        self._by_name: dict[str, ConnectionEntry] = {}
        for entry in self._entries:
            for name in entry.names:
                self._by_name.setdefault(name, entry)

        self._sources: dict[str, LiveSource] = {}
        self._pumps: dict[str, asyncio.Task[None]] = {}
        self._locks: dict[str, asyncio.Lock] = {
            entry.profile: asyncio.Lock() for entry in self._entries
        }
        self._status: dict[str, ConnectionStatus] = {
            entry.profile: ConnectionStatus(
                profile=entry.profile,
                endpoint=entry.safe_endpoint,
                aliases=entry.aliases,
                problem=entry.problem,
            )
            for entry in self._entries
        }

        self._queue: asyncio.Queue[Any] = asyncio.Queue()
        self._pulls_allowed = asyncio.Event()
        self._pulls_allowed.set()
        #: Profile to the generation of the pump currently feeding the merged
        #: stream. A profile leaves this map when *its own* generation's end
        #: marker is drained, so a stale marker from a source already replaced
        #: by :meth:`ensure` cannot retire a live connection.
        self._running: dict[str, int] = {}
        #: How many sources each profile has had. Only ever increases.
        self._generation: dict[str, int] = {}
        #: Whether the fleet has been asked to run at all. Until it has,
        #: "nothing is running" is vacuously true and must not end the merged
        #: stream — a consumer may legitimately start iterating before
        #: :meth:`start`.
        self._ever_ran = False
        #: How many :meth:`ensure` calls are mid-flight. A connection being
        #: replaced is momentarily absent from :attr:`_running`, and without
        #: this the merged stream could observe that instant, conclude the
        #: fleet had ended, and stop — on the way *into* a working connection.
        self._settling = 0
        self._closed = False
        #: Which connection new sessions are created or resumed on. Set by a
        #: successful :meth:`ensure`; the first entry until one is made.
        self._home = self._entries[0].profile if self._entries else ""

        # Observable counters, for the same reason ``LiveSource``'s are public:
        # each is the only trace of a behaviour otherwise invisible from
        # outside.
        self.merge_pauses = 0
        self.peak_merged_frames = 0

    # ── shape ────────────────────────────────────────────────────────────

    @property
    def entries(self) -> tuple[ConnectionEntry, ...]:
        return self._entries

    @property
    def profiles(self) -> tuple[str, ...]:
        return tuple(entry.profile for entry in self._entries)

    @property
    def home(self) -> str:
        """Which connection the next ``session.create``/``session.resume`` uses."""
        return self._home

    @property
    def closed(self) -> bool:
        return self._closed

    @property
    def statuses(self) -> tuple[ConnectionStatus, ...]:
        return tuple(self._status[entry.profile] for entry in self._entries)

    def entry_for(self, profile: str) -> ConnectionEntry | None:
        """The entry a name addresses, canonical or alias."""
        return self._by_name.get(profile)

    def status_of(self, profile: str) -> ConnectionStatus | None:
        entry = self.entry_for(profile)
        return self._status[entry.profile] if entry is not None else None

    def source_for(self, profile: str) -> LiveSource | None:
        """The live source behind a name, or ``None`` when it is not up."""
        entry = self.entry_for(profile)
        return self._sources.get(entry.profile) if entry is not None else None

    @property
    def connected_profiles(self) -> tuple[str, ...]:
        return tuple(
            entry.profile
            for entry in self._entries
            if self._status[entry.profile].connected
        )

    # ── dialling ─────────────────────────────────────────────────────────

    async def start(self) -> tuple[EnsureReport, ...]:
        """Dial every entry concurrently; report each outcome on its own.

        Concurrent because serial dialling would make one unreachable gateway's
        connect timeout the launch latency of every gateway behind it in the
        inventory. Isolated because :meth:`ensure` never raises: a failure is a
        report, and the reports come back in inventory order however the dials
        interleaved.
        """
        if self._closed:
            return tuple(
                EnsureReport(entry.profile, "closed", detail="the connection set is closed")
                for entry in self._entries
            )
        reports = await asyncio.gather(
            *(self.ensure(entry.profile, make_home=False) for entry in self._entries)
        )
        # Set whatever the outcome: a start where *nothing* connected must end
        # the merged stream rather than park a consumer on a fleet that will
        # never speak, which is exactly what a single-connection ``LiveSource``
        # does when its one dial fails.
        self._ever_ran = True
        self._queue.put_nowait(_ConnectionEnded("", -1))
        for report in reports:
            if report.ok:
                self._home = report.profile
                break
        return tuple(reports)

    async def ensure(self, profile: str, *, make_home: bool = True) -> EnsureReport:
        """Bring one connection up if it is not, and make it the home (KTD1).

        This is what selecting a profile means in the fleet. It is deliberately
        **not** :meth:`~talaria.transport.source.LiveSource.switch_to_endpoint`,
        which retargets one socket and therefore drops whatever it was
        connected to: dropping a gateway in order to look at another is exactly
        the behaviour a fleet client must not have, since the connection just
        dropped is the only feed for that gateway's sessions.
        ``switch_to_endpoint`` remains as the lower-level primitive it always
        was, for a Talaria holding exactly one connection.

        An ensure of a connection already up costs nothing and re-reports
        ``already_up``; an ensure of one whose stream has ended builds a fresh
        source, because a :class:`~talaria.transport.source.LiveSource` that has
        given up is deliberately not restartable.
        """
        if self._closed:
            return EnsureReport(profile, "closed", detail="the connection set is closed")

        entry = self.entry_for(profile)
        if entry is None:
            return EnsureReport(
                profile,
                "unknown_profile",
                detail="no configured endpoint answers to that profile name",
            )
        if entry.problem:
            return EnsureReport(entry.profile, "refused_endpoint", detail=entry.problem)

        async with self._locks[entry.profile]:
            self._settling += 1
            try:
                return await self._ensure_locked(entry, make_home=make_home)
            finally:
                self._settling -= 1
                # Wake the merged stream, always — not only when this ensure
                # produced a pump. ``__aiter__`` re-tests the finish condition
                # only when an item arrives, and a *failed* ensure enqueues
                # nothing: if the last connection's end marker was drained
                # while this ensure held ``_settling`` above zero, the consumer
                # parked on an empty queue, and the finish condition became
                # true the instant ``_settling`` returned to zero — with
                # nothing left to wake it. That is a hang where the class
                # promises a named failure, and it is reachable from an
                # ordinary `/profiles <name>` against an unreachable gateway.
                # The sentinel generation answers to no connection, so this
                # retires nothing and only forces the re-check.
                #
                # Unconditional, deliberately. A ``not self._closed`` guard here
                # looked like tidiness and was a second hang: ``close`` enqueues
                # its own sentinel, a consumer can drain that one while this
                # ensure still holds the settling guard above zero, and then the
                # guard suppressed the only wake left — so the stream never
                # ended after a quit issued while a ``/profiles`` dial was in
                # flight. Enqueuing after close costs one inert item on a queue
                # nobody will read again.
                self._queue.put_nowait(_ConnectionEnded("", -1))

    async def _ensure_locked(
        self, entry: ConnectionEntry, *, make_home: bool
    ) -> EnsureReport:
        """:meth:`ensure`'s body, under this profile's lock and settling guard."""
        existing = self._sources.get(entry.profile)
        if existing is not None and existing.connected:
            if make_home:
                self._home = entry.profile
            return EnsureReport(entry.profile, "already_up", existing.state)
        if existing is not None:
            await self._retire(entry.profile)

        generation = self._generation.get(entry.profile, 0) + 1
        self._generation[entry.profile] = generation

        source = self._factory(entry)
        source.bind(on_connection=self._connection_observer(entry.profile))
        self._sources[entry.profile] = source
        self._status[entry.profile].ended = False

        state = await source.start()

        # The dial is the one await here long enough for ``close`` to land
        # underneath it, and ``close`` takes no per-profile lock — so by the
        # time a *successful* dial returns, this set may already be closed and
        # have run its whole teardown. Without this re-check the attach's open,
        # authenticated socket is stored on a source nothing will ever close
        # again: verified live against the stub gateway, where a ping over that
        # socket was still answered after ``close`` returned. R36 calls that
        # leak the thing teardown exists to prevent, and ``EnsureReason``'s own
        # contract says an ensure is not a way to reopen a closed set.
        if self._closed:
            await self._retire(entry.profile)
            return EnsureReport(
                entry.profile,
                "closed",
                state,
                detail="the connection set closed while this dial was in flight",
            )

        self._refresh_status(entry.profile)
        if state == "connected":
            self._ever_ran = True
            self._running[entry.profile] = generation
            self._pumps[entry.profile] = asyncio.create_task(
                self._pump(entry.profile, source, generation)
            )
            if make_home:
                self._home = entry.profile
            return EnsureReport(entry.profile, "connected", state)

        # A dial that did not connect: the source has already named the
        # failure kind and the detail. This restates them rather than
        # inventing a parallel vocabulary, and touches no other connection.
        await self._retire(entry.profile)
        reason: EnsureReason
        if source.failure_kind == "credential_unavailable":
            reason = "credential_unavailable"
        elif source.failure_kind == "auth_failed" or state == "auth_failed":
            reason = "auth_failed"
        else:
            reason = "connect_failed"
        return EnsureReport(entry.profile, reason, state, source.last_failure)

    def _connection_observer(
        self, profile: str
    ) -> Callable[[LiveConnectionState, str, TerminalCause | None], None]:
        def observe(
            state: LiveConnectionState, detail: str, cause: TerminalCause | None
        ) -> None:
            status = self._status[profile]
            status.state = state
            status.detail = detail
            status.cause = cause
            source = self._sources.get(profile)
            if source is not None:
                status.failure_kind = source.failure_kind
                status.epoch = source.epoch
            if cause is not None:
                status.ended = True
            if self._on_connection is not None:
                self._on_connection(profile, state, detail, cause)

        return observe

    def _refresh_status(self, profile: str) -> None:
        source = self._sources.get(profile)
        status = self._status[profile]
        if source is None:
            return
        status.state = source.state
        status.failure_kind = source.failure_kind
        status.detail = source.last_failure
        status.epoch = source.epoch

    async def _retire(self, profile: str) -> None:
        """Close and forget one connection's source, leaving every other alone."""
        pump = self._pumps.pop(profile, None)
        if pump is not None and not pump.done():
            pump.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await pump
        # Dropped here rather than left for the end marker to clear: this
        # profile has no pump any more whatever the marker says, and a
        # ``_running`` entry nothing will ever retire is a merged stream that
        # waits forever on a connection that is gone.
        self._running.pop(profile, None)
        source = self._sources.pop(profile, None)
        if source is not None:
            await source.close()
        self._refresh_status(profile)
        self._status[profile].ended = True

    # ── the merged stream ────────────────────────────────────────────────

    async def _pump(self, profile: str, source: LiveSource, generation: int) -> None:
        """Move one connection's frames onto the merged queue, tagged.

        Pauses on :attr:`_pulls_allowed` rather than pushing regardless, so a
        consumer slower than the fleet backs pressure all the way down to each
        socket. Without it the sources would drain their own bounded queues
        into an unbounded one and their read-pause would never fire.

        The end marker is written in a ``finally`` so it is written on
        cancellation too — a pump cancelled by :meth:`_retire` must still
        release the merged stream's bookkeeping, or a consumer would wait
        forever on a connection nobody is feeding.
        """
        try:
            async for record in source:
                await self._pulls_allowed.wait()
                # ``arrival_epoch``, not ``epoch``: the frame's origin socket,
                # not whichever socket is current by the time it is pumped.
                self._queue.put_nowait(TaggedFrame(profile, record, source.arrival_epoch))
                self.peak_merged_frames = max(self.peak_merged_frames, self._queue.qsize())
                if (
                    self._pulls_allowed.is_set()
                    and self._queue.qsize() >= self._max_merged_frames
                ):
                    self._pulls_allowed.clear()
                    self.merge_pauses += 1
        finally:
            self._queue.put_nowait(_ConnectionEnded(profile, generation))

    async def __aiter__(self) -> AsyncIterator[TaggedFrame]:
        """Yield every connection's frames in native arrival order.

        Native order is the whole recording contract (KTD6): one log, one
        interleaving, no cross-log merge rule for replay to reproduce.

        One gateway going away ends its own pump and nothing else — the merged
        stream keeps yielding the others, which is the isolation this class
        exists for. The merged stream itself ends only once every connection
        that ever ran has ended and none is running, matching what a
        single-connection Talaria does when its one source ends.

        **Total loss is terminal, and deliberately so.** Once the last
        connection has gone the stream ends, and a later :meth:`ensure` does
        not revive it — a consumer's ``async for`` has already returned by
        then. That is the shipped single-connection contract, unchanged: a
        launch where nothing answered must reach the consumer as a stream that
        ended, not as one that never speaks, or a dead gateway presents as a
        hang instead of as a named failure.
        """
        while True:
            if self._finished():
                return
            item = await self._queue.get()
            if isinstance(item, _ConnectionEnded):
                if self._running.get(item.profile) == item.generation:
                    del self._running[item.profile]
                    self._status[item.profile].ended = True
                if self._finished():
                    return
                continue
            if not self._pulls_allowed.is_set() and (
                self._queue.qsize() <= self._max_merged_frames // 2
            ):
                self._pulls_allowed.set()
            yield item

    def _finished(self) -> bool:
        """Whether the merged stream has nothing left to wait for.

        Three conditions, and each rules out a different way of being wrong:
        nothing is running, nothing is mid-:meth:`ensure` (a connection being
        replaced is momentarily absent from both), and the fleet has either
        been closed or been asked to run at least once — so a consumer that
        starts iterating before :meth:`start` waits rather than concluding the
        fleet is over before it began.
        """
        if self._running or self._settling:
            return False
        return self._closed or self._ever_ran

    # ── teardown ─────────────────────────────────────────────────────────

    async def close(self) -> None:
        """Stop every connection this set started. Idempotent (R36).

        A close that raises would leave the remaining connections open — the
        exact leak R36 exists to prevent — so each retirement is contained and
        the loop always reaches the end of the inventory.
        """
        if self._closed:
            return
        self._closed = True
        self._pulls_allowed.set()
        for profile in list(self._sources):
            with contextlib.suppress(Exception):
                await self._retire(profile)
        # Wakes a consumer parked on an empty queue with nothing running: the
        # generation is deliberately one nothing answers to, so the marker
        # retires no connection and only re-checks the finish condition.
        self._queue.put_nowait(_ConnectionEnded("", -1))
        # Dropped last, after every source has retired its own view, so the log
        # records the whole run and closes exactly once.
        if self._recorder is not None:
            self._recorder.release_hold()
