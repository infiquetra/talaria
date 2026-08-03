"""KTD7 startup-precedence tests for talaria/cli.py.

``tests/domain/test_startup_precedence.py`` (U3, requirement R2) will test
precedence against the resolved domain model. These tests pin the argument
layer that feeds it, so the behavior is protected from the commit that
introduces it rather than from the commit that consumes it.
"""

from __future__ import annotations

import pytest

import talaria.cli as cli_module
from talaria.cli import main, parse_args, selection_from_args


def test_explicit_session_beats_resume_and_default() -> None:
    selection = selection_from_args(parse_args(["--session", "abc123"]))
    assert selection.mode == "session"
    assert selection.session_id == "abc123"


def test_resume_beats_default() -> None:
    selection = selection_from_args(parse_args(["--resume"]))
    assert selection.mode == "resume"
    assert selection.session_id is None


def test_no_flags_starts_a_new_session() -> None:
    selection = selection_from_args(parse_args([]))
    assert selection.mode == "new"
    assert selection.session_id is None


def test_conflicting_flags_are_a_usage_error(capsys: pytest.CaptureFixture[str]) -> None:
    """KTD7: the conflicting pair fails before any connection is dialed."""
    with pytest.raises(SystemExit) as excinfo:
        parse_args(["--session", "abc123", "--resume"])

    assert excinfo.value.code == 2
    assert "mutually exclusive" in capsys.readouterr().err


def test_startup_selection_is_immutable() -> None:
    selection = selection_from_args(parse_args(["--session", "abc123"]))
    with pytest.raises(AttributeError):
        selection.session_id = "other"  # type: ignore[misc]


def test_main_exits_zero_on_a_well_formed_invocation() -> None:
    assert main([]) == 0


def test_record_subcommand_parses_url_and_default_out() -> None:
    args = parse_args(["record", "ws://127.0.0.1:9119/api/ws?token=abc"])
    assert args.command == "record"
    assert args.url == "ws://127.0.0.1:9119/api/ws?token=abc"
    assert args.out is None


def test_record_subcommand_parses_explicit_out_path() -> None:
    args = parse_args(["record", "ws://127.0.0.1:9119/api/ws", "--out", "/tmp/x.jsonl"])
    assert args.out == "/tmp/x.jsonl"


def test_record_subcommand_requires_a_url() -> None:
    with pytest.raises(SystemExit) as excinfo:
        parse_args(["record"])
    assert excinfo.value.code == 2


def test_main_dispatches_record_subcommand_to_run_record(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """U2 wiring: `main()` routes a `record` invocation to
    `talaria.recorder.command.run_record` rather than the default startup
    path -- proved with a fake `run_record` so this test never opens a
    socket."""
    calls: list[dict[str, object]] = []

    async def fake_run_record(url: str, **kwargs: object) -> int:
        calls.append({"url": url, **kwargs})
        return 0

    import talaria.recorder.command as command_module

    monkeypatch.setattr(command_module, "run_record", fake_run_record)

    exit_code = cli_module.main(["record", "ws://127.0.0.1:9119/api/ws?token=abc"])

    assert exit_code == 0
    assert len(calls) == 1
    assert calls[0]["url"] == "ws://127.0.0.1:9119/api/ws?token=abc"
