"""KTD7 startup-precedence tests for talaria/cli.py.

``tests/domain/test_startup_precedence.py`` (U3, requirement R2) will test
precedence against the resolved domain model. These tests pin the argument
layer that feeds it, so the behavior is protected from the commit that
introduces it rather than from the commit that consumes it.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import talaria.cli as cli_module
from talaria import config as config_module
from talaria.cli import main, parse_args, selection_from_args
from talaria.transport.credentials import LoopbackTokenProvider


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


def test_main_routes_a_bare_invocation_to_the_live_launcher(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A bare ``talaria`` launches the live shell (U10), proved with a double.

    ``run_live`` is replaced rather than called. Calling it would dial whatever
    :data:`~talaria.transport.credentials.DEFAULT_GATEWAY_URL` resolves to, and
    on any machine that is running Hermes — which is every machine this client
    is developed on — that is a real gateway and a real session. No test in this
    repository may attach to one.
    """
    calls: list[str] = []

    def fake_run_live(args: object) -> int:
        calls.append(getattr(args, "command", None) or "default")
        return 0

    monkeypatch.setattr(cli_module, "run_live", fake_run_live)

    assert main([]) == 0
    assert calls == ["default"]


def test_a_conflicting_pair_never_reaches_the_launcher(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """KTD7's usage error fires before anything is dialled."""
    launched: list[object] = []

    def fake_run_live(args: object) -> int:
        launched.append(args)
        return 0

    monkeypatch.setattr(cli_module, "run_live", fake_run_live)

    with pytest.raises(SystemExit) as excinfo:
        main(["--session", "abc123", "--resume"])

    assert excinfo.value.code == 2
    assert launched == []


# ── U10: the live launcher is assembled, without dialling anything ───────


def test_the_live_launcher_builds_a_live_app_over_a_live_source() -> None:
    """``build_live_app`` is the whole wiring, and it opens no socket.

    :class:`~talaria.transport.source.LiveSource` dials from ``start()``, never
    from its constructor, which is what makes the assembly assertable here
    rather than only against a stub gateway.
    """
    from talaria.transport.source import LiveSource
    from talaria.ui.app import TalariaApp

    cfg = config_module.load_config()
    app, source = cli_module.build_live_app(parse_args([]), cfg)

    assert isinstance(app, TalariaApp)
    assert isinstance(source, LiveSource)
    assert app.mode == "live"
    assert app.dispatcher is source
    assert app.source is source
    assert not source.closed
    assert source.state == "disconnected", "building the launcher dialled a gateway"


def test_the_launcher_carries_the_startup_selection_it_was_given() -> None:
    """KTD7's precedence reaches the app that will act on it."""
    cfg = config_module.load_config()

    explicit, _ = cli_module.build_live_app(parse_args(["--session", "abc123"]), cfg)
    resumed, _ = cli_module.build_live_app(parse_args(["--resume"]), cfg)
    fresh, _ = cli_module.build_live_app(parse_args([]), cfg)

    assert explicit.startup is not None and explicit.startup.mode == "session"
    assert explicit.startup.session_id == "abc123"
    assert resumed.startup is not None and resumed.startup.mode == "resume"
    assert fresh.startup is not None and fresh.startup.mode == "new"


def test_the_configured_paste_thresholds_reach_the_live_app(
    isolated_global_config_dir: object, monkeypatch: pytest.MonkeyPatch
) -> None:
    """KTD16's bounds are configurable, and live is the only mode that uses them.

    Until the live launcher existed, ``_build_paste_threshold`` was passed only
    from ``run_replay`` — and replay never collapses a paste, because collapsing
    one takes a gateway. So the setting was reachable in the mode that ignores
    it and unreachable in the mode that reads it, which is the gap
    ``docs/engineering-journal/QUEUED.md`` recorded and this closes.
    """
    monkeypatch.setenv("TALARIA_COMPOSER_PASTE_COLLAPSE_LINES", "9")
    monkeypatch.setenv("TALARIA_COMPOSER_PASTE_COLLAPSE_BYTES", "4096")

    app, _ = cli_module.build_live_app(parse_args([]), config_module.load_config())

    assert app.paste_threshold.lines == 9
    assert app.paste_threshold.byte_limit == 4096
    # Paired with the default, so this asserts the configuration was *read*
    # rather than that the app has a threshold at all.
    default, _ = cli_module.build_live_app(parse_args([]), config_module.load_config())
    monkeypatch.delenv("TALARIA_COMPOSER_PASTE_COLLAPSE_LINES")
    monkeypatch.delenv("TALARIA_COMPOSER_PASTE_COLLAPSE_BYTES")
    plain, _ = cli_module.build_live_app(parse_args([]), config_module.load_config())
    assert plain.paste_threshold.lines != 9
    assert default.paste_threshold.lines == 9


def test_a_replay_launch_still_gets_the_same_configured_thresholds(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Pairs the test above: adding the live path must not divert the replay one."""
    monkeypatch.setenv("TALARIA_COMPOSER_PASTE_COLLAPSE_LINES", "9")
    threshold = cli_module._build_paste_threshold(config_module.load_config())
    assert threshold.lines == 9


def test_the_configured_status_command_reaches_the_live_app(
    isolated_global_config_dir: object, monkeypatch: pytest.MonkeyPatch
) -> None:
    """U6's runner is wired into the bare-``talaria`` launch, not only into replay.

    The sibling arguments (``paste_threshold``, ``startup``) were each pinned
    when the live launcher was written and this one was not: deleting
    ``status_runner=`` from ``build_live_app`` left the whole suite green, so a
    client that silently never ran the operator's configured status command
    would have shipped. Both halves are asserted in one observation — the
    configured argv arrives, and an unconfigured launch gets no runner at all —
    because "the app has a runner" alone is satisfied by a default.
    """
    monkeypatch.setenv("TALARIA_STATUS_COMMAND", "git status --short")
    monkeypatch.setenv("TALARIA_STATUS_INTERVAL_SECONDS", "11")

    app, _ = cli_module.build_live_app(parse_args([]), config_module.load_config())

    assert app.status_runner is not None, "the live launcher built no status runner"
    assert app.status_runner._argv == ("git", "status", "--short")
    assert app.status_interval == 11.0

    monkeypatch.delenv("TALARIA_STATUS_COMMAND")
    monkeypatch.delenv("TALARIA_STATUS_INTERVAL_SECONDS")
    unconfigured, _ = cli_module.build_live_app(parse_args([]), config_module.load_config())
    assert unconfigured.status_runner is None, (
        "an unconfigured launch spawned a status runner anyway, so the assertion "
        "above says nothing about configuration being read"
    )


def test_a_status_command_written_as_a_toml_array_does_not_stop_the_launch(
    isolated_global_config_dir: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A malformed ``status.command`` turns off the status line, not the client.

    ``command = ["sh", "-c", "date"]`` is the obvious operator guess for an argv
    array and it is a TOML array, so ``cfg.get("status", "command")`` hands
    ``parse_command`` a list. That used to raise ``AttributeError`` out of
    ``shlex`` — and once the live launcher put ``_build_status_runner`` on the
    bare-``talaria`` path, it became a full traceback and exit 1 with no
    interface. The client must still come up.
    """
    (isolated_global_config_dir / "config.toml").write_text(
        '[status]\ncommand = ["sh", "-c", "date"]\n', encoding="utf-8"
    )

    app, _ = cli_module.build_live_app(parse_args([]), config_module.load_config())

    assert app.status_runner is None
    # The positive half: the same config file is otherwise being read, so this
    # is not passing because nothing was loaded.
    assert config_module.load_config().get("status", "command") == ("sh", "-c", "date")


def test_the_live_launcher_can_record_the_session_it_drives(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    """``--record`` closes the gap between "a usable client" and "a recording".

    R3 asks for one live turn compared against a replay of the same frames, so
    the run that would satisfy it has to be recorded *while somebody is using
    it*. ``LiveSource`` has always taken a recorder and the launcher never
    passed one, and ``talaria record`` draws no interface — so the document that
    names R3 as the thing that would move the verdict described a step the
    shipped client could not perform.

    Both directions are asserted in one observation: a launch without the flag
    must record nothing, or "the source has a recorder" would be satisfied by a
    client that always records.
    """
    plain, plain_source = cli_module.build_live_app(
        parse_args([]), config_module.load_config()
    )
    assert plain_source._recorder is None, "an unasked-for launch opened a frame log"

    out = tmp_path / "session.jsonl"
    _app, source = cli_module.build_live_app(
        parse_args(["--record", str(out)]), config_module.load_config()
    )
    try:
        assert source._recorder is not None
        assert out.exists(), "the frame log was not opened"
        header = json.loads(out.read_text(encoding="utf-8").splitlines()[0])
        assert header["kind"] == "header"
        # The endpoint in the header is the stripped one (R1, KTD13): the
        # recorder is handed ``AttachTarget.url``, never the dialled URL.
        assert "token=" not in header["endpoint"]
    finally:
        if source._recorder is not None:
            source._recorder.close()


def test_run_live_propagates_the_apps_exit_code(monkeypatch: pytest.MonkeyPatch) -> None:
    """``talaria``'s process exit code is the app's, not a hard-coded zero.

    ``TalariaApp`` exits 70 when its frame source fails mid-stream (R36), and
    the console script is the only thing that turns that into an exit status a
    supervisor can see. Every other test in this file replaces ``run_live`` with
    a double, so replacing ``return app.return_code or 0`` with ``return 0``
    left the suite green. The app is faked here — building a real one would
    dial — and both a failure code and a clean exit are asserted, so a
    ``return 70`` would fail this just as a ``return 0`` does.
    """

    class FakeApp:
        def __init__(self, code: int | None) -> None:
            self.return_code = code
            self.ran = False

        def run(self) -> None:
            self.ran = True

    built: list[FakeApp] = []
    codes: list[int | None] = []

    def fake_build(args: object, cfg: object) -> tuple[FakeApp, object]:
        app = FakeApp(codes.pop(0))
        built.append(app)
        # A source whose provider has no ``prime``: nothing to resolve up front,
        # so the launch proceeds straight to the interface.
        return app, _FakeSource(object())

    monkeypatch.setattr(cli_module, "build_live_app", fake_build)

    codes[:] = [70]
    assert cli_module.run_live(parse_args([])) == 70
    codes[:] = [None]
    assert cli_module.run_live(parse_args([])) == 0
    assert [app.ran for app in built] == [True, True], "run_live never started the app"


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


# ── refreshing the credential from a running dashboard ───────────────────


def test_main_routes_refresh_credential_to_its_command(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    seen: list[object] = []

    def fake_refresh_command(args: object) -> int:
        seen.append(args)
        return 0

    monkeypatch.setattr(cli_module, "run_refresh_credential", fake_refresh_command)

    assert cli_module.main(["refresh-credential"]) == 0
    assert len(seen) == 1


def test_refresh_credential_derives_the_dashboard_from_the_gateway_endpoint(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """With no ``--from``, the dashboard is the host and port Talaria already dials.

    Deriving it means an operator cannot refresh the credential for one gateway
    into the file another gateway will be dialled with.
    """
    import talaria.transport.refresh as refresh_module

    seen: list[str] = []

    def fake_refresh(origin: str, path: Path, *, timeout: float) -> object:
        seen.append(origin)
        return refresh_module.RefreshReport(
            path=path, origin=origin, created=True, tightened=False, preserved_keys=("url",)
        )

    monkeypatch.setenv("TALARIA_GATEWAY_URL", "ws://127.0.0.1:9119/api/ws")
    monkeypatch.setattr(refresh_module, "refresh_credential", fake_refresh)

    assert cli_module.main(["refresh-credential"]) == 0
    assert seen == ["http://127.0.0.1:9119/"]

    printed = capsys.readouterr().out
    assert "created" in printed
    assert "kept: url" in printed


def test_refresh_credential_reports_a_missing_dashboard_and_exits_two(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """Port 1 on loopback is reserved and nothing listens there."""
    monkeypatch.setenv("TALARIA_GATEWAY_URL", "ws://127.0.0.1:1/api/ws")

    assert cli_module.main(["refresh-credential", "--timeout", "5"]) == 2
    assert "no dashboard answered" in capsys.readouterr().err


# ── the credential is resolved before the interface takes the terminal ───


class _RecordingApp:
    """Stands in for the Textual app, recording only whether it was started."""

    def __init__(self, order: list[str]) -> None:
        self._order = order
        self.return_code = 0

    def run(self) -> None:
        self._order.append("interface-started")


class _FakeSource:
    """Carries a provider and nothing else; ``run_live`` needs no more."""

    def __init__(self, provider: object) -> None:
        self.provider = provider


def test_the_credential_is_resolved_before_the_interface_starts(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """The ordering defect this test exists for, stated plainly.

    Acquisition used to happen on the first dial, which happens in ``on_mount``
    -- inside a running Textual app that owns the screen and reads stdin. The
    hidden prompt was written where nothing could show it and blocked on a read
    racing the UI's input driver, so the client sat on "connecting to gateway"
    with no socket ever opened. Asserting the *order* is the only way to pin
    this: both calls happen either way, and only their sequence was ever wrong.
    """
    order: list[str] = []

    def _prompt(label: str) -> str:
        order.append("prompted")
        return "typed-at-launch"

    provider = LoopbackTokenProvider(
        credentials_path=tmp_path / "absent", environ={}, prompt=_prompt
    )
    app = _RecordingApp(order)
    monkeypatch.setattr(
        cli_module, "build_live_app", lambda args, cfg: (app, _FakeSource(provider))
    )

    assert cli_module.run_live(parse_args([])) == 0
    assert order == ["prompted", "interface-started"]


def test_the_interface_never_starts_without_a_credential(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    """A missing credential must be a printed remedy, not a screen that hangs."""
    order: list[str] = []
    missing = tmp_path / "absent"
    provider = LoopbackTokenProvider(
        credentials_path=missing, environ={}, allow_prompt=False
    )
    app = _RecordingApp(order)
    monkeypatch.setattr(
        cli_module, "build_live_app", lambda args, cfg: (app, _FakeSource(provider))
    )

    exit_code = cli_module.run_live(parse_args([]))

    assert order == [], "the interface started with no credential to dial with"
    assert exit_code == 2
    message = capsys.readouterr().err
    assert "credential" in message
    assert str(missing) in message, "the remedy has to name the file the operator should write"


def test_cancelling_at_the_credential_prompt_stops_before_the_interface(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    """Ctrl-C at the prompt is an answer, and it is not a traceback."""
    order: list[str] = []

    def _cancelled(label: str) -> str:
        raise KeyboardInterrupt

    provider = LoopbackTokenProvider(
        credentials_path=tmp_path / "absent", environ={}, prompt=_cancelled
    )
    app = _RecordingApp(order)
    monkeypatch.setattr(
        cli_module, "build_live_app", lambda args, cfg: (app, _FakeSource(provider))
    )

    exit_code = cli_module.run_live(parse_args([]))

    assert exit_code == 130
    assert order == []
    assert "cancelled" in capsys.readouterr().err
