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
    status_payload,
    transcript_view,
)
from talaria.domain.state import SessionState, set_connection

from .conftest import raw_event, replay


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
