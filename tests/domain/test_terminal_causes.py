"""U2 (KTD6, KTD7, R6, R14, R16, R18): terminal causes commit partial buffers,
content is preserved exactly end to end, and the entry-scoped projection
carries a stable id, an unwelded raw body, and two provisional tails.

Three defects motivate this file, and each gets its own section below:

* ``_on_error`` used to clear ``streaming_text``/``reasoning_text`` without
  committing them — an error mid-reply silently dropped whatever had already
  streamed.
* ``coerce_text`` strips both ends, and it used to run on the content channel
  (``message.interim``, ``reasoning.available``) as well as on diagnostics —
  so a four-space indented reply lost its indentation the moment it sealed,
  and a markdown construct that depends on trailing blank lines lost them.
* The transport reported four distinct disconnect causes through one
  untyped callback shape, so the domain could not tell "the stream ended for
  good" from "a reconnect is about to resume the same response" — the first
  must commit partial content (R6), the second must commit nothing (else a
  resumed response duplicates what already streamed).
"""

from __future__ import annotations

from talaria.domain.projection import entry_scoped_view, transcript_view
from talaria.domain.state import SessionState, cancel_turn, set_connection

from .conftest import BASE_TIME, raw_event, replay

# ── R6/KTD7: terminal causes commit partial buffers ───────────────────────


def test_an_error_mid_stream_commits_the_partial_reply_before_clearing() -> None:
    """The defect this unit fixes: an error used to erase whatever had
    streamed so far without a trace."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("reasoning.delta", {"text": "thinking about it"}),
            raw_event("message.delta", {"text": "half a rep"}),
            raw_event("error", {"message": "provider hung up"}),
        ]
    )
    assert [e.kind for e in state.transcript] == ["reasoning", "assistant", "error"], (
        "partial content commits before the error line, in narrative order"
    )
    assert [e.text for e in state.transcript if e.kind == "reasoning"] == [
        "thinking about it"
    ]
    assert [e.text for e in state.transcript if e.kind == "assistant"] == ["half a rep"]
    assert state.streaming_text == ""
    assert state.reasoning_text == ""
    assert state.turn == "idle"


def test_an_error_with_nothing_streamed_commits_nothing_extra() -> None:
    """No partial buffers means no phantom entries — only the error line."""
    state = replay([raw_event("message.start"), raw_event("error", {"message": "boom"})])
    assert [e.kind for e in state.transcript] == ["error"]


def test_each_typed_terminal_cause_commits_the_partial_reply() -> None:
    """KTD7's four typed causes each make the domain commit before clearing —
    exercised directly against ``set_connection``, the transition the
    transport's typed cause seam feeds (``note_connection_state``)."""
    for cause in ("auth_failed", "dial_failed", "orderly_close", "reconnect_exhausted"):
        state = replay(
            [
                raw_event("message.start"),
                raw_event("reasoning.delta", {"text": "reasoning before the drop"}),
                raw_event("message.delta", {"text": "partial reply"}),
            ]
        )
        after = set_connection(state, "disconnected", cause=cause, at=BASE_TIME + 5)
        assert [e.kind for e in after.transcript] == ["reasoning", "assistant"], cause
        assert after.transcript[0].text == "reasoning before the drop", cause
        assert after.transcript[1].text == "partial reply", cause
        assert after.streaming_text == "", cause
        assert after.reasoning_text == "", cause
        assert after.turn == "idle", cause
        assert after.connection == "disconnected", cause


def test_a_typed_cause_does_not_resurrect_a_cancelled_turn() -> None:
    """A connection that drops after the operator already cancelled must not
    overwrite that outcome, exactly like ``_on_error``'s own cancelled guard."""
    state = replay(
        [raw_event("message.start"), raw_event("message.delta", {"text": "x"})]
    )
    cancelled = cancel_turn(state, at=BASE_TIME + 1)
    assert cancelled.turn == "cancelled"

    after = set_connection(cancelled, "disconnected", cause="orderly_close", at=BASE_TIME + 2)
    assert after.turn == "cancelled"


def test_a_transient_status_change_commits_nothing() -> None:
    """``cause=None`` — ``connecting``, ``connected``, ``reconnecting`` — must
    not touch the buffers at all: they are still in flight."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "still typing"}),
        ]
    )
    for status in ("connecting", "connected", "reconnecting"):
        after = set_connection(state, status)
        assert after.streaming_text == "still typing", status
        assert [e.kind for e in after.transcript] == [], status


def test_a_transient_reconnect_that_resumes_never_duplicates_content() -> None:
    """The scenario KTD7 names explicitly: a reconnect mid-response, followed
    by the rest of the same response, must produce the content exactly once.
    The segment/interim machinery is the dedupe backstop; a cause-less status
    change must not itself commit or clear anything for that backstop to
    protect."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "first half. "}),
        ]
    )
    # A reconnect cycle: reconnecting, then connected again. Cause-less both
    # times — the stream never actually ended.
    state = set_connection(state, "reconnecting")
    state = set_connection(state, "connected")
    state = replay(
        [
            raw_event("message.delta", {"text": "second half."}),
            raw_event("message.complete", {"text": "first half. second half."}),
        ],
        state,
    )
    assistant_text = [e.text for e in state.transcript if e.kind == "assistant"]
    assert assistant_text == ["first half. second half."], (
        "a transient reconnect duplicated or lost content across the resume"
    )


def test_set_connection_still_no_ops_a_repeated_identical_status_with_no_cause() -> None:
    """The pre-existing latch-rearm dedup survives: an unchanged, cause-less
    status change is still exactly a no-op (KTD7 only widens the guard for a
    cause, never removes it)."""
    state = SessionState(connection="connected")
    assert set_connection(state, "connected") is state


# ── cancel_turn: commit pinned, stripping removed ─────────────────────────


def test_cancelling_preserves_a_whitespace_only_streaming_buffer() -> None:
    """Content preservation, not just non-empty content: a buffer that is
    *only* whitespace when cancellation lands must still leave its trace —
    the old ``.strip()``-truthiness guard would have silently dropped it as
    "nothing streamed"."""
    state = replay([raw_event("message.start"), raw_event("message.delta", {"text": "   "})])
    after = cancel_turn(state, at=BASE_TIME + 1)
    assert [e.kind for e in after.transcript] == ["assistant"]
    assert after.transcript[0].text == "   \n\n*[interrupted]*"


def test_cancelling_preserves_leading_indentation_exactly() -> None:
    """The fixture from the plan: four leading spaces are an indented code
    block in GitHub-flavoured markdown; two are not. Losing even one space
    changes what the committed entry means."""
    state = replay(
        [raw_event("message.start"), raw_event("message.delta", {"text": "    indented code"})]
    )
    after = cancel_turn(state, at=BASE_TIME + 1)
    assert after.transcript[0].text == "    indented code\n\n*[interrupted]*"


# ── Content-channel exact preservation (KTD7) ──────────────────────────────


INDENTED = "    def f():\n        return 1"
TRAILING_BLANK = "paragraph text\n\n\n"
WHITESPACE_ONLY = "   \n  "
OPEN_FENCE = "```python\ndef f():\n    return 1"


def test_interim_preserves_leading_indentation_exactly() -> None:
    state = replay(
        [raw_event("message.start"), raw_event("message.interim", {"text": INDENTED})]
    )
    assert [e.text for e in state.transcript if e.kind == "assistant"] == [INDENTED]


def test_interim_preserves_trailing_blank_lines_exactly() -> None:
    state = replay(
        [raw_event("message.start"), raw_event("message.interim", {"text": TRAILING_BLANK})]
    )
    assert [e.text for e in state.transcript if e.kind == "assistant"] == [TRAILING_BLANK]


def test_interim_preserves_a_whitespace_only_body() -> None:
    """``coerce_text`` would have collapsed this to ``""`` and dropped it
    entirely; ``coerce_text_exact`` does not."""
    state = replay(
        [raw_event("message.start"), raw_event("message.interim", {"text": WHITESPACE_ONLY})]
    )
    assert [e.text for e in state.transcript if e.kind == "assistant"] == [WHITESPACE_ONLY]


def test_interim_preserves_an_open_fence_exactly() -> None:
    state = replay(
        [raw_event("message.start"), raw_event("message.interim", {"text": OPEN_FENCE})]
    )
    assert [e.text for e in state.transcript if e.kind == "assistant"] == [OPEN_FENCE]


def test_final_replacement_preserves_indentation_and_does_not_duplicate() -> None:
    """The final-tail dedupe has to match the *exact*, unstripped interim
    segment against the head of the final text — a stripped comparison would
    fail to recognize the already-shown indented segment and duplicate the
    whole reply (see ``_final_tail``'s docstring)."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.interim", {"text": INDENTED}),
            raw_event(
                "message.complete",
                {"text": INDENTED, "response_previewed": True},
            ),
        ]
    )
    assert [e.text for e in state.transcript if e.kind == "assistant"] == [INDENTED], (
        "the final message duplicated the already-sealed indented segment"
    )


def test_final_replacement_preserves_trailing_blank_lines() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": TRAILING_BLANK}),
            raw_event("message.complete", {"text": ""}),
        ]
    )
    assert [e.text for e in state.transcript if e.kind == "assistant"] == [TRAILING_BLANK]


def test_reasoning_available_preserves_indentation_exactly() -> None:
    state = replay(
        [raw_event("message.start"), raw_event("reasoning.available", {"text": INDENTED})]
    )
    assert state.reasoning_text == INDENTED


def test_error_path_preserves_indentation_and_an_open_fence() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("reasoning.delta", {"text": INDENTED}),
            raw_event("message.delta", {"text": OPEN_FENCE}),
            raw_event("error", {"message": "boom"}),
        ]
    )
    assert [e.text for e in state.transcript if e.kind == "reasoning"] == [INDENTED]
    assert [e.text for e in state.transcript if e.kind == "assistant"] == [OPEN_FENCE]


def test_cancellation_preserves_indentation_and_trailing_blank_lines() -> None:
    state = replay(
        [raw_event("message.start"), raw_event("message.delta", {"text": TRAILING_BLANK})]
    )
    after = cancel_turn(state, at=BASE_TIME + 1)
    assert after.transcript[0].text == f"{TRAILING_BLANK}\n\n*[interrupted]*"


def test_disconnect_preserves_indentation_and_an_open_fence() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("reasoning.delta", {"text": INDENTED}),
            raw_event("message.delta", {"text": OPEN_FENCE}),
        ]
    )
    after = set_connection(state, "disconnected", cause="reconnect_exhausted")
    assert [e.text for e in after.transcript if e.kind == "reasoning"] == [INDENTED]
    assert [e.text for e in after.transcript if e.kind == "assistant"] == [OPEN_FENCE]


# ── R14: a cancellation mid-table commits the partial table ──────────────


def test_a_cancellation_mid_table_commits_the_partial_table_after_the_header_row() -> None:
    """A parser reinterprets an unterminated GitHub-flavoured-markdown table
    as a plain paragraph — exactly why this case exists rather than being
    folded into the generic partial-content tests above."""
    table = "| a | b |\n| - | - |\n| 1 | 2 |"
    state = replay([raw_event("message.start"), raw_event("message.delta", {"text": table})])
    after = cancel_turn(state, at=BASE_TIME + 1)
    assert after.transcript[0].text == f"{table}\n\n*[interrupted]*"


# ── KTD6: the entry-scoped projection ──────────────────────────────────────


def test_entry_scoped_view_carries_a_stable_id_kind_body_and_line_span() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "line one\nline two"}),
            raw_event("message.complete", {"text": ""}),
        ]
    )
    view = entry_scoped_view(state)
    assert len(view.entries) == 1
    record = view.entries[0]
    assert record.entry_id == state.transcript[0].seq
    assert record.kind == "assistant"
    assert record.raw_body == "line one\nline two"
    assert record.committed is True
    assert record.line_span == (0, 2)


def test_entry_scoped_view_line_spans_agree_with_the_flattened_line_buffer() -> None:
    """The two projections are independent functions of the same
    ``state.transcript`` and must agree on line ownership without either
    reading the other's output."""
    state = replay(
        [
            raw_event("status.update", {"text": "one line"}),
            raw_event("message.start"),
            raw_event(
                "message.delta", {"text": "multi\nline\nreply"}
            ),
            raw_event("message.complete", {"text": ""}),
        ]
    )
    view = entry_scoped_view(state)
    lines = transcript_view(state).lines
    for record in view.entries:
        start, count = record.line_span
        assert count == len(record.raw_body.split("\n"))
        # The flattened buffer welds a presentation prefix onto some kinds
        # (``· `` for reasoning, ``— `` for system); the raw body never
        # carries that weld, so the comparison is by line *count*, not by
        # exact text, except for kinds with no weld at all.
        if record.kind in ("assistant",):
            assert lines[start : start + count] == tuple(record.raw_body.split("\n"))


def test_entry_scoped_view_has_no_speaker_weld_on_the_raw_body() -> None:
    """R18: a first-line heading in a reasoning stream must be valid
    markdown. The ``· `` weld :func:`transcript_view` applies is presentation
    for line-rendered surfaces only — it must not appear on the raw body a
    block-rendering consumer parses."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("reasoning.delta", {"text": "# a heading"}),
            raw_event("message.complete", {"text": "done"}),
        ]
    )
    view = entry_scoped_view(state)
    reasoning = next(r for r in view.entries if r.kind == "reasoning")
    assert reasoning.raw_body == "# a heading"
    assert "· " not in reasoning.raw_body

    # The decorated line buffer *does* still carry the weld — unchanged.
    assert any(line.startswith("· ") for line in transcript_view(state).lines)


def test_entry_scoped_view_carries_both_provisional_tails_independently() -> None:
    """R18: the domain holds both buffers simultaneously and neither tail may
    steal the other's progressive rendering."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("reasoning.delta", {"text": "thinking"}),
            raw_event("message.delta", {"text": "typing"}),
        ]
    )
    view = entry_scoped_view(state)
    assert view.assistant_tail.kind == "assistant"
    assert view.assistant_tail.raw_text == "typing"
    assert view.reasoning_tail.kind == "reasoning"
    assert view.reasoning_tail.raw_text == "thinking"
    assert not view.assistant_tail.is_empty
    assert not view.reasoning_tail.is_empty


def test_a_committed_entry_never_reuses_a_stream_generation_as_its_id() -> None:
    """The entry id is the transcript entry's own ``seq`` — a distinct
    counter from either stream's generation — so nothing conflates "which
    entry is this" with "which version of the live tail is this"."""
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.interim", {"text": "first"}),
            raw_event("message.delta", {"text": "second"}),
        ]
    )
    view = entry_scoped_view(state)
    assert view.entries[0].entry_id == 0
    assert view.assistant_tail.generation >= 1


# ── KTD3: message.interim respects the boundary and bumps the generation ──


def test_interim_bumps_the_assistant_generation_and_clears_the_tail() -> None:
    state = replay(
        [raw_event("message.start"), raw_event("message.delta", {"text": "partial"})]
    )
    before = state.assistant_stream_generation
    after = replay([raw_event("message.interim", {"text": "partial"})], state)
    assert after.assistant_stream_generation == before + 1
    assert after.streaming_text == ""


def test_a_plain_delta_does_not_bump_the_generation() -> None:
    """Growth is not replacement (KTD3): a consumer must be able to append
    the delta's suffix rather than re-render, so a plain accumulation must
    not bump the generation the way a seal or a clear does."""
    state = replay([raw_event("message.start"), raw_event("message.delta", {"text": "a"})])
    generation = state.assistant_stream_generation
    after = replay([raw_event("message.delta", {"text": "b"})], state)
    assert after.assistant_stream_generation == generation
    assert after.streaming_text == "ab"


def test_a_cancelled_turn_ignores_late_interim_without_bumping_anything() -> None:
    """The committed/interim boundary: an interim arriving after cancellation
    is a late event, not a second seal — the existing ``late_events_ignored``
    guard, unaffected by the generation counter's addition."""
    state = replay(
        [raw_event("message.start"), raw_event("message.delta", {"text": "x"})]
    )
    cancelled = cancel_turn(state, at=BASE_TIME + 1)
    generation = cancelled.assistant_stream_generation
    after = replay([raw_event("message.interim", {"text": "late"})], cancelled)
    assert after.assistant_stream_generation == generation
    assert after.late_events_ignored == 1
