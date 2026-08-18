"""R6, R16, R19 — the projection's shape, completeness, and change markers.

The load-bearing assertion in this file is content completeness: every piece of
transcript content has to survive the trip into the plain-text line buffer,
because R6 puts markdown, diff, and reasoning *presentation* out of scope while
insisting their *content* is never dropped. It is an easy requirement to fail
quietly — a renderer that skips an entry kind it does not know how to style loses
data without erroring.

The change-marker assertions pin U3's ADR-0002 answer: immutable snapshots plus a
set naming what moved. U5 measures what that costs; these tests fix what it
means.
"""

from __future__ import annotations

import json

from talaria.domain.projection import (
    SNAPSHOT_REGIONS,
    STATUS_PAYLOAD_VERSION,
    project,
    prompt_feed,
    status_payload,
    transcript_view,
)
from talaria.domain.queue import wait_line
from talaria.domain.session_list import decode_active_list
from talaria.domain.state import (
    FleetState,
    SessionState,
    apply_active_list,
    fleet_queue,
    focus_session,
    respond_to_prompt,
    route_frames,
    set_connection,
)

from .conftest import BASE_TIME, decode_all, raw_event, replay


def _long_turn() -> SessionState:
    return replay(
        [
            raw_event("gateway.ready", {}, session_id=None),
            raw_event("session.info", {"title": "Compat work", "usage": {"input_tokens": 12}}),
            raw_event("message.start"),
            raw_event("reasoning.delta", {"text": "weighing options"}),
            raw_event("tool.start", {"tool_id": "t1", "name": "read_file", "context": "AGENTS.md"}),
            raw_event("tool.complete", {"tool_id": "t1", "name": "read_file", "summary": "ok"}),
            raw_event("message.delta", {"text": "Line one\nLine two"}),
            raw_event(
                "message.complete",
                {"text": "Line one\nLine two", "usage": {"output_tokens": 5}},
            ),
        ]
    )


# ── R6: content completeness ─────────────────────────────────────────────


def test_every_transcript_entry_survives_into_the_line_buffer() -> None:
    state = _long_turn()
    view = transcript_view(state)
    assert view.entry_count == len(state.transcript)
    for entry in state.transcript:
        for line in entry.text.split("\n"):
            assert any(line in rendered for rendered in view.lines), (
                f"{entry.kind} content was dropped by the plain-text projection: {line!r}"
            )


def test_a_multi_line_entry_becomes_multiple_buffer_lines() -> None:
    view = transcript_view(_long_turn())
    assert "Line one" in view.lines
    assert "Line two" in view.lines


def test_in_flight_streaming_text_is_part_of_the_visible_buffer() -> None:
    """A terminal read arriving mid-stream should describe the screen the
    operator is looking at, not the last committed entry."""
    state = replay(
        [raw_event("message.start"), raw_event("message.delta", {"text": "still typing"})]
    )
    assert transcript_view(state).lines[-1] == "still typing"


def test_speaker_markers_do_not_swallow_the_content_they_prefix() -> None:
    state = replay([raw_event("status.update", {"text": "compressing context"})])
    assert transcript_view(state).lines == ("— compressing context",)


# ── R19: the frozen v1 status payload ────────────────────────────────────


def test_status_payload_carries_exactly_the_frozen_v1_field_set() -> None:
    payload = status_payload(_long_turn(), mode="replay").to_json_dict()
    assert set(payload) == {
        "version",
        "mode",
        "connection",
        "session",
        "turn",
        "pending_prompts",
        "subagents",
        "usage",
    }
    assert set(payload["session"]) == {"id", "title"}
    assert set(payload["subagents"]) == {"active", "terminal"}
    assert set(payload["usage"]) == {"input_tokens", "output_tokens"}


def test_status_payload_carries_a_version_from_its_first_commit() -> None:
    payload = status_payload(SessionState(), mode="replay").to_json_dict()
    assert payload["version"] == STATUS_PAYLOAD_VERSION == 1


def test_status_payload_is_json_serializable_with_no_framework_types() -> None:
    """R20: a projection of domain state, no terminal-framework types, no
    credential-bearing values. Anything that fails to serialize is by definition
    not a plain projection."""
    payload = status_payload(_long_turn(), mode="live").to_json_dict()
    round_tripped = json.loads(json.dumps(payload))
    assert round_tripped == payload


def test_usage_is_null_until_the_gateway_reports_any() -> None:
    assert status_payload(SessionState(), mode="replay").to_json_dict()["usage"] is None
    assert status_payload(_long_turn(), mode="replay").to_json_dict()["usage"] == {
        "input_tokens": 12,
        "output_tokens": 5,
    }


def test_connection_state_reaches_the_payload() -> None:
    state = set_connection(SessionState(), "reconnecting")
    assert status_payload(state, mode="live").connection == "reconnecting"


# ── KTD13: the v1 contract does not change meaning under a fleet ─────────


def test_pending_prompts_stays_the_focused_sessions_count_with_a_full_fleet() -> None:
    """KTD13's pin. ``pending_prompts`` is frozen v1 and means *this session*.

    Folding the install-wide needs-you count into it would silently change what
    every existing consumer reads — a status command written against v0.1 would
    start reporting another session's approval as this session's. The fleet
    count lives on the needs-you surface; ``docs/formats/status-line.md`` says
    the same thing to the external reader.
    """
    fleet = FleetState(focused_profile="default")
    fleet = route_frames(
        fleet,
        decode_all(
            [
                raw_event("message.start"),
                raw_event("clarify.request", {"request_id": "c-1", "question": "which?"}),
            ]
        ),
        profile="default",
    )
    # Three more sessions on the same connection, every one of them waiting.
    fleet = apply_active_list(
        fleet,
        decode_active_list(
            {
                "sessions": [
                    {"id": f"bg-{index}", "session_key": f"bg-{index}", "status": "waiting"}
                    for index in range(3)
                ]
            }
        ),
        profile="default",
        at=BASE_TIME + 50,
        poll_epoch=1,
    )

    assert fleet_queue(fleet).count == 4
    payload = status_payload(fleet.focused, mode="live")
    assert payload.pending_prompts == 1
    assert payload.turn == "waiting"


def test_a_prompt_parked_in_another_session_stays_out_of_the_v1_count() -> None:
    """The other half of the same pin, and the one that already shipped: the
    fleet queue counts it (R14 — the install's whole truth), the v1 payload does
    not (the focused session has nothing outstanding on screen)."""
    state = replay([raw_event("sudo.request", {"request_id": "s-1"}, session_id="other")])
    state = focus_session(state, "sess-focus")
    assert status_payload(state, mode="live").pending_prompts == 0
    assert [p.request_id for p in prompt_feed(state)] == ["s-1"]


# ── KTD12: a polled wait renders a floor, never a start time ─────────────


def test_a_poll_first_seen_wait_renders_waiting_at_least_the_observed_span() -> None:
    """KTD12's rendering, on the projection's own surface.

    No roster row and no approval payload carries a start stamp at any revision
    U1 examined, so the span since the first sighting is the only honest age.
    The floor renders with its ``≥``; a wait Talaria watched begin does not.
    """
    fleet = FleetState(focused_profile="default")
    fleet = apply_active_list(
        fleet,
        decode_active_list(
            {"sessions": [{"id": "bg", "session_key": "bg", "status": "waiting"}]}
        ),
        profile="default",
        at=BASE_TIME + 10,
        poll_epoch=1,
    )
    item = fleet_queue(fleet).items[0]
    assert item.age_is_floor is True
    assert wait_line(item, BASE_TIME + 100) == "waiting ≥ 90s"

    watched = replay([raw_event("clarify.request", {"request_id": "c-9", "question": "?"})])
    assert [row.opened_at for row in prompt_feed(watched)] == [BASE_TIME]


def test_the_new_projection_keeps_the_stamps_the_card_projection_drops() -> None:
    """Feed A exists because ``opened_at`` and ``seq`` are already on every
    prompt and ``prompt_view`` drops both — the queue orders by wait age and
    tie-breaks by arrival, and a card needs neither."""
    state = replay(
        [
            raw_event("clarify.request", {"request_id": "c-1", "question": "first?"}),
            raw_event("sudo.request", {"request_id": "u-1"}),
        ]
    )
    rows = prompt_feed(state)
    assert [row.request_id for row in rows] == ["c-1", "u-1"]
    assert [row.opened_at for row in rows] == [BASE_TIME, BASE_TIME + 1]
    assert [row.seq for row in rows] == [1, 2]
    assert all(row.in_flight is False for row in rows)


def test_a_prompt_whose_answer_is_travelling_is_still_in_the_feed_and_flagged() -> None:
    """R18: a row clears on gateway-confirmed resolution, not on the answer
    leaving. The prompt is out of ``prompts`` and in ``answering``, and feed A
    carries it with the flag that makes it render requested-with-age."""
    state = replay([raw_event("clarify.request", {"request_id": "c-1", "question": "?"})])
    state, refusal = respond_to_prompt(state, "c-1")
    assert refusal is None
    assert state.prompts == ()

    rows = prompt_feed(state)
    assert [row.request_id for row in rows] == ["c-1"]
    assert rows[0].in_flight is True


# ── Snapshots and change markers ─────────────────────────────────────────


def test_the_first_snapshot_marks_every_region_changed() -> None:
    snapshot = project(_long_turn())
    assert snapshot.changed == SNAPSHOT_REGIONS


def test_an_unchanged_state_marks_nothing_changed() -> None:
    state = _long_turn()
    first = project(state)
    second = project(state, previous=first)
    assert second.changed == frozenset()


def test_only_the_regions_that_moved_are_marked() -> None:
    state = _long_turn()
    first = project(state)
    after = replay([raw_event("subagent.start", {"subagent_id": "sa-1", "goal": "check"})], state)
    second = project(after, previous=first)
    assert second.changed == {"subagents", "status"}
    assert second.transcript == first.transcript


def test_a_snapshot_is_immutable_and_comparable() -> None:
    """AE2 compares projections across two replays. Comparing two values is
    trivial; comparing two mutation histories is not — which is the whole
    argument for snapshots over in-place mutation."""
    state = _long_turn()
    assert project(state) == project(state)
    assert hash(project(state).status) == hash(project(state).status)


def test_change_marker_names_are_a_closed_published_set() -> None:
    """A UI keyed off a typo would silently never re-render."""
    assert project(_long_turn()).changed <= SNAPSHOT_REGIONS
