"""KTD2's poll cadence, driven at last (v0.4 U8B).

``next_poll_due_at`` has carried the whole rule since U3 — a 2-second coalesce
after a ``sessions.changed`` hint, a 30-second backstop — and until this unit
nothing in ``talaria/`` called it. A gateway announcing that its session list had
changed was heard and not acted on.

The property these tests exist for, and the one the cadence was measured on the
wrong clock to have: **a background connection's own silence brings its poll
due.** That is what a backstop is. Measured on ``state.last_observed_at`` — the
focused session's frame clock — it could not, because only focused traffic
advances that clock, so connection A's schedule was a function of traffic on
connection B and a fleet quiet everywhere never polled at all.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import replace
from typing import Any

import pytest

from talaria.domain.registry import POLL_BACKSTOP_S, POLL_COALESCE_S, ConnectionChannel
from talaria.transport.rpc import RpcOutcome
from talaria.ui.app import TalariaApp

from .conftest import live_app

HOME = "home-gateway"
AWAY = "away-gateway"


APPROVAL_PENDING = "approval.pending"
LIST_ACTIVE = "session.active_list"


class CountingDispatcher:
    """Answers from a scripted table and remembers what it was asked, verbatim."""

    def __init__(self) -> None:
        self.calls: list[str] = []
        self.params: list[Any] = []
        self.results: dict[str, Any] = {}
        #: Methods this gateway refuses outright, so a caller sees an
        #: unconfirmed outcome rather than a reply it can decode.
        self.refuse: set[str] = set()
        #: Methods that never left the client at all — the one delivery that is
        #: DEFINITE about non-delivery and therefore the only one that restores.
        self.never_sent: set[str] = set()

    async def call(
        self, method: str, params: Any = None, *, timeout: float | None = None
    ) -> RpcOutcome:
        self.calls.append(method)
        if method == APPROVAL_PENDING:
            self.params.append(params)
        if method in self.never_sent:
            from talaria.transport.rpc import NOT_CONNECTED

            return RpcOutcome(
                status="unknown",
                method=method,
                request_id="1",
                epoch=1,
                reason=NOT_CONNECTED,
            )
        if method in self.refuse:
            return RpcOutcome(
                status="error", method=method, request_id="1", epoch=1, error_message="refused"
            )
        return RpcOutcome(
            status="ok",
            method=method,
            request_id="1",
            epoch=1,
            result=self.results.get(method, {}),
        )


class FleetInventory:
    """A :class:`~talaria.ui.app.ConnectionInventory` double."""

    def __init__(self, sources: Mapping[str, CountingDispatcher]) -> None:
        self._sources = dict(sources)

    @property
    def profiles(self) -> tuple[str, ...]:
        return tuple(self._sources)

    def source_for(self, profile: str) -> CountingDispatcher | None:
        return self._sources.get(profile)

    def status_of(self, profile: str) -> None:
        return None


def fleet_app(now: float) -> tuple[TalariaApp, dict[str, CountingDispatcher]]:
    """A live two-connection app whose wall clock the test controls."""
    sources = {HOME: CountingDispatcher(), AWAY: CountingDispatcher()}
    app = live_app(sources[HOME], connections=FleetInventory(sources))
    # ``fleet_profile`` reads the fleet's focused profile, so it moves with the
    # fleet rather than being set beside it.
    app.fleet = replace(app.fleet, focused_profile=HOME)
    assert app.fleet_profile == HOME
    return app, sources


def with_channel(app: TalariaApp, profile: str, **fields: Any) -> None:
    channel = app.fleet.channels.get(profile) or ConnectionChannel(profile=profile)
    app.fleet = replace(
        app.fleet, channels={**app.fleet.channels, profile: replace(channel, **fields)}
    )


@pytest.mark.asyncio
async def test_a_background_connections_own_silence_brings_its_poll_due(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The property the whole cadence exists for, and the clock split serves.

    Nothing has happened anywhere. The focused session's frame clock is frozen at
    zero — no focused traffic has ever arrived — and the background connection
    was last polled just over the backstop ago. It must be swept.

    Under the frame clock this cannot pass, and that is the point: measured
    there, ``now`` is 0.0, the connection is due at ``last_poll_at + 30``, and no
    amount of waiting moves the comparison. Pinned as a *silence* test rather
    than a traffic test for exactly that reason.
    """
    import talaria.ui.app as app_module

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)

    with_channel(app, AWAY, last_poll_at=now - POLL_BACKSTOP_S - 1.0)
    with_channel(app, HOME, last_poll_at=now)
    assert app.state.last_observed_at == 0.0, "the frame clock is not frozen; test is stale"

    async with app.run_test() as pilot:
        await app.poll_due_connections()
        await pilot.pause()

    assert "session.list" in sources[AWAY].calls, (
        "a background connection went unpolled through its whole backstop while "
        "the fleet was silent — which is when a poll is the only way to learn "
        "anything"
    )
    assert "session.list" not in sources[HOME].calls, (
        "the connection polled a moment ago was swept again"
    )


@pytest.mark.asyncio
async def test_a_connection_polled_within_the_backstop_is_left_alone(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The other half: the cadence is a floor, not a licence to poll every tick.

    The timer fires once a second. Without the due-check that would be a poll per
    second per connection — thirty times KTD2's backstop.
    """
    import talaria.ui.app as app_module

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)

    with_channel(app, AWAY, last_poll_at=now - POLL_BACKSTOP_S + 1.0)
    with_channel(app, HOME, last_poll_at=now)

    async with app.run_test() as pilot:
        for _ in range(5):
            await app.poll_due_connections()
        await pilot.pause()

    assert sources[AWAY].calls == [], (
        f"five ticks inside one backstop window produced {sources[AWAY].calls}"
    )


@pytest.mark.asyncio
async def test_a_sessions_changed_hint_coalesces_rather_than_polling_per_event(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """KTD2's other half: a hint polls soon, not immediately and not per event.

    ``route_frame`` records ``hint_at`` on every ``sessions.changed``. A gateway
    that renames five sessions sends five of them; the coalesce window is what
    turns that into one poll.
    """
    import talaria.ui.app as app_module

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)

    # Polled a moment ago, so the backstop is nowhere near — the hint is the
    # only thing that could make this connection due.
    with_channel(app, AWAY, last_poll_at=now - 0.5, hint_at=now)
    with_channel(app, HOME, last_poll_at=now)

    async with app.run_test() as pilot:
        await app.poll_due_connections()
        await pilot.pause()
        assert sources[AWAY].calls == [], (
            "the hint polled immediately; the coalesce window did nothing"
        )

        monkeypatch.setattr(app_module, "_wall_clock", lambda: now + POLL_COALESCE_S)
        await app.poll_due_connections()
        await pilot.pause()

    assert "session.list" in sources[AWAY].calls, (
        "the hint never produced a poll at all, so a gateway announcing a changed "
        "session list is still being heard and not acted on"
    )


@pytest.mark.asyncio
async def test_the_focused_connection_is_polled_without_a_second_probe_path(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """``hint_at`` is recorded for every connection, so every connection is polled.

    But the focused one gets the roster sweep alone: its seams belong to
    ``verify_gateway``, and a probe round issued here would carry an empty
    session id that connection never asked for.
    """
    import talaria.ui.app as app_module

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)

    with_channel(app, HOME, last_poll_at=now - POLL_BACKSTOP_S - 1.0)
    with_channel(app, AWAY, last_poll_at=now)

    async with app.run_test() as pilot:
        await app.poll_due_connections()
        await pilot.pause()

    assert "session.list" in sources[HOME].calls, "the focused connection was never polled"
    assert not any(call.startswith("approval.") for call in sources[HOME].calls), (
        "the poll loop opened a second probe path on the focused connection"
    )


@pytest.mark.asyncio
async def test_a_connection_never_polled_is_due_at_once(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A connection with no channel yet has never been polled, so it is due.

    Reached without constructing a row for it first, which matters because the
    fleet learns channels from traffic and a connection that has sent none has
    no channel to read a stamp from.
    """
    import talaria.ui.app as app_module

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)
    assert AWAY not in app.fleet.channels, "the precondition no longer holds"

    async with app.run_test() as pilot:
        await app.poll_due_connections()
        await pilot.pause()

    assert "session.list" in sources[AWAY].calls


#  ── feed B: the approvals of sessions someone else drives ─────────────────


def _prompt(*, request_id: str, session_id: str) -> Any:
    """A minimal approval prompt, as the card path would hand one to the latch."""
    from talaria.domain.models import PendingPrompt

    return PendingPrompt(
        kind="approval",
        request_id=request_id,
        summary="rm -rf /data",
        session_id=session_id,
        opened_at=1.0,
        seq=1,
    )


def waiting_row(profile: str, durable: str, runtime: str | None) -> Any:
    """One foreign session the gateway reports as waiting on an approval."""
    from talaria.domain.registry import RegistryRow

    return RegistryRow(
        profile=profile,
        durable_id=durable,
        runtime_ids=(runtime,) if runtime else (),
        status="waiting",
        waiting_kind="unobserved",
        live_listed=True,
    )


def present_board(profile: str) -> Any:
    """A board whose roster and approval-detail seams both answered present.

    Built the same way ``tests/domain/test_needs_you_queue.py``'s ``probed``
    helper builds one, rather than by a second route — a board constructed
    differently here would be testing this file's idea of a board.
    """
    from talaria.domain.compat import empty_board

    board = empty_board(profile)
    resolved = {"roster": "present", "approval-detail": "present"}
    return replace(
        board,
        observations=tuple(
            replace(
                observation,
                status=resolved.get(observation.seam, observation.status),  # type: ignore[arg-type]
                source="probe" if observation.seam in resolved else observation.source,
                observed_at=1.0,
            )
            for observation in board.observations
        ),
        last_round_at=1.0,
    )


def with_detail_seam(app: TalariaApp, profile: str, rows: dict[Any, Any]) -> None:
    """A connection whose approval-detail seam answered present, holding ``rows``.

    Binds each row's runtime ids as aliases too, because ``_bind_alias`` keeps
    the two in step in production and ``apply_approval_pending`` resolves what
    the wire was told — a runtime id — back to the durable key through exactly
    that table. A fixture that set ``runtime_ids`` alone would make the fold
    return unchanged and the test would be measuring its own omission.
    """
    aliases = {
        (profile, runtime): key
        for key, row in rows.items()
        for runtime in row.runtime_ids
    }
    app.fleet = replace(
        app.fleet,
        rows={**app.fleet.rows, **rows},
        aliases={**app.fleet.aliases, **aliases},
        seam_boards={**app.fleet.seam_boards, profile: present_board(profile)},
    )


@pytest.mark.asyncio
async def test_the_detail_call_names_the_runtime_id_the_gateway_can_resolve(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The wire wants the RUNTIME id; the fold files under the durable one.

    ``approval.pending`` resolves ``params["session_id"]`` against the gateway's
    ``_sessions`` mapping, which is keyed by runtime id — the durable id lives
    there as a field. Naming the durable id answers ``4001 session not found``,
    and ``apply_approval_pending`` reads a non-answering reply as "changes
    nothing", so this would have failed completely silently: an empty queue and
    a code path that looked alive.
    """
    import talaria.ui.app as app_module

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)
    durable = "20260806_120031_fd736f"
    with_detail_seam(app, AWAY, {(AWAY, durable): waiting_row(AWAY, durable, "99dcf142")})

    async with app.run_test() as pilot:
        await app.poll_approval_detail(AWAY, sources[AWAY])
        await pilot.pause()

    assert sources[AWAY].params == [{"session_id": "99dcf142"}], (
        f"the detail call named {sources[AWAY].params}, which the gateway cannot "
        "resolve — it keys sessions by runtime id"
    )


@pytest.mark.asyncio
async def test_no_approval_detail_is_asked_for_a_session_outside_the_trigger_statuses(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Rider one, pinned: the restriction that makes a build-warming call safe.

    ``approval.pending`` resolves through ``_sess``, which warms the session's
    agent build (``tui_gateway/server.py:2519``). That is the side effect the
    bare presence probe exists to avoid, and a call naming a session cannot
    avoid it. What makes it acceptable is that only sessions the gateway already
    reports as ``waiting`` or ``working`` are asked about — those are already
    built. An ``idle`` session would be BUILT in order to discover it has nothing
    to say, on someone else's machine.

    Asserted as "no call at all", not "a call that returns nothing".
    """
    import talaria.ui.app as app_module

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)
    idle = waiting_row(AWAY, "s-idle", "run-idle")
    idle = replace(idle, status="idle", waiting_kind="")
    with_detail_seam(app, AWAY, {(AWAY, "s-idle"): idle})

    async with app.run_test() as pilot:
        await app.poll_approval_detail(AWAY, sources[AWAY])
        await pilot.pause()

    assert sources[AWAY].calls == [], (
        "an idle session was asked for approval detail, which builds its agent "
        "to find out it has nothing to say"
    )


@pytest.mark.asyncio
async def test_a_foreign_approval_reaches_the_queue_through_a_poll(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The unit's goal, end to end: someone else's wait becomes our queue item."""
    import talaria.ui.app as app_module
    from talaria.domain.state import fleet_queue

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)
    with_detail_seam(app, AWAY, {(AWAY, "s-away"): waiting_row(AWAY, "s-away", "run-away")})
    sources[AWAY].results[APPROVAL_PENDING] = {
        "approvals": [
            {
                "request_id": "gw-7",
                "description": "rm -rf /data",
                "choices": ["once", "deny"],
                "requested_at": now - 40.0,
            }
        ]
    }

    async with app.run_test() as pilot:
        await app.poll_approval_detail(AWAY, sources[AWAY])
        await pilot.pause()

    detail = app.fleet.approval_detail
    assert (AWAY, "s-away") in detail, (
        "the reply was folded under a key the fold could not resolve; the runtime "
        "id must resolve through the row's alias to the durable key"
    )
    queue = fleet_queue(app.fleet)
    assert any(item.kind == "approval" for item in queue.items), (
        "a foreign session waiting on an approval is still not in the queue"
    )


@pytest.mark.asyncio
async def test_a_malformed_detail_reply_is_not_read_as_no_approvals(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """R10's fabricated zero, at the feed's own boundary."""
    import talaria.ui.app as app_module

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)
    with_detail_seam(app, AWAY, {(AWAY, "s-away"): waiting_row(AWAY, "s-away", "run-away")})
    sources[AWAY].results[APPROVAL_PENDING] = {
        "approvals": [{"request_id": "gw-7", "description": "rm -rf /data"}]
    }

    async with app.run_test() as pilot:
        await app.poll_approval_detail(AWAY, sources[AWAY])
        await pilot.pause()
        known = dict(app.fleet.approval_detail)
        assert known, "the precondition did not hold: nothing was learned to lose"

        sources[AWAY].results[APPROVAL_PENDING] = {"not-approvals": "garbage"}
        await app.poll_approval_detail(AWAY, sources[AWAY])
        await pilot.pause()

    assert app.fleet.approval_detail == known, (
        "a malformed reply cleared a known approval, which is the fabricated zero "
        "that makes an empty queue mean 'we could not ask'"
    )


def test_every_due_row_has_a_wire_handle() -> None:
    """The invariant that replaced a notice the operator ruled for, and why.

    The ruling was that a row with no runtime id must be NAMED in the queue
    rather than silently skipped — R14's rule, correctly applied to the first
    version of ``approval_detail_targets``, which dropped such rows.

    That version was wrong twice over. ``_bind_alias`` records nothing when the
    runtime id EQUALS the durable id (``state.py:2794``), because there is no
    alias to make — so an empty ``runtime_ids`` means the two coincide and the
    key is the handle, not that no handle exists. Dropping those rows removed
    real, askable sessions from the feed; a fixture in
    ``tests/domain/test_needs_you_queue.py`` builds exactly that shape and caught
    it.

    With the fallback in place the disclosed state became unreachable, so the
    notice would have been a fixture-only path wearing R14's clothes.
    ``RegistryRow.status`` is written in exactly one place (``state.py:3408``,
    inside ``apply_active_list``), which binds the alias in the same fold — so a
    row this gate admits has always been seen in an active list. The guarantee is
    asserted here instead of announced there; if it ever breaks, this fails and
    the notice is the right answer again.
    """
    from talaria.domain.state import FleetState, approval_detail_targets, wire_handle

    board = present_board(AWAY)
    coincident = waiting_row(AWAY, "s-same", None)
    aliased = waiting_row(AWAY, "s-diff", "run-diff")
    rows = {(AWAY, "s-same"): coincident, (AWAY, "s-diff"): aliased}
    fleet = replace(
        FleetState(focused_profile=HOME), rows=rows, seam_boards={AWAY: board}
    )

    assert wire_handle((AWAY, "s-same"), coincident) == "s-same", (
        "a row whose runtime id equals its durable id has the key as its handle"
    )
    assert wire_handle((AWAY, "s-diff"), aliased) == "run-diff"

    targets = approval_detail_targets(fleet, AWAY)
    assert len(targets) == len(rows), (
        f"a due row was dropped from the feed with no handle: {targets}"
    )
    assert all(target for target in targets), "a due row produced an empty wire handle"


#  ── the three probes the mutation pass said these pins owed ────────────────


@pytest.mark.asyncio
async def test_the_newest_runtime_id_is_named_after_a_resume(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """M56's probe: a row with ONE runtime id cannot tell newest from oldest.

    A resumed session has several. The gateway remints the runtime id on every
    resume and the old one is gone from ``_sessions`` — two local recordings show
    the same durable session answering ``99dcf142`` and then ``fd0367c9`` — so
    naming anything but the newest answers 4001 and this feed goes quiet.
    """
    import talaria.ui.app as app_module

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)
    row = waiting_row(AWAY, "20260806_120031_fd736f", None)
    # Oldest first, exactly as ``_bind_alias`` keeps them.
    row = replace(row, runtime_ids=("99dcf142", "fd0367c9"))
    with_detail_seam(app, AWAY, {(AWAY, "20260806_120031_fd736f"): row})

    async with app.run_test() as pilot:
        await app.poll_approval_detail(AWAY, sources[AWAY])
        await pilot.pause()

    assert sources[AWAY].params == [{"session_id": "fd0367c9"}], (
        "the feed named a runtime id the gateway retired at the last resume"
    )


@pytest.mark.asyncio
async def test_a_refused_detail_call_leaves_what_was_known_alone(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """M59's probe: the transport guard, not the decoder's.

    The malformed-reply test beside this one exercises a reply that ARRIVED and
    was garbage, which ``decode_pending_approvals`` turns into ``answered=False``
    — so the fold declines it and the call site's own ``confirmed`` check is
    never reached. This drives the other path: a call that did not answer at all.

    **This test cannot distinguish WHERE that is enforced, and does not claim
    to.** Deleting the call site's ``confirmed`` guard leaves it green, because
    an unconfirmed ``RpcOutcome`` carries ``result=None`` by construction
    (``talaria/transport/rpc.py:332-388`` passes ``result=`` only on the ``ok``
    branch) and the decoder declines ``None`` anyway. That is a structural proof
    of equivalence, recorded at the guard itself. What this pins is the
    end-to-end property — a refused call never costs us a known approval — which
    is worth a test whichever layer keeps it true.
    """
    import talaria.ui.app as app_module

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)
    with_detail_seam(app, AWAY, {(AWAY, "s-away"): waiting_row(AWAY, "s-away", "run-away")})
    sources[AWAY].results[APPROVAL_PENDING] = {
        "approvals": [{"request_id": "gw-7", "description": "rm -rf /data"}]
    }

    async with app.run_test() as pilot:
        await app.poll_approval_detail(AWAY, sources[AWAY])
        await pilot.pause()
        known = dict(app.fleet.approval_detail)
        assert known, "the precondition did not hold: nothing was learned to lose"

        sources[AWAY].refuse.add(APPROVAL_PENDING)
        await app.poll_approval_detail(AWAY, sources[AWAY])
        await pilot.pause()

    assert app.fleet.approval_detail == known, (
        "a refused detail call cleared a known approval — the fabricated zero R10 "
        "forbids, reached through the transport rather than the decoder"
    )


@pytest.mark.asyncio
async def test_detail_is_asked_after_the_roster_that_decides_who_is_due(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """M60's probe: the ordering inside the poll round, which nothing drove.

    Which sessions are due is a function of the statuses the roster sweep just
    refreshed. Asking first aims the detail call at the previous round's picture
    of the fleet — a session that stopped waiting is still asked, and one that
    just started waiting is not.

    Driven through ``poll_due_connections`` rather than by calling the two halves
    in order, because the order under test is the one that method chooses.
    """
    import talaria.ui.app as app_module

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)
    with_detail_seam(app, AWAY, {(AWAY, "s-away"): waiting_row(AWAY, "s-away", "run-away")})
    with_channel(app, HOME, last_poll_at=now)
    # The sweep must find the row still there, or it retires and nothing is due.
    sources[AWAY].results["session.list"] = {
        "sessions": [{"id": "s-away", "title": "someone else's", "status": "waiting"}]
    }
    sources[AWAY].results[LIST_ACTIVE] = {
        "sessions": [{"id": "run-away", "session_key": "s-away", "status": "waiting"}]
    }

    async with app.run_test() as pilot:
        await app.poll_due_connections()
        await pilot.pause()

    calls = sources[AWAY].calls
    assert "session.list" in calls, f"the roster was never swept: {calls}"
    assert APPROVAL_PENDING in calls, (
        f"no detail call was made, so this test asserts nothing about order: {calls}"
    )
    assert calls.index("session.list") < calls.index(APPROVAL_PENDING), (
        f"detail was asked before the roster that decides who is due: {calls}"
    )


#  ── the settle latch: the queue's copy follows the card's ──────────────────


@pytest.mark.asyncio
async def test_an_ambiguous_answer_latches_the_polled_copy_too(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """AE2's fleet leg: ``settled_items`` stops being permanently empty.

    ``settle_prompt`` clears the focused registry, which removes feed A's row.
    Feed B's copy is derived from ``FleetState.approval_detail`` and survives
    until the gateway stops listing it — which, for an answer whose outcome
    Talaria could not read, may be never. Re-offering an approval that may
    already have been answered is the worst retry available (R18).

    Invisible before U8B: ``approval_detail`` was permanently empty, so there
    was no second copy to leave behind.
    """
    import talaria.ui.app as app_module
    from talaria.domain.state import fleet_queue

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)
    with_detail_seam(app, HOME, {(HOME, "s-mine"): waiting_row(HOME, "s-mine", "run-mine")})
    sources[HOME].results[APPROVAL_PENDING] = {
        "approvals": [
            {"request_id": "gw-9", "description": "rm -rf /data", "choices": ["once", "deny"]}
        ]
    }

    async with app.run_test() as pilot:
        await app.poll_approval_detail(HOME, sources[HOME])
        await pilot.pause()
        before = fleet_queue(app.fleet)
        assert any(item.request_key == "gw-9" for item in before.items), (
            "the precondition did not hold: no polled copy to leave behind"
        )

        app.fleet = replace(
            app.fleet,
            settled_items=(),
        )
        app._latch_queue_copy(
            _prompt(request_id="gw-9", session_id="run-mine")
        )
        await pilot.pause()

    assert app.fleet.settled_items, "nothing was latched, so settled_items is still empty"
    after = fleet_queue(app.fleet)
    assert not any(item.request_key == "gw-9" for item in after.items), (
        "the polled copy survived the answer and is still offered"
    )


def test_the_latch_is_filed_under_the_key_the_queue_builds() -> None:
    """The identity space, which is the durable one — not whatever id was answered.

    ``respond_live`` is given the id the prompt carries, which for a resumed
    session is a runtime id. ``build_queue`` keys both feeds from ``row_key[1]``.
    A latch filed under the runtime id names an item the queue never builds, so
    it would silently latch nothing and the control would stay offered.
    """
    from talaria.domain.state import FleetState, settle_queue_item

    rows = {(AWAY, "s-durable"): waiting_row(AWAY, "s-durable", "run-1")}
    fleet = replace(
        FleetState(focused_profile=AWAY),
        rows=rows,
        aliases={(AWAY, "run-1"): (AWAY, "s-durable")},
    )

    latched = settle_queue_item(
        fleet, profile=AWAY, session_id="run-1", request_key="gw-9", at=1.0
    )
    assert latched.settled_items == ((AWAY, "s-durable", "gw-9"),), (
        f"the latch was filed under {latched.settled_items}, which the queue never builds"
    )


@pytest.mark.asyncio
async def test_a_restored_prompt_keeps_its_queue_row(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The carve-out that makes the latch's placement load-bearing.

    ``not_sent`` is the one outcome definite about non-delivery, so
    ``restore_prompt`` puts the control back and the operator can answer again.
    Latching there would tombstone a question that is still live: the card would
    offer a control the queue had already written off, and the item would never
    return however long it waited.

    A mutation adding ``_latch_queue_copy`` to the restore branch survived every
    other test in this file and in ``test_prompts.py``. This is the probe it
    owed. Driven through ``respond_live`` rather than asserted about the source,
    because the claim is about which branch runs.
    """
    import talaria.ui.app as app_module

    from .conftest import event, feed, settle

    now = 10_000.0
    monkeypatch.setattr(app_module, "_wall_clock", lambda: now)
    app, sources = fleet_app(now)
    sources[HOME].never_sent.add("clarify.respond")

    async with app.run_test() as pilot:
        feed(app, event("clarify.request", {"request_id": "c-1", "question": "which branch?"}))
        await settle(app, pilot)
        assert app.state.prompt_for("c-1", app.state.focused_session_id) is not None, (
            "the precondition did not hold: no live prompt to restore"
        )

        await app.respond_live("c-1", "main")
        await settle(app, pilot)

        assert app.state.prompt_for("c-1", app.state.focused_session_id) is not None, (
            "the prompt was not restored, so this test is not exercising that branch"
        )

    assert app.fleet.settled_items == (), (
        "a restored prompt was latched as settled — its queue row is tombstoned "
        "while its control is back on the card, so it can never return"
    )
