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


class CountingDispatcher:
    """Answers every call emptily and remembers which methods it was asked."""

    def __init__(self) -> None:
        self.calls: list[str] = []

    async def call(
        self, method: str, params: Any = None, *, timeout: float | None = None
    ) -> RpcOutcome:
        self.calls.append(method)
        return RpcOutcome(
            status="ok", method=method, request_id="1", epoch=1, result={}
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
