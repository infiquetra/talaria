"""R2 / AE12 / PC5 — KTD7's startup precedence, every combination.

AE12 names three inputs (an explicit session target, a stored session choice, a
request for a new session) and asks that "the documented precedence selects
exactly one". Three inputs give eight combinations, and the table below is all
eight rather than the three happy paths — the interesting cell is the one where
both flags are given, which is a usage error rather than a precedence win.
"""

from __future__ import annotations

import pytest

from talaria.cli import parse_args, selection_from_args
from talaria.domain.startup import (
    StartupConflictError,
    StartupSelection,
    resolve_startup,
)


@pytest.mark.parametrize(
    ("session", "resume", "expected"),
    [
        (None, False, StartupSelection(mode="new")),
        (None, True, StartupSelection(mode="resume")),
        ("abc123", False, StartupSelection(mode="session", session_id="abc123")),
        ("", False, StartupSelection(mode="new")),
        ("   ", True, StartupSelection(mode="resume")),
    ],
)
def test_every_non_conflicting_combination_selects_exactly_one_path(
    session: str | None, resume: bool, expected: StartupSelection
) -> None:
    assert resolve_startup(session=session, resume=resume) == expected


def test_the_conflicting_pair_is_a_usage_error_not_a_precedence_win() -> None:
    """Picking one for the operator would hide the mistake until they noticed
    they were in the wrong conversation."""
    with pytest.raises(StartupConflictError):
        resolve_startup(session="abc123", resume=True)


def test_an_explicit_session_id_is_trimmed_before_it_is_used() -> None:
    assert resolve_startup(session="  abc123  ") == StartupSelection(
        mode="session", session_id="abc123"
    )


def test_the_selection_names_one_mode_and_offers_no_switcher() -> None:
    """R2's "no session switcher in v0.1" is a property of the return type:
    there is nothing on :class:`StartupSelection` that changes the mode."""
    selection = resolve_startup(resume=True)
    assert selection.mode == "resume"
    with pytest.raises((AttributeError, TypeError)):
        selection.mode = "new"  # type: ignore[misc]


def test_the_command_line_resolves_through_the_same_pure_function() -> None:
    assert selection_from_args(parse_args(["--session", "abc123"])) == StartupSelection(
        mode="session", session_id="abc123"
    )
    assert selection_from_args(parse_args(["--resume"])) == StartupSelection(mode="resume")
    assert selection_from_args(parse_args([])) == StartupSelection(mode="new")


def test_the_command_line_rejects_the_conflicting_pair_before_dialing() -> None:
    """``parse_args`` errors on the pair, so nothing downstream ever sees a
    namespace carrying both — the connection is never dialed."""
    with pytest.raises(SystemExit):
        parse_args(["--session", "abc123", "--resume"])
