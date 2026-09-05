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
    DETAIL_LINE_CLIP,
    TRANSCRIPT_LINE_CLIP,
    applies_to_focused_session,
    clip_detail_line,
    clip_transcript_line,
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
    """Updated for plan ``2026-08-11-v0-3-unit-b4-unknown-event-flood-plan.md``:
    an unknown type is announced once per connection rather than once per
    occurrence. The second arrival of the same type is counted in
    ``unknown_event_repeats`` and produces no transcript row (KTD3)."""
    state = replay(
        [
            raw_event("cauldron.bubbled", {}),
            raw_event("cauldron.bubbled", {}),
        ]
    )
    assert state.unknown_event_types == ("cauldron.bubbled",)
    unknown = [e for e in state.transcript if e.kind == "unknown-event"]
    assert len(unknown) == 1, "the type is announced once per connection, not once per occurrence"
    assert state.unknown_event_repeats == 1


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


#: The three payloads below are transcribed from a recording of a real gateway,
#: ``2026-08-04T16-02-24-964Z.jsonl``, rather than composed here. That matters:
#: every other fixture in this suite was written from a *reading* of Hermes, and
#: a reading is exactly what missed these three.
_OBSERVED_LIVE_FRAMES = (
    raw_event("sessions.changed", {}, session_id=None),
    raw_event(
        "session.title",
        {"session_id": "20260804_120225_7487d0", "title": "Testing the Assistant"},
    ),
    raw_event(
        "session.reclaimed",
        {
            "session_id": "e5696a04",
            "stored_session_id": "20260804_115840_20ea65",
            "reason": "ws_orphan_reap",
        },
        session_id=None,
    ),
    raw_event(
        "session.usage",
        {"usage": {"input": 1200, "output": 350, "total": 1550}},
        session_id="20260804_120225_7487d0",
    ),
)


def test_known_event_set_covers_what_a_live_gateway_actually_sends() -> None:
    """Three types a running gateway emitted that the source reading missed.

    All three are present in Hermes at ``7f4d15515``, the revision
    ``KNOWN_EVENT_TYPES`` was derived from, so this is a hole in the derivation
    rather than a newer Hermes. ``session.title`` is the instructive one: it is
    also a callable *method*, which is how a grep for the emitting site slid
    past it.
    """
    for frame in _OBSERVED_LIVE_FRAMES:
        event_type = frame["params"]["type"]
        assert isinstance(_decode(frame), GatewayEvent), event_type


def test_an_ordinary_live_turn_leaves_no_unknown_markers_in_the_transcript() -> None:
    """The cost of the gap was not correctness but noise.

    Two ordinary turns against a live gateway put *seven* ``unknown event type``
    lines into the transcript — ``sessions.changed`` alone fires several times
    per turn. The module's own docstring calls unknown events "expected
    traffic"; expected traffic that annotates itself on every occurrence drowns
    the conversation it is interleaved with.
    """
    state = replay(list(_OBSERVED_LIVE_FRAMES))
    assert state.unknown_event_types == ()
    assert [entry for entry in state.transcript if entry.kind == "unknown-event"] == []


# ── Unit B4: the unknown-event flood, and the latch that stops the next one ─

# Acceptance evidence from plan ``2026-08-11-v0-3-unit-b4-unknown-event-flood-plan.md``.
# AE1 through AE6 asserted below in the style of the existing domain suite.


def test_platforms_changed_is_known_not_merely_quiet() -> None:
    """AE1: A feed containing ``platforms.changed`` produces zero unknown-event
    rows, and ``unknown_event_types`` does not contain it — it is known now,
    not merely silent."""
    state = replay(
        [
            raw_event("platforms.changed", {}),
            raw_event("platforms.changed", {}),
            raw_event("platforms.changed", {}),
        ]
    )
    assert "platforms.changed" not in state.unknown_event_types
    unknown = [e for e in state.transcript if e.kind == "unknown-event"]
    assert unknown == []


def test_unknown_event_announced_once_twenty_six_arrivals_is_one_row() -> None:
    """AE2: A feed containing the same genuinely unknown type 26 times produces
    exactly one transcript row naming that type, and ``unknown_event_repeats == 25``."""
    frames = [raw_event("cauldron.bubbled", {"heat": i}) for i in range(26)]
    state = replay(frames)
    assert state.unknown_event_types == ("cauldron.bubbled",)
    unknown = [e for e in state.transcript if e.kind == "unknown-event"]
    assert len(unknown) == 1
    assert state.unknown_event_repeats == 25


def test_two_distinct_unknown_types_each_get_their_own_row() -> None:
    """AE3: A feed containing two distinct unknown types produces two rows,
    one per type, in arrival order. The latch is per type, not global."""
    state = replay(
        [
            raw_event("cauldron.bubbled", {}),
            raw_event("phial.fizzed", {}),
            raw_event("cauldron.bubbled", {}),
            raw_event("phial.fizzed", {}),
            raw_event("cauldron.bubbled", {}),
        ]
    )
    unknown = [e for e in state.transcript if e.kind == "unknown-event"]
    assert len(unknown) == 2
    assert unknown[0].text == "unknown event type: cauldron.bubbled"
    assert unknown[1].text == "unknown event type: phial.fizzed"
    assert state.unknown_event_types == ("cauldron.bubbled", "phial.fizzed")
    # 5 arrivals total, 2 produced rows, so 3 repeats
    assert state.unknown_event_repeats == 3


def test_reconnect_resets_the_unknown_event_latch() -> None:
    """AE4: A connection status change resets the latch — the same unknown type
    announced once, then a reconnect, then the same type again, produces a second
    row and ``unknown_event_repeats == 0``."""
    from talaria.domain.state import set_connection

    # First connection: announce the type once, then another arrival is latched.
    state = replay(
        [
            raw_event("cauldron.bubbled", {}),
            raw_event("cauldron.bubbled", {}),
        ]
    )
    assert len([e for e in state.transcript if e.kind == "unknown-event"]) == 1
    assert state.unknown_event_types == ("cauldron.bubbled",)
    assert state.unknown_event_repeats == 1

    # Reconnect resets the latch.
    state = set_connection(state, "reconnecting")
    assert state.unknown_event_types == ()
    assert state.unknown_event_repeats == 0

    # Second connection: the same type is announced again.
    state = replay([raw_event("cauldron.bubbled", {})], state=state)
    unknown = [e for e in state.transcript if e.kind == "unknown-event"]
    assert len(unknown) == 2, "reconnect resets the latch, so the type is announced again"
    assert state.unknown_event_repeats == 0


def test_session_switch_does_not_reset_the_unknown_event_latch() -> None:
    """AE5: A session switch does NOT reset the latch — the same feed with a
    ``focus_session`` between two occurrences produces one row. AE4 and AE5 are a
    pair, and the pair is what makes the latch's lifetime a decision on the
    record rather than whichever behaviour fell out."""
    from talaria.domain.state import SessionState, focus_session

    # First session: announce the type once.
    state = replay(
        [raw_event("cauldron.bubbled", {}, session_id="sess-one")],
        state=SessionState(focused_session_id="sess-one"),
    )
    assert len([e for e in state.transcript if e.kind == "unknown-event"]) == 1

    # Switch to a different session — this must NOT reset the latch.
    state = focus_session(state, "sess-two")
    assert state.unknown_event_types == ("cauldron.bubbled",), (
        "focus_session leaves the latch alone; see KTD3 in the B4 plan"
    )

    # The same type arrives in the new session — latched, no second row.
    state = replay(
        [raw_event("cauldron.bubbled", {}, session_id="sess-two")], state=state
    )
    unknown = [e for e in state.transcript if e.kind == "unknown-event"]
    assert len(unknown) == 1, "session switch did not reset the latch — one row total"
    assert state.unknown_event_repeats == 1


def test_background_session_unknown_event_is_dropped() -> None:
    """AE6: An unknown event naming a background session produces zero transcript
    rows and increments ``cross_session_events_ignored`` by one — the cross-session
    guard for ``UnknownEventFrame`` (KTD5)."""
    from talaria.domain.state import SessionState

    state = replay(
        [
            raw_event("gateway.ready", {}, session_id=None),
            raw_event("cauldron.bubbled", {}, session_id="sess-background"),
        ],
        state=SessionState(focused_session_id="sess-focus"),
    )
    unknown = [e for e in state.transcript if e.kind == "unknown-event"]
    assert unknown == [], "a background session's unknown event must not write a row"
    assert state.cross_session_events_ignored == 1
    assert state.unknown_event_types == ()
    assert state.unknown_event_repeats == 0


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


def test_clip_detail_line_marks_the_cut() -> None:
    clipped = clip_detail_line("x" * 200)
    assert len(clipped) == DETAIL_LINE_CLIP + 1
    assert clipped.endswith("…")


def test_clip_transcript_line_marks_the_cut() -> None:
    clipped = clip_transcript_line("x" * (TRANSCRIPT_LINE_CLIP + 80))
    assert len(clipped) == TRANSCRIPT_LINE_CLIP + 1
    assert clipped.endswith("…")


#: Transcribed from a live gateway's ``status.update``. Hermes emits this whole
#: — its handler passes ``p.text`` to ``pushActivity`` with no ``slice`` — and
#: Talaria used to cut it at 120 characters, landing mid-identifier on
#: ``context_file_max_`` and discarding "chars, or use a larger-context model!".
#: The remedy is the only part of a warning worth reading.
_TRUNCATION_WARNING = (
    "⚠️  Context file AGENTS.md TRUNCATED: 74668 chars exceeds limit of 65280 "
    "— trim the file, pin a larger context_file_max_chars, or use a "
    "larger-context model!"
)


def test_a_gateway_warning_reaches_the_transcript_with_its_remedy_intact() -> None:
    """The measured regression: a 157-character warning, cut at 120.

    Asserted on the *remedy* rather than on a length, because the defect was
    never that the line was shortened — it was that the half naming the fix is
    the half a clip takes first.
    """
    state = replay(
        [raw_event("status.update", {"text": _TRUNCATION_WARNING, "kind": "warn"})]
    )
    written = next(e.text for e in state.transcript if "AGENTS.md" in e.text)
    assert written == _TRUNCATION_WARNING
    assert "use a larger-context model!" in written
    assert "context_file_max_chars" in written, "the setting must stay copyable"
    assert not written.endswith("…")


def test_a_subagent_detail_line_is_still_bound_to_one_row() -> None:
    """Loosening the transcript bound must not loosen this one.

    A sub-agent row renders its ``detail`` on a single screen line, so here the
    cut is a layout constraint rather than a judgement about diagnostics.
    """
    state = replay(
        [
            raw_event("subagent.start", {"subagent_id": "sa-1", "task_index": 0}),
            raw_event("subagent.progress", {"subagent_id": "sa-1", "text": "y" * 400}),
        ]
    )
    detail = state.subagents[0].detail[-1]
    assert len(detail) == DETAIL_LINE_CLIP + 1
    assert detail.endswith("…")


def test_coerce_text_never_stringifies_arbitrary_values() -> None:
    assert coerce_text("  hi  ") == "hi"
    assert coerce_text({"leak": "me"}) == ""
    assert coerce_text(None) == ""


def test_frame_time_parses_iso_and_never_reads_a_clock() -> None:
    expected = datetime(2026, 8, 2, 12, 21, 35, 16_000, tzinfo=UTC).timestamp()
    assert parse_frame_time("2026-08-02T12:21:35.016Z") == pytest.approx(expected)
    assert parse_frame_time("not a time") == 0.0
