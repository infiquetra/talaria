"""Tests for C10 (#148): Mixture of Agents event decoding, normalization, and models."""

from __future__ import annotations

import json
from pathlib import Path

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
    is_terminal_moa_phase,
    keep_terminal_moa_phase,
    normalize_moa_phase,
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
