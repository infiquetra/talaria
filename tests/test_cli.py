"""KTD7 startup-precedence tests for talaria/cli.py.

``tests/domain/test_startup_precedence.py`` (U3, requirement R2) will test
precedence against the resolved domain model. These tests pin the argument
layer that feeds it, so the behavior is protected from the commit that
introduces it rather than from the commit that consumes it.
"""

from __future__ import annotations

import pytest

from talaria.cli import main, parse_args, resolve_startup


def test_explicit_session_beats_resume_and_default() -> None:
    selection = resolve_startup(parse_args(["--session", "abc123"]))
    assert selection.mode == "session"
    assert selection.session_id == "abc123"


def test_resume_beats_default() -> None:
    selection = resolve_startup(parse_args(["--resume"]))
    assert selection.mode == "resume"
    assert selection.session_id is None


def test_no_flags_starts_a_new_session() -> None:
    selection = resolve_startup(parse_args([]))
    assert selection.mode == "new"
    assert selection.session_id is None


def test_conflicting_flags_are_a_usage_error(capsys: pytest.CaptureFixture[str]) -> None:
    """KTD7: the conflicting pair fails before any connection is dialed."""
    with pytest.raises(SystemExit) as excinfo:
        parse_args(["--session", "abc123", "--resume"])

    assert excinfo.value.code == 2
    assert "mutually exclusive" in capsys.readouterr().err


def test_startup_selection_is_immutable() -> None:
    selection = resolve_startup(parse_args(["--session", "abc123"]))
    with pytest.raises(AttributeError):
        selection.session_id = "other"  # type: ignore[misc]


def test_main_exits_zero_on_a_well_formed_invocation() -> None:
    assert main([]) == 0
