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
    """Validate the sanitized live-captured MoA route turn fixture."""
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

    # Interrupted while aggregating without aggregator
    run_interrupted = MoaRun(phase="cancelled", refs_done=3, refs_total=3, wire_phase="aggregator")
    assert format_moa_committed_line(run_interrupted) == (
        "Mixture of Agents: interrupted while aggregating"
    )

    # Failed while aggregating without aggregator
    run_failed = MoaRun(phase="failed", refs_done=3, refs_total=3, wire_phase="aggregator")
    assert format_moa_committed_line(run_failed) == "Mixture of Agents: failed while aggregating"

    # Lost while aggregating without aggregator
    run_lost = MoaRun(phase="lost", refs_done=3, refs_total=3, wire_phase="aggregator")
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
