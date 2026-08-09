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

from dataclasses import replace
from typing import Any

from talaria.domain.models import PendingPrompt
from talaria.domain.projection import transcript_view
from talaria.domain.state import (
    WITHHELD_HISTORY_PREFIX,
    SessionState,
    land_session,
    record_local_note,
    seed_history,
)

from .conftest import raw_event, replay


def _answering_prompt() -> PendingPrompt:
    """One prompt parked in ``answering`` — the state that refuses a switch."""
    return PendingPrompt(
        request_id="req-1", kind="sudo", summary="a password", opened_at=0.0, seq=1
    )


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


def test_session_key_is_refreshed_from_session_info() -> None:
    """P1, U7 round two. ``stored_session_id`` on ``session.info`` is the
    durable key, and the installed gateway sends a fresh one whenever it
    changes under a live session — most concretely, a compression rotation
    (``tui_gateway/server.py:5200``). Without this read the picker kept
    comparing rows against the id ``session.create``/``session.resume``
    handed back at landing, forever, so the freshly-current row never
    matched and stayed selectable.
    """
    state = replay([raw_event("session.info", {"stored_session_id": "sess-key-1"})])
    assert state.session_key == "sess-key-1"

    rotated = replay(
        [raw_event("session.info", {"stored_session_id": "sess-key-2"})], state
    )
    assert rotated.session_key == "sess-key-2"

    # A later update carrying no stored id at all keeps the one already
    # known, the same fallback ``session_title`` already gets.
    unchanged = replay([raw_event("session.info", {"title": "renamed"})], rotated)
    assert unchanged.session_key == "sess-key-2"


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


# ── U6: the resumed history is projected into the transcript (KTD2) ──────


def test_seeded_history_becomes_committed_entries_in_wire_order() -> None:
    """Three messages, three entries, in the order the gateway sent them."""
    state = seed_history(
        SessionState(),
        [
            {"role": "user", "text": "q", "row_id": 1},
            {"role": "tool", "name": "grep", "context": "pattern"},
            {"role": "assistant", "text": "a", "row_id": 2},
        ],
        omitted=False,
        count=3,
    )
    assert [(e.kind, e.text) for e in state.transcript] == [
        ("user", "q"),
        ("tool", "⏺ grep pattern"),
        ("assistant", "a"),
    ]
    assert [e.seq for e in state.transcript] == [0, 1, 2]


def test_the_projection_serves_seeded_history_from_the_transcript_alone() -> None:
    """The whole reason KTD2 appends entries instead of synthesizing events:
    ``transcript_view`` reads ``state.transcript`` for committed content and
    nothing else, so an appended entry is on screen by construction."""
    state = seed_history(
        SessionState(),
        [{"role": "user", "text": "q"}, {"role": "assistant", "text": "a"}],
        omitted=False,
        count=2,
    )
    view = transcript_view(state)
    assert view.lines == ("› q", "a")
    assert view.entry_count == 2
    assert view.committed_lines == 2


def test_a_live_event_after_the_seed_appends_after_the_seeded_entries() -> None:
    """Append-only across the seam. The seeded conversation is history; the
    turn that follows it is the next thing that happened, not an insertion."""
    seeded = seed_history(
        SessionState(), [{"role": "user", "text": "q"}], omitted=False, count=1
    )
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "live"}),
            raw_event("message.complete", {"text": "live"}),
        ],
        seeded,
    )
    assert [(e.kind, e.text) for e in state.transcript] == [
        ("user", "q"),
        ("assistant", "live"),
    ]


def test_a_live_message_repeating_a_seeded_one_is_not_deduplicated() -> None:
    """The gateway owns history truth. A client that swallowed the repeat would
    be betting its own identity guess against what the gateway just said, and
    losing that bet deletes a real repeated message with no trace."""
    seeded = seed_history(
        SessionState(), [{"role": "assistant", "text": "same"}], omitted=False, count=1
    )
    state = replay(
        [raw_event("message.start"), raw_event("message.complete", {"text": "same"})],
        seeded,
    )
    assert [e.text for e in state.transcript] == ["same", "same"]


def test_an_empty_history_seeds_nothing_and_notices_nothing() -> None:
    """A session with no messages is not the same thing as a withheld history,
    and neither one gets a line here."""
    state = seed_history(SessionState(), [], omitted=False, count=0)
    assert state.transcript == ()


def test_the_gateways_omission_shape_names_the_whole_withheld_history() -> None:
    """An **empty** ``messages`` array with the full ``message_count`` is what
    ``omit_messages`` actually sends (``methods_session.py:494-500``) — not a
    truncated array — so the count to name is the count itself."""
    state = seed_history(SessionState(), [], omitted=True, count=7)
    assert len(state.transcript) == 1
    entry = state.transcript[0]
    assert entry.kind == "system"
    assert entry.text.startswith(WITHHELD_HISTORY_PREFIX)
    assert "7 earlier messages" in entry.text


def test_a_history_that_was_not_withheld_gets_no_withheld_line() -> None:
    state = seed_history(
        SessionState(), [{"role": "user", "text": "q"}], omitted=False, count=1
    )
    assert [e.kind for e in state.transcript] == ["user"]


def test_a_partial_delivery_names_the_difference_rather_than_the_total() -> None:
    """Hypothetical today — the gateway either sends everything or nothing —
    and pinned so a future partial mode cannot silently report the whole
    history as missing."""
    state = seed_history(
        SessionState(),
        [{"role": "user", "text": "q"}, {"role": "assistant", "text": "a"}],
        omitted=True,
        count=7,
    )
    assert "5 earlier messages" in state.transcript[0].text


def test_the_withheld_notice_precedes_the_delivered_lines_it_describes() -> None:
    """CR6 finding 4: the notice used to be appended *after* the delivered
    lines, even though it describes messages that precede them in time — the
    gateway withheld the *earlier* part of the conversation, not the later
    part, so a notice sitting below the delivered lines read as following
    them rather than introducing them."""
    state = seed_history(
        SessionState(),
        [{"role": "user", "text": "newer A"}, {"role": "assistant", "text": "newer B"}],
        omitted=True,
        count=7,
    )
    assert [(e.kind, e.text) for e in state.transcript] == [
        ("system", "earlier history withheld: the gateway did not send 5 earlier messages"),
        ("user", "newer A"),
        ("assistant", "newer B"),
    ]


def test_one_withheld_message_is_named_in_the_singular() -> None:
    state = seed_history(SessionState(), [], omitted=True, count=1)
    assert "1 earlier message" in state.transcript[0].text
    assert "messages" not in state.transcript[0].text


def test_a_withheld_flag_whose_arithmetic_disagrees_still_says_so() -> None:
    """``messages_omitted`` is the gateway's own statement. Contradicting it
    with silence because the subtraction came out non-positive would drop the
    one fact the line exists to carry."""
    state = seed_history(SessionState(), [], omitted=True, count=0)
    assert state.transcript[0].text.startswith(WITHHELD_HISTORY_PREFIX)


def test_a_malformed_history_element_is_seeded_as_an_unknown_event() -> None:
    """Nothing dropped, all the way through to the committed entry."""
    state = seed_history(
        SessionState(),
        ["a bare string", {"role": "oracle", "text": "the prophecy"}],
        omitted=False,
        count=2,
    )
    assert [e.kind for e in state.transcript] == ["unknown-event", "unknown-event"]
    assert "the prophecy" in state.transcript[1].text


# ── U6: landing keeps both identities and decides about the transcript ───


def test_landing_keeps_the_runtime_id_for_correlation_and_the_stored_id_for_identity() -> None:
    """R6/R7. ``session_id`` is what events are stamped with; ``session_key``
    is what survives the process and names the session in the picker."""
    state = land_session(SessionState(), "s-live-042", session_key="k-durable")
    assert state.focused_session_id == "s-live-042"
    assert state.session_key == "k-durable"


def test_landing_a_different_session_begins_a_fresh_transcript() -> None:
    """KTD3: seeding session B onto session A's retained transcript is the
    merged two-session view the non-goals forbid."""
    in_a = seed_history(
        land_session(SessionState(), "sess-a", session_key="k-a"),
        [{"role": "user", "text": "asked in A"}],
        omitted=False,
        count=1,
    )
    in_b = seed_history(
        land_session(in_a, "sess-b", session_key="k-b"),
        [{"role": "user", "text": "asked in B"}],
        omitted=False,
        count=1,
    )
    assert [e.text for e in in_b.transcript] == ["asked in B"]
    assert in_b.session_key == "k-b"


def test_landing_the_same_session_again_retains_its_transcript() -> None:
    """The reconnect reading. Blanking the pane on every dropped socket is the
    failure this half prevents."""
    in_a = seed_history(
        land_session(SessionState(), "sess-a", session_key="k-a"),
        [{"role": "user", "text": "asked in A"}],
        omitted=False,
        count=1,
    )
    again = land_session(in_a, "sess-a", session_key="k-a")
    assert [e.text for e in again.transcript] == ["asked in A"]


def test_a_fresh_transcript_does_not_reissue_entry_sequence_numbers() -> None:
    """``entry_seq`` is an identity the renderer keys widgets off, not a
    position: two entries sharing ``seq=0`` inside one process is a collision."""
    in_a = seed_history(
        land_session(SessionState(), "sess-a"),
        [{"role": "user", "text": "asked in A"}],
        omitted=False,
        count=1,
    )
    in_b = seed_history(
        land_session(in_a, "sess-b"),
        [{"role": "user", "text": "asked in B"}],
        omitted=False,
        count=1,
    )
    assert in_b.transcript[0].seq == 1


def test_a_landing_refused_while_an_answer_is_travelling_keeps_the_transcript() -> None:
    """Clearing history for a switch that then does not happen destroys the
    operator's conversation to no purpose."""
    in_a = seed_history(
        land_session(SessionState(), "sess-a"),
        [{"role": "user", "text": "asked in A"}],
        omitted=False,
        count=1,
    )
    answering = replace(in_a, answering=(_answering_prompt(),))
    refused = land_session(answering, "sess-b", session_key="k-b")
    assert refused is answering
    assert [e.text for e in refused.transcript] == ["asked in A"]


def test_the_first_landing_of_a_process_keeps_the_notes_written_before_it() -> None:
    """There is no previous session to have belonged to, so nothing is foreign.

    What the transcript holds before the first landing is Talaria's own account
    of this launch — a blocking compatibility verdict, "there is no previous
    session to resume". Clearing it made the one line explaining a degraded
    startup vanish at the instant the session opened.
    """
    noted = record_local_note(SessionState(), "compatibility check blocked", at=0.0)
    landed = seed_history(
        land_session(noted, "sess-a", session_key="k-a"),
        [{"role": "user", "text": "asked in A"}],
        omitted=False,
        count=1,
    )
    assert [e.text for e in landed.transcript] == [
        "compatibility check blocked",
        "asked in A",
    ]
