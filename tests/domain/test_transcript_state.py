"""R3: a prompt is submitted and the response streams into the transcript.

The interesting assertions here are not "text arrives" — they are the three ways
a streaming transcript loses or duplicates content, each of which Hermes hit and
fixed, and each of which is a line of code that is easy to omit:

* reading ``payload.rendered`` during streaming discards everything streamed so
  far, because ``rendered`` is an incremental ANSI fragment rather than the
  running text;
* not stripping already-shown text from ``message.complete`` shows the opening
  of every reply twice;
* stripping too eagerly deletes an interim-sealed message the operator already
  read.
"""

from __future__ import annotations

from typing import Any

from talaria.domain.projection import transcript_view

from .conftest import raw_event, replay


def _assistant_text(state: Any) -> list[str]:
    return [e.text for e in state.transcript if e.kind == "assistant"]


def test_a_turn_streams_into_the_transcript(streaming_turn: list[dict[str, Any]]) -> None:
    state = replay(streaming_turn)
    assert state.turn == "idle"
    assert _assistant_text(state) == ["Hello, world."]


def test_deltas_accumulate_and_are_visible_before_the_turn_completes() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "par"}),
            raw_event("message.delta", {"text": "tial"}),
        ]
    )
    assert state.turn == "streaming"
    assert state.streaming_text == "partial"
    assert transcript_view(state).lines == ("partial",)


def test_rendered_is_never_read_during_streaming() -> None:
    """``recordMessageDelta`` (``turnController.ts:669-687``) accumulates
    ``text`` and ignores ``rendered``; the comment there records the defect the
    other way round — assigning ``rendered`` on every tick discarded the stream.
    """
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "one "}),
            raw_event("message.delta", {"rendered": "\x1b[31mANSI\x1b[0m", "text": "two"}),
        ]
    )
    assert state.streaming_text == "one two"
    assert "\x1b" not in transcript_view(state).text


def test_final_message_does_not_repeat_a_sealed_interim_message() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "First half. "}),
            raw_event("message.interim", {"text": "First half."}),
            raw_event("message.delta", {"text": "Second half."}),
            raw_event("message.complete", {"text": "First half. Second half."}),
        ]
    )
    assert _assistant_text(state) == ["First half.", "First half. Second half."], (
        "the interim seal is preserved; the final message is not deduped against it "
        "unless response_previewed says it is the same response"
    )


def test_response_previewed_dedupes_against_the_sealed_interim() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.interim", {"text": "The whole answer."}),
            raw_event(
                "message.complete",
                {"text": "The whole answer.", "response_previewed": True},
            ),
        ]
    )
    assert _assistant_text(state) == ["The whole answer."]


def test_final_text_prefers_raw_text_over_rendered_ansi() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event(
                "message.complete",
                {"text": "plain answer", "rendered": "\x1b[1mplain answer\x1b[0m"},
            ),
        ]
    )
    assert _assistant_text(state) == ["plain answer"]


def test_rendered_is_used_only_when_the_gateway_sent_no_text() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.complete", {"rendered": "fallback answer"}),
        ]
    )
    assert _assistant_text(state) == ["fallback answer"]


def test_buffered_text_is_committed_when_the_final_message_is_empty() -> None:
    """R6 says transcript content is never dropped. Hermes would render the
    empty final message and lose the buffer; Talaria commits the buffer."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "streamed but never finalized"}),
            raw_event("message.complete", {"text": ""}),
        ]
    )
    assert _assistant_text(state) == ["streamed but never finalized"]


def test_reasoning_is_committed_at_turn_end_and_never_truncated() -> None:
    """Hermes truncates its reasoning buffer at 80,000 characters down to the
    last 60,000 (``turnController.ts:778-780``). R6 puts reasoning presentation
    out of scope while requiring that its content is never dropped, so Talaria
    keeps all of it."""
    long_chunk = "z" * 50_000
    state = replay(
        [
            raw_event("message.start"),
            raw_event("reasoning.delta", {"text": long_chunk}),
            raw_event("reasoning.delta", {"text": long_chunk}),
            raw_event("message.complete", {"text": "done"}),
        ]
    )
    reasoning = [e.text for e in state.transcript if e.kind == "reasoning"]
    assert len(reasoning) == 1
    assert len(reasoning[0]) == 100_000


def test_a_whole_reasoning_block_does_not_duplicate_streamed_reasoning() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("reasoning.delta", {"text": "thinking hard"}),
            raw_event("reasoning.available", {"text": "thinking hard"}),
            raw_event("message.complete", {"text": "answer"}),
        ]
    )
    reasoning = [e.text for e in state.transcript if e.kind == "reasoning"]
    assert reasoning == ["thinking hard"]


def test_usage_is_merged_field_wise_and_never_zeroed_by_a_partial_payload() -> None:
    state = replay(
        [
            raw_event("session.info", {"title": "A chat", "usage": {"input_tokens": 100}}),
            raw_event("message.start"),
            raw_event(
                "message.complete", {"text": "hi", "usage": {"output_tokens": 42}}
            ),
        ]
    )
    assert state.session_title == "A chat"
    assert state.usage.input_tokens == 100
    assert state.usage.output_tokens == 42


def test_a_repeated_status_note_is_recorded_once() -> None:
    state = replay(
        [
            raw_event("status.update", {"text": "compressing context"}),
            raw_event("status.update", {"text": "compressing context"}),
            raw_event("status.update", {"text": "ready"}),
        ]
    )
    assert [e.text for e in state.transcript if e.kind == "system"] == [
        "compressing context",
        "ready",
    ]


def test_a_tool_run_lands_in_the_transcript_as_plain_text() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("tool.start", {"tool_id": "t1", "name": "read_file", "context": "README"}),
            raw_event(
                "tool.complete",
                {"tool_id": "t1", "name": "read_file", "summary": "42 lines"},
            ),
        ]
    )
    tools = [e.text for e in state.transcript if e.kind == "tool"]
    assert tools == ["⏺ read_file README", "⏺ read_file ✓ 42 lines"]


def test_an_interim_message_seals_the_stream_as_a_segment() -> None:
    """``recordInterimMessage`` (``turnController.ts:689-713``) treats the
    interim text as authoritative even when the stream did not carry every
    token, then seals it so the final message's dedupe pass leaves it alone."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "par"}),
            raw_event("message.interim", {"text": "partial answer"}),
        ]
    )
    assert state.segments == ("partial answer",)
    assert state.interim_boundary == 1
    assert state.streaming_text == ""
    assert _assistant_text(state) == ["partial answer"]
