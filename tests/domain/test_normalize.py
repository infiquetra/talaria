"""R5 and the pure half of the reconciliation catalogue.

R5 asks for two things that pull against each other: an unrecognized event must
be *surfaced by type*, and a malformed frame must be surfaced *without rendering
untrusted raw bytes*. The tests below pin both, and the second one is asserted
negatively — the payload's distinctive content must not appear anywhere in the
transcript — because the failure mode is a diagnostic that helpfully includes
"the offending value" and quietly ships a credential to the screen.
"""

from __future__ import annotations

from datetime import UTC, datetime

import pytest

from talaria.domain.decode import (
    KNOWN_EVENT_TYPES,
    NonEventFrame,
    ProtocolErrorFrame,
    UnknownEventFrame,
    decode_frame,
)
from talaria.domain.models import GatewayEvent, SubagentStatus
from talaria.domain.normalize import (
    applies_to_focused_session,
    clip_system_line,
    coerce_text,
    is_terminal_status,
    keep_terminal_else,
    normalize_subagent_status,
    parse_frame_time,
    push_unique,
    subagent_identity,
    subagent_sort_key,
)

from .conftest import BASE_TIME, raw_event, replay


def _decode(frame: object) -> object:
    return decode_frame(frame, at=BASE_TIME, seq=1)


# ── R5: unknown event types ──────────────────────────────────────────────


def test_unknown_event_type_is_surfaced_by_name() -> None:
    decoded = _decode(raw_event("cauldron.bubbled", {"heat": 11}))
    assert isinstance(decoded, UnknownEventFrame)
    assert decoded.type == "cauldron.bubbled"
    assert decoded.text == "unknown event type: cauldron.bubbled"


def test_unknown_event_reaches_the_transcript_and_is_counted_once() -> None:
    state = replay(
        [
            raw_event("cauldron.bubbled", {}),
            raw_event("cauldron.bubbled", {}),
        ]
    )
    assert state.unknown_event_types == ("cauldron.bubbled",)
    unknown = [e for e in state.transcript if e.kind == "unknown-event"]
    assert len(unknown) == 2, "every occurrence is shown; only the type list dedupes"


def test_unknown_event_payload_never_reaches_the_transcript() -> None:
    state = replay([raw_event("cauldron.bubbled", {"secret": "hunter2-in-the-payload"})])
    assert "hunter2-in-the-payload" not in "\n".join(e.text for e in state.transcript)


def test_known_event_set_covers_the_four_blocking_bridges() -> None:
    """``terminal.read.request`` is a bridge the shipping Hermes TUI never
    handles; decoding it as unknown would make every desktop-style read show up
    as protocol noise."""
    for event_type in (
        "approval.request",
        "clarify.request",
        "secret.request",
        "sudo.request",
        "terminal.read.request",
    ):
        assert event_type in KNOWN_EVENT_TYPES


# ── R5: malformed frames ─────────────────────────────────────────────────


@pytest.mark.parametrize(
    ("frame", "reason"),
    [
        ("not-an-object", "frame-not-an-object"),
        ({"nonsense": True}, "frame-not-recognizable"),
        ({"method": "event", "params": "nope"}, "event-params-not-an-object"),
        ({"method": "event", "params": {}}, "event-type-missing"),
        ({"method": "event", "params": {"type": 7}}, "event-type-not-a-string"),
        (
            {"method": "event", "params": {"type": "message.delta", "payload": []}},
            "event-payload-not-an-object",
        ),
    ],
)
def test_malformed_frame_becomes_a_categorical_protocol_error(
    frame: object, reason: str
) -> None:
    decoded = _decode(frame)
    assert isinstance(decoded, ProtocolErrorFrame)
    assert decoded.reason == reason


def test_recorder_parse_error_is_a_protocol_error_not_an_empty_frame() -> None:
    decoded = decode_frame(None, at=BASE_TIME, seq=1, parse_error=True)
    assert isinstance(decoded, ProtocolErrorFrame)
    assert decoded.reason == "frame-unparseable"


def test_protocol_error_text_contains_no_payload_fragment() -> None:
    state = replay(
        [
            {
                "method": "event",
                "params": {"type": "message.delta", "payload": ["sk-live-do-not-log"]},
            }
        ]
    )
    rendered = "\n".join(e.text for e in state.transcript)
    assert "sk-live-do-not-log" not in rendered
    assert "protocol error" in rendered


def test_protocol_noise_is_announced_once_per_connection() -> None:
    state = replay([{"bad": 1}, {"bad": 2}, {"bad": 3}])
    announcements = [
        e for e in state.transcript if e.text == "protocol noise detected on this connection"
    ]
    assert len(announcements) == 1
    assert state.protocol_error_count == 3


def test_rpc_traffic_is_not_a_protocol_error() -> None:
    """A recorded corpus carries responses and the client's own method calls.
    Classifying them as errors would fill a healthy transcript with noise."""
    response = _decode({"id": 4, "result": {"status": "ok"}})
    request = _decode({"id": 5, "method": "prompt.submit", "params": {}})
    assert isinstance(response, NonEventFrame) and response.kind == "response"
    assert isinstance(request, NonEventFrame) and request.kind == "request"

    state = replay([{"id": 4, "result": {"status": "ok"}}])
    assert state.transcript == ()


# ── Pure reconciliation helpers ──────────────────────────────────────────


@pytest.mark.parametrize(
    ("wire", "expected"),
    [
        ("completed", "completed"),
        ("COMPLETED", "completed"),
        ("Timeout", "timeout"),
        ("exploded", "running"),
        (None, "running"),
        (7, "running"),
    ],
)
def test_unknown_subagent_status_falls_back_safely(wire: object, expected: str) -> None:
    assert normalize_subagent_status(wire, "running") == expected


@pytest.mark.parametrize(
    "status", ["completed", "error", "failed", "interrupted", "timeout"]
)
def test_every_terminal_status_survives_a_later_proposal(status: SubagentStatus) -> None:
    assert is_terminal_status(status)
    assert keep_terminal_else(status, "running") == status


@pytest.mark.parametrize("status", ["queued", "running"])
def test_non_terminal_status_accepts_a_later_proposal(status: SubagentStatus) -> None:
    assert not is_terminal_status(status)
    assert keep_terminal_else(status, "running") == "running"


def test_subagent_identity_prefers_the_server_issued_id() -> None:
    assert subagent_identity({"subagent_id": "sa-77", "task_index": 3}) == "sa-77"


def test_subagent_identity_falls_back_to_a_composite_key() -> None:
    assert subagent_identity({"task_index": 3, "goal": "read docs"}) == "sa:3:read docs"
    assert subagent_identity({}) == "sa:0:subagent"


def test_subagent_sort_key_is_total() -> None:
    """Hermes sorts on (depth, index) alone, which leaves ties in arrival order.
    Under a reordered replay that is not the same order twice, so the identity
    is appended to make the comparison total."""
    first = subagent_sort_key(0, 0, "sa-b")
    second = subagent_sort_key(0, 0, "sa-a")
    assert sorted([first, second]) == [second, first]


def test_cross_session_events_are_scoped_and_gateway_events_are_not() -> None:
    focused = "sess-focus"
    mine = GatewayEvent("message.delta", focused, {}, BASE_TIME, 1)
    theirs = GatewayEvent("message.delta", "sess-other", {}, BASE_TIME, 2)
    transport = GatewayEvent("gateway.ready", "sess-other", {}, BASE_TIME, 3)

    assert applies_to_focused_session(mine, focused)
    assert not applies_to_focused_session(theirs, focused)
    assert applies_to_focused_session(transport, focused)


def test_push_unique_skips_a_repeated_tail_and_bounds_growth() -> None:
    items = push_unique((), "a")
    items = push_unique(items, "a")
    assert items == ("a",)
    for index in range(12):
        items = push_unique(items, f"line-{index}")
    assert len(items) == 8


def test_clip_system_line_marks_the_cut() -> None:
    clipped = clip_system_line("x" * 200)
    assert len(clipped) == 121
    assert clipped.endswith("…")


def test_coerce_text_never_stringifies_arbitrary_values() -> None:
    assert coerce_text("  hi  ") == "hi"
    assert coerce_text({"leak": "me"}) == ""
    assert coerce_text(None) == ""


def test_frame_time_parses_iso_and_never_reads_a_clock() -> None:
    expected = datetime(2026, 8, 2, 12, 21, 35, 16_000, tzinfo=UTC).timestamp()
    assert parse_frame_time("2026-08-02T12:21:35.016Z") == pytest.approx(expected)
    assert parse_frame_time("not a time") == 0.0
