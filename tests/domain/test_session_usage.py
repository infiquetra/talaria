"""Tests for C6 (#145): session.usage gateway event handling and token accounting."""

from __future__ import annotations

import json
from pathlib import Path

from talaria.domain.changes import inspector_view
from talaria.domain.decode import KNOWN_EVENT_TYPES, decode_frame
from talaria.domain.models import GatewayEvent, Usage
from talaria.domain.projection import entry_scoped_view, status_payload
from talaria.domain.state import SessionState
from talaria.ui.inspector import _context_lines

from .conftest import BASE_TIME, raw_event, replay

_FIXTURES_DIR = Path(__file__).resolve().parents[1] / "fixtures"
_USAGE_FIXTURE_PATH = _FIXTURES_DIR / "events" / "session_usage.json"


def _decode(frame: object) -> object:
    return decode_frame(frame, at=BASE_TIME, seq=1)


def test_session_usage_is_known_event_and_decodes_cleanly() -> None:
    assert "session.usage" in KNOWN_EVENT_TYPES
    frame = raw_event(
        "session.usage",
        {"usage": {"input": 1200, "output": 350, "total": 1550}},
        session_id="sess-1",
    )
    decoded = _decode(frame)
    assert isinstance(decoded, GatewayEvent)
    assert decoded.type == "session.usage"
    assert decoded.session_id == "sess-1"
    assert decoded.payload == {"usage": {"input": 1200, "output": 350, "total": 1550}}


def test_session_usage_produces_no_transcript_or_unknown_rows() -> None:
    frames = [
        raw_event("message.start", session_id="sess-1"),
        raw_event(
            "session.usage",
            {"usage": {"input": 100, "output": 20}},
            session_id="sess-1",
        ),
        raw_event(
            "session.usage",
            {"usage": {"input": 150, "output": 50}},
            session_id="sess-1",
        ),
    ]
    state = replay(frames)
    assert state.unknown_event_types == ()
    assert state.unknown_event_repeats == 0
    assert not any(e.kind == "unknown-event" for e in state.transcript)
    assert not any("usage" in e.text for e in state.transcript)


def test_session_usage_merges_counters_live_and_advances() -> None:
    state = replay(
        [
            raw_event("message.start", session_id="sess-1"),
            raw_event(
                "session.usage",
                {"usage": {"input": 1200, "output": 350}},
                session_id="sess-1",
            ),
        ]
    )
    assert state.usage.observed is True
    assert state.usage.input_tokens == 1200
    assert state.usage.output_tokens == 350

    next_state = replay(
        [
            raw_event(
                "session.usage",
                {"usage": {"output": 400}},
                session_id="sess-1",
            )
        ],
        state=state,
    )
    assert next_state.usage.observed is True
    assert next_state.usage.input_tokens == 1200
    assert next_state.usage.output_tokens == 400


def test_session_usage_accepts_wire_token_aliases() -> None:
    u1 = Usage().merged_with({"input": 10, "output": 20})
    assert u1 == Usage(input_tokens=10, output_tokens=20, observed=True)

    u2 = Usage().merged_with({"prompt": 30, "completion": 40})
    assert u2 == Usage(input_tokens=30, output_tokens=40, observed=True)

    u3 = Usage().merged_with({"input_tokens": 50, "output_tokens": 60})
    assert u3 == Usage(input_tokens=50, output_tokens=60, observed=True)


def test_missing_data_distinguished_from_measured_zero() -> None:
    fresh = SessionState()
    assert fresh.usage.observed is False
    payload_unobserved = status_payload(fresh, mode="live")
    assert payload_unobserved.input_tokens is None
    assert payload_unobserved.output_tokens is None
    assert payload_unobserved.to_json_dict()["usage"] is None

    empty_tick = replay(
        [raw_event("session.usage", {"usage": {}}, session_id="sess-1")],
        state=fresh,
    )
    assert empty_tick.usage.observed is False
    payload_empty = status_payload(empty_tick, mode="live")
    assert payload_empty.input_tokens is None
    assert payload_empty.output_tokens is None
    assert payload_empty.to_json_dict()["usage"] is None

    zero_tick = replay(
        [raw_event("session.usage", {"usage": {"input": 0, "output": 0}}, session_id="sess-1")],
        state=fresh,
    )
    assert zero_tick.usage.observed is True
    assert zero_tick.usage.input_tokens == 0
    assert zero_tick.usage.output_tokens == 0
    payload_zero = status_payload(zero_tick, mode="live")
    assert payload_zero.input_tokens == 0
    assert payload_zero.output_tokens == 0
    assert payload_zero.to_json_dict()["usage"] == {"input_tokens": 0, "output_tokens": 0}


def test_session_usage_cross_session_guard() -> None:
    initial = SessionState()
    state = replay([raw_event("message.start", session_id="focus-sess")], state=initial)
    assert state.focused_session_id == "focus-sess"
    assert state.usage.observed is False

    after_bg = replay(
        [
            raw_event(
                "session.usage",
                {"usage": {"input": 9999, "output": 9999}},
                session_id="other-sess",
            )
        ],
        state=state,
    )
    assert after_bg.usage.observed is False
    assert after_bg.cross_session_events_ignored == 1


def test_session_usage_composed_producer_fixture() -> None:
    """Validate JSON-RPC 2.0 fixture composed from Hermes source reading (issue #145 / I5 finding;
    Hermes 63279301b tui_gateway/server.py:13125-13175). A future Live 11 test capture can
    replace this fixture with recorded frames when run.
    """
    assert _USAGE_FIXTURE_PATH.is_file(), f"Fixture missing: {_USAGE_FIXTURE_PATH}"
    data = json.loads(_USAGE_FIXTURE_PATH.read_text(encoding="utf-8"))
    decoded = decode_frame(data, at=BASE_TIME, seq=1)
    assert isinstance(decoded, GatewayEvent)
    assert decoded.type == "session.usage"
    assert decoded.session_id == "20260807_125539_42afdb"

    state = replay([data])
    assert state.focused_session_id == "20260807_125539_42afdb"
    assert state.usage.observed is True
    assert state.usage.input_tokens == 1200
    assert state.usage.output_tokens == 350
    assert state.unknown_event_types == ()


def test_inspector_usage_row_presentation() -> None:
    state_unobserved = SessionState()
    view_unobserved = inspector_view(
        entry_scoped_view(state_unobserved), usage=state_unobserved.usage
    )
    lines_unobserved = _context_lines(view_unobserved)
    assert not any(line.strip().startswith("usage") for line in lines_unobserved)

    state_zero = SessionState()
    zero_usage = state_zero.usage.merged_with({"input": 0, "output": 0})
    view_zero = inspector_view(entry_scoped_view(state_zero), usage=zero_usage)
    lines_zero = _context_lines(view_zero)
    assert any("usage    0 input · 0 output" in line for line in lines_zero)

    state_active = SessionState()
    active_usage = state_active.usage.merged_with({"input": 1200, "output": 350})
    view_active = inspector_view(entry_scoped_view(state_active), usage=active_usage)
    lines_active = _context_lines(view_active)
    assert any("usage    1200 input · 350 output" in line for line in lines_active)
