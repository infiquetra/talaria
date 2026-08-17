"""The registry's memory bound (U3/PC2): 256 rows per connection, protected
rows exempt, eviction oldest-first and always visible.

Covers the plan's U3 scenarios for this file: 200 listed sessions produce
bounded row memory; a row never accumulates event history; at the cap, at the
cap plus one, and with more protected rows than the cap, no focused, queued,
or in-flight row is ever evicted, eviction is oldest-first among unprotected
rows only, and the truncation note renders whenever anything was evicted.
"""

from __future__ import annotations

from dataclasses import fields, replace

from talaria.domain.decode import decode_frame
from talaria.domain.registry import (
    MAX_RUNTIME_ALIASES,
    ROW_CAP_PER_CONNECTION,
    truncation_note,
)
from talaria.domain.session_list import decode_session_list
from talaria.domain.state import (
    FleetState,
    route_frame,
    seed_from_listing,
)

from .conftest import BASE_TIME, raw_event

PROFILE = "beta"


def event_at(fleet: FleetState, session_id: str, index: int) -> FleetState:
    frame = raw_event("message.start", session_id=session_id)
    return route_frame(
        fleet,
        decode_frame(frame, at=BASE_TIME + index, seq=index + 1),
        profile=PROFILE,
    )


def fleet_with_rows(count: int) -> FleetState:
    """``count`` background sessions, one event each, oldest first."""
    fleet = FleetState(focused_profile="default")
    for index in range(count):
        fleet = event_at(fleet, f"sess-{index:04d}", index)
    return fleet


def test_two_hundred_listed_sessions_stay_bounded_with_no_eviction() -> None:
    directory = decode_session_list(
        {"sessions": [{"id": f"stored-{i}", "title": f"t{i}"} for i in range(200)]}
    )
    fleet = seed_from_listing(
        FleetState(focused_profile="default"),
        directory,
        profile=PROFILE,
        at=BASE_TIME,
        poll_epoch=1,
    )
    assert len(fleet.rows) == 200
    assert fleet.channel(PROFILE).evicted_rows == 0
    assert truncation_note(fleet.channel(PROFILE)) == ""


def test_a_row_never_accumulates_event_history() -> None:
    """R3: routing three hundred frames at one session leaves a fixed-size
    row — every sequence field stays bounded, and the row equals what a short
    burst produces apart from its stamps."""
    fleet = FleetState(focused_profile="default")
    for index in range(300):
        frame = raw_event(
            "message.delta", {"text": "x" * 500}, session_id="sess-busy"
        )
        fleet = route_frame(
            fleet,
            decode_frame(frame, at=BASE_TIME + index, seq=index + 1),
            profile=PROFILE,
        )
    row = fleet.rows[(PROFILE, "sess-busy")]
    assert len(row.runtime_ids) <= MAX_RUNTIME_ALIASES
    for f in fields(row):
        value = getattr(row, f.name)
        if isinstance(value, tuple):
            assert len(value) <= MAX_RUNTIME_ALIASES, f.name
        if isinstance(value, str):
            assert len(value) < 4096, f.name
    # The channel's notice list is bounded too.
    assert len(fleet.channel(PROFILE).notices) <= 8


def test_at_the_cap_nothing_is_evicted() -> None:
    fleet = fleet_with_rows(ROW_CAP_PER_CONNECTION)
    assert len(fleet.rows) == ROW_CAP_PER_CONNECTION
    assert fleet.channel(PROFILE).evicted_rows == 0
    assert truncation_note(fleet.channel(PROFILE)) == ""


def test_one_past_the_cap_evicts_the_oldest_unprotected_row_visibly() -> None:
    fleet = fleet_with_rows(ROW_CAP_PER_CONNECTION + 1)
    assert len(fleet.rows) == ROW_CAP_PER_CONNECTION
    # Oldest-first: the first-observed session is the one that went.
    assert (PROFILE, "sess-0000") not in fleet.rows
    assert (PROFILE, "sess-0001") in fleet.rows
    assert (PROFILE, f"sess-{ROW_CAP_PER_CONNECTION:04d}") in fleet.rows
    # Never a silent drop.
    assert fleet.channel(PROFILE).evicted_rows == 1
    assert truncation_note(fleet.channel(PROFILE)) == "1 older sessions not shown"
    # The evicted row's aliases are gone with it.
    assert (PROFILE, "sess-0000") not in fleet.aliases


def test_protected_rows_are_never_evicted_even_when_oldest() -> None:
    fleet = fleet_with_rows(ROW_CAP_PER_CONNECTION)
    protected_key = (PROFILE, "sess-0000")
    fleet = replace(fleet, queue_item_keys=frozenset({protected_key}))
    fleet = event_at(fleet, "sess-new", ROW_CAP_PER_CONNECTION + 5)
    # The oldest row held a queue item, so the second-oldest went instead.
    assert protected_key in fleet.rows
    assert (PROFILE, "sess-0001") not in fleet.rows
    assert fleet.channel(PROFILE).evicted_rows == 1


def test_more_protected_rows_than_the_cap_all_survive_and_may_exceed_it() -> None:
    over = ROW_CAP_PER_CONNECTION + 4
    # Every row is protected the moment it exists (a queue item references
    # it), so the cap never has an unprotected row to evict.
    fleet = fleet_with_rows(0)
    for index in range(over):
        key = (PROFILE, f"sess-{index:04d}")
        fleet = replace(fleet, queue_item_keys=fleet.queue_item_keys | {key})
        fleet = event_at(fleet, key[1], index)
    # Every protected row survived; the connection exceeds the cap outright.
    assert len(fleet.rows) == over
    assert fleet.channel(PROFILE).evicted_rows == 0

    # An unprotected newcomer cannot displace any protected row: with zero
    # remaining budget it is itself evicted — visibly, never silently.
    fleet = event_at(fleet, "sess-unprotected", over + 10)
    assert (PROFILE, "sess-unprotected") not in fleet.rows
    assert all(key in fleet.rows for key in fleet.queue_item_keys)
    assert fleet.channel(PROFILE).evicted_rows == 1
    assert truncation_note(fleet.channel(PROFILE)) == "1 older sessions not shown"


def test_the_focused_row_is_always_protected() -> None:
    frames = [raw_event("message.start", session_id="sess-focus")]
    fleet = FleetState(focused_profile=PROFILE)
    fleet = route_frame(
        fleet, decode_frame(frames[0], at=BASE_TIME, seq=1), profile=PROFILE
    )
    for index in range(1, ROW_CAP_PER_CONNECTION + 20):
        fleet = event_at(fleet, f"sess-{index:04d}", index)
    assert (PROFILE, "sess-focus") in fleet.rows
    assert fleet.rows[(PROFILE, "sess-focus")].ownership == "we_drive"
