"""C13 window two, interface half: the two CONTEXT rows in every status.

Built through the same seam the app uses — :func:`inspector_view` with the
launch, agent, and status the state folds, rendered by :func:`_context_lines`
— so these tests pin what the operator reads, not a reimplementation of it.
"""

from __future__ import annotations

import pytest

from talaria.domain.changes import inspector_view
from talaria.domain.projection import entry_scoped_view
from talaria.domain.state import SessionState
from talaria.domain.workdir import DirectoryStatus
from talaria.ui.inspector import _context_lines

LAUNCH = "/talaria"
ELSEWHERE = "/var/gateway"


def _lines(launch: str, agent: str, status: DirectoryStatus) -> tuple[str, ...]:
    state = SessionState()
    view = inspector_view(
        entry_scoped_view(state), launch=launch, agent=agent, status=status
    )
    assert view.context.launch == launch
    assert view.context.agent == agent
    assert view.context.status == status
    return _context_lines(view, "composer")


def test_unreported_names_the_absence() -> None:
    lines = _lines(LAUNCH, "", "unreported")
    assert "  launch   /talaria" in lines
    assert "  agent    not reported" in lines


def test_reported_marks_the_session_s_own() -> None:
    lines = _lines(LAUNCH, ELSEWHERE, "reported")
    assert "  launch   /talaria" in lines
    assert "  agent    /var/gateway (session's own)" in lines


def test_adopted_is_bare() -> None:
    lines = _lines(LAUNCH, LAUNCH, "adopted")
    assert "  launch   /talaria" in lines
    assert "  agent    /talaria" in lines
    assert not any("(session's own)" in line for line in lines)
    assert not any("not adopted" in line for line in lines)
    assert not any("moved by the agent" in line for line in lines)


def test_not_adopted_says_so_where_the_operator_reads() -> None:
    lines = _lines(LAUNCH, ELSEWHERE, "not-adopted")
    assert "  agent    /var/gateway (launch directory not adopted)" in lines


def test_moved_says_so_where_the_operator_reads() -> None:
    lines = _lines(LAUNCH, ELSEWHERE, "moved")
    assert "  agent    /var/gateway (moved by the agent)" in lines


def test_a_view_from_before_any_session_shows_neither_row() -> None:
    state = SessionState()
    lines = _context_lines(inspector_view(entry_scoped_view(state)), "composer")
    assert not any("launch" in line for line in lines)
    assert not any("agent" in line for line in lines)


@pytest.mark.parametrize(
    ("status", "fragment"),
    [
        ("reported", "(session's own)"),
        ("not-adopted", "(launch directory not adopted)"),
        ("moved", "(moved by the agent)"),
    ],
)
def test_each_mismatch_annotation_is_its_own_string(
    status: DirectoryStatus, fragment: str
) -> None:
    """No annotation may read as another's substring in the rendered row."""
    lines = _lines(LAUNCH, ELSEWHERE, status)
    agent_lines = [line for line in lines if "agent " in line and ELSEWHERE in line]
    assert len(agent_lines) == 1
    assert fragment in agent_lines[0]


def _wide_lines(launch: str, agent: str, status: DirectoryStatus, width: int) -> tuple[str, ...]:
    state = SessionState()
    view = inspector_view(
        entry_scoped_view(state), launch=launch, agent=agent, status=status
    )
    return _context_lines(view, "composer", width=width)


def test_long_paths_clip_to_one_screen_line_each() -> None:
    """A 110-character scratch path must not wrap over the diagnostics below:
    each directory row renders on exactly one line at panel width."""
    long_launch = "/private/var/folders/ky/pytest-of-jefcox/pytest-1/test_launch0"
    lines = _wide_lines(long_launch, "", "unreported", 32)
    launch_lines = [line for line in lines if line.strip().startswith("launch")]
    assert len(launch_lines) == 1
    assert launch_lines[0].endswith("…")
    assert "not reported" in lines[-1]


def test_clipping_yields_budget_to_the_mismatch_annotation() -> None:
    """Clipping the annotation away would read as an adoption — the original
    complaint reproduced. The path shortens first; the note stays whole."""
    long_agent = "/var/gateway/sessions/persisted-conversation-directory"
    lines = _wide_lines(LAUNCH, long_agent, "not-adopted", 48)
    agent_lines = [line for line in lines if "agent " in line]
    assert len(agent_lines) == 1
    assert agent_lines[0].endswith("(launch directory not adopted)")
    assert "…" in agent_lines[0]


def test_short_values_pass_through_untouched() -> None:
    lines = _wide_lines(LAUNCH, LAUNCH, "adopted", 48)
    assert "  launch   /talaria" in lines
    assert "  agent    /talaria" in lines
