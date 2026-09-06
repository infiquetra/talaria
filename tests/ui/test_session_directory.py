"""C13 window two, interface half: the two CONTEXT rows in every status.

Built through the same seam the app uses — :func:`inspector_view` with the
launch, agent, and status the state folds, rendered by :func:`_context_lines`
— so these tests pin what the operator reads, not a reimplementation of it.
"""

from __future__ import annotations

import pytest
from rich.cells import cell_len

from talaria.domain.changes import inspector_view
from talaria.domain.projection import entry_scoped_view
from talaria.domain.state import SessionState
from talaria.domain.workdir import DirectoryStatus
from talaria.ui.inspector import _context_lines
from tests.ui.conftest import event, paused_app

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


def test_mismatch_note_rides_on_its_own_row_whole() -> None:
    """The ruled shape: the clipped path keeps every cell the panel allows
    and the annotation rides beneath it, verbatim, never budgeted and never
    clipped. A note rejoined onto the path row — or a reservation shrinking
    the path for it — fails the exact strings below."""
    long_agent = "/var/gateway/sessions/persisted-conversation-directory"
    lines = _wide_lines(LAUNCH, long_agent, "not-adopted", 32)
    agent_lines = [line for line in lines if "agent " in line]
    assert agent_lines == ["  agent    /var/gateway/session…"]
    note_lines = [line for line in lines if "launch directory not adopted" in line]
    assert note_lines == ["  (launch directory not adopted)"]


def test_directory_rows_hold_the_panel_budget() -> None:
    """The binding assertion: every labelled row's total cell width against
    the panel budget, so a long path wraps over nothing below it."""
    long_path = "/var/gateway/sessions/persisted-conversation-directory"
    for width in range(12, 63):
        lines = _wide_lines(long_path, long_path, "adopted", width)
        for line in lines:
            if line.strip().startswith(("launch", "agent")):
                assert cell_len(line) <= width, (width, line)


def test_note_row_is_never_clipped() -> None:
    """The note's survival is structural — it never passes through the
    clipper — so at any panel wide enough to hold it whole, it reads whole."""
    long_agent = "/var/gateway/sessions/persisted-conversation-directory"
    for width in (30, 32, 36, 48):
        lines = _wide_lines(LAUNCH, long_agent, "not-adopted", width)
        assert "  (launch directory not adopted)" in lines


def test_short_values_pass_through_untouched() -> None:
    lines = _wide_lines(LAUNCH, LAUNCH, "adopted", 48)
    assert "  launch   /talaria" in lines
    assert "  agent    /talaria" in lines


@pytest.mark.asyncio
async def test_a_width_change_reclips_the_directory_rows() -> None:
    """The clip follows the panel, not the first paint: widening re-clips
    longer, narrowing re-clips shorter. Without the repaint in
    ``set_terminal_width`` the rows would stick at whatever width they were
    first painted for."""
    app, _ = paused_app([event("gateway.ready", {})])
    async with app.run_test(size=(132, 40)) as pilot:
        await pilot.pause()
        narrow = app.inspector.context_text
        app.inspector.panel_width = 48
        app.inspector.set_terminal_width(132)
        await pilot.pause()
        wide = app.inspector.context_text
        assert wide != narrow
        launch_wide = next(
            line for line in wide.splitlines() if line.strip().startswith("launch")
        )
        launch_narrow = next(
            line for line in narrow.splitlines() if line.strip().startswith("launch")
        )
        assert len(launch_wide) > len(launch_narrow)
        app.inspector.panel_width = 36
        app.inspector.set_terminal_width(132)
        await pilot.pause()
        assert app.inspector.context_text == narrow
        await app.shutdown_sources()
