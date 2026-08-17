"""Seam probes under replay: recorded replies only, no socket (v0.4 U5).

The plan's second test scenario for this unit — "probes under replay: recorded
replies reproduce; an unobserved seam is never-observed; no dial occurs".

**The no-dial claim is asserted three ways, because the interesting failure is a
reconstruction that quietly reaches for the network to fill a gap.** The
signature carries no dispatcher and no origin, the module imports nothing that
can open a socket, and the reconstruction runs with :func:`socket.socket`
replaced by a function that raises. The third is the one that would actually
catch a regression: the first two are properties of today's code, and only the
patch survives somebody adding a "just fetch it if it's missing" branch.
"""

from __future__ import annotations

import socket
from collections.abc import Iterator
from typing import Any

import pytest

from talaria.domain.compat import (
    RECORDED_METHOD_NOT_FOUND,
    RecordedFrame,
    baseline_for,
    board_lines,
    classify_recorded_reply,
    seam_board_from_recording,
    seam_line,
)
from talaria.transport.compat_check import METHOD_NOT_FOUND

#: A ``session.active_list`` reply body, matching the row shape U1 verified live
#: on 2026-08-17. Only the top-level key is compared, but the row is written out
#: so a reader can see what a real recording carries.
ACTIVE_LIST_RESULT: dict[str, Any] = {
    "sessions": [
        {
            "current": True,
            "id": "s-9f12",
            "last_active": 1785000600.0,
            "message_count": 12,
            "model": "a-model",
            "preview": "let's pick this back up",
            "session_key": "k-9f12",
            "started_at": 1785000000.0,
            "status": "idle",
            "title": "refactor the ingest pipeline",
        }
    ]
}


def request(method: str, rid: str, params: dict[str, Any] | None, at: float) -> RecordedFrame:
    frame: dict[str, Any] = {"jsonrpc": "2.0", "id": rid, "method": method}
    if params is not None:
        frame["params"] = params
    return RecordedFrame(direction="out", at=at, frame=frame)


def reply(rid: str, result: dict[str, Any], at: float) -> RecordedFrame:
    return RecordedFrame(
        direction="in", at=at, frame={"jsonrpc": "2.0", "id": rid, "result": result}
    )


def failure(rid: str, code: int, message: str, at: float) -> RecordedFrame:
    return RecordedFrame(
        direction="in",
        at=at,
        frame={"jsonrpc": "2.0", "id": rid, "error": {"code": code, "message": message}},
    )


@pytest.fixture
def no_sockets() -> Iterator[None]:
    """Make any socket construction inside the block a loud failure.

    Patching the constructor rather than counting connections, because a
    reconstruction that *built* a socket and never connected it would still be
    reaching for the network, and the point of the replay gate is that the
    process holds none.
    """
    original = socket.socket

    def _refuse(*args: Any, **kwargs: Any) -> Any:
        raise AssertionError("replay opened a socket")

    socket.socket = _refuse  # type: ignore[assignment,misc]
    try:
        yield
    finally:
        socket.socket = original  # type: ignore[misc]


# ── recorded replies reproduce ───────────────────────────────────────────


def test_a_recorded_roster_reply_reproduces_a_present_seam(no_sockets: None) -> None:
    frames = (
        request("session.active_list", "1", {}, 10.0),
        reply("1", ACTIVE_LIST_RESULT, 10.5),
    )
    board = seam_board_from_recording(frames, profile="default")
    observation = board.observation_for("roster")

    assert observation.status == "present"
    assert observation.source == "recording session.active_list"
    assert observation.observed_at == 10.5
    assert "fleet roster available" in seam_line(observation, 10.5)


def test_a_recorded_method_not_found_reproduces_the_named_absence(no_sockets: None) -> None:
    """The state U1 measured live, replayed from a file: the roster is there and
    ``approval.pending`` is not, and the line names what that turns off."""
    frames = (
        request("session.active_list", "1", {}, 1.0),
        reply("1", ACTIVE_LIST_RESULT, 1.2),
        request("approval.pending", "2", {}, 1.3),
        failure("2", RECORDED_METHOD_NOT_FOUND, "unknown method: approval.pending", 1.4),
    )
    board = seam_board_from_recording(frames)

    assert board.observation_for("roster").status == "present"
    assert board.observation_for("approval-detail").status == "absent"
    line = seam_line(board.observation_for("approval-detail"), 1.4)
    assert "approval detail unavailable: approval.pending absent" in line


def test_a_recorded_refusal_of_a_presence_probe_reproduces_presence(no_sockets: None) -> None:
    """KTD11 in replay: 4001 is the proof, exactly as it is on the wire."""
    frames = (
        request("approval.pending", "1", {}, 3.0),
        failure("1", 4001, "session not found", 3.1),
    )
    board = seam_board_from_recording(frames)
    observation = board.observation_for("approval-detail")
    assert observation.status == "present"
    assert "proved by refusal" in observation.detail


def test_a_recorded_parameterized_method_not_found_is_not_read_as_absence(
    no_sockets: None,
) -> None:
    """R11 in replay. The recording holds one parameterized call and no bare
    re-ask, so absence was never established and must not be claimed here.

    This is the branch a straightforward implementation gets wrong: ``-32601``
    looks like a settled answer, and reading it as one would put the closed
    ``absent_capability`` misdiagnosis back into a code path nobody inspects.
    """
    frames = (
        request("session.list", "1", {"limit": 200}, 5.0),
        failure("1", RECORDED_METHOD_NOT_FOUND, "unknown method: session.list", 5.1),
    )
    status, detail = classify_recorded_reply(
        baseline_for("session.list"),
        frames[1].frame,
        parameterized=True,
    )
    # ``parameter-invalid``, never ``absent`` — absence was not established by
    # anything in this corpus.
    assert status == "parameter-invalid"
    assert "parameterized" in detail


def test_a_recorded_drifted_reply_reproduces_an_incompatible_seam(no_sockets: None) -> None:
    frames = (
        request("session.active_list", "1", {}, 2.0),
        reply("1", {"rows": []}, 2.5),
    )
    board = seam_board_from_recording(frames)
    observation = board.observation_for("roster")
    assert observation.status == "incompatible"
    assert "sessions" in observation.detail


def test_a_recorded_parameterized_seam_call_is_not_read_as_absence(
    no_sockets: None,
) -> None:
    """R11 in replay, through the reconstruction rather than around it.

    The test above hands ``classify_recorded_reply`` a ``parameterized=True`` it
    was told, so it pins the grading rule and nothing about the code that decides
    which requests carried parameters. That decision is
    :func:`~talaria.domain.compat._probe_exchanges` reading ``params`` off the
    recorded request, and until 2026-08-17 no test fed it a parameterized seam
    call at all — every recorded request in this file passed ``{}``, so the whole
    detection could have been ``parameterized=False`` and the suite would not have
    noticed.

    So: a recorded roster probe that carried parameters, refused ``-32601``, with
    no bare re-ask anywhere in the corpus. Absence was never established, and the
    seam must not claim it.
    """
    frames = (
        request("session.active_list", "1", {"profile": "mistyped"}, 4.0),
        failure(
            "1", RECORDED_METHOD_NOT_FOUND, "unknown method: session.active_list", 4.1
        ),
    )
    board = seam_board_from_recording(frames)
    observation = board.observation_for("roster")
    assert observation.status == "parameter-invalid"
    assert "absent" not in seam_line(observation, 5.0)


def test_the_last_recorded_reply_wins(no_sockets: None) -> None:
    """A corpus that recorded a reconnect tells the story as it ended."""
    frames = (
        request("session.active_list", "1", {}, 1.0),
        failure("1", RECORDED_METHOD_NOT_FOUND, "unknown method", 1.1),
        request("session.active_list", "2", {}, 60.0),
        reply("2", ACTIVE_LIST_RESULT, 60.1),
    )
    board = seam_board_from_recording(frames)
    assert board.observation_for("roster").status == "present"
    assert board.observation_for("roster").observed_at == 60.1


def event(rid: str, kind: str, at: float, **body: Any) -> RecordedFrame:
    """A gateway event that happens to carry a top-level ``id``.

    Not hypothetical: ``{"type": "tool.complete", "id": "1", "result": {…}}`` is
    the recorded shape that once resolved an in-flight ``session.interrupt`` as a
    success, and the redactor writes ``type`` and ``id`` through to the log
    untouched, so a corpus really does hold frames like this.
    """
    frame: dict[str, Any] = {"jsonrpc": "2.0", "id": rid, "type": kind}
    frame.update(body)
    return RecordedFrame(direction="in", at=at, frame=frame)


def test_an_event_carrying_an_id_is_not_graded_as_a_probe_reply(
    no_sockets: None,
) -> None:
    """An id is not an envelope, and the gateway reuses the small ones.

    Talaria's request ids are decimal strings counting from one, restarted at
    every reconnect, and the gateway's own events carry ids in the same space. So
    an event can land between a probe and its answer and match on id alone.

    Both harms are asserted because they are separate: the event must not be
    graded against the roster's pinned signature, and the genuine reply must not
    arrive to find its pairing already consumed. Reading the id alone produced
    ``incompatible`` here, with the event's ``ok`` key reported as roster drift.
    """
    frames = (
        request("session.active_list", "1", {}, 10.0),
        event("1", "tool.complete", 10.1, result={"ok": True}),
        reply("1", ACTIVE_LIST_RESULT, 10.2),
    )
    board = seam_board_from_recording(frames)
    observation = board.observation_for("roster")
    assert observation.status == "present"
    assert observation.observed_at == 10.2
    assert "ok" not in observation.detail


def test_an_event_with_neither_result_nor_error_demotes_nothing(
    no_sockets: None,
) -> None:
    """The other half of the same defect, which fails differently.

    A ``turn.delta`` carries no ``result`` and no ``error``, so grading it
    returns no status at all — and a seam that had just been confirmed present
    was demoted to ``degraded`` by an observation that settled nothing. The
    envelope check makes the frame invisible to the reconstruction instead, which
    is what it is.
    """
    frames = (
        request("session.active_list", "1", {}, 1.0),
        reply("1", ACTIVE_LIST_RESULT, 1.1),
        request("session.active_list", "2", {}, 2.0),
        event("2", "turn.delta", 2.1, seq=4),
    )
    board = seam_board_from_recording(frames)
    observation = board.observation_for("roster")
    assert observation.status == "present"
    assert "degraded" not in seam_line(observation, 3.0)


def test_a_reply_is_graded_against_the_request_it_answers_not_a_later_one(
    no_sockets: None,
) -> None:
    """A JSON-RPC id is reused across a reconnect, and must not join two epochs.

    The correlator restarts its id counter at every reconnect on purpose, and a
    frame-log record carries no epoch, so a recording that spans one holds the
    same id twice for two different calls. The first version of this
    reconstruction built one id-keyed map over the whole file with last-write-wins
    and *then* graded replies against it, which let a reply be attributed to a
    request sent long after it.

    Staged here at its most legible: epoch one asks ``approval.pending`` bare on
    id ``6`` and is told the method is unknown; the reconnect reuses id ``6`` for
    the roster probe, whose reply the log never reaches. Two seams, two chances to
    be wrong. The roster must not be declared absent on a refusal that names a
    different method, and the approval seam — which this corpus really does prove
    absent — must not be reported never-observed because its answer was stolen.
    """
    frames = (
        request("approval.pending", "6", {}, 10.0),
        failure(
            "6", RECORDED_METHOD_NOT_FOUND, "unknown method: approval.pending", 10.1
        ),
        # reconnect: the id counter restarts and 6 is handed to the roster probe
        request("session.active_list", "6", {}, 400.0),
    )
    board = seam_board_from_recording(frames)

    roster = board.observation_for("roster")
    assert roster.status is None, "an unanswered probe proves nothing"
    approval = board.observation_for("approval-detail")
    assert approval.status == "absent"
    assert "approval.pending" in approval.detail
    # The mis-attribution was legible in the rendered sentence, so assert on the
    # sentence too: the roster line must not carry another method's refusal.
    assert "approval.pending" not in seam_line(roster, 500.0)


# ── an unobserved seam is never-observed ─────────────────────────────────


def test_a_seam_the_recording_never_observed_is_never_observed(no_sockets: None) -> None:
    """A frame log records WebSocket frames, so the HTTP runner seam is never in
    one — and the honest rendering of that is "not observed", never "absent".

    The kanban seam is checked in the same observation because it is
    never-observed for a *different* reason (no route exists at all), and the
    rendering must not depend on which reason applies.
    """
    frames = (
        request("session.active_list", "1", {}, 1.0),
        reply("1", ACTIVE_LIST_RESULT, 1.1),
    )
    board = seam_board_from_recording(frames)

    for name in ("http-runner", "kanban-dispatcher"):
        observation = board.observation_for(name)
        assert observation.status is None, name
        assert observation.observation(1.1) == "never-observed", name
        line = seam_line(observation, 1.1)
        assert "not observed" in line, name
        assert "absent" not in line, name


def test_an_empty_recording_leaves_every_seam_never_observed(no_sockets: None) -> None:
    board = seam_board_from_recording(())
    assert all(observation.status is None for observation in board.observations)
    assert all("not observed" in line for line in board_lines(board, 0.0))


def test_frames_unrelated_to_probes_change_nothing(no_sockets: None) -> None:
    """A corpus is mostly events and mutating calls. Only probeable methods
    contribute, and an id that was never a probe request is ignored."""
    frames = (
        RecordedFrame(
            direction="out",
            at=1.0,
            frame={"jsonrpc": "2.0", "id": "9", "method": "prompt.submit", "params": {}},
        ),
        reply("9", {"status": "queued"}, 1.1),
        RecordedFrame(
            direction="in",
            at=1.2,
            frame={"jsonrpc": "2.0", "method": "event", "params": {"type": "gateway.ready"}},
        ),
    )
    board = seam_board_from_recording(frames)
    assert all(observation.status is None for observation in board.observations)


# ── determinism and the no-dial claim ────────────────────────────────────


def test_two_replays_of_one_corpus_render_identical_lines(no_sockets: None) -> None:
    """AE8, for this surface. Ages ride the recorded stamps, so the rendered
    lines are byte-identical across runs — no wall clock is read anywhere."""
    frames = (
        request("session.active_list", "1", {}, 99.0),
        reply("1", ACTIVE_LIST_RESULT, 100.0),
        request("approval.pending", "2", {}, 110.0),
        failure("2", RECORDED_METHOD_NOT_FOUND, "unknown method: approval.pending", 111.0),
    )
    first = board_lines(seam_board_from_recording(frames), 200.0)
    second = board_lines(seam_board_from_recording(frames), 200.0)

    assert first == second
    roster = next(line for line in first if line.startswith("roster:"))
    approval = next(line for line in first if line.startswith("approval-detail:"))
    assert "100s ago" in roster, "the age must come from the recorded reply's stamp"
    assert "89s ago" in approval, "and each seam's age from its own reply"


def test_the_reconstruction_takes_no_dispatcher_and_no_origin() -> None:
    """Structural: there is nothing in the signature a socket could arrive
    through, which is what makes "replay opens no socket" a property of the type
    rather than a promise about behavior."""
    import inspect

    parameters = inspect.signature(seam_board_from_recording).parameters
    assert set(parameters) == {"frames", "profile"}
    assert parameters["profile"].annotation == "str"


def test_the_recorded_absence_code_matches_the_live_one() -> None:
    """The domain restates ``-32601`` rather than importing it from the
    transport. This is the assertion that stops the two from drifting apart in
    silence — a replay that graded a different code would classify a recording
    differently from the wire it came from."""
    assert RECORDED_METHOD_NOT_FOUND == METHOD_NOT_FOUND
