"""The fleet registry: routing, seeding, observation classes, retirement (U3).

Covers the plan's U3 test scenarios for ``tests/domain/test_registry.py``:
AE1 verbatim (a background session's event — known and unknown kind — updates
its row, leaves the focused transcript unchanged, and fails against the old
discard-and-count behavior), R25's identity-less path, AE4's two halves at row
level (never-observed seeding, stale-since on a dropped connection), the
reclaimed latch, and AE8's domain half (ages ride the frame clock and
reproduce exactly under re-application).

Frames are built raw and pushed through :mod:`talaria.domain.decode`, exactly
as ``conftest.py`` documents — a helper that skipped the decoder would let a
decoding regression pass every routing test here.
"""

from __future__ import annotations

from dataclasses import replace
from typing import Any

from talaria.domain.decode import decode_frame
from talaria.domain.registry import (
    ConnectionChannel,
    RegistryRow,
    next_poll_due_at,
    row_age,
    stale_for,
    truncation_note,
    waiting_floor_span,
)
from talaria.domain.session_list import (
    decode_active_list,
    decode_session_list,
)
from talaria.domain.state import (
    FleetState,
    SessionState,
    apply_active_list,
    apply_frames,
    fleet_connection_lost,
    fleet_connection_restored,
    listing_failed,
    mark_we_drive,
    route_frame,
    route_frames,
    seed_from_listing,
)

from .conftest import BASE_TIME, raw_event

FOCUSED_PROFILE = "default"
OTHER_PROFILE = "beta"


def decoded(frame: Any, *, at: float, seq: int) -> Any:
    return decode_frame(frame, at=at, seq=seq)


def fleet_with_focused_session() -> FleetState:
    """A fleet whose focused engine adopted ``sess-focus`` through a real turn."""
    frames = [
        raw_event("message.start"),
        raw_event("message.delta", {"text": "Hello"}),
        raw_event("message.complete", {"text": "Hello"}),
    ]
    fleet = FleetState(focused_profile=FOCUSED_PROFILE)
    return route_frames(
        fleet,
        [decoded(f, at=BASE_TIME + i, seq=i + 1) for i, f in enumerate(frames)],
        profile=FOCUSED_PROFILE,
    )


# ── AE1: routing replaces the discard ────────────────────────────────────


def test_background_known_event_updates_its_row_and_not_the_focused_transcript() -> None:
    fleet = fleet_with_focused_session()
    before = fleet.focused

    frame = raw_event(
        "approval.request",
        {"request_id": "r-1", "command": "rm -rf /tmp/x"},
        session_id="sess-bg",
    )
    fleet = route_frame(
        fleet, decoded(frame, at=BASE_TIME + 10, seq=11), profile=FOCUSED_PROFILE
    )

    row = fleet.rows[(FOCUSED_PROFILE, "sess-bg")]
    assert row.waiting_kind == "approval"
    assert row.observed
    assert row.last_event_at == BASE_TIME + 10
    assert row.last_event_source == "event"
    # The focused transcript did not change — the guard's *protection* holds.
    assert fleet.focused == before
    # The discard is gone: nothing was counted as ignored. Fold the same frame
    # straight into the focused engine — the pre-U3 path — and the old
    # behavior shows itself: counted, and no row anywhere. This is the
    # assertion that fails against the current discard-and-count behavior.
    discarded = apply_frames(before, [decoded(frame, at=BASE_TIME + 10, seq=11)])
    assert discarded.cross_session_events_ignored == before.cross_session_events_ignored + 1
    assert fleet.focused.cross_session_events_ignored == before.cross_session_events_ignored


def test_background_unknown_event_kind_also_updates_its_row() -> None:
    fleet = fleet_with_focused_session()
    before = fleet.focused

    frame = raw_event("totally.new.kind", {"anything": 1}, session_id="sess-bg")
    fleet = route_frame(
        fleet, decoded(frame, at=BASE_TIME + 20, seq=21), profile=FOCUSED_PROFILE
    )

    row = fleet.rows[(FOCUSED_PROFILE, "sess-bg")]
    assert row.observed
    assert row.last_event_at == BASE_TIME + 20
    assert fleet.focused == before
    assert fleet.focused.cross_session_events_ignored == before.cross_session_events_ignored


def test_focused_session_traffic_still_feeds_the_engine_unchanged() -> None:
    """KTD3: the focused engine is fed exactly as today — same frames, same
    resulting state as the pre-fleet reducer, plus a mirrored registry row."""
    frames = [
        raw_event("message.start"),
        raw_event("message.delta", {"text": "Hello"}),
        raw_event("message.complete", {"text": "Hello"}),
    ]
    decoded_frames = [
        decoded(f, at=BASE_TIME + i, seq=i + 1) for i, f in enumerate(frames)
    ]
    engine_only = apply_frames(SessionState(), decoded_frames)
    fleet = route_frames(
        FleetState(focused_profile=FOCUSED_PROFILE),
        decoded_frames,
        profile=FOCUSED_PROFILE,
    )
    assert fleet.focused == engine_only
    row = fleet.rows[(FOCUSED_PROFILE, "sess-focus")]
    assert row.ownership == "we_drive"
    assert fleet.focused_key() == (FOCUSED_PROFILE, "sess-focus")


# ── R25: identity-less traffic ───────────────────────────────────────────


def test_identityless_event_surfaces_on_the_channel_and_creates_no_row() -> None:
    fleet = fleet_with_focused_session()
    rows_before = dict(fleet.rows)

    frame = raw_event("message.delta", {"text": "hi"}, session_id=None)
    fleet = route_frame(
        fleet, decoded(frame, at=BASE_TIME + 5, seq=6), profile=OTHER_PROFILE
    )

    channel = fleet.channel(OTHER_PROFILE)
    assert channel.identityless_count == 1
    assert channel.notices  # surfaced, not just counted
    assert "message.delta" in channel.notices[-1]
    assert dict(fleet.rows) == rows_before


def test_protocol_error_is_counted_identityless_and_creates_no_row() -> None:
    fleet = fleet_with_focused_session()
    rows_before = dict(fleet.rows)

    bad = decode_frame("not json at all", at=BASE_TIME + 6, seq=7, parse_error=True)
    fleet = route_frame(fleet, bad, profile=OTHER_PROFILE)

    assert fleet.channel(OTHER_PROFILE).identityless_count == 1
    assert dict(fleet.rows) == rows_before


def test_sessions_changed_broadcast_is_a_poll_hint_not_an_identityless_defect() -> None:
    fleet = fleet_with_focused_session()
    frame = raw_event("sessions.changed", {}, session_id=None)
    fleet = route_frame(
        fleet, decoded(frame, at=BASE_TIME + 7, seq=8), profile=OTHER_PROFILE
    )
    channel = fleet.channel(OTHER_PROFILE)
    assert channel.identityless_count == 0
    assert channel.hint_at == BASE_TIME + 7


# ── Seeding: list-only rows are never-observed, not idle (AE4 second half) ─


def listing(*ids: str) -> Any:
    return decode_session_list(
        {"sessions": [{"id": i, "title": f"t-{i}", "message_count": 3} for i in ids]}
    )


def test_seeded_but_never_polled_row_renders_never_observed_not_idle() -> None:
    fleet = FleetState(focused_profile=FOCUSED_PROFILE)
    fleet = seed_from_listing(
        fleet, listing("stored-1", "stored-2"), profile=OTHER_PROFILE,
        at=BASE_TIME, poll_epoch=1,
    )
    row = fleet.rows[(OTHER_PROFILE, "stored-1")]
    assert row.observation() == "never-observed"
    assert row.status == ""
    assert row.lifecycle() != "idle"
    assert row.title == "t-stored-1"
    # No age is fabricated for a row nothing has observed (R24).
    assert row_age(row, fleet.clock) is None
    assert stale_for(row, fleet.clock) is None


def test_active_list_poll_confirms_lifecycle_verbatim_including_starting() -> None:
    fleet = FleetState(focused_profile=FOCUSED_PROFILE)
    fleet = seed_from_listing(
        fleet, listing("stored-1"), profile=OTHER_PROFILE, at=BASE_TIME, poll_epoch=1
    )
    directory = decode_active_list(
        {
            "sessions": [
                {
                    "id": "run-1",
                    "session_key": "stored-1",
                    "status": "starting",
                    "title": "t-stored-1",
                    "model": "m1",
                    "message_count": 0,
                    "current": False,
                    "started_at": BASE_TIME,
                    "last_active": BASE_TIME,
                },
                {
                    "id": "run-2",
                    "session_key": "live-only",
                    "status": "waiting",
                    "title": "waits",
                    "model": "m2",
                    "message_count": 4,
                },
            ]
        }
    )
    fleet = apply_active_list(
        fleet, directory, profile=OTHER_PROFILE, at=BASE_TIME + 2, poll_epoch=1
    )
    starting = fleet.rows[(OTHER_PROFILE, "stored-1")]
    assert starting.status == "starting"  # rendered, not treated as unreachable
    assert starting.lifecycle() == "starting"
    assert starting.observation() == "live"
    assert starting.model == "m1"
    # The runtime id is an alias, never the key (KTD3).
    assert (OTHER_PROFILE, "run-1") not in fleet.rows
    assert fleet.aliases[(OTHER_PROFILE, "run-1")] == (OTHER_PROFILE, "stored-1")

    waiting = fleet.rows[(OTHER_PROFILE, "live-only")]
    # The gateway flattens every waiting kind (U1): the kind is unobserved.
    assert waiting.waiting_kind == "unobserved"
    assert waiting_floor_span(waiting, fleet.clock) == 0.0


def test_wait_age_is_a_floor_from_first_observation_never_a_start_time() -> None:
    """KTD12: 'waiting ≥ observed span' — the floor starts at the poll that
    first saw the wait and grows with the frame clock, never jumping back to
    a fabricated start."""
    fleet = FleetState(focused_profile=FOCUSED_PROFILE)
    directory = decode_active_list(
        {"sessions": [{"id": "run-9", "session_key": "s-9", "status": "waiting"}]}
    )
    fleet = apply_active_list(
        fleet, directory, profile=OTHER_PROFILE, at=BASE_TIME, poll_epoch=1
    )
    fleet = apply_active_list(
        fleet, directory, profile=OTHER_PROFILE, at=BASE_TIME + 8, poll_epoch=2
    )
    row = fleet.rows[(OTHER_PROFILE, "s-9")]
    assert row.status_floor_at == BASE_TIME  # first observation, kept
    assert waiting_floor_span(row, fleet.clock) == 8.0


# ── Retirement and the reclaimed latch ───────────────────────────────────


def test_reclaimed_row_shows_stale_since_with_the_reap_reason() -> None:
    fleet = fleet_with_focused_session()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("message.start", session_id="sess-bg"), at=BASE_TIME + 10, seq=11
        ),
        profile=FOCUSED_PROFILE,
    )
    fleet = route_frame(
        fleet,
        decoded(
            raw_event(
                "session.reclaimed", {"reason": "idle timeout"}, session_id="sess-bg"
            ),
            at=BASE_TIME + 30,
            seq=12,
        ),
        profile=FOCUSED_PROFILE,
    )
    row = fleet.rows[(FOCUSED_PROFILE, "sess-bg")]
    assert row.reclaimed_reason == "idle timeout"
    assert row.lifecycle() == "reclaimed(idle timeout)"
    assert row.observation() == "stale"
    assert row.stale_since == BASE_TIME + 30
    assert stale_for(row, fleet.clock) == 0.0


def test_dual_listing_retirement_drops_only_doubly_absent_unprotected_rows() -> None:
    """KTD2's retirement rule: absent from both same-epoch sweeps → dropped,
    unless focused, holding a queue item, or answering. Live-only rows kept."""
    fleet = FleetState(focused_profile=FOCUSED_PROFILE)
    fleet = seed_from_listing(
        fleet, listing("keep", "gone", "queued"), profile=OTHER_PROFILE,
        at=BASE_TIME, poll_epoch=1,
    )
    fleet = replace(fleet, queue_item_keys=frozenset({(OTHER_PROFILE, "queued")}))
    # Epoch 2: the listing still knows "keep"; "gone" and "queued" vanish from
    # both sweeps; "live-only" appears in the active list alone.
    fleet = seed_from_listing(
        fleet, listing("keep"), profile=OTHER_PROFILE, at=BASE_TIME + 10, poll_epoch=2
    )
    directory = decode_active_list(
        {"sessions": [{"id": "r-lo", "session_key": "live-only", "status": "working"}]}
    )
    fleet = apply_active_list(
        fleet, directory, profile=OTHER_PROFILE, at=BASE_TIME + 11, poll_epoch=2
    )
    assert (OTHER_PROFILE, "keep") in fleet.rows
    assert (OTHER_PROFILE, "live-only") in fleet.rows  # live-only rows are kept
    assert (OTHER_PROFILE, "gone") not in fleet.rows
    # The queue-item row survived its double absence — never removed while
    # anything references it.
    assert (OTHER_PROFILE, "queued") in fleet.rows


def test_mismatched_epochs_never_retire() -> None:
    fleet = FleetState(focused_profile=FOCUSED_PROFILE)
    fleet = seed_from_listing(
        fleet, listing("only-listed"), profile=OTHER_PROFILE, at=BASE_TIME, poll_epoch=1
    )
    # An active-list poll of a *different* epoch that omits the row must not
    # drop it — the dual-listing rule requires agreement on one epoch.
    fleet = apply_active_list(
        fleet,
        decode_active_list({"sessions": []}),
        profile=OTHER_PROFILE,
        at=BASE_TIME + 1,
        poll_epoch=2,
    )
    assert (OTHER_PROFILE, "only-listed") in fleet.rows


def test_failed_listing_marks_listing_derived_fields_stale_never_cleared() -> None:
    fleet = FleetState(focused_profile=FOCUSED_PROFILE)
    fleet = seed_from_listing(
        fleet, listing("s-1"), profile=OTHER_PROFILE, at=BASE_TIME, poll_epoch=1
    )
    fleet = listing_failed(fleet, profile=OTHER_PROFILE, at=BASE_TIME + 5)
    assert fleet.channel(OTHER_PROFILE).listing_stale_since == BASE_TIME + 5
    # The data is marked stale, not cleared.
    assert fleet.rows[(OTHER_PROFILE, "s-1")].title == "t-s-1"


# ── A dropped connection (AE4 first half, at row level) ──────────────────


def test_dropped_connection_marks_every_row_it_fed_stale_with_age() -> None:
    fleet = fleet_with_focused_session()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("message.start", session_id="sess-bg"), at=BASE_TIME + 10, seq=11
        ),
        profile=FOCUSED_PROFILE,
    )
    fleet = seed_from_listing(
        fleet, listing("never-polled"), profile=FOCUSED_PROFILE,
        at=BASE_TIME + 11, poll_epoch=1,
    )
    fleet = fleet_connection_lost(fleet, profile=FOCUSED_PROFILE, at=BASE_TIME + 20)

    observed_row = fleet.rows[(FOCUSED_PROFILE, "sess-bg")]
    assert observed_row.observation() == "stale"
    assert observed_row.stale_since == BASE_TIME + 20
    assert observed_row.lifecycle() == "disconnected"

    # Advance the frame clock via another connection: the stale age grows —
    # the value is never presented frozen-fresh.
    fleet = route_frame(
        fleet,
        decoded(raw_event("message.start", session_id="x"), at=BASE_TIME + 50, seq=99),
        profile=OTHER_PROFILE,
    )
    assert stale_for(fleet.rows[(FOCUSED_PROFILE, "sess-bg")], fleet.clock) == 30.0

    # A never-observed row does not become stale-since-nothing (R24).
    seeded = fleet.rows[(FOCUSED_PROFILE, "never-polled")]
    assert seeded.observation() == "never-observed"
    assert stale_for(seeded, fleet.clock) is None


def test_reconnect_restores_the_channel_but_not_the_rows() -> None:
    fleet = fleet_with_focused_session()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("message.start", session_id="sess-bg"), at=BASE_TIME + 10, seq=11
        ),
        profile=FOCUSED_PROFILE,
    )
    fleet = fleet_connection_lost(fleet, profile=FOCUSED_PROFILE, at=BASE_TIME + 20)
    fleet = fleet_connection_restored(
        fleet, profile=FOCUSED_PROFILE, generation=2, at=BASE_TIME + 25
    )
    assert fleet.channel(FOCUSED_PROFILE).connected
    # Rows stay stale until a fresh poll or event re-confirms each one.
    assert fleet.rows[(FOCUSED_PROFILE, "sess-bg")].observation() == "stale"


def test_stale_generation_frames_never_unstale_a_row() -> None:
    """U2's finding: (profile, epoch) is not unique across a reconnect-by-
    ensure; the router distinguishes connections by generation."""
    fleet = fleet_with_focused_session()
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("message.start", session_id="sess-bg"), at=BASE_TIME + 10, seq=11
        ),
        profile=FOCUSED_PROFILE,
        generation=1,
    )
    fleet = fleet_connection_lost(fleet, profile=FOCUSED_PROFILE, at=BASE_TIME + 20)
    fleet = fleet_connection_restored(
        fleet, profile=FOCUSED_PROFILE, generation=2, at=BASE_TIME + 25
    )
    # A slow consumer delivers an old-generation frame after the rebuild.
    fleet = route_frame(
        fleet,
        decoded(
            raw_event("message.delta", {"text": "x"}, session_id="sess-bg"),
            at=BASE_TIME + 26,
            seq=12,
        ),
        profile=FOCUSED_PROFILE,
        generation=1,
    )
    row = fleet.rows[(FOCUSED_PROFILE, "sess-bg")]
    assert row.observation() == "stale"  # the old socket's frame proves nothing fresh
    assert row.last_event_at == BASE_TIME + 26  # but the observation is recorded


# ── Ownership ────────────────────────────────────────────────────────────


def test_mark_we_drive_records_ownership_without_faking_observation() -> None:
    fleet = FleetState(focused_profile=FOCUSED_PROFILE)
    fleet = mark_we_drive(
        fleet, profile=OTHER_PROFILE, session_id="resumed", at=BASE_TIME
    )
    row = fleet.rows[(OTHER_PROFILE, "resumed")]
    assert row.ownership == "we_drive"
    assert row.observation() == "never-observed"


# ── The KTD2 poll cadence, pure half ─────────────────────────────────────


def test_poll_due_arithmetic_coalesces_hints_and_backstops() -> None:
    never_polled = ConnectionChannel(profile="p")
    assert next_poll_due_at(never_polled) == 0.0
    quiet = ConnectionChannel(profile="p", last_poll_at=100.0)
    assert next_poll_due_at(quiet) == 130.0  # 30-second backstop
    hinted = ConnectionChannel(profile="p", last_poll_at=100.0, hint_at=100.5)
    assert next_poll_due_at(hinted) == 102.0  # 2-second coalesce floor
    late_hint = ConnectionChannel(profile="p", last_poll_at=100.0, hint_at=110.0)
    assert next_poll_due_at(late_hint) == 110.0


# ── AE8's domain half: determinism and frame-clock ages ──────────────────


def build_fleet_twice() -> tuple[FleetState, FleetState]:
    def build() -> FleetState:
        fleet = FleetState(focused_profile=FOCUSED_PROFILE)
        fleet = seed_from_listing(
            fleet, listing("stored-1", "stored-2"), profile=OTHER_PROFILE,
            at=BASE_TIME, poll_epoch=1,
        )
        fleet = route_frames(
            fleet,
            [
                decoded(raw_event("message.start"), at=BASE_TIME + 1, seq=1),
                decoded(
                    raw_event("approval.request", {"command": "ls"}, session_id="bg"),
                    at=BASE_TIME + 2,
                    seq=2,
                ),
            ],
            profile=FOCUSED_PROFILE,
        )
        directory = decode_active_list(
            {"sessions": [{"id": "r1", "session_key": "stored-1", "status": "waiting"}]}
        )
        fleet = apply_active_list(
            fleet, directory, profile=OTHER_PROFILE, at=BASE_TIME + 3, poll_epoch=2
        )
        fleet = fleet_connection_lost(fleet, profile=OTHER_PROFILE, at=BASE_TIME + 4)
        return fleet

    return build(), build()


def test_ages_derive_from_frame_stamps_and_reproduce_exactly() -> None:
    first, second = build_fleet_twice()
    assert first == second
    assert first.clock == BASE_TIME + 4
    for key, row in first.rows.items():
        assert row_age(row, first.clock) == row_age(second.rows[key], second.clock)
        assert stale_for(row, first.clock) == stale_for(second.rows[key], second.clock)
    stored = first.rows[(OTHER_PROFILE, "stored-1")]
    assert row_age(stored, first.clock) == 1.0  # (BASE+4) - (BASE+3), frame stamps only


def test_truncation_note_names_the_hidden_count() -> None:
    assert truncation_note(ConnectionChannel(profile="p")) == ""
    assert (
        truncation_note(ConnectionChannel(profile="p", evicted_rows=7))
        == "7 older sessions not shown"
    )


def test_registry_row_is_a_summary_with_no_transcript_field() -> None:
    """R3: a row is a fixed-size summary. No field of a row is an unbounded
    text accumulation; the only sequences are the bounded alias tuple."""
    from dataclasses import fields

    sequence_fields = [
        f.name
        for f in fields(RegistryRow)
        if "tuple" in str(f.type)
    ]
    assert sequence_fields == ["runtime_ids"]


# ── CR3 remediation: three defects the adversarial review demonstrated ─────


def test_a_queue_item_survives_the_rebind_to_a_durable_key() -> None:
    """Protection must follow a row when it moves (CR3 finding 1).

    The ordinary sequence, not a corner: an event creates a row keyed by its
    *runtime* id, the queue records that key, and the first poll that learns the
    session's ``session_key`` rebinds the row to its durable key. Protection was
    a raw key-membership test, so after the rebind it named a key no row
    answered to — and the next dual-listing sweep retired a row the queue was
    still pointing at.
    """
    fleet = fleet_with_focused_session()
    fleet = route_frame(
        fleet,
        decoded(raw_event("message.start", session_id="run-1"), at=BASE_TIME + 5, seq=9),
        profile=OTHER_PROFILE,
        generation=1,
    )
    runtime_key = (OTHER_PROFILE, "run-1")
    assert runtime_key in fleet.rows
    # The queue takes hold of the row under the only key it has.
    fleet = replace(fleet, queue_item_keys=frozenset({runtime_key}))

    # The first poll teaches the durable identity and moves the row.
    fleet = apply_active_list(
        fleet,
        decode_active_list(
            {
                "sessions": [
                    {
                        "id": "run-1",
                        "session_key": "durable-1",
                        "status": "waiting",
                        "last_active": BASE_TIME + 6,
                        "started_at": BASE_TIME,
                        "message_count": 1,
                        "model": "m",
                        "preview": "p",
                        "title": "t",
                    }
                ]
            },
        ),
        profile=OTHER_PROFILE,
        at=BASE_TIME + 6,
        poll_epoch=1,
    )
    durable_key = (OTHER_PROFILE, "durable-1")
    assert durable_key in fleet.rows and runtime_key not in fleet.rows

    # The protection recorded under the old key still reaches the moved row.
    assert durable_key in fleet.protected_keys()

    # And a dual-listing sweep that omits it entirely leaves it alone.
    fleet = seed_from_listing(
        fleet, decode_session_list({"sessions": []}), profile=OTHER_PROFILE,
        at=BASE_TIME + 7, poll_epoch=2,
    )
    fleet = apply_active_list(
        fleet, decode_active_list({"sessions": []}),
        profile=OTHER_PROFILE, at=BASE_TIME + 7, poll_epoch=2,
    )
    assert durable_key in fleet.rows, "a queue item's row was retired under it"


def test_resume_churn_does_not_grow_the_alias_index() -> None:
    """Aliases trimmed by a merge must lose their index entries (CR3 finding 2).

    ``_bind_alias`` cleans up the ids it pushes out, but it derives the dropped
    set from the row's current ``runtime_ids`` — so an id a *merge* trimmed was
    invisible to it forever after. Fifty resumes of one session left fifty
    permanent entries pointing at a row that holds four runtime ids.
    """
    fleet = fleet_with_focused_session()
    for index in range(50):
        runtime = f"run-{index}"
        fleet = route_frame(
            fleet,
            decoded(
                raw_event("message.start", session_id=runtime),
                at=BASE_TIME + 10 + index,
                seq=100 + index,
            ),
            profile=OTHER_PROFILE,
            generation=1,
        )
        fleet = apply_active_list(
            fleet,
            decode_active_list(
                {
                    "sessions": [
                        {
                            "id": runtime,
                            "session_key": "durable-stable",
                            "status": "idle",
                            "last_active": BASE_TIME + 10 + index,
                            "started_at": BASE_TIME,
                            "message_count": 1,
                            "model": "m",
                            "preview": "p",
                            "title": "t",
                        }
                    ]
                },
            ),
            profile=OTHER_PROFILE,
            at=BASE_TIME + 10 + index,
            poll_epoch=1,
        )

    row = fleet.rows[(OTHER_PROFILE, "durable-stable")]
    pointing_here = [
        pair for pair, target in fleet.aliases.items()
        if target == (OTHER_PROFILE, "durable-stable")
    ]
    # The row itself stays bounded, and so does the index that points at it.
    assert len(row.runtime_ids) <= 4
    assert len(pointing_here) <= len(row.runtime_ids) + 1, pointing_here


def test_a_stale_generation_frame_does_not_unstale_the_focused_row() -> None:
    """The focused row is not exempt from the generation rule (CR3 finding 3).

    The router enforced it for background rows only; the focused path applied a
    fresh observation unconditionally, so one late frame from a replaced socket
    flipped the focused row back to live while every background row on the same
    connection correctly stayed stale. The engine feed is deliberately unchanged
    — R2 governs the transcript, and a late frame is still a real frame the
    transcript must show. What it is not is evidence the connection is back.
    """
    fleet = fleet_with_focused_session()
    focused_key = fleet.focused_key()
    assert focused_key is not None

    fleet = fleet_connection_lost(fleet, profile=FOCUSED_PROFILE, at=BASE_TIME + 20)
    fleet = fleet_connection_restored(
        fleet, profile=FOCUSED_PROFILE, generation=2, at=BASE_TIME + 25
    )
    before = fleet.focused

    fleet = route_frame(
        fleet,
        decoded(
            raw_event("message.delta", {"text": "late"}, session_id="sess-focus"),
            at=BASE_TIME + 26,
            seq=90,
        ),
        profile=FOCUSED_PROFILE,
        generation=1,
    )

    assert fleet.rows[focused_key].observation() == "stale"
    # The transcript still saw it: R2 is about the engine, not the row.
    assert fleet.focused != before


def test_a_queue_item_survives_rebind_then_alias_churn() -> None:
    """Protection must outlive the alias entry it was first resolvable through.

    CR3 round 2, and the sharpest finding of this segment: two individually
    correct fixes destroyed each other. Protection resolved through the alias
    map, and the alias index is deliberately bounded — so once a protected
    runtime id aged out of the row's four-id window, the trim deleted the only
    route from the recorded key to the live row, and the row a queue item still
    referenced became retirable again.

    The rebind is the one place a row's key ever changes, so protection is
    re-anchored there. That holds across any number of rebinds, because each one
    re-anchors again, and it does not depend on an index entry the bound is
    entitled to reclaim.
    """
    fleet = fleet_with_focused_session()

    def poll(runtime: str, at: float) -> FleetState:
        return apply_active_list(
            fleet,
            decode_active_list(
                {
                    "sessions": [
                        {
                            "id": runtime,
                            "session_key": "durable-1",
                            "status": "idle",
                            "last_active": at,
                            "started_at": BASE_TIME,
                            "message_count": 1,
                            "model": "m",
                            "preview": "p",
                            "title": "t",
                        }
                    ]
                }
            ),
            profile=OTHER_PROFILE,
            at=at,
            poll_epoch=1,
        )

    # An event creates the row under its runtime id, and the queue takes hold.
    fleet = route_frame(
        fleet,
        decoded(raw_event("message.start", session_id="r1"), at=BASE_TIME + 5, seq=9),
        profile=OTHER_PROFILE,
        generation=1,
    )
    fleet = replace(fleet, queue_item_keys=frozenset({(OTHER_PROFILE, "r1")}))
    fleet = poll("r1", BASE_TIME + 6)

    durable_key = (OTHER_PROFILE, "durable-1")
    assert durable_key in fleet.protected_keys()

    # Four resumes age "r1" out of the row's runtime-id window, and the bound
    # reclaims its alias entry — correctly.
    for index, runtime in enumerate(("r2", "r3", "r4", "r5")):
        fleet = route_frame(
            fleet,
            decoded(
                raw_event("message.start", session_id=runtime),
                at=BASE_TIME + 10 + index,
                seq=20 + index,
            ),
            profile=OTHER_PROFILE,
            generation=1,
        )
        fleet = poll(runtime, BASE_TIME + 10 + index)
    assert "r1" not in fleet.rows[durable_key].runtime_ids
    assert (OTHER_PROFILE, "r1") not in fleet.aliases

    # Protection survived the churn, so the dual sweep leaves the row alone.
    assert durable_key in fleet.protected_keys()
    fleet = seed_from_listing(
        fleet, decode_session_list({"sessions": []}), profile=OTHER_PROFILE,
        at=BASE_TIME + 30, poll_epoch=2,
    )
    fleet = apply_active_list(
        fleet, decode_active_list({"sessions": []}),
        profile=OTHER_PROFILE, at=BASE_TIME + 30, poll_epoch=2,
    )
    assert durable_key in fleet.rows, "a queue item's row was retired under it"
