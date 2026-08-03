"""R40 and AE11: pause, resume, speed, and controls that must do nothing.

AE11's determinism claim is the load-bearing one, and it is only meaningful if
the comparison is over the *whole* domain state rather than a summary. Comparing
transcript text would pass for an implementation that silently dropped a
sub-agent row; comparing the frozen :class:`SessionState` dataclass compares
every field, including the ones a careless replay would corrupt — turn index,
prompt registry, usage counters, ignored-event tallies.
"""

from __future__ import annotations

import asyncio
import math
import sys
from typing import Any

import pytest

from talaria.replay.controls import (
    INERT_NOTICE,
    MAX_GAP_SECONDS,
    MAX_SPEED,
    MIN_SPEED,
    MUTATION_CONTROLS,
    ReplayControls,
)
from talaria.replay.source import ReplaySource
from talaria.status.contract import ProcessLimits
from talaria.status.runner import StatusRunner
from talaria.ui.app import TalariaApp
from tests.ui.conftest import event, records, streaming_turn


def _corpus() -> list[Any]:
    frames: list[Any] = [event("gateway.ready", {})]
    for turn in range(6):
        frames.extend(streaming_turn([f"t{turn} chunk {n} " for n in range(20)]))
        frames.append(
            event(
                "subagent.start",
                {"subagent_id": f"a{turn}", "goal": "worker", "task_index": turn},
            )
        )
        frames.append(
            event("subagent.complete", {"subagent_id": f"a{turn}", "status": "completed"})
        )
    frames.append({"jsonrpc": "2.0", "method": "event", "params": {"type": "no.such.event"}})
    frames.append("not an object at all")
    return frames


async def _final_state(*, speed: float, pause_at: int | None = None) -> Any:
    controls = ReplayControls()
    controls.set_speed(speed)
    source = ReplaySource(records(_corpus()), controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)
    async with app.run_test(size=(100, 30)) as pilot:
        if pause_at is not None:
            while app.frames_applied < pause_at and not app.replay_complete.is_set():
                await asyncio.sleep(0.005)
            controls.pause()
            assert controls.paused is True
            frozen = app.frames_applied
            await pilot.pause()
            await asyncio.sleep(0.05)
            # Pause takes effect at the next frame boundary, so at most the one
            # frame already past the gate may still land. What must not happen
            # is the stream continuing.
            assert app.frames_applied <= frozen + 1, "frames kept arriving while paused"
            controls.resume()
        await app.drain(timeout=60.0)
        state = app.state
        await app.shutdown_sources()
    return state


# ── determinism (AE11) ───────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_replay_speed_and_pausing_cannot_change_the_final_state() -> None:
    baseline = await _final_state(speed=MAX_SPEED)
    unbounded = await _final_state(speed=math.inf)
    with_pause = await _final_state(speed=MAX_SPEED, pause_at=40)
    assert baseline == unbounded
    assert baseline == with_pause


@pytest.mark.asyncio
async def test_replaying_the_same_corpus_twice_gives_the_same_state() -> None:
    first = await _final_state(speed=math.inf)
    second = await _final_state(speed=math.inf)
    assert first == second


# ── the controls themselves ──────────────────────────────────────────────


def test_speed_is_clamped_and_unbounded_is_its_own_state() -> None:
    controls = ReplayControls()
    assert controls.set_speed(0) == MIN_SPEED
    assert controls.set_speed(-4) == MIN_SPEED
    assert controls.set_speed(float("nan")) == MIN_SPEED
    assert controls.set_speed(10_000) == MAX_SPEED
    assert controls.set_unbounded() == math.inf
    assert controls.unbounded is True
    # Speeding up past unbounded is a no-op rather than an error.
    assert controls.speed_up() == math.inf
    # Slowing down from unbounded lands on the fastest finite rate.
    assert controls.slow_down() == MAX_SPEED


def test_a_recorded_idle_gap_is_clamped_before_it_is_scaled() -> None:
    controls = ReplayControls(speed=1.0)
    assert controls.delay_for(300.0) == MAX_GAP_SECONDS
    assert controls.delay_for(-5.0) == 0.0
    controls.set_speed(4.0)
    assert controls.delay_for(0.4) == pytest.approx(MAX_GAP_SECONDS / 4)
    controls.set_unbounded()
    assert controls.delay_for(300.0) == 0.0


def test_the_label_names_the_state_the_operator_is_in() -> None:
    controls = ReplayControls(speed=2.0)
    assert controls.label == "replaying · 2x"
    controls.pause()
    assert controls.label == "paused · 2x"
    controls.set_unbounded()
    assert controls.label == "paused · max"


# ── inert mutation controls (AE11) ───────────────────────────────────────


def test_every_mutation_control_refuses_and_says_why() -> None:
    controls = ReplayControls()
    for name in sorted(MUTATION_CONTROLS):
        outcome = controls.attempt(name)
        assert outcome.performed is False
        assert outcome.inert is True
        assert outcome.notice == INERT_NOTICE
    assert len(controls.refusals) == len(MUTATION_CONTROLS)


def test_an_unclassified_control_raises_rather_than_being_allowed() -> None:
    controls = ReplayControls()
    with pytest.raises(ValueError, match="unclassified control"):
        controls.attempt("delete-everything")


@pytest.mark.asyncio
async def test_a_mutation_control_invoked_mid_replay_opens_no_socket() -> None:
    controls = ReplayControls(speed=1.0)
    source = ReplaySource(records(_corpus()), controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)
    async with app.run_test(size=(100, 30)) as pilot:
        await pilot.pause()
        modules_before = set(sys.modules)
        app.action_interrupt()
        app.composer.text = "nope"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()

        assert [outcome.name for outcome in controls.refusals] == ["interrupt", "submit"]
        assert INERT_NOTICE in app.composer.notice
        assert app.composer.text == "nope"
        # Nothing dialled: no transport module was pulled in by the attempt.
        newly_imported = set(sys.modules) - modules_before
        assert not any(name.startswith("websockets") for name in newly_imported)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_pause_and_speed_keys_are_bound_and_report_their_state() -> None:
    controls = ReplayControls(speed=1.0)
    source = ReplaySource(records(_corpus()), controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)
    async with app.run_test(size=(100, 30)) as pilot:
        await pilot.press("f8")
        await pilot.pause()
        assert controls.paused is True
        assert "paused" in app.composer.notice

        await pilot.press("f10")
        await pilot.pause()
        assert controls.speed == 2.0
        await pilot.press("f9")
        await pilot.pause()
        assert controls.speed == 1.0

        await pilot.press("f8")
        await pilot.pause()
        assert controls.paused is False
        await app.shutdown_sources()


# ── the status command under replay control (R40) ────────────────────────


@pytest.mark.asyncio
async def test_the_status_command_runs_and_renders_under_replay(tmp_path: Any) -> None:
    script = tmp_path / "status.py"
    script.write_text(
        "import sys\n"
        "payload = sys.stdin.read()\n"
        "assert '\"version\": 1' in payload\n"
        "print('replay status row 1')\n"
        "print('replay status row 2')\n",
        encoding="utf-8",
    )
    runner = StatusRunner(
        argv=(sys.executable, str(script)),
        launch_cwd=tmp_path,
        limits=ProcessLimits(),
    )
    controls = ReplayControls(speed=1.0)
    source = ReplaySource(records(_corpus()), controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls, status_runner=runner)
    async with app.run_test(size=(100, 30)) as pilot:
        await pilot.pause()
        result = await app.status_tick()
        await pilot.pause()
        assert result is not None
        assert result.outcome == "ok"
        assert app.status_region.row_texts == ("replay status row 1", "replay status row 2")
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_source_closes_idempotently_and_cancels_a_pending_sleep() -> None:
    controls = ReplayControls(speed=MIN_SPEED)
    source = ReplaySource(records(_corpus()), controls=controls)
    iterator = source.__aiter__()
    await iterator.__anext__()

    async def close_soon() -> None:
        await asyncio.sleep(0.01)
        await source.close()
        await source.close()  # idempotent

    closer = asyncio.create_task(close_soon())
    with pytest.raises(StopAsyncIteration):
        # The second frame is behind a scaled sleep; closing cancels it and the
        # iteration ends rather than hanging.
        await asyncio.wait_for(iterator.__anext__(), timeout=5.0)
    await closer
    assert source.closed is True
