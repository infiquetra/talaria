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
from talaria.recorder.command import RecordTarget
from talaria.status.contract import StatusBarSettings
from talaria.transport.credentials import LoopbackTokenProvider

VSCODE_THEME_FIXTURES = Path(__file__).parent / "fixtures" / "vscode-themes"


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


# ── issue #105: bounded Visual Studio Code theme import ─────────────────


def test_theme_import_parses_the_nested_argv_and_optional_name() -> None:
    args = parse_args(
        ["theme", "import", "/tmp/source-theme.json", "--name", "stored-theme"]
    )

    assert args.command == "theme"
    assert args.theme_command == "import"
    assert args.source == Path("/tmp/source-theme.json")
    assert args.name == "stored-theme"


def test_theme_requires_a_nested_operation(
    capsys: pytest.CaptureFixture[str],
) -> None:
    with pytest.raises(SystemExit) as excinfo:
        parse_args(["theme"])

    assert excinfo.value.code == 2
    assert "required" in capsys.readouterr().err


def test_theme_import_prints_the_complete_success_report_and_exits_zero(
    isolated_global_config_dir: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    source = VSCODE_THEME_FIXTURES / "sample-dark.json"

    assert main(["theme", "import", str(source), "--name", "cli-sample"]) == 0

    printed = capsys.readouterr()
    assert printed.err == ""
    assert (
        "Imported cli-sample as user theme cli-sample: "
        "40 source tokens, 18 fallbacks, 0 warnings."
    ) in printed.out
    assert (
        "composite: colors.editor.selectionBackground -> "
        "talaria.selection.background: #FF000080 over #102030 = #881018"
    ) in printed.out
    assert "fallback: talaria.secondary <- Refined Default #6F42C1" in printed.out
    assert (isolated_global_config_dir / "themes" / "cli-sample.json").is_file()


def test_theme_import_warnings_use_stderr_without_turning_success_into_failure(
    isolated_global_config_dir: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    source = VSCODE_THEME_FIXTURES / "unsupported-dark.json"

    assert main(["theme", "import", str(source)]) == 0

    printed = capsys.readouterr()
    assert (
        "Imported warnings-dark as user theme warnings-dark: "
        "2 source tokens, 56 fallbacks, 19 warnings."
    ) in printed.out
    assert "warning:" not in printed.out
    warnings = printed.err.splitlines()
    assert len(warnings) == 19
    assert all(line.startswith("warning: ") for line in warnings)
    assert warnings[0] == (
        "warning: root.include is unsupported; external theme files are not read"
    )


def test_theme_import_defangs_attacker_controlled_warning_keys(
    isolated_global_config_dir: Path,
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    escape = chr(27)
    source = tmp_path / "hostile-key.json"
    source.write_text(
        json.dumps(
            {
                "colors": {
                    "editor.background": "#123456",
                    f"junk{escape}]0;PWNED{chr(7)}{escape}[2J": "#FFFFFF",
                }
            }
        ),
        encoding="utf-8",
    )

    assert main(["theme", "import", str(source)]) == 0

    printed = capsys.readouterr()
    assert b"\x1b" not in printed.out.encode()
    assert b"\x1b" not in printed.err.encode()
    assert (isolated_global_config_dir / "themes" / "hostile-key.json").is_file()


def test_theme_import_defangs_an_attacker_controlled_error_path(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    source = tmp_path / f"missing{chr(27)}[2J.json"

    assert main(["theme", "import", str(source)]) == 3

    printed = capsys.readouterr()
    assert printed.out == ""
    assert b"\x1b" not in printed.err.encode()


def test_theme_import_json_report_is_one_versioned_stdout_object(
    isolated_global_config_dir: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    source = VSCODE_THEME_FIXTURES / "unsupported-dark.json"

    assert main(["theme", "import", str(source), "--json"]) == 0

    printed = capsys.readouterr()
    assert printed.err == ""
    report = json.loads(printed.out)
    assert report == {
        "composites": [],
        "fallback_count": 56,
        "fallbacks": report["fallbacks"],
        "schema_version": "talaria-theme-import-report-v1",
        "slug": "warnings-dark",
        "source_token_count": 2,
        "target_path": str(
            isolated_global_config_dir / "themes" / "warnings-dark.json"
        ),
        "warning_count": 19,
        "warnings": report["warnings"],
    }
    assert len(report["fallbacks"]) == 56
    assert report["fallbacks"][0] == {
        "severity": "info",
        "source": "refined-default",
        "token": "talaria.surface",
        "value": "#FFFFFF",
    }
    assert len(report["warnings"]) == 19
    assert report["warnings"][0] == {
        "message": "root.include is unsupported; external theme files are not read",
        "severity": "warning",
    }


def test_theme_import_json_reports_every_prose_alpha_composite(
    isolated_global_config_dir: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    source = VSCODE_THEME_FIXTURES / "sample-dark.json"
    argv = ["theme", "import", str(source), "--name", "json-composite"]

    assert main(argv) == 0
    prose = capsys.readouterr()
    prose_composites = tuple(
        line for line in prose.out.splitlines() if line.startswith("composite: ")
    )

    assert main([*argv, "--json"]) == 0
    printed = capsys.readouterr()
    report = json.loads(printed.out)

    assert printed.err == ""
    assert len(report["composites"]) == len(prose_composites) == 1
    assert report["composites"] == [
        {
            "background": "#102030",
            "path": "colors.editor.selectionBackground",
            "severity": "info",
            "source": "#FF000080",
            "token": "talaria.selection.background",
            "value": "#881018",
        }
    ]


def test_theme_import_json_failures_preserve_distinct_error_kinds(
    capsys: pytest.CaptureFixture[str],
) -> None:
    failures: list[dict[str, object]] = []

    for fixture, expected_kind in (
        ("malformed.json", "malformed"),
        ("wrong-root.json", "wrong-root"),
    ):
        source = VSCODE_THEME_FIXTURES / fixture
        assert main(["theme", "import", str(source), "--json"]) == 3
        printed = capsys.readouterr()
        error = json.loads(printed.out)

        assert printed.err == ""
        assert error["schema_version"] == "talaria-theme-import-error-v1"
        assert error["kind"] == expected_kind
        assert isinstance(error["message"], str) and error["message"]
        failures.append(error)

    assert failures[0]["kind"] != failures[1]["kind"]


@pytest.mark.parametrize(
    ("fixture", "extra_argv", "expected"),
    [
        ("does-not-exist.json", (), 3),
        ("empty.json", (), 3),
        ("malformed.json", (), 3),
        ("wrong-root.json", (), 3),
        ("sample-dark.json", ("--name", "refined-default"), 4),
        ("sample-dark.json", ("--name", "Bad Name"), 4),
    ],
    ids=[
        "unreadable",
        "empty",
        "malformed",
        "wrong-root",
        "reserved-slug",
        "invalid-slug",
    ],
)
def test_theme_import_failure_kinds_have_non_usage_exit_codes(
    fixture: str,
    extra_argv: tuple[str, ...],
    expected: int,
    capsys: pytest.CaptureFixture[str],
) -> None:
    source = VSCODE_THEME_FIXTURES / fixture

    assert main(["theme", "import", str(source), *extra_argv]) == expected

    printed = capsys.readouterr()
    assert printed.out == ""
    assert printed.err.startswith("talaria: theme import failed: ")


def test_theme_import_unwritable_failure_has_write_exit_code(
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    import talaria.ui.theme_import as theme_import_module

    def fail_write(*args: object, **kwargs: object) -> None:
        del args, kwargs
        raise OSError("write refused")

    monkeypatch.setattr(theme_import_module, "write_user_theme", fail_write)
    source = VSCODE_THEME_FIXTURES / "sample-dark.json"

    assert main(["theme", "import", str(source)]) == 5
    assert "write refused" in capsys.readouterr().err


def test_theme_import_argparse_usage_error_remains_two(
    capsys: pytest.CaptureFixture[str],
) -> None:
    with pytest.raises(SystemExit) as excinfo:
        main(["theme", "import"])

    assert excinfo.value.code == 2
    assert "required" in capsys.readouterr().err


def test_theme_import_failure_uses_stderr_exits_three_and_writes_nothing(
    isolated_global_config_dir: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    source = VSCODE_THEME_FIXTURES / "malformed.json"

    assert main(["theme", "import", str(source)]) == 3

    printed = capsys.readouterr()
    assert printed.out == ""
    assert "talaria: theme import failed:" in printed.err
    assert "not strict JSON" in printed.err
    assert not (isolated_global_config_dir / "themes").exists()


def test_invalid_config_is_one_operator_error_and_gate_failure_stays_one(
    isolated_global_config_dir: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    path = isolated_global_config_dir / "config.toml"
    path.write_text("[broken\n", encoding="utf-8")

    assert main([]) == 2

    printed = capsys.readouterr()
    assert printed.out == ""
    assert printed.err.splitlines() == [
        next(
            line
            for line in printed.err.splitlines()
            if str(path) in line and "not valid TOML" in line
        )
    ]
    assert "Traceback" not in printed.err

    path.unlink()
    monkeypatch.setattr(cli_module, "run_gate_command", lambda args: 1)
    assert main(["gate"]) == 1


def test_a_fresh_live_and_replay_launch_receive_the_imported_registry(
    isolated_global_config_dir: Path,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    source = VSCODE_THEME_FIXTURES / "sample-dark.json"
    cfg_before = config_module.load_config(cwd=tmp_path)
    assert "restart-dark" not in cli_module._theme_registry(cfg_before).slugs

    assert main(["theme", "import", str(source), "--name", "restart-dark"]) == 0
    capsys.readouterr()

    cfg_after = config_module.load_config(cwd=tmp_path)
    live, _source = cli_module.build_live_app(parse_args([]), cfg_after)
    live_theme = live.theme_registry.resolve("restart-dark")
    assert "restart-dark" in live.theme_registry.slugs
    assert live_theme.tokens["talaria.selection.background"] == "#881018"

    captured: dict[str, object] = {}

    class FakeSource:
        focus_profile = ""

    class CapturingApp:
        def __init__(self, source: object, **kwargs: object) -> None:
            captured["source"] = source
            captured.update(kwargs)

        def run(self) -> None:
            captured["ran"] = True

    import talaria.replay.source as replay_source_module
    import talaria.ui.app as app_module

    monkeypatch.setattr(
        replay_source_module,
        "source_from_path",
        lambda *args, **kwargs: FakeSource(),
    )
    monkeypatch.setattr(app_module, "TalariaApp", CapturingApp)

    assert cli_module.run_replay(parse_args(["replay", str(tmp_path / "corpus")])) == 0
    replay_registry = captured["theme_registry"]
    from talaria.ui.theme import ThemeRegistry

    assert isinstance(replay_registry, ThemeRegistry)
    assert replay_registry.resolve("restart-dark").tokens[
        "talaria.selection.background"
    ] == "#881018"
    assert captured["ran"] is True


# ── U10: the live launcher is assembled, without dialling anything ───────


def test_the_live_app_is_assembled_on_a_connection_set() -> None:
    """``build_live_app`` is the whole wiring, it opens no socket, and the thing
    it wires the app to is the CONNECTION SET rather than one ``LiveSource``.

    This test used to assert ``app.source is source`` and ``app.dispatcher is
    source``, which was true and was the defect: U2 built the connection set and
    its two-gateway tests, nothing ever assembled one, so ``app.connections`` was
    permanently ``None``, every multi-connection branch was dead, and U2's goal —
    "Talaria dials every configured profile endpoint concurrently" — was not true
    of the running program. U9's two-profile live acceptance would have been the
    first thing to discover it.

    It is rewritten rather than repointed, and it is the regression guard the
    operator's ruling of 2026-08-18 asked for: it fails if this entry point ever
    goes back to handing the app a lone ``LiveSource``.

    :class:`~talaria.transport.source.LiveSource` dials from ``start()``, never
    from its constructor, and :class:`ConnectionSet` dials from ``start()`` too,
    which is what makes the assembly assertable here rather than only against a
    stub gateway.
    """
    from talaria.transport.connection_set import ConnectionSet
    from talaria.transport.source import LiveSource
    from talaria.ui.app import ConnectionFleet, ConnectionInventory, TalariaApp

    cfg = config_module.load_config()
    app, source = cli_module.build_live_app(parse_args([]), cfg)

    assert isinstance(app, TalariaApp)
    assert app.mode == "live"

    # The regression guard, stated three ways because each one is a different way
    # of reverting: the app's stream, the ensure-beside seam, and the fleet dial.
    assert isinstance(app.source, ConnectionSet), (
        "the app was handed a single source again; the fleet cannot see any "
        "connection but one"
    )
    assert app.connections is app.source, (
        "the app has no ConnectionEnsurer, so /profiles falls back to "
        "drop-switching and every multi-connection branch is dead again"
    )
    assert isinstance(app.connections, ConnectionFleet), (
        "nothing will dial the inventory on mount"
    )
    assert isinstance(app.connections, ConnectionInventory), (
        "no background connection can be probed or swept"
    )

    # The launch depends on two constants in two packages agreeing, and nothing
    # made them agree on purpose: the set's home member is always named
    # ``DEFAULT_PROFILE_NAME`` (``talaria/transport/credentials.py``) while the
    # app's fleet starts focused on ``DEFAULT_PROFILE`` (``talaria/ui/app.py``).
    # Both read ``"default"`` today. If either moved, the home connection would
    # arrive at ``note_connection_state`` as a BACKGROUND connection — no epoch
    # bump, no catalogue fetch, and ``begin_live_startup`` never called, so the
    # client would come up attached to nothing with no error anywhere. Asserted
    # here so the coincidence is a requirement rather than a coincidence.
    assert app.fleet_profile == app.connections.home, (
        "the app is focused on a profile the connection set does not call home; "
        "the home gateway's connect would be handled as a background one"
    )

    # The single-connection primitive is refused, not merely unused: retargeting
    # one socket drops whatever it was connected to, and the connection it drops
    # is the only feed for that gateway's sessions.
    assert app.switcher is None

    # Nothing dialled. ``source`` survives only to prime the interactive
    # credential level while a human can still see the terminal.
    assert isinstance(source, LiveSource)
    assert not source.closed
    assert source.state == "disconnected", "building the launcher dialled a gateway"
    assert not app.source.closed
    assert app.source.connected_profiles == (), "building the launcher dialled a gateway"


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


def test_the_live_launcher_forwards_theme_config_and_save_targets(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[theme]\nname = "neutral-dark"\n\n[ui]\nreduced_motion = true\n',
        encoding="utf-8",
    )
    cfg = config_module.load_config(cwd=tmp_path)

    app, _ = cli_module.build_live_app(parse_args([]), cfg)

    assert app.theme == "neutral-dark"
    assert app.configured_theme_slug == "neutral-dark"
    assert app.theme_config_dir == isolated_global_config_dir
    assert app.launch_cwd == Path.cwd()
    assert app.motion.reduced is True
    assert app._startup_notices == ()


def test_the_live_launcher_forwards_normalized_status_bar_settings(
    isolated_global_config_dir: Path,
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[status]\n"
        'segments = ["connection", "cwd"]\n'
        "cwd_max_columns = 36\n"
        "git_branch_max_columns = 30\n"
        "agent_model_max_columns = 40\n",
        encoding="utf-8",
    )

    app, _ = cli_module.build_live_app(parse_args([]), config_module.load_config())

    assert app.status_bar_settings.segments == ("connection", "cwd")
    assert app.status_bar_settings.cwd_max_columns == 36
    assert app.status_bar_settings.git_branch_max_columns == 30
    assert app.status_bar_settings.agent_model_max_columns == 40


def test_the_replay_launcher_forwards_theme_fallback_notices(
    isolated_global_config_dir: Path,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[theme]\nname = 7\n\n[ui]\nreduced_motion = true\n", encoding="utf-8"
    )
    captured: dict[str, object] = {}

    class FakeSource:
        focus_profile = ""

    class CapturingApp:
        def __init__(self, source: object, **kwargs: object) -> None:
            captured["source"] = source
            captured.update(kwargs)

        def run(self) -> None:
            captured["ran"] = True

    import talaria.replay.source as replay_source_module
    import talaria.ui.app as app_module

    monkeypatch.setattr(
        replay_source_module,
        "source_from_path",
        lambda *args, **kwargs: FakeSource(),
    )
    monkeypatch.setattr(app_module, "TalariaApp", CapturingApp)

    assert cli_module.run_replay(parse_args(["replay", str(tmp_path / "corpus")])) == 0
    assert captured["theme_name"] == "refined-default"
    notices = captured["startup_notices"]
    assert isinstance(notices, tuple)
    assert "must be a string" in notices[0]
    assert captured["theme_config_dir"] == isolated_global_config_dir
    assert captured["launch_cwd"] == Path.cwd()
    assert captured["reduced_motion"] is True
    status_settings = captured["status_bar_settings"]
    assert isinstance(status_settings, StatusBarSettings)
    assert status_settings.segments == (
        "cwd",
        "git_branch",
        "agent_model",
        "context",
        "task_progress",
        "connection",
        "version",
    )
    assert captured["ran"] is True


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


@pytest.mark.parametrize(
    ("toml_value", "expected_allowlist"),
    [
        ("false", ()),
        ("0", ()),
        ("0.0", ()),
        ("[]", ()),
        ('"FOO"', ("F", "O", "O")),
    ],
)
def test_falsy_and_string_allowlists_reach_the_status_runner_without_notice(
    isolated_global_config_dir: Path,
    tmp_path: Path,
    toml_value: str,
    expected_allowlist: tuple[str, ...],
) -> None:
    """Pin the silent cases that the deferred allowlist debt must describe."""
    (isolated_global_config_dir / "config.toml").write_text(
        f'[status]\ncommand = "/usr/bin/true"\n'
        f"[environment]\nallowlist = {toml_value}\n",
        encoding="utf-8",
    )
    cfg = config_module.load_config(cwd=tmp_path)

    runner = cli_module._build_status_runner(cfg)

    assert runner is not None
    assert runner._allowlist == expected_allowlist
    assert cfg.notices == ()


@pytest.mark.parametrize("toml_value", ["42", "true"])
def test_truthy_non_iterable_allowlists_raise_at_status_runner_construction(
    isolated_global_config_dir: Path,
    tmp_path: Path,
    toml_value: str,
) -> None:
    """Pin the raising cases without broadening this repair into validation."""
    (isolated_global_config_dir / "config.toml").write_text(
        f'[status]\ncommand = "/usr/bin/true"\n'
        f"[environment]\nallowlist = {toml_value}\n",
        encoding="utf-8",
    )
    cfg = config_module.load_config(cwd=tmp_path)

    with pytest.raises(TypeError):
        cli_module._build_status_runner(cfg)

    assert cfg.notices == ()


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
    cfg = config_module.load_config()
    assert cfg.get("status", "command") is None
    assert any(
        "status.command" in notice and "disabled" in notice
        for notice in cfg.notices
    )
    assert any("status.command" in notice for notice in app._status_notices)


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
    must record nothing, or "the recorder is present" would be satisfied by a
    client that always records.

    **Asserted on the connection set, not on the returned source, since U8.** It
    used to read ``source._recorder is not None`` — true, and evidence of
    nothing: that ``LiveSource`` is never dialled (it survives only to prime the
    interactive credential level), so no frame ever reached the recorder it was
    handed. Every recorded frame arrives through the set's per-connection views.
    The old assertion would have stayed green through a launch that recorded
    not one frame, which is the shape of defect this release keeps finding.
    """
    plain_app, _plain_source = cli_module.build_live_app(
        parse_args([]), config_module.load_config()
    )
    from talaria.transport.connection_set import ConnectionSet

    assert isinstance(plain_app.connections, ConnectionSet)
    assert plain_app.connections._recorder is None, (
        "an unasked-for launch opened a frame log"
    )

    out = tmp_path / "session.jsonl"
    app, source = cli_module.build_live_app(
        parse_args(["--record", str(out)]), config_module.load_config()
    )
    try:
        assert isinstance(app.connections, ConnectionSet)
        assert app.connections._recorder is not None, (
            "the fleet that actually receives frames was handed no recorder"
        )
        assert source._recorder is None, (
            "the priming source was handed a recorder it can never feed"
        )
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


def test_record_subcommand_parses_endpoint_and_default_out() -> None:
    args = parse_args(["record", "ws://127.0.0.1:9119/api/ws"])
    assert args.command == "record"
    assert args.url == "ws://127.0.0.1:9119/api/ws"
    assert args.out is None


def test_record_subcommand_parses_explicit_out_path() -> None:
    args = parse_args(["record", "ws://127.0.0.1:9119/api/ws", "--out", "/tmp/x.jsonl"])
    assert args.out == "/tmp/x.jsonl"


def test_record_subcommand_takes_no_endpoint_at_all() -> None:
    """R3: the positional is optional, so nothing forces a URL onto the command
    line -- and a URL is where the credential used to ride."""
    args = parse_args(["record"])
    assert args.command == "record"
    assert args.url is None


# ── `record` and the credential (R3-R6) ──────────────────────────────────
#
# The canary below is a made-up string that has never authenticated anything. It
# is written as one literal per test rather than a shared constant so that a
# reader checking R10 -- no credential value anywhere in this work -- can see at
# the point of use that the value is synthetic.


def _fake_record_capture(monkeypatch: pytest.MonkeyPatch) -> list[RecordTarget]:
    """Replace `run_record` with a recorder of the target it was handed.

    `main()` imports `run_record` from the module inside the dispatch function,
    so patching the module attribute is enough and no socket is ever opened.
    """
    calls: list[RecordTarget] = []

    async def fake_run_record(target: RecordTarget, **kwargs: object) -> int:
        calls.append(target)
        return 0

    import talaria.recorder.command as command_module

    monkeypatch.setattr(command_module, "run_record", fake_run_record)
    return calls


#: The one phrase only the credential refusal emits.
#:
#: `run_record_command` exits 2 for two unrelated reasons (KTD7): the refusal,
#: and a credential the chain could not supply. Under pytest the chain supplies
#: nothing and has no terminal to prompt on, so it raises `CredentialError` and
#: exits 2 as well -- which means an exit-code assertion on its own cannot tell
#: a working refusal from a deleted one. Every refusal test asserts this string
#: so that it can.
REFUSAL_SIGNATURE = "refusing to record"


@pytest.mark.parametrize(
    ("label", "url"),
    [
        ("query string", "ws://127.0.0.1:9119/api/ws?token=NOT-A-REAL-CANARY-4f2b91"),
        ("userinfo", "ws://operator:NOT-A-REAL-CANARY-4f2b91@127.0.0.1:9119/api/ws"),
        ("fragment", "ws://127.0.0.1:9119/api/ws#token=NOT-A-REAL-CANARY-4f2b91"),
    ],
)
def test_record_refuses_a_credential_on_the_command_line(
    label: str,
    url: str,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    """R5: every credential-bearing shape is refused, and the exit code is 2.

    Query parameters are the form Hermes actually reads. Userinfo would never
    have authenticated -- Hermes reads the upgrade credential only from
    `ws.query_params` -- but it still put a secret in the process table and the
    shell history, which is the thing KTD1 exists to tell the operator about.

    The fragment is the same argument again, and it is the case that made this
    parametrize list grow: it cannot authenticate either, and unlike the other
    two nothing downstream withholds it. `strip_credential_query` drops
    credential query keys and `redact_url` withholds userinfo; neither touches a
    fragment. Before this row existed, `record` accepted such a URL, printed it
    to the terminal, and wrote it into the frame-log header verbatim.

    The exit code alone does not prove the refusal fired, which is why
    `REFUSAL_SIGNATURE` is asserted as well. `run_record_command` returns 2 for
    two different reasons -- a credential on the command line, and a credential
    the chain could not supply -- and under pytest the chain supplies nothing and
    cannot prompt, so it raises `CredentialError` and returns 2 anyway. Asserting
    only `== 2` and `calls == []` passes with the refusal deleted outright:
    verified by deleting it. The signature is the one string only the refusal
    emits.
    """
    calls = _fake_record_capture(monkeypatch)

    assert cli_module.main(["record", url]) == 2, label
    assert calls == [], f"{label}: a refused invocation still reached run_record"
    assert REFUSAL_SIGNATURE in capsys.readouterr().err, (
        f"{label}: exited 2 without refusing -- the credential check did not fire"
    )


@pytest.mark.parametrize(
    ("label", "url"),
    [
        ("query string", "ws://127.0.0.1:9119/api/ws?token=NOT-A-REAL-CANARY-4f2b91"),
        ("userinfo", "ws://operator:NOT-A-REAL-CANARY-4f2b91@127.0.0.1:9119/api/ws"),
        ("fragment", "ws://127.0.0.1:9119/api/ws#token=NOT-A-REAL-CANARY-4f2b91"),
    ],
)
def test_the_record_refusal_reproduces_nothing_it_was_given(
    label: str,
    url: str,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    """R6: the refusal echoes neither the credential nor the URL that carried it.

    The value is already in the process table and in the shell history by the
    time the refusal is printed. Writing it to stderr would add a third copy, and
    in continuous integration stderr is a log file -- in a public repository.

    Fragments are checked, not just the whole value: a message that printed half
    a credential would still have leaked half a credential. Every four-character
    window of the canary is searched for case-insensitively, which is the
    strongest form of this assertion that does not start flagging ordinary
    English.
    """
    _fake_record_capture(monkeypatch)
    credential = "NOT-A-REAL-CANARY-4f2b91"

    assert cli_module.main(["record", url]) == 2

    message = capsys.readouterr().err
    assert message.strip(), f"{label}: the refusal printed nothing"
    # Without this, every assertion below is vacuous: an unrelated error message
    # also fails to contain the canary. Prove the refusal is what was printed
    # before proving what the refusal does not say.
    assert REFUSAL_SIGNATURE in message, (
        f"{label}: exited 2 without refusing -- the credential check did not fire"
    )

    haystack = message.lower()
    assert url not in message, f"{label}: the refusal echoed the URL it refused"
    assert "127.0.0.1:9119" not in message, f"{label}: the refusal echoed the endpoint"
    for start in range(len(credential) - 3):
        window = credential[start : start + 4].lower()
        assert window not in haystack, (
            f"{label}: the refusal contains a fragment of the credential it was given"
        )


def test_the_record_refusal_names_only_the_surviving_routes(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """A refusal that only says no teaches the operator nothing (KTD1).

    It must also say that the value they passed is now exposed, which is the part
    a silent strip would never have told them.

    **And every route it names has to work.** This message advertised
    ``HERMES_DASHBOARD_SESSION_TOKEN`` until KTD8 removed that level on
    2026-08-06. Sending an operator who has just leaked a credential to a route
    that silently resolves nothing is the worst available failure, so the
    absence is asserted alongside the presences.
    """
    from talaria.transport.credentials import GATEWAY_URL_ENV_VAR

    _fake_record_capture(monkeypatch)

    assert cli_module.main(["record", "ws://127.0.0.1:9119/api/ws?token=NOT-REAL-9zq"]) == 2

    message = capsys.readouterr().err
    assert "talaria refresh-credential" in message
    assert GATEWAY_URL_ENV_VAR in message
    assert "interactive prompt" in message
    assert "HERMES_DASHBOARD_SESSION_TOKEN" not in message
    assert "rotate" in message.lower()


def _write_credential_file(config_dir: Path, token: str, *, url: str | None = None) -> Path:
    """The `0600` file `talaria refresh-credential` writes, in a test's config dir.

    The `url` key is the second half of the environment-free configuration: since
    2026-08-07 it is the only way to name a non-default endpoint without
    exporting anything.
    """
    path = config_dir / "credentials"
    body = f'token = "{token}"\n' + (f'url = "{url}"\n' if url else "")
    path.write_text(body, encoding="utf-8")
    path.chmod(0o600)
    return path


def test_record_with_no_endpoint_resolves_the_one_the_launcher_would(
    monkeypatch: pytest.MonkeyPatch, isolated_global_config_dir: Path
) -> None:
    """R3/R4: a bare `talaria record` resolves both halves through the chain.

    **Both halves came from `TALARIA_GATEWAY_URL` until 2026-08-07** — the
    endpoint from the variable and the credential from its `token` query
    parameter. That route is gone and the variable is refused for carrying one,
    so both halves now come from the `0600` credential file: `token` for the
    credential, `url` for the endpoint. Nothing is exported, nothing is on the
    command line, and this is what `AttachTarget.from_environment` and
    `LoopbackTokenProvider` do for the live launcher too.
    """
    calls = _fake_record_capture(monkeypatch)
    _write_credential_file(
        isolated_global_config_dir,
        "NOT-A-REAL-CANARY-7c40de",
        url="ws://127.0.0.1:9911/api/ws",
    )

    assert cli_module.main(["record"]) == 0
    assert len(calls) == 1

    assert calls[0].endpoint == "ws://127.0.0.1:9911/api/ws"
    assert calls[0].credential.source == "file"
    assert calls[0].credential.value == "NOT-A-REAL-CANARY-7c40de"


def test_record_takes_a_credential_free_endpoint_as_an_override(
    monkeypatch: pytest.MonkeyPatch, isolated_global_config_dir: Path
) -> None:
    """KTD2: the positional means the endpoint, and only the endpoint.

    It overrides the configured endpoint the same way the launcher's `override=`
    does, while the credential still comes from the chain.
    """
    calls = _fake_record_capture(monkeypatch)
    _write_credential_file(
        isolated_global_config_dir,
        "NOT-A-REAL-CANARY-7c40de",
        url="ws://127.0.0.1:9911/api/ws",
    )

    assert cli_module.main(["record", "ws://127.0.0.1:9222/api/ws"]) == 0

    assert calls[0].endpoint == "ws://127.0.0.1:9222/api/ws"
    assert calls[0].credential.value == "NOT-A-REAL-CANARY-7c40de"


def test_record_refuses_an_exported_endpoint_that_carries_a_credential(
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    isolated_global_config_dir: Path,
) -> None:
    """The environment gets the same treatment argv has always had (R1).

    This invocation *worked* until 2026-08-07: the token was stripped off the
    endpoint and read straight back as the highest-precedence credential. The
    command line has refused the identical string for far longer, and the two
    were only ever different in which place the operator had put it.

    Exits 2 through the ordinary credential-failure path rather than the argv
    refusal, because nothing is wrong with the *argument* here — `record` was
    given none. What is asserted is that no recording started and that the
    credential never reaches stderr.
    """
    calls = _fake_record_capture(monkeypatch)
    _write_credential_file(isolated_global_config_dir, "NOT-A-REAL-FILE-CANARY-11")
    monkeypatch.setenv(
        "TALARIA_GATEWAY_URL", "ws://127.0.0.1:9911/api/ws?token=NOT-A-REAL-CANARY-7c40de"
    )

    assert cli_module.main(["record"]) == 2
    assert calls == [], "a refused endpoint still reached run_record"

    message = capsys.readouterr().err
    assert "NOT-A-REAL-CANARY-7c40de" not in message
    assert "TALARIA_GATEWAY_URL" in message
    assert "refresh-credential" in message


def test_record_reports_a_credential_the_chain_cannot_supply_and_exits_two(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """The other operator error on this path exits 2 as well (KTD7).

    With no environment variable, no credential file and the prompt refused,
    `LoopbackTokenProvider` raises `CredentialError`. That is neither of
    `run_record`'s two documented outcomes, so it is handled in the dispatch and
    `run_record` is never reached.
    """
    calls = _fake_record_capture(monkeypatch)

    import talaria.transport.credentials as credentials_module

    monkeypatch.setattr(credentials_module, "_has_controlling_terminal", lambda: False)

    assert cli_module.main(["record"]) == 2
    assert calls == []
    assert "talaria:" in capsys.readouterr().err


def test_an_unparseable_record_endpoint_is_refused_rather_than_parsed(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A string `urlsplit` cannot read cannot be shown to be credential-free.

    It is also exactly what an operator produces by pasting a credential into a
    URL by hand, so it is refused on the credential path rather than allowed
    through to fail later as a bad endpoint.
    """
    calls = _fake_record_capture(monkeypatch)

    assert cli_module.main(["record", "ws://[bad::/api/ws"]) == 2
    assert calls == []


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

    def fake_refresh(
        origin: str, path: Path, *, timeout: float, profile: str = ""
    ) -> object:
        seen.append(origin)
        return refresh_module.RefreshReport(
            path=path,
            origin=origin,
            created=True,
            tightened=False,
            preserved_keys=("url",),
            profile=profile,
        )

    monkeypatch.setenv("TALARIA_GATEWAY_URL", "ws://127.0.0.1:9119/api/ws")
    monkeypatch.setattr(refresh_module, "refresh_credential", fake_refresh)

    assert cli_module.main(["refresh-credential"]) == 0
    assert seen == ["http://127.0.0.1:9119/"]

    printed = capsys.readouterr().out
    assert "created" in printed
    assert "kept: url" in printed


def test_refresh_credential_profile_derives_the_dashboard_from_the_configured_endpoint(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    """KTD5: a named profile is paired against *its own* gateway's dashboard.

    Deriving the origin from the default endpoint instead would write one
    gateway's credential under another gateway's name — the confusion the
    single-endpoint-source rule exists to prevent.
    """
    import talaria.transport.refresh as refresh_module

    seen: list[tuple[str, str]] = []

    def fake_refresh(
        origin: str, path: Path, *, timeout: float, profile: str = ""
    ) -> object:
        seen.append((origin, profile))
        return refresh_module.RefreshReport(
            path=path,
            origin=origin,
            created=True,
            tightened=False,
            preserved_keys=(),
            profile=profile,
        )

    config_dir = tmp_path / "config"
    config_dir.mkdir()
    (config_dir / "config.toml").write_text(
        '[profiles.endpoints]\nalpha-fixture = "ws://127.0.0.1:9130/api/ws"\n',
        encoding="utf-8",
    )
    monkeypatch.setenv("TALARIA_CONFIG_DIR", str(config_dir))
    monkeypatch.setenv("TALARIA_GATEWAY_URL", "ws://127.0.0.1:9119/api/ws")
    monkeypatch.setattr(refresh_module, "refresh_credential", fake_refresh)

    assert cli_module.main(["refresh-credential", "--profile", "alpha-fixture"]) == 0
    assert seen == [("http://127.0.0.1:9130/", "alpha-fixture")]
    assert "[profiles.alpha-fixture]" in capsys.readouterr().out


def test_refresh_credential_refuses_a_profile_with_no_configured_endpoint(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    """Never silently paired against whichever gateway happens to be default."""
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    monkeypatch.setenv("TALARIA_CONFIG_DIR", str(config_dir))
    monkeypatch.setenv("TALARIA_GATEWAY_URL", "ws://127.0.0.1:9119/api/ws")

    assert cli_module.main(["refresh-credential", "--profile", "alpha-fixture"]) == 2
    message = capsys.readouterr().err
    assert "alpha-fixture" in message
    assert "[profiles.endpoints]" in message


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
        credentials_path=tmp_path / "absent", prompt=_prompt
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
    provider = LoopbackTokenProvider(credentials_path=missing, allow_prompt=False)
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
        credentials_path=tmp_path / "absent", prompt=_cancelled
    )
    app = _RecordingApp(order)
    monkeypatch.setattr(
        cli_module, "build_live_app", lambda args, cfg: (app, _FakeSource(provider))
    )

    exit_code = cli_module.run_live(parse_args([]))

    assert exit_code == 130
    assert order == []
    assert "cancelled" in capsys.readouterr().err


def test_the_assembled_app_knows_which_connection_it_is_on() -> None:
    """CR7 finding 4: the set knew its home and the app did not.

    Two shipped behaviours died on this, and neither of them noisily. ``/profiles``
    marks the current row through ``flatten_profiles(..., current=...)``, so with
    an empty ``current_profile`` it marked nothing and opened at row one —
    contradicting ``Selection.opened``'s own documented promise to open on the row
    already in use. And ``set_model_default`` returns early on
    ``if not self.current_profile``, so ``/models <n> default`` was refused in
    every live session, with a notice blaming a session "started without one
    named" that described all of them.

    Asserted against the connection set rather than against the literal
    ``"default"``, because the property is agreement between the two — a launcher
    that learned to take ``--profile`` should keep them in step without this test
    needing to know the name.
    """
    cfg = config_module.load_config()
    app, _ = cli_module.build_live_app(parse_args([]), cfg)

    connections = app.connections
    assert connections is not None, "no connection set was assembled at all"
    assert app.current_profile, "the app was assembled not knowing its own profile"
    assert app.current_profile == connections.home, (
        "the app and its connection set disagree about which profile is home"
    )
    assert app.fleet_profile == app.current_profile


def test_a_recording_launch_tells_the_recorder_the_whole_inventory() -> None:
    """U8's first finding: the live recorder was built before the fleet existed.

    ``recorded_connections`` needs the *resolved* inventory, and that does not
    exist until ``resolve_connections`` has run. A recorder constructed before
    it could only ever be told about nothing, which set ``multi_connection``
    False — and that flag is what makes ``view()`` skip its profile validation
    and ``_tag()`` drop the profile key. A live two-gateway ``talaria --record``
    run therefore wrote a **version-1, untagged** log: precisely the recording
    U8 exists to replay per connection, and the one shape it could not produce.

    Asserted as agreement between the recorder and the set rather than against
    ``multi_connection`` being True, because the correct answer depends on how
    many profiles are configured where this runs — one connection SHOULD write
    an untagged version-1 log (KTD6: "a log with none is one connection"). What
    must hold either way is that the recorder was told what the set knows.
    """
    import tempfile

    from talaria.recorder.framelog import FrameRecorder

    cfg = config_module.load_config()
    with tempfile.TemporaryDirectory() as directory:
        log = Path(directory) / "run.jsonl"
        app, _ = cli_module.build_live_app(parse_args(["--record", str(log)]), cfg)

        from talaria.transport.connection_set import ConnectionSet

        connections = app.connections
        assert isinstance(connections, ConnectionSet), "no connection set was assembled"
        recorder = connections._recorder
        assert isinstance(recorder, FrameRecorder), "the fleet was handed no recorder"

        assert [row.profile for row in recorder.connections] == list(connections.profiles), (
            "the recorder does not know the connections the set will record; a "
            "two-gateway run would write an untagged single-connection log"
        )
        assert recorder.multi_connection == (len(connections.profiles) > 1)
