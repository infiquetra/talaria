"""R4: a cancelled turn stays cancelled, and it shows that it was cancelled.

Two halves, and the second is the one that gets forgotten. Making cancellation
terminal is a guard; making the transcript say *why* the turn ended is a
requirement — R4 asks that the transcript show the turn "was cancelled rather
than that it ended", which is a different sentence from "no more text arrived".
"""

from __future__ import annotations

from talaria.domain.projection import status_payload, turn_status
from talaria.domain.state import SessionState, cancel_turn

from .conftest import BASE_TIME, raw_event, replay


def _cancelled_mid_stream() -> SessionState:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "half an answer"}),
        ]
    )
    return cancel_turn(state, at=BASE_TIME + 10)


def test_cancelling_preserves_the_partial_answer_and_marks_it() -> None:
    state = _cancelled_mid_stream()
    assert state.turn == "cancelled"
    assert [e.text for e in state.transcript if e.kind == "assistant"] == [
        "half an answer\n\n*[interrupted]*"
    ]


def test_cancelling_with_nothing_streamed_still_leaves_a_trace() -> None:
    """``interruptTurn`` (``turnController.ts:322-331``) emits a bare note when
    there is no partial text, "so the transcript always records that the turn
    was cancelled"."""
    state = cancel_turn(replay([raw_event("message.start")]), at=BASE_TIME + 5)
    assert [e.kind for e in state.transcript] == ["cancelled"]
    assert state.turn == "cancelled"


def test_a_late_completion_cannot_overwrite_the_cancelled_state() -> None:
    state = _cancelled_mid_stream()
    after = replay([raw_event("message.complete", {"text": "the full answer"})], state)

    assert after.turn == "cancelled"
    assert "the full answer" not in "\n".join(e.text for e in after.transcript)
    assert after.late_events_ignored == 1


def test_a_late_completion_still_merges_its_usage() -> None:
    """Token accounting describes what the provider billed. It is not a claim
    about the turn's outcome, so a cancelled turn still records it."""
    state = _cancelled_mid_stream()
    after = replay(
        [raw_event("message.complete", {"usage": {"input_tokens": 9, "output_tokens": 3}})],
        state,
    )
    assert after.usage.input_tokens == 9
    assert after.usage.output_tokens == 3


def test_late_deltas_and_tool_events_after_cancelling_are_ignored() -> None:
    state = _cancelled_mid_stream()
    after = replay(
        [
            raw_event("message.delta", {"text": " more text"}),
            raw_event("reasoning.delta", {"text": "more thinking"}),
            raw_event("tool.start", {"tool_id": "t9", "name": "write_file"}),
        ],
        state,
    )
    assert after.streaming_text == ""
    assert after.reasoning_text == ""
    assert after.late_events_ignored == 3
    assert [e.kind for e in after.transcript] == [e.kind for e in state.transcript]


def test_a_new_turn_clears_the_cancelled_state() -> None:
    """Cancellation is terminal for the turn it cancelled, not for the session.
    ``startMessage`` is the only transition that clears it, matching Hermes's
    ``interrupted = false`` at ``turnController.ts:989``."""
    state = _cancelled_mid_stream()
    after = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "second attempt"}),
        ],
        state,
    )
    assert after.turn == "streaming"
    assert after.streaming_text == "second attempt"


def test_an_error_after_cancelling_does_not_become_a_second_outcome() -> None:
    state = _cancelled_mid_stream()
    after = replay([raw_event("error", {"message": "provider hung up"})], state)
    assert after.turn == "cancelled"
    assert [e.text for e in after.transcript if e.kind == "error"] == [
        "error: provider hung up"
    ]


def test_cancelling_an_idle_turn_does_nothing() -> None:
    idle = replay([raw_event("message.start"), raw_event("message.complete", {"text": "hi"})])
    assert cancel_turn(idle, at=BASE_TIME + 99) == idle


def test_a_cancelled_turn_reports_cancelled_in_the_status_payload() -> None:
    state = _cancelled_mid_stream()
    assert turn_status(state) == "cancelled"
    assert status_payload(state, mode="replay").turn == "cancelled"


def test_an_error_settles_a_streaming_turn() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "partial"}),
            raw_event("error", {"message": "boom"}),
        ]
    )
    assert state.turn == "idle"
    assert state.streaming_text == ""


def test_a_delta_without_a_start_opens_a_visible_synthetic_turn() -> None:
    """AE2 names "missing start" as a sequence that must land in a catalogued
    outcome. Hermes drops it; R6 forbids dropping content, so Talaria opens the
    turn, counts it, and says so in the transcript."""
    state = replay([raw_event("message.delta", {"text": "orphaned text"})])
    assert state.turn == "streaming"
    assert state.synthetic_turn_starts == 1
    assert state.transcript[0].text == "stream began without a message.start event"
    assert state.streaming_text == "orphaned text"
