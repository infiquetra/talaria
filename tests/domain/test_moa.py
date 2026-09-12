"""Tests for C10 (#148): Mixture of Agents event decoding, normalization, and models."""

from __future__ import annotations

import json
from pathlib import Path

from talaria.domain.changes import inspector_view
from talaria.domain.decode import (
    KNOWN_EVENT_TYPES,
    MoaAggregatingPayload,
    MoaPhasePayload,
    MoaProgressPayload,
    MoaReferencePayload,
    decode_frame,
    decode_moa_aggregating,
    decode_moa_phase,
    decode_moa_progress,
    decode_moa_reference,
)
from talaria.domain.models import (
    KNOWN_MOA_PHASES,
    TERMINAL_MOA_PHASES,
    GatewayEvent,
    MoaPhase,
    MoaReferenceRecord,
    MoaRun,
    MoaView,
)
from talaria.domain.normalize import (
    AMBIENT_IGNORED_EVENTS,
    MOA_FALLBACK_TEXT,
    TRANSCRIPT_LINE_CLIP,
    format_moa_committed_line,
    format_moa_inspector_rows,
    format_moa_live_line,
    is_terminal_moa_phase,
    keep_terminal_moa_phase,
    normalize_moa_phase,
)
from talaria.domain.projection import entry_scoped_view, moa_view, project
from talaria.domain.state import (
    cancel_turn,
    set_connection,
)

from .conftest import BASE_TIME, raw_event, replay

_FIXTURES_DIR = Path(__file__).resolve().parents[1] / "fixtures"
_MOA_TURN_FIXTURE_PATH = _FIXTURES_DIR / "events" / "moa_turn.jsonl"


def _decode(frame: object) -> object:
    return decode_frame(frame, at=BASE_TIME, seq=1)


# ── Event registration & inventory ───────────────────────────────────────


def test_moa_events_in_known_event_types() -> None:
    expected = {"moa.aggregating", "moa.phase", "moa.progress", "moa.reference"}
    for ev_type in expected:
        assert ev_type in KNOWN_EVENT_TYPES, f"{ev_type} must be in KNOWN_EVENT_TYPES"


def test_moa_events_not_in_ambient_ignored() -> None:
    """MoA progress/phase/aggregating are no longer ambient chatter (C10, #148)."""
    assert "moa.progress" not in AMBIENT_IGNORED_EVENTS
    assert "moa.phase" not in AMBIENT_IGNORED_EVENTS
    assert "moa.aggregating" not in AMBIENT_IGNORED_EVENTS


# ── Payload decoders ─────────────────────────────────────────────────────


def test_decode_moa_progress_valid() -> None:
    payload = {"label": "gemini:gemini-3.6-flash", "refs_done": 1, "refs_total": 5}
    decoded = decode_moa_progress(payload)
    assert decoded == MoaProgressPayload(
        label="gemini:gemini-3.6-flash", refs_done=1, refs_total=5
    )


def test_decode_moa_progress_requires_both_counters() -> None:
    assert decode_moa_progress({"label": "m1", "refs_done": 1}) is None
    assert decode_moa_progress({"label": "m1", "refs_total": 5}) is None
    assert decode_moa_progress({"label": "m1"}) is None
    # Booleans are rejected
    assert decode_moa_progress({"label": "m1", "refs_done": True, "refs_total": 5}) is None
    assert decode_moa_progress({"label": "m1", "refs_done": 1, "refs_total": False}) is None
    # Non-dict rejected
    assert decode_moa_progress("invalid") is None  # type: ignore[arg-type]


def test_decode_moa_reference_valid() -> None:
    payload = {
        "label": "openai-codex:gpt-5.6-sol",
        "text": "Four.",
        "index": 1,
        "count": 5,
    }
    decoded = decode_moa_reference(payload)
    assert decoded == MoaReferencePayload(
        label="openai-codex:gpt-5.6-sol",
        text="Four.",
        index=1,
        count=5,
    )


def test_decode_moa_reference_defaults() -> None:
    payload = {"text": "Advice text"}
    decoded = decode_moa_reference(payload)
    assert decoded == MoaReferencePayload(
        label="reference",
        text="Advice text",
        index=None,
        count=None,
    )
    assert decode_moa_reference(123) is None  # type: ignore[arg-type]


def test_decode_moa_phase_valid() -> None:
    payload = {
        "phase": "aggregator",
        "refs_done": 5,
        "refs_total": 5,
        "aggregator": "openai-codex:gpt-5.6-sol",
    }
    decoded = decode_moa_phase(payload)
    assert decoded == MoaPhasePayload(
        phase="aggregator",
        refs_done=5,
        refs_total=5,
        aggregator="openai-codex:gpt-5.6-sol",
    )


def test_decode_moa_phase_requires_phase() -> None:
    assert decode_moa_phase({}) is None
    assert decode_moa_phase({"phase": ""}) is None
    assert decode_moa_phase({"aggregator": "m1"}) is None


def test_decode_moa_aggregating_valid() -> None:
    payload = {"aggregator": "openai-codex:gpt-5.6-sol"}
    decoded = decode_moa_aggregating(payload)
    assert decoded == MoaAggregatingPayload(aggregator="openai-codex:gpt-5.6-sol")


# ── Live turn fixture verification ───────────────────────────────────────


def test_moa_turn_fixture_decodes_and_matches_wire_inventory() -> None:
    """Validate the live-captured MoA route turn fixture.

    Provenance (ruling section 7, F-2 of the C10 review): derived from the
    tester's route-qualification capture on candidate commit
    ``9091bd1fe47d83b943a6b98d94f9f4d5f1278006``, observed at
    ``2026-09-06T02:58:06.069456+00:00``. What it holds is the observed
    turn's four Mixture-of-Agents event frames — five ``moa.progress``,
    five ``moa.reference``, one ``moa.phase``, one ``moa.aggregating`` —
    followed by the turn's ``message.complete``, in wire order, one
    JSON-RPC frame per line, no frame added, reordered, or invented. The
    wire ``seq`` numbers are the capture's own (6 through 23, not
    contiguous: the frames between them were the turn's content, which this
    fixture is not about). The capture's ``message.start`` is likewise not
    part of the file; the replay tests open the turn themselves. Substituted
    from the capture: every ``session_id`` is the fixture value
    ``moa-fixture`` (the real session identifier never enters the tree), and
    each reference ``text`` is truncated to its first line.
    """
    assert _MOA_TURN_FIXTURE_PATH.is_file(), f"Missing fixture: {_MOA_TURN_FIXTURE_PATH}"
    lines = [
        json.loads(line)
        for line in _MOA_TURN_FIXTURE_PATH.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    assert len(lines) == 13

    event_counts: dict[str, int] = {}
    for i, frame in enumerate(lines):
        decoded = decode_frame(frame, at=BASE_TIME + i, seq=i + 1)
        assert isinstance(decoded, GatewayEvent)
        event_counts[decoded.type] = event_counts.get(decoded.type, 0) + 1

        if decoded.type == "moa.progress":
            progress = decode_moa_progress(decoded.payload)
            assert progress is not None
            assert progress.refs_total == 5
        elif decoded.type == "moa.reference":
            ref = decode_moa_reference(decoded.payload)
            assert ref is not None
            assert ref.text
            assert ref.count == 5
        elif decoded.type == "moa.phase":
            phase = decode_moa_phase(decoded.payload)
            assert phase is not None
            assert phase.phase == "aggregator"
            assert phase.aggregator == "openai-codex:gpt-5.6-sol"
        elif decoded.type == "moa.aggregating":
            agg = decode_moa_aggregating(decoded.payload)
            assert agg is not None
            assert agg.aggregator == "openai-codex:gpt-5.6-sol"
        elif decoded.type == "message.complete":
            assert decoded.payload.get("status") == "complete"

    assert event_counts == {
        "moa.progress": 5,
        "moa.reference": 5,
        "moa.phase": 1,
        "moa.aggregating": 1,
        "message.complete": 1,
    }


# ── Normalization & terminal protection ──────────────────────────────────


def test_known_and_terminal_moa_phase_constants() -> None:
    assert "references" in KNOWN_MOA_PHASES
    assert "aggregating" in KNOWN_MOA_PHASES
    assert "complete" in KNOWN_MOA_PHASES
    assert "cancelled" in KNOWN_MOA_PHASES
    assert "failed" in KNOWN_MOA_PHASES
    assert "lost" in KNOWN_MOA_PHASES
    assert TERMINAL_MOA_PHASES == {"complete", "cancelled", "failed", "lost"}


def test_normalize_moa_phase_aliases() -> None:
    assert normalize_moa_phase("aggregator") == "aggregating"
    assert normalize_moa_phase("AGGREGATING") == "aggregating"
    assert normalize_moa_phase("reference") == "references"
    assert normalize_moa_phase("references") == "references"
    assert normalize_moa_phase("advisors") == "references"
    assert normalize_moa_phase("complete") == "complete"
    assert normalize_moa_phase("completed") == "complete"
    assert normalize_moa_phase("cancelled") == "cancelled"
    assert normalize_moa_phase("interrupted") == "cancelled"
    assert normalize_moa_phase("failed") == "failed"
    assert normalize_moa_phase("error") == "failed"
    assert normalize_moa_phase("lost") == "lost"
    assert normalize_moa_phase("unknown", fallback="references") == "references"
    assert normalize_moa_phase(None, fallback="references") == "references"


def test_terminal_moa_phases() -> None:
    assert is_terminal_moa_phase("complete")
    assert is_terminal_moa_phase("cancelled")
    assert is_terminal_moa_phase("failed")
    assert is_terminal_moa_phase("lost")
    assert not is_terminal_moa_phase("references")
    assert not is_terminal_moa_phase("aggregating")


def test_keep_terminal_moa_phase() -> None:
    # Terminal phase wins over proposed resumption
    assert keep_terminal_moa_phase("complete", "references") == "complete"
    assert keep_terminal_moa_phase("cancelled", "aggregating") == "cancelled"
    assert keep_terminal_moa_phase("failed", "references") == "failed"
    assert keep_terminal_moa_phase("lost", "references") == "lost"
    # Non-terminal accepts proposed
    assert keep_terminal_moa_phase("references", "aggregating") == "aggregating"


def test_moa_fallback_text_constant() -> None:
    assert MOA_FALLBACK_TEXT == "no progress events observed"


# ── Domain models & state ────────────────────────────────────────────────


def test_moa_run_and_reference_record() -> None:
    ref = MoaReferenceRecord(label="model-1", first_line="Answer line", chars=100)
    assert ref.label == "model-1"
    assert ref.first_line == "Answer line"
    assert ref.chars == 100

    run = MoaRun(
        phase="references",
        refs_done=2,
        refs_total=5,
        finished=("m1", "m2"),
        references=(ref,),
        aggregator="agg-model",
    )
    assert run.phase == "references"
    assert not run.is_terminal

    terminal_run = MoaRun(phase="complete")
    assert terminal_run.is_terminal

    phase: MoaPhase = "references"
    assert phase in KNOWN_MOA_PHASES


def test_moa_view_defaults() -> None:
    view = MoaView()
    assert view.live_text is None
    assert view.committed_text is None
    assert view.inspector_rows == ("no progress events observed",)
    assert not view.is_active


# ── Preservation of existing reference-output handling ───────────────────


def test_existing_moa_reference_transcript_preservation() -> None:
    """Existing moa.reference handling in SessionState must remain intact."""
    frames = [
        raw_event("message.start", session_id="sess-moa"),
        raw_event(
            "moa.reference",
            {
                "label": "advisor-1",
                "text": "Two plus two is four.",
                "index": 1,
                "count": 2,
            },
            session_id="sess-moa",
        ),
    ]
    state = replay(frames)
    reasoning_entries = [e for e in state.transcript if e.kind == "reasoning"]
    assert len(reasoning_entries) == 1
    assert "◇ Reference — advisor-1" in reasoning_entries[0].text
    assert "Two plus two is four." in reasoning_entries[0].text


# ── Full turn replay with live fixture ───────────────────────────────────


def test_moa_full_turn_replay_with_fixture() -> None:
    """Replay the live MoA turn fixture and verify state, transcript, and ordering."""
    assert _MOA_TURN_FIXTURE_PATH.is_file()
    frames = [
        json.loads(line)
        for line in _MOA_TURN_FIXTURE_PATH.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    state = replay(frames)
    assert state.moa is not None
    assert state.moa.phase == "complete"
    assert state.moa.refs_done == 5
    assert state.moa.refs_total == 5
    assert state.moa.aggregator == "openai-codex:gpt-5.6-sol"
    assert len(state.moa.references) == 5
    assert len(state.moa.finished) == 5

    # 5 reasoning entries -> 1 system summary entry -> 1 assistant entry
    reasoning_entries = [e for e in state.transcript if e.kind == "reasoning"]
    assert len(reasoning_entries) == 5
    system_entries = [e for e in state.transcript if e.kind == "system"]
    moa_system = [e for e in system_entries if "Mixture of Agents:" in e.text]
    assert len(moa_system) == 1
    assert moa_system[0].text == (
        "Mixture of Agents: 5/5 references · aggregated by openai-codex:gpt-5.6-sol"
    )
    assistant_entries = [e for e in state.transcript if e.kind == "assistant"]
    assert len(assistant_entries) == 1
    assert assistant_entries[0].text.startswith("Two plus two equals four")


# ── Live line formatting and progression ─────────────────────────────────


def test_moa_live_line_progress() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "advisor-1", "refs_done": 1, "refs_total": 5}),
    ])
    assert state.moa is not None
    assert format_moa_live_line(state.moa) == (
        "Mixture of Agents: collecting 1/5 references · advisor-1 finished"
    )
    v = moa_view(state.moa)
    assert v.live_text == "Mixture of Agents: collecting 1/5 references · advisor-1 finished"
    assert v.is_active

    state = replay([
        raw_event("moa.progress", {"label": "advisor-2", "refs_done": 2, "refs_total": 5}),
    ], state)
    assert state.moa is not None
    assert format_moa_live_line(state.moa) == (
        "Mixture of Agents: collecting 2/5 references · advisor-2 finished"
    )

    state = replay([
        raw_event(
            "moa.phase",
            {"phase": "aggregator", "refs_done": 5, "refs_total": 5, "aggregator": "agg-model"},
        ),
    ], state)
    assert state.moa is not None
    assert state.moa.phase == "aggregating"
    assert format_moa_live_line(state.moa) == (
        "Mixture of Agents: aggregating 5/5 references · agg-model"
    )


# ── Cancellation, failure, and connection loss ───────────────────────────


def test_moa_cancellation_during_references() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 2, "refs_total": 5}),
    ])
    cancelled = cancel_turn(state, at=BASE_TIME + 10)
    assert cancelled.moa is not None
    assert cancelled.moa.phase == "cancelled"
    assert cancelled.moa.is_terminal
    system_entries = [e for e in cancelled.transcript if e.kind == "system"]
    assert any("Mixture of Agents: interrupted at 2/5 references" in e.text for e in system_entries)


def test_moa_cancellation_while_aggregating() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("moa.aggregating", {"aggregator": "openai-codex:gpt-5.6-sol"}),
    ])
    cancelled = cancel_turn(state, at=BASE_TIME + 10)
    assert cancelled.moa is not None
    assert cancelled.moa.phase == "cancelled"
    system_entries = [e for e in cancelled.transcript if e.kind == "system"]
    assert any(
        "Mixture of Agents: interrupted while aggregating · openai-codex:gpt-5.6-sol" in e.text
        for e in system_entries
    )


def test_moa_failure_during_references() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 2, "refs_total": 5}),
        raw_event("error", {"message": "turn failed"}),
    ])
    assert state.moa is not None
    assert state.moa.phase == "failed"
    system_entries = [e for e in state.transcript if e.kind == "system"]
    assert any("Mixture of Agents: failed at 2/5 references" in e.text for e in system_entries)


def test_moa_failure_while_aggregating() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("moa.aggregating", {"aggregator": "openai-codex:gpt-5.6-sol"}),
        raw_event("error", {"message": "turn failed"}),
    ])
    assert state.moa is not None
    assert state.moa.phase == "failed"
    system_entries = [e for e in state.transcript if e.kind == "system"]
    assert any(
        "Mixture of Agents: failed while aggregating · openai-codex:gpt-5.6-sol" in e.text
        for e in system_entries
    )


def test_moa_connection_lost_during_references() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 2, "refs_total": 5}),
    ])
    lost = set_connection(state, "disconnected", cause="orderly_close", at=BASE_TIME + 10)
    assert lost.moa is not None
    assert lost.moa.phase == "lost"
    system_entries = [e for e in lost.transcript if e.kind == "system"]
    assert any(
        "Mixture of Agents: connection lost at 2/5 references" in e.text
        for e in system_entries
    )


def test_moa_connection_lost_while_aggregating() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("moa.aggregating", {"aggregator": "openai-codex:gpt-5.6-sol"}),
    ])
    lost = set_connection(state, "disconnected", cause="orderly_close", at=BASE_TIME + 10)
    assert lost.moa is not None
    assert lost.moa.phase == "lost"
    system_entries = [e for e in lost.transcript if e.kind == "system"]
    assert any(
        "Mixture of Agents: connection lost while aggregating · openai-codex:gpt-5.6-sol" in e.text
        for e in system_entries
    )


# ── Fallback, partial run, unrecognised phase, late events ───────────────


def test_moa_fallback_when_no_moa_events() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("message.delta", {"text": "hello"}),
        raw_event("message.complete", {"text": "hello"}),
    ])
    assert state.moa is None
    v = moa_view(state.moa)
    assert v.live_text is None
    assert v.committed_text is None
    assert v.inspector_rows == ("no progress events observed",)
    assert not v.is_active


def test_moa_partial_run_no_aggregation_phase() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("message.complete", {"text": "result"}),
    ])
    assert state.moa is not None
    assert state.moa.phase == "complete"
    system_entries = [e for e in state.transcript if e.kind == "system"]
    assert any(
        "Mixture of Agents: 5/5 references · no aggregation phase observed" in e.text
        for e in system_entries
    )


def test_moa_unrecognised_wire_phase() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.phase", {"phase": "evaluating", "refs_done": 3, "refs_total": 5}),
    ])
    assert state.moa is not None
    assert state.moa.wire_phase == "evaluating"
    assert format_moa_live_line(state.moa) == (
        'Mixture of Agents: phase "evaluating" · 3/5 references'
    )


def test_moa_late_events_ignored() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("message.complete", {"text": "done"}),
    ])
    assert state.moa is not None
    assert state.moa.is_terminal
    initial_ignored = state.late_events_ignored

    after_progress = replay(
        [raw_event("moa.progress", {"label": "m2", "refs_done": 5, "refs_total": 5})], state
    )
    assert after_progress.late_events_ignored == initial_ignored + 1

    after_ref = replay([raw_event("moa.reference", {"text": "late"})], after_progress)
    assert after_ref.late_events_ignored == initial_ignored + 2

    after_phase = replay([raw_event("moa.phase", {"phase": "aggregator"})], after_ref)
    assert after_phase.late_events_ignored == initial_ignored + 3

    after_agg = replay([raw_event("moa.aggregating", {"aggregator": "late-agg"})], after_phase)
    assert after_agg.late_events_ignored == initial_ignored + 4


def test_moa_cleared_on_next_message_start() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("message.complete", {"text": "done"}),
    ])
    assert state.moa is not None
    next_state = replay([raw_event("message.start")], state)
    assert next_state.moa is None


# ── Formatting permutations & Inspector rows ────────────────────────────


def test_format_moa_inspector_rows_live_and_terminal() -> None:
    assert format_moa_inspector_rows(None) == ("no progress events observed",)

    ref1 = MoaReferenceRecord(label="model-a", first_line="First advice", chars=50)
    ref2 = MoaReferenceRecord(label="model-b", first_line="Second advice", chars=60)
    run_live = MoaRun(
        phase="references",
        refs_done=2,
        refs_total=5,
        finished=("model-a", "model-b"),
        references=(ref1, ref2),
    )
    rows = format_moa_inspector_rows(run_live)
    assert rows == (
        "collecting 2/5 references · model-b finished",
        "model-a · finished 1/5 · First advice",
        "model-b · finished 2/5 · Second advice",
        "3 pending",
    )

    run_complete = MoaRun(
        phase="complete",
        refs_done=5,
        refs_total=5,
        finished=("model-a", "model-b", "model-c", "model-d", "model-e"),
        references=(ref1, ref2),
        aggregator="gpt-5.6",
    )
    rows_complete = format_moa_inspector_rows(run_complete)
    assert rows_complete[0] == "5/5 references · aggregated by gpt-5.6"
    assert rows_complete[1] == "model-a · finished 1/5 · First advice"
    assert rows_complete[2] == "model-b · finished 2/5 · Second advice"
    assert rows_complete[3] == "model-c · finished 3/5"
    assert not any("pending" in r for r in rows_complete)


def test_format_moa_lines_edge_cases() -> None:
    # None counters format as '?'
    run_unknown = MoaRun(phase="references")
    assert format_moa_live_line(run_unknown) == "Mixture of Agents: collecting 0/? references"
    assert format_moa_committed_line(run_unknown) == "Mixture of Agents: collecting 0/? references"

    # Aggregating without aggregator name
    run_agg_no_name = MoaRun(phase="aggregating", refs_done=3, refs_total=3)
    assert format_moa_live_line(run_agg_no_name) == "Mixture of Agents: aggregating 3/3 references"

    # Interrupted while aggregating without aggregator. The taxonomy addendum
    # (F-1, Shape B) makes reached_aggregating the claim the terminal strings
    # read; a record with the aggregator wire string but without the flag is
    # state no event sequence can produce, so the constructed records carry
    # both — exactly what the reducer emits for moa.phase "aggregator".
    run_interrupted = MoaRun(
        phase="cancelled",
        refs_done=3,
        refs_total=3,
        wire_phase="aggregator",
        reached_aggregating=True,
    )
    assert format_moa_committed_line(run_interrupted) == (
        "Mixture of Agents: interrupted while aggregating"
    )

    # Failed while aggregating without aggregator
    run_failed = MoaRun(
        phase="failed",
        refs_done=3,
        refs_total=3,
        wire_phase="aggregator",
        reached_aggregating=True,
    )
    assert format_moa_committed_line(run_failed) == "Mixture of Agents: failed while aggregating"

    # Lost while aggregating without aggregator
    run_lost = MoaRun(
        phase="lost",
        refs_done=3,
        refs_total=3,
        wire_phase="aggregator",
        reached_aggregating=True,
    )
    assert format_moa_committed_line(run_lost) == (
        "Mixture of Agents: connection lost while aggregating"
    )


# ── Projection and Changes wiring ────────────────────────────────────────


def test_moa_projection_and_changes() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 1, "refs_total": 3}),
    ])
    snap = project(state)
    assert snap.moa.is_active
    assert snap.moa.live_text == "Mixture of Agents: collecting 1/3 references · m1 finished"

    entries = entry_scoped_view(state)
    assert entries.moa.is_active
    assert entries.moa.live_text == "Mixture of Agents: collecting 1/3 references · m1 finished"

    insp = inspector_view(entries)
    assert insp.moa.is_active
    assert insp.moa.inspector_rows[0] == "collecting 1/3 references · m1 finished"
    assert insp.moa.inspector_rows[1] == "m1 · finished 1/3"
    assert insp.moa.inspector_rows[2] == "2 pending"


# ── C10 review findings: F-3, F-4, F-6 ──────────────────────────────────


def test_moa_unrecognised_wire_phase_is_released_by_aggregating() -> None:
    """F-3 of the C10 review, as the taxonomy addendum closed it: the release
    is a rendering precedence, not a field change. Most advanced state wins —
    aggregating beats an unrecognised wire phase in either arrival order —
    and ``wire_phase`` keeps the unrecognised string verbatim for the record
    (it renders again only if the run is still in ``references``)."""
    released = replay([
        raw_event("message.start"),
        raw_event("moa.phase", {"phase": "deliberating", "refs_done": 1, "refs_total": 4}),
        raw_event("moa.aggregating", {"aggregator": "claude"}),
    ])
    assert released.moa is not None
    assert released.moa.phase == "aggregating"
    assert released.moa.wire_phase == "deliberating"
    assert format_moa_live_line(released.moa) == (
        "Mixture of Agents: aggregating 1/4 references · claude"
    )

    # The reverse order, same precedence: an unrecognized phase arriving
    # after the aggregator keeps its string on the record but cannot
    # displace the aggregating line — and cannot un-reach aggregation.
    restated = replay([
        raw_event("message.start"),
        raw_event("moa.phase", {"phase": "aggregator", "refs_done": 4, "refs_total": 4}),
        raw_event("moa.aggregating", {"aggregator": "claude"}),
        raw_event("moa.phase", {"phase": "voting"}),
    ])
    assert restated.moa is not None
    assert restated.moa.wire_phase == "voting"
    assert restated.moa.reached_aggregating is True
    assert format_moa_live_line(restated.moa) == (
        "Mixture of Agents: aggregating 4/4 references · claude"
    )

    # While the run is still in references, an unrecognized phase renders.
    still_references = replay([
        raw_event("message.start"),
        raw_event("moa.phase", {"phase": "deliberating", "refs_done": 3, "refs_total": 5}),
    ])
    assert still_references.moa is not None
    assert format_moa_live_line(still_references.moa) == (
        'Mixture of Agents: phase "deliberating" · 3/5 references'
    )


# ── C10 F-1, ruled Shape B (taxonomy addendum on #148) ───────────────────


def test_either_aggregator_event_alone_reaches_aggregating_for_all_three_terminal_lines() -> None:
    """F-1 of the C10 review: the bare ``moa.aggregating`` — the shape section
    2 promises will suffice and the observed fixture never exercises — now
    carries into every terminal "… while aggregating" string."""
    collecting = [
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "gemini", "refs_done": 3, "refs_total": 5}),
    ]
    aggregating_alone = replay([raw_event("moa.aggregating", {})], replay(collecting))
    assert aggregating_alone.moa is not None
    assert aggregating_alone.moa.reached_aggregating is True
    assert aggregating_alone.moa.aggregator == ""
    assert format_moa_live_line(aggregating_alone.moa) == (
        "Mixture of Agents: aggregating 3/5 references"
    )

    cancelled = cancel_turn(aggregating_alone, at=BASE_TIME + 10)
    assert cancelled.moa is not None
    assert format_moa_committed_line(cancelled.moa) == (
        "Mixture of Agents: interrupted while aggregating"
    )

    failed = replay(
        [raw_event("error", {"message": "turn failed"})],
        replay(collecting + [raw_event("moa.aggregating", {})]),
    )
    assert failed.moa is not None
    assert format_moa_committed_line(failed.moa) == (
        "Mixture of Agents: failed while aggregating"
    )

    lost = set_connection(
        replay(collecting + [raw_event("moa.aggregating", {})]),
        "disconnected",
        cause="orderly_close",
        at=BASE_TIME + 10,
    )
    assert lost.moa is not None
    assert format_moa_committed_line(lost.moa) == (
        "Mixture of Agents: connection lost while aggregating"
    )


def test_moa_phase_with_aggregator_alone_also_reaches_aggregating() -> None:
    """The second of the two events section 2 promises suffices on its own."""
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 2, "refs_total": 5}),
        raw_event("moa.phase", {"phase": "aggregator", "refs_done": 5, "refs_total": 5}),
    ])
    assert state.moa is not None
    assert state.moa.reached_aggregating is True
    cancelled = cancel_turn(state, at=BASE_TIME + 10)
    assert cancelled.moa is not None
    assert format_moa_committed_line(cancelled.moa) == (
        "Mixture of Agents: interrupted while aggregating"
    )


def test_reached_aggregating_is_monotonic_under_a_later_unknown_phase() -> None:
    """The case the reviewer simulated breaking Shape A: a later
    ``moa.phase`` string cannot un-reach an aggregation that happened, so
    the terminal line does not revert to the references string."""
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 3, "refs_total": 5}),
        raw_event("moa.aggregating", {"aggregator": "claude"}),
        raw_event("moa.phase", {"phase": "voting"}),
    ])
    assert state.moa is not None
    assert state.moa.wire_phase == "voting"
    assert state.moa.reached_aggregating is True
    cancelled = cancel_turn(state, at=BASE_TIME + 10)
    assert cancelled.moa is not None
    assert format_moa_committed_line(cancelled.moa) == (
        "Mixture of Agents: interrupted while aggregating · claude"
    )


def test_a_run_that_never_aggregated_keeps_the_references_terminal_strings() -> None:
    """The flag is a domain claim, not a default: without either aggregator
    event the terminal strings stay the references form."""
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 2, "refs_total": 5}),
    ])
    assert state.moa is not None
    assert state.moa.reached_aggregating is False
    cancelled = cancel_turn(state, at=BASE_TIME + 10)
    assert cancelled.moa is not None
    assert format_moa_committed_line(cancelled.moa) == (
        "Mixture of Agents: interrupted at 2/5 references"
    )


def test_moa_reference_before_progress_waits_for_the_finish_event() -> None:
    """F-4 of the C10 review, from the ruling's two events: a row exists
    because the advisor finished (``moa.progress``) and only *becomes*
    referenced once its ``moa.reference`` arrives. A reference for an advisor
    that has not finished waits — it never invents a finish or an ordinal the
    ``finished`` roster does not contain."""
    before_progress = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "gemini", "refs_done": 1, "refs_total": 5}),
        raw_event("moa.reference", {"label": "zeta", "text": "z line"}),
    ])
    assert before_progress.moa is not None
    assert format_moa_inspector_rows(before_progress.moa) == (
        "collecting 1/5 references · gemini finished",
        "gemini · finished 1/5",
        "4 pending",
    )

    after_progress = replay(
        [raw_event("moa.progress", {"label": "zeta", "refs_done": 2, "refs_total": 5})],
        before_progress,
    )
    assert after_progress.moa is not None
    assert format_moa_inspector_rows(after_progress.moa) == (
        "collecting 2/5 references · zeta finished",
        "gemini · finished 1/5",
        "zeta · finished 2/5 · z line",
        "3 pending",
    )


def test_moa_terminal_run_leaves_no_pending_row() -> None:
    """F-6 of the C10 review: the remainder row is "while any are
    outstanding", and outstanding ends with the run. A terminal turn
    abandoned its remainder — a pending row would tell the operator advisors
    are still working on a turn that ended."""
    completed = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 1, "refs_total": 3}),
        raw_event("moa.progress", {"label": "m2", "refs_done": 2, "refs_total": 3}),
        raw_event("message.complete", {"text": "done"}),
    ])
    assert completed.moa is not None
    assert completed.moa.is_terminal
    assert format_moa_inspector_rows(completed.moa) == (
        "2/3 references · no aggregation phase observed",
        "m1 · finished 1/3",
        "m2 · finished 2/3",
    )

    interrupted = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 3, "refs_total": 5}),
    ])
    cancelled = cancel_turn(interrupted, at=BASE_TIME + 10)
    assert cancelled.moa is not None
    assert cancelled.moa.is_terminal
    assert format_moa_inspector_rows(cancelled.moa) == (
        "interrupted at 3/5 references",
        "m1 · finished 1/5",
    )


# ── message.complete payload status honesty ──────────────────────────────
#
# Hermes can close a turn with message.complete and payload.status in
# {complete, error, interrupted}. _on_message_complete currently ignores
# that key and always treats the turn as a successful complete.


_AGGREGATOR = "openai-codex:gpt-5.6-sol"


def test_message_complete_status_error_during_aggregation_fails_honestly() -> None:
    """A failing aggregator must not fabricate a completion line."""
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("moa.aggregating", {"aggregator": _AGGREGATOR}),
        raw_event("message.complete", {"text": "API failed", "status": "error"}),
    ])
    assert state.moa is not None
    assert state.moa.phase == "failed"
    assert state.turn == "idle"
    system_entries = [e for e in state.transcript if e.kind == "system"]
    assert any(
        e.text == f"Mixture of Agents: failed while aggregating · {_AGGREGATOR}"
        for e in system_entries
    )
    assert not any("aggregated by" in e.text for e in system_entries)
    error_entries = [e for e in state.transcript if e.kind == "error"]
    assert any("API failed" in e.text for e in error_entries)
    assert not any(e.kind == "assistant" and "API failed" in e.text for e in state.transcript)


def test_message_complete_status_error_without_moa_is_error_entry() -> None:
    """Non-MoA completions with status=error are errors, not assistant text."""
    state = replay([
        raw_event("message.start"),
        raw_event("message.complete", {"text": "something broke", "status": "error"}),
    ])
    assert state.moa is None
    assert state.turn == "idle"
    assert [e.kind for e in state.transcript] == ["error"]
    assert "something broke" in state.transcript[0].text


def test_message_complete_status_complete_matches_existing_path() -> None:
    """status=complete must stay byte-identical to the status-absent complete path."""
    shared = [
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("moa.aggregating", {"aggregator": _AGGREGATOR}),
    ]
    with_status = replay(
        [*shared, raw_event("message.complete", {"text": "result", "status": "complete"})]
    )
    without_status = replay([*shared, raw_event("message.complete", {"text": "result"})])
    assert with_status.turn == without_status.turn == "idle"
    assert with_status.transcript == without_status.transcript
    assert with_status.moa == without_status.moa
    assert with_status.moa is not None
    assert with_status.moa.phase == "complete"
    system_entries = [e for e in with_status.transcript if e.kind == "system"]
    assert any(
        e.text == f"Mixture of Agents: 5/5 references · aggregated by {_AGGREGATOR}"
        for e in system_entries
    )
    assert any(e.kind == "assistant" and e.text == "result" for e in with_status.transcript)


def test_message_complete_status_interrupted_mid_aggregation_cancels() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("moa.aggregating", {"aggregator": _AGGREGATOR}),
        raw_event("message.complete", {"text": "stopped", "status": "interrupted"}),
    ])
    assert state.moa is not None
    assert state.moa.phase == "cancelled"
    assert state.turn == "cancelled"
    system_entries = [e for e in state.transcript if e.kind == "system"]
    assert any(
        f"interrupted while aggregating · {_AGGREGATOR}" in e.text for e in system_entries
    )
    assert not any("aggregated by" in e.text for e in system_entries)


def test_message_complete_absent_status_is_treated_as_complete() -> None:
    """Older gateways omit status; that remains a successful complete."""
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("moa.aggregating", {"aggregator": _AGGREGATOR}),
        raw_event("message.complete", {"text": "result"}),
    ])
    assert state.moa is not None
    assert state.moa.phase == "complete"
    assert state.turn == "idle"
    system_entries = [e for e in state.transcript if e.kind == "system"]
    assert any(
        e.text == f"Mixture of Agents: 5/5 references · aggregated by {_AGGREGATOR}"
        for e in system_entries
    )
    assert any(e.kind == "assistant" and e.text == "result" for e in state.transcript)


def test_error_event_then_message_complete_status_error_keeps_terminal() -> None:
    """The first terminal outcome wins; a later status=error must not restamp it."""
    after_error = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("moa.aggregating", {"aggregator": _AGGREGATOR}),
        raw_event("error", {"message": "turn failed"}),
    ])
    assert after_error.moa is not None
    assert after_error.moa.phase == "failed"
    summaries_after_error = [
        e
        for e in after_error.transcript
        if e.kind == "system" and "Mixture of Agents:" in e.text
    ]
    assert len(summaries_after_error) == 1

    after_complete = replay(
        [raw_event("message.complete", {"text": "API failed", "status": "error"})],
        after_error,
    )
    assert after_complete.moa is not None
    assert after_complete.moa.phase == "failed"
    assert after_complete.turn == "idle"
    summaries_after_complete = [
        e
        for e in after_complete.transcript
        if e.kind == "system" and "Mixture of Agents:" in e.text
    ]
    assert summaries_after_complete == summaries_after_error
    assert [e.kind for e in after_complete.transcript] == [
        e.kind for e in after_error.transcript
    ]
    assert [e for e in after_complete.transcript if e.kind == "error"] == [
        e for e in after_error.transcript if e.kind == "error"
    ]


def test_message_complete_unrecognized_status_falls_back_to_complete() -> None:
    state = replay([
        raw_event("message.start"),
        raw_event("moa.progress", {"label": "m1", "refs_done": 5, "refs_total": 5}),
        raw_event("moa.aggregating", {"aggregator": _AGGREGATOR}),
        raw_event(
            "message.complete",
            {"text": "result", "status": "unknown_weird_status"},
        ),
    ])
    assert state.moa is not None
    assert state.moa.phase == "complete"
    assert state.turn == "idle"
    system_entries = [e for e in state.transcript if e.kind == "system"]
    assert any(
        e.text == f"Mixture of Agents: 5/5 references · aggregated by {_AGGREGATOR}"
        for e in system_entries
    )
    assert any(e.kind == "assistant" and e.text == "result" for e in state.transcript)


def test_moa_failing_turn_wire_replay_orders_summary_then_error() -> None:
    """End-to-end replay of the live MoA fixture with a failing complete.

    The capture is a successful turn. Only the terminal payload is rewritten
    to the failing-aggregation wire shape; advisor frames stay as observed.
    """
    frames = [
        json.loads(line)
        for line in _MOA_TURN_FIXTURE_PATH.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    complete = frames[-1]
    assert complete["params"]["type"] == "message.complete"
    complete["params"]["payload"]["text"] = "API failed"
    complete["params"]["payload"]["status"] = "error"

    state = replay(frames)
    assert state.moa is not None
    assert state.moa.phase == "failed"
    assert state.turn == "idle"
    assert [e.kind for e in state.transcript[-2:]] == ["system", "error"]
    assert state.transcript[-2].text == (
        f"Mixture of Agents: failed while aggregating · {_AGGREGATOR}"
    )
    assert "API failed" in state.transcript[-1].text
    assert not any(e.kind == "assistant" for e in state.transcript)


def test_message_complete_status_error_preserves_partial_text_and_error_field() -> None:
    """Gateway error completions can carry partial assistant text plus a
    separate error field. The partial is content (R6); the error is why it
    stopped. They must not be collapsed into one error line."""
    state = replay([
        raw_event("message.start"),
        raw_event(
            "message.complete",
            {
                "text": "half an ans",
                "error": "connection reset mid-stream",
                "status": "error",
                "partial": True,
            },
        ),
    ])
    assert state.turn == "idle"
    assert [e.kind for e in state.transcript] == ["assistant", "error"]
    assert state.transcript[0].text == "half an ans"
    assert "connection reset mid-stream" in state.transcript[1].text
    assert "half an ans" not in state.transcript[1].text


def test_message_complete_status_error_without_partial_does_not_become_assistant() -> None:
    """Ordinary failures synthesize an error representation in ``text``.
    Without ``partial=True`` that string is not assistant content."""
    state = replay([
        raw_event("message.start"),
        raw_event(
            "message.complete",
            {
                "text": "Error: invalid model",
                "error": "invalid model",
                "status": "error",
            },
        ),
    ])
    assert state.turn == "idle"
    assert [e.kind for e in state.transcript] == ["error"]
    assert state.transcript[0].text == "error: invalid model"
    assert not any(e.kind == "assistant" for e in state.transcript)


def test_message_complete_non_string_error_field_is_coerced_not_dumped() -> None:
    """A dict (or other non-string) ``error`` field must go through
    ``coerce_text``, not ``str(dict)``, so the transcript never contains a
    Python repr dump."""
    error_field = {"code": "E_CONN", "detail": "reset"}
    state = replay([
        raw_event("message.start"),
        raw_event(
            "message.complete",
            {
                "text": "something broke",
                "error": error_field,
                "status": "error",
            },
        ),
    ])
    joined = "\n".join(e.text for e in state.transcript)
    assert str(error_field) not in joined
    assert "E_CONN" not in joined
    error_entries = [e for e in state.transcript if e.kind == "error"]
    assert error_entries
    assert "something broke" in error_entries[0].text or "unknown error" in error_entries[0].text
    assert not any(e.kind == "assistant" for e in state.transcript)


def test_error_event_then_message_complete_status_error_without_moa_does_not_duplicate() -> None:
    """A prior error event already recorded the outcome; the follow-up
    status=error complete must not append a second error line."""
    after_error = replay([
        raw_event("message.start"),
        raw_event("error", {"message": "first error"}),
    ])
    assert [e.kind for e in after_error.transcript] == ["error"]
    assert [e.text for e in after_error.transcript] == ["error: first error"]

    after_complete = replay(
        [raw_event("message.complete", {"text": "first error", "status": "error"})],
        after_error,
    )
    assert after_complete.turn == "idle"
    assert [e.kind for e in after_complete.transcript] == ["error"]
    assert [e.text for e in after_complete.transcript] == ["error: first error"]


def test_message_complete_preprefixed_long_error_is_clipped() -> None:
    """A payload whose ``text`` already starts with ``error: `` must still
    be bounded by ``TRANSCRIPT_LINE_CLIP``. The prefix is not a licence to
    skip the clip."""
    state = replay([
        raw_event("message.start"),
        raw_event(
            "message.complete",
            {"text": "error: " + ("x" * 2500), "status": "error"},
        ),
    ])
    errors = [e for e in state.transcript if e.kind == "error"]
    assert len(errors) == 1
    assert errors[0].text.startswith("error: ")
    assert len(errors[0].text) <= TRANSCRIPT_LINE_CLIP + 1
    assert errors[0].text.endswith("…")
    assert not any(e.kind == "assistant" for e in state.transcript)


def test_partial_error_without_error_field_does_not_reuse_assistant_text() -> None:
    """Partial assistant content is not an error reason. With no ``error``
    field the diagnostic line is the unknown-error fallback."""
    state = replay([
        raw_event("message.start"),
        raw_event(
            "message.complete",
            {"text": "half an ans", "status": "error", "partial": True},
        ),
    ])
    assert [e.kind for e in state.transcript] == ["assistant", "error"]
    assert state.transcript[0].text == "half an ans"
    assert state.transcript[1].text == "error: unknown error"


def test_partial_error_with_non_string_error_field_uses_unknown_error() -> None:
    """A non-string ``error`` field is not a reason and must not dump a
    dict repr or reuse the partial assistant text."""
    error_field = {"detail": "fail"}
    state = replay([
        raw_event("message.start"),
        raw_event(
            "message.complete",
            {
                "text": "half an ans",
                "error": error_field,
                "status": "error",
                "partial": True,
            },
        ),
    ])
    assert [e.kind for e in state.transcript] == ["assistant", "error"]
    assert state.transcript[0].text == "half an ans"
    assert state.transcript[1].text == "error: unknown error"
    assert str(error_field) not in "\n".join(e.text for e in state.transcript)
