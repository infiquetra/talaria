"""R14, R16, R17 and AE14 — the delegation view's domain half.

AE14 is stated with one example ("a sub-agent reaches a terminal state and a late
progress event follows"). Hermes's guard covers five terminal statuses, so the
test below covers five: a guard written for one member and tested with one member
is a guard that happens to work.

R17 — "Talaria reads sub-agent state and never authors it" — is tested against the
compatibility baseline rather than against behaviour, because the way that rule
gets broken is by adding a method to the client's vocabulary, not by writing bad
state-transition code.
"""

from __future__ import annotations

from typing import Any

import pytest

from talaria.domain.compat import (
    COMPAT_BASELINE,
    REQUIRED_METHODS,
    SUBAGENT_AUTHORING_METHODS,
)
from talaria.domain.models import SubagentStatus
from talaria.domain.projection import project, subagent_view

from .conftest import BASE_TIME, raw_event, replay


def _spawn(subagent_id: str = "sa-1", **payload: Any) -> dict[str, Any]:
    base: dict[str, Any] = {"subagent_id": subagent_id, "goal": "read the docs"}
    base.update(payload)
    return raw_event("subagent.start", base)


def test_a_delegated_child_is_visible_while_the_parent_still_streams() -> None:
    state = replay(
        [
            raw_event("message.start"),
            raw_event("message.delta", {"text": "delegating…"}),
            _spawn(),
        ]
    )
    assert state.turn == "streaming"
    view = subagent_view(state)
    assert [row.name for row in view.rows] == ["read the docs"]
    assert view.active_count == 1


def test_a_row_carries_exactly_the_five_projected_fields() -> None:
    state = replay(
        [
            _spawn(),
            raw_event(
                "subagent.progress",
                {"subagent_id": "sa-1", "text": "halfway through"},
            ),
        ]
    )
    row = subagent_view(state, now=BASE_TIME + 30).rows[0]
    assert row.id == "sa-1"
    assert row.name == "read the docs"
    assert row.status == "running"
    assert row.elapsed == pytest.approx(30.0)
    assert row.detail == "halfway through"


@pytest.mark.parametrize(
    "terminal", ["completed", "error", "failed", "interrupted", "timeout"]
)
def test_a_terminal_row_survives_every_late_live_event(terminal: SubagentStatus) -> None:
    """AE14, generalized. The guard at ``createGatewayEventHandler.ts:606-612``
    names the clobber it prevents: "a stale ``subagent.start`` /
    ``spawn_requested`` can clobber a terminal state from complete"."""
    state = replay(
        [
            _spawn(),
            raw_event("subagent.complete", {"subagent_id": "sa-1", "status": terminal}),
            raw_event("subagent.progress", {"subagent_id": "sa-1", "text": "late note"}),
            raw_event("subagent.tool", {"subagent_id": "sa-1", "tool_name": "grep"}),
            _spawn(),
            raw_event("subagent.spawn_requested", {"subagent_id": "sa-1"}),
        ]
    )
    row = subagent_view(state).rows[0]
    assert row.status == terminal
    assert row.is_terminal


def test_a_completion_may_still_overwrite_another_terminal_status() -> None:
    """The guard exists to stop a stale ``start``/``spawn_requested`` clobbering
    a completion, not to stop the completion itself — ``subagent.complete`` is
    the authoritative outcome and Hermes assigns it directly
    (``createGatewayEventHandler.ts:1306-1316``)."""
    state = replay(
        [
            _spawn(),
            raw_event("subagent.complete", {"subagent_id": "sa-1", "status": "timeout"}),
            raw_event("subagent.complete", {"subagent_id": "sa-1", "status": "failed"}),
        ]
    )
    assert subagent_view(state).rows[0].status == "failed"


def test_an_unknown_status_falls_back_instead_of_leaking_onto_the_row() -> None:
    state = replay(
        [
            _spawn(),
            raw_event("subagent.complete", {"subagent_id": "sa-1", "status": "vaporized"}),
        ]
    )
    assert subagent_view(state).rows[0].status == "completed"


def test_a_late_event_never_resurrects_a_child_whose_start_was_missed() -> None:
    """``createIfMissing: false`` (``turnController.ts:1021-1027``) — a
    ``subagent.complete``/``tool``/``progress`` arriving after the turn ended
    would otherwise create a fresh row for a child that already finished."""
    state = replay(
        [
            raw_event(
                "subagent.progress", {"subagent_id": "ghost", "text": "I never started"}
            ),
            raw_event("subagent.complete", {"subagent_id": "ghost", "status": "completed"}),
        ]
    )
    assert state.subagents == ()
    assert state.late_events_ignored == 2


def test_spawn_requested_and_start_do_create_rows() -> None:
    state = replay(
        [
            raw_event("subagent.spawn_requested", {"subagent_id": "sa-q", "goal": "queued work"}),
        ]
    )
    assert [(r.id, r.status) for r in subagent_view(state).rows] == [("sa-q", "queued")]


def test_a_partial_payload_never_clears_a_field_it_omits() -> None:
    """The ``??`` chain at ``turnController.ts:1057-1076`` exists because
    streaming sub-agent events carry partial payloads."""
    state = replay(
        [
            _spawn(depth=1, task_index=2, model="deepseek-v4-flash", parent_id="root"),
            raw_event("subagent.progress", {"subagent_id": "sa-1", "text": "still going"}),
        ]
    )
    row = state.subagents[0]
    assert row.depth == 1
    assert row.index == 2
    assert row.model == "deepseek-v4-flash"
    assert row.parent_id == "root"
    assert row.name == "read the docs"


def test_rows_are_ordered_by_spawn_position_not_arrival() -> None:
    """``turnController.ts:1078-1083``: without a stable order, grandchildren
    shuffle relative to siblings when events arrive out of order."""
    state = replay(
        [
            raw_event(
                "subagent.start",
                {"subagent_id": "grandchild", "goal": "deep", "depth": 2, "task_index": 0},
            ),
            raw_event(
                "subagent.start",
                {"subagent_id": "second", "goal": "b", "depth": 1, "task_index": 1},
            ),
            raw_event(
                "subagent.start",
                {"subagent_id": "first", "goal": "a", "depth": 1, "task_index": 0},
            ),
        ]
    )
    assert [row.id for row in state.subagents] == ["first", "second", "grandchild"]


def test_detail_lines_dedupe_and_stay_bounded() -> None:
    frames = [_spawn()]
    frames.append(raw_event("subagent.progress", {"subagent_id": "sa-1", "text": "same"}))
    frames.append(raw_event("subagent.progress", {"subagent_id": "sa-1", "text": "same"}))
    for index in range(12):
        frames.append(
            raw_event("subagent.progress", {"subagent_id": "sa-1", "text": f"note {index}"})
        )
    state = replay(frames)
    assert len(state.subagents[0].detail) == 8
    assert state.subagents[0].detail[-1] == "note 11"


def test_the_collapsed_count_stays_visible() -> None:
    """R16: when detail is collapsed a count remains, so the operator always
    knows whether delegated work exists."""
    state = replay(
        [
            _spawn("sa-1"),
            _spawn("sa-2"),
            raw_event("subagent.complete", {"subagent_id": "sa-2", "status": "completed"}),
        ]
    )
    view = subagent_view(state)
    assert (view.active_count, view.terminal_count) == (1, 1)
    assert view.collapsed_label == "1 active · 1 finished"

    payload = project(state).status
    assert (payload.subagents_active, payload.subagents_terminal) == (1, 1)


def test_a_terminal_rows_elapsed_time_stops_when_it_finished() -> None:
    state = replay(
        [
            _spawn(),
            raw_event("subagent.complete", {"subagent_id": "sa-1", "status": "completed"}),
        ]
    )
    row = subagent_view(state, now=BASE_TIME + 10_000).rows[0]
    assert row.elapsed == pytest.approx(1.0), "frame 1 to frame 2 is one second apart"


def test_a_new_turn_clears_the_previous_turns_rows() -> None:
    state = replay(
        [
            raw_event("message.start"),
            _spawn(),
            raw_event("message.complete", {"text": "done"}),
        ]
    )
    assert len(state.subagents) == 1, "rows outlive the turn so a late event has a target"

    after = replay([raw_event("message.start")], state)
    assert after.subagents == ()


# ── R17: Talaria reads sub-agent state and never authors it ──────────────


def test_the_compat_baseline_contains_no_subagent_authoring_method() -> None:
    """The concrete method this excludes is ``spawn_tree.save``, which Hermes's
    own turn controller calls at the end of every delegating turn
    (``turnController.ts:640-652``) to archive the fan-out to disk. Talaria does
    not build that archive, so the method never enters its vocabulary."""
    assert REQUIRED_METHODS.isdisjoint(SUBAGENT_AUTHORING_METHODS)


def test_every_subagent_touching_baseline_method_is_a_read_or_the_interrupt() -> None:
    reading = {"spawn_tree.list", "agents.list", "delegation.status"}
    control = {"subagent.interrupt"}
    for entry in COMPAT_BASELINE:
        if "subagent" not in entry.method and "spawn_tree" not in entry.method:
            continue
        assert entry.method in reading | control
        if entry.method in reading:
            assert entry.classification == "read-only"
        else:
            assert entry.classification == "evidence-only"


def test_an_empty_subagent_event_changes_nothing() -> None:
    """``createGatewayEventHandler.ts:1240-1244`` and ``:1279-1284`` drop
    empty-text sub-agent events before they reach the upsert, so a heartbeat
    carrying no note cannot append a blank detail line."""
    state = replay(
        [
            _spawn(),
            raw_event("subagent.progress", {"subagent_id": "sa-1", "text": "   "}),
            raw_event("subagent.thinking", {"subagent_id": "sa-1", "text": ""}),
        ]
    )
    assert state.subagents[0].detail == ()
