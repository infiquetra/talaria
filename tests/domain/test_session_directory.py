"""C13 window two, domain half: the reply and event folds behind the status.

The contract's seam tests, restated: a create reply folds ``info.cwd`` and
yields ``adopted``; a differing reply yields ``not-adopted`` and one
``system`` line; a later ``session.info`` with a different ``cwd`` yields
``moved`` and one line; a resume reply yields ``reported`` with no request
and no line; a reply without ``info.cwd`` leaves ``unreported``; replaying
the same frames twice gives identical state.

The reply folds below call the same :func:`fold_reported_cwd` the app calls
with a landed reply's ``info.cwd`` — the reply itself is transport, the fold
is domain, and testing the fold tests what the reply path decides.
"""

from __future__ import annotations

from talaria.domain.state import (
    SessionState,
    focus_session,
    fold_reported_cwd,
    record_requested_cwd,
)
from tests.domain.conftest import raw_event, replay

LAUNCH = "/talaria"
ELSEWHERE = "/var/gateway"


def _created(requested: str = LAUNCH, reported: str = LAUNCH) -> SessionState:
    """A session whose create reply has landed and folded."""
    return fold_reported_cwd(record_requested_cwd(SessionState(), requested), reported)


def _system_lines(state: SessionState) -> list[str]:
    return [entry.text for entry in state.transcript if entry.kind == "system"]


def test_a_matching_create_reply_yields_adopted_silently() -> None:
    state = _created()
    assert state.requested_cwd == LAUNCH
    assert state.agent_cwd == LAUNCH
    assert state.directory == "adopted"
    assert _system_lines(state) == []


def test_a_differing_create_reply_yields_not_adopted_with_one_line() -> None:
    state = _created(reported=ELSEWHERE)
    assert state.directory == "not-adopted"
    assert _system_lines(state) == [
        f"agent directory: {ELSEWHERE} (launch directory not adopted)"
    ]


def test_a_later_session_info_with_a_different_cwd_yields_moved_with_one_line() -> None:
    state = replay(
        [
            raw_event("message.start", session_id="sess-1"),
            raw_event("session.info", {"cwd": ELSEWHERE}, session_id="sess-1"),
        ],
        state=_created(),
    )
    assert state.agent_cwd == ELSEWHERE
    assert state.directory == "moved"
    assert _system_lines(state) == [f"agent directory changed to {ELSEWHERE}"]


def test_a_session_info_echoing_the_adopted_directory_stays_silent() -> None:
    state = replay(
        [raw_event("session.info", {"cwd": LAUNCH}, session_id="sess-1")],
        state=replay(
            [raw_event("message.start", session_id="sess-1")], state=_created()
        ),
    )
    assert state.directory == "adopted"
    assert _system_lines(state) == []


def test_a_resume_reply_yields_reported_with_no_request_and_no_line() -> None:
    state = fold_reported_cwd(SessionState(), ELSEWHERE)
    assert state.requested_cwd is None
    assert state.agent_cwd == ELSEWHERE
    assert state.directory == "reported"
    assert _system_lines(state) == []


def test_a_reply_without_info_cwd_leaves_unreported() -> None:
    assert fold_reported_cwd(SessionState(), None) == SessionState()
    assert fold_reported_cwd(SessionState(), "") == SessionState()
    assert fold_reported_cwd(SessionState(), 42) == SessionState()  # type: ignore[arg-type]


def test_a_repeat_report_adds_no_second_line() -> None:
    once = _created(reported=ELSEWHERE)
    twice = fold_reported_cwd(once, ELSEWHERE)
    assert _system_lines(twice) == _system_lines(once)
    assert twice == once


def test_a_move_back_to_the_launch_directory_reads_adopted() -> None:
    moved = replay(
        [
            raw_event("message.start", session_id="sess-1"),
            raw_event("session.info", {"cwd": ELSEWHERE}, session_id="sess-1"),
            raw_event("session.info", {"cwd": LAUNCH}, session_id="sess-1"),
        ],
        state=_created(),
    )
    assert moved.directory == "adopted"
    # The move was spoken when it happened; the return adds nothing.
    assert _system_lines(moved) == [f"agent directory changed to {ELSEWHERE}"]


def test_a_first_report_that_differs_stays_not_adopted() -> None:
    """The stored first report is what makes this honest.

    An earlier shape of the fold reconstructed "first" from the latest
    report; the agent wandering into the requested directory then flipped
    ``not-adopted`` to ``adopted`` — the gateway never adopted the request,
    and the interface must not claim it did.
    """
    state = fold_reported_cwd(fold_reported_cwd(_created(reported=ELSEWHERE), LAUNCH), LAUNCH)
    assert state.first_reported_cwd == ELSEWHERE
    assert state.agent_cwd == LAUNCH
    assert state.directory == "not-adopted"
    assert _system_lines(state) == [
        f"agent directory: {ELSEWHERE} (launch directory not adopted)"
    ]


def test_replaying_the_same_frames_twice_gives_identical_state() -> None:
    frames = [
        raw_event("message.start", session_id="sess-1"),
        raw_event("session.info", {"cwd": ELSEWHERE}, session_id="sess-1"),
        raw_event("session.info", {"cwd": ELSEWHERE}, session_id="sess-1"),
    ]
    first = replay(frames, state=_created())
    second = replay(frames, state=_created())
    assert first == second
    assert first.directory == "moved"
    assert len(_system_lines(first)) == 1


def test_switching_sessions_clears_the_directory_claims() -> None:
    state = focus_session(_created(), "sess-2")
    assert state.requested_cwd is None
    assert state.agent_cwd is None
    assert state.first_reported_cwd is None
    assert state.directory == "unreported"


def test_session_info_without_cwd_keeps_what_is_there() -> None:
    state = replay(
        [
            raw_event("message.start", session_id="sess-1"),
            raw_event("session.info", {"title": "renamed"}, session_id="sess-1"),
        ],
        state=_created(),
    )
    assert state.agent_cwd == LAUNCH
    assert state.directory == "adopted"
    assert state.session_title == "renamed"
