"""``thinking.delta`` is the spinner, not the reasoning.

The two channels look alike on the wire and are not alike at all. Hermes says
which is which in its own source: ``thinking_callback`` is "bridged to the
``thinking.delta`` event, which both render as the live spinner/status line"
(``run_agent.py:1047``), while the model's actual thinking reaches the client
through ``agent._fire_reasoning_delta`` and arrives as ``reasoning.delta``
(``chat_completion_helpers.py:3629-3633``). Talaria routed both into one
accumulating string, so a spinner frame was concatenated onto the front of the
reasoning block and the transcript read

    · (◐) indexing...The user wants me to reply with exactly a specific string

which is the line this file exists to keep from coming back. The opening test
replays the exact frames that produced it, taken from a recording of a live
gateway rather than composed here.

Two claims are load-bearing beyond that line:

*R6 is untouched.* Its obligation is that reasoning-block content is never
dropped. The reasoning block arrives on ``reasoning.delta``, which still
accumulates in full and is still committed at turn end, and the tests that say
so are here rather than left to the transcript suite, because this change is
exactly the kind that satisfies a requirement for the channel it moved and
quietly breaks the one it didn't.

*The note is not dropped either.* It is replaced in place and shown on the
activity line, which is one row and describes right now — the shape Hermes gave
it. The precedence around that row is tested in ``tests/ui/test_prompts.py``,
because it is a screen decision; what is tested here is that the value reaches
the projection at all.
"""

from __future__ import annotations

from typing import Any

from talaria.domain.normalize import DETAIL_LINE_CLIP
from talaria.domain.projection import prompt_view, transcript_view
from talaria.domain.state import SessionState, cancel_turn, focus_session

from .conftest import raw_event, replay

#: Exactly what the gateway sent on 2026-08-04, frames 21-25 of
#: ``2026-08-04T18-38-10-881Z.jsonl``: a spinner frame, the empty frame that
#: retires it, and the reasoning that followed. The empty frame matters — the
#: old handler ignored it because it ignored every falsy text, so the spinner it
#: was sent to clear stayed in the buffer.
LIVE_CAPTURE: list[dict[str, Any]] = [
    raw_event("message.start"),
    raw_event("thinking.delta", {"text": "(◐) indexing..."}),
    raw_event("thinking.delta", {"text": ""}),
    raw_event("reasoning.delta", {"text": "The"}),
    raw_event("reasoning.delta", {"text": " user"}),
    raw_event("reasoning.delta", {"text": " wants"}),
    raw_event("message.complete", {"text": "Answer."}),
]


def _reasoning(state: SessionState) -> list[str]:
    return [entry.text for entry in state.transcript if entry.kind == "reasoning"]


def _kinds(state: SessionState) -> list[str]:
    return [entry.kind for entry in state.transcript]


# ── the reported line ────────────────────────────────────────────────────


def test_the_spinner_frame_never_reaches_the_reasoning_entry() -> None:
    """The defect, replayed from the capture that showed it."""
    state = replay(LIVE_CAPTURE)
    assert _reasoning(state) == ["The user wants"]
    assert "◐" not in "\n".join(transcript_view(state).lines)


def test_the_transcript_holds_nothing_the_spinner_put_there() -> None:
    """Not just the reasoning entry — no entry of any kind.

    Asserted separately because a fix that merely stopped *concatenating* would
    pass the test above while appending the spinner as its own line, which is
    the same spam one row further down.
    """
    state = replay(LIVE_CAPTURE)
    assert _kinds(state) == ["reasoning", "assistant"]


def test_a_spinner_does_not_make_the_whole_reasoning_block_be_refused() -> None:
    """The same defect's worse face: not a prefix, a wholesale drop.

    ``reasoning.available`` delivers the complete block, and it declines to
    overwrite deltas that already built one — correctly, or the block would
    duplicate what is on screen. A spinner frame in the delta buffer looks
    exactly like that, so the guard fired and the entire reasoning block was
    discarded in favour of the spinner. This is the sequence a live gateway sent
    on 2026-08-04 (frames 20 and 26 of ``2026-08-04T19-00-12-373Z.jsonl``): with
    the spinner routed to the reasoning buffer, the transcript's only reasoning
    entry was ``(▶) optimizing...`` and the model's reasoning never appeared —
    content loss, which is the thing R6 forbids outright.
    """
    state = replay(
        [
            raw_event("message.start"),
            raw_event("thinking.delta", {"text": "(▶) optimizing..."}),
            raw_event("reasoning.available", {"text": "the block the model wrote"}),
            raw_event("message.complete", {"text": "ALPHA"}),
        ]
    )
    assert _reasoning(state) == ["the block the model wrote"]


def test_the_reasoning_stream_is_still_accumulated_in_full() -> None:
    """R6's surviving obligation, on the channel that actually carries it."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("reasoning.delta", {"text": "first"}),
            raw_event("reasoning.delta", {"text": " and second"}),
            raw_event("message.complete", {"text": "done"}),
        ]
    )
    assert _reasoning(state) == ["first and second"]


# ── the note itself ──────────────────────────────────────────────────────


def test_the_note_is_replaced_rather_than_accumulated() -> None:
    """The whole difference between a status line and a transcript."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("thinking.delta", {"text": "(◐) indexing..."}),
            raw_event("thinking.delta", {"text": "(◓) waiting on the provider"}),
        ]
    )
    assert state.thinking_notice == "(◓) waiting on the provider"


def test_an_empty_note_retires_the_one_before_it() -> None:
    """Hermes's own convention: an empty value falls back to the busy status.

    This is the frame the old handler dropped, and dropping it is why a spinner
    the gateway had already cleared was still on screen at turn end.
    """
    state = replay(
        [
            raw_event("message.start"),
            raw_event("thinking.delta", {"text": "(◐) indexing..."}),
            raw_event("thinking.delta", {"text": "   "}),
        ]
    )
    assert state.thinking_notice == ""


def test_a_note_that_is_not_a_string_is_left_alone() -> None:
    """A malformed payload must not blank a note that is still true."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("thinking.delta", {"text": "(◐) indexing..."}),
            raw_event("thinking.delta", {"text": None}),
            raw_event("thinking.delta", {}),
        ]
    )
    assert state.thinking_notice == "(◐) indexing..."


def test_a_long_note_is_cut_to_one_row() -> None:
    """The activity line does not wrap, so an unbounded note would push its own
    operative clause off the end of a row it shares with nothing."""
    state = replay(
        [raw_event("message.start"), raw_event("thinking.delta", {"text": "x" * 500})]
    )
    assert state.thinking_notice == "x" * DETAIL_LINE_CLIP + "…"


# ── it opens nothing and outlives nothing ────────────────────────────────


def test_a_spinner_outside_a_turn_does_not_synthesize_one() -> None:
    """``_ensure_streaming`` exists so content cannot arrive into no turn. A
    spinner frame carries no content, so spending a synthetic turn start and a
    ``stream began without a message.start event`` line on one is pure noise —
    and the gateway sends these while idle."""
    state = replay([raw_event("thinking.delta", {"text": "(◐) indexing..."})])
    assert state.turn == "idle"
    assert state.synthetic_turn_starts == 0
    assert state.transcript == ()
    assert state.thinking_notice == "(◐) indexing..."


def test_the_note_is_cleared_when_a_turn_starts_and_when_one_ends() -> None:
    """It describes right now, so it must not survive the moment it described."""
    opened = replay(
        [raw_event("message.start"), raw_event("thinking.delta", {"text": "(◐) indexing..."})]
    )
    assert opened.thinking_notice

    assert replay([raw_event("message.start")], opened).thinking_notice == ""
    assert replay([raw_event("message.complete", {"text": "hi"})], opened).thinking_notice == ""
    assert replay([raw_event("error", {"message": "boom"})], opened).thinking_notice == ""
    assert cancel_turn(opened, at=1.0).thinking_notice == ""
    assert focus_session(opened, "another-session").thinking_notice == ""


def test_a_spinner_arriving_after_a_cancel_is_counted_and_ignored() -> None:
    """Same rule the other delta handlers follow: a cancelled turn is terminal,
    and a late frame is recorded rather than acted on."""
    cancelled = cancel_turn(replay([raw_event("message.start")]), at=1.0)
    late = replay([raw_event("thinking.delta", {"text": "(◐) indexing..."})], cancelled)
    assert late.thinking_notice == ""
    assert late.late_events_ignored == cancelled.late_events_ignored + 1


# ── it reaches the projection ────────────────────────────────────────────


def test_the_note_is_projected_for_the_activity_line() -> None:
    state = replay(
        [raw_event("message.start"), raw_event("thinking.delta", {"text": "(◐) indexing..."})]
    )
    assert prompt_view(state).notice == "(◐) indexing..."
    assert prompt_view(SessionState()).notice == ""
