"""The gate harness itself: does the instrument work, and can it fail?

A measurement harness nobody has seen fail is a harness that might not measure
anything. Every check below either proves a measurement responds to the thing it
claims to measure, or proves the verdict arithmetic turns a breached threshold
into a ``fail`` — because the plan makes a fail verdict *halt the run*, and a
gate that cannot produce one is worse than no gate.

The gate is run here at a fraction of KTD14's size. The full-size run is not a
test; it is the recorded measurement in
``docs/analysis/2026-08-03-textual-validation-gate-results.md``.
"""

from __future__ import annotations

from typing import Any

import pytest

from talaria.domain.models import TranscriptEntry
from talaria.domain.projection import TranscriptView
from talaria.domain.state import SessionState
from talaria.replay import gate as gate_module
from talaria.replay.gate import (
    CorpusIdentity,
    GateMeasurement,
    GateResult,
    _cadence_speed,
    _fit_slope,
    content_is_complete,
    measure_replay,
    run_gate,
    stress_corpus_identity,
)
from talaria.replay.stress import build_stress_corpus


def _identity() -> CorpusIdentity:
    return CorpusIdentity(label="t", sha256="0" * 64, frame_count=0, kind="test")


def _measurement(**overrides: Any) -> GateMeasurement:
    base: dict[str, Any] = {
        "corpus": _identity(),
        "frames_applied": 1,
        "render_ticks": 1,
        "elapsed_seconds": 1.0,
        "render_ticks_per_second": 1.0,
        "peak_mounted_widgets": 1,
        "condensed_lines": 0,
    }
    base.update(overrides)
    return GateMeasurement(**base)


# ── the measurements respond to what they measure ────────────────────────


def test_content_completeness_detects_a_dropped_entry() -> None:
    state = SessionState(
        transcript=(
            TranscriptEntry(kind="assistant", text="first", turn_index=0, seq=1),
            TranscriptEntry(kind="assistant", text="second", turn_index=0, seq=2),
        )
    )
    complete = TranscriptView(lines=("first", "second"), entry_count=2)
    lossy = TranscriptView(lines=("first",), entry_count=1)
    reordered = TranscriptView(lines=("second", "first"), entry_count=2)

    assert content_is_complete(state, complete) is True
    assert content_is_complete(state, lossy) is False
    assert content_is_complete(state, reordered) is False


def test_the_memory_slope_is_a_fit_over_the_series_not_the_endpoints() -> None:
    flat = [(0, 50.0), (5000, 50.0), (10000, 50.0)]
    rising = [(0, 50.0), (5000, 55.0), (10000, 60.0)]
    assert _fit_slope(flat) == pytest.approx(0.0)
    assert _fit_slope(rising) == pytest.approx(1.0)
    assert _fit_slope([(0, 50.0)]) == 0.0


def test_the_cadence_speed_scales_with_the_corpus_duration() -> None:
    short = build_stress_corpus(deltas=200).records
    long_corpus = build_stress_corpus(deltas=20_000).records
    assert _cadence_speed(short) < _cadence_speed(long_corpus)
    assert _cadence_speed(()) == 1.0


# ── the verdict arithmetic ───────────────────────────────────────────────


def test_a_breached_threshold_produces_a_fail_verdict() -> None:
    result = GateResult(
        stress=_measurement(),
        cadence=_measurement(),
        live=None,
        determinism_identical=True,
        inert_controls_refused=(),
        matrix={},
        checks={
            "ok": {"measured": 1, "threshold": 2, "comparison": "<=", "pass": True},
            "breached": {"measured": 900, "threshold": 600, "comparison": "<=", "pass": False},
        },
    )
    assert result.passed is False
    assert result.verdict == "fail"
    assert result.to_dict()["verdict"] == "fail"


def test_all_checks_passing_produces_a_pass_verdict() -> None:
    result = GateResult(
        stress=_measurement(),
        cadence=_measurement(),
        live=None,
        determinism_identical=True,
        inert_controls_refused=(),
        matrix={},
        checks={"ok": {"measured": 1, "threshold": 2, "comparison": "<=", "pass": True}},
    )
    assert result.passed is True
    assert result.verdict == "pass"


# ── an end-to-end run at reduced size ────────────────────────────────────


@pytest.mark.asyncio
async def test_the_gate_runs_end_to_end_and_records_corpus_identity() -> None:
    result = await run_gate(live_corpus=None, deltas=600, seed=7)
    assert result.verdict == "pass"
    assert result.stress.corpus.kind == "synthetic-stress"
    assert len(result.stress.corpus.sha256) == 64
    assert result.stress.corpus.frame_count > 600
    assert result.stress.frames_applied == result.stress.corpus.frame_count
    assert result.stress.content_loss_failures == 0
    assert result.cadence.content_loss_failures == 0
    assert result.matrix["textual"].startswith("8.")
    # The identity is opaque: nothing that could be a local path.
    assert "/" not in result.stress.corpus.label


@pytest.mark.asyncio
async def test_the_stress_corpus_is_reproducible_from_its_seed() -> None:
    first = build_stress_corpus(deltas=400, seed=11)
    second = build_stress_corpus(deltas=400, seed=11)
    different = build_stress_corpus(deltas=400, seed=12)
    assert first.sha256 == second.sha256
    assert first.sha256 != different.sha256
    assert first.label == second.label


@pytest.mark.asyncio
async def test_a_lowered_ceiling_makes_the_gate_fail(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The gate is demonstrably capable of failing, not merely of passing."""
    monkeypatch.setattr(gate_module, "MOUNTED_WIDGET_CEILING", 1)
    result = await run_gate(live_corpus=None, deltas=400, seed=3)
    assert result.verdict == "fail"
    assert result.checks["mounted_widgets"]["pass"] is False


@pytest.mark.asyncio
async def test_measuring_a_replay_reports_the_frames_it_actually_applied() -> None:
    corpus = build_stress_corpus(deltas=300, seed=5)
    measurement, state, refusals = await measure_replay(
        corpus.records, stress_corpus_identity(corpus)
    )
    assert measurement.frames_applied == corpus.frame_count
    assert measurement.transcript_entries > 0
    assert measurement.rss_series_mb[0][0] == 0
    assert refusals == ()
    # The malformed frames in the corpus were surfaced, not skipped.
    assert state.protocol_error_count > 0
    assert state.unknown_event_types
