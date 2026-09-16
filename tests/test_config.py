"""KTD15 precedence-chain tests for talaria/config.py.

The ``isolated_global_config_dir`` fixture these tests depend on is autouse and
lives in ``tests/conftest.py``, so the operator's real ``~/.talaria`` is never
read or written by any test in this suite.
"""

from __future__ import annotations

import os
import re
import stat
import tomllib
from pathlib import Path

import pytest

from talaria import config as config_module
from talaria.config import (
    CONFIG_VIEW_KEYS,
    DEFAULTS,
    ConfigError,
    credentials_path,
    load_config,
    recordings_dir,
    save_theme,
    setting_scopes,
)
from talaria.themes import ThemeSpec
from talaria.themes.builtins import REFINED_DEFAULT
from talaria.themes.storage import serialize_user_theme
from tests.conftest import HERMES_DASHBOARD_TOKEN_VAR


def test_defaults_apply_when_nothing_else_is_set(tmp_path: Path) -> None:
    cfg = load_config(cwd=tmp_path)
    assert cfg.get("theme", "name") == "refined-default"
    assert cfg.notices == ()
    assert cfg.get("status", "command") is None
    assert cfg.get("status", "interval_seconds") == 5
    assert cfg.get("status", "segments") == (
        "cwd",
        "git_branch",
        "agent_model",
        "context",
        "task_progress",
        "connection",
        "version",
    )
    assert cfg.get("status", "cwd_max_columns") == 24
    assert cfg.get("status", "git_branch_max_columns") == 18
    assert cfg.get("status", "agent_model_max_columns") == 24
    assert cfg.get("ui", "reduced_motion") is False
    assert cfg.get("composer", "paste_collapse_lines") == 6
    assert cfg.get("composer", "paste_collapse_bytes") == 512


def test_theme_precedence_is_default_then_user_then_repository(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[theme]\nname = "dark-green-terminal"\n', encoding="utf-8"
    )
    repository = tmp_path / ".talaria"
    repository.mkdir()
    (repository / "config.toml").write_text(
        '[theme]\nname = "neutral-dark"\n', encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("theme", "name") == "neutral-dark"
    assert cfg.notices == ()


def test_invalid_winning_theme_falls_to_default_not_a_weaker_scope(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[theme]\nname = "dark-green-terminal"\n', encoding="utf-8"
    )
    repository = tmp_path / ".talaria"
    repository.mkdir()
    (repository / "config.toml").write_text(
        '[theme]\nname = "not-installed"\n', encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("theme", "name") == "refined-default"
    assert len(cfg.notices) == 1
    assert "not-installed" in cfg.notices[0]
    assert "Refined Default" in cfg.notices[0]


def test_non_string_theme_falls_back_with_an_immutable_notice(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[theme]\nname = 7\n", encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("theme", "name") == "refined-default"
    assert isinstance(cfg.notices, tuple)
    assert "must be a string" in cfg.notices[0]


def test_scalar_theme_is_normalized_to_a_table_with_a_shape_notice(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        'theme = "refined-default"\n', encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("theme", "name") == "refined-default"
    assert isinstance(cfg.get("theme", "name"), str)
    assert cfg.notices == (
        "theme must be a table with a name key; using Refined Default "
        "(refined-default)",
    )


@pytest.mark.parametrize(
    ("before", "expected"),
    [
        (
            b'theme.name = "midnight-ink"\n',
            b'theme.name = "aurora-slate"\n',
        ),
        (
            b'theme.name = "midnight-ink"  # keep\n',
            b'theme.name = "aurora-slate"  # keep\n',
        ),
        (
            b"theme.name = 'midnight-ink'\n",
            b'theme.name = "aurora-slate"\n',
        ),
        (
            b'"theme".name = "midnight-ink"\n',
            b'"theme".name = "aurora-slate"\n',
        ),
        (
            b'theme."name" = "midnight-ink"\n',
            b'theme."name" = "aurora-slate"\n',
        ),
        (
            b'theme = { name = "midnight-ink" }\n',
            b'theme = { name = "aurora-slate" }\n',
        ),
        (
            b'theme={name="midnight-ink"}\n',
            b'theme={name="aurora-slate"}\n',
        ),
        (
            b'theme = { name = "midnight-ink" } # keep\n',
            b'theme = { name = "aurora-slate" } # keep\n',
        ),
        (
            b'theme = { source = "operator", name = "midnight-ink" }\n',
            b'theme = { source = "operator", name = "aurora-slate" }\n',
        ),
        (
            b'# keep\r\ntheme.name = "midnight-ink"\r\n',
            b'# keep\r\ntheme.name = "aurora-slate"\r\n',
        ),
        (
            b'[theme] # keep\nname = "midnight-ink"\n',
            b'[theme] # keep\nname = "aurora-slate"\n',
        ),
        (b"", b'[theme]\nname = "aurora-slate"\n'),
        (
            b"[status]\ninterval_seconds = 7\n",
            b'[status]\ninterval_seconds = 7\n\n[theme]\nname = "aurora-slate"\n',
        ),
    ],
    ids=(
        "dotted",
        "dotted-comment",
        "single-quoted",
        "quoted-theme-key",
        "quoted-name-key",
        "inline-spaced",
        "inline-tight",
        "inline-comment",
        "inline-second-key",
        "windows-crlf",
        "theme-header",
        "empty-file",
        "no-theme-table",
    ),
)
def test_save_theme_preserves_every_supported_toml_shape(
    isolated_global_config_dir: Path,
    before: bytes,
    expected: bytes,
) -> None:
    path = isolated_global_config_dir / "config.toml"
    path.write_bytes(before)

    saved = save_theme("aurora-slate", config_dir=isolated_global_config_dir)

    after = path.read_bytes()
    assert saved == path
    assert after == expected
    expected_document = tomllib.loads(before.decode("utf-8"))
    expected_document.setdefault("theme", {})["name"] = "aurora-slate"
    assert tomllib.loads(after.decode("utf-8")) == expected_document


@pytest.mark.parametrize(
    "original",
    [
        b'theme.name = """midnight"""\n',
        b"theme.name = '''midnight'''\n",
    ],
    ids=("multiline-basic", "multiline-literal"),
)
def test_save_theme_names_its_limit_for_multiline_string_values(
    isolated_global_config_dir: Path,
    original: bytes,
) -> None:
    path = isolated_global_config_dir / "config.toml"
    path.write_bytes(original)

    with pytest.raises(
        ConfigError,
        match=r"Talaria cannot safely rewrite theme\.name in this form",
    ) as caught:
        save_theme("solar-flare", config_dir=isolated_global_config_dir)

    assert "not valid TOML" not in str(caught.value)
    assert path.read_bytes() == original


def test_save_theme_through_a_symlink_preserves_link_target_and_mode(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    dotfiles = tmp_path / "dotfiles"
    dotfiles.mkdir()
    target = dotfiles / "talaria.toml"
    target.write_text('[theme]\nname = "refined-default"\n', encoding="utf-8")
    target.chmod(0o640)
    original_mode = stat.S_IMODE(target.stat().st_mode)
    link = isolated_global_config_dir / "config.toml"
    link.symlink_to(target)

    save_theme("neutral-dark", config_dir=isolated_global_config_dir)

    assert link.is_symlink()
    assert tomllib.loads(target.read_text(encoding="utf-8")) == {
        "theme": {"name": "neutral-dark"}
    }
    assert stat.S_IMODE(target.stat().st_mode) == original_mode


@pytest.mark.parametrize(
    ("state", "content"),
    [
        (
            "extra-field",
            '{"dark":true,"extra":1,"name":"Broken","slug":"broken","tokens":{}}',
        ),
        ("unrelated-json", '{"bogus":1}'),
        ("truncated-json", '{"dark":'),
        ("empty-file", ""),
        (
            "missing-tokens",
            '{"dark":true,"name":"Broken",'
            '"slug":"broken-missing-tokens","tokens":{}}',
        ),
    ],
)
def test_broken_stored_themes_are_skipped_without_hiding_valid_themes(
    isolated_global_config_dir: Path,
    state: str,
    content: str,
) -> None:
    themes = isolated_global_config_dir / "themes"
    themes.mkdir()
    broken = themes / f"broken-{state}.json"
    broken.write_text(content, encoding="utf-8")
    valid = ThemeSpec(
        slug="valid-user",
        name="Valid User",
        dark=REFINED_DEFAULT.dark,
        tokens=REFINED_DEFAULT.tokens,
    )
    (themes / "valid-user.json").write_bytes(serialize_user_theme(valid))
    (isolated_global_config_dir / "config.toml").write_text(
        '[theme]\nname = "valid-user"\n',
        encoding="utf-8",
    )

    cfg = load_config()

    assert cfg.get("theme", "name") == "valid-user"
    assert len(cfg.notices) == 1
    assert str(broken) in cfg.notices[0]
    assert "skipped" in cfg.notices[0]


def test_a_stored_imported_theme_slug_is_accepted_at_startup(
    isolated_global_config_dir: Path,
) -> None:
    themes = isolated_global_config_dir / "themes"
    themes.mkdir()
    imported = ThemeSpec(
        slug="stored-import",
        name="Stored Import",
        dark=REFINED_DEFAULT.dark,
        tokens=REFINED_DEFAULT.tokens,
    )
    (themes / "stored-import.json").write_bytes(serialize_user_theme(imported))
    (isolated_global_config_dir / "config.toml").write_text(
        '[theme]\nname = "stored-import"\n',
        encoding="utf-8",
    )

    cfg = load_config()

    assert cfg.get("theme", "name") == "stored-import"
    assert cfg.notices == ()


def test_theme_has_no_command_line_override(tmp_path: Path) -> None:
    cfg = load_config(
        cli_overrides={"theme": {"name": "neutral-dark"}}, cwd=tmp_path
    )

    assert cfg.get("theme", "name") == "refined-default"


def test_reduced_motion_precedence_is_default_then_user_then_repository(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[ui]\nreduced_motion = true\n", encoding="utf-8"
    )
    repository = tmp_path / ".talaria"
    repository.mkdir()
    (repository / "config.toml").write_text(
        "[ui]\nreduced_motion = false\n", encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("ui", "reduced_motion") is False
    assert cfg.notices == ()


def test_invalid_winning_reduced_motion_uses_false_not_a_weaker_scope(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[ui]\nreduced_motion = true\n", encoding="utf-8"
    )
    repository = tmp_path / ".talaria"
    repository.mkdir()
    (repository / "config.toml").write_text(
        '[ui]\nreduced_motion = "yes"\n', encoding="utf-8"
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("ui", "reduced_motion") is False
    assert isinstance(cfg.notices, tuple)
    assert cfg.notices == ("ui.reduced_motion must be a boolean; using false",)


def test_reduced_motion_has_no_environment_or_command_line_alias(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("TALARIA_UI_REDUCED_MOTION", "true")

    cfg = load_config(
        cli_overrides={"ui": {"reduced_motion": True}},
        cwd=tmp_path,
    )

    assert cfg.get("ui", "reduced_motion") is False
    assert cfg.notices == ()


def test_keys_defaults_are_ctrl_o_and_ctrl_s_with_no_notices(
    tmp_path: Path,
) -> None:
    cfg = load_config(cwd=tmp_path)

    assert cfg.get("keys", "toggle_inspector") == "ctrl+o"
    assert cfg.get("keys", "interrupt") == "ctrl+s"
    assert cfg.notices == ()


def test_keys_toml_override_takes_effect(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[keys]\ntoggle_inspector = "ctrl+x"\ninterrupt = "f4"\n',
        encoding="utf-8",
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("keys", "toggle_inspector") == "ctrl+x"
    assert cfg.get("keys", "interrupt") == "f4"
    assert cfg.notices == ()


def test_keys_environment_override_beats_the_toml_file(
    isolated_global_config_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[keys]\ntoggle_inspector = "ctrl+x"\n', encoding="utf-8"
    )
    monkeypatch.setenv("TALARIA_KEYS_TOGGLE_INSPECTOR", "alt+o")
    monkeypatch.setenv("TALARIA_KEYS_INTERRUPT", "alt+s")

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("keys", "toggle_inspector") == "alt+o"
    assert cfg.get("keys", "interrupt") == "alt+s"
    assert cfg.notices == ()


@pytest.mark.parametrize(
    ("body", "inspector", "interrupt", "notice_fragment"),
    [
        ('[keys]\ntoggle_inspector = ""\n', "ctrl+o", "ctrl+s", "keys.toggle_inspector"),
        ('[keys]\ntoggle_inspector = 7\n', "ctrl+o", "ctrl+s", "keys.toggle_inspector"),
        (
            '[keys]\ntoggle_inspector = "ctrl+banana"\n',
            "ctrl+o",
            "ctrl+s",
            "keys.toggle_inspector",
        ),
        (
            '[keys]\ninterrupt = "ctrl+q"\n',
            "ctrl+o",
            "ctrl+s",
            "reserved for quitting",
        ),
        (
            '[keys]\ntoggle_inspector = "ctrl+x"\ninterrupt = "ctrl+x"\n',
            "ctrl+o",
            "ctrl+s",
            "are both",
        ),
        ("[keys]\n", "ctrl+o", "ctrl+s", None),
        ('keys = "ctrl+x"\n', "ctrl+o", "ctrl+s", "must be a table"),
    ],
)
def test_keys_invalid_values_fall_back_visibly(
    isolated_global_config_dir: Path,
    tmp_path: Path,
    body: str,
    inspector: str,
    interrupt: str,
    notice_fragment: str | None,
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(body, encoding="utf-8")

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("keys", "toggle_inspector") == inspector
    assert cfg.get("keys", "interrupt") == interrupt
    if notice_fragment is None:
        assert cfg.notices == ()
    else:
        assert any(notice_fragment in notice for notice in cfg.notices), cfg.notices


def test_global_config_toml_overrides_default(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[status]\ncommand = "git status"\ninterval_seconds = 9\n'
    )
    cfg = load_config(cwd=tmp_path)
    assert cfg.get("status", "command") == "git status"
    assert cfg.get("status", "interval_seconds") == 9


def test_repo_local_config_overrides_global(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[status]\ncommand = "global-status"\n'
    )
    repo_local_dir = tmp_path / ".talaria"
    repo_local_dir.mkdir()
    (repo_local_dir / "config.toml").write_text('[status]\ncommand = "repo-status"\n')

    cfg = load_config(cwd=tmp_path)
    assert cfg.get("status", "command") == "repo-status"


def test_environment_variable_overrides_repo_local_config(
    isolated_global_config_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo_local_dir = tmp_path / ".talaria"
    repo_local_dir.mkdir()
    (repo_local_dir / "config.toml").write_text('[status]\ncommand = "repo-status"\n')
    monkeypatch.setenv("TALARIA_STATUS_COMMAND", "env-status")

    cfg = load_config(cwd=tmp_path)
    assert cfg.get("status", "command") == "env-status"


def test_cli_override_beats_environment_variable(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("TALARIA_STATUS_COMMAND", "env-status")

    cfg = load_config(cli_overrides={"status": {"command": "cli-status"}}, cwd=tmp_path)
    assert cfg.get("status", "command") == "cli-status"


def test_full_five_level_chain_resolves_to_the_highest_precedence(
    isolated_global_config_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[composer]\npaste_collapse_lines = 1\npaste_collapse_bytes = 1\n'
    )
    repo_local_dir = tmp_path / ".talaria"
    repo_local_dir.mkdir()
    (repo_local_dir / "config.toml").write_text(
        '[composer]\npaste_collapse_lines = 2\npaste_collapse_bytes = 2\n'
    )
    monkeypatch.setenv("TALARIA_COMPOSER_PASTE_COLLAPSE_LINES", "3")

    cfg = load_config(
        cli_overrides={"composer": {"paste_collapse_lines": 4}},
        cwd=tmp_path,
    )

    # cli_overrides wins outright for the key it sets.
    assert cfg.get("composer", "paste_collapse_lines") == 4
    # bytes is untouched by cli_overrides and the env var, so repo-local
    # (2) must win over global (1).
    assert cfg.get("composer", "paste_collapse_bytes") == 2


def test_talaria_config_dir_redirects_credentials_and_recordings_paths(
    isolated_global_config_dir: Path,
) -> None:
    assert credentials_path() == isolated_global_config_dir / "credentials"
    assert recordings_dir() == isolated_global_config_dir / "recordings"


def test_config_dir_defaults_to_home_talaria_without_the_env_override(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("TALARIA_CONFIG_DIR", raising=False)
    assert config_module.global_config_dir() == Path.home() / ".talaria"


def test_repo_local_level_is_isolated_to_tmp_path(tmp_path: Path) -> None:
    """A ``load_config()`` with no ``cwd`` must not reach the real repository.

    The repo-local level resolves against ``Path.cwd()``, so without the
    conftest fixture's ``chdir`` this call would read the repository's own
    git-ignored ``.talaria/config.toml`` and pass or fail on machine state.
    """
    repo_local = tmp_path / ".talaria"
    repo_local.mkdir()
    (repo_local / "config.toml").write_text('[status]\ncommand = "tmp-scoped"\n')

    cfg = load_config()  # deliberately no cwd argument

    assert Path.cwd().resolve() == tmp_path.resolve()
    assert cfg.get("status", "command") == "tmp-scoped"


#: The names an operator's shell can hold that would change what the suite reads
#: or dials. Written out literally rather than read back from the modules that
#: use them: a list derived from the thing under test survives deleting an entry
#: from that thing.
LEAKABLE_ENV_NAMES = (
    "TALARIA_GATEWAY_URL",
    "TALARIA_PROFILE",
    "TALARIA_LOG_LEVEL",
    "TALARIA_STATUS_INTERVAL",
    "TALARIA_STATUS_COMMAND",
    "TALARIA_STATUS_INTERVAL_SECONDS",
    "TALARIA_COMPOSER_PASTE_COLLAPSE_LINES",
    "TALARIA_COMPOSER_PASTE_COLLAPSE_BYTES",
    # CFG v0.6.2 D12: the new bindable chords keep the existing convention —
    # every [keys] chord has an env alias, because no single default survives
    # every terminal multiplexer.
    "TALARIA_KEYS_AGENTS",
    "TALARIA_KEYS_COMMANDS",
    "TALARIA_KEYS_MODELS",
    "TALARIA_KEYS_PROFILES",
    "TALARIA_KEYS_CONFIG",
    "TALARIA_KEYS_FOLLOW",
    "TALARIA_KEYS_REPLAY_PAUSE",
    "TALARIA_KEYS_REPLAY_SLOWER",
    "TALARIA_KEYS_REPLAY_FASTER",
)


def test_no_talaria_variable_from_the_shell_is_visible_inside_a_test() -> None:
    """The state every other test in this suite is entitled to assume."""
    for name in LEAKABLE_ENV_NAMES:
        assert name not in os.environ, f"{name} leaked into the suite from the shell"
    # The positive half: the fixture does set the one variable it owns, so a
    # fixture that had simply emptied the environment could not pass this.
    assert os.environ.get("TALARIA_CONFIG_DIR"), "the isolation fixture ran no setup"
    assert HERMES_DASHBOARD_TOKEN_VAR not in os.environ


def test_the_isolation_fixture_clears_variables_the_shell_actually_exported(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The fixture is run *against a polluted environment*, which is the only
    way this claim can fail on a clean developer machine.

    The test above passes trivially here: nothing exports ``TALARIA_GATEWAY_URL``
    on this machine, so it would stay green with the sweep deleted entirely.
    This one sets every leakable name first and then calls the real fixture
    function, so the sweep is the thing under test rather than the shell.

    That matters because the sweep replaced a narrower one.
    ``tests/conftest.py`` used to iterate ``config._ENV_KEY_MAP`` — four names,
    not including ``TALARIA_GATEWAY_URL``, which is what decides where a live
    dial would attach. On a machine with a Hermes dashboard listening, that is
    the difference between a test and a real session; U10 came within one
    unexported variable of finding out.
    """
    import tests.conftest as suite_conftest

    for name in LEAKABLE_ENV_NAMES:
        monkeypatch.setenv(name, f"shell-value-for-{name}")
    monkeypatch.setenv(HERMES_DASHBOARD_TOKEN_VAR, "shell-token")
    # The positive control, in the same observation: the pollution is really
    # there before the fixture runs.
    assert all(name in os.environ for name in LEAKABLE_ENV_NAMES)
    assert HERMES_DASHBOARD_TOKEN_VAR in os.environ

    inner_root = tmp_path / "inner"
    inner_root.mkdir()
    inner = pytest.MonkeyPatch()
    try:
        # ``__wrapped__`` is the undecorated function pytest stores on the
        # fixture object; calling the fixture itself would raise, and
        # duplicating its body here would test a copy rather than the fixture
        # every other test in this suite actually uses.
        fixture_body = suite_conftest.isolated_global_config_dir.__wrapped__  # type: ignore[attr-defined]
        global_dir = fixture_body(inner_root, inner)
        leaked = [name for name in LEAKABLE_ENV_NAMES if name in os.environ]
        assert not leaked, f"the isolation fixture left these set: {leaked}"
        assert HERMES_DASHBOARD_TOKEN_VAR not in os.environ
        assert os.environ["TALARIA_CONFIG_DIR"] == str(global_dir)
    finally:
        inner.undo()


def test_string_setting_keeps_a_numeric_looking_env_value_as_a_string(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """status.command is executed as an argument array (KTD5), never as an int."""
    monkeypatch.setenv("TALARIA_STATUS_COMMAND", "42")

    cfg = load_config(cwd=tmp_path)
    assert cfg.get("status", "command") == "42"
    assert isinstance(cfg.get("status", "command"), str)


def test_integer_setting_coerces_a_numeric_env_value(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("TALARIA_STATUS_INTERVAL_SECONDS", "11")

    cfg = load_config(cwd=tmp_path)
    assert cfg.get("status", "interval_seconds") == 11


def test_malformed_status_interval_env_value_falls_back_visibly(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("TALARIA_STATUS_INTERVAL_SECONDS", "--5")

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("status", "interval_seconds") == 5
    assert any(
        "status.interval_seconds" in notice and "using 5" in notice
        for notice in cfg.notices
    )


@pytest.mark.parametrize("bad", (-1, 0, 3601, "5", 2.5, True))
def test_invalid_winning_status_interval_uses_default_with_a_notice(
    tmp_path: Path, bad: object
) -> None:
    cfg = load_config(cli_overrides={"status": {"interval_seconds": bad}}, cwd=tmp_path)

    assert cfg.get("status", "interval_seconds") == 5
    assert any(
        "status.interval_seconds" in notice and "using 5" in notice
        for notice in cfg.notices
    )


@pytest.mark.parametrize("valid", (1, 17, 3600))
def test_valid_status_interval_counterexamples_survive_normalization(
    tmp_path: Path, valid: int
) -> None:
    cfg = load_config(
        cli_overrides={"status": {"interval_seconds": valid}}, cwd=tmp_path
    )

    assert cfg.get("status", "interval_seconds") == valid
    assert not any("status.interval_seconds" in notice for notice in cfg.notices)


@pytest.mark.parametrize(
    ("key", "bad", "fallback", "valid"),
    [
        ("cwd_max_columns", 7, 24, 48),
        ("cwd_max_columns", "24", 24, 8),
        ("git_branch_max_columns", 41, 18, 40),
        ("git_branch_max_columns", False, 18, 8),
        ("agent_model_max_columns", 9, 24, 48),
        ("agent_model_max_columns", 24.0, 24, 10),
    ],
)
def test_status_width_caps_validate_type_and_range_after_precedence(
    tmp_path: Path,
    key: str,
    bad: object,
    fallback: int,
    valid: int,
) -> None:
    invalid = load_config(cli_overrides={"status": {key: bad}}, cwd=tmp_path)
    accepted = load_config(cli_overrides={"status": {key: valid}}, cwd=tmp_path)

    assert invalid.get("status", key) == fallback
    assert any(f"status.{key}" in notice for notice in invalid.notices)
    assert accepted.get("status", key) == valid
    assert not any(f"status.{key}" in notice for notice in accepted.notices)


def test_status_segment_order_is_normalized_once_and_frozen(tmp_path: Path) -> None:
    cfg = load_config(
        cli_overrides={
            "status": {
                "segments": ["version", "unknown", "connection", "version"]
            }
        },
        cwd=tmp_path,
    )

    assert cfg.get("status", "segments") == ("version", "connection")
    assert any("unknown segment" in notice for notice in cfg.notices)
    assert any("duplicate" in notice for notice in cfg.notices)

    fallback = load_config(
        cli_overrides={"status": {"segments": ["unknown"]}}, cwd=tmp_path
    )
    assert fallback.get("status", "segments") == ("connection",)
    assert any("connection only" in notice for notice in fallback.notices)


def test_malformed_toml_names_the_offending_file(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    bad = isolated_global_config_dir / "config.toml"
    bad.write_text("[status\ncommand = ")

    with pytest.raises(ConfigError) as excinfo:
        load_config(cwd=tmp_path)
    assert str(bad) in str(excinfo.value)


def test_loading_config_never_aliases_the_builtin_defaults(tmp_path: Path) -> None:
    """A shallow copy of DEFAULTS would hand out its own nested section objects."""
    cfg = load_config(cwd=tmp_path)
    assert cfg.get("environment", "allowlist") is not DEFAULTS["environment"]["allowlist"]
    assert DEFAULTS["environment"]["allowlist"] == []


def test_resolved_config_is_deeply_immutable(tmp_path: Path) -> None:
    """frozen=True is cosmetic unless the nested containers are read-only too."""
    cfg = load_config(cwd=tmp_path)

    with pytest.raises(TypeError):
        cfg.values["status"]["interval_seconds"] = 999
    with pytest.raises(AttributeError):
        cfg.get("environment", "allowlist").append("HOME")

    assert cfg.get("status", "interval_seconds") == 5
    assert DEFAULTS["status"]["interval_seconds"] == 5


# ── U4: the profile endpoint map ──────────────────────────────────────────
#
# Hermes publishes no address for a profile's gateway, so this is where one
# comes from. Every profile name below is synthetic (R12): this is a public
# repository and the real inventory is the operator's.


def test_no_profile_endpoint_is_configured_by_default(tmp_path: Path) -> None:
    """The honest starting state: every profile reads as unaddressable."""
    cfg = load_config(cwd=tmp_path)
    assert config_module.profile_endpoints(cfg) == {}


def test_the_operator_supplies_profile_endpoints_in_config_toml(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[profiles.endpoints]\n"
        'alpha-fixture = "ws://127.0.0.1:9119/api/ws"\n'
        'beta-fixture = "ws://127.0.0.1:9120/api/ws"\n'
    )
    cfg = load_config(cwd=tmp_path)
    assert config_module.profile_endpoints(cfg) == {
        "alpha-fixture": "ws://127.0.0.1:9119/api/ws",
        "beta-fixture": "ws://127.0.0.1:9120/api/ws",
    }


def test_a_non_string_or_blank_endpoint_is_dropped_rather_than_coerced(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    """The value is a URL that will be dialled; ``str(7)`` is not one."""
    (isolated_global_config_dir / "config.toml").write_text(
        "[profiles.endpoints]\n"
        'alpha-fixture = "ws://127.0.0.1:9119/api/ws"\n'
        "beta-fixture = 9120\n"
        'gamma-fixture = "   "\n'
    )
    cfg = load_config(cwd=tmp_path)
    assert config_module.profile_endpoints(cfg) == {
        "alpha-fixture": "ws://127.0.0.1:9119/api/ws"
    }


def test_a_profiles_section_of_the_wrong_shape_yields_no_endpoints(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[profiles]\nendpoints = 7\n"
    )
    cfg = load_config(cwd=tmp_path)
    assert config_module.profile_endpoints(cfg) == {}


def test_v050_user_guide_toml_examples_parse_and_match_runtime_defaults() -> None:
    """Every TOML fence added with the v0.5 guides is executable documentation."""
    repository = Path(__file__).resolve().parents[1]
    fence = re.compile(r"(?ms)^```toml\n(.*?)^```[ \t]*$")
    documents = (
        repository / "docs" / "themes.md",
        repository / "docs" / "configuration.md",
        repository / "docs" / "terminal-ui.md",
    )

    examples = [
        tomllib.loads(source)
        for document in documents
        for source in fence.findall(document.read_text(encoding="utf-8"))
    ]

    # Exactly one fence carries the complete default shape (C4's starter
    # fence in the same document carries only a [status] table and is
    # validated by tests/status/test_starter_config.py instead).
    defaults = [example for example in examples if "theme" in example]
    assert len(defaults) == 1
    example = defaults[0]
    assert example["theme"] == DEFAULTS["theme"]
    assert example["ui"] == DEFAULTS["ui"]
    assert example["status"] == {
        key: value for key, value in DEFAULTS["status"].items() if key != "command"
    }
    assert example["environment"] == DEFAULTS["environment"]
    assert example["composer"] == DEFAULTS["composer"]
    assert example["keys"] == DEFAULTS["keys"]
    assert example["profiles"] == DEFAULTS["profiles"]


def test_configuration_guide_distinguishes_malformed_allowlist_behaviors() -> None:
    """A malformed allowlist has one outcome: the safe empty default."""
    guide = (
        Path(__file__).resolve().parents[1] / "docs" / "configuration.md"
    ).read_text(encoding="utf-8")
    guide = re.sub(r"\s+", " ", guide)

    assert "only a list of strings forwards as `environment.allowlist`" in guide
    assert (
        "falls back to the empty default with no notice: it never raises "
        "and never forwards character fragments"
    ) in guide


# ── the configuration view's provenance walk (issue #149, D8 recorded) ────


def test_setting_scopes_defaults_with_nothing_configured(tmp_path: Path) -> None:
    scopes = setting_scopes(cwd=tmp_path)

    assert scopes == {key: "default" for key in CONFIG_VIEW_KEYS}


def test_setting_scopes_names_the_user_file_for_its_keys(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[status]\ncommand = "user-status"\ninterval_seconds = 30\n'
    )

    scopes = setting_scopes(cwd=tmp_path)

    assert scopes[("status", "command")] == "user"
    assert scopes[("status", "interval_seconds")] == "user"
    # A table that omits a key claims nothing: the key stays at its default.
    assert scopes[("status", "segments")] == "default"
    assert scopes[("theme", "name")] == "default"


def test_setting_scopes_names_the_repository_file_when_it_wins(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[status]\ncommand = "user-status"\n'
    )
    repo_local = tmp_path / ".talaria"
    repo_local.mkdir()
    (repo_local / "config.toml").write_text('[status]\ncommand = "repo-status"\n')

    scopes = setting_scopes(cwd=tmp_path)

    assert scopes[("status", "command")] == "repository"


def test_setting_scopes_names_the_environment_when_it_wins(
    isolated_global_config_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("TALARIA_STATUS_INTERVAL_SECONDS", "30")

    scopes = setting_scopes(cwd=tmp_path)

    assert scopes[("status", "interval_seconds")] == "environment"
    # theme.name has no environment alias, so no TALARIA_* name can claim it.
    assert scopes[("theme", "name")] == "default"


def test_setting_scopes_names_a_cli_override(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.delenv("TALARIA_STATUS_COMMAND", raising=False)

    scopes = setting_scopes({"status": {"command": "cli-status"}}, cwd=tmp_path)

    assert scopes[("status", "command")] == "command line"


def test_setting_scopes_still_names_the_layer_that_supplied_an_invalid_value(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    """An invalid interval falls back to 5, but it still came from the user file.

    The view pairs this scope with the fallback notice ``load_config``
    produced, so the row reads "invalid, using default" and still says where
    the bad value came from rather than hiding it behind the default.
    """
    (isolated_global_config_dir / "config.toml").write_text(
        '[status]\ninterval_seconds = 0\n'
    )

    scopes = setting_scopes(cwd=tmp_path)

    assert scopes[("status", "interval_seconds")] == "user"


# ── CFG v0.6.2 D11/D12: new Talaria settings (unimplemented until B2) ─────
#
# Every assertion below reads through ``load_config``/``cfg.get``, which
# answer None for absent keys — so each test fails cleanly on the current
# DEFAULTS rather than erroring.


def test_d12_ui_defaults_match_current_fixed_behavior(tmp_path: Path) -> None:
    """Defaults are today's hardcoded values (inspector geometry, diff
    breakpoint, no timestamps): adopting the file changes nothing."""
    cfg = load_config(cwd=tmp_path)

    assert cfg.get("ui", "inspector_width") == 36
    assert cfg.get("ui", "inspector_open_at_start") is False
    assert cfg.get("ui", "inspector_dock_min_columns") == 120
    assert cfg.get("ui", "diff_side_by_side_min_columns") == 112
    assert cfg.get("ui", "show_timestamps") is False
    assert cfg.notices == ()


def test_d12_new_chord_defaults_match_current_fixed_bindings(
    tmp_path: Path,
) -> None:
    """Each default is the binding the action already has in
    ``talaria/ui/app.py`` — priority aliases where the desktop delivers
    several, so the pinned default works in every focus context."""
    cfg = load_config(cwd=tmp_path)

    assert cfg.get("keys", "agents") == "ctrl+g"
    assert cfg.get("keys", "commands") == "f3"
    assert cfg.get("keys", "models") == "f11"
    assert cfg.get("keys", "profiles") == "f12"
    assert cfg.get("keys", "config") == "ctrl+k"
    assert cfg.get("keys", "follow") == "f5"
    assert cfg.get("keys", "replay_pause") == "f8"
    assert cfg.get("keys", "replay_slower") == "f9"
    assert cfg.get("keys", "replay_faster") == "f10"
    assert cfg.notices == ()


def test_d12_composer_and_notification_defaults(tmp_path: Path) -> None:
    """16 MB mirrors the Desktop attachment cap; transcript_line True keeps
    today's rendering until the operator opts out."""
    cfg = load_config(cwd=tmp_path)

    assert cfg.get("composer", "attachment_max_mb") == 16
    assert cfg.get("notifications", "transcript_line") is True
    assert cfg.notices == ()


def test_d11_connections_default_to_no_inventory(tmp_path: Path) -> None:
    cfg = load_config(cwd=tmp_path)

    assert cfg.get("connections") == {}


def test_d11_profiles_endpoints_remain_the_compatibility_alias(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    """D11 adds ``[connections.*]``; it does not remove ``profiles.endpoints``.
    Both resolve side by side with no migration rewrite."""
    (isolated_global_config_dir / "config.toml").write_text(
        "[profiles.endpoints]\n"
        'alpha-fixture = "ws://127.0.0.1:9119/api/ws"\n'
        "[connections.office]\n"
        'url = "http://10.220.1.139:8765"\n'
        'auth = "gated"\n',
        encoding="utf-8",
    )

    cfg = load_config(cwd=tmp_path)

    assert config_module.profile_endpoints(cfg) == {
        "alpha-fixture": "ws://127.0.0.1:9119/api/ws"
    }
    assert cfg.get("connections", "office", "url") == "http://10.220.1.139:8765"
    assert cfg.get("connections", "office", "auth") == "gated"


@pytest.mark.parametrize(
    ("key", "bad", "fallback", "valid"),
    [
        ("inspector_width", 27, 36, 48),
        ("inspector_width", 49, 36, 28),
        ("inspector_width", True, 36, 40),
        ("inspector_dock_min_columns", 0, 120, 100),
        ("inspector_dock_min_columns", "120", 120, 140),
        ("diff_side_by_side_min_columns", -1, 112, 90),
        ("diff_side_by_side_min_columns", 112.0, 112, 130),
    ],
)
def test_d12_ui_geometry_validates_type_and_range_after_precedence(
    tmp_path: Path,
    key: str,
    bad: object,
    fallback: int,
    valid: int,
) -> None:
    """Inspector width keeps its 28–48 clamp from ``ui/inspector.py``; the
    breakpoints take any positive integer."""
    invalid = load_config(cli_overrides={"ui": {key: bad}}, cwd=tmp_path)
    accepted = load_config(cli_overrides={"ui": {key: valid}}, cwd=tmp_path)

    assert invalid.get("ui", key) == fallback
    assert any(f"ui.{key}" in notice for notice in invalid.notices)
    assert accepted.get("ui", key) == valid
    assert not any(f"ui.{key}" in notice for notice in accepted.notices)


@pytest.mark.parametrize(
    ("section", "key", "fallback"),
    [
        ("ui", "inspector_open_at_start", False),
        ("ui", "show_timestamps", False),
        ("notifications", "transcript_line", True),
    ],
)
def test_d12_bool_settings_reject_non_bools_visibly(
    tmp_path: Path, section: str, key: str, fallback: bool
) -> None:
    cfg = load_config(cli_overrides={section: {key: "yes"}}, cwd=tmp_path)

    assert cfg.get(section, key) is fallback
    assert any(f"{section}.{key}" in notice for notice in cfg.notices)


@pytest.mark.parametrize("bad", (0, -1, "lots", 16.5, True))
def test_d12_attachment_cap_takes_positive_integers_only(
    tmp_path: Path, bad: object
) -> None:
    cfg = load_config(
        cli_overrides={"composer": {"attachment_max_mb": bad}}, cwd=tmp_path
    )

    assert cfg.get("composer", "attachment_max_mb") == 16
    assert any("composer.attachment_max_mb" in notice for notice in cfg.notices)


def test_d12_new_chord_toml_override_takes_effect(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[keys]\nagents = "ctrl+a"\nconfig = "f1"\n',
        encoding="utf-8",
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("keys", "agents") == "ctrl+a"
    assert cfg.get("keys", "config") == "f1"
    assert cfg.notices == ()


def test_d12_new_chord_environment_override_beats_the_toml_file(
    isolated_global_config_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[keys]\nagents = "ctrl+a"\n', encoding="utf-8"
    )
    monkeypatch.setenv("TALARIA_KEYS_AGENTS", "alt+g")

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("keys", "agents") == "alt+g"
    assert cfg.notices == ()


@pytest.mark.parametrize(
    ("body", "key", "fallback", "notice_fragment"),
    [
        ('[keys]\nagents = ""\n', "agents", "ctrl+g", "keys.agents"),
        ('[keys]\nmodels = "ctrl+banana"\n', "models", "f11", "keys.models"),
        ('[keys]\nconfig = "ctrl+q"\n', "config", "ctrl+k", "reserved for quitting"),
        (
            '[keys]\nagents = "ctrl+x"\ncommands = "ctrl+x"\n',
            "agents",
            "ctrl+g",
            "are both",
        ),
    ],
)
def test_d12_new_chords_keep_the_existing_fallback_rules(
    isolated_global_config_dir: Path,
    tmp_path: Path,
    body: str,
    key: str,
    fallback: str,
    notice_fragment: str,
) -> None:
    """Invalid, reserved, and duplicated chords fall back with a notice —
    the same three rules the two existing chords follow."""
    (isolated_global_config_dir / "config.toml").write_text(body, encoding="utf-8")

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("keys", key) == fallback
    assert any(notice_fragment in notice for notice in cfg.notices), cfg.notices


def test_d12_duplicate_chords_reset_both_sides_to_defaults(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        '[keys]\nagents = "ctrl+x"\ncommands = "ctrl+x"\n',
        encoding="utf-8",
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("keys", "agents") == "ctrl+g"
    assert cfg.get("keys", "commands") == "f3"


def test_d11_connection_entry_parses_url_auth_and_label(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[connections.office]\n"
        'url = "http://10.220.1.139:8765"\n'
        'auth = "gated"\n'
        'label = "office dashboard"\n',
        encoding="utf-8",
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("connections", "office", "url") == "http://10.220.1.139:8765"
    assert cfg.get("connections", "office", "auth") == "gated"
    assert cfg.get("connections", "office", "label") == "office dashboard"
    assert cfg.notices == ()


def test_d11_unknown_auth_mode_falls_back_to_loopback_with_a_notice(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[connections.office]\n"
        'url = "http://10.220.1.139:8765"\n'
        'auth = "kerberos"\n',
        encoding="utf-8",
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("connections", "office", "auth") == "loopback"
    assert any("connections.office" in notice for notice in cfg.notices)


def test_d11_credentialed_connection_url_is_dropped_never_loaded(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    """Loads never raise, so a credentialed URL unloads to empty with a
    notice — the dial then fails closed as unaddressable rather than
    carrying the secret."""
    (isolated_global_config_dir / "config.toml").write_text(
        "[connections.office]\n"
        'url = "http://user:pass@10.220.1.139:8765"\n'
        'auth = "gated"\n',
        encoding="utf-8",
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("connections", "office", "auth") == "gated"
    assert cfg.get("connections", "office", "url") in (None, "")
    assert any(
        "connections.office" in notice and "credential" in notice.lower()
        for notice in cfg.notices
    )


def test_p2_3_gated_literal_rfc1918_connection_is_kept(
    isolated_global_config_dir: Path, tmp_path: Path
) -> None:
    (isolated_global_config_dir / "config.toml").write_text(
        "[connections.remote]\n"
        'url = "ws://10.220.1.139:8765/api/ws"\n'
        'auth = "gated"\n',
        encoding="utf-8",
    )

    cfg = load_config(cwd=tmp_path)

    assert cfg.get("connections", "remote", "auth") == "gated"
    assert cfg.get("connections", "remote", "url") == "ws://10.220.1.139:8765/api/ws"
