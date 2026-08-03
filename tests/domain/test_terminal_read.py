"""KTD10 / PC9 — terminal-read is served from the projection, truthfully.

Two semantics come straight from the gateway's own contract and are not open to
reinterpretation (``tools/read_terminal_tool.py:18-31``, ``:58-70``): omitting
both arguments means **the visible screen**, and valid lines are the half-open
range ``[0, total_lines)`` with floors of ``0`` for ``start`` and ``1`` for
``count``.

The rest of this file is about honesty. An empty transcript answers ``0``; an
unavailable projection answers *nothing at all* and lets the gateway's own
30-second expiry fire (``tui_gateway/server.py:2981-2998``), because a fabricated
screen is a confident wrong answer an agent will act on.
"""

from __future__ import annotations

import pytest

from talaria.domain.projection import (
    ProjectionUnavailableError,
    TranscriptView,
    terminal_read,
    transcript_view,
)
from talaria.domain.state import SessionState

from .conftest import raw_event, replay


def _view(count: int) -> TranscriptView:
    return TranscriptView(lines=tuple(f"line-{i}" for i in range(count)), entry_count=count)


def test_omitting_both_arguments_returns_the_visible_screen_not_the_transcript() -> None:
    response = terminal_read(_view(100), viewport_rows=10)
    assert response.total_lines == 100
    assert response.start == 90
    assert response.end == 100
    assert response.text.splitlines()[0] == "line-90"


def test_a_short_transcript_returns_all_of_it() -> None:
    response = terminal_read(_view(3), viewport_rows=10)
    assert (response.start, response.end) == (0, 3)
    assert response.text == "line-0\nline-1\nline-2"


def test_start_and_count_select_a_window() -> None:
    response = terminal_read(_view(100), viewport_rows=10, start_line=5, count=3)
    assert (response.start, response.end) == (5, 8)
    assert response.text == "line-5\nline-6\nline-7"


def test_start_is_floored_at_zero_and_count_at_one() -> None:
    """``tools/read_terminal_tool.py:29-30`` clamps with exactly these floors."""
    response = terminal_read(_view(10), viewport_rows=5, start_line=-40, count=0)
    assert response.start == 0
    assert response.end == 1


def test_a_window_running_past_the_end_stops_at_total_lines() -> None:
    response = terminal_read(_view(10), viewport_rows=5, start_line=8, count=100)
    assert (response.start, response.end) == (8, 10)
    assert response.text == "line-8\nline-9"


def test_a_start_beyond_the_end_answers_empty_rather_than_erroring() -> None:
    response = terminal_read(_view(10), viewport_rows=5, start_line=999)
    assert (response.start, response.end) == (10, 10)
    assert response.text == ""


def test_an_empty_transcript_answers_honestly() -> None:
    response = terminal_read(transcript_view(SessionState()), viewport_rows=24)
    assert response.total_lines == 0
    assert response.text == ""
    assert (response.start, response.end) == (0, 0)


def test_viewport_rows_is_served_as_the_real_rendered_height() -> None:
    assert terminal_read(_view(50), viewport_rows=31).viewport_rows == 31


def test_cursor_row_is_null_because_the_transcript_has_no_caret() -> None:
    """The only caret on screen belongs to the composer, which is not part of
    the transcript the agent asked to read. An explicit ``null`` is information
    it can act on; a synthesised row is not."""
    assert terminal_read(_view(5), viewport_rows=5).cursor_row is None
    assert terminal_read(_view(5), viewport_rows=5).to_json_dict()["cursor_row"] is None


def test_the_response_carries_exactly_the_contract_field_set() -> None:
    payload = terminal_read(_view(5), viewport_rows=5).to_json_dict()
    assert set(payload) == {
        "total_lines",
        "start",
        "end",
        "viewport_rows",
        "cursor_row",
        "text",
    }


def test_an_unavailable_projection_sends_nothing_rather_than_a_fabrication() -> None:
    with pytest.raises(ProjectionUnavailableError):
        terminal_read(None, viewport_rows=24)


def test_terminal_read_serves_the_live_transcript_buffer() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "alpha\nbeta\ngamma"}),
        ]
    )
    response = terminal_read(transcript_view(state), viewport_rows=2)
    assert response.total_lines == 3
    assert response.text == "beta\ngamma"
